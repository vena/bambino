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
//!    stream corruption occurs. Use [`BinaryCameraStream::with_max_frame_size`] to lower
//!    this cap on constrained (`no_std`/Embassy) targets.

#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::client::dummy::DummyTimer;
use crate::error::Error;
use crate::identity::{ACCESS_CODE_MAX_LEN, validate_access_code};
use crate::io::{AsyncIo, TimerProvider, map_embedded_io_error_kind, read_chunk, with_deadline};

pub(crate) const CAMERA_HANDSHAKE_SIZE: usize = 80;
pub(crate) const CAMERA_HANDSHAKE_MAGIC: u32 = 64;
pub(crate) const CAMERA_HANDSHAKE_COMMAND_ID: u32 = 12288;
pub(crate) const CAMERA_USERNAME_OFFSET: usize = 16;
pub(crate) const CAMERA_PASSWORD_OFFSET: usize = 48;
// The password field runs to the end of the packet; `ACCESS_CODE_MAX_LEN` must fit it exactly.
const _: () = assert!(CAMERA_HANDSHAKE_SIZE - CAMERA_PASSWORD_OFFSET == ACCESS_CODE_MAX_LEN);
pub(crate) const CAMERA_FRAME_HEADER_SIZE: usize = 16;
pub(crate) const CAMERA_FRAME_MAX_SIZE: usize = 10 * 1024 * 1024;
/// JPEG start-of-image marker every frame payload must begin with.
pub(crate) const JPEG_SOI: [u8; 2] = [0xFF, 0xD8];
/// JPEG end-of-image marker every frame payload must end with.
pub(crate) const JPEG_EOI: [u8; 2] = [0xFF, 0xD9];
/// Shortest payload that can carry both markers without them overlapping (`FF D8 D9` can't).
const JPEG_MIN_LEN: usize = JPEG_SOI.len() + JPEG_EOI.len();

/// Per-read wall-clock deadline for [`BinaryCameraStream::read_next_frame_with_timer`] when a real timer is available (see [`TimerProvider::has_real_clock`]) — same value and rationale as `MQTT_READ_TIMEOUT_SECS` (`src/mqtt/client/frame.rs`): a 30s gap between frames on an otherwise-live connection indicates a genuine stall, not normal frame-pacing jitter.
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
pub fn build_handshake_packet(access_code: &str) -> Result<[u8; CAMERA_HANDSHAKE_SIZE], Error> {
    let mut packet = [0u8; CAMERA_HANDSHAKE_SIZE];

    packet[0..4].copy_from_slice(&CAMERA_HANDSHAKE_MAGIC.to_le_bytes());
    packet[4..8].copy_from_slice(&CAMERA_HANDSHAKE_COMMAND_ID.to_le_bytes());

    let username = b"bblp";
    packet[CAMERA_USERNAME_OFFSET..CAMERA_USERNAME_OFFSET + username.len()]
        .copy_from_slice(username);

    validate_access_code(access_code)?;
    let code_bytes = access_code.as_bytes();
    packet[CAMERA_PASSWORD_OFFSET..CAMERA_PASSWORD_OFFSET + code_bytes.len()]
        .copy_from_slice(code_bytes);

    Ok(packet)
}

/// Byte-level progress of an in-flight camera frame read, preserved across a timed-out [`BinaryCameraStream::read_next_frame_with_timer`] call so a subsequent call resumes exactly where the previous one left off — losing this state would permanently desync the stream, the same failure class `FrameReadState` guards against for MQTT (`src/mqtt/client/frame.rs`).
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
    /// Header fully decoded; `buf` is sized to the declared payload and accumulates bytes as they arrive, `filled` tracks how many are valid so far.
    ReadingPayload { buf: Vec<u8>, filled: usize },
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
pub struct BinaryCameraStream<IO: AsyncIo> {
    stream: IO,
    max_frame_size: usize,
    read_state: CameraFrameReadState,
}

impl<IO: AsyncIo> BinaryCameraStream<IO> {
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

    /// Hands out the underlying transport so a teardown path can shut its TLS session down.
    ///
    /// Exists for `PrinterClient::disconnect_camera`, which needs `&mut Self::Stream` to call
    /// [`TlsConnector::close`](crate::io::TlsConnector::close) before the stream is dropped
    /// (GitHub issue #293). `pub(crate)` on purpose: reading from this directly would strand
    /// `read_state` mid-frame.
    pub(crate) fn stream_mut(&mut self) -> &mut IO {
        &mut self.stream
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
    /// the access code. A wrong code surfaces only on the *next* frame read, in one of three
    /// ways depending on what the printer does and which read is used:
    ///
    /// - the printer closes the socket: `Error::Network(SocketError::ConnectionReset)`;
    /// - the printer stays silent, read with a real timer (e.g.
    ///   [`PrinterClient::read_camera_frame`](crate::client::PrinterClient::read_camera_frame)):
    ///   `Error::Network(SocketError::TimedOut)` after `CAMERA_READ_TIMEOUT_SECS` (30s);
    /// - the printer stays silent, read with [`Self::read_next_frame`] (no timer): the read
    ///   blocks indefinitely.
    ///
    /// Each is also what a network fault would produce, so a caller cannot distinguish "wrong
    /// access code" from a network problem through this API alone, and matching only
    /// `ConnectionReset` misses the silent cases.
    pub async fn authenticate(&mut self, access_code: &str) -> Result<(), Error> {
        self.authenticate_with_timer(access_code, &DummyTimer, CAMERA_READ_TIMEOUT_SECS * 1000)
            .await
    }

    /// Bounds the handshake write+flush against `timer` when a real wall-clock is available (see [`TimerProvider::has_real_clock`]), mirroring [`Self::read_next_frame_with_timer`]'s naming/delegation convention.
    /// Unlike that read-side method, a timed-out write here has no partial-progress state worth
    /// preserving — the handshake is a single ~80-byte packet, small enough that losing/retrying the
    /// whole write on timeout is an acceptable simplification (unlike MQTT/camera frame *reads*, which
    /// must not lose already-read bytes) — so this races the whole `write_all`+`flush` sequence against
    /// `timer.sleep()` via [`with_deadline`] instead of needing a resumable chunk-at-a-time helper
    /// like `read_chunk`.
    pub(crate) async fn authenticate_with_timer<T: TimerProvider>(
        &mut self,
        access_code: &str,
        timer: &T,
        budget_ms: u64,
    ) -> Result<(), Error> {
        let handshake = build_handshake_packet(access_code)?;
        let io_error = |e: IO::Error| {
            Error::Network(map_embedded_io_error_kind(embedded_io_async::Error::kind(
                &e,
            )))
        };

        let write_fut = async {
            self.stream.write_all(&handshake).await.map_err(io_error)?;
            self.stream.flush().await.map_err(io_error)
        };
        with_deadline(write_fut, timer, budget_ms)
            .await
            .map_err(Error::Network)?
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
        timer: &T,
        budget_ms: u64,
    ) -> Result<Vec<u8>, Error> {
        let deadline_ms = if timer.has_real_clock() {
            Some(timer.now_millis().saturating_add(budget_ms))
        } else {
            None
        };

        loop {
            match &mut self.read_state {
                CameraFrameReadState::Idle => {
                    self.read_state = CameraFrameReadState::ReadingHeader {
                        buf: [0u8; CAMERA_FRAME_HEADER_SIZE],
                        filled: 0,
                    };
                }
                CameraFrameReadState::ReadingHeader { buf, filled } => {
                    fill(&mut self.stream, buf, filled, timer, deadline_ms).await?;

                    // Extract little-endian payload size N from first 4 bytes. Use a fallible
                    // conversion rather than `as usize` — on a hypothetical <32-bit `usize`
                    // target an `as` cast would silently truncate the length field instead of
                    // erroring, before the frame-size sanity check below even runs.
                    let raw_size = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    let size = usize::try_from(raw_size).map_err(|_| {
                        Error::ProtocolViolation(
                            "Frame size descriptor does not fit in this platform's usize".into(),
                        )
                    })?;

                    // Bounded allocation check to guard against memory allocation overflow
                    // attacks. The declared payload is still pending on the wire, so drain it
                    // (never allocating `size` bytes) before returning: a caller that keeps
                    // polling the same instance instead of reconnecting must not be desynced.
                    if size == 0 {
                        self.read_state = CameraFrameReadState::Idle;
                        return Err(Error::ProtocolViolation(
                            "Acquired empty frame payload descriptor".into(),
                        ));
                    }
                    self.read_state = if size > self.max_frame_size {
                        CameraFrameReadState::DiscardingOversizedPayload { remaining: size }
                    } else {
                        CameraFrameReadState::ReadingPayload {
                            buf: vec![0u8; size],
                            filled: 0,
                        }
                    };
                }
                CameraFrameReadState::ReadingPayload { buf, filled } => {
                    fill(&mut self.stream, buf, filled, timer, deadline_ms).await?;
                    let payload = core::mem::take(buf);
                    self.read_state = CameraFrameReadState::Idle;

                    // Validate frame bounds to protect downstream graphic engines against
                    // decoding crashes. Pure post-processing on a fully consumed payload, so it
                    // doesn't interact with resumability.
                    if payload.len() < JPEG_MIN_LEN
                        || !payload.starts_with(&JPEG_SOI)
                        || !payload.ends_with(&JPEG_EOI)
                    {
                        return Err(Error::ProtocolViolation(
                            "Acquired stream packet lacks valid JPEG magic marker boundaries"
                                .into(),
                        ));
                    }
                    return Ok(payload);
                }
                // Drains an oversized frame's declared-but-rejected payload in bounded
                // `CAMERA_DISCARD_CHUNK_SIZE` chunks (never allocating `remaining` bytes, which
                // can be attacker/corruption-controlled up to `u32::MAX`), keeping the stream in
                // sync so a retry on this instance reads the *next* real frame's header.
                // Resumable: on a mid-drain timeout `remaining` persists in `self.read_state`.
                CameraFrameReadState::DiscardingOversizedPayload { remaining } => {
                    let mut scratch = [0u8; CAMERA_DISCARD_CHUNK_SIZE];
                    while *remaining > 0 {
                        let want = core::cmp::min(*remaining, scratch.len());
                        let n =
                            read_chunk(&mut self.stream, &mut scratch[..want], timer, deadline_ms)
                                .await
                                .map_err(Error::Network)?;
                        *remaining -= n;
                    }
                    self.read_state = CameraFrameReadState::Idle;
                    return Err(Error::ProtocolViolation(
                        "Extracted JPEG frame size exceeds configured safety allocation limit"
                            .into(),
                    ));
                }
            }
        }
    }

    /// Asynchronously extracts and returns the next complete frame from the stream.
    ///
    /// Delegates to `read_next_frame_with_timer` under [`DummyTimer`], which degrades to a
    /// plain unbounded read — behavior-preserving for every existing caller not going through
    /// `PrinterClient`.
    pub async fn read_next_frame(&mut self) -> Result<Vec<u8>, Error> {
        self.read_next_frame_with_timer(&DummyTimer, CAMERA_READ_TIMEOUT_SECS * 1000)
            .await
    }
}

/// Reads into `buf[*filled..]` until it is full, advancing `filled` as bytes arrive.
///
/// `filled` lives in the caller's read state, not a local, so bytes read before a timeout are
/// kept for the next call (`.claude/rules/wire-read-deadline.md`).
async fn fill<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    buf: &mut [u8],
    filled: &mut usize,
    timer: &T,
    deadline_ms: Option<u64>,
) -> Result<(), Error> {
    while *filled < buf.len() {
        *filled += read_chunk(stream, &mut buf[*filled..], timer, deadline_ms)
            .await
            .map_err(Error::Network)?;
    }
    Ok(())
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
        let code = "A".repeat(ACCESS_CODE_MAX_LEN);
        let packet = build_handshake_packet(&code).unwrap();
        assert_eq!(
            &packet[CAMERA_PASSWORD_OFFSET..CAMERA_PASSWORD_OFFSET + 32],
            code.as_bytes()
        );
    }

    #[test]
    fn test_handshake_oversized_access_code() {
        let code = "A".repeat(ACCESS_CODE_MAX_LEN + 1);
        let result = build_handshake_packet(&code);
        assert!(matches!(result, Err(Error::InvalidArgument(_))));
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
        // with a zero-length password field.
        assert!(matches!(
            build_handshake_packet(""),
            Err(Error::InvalidArgument(_))
        ));
    }

    /// The handshake write keeps the I/O error's kind, so running out of memory mid-handshake
    /// reports `ResourceExhausted` rather than a dropped link (#389) — with and without a timer.
    #[tokio::test]
    async fn test_authenticate_write_failure_keeps_its_error_kind() {
        use crate::io::SocketError;
        use crate::test_support::{MockIo, MockTimer};
        use embedded_io_async::ErrorKind;

        let cases = [
            (ErrorKind::OutOfMemory, SocketError::ResourceExhausted),
            (ErrorKind::ConnectionReset, SocketError::ConnectionReset),
            (ErrorKind::ConnectionAborted, SocketError::ConnectionAborted),
        ];
        for (kind, expected) in cases {
            let mut camera = BinaryCameraStream::new(MockIo::failing(kind));
            let untimed = camera
                .authenticate_with_timer("ABCDEF12", &DummyTimer, 1_000)
                .await;
            assert!(
                matches!(&untimed, Err(Error::Network(e)) if *e == expected),
                "{kind:?}: {untimed:?}"
            );

            let mut camera = BinaryCameraStream::new(MockIo::failing(kind));
            let timed = camera
                .authenticate_with_timer("ABCDEF12", &MockTimer::new(), 1_000)
                .await;
            assert!(
                matches!(&timed, Err(Error::Network(e)) if *e == expected),
                "{kind:?}: {timed:?}"
            );
        }
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
            let mut camera = BinaryCameraStream::new(TokioIo::new(cursor));
            let result = camera.read_next_frame().await;
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
            let mut camera = BinaryCameraStream::new(TokioIo::new(cursor)).with_max_frame_size(64);
            let result = camera.read_next_frame().await;
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
            let valid_frame = [JPEG_SOI, JPEG_EOI].concat();
            data.extend(make_frame_header(valid_frame.len() as u32));
            data.extend(&valid_frame);

            let cursor = std::io::Cursor::new(data);
            let mut camera = BinaryCameraStream::new(TokioIo::new(cursor)).with_max_frame_size(64);

            let oversized_result = camera.read_next_frame().await;
            assert!(matches!(oversized_result, Err(Error::ProtocolViolation(_))));

            let resynced_result = camera.read_next_frame().await;
            assert_eq!(
                resynced_result.ok(),
                Some(valid_frame),
                "expected the stream to resync onto the next real frame"
            );
        }

        #[tokio::test]
        async fn test_read_frame_zero_size() {
            let data = make_frame_header(0);
            let cursor = std::io::Cursor::new(data);
            let mut camera = BinaryCameraStream::new(TokioIo::new(cursor));
            let result = camera.read_next_frame().await;
            assert!(matches!(result, Err(Error::ProtocolViolation(_))));
        }

        #[tokio::test]
        async fn test_read_frame_rejects_overlapping_markers() {
            // `FF D8 D9` starts with SOI and ends with EOI only by sharing a byte.
            let mut data = make_frame_header(3);
            data.extend_from_slice(&[0xFF, 0xD8, 0xD9]);
            let mut camera = BinaryCameraStream::new(TokioIo::new(std::io::Cursor::new(data)));
            let result = camera.read_next_frame().await;
            assert!(matches!(result, Err(Error::ProtocolViolation(_))));
        }

        #[tokio::test]
        async fn test_read_frame_invalid_jpeg_markers() {
            let mut data = make_frame_header(4);
            data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            let cursor = std::io::Cursor::new(data);
            let mut camera = BinaryCameraStream::new(TokioIo::new(cursor));
            let result = camera.read_next_frame().await;
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

            let mut camera = BinaryCameraStream::new(TokioIo::new(client_stream));
            let timer = crate::io::tokio::TokioTimer::new();
            let budget_ms = 50;

            let started = std::time::Instant::now();
            let result = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                camera.read_next_frame_with_timer(&timer, budget_ms),
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

            let mut camera = BinaryCameraStream::new(TokioIo::new(client_stream));
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
            let mut camera = BinaryCameraStream::new(TokioIo::new(client_stream));
            let timer = crate::io::tokio::TokioTimer::new();

            // Header declares a 4-byte payload; server sends header + first 2 payload bytes,
            // then stops.
            let mut sent = make_frame_header(4);
            sent.extend_from_slice(&JPEG_SOI);
            server_stream.write_all(&sent).await.unwrap();
            server_stream.flush().await.unwrap();

            let first_attempt = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                camera.read_next_frame_with_timer(&timer, 50),
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
            server_stream.write_all(&JPEG_EOI).await.unwrap();
            server_stream.flush().await.unwrap();

            let second_attempt = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                camera.read_next_frame_with_timer(&timer, 50),
            )
            .await
            .expect("second attempt hung past the meta-safety timeout");

            assert_eq!(
                second_attempt.ok(),
                Some([JPEG_SOI, JPEG_EOI].concat()),
                "expected the resumed read to reconstruct the original frame"
            );
        }
    }
}
