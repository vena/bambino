//! # Mock FTPS Server
//!
//! Provides deterministic, state-machine driven FTP server fixtures designed to test
//! the `BambuFtpsClient` over in-memory `tokio::io::duplex` streams.
//!
//! Supports multiple test scenarios via separate server functions, each exercising
//! different FTPS protocol paths (happy path, A1 plaintext, STAT fallback,
//! download, directory ops, upload error recovery).

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

use bambino::io::TokioIo;

/// Helper: reads the next command from the control stream and returns it as a string.
///
/// This single, non-looping `read()` is *not* a safety net against
/// `write_command`'s single-write-call guarantee regressing back to two writes — under tokio's
/// cooperative scheduling, two sequential small writes on a `tokio::io::duplex` normally
/// coalesce into one `.read()` before this task is ever polled, so every test built on this
/// harness would very likely keep passing even if `write_command` regressed. The dedicated
/// `WriteRecorder`-based unit test in `src/ftps/protocol.rs` is the only thing actually guarding
/// that invariant end-to-end; don't rely on this helper for it.
async fn read_cmd(stream: &mut tokio::io::DuplexStream, buf: &mut [u8]) -> String {
    let n = stream.read(buf).await.expect("Failed to read FTP command");
    core::str::from_utf8(&buf[..n])
        .expect("FTP command is not valid UTF-8")
        .to_string()
}

/// Helper: writes a response line to the control stream.
async fn respond(stream: &mut tokio::io::DuplexStream, response: &[u8]) {
    stream
        .write_all(response)
        .await
        .expect("Failed to write FTP response");
}

/// Helper: runs the standard handshake (greeting, login, PBSZ, PROT P, TYPE I).
async fn run_standard_handshake(
    server_control: &mut tokio::io::DuplexStream,
    buf: &mut [u8],
    expect_prot_p: bool,
) {
    // Greeting
    respond(server_control, b"220 vsFTPd 3.0.3\r\n").await;

    // USER
    let cmd = read_cmd(server_control, buf).await;
    assert!(cmd.starts_with("USER bblp"), "Expected USER bblp");
    respond(server_control, b"331 Please specify the password.\r\n").await;

    // PASS
    let cmd = read_cmd(server_control, buf).await;
    assert!(cmd.starts_with("PASS 12345678"), "Expected PASS");
    respond(server_control, b"230 Login successful.\r\n").await;

    // PBSZ
    let cmd = read_cmd(server_control, buf).await;
    assert_eq!(cmd, "PBSZ 0\r\n");
    respond(server_control, b"200 PBSZ set to 0.\r\n").await;

    // PROT P or TYPE I (depending on model)
    if expect_prot_p {
        let cmd = read_cmd(server_control, buf).await;
        assert_eq!(cmd, "PROT P\r\n");
        respond(server_control, b"200 PROT level set to P.\r\n").await;
    }

    // TYPE I
    let cmd = read_cmd(server_control, buf).await;
    assert_eq!(cmd, "TYPE I\r\n");
    respond(server_control, b"200 Switching to Binary mode.\r\n").await;
}

/// Helper: handles a PASV negotiation, creating a mock data stream.
async fn handle_pasv(
    server_control: &mut tokio::io::DuplexStream,
    buf: &mut [u8],
    data_container: &Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) -> tokio::io::DuplexStream {
    let cmd = read_cmd(server_control, buf).await;
    assert_eq!(cmd, "PASV\r\n");

    let (client_data, server_data) = tokio::io::duplex(4096);
    {
        let mut guard = data_container.lock().await;
        *guard = Some(TokioIo(client_data));
    }

    // Port = 192 * 256 + 168 = 49320
    respond(
        server_control,
        b"227 Entering Passive Mode (127,0,0,1,192,168).\r\n",
    )
    .await;

    server_data
}

/// Primary happy-path mock server: handshake, list, AVBL, SIZE, upload, delete.
pub async fn run_mock_server(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    // LIST
    let mut server_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "LIST /model\r\n");
    respond(
        &mut server_control,
        b"150 Here comes directory listing.\r\n",
    )
    .await;
    server_data
        .write_all(b"-rw-r--r--    1 1000     1000      102400 Jun 17 12:14 job.3mf\r\n")
        .await
        .expect("LIST data write");
    server_data.flush().await.expect("LIST data flush");
    drop(server_data);
    respond(&mut server_control, b"226 Directory send OK.\r\n").await;

    // AVBL
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "AVBL\r\n");
    respond(&mut server_control, b"213 107374182400\r\n").await;

    // SIZE
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "SIZE /model/job.3mf\r\n");
    respond(&mut server_control, b"213 102400\r\n").await;

    // STOR upload
    let mut server_upload_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "STOR /model/job.3mf\r\n");
    respond(&mut server_control, b"150 Ok to send data.\r\n").await;

    let mut upload_buf = vec![0u8; 100];
    let bytes_read = server_upload_data
        .read(&mut upload_buf)
        .await
        .expect("upload data read");
    assert_eq!(&upload_buf[..bytes_read], b"MOCK_UPLOAD_DATA");
    drop(server_upload_data);

    respond(&mut server_control, b"226 File receive OK.\r\n").await;

    // Post-upload SIZE verification
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "SIZE /model/job.3mf\r\n");
    respond(&mut server_control, b"213 16\r\n").await;

    // DELE
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "DELE /model/job.3mf\r\n");
    respond(&mut server_control, b"250 File deleted successfully.\r\n").await;
}

/// Mock server for upload exercising `upload_file`'s multi-chunk write loop with a payload
/// larger than one `FTPS_UPLOAD_CHUNK_SIZE` (64 KiB).
///
/// `run_mock_server`'s upload capture does a single non-looping `read()` into a
/// fixed 100-byte buffer, and every test payload built on this harness (e.g.
/// `b"MOCK_UPLOAD_DATA"`, 16 bytes) is far under one chunk — so no test ever exercised
/// `upload_file`'s multi-chunk loop past its first iteration. This loops the read until
/// `expected_len` bytes are captured and returns them so the caller can assert content
/// integrity across chunk boundaries — it can't prove how many separate `write_all()` calls
/// produced the bytes (`.claude/rules/wire-framing-hardware-verification.md`: a mock reads a
/// stream regardless of write count), only that the client's offset-tracking loop reassembles
/// a multi-chunk payload correctly end-to-end.
pub async fn run_mock_server_upload_multi_chunk(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
    expected_len: usize,
) -> Vec<u8> {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    let mut server_upload_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "STOR /model/big.bin\r\n");
    respond(&mut server_control, b"150 Ok to send data.\r\n").await;

    let mut received = Vec::with_capacity(expected_len);
    let mut chunk = vec![0u8; 8192];
    while received.len() < expected_len {
        let n = server_upload_data
            .read(&mut chunk)
            .await
            .expect("upload data read");
        assert!(n > 0, "data channel closed before all bytes were received");
        received.extend_from_slice(&chunk[..n]);
    }
    drop(server_upload_data);

    respond(&mut server_control, b"226 File receive OK.\r\n").await;

    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "SIZE /model/big.bin\r\n");
    respond(
        &mut server_control,
        format!("213 {}\r\n", expected_len).as_bytes(),
    )
    .await;

    received
}

/// Mock server for A1 plaintext data channel tests: skips PROT P.
pub async fn run_mock_server_a1_plaintext(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    // A1 handshake: no PROT P
    run_standard_handshake(&mut server_control, &mut buf, false).await;

    // LIST over plaintext data channel
    let mut server_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "LIST /\r\n");
    respond(
        &mut server_control,
        b"150 Here comes directory listing.\r\n",
    )
    .await;
    server_data
        .write_all(b"drwxr-xr-x    2 1000     1000         4096 Jun 17  2025 cache\r\n")
        .await
        .expect("LIST data write");
    server_data.flush().await.expect("LIST data flush");
    drop(server_data);
    respond(&mut server_control, b"226 Directory send OK.\r\n").await;
}

/// Mock server for download (RETR) test.
pub async fn run_mock_server_download(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    // RETR download
    let mut server_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "RETR /model/job.3mf\r\n");
    respond(&mut server_control, b"150 Opening data connection.\r\n").await;

    server_data
        .write_all(b"MOCK_FILE_CONTENT_FOR_DOWNLOAD")
        .await
        .expect("RETR data write");
    server_data.flush().await.expect("RETR data flush");
    drop(server_data);
    respond(&mut server_control, b"226 Transfer complete.\r\n").await;

    // Post-download SIZE verification
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "SIZE /model/job.3mf\r\n");
    respond(&mut server_control, b"213 30\r\n").await;
}

/// Mock server for download (RETR) with a SIZE mismatch (should trigger ProtocolViolation).
pub async fn run_mock_server_download_size_mismatch(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    // RETR download
    let mut server_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "RETR /model/job.3mf\r\n");
    respond(&mut server_control, b"150 Opening data connection.\r\n").await;

    // Data channel closes early after only partial content — the client still sees a clean
    // 226 confirmation, but the payload it actually read is shorter than the real file.
    server_data
        .write_all(b"MOCK_FILE_CONTENT_FOR_DOWNLOAD")
        .await
        .expect("RETR data write");
    server_data.flush().await.expect("RETR data flush");
    drop(server_data);
    respond(&mut server_control, b"226 Transfer complete.\r\n").await;

    // SIZE verification — report a larger size than what was actually transferred.
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "SIZE /model/job.3mf\r\n");
    respond(&mut server_control, b"213 99999\r\n").await;
}

/// Mock server for directory operations: MKD, RMD, RNFR/RNTO.
pub async fn run_mock_server_dir_ops(
    mut server_control: tokio::io::DuplexStream,
    _data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    // MKD
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "MKD /model/subdir\r\n");
    respond(&mut server_control, b"257 \"/model/subdir\" created.\r\n").await;

    // RMD
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "RMD /model/subdir\r\n");
    respond(
        &mut server_control,
        b"250 Directory removed successfully.\r\n",
    )
    .await;

    // RNFR + RNTO
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "RNFR /model/old.3mf\r\n");
    respond(&mut server_control, b"350 Ready for destination name.\r\n").await;

    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "RNTO /model/new.3mf\r\n");
    respond(&mut server_control, b"250 Rename successful.\r\n").await;

    // RMD on non-existent directory (550 = idempotent success)
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "RMD /model/gone\r\n");
    respond(&mut server_control, b"550 No such file or directory.\r\n").await;
}

/// Mock server for `get_available_space()` when AVBL is unsupported.
///
/// review/ftps.md Phase 7c: the STAT fallback was removed — real Bambu firmware (P1S capture)
/// responds to `STAT` with `502 Command not implemented`, so the fallback was dead code. The
/// client must now surface `Err(ProtocolViolation)` directly off the failed `AVBL` reply,
/// without ever sending `STAT`.
pub async fn run_mock_server_avbl_unsupported(
    mut server_control: tokio::io::DuplexStream,
    _data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    // AVBL — unsupported, no STAT fallback follows.
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "AVBL\r\n");
    respond(
        &mut server_control,
        b"500 Syntax error, command unrecognized.\r\n",
    )
    .await;
}

/// Mock server for upload with 426 (TLS 1.3 close race) + SIZE recovery.
///
/// `expected_len` is the caller's independently-known payload length, and the SIZE reply is
/// derived from it — not from the byte count this mock happened to read. Echoing the observed
/// count made the client's post-426 SIZE recheck tautological: a client bug that truncated the
/// upload would have been confirmed "correct" by a SIZE reply that shrank to match it. That
/// recheck is what `src/ftps/CLAUDE.md` cites to justify the fail-open
/// `allow_unverified_tls_1_2` opt-out, so the test proving it has to be able to fail.
pub async fn run_mock_server_upload_426_recovery(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
    expected_len: usize,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    // STOR upload
    let mut server_upload_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "STOR /model/job.3mf\r\n");
    respond(&mut server_control, b"150 Ok to send data.\r\n").await;

    // Loop until the full expected payload arrives, like run_mock_server_upload_multi_chunk:
    // a single read can return a short chunk, which the old single-read version silently
    // accepted as the whole upload.
    let mut received = 0usize;
    let mut chunk = vec![0u8; 8192];
    while received < expected_len {
        let n = server_upload_data
            .read(&mut chunk)
            .await
            .expect("upload data read");
        assert!(n > 0, "data channel closed before all bytes were received");
        received += n;
    }
    drop(server_upload_data);

    // Return 426 (TLS 1.3 close race) instead of 226
    respond(
        &mut server_control,
        b"426 Failure reading network stream.\r\n",
    )
    .await;

    // SIZE verification — report the independently-known length, not the observed count.
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert!(cmd.starts_with("SIZE "));
    respond(
        &mut server_control,
        format!("213 {}\r\n", expected_len).as_bytes(),
    )
    .await;
}

/// Mock server for upload with 426 + SIZE mismatch (should trigger DiskWriteFailure).
pub async fn run_mock_server_upload_size_mismatch(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    // STOR upload
    let mut server_upload_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "STOR /model/job.3mf\r\n");
    respond(&mut server_control, b"150 Ok to send data.\r\n").await;

    let mut upload_buf = vec![0u8; 100];
    let _bytes_read = server_upload_data
        .read(&mut upload_buf)
        .await
        .expect("upload data read");
    drop(server_upload_data);

    // Return 426 (TLS close race)
    respond(
        &mut server_control,
        b"426 Failure reading network stream.\r\n",
    )
    .await;

    // SIZE verification — report WRONG size (truncated write)
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert!(cmd.starts_with("SIZE "));
    respond(&mut server_control, b"213 0\r\n").await;
}

/// Mock server for the Phase 2 desync regression test.
///
/// Sends the `150` reply for a `LIST` command and then stops — it deliberately never sends the
/// matching `226`. The test pairs this with a `TlsConnector` that fails the data-channel
/// connect, so the client is expected to poison itself and return before ever trying to read a
/// final reply that will never arrive; if it instead ignored the failure and tried to read the
/// control channel again, that read would hang forever against this mock.
pub async fn run_mock_server_data_channel_failure(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    let _server_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "LIST /model\r\n");
    respond(
        &mut server_control,
        b"150 Here comes directory listing.\r\n",
    )
    .await;
    // Intentionally no matching 226 — the client's TLS connector fails the data-channel connect
    // before it would ever consume this reply.
}

/// Mock server for the single-reply-command poisoning regression test.
///
/// Reads the `DELE` command and then drops the control stream without ever replying — the
/// client's `read_response` sees a clean 0-byte read, which `read_chunk` maps to
/// `SocketError::ConnectionReset` immediately (no 30s timeout wait needed for this test).
pub async fn run_mock_server_dele_connection_drop(
    mut server_control: tokio::io::DuplexStream,
    _data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "DELE /model/job.3mf\r\n");
    // Drop the stream instead of responding.
}

/// Mock server for the regression test: a transport failure between `rename_file`'s two-step
/// `RNFR`/`RNTO` sequence must poison the client the same way a single-reply command's failure
/// already does. Acks `RNFR` normally, then drops the connection instead of responding to
/// `RNTO`.
pub async fn run_mock_server_rnto_connection_drop(
    mut server_control: tokio::io::DuplexStream,
    _data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "RNFR /model/old.3mf\r\n");
    respond(&mut server_control, b"350 Ready for destination name.\r\n").await;

    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "RNTO /model/new.3mf\r\n");
    // Drop the stream instead of responding.
}

/// Mock server for the regression test: `LIST`'s transfer-confirmation read must accept `426`
/// (the documented P2S/X2D TLS 1.3 close race [REF-FTPS-CONN]) the same way upload/download
/// already do — upload/download both have a dedicated 426-recovery test; LIST did not.
pub async fn run_mock_server_list_426_recovery(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    let mut server_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "LIST /model\r\n");
    respond(
        &mut server_control,
        b"150 Here comes directory listing.\r\n",
    )
    .await;
    server_data
        .write_all(b"-rw-r--r--    1 1000     1000      102400 Jun 17 12:14 job.3mf\r\n")
        .await
        .expect("LIST data write");
    server_data.flush().await.expect("LIST data flush");
    drop(server_data);
    // 426 instead of 226 — the TLS 1.3 close race, tolerated the same as upload/download.
    respond(&mut server_control, b"426 Connection closed; transfer aborted.\r\n").await;
}

/// Mock server for the regression test: `LIST`'s *initial* write/read (the `150`/`125`
/// negotiation, before the data-transfer window the single-reply-command case already covered) must
/// poison the client on failure too. Drops the control stream right after reading the `LIST`
/// command, before ever sending a `150`/`125` reply.
pub async fn run_mock_server_list_connection_drop(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    let _server_data = handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "LIST /model\r\n");
    // Drop the stream instead of responding.
}

/// Mock server for the regression test: `download_file`'s confirmation-read handling
/// must accept `426` (the documented P2S/X2D TLS 1.3 close race [REF-FTPS-CONN]) and fall
/// through to the SIZE recheck, symmetric with `upload_file`'s existing 426 handling —
/// previously RETR treated 426 as an unconditional hard failure.
pub async fn run_mock_server_download_426_recovery(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    let mut server_download_data =
        handle_pasv(&mut server_control, &mut buf, &data_container).await;
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "RETR /model/job.3mf\r\n");
    respond(&mut server_control, b"150 Opening data connection.\r\n").await;

    let payload = b"TEST_DATA";
    server_download_data
        .write_all(payload)
        .await
        .expect("download data write");
    drop(server_download_data);

    // Return 426 (TLS 1.3 close race) instead of 226 — the payload was already fully sent.
    respond(
        &mut server_control,
        b"426 Failure reading network stream.\r\n",
    )
    .await;

    // SIZE verification — report size matches, so download_file should still succeed.
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert!(cmd.starts_with("SIZE "));
    respond(
        &mut server_control,
        format!("213 {}\r\n", payload.len()).as_bytes(),
    )
    .await;
}

/// Mock server for disconnect (QUIT) test.
pub async fn run_mock_server_disconnect(
    mut server_control: tokio::io::DuplexStream,
    _data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    run_standard_handshake(&mut server_control, &mut buf, true).await;

    // QUIT
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert_eq!(cmd, "QUIT\r\n");
    respond(&mut server_control, b"221 Goodbye.\r\n").await;
}
