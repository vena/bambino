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

use bambino::ftps::BambuFtpsClient;
use bambino::io::TokioIo;
use bambino::models::BambuModel;

use common::io::{DummyTlsConnector, MockDataStreamFactory};
use common::mock_ftps;

/// Helper: creates the standard test infrastructure (duplex control stream, data container, factory).
fn setup() -> (
    tokio::io::DuplexStream,
    tokio::io::DuplexStream,
    Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
    MockDataStreamFactory,
) {
    let (client_control, server_control) = tokio::io::duplex(8192);
    let data_container = Arc::new(Mutex::new(None));
    let factory = MockDataStreamFactory {
        active_stream: data_container.clone(),
    };
    (client_control, server_control, data_container, factory)
}

/// Helper: connects the FTPS client using the standard test infrastructure.
async fn connect_client(
    client_control: tokio::io::DuplexStream,
    factory: MockDataStreamFactory,
    model: BambuModel,
) -> BambuFtpsClient<TokioIo<tokio::io::DuplexStream>, DummyTlsConnector, MockDataStreamFactory> {
    BambuFtpsClient::connect(
        TokioIo(client_control),
        DummyTlsConnector,
        factory,
        model,
        "127.0.0.1",
        "12345678",
    )
    .await
    .expect("FTPS handshake failed")
}

#[tokio::test]
async fn test_ftps_client_lifecycle_and_operations() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::P1S).await;

    let list = client
        .list_directory("/model", 2026, 6, 17, 15, 0)
        .await
        .expect("LIST failed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "job.3mf");
    assert_eq!(list[0].size, 102400);

    let space = client.get_available_space().await.expect("AVBL failed");
    assert_eq!(space, 107_374_182_400);

    let size = client
        .get_file_size("/model/job.3mf")
        .await
        .expect("SIZE failed");
    assert_eq!(size, 102400);

    client
        .upload_file("/model/job.3mf", b"MOCK_UPLOAD_DATA")
        .await
        .expect("STOR upload failed");

    client
        .delete_file("/model/job.3mf")
        .await
        .expect("DELE failed");

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_a1_plaintext_data_channel() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_a1_plaintext(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::A1).await;

    let list = client
        .list_directory("/", 2026, 6, 17, 15, 0)
        .await
        .expect("LIST failed on A1 plaintext path");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "cache");
    assert!(list[0].is_dir);

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_download_file() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_download(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::P1S).await;

    let data = client
        .download_file("/model/job.3mf")
        .await
        .expect("RETR download failed");
    assert_eq!(data, b"MOCK_FILE_CONTENT_FOR_DOWNLOAD");

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_directory_operations() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_dir_ops(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::P1S).await;

    client
        .create_directory("/model/subdir")
        .await
        .expect("MKD failed");

    client
        .remove_directory("/model/subdir")
        .await
        .expect("RMD failed");

    client
        .rename_file("/model/old.3mf", "/model/new.3mf")
        .await
        .expect("RNFR/RNTO failed");

    // RMD on non-existent directory should succeed (550 = idempotent)
    client
        .remove_directory("/model/gone")
        .await
        .expect("RMD on absent directory should be idempotent");

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_stat_fallback_for_available_space() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_stat_fallback(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::P1S).await;

    let space = client
        .get_available_space()
        .await
        .expect("STAT fallback space query failed");
    assert_eq!(space, 14_820_352_000);

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_upload_426_recovery_via_size() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_upload_426_recovery(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::P2S).await;

    client
        .upload_file("/model/job.3mf", b"TEST_DATA")
        .await
        .expect("Upload should succeed via 426 + SIZE verification recovery");

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_upload_size_mismatch_returns_disk_failure() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_upload_size_mismatch(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::P2S).await;

    let result = client.upload_file("/model/job.3mf", b"TEST_DATA").await;
    assert!(
        matches!(result, Err(bambino::error::BambuError::DiskWriteFailure)),
        "Expected DiskWriteFailure on SIZE mismatch, got {:?}",
        result
    );

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_disconnect() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::P1S).await;

    client.disconnect().await;

    server_handle.await.expect("Mock server panicked");
}
