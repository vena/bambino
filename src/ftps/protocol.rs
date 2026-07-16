#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::error::Error;
use crate::io::{AsyncIo, Raced, SocketError, TimerProvider, race, read_chunk};

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
/// Reply-code prefix width in an FTP response line (e.g. `"200"`, `"550"`) — RFC 959 §4.2.
pub(crate) const FTP_REPLY_CODE_LEN: usize = 3;
/// Byte offset immediately after the reply code, where the separator (`' '` or `'-'`) sits.
pub(crate) const FTP_REPLY_SEPARATOR_OFFSET: usize = FTP_REPLY_CODE_LEN;
/// Byte offset where the reply text begins, after the code and its separator.
pub(crate) const FTP_REPLY_TEXT_OFFSET: usize = FTP_REPLY_CODE_LEN + 1;
/// Exclusive upper bound of the ASCII control-character range (space, `0x20`) rejected by
/// `validate_ftp_path` — anything below this is a control character.
pub(crate) const FTP_PATH_CONTROL_CHAR_MAX: u8 = 0x20;
/// The DEL control character (`0x7F`), rejected by `validate_ftp_path` alongside the
/// low control-character range above it.
pub(crate) const FTP_PATH_DEL_CHAR: u8 = 0x7F;
/// Size of each buffered socket read `read_line_raw` issues while scanning for `\n`.
/// Control responses are short ASCII text, so this comfortably holds several lines per read while
/// staying well under `FTP_MAX_RESPONSE_LINE_BYTES`.
pub(crate) const FTP_LINE_READ_CHUNK_SIZE: usize = 512;

/// Maximum bytes accepted from a single FTPS data-channel transfer (`list_directory`'s listing payload, `download_file`'s file payload) before `read_to_eof` aborts with `ProtocolViolation` rather than growing `out` without bound.
/// Mirrors `CAMERA_FRAME_MAX_SIZE`'s rationale (`src/camera/binary.rs`) — unbounded allocation on a
/// no_std/Embassy target hits the uncatchable `alloc_error_handler` abort, not a recoverable
/// `Result`. Chosen generously for legitimate large downloads (multi-hundred-MB timelapse videos)
/// while still bounding worst case. Fixed, not yet caller-configurable — unlike camera's
/// `with_max_frame_size`, there is currently no `BambuFtpsClient` builder to lower this for
/// embedded targets with tighter memory budgets.
pub(crate) const FTPS_MAX_TRANSFER_BYTES: usize = 512 * 1024 * 1024;

/// Per-call wall-clock budget for ordinary control-channel reads (`read_response`/ `read_line_raw`, and each individual read step inside `read_to_eof`) — matches `MQTT_READ_TIMEOUT_SECS`/`CAMERA_READ_TIMEOUT_SECS` for consistency.
/// Ordinary FTP command replies (USER/PASS/PBSZ/PROT/TYPE/SIZE/DELE/MKD/RMD/RNFR/RNTO/AVBL/PASV,
/// and the initial `150`/`125` "opening data connection" replies) are short single/few-line
/// responses that complete quickly under healthy conditions, so a flat per-call budget is
/// appropriate here — unlike the post-transfer confirmation wait below, which needs a much longer
/// allowance for entirely different reasons.
pub(crate) const FTPS_READ_TIMEOUT_SECS: u64 = 30;

/// Per-chunk wall-clock budget for `upload_file`'s data-write loop (BUG-164) — the write-side
/// counterpart to `FTPS_READ_TIMEOUT_SECS`. The write loop and final `flush()` had no deadline
/// at all, unlike the STOR negotiation and post-transfer confirmation reads immediately around
/// them; a stalled write would hang forever.
pub(crate) const FTPS_WRITE_TIMEOUT_SECS: u64 = 30;

/// Wall-clock budget specifically for the post-transfer confirmation `read_response` call in `list_directory`/`upload_file`/`download_file` (the one waiting for `226`/`426` after the data channel closes).
/// `upload_file`'s own doc comment already documents waiting "up to 300 seconds for the `226`
/// transfer confirmation to print" due to microSD flush latency — that wait is a genuine, entirely
/// silent gap (zero bytes at all, not slow-trickling data), so it needs a long flat deadline rather
/// than benefiting from `read_to_eof`'s per-chunk reset.
pub(crate) const FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS: u64 = 300;

/// Computes an absolute deadline (epoch-ms) `budget_secs` in the future, or `None` if `timer` has no real wall-clock (see `TimerProvider::has_real_clock`) — the same `has_real_clock()`-gated pattern used throughout this crate (`read_exact_packet`, `read_next_frame_with_timer`) so a `DummyTimer`-backed client sees zero behavior change (unbounded reads, exactly as before per-read deadlines existed).
pub(crate) fn ftps_deadline_ms<T: TimerProvider>(timer: &T, budget_secs: u64) -> Option<u64> {
    if timer.has_real_clock() {
        Some(timer.now_millis().saturating_add(budget_secs * 1000))
    } else {
        None
    }
}

/// Like `crate::io::read_chunk`, but treats a `0`-byte read as legitimate EOF (`Ok(0)`) rather than mapping it to `SocketError::ConnectionReset`.
/// Required for `read_to_eof`: its data transfers signal "transfer complete" via the data-channel
/// socket closing normally (the standard passive-mode end-of-transfer signal) — unlike
/// `read_chunk`'s other callers (MQTT frames, camera frames, and this same module's
/// `read_line_raw`/control-channel replies), none of which expect a legitimate stream closure
/// mid-read, `read_to_eof` must not treat a clean EOF as an error or every successful download
/// would fail.
async fn read_transfer_chunk<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    buf: &mut [u8],
    timer: &T,
    deadline_ms: Option<u64>,
) -> Result<usize, SocketError> {
    let Some(deadline_ms) = deadline_ms else {
        return stream
            .read(buf)
            .await
            .map_err(|_| SocketError::ConnectionReset);
    };

    let remaining_ms = deadline_ms.saturating_sub(timer.now_millis());
    if remaining_ms == 0 {
        return Err(SocketError::TimedOut);
    }

    let read_fut = stream.read(buf);
    let sleep_fut = timer.sleep(core::time::Duration::from_millis(remaining_ms));

    match race(read_fut, sleep_fut).await {
        Raced::Left(Ok(n)) => Ok(n),
        Raced::Left(Err(_)) => Err(SocketError::ConnectionReset),
        Raced::Right(_) => Err(SocketError::TimedOut),
    }
}

/// Sends a formatted ASCII FTP command string cleanly terminated with CRLF boundaries.
pub(crate) async fn write_command<IO: AsyncIo>(
    stream: &mut IO,
    cmd: &str,
) -> Result<(), Error> {
    // Single write_all call for "cmd\r\n" together — some embedded FTP servers (confirmed live
    // against a P1S) don't correctly reassemble a command line split across two separate writes.
    let mut payload = String::from(cmd);
    payload.push_str("\r\n");

    stream
        .write_all(payload.as_bytes())
        .await
        .map_err(|_| Error::NetworkError(SocketError::ConnectionAborted))?;
    stream
        .flush()
        .await
        .map_err(|_| Error::NetworkError(SocketError::ConnectionAborted))?;

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
async fn read_line_raw<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    line_buf: &mut Vec<u8>,
    fill_buf: &mut Vec<u8>,
    timer: &T,
    deadline_ms: Option<u64>,
) -> Result<(), Error> {
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
            return Err(Error::ProtocolViolation(
                "FTP response line exceeds maximum length".into(),
            ));
        }

        let mut chunk = [0u8; FTP_LINE_READ_CHUNK_SIZE];
        let n = read_chunk(stream, &mut chunk, timer, deadline_ms)
            .await
            .map_err(Error::NetworkError)?;
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
pub(crate) async fn read_response<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    line_buf: &mut Vec<u8>,
    fill_buf: &mut Vec<u8>,
    timer: &T,
    deadline_ms: Option<u64>,
) -> Result<(u16, String), Error> {
    let mut accumulated = String::new();
    let mut lines_read: usize = 0;
    // BUG-028: RFC 959 §4.2 requires tracking the reply's opening code and only treating a
    // later line as the terminator if *both* its separator is ' ' and its code matches this —
    // an intermediate multi-line body line can itself start with a 3-digit-number-plus-space
    // sequence that must not be mistaken for the terminator. `None` until the header line
    // (first line of the reply) has been read.
    let mut header_code: Option<u16> = None;

    loop {
        read_line_raw(stream, line_buf, fill_buf, timer, deadline_ms).await?;
        lines_read += 1;
        if lines_read > FTP_MAX_RESPONSE_LINES {
            return Err(Error::ProtocolViolation(
                "FTP response exceeded maximum line count".into(),
            ));
        }

        let Some(code) = header_code else {
            // No header established yet — this line must establish one (single-line reply,
            // or the opening line of a multi-line reply). A line that can't be parsed as
            // such is skipped rather than treated as body text, matching the prior behavior.
            if line_buf.len() < FTP_REPLY_TEXT_OFFSET {
                log::debug!(
                    "skipping malformed FTP response line ({} bytes)",
                    line_buf.len()
                );
                continue;
            }
            let code_str = match core::str::from_utf8(&line_buf[0..FTP_REPLY_CODE_LEN]) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let code = match code_str.parse::<u16>() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let separator = line_buf[FTP_REPLY_SEPARATOR_OFFSET];

            if separator == b' ' {
                let text = core::str::from_utf8(&line_buf[FTP_REPLY_TEXT_OFFSET..]).unwrap_or("").trim();
                return Ok((code, text.to_string()));
            } else if separator == b'-' {
                header_code = Some(code);
                let line_text = core::str::from_utf8(&line_buf[FTP_REPLY_TEXT_OFFSET..]).unwrap_or("").trim();
                accumulated.push_str(line_text);
            } else {
                // BUG-062: a header line whose 4th byte is neither ' ' nor '-' (e.g. "200\r\n",
                // code immediately followed by CRLF with no separator) previously fell through
                // to `continue` and was silently discarded, burning a line out of
                // FTP_MAX_RESPONSE_LINES and eventually surfacing as a generic "exceeded maximum
                // line count" error that obscures the real reply. Treat it as a terminal
                // single-line reply with empty text instead.
                return Ok((code, String::new()));
            }
            continue;
        };

        // Inside a multi-line reply body: only a line starting with the *same* code followed
        // by a space terminates it. A line starting with the same code followed by a hyphen is
        // a continuation line in this parser's supported padded format — its prefix is
        // stripped like the header line's. Anything else (wrong code, or plain free text with
        // no code prefix at all) is body content appended verbatim, per RFC 959 §4.2's warning
        // that intermediate lines aren't required to carry any code prefix.
        let prefix_code = if line_buf.len() >= FTP_REPLY_TEXT_OFFSET {
            core::str::from_utf8(&line_buf[0..FTP_REPLY_CODE_LEN])
                .ok()
                .and_then(|s| s.parse::<u16>().ok())
                .filter(|c| *c == code)
        } else {
            None
        };

        match (prefix_code, line_buf.get(FTP_REPLY_SEPARATOR_OFFSET)) {
            (Some(_), Some(b' ')) => {
                let text = core::str::from_utf8(&line_buf[FTP_REPLY_TEXT_OFFSET..]).unwrap_or("").trim();
                if !text.is_empty() {
                    accumulated.push('\n');
                    accumulated.push_str(text);
                }
                return Ok((code, accumulated));
            }
            (Some(_), Some(b'-')) => {
                let text = core::str::from_utf8(&line_buf[FTP_REPLY_TEXT_OFFSET..]).unwrap_or("").trim();
                accumulated.push('\n');
                accumulated.push_str(text);
            }
            _ => {
                let line_text = core::str::from_utf8(line_buf).unwrap_or("").trim_end();
                accumulated.push('\n');
                accumulated.push_str(line_text);
            }
        }
    }
}

/// Extracts the passive port number from a PASV response text.
///
/// Parses the `(IP_1,IP_2,IP_3,IP_4,PORT_1,PORT_2)` tuple and computes
/// the port as `PORT_1 * 256 + PORT_2`.
pub(crate) fn parse_pasv_port(text: &str) -> Result<u16, Error> {
    let start = text
        .find('(')
        .ok_or(Error::ProtocolViolation("Invalid PASV format".into()))?;
    let end = text[start + 1..]
        .find(')')
        .map(|e| e + start + 1)
        .ok_or(Error::ProtocolViolation("Invalid PASV format".into()))?;
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
            .ok_or(Error::ProtocolViolation(
                "Failed to parse PORT_1 in PASV".into(),
            ))?;
    let p2 =
        parts
            .next()
            .and_then(|p| p.parse::<u16>().ok())
            .ok_or(Error::ProtocolViolation(
                "Failed to parse PORT_2 in PASV".into(),
            ))?;

    let port = (p1 as u32) * (FTPS_PASV_PORT_MULTIPLIER as u32) + (p2 as u32);
    if port > u16::MAX as u32 {
        return Err(Error::ProtocolViolation(
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
pub(crate) fn validate_ftp_path(path: &str) -> Result<(), Error> {
    // Covers CR/LF/NUL (the original command-injection hazard this function guards
    // against) plus every other C0 control byte and DEL — non-CR/LF control characters can
    // smuggle ANSI escapes into a filename a caller later prints/logs.
    if path.bytes().any(|b| b < FTP_PATH_CONTROL_CHAR_MAX || b == FTP_PATH_DEL_CHAR) {
        return Err(Error::ProtocolViolation(
            "FTP path contains an illegal control character".into(),
        ));
    }
    if path.split(['/', '\\']).any(|segment| segment == "..") {
        return Err(Error::ProtocolViolation(
            "FTP path contains a '..' path traversal segment".into(),
        ));
    }
    // Some FTP daemons interpret a leading-dash filename as a flag argument. Only the final
    // segment matters here — a leading-dash directory component earlier in the path is not
    // the same hazard. BUG-075: `.next_back()` alone returns `""` for a trailing-slash path
    // (e.g. "/cache/-dir/"), silently skipping the check on that dash-prefixed final directory
    // component — filter out empty trailing segments first so the real final segment is checked.
    if path
        .split(['/', '\\'])
        .rev()
        .find(|segment| !segment.is_empty())
        .is_some_and(|segment| segment.starts_with('-'))
    {
        return Err(Error::ProtocolViolation(
            "FTP path's final segment must not start with '-'".into(),
        ));
    }
    Ok(())
}

/// Utility capturing passive stream data up to socket EOF bounds.
///
/// `budget_ms` is a *duration*, not an absolute deadline: a fresh absolute deadline is
/// computed from `timer.now_millis() + budget_ms` before every individual read attempt (not
/// once for the whole transfer) — so a truly-stalled connection (zero bytes for `budget_ms`
/// straight) times out, while a slow-but-live transfer that keeps producing at least some
/// bytes every `budget_ms` never does, regardless of the transfer's total duration. This
/// matters here specifically because transfers can legitimately be large (up to
/// `FTPS_MAX_TRANSFER_BYTES`, hundreds of MB) — unlike `read_response`'s fixed per-call
/// deadline, which is fine for short control-channel replies but would falsely reject a
/// large, slow-but-healthy download if applied to the whole transfer at once.
pub(crate) async fn read_to_eof<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    out: &mut Vec<u8>,
    timer: &T,
    budget_ms: u64,
) -> Result<(), Error> {
    let mut chunk = [0u8; FTPS_DATA_READ_BUF_SIZE];
    loop {
        let deadline_ms = if timer.has_real_clock() {
            Some(timer.now_millis().saturating_add(budget_ms))
        } else {
            None
        };
        match read_transfer_chunk(stream, &mut chunk, timer, deadline_ms).await {
            Ok(0) => break,
            Ok(n) => {
                if out.len() + n > FTPS_MAX_TRANSFER_BYTES {
                    return Err(Error::ProtocolViolation(
                        "FTPS transfer exceeds maximum accepted size".into(),
                    ));
                }
                out.extend_from_slice(&chunk[..n]);
            }
            Err(e) => return Err(Error::NetworkError(e)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
