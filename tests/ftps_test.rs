//! # FTPS Client Integration & Protocol Mock Tests
//!
//! Validates the state machine transitions, passive port parsing, list directory formatting,
//! and chunked file transfer workflows of the custom implicit FTPS client.
//!
//! Employs in-memory `tokio::io::duplex` bidirectional streams to perform isolated,
//! asynchronous network simulation with zero external interface bindings or port collisions.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

use bambu_lan::discovery::BambuModel;
use bambu_lan::ftps::{BambuFtpsClient, FtpDataStreamFactory};
use bambu_lan::io::{AsyncIo, SocketError, TlsConnector, TokioIo};

// ============================================================================
// 1. Mock TLS and TCP Connection Primitives
// ============================================================================

/// Pass-through TLS connector.
///
/// Bypasses cryptographic overhead by returning the raw stream unchanged. This allows
/// deterministic verification of text-based FTP states without certificate dependencies.
struct DummyTlsConnector;

impl<RawIO: AsyncIo> TlsConnector<RawIO> for DummyTlsConnector {
    type Stream = RawIO;

    async fn connect(
        &self,
        _host: &str,
        _port: u16,
        raw_stream: RawIO,
    ) -> Result<Self::Stream, SocketError> {
        Ok(raw_stream)
    }
}

/// Dynamic, in-memory stream factory.
///
/// Swaps pre-allocated loopback duplex streams to simulate passive TCP connections.
struct MockDataStreamFactory {
    active_stream: Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
}

impl FtpDataStreamFactory<TokioIo<tokio::io::DuplexStream>> for MockDataStreamFactory {
    async fn create_data_stream(
        &self,
        _host: &str,
        _port: u16,
    ) -> Result<TokioIo<tokio::io::DuplexStream>, SocketError> {
        let mut guard = self.active_stream.lock().await;
        guard.take().ok_or(SocketError::ConnectionRefused)
    }
}

// ============================================================================
// 2. Integration Test Orchestration
// ============================================================================

#[tokio::test]
async fn test_ftps_client_lifecycle_and_operations() {
    // 1. Set up in-memory bidirectional control streams
    let (client_control, mut server_control) = tokio::io::duplex(8192);

    // 2. Prepare mock passive data streams
    let data_container = Arc::new(Mutex::new(None));
    let factory = MockDataStreamFactory {
        active_stream: data_container.clone(),
    };

    // 3. Spawn background mock FTP server
    let server_handle = tokio::spawn(async move {
        let mut buf = vec![0u8; 1024];

        // Frame 1: server greeting
        server_control
            .write_all(b"220 vsFTPd 3.0.3\r\n")
            .await
            .unwrap();

        // Frame 2: USER login
        let n = server_control.read(&mut buf).await.unwrap();
        assert!(core::str::from_utf8(&buf[..n])
            .unwrap()
            .starts_with("USER bblp"));
        server_control
            .write_all(b"331 Please specify the password.\r\n")
            .await
            .unwrap();

        // Frame 3: PASS verification
        let n = server_control.read(&mut buf).await.unwrap();
        assert!(core::str::from_utf8(&buf[..n])
            .unwrap()
            .starts_with("PASS 12345678"));
        server_control
            .write_all(b"230 Login successful.\r\n")
            .await
            .unwrap();

        // Frame 4: PBSZ configuration
        let n = server_control.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"PBSZ 0\r\n");
        server_control
            .write_all(b"200 PBSZ set to 0.\r\n")
            .await
            .unwrap();

        // Frame 5: PROT protection
        let n = server_control.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"PROT P\r\n");
        server_control
            .write_all(b"200 PROT level set to P.\r\n")
            .await
            .unwrap();

        // Frame 6: First passive negotiation (LIST preparation)
        let n = server_control.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"PASV\r\n");
        server_control
            .write_all(b"227 Entering Passive Mode (127,0,0,1,192,168).\r\n") // Port = 192 * 256 + 168 = 49320
            .await
            .unwrap();

        // Establish the mock LIST data stream channel
        let (client_data, mut server_data) = tokio::io::duplex(4096);
        {
            let mut guard = data_container.lock().await;
            *guard = Some(TokioIo(client_data));
        }

        // Frame 7: LIST directory command
        let n = server_control.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"LIST /model\r\n");
        server_control
            .write_all(b"150 Here comes directory listing.\r\n")
            .await
            .unwrap();

        // Write mock listings directly to passive data stream
        server_data
            .write_all(b"-rw-r--r--    1 1000     1000      102400 Jun 17 12:14 job.3mf\r\n")
            .await
            .unwrap();
        server_data.flush().await.unwrap();
        drop(server_data); // Complete transfer by closing data socket (EOF)

        server_control
            .write_all(b"226 Directory send OK.\r\n")
            .await
            .unwrap();

        // Frame 8: AVBL capacity query
        let n = server_control.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"AVBL\r\n");
        server_control
            .write_all(b"213 107374182400\r\n")
            .await
            .unwrap();

        // Frame 9: SIZE file query
        let n = server_control.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"SIZE /model/job.3mf\r\n");
        server_control.write_all(b"213 102400\r\n").await.unwrap();

        // Frame 10: Second passive negotiation (STOR preparation)
        let n = server_control.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"PASV\r\n");
        server_control
            .write_all(b"227 Entering Passive Mode (127,0,0,1,192,168).\r\n")
            .await
            .unwrap();

        // Establish the mock STOR data stream channel
        let (client_upload_data, mut server_upload_data) = tokio::io::duplex(4096);
        {
            let mut guard = data_container.lock().await;
            *guard = Some(TokioIo(client_upload_data));
        }

        // Frame 11: STOR upload command
        let n = server_control.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"STOR /model/job.3mf\r\n");
        server_control
            .write_all(b"150 Ok to send data.\r\n")
            .await
            .unwrap();

        // Read uploaded mock data from passive stream
        let mut upload_buf = vec![0u8; 100];
        let bytes_read = server_upload_data.read(&mut upload_buf).await.unwrap();
        assert_eq!(&upload_buf[..bytes_read], b"MOCK_UPLOAD_DATA");
        drop(server_upload_data);

        server_control
            .write_all(b"226 File receive OK.\r\n")
            .await
            .unwrap();

        // Frame 12: DELE file command
        let n = server_control.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"DELE /model/job.3mf\r\n");
        server_control
            .write_all(b"250 File deleted successfully.\r\n")
            .await
            .unwrap();
    });

    // 4. Initialize client and execute transactional flows
    let mut client = BambuFtpsClient::connect(
        TokioIo(client_control),
        DummyTlsConnector,
        factory,
        BambuModel::P1S,
        "127.0.0.1",
        "12345678",
    )
    .await
    .unwrap();

    // Verify Directory List retrieval
    let list = client
        .list_directory("/model", 2026, 6, 17, 15, 0)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "job.3mf");
    assert_eq!(list[0].size, 102400);

    // Verify Capacity retrieval
    let space = client.get_available_space().await.unwrap();
    assert_eq!(space, 107_374_182_400);

    // Verify SIZE file parameter retrieval
    let size = client.get_file_size("/model/job.3mf").await.unwrap();
    assert_eq!(size, 102400);

    // Verify Upload pipeline
    client
        .upload_file("/model/job.3mf", b"MOCK_UPLOAD_DATA")
        .await
        .unwrap();

    // Verify Deletion
    client.delete_file("/model/job.3mf").await.unwrap();

    // Clean up mock background server thread
    server_handle.await.unwrap();
}
