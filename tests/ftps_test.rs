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

use bambino::client::DummyTimer;
use bambino::error::BambuError;
use bambino::ftps::BambuFtpsClient;
use bambino::io::TokioIo;
use bambino::models::BambuModel;

use bambino::io::TlsVersion;

use common::io::{
    DummyTlsConnector, FailingDataTlsConnector, HostCapturingTlsConnector, MockDataStreamFactory,
    VersionReportingTlsConnector,
};
use common::mock_ftps;

/// Return type of [`setup()`]: control-stream pair, shared data-stream container, and factory.
type SetupResult = (
    tokio::io::DuplexStream,
    tokio::io::DuplexStream,
    Arc<Mutex<Option<TokioIo<tokio::io::DuplexStream>>>>,
    MockDataStreamFactory,
);

/// Helper: creates the standard test infrastructure (duplex control stream, data container, factory).
fn setup() -> SetupResult {
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
) -> BambuFtpsClient<
    TokioIo<tokio::io::DuplexStream>,
    DummyTlsConnector,
    MockDataStreamFactory,
    DummyTimer,
> {
    BambuFtpsClient::connect(
        TokioIo(client_control),
        DummyTlsConnector,
        factory,
        model,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        false,
    )
    .await
    .expect("FTPS handshake failed")
}

/// Regression test for `TLS_SNI_HOSTNAME_MISMATCH_PLAN.md`: the control-channel TLS connect
/// must send the printer's serial as the SNI/identity value, never the IP — the printer's cert
/// has the serial in its Subject CN and no SAN, so a verified connection checking hostname
/// against the IP could never match.
#[tokio::test]
async fn test_ftps_control_channel_connects_with_serial_not_ip() {
    let (client_control, server_control, data_container, factory) = setup();
    let server_handle = tokio::spawn(mock_ftps::run_mock_server(
        server_control,
        data_container.clone(),
    ));

    let (connector, captured_host) = HostCapturingTlsConnector::new();
    let mut client = BambuFtpsClient::connect(
        TokioIo(client_control),
        connector,
        factory,
        BambuModel::P1S,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        false,
    )
    .await
    .expect("FTPS handshake failed");

    assert_eq!(
        captured_host.lock().await.as_deref(),
        Some("TEST0000000001"),
        "control-channel TLS connect must use the serial, not the IP, as SNI/identity"
    );

    client.disconnect().await;
    server_handle.abort();
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

/// review/ftps.md Phase 7c: the STAT fallback was removed (confirmed dead against real
/// firmware — a P1S capture found `STAT` unimplemented, `502 Command not implemented`).
/// `get_available_space()` must now surface `Err(ProtocolViolation)` directly off a failed
/// `AVBL` reply, without ever attempting a STAT round-trip.
#[tokio::test]
async fn test_ftps_avbl_failure_returns_error_without_stat_fallback() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_avbl_unsupported(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::P1S).await;

    let result = client.get_available_space().await;
    assert!(
        matches!(result, Err(BambuError::ProtocolViolation(_))),
        "expected ProtocolViolation on a non-success AVBL reply, got {:?}",
        result.map(|_| ())
    );

    server_handle.await.expect("Mock server panicked");
}

/// Regression test for review/ftps.md Phase 2: a data-channel TLS connect failure after the
/// server has already sent its `150` reply must poison the client, so the *next* command on the
/// same instance fails immediately and cleanly instead of hanging, panicking, or silently
/// misreading a stale trailing reply as its own response.
#[tokio::test]
async fn test_ftps_data_channel_failure_poisons_client() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_data_channel_failure(
        server_control,
        data_container.clone(),
    ));

    let mut client = BambuFtpsClient::connect(
        TokioIo(client_control),
        FailingDataTlsConnector::new(),
        factory,
        BambuModel::P1S,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        false,
    )
    .await
    .expect("FTPS handshake failed");

    let result = client.list_directory("/model", 2026, 6, 17, 15, 0).await;
    assert!(
        matches!(result, Err(bambino::error::BambuError::NetworkError(_))),
        "Expected the data-channel TLS connect failure to surface as NetworkError, got {:?}",
        result
    );

    // The control channel now has an unread 150 reply pending with no matching final reply ever
    // coming — the client must be poisoned, not left in a state where the next command could
    // hang waiting for that reply or misread it as its own response.
    let next_result = client.get_available_space().await;
    assert!(
        matches!(
            next_result,
            Err(bambino::error::BambuError::ProtocolViolation(_))
        ),
        "Expected the poisoned client to reject the next command with ProtocolViolation, got {:?}",
        next_result
    );

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_upload_426_recovery_via_size() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_upload_426_recovery(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, BambuModel::P1S).await;

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

    let mut client = connect_client(client_control, factory, BambuModel::P1S).await;

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

#[tokio::test]
async fn test_ftps_tls13_rejected_for_p2s() {
    let (client_control, _server_control, _data_container, factory) = setup();

    let result = BambuFtpsClient::connect(
        TokioIo(client_control),
        VersionReportingTlsConnector(Some(TlsVersion::Tls13)),
        factory,
        BambuModel::P2S,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        false,
    )
    .await;

    match result {
        Err(bambino::error::BambuError::ProtocolViolation(_)) => {}
        Err(e) => panic!("Expected ProtocolViolation for TLS 1.3 on P2S, got {:?}", e),
        Ok(_) => panic!("Expected error for TLS 1.3 on P2S, but connect succeeded"),
    }
}

#[tokio::test]
async fn test_ftps_tls13_rejected_for_x2d() {
    let (client_control, _server_control, _data_container, factory) = setup();

    let result = BambuFtpsClient::connect(
        TokioIo(client_control),
        VersionReportingTlsConnector(Some(TlsVersion::Tls13)),
        factory,
        BambuModel::X2D,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        false,
    )
    .await;

    match result {
        Err(bambino::error::BambuError::ProtocolViolation(_)) => {}
        Err(e) => panic!("Expected ProtocolViolation for TLS 1.3 on X2D, got {:?}", e),
        Ok(_) => panic!("Expected error for TLS 1.3 on X2D, but connect succeeded"),
    }
}

#[tokio::test]
async fn test_ftps_tls12_accepted_for_p2s() {
    let (client_control, server_control, _data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(
        server_control,
        Arc::new(Mutex::new(None)),
    ));

    let mut client = BambuFtpsClient::connect(
        TokioIo(client_control),
        VersionReportingTlsConnector(Some(TlsVersion::Tls12)),
        factory,
        BambuModel::P2S,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        false,
    )
    .await
    .expect("TLS 1.2 should be accepted for P2S");

    client.disconnect().await;
    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_tls13_accepted_for_p1s() {
    let (client_control, server_control, _data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(
        server_control,
        Arc::new(Mutex::new(None)),
    ));

    let mut client = BambuFtpsClient::connect(
        TokioIo(client_control),
        VersionReportingTlsConnector(Some(TlsVersion::Tls13)),
        factory,
        BambuModel::P1S,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        false,
    )
    .await
    .expect("TLS 1.3 should be accepted for P1S");

    client.disconnect().await;
    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_version_none_rejected_for_p2s() {
    let (client_control, _server_control, _data_container, factory) = setup();

    let result = BambuFtpsClient::connect(
        TokioIo(client_control),
        VersionReportingTlsConnector(None),
        factory,
        BambuModel::P2S,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        false,
    )
    .await;

    match result {
        Err(bambino::error::BambuError::ProtocolViolation(_)) => {}
        Err(e) => panic!(
            "Expected ProtocolViolation for undetermined TLS version on P2S, got {:?}",
            e
        ),
        Ok(_) => {
            panic!("Expected error for undetermined TLS version on P2S, but connect succeeded")
        }
    }
}

/// Track A of EMBASSY_TLS_ESCAPE_HATCH_PLAN.md: `allow_unverified_tls_1_2 == true` must bypass
/// `require_tls_1_2_if_enforced`'s rejection even though P2S enforces TLS 1.2 and the mock
/// connector reports TLS 1.3 was negotiated — mirrors `test_ftps_tls13_rejected_for_p2s` but
/// with the bypass flag set, so `connect()` must now succeed instead of erroring.
#[tokio::test]
async fn test_ftps_tls13_bypassed_for_p2s_when_allow_unverified() {
    let (client_control, server_control, _data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(
        server_control,
        Arc::new(Mutex::new(None)),
    ));

    let mut client = BambuFtpsClient::connect(
        TokioIo(client_control),
        VersionReportingTlsConnector(Some(TlsVersion::Tls13)),
        factory,
        BambuModel::P2S,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        true,
    )
    .await
    .expect("allow_unverified_tls_1_2 should bypass the TLS 1.3 rejection for P2S");

    client.disconnect().await;
    server_handle.await.expect("Mock server panicked");
}

/// Same as above but for the undetermined-version (`None`) case — mirrors
/// `test_ftps_version_none_rejected_for_p2s` but with the bypass flag set.
#[tokio::test]
async fn test_ftps_version_none_bypassed_for_p2s_when_allow_unverified() {
    let (client_control, server_control, _data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(
        server_control,
        Arc::new(Mutex::new(None)),
    ));

    let mut client = BambuFtpsClient::connect(
        TokioIo(client_control),
        VersionReportingTlsConnector(None),
        factory,
        BambuModel::P2S,
        "127.0.0.1",
        "TEST0000000001",
        "12345678",
        DummyTimer,
        true,
    )
    .await
    .expect("allow_unverified_tls_1_2 should bypass the undetermined-version rejection for P2S");

    client.disconnect().await;
    server_handle.await.expect("Mock server panicked");
}
