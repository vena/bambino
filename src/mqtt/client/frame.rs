//! Resumable MQTT frame reading over an abstract `AsyncIo` stream.

#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::io::{AsyncIo, SocketError, TimerProvider, read_chunk};

pub(crate) const MQTT_MAX_PAYLOAD_BYTES: usize = 1_048_576; // 1 MiB

/// Per-call deadline for `read_exact_packet` when a genuine wall-clock [`TimerProvider`] is available (see [`TimerProvider::has_real_clock`]).
///
/// Bounds a single `poll_wire()` invocation's total wait for *new* bytes to arrive —
/// independent of, and strictly lower-level than, `PrinterClient::poll_until`'s
/// `command_timeout_secs`/`POLL_UNTIL_MAX_MESSAGES` valves (`src/client/mod.rs`), which
/// only ever run *after* a full frame has already been received and therefore cannot
/// catch a stall that happens mid-read [REF-MQTT-STALL]. A connection that stalls with
/// zero incoming bytes may take up to this long to surface as
/// `Error::Network(SocketError::TimedOut)`, even if the caller configured a
/// shorter `command_timeout_secs` — the two timeouts are independent layers, not summed
/// or coordinated.
pub(crate) const MQTT_READ_TIMEOUT_SECS: u64 = 30;

/// Per-call deadline for `write_frame_with_timer` when a genuine wall-clock [`TimerProvider`]
/// is available — the write-side counterpart to [`MQTT_READ_TIMEOUT_SECS`]. A stalled write
/// (e.g. the peer stopped reading its socket buffer) would otherwise block `write_all()`/
/// `flush()` forever, unlike the read path which already had this protection (BUG-159).
pub(crate) const MQTT_WRITE_TIMEOUT_SECS: u64 = 30;

/// Byte-level progress of an in-flight MQTT frame read, preserved across a timed-out `read_exact_packet` call so a subsequent call resumes exactly where the previous one left off instead of misinterpreting still-arriving bytes of the *same* frame as a new frame's header — see `read_exact_packet`'s doc comment for why losing this state would permanently desync the stream parser.
#[derive(Default)]
pub(crate) enum FrameReadState {
    /// No partial frame in progress — the next read starts a fresh header byte.
    #[default]
    Idle,
    /// Header byte read; the MQTT variable-length "remaining length" field is not yet fully decoded.
    ReadingRemainingLength {
        header: u8,
        value: usize,
        multiplier: usize,
    },
    /// Remaining length fully decoded; `buf` is pre-sized to the full payload length and accumulates bytes as they arrive, `filled` tracks how many are valid so far.
    ReadingPayload {
        header: u8,
        buf: Vec<u8>,
        filled: usize,
    },
}

/// Reads exactly one standard MQTT frame asynchronously from our abstract socket, resuming from `state` if a prior call on this same stream timed out partway through.
///
/// **Correctness invariant — never violate this:** on a `SocketError::TimedOut` return,
/// `state` must retain every byte already read for the in-progress frame. The MQTT wire
/// format has no resynchronization marker — if bytes already consumed from `stream` were
/// ever discarded here, the *next* call would start reading from the middle of whatever
/// the peer sends next, permanently desyncing the frame parser until the connection is
/// dropped and re-established (the same failure class as the `write_command` regression
/// documented in `CLAUDE.md`). This is why the payload is read via a loop of small
/// `read_chunk()` steps (each individually resumable) instead of one atomic multi-byte
/// read — see `read_chunk`'s doc comment for the cancellation-safety reasoning. A
/// `SocketError::ConnectionReset`/`InvalidInput` return means the connection itself is no
/// longer usable regardless of `state` — the caller must reconnect (constructing a new
/// `MqttClient`, and thus a fresh `FrameReadState`) rather than keep polling the
/// same stream.
///
/// Computes a fresh deadline every call from `budget_ms` (not once per logical frame) —
/// each call to this function gets its own bounded window to make progress, regardless
/// of how many prior calls already timed out waiting on this same in-progress frame.
/// Callers outside tests should pass `MQTT_READ_TIMEOUT_SECS * 1000`; tests use a small
/// `budget_ms` directly so stalled-read regression tests don't need to wait out the real
/// production timeout.
/// Reads exactly one byte from `stream`, retrying partial reads via `read_chunk`.
///
/// Either fully succeeds (one byte consumed and returned) or fails before any byte is
/// consumed — there's no partial-byte state for a caller to lose across a timeout, unlike
/// the multi-byte payload read in [`read_exact_packet`], which must stay a manual loop.
async fn read_one_byte<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    timer: &T,
    deadline_ms: Option<u64>,
) -> Result<u8, SocketError> {
    let mut b = [0u8; 1];
    let mut filled = 0;
    while filled < b.len() {
        let n = read_chunk(stream, &mut b[filled..], timer, deadline_ms).await?;
        filled += n;
    }
    Ok(b[0])
}

pub(crate) async fn read_exact_packet<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    state: &mut FrameReadState,
    timer: &T,
    budget_ms: u64,
) -> Result<(u8, Vec<u8>), SocketError> {
    let deadline_ms = if timer.has_real_clock() {
        Some(timer.now_millis().saturating_add(budget_ms))
    } else {
        None
    };

    // Fixed header packet type byte (only if not already read by a prior, timed-out call).
    if matches!(state, FrameReadState::Idle) {
        let header = read_one_byte(stream, timer, deadline_ms).await?;
        *state = FrameReadState::ReadingRemainingLength {
            header,
            value: 0,
            multiplier: 1,
        };
    }

    // Variable-length remaining length (resumes mid-varint if a prior call stalled here).
    if let FrameReadState::ReadingRemainingLength {
        header,
        value,
        multiplier,
    } = state
    {
        loop {
            let b = read_one_byte(stream, timer, deadline_ms).await?;
            *value += ((b & 127) as usize) * *multiplier;
            if (b & 128) == 0 {
                break;
            }
            *multiplier *= 128;
            if *multiplier > 128 * 128 * 128 {
                *state = FrameReadState::Idle;
                return Err(SocketError::InvalidInput); // Protocol violation
            }
        }

        let rem_len = *value;
        let hdr = *header;

        if rem_len > MQTT_MAX_PAYLOAD_BYTES {
            *state = FrameReadState::Idle;
            log::warn!("MQTT payload length {} exceeds maximum", rem_len);
            return Err(SocketError::InvalidInput);
        }

        *state = FrameReadState::ReadingPayload {
            header: hdr,
            buf: vec![0u8; rem_len],
            filled: 0,
        };
    }

    // Payload bytes (resumes from `filled` if a prior call stalled mid-payload).
    if let FrameReadState::ReadingPayload {
        header,
        buf,
        filled,
    } = state
    {
        while *filled < buf.len() {
            let n = read_chunk(stream, &mut buf[*filled..], timer, deadline_ms).await?;
            *filled += n;
        }
        let hdr = *header;
        let payload = core::mem::take(buf);
        *state = FrameReadState::Idle;
        return Ok((hdr, payload));
    }

    unreachable!("FrameReadState must be ReadingPayload after remaining-length decode")
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "tokio")]
    mod async_tests {
        use super::super::*;
        use crate::client::dummy::DummyTimer;
        use crate::io::TokioIo;
        use crate::mqtt::client::codec::encode_remaining_length;

        #[tokio::test]
        async fn test_read_exact_packet_oom_guard() {
            // Craft a packet with remaining length exceeding MQTT_MAX_PAYLOAD_BYTES (1 MiB)
            let oversized_len: usize = MQTT_MAX_PAYLOAD_BYTES + 1;
            let mut data = vec![0x30u8]; // PUBLISH header
            data.extend_from_slice(&encode_remaining_length(oversized_len));

            let cursor = std::io::Cursor::new(data);
            let mut stream = TokioIo(cursor);
            let mut state = FrameReadState::default();
            let result = read_exact_packet(
                &mut stream,
                &mut state,
                &DummyTimer,
                MQTT_READ_TIMEOUT_SECS * 1000,
            )
            .await;
            assert!(
                matches!(result, Err(crate::io::SocketError::InvalidInput)),
                "Expected InvalidInput for oversized payload, got {:?}",
                result
            );
        }

        #[tokio::test]
        async fn test_read_exact_packet_malformed_remaining_length() {
            // 5 continuation bytes → multiplier exceeds 128^3, protocol violation
            let data = vec![0x30, 0x80, 0x80, 0x80, 0x80, 0x01];
            let cursor = std::io::Cursor::new(data);
            let mut stream = TokioIo(cursor);
            let mut state = FrameReadState::default();
            let result = read_exact_packet(
                &mut stream,
                &mut state,
                &DummyTimer,
                MQTT_READ_TIMEOUT_SECS * 1000,
            )
            .await;
            assert!(
                matches!(result, Err(crate::io::SocketError::InvalidInput)),
                "Expected InvalidInput for malformed remaining length, got {:?}",
                result
            );
        }

        /// Regression test: a connection that stalls with zero incoming bytes must not hang `read_exact_packet`/`poll_wire` forever.
        /// Uses a `tokio::io::duplex` whose server side never writes anything, so the client's low-level
        /// `read()` call is genuinely pending (not merely slow) — exactly the "dead TCP, printer powered
        /// off mid-session" scenario the fix targets. Passes a small `budget_ms` directly (bypassing the
        /// real `MQTT_READ_TIMEOUT_SECS` constant) so this test doesn't need to wait out the production
        /// timeout. The outer `tokio::time::timeout` is a meta-safety net: if the implementation regresses
        /// to hanging forever, this test fails promptly instead of wedging the whole suite.
        #[tokio::test]
        async fn test_read_exact_packet_stalled_connection_times_out() {
            let (client_stream, _server_stream) = tokio::io::duplex(64);
            // Server side is kept alive (bound to `_server_stream`) but never writes —
            // dropping it would deliver `Ok(0)`/EOF instead of a genuine stall.

            let mut stream = TokioIo(client_stream);
            let mut state = FrameReadState::default();
            let timer = crate::io::tokio::TokioTimer::new();
            let budget_ms = 50;

            let started = std::time::Instant::now();
            let result = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                read_exact_packet(&mut stream, &mut state, &timer, budget_ms),
            )
            .await
            .expect(
                "read_exact_packet hung past the 5s meta-safety timeout instead of \
                 honoring its own budget — this is the exact regression this test guards \
                 against",
            );
            let elapsed = started.elapsed();

            assert!(
                matches!(result, Err(crate::io::SocketError::TimedOut)),
                "Expected TimedOut for a stalled connection, got {:?}",
                result
            );
            assert!(
                elapsed < core::time::Duration::from_secs(2),
                "read_exact_packet took {:?} to time out against a {}ms budget — too slow",
                elapsed,
                budget_ms
            );
        }

        /// Regression test for the correctness hinge above: bytes already read into a partial-packet buffer before a timeout must never be lost.
        /// Simulates a connection that delivers *part* of a frame, stalls long enough to time out, then
        /// delivers the rest — and asserts the second `read_exact_packet` call reconstructs the exact
        /// original frame (not corrupted, not desynced, not duplicated), proving `FrameReadState` correctly
        /// carried the partial payload across the timed-out attempt.
        #[tokio::test]
        async fn test_read_exact_packet_resumes_after_timeout_without_losing_bytes() {
            use tokio::io::AsyncWriteExt;

            let (client_stream, mut server_stream) = tokio::io::duplex(64);
            let mut stream = TokioIo(client_stream);
            let mut state = FrameReadState::default();
            let timer = crate::io::tokio::TokioTimer::new();

            // Full intended frame: header 0x99, remaining-length 4, payload [AA BB CC DD].
            // Server sends the header, remaining-length, and only the first 2 payload
            // bytes, then stops — the client will read header+remlen+2 payload bytes
            // successfully, then stall waiting for the last 2 payload bytes.
            server_stream
                .write_all(&[0x99, 0x04, 0xAA, 0xBB])
                .await
                .unwrap();
            server_stream.flush().await.unwrap();

            let first_attempt = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                read_exact_packet(&mut stream, &mut state, &timer, 50),
            )
            .await
            .expect("first read_exact_packet attempt hung past the meta-safety timeout");

            assert!(
                matches!(first_attempt, Err(crate::io::SocketError::TimedOut)),
                "Expected the first attempt to time out waiting on the missing payload \
                 bytes, got {:?}",
                first_attempt
            );

            // The partial frame must be preserved exactly: header captured, 2 of 4
            // payload bytes already landed correctly, nothing corrupted or lost.
            match &state {
                FrameReadState::ReadingPayload {
                    header,
                    buf,
                    filled,
                } => {
                    assert_eq!(*header, 0x99, "header byte must survive the timeout");
                    assert_eq!(
                        *filled, 2,
                        "exactly the 2 bytes that arrived must be recorded"
                    );
                    assert_eq!(
                        &buf[..2],
                        &[0xAA, 0xBB],
                        "already-read payload bytes must not be corrupted"
                    );
                }
                other => panic!(
                    "expected FrameReadState::ReadingPayload with 2 bytes filled after a \
                     mid-payload timeout, got a different state variant (state debug \
                     unavailable, matched arm: {})",
                    match other {
                        FrameReadState::Idle => "Idle",
                        FrameReadState::ReadingRemainingLength { .. } => "ReadingRemainingLength",
                        FrameReadState::ReadingPayload { .. } => unreachable!(),
                    }
                ),
            }

            // Now the rest of the frame arrives.
            server_stream.write_all(&[0xCC, 0xDD]).await.unwrap();
            server_stream.flush().await.unwrap();

            let second_attempt = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                read_exact_packet(&mut stream, &mut state, &timer, 2000),
            )
            .await
            .expect("second read_exact_packet attempt hung past the meta-safety timeout")
            .expect("second attempt should succeed now that the rest of the frame arrived");

            assert_eq!(
                second_attempt,
                (0x99u8, vec![0xAA, 0xBB, 0xCC, 0xDD]),
                "resumed read must reconstruct the exact original frame with no lost, \
                 duplicated, or reordered bytes"
            );
            assert!(
                matches!(state, FrameReadState::Idle),
                "state must reset to Idle after a fully-assembled frame is returned"
            );
        }
    }
}
