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
pub(crate) const FTP_STAT_OK: u16 = 211;
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

pub(crate) const FTPS_IMPLICIT_PORT: u16 = 990;
pub(crate) const FTPS_UPLOAD_CHUNK_SIZE: usize = 65536;
pub(crate) const FTPS_DATA_READ_BUF_SIZE: usize = 4096;
pub(crate) const FTPS_AVBL_SIZE_HEURISTIC_THRESHOLD: u64 = 100_000_000;
pub(crate) const FTPS_PASV_PORT_MULTIPLIER: u16 = 256;
pub(crate) const FTP_MAX_RESPONSE_LINE_BYTES: usize = 4096;
pub(crate) const FTP_MAX_RESPONSE_LINES: usize = 100;

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
/// Enforces a maximum line length to prevent OOM from malformed server output.
async fn read_line_raw<IO: AsyncIo>(
    stream: &mut IO,
    line_buf: &mut Vec<u8>,
) -> Result<(), BambuError> {
    line_buf.clear();
    let mut byte = [0u8; 1];
    loop {
        stream
            .read_exact(&mut byte)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionReset))?;
        let b = byte[0];
        line_buf.push(b);
        if b == b'\n' {
            break;
        }
        if line_buf.len() >= FTP_MAX_RESPONSE_LINE_BYTES {
            return Err(BambuError::ProtocolViolation(
                "FTP response line exceeds maximum length".into(),
            ));
        }
    }
    Ok(())
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
pub(crate) async fn read_response<IO: AsyncIo>(
    stream: &mut IO,
    line_buf: &mut Vec<u8>,
) -> Result<(u16, String), BambuError> {
    let mut accumulated = String::new();
    let mut lines_read: usize = 0;

    loop {
        read_line_raw(stream, line_buf).await?;
        lines_read += 1;
        if lines_read > FTP_MAX_RESPONSE_LINES {
            return Err(BambuError::ProtocolViolation(
                "FTP response exceeded maximum line count".into(),
            ));
        }
        if line_buf.len() < 4 {
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

/// Utility capturing passive stream data up to socket EOF bounds.
pub(crate) async fn read_to_eof<IO: AsyncIo>(
    stream: &mut IO,
    out: &mut Vec<u8>,
) -> Result<(), BambuError> {
    let mut chunk = [0u8; FTPS_DATA_READ_BUF_SIZE];
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&chunk[..n]),
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
