//! # Mock FTPS Server
//!
//! Provides deterministic, state-machine driven FTP server fixtures designed to test
//! the `FtpsClient` over in-memory `tokio::io::duplex` streams.
//!
//! Supports multiple test scenarios via separate server functions, each exercising
//! different FTPS protocol paths (happy path, A1 plaintext, STAT fallback,
//! download, directory ops, upload error recovery).

use super::client::ACCESS_CODE;
use super::io::DataContainer;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

use bambino::io::TokioIo;

/// The mock server's end of the control channel, and the slot its passive data channels go into.
struct Control {
    stream: DuplexStream,
    buf: Vec<u8>,
    /// `None` for a mock built with [`Control::control_only`]: [`Control::pasv`] then panics
    /// rather than hand the client's data stream to a slot its factory never reads.
    data: Option<DataContainer>,
}

impl Control {
    /// A control channel whose passive data channels are handed to the client through `data`.
    fn new(stream: DuplexStream, data: DataContainer) -> Self {
        Self {
            stream,
            buf: vec![0u8; 1024],
            data: Some(data),
        }
    }

    /// A control channel for a mock that never opens a data channel.
    fn control_only(stream: DuplexStream) -> Self {
        Self {
            stream,
            buf: vec![0u8; 1024],
            data: None,
        }
    }

    /// Reads the next command from the control stream and returns it as a string.
    ///
    /// This single, non-looping `read()` is *not* a safety net against
    /// `write_command`'s single-write-call guarantee regressing back to two writes — under
    /// tokio's cooperative scheduling, two sequential small writes on a `tokio::io::duplex`
    /// normally coalesce into one `.read()` before this task is ever polled, so every test built
    /// on this harness would very likely keep passing even if `write_command` regressed. The
    /// dedicated `WriteRecorder`-based unit test in `src/ftps/protocol.rs` is the only thing
    /// actually guarding that invariant end-to-end; don't rely on this helper for it.
    async fn read_cmd(&mut self) -> String {
        let n = self
            .stream
            .read(&mut self.buf)
            .await
            .expect("Failed to read FTP command");
        core::str::from_utf8(&self.buf[..n])
            .expect("FTP command is not valid UTF-8")
            .to_string()
    }

    /// Writes a response line to the control stream.
    async fn respond(&mut self, response: &[u8]) {
        self.stream
            .write_all(response)
            .await
            .expect("Failed to write FTP response");
    }

    /// Reads the next command, asserts it is `cmd`, and answers with `reply`.
    async fn expect(&mut self, cmd: &str, reply: &[u8]) {
        let got = self.read_cmd().await;
        assert_eq!(got, cmd);
        self.respond(reply).await;
    }

    /// Runs the standard handshake (greeting, login, PBSZ, PROT P, TYPE I).
    async fn handshake(&mut self, expect_prot_p: bool) {
        self.handshake_with_greeting(expect_prot_p, b"220 vsFTPd 3.0.3\r\n")
            .await;
    }

    /// [`Control::handshake`] with a caller-supplied greeting.
    ///
    /// Split out so a test can drive a multi-line (`220-`…`220 `) greeting through
    /// `read_response`'s RFC 959 §4.2 continuation handling — every other mock here writes
    /// single-line replies only, so no integration test exercised that path end to end.
    async fn handshake_with_greeting(&mut self, expect_prot_p: bool, greeting: &[u8]) {
        self.respond(greeting).await;

        let cmd = self.read_cmd().await;
        assert!(cmd.starts_with("USER bblp"), "Expected USER bblp");
        self.respond(b"331 Please specify the password.\r\n").await;

        self.expect(
            &format!("PASS {ACCESS_CODE}\r\n"),
            b"230 Login successful.\r\n",
        )
        .await;
        self.expect("PBSZ 0\r\n", b"200 PBSZ set to 0.\r\n").await;
        // PROT P or TYPE I (depending on model)
        if expect_prot_p {
            self.expect("PROT P\r\n", b"200 PROT level set to P.\r\n")
                .await;
        }
        self.expect("TYPE I\r\n", b"200 Switching to Binary mode.\r\n")
            .await;
    }

    /// Handles a PASV negotiation, handing the client end of a fresh data stream to the factory.
    async fn pasv(&mut self) -> DuplexStream {
        let cmd = self.read_cmd().await;
        assert_eq!(cmd, "PASV\r\n");

        let (client_data, server_data) = tokio::io::duplex(4096);
        let data = self.data.as_ref().expect(
            "PASV on a control-only mock: build it with Control::new and the factory's container",
        );
        *data.lock().await = Some(TokioIo::new(client_data));

        // Port = 192 * 256 + 168 = 49320
        self.respond(b"227 Entering Passive Mode (127,0,0,1,192,168).\r\n")
            .await;
        server_data
    }

    /// One complete passive transfer from the server: `PASV`, then `cmd` answered with
    /// `opening`, then `data` written and the data channel closed, then `closing`.
    async fn serve_data(&mut self, cmd: &str, opening: &[u8], data: &[u8], closing: &[u8]) {
        let mut server_data = self.pasv().await;
        self.expect(cmd, opening).await;
        server_data.write_all(data).await.expect("data write");
        server_data.flush().await.expect("data flush");
        drop(server_data);
        self.respond(closing).await;
    }

    /// Answers `LIST path` with `lines` as the listing, over a fresh passive data channel.
    async fn serve_listing(&mut self, path: &str, lines: &[u8]) {
        self.serve_data(
            &format!("LIST {path}\r\n"),
            b"150 Here comes directory listing.\r\n",
            lines,
            b"226 Directory send OK.\r\n",
        )
        .await;
    }
}

/// Primary happy-path mock server: handshake, list, AVBL, SIZE, upload, delete.
pub async fn run_mock_server(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    // LIST
    ctl.serve_data(
        "LIST /model\r\n",
        b"150 Here comes directory listing.\r\n",
        b"-rw-r--r--    1 1000     1000      102400 Jun 17 12:14 job.3mf\r\n",
        b"226 Directory send OK.\r\n",
    )
    .await;

    // AVBL
    ctl.expect("AVBL\r\n", b"213 107374182400\r\n").await;

    // SIZE
    ctl.expect("SIZE /model/job.3mf\r\n", b"213 102400\r\n")
        .await;

    // STOR upload
    let mut server_upload_data = ctl.pasv().await;
    ctl.expect("STOR /model/job.3mf\r\n", b"150 Ok to send data.\r\n")
        .await;

    let mut upload_buf = vec![0u8; 100];
    let bytes_read = server_upload_data
        .read(&mut upload_buf)
        .await
        .expect("upload data read");
    assert_eq!(&upload_buf[..bytes_read], b"MOCK_UPLOAD_DATA");
    drop(server_upload_data);

    ctl.respond(b"226 File receive OK.\r\n").await;

    // Post-upload SIZE verification
    ctl.expect("SIZE /model/job.3mf\r\n", b"213 16\r\n").await;

    // DELE
    ctl.expect(
        "DELE /model/job.3mf\r\n",
        b"250 File deleted successfully.\r\n",
    )
    .await;
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
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
    expected_len: usize,
) -> Vec<u8> {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let mut server_upload_data = ctl.pasv().await;
    ctl.expect("STOR /model/big.bin\r\n", b"150 Ok to send data.\r\n")
        .await;

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

    ctl.respond(b"226 File receive OK.\r\n").await;

    ctl.expect(
        "SIZE /model/big.bin\r\n",
        format!("213 {}\r\n", expected_len).as_bytes(),
    )
    .await;

    received
}

/// Mock server for A1 plaintext data channel tests: skips PROT P.
pub async fn run_mock_server_a1_plaintext(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    // A1 handshake: no PROT P
    ctl.handshake(false).await;

    // LIST over plaintext data channel
    ctl.serve_data(
        "LIST /\r\n",
        b"150 Here comes directory listing.\r\n",
        b"drwxr-xr-x    2 1000     1000         4096 Jun 17  2025 cache\r\n",
        b"226 Directory send OK.\r\n",
    )
    .await;
}

/// Mock server for download (RETR) test.
pub async fn run_mock_server_download(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    // RETR download
    ctl.serve_data(
        "RETR /model/job.3mf\r\n",
        b"150 Opening data connection.\r\n",
        b"MOCK_FILE_CONTENT_FOR_DOWNLOAD",
        b"226 Transfer complete.\r\n",
    )
    .await;

    // Post-download SIZE verification
    ctl.expect("SIZE /model/job.3mf\r\n", b"213 30\r\n").await;
}

/// Mock server for download (RETR) with a SIZE mismatch (should trigger ProtocolViolation).
pub async fn run_mock_server_download_size_mismatch(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    // RETR download
    // Data channel closes early after only partial content — the client still sees a clean
    // 226 confirmation, but the payload it actually read is shorter than the real file.
    ctl.serve_data(
        "RETR /model/job.3mf\r\n",
        b"150 Opening data connection.\r\n",
        b"MOCK_FILE_CONTENT_FOR_DOWNLOAD",
        b"226 Transfer complete.\r\n",
    )
    .await;

    // SIZE verification — report a larger size than what was actually transferred.
    ctl.expect("SIZE /model/job.3mf\r\n", b"213 99999\r\n")
        .await;
}

/// Mock server for directory operations: MKD, RMD, RNFR/RNTO, and the `550` re-check listing.
pub async fn run_mock_server_dir_ops(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    // MKD, answered the RFC 959 way.
    ctl.expect(
        "MKD /model/subdir\r\n",
        b"257 \"/model/subdir\" created.\r\n",
    )
    .await;

    // MKD, answered the way a P1S's `BBL-P003` server does: a bare `250` (#613).
    ctl.expect("MKD /model/other\r\n", b"250 \r\n").await;

    // RMD
    ctl.expect(
        "RMD /model/subdir\r\n",
        b"250 Directory removed successfully.\r\n",
    )
    .await;

    // RNFR + RNTO
    ctl.expect(
        "RNFR /model/old.3mf\r\n",
        b"350 Ready for destination name.\r\n",
    )
    .await;

    ctl.expect("RNTO /model/new.3mf\r\n", b"250 Rename successful.\r\n")
        .await;

    // RMD of a missing directory: a bare 550, and the parent listing doesn't show it (#392).
    ctl.expect("RMD /model/gone\r\n", b"550 \r\n").await;
    ctl.serve_listing(
        "/model",
        b"drwxr-xr-x    2 1000     1000           0 Jun 17 12:14 other\r\n",
    )
    .await;

    // DELE the printer refuses (a non-empty directory): the same bare 550, but the parent
    // listing still shows the target.
    ctl.expect("DELE /model/full\r\n", b"550 \r\n").await;
    ctl.serve_listing(
        "/model",
        b"drwxr-xr-x    2 1000     1000           0 Jun 17 12:14 full\r\n",
    )
    .await;
}

/// Mock server for `get_available_space()` when AVBL is unsupported.
///
/// There is no STAT fallback: real Bambu firmware (P1S capture)
/// responds to `STAT` with `502 Command not implemented`, so the fallback was dead code. The
/// client must now surface `Err(ProtocolViolation)` directly off the failed `AVBL` reply,
/// without ever sending `STAT`.
pub async fn run_mock_server_avbl_unsupported(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    // AVBL — unsupported, no STAT fallback follows.
    ctl.expect("AVBL\r\n", b"500 Syntax error, command unrecognized.\r\n")
        .await;
}

/// Mock server for `modification_time()` (`MDTM`) success — a well-formed `213 YYYYMMDDHHMMSS`
/// reply.
pub async fn run_mock_server_mdtm_success(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    ctl.expect("MDTM /model/job.3mf\r\n", b"213 20230415101530\r\n")
        .await;
}

/// Mock server for `modification_time()` when the firmware doesn't implement `MDTM` (`500`).
pub async fn run_mock_server_mdtm_unsupported(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    ctl.expect(
        "MDTM /model/job.3mf\r\n",
        b"500 Syntax error, command unrecognized.\r\n",
    )
    .await;
}

/// Mock server for `modification_time()` on an absent file (`550`).
pub async fn run_mock_server_mdtm_not_found(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    ctl.expect(
        "MDTM /model/gone.3mf\r\n",
        b"550 Failed to get modification time.\r\n",
    )
    .await;
}

/// Mock server for `modification_time()` with a malformed `213` body (not `YYYYMMDDHHMMSS`).
pub async fn run_mock_server_mdtm_malformed(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    ctl.expect("MDTM /model/job.3mf\r\n", b"213 not-a-timestamp\r\n")
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
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
    expected_len: usize,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    // STOR upload
    let mut server_upload_data = ctl.pasv().await;
    ctl.expect("STOR /model/job.3mf\r\n", b"150 Ok to send data.\r\n")
        .await;

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
    ctl.respond(b"426 Failure reading network stream.\r\n")
        .await;

    // SIZE verification — report the independently-known length, not the observed count.
    let cmd = ctl.read_cmd().await;
    assert!(cmd.starts_with("SIZE "));
    ctl.respond(format!("213 {}\r\n", expected_len).as_bytes())
        .await;
}

/// Mock server for upload with 426 + SIZE mismatch (should trigger DiskWriteFailure).
pub async fn run_mock_server_upload_size_mismatch(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    // STOR upload
    let mut server_upload_data = ctl.pasv().await;
    ctl.expect("STOR /model/job.3mf\r\n", b"150 Ok to send data.\r\n")
        .await;

    let mut upload_buf = vec![0u8; 100];
    let _bytes_read = server_upload_data
        .read(&mut upload_buf)
        .await
        .expect("upload data read");
    drop(server_upload_data);

    // Return 426 (TLS close race)
    ctl.respond(b"426 Failure reading network stream.\r\n")
        .await;

    // SIZE verification — report WRONG size (truncated write)
    let cmd = ctl.read_cmd().await;
    assert!(cmd.starts_with("SIZE "));
    ctl.respond(b"213 0\r\n").await;
}

/// Mock server for the data-channel desync regression test
/// (`ftps_test.rs::test_ftps_data_channel_failure_poisons_client`).
///
/// Sends the `150` reply for a `LIST` command and then stops — it deliberately never sends the
/// matching `226`. The test pairs this with a `TlsConnector` that fails the data-channel
/// connect, so the client is expected to poison itself and return before ever trying to read a
/// final reply that will never arrive; if it instead ignored the failure and tried to read the
/// control channel again, that read would hang forever against this mock.
pub async fn run_mock_server_data_channel_failure(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let _server_data = ctl.pasv().await;
    ctl.expect("LIST /model\r\n", b"150 Here comes directory listing.\r\n")
        .await;
    // Intentionally no matching 226 — the client's TLS connector fails the data-channel connect
    // before it would ever consume this reply.
}

/// Mock server for a data-channel write failure during `STOR`: sends the `150` reply, then
/// drains the data channel until the client drops it, and never sends a final reply. Paired
/// with a connector whose data stream fails its writes or flush, so the client returns before
/// reading that reply. Draining keeps the data channel open while the client writes, so a
/// write that is meant to pass through doesn't fail on a closed pipe instead.
pub async fn run_mock_server_upload_data_failure(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let mut server_data = ctl.pasv().await;
    ctl.expect("STOR /model/job.3mf\r\n", b"150 Ok to send data.\r\n")
        .await;

    let mut drain = [0u8; 1024];
    while matches!(server_data.read(&mut drain).await, Ok(n) if n > 0) {}
}

/// Mock server for the single-reply-command poisoning regression test.
///
/// Reads the `DELE` command and then drops the control stream without ever replying — the
/// client's `read_response` sees a clean 0-byte read, which `read_chunk` maps to
/// `SocketError::ConnectionReset` immediately (no 30s timeout wait needed for this test).
pub async fn run_mock_server_dele_connection_drop(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    let cmd = ctl.read_cmd().await;
    assert_eq!(cmd, "DELE /model/job.3mf\r\n");
    // Drop the stream instead of responding.
}

/// Mock server for a `DELE` answered `550` whose parent re-check listing loses the connection.
///
/// Drops the control stream on the `PASV` that opens the listing, so the client is poisoned
/// mid-`delete_file` (#392).
pub async fn run_mock_server_dele_550_listing_drop(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    ctl.expect("DELE /model/job.3mf\r\n", b"550 \r\n").await;

    let cmd = ctl.read_cmd().await;
    assert_eq!(cmd, "PASV\r\n");
    // Drop the stream instead of responding.
}

/// Mock server for the `PASV`-step coverage gap (issue #261): reads `PASV` and then drops the
/// control stream without replying. A transport failure *during* the PASV exchange must poison
/// the client, the way every other control-channel transport failure does — every existing
/// poisoning test exercises a command issued *after* PASV already succeeded, so
/// `negotiate_passive_port`'s own calls to the poisoning helpers were never covered.
pub async fn run_mock_server_pasv_connection_drop(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    let cmd = ctl.read_cmd().await;
    assert_eq!(cmd, "PASV\r\n");
    // Drop the stream instead of responding.
}

/// Mock server for the other half of issue #261: `PASV` is answered, but with a rejection code
/// rather than `227`. The control channel is still perfectly in sync, so this must surface as a
/// `ProtocolViolation` *without* poisoning the client — the asymmetry with the drop case above
/// is the thing worth pinning.
pub async fn run_mock_server_pasv_rejected(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    ctl.expect("PASV\r\n", b"425 Can't open data connection.\r\n")
        .await;

    // The client is not poisoned, so it may legitimately issue a follow-up command; answer it
    // so the assertion under test is the client's own state, not a second transport failure.
    ctl.expect("AVBL\r\n", b"213 1024000\r\n").await;
}

/// Mock server for the regression test: a transport failure between `rename_file`'s two-step
/// `RNFR`/`RNTO` sequence must poison the client the same way a single-reply command's failure
/// already does. Acks `RNFR` normally, then drops the connection instead of responding to
/// `RNTO`.
pub async fn run_mock_server_rnto_connection_drop(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    ctl.expect(
        "RNFR /model/old.3mf\r\n",
        b"350 Ready for destination name.\r\n",
    )
    .await;

    let cmd = ctl.read_cmd().await;
    assert_eq!(cmd, "RNTO /model/new.3mf\r\n");
    // Drop the stream instead of responding.
}

/// Mock server for the regression test: `LIST`'s transfer-confirmation read must accept `426`
/// (the documented P2S/X2D TLS 1.3 close race [REF-FTPS-CONN]) the same way upload/download
/// already do — upload/download both have a dedicated 426-recovery test; LIST did not.
pub async fn run_mock_server_list_426_recovery(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let mut server_data = ctl.pasv().await;
    ctl.expect("LIST /model\r\n", b"150 Here comes directory listing.\r\n")
        .await;
    server_data
        .write_all(b"-rw-r--r--    1 1000     1000      102400 Jun 17 12:14 job.3mf\r\n")
        .await
        .expect("LIST data write");
    server_data.flush().await.expect("LIST data flush");
    drop(server_data);
    // 426 instead of 226 — the TLS 1.3 close race, tolerated the same as upload/download.
    ctl.respond(b"426 Connection closed; transfer aborted.\r\n")
        .await;
}

/// Mock server for the regression test: a `426` on `LIST` whose listing was cut mid-line.
///
/// The 426 tolerance exists for the P2S/X2D TLS 1.3 close race, but unlike upload/download it has
/// no `SIZE` recheck behind it — a data channel closing early yields a listing truncated mid-line,
/// `parse_unix_listing` drops the truncated tail as just another malformed line, and the caller
/// silently gets a short file list. The final line here ends without a terminator, which is the
/// signal `list_directory` now rejects.
pub async fn run_mock_server_list_426_truncated(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let mut server_data = ctl.pasv().await;
    ctl.expect("LIST /model\r\n", b"150 Here comes directory listing.\r\n")
        .await;
    server_data
        .write_all(
            b"-rw-r--r--    1 1000     1000      102400 Jun 17 12:14 job.3mf\r\n\
              -rw-r--r--    1 1000     1000      512 Jun 17 12:1",
        )
        .await
        .expect("LIST data write");
    server_data.flush().await.expect("LIST data flush");
    drop(server_data);
    ctl.respond(b"426 Connection closed; transfer aborted.\r\n")
        .await;
}

/// Mock server for the regression test: a `426` on `LIST` with **zero** data bytes delivered.
///
/// vsFTPd reports a genuinely empty directory as `226` + zero bytes, so a 426 with an empty
/// payload means the data channel died before delivering anything — accepting it would map a
/// failed transfer onto "empty directory", the exact silent-truncation class the framing guard
/// exists to catch.
pub async fn run_mock_server_list_426_empty(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let server_data = ctl.pasv().await;
    ctl.expect("LIST /model\r\n", b"150 Here comes directory listing.\r\n")
        .await;
    // No data bytes at all — the data channel dies before delivering anything.
    drop(server_data);
    ctl.respond(b"426 Connection closed; transfer aborted.\r\n")
        .await;
}

/// Mock server for the regression test: a listing carrying one valid line and one
/// non-UTF-8 filename (a Latin-1 byte on a FAT microSD). The whole listing completes
/// normally (`226`), so only the per-line decoding is under test.
pub async fn run_mock_server_list_non_utf8_line(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let mut server_data = ctl.pasv().await;
    ctl.expect("LIST /model\r\n", b"150 Here comes directory listing.\r\n")
        .await;
    server_data
        .write_all(b"-rw-r--r--    1 1000     1000      102400 Jun 17 12:14 job.3mf\r\n")
        .await
        .expect("LIST data write");
    // 0xE9 is not valid UTF-8 on its own; the whole line must be skipped, not fail the LIST.
    server_data
        .write_all(b"-rw-r--r--    1 1000     1000      512 Jun 17 12:15 caf\xE9.3mf\r\n")
        .await
        .expect("LIST data write");
    server_data.flush().await.expect("LIST data flush");
    drop(server_data);
    ctl.respond(b"226 Transfer complete.\r\n").await;
}

/// Mock server for the regression test: `LIST`'s *initial* write/read (the `150`/`125`
/// negotiation, before the data-transfer window the single-reply-command case already covered) must
/// poison the client on failure too. Drops the control stream right after reading the `LIST`
/// command, before ever sending a `150`/`125` reply.
pub async fn run_mock_server_list_connection_drop(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let _server_data = ctl.pasv().await;
    let cmd = ctl.read_cmd().await;
    assert_eq!(cmd, "LIST /model\r\n");
    // Drop the stream instead of responding.
}

/// Mock server for the regression test: `download_file`'s confirmation-read handling
/// must accept `426` (the documented P2S/X2D TLS 1.3 close race [REF-FTPS-CONN]) and fall
/// through to the SIZE recheck, symmetric with `upload_file`'s existing 426 handling —
/// previously RETR treated 426 as an unconditional hard failure.
pub async fn run_mock_server_download_426_recovery(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let mut server_download_data = ctl.pasv().await;
    ctl.expect(
        "RETR /model/job.3mf\r\n",
        b"150 Opening data connection.\r\n",
    )
    .await;

    let payload = b"TEST_DATA";
    server_download_data
        .write_all(payload)
        .await
        .expect("download data write");
    drop(server_download_data);

    // Return 426 (TLS 1.3 close race) instead of 226 — the payload was already fully sent.
    ctl.respond(b"426 Failure reading network stream.\r\n")
        .await;

    // SIZE verification — report size matches, so download_file should still succeed.
    let cmd = ctl.read_cmd().await;
    assert!(cmd.starts_with("SIZE "));
    ctl.respond(format!("213 {}\r\n", payload.len()).as_bytes())
        .await;
}

/// Mock server whose greeting is a multi-line `220-`…`220 ` reply written in one `write_all`.
///
/// Every other mock here writes one complete single-line reply per command, so `read_response`'s
/// multi-line continuation handling (`FTP_MAX_RESPONSE_LINES`, the header-code terminator rule)
/// was only ever covered by unit tests over an in-memory reader, never through a real socket and
/// the client's own `control_fill_buf`. Ends with QUIT so the caller can assert a clean session.
pub async fn run_mock_server_multiline_greeting(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake_with_greeting(
        true,
        b"220-vsFTPd 3.0.3\r\n220-Bambu Lab storage service\r\n220 Ready.\r\n",
    )
    .await;

    ctl.expect("QUIT\r\n", b"221 Goodbye.\r\n").await;
}

/// Mock server for RETR whose `150` and `226` replies are written in a *single* `write_all`.
///
/// `read_response`'s doc credits `test_ftps_download_file` with catching the `fill_buf`-scoping
/// desync, but that test only reproduces the coalescing because the mock's two back-to-back
/// writes happen not to yield to the client task in between — nothing forces it, so the same
/// regression could slip through under a different runtime or a small reordering. Here the two
/// replies are unavoidably one socket read, so the second `read_response` *must* come from the
/// carried-over leftover bytes.
pub async fn run_mock_server_download_coalesced_replies(
    server_control: tokio::io::DuplexStream,
    data_container: DataContainer,
) {
    let mut ctl = Control::new(server_control, data_container);

    ctl.handshake(true).await;

    let mut server_data = ctl.pasv().await;
    let cmd = ctl.read_cmd().await;
    assert_eq!(cmd, "RETR /model/job.3mf\r\n");

    let payload = b"MOCK_FILE_CONTENT_FOR_DOWNLOAD";
    server_data
        .write_all(payload)
        .await
        .expect("RETR data write");
    server_data.flush().await.expect("RETR data flush");
    drop(server_data);

    // Both replies in one write: not two writes that merely tend to coalesce.
    ctl.respond(b"150 Opening data connection.\r\n226 Transfer complete.\r\n")
        .await;

    ctl.expect(
        "SIZE /model/job.3mf\r\n",
        format!("213 {}\r\n", payload.len()).as_bytes(),
    )
    .await;
}

/// Mock server for disconnect (QUIT) test.
pub async fn run_mock_server_disconnect(server_control: tokio::io::DuplexStream) {
    let mut ctl = Control::control_only(server_control);

    ctl.handshake(true).await;

    // QUIT
    ctl.expect("QUIT\r\n", b"221 Goodbye.\r\n").await;
}
