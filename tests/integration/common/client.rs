//! # Shared `PrinterClient` Connection Helpers
//!
//! [`with_broker`] owns the whole "duplex + broker task + connect" setup most client tests open
//! with, in the one order that works: `MqttClient::connect()` performs a real handshake that
//! blocks until the server end of the stream is driven, so the broker task must be spawned
//! before the connect is awaited. A helper that connected first and spawned second would
//! deadlock; this one spawns first by construction. [`spawn_broker`] is the lower half, for a
//! test that connects through something other than [`connect_test_client`].

use std::future::Future;

use bambino::client::{
    DummyFactory, DummyRawIo, DummyTimer, DummyTls, PreConnected, PrinterClient,
};
use bambino::identity::PrinterIdentity;
use bambino::io::{AsyncIo, TokioIo};
use bambino::models::PrinterModel;
use bambino::mqtt::MqttClient;
use tokio::io::DuplexStream;
use tokio::task::JoinHandle;

/// Serial shared by the `client_*_test.rs` suites that don't need a distinct one (a P1-series prefix).
pub const SERIAL: &str = "01P000000000000";

/// An X1-series serial, for tests whose model behavior keys off the serial prefix.
pub const X1_SERIAL: &str = "00M000000000000";

/// Access code every mock broker and mock server accepts.
pub const ACCESS_CODE: &str = "12345678";

/// Duplex buffer size the client tests use.
const DUPLEX_BYTES: usize = 8192;

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

/// An identity for a printer at `127.0.0.1` with [`SERIAL`] and [`ACCESS_CODE`], as `model`.
pub fn test_identity(model: PrinterModel) -> PrinterIdentity {
    PrinterIdentity::new("127.0.0.1", SERIAL, ACCESS_CODE).with_model(model)
}

/// Creates a duplex pair and spawns `broker` on its server end, returning the client end.
///
/// Connect over the returned stream only after this returns — the broker is already running.
pub fn spawn_broker<F, Fut, T>(broker: F) -> (TokioIo<DuplexStream>, JoinHandle<T>)
where
    F: FnOnce(DuplexStream) -> Fut,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (client_stream, server_stream) = tokio::io::duplex(DUPLEX_BYTES);
    let task = tokio::spawn(broker(server_stream));
    (TokioIo::new(client_stream), task)
}

/// Spawns `broker`, then connects a `PrinterClient` to it — the order the handshake requires.
pub async fn with_broker<F, Fut, T>(
    serial: &str,
    model: PrinterModel,
    broker: F,
) -> (TestClient<TokioIo<DuplexStream>>, JoinHandle<T>)
where
    F: FnOnce(DuplexStream) -> Fut,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let (stream, task) = spawn_broker(broker);
    let client = connect_test_client(stream, serial, model).await;
    (client, task)
}

/// A connected client whose broker does only the handshake — for tests the client refuses
/// before publishing anything.
pub async fn connect_idle_client(
    model: PrinterModel,
) -> (TestClient<TokioIo<DuplexStream>>, JoinHandle<DuplexStream>) {
    with_broker(SERIAL, model, |mut server| async move {
        super::mock_mqtt::handle_mqtt_handshake(&mut server).await;
        server
    })
    .await
}

/// Completes the MQTT connect handshake over `stream` and wraps the result in a
/// `PrinterClient`. Caller must have already spawned whatever's driving the other end of
/// `stream` (see the module doc comment) before awaiting this.
pub async fn connect_test_client<IO: AsyncIo>(
    stream: IO,
    serial: &str,
    model: PrinterModel,
) -> TestClient<IO> {
    PrinterClient::from_mqtt(connect_test_mqtt(stream, serial).await, model)
}

/// Completes the MQTT connect handshake over `stream` and returns the bare [`MqttClient`],
/// for tests that need to hand a *second* session to
/// [`PrinterClient::attach_mqtt()`](bambino::client::PrinterClient::attach_mqtt) rather than
/// build a new client. Same broker-task ordering requirement as [`connect_test_client`].
pub async fn connect_test_mqtt<IO: AsyncIo>(stream: IO, serial: &str) -> MqttClient<IO> {
    MqttClient::connect(stream, serial, ACCESS_CODE)
        .await
        .expect("MQTT connect handshake failed")
}
