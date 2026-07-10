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

    // Post-download SIZE verification (BUG-003)
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
pub async fn run_mock_server_upload_426_recovery(
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
    let bytes_read = server_upload_data
        .read(&mut upload_buf)
        .await
        .expect("upload data read");
    drop(server_upload_data);

    // Return 426 (TLS 1.3 close race) instead of 226
    respond(
        &mut server_control,
        b"426 Failure reading network stream.\r\n",
    )
    .await;

    // SIZE verification — report size matches
    let cmd = read_cmd(&mut server_control, &mut buf).await;
    assert!(cmd.starts_with("SIZE "));
    respond(
        &mut server_control,
        format!("213 {}\r\n", bytes_read).as_bytes(),
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

/// Mock server for the BUG-004 single-reply-command poisoning regression test.
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
