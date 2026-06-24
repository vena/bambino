//! # Mock FTPS Server
//!
//! Provides a deterministic, state-machine driven FTP server designed to test
//! the `BambuFtpsClient` over in-memory `tokio::io::duplex` streams.
//!
//! **Behavioral Design:**
//! This server expects a strict "happy path" sequence of commands matching the
//! standard Bambu Lab implicit FTPS handshake, directory listing, sizing, upload,
//! and deletion cycles. It automatically manages the passive data channel handoffs
//! by injecting dynamically created duplex streams into the shared data container
//! whenever a `PASV` command is negotiated.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

use bambino::io::TokioIo;

/// Executes the sequential mock FTP server task on the provided control stream.
///
/// * `server_control`: The server-side end of the duplex TCP control stream.
/// * `data_container`: A shared mutex where the server will deposit the client-side
///   end of dynamically generated passive data streams following `PASV` commands.
pub async fn run_mock_server(
    mut server_control: tokio::io::DuplexStream,
    data_container: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
) {
    let mut buf = vec![0u8; 1024];

    // Frame 1: Server Greeting
    server_control
        .write_all(b"220 vsFTPd 3.0.3\r\n")
        .await
        .expect("Failed to write FTP greeting");

    // Frame 2: USER login
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read USER command");
    assert!(
        core::str::from_utf8(&buf[..n])
            .expect("USER command is not valid UTF-8")
            .starts_with("USER bblp"),
        "Expected USER bblp"
    );
    server_control
        .write_all(b"331 Please specify the password.\r\n")
        .await
        .expect("Failed to write 331 response");

    // Frame 3: PASS verification
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read PASS command");
    assert!(
        core::str::from_utf8(&buf[..n])
            .expect("PASS command is not valid UTF-8")
            .starts_with("PASS 12345678"),
        "Expected PASS 12345678"
    );
    server_control
        .write_all(b"230 Login successful.\r\n")
        .await
        .expect("Failed to write 230 login response");

    // Frame 4: PBSZ configuration
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read PBSZ command");
    assert_eq!(&buf[..n], b"PBSZ 0\r\n");
    server_control
        .write_all(b"200 PBSZ set to 0.\r\n")
        .await
        .expect("Failed to write PBSZ response");

    // Frame 5: PROT protection
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read PROT command");
    assert_eq!(&buf[..n], b"PROT P\r\n");
    server_control
        .write_all(b"200 PROT level set to P.\r\n")
        .await
        .expect("Failed to write PROT response");

    // ========================================================================
    // Directory Listing (LIST) Sequence
    // ========================================================================

    // Frame 6: First passive negotiation (LIST preparation)
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read PASV command for LIST");
    assert_eq!(&buf[..n], b"PASV\r\n");

    // Establish the mock LIST data stream channel
    let (client_data, mut server_data) = tokio::io::duplex(4096);
    {
        let mut guard = data_container.lock().await;
        *guard = Some(TokioIo(client_data));
    }

    server_control
        // Port = 192 * 256 + 168 = 49320
        .write_all(b"227 Entering Passive Mode (127,0,0,1,192,168).\r\n")
        .await
        .expect("Failed to write PASV response for LIST");

    // Frame 7: LIST directory command
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read LIST command");
    assert_eq!(&buf[..n], b"LIST /model\r\n");
    server_control
        .write_all(b"150 Here comes directory listing.\r\n")
        .await
        .expect("Failed to write 150 LIST response");

    // Write mock listings directly to passive data stream
    server_data
        .write_all(b"-rw-r--r--    1 1000     1000      102400 Jun 17 12:14 job.3mf\r\n")
        .await
        .expect("Failed to write LIST data payload");
    server_data
        .flush()
        .await
        .expect("Failed to flush LIST data stream");

    // Drop the server end of the data socket to signal EOF to the client parser
    drop(server_data);

    server_control
        .write_all(b"226 Directory send OK.\r\n")
        .await
        .expect("Failed to write 226 LIST completion");

    // ========================================================================
    // Status Query Sequences
    // ========================================================================

    // Frame 8: AVBL capacity query
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read AVBL command");
    assert_eq!(&buf[..n], b"AVBL\r\n");
    server_control
        .write_all(b"213 107374182400\r\n")
        .await
        .expect("Failed to write AVBL response");

    // Frame 9: SIZE file query
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read SIZE command");
    assert_eq!(&buf[..n], b"SIZE /model/job.3mf\r\n");
    server_control
        .write_all(b"213 102400\r\n")
        .await
        .expect("Failed to write SIZE response");

    // ========================================================================
    // File Upload (STOR) Sequence
    // ========================================================================

    // Frame 10: Second passive negotiation (STOR preparation)
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read PASV command for STOR");
    assert_eq!(&buf[..n], b"PASV\r\n");

    // Establish the mock STOR data stream channel
    let (client_upload_data, mut server_upload_data) = tokio::io::duplex(4096);
    {
        let mut guard = data_container.lock().await;
        *guard = Some(TokioIo(client_upload_data));
    }

    server_control
        .write_all(b"227 Entering Passive Mode (127,0,0,1,192,168).\r\n")
        .await
        .expect("Failed to write PASV response for STOR");

    // Frame 11: STOR upload command
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read STOR command");
    assert_eq!(&buf[..n], b"STOR /model/job.3mf\r\n");
    server_control
        .write_all(b"150 Ok to send data.\r\n")
        .await
        .expect("Failed to write 150 STOR response");

    // Read uploaded mock data from passive stream
    let mut upload_buf = vec![0u8; 100];
    let bytes_read = server_upload_data
        .read(&mut upload_buf)
        .await
        .expect("Failed to read uploaded data from passive stream");
    assert_eq!(&upload_buf[..bytes_read], b"MOCK_UPLOAD_DATA");

    // Disconnect the passive channel cleanly
    drop(server_upload_data);

    server_control
        .write_all(b"226 File receive OK.\r\n")
        .await
        .expect("Failed to write 226 STOR completion");

    // Post-upload SIZE verification
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read post-upload SIZE command");
    assert_eq!(&buf[..n], b"SIZE /model/job.3mf\r\n");
    server_control
        .write_all(b"213 16\r\n")
        .await
        .expect("Failed to write post-upload SIZE response");

    // ========================================================================
    // Deletion (DELE) Sequence
    // ========================================================================

    // DELE file command
    let n = server_control
        .read(&mut buf)
        .await
        .expect("Failed to read DELE command");
    assert_eq!(&buf[..n], b"DELE /model/job.3mf\r\n");
    server_control
        .write_all(b"250 File deleted successfully.\r\n")
        .await
        .expect("Failed to write 250 DELE response");
}
