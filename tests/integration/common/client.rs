//! # Shared `PrinterClient` Connection Helper
//!
//! Collapses the `MqttClient::connect()` + `PrinterClient::from_mqtt()` pair repeated at
//! ~80 call sites across `tests/client_test.rs` into one call. Deliberately does
//! NOT also swallow the preceding `tokio::io::duplex()` + broker-task `tokio::spawn()` —
//! `MqttClient::connect()` performs a real handshake that blocks until the server end of the
//! stream is driven, so the broker task must already be running before this is awaited. Moving
//! that spawn behind this helper would silently reorder it after the connect call and deadlock.

use bambino::client::{
    DummyFactory, DummyRawIo, DummyTimer, DummyTls, PreConnected, PrinterClient,
};
use bambino::identity::PrinterIdentity;
use bambino::io::AsyncIo;
use bambino::models::PrinterModel;
use bambino::mqtt::MqttClient;

/// Serial shared by the `client_*_test.rs` suites that don't need a distinct one.
pub const SERIAL: &str = "01P000000000000";

/// `PrinterClient` type produced by [`connect_test_client`].
pub type TestClient<IO> = PrinterClient<
    IO,
    PreConnected<IO>,
    PreConnected<IO>,
    DummyTimer,
    DummyRawIo,
    DummyTls,
    DummyFactory,
    DummyTimer,
    DummyRawIo,
    DummyTls,
    DummyFactory,
>;

/// Completes the MQTT connect handshake over `stream` and wraps the result in a
/// `PrinterClient`. Caller must have already spawned whatever's driving the other end of
/// `stream` (see module doc comment) before awaiting this.
pub async fn connect_test_client<IO: AsyncIo>(
    stream: IO,
    serial: &str,
    model: PrinterModel,
) -> TestClient<IO> {
    let mqtt_client = MqttClient::connect(
        stream,
        &PrinterIdentity {
            ip: String::new(),
            serial: serial.to_string(),
            access_code: "12345678".to_string(),
            model,
        },
    )
    .await
    .expect("MQTT connect handshake failed");
    PrinterClient::from_mqtt(mqtt_client, model)
}
