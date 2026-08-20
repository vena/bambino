//! # Chamber Image Binary JPEG Socket Protocol (Port 6000)
//!
//! Handles connection handshakes and payload processing for constrained printer lines
//! (P1 and A1 series, including A2L) transmitting discrete camera frames over raw TLS TCP sockets [REF-CAM-BINARY].
//!
//! **Handshake Architecture [REF-CAM-BINARY]:**
//! Upon establishing a TLS session, the connecting client must immediately transmit a
//! packed 80-byte authentication packet formatted in little-endian order. If the handshake is
//! accepted, the physical machine begins continuously writing raw JPEG frames prefixed with
//! a standard 16-byte length descriptor.
//!
//! **Flow Integrity Guards:**
//! 1. Verifies that incoming payloads conform strictly to JPEG magic start (`FF D8`) and
//!    end (`FF D9`) markers before returning buffers to upstream applications to insulate
//!    against decoding crashes.
//! 2. Clamps incoming frame sizes to a reasonable upper boundary (10MB by default) to protect
//!    against unbounded memory allocation crashes on low-resource environments if transport
//!    stream corruption occurs. Use [`BambuBinaryCameraStream::with_max_frame_size`] to lower
//!    this cap on constrained (`no_std`/Embassy) targets.

#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::client::dummy::DummyTimer;
use crate::error::Error;
use crate::identity::PrinterIdentity;
use crate::io::{AsyncIo, SocketError, TimerProvider, read_chunk};

pub(crate) const CAMERA_HANDSHAKE_SIZE: usize = 80;
pub(crate) const CAMERA_HANDSHAKE_MAGIC: u32 = 64;
pub(crate) const CAMERA_HANDSHAKE_COMMAND_ID: u32 = 12288;
pub(crate) const CAMERA_USERNAME_OFFSET: usize = 16;
pub(crate) const CAMERA_PASSWORD_OFFSET: usize = 48;
/// Maximum accepted access-code length for the camera handshake, in bytes. RTSPS auth
/// (`camera::rtsps::build_rtsps_url`) doesn't enforce this bound itself — the CLI's
/// connection-arg validation is the intended enforcement point for that path.
pub const CAMERA_PASSWORD_MAX_LEN: usize = 32;
pub(crate) const CAMERA_FRAME_HEADER_SIZE: usize = 16;
pub(crate) const CAMERA_FRAME_MAX_SIZE: usize = 10 * 1024 * 1024;
pub(crate) const JPEG_MARKER_SOI_HIGH: u8 = 0xFF;
pub(crate) const JPEG_MARKER_SOI_LOW: u8 = 0xD8;
pub(crate) const JPEG_MARKER_EOI_HIGH: u8 = 0xFF;
pub(crate) const JPEG_MARKER_EOI_LOW: u8 = 0xD9;

/// Per-read wall-clock deadline for [`BambuBinaryCameraStream::read_next_frame_with_timer`] when a real timer is available (see [`TimerProvider::has_real_clock`]) — same value and rationale as `MQTT_READ_TIMEOUT_SECS` (`src/mqtt/client/frame.rs`): a 30s gap between frames on an otherwise-live connection indicates a genuine stall, not normal frame-pacing jitter.
pub(crate) const CAMERA_READ_TIMEOUT_SECS: u64 = 30;

/// Chunk size for draining an oversized frame's declared-but-rejected payload off the wire
/// (see `CameraFrameReadState::DiscardingOversizedPayload`) — matches `FTP_LINE_READ_CHUNK_SIZE`
/// (`src/ftps/protocol.rs`)'s rationale: small enough to never itself risk an allocation
/// concern, since `remaining` can be attacker/corruption-controlled up to `u32::MAX`.
pub(crate) const CAMERA_DISCARD_CHUNK_SIZE: usize = 512;

/// Constructs the static 80-byte binary authentication packet required by the printer [REF-CAM-BINARY].
///
/// **Byte Ordering Specifications:**
/// * Offset 0-3 (4 bytes): Magic identifier header (`0x00000040` / 64)
/// * Offset 4-7 (4 bytes): Control operation Command ID (`0x00003000` / 12288)
/// * Offset 8-15 (8 bytes): Zero-padding block
/// * Offset 16-47 (32 bytes): Null-padded ASCII username (`"bblp"`)
/// * Offset 48-79 (32 bytes): Null-padded ASCII LAN access code
pub fn build_handshake_packet(
    access_code: &str,
) -> Result<[u8; CAMERA_HANDSHAKE_SIZE], Error> {
    let mut packet = [0u8; CAMERA_HANDSHAKE_SIZE];

    packet[0..4].copy_from_slice(&CAMERA_HANDSHAKE_MAGIC.to_le_bytes());
    packet[4..8].copy_from_slice(&CAMERA_HANDSHAKE_COMMAND_ID.to_le_bytes());

    let username = b"bblp";
    packet[CAMERA_USERNAME_OFFSET..CAMERA_USERNAME_OFFSET + username.len()]
        .copy_from_slice(username);

    if access_code.is_empty() || !access_code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Error::ProtocolViolation(
            "access_code must be a non-empty ASCII alphanumeric string".into(),
        ));
    }
    let code_bytes = access_code.as_bytes();
    if code_bytes.len() > CAMERA_PASSWORD_MAX_LEN {
        return Err(Error::ProtocolViolation(
            "Access code length exceeds maximum 32-byte authorization boundary".into(),
        ));
    }
    packet[CAMERA_PASSWORD_OFFSET..CAMERA_PASSWORD_OFFSET + code_bytes.len()]
        .copy_from_slice(code_bytes);

    Ok(packet)
}

/// Byte-level progress of an in-flight camera frame read, preserved across a timed-out [`BambuBinaryCameraStream::read_next_frame_with_timer`] call so a subsequent call resumes exactly where the previous one left off — losing this state would permanently desync the stream, the same failure class `FrameReadState` guards against for MQTT (`src/mqtt/client/frame.rs`).
/// Not a straight copy of that shape: MQTT's 1-byte header can't partially complete a single
/// `read()` step, so its `Idle` variant never needs header-partial-progress tracking — camera's
/// 16-byte header can, so `ReadingHeader` carries its own `filled` counter (closer in shape to
/// `ReadingPayload` below).
#[derive(Default)]
enum CameraFrameReadState {
    /// No partial frame in progress — the next read starts a fresh header.
    #[default]
    Idle,
    /// Header bytes read so far; `filled` may be less than `CAMERA_FRAME_HEADER_SIZE` if a prior call timed out mid-header.
    ReadingHeader {
        buf: [u8; CAMERA_FRAME_HEADER_SIZE],
        filled: usize,
    },
    /// Header fully decoded; `size` is the expected payload length, `buf` accumulates bytes as they arrive, `filled` tracks how many are valid so far.
    ReadingPayload {
        size: usize,
        buf: Vec<u8>,
        filled: usize,
    },
    /// An oversized frame's header was decoded, but its declared payload is still pending on
    /// the wire — `remaining` counts bytes left to discard before the stream is resynced.
    /// Never allocates `remaining` bytes up front (it's an attacker/corruption-controlled
    /// value up to `u32::MAX`); drains in small fixed chunks instead. Preserved across a
    /// timed-out call the same way `ReadingPayload` is — losing this would permanently desync
    /// the stream, which is the exact bug this state exists to fix (see
    /// `drain_oversized_payload`'s doc comment).
    DiscardingOversizedPayload { remaining: usize },
}

/// Abstract state controller parsing incoming frame buffers from raw Port 6000 streams.
///
/// Does not own dial/redial logic itself — a caller writing its own reconnect loop against
/// port 6000 must not redial immediately after a disconnect. The printer's port-6000 socket
/// accepts only one connection at a time; reopening before the prior TCP FIN completes can
/// orphan the old socket server-side until keepalive reaps it (~20 min stall). Confirmed
/// printer behavior (bambuddy `fix(camera) #2521`); add a delay or wait for the old socket
/// to fully close before redialing.
pub struct BambuBinaryCameraStream<IO: AsyncIo> {
    stream: IO,
    max_frame_size: usize,
    read_state: CameraFrameReadState,
}

impl<IO: AsyncIo> BambuBinaryCameraStream<IO> {
    /// Instantiates a camera parser wrapper surrounding an active secure stream socket.
    ///
    /// The accepted frame size defaults to `CAMERA_FRAME_MAX_SIZE` (10MB). Use
    /// [`Self::with_max_frame_size`] to lower it — useful on `no_std`/Embassy targets, where a
    /// 10MB transient allocation (see [`Self::read_next_frame`]) can exceed the entire SRAM
    /// budget and trigger an uncatchable `alloc_error_handler` abort rather than a recoverable
    /// `Result`.
    pub fn new(stream: IO) -> Self {
        Self {
            stream,
            max_frame_size: CAMERA_FRAME_MAX_SIZE,
            read_state: CameraFrameReadState::default(),
        }
    }

    /// Overrides the maximum accepted frame size (default: `CAMERA_FRAME_MAX_SIZE`, 10MB).
    ///
    /// Consuming builder, matching the `PrinterClient::with_mqtt_port`/`with_ftps_port`
    /// convention (`src/client/connect.rs`). Embedded callers should clamp this to a value that
    /// fits their actual JPEG resolution and buffer budget (e.g. 64-256KB) rather than relying
    /// on the desktop-sized default.
    #[must_use]
    pub fn with_max_frame_size(mut self, max: usize) -> Self {
        self.max_frame_size = max;
        self
    }

    /// Transmits the 80-byte authentication handshake to activate the continuous frame-push process.
    ///
    /// Per [REF-CAM-BINARY], this handshake protocol has no ack byte: a successful return only
    /// means the packet was written and flushed to the socket, **not** that the printer accepted
    /// the access code. If the code is wrong, the printer's real-world response (closing the
    /// socket, or simply never sending a frame) only surfaces later, on the *next*
    /// [`Self::read_next_frame`] call, as `Error::Network(SocketError::ConnectionReset)`
    /// — the same error variant a mid-stream network blip would produce. Callers that need to
    /// distinguish "wrong access code" from "transient network hiccup" cannot do so from this
    /// API alone.
    pub async fn authenticate(&mut self, identity: &PrinterIdentity) -> Result<(), Error> {
        self.authenticate_with_timer(
            &identity.access_code,
            &DummyTimer,
            CAMERA_READ_TIMEOUT_SECS * 1000,
        )
        .await
    }

    /// Bounds the handshake write+flush against `timer` when a real wall-clock is available (see [`TimerProvider::has_real_clock`]), mirroring [`Self::read_next_frame_with_timer`]'s naming/delegation convention.
    /// Unlike that read-side method, a timed-out write here has no partial-progress state worth
    /// preserving — the handshake is a single ~80-byte packet, small enough that losing/retrying the
    /// whole write on timeout is an acceptable simplification (unlike MQTT/camera frame *reads*, which
    /// must not lose already-read bytes) — so this races the whole `write_all`+`flush` sequence against
    /// `timer.sleep()` directly via the shared `race()` combinator instead of needing a resumable
    /// chunk-at-a-time helper like `read_chunk`.
    pub(crate) async fn authenticate_with_timer<T: TimerProvider>(
        &mut self,
        access_code: &str,
        timer: &T,
        budget_ms: u64,
    ) -> Result<(), Error> {
        let handshake = build_handshake_packet(access_code)?;

        let write_fut = async {
            self.stream
                .write_all(&handshake)
                .await
                .map_err(|_| Error::Network(SocketError::ConnectionAborted))?;
            self.stream
                .flush()
                .await
                .map_err(|_| Error::Network(SocketError::ConnectionAborted))
        };

        if !timer.has_real_clock() {
            return write_fut.await;
        }

        match crate::io::race(
            write_fut,
            timer.sleep(core::time::Duration::from_millis(budget_ms)),
        )
        .await
        {
            crate::io::Raced::Left(result) => result,
            crate::io::Raced::Right(_) => Err(Error::Network(SocketError::TimedOut)),
        }
    }

    /// Asynchronously extracts the next complete frame from the stream, bounding each low-level read step against `timer` when a real wall-clock is available (see [`TimerProvider::has_real_clock`]).
    /// Resumable: if a prior call on this stream timed out partway through a frame, the next call picks
    /// up from `self.read_state` instead of re-reading a fresh header — losing already-read bytes here
    /// would permanently desync the stream, the same failure class documented for MQTT's
    /// `read_exact_packet` (`src/mqtt/client/frame.rs`).
    ///
    /// `budget_ms` is an explicit parameter (not a hardcoded constant) so tests can pass a
    /// small budget instead of waiting out [`CAMERA_READ_TIMEOUT_SECS`] for real; production
    /// callers should pass `CAMERA_READ_TIMEOUT_SECS * 1000`. A fresh deadline is computed
    /// every call from `budget_ms`, mirroring `read_exact_packet`'s behavior (not once per
    /// logical frame).
    pub(crate) async fn read_next_frame_with_timer<T: TimerProvider>(
        &mut self,
        frame_buf: &mut Vec<u8>,
        timer: &T,
        budget_ms: u64,
    ) -> Result<(), Error> {
        let deadline_ms = if timer.has_real_clock() {
            Some(timer.now_millis().saturating_add(budget_ms))
        } else {
            None
        };

        // Resume draining a prior oversized frame's payload before anything else — bytes still
        // pending on the wire from that frame must be consumed before a fresh header can be
        // read, or the next header/payload split would desync against stale payload bytes.
        if matches!(
            self.read_state,
            CameraFrameReadState::DiscardingOversizedPayload { .. }
        ) {
            return self.drain_oversized_payload(timer, deadline_ms).await;
        }

        // Header bytes (only start a fresh header if not already mid-header from a prior,
        // timed-out call).
        if matches!(self.read_state, CameraFrameReadState::Idle) {
            self.read_state = CameraFrameReadState::ReadingHeader {
                buf: [0u8; CAMERA_FRAME_HEADER_SIZE],
                filled: 0,
            };
        }

        if let CameraFrameReadState::ReadingHeader { buf, filled } = &mut self.read_state {
            while *filled < buf.len() {
                let n = read_chunk(&mut self.stream, &mut buf[*filled..], timer, deadline_ms)
                    .await
                    .map_err(Error::Network)?;
                *filled += n;
            }

            // Extract little-endian payload size N from first 4 bytes. Use a fallible
            // conversion rather than `as usize` — on a hypothetical <32-bit `usize` target an
            // `as` cast would silently truncate the length field instead of erroring, before
            // the frame-size sanity check below even runs.
            let raw_size = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            let size = usize::try_from(raw_size).map_err(|_| {
                Error::ProtocolViolation(
                    "Frame size descriptor does not fit in this platform's usize".into(),
                )
            })?;

            // Bounded allocation check to guard against memory allocation overflow attacks.
            // The declared payload is still pending on the wire — drain it (never allocating
            // `size` bytes) before returning, so a caller that keeps polling the same instance
            // instead of reconnecting doesn't get permanently desynced (previously this reset
            // straight to `Idle` without draining, unlike the JPEG-marker-validation failure
            // path below, which is safe only because its payload was already fully consumed).
            if size > self.max_frame_size {
                self.read_state =
                    CameraFrameReadState::DiscardingOversizedPayload { remaining: size };
                return self.drain_oversized_payload(timer, deadline_ms).await;
            }
            if size == 0 {
                self.read_state = CameraFrameReadState::Idle;
                return Err(Error::ProtocolViolation(
                    "Acquired empty frame payload descriptor".into(),
                ));
            }

            self.read_state = CameraFrameReadState::ReadingPayload {
                size,
                buf: vec![0u8; size],
                filled: 0,
            };
        }

        // Payload bytes (resumes from `filled` if a prior call stalled mid-payload).
        if let CameraFrameReadState::ReadingPayload { size, buf, filled } = &mut self.read_state {
            while *filled < buf.len() {
                let n = read_chunk(&mut self.stream, &mut buf[*filled..], timer, deadline_ms)
                    .await
                    .map_err(Error::Network)?;
                *filled += n;
            }

            let size = *size;
            let payload = core::mem::take(buf);
            self.read_state = CameraFrameReadState::Idle;

            // Validate frame bounds to protect downstream graphic engines against decoding
            // crashes (validation only runs after `read_state` has already collapsed back to
            // `Idle` — pure buffer post-processing, no I/O, doesn't interact with resumability).
            if size < 4
                || payload[0] != JPEG_MARKER_SOI_HIGH
                || payload[1] != JPEG_MARKER_SOI_LOW
                || payload[size - 2] != JPEG_MARKER_EOI_HIGH
                || payload[size - 1] != JPEG_MARKER_EOI_LOW
            {
                return Err(Error::ProtocolViolation(
                    "Acquired stream packet lacks valid JPEG magic marker boundaries".into(),
                ));
            }

            *frame_buf = payload;
            return Ok(());
        }

        unreachable!("CameraFrameReadState must be ReadingPayload after header decode")
    }

    /// Drains an oversized frame's declared-but-rejected payload off the wire in bounded
    /// `CAMERA_DISCARD_CHUNK_SIZE` chunks (never allocating `remaining` bytes, which can be
    /// attacker/corruption-controlled up to `u32::MAX`), keeping the stream in sync so a
    /// caller that retries `read_next_frame`/`read_next_frame_with_timer` on this same
    /// instance — rather than reconnecting — reads the *next* real frame's header instead of
    /// misreading stale payload bytes. Resumable: if the deadline hits mid-drain,
    /// `self.read_state`'s `remaining` count persists and the next call picks up the drain
    /// before attempting anything else (see the check at the top of
    /// `read_next_frame_with_timer`).
    async fn drain_oversized_payload<T: TimerProvider>(
        &mut self,
        timer: &T,
        deadline_ms: Option<u64>,
    ) -> Result<(), Error> {
        if let CameraFrameReadState::DiscardingOversizedPayload { remaining } = &mut self.read_state
        {
            let mut scratch = [0u8; CAMERA_DISCARD_CHUNK_SIZE];
            while *remaining > 0 {
                let want = core::cmp::min(*remaining, scratch.len());
                let n = read_chunk(&mut self.stream, &mut scratch[..want], timer, deadline_ms)
                    .await
                    .map_err(Error::Network)?;
                *remaining -= n;
            }
        }
        self.read_state = CameraFrameReadState::Idle;
        Err(Error::ProtocolViolation(
            "Extracted JPEG frame size exceeds configured safety allocation limit".into(),
        ))
    }

    /// Asynchronously extracts the next complete frame from the stream.
    ///
    /// Wholesale-replaces the user-supplied `Vec<u8>` with the decoded frame each call
    /// (`*frame_buf = payload`) — no buffer reuse. Delegates to `read_next_frame_with_timer` under
    /// [`DummyTimer`], which degrades to a plain unbounded read — behavior-preserving for
    /// every existing caller not going through `PrinterClient`.
    pub async fn read_next_frame(&mut self, frame_buf: &mut Vec<u8>) -> Result<(), Error> {
        self.read_next_frame_with_timer(frame_buf, &DummyTimer, CAMERA_READ_TIMEOUT_SECS * 1000)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handshake_packet_construction() {
        let packet = build_handshake_packet("ABCDEF12").unwrap();

        assert_eq!(packet[0..4], 64u32.to_le_bytes());
        assert_eq!(packet[4..8], 12288u32.to_le_bytes());
        assert_eq!(&packet[16..20], b"bblp");
        assert_eq!(packet[20], 0);
        assert_eq!(&packet[48..56], b"ABCDEF12");
        assert_eq!(packet[56], 0);
    }

    #[test]
    fn test_handshake_max_length_access_code() {
        let code = "A".repeat(CAMERA_PASSWORD_MAX_LEN);
        let packet = build_handshake_packet(&code).unwrap();
        assert_eq!(
            &packet[CAMERA_PASSWORD_OFFSET..CAMERA_PASSWORD_OFFSET + 32],
            code.as_bytes()
        );
    }

    #[test]
    fn test_handshake_oversized_access_code() {
        let code = "A".repeat(CAMERA_PASSWORD_MAX_LEN + 1);
        let result = build_handshake_packet(&code);
        assert!(matches!(result, Err(Error::ProtocolViolation(_))));
    }

    #[test]
    fn test_handshake_rejects_non_alphanumeric_access_code() {
        assert!(build_handshake_packet("1234@678").is_err());
        assert!(build_handshake_packet("1234 678").is_err());
        assert!(build_handshake_packet("1234\n678").is_err());
    }

    #[test]
    fn test_handshake_rejects_empty_access_code() {
        // `.all()` on an empty string's char iterator vacuously returns true, so the
        // alphanumeric check alone let an empty access_code silently build a handshake packet
        // with a zero-length password field. rtsps.rs's build_rtsps_url already has this
        // explicit empty-string guard for the same copy-paste-mistake reason.
        assert!(matches!(
            build_handshake_packet(""),
            Err(Error::ProtocolViolation(_))
        ));
    }

    #[cfg(feature = "tokio")]
    mod async_tests {
        use super::*;
        use crate::io::TokioIo;
        use tokio::io::AsyncWriteExt;

        fn make_frame_header(size: u32) -> Vec<u8> {
            let mut header = vec![0u8; CAMERA_FRAME_HEADER_SIZE];
            header[0..4].copy_from_slice(&size.to_le_bytes());
            header
        }

        #[tokio::test]
        async fn test_read_frame_oversized() {
            let data = make_frame_header((CAMERA_FRAME_MAX_SIZE + 1) as u32);
            let cursor = std::io::Cursor::new(data);
            let mut camera = BambuBinaryCameraStream::new(TokioIo(cursor));
            let mut buf = Vec::new();
            let result = camera.read_next_frame(&mut buf).await;
            assert!(matches!(result, Err(Error::Network(_))));
            // Cursor has no more bytes after the header, so draining the (never-sent) declared
            // payload hits EOF — confirms the drain path is actually exercised (BUG: this used
            // to bail straight to Idle without draining at all, which this test's mere
            // ProtocolViolation-without-EOF assertion couldn't have caught). See
            // `test_read_frame_oversized_drains_and_resyncs_stream` for the full happy-path
            // proof that a subsequent frame reads correctly after an oversized one.
        }

        #[tokio::test]
        async fn test_read_frame_respects_custom_max_frame_size() {
            // A frame well under the default 10MB cap but over a custom, smaller cap must be
            // rejected — this is the behavior embedded callers rely on via `with_max_frame_size`.
            // The full declared payload is included so the oversized-frame drain path (BUG)
            // actually completes instead of hitting EOF, yielding the real ProtocolViolation.
            let mut data = make_frame_header(1024);
            data.extend(vec![0u8; 1024]);
            let cursor = std::io::Cursor::new(data);
            let mut camera = BambuBinaryCameraStream::new(TokioIo(cursor)).with_max_frame_size(64);
            let mut buf = Vec::new();
            let result = camera.read_next_frame(&mut buf).await;
            assert!(matches!(result, Err(Error::ProtocolViolation(_))));
        }

        #[tokio::test]
        async fn test_read_frame_oversized_drains_and_resyncs_stream() {
            // BUG: an oversized-frame rejection used to reset read_state to Idle without
            // draining the declared payload still pending on the wire, permanently desyncing
            // the stream for any caller that retries on the same instance instead of
            // reconnecting. Sends an oversized frame's full payload followed by a real valid
            // frame, and asserts the second read correctly recovers the valid frame instead of
            // misreading stale oversized-payload bytes as a bogus header.
            let mut data = make_frame_header(1024);
            data.extend(vec![0xAAu8; 1024]);
            let mut valid_frame = vec![JPEG_MARKER_SOI_HIGH, JPEG_MARKER_SOI_LOW];
            valid_frame.extend([JPEG_MARKER_EOI_HIGH, JPEG_MARKER_EOI_LOW]);
            data.extend(make_frame_header(valid_frame.len() as u32));
            data.extend(&valid_frame);

            let cursor = std::io::Cursor::new(data);
            let mut camera = BambuBinaryCameraStream::new(TokioIo(cursor)).with_max_frame_size(64);
            let mut buf = Vec::new();

            let oversized_result = camera.read_next_frame(&mut buf).await;
            assert!(matches!(
                oversized_result,
                Err(Error::ProtocolViolation(_))
            ));

            let resynced_result = camera.read_next_frame(&mut buf).await;
            assert!(
                resynced_result.is_ok(),
                "expected the stream to resync onto the next real frame, got {:?}",
                resynced_result
            );
            assert_eq!(buf, valid_frame);
        }

        #[tokio::test]
        async fn test_read_frame_zero_size() {
            let data = make_frame_header(0);
            let cursor = std::io::Cursor::new(data);
            let mut camera = BambuBinaryCameraStream::new(TokioIo(cursor));
            let mut buf = Vec::new();
            let result = camera.read_next_frame(&mut buf).await;
            assert!(matches!(result, Err(Error::ProtocolViolation(_))));
        }

        #[tokio::test]
        async fn test_read_frame_invalid_jpeg_markers() {
            let mut data = make_frame_header(4);
            data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            let cursor = std::io::Cursor::new(data);
            let mut camera = BambuBinaryCameraStream::new(TokioIo(cursor));
            let mut buf = Vec::new();
            let result = camera.read_next_frame(&mut buf).await;
            assert!(matches!(result, Err(Error::ProtocolViolation(_))));
        }

        /// Regression test mirroring `test_read_exact_packet_stalled_connection_times_out` (`src/mqtt/client/frame.rs`): a connection that stalls with zero incoming bytes must not hang `read_next_frame_with_timer` forever.
        /// Uses a `tokio::io::duplex` whose server side never writes, so the client's low-level `read()`
        /// call is genuinely pending. The outer `tokio::time::timeout` is a meta-safety net.
        #[tokio::test]
        async fn test_read_next_frame_with_timer_stalled_connection_times_out() {
            let (client_stream, _server_stream) = tokio::io::duplex(64);
            // Server side is kept alive (bound to `_server_stream`) but never writes —
            // dropping it would deliver `Ok(0)`/EOF instead of a genuine stall.

            let mut camera = BambuBinaryCameraStream::new(TokioIo(client_stream));
            let timer = crate::io::tokio::TokioTimer::new();
            let budget_ms = 50;
            let mut buf = Vec::new();

            let started = std::time::Instant::now();
            let result = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                camera.read_next_frame_with_timer(&mut buf, &timer, budget_ms),
            )
            .await
            .expect(
                "read_next_frame_with_timer hung past the 5s meta-safety timeout instead of \
                 honoring its own budget",
            );
            let elapsed = started.elapsed();

            assert!(
                matches!(
                    result,
                    Err(Error::Network(crate::io::SocketError::TimedOut))
                ),
                "Expected TimedOut for a stalled connection, got {:?}",
                result
            );
            assert!(
                elapsed < core::time::Duration::from_secs(2),
                "read_next_frame_with_timer took {:?} to time out against a {}ms budget — too slow",
                elapsed,
                budget_ms
            );
        }

        /// Regression test: a peer that never drains its TCP receive buffer during the handshake must not hang `authenticate_with_timer` forever.
        /// `duplex(64)` gives the write side a smaller buffer than the 80-byte handshake packet, and the
        /// server side is kept alive but never reads — so `write_all` genuinely stalls partway through once
        /// the buffer fills, rather than merely being slow.
        #[tokio::test]
        async fn test_authenticate_with_timer_stalled_connection_times_out() {
            let (client_stream, _server_stream) = tokio::io::duplex(64);

            let mut camera = BambuBinaryCameraStream::new(TokioIo(client_stream));
            let timer = crate::io::tokio::TokioTimer::new();
            let budget_ms = 50;

            let started = std::time::Instant::now();
            let result = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                camera.authenticate_with_timer("ABCDEF12", &timer, budget_ms),
            )
            .await
            .expect(
                "authenticate_with_timer hung past the 5s meta-safety timeout instead of \
                 honoring its own budget",
            );
            let elapsed = started.elapsed();

            assert!(
                matches!(
                    result,
                    Err(Error::Network(crate::io::SocketError::TimedOut))
                ),
                "Expected TimedOut for a stalled connection, got {:?}",
                result
            );
            assert!(
                elapsed < core::time::Duration::from_secs(2),
                "authenticate_with_timer took {:?} to time out against a {}ms budget — too slow",
                elapsed,
                budget_ms
            );
        }

        /// Regression test mirroring `test_read_exact_packet_resumes_after_timeout_without_losing_bytes` (`src/mqtt/client/frame.rs`): bytes already read into a partial-frame buffer before a timeout must never be lost.
        /// Server delivers the full 16-byte header plus 2 of 4 expected payload bytes, then stalls; the
        /// first call times out mid-payload; the second call (after the rest arrives) must reconstruct the
        /// exact original frame.
        #[tokio::test]
        async fn test_read_next_frame_with_timer_resumes_after_timeout_without_losing_bytes() {
            let (client_stream, mut server_stream) = tokio::io::duplex(64);
            let mut camera = BambuBinaryCameraStream::new(TokioIo(client_stream));
            let timer = crate::io::tokio::TokioTimer::new();
            let mut buf = Vec::new();

            // Header declares a 4-byte payload; server sends header + first 2 payload bytes,
            // then stops.
            let mut sent = make_frame_header(4);
            sent.extend_from_slice(&[JPEG_MARKER_SOI_HIGH, JPEG_MARKER_SOI_LOW]);
            server_stream.write_all(&sent).await.unwrap();
            server_stream.flush().await.unwrap();

            let first_attempt = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                camera.read_next_frame_with_timer(&mut buf, &timer, 50),
            )
            .await
            .expect("first attempt hung past the meta-safety timeout");

            assert!(
                matches!(
                    first_attempt,
                    Err(Error::Network(crate::io::SocketError::TimedOut))
                ),
                "Expected the first attempt to time out waiting on the missing payload bytes, \
                 got {:?}",
                first_attempt
            );

            // Send the remaining 2 payload bytes to complete a valid JPEG frame.
            server_stream
                .write_all(&[JPEG_MARKER_EOI_HIGH, JPEG_MARKER_EOI_LOW])
                .await
                .unwrap();
            server_stream.flush().await.unwrap();

            let second_attempt = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                camera.read_next_frame_with_timer(&mut buf, &timer, 50),
            )
            .await
            .expect("second attempt hung past the meta-safety timeout");

            assert!(
                second_attempt.is_ok(),
                "expected the resumed read to succeed, got {:?}",
                second_attempt
            );
            assert_eq!(
                buf,
                vec![
                    JPEG_MARKER_SOI_HIGH,
                    JPEG_MARKER_SOI_LOW,
                    JPEG_MARKER_EOI_HIGH,
                    JPEG_MARKER_EOI_LOW
                ]
            );
        }
    }
}
