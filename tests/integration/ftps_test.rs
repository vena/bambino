//! # FTPS Client Integration & Protocol Mock Tests
//!
//! Validates the state machine transitions, passive port parsing, list directory formatting,
//! and chunked file transfer workflows of the custom implicit FTPS client.
//!
//! Employs in-memory `tokio::io::duplex` bidirectional streams to perform isolated,
//! asynchronous network simulation utilizing the shared test infrastructure, preventing
//! port collisions and flaky cryptography checks.

use bambino::client::DummyTimer;
use bambino::error::Error;
use bambino::ftps::{CurrentDateTime, FtpTimestamp, FtpsClient, TlsVersionCheck};
use bambino::io::TokioIo;
use bambino::models::PrinterModel;

use bambino::io::{TlsConnector, TlsVersion};

/// The printer clock every listing test parses against.
const REF_NOW: CurrentDateTime = CurrentDateTime::new(2026, 6, 17, 15, 0);

use crate::common::client::{SERIAL, test_identity};
use crate::common::io::{
    CloseCountingTlsConnector, DataContainer, DummyTlsConnector, FailingDataTlsConnector,
    FaultyDataTlsConnector, HostCapturingTlsConnector, MockDataStreamFactory,
    PerCallVersionReportingTlsConnector, VersionReportingTlsConnector,
};
use crate::common::mock_ftps;

/// Return type of [`setup()`]: control-stream pair, shared data-stream container, and factory.
type SetupResult = (
    tokio::io::DuplexStream,
    tokio::io::DuplexStream,
    DataContainer,
    MockDataStreamFactory,
);

/// Helper: creates the standard test infrastructure (duplex control stream, data container, factory).
fn setup() -> SetupResult {
    let (client_control, server_control) = tokio::io::duplex(8192);
    let factory = MockDataStreamFactory::empty();
    let data_container = factory.active_stream.clone();
    (client_control, server_control, data_container, factory)
}

/// The FTPS client every test here connects, over `Tls`.
type TestFtps<Tls> =
    FtpsClient<TokioIo<tokio::io::DuplexStream>, Tls, MockDataStreamFactory, DummyTimer>;

/// Helper: connects an FTPS client as `model` over `tls`, returning the handshake's result.
async fn try_connect_with<Tls: TlsConnector<TokioIo<tokio::io::DuplexStream>>>(
    client_control: tokio::io::DuplexStream,
    tls: Tls,
    factory: MockDataStreamFactory,
    model: PrinterModel,
    tls_version_check: TlsVersionCheck,
) -> Result<TestFtps<Tls>, Error> {
    FtpsClient::connect(
        TokioIo::new(client_control),
        tls,
        factory,
        test_identity(model),
        DummyTimer,
        tls_version_check,
    )
    .await
}

/// Helper: connects an FTPS client as `model` over `tls`, enforcing the TLS version check.
async fn connect_client_with<Tls: TlsConnector<TokioIo<tokio::io::DuplexStream>>>(
    client_control: tokio::io::DuplexStream,
    tls: Tls,
    factory: MockDataStreamFactory,
    model: PrinterModel,
) -> TestFtps<Tls> {
    try_connect_with(
        client_control,
        tls,
        factory,
        model,
        TlsVersionCheck::Enforce,
    )
    .await
    .expect("FTPS handshake failed")
}

/// Helper: connects the FTPS client using the standard test infrastructure.
async fn connect_client(
    client_control: tokio::io::DuplexStream,
    factory: MockDataStreamFactory,
    model: PrinterModel,
) -> TestFtps<DummyTlsConnector> {
    connect_client_with(client_control, DummyTlsConnector, factory, model).await
}

/// Regression test for `.claude/rules/tls-identity-sni.md`: the control-channel TLS connect
/// must send the printer's serial as the SNI/identity value, never the IP — the printer's
/// cert has the serial in its Subject CN and no SAN, so a verified connection checking
/// hostname against the IP could never match.
#[tokio::test]
async fn test_ftps_control_channel_connects_with_serial_not_ip() {
    let (client_control, server_control, data_container, factory) = setup();
    let server_handle = tokio::spawn(mock_ftps::run_mock_server(
        server_control,
        data_container.clone(),
    ));

    let (connector, captured_host) = HostCapturingTlsConnector::new();
    let client = connect_client_with(client_control, connector, factory, PrinterModel::P1S).await;

    assert_eq!(
        captured_host.lock().await.as_deref(),
        Some(SERIAL),
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

    // The PASV reply advertises port 49320 (192*256+168) and the client is configured with IP
    // 127.0.0.1. Nothing observed either value before: `MockDataStreamFactory::dial` hands back
    // the same preloaded stream whatever it is asked for, so a regression dialing port 0, a
    // stale port, or the serial in place of the IP left every FTPS test green.
    let dialed = factory.dialed.clone();
    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let list = client
        .list_directory("/model", REF_NOW)
        .await
        .expect("LIST failed");
    assert_eq!(
        dialed.lock().await.as_slice(),
        [("127.0.0.1".to_string(), 49320u16)],
        "LIST's data channel must dial the printer IP on the PASV-advertised port"
    );
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
    assert_eq!(
        dialed.lock().await.as_slice(),
        [
            ("127.0.0.1".to_string(), 49320u16),
            ("127.0.0.1".to_string(), 49320u16)
        ],
        "STOR's data channel must re-dial the IP on the second PASV's advertised port"
    );

    client
        .delete_file("/model/job.3mf")
        .await
        .expect("DELE failed");

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_upload_multi_chunk_reassembles_correctly() {
    // Proves upload_file's chunked write loop (FTPS_UPLOAD_CHUNK_SIZE = 65536)
    // reassembles correctly server-side across more than one chunk — every other upload test
    // uses a payload far under one chunk, so this loop's second-and-later iterations were
    // previously untested. Payload size is chosen to force exactly two chunks with a
    // non-power-of-two final partial chunk.
    let payload: Vec<u8> = (0..77_881usize).map(|i| (i % 256) as u8).collect();

    let (client_control, server_control, data_container, factory) = setup();
    let server_handle = tokio::spawn(mock_ftps::run_mock_server_upload_multi_chunk(
        server_control,
        data_container.clone(),
        payload.len(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    client
        .upload_file("/model/big.bin", &payload)
        .await
        .expect("multi-chunk STOR upload failed");

    let received = server_handle.await.expect("Mock server panicked");
    assert_eq!(
        received, payload,
        "payload must reassemble byte-for-byte across chunk boundaries"
    );

    client.disconnect().await;
}

#[tokio::test]
async fn test_ftps_a1_plaintext_data_channel() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_a1_plaintext(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::A1).await;

    let list = client
        .list_directory("/", REF_NOW)
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

    let dialed = factory.dialed.clone();
    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let data = client
        .download_file("/model/job.3mf")
        .await
        .expect("RETR download failed");
    assert_eq!(data, b"MOCK_FILE_CONTENT_FOR_DOWNLOAD");
    assert_eq!(
        dialed.lock().await.as_slice(),
        [("127.0.0.1".to_string(), 49320u16)],
        "RETR's data channel must dial the printer IP on the PASV-advertised port"
    );

    server_handle.await.expect("Mock server panicked");
}

/// `download_prefix` with a buffer shorter than the file returns the file's first bytes, ends the
/// data channel with an RST, and leaves the client spent (GitHub issue #629), on both the TLS and
/// the A1's plaintext data channel.
#[tokio::test]
async fn test_ftps_download_prefix_stops_early_and_ends_the_session() {
    for (model, prot_p) in [(PrinterModel::P1S, true), (PrinterModel::A1, false)] {
        let (client_control, server_control, data_container, factory) = setup();
        let server_handle = tokio::spawn(mock_ftps::run_mock_server_download_prefix(
            server_control,
            data_container.clone(),
            prot_p,
        ));
        let aborted = factory.aborted.clone();
        let mut client = connect_client(client_control, factory, model).await;

        let mut buf = [0u8; 8];
        let n = client
            .download_prefix("/model/job.3mf", &mut buf)
            .await
            .expect("prefix read failed");
        assert_eq!(&buf[..n], &mock_ftps::PREFIX_FILE[..8], "{model:?}");
        assert_eq!(
            aborted.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "{model:?}: an early stop must end the data channel with an RST"
        );
        assert!(
            client.is_poisoned(),
            "{model:?}: the printer closes the control connection after the RST"
        );
        assert!(
            matches!(
                client.get_available_space().await,
                Err(Error::ProtocolViolation(_))
            ),
            "{model:?}: no command may follow an early stop"
        );

        server_handle.await.expect("Mock server panicked");
    }
}

/// Appends one `.3mf` entry as the slicers write it: flag bit 3, stored, then a signed data
/// descriptor. CRC-32 is computed bitwise here so the test doesn't share the library's.
fn stored_entry(out: &mut Vec<u8>, name: &str, data: &[u8]) {
    let crc = !data.iter().fold(!0u32, |mut crc, &b| {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
        crc
    });
    out.extend_from_slice(b"PK\x03\x04\x14\x00\x08\x08\x00\x00");
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(b"PK\x07\x08");
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
}

/// A PNG of `pixels` IDAT bytes: signature, IDAT, IEND (chunk CRCs zeroed).
fn test_png(pixels: &[u8]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [(&b"IDAT"[..], pixels), (b"IEND", &[])] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&[0; 4]);
    }
    out
}

/// `read_front_members` returns the members byte for byte and stops the transfer once it has
/// them: the file's tail is never read, so the data channel ends with an RST and the session
/// with it. A buffer too small to reach them is refused rather than returning a cut member.
#[tokio::test]
async fn test_ftps_read_front_members_stops_after_the_members() {
    use bambino::ftps::project::{pick_image_member, plate_image_member};

    let plate = test_png(&[7u8; 6000]);
    let pick = test_png(&[9u8; 3000]);
    let mut file = Vec::new();
    stored_entry(&mut file, "Metadata/plate_1.png", &plate);
    stored_entry(&mut file, "Metadata/pick_1.png", &pick);
    let front_len = file.len();
    stored_entry(&mut file, "3D/3dmodel.model", &[1u8; 64 * 1024]);
    let (plate_name, pick_name) = (plate_image_member(1), pick_image_member(1));

    for (buf_len, fits) in [(front_len + 8192, true), (front_len - 1, false)] {
        let (client_control, server_control, data_container, factory) = setup();
        let server_handle = tokio::spawn(mock_ftps::run_mock_server_front_members(
            server_control,
            data_container.clone(),
            file.clone(),
        ));
        let aborted = factory.aborted.clone();
        let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

        let mut buf = vec![0u8; buf_len];
        let result = client
            .read_front_members(
                "/model/job.3mf",
                &mut buf,
                [plate_name.as_str(), pick_name.as_str()],
            )
            .await;
        if fits {
            let [p, k] = result.expect("members read");
            assert_eq!(p, Some(&plate[..]));
            assert_eq!(k, Some(&pick[..]));
        } else {
            assert!(
                matches!(result, Err(Error::InvalidArgument(_))),
                "{:?}",
                result.map(|_| ())
            );
        }
        assert_eq!(aborted.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(client.is_poisoned());

        server_handle.await.expect("Mock server panicked");
    }
}

/// A file shorter than the buffer completes like a normal `RETR`: no RST, and the session goes on.
#[tokio::test]
async fn test_ftps_download_prefix_of_a_short_file_keeps_the_session() {
    let (client_control, server_control, data_container, factory) = setup();
    let server_handle = tokio::spawn(mock_ftps::run_mock_server_download_prefix_short_file(
        server_control,
        data_container.clone(),
    ));
    let aborted = factory.aborted.clone();
    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let mut buf = [0u8; 256];
    let n = client
        .download_prefix("/model/job.3mf", &mut buf)
        .await
        .expect("prefix read failed");
    assert_eq!(&buf[..n], mock_ftps::PREFIX_FILE);
    assert_eq!(aborted.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(!client.is_poisoned());
    client
        .get_available_space()
        .await
        .expect("AVBL after a completed prefix read");

    server_handle.await.expect("Mock server panicked");
}

/// An empty buffer is refused before anything is sent: the mock's next expected command is QUIT.
#[tokio::test]
async fn test_ftps_download_prefix_rejects_an_empty_buffer() {
    let (client_control, server_control, _, factory) = setup();
    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(server_control));
    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.download_prefix("/model/job.3mf", &mut []).await;
    assert!(
        matches!(result, Err(Error::InvalidArgument(_))),
        "{result:?}"
    );
    client.disconnect().await;

    server_handle.await.expect("Mock server panicked");
}

/// Regression test for the `fill_buf`-scoping desync `read_response`'s doc describes, made
/// deterministic: the mock writes `150 ...\r\n226 ...\r\n` in one `write_all`, so both replies
/// are unavoidably in one socket read. `test_ftps_download_file` reproduces the same coalescing
/// only by scheduler luck (two back-to-back writes that happen not to yield), so it could stop
/// covering this under a different runtime or a small reordering.
#[tokio::test]
async fn test_ftps_download_with_coalesced_replies_in_one_write() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_download_coalesced_replies(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let data = client
        .download_file("/model/job.3mf")
        .await
        .expect("RETR download failed with 150/226 delivered in a single socket read");
    assert_eq!(data, b"MOCK_FILE_CONTENT_FOR_DOWNLOAD");

    server_handle.await.expect("Mock server panicked");
}

/// A multi-line `220-`…`220 ` greeting (RFC 959 §4.2, and what several real FTP daemons send)
/// must be consumed as one reply. Covered only by unit tests over an in-memory reader before —
/// no mock here ever emitted a multi-line reply, so the client's own `control_fill_buf` never
/// saw one.
#[tokio::test]
async fn test_ftps_connect_accepts_multiline_greeting() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_multiline_greeting(
        server_control,
    ));

    let client = connect_client(client_control, factory, PrinterModel::P1S).await;
    client.disconnect().await;

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_download_size_mismatch_returns_protocol_violation() {
    // download_file previously trusted a clean 226 alone, with no comparison against
    // the file's expected length — a data channel that closes early while the server still
    // emits 226 was silently reported as a successful (but truncated) download.
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_download_size_mismatch(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.download_file("/model/job.3mf").await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "Expected ProtocolViolation on a downloaded-size/SIZE mismatch, got {:?}",
        result.map(|data| data.len())
    );

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_directory_operations() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_dir_ops(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    client
        .create_directory("/model/subdir")
        .await
        .expect("MKD failed");

    client
        .create_directory("/model/other")
        .await
        .expect("MKD answered 250 (as a P1S does) must succeed");

    client
        .remove_directory("/model/subdir")
        .await
        .expect("RMD failed");

    client
        .rename_file("/model/old.3mf", "/model/new.3mf")
        .await
        .expect("RNFR/RNTO failed");

    // A 550 for a target the parent listing no longer shows is "already gone".
    client
        .remove_directory("/model/gone")
        .await
        .expect("RMD on absent directory should be idempotent");

    // A 550 for a target the parent listing still shows is a refusal, not success.
    let refused = client.delete_file("/model/full").await;
    assert!(
        matches!(
            refused,
            Err(Error::FtpReply {
                command: "DELE",
                code: 550,
                ..
            })
        ),
        "a refused DELE must not report success: {refused:?}"
    );

    server_handle.await.expect("Mock server panicked");
}

/// There is no STAT fallback (it was removed as dead code: a P1S capture found `STAT`
/// unimplemented, `502 Command not implemented`).
/// `get_available_space()` must now surface `Err(ProtocolViolation)` directly off a failed
/// `AVBL` reply, without ever attempting a STAT round-trip.
#[tokio::test]
async fn test_ftps_avbl_failure_returns_error_without_stat_fallback() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_avbl_unsupported(server_control));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.get_available_space().await;
    assert!(
        matches!(
            result,
            Err(Error::FtpReply {
                command: "AVBL",
                ..
            })
        ),
        "expected FtpReply on a non-success AVBL reply, got {:?}",
        result.map(|_| ())
    );

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_mdtm_success() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_mdtm_success(server_control));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client
        .modification_time("/model/job.3mf")
        .await
        .expect("MDTM should succeed");
    assert_eq!(
        result,
        Some(FtpTimestamp {
            year: 2023,
            month: 4,
            day: 15,
            hour: 10,
            minute: 15,
            second: 30,
        })
    );

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_mdtm_unsupported_returns_none() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_mdtm_unsupported(server_control));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client
        .modification_time("/model/job.3mf")
        .await
        .expect("unsupported MDTM should degrade to Ok(None), not Err");
    assert_eq!(result, None);

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_mdtm_not_found_returns_error() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_mdtm_not_found(server_control));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.modification_time("/model/gone.3mf").await;
    assert!(
        matches!(
            result,
            Err(Error::FtpReply {
                command: "MDTM",
                code: 550,
                ..
            })
        ),
        "expected FtpReply on a 550 MDTM reply, got {:?}",
        result
    );

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_mdtm_malformed_body_returns_error() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_mdtm_malformed(server_control));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.modification_time("/model/job.3mf").await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "expected ProtocolViolation on a malformed 213 body, got {:?}",
        result
    );

    server_handle.await.expect("Mock server panicked");
}

/// Regression test for control-channel desync (`.claude/rules/ftps-poisoning.md`): a
/// data-channel TLS connect failure after the server has already sent its `150` reply must
/// poison the client, so the *next* command on the same instance fails immediately and cleanly
/// instead of hanging, panicking, or silently misreading a stale trailing reply as its own
/// response.
#[tokio::test]
async fn test_ftps_data_channel_failure_poisons_client() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_data_channel_failure(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client_with(
        client_control,
        FailingDataTlsConnector::new(),
        factory,
        PrinterModel::P1S,
    )
    .await;

    let result = client.list_directory("/model", REF_NOW).await;
    assert!(
        matches!(result, Err(bambino::error::Error::Network(_))),
        "Expected the data-channel TLS connect failure to surface as Network, got {:?}",
        result
    );

    // The control channel now has an unread 150 reply pending with no matching final reply ever
    // coming — the client must be poisoned, not left in a state where the next command could
    // hang waiting for that reply or misread it as its own response.
    let next_result = client.get_available_space().await;
    assert!(
        matches!(
            next_result,
            Err(bambino::error::Error::ProtocolViolation(_))
        ),
        "Expected the poisoned client to reject the next command with ProtocolViolation, got {:?}",
        next_result
    );

    server_handle.await.expect("Mock server panicked");
}

/// Regression: a `read_response` failure on a single-reply command (`delete_file` here)
/// must poison the client the same way the data-transfer methods already do — previously these
/// six single-reply commands (`get_file_size`/`delete_file`/`create_directory`/
/// `remove_directory`/`rename_file`/`get_available_space`) plus `negotiate_passive_port` left the
/// client silently desynced on a transport error, with no poisoning.
#[tokio::test]
async fn test_ftps_single_reply_command_failure_poisons_client() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_dele_connection_drop(
        server_control,
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.delete_file("/model/job.3mf").await;
    assert!(
        matches!(result, Err(Error::Network(_))),
        "Expected the dropped connection to surface as Network, got {:?}",
        result
    );

    let next_result = client.get_available_space().await;
    assert!(
        matches!(next_result, Err(Error::ProtocolViolation(_))),
        "Expected the poisoned client to reject the next command with ProtocolViolation, got {:?}",
        next_result
    );

    server_handle.await.expect("Mock server panicked");
}

/// A connection lost during `delete_file`'s `550` re-check listing surfaces as that transport
/// error, not as the `550` — which would read as a refusal on a still-usable session (#392).
#[tokio::test]
async fn test_ftps_delete_550_recheck_failure_poisons_client() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_dele_550_listing_drop(
        server_control,
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.delete_file("/model/job.3mf").await;
    assert!(
        matches!(result, Err(Error::Network(_))),
        "Expected the dropped re-check listing to surface as Network, got {result:?}"
    );
    assert!(client.is_poisoned());

    server_handle.await.expect("Mock server panicked");
}

/// Regression: a transport failure between `rename_file`'s `RNFR` and `RNTO` steps must
/// poison the client the same way the single-reply commands' poisoning test already covers —
/// previously only `delete_file` (a one-shot command) had a dedicated poisoning test; the
/// two-step rename sequence had no coverage for a failure landing mid-sequence.
#[tokio::test]
async fn test_ftps_rename_file_mid_sequence_failure_poisons_client() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_rnto_connection_drop(
        server_control,
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.rename_file("/model/old.3mf", "/model/new.3mf").await;
    assert!(
        matches!(result, Err(Error::Network(_))),
        "Expected the dropped connection to surface as Network, got {:?}",
        result
    );

    let next_result = client.get_available_space().await;
    assert!(
        matches!(next_result, Err(Error::ProtocolViolation(_))),
        "Expected the poisoned client to reject the next command with ProtocolViolation, got {:?}",
        next_result
    );

    server_handle.await.expect("Mock server panicked");
}

/// Regression: `list_directory`'s transfer-confirmation read must tolerate `426` the same
/// way `upload_file`/`download_file` already do — both of those have a dedicated
/// 426-recovery test; LIST did not, so a regression reverting LIST's 426 tolerance would pass
/// `cargo test` cleanly.
#[tokio::test]
async fn test_ftps_list_directory_426_recovery() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_list_426_recovery(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let list = client
        .list_directory("/model", REF_NOW)
        .await
        .expect("LIST should tolerate 426 confirmation");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "job.3mf");

    server_handle.await.expect("Mock server panicked");
}

/// Counterpart to the test above: the 426 tolerance is bounded by a completeness check, because
/// LIST has no `SIZE` recheck backing it the way upload/download do. A listing cut mid-line must
/// surface an error instead of silently handing the caller a short file list —
/// `parse_unix_listing` discards the truncated tail as just another malformed line.
#[tokio::test]
async fn test_ftps_list_directory_426_with_truncated_entry_rejected() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_list_426_truncated(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.list_directory("/model", REF_NOW).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "a 426-aborted LIST cut mid-entry must error, not return a silently short listing, got {:?}",
        result.map(|l| l.len())
    );

    server_handle.await.expect("Mock server panicked");
}

/// A 426 with **zero** data bytes is not the close race — vsFTPd reports a genuinely empty
/// directory as `226` + zero bytes, so this must error rather than read as "empty directory".
#[tokio::test]
async fn test_ftps_list_directory_426_with_empty_payload_rejected() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_list_426_empty(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.list_directory("/model", REF_NOW).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "a 426-aborted LIST with no data must error, not return an empty listing, got {:?}",
        result.map(|l| l.len())
    );

    server_handle.await.expect("Mock server panicked");
}

/// One non-UTF-8 filename must skip just that entry, not fail the whole directory — the same
/// per-line leniency as the parser's drop-malformed-lines contract.
#[tokio::test]
async fn test_ftps_list_directory_skips_non_utf8_line() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_list_non_utf8_line(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let list = client
        .list_directory("/model", REF_NOW)
        .await
        .expect("LIST with one non-UTF-8 line should still succeed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "job.3mf");

    server_handle.await.expect("Mock server panicked");
}

/// Runs `list_directory_each` against one mock server, returning what the sink got and the result.
async fn list_each_with<F>(
    server: impl FnOnce(tokio::io::DuplexStream, DataContainer) -> F,
) -> (Vec<bambino::ftps::FtpFile>, Result<(), Error>)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let (client_control, server_control, data_container, factory) = setup();
    let server_handle = tokio::spawn(server(server_control, data_container.clone()));
    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;
    let mut entries = Vec::new();
    let result = client
        .list_directory_each("/model", REF_NOW, |entry| entries.push(entry))
        .await;
    server_handle.await.expect("Mock server panicked");
    (entries, result)
}

/// The sink sees exactly what `list_directory` returns, including the per-line non-UTF-8 skip.
#[tokio::test]
async fn test_ftps_list_directory_each_matches_list_directory() {
    let (entries, result) = list_each_with(mock_ftps::run_mock_server_list_non_utf8_line).await;
    result.expect("LIST with one non-UTF-8 line should still succeed");
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["job.3mf"]);
    assert_eq!(entries[0].size, 102400);

    let (entries, result) = list_each_with(mock_ftps::run_mock_server_list_426_recovery).await;
    result.expect("LIST should tolerate 426 confirmation");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "job.3mf");
}

/// A 426 cut mid-line still errors; the entry before the cut was already delivered, which is
/// why the doc tells the caller to discard everything on error.
#[tokio::test]
async fn test_ftps_list_directory_each_426_truncated_errors_after_delivering() {
    let (entries, result) = list_each_with(mock_ftps::run_mock_server_list_426_truncated).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "got {result:?}"
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "job.3mf");

    let (entries, result) = list_each_with(mock_ftps::run_mock_server_list_426_empty).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "got {result:?}"
    );
    assert!(entries.is_empty());
}

#[tokio::test]
async fn test_ftps_upload_426_recovery_via_size() {
    let (client_control, server_control, data_container, factory) = setup();

    const PAYLOAD: &[u8] = b"TEST_DATA";
    let server_handle = tokio::spawn(mock_ftps::run_mock_server_upload_426_recovery(
        server_control,
        data_container.clone(),
        PAYLOAD.len(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    client
        .upload_file("/model/job.3mf", PAYLOAD)
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

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.upload_file("/model/job.3mf", b"TEST_DATA").await;
    assert!(
        matches!(result, Err(bambino::error::Error::DiskWriteFailure)),
        "Expected DiskWriteFailure on SIZE mismatch, got {:?}",
        result
    );

    server_handle.await.expect("Mock server panicked");
}

/// A data-channel write or flush failure during `upload_file` keeps its error kind: running out
/// of memory reports `ResourceExhausted`, not a dropped link (issue #389). Either failure also
/// poisons the client, since the server's final `STOR` reply is still pending.
#[tokio::test]
async fn test_ftps_upload_data_failure_keeps_its_error_kind() {
    use bambino::io::SocketError;
    use embedded_io_async::ErrorKind;

    let cases = [
        (
            Some(ErrorKind::OutOfMemory),
            None,
            SocketError::ResourceExhausted,
        ),
        (
            None,
            Some(ErrorKind::OutOfMemory),
            SocketError::ResourceExhausted,
        ),
        (
            Some(ErrorKind::ConnectionReset),
            None,
            SocketError::ConnectionReset,
        ),
    ];
    for (write_error, flush_error, expected) in cases {
        let (client_control, server_control, data_container, factory) = setup();
        let server_handle = tokio::spawn(mock_ftps::run_mock_server_upload_data_failure(
            server_control,
            data_container.clone(),
        ));

        let mut client = connect_client_with(
            client_control,
            FaultyDataTlsConnector::new(write_error, flush_error),
            factory,
            PrinterModel::P1S,
        )
        .await;

        let result = client.upload_file("/model/job.3mf", b"TEST_DATA").await;
        assert!(
            matches!(&result, Err(Error::Network(e)) if *e == expected),
            "write {write_error:?}, flush {flush_error:?}: {result:?}"
        );

        let next_result = client.get_available_space().await;
        assert!(
            matches!(next_result, Err(Error::ProtocolViolation(_))),
            "Expected the poisoned client to reject the next command, got {next_result:?}"
        );

        server_handle.await.expect("Mock server panicked");
    }
}

/// Regression: a `write_command`/`read_response` failure on `LIST`'s *initial*
/// negotiation (before the `150`/`226` data-transfer window's poisoning already
/// covered) must poison the client too — previously only failures after the initial exchange
/// did, leaving the control channel silently desynced on this specific failure window.
#[tokio::test]
async fn test_ftps_list_initial_negotiation_failure_poisons_client() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_list_connection_drop(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.list_directory("/model", REF_NOW).await;
    assert!(
        matches!(result, Err(Error::Network(_))),
        "Expected the dropped connection to surface as Network, got {:?}",
        result
    );

    let next_result = client.get_available_space().await;
    assert!(
        matches!(next_result, Err(Error::ProtocolViolation(_))),
        "Expected the poisoned client to reject the next command with ProtocolViolation, got {:?}",
        next_result
    );

    server_handle.await.expect("Mock server panicked");
}

/// Issue #261: a transport failure during the `PASV` exchange itself must poison the client.
///
/// Every other poisoning test issues its failing command *after* `PASV` has already succeeded,
/// so `negotiate_passive_port`'s own `write_command_poisoning`/`read_response_poisoning` calls
/// were only ever exercised on the happy path.
#[tokio::test]
async fn test_ftps_pasv_transport_failure_poisons_client() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_pasv_connection_drop(
        server_control,
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.list_directory("/model", REF_NOW).await;
    assert!(
        matches!(result, Err(Error::Network(_))),
        "Expected the connection dropped during PASV to surface as Network, got {:?}",
        result
    );

    let next_result = client.get_available_space().await;
    assert!(
        matches!(next_result, Err(Error::ProtocolViolation(_))),
        "Expected the poisoned client to reject the next command with ProtocolViolation, got {:?}",
        next_result
    );

    server_handle.await.expect("Mock server panicked");
}

/// Issue #261, the complement: a non-`227` reply to `PASV` is a clean protocol rejection, not a
/// transport failure — the control channel is still in sync, so the client must surface the
/// error *without* poisoning itself and must keep serving subsequent commands.
#[tokio::test]
async fn test_ftps_pasv_rejection_reply_does_not_poison_client() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_pasv_rejected(server_control));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let result = client.list_directory("/model", REF_NOW).await;
    assert!(
        matches!(
            result,
            Err(Error::FtpReply {
                command: "PASV",
                ..
            })
        ),
        "Expected a non-227 PASV reply to surface as FtpReply, got {:?}",
        result
    );

    let next_result = client.get_available_space().await;
    assert!(
        next_result.is_ok(),
        "A rejected PASV must leave the control channel usable, got {:?}",
        next_result
    );

    server_handle.await.expect("Mock server panicked");
}

/// Regression: `download_file`'s confirmation-read handling must accept `426` (the
/// documented P2S/X2D TLS 1.3 close race) and fall through to the SIZE recheck, symmetric with
/// `upload_file`'s existing 426 handling — previously RETR treated 426 as an unconditional hard
/// failure, discarding an already-fully-received payload.
#[tokio::test]
async fn test_ftps_download_426_recovery_via_size() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_download_426_recovery(
        server_control,
        data_container.clone(),
    ));

    let mut client = connect_client(client_control, factory, PrinterModel::P1S).await;

    let data = client
        .download_file("/model/job.3mf")
        .await
        .expect("Download should succeed via 426 + SIZE verification recovery");
    assert_eq!(data, b"TEST_DATA");

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_disconnect() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(server_control));

    let client = connect_client(client_control, factory, PrinterModel::P1S).await;

    client.disconnect().await;

    server_handle.await.expect("Mock server panicked");
}

/// Regression test for GitHub issue #293: every FTPS TLS session must be shut down through
/// [`TlsConnector::close`], not merely dropped.
///
/// Two sessions are closed here, and both matter for a different reason: the per-transfer data
/// channel (whose memory already comes back when the transfer ends, but whose peer would
/// otherwise see a truncated stream every time) and the control channel (which `disconnect()`
/// also drops, to return MbedTLS's ~48 KB of record buffers on embassy rather than holding them
/// until the client itself goes out of scope). The no-op default on `TlsConnector::close` means
/// a teardown path that skips it is silent, so this counts the calls rather than trusting them.
#[tokio::test]
async fn test_ftps_closes_tls_sessions_on_teardown() {
    let (client_control, server_control, data_container, factory) = setup();
    let server_handle = tokio::spawn(mock_ftps::run_mock_server(
        server_control,
        data_container.clone(),
    ));

    let (connector, closes) = CloseCountingTlsConnector::new();
    let mut client =
        connect_client_with(client_control, connector, factory, PrinterModel::P1S).await;

    client
        .list_directory("/model", REF_NOW)
        .await
        .expect("list_directory should succeed");
    assert_eq!(
        closes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a completed transfer must close its data channel's TLS session"
    );

    client.disconnect().await;
    assert_eq!(
        closes.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "disconnect() must close the control channel's TLS session"
    );

    server_handle.abort();
}

/// An early-stopped `download_prefix` still closes the data channel's TLS session before the RST
/// (`.claude/rules/tls-session-teardown.md`), and a disconnect of the spent client then closes
/// nothing more: the printer has already closed the control connection.
#[tokio::test]
async fn test_ftps_download_prefix_closes_the_data_session_before_aborting() {
    let (client_control, server_control, data_container, factory) = setup();
    let server_handle = tokio::spawn(mock_ftps::run_mock_server_download_prefix(
        server_control,
        data_container.clone(),
        true,
    ));

    let (connector, closes) = CloseCountingTlsConnector::new();
    let mut client =
        connect_client_with(client_control, connector, factory, PrinterModel::P1S).await;

    let mut buf = [0u8; 8];
    client
        .download_prefix("/model/job.3mf", &mut buf)
        .await
        .expect("prefix read failed");
    assert_eq!(
        closes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "an early stop must close the data channel's TLS session"
    );

    client.disconnect().await;
    assert_eq!(
        closes.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a spent client's disconnect must not write to the closed control connection"
    );

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_tls13_rejected_for_p2s() {
    let (client_control, _server_control, _, factory) = setup();

    let result = try_connect_with(
        client_control,
        VersionReportingTlsConnector(Some(TlsVersion::Tls13)),
        factory,
        PrinterModel::P2S,
        TlsVersionCheck::Enforce,
    )
    .await;

    match result {
        Err(bambino::error::Error::ProtocolViolation(_)) => {}
        Err(e) => panic!("Expected ProtocolViolation for TLS 1.3 on P2S, got {:?}", e),
        Ok(_) => panic!("Expected error for TLS 1.3 on P2S, but connect succeeded"),
    }
}

#[tokio::test]
async fn test_ftps_tls13_rejected_for_x2d() {
    let (client_control, _server_control, _, factory) = setup();

    let result = try_connect_with(
        client_control,
        VersionReportingTlsConnector(Some(TlsVersion::Tls13)),
        factory,
        PrinterModel::X2D,
        TlsVersionCheck::Enforce,
    )
    .await;

    match result {
        Err(bambino::error::Error::ProtocolViolation(_)) => {}
        Err(e) => panic!("Expected ProtocolViolation for TLS 1.3 on X2D, got {:?}", e),
        Ok(_) => panic!("Expected error for TLS 1.3 on X2D, but connect succeeded"),
    }
}

#[tokio::test]
async fn test_ftps_tls12_accepted_for_p2s() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(server_control));

    let client = try_connect_with(
        client_control,
        VersionReportingTlsConnector(Some(TlsVersion::Tls12)),
        factory,
        PrinterModel::P2S,
        TlsVersionCheck::Enforce,
    )
    .await
    .expect("TLS 1.2 should be accepted for P2S");

    client.disconnect().await;
    server_handle.await.expect("Mock server panicked");
}

/// Regression for issue #58: `open_data_channel`'s "defense in depth" recheck
/// (its call to `require_tls_1_2_if_enforced`) must actually reject a data channel that
/// renegotiated down from TLS 1.2 to TLS 1.3, independent of the control-channel check
/// `FtpsClient::connect` already performed — previously every TLS-1.2-enforcement test only
/// exercised `connect()`, never a real `list_directory`/`upload_file`/`download_file` call, so
/// this embedded recheck had zero coverage.
#[tokio::test]
async fn test_ftps_data_channel_tls12_recheck_rejects_tls13_for_p2s() {
    let (client_control, server_control, data_container, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_data_channel_failure(
        server_control,
        data_container.clone(),
    ));

    let mut client = try_connect_with(
        client_control,
        PerCallVersionReportingTlsConnector::new(Some(TlsVersion::Tls12), Some(TlsVersion::Tls13)),
        factory,
        PrinterModel::P2S,
        TlsVersionCheck::Enforce,
    )
    .await
    .expect("Control channel at TLS 1.2 should be accepted for P2S");

    let result = client.list_directory("/model", REF_NOW).await;
    assert!(
        matches!(result, Err(bambino::error::Error::ProtocolViolation(_))),
        "Expected the data-channel TLS 1.3 recheck to reject with ProtocolViolation, got {:?}",
        result
    );

    let next_result = client.get_available_space().await;
    assert!(
        matches!(
            next_result,
            Err(bambino::error::Error::ProtocolViolation(_))
        ),
        "Expected the client to be poisoned after the data-channel recheck failure, got {:?}",
        next_result
    );

    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_tls13_accepted_for_p1s() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(server_control));

    let client = try_connect_with(
        client_control,
        VersionReportingTlsConnector(Some(TlsVersion::Tls13)),
        factory,
        PrinterModel::P1S,
        TlsVersionCheck::Enforce,
    )
    .await
    .expect("TLS 1.3 should be accepted for P1S");

    client.disconnect().await;
    server_handle.await.expect("Mock server panicked");
}

#[tokio::test]
async fn test_ftps_version_none_rejected_for_p2s() {
    let (client_control, _server_control, _, factory) = setup();

    let result = try_connect_with(
        client_control,
        VersionReportingTlsConnector(None),
        factory,
        PrinterModel::P2S,
        TlsVersionCheck::Enforce,
    )
    .await;

    match result {
        Err(bambino::error::Error::ProtocolViolation(_)) => {}
        Err(e) => panic!(
            "Expected ProtocolViolation for undetermined TLS version on P2S, got {:?}",
            e
        ),
        Ok(_) => {
            panic!("Expected error for undetermined TLS version on P2S, but connect succeeded")
        }
    }
}

/// Regression test for `src/ftps/CLAUDE.md`'s TLS-1.2-enforcement opt-out:
/// `allow_unverified_tls_1_2 == true` must bypass `require_tls_1_2_if_enforced`'s rejection
/// even though P2S enforces TLS 1.2 and the mock connector reports TLS 1.3 was negotiated —
/// mirrors `test_ftps_tls13_rejected_for_p2s` but with the bypass flag set, so `connect()`
/// must now succeed instead of erroring.
#[tokio::test]
async fn test_ftps_tls13_bypassed_for_p2s_when_allow_unverified() {
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(server_control));

    let client = try_connect_with(
        client_control,
        VersionReportingTlsConnector(Some(TlsVersion::Tls13)),
        factory,
        PrinterModel::P2S,
        TlsVersionCheck::Bypass,
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
    let (client_control, server_control, _, factory) = setup();

    let server_handle = tokio::spawn(mock_ftps::run_mock_server_disconnect(server_control));

    let client = try_connect_with(
        client_control,
        VersionReportingTlsConnector(None),
        factory,
        PrinterModel::P2S,
        TlsVersionCheck::Bypass,
    )
    .await
    .expect("allow_unverified_tls_1_2 should bypass the undetermined-version rejection for P2S");

    client.disconnect().await;
    server_handle.await.expect("Mock server panicked");
}
