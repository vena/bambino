use super::*;
use crate::client::dummy::DummyTimer;
use crate::io::TokioIo;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

/// Records each individual `poll_write` call as its own chunk — lets a test assert how many separate writes a function issued, not just the concatenated bytes.
#[derive(Clone, Default)]
struct WriteRecorder(Arc<Mutex<Vec<Vec<u8>>>>);

impl tokio::io::AsyncRead for WriteRecorder {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
}

impl tokio::io::AsyncWrite for WriteRecorder {
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

/// Returns one queued chunk per `poll_read` call — lets a test control exactly how many bytes a single underlying socket read returns, to exercise `read_line_raw`'s buffered leftover-carry behavior deterministically.
/// Once the queue is drained, further reads report EOF (0 bytes), which is fine since these tests
/// only ever issue as many reads as chunks provided.
#[derive(Clone, Default)]
struct ChunkedReader(Arc<Mutex<std::collections::VecDeque<Vec<u8>>>>);

impl ChunkedReader {
    fn with_chunks(chunks: &[&[u8]]) -> Self {
        let queue = chunks.iter().map(|c| c.to_vec()).collect();
        Self(Arc::new(Mutex::new(queue)))
    }
}

impl tokio::io::AsyncRead for ChunkedReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if let Some(chunk) = self.0.lock().unwrap().pop_front() {
            buf.put_slice(&chunk);
        }
        Poll::Ready(Ok(()))
    }
}

impl tokio::io::AsyncWrite for ChunkedReader {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn test_read_line_raw_carries_leftover_bytes_across_calls() {
    // Regression test: a single socket read delivering two full
    // FTP response lines back-to-back must not lose the second line. The first call to
    // `read_line_raw` must return only the first line; the second call must return the
    // second line using the bytes already buffered from the first read, without issuing
    // any further socket read (the mock's queue only has one chunk).
    let reader = ChunkedReader::with_chunks(&[
        b"150 Opening data connection\r\n226 Transfer complete\r\n",
    ]);
    let mut stream = TokioIo(reader);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    read_line_raw(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
        .await
        .expect("first line");
    assert_eq!(line_buf, b"150 Opening data connection\r\n");

    read_line_raw(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
        .await
        .expect("second line");
    assert_eq!(line_buf, b"226 Transfer complete\r\n");
}

#[tokio::test]
async fn test_read_line_raw_assembles_line_split_across_reads() {
    // A line with no '\n' in the first socket read (partial line) must still be assembled
    // correctly once the rest arrives in a second read.
    let reader = ChunkedReader::with_chunks(&[b"220 Wel", b"come\r\n"]);
    let mut stream = TokioIo(reader);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    read_line_raw(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
        .await
        .expect("assembled line");
    assert_eq!(line_buf, b"220 Welcome\r\n");
}

#[tokio::test]
async fn test_read_response_multiline_in_single_socket_read() {
    // End-to-end through `read_response`: a multi-line response (code-prefixed
    // continuation lines, per this parser's supported format) delivered in a *single*
    // socket read must still parse into the correct accumulated text across all three
    // lines — exercising the leftover-carry path via the public entry point rather than
    // calling `read_line_raw` directly.
    let reader = ChunkedReader::with_chunks(&[
        b"213-First line\r\n213-Second line\r\n213 Final line\r\n",
    ]);
    let mut stream = TokioIo(reader);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    let (code, text) =
        read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
            .await
            .expect("multi-line response");
    assert_eq!(code, 213);
    assert_eq!(text, "First line\nSecond line\nFinal line");
}

#[tokio::test]
async fn test_read_response_intermediate_line_matching_terminator_shape_not_mistaken() {
    // RFC 959 §4.2 explicitly warns that an intermediate line can itself start
    // with a 3-digit-number-plus-space sequence — it must not be mistaken for the
    // terminator unless its code also matches the reply's opening code.
    let reader = ChunkedReader::with_chunks(&[
        b"213-Header\r\n150 looks like a terminator but isn't\r\n213 Final line\r\n",
    ]);
    let mut stream = TokioIo(reader);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    let (code, text) =
        read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
            .await
            .expect("multi-line response");
    assert_eq!(code, 213);
    assert_eq!(
        text,
        "Header\n150 looks like a terminator but isn't\nFinal line"
    );
}

#[tokio::test]
async fn test_read_response_free_text_intermediate_line_preserved() {
    // RFC 959 §4.2 — intermediate lines aren't required to carry any code prefix
    // at all; free text must be preserved verbatim, not silently dropped.
    let reader =
        ChunkedReader::with_chunks(&[b"213-Header\r\nplain free text, no code\r\n213 End\r\n"]);
    let mut stream = TokioIo(reader);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    let (code, text) =
        read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
            .await
            .expect("multi-line response");
    assert_eq!(code, 213);
    assert_eq!(text, "Header\nplain free text, no code\nEnd");
}

#[tokio::test]
async fn test_read_response_header_with_no_separator_treated_as_terminal() {
    // A header line whose 4th byte is neither ' ' nor '-' (e.g. code immediately
    // followed by CRLF, no separator at all) used to fall through and be silently discarded
    // instead of surfacing as a reply.
    let reader = ChunkedReader::with_chunks(&[b"200\r\n"]);
    let mut stream = TokioIo(reader);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    let (code, text) =
        read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
            .await
            .expect("non-conformant header line should still produce a reply");
    assert_eq!(code, 200);
    assert_eq!(text, "");
}

#[tokio::test]
async fn test_read_response_leftover_bytes_carry_to_next_call() {
    // Regression test for the bug this design change fixes: FTP servers may write two
    // logically separate replies to the same command (e.g. `150` then `226`) without
    // waiting for the client to finish reading the first — both can land in one socket
    // read. `fill_buf` must be threaded across *both* `read_response` calls (as
    // `BambuFtpsClient` does via its `control_fill_buf` field) so the second call sees the
    // already-buffered `226` line instead of blocking on a socket read that never comes.
    let reader = ChunkedReader::with_chunks(&[
        b"150 Opening data connection.\r\n226 Transfer complete.\r\n",
    ]);
    let mut stream = TokioIo(reader);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    let (code, _) = read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
        .await
        .expect("first reply");
    assert_eq!(code, 150);

    let (code, _) = read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
        .await
        .expect("second reply, from carried-over leftover bytes only");
    assert_eq!(code, 226);
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
    let result = read_response(&mut stream, &mut line_buf, &mut fill_buf, &timer, deadline_ms).await;
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
    let (code, text) = read_response(&mut stream, &mut line_buf, &mut fill_buf, &timer, deadline_ms)
        .await
        .expect("second read must complete the line held over from the timed-out call");
    assert_eq!(code, 226);
    assert_eq!(text, "Transfer complete.");
}

/// Returns a fixed-size nonzero chunk on every `poll_read` call, forever — never signals EOF.
/// Used to exercise `read_to_eof`'s size cap against a stream that never stops sending data.
#[derive(Clone)]
struct InfiniteReader {
    chunk_len: usize,
}

impl tokio::io::AsyncRead for InfiniteReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let chunk = vec![0u8; self.chunk_len];
        buf.put_slice(&chunk);
        Poll::Ready(Ok(()))
    }
}

impl tokio::io::AsyncWrite for InfiniteReader {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Ready(Ok(buf.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn test_read_to_eof_rejects_oversized_transfer() {
    // A stream that never sends EOF and exceeds the transfer cap must error cleanly instead of
    // growing `out` without bound. Driven through `read_to_eof_bounded` with a small cap: the
    // real `FTPS_MAX_TRANSFER_BYTES` is 512 MiB, so running this against the production
    // constant allocated half a gigabyte (peaking near 1 GiB through `Vec` doubling) on every
    // test run, pre-commit hook, and CI job. `read_to_eof` is a thin delegation to this
    // function with the constant, so the abort path under test is the same one.
    const TEST_MAX_BYTES: usize = FTPS_DATA_READ_BUF_SIZE * 4;
    let reader = InfiniteReader {
        chunk_len: FTPS_DATA_READ_BUF_SIZE,
    };
    let mut stream = TokioIo(reader);
    let mut out = Vec::new();

    let result =
        read_to_eof_bounded(&mut stream, &mut out, &DummyTimer, 30_000, TEST_MAX_BYTES).await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));
    assert!(out.len() <= TEST_MAX_BYTES);
}

/// Regression test mirroring `read_exact_packet`'s `test_read_exact_packet_stalled_connection_times_out`: a data channel that stalls with zero incoming bytes (e.g. firmware hang mid-transfer) must not hang `read_to_eof` forever.
/// `WriteRecorder`'s `poll_read` always returns `Pending`, simulating a genuinely stalled socket
/// rather than a merely slow or closed one. The outer `tokio::time::timeout` is a meta-safety net —
/// if the implementation regresses to hanging forever, this test fails promptly instead of wedging
/// the whole suite.
#[tokio::test]
async fn test_read_to_eof_stalled_connection_times_out() {
    let mut stream = TokioIo(WriteRecorder::default());
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
    let mut stream = TokioIo(WriteRecorder::default());
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

#[tokio::test]
async fn test_write_command_sends_single_write_call() {
    // Regression test: write_command must send "cmd\r\n" as one write_all call, not two
    // separate ones. Some embedded FTP servers (confirmed live against a Bambu P1S) don't
    // reliably reassemble a command line split across two writes/TLS records.
    let recorder = WriteRecorder::default();
    let mut stream = TokioIo(recorder.clone());

    write_command(&mut stream, "USER bblp", &DummyTimer, None)
        .await
        .unwrap();

    let calls = recorder.0.lock().unwrap();
    assert_eq!(
        calls.len(),
        1,
        "write_command must issue exactly one write call, got {}: {calls:?}",
        calls.len()
    );
    assert_eq!(calls[0], b"USER bblp\r\n");
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

#[test]
fn test_valid_pasv_response() {
    let port =
        parse_pasv_port("Entering Passive Mode (127,0,0,1,192,168).").expect("valid PASV");
    assert_eq!(port, 49320);
}

#[test]
fn test_pasv_port_zero() {
    let port = parse_pasv_port("Entering Passive Mode (127,0,0,1,0,21).").expect("valid PASV");
    assert_eq!(port, 21);
}

#[test]
fn test_pasv_missing_parentheses() {
    let result = parse_pasv_port("227 No parentheses here");
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));
}

#[test]
fn test_pasv_tolerates_whitespace_after_commas() {
    // A server formatting the tuple as `(127, 0, 0, 1, 192, 168)` yields `" 192"` per
    // component, which Rust's integer parser rejects (no leading-whitespace skipping) — the
    // connection failed with "Failed to parse PORT_1 in PASV" even though the reply is
    // unambiguous. Not RFC 959-legal, but the rest of this parser is deliberately lenient.
    let port = parse_pasv_port("Entering Passive Mode (127, 0, 0, 1, 192, 168).")
        .expect("PASV with spaces after commas");
    assert_eq!(port, 49320);
}

#[test]
fn test_pasv_non_numeric_port() {
    let result = parse_pasv_port("(127,0,0,1,abc,168)");
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));
}

#[test]
fn test_pasv_incomplete_components() {
    let result = parse_pasv_port("(127,0,0,1,192)");
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));
}

#[test]
fn test_pasv_empty_parens() {
    let result = parse_pasv_port("()");
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));
}

#[test]
fn test_pasv_port_overflow() {
    let result = parse_pasv_port("(127,0,0,1,256,0)");
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));
}

#[test]
fn test_pasv_reversed_parentheses_does_not_panic() {
    // Regression test: a ')' appearing before a '(' used to make `start + 1..end` a
    // reversed range, which panics. Must return a clean error instead of crashing.
    let result = parse_pasv_port("227 Response ) some text ( more");
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));
}

#[test]
fn test_validate_ftp_path_rejects_traversal_segment() {
    assert!(matches!(
        validate_ftp_path("../../etc/passwd"),
        Err(Error::ProtocolViolation(_))
    ));
    assert!(matches!(
        validate_ftp_path("foo/../bar"),
        Err(Error::ProtocolViolation(_))
    ));
    assert!(matches!(
        validate_ftp_path("foo\\..\\bar"),
        Err(Error::ProtocolViolation(_))
    ));
}

#[test]
fn test_validate_ftp_path_allows_literal_dots_in_filename() {
    // Segment-wise matching only: a filename that merely contains the substring ".."
    // (not as a whole path segment) must not be spuriously rejected.
    assert!(validate_ftp_path("/cache/model..with..dots.3mf").is_ok());
    assert!(validate_ftp_path("my..cool..file.gcode").is_ok());
}

#[test]
fn test_validate_ftp_path_rejects_leading_dash_in_final_segment() {
    assert!(matches!(
        validate_ftp_path("-rf"),
        Err(Error::ProtocolViolation(_))
    ));
    assert!(matches!(
        validate_ftp_path("/cache/-file.3mf"),
        Err(Error::ProtocolViolation(_))
    ));
    // A leading-dash directory component earlier in the path is not the same hazard.
    assert!(validate_ftp_path("/-oddly-named-dir/file.3mf").is_ok());
    // A trailing slash made `.next_back()` return "" (the empty segment after
    // the slash), silently skipping the check on the actual dash-prefixed final directory.
    assert!(matches!(
        validate_ftp_path("/cache/-dir/"),
        Err(Error::ProtocolViolation(_))
    ));
}

#[test]
fn test_validate_ftp_path_rejects_crlf_and_nul_injection() {
    // The command-injection case `validate_ftp_path` exists for: a `\r\n` inside a path is a
    // second, caller-invisible command on the control channel. Covered only incidentally by
    // the `b < FTP_PATH_CONTROL_CHAR_MAX` predicate, so narrowing that predicate to an
    // allow-list (plausible — it also rejects tab, stricter than any FTP spec) could weaken
    // the guard with the suite still green.
    assert!(matches!(
        validate_ftp_path("/model/a\r\nDELE /model/b"),
        Err(Error::ProtocolViolation(_))
    ));
    assert!(matches!(
        validate_ftp_path("/model/a\nDELE /model/b"),
        Err(Error::ProtocolViolation(_))
    ));
    assert!(matches!(
        validate_ftp_path("/model/a\r"),
        Err(Error::ProtocolViolation(_))
    ));
    assert!(matches!(
        validate_ftp_path("/model/job\0.3mf"),
        Err(Error::ProtocolViolation(_))
    ));
}

#[test]
fn test_validate_ftp_path_rejects_non_crlf_control_chars() {
    assert!(matches!(
        validate_ftp_path("/cache/\x01file.3mf"),
        Err(Error::ProtocolViolation(_))
    ));
    assert!(matches!(
        validate_ftp_path("/cache/file\x7f.3mf"),
        Err(Error::ProtocolViolation(_))
    ));
}
