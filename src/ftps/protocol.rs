#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::error::BambuError;
use crate::io::{AsyncIo, SocketError};

// FTP response codes (RFC 959)
pub(crate) const FTP_GREETING: u16 = 220;
pub(crate) const FTP_TRANSFER_STARTING: u16 = 125;
pub(crate) const FTP_TRANSFER_OPENING: u16 = 150;
pub(crate) const FTP_SIZE_OK: u16 = 213;
pub(crate) const FTP_TRANSFER_COMPLETE: u16 = 226;
pub(crate) const FTP_PASSIVE_MODE: u16 = 227;
pub(crate) const FTP_LOGIN_OK: u16 = 230;
pub(crate) const FTP_FILE_ACTION_OK: u16 = 250;
pub(crate) const FTP_PATHNAME_CREATED: u16 = 257;
pub(crate) const FTP_PASSWORD_NEEDED: u16 = 331;
pub(crate) const FTP_RENAME_PENDING: u16 = 350;
pub(crate) const FTP_TRANSFER_ABORTED: u16 = 426;
pub(crate) const FTP_FILE_NOT_FOUND: u16 = 550;
pub(crate) const FTP_COMMAND_OK: u16 = 200;

pub(crate) const FTPS_UPLOAD_CHUNK_SIZE: usize = 65536;
pub(crate) const FTPS_DATA_READ_BUF_SIZE: usize = 4096;
pub(crate) const FTPS_PASV_PORT_MULTIPLIER: u16 = 256;
pub(crate) const FTP_MAX_RESPONSE_LINE_BYTES: usize = 4096;
pub(crate) const FTP_MAX_RESPONSE_LINES: usize = 100;
/// Size of each buffered socket read `read_line_raw` issues while scanning for `\n`. Control
/// responses are short ASCII text, so this comfortably holds several lines per read while
/// staying well under `FTP_MAX_RESPONSE_LINE_BYTES`.
pub(crate) const FTP_LINE_READ_CHUNK_SIZE: usize = 512;

/// Maximum bytes accepted from a single FTPS data-channel transfer (`list_directory`'s
/// listing payload, `download_file`'s file payload) before `read_to_eof` aborts with
/// `ProtocolViolation` rather than growing `out` without bound. Mirrors
/// `CAMERA_FRAME_MAX_SIZE`'s rationale (`src/camera/binary.rs`) — unbounded allocation on a
/// no_std/Embassy target hits the uncatchable `alloc_error_handler` abort, not a recoverable
/// `Result`. Chosen generously for legitimate large downloads (multi-hundred-MB timelapse
/// videos) while still bounding worst case. Fixed, not yet caller-configurable — unlike
/// camera's `with_max_frame_size`, there is currently no `BambuFtpsClient` builder to lower
/// this for embedded targets with tighter memory budgets.
pub(crate) const FTPS_MAX_TRANSFER_BYTES: usize = 512 * 1024 * 1024;

/// Sends a formatted ASCII FTP command string cleanly terminated with CRLF boundaries.
pub(crate) async fn write_command<IO: AsyncIo>(
    stream: &mut IO,
    cmd: &str,
) -> Result<(), BambuError> {
    // Single write_all call for "cmd\r\n" together — some embedded FTP servers (confirmed live
    // against a P1S) don't correctly reassemble a command line split across two separate writes.
    let mut payload = String::from(cmd);
    payload.push_str("\r\n");

    stream
        .write_all(payload.as_bytes())
        .await
        .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
    stream
        .flush()
        .await
        .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

    Ok(())
}

/// Reads a line-by-line buffer stream incrementally up to the terminating LF character.
///
/// Buffers socket reads into `fill_buf` and scans in memory for `\n`, only issuing another
/// socket read once the buffered bytes are exhausted (replaces a previous byte-at-a-time
/// `read_exact` loop, which cost one `AsyncIo::read_exact` call, and for TLS-wrapped streams
/// one record-layer round trip, per byte).
///
/// **Leftover-byte carry-over (the correctness hinge for this function):** `fill_buf` holds
/// bytes already pulled off the socket but not yet consumed into a returned line. FTP servers
/// routinely flush multiple lines back-to-back without waiting for the client — both within
/// one multi-line reply (e.g. `LIST`, `STAT`) and across logically separate replies to the
/// same command (e.g. `150` immediately followed by `226`, since nothing requires the server
/// to wait for the client to finish reading `150` first) — so a single socket read can
/// legitimately contain more than one line's worth of bytes, from more than one reply. Any
/// bytes past the first `\n` found in a given fill are left in `fill_buf` untouched rather
/// than discarded — the *next* call to `read_line_raw` consumes them first, before issuing
/// any further socket read. **Callers must pass the same `fill_buf` across every call against
/// a given stream for the life of that stream, not just within one `read_response` call** —
/// see `read_response`'s doc comment for a concrete failure this caused when that scoping was
/// too narrow, and `BambuFtpsClient::control_fill_buf` for how the sole caller now satisfies
/// this (a struct field threaded through every method, not a local per-response variable).
///
/// Enforces a maximum line length to prevent OOM from malformed server output.
async fn read_line_raw<IO: AsyncIo>(
    stream: &mut IO,
    line_buf: &mut Vec<u8>,
    fill_buf: &mut Vec<u8>,
) -> Result<(), BambuError> {
    line_buf.clear();
    loop {
        if let Some(pos) = fill_buf.iter().position(|&b| b == b'\n') {
            // Found a full line already sitting in the leftover buffer from a prior read —
            // consume only up through the newline; anything after stays in `fill_buf` for the
            // next call.
            line_buf.extend(fill_buf.drain(..=pos));
            return Ok(());
        }

        // No newline buffered yet: everything currently in `fill_buf` belongs to this line.
        line_buf.append(fill_buf);

        if line_buf.len() >= FTP_MAX_RESPONSE_LINE_BYTES {
            return Err(BambuError::ProtocolViolation(
                "FTP response line exceeds maximum length".into(),
            ));
        }

        let mut chunk = [0u8; FTP_LINE_READ_CHUNK_SIZE];
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionReset))?;
        if n == 0 {
            // EOF mid-line: the server closed the connection before terminating its response.
            return Err(BambuError::NetworkError(SocketError::ConnectionReset));
        }
        fill_buf.extend_from_slice(&chunk[..n]);
    }
}

/// Parses multi-line and single-line command channel response arrays returned by FTP servers.
///
/// Under RFC-959, standard command responses take the shape:
/// * Single-Line: `XYZ Response text\r\n`
/// * Multi-Line:
///   ```text
///   XYZ-Header description line\r\n
///    Intermediate content lines\r\n
///   XYZ Termination line\r\n
///   ```
/// Accumulates all response text across lines so multi-line body content (e.g., from STAT)
/// is preserved in the returned string.
///
/// `fill_buf` is `read_line_raw`'s leftover-byte carry buffer (see that function's doc
/// comment). **Callers must pass the same `fill_buf` across every `read_response` call made
/// against a given control-channel stream, for the life of that stream** — not just within
/// one call. A single socket read can pull in bytes belonging to a *later*, logically
/// separate response: FTP servers are not required to wait for the client to finish reading
/// one reply before writing the next (e.g. a server may write `150 ...` and then, without any
/// further input from the client, go on to write the eventual `226 ...` for the same command
/// soon after) — confirmed via `tests/ftps_test.rs::test_ftps_download_file` failing when an
/// earlier version of this function scoped `fill_buf` to a single call: the `150`/`226` pair
/// for one `RETR` arrived in one control-channel read, and scoping the leftover buffer to one
/// `read_response` call silently dropped the buffered `226` bytes, desyncing the next read.
/// `BambuFtpsClient` (`src/ftps/client.rs`) holds its `fill_buf` as a field for exactly this
/// reason, threading it through every method's `read_response` call.
pub(crate) async fn read_response<IO: AsyncIo>(
    stream: &mut IO,
    line_buf: &mut Vec<u8>,
    fill_buf: &mut Vec<u8>,
) -> Result<(u16, String), BambuError> {
    let mut accumulated = String::new();
    let mut lines_read: usize = 0;

    loop {
        read_line_raw(stream, line_buf, fill_buf).await?;
        lines_read += 1;
        if lines_read > FTP_MAX_RESPONSE_LINES {
            return Err(BambuError::ProtocolViolation(
                "FTP response exceeded maximum line count".into(),
            ));
        }
        if line_buf.len() < 4 {
            log::debug!(
                "skipping malformed FTP response line ({} bytes)",
                line_buf.len()
            );
            continue;
        }
        let code_str = match core::str::from_utf8(&line_buf[0..3]) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let code = match code_str.parse::<u16>() {
            Ok(c) => c,
            Err(_) => continue,
        };
        let separator = line_buf[3];

        if separator == b' ' {
            let text = core::str::from_utf8(&line_buf[4..]).unwrap_or("").trim();
            if accumulated.is_empty() {
                return Ok((code, text.to_string()));
            }
            if !text.is_empty() {
                accumulated.push('\n');
                accumulated.push_str(text);
            }
            return Ok((code, accumulated));
        } else if separator == b'-' {
            let line_text = core::str::from_utf8(&line_buf[4..]).unwrap_or("").trim();
            if !accumulated.is_empty() {
                accumulated.push('\n');
            }
            accumulated.push_str(line_text);
        }
    }
}

/// Extracts the passive port number from a PASV response text.
///
/// Parses the `(IP_1,IP_2,IP_3,IP_4,PORT_1,PORT_2)` tuple and computes
/// the port as `PORT_1 * 256 + PORT_2`.
pub(crate) fn parse_pasv_port(text: &str) -> Result<u16, BambuError> {
    let start = text
        .find('(')
        .ok_or(BambuError::ProtocolViolation("Invalid PASV format".into()))?;
    let end = text[start + 1..]
        .find(')')
        .map(|e| e + start + 1)
        .ok_or(BambuError::ProtocolViolation("Invalid PASV format".into()))?;
    let inner = &text[start + 1..end];
    let mut parts = inner.split(',');

    let _ = parts.next();
    let _ = parts.next();
    let _ = parts.next();
    let _ = parts.next();

    let p1 =
        parts
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or(BambuError::ProtocolViolation(
                "Failed to parse PORT_1 in PASV".into(),
            ))?;
    let p2 =
        parts
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or(BambuError::ProtocolViolation(
                "Failed to parse PORT_2 in PASV".into(),
            ))?;

    let port = (p1 as u32) * (FTPS_PASV_PORT_MULTIPLIER as u32) + (p2 as u32);
    if port > u16::MAX as u32 {
        return Err(BambuError::ProtocolViolation(
            "PASV port value out of range".into(),
        ));
    }
    Ok(port as u16)
}

/// Validates a caller-supplied FTP path argument before it is interpolated into a command line.
///
/// Every path-taking method on `BambuFtpsClient` sends `format!("CMD {}", path)` followed by a
/// single trailing CRLF (`write_command`). If `path` itself contains `\r` or `\n`, the bytes
/// written to the control channel contain an embedded line break that the FTP server parses as a
/// written to the control channel contain an embedded line break that the FTP server parses as
/// a *second*, caller/attacker-controlled command — invisible to whoever called the original
/// method. Also rejects NUL (`\0`), which some FTP daemons treat as a string terminator, for
/// the same class of confusion.
pub(crate) fn validate_ftp_path(path: &str) -> Result<(), BambuError> {
    if path.bytes().any(|b| b == b'\r' || b == b'\n' || b == 0) {
        return Err(BambuError::ProtocolViolation(
            "FTP path contains an illegal control character (CR, LF, or NUL)".into(),
        ));
    }
    Ok(())
}

/// Utility capturing passive stream data up to socket EOF bounds.
pub(crate) async fn read_to_eof<IO: AsyncIo>(
    stream: &mut IO,
    out: &mut Vec<u8>,
) -> Result<(), BambuError> {
    let mut chunk = [0u8; FTPS_DATA_READ_BUF_SIZE];
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                if out.len() + n > FTPS_MAX_TRANSFER_BYTES {
                    return Err(BambuError::ProtocolViolation(
                        "FTPS transfer exceeds maximum accepted size".into(),
                    ));
                }
                out.extend_from_slice(&chunk[..n]);
            }
            Err(_) => return Err(BambuError::NetworkError(SocketError::ConnectionAborted)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::TokioIo;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};

    /// Records each individual `poll_write` call as its own chunk — lets a test assert
    /// how many separate writes a function issued, not just the concatenated bytes.
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

    /// Returns one queued chunk per `poll_read` call — lets a test control exactly how many
    /// bytes a single underlying socket read returns, to exercise `read_line_raw`'s buffered
    /// leftover-carry behavior deterministically. Once the queue is drained, further reads
    /// report EOF (0 bytes), which is fine since these tests only ever issue as many reads as
    /// chunks provided.
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

        read_line_raw(&mut stream, &mut line_buf, &mut fill_buf)
            .await
            .expect("first line");
        assert_eq!(line_buf, b"150 Opening data connection\r\n");

        read_line_raw(&mut stream, &mut line_buf, &mut fill_buf)
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

        read_line_raw(&mut stream, &mut line_buf, &mut fill_buf)
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

        let (code, text) = read_response(&mut stream, &mut line_buf, &mut fill_buf)
            .await
            .expect("multi-line response");
        assert_eq!(code, 213);
        assert_eq!(text, "First line\nSecond line\nFinal line");
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

        let (code, _) = read_response(&mut stream, &mut line_buf, &mut fill_buf)
            .await
            .expect("first reply");
        assert_eq!(code, 150);

        let (code, _) = read_response(&mut stream, &mut line_buf, &mut fill_buf)
            .await
            .expect("second reply, from carried-over leftover bytes only");
        assert_eq!(code, 226);
    }

    /// Returns a fixed-size nonzero chunk on every `poll_read` call, forever — never signals
    /// EOF. Used to exercise `read_to_eof`'s size cap against a stream that never stops
    /// sending data.
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
        // A stream that never sends EOF and exceeds FTPS_MAX_TRANSFER_BYTES must error
        // cleanly instead of growing `out` without bound.
        let reader = InfiniteReader {
            chunk_len: FTPS_DATA_READ_BUF_SIZE,
        };
        let mut stream = TokioIo(reader);
        let mut out = Vec::new();

        let result = read_to_eof(&mut stream, &mut out).await;
        assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
        assert!(out.len() <= FTPS_MAX_TRANSFER_BYTES);
    }

    #[tokio::test]
    async fn test_write_command_sends_single_write_call() {
        // Regression test: write_command must send "cmd\r\n" as one write_all call, not two
        // separate ones. Some embedded FTP servers (confirmed live against a Bambu P1S) don't
        // reliably reassemble a command line split across two writes/TLS records — a bug
        // introduced in commit 6385019 and fixed by combining back into a single write.
        let recorder = WriteRecorder::default();
        let mut stream = TokioIo(recorder.clone());

        write_command(&mut stream, "USER bblp").await.unwrap();

        let calls = recorder.0.lock().unwrap();
        assert_eq!(
            calls.len(),
            1,
            "write_command must issue exactly one write call, got {}: {calls:?}",
            calls.len()
        );
        assert_eq!(calls[0], b"USER bblp\r\n");
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
        assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
    }

    #[test]
    fn test_pasv_non_numeric_port() {
        let result = parse_pasv_port("(127,0,0,1,abc,168)");
        assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
    }

    #[test]
    fn test_pasv_incomplete_components() {
        let result = parse_pasv_port("(127,0,0,1,192)");
        assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
    }

    #[test]
    fn test_pasv_empty_parens() {
        let result = parse_pasv_port("()");
        assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
    }

    #[test]
    fn test_pasv_port_overflow() {
        let result = parse_pasv_port("(127,0,0,1,256,0)");
        assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
    }

    #[test]
    fn test_pasv_reversed_parentheses_does_not_panic() {
        // Regression test: a ')' appearing before a '(' used to make `start + 1..end` a
        // reversed range, which panics. Must return a clean error instead of crashing.
        let result = parse_pasv_port("227 Response ) some text ( more");
        assert!(matches!(result, Err(BambuError::ProtocolViolation(_))));
    }
}
