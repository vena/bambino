//! # FTPS Client Integration & Protocol Mock Tests
//!
//! Validates the state machine transitions, passive port parsing, list directory formatting,
//! and chunked file transfer workflows of the custom implicit FTPS client.
//!
//! Employs in-memory `tokio::io::duplex` bidirectional streams to perform isolated,
//! asynchronous network simulation utilizing the shared test infrastructure, preventing
//! port collisions and flaky cryptography checks.

mod common;

use std::sync::Arc;
use tokio::sync::Mutex;

use bambu_lan::discovery::BambuModel;
use bambu_lan::ftps::BambuFtpsClient;
use bambu_lan::io::TokioIo;

use common::io::{DummyTlsConnector, MockDataStreamFactory};
use common::mock_ftps::run_mock_server;

#[tokio::test]
async fn test_ftps_client_lifecycle_and_operations() {
    // 1. Set up in-memory bidirectional control streams
    let (client_control, server_control) = tokio::io::duplex(8192);

    // 2. Prepare mock passive data streams container for dynamic channel allocation
    let data_container = Arc::new(Mutex::new(None));
    let factory = MockDataStreamFactory {
        active_stream: data_container.clone(),
    };

    // 3. Spawn background mock FTP server fixture
    let server_handle = tokio::spawn(run_mock_server(server_control, data_container.clone()));

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
    .expect("Failed to execute FTPS login and security handshake");

    // Verify Directory List retrieval
    let list = client
        .list_directory("/model", 2026, 6, 17, 15, 0)
        .await
        .expect("Failed to retrieve and parse UNIX directory listing");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "job.3mf");
    assert_eq!(list[0].size, 102400);

    // Verify Capacity retrieval
    let space = client
        .get_available_space()
        .await
        .expect("Failed to query hardware capacity bounds");
    assert_eq!(space, 107_374_182_400);

    // Verify SIZE file parameter retrieval
    let size = client
        .get_file_size("/model/job.3mf")
        .await
        .expect("Failed to query exact file size");
    assert_eq!(size, 102400);

    // Verify Chunked Upload pipeline
    client
        .upload_file("/model/job.3mf", b"MOCK_UPLOAD_DATA")
        .await
        .expect("Failed to complete chunked binary upload");

    // Verify Clean Deletion
    client
        .delete_file("/model/job.3mf")
        .await
        .expect("Failed to command file removal");

    // Ensure the background server executed cleanly to completion
    server_handle
        .await
        .expect("Background mock FTPS server panicked");
}
