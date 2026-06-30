//! # Printer Client
//!
//! This is the main entry point for most users. [`PrinterClient`] wraps an MQTT session
//! (and optionally an FTPS connection) into a single coordinated interface with methods
//! for thermal control, motion, print management, AMS operations, and hardware queries.
//!
//! The client applies model-aware safety checks automatically:
//!
//! - **Homing safety** — On CoreXY (bed-on-Z) printers, partial homing commands like
//!   `G28 Z` can crash the nozzle into the plate. The client enforces bare `G28` only.
//! - **Z-axis travel limits** — Relative Z moves are clamped to the model's mechanical
//!   bounds and wrapped in reference-mode push/pop (`M1002`) to prevent bed crashes.
//! - **Chamber heater guards** — `set_chamber_temperature()` rejects requests on models
//!   without an active PTC heater (open-frame machines like A1/P1).
//! - **Fan routing** — Fan commands are directed to the correct controller, including
//!   the secondary right-side auxiliary fan on models that have one (P2S, X2D, etc.).

mod ams;
pub mod dummy;
mod hardware;
mod motion;
mod print;
mod storage;
mod thermal;
pub mod types;

pub use dummy::{DummyFactory, DummyRawIo, DummySecureConnect, DummyTimer, DummyTls, PreConnected};
#[doc(inline)]
pub use types::{CalibrationOption, FanTarget, PrintSpeed, PrintStatus, TelemetryEvent};

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::Serialize;

use core::marker::PhantomData;

use crate::error::BambuError;
use crate::ftps::{BambuFtpsClient, FtpDataStreamFactory};
use crate::io::{AsyncIo, SecureConnect, TimerProvider, TlsConnector};
use crate::models::BambuModel;
use crate::mqtt::commands::TASK_ID_MAX;
use crate::mqtt::{BambuMqttClient, MqttMessage};
use crate::types::TelemetryReport;

pub(crate) const INITIAL_SEQUENCE_ID: u64 = 10000;
pub(crate) const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 10;
pub(crate) const POLL_UNTIL_MAX_MESSAGES: usize = 200;

/// High-level client for controlling a Bambu Lab printer.
///
/// Wraps an MQTT session (connected or lazy) and optionally a [`BambuFtpsClient`] for
/// SD card access. The first type parameter is a [`SecureConnect`] connector that
/// determines the MQTT stream type. Use [`PreConnected`] when wrapping an already-connected
/// [`BambuMqttClient`], or a platform connector (e.g. `TokioSecureConnector`) for lazy
/// connection.
pub struct PrinterClient<
    Conn,
    Timer = DummyTimer,
    RawIO = DummyRawIo,
    Tls = DummyTls,
    Factory = DummyFactory,
> where
    Conn: SecureConnect,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    pub(crate) mqtt: Option<BambuMqttClient<Conn::Stream>>,
    pub(crate) ftps: Option<BambuFtpsClient<RawIO, Tls, Factory>>,
    pub(crate) ftps_config: Option<(Tls, Factory)>,
    pub(crate) connector: Conn,
    pub(crate) timer: Timer,
    pub(crate) serial: String,
    pub(crate) ip: String,
    pub(crate) access_code: String,
    pub(crate) model: BambuModel,
    pub(crate) sequence_counter: u64,
    pub(crate) k_profile_primed: bool,
    pub(crate) last_home_flag: Option<u32>,
    pub(crate) last_gcode_state: Option<String>,
    pub(crate) command_timeout_secs: u64,
    pub(crate) mqtt_port: u16,
    pub(crate) ftps_port: u16,
}

impl<Conn> PrinterClient<Conn, DummyTimer, DummyRawIo, DummyTls, DummyFactory>
where
    Conn: SecureConnect,
{
    /// Creates a lazy client that defers MQTT connection until first use.
    ///
    /// The MQTT session is established automatically on the first method call that
    /// requires it (e.g. [`poll_telemetry()`](Self::poll_telemetry),
    /// [`request_pushall()`](Self::request_pushall)), or eagerly via
    /// [`connect_mqtt()`](Self::connect_mqtt).
    ///
    /// Without a [`TimerProvider`], command-response methods like
    /// [`get_version()`](Self::get_version) rely on a message-count safety valve
    /// instead of wall-clock timeouts. Chain [`.with_timer()`](Self::with_timer)
    /// for real timeouts.
    pub fn new(
        connector: Conn,
        ip: &str,
        serial: &str,
        access_code: &str,
        model: BambuModel,
    ) -> Self {
        Self {
            mqtt: None,
            ftps: None,
            ftps_config: None,
            connector,
            timer: DummyTimer,
            serial: String::from(serial),
            ip: String::from(ip),
            access_code: String::from(access_code),
            model,
            sequence_counter: INITIAL_SEQUENCE_ID,
            k_profile_primed: false,
            last_home_flag: None,
            last_gcode_state: None,
            command_timeout_secs: DEFAULT_COMMAND_TIMEOUT_SECS,
            mqtt_port: crate::mqtt::MQTTS_PORT,
            ftps_port: crate::ftps::FTPS_PORT,
        }
    }
}

impl<IO> PrinterClient<PreConnected<IO>, DummyTimer, DummyRawIo, DummyTls, DummyFactory>
where
    IO: AsyncIo,
{
    /// Wraps an already-connected [`BambuMqttClient`] in a `PrinterClient`.
    ///
    /// Use this when you have a pre-established MQTT session (tests, Embassy,
    /// or any context where the caller manages the connection). The resulting
    /// client uses [`PreConnected`] as its connector — calling
    /// [`connect_mqtt()`](Self::connect_mqtt) on a disconnected `PreConnected`
    /// client will return [`SocketError::NotConnected`](crate::io::SocketError::NotConnected).
    pub fn from_mqtt(mqtt_client: BambuMqttClient<IO>, serial: &str, model: BambuModel) -> Self {
        Self {
            mqtt: Some(mqtt_client),
            ftps: None,
            ftps_config: None,
            connector: PreConnected(PhantomData),
            timer: DummyTimer,
            serial: String::from(serial),
            ip: String::new(),
            access_code: String::new(),
            model,
            sequence_counter: INITIAL_SEQUENCE_ID,
            k_profile_primed: false,
            last_home_flag: None,
            last_gcode_state: None,
            command_timeout_secs: DEFAULT_COMMAND_TIMEOUT_SECS,
            mqtt_port: crate::mqtt::MQTTS_PORT,
            ftps_port: crate::ftps::FTPS_PORT,
        }
    }
}

impl<Conn, Timer, RawIO, Tls, Factory> PrinterClient<Conn, Timer, RawIO, Tls, Factory>
where
    Conn: SecureConnect,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    /// Establishes the MQTT connection if not already connected.
    ///
    /// Short-circuits when `self.mqtt` is already `Some`. Otherwise, calls
    /// `self.connector.secure_connect()` followed by `BambuMqttClient::connect()`
    /// to create the session lazily.
    async fn ensure_mqtt(&mut self) -> Result<(), BambuError> {
        if self.mqtt.is_some() {
            return Ok(());
        }
        let stream = self
            .connector
            .secure_connect(&self.ip, self.mqtt_port)
            .await?;
        let mqtt_client = BambuMqttClient::connect(stream, &self.serial, &self.access_code).await?;
        self.mqtt = Some(mqtt_client);
        // Reseed from wall-clock time so two independent sessions connecting to the
        // same printer don't start from the same fixed counter and risk colliding
        // sequence IDs while both have in-flight requests.
        self.sequence_counter =
            crate::mqtt::commands::clamp_task_id(self.timer.now_millis()) as u64;
        Ok(())
    }

    /// Eagerly establishes the MQTT connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_mqtt(&mut self) -> Result<(), BambuError> {
        self.ensure_mqtt().await
    }

    /// Returns whether the MQTT session is currently established.
    pub fn mqtt_connected(&self) -> bool {
        self.mqtt.is_some()
    }

    /// Sets a [`TimerProvider`] for wall-clock command-response timeouts.
    ///
    /// Consuming builder — works on both [`new()`](Self::new) and
    /// [`from_mqtt()`](PrinterClient::from_mqtt) construction paths.
    pub fn with_timer<NewTimer: TimerProvider>(
        self,
        timer: NewTimer,
    ) -> PrinterClient<Conn, NewTimer, RawIO, Tls, Factory> {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: self.ftps,
            ftps_config: self.ftps_config,
            connector: self.connector,
            timer,
            serial: self.serial,
            ip: self.ip,
            access_code: self.access_code,
            model: self.model,
            sequence_counter: self.sequence_counter,
            k_profile_primed: self.k_profile_primed,
            last_home_flag: self.last_home_flag,
            last_gcode_state: self.last_gcode_state,
            command_timeout_secs: self.command_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
        }
    }

    /// Overrides the default MQTT port (8883).
    pub fn with_mqtt_port(mut self, port: u16) -> Self {
        self.mqtt_port = port;
        self
    }

    /// Configures FTPS for lazy connection on first storage method call.
    ///
    /// Consuming builder — changes the `RawIO`, `Tls`, and `Factory` type parameters.
    /// The FTPS [`TlsConnector`] is independent from MQTT's (some models require
    /// different TLS settings for FTPS, e.g. `force_tls_1_2`).
    pub fn with_ftps<NewRawIO, NewTls, NewFactory>(
        self,
        tls: NewTls,
        factory: NewFactory,
    ) -> PrinterClient<Conn, Timer, NewRawIO, NewTls, NewFactory>
    where
        NewRawIO: AsyncIo,
        NewTls: TlsConnector<NewRawIO>,
        NewFactory: FtpDataStreamFactory<NewRawIO>,
    {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: None,
            ftps_config: Some((tls, factory)),
            connector: self.connector,
            timer: self.timer,
            serial: self.serial,
            ip: self.ip,
            access_code: self.access_code,
            model: self.model,
            sequence_counter: self.sequence_counter,
            k_profile_primed: self.k_profile_primed,
            last_home_flag: self.last_home_flag,
            last_gcode_state: self.last_gcode_state,
            command_timeout_secs: self.command_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
        }
    }

    /// Overrides the default FTPS port (990).
    pub fn with_ftps_port(mut self, port: u16) -> Self {
        self.ftps_port = port;
        self
    }

    /// Establishes the FTPS connection if not already connected.
    ///
    /// Short-circuits when `self.ftps` is already `Some`. Otherwise, takes the
    /// TLS connector and data factory from `ftps_config`, creates a raw TCP
    /// connection, and calls `BambuFtpsClient::connect()`. The config is consumed
    /// on first connection — reconnecting requires a new `PrinterClient`.
    async fn ensure_ftps(&mut self) -> Result<(), BambuError> {
        if self.ftps.is_some() {
            return Ok(());
        }
        let (tls, factory) = self.ftps_config.take().ok_or_else(|| {
            BambuError::ProtocolViolation(
                "FTPS not configured — call .with_ftps() or .attach_storage()".into(),
            )
        })?;
        let raw_stream = factory.create_data_stream(&self.ip, self.ftps_port).await?;
        let ftps_client = BambuFtpsClient::connect(
            raw_stream,
            tls,
            factory,
            self.model,
            &self.ip,
            &self.access_code,
        )
        .await?;
        self.ftps = Some(ftps_client);
        Ok(())
    }

    /// Eagerly establishes the FTPS connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_ftps(&mut self) -> Result<(), BambuError> {
        self.ensure_ftps().await
    }

    /// Returns whether the FTPS session is currently established.
    pub fn ftps_connected(&self) -> bool {
        self.ftps.is_some()
    }

    /// Increments and returns the next transaction/sequence identifier tracking commands.
    ///
    /// Wraps at `TASK_ID_MAX` (32-bit signed integer limit) to stay within firmware
    /// parsing constraints [REF-MQTT-ENV].
    pub fn next_sequence_id(&mut self) -> u64 {
        self.sequence_counter = self.sequence_counter.wrapping_add(1);
        if self.sequence_counter > TASK_ID_MAX {
            self.sequence_counter = INITIAL_SEQUENCE_ID;
        }
        self.sequence_counter
    }

    /// Sets the timeout (in seconds) used by command-response methods like
    /// [`get_version()`](Self::get_version) and [`get_k_profiles()`](Self::get_k_profiles).
    pub fn set_command_timeout(&mut self, secs: u64) {
        self.command_timeout_secs = secs;
    }

    /// Pulls the next telemetry event from the MQTT channel.
    ///
    /// Returns a [`TelemetryEvent::Report`] if the payload deserializes as a known
    /// telemetry structure, or [`TelemetryEvent::Unknown`] otherwise. Drains any
    /// internally buffered messages (from command-response round-trips) before
    /// reading from the wire.
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     match printer.poll_telemetry().await? {
    ///         TelemetryEvent::Report(report, _raw) => {
    ///             if let Some(state) = &report.print.gcode_state {
    ///                 println!("Printer state: {}", state);
    ///             }
    ///         }
    ///         TelemetryEvent::Unknown(_) => {}
    ///     }
    /// }
    /// ```
    pub async fn poll_telemetry(&mut self) -> Result<TelemetryEvent, BambuError> {
        self.ensure_mqtt().await?;
        let msg = self.mqtt.as_mut().unwrap().poll_telemetry().await?;
        match serde_json::from_slice::<TelemetryReport>(&msg.payload) {
            Ok(report) => {
                if let Some(print) = report.print.as_ref() {
                    if let Some(flag) = print.home_flag {
                        self.last_home_flag = Some(flag);
                    }
                    if let Some(state) = &print.gcode_state {
                        self.last_gcode_state = Some(state.clone());
                    }
                }
                Ok(TelemetryEvent::Report(Box::new(report), msg))
            }
            Err(_) => Ok(TelemetryEvent::Unknown(msg)),
        }
    }

    /// Returns the printer's high-level activity classification as of the
    /// last-observed `gcode_state` telemetry (via
    /// [`poll_telemetry()`](Self::poll_telemetry)). `None` means no telemetry
    /// carrying `gcode_state` has been observed yet.
    pub fn print_status(&self) -> Option<PrintStatus> {
        self.last_gcode_state
            .as_deref()
            .map(PrintStatus::from_gcode_state)
    }

    /// Pulls the next raw MQTT message without deserialization.
    pub async fn poll_raw(&mut self) -> Result<MqttMessage, BambuError> {
        self.ensure_mqtt().await?;
        self.mqtt.as_mut().unwrap().poll_telemetry().await
    }

    /// Polls the MQTT stream until `matcher` returns `Some(T)`, buffering non-matching
    /// messages for later retrieval via `poll_telemetry()` / `poll_raw()`.
    ///
    /// Returns `BambuError::Timeout` if the wall-clock timeout (`command_timeout_secs`)
    /// or message-count safety valve (`POLL_UNTIL_MAX_MESSAGES`) is exceeded.
    pub(crate) async fn poll_until<F, T>(&mut self, mut matcher: F) -> Result<T, BambuError>
    where
        F: FnMut(&MqttMessage) -> Option<T>,
    {
        self.ensure_mqtt().await?;
        let start = self.timer.now_millis();
        let timeout_ms = self.command_timeout_secs * 1000;
        let mut count: usize = 0;

        loop {
            let msg = self.mqtt.as_mut().unwrap().poll_wire().await?;
            if let Some(result) = matcher(&msg) {
                return Ok(result);
            }
            self.mqtt.as_mut().unwrap().push_pending(msg);
            count += 1;

            if count >= POLL_UNTIL_MAX_MESSAGES {
                return Err(BambuError::Timeout);
            }
            let elapsed = self.timer.now_millis().wrapping_sub(start);
            if timeout_ms > 0 && elapsed >= timeout_ms {
                return Err(BambuError::Timeout);
            }
        }
    }

    /// Serializes a request struct and publishes it to the printer's MQTT command channel.
    pub(crate) async fn publish_request<T: Serialize>(
        &mut self,
        request: &T,
    ) -> Result<u16, BambuError> {
        self.ensure_mqtt().await?;
        let payload = serde_json::to_vec(request).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.as_mut().unwrap().publish_command(&payload).await
    }

    /// Requests a full state dump from the printer [REF-MQTT-LIFECYCLE].
    pub async fn request_pushall(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::PushAllRequest::new(seq);
        self.publish_request(&req).await
    }

    /// Dispatches a PINGREQ keep-alive frame to maintain connection liveness.
    pub async fn send_ping(&mut self) -> Result<(), BambuError> {
        self.ensure_mqtt().await?;
        self.mqtt.as_mut().unwrap().send_ping().await
    }

    /// Returns a reference to the printer's unique hardware serial number.
    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// Returns the resolved printer hardware model.
    pub fn model(&self) -> BambuModel {
        self.model
    }

    /// Returns direct access to the underlying [`BambuMqttClient`], auto-connecting
    /// if needed.
    ///
    /// Use this for sending custom MQTT payloads, managing zombie detection via
    /// [`tick_zombie_check()`](BambuMqttClient::tick_zombie_check), or inspecting
    /// in-flight state — anything that [`PrinterClient`] doesn't expose directly.
    pub async fn mqtt(&mut self) -> Result<&mut BambuMqttClient<Conn::Stream>, BambuError> {
        self.ensure_mqtt().await?;
        Ok(self.mqtt.as_mut().unwrap())
    }
}
