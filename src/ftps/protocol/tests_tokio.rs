//! FTPS control/data-channel tests that genuinely need a real platform timer and a real async
//! transport.
//!
//! Split out of `tests.rs` and gated on `feature = "tokio"` per #291: everything there runs under
//! every feature set against `crate::test_support::MockIo`, so the no_std unit-test surface stays
//! compilable. These four cannot follow, because each asserts *wall-clock* stall behaviour — a
//! socket that is genuinely pending rather than merely empty, raced against a timer whose sleep
//! really elapses. `MockIo` always completes a read immediately and `MockTimer`'s sleep returns
//! instantly, so neither can model the stall these guard against.

use super::*;
use crate::io::TokioIo;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

/// Records each individual `poll_write` call as its own chunk, and never completes a read — a
/// genuinely stalled socket (`Poll::Pending`), which is what the timeout tests below need and
/// `MockIo` deliberately cannot provide.
#[derive(Clone, Default)]
struct StalledStream(Arc<Mutex<Vec<Vec<u8>>>>);

impl tokio::io::AsyncRead for StalledStream {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
}

impl tokio::io::AsyncWrite for StalledStream {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.0.lock().unwrap().push(buf.to_vec());
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// Regression test: a read deadline firing mid-line must not discard the bytes already read.
/// `read_line_raw` used to `append` the leftover buffer into the per-call `line_buf` before every
/// socket read, so a timeout dropped the partial line on the floor and the next call resumed
/// mid-line — desyncing the reply parser. Nothing structural prevented that; only
/// `.claude/rules/ftps-poisoning.md`'s "never un-poison" convention kept it from being observable.
#[tokio::test]
async fn test_read_response_keeps_partial_line_across_a_timeout() {
    let (client_half, mut server_half) = tokio::io::duplex(4096);
    let mut stream = TokioIo(client_half);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();
    let timer = crate::io::tokio::TokioTimer::new();

    // Half a reply arrives, then the server goes silent past the deadline.
    tokio::io::AsyncWriteExt::write_all(&mut server_half, b"226 Transfer com")
        .await
        .expect("partial reply write");

    let deadline_ms = Some(timer.now_millis().saturating_add(50));
    let result = read_response(
        &mut stream,
        &mut line_buf,
        &mut fill_buf,
        &timer,
        deadline_ms,
    )
    .await;
    assert!(
        matches!(result, Err(Error::Network(SocketError::TimedOut))),
        "expected the stall deadline to fire, got {:?}",
        result
    );

    // The rest arrives; a fresh call must reassemble the whole line from the retained prefix.
    tokio::io::AsyncWriteExt::write_all(&mut server_half, b"plete.\r\n")
        .await
        .expect("remainder write");

    let deadline_ms = Some(timer.now_millis().saturating_add(5_000));
    let (code, text) = read_response(
        &mut stream,
        &mut line_buf,
        &mut fill_buf,
        &timer,
        deadline_ms,
    )
    .await
    .expect("second read must complete the line held over from the timed-out call");
    assert_eq!(code, 226);
    assert_eq!(text, "Transfer complete.");
}

/// Regression test mirroring `read_exact_packet`'s `test_read_exact_packet_stalled_connection_times_out`: a data channel that stalls with zero incoming bytes (e.g. firmware hang mid-transfer) must not hang `read_to_eof` forever.
/// `StalledStream`'s `poll_read` always returns `Pending`, simulating a genuinely stalled socket
/// rather than a merely slow or closed one. The outer `tokio::time::timeout` is a meta-safety net —
/// if the implementation regresses to hanging forever, this test fails promptly instead of wedging
/// the whole suite.
#[tokio::test]
async fn test_read_to_eof_stalled_connection_times_out() {
    let mut stream = TokioIo(StalledStream::default());
    let mut out = Vec::new();
    let timer = crate::io::tokio::TokioTimer::new();
    let budget_ms = 50;

    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        core::time::Duration::from_secs(5),
        read_to_eof(&mut stream, &mut out, &timer, budget_ms),
    )
    .await
    .expect(
        "read_to_eof hung past the 5s meta-safety timeout instead of honoring its own \
         budget — this is the exact regression this test guards against",
    );
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(Error::Network(SocketError::TimedOut))),
        "Expected TimedOut for a stalled connection, got {:?}",
        result
    );
    assert!(
        elapsed < core::time::Duration::from_secs(2),
        "read_to_eof took {:?} to time out against a {}ms budget — too slow",
        elapsed,
        budget_ms
    );
}

/// Regression test mirroring the above, at the control-channel `read_response` level: a control channel that stalls with zero incoming bytes (e.g. after a `150`/`125` reply, before the eventual `226`) must not hang `read_response` forever.
#[tokio::test]
async fn test_read_response_stalled_connection_times_out() {
    let mut stream = TokioIo(StalledStream::default());
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();
    let timer = crate::io::tokio::TokioTimer::new();
    let budget_ms = 50;
    let deadline_ms = Some(timer.now_millis().saturating_add(budget_ms));

    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        core::time::Duration::from_secs(5),
        read_response(
            &mut stream,
            &mut line_buf,
            &mut fill_buf,
            &timer,
            deadline_ms,
        ),
    )
    .await
    .expect(
        "read_response hung past the 5s meta-safety timeout instead of honoring its own \
         budget — this is the exact regression this test guards against",
    );
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(Error::Network(SocketError::TimedOut))),
        "Expected TimedOut for a stalled connection, got {:?}",
        result
    );
    assert!(
        elapsed < core::time::Duration::from_secs(2),
        "read_response took {:?} to time out against a {}ms budget — too slow",
        elapsed,
        budget_ms
    );
}

/// Regression test: `write_command` had no deadline at all, so a printer wedged with a full
/// receive window blocked every control-channel command (SIZE/DELE/MKD/PASV/QUIT) forever, and
/// the caller never reached its poisoning path. A 1-byte duplex whose peer never reads models
/// exactly that: the first byte lands, the rest of the write blocks indefinitely.
#[tokio::test]
async fn test_write_command_stalled_connection_times_out() {
    let (client_half, _server_half) = tokio::io::duplex(1);
    let mut stream = TokioIo(client_half);
    let timer = crate::io::tokio::TokioTimer::new();
    let budget_ms = 50;
    let deadline_ms = Some(timer.now_millis().saturating_add(budget_ms));

    let started = std::time::Instant::now();
    let result = tokio::time::timeout(
        core::time::Duration::from_secs(5),
        write_command(&mut stream, "USER bblp", &timer, deadline_ms),
    )
    .await
    .expect(
        "write_command hung past the 5s meta-safety timeout instead of honoring its own \
         budget — this is the exact regression this test guards against",
    );
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(Error::Network(SocketError::TimedOut))),
        "Expected TimedOut for a stalled control channel, got {:?}",
        result
    );
    assert!(
        elapsed < core::time::Duration::from_secs(2),
        "write_command took {:?} to time out against a {}ms budget — too slow",
        elapsed,
        budget_ms
    );
}
