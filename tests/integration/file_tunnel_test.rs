//! # Port-6000 File Tunnel Through `PrinterClient`
//!
//! Exercises `PrinterClient::file_tunnel()` against a scripted tunnel server on an in-memory
//! duplex: the capability gate, lazy connect through `.with_file_tunnel()`, and teardown. The
//! frame-level protocol is unit-tested in `src/file_tunnel/`.

use std::sync::atomic::Ordering;

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

use bambino::client::{DummyFactory, DummyTls, PrinterClient};
use bambino::error::Error;
use bambino::models::PrinterModel;

use crate::common::client::{ACCESS_CODE, spawn_broker, test_identity};
use crate::common::io::{CloseCountingTlsConnector, DummyTlsConnector, MockDataStreamFactory};

const MAGIC_LOGIN: u32 = 0x0101_013F;
const MAGIC_LOGIN_ACK: u32 = 0x0001_013F;
const MAGIC_CTRL: u32 = 0x0102_013F;
const MAGIC_CTRL_REPLY: u32 = 0x0002_013F;

async fn read_frame(server: &mut DuplexStream) -> (u32, Vec<u8>) {
    let mut header = [0u8; 16];
    server.read_exact(&mut header).await.unwrap();
    let len = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
    let magic = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let mut payload = vec![0u8; len];
    server.read_exact(&mut payload).await.unwrap();
    (magic, payload)
}

async fn write_frame(server: &mut DuplexStream, magic: u32, payload: &[u8]) {
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[4..8].copy_from_slice(&magic.to_le_bytes());
    server.write_all(&header).await.unwrap();
    server.write_all(payload).await.unwrap();
}

/// Accepts the login and setup, answers one `REQUEST_MEDIA_ABILITY`, and returns what the client sent.
async fn run_mock_tunnel(mut server: DuplexStream) -> (Vec<u8>, String) {
    let (magic, login) = read_frame(&mut server).await;
    assert_eq!(magic, MAGIC_LOGIN);
    write_frame(&mut server, MAGIC_LOGIN_ACK, &[0, 0, 0, 0]).await;

    let (magic, setup) = read_frame(&mut server).await;
    assert_eq!(magic, MAGIC_CTRL);
    assert!(
        String::from_utf8(setup)
            .unwrap()
            .contains(r#""mtype":12291"#)
    );
    write_frame(
        &mut server,
        MAGIC_CTRL_REPLY,
        br#"{"mtype":12291,"sequence":0,"result":0,"reply":{}}"#,
    )
    .await;

    let (magic, request) = read_frame(&mut server).await;
    assert_eq!(magic, MAGIC_CTRL);
    write_frame(
        &mut server,
        MAGIC_CTRL_REPLY,
        br#"{"cmdtype":7,"mtype":12289,"reply":{"storage":["emmc","udisk"]},"result":0,"sequence":1}"#,
    )
    .await;
    (login, String::from_utf8(request).unwrap())
}

/// A chamber-image printer's port 6000 is the camera server, which holds a tunnel login for 60 s.
/// The gate must refuse before dialing: the empty factory would turn a dial into `Network`.
#[tokio::test]
async fn chamber_image_model_is_refused_without_dialing() {
    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P1S))
        .with_file_tunnel(DummyTlsConnector, MockDataStreamFactory::empty());
    let result = printer.file_tunnel().await.map(|_| ());
    assert!(
        matches!(result, Err(Error::ModelMismatch(_))),
        "expected ModelMismatch, got {result:?}"
    );
    assert!(!printer.is_file_tunnel_connected());
}

#[tokio::test]
async fn unconfigured_tunnel_reports_not_configured() {
    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P2S));
    let result = printer.file_tunnel().await.map(|_| ());
    assert!(
        matches!(result, Err(Error::NotConfigured(_))),
        "expected NotConfigured, got {result:?}"
    );
}

#[tokio::test]
async fn tunnel_connects_lazily_answers_and_closes_on_disconnect() {
    let (stream, server) = spawn_broker(run_mock_tunnel);
    let (factory, _) = MockDataStreamFactory::with_stream(stream);
    let (tls, closes) = CloseCountingTlsConnector::new();
    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P2S))
        .with_file_tunnel(tls, factory);

    assert!(!printer.is_file_tunnel_connected());
    let storages = printer
        .file_tunnel()
        .await
        .expect("file_tunnel should dial, log in and set up")
        .media_ability()
        .await
        .expect("media_ability should succeed");
    assert_eq!(storages, ["emmc", "udisk"]);
    assert!(printer.is_file_tunnel_connected());

    let (login, request) = server.await.expect("mock tunnel server panicked");
    assert_eq!(&login[..8], b"bblp\0\0\0\0");
    assert_eq!(&login[8..], ACCESS_CODE.as_bytes());
    assert_eq!(
        request,
        r#"{"mtype":12289,"cmdtype":7,"sequence":1,"req":{"peer":"studio","api_version":2}}"#
    );

    printer.disconnect_file_tunnel().await;
    assert_eq!(closes.load(Ordering::SeqCst), 1);
    assert!(!printer.is_file_tunnel_connected());
}

/// A failed dial must leave the port-6000 config in place, so the next call retries the dial
/// rather than reporting "not configured".
#[tokio::test]
async fn failed_dial_keeps_the_config_for_a_retry() {
    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P2S))
        .with_file_tunnel(DummyTlsConnector, MockDataStreamFactory::empty());
    for attempt in 1..=2 {
        let result = printer.file_tunnel().await.map(|_| ());
        assert!(
            matches!(result, Err(Error::Network(_))),
            "attempt {attempt}: expected the dial failure, got {result:?}"
        );
    }
}
