use super::*;
use crate::client::dummy::DummyTimer;
use crate::test_support::MockIo;

#[tokio::test]
async fn test_read_line_raw_carries_leftover_bytes_across_calls() {
    // Regression test: a single socket read delivering two full
    // FTP response lines back-to-back must not lose the second line. The first call to
    // `read_line_raw` must return only the first line; the second call must return the
    // second line using the bytes already buffered from the first read, without issuing
    // any further socket read (the mock's queue only has one chunk).
    let mut stream =
        MockIo::with_chunks(&[b"150 Opening data connection\r\n226 Transfer complete\r\n"]);
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
    let mut stream = MockIo::with_chunks(&[b"220 Wel", b"come\r\n"]);
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
    let mut stream =
        MockIo::with_chunks(&[b"213-First line\r\n213-Second line\r\n213 Final line\r\n"]);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    let (code, text) = read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
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
    let mut stream = MockIo::with_chunks(&[
        b"213-Header\r\n150 looks like a terminator but isn't\r\n213 Final line\r\n",
    ]);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    let (code, text) = read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
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
    let mut stream =
        MockIo::with_chunks(&[b"213-Header\r\nplain free text, no code\r\n213 End\r\n"]);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    let (code, text) = read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
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
    let mut stream = MockIo::with_chunks(&[b"200\r\n"]);
    let mut line_buf = Vec::new();
    let mut fill_buf = Vec::new();

    let (code, text) = read_response(&mut stream, &mut line_buf, &mut fill_buf, &DummyTimer, None)
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
    // `FtpsClient` does via its `control_fill_buf` field) so the second call sees the
    // already-buffered `226` line instead of blocking on a socket read that never comes.
    let mut stream =
        MockIo::with_chunks(&[b"150 Opening data connection.\r\n226 Transfer complete.\r\n"]);
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

#[tokio::test]
async fn test_read_to_eof_rejects_oversized_transfer() {
    // A stream that never sends EOF and exceeds the transfer cap must error cleanly instead of
    // growing `out` without bound. Driven through `read_to_eof_bounded` with a small cap: the
    // real `FTPS_MAX_TRANSFER_BYTES` is 512 MiB, so running this against the production
    // constant allocated half a gigabyte (peaking near 1 GiB through `Vec` doubling) on every
    // test run, pre-commit hook, and CI job. `read_to_eof` is a thin delegation to this
    // function with the constant, so the abort path under test is the same one.
    const TEST_MAX_BYTES: usize = FTPS_DATA_READ_BUF_SIZE * 4;
    let mut stream = MockIo::infinite();
    let mut out = Vec::new();

    let result =
        read_to_eof_bounded(&mut stream, &mut out, &DummyTimer, 30_000, TEST_MAX_BYTES).await;
    assert!(matches!(result, Err(Error::ProtocolViolation(_))));
    assert!(out.len() <= TEST_MAX_BYTES);
}

#[tokio::test]
async fn test_write_command_sends_single_write_call() {
    // Regression test: write_command must send "cmd\r\n" as one write_all call, not two
    // separate ones. Some embedded FTP servers (confirmed live against a Bambu P1S) don't
    // reliably reassemble a command line split across two writes/TLS records.
    let mut stream = MockIo::empty();

    write_command(&mut stream, "USER bblp", &DummyTimer, None)
        .await
        .unwrap();

    let calls = &stream.writes;
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
    let port = parse_pasv_port("Entering Passive Mode (127,0,0,1,192,168).").expect("valid PASV");
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
