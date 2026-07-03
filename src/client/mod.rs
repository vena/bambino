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

pub use dummy::{DummyFactory, DummyRawIo, DummyTimer, DummyTls, PreConnected};
#[doc(inline)]
pub use types::{CalibrationOption, FanTarget, PrintSpeed, PrintStatus, TelemetryEvent};

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::Serialize;

use core::marker::PhantomData;

use core::future::Future;

use crate::error::BambuError;
use crate::ftps::BambuFtpsClient;
use crate::io::{AsyncIo, RawStreamFactory, SocketError, TimerProvider, TlsConnector};
use crate::models::BambuModel;
use crate::mqtt::client::{Raced, race};
use crate::mqtt::commands::TASK_ID_MAX;
use crate::mqtt::{BambuMqttClient, MqttMessage};
use crate::types::TelemetryReport;

pub(crate) const INITIAL_SEQUENCE_ID: u64 = 10000;
pub(crate) const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 10;
pub(crate) const POLL_UNTIL_MAX_MESSAGES: usize = 200;
/// Default upper bound on `ensure_mqtt()`/`ensure_ftps()`'s combined dial+connect sequence
/// (`PLAN.md` Phase 12, decision 6) — matches ESP-IDF's pre-existing `DEFAULT_CONNECT_TIMEOUT`
/// (`src/io/esp_idf.rs`). Override via `.with_connect_timeout(secs)`.
pub(crate) const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Races `fut` against a `connect_timeout_secs`-second deadline on `timer`, used by
/// `ensure_mqtt()`/`ensure_ftps()` to bound their two-step dial+connect sequences. Reuses the
/// `race()` combinator `src/mqtt/client.rs`'s `poll_wire`/`read_exact_packet` per-read deadline
/// is built on, including its `has_real_clock()` guard: under `DummyTimer` (`has_real_clock()
/// == false`), `sleep()` completes instantly regardless of duration, so racing against it
/// unconditionally would make every connect attempt look timed out instead of providing real
/// protection — see `TimerProvider::has_real_clock`'s doc comment.
async fn race_against_connect_timeout<TP, F, T, E>(
    timer: &TP,
    connect_timeout_secs: u64,
    fut: F,
) -> Result<T, E>
where
    TP: TimerProvider,
    F: Future<Output = Result<T, E>>,
    E: From<SocketError>,
{
    if !timer.has_real_clock() {
        return fut.await;
    }
    let sleep_fut = timer.sleep(core::time::Duration::from_secs(connect_timeout_secs));
    match race(fut, sleep_fut).await {
        Raced::Left(result) => result,
        Raced::Right(_) => Err(E::from(SocketError::TimedOut)),
    }
}

/// High-level client for controlling a Bambu Lab printer.
///
/// Wraps an MQTT session (connected or lazy) and optionally a [`BambuFtpsClient`] for
/// SD card access. `MqttRawIO`/`MqttTls`/`MqttFactory` are MQTT's [`TlsConnector`]+
/// [`RawStreamFactory`] pair (mandatory — every `PrinterClient` needs MQTT);
/// `FtpsRawIO`/`FtpsTls`/`FtpsFactory` are FTPS's independent pair (defaulted, configured via
/// [`.with_ftps()`](Self::with_ftps)). Use [`PreConnected`] for both MQTT slots when wrapping
/// an already-connected [`BambuMqttClient`] (see [`from_mqtt()`](Self::from_mqtt)), or a
/// platform's `TlsConnector`+`RawStreamFactory` pair (e.g. `TokioTlsConnector`+
/// `TokioRawStreamFactory`) for lazy connection via [`new()`](Self::new).
pub struct PrinterClient<
    MqttRawIO,
    MqttTls,
    MqttFactory,
    Timer = DummyTimer,
    FtpsRawIO = DummyRawIo,
    FtpsTls = DummyTls,
    FtpsFactory = DummyFactory,
> where
    MqttRawIO: AsyncIo,
    MqttTls: TlsConnector<MqttRawIO>,
    MqttFactory: RawStreamFactory<MqttRawIO>,
    Timer: TimerProvider,
    FtpsRawIO: AsyncIo,
    FtpsTls: TlsConnector<FtpsRawIO>,
    FtpsFactory: RawStreamFactory<FtpsRawIO>,
{
    pub(crate) mqtt: Option<BambuMqttClient<MqttTls::Stream>>,
    pub(crate) ftps: Option<BambuFtpsClient<FtpsRawIO, FtpsTls, FtpsFactory>>,
    pub(crate) ftps_config: Option<(FtpsTls, FtpsFactory)>,
    pub(crate) mqtt_tls: MqttTls,
    pub(crate) mqtt_factory: MqttFactory,
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
    pub(crate) connect_timeout_secs: u64,
    pub(crate) mqtt_port: u16,
    pub(crate) ftps_port: u16,
    pub(crate) _mqtt_raw_io: PhantomData<MqttRawIO>,
}

impl<MqttRawIO, MqttTls, MqttFactory>
    PrinterClient<MqttRawIO, MqttTls, MqttFactory, DummyTimer, DummyRawIo, DummyTls, DummyFactory>
where
    MqttRawIO: AsyncIo,
    MqttTls: TlsConnector<MqttRawIO>,
    MqttFactory: RawStreamFactory<MqttRawIO>,
{
    /// Creates a lazy client that defers MQTT connection until first use.
    ///
    /// The MQTT session is established automatically on the first method call that
    /// requires it (e.g. [`poll_telemetry()`](Self::poll_telemetry),
    /// [`request_pushall()`](Self::request_pushall)), or eagerly via
    /// [`connect_mqtt()`](Self::connect_mqtt). `tls`/`factory` mirror
    /// [`.with_ftps(tls, factory)`](Self::with_ftps)'s call shape — `factory.dial()` opens the
    /// raw TCP socket, then `tls.connect()` wraps it in TLS.
    ///
    /// Without a [`TimerProvider`], command-response methods like
    /// [`get_version()`](Self::get_version) rely on a message-count safety valve
    /// instead of wall-clock timeouts. Chain [`.with_timer()`](Self::with_timer)
    /// for real timeouts.
    pub fn new(
        tls: MqttTls,
        factory: MqttFactory,
        ip: &str,
        serial: &str,
        access_code: &str,
        model: BambuModel,
    ) -> Self {
        Self {
            mqtt: None,
            ftps: None,
            ftps_config: None,
            mqtt_tls: tls,
            mqtt_factory: factory,
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
            connect_timeout_secs: DEFAULT_CONNECT_TIMEOUT_SECS,
            mqtt_port: crate::mqtt::MQTTS_PORT,
            ftps_port: crate::ftps::FTPS_PORT,
            _mqtt_raw_io: PhantomData,
        }
    }
}

impl<IO>
    PrinterClient<
        IO,
        PreConnected<IO>,
        PreConnected<IO>,
        DummyTimer,
        DummyRawIo,
        DummyTls,
        DummyFactory,
    >
where
    IO: AsyncIo,
{
    /// Wraps an already-connected [`BambuMqttClient`] in a `PrinterClient`.
    ///
    /// Use this when you have a pre-established MQTT session (tests, Embassy,
    /// or any context where the caller manages the connection). The resulting client uses
    /// [`PreConnected`] for both the MQTT `Tls` and `Factory` slots — `ensure_mqtt()`
    /// short-circuits on `self.mqtt.is_some()` before either is ever called, so
    /// `PreConnected`'s `RawStreamFactory::dial` (which returns
    /// [`SocketError::NotConnected`](crate::io::SocketError::NotConnected)) is unreachable in
    /// practice.
    pub fn from_mqtt(mqtt_client: BambuMqttClient<IO>, serial: &str, model: BambuModel) -> Self {
        Self {
            mqtt: Some(mqtt_client),
            ftps: None,
            ftps_config: None,
            mqtt_tls: PreConnected(PhantomData),
            mqtt_factory: PreConnected(PhantomData),
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
            connect_timeout_secs: DEFAULT_CONNECT_TIMEOUT_SECS,
            mqtt_port: crate::mqtt::MQTTS_PORT,
            ftps_port: crate::ftps::FTPS_PORT,
            _mqtt_raw_io: PhantomData,
        }
    }
}

impl<MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory>
    PrinterClient<MqttRawIO, MqttTls, MqttFactory, Timer, FtpsRawIO, FtpsTls, FtpsFactory>
where
    MqttRawIO: AsyncIo,
    MqttTls: TlsConnector<MqttRawIO>,
    MqttFactory: RawStreamFactory<MqttRawIO>,
    Timer: TimerProvider,
    FtpsRawIO: AsyncIo,
    FtpsTls: TlsConnector<FtpsRawIO>,
    FtpsFactory: RawStreamFactory<FtpsRawIO>,
{
    /// Establishes the MQTT connection if not already connected.
    ///
    /// Short-circuits when `self.mqtt` is already `Some`. Otherwise, dials a raw stream via
    /// `self.mqtt_factory.dial()`, wraps it in TLS via `self.mqtt_tls.connect()`, then calls
    /// `BambuMqttClient::connect()` — the whole dial+TLS sequence is raced against
    /// `self.connect_timeout_secs`.
    async fn ensure_mqtt(&mut self) -> Result<(), BambuError> {
        if self.mqtt.is_some() {
            return Ok(());
        }
        let stream = race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async {
            let raw = self.mqtt_factory.dial(&self.ip, self.mqtt_port).await?;
            self.mqtt_tls.connect(&self.ip, raw).await
        })
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
    ) -> PrinterClient<MqttRawIO, MqttTls, MqttFactory, NewTimer, FtpsRawIO, FtpsTls, FtpsFactory>
    {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: self.ftps,
            ftps_config: self.ftps_config,
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
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
            connect_timeout_secs: self.connect_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
            _mqtt_raw_io: PhantomData,
        }
    }

    /// Overrides the default MQTT port (8883).
    pub fn with_mqtt_port(mut self, port: u16) -> Self {
        self.mqtt_port = port;
        self
    }

    /// Overrides the default connect-timeout deadline (10s) that bounds
    /// `ensure_mqtt()`/`ensure_ftps()`'s combined dial+TLS-connect sequence.
    /// Non-consuming — chain onto any construction path.
    pub fn with_connect_timeout(mut self, secs: u64) -> Self {
        self.connect_timeout_secs = secs;
        self
    }

    /// Configures FTPS for lazy connection on first storage method call.
    ///
    /// Consuming builder — changes the `FtpsRawIO`, `FtpsTls`, and `FtpsFactory` type
    /// parameters. The FTPS [`TlsConnector`] is independent from MQTT's (some models require
    /// different TLS settings for FTPS, e.g. `force_tls_1_2`).
    pub fn with_ftps<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory>(
        self,
        tls: NewFtpsTls,
        factory: NewFtpsFactory,
    ) -> PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        NewFtpsRawIO,
        NewFtpsTls,
        NewFtpsFactory,
    >
    where
        NewFtpsRawIO: AsyncIo,
        NewFtpsTls: TlsConnector<NewFtpsRawIO>,
        NewFtpsFactory: RawStreamFactory<NewFtpsRawIO>,
    {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: None,
            ftps_config: Some((tls, factory)),
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
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
            connect_timeout_secs: self.connect_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
            _mqtt_raw_io: PhantomData,
        }
    }

    /// Overrides the default FTPS port (990).
    pub fn with_ftps_port(mut self, port: u16) -> Self {
        self.ftps_port = port;
        self
    }

    /// Establishes the FTPS connection if not already connected.
    ///
    /// Short-circuits when `self.ftps` is already `Some`. Otherwise, takes the TLS connector
    /// and data factory from `ftps_config`, dials a raw connection, and calls
    /// `BambuFtpsClient::connect()` — the whole dial+connect sequence is raced against
    /// `self.connect_timeout_secs`. The config is consumed on first connection —
    /// reconnecting requires a new `PrinterClient`.
    async fn ensure_ftps(&mut self) -> Result<(), BambuError> {
        if self.ftps.is_some() {
            return Ok(());
        }
        let (tls, factory) = self.ftps_config.take().ok_or_else(|| {
            BambuError::ProtocolViolation(
                "FTPS not configured — call .with_ftps() or .attach_storage()".into(),
            )
        })?;
        let ip = &self.ip;
        let access_code = &self.access_code;
        let model = self.model;
        let ftps_port = self.ftps_port;
        let ftps_client =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async move {
                let raw_stream = factory.dial(ip, ftps_port).await?;
                BambuFtpsClient::connect(raw_stream, tls, factory, model, ip, access_code).await
            })
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
        let msg = self
            .mqtt
            .as_mut()
            .unwrap()
            .poll_telemetry_with_timer(&self.timer)
            .await?;
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
        self.mqtt
            .as_mut()
            .unwrap()
            .poll_telemetry_with_timer(&self.timer)
            .await
    }

    /// Polls the MQTT stream until `matcher` returns `Some(T)`, buffering non-matching
    /// messages for later retrieval via `poll_telemetry()` / `poll_raw()`.
    ///
    /// Checks previously-buffered messages (stashed by an earlier `poll_until()` call)
    /// for a match before reading from the wire — a leftover message from a prior
    /// request-response round-trip may already satisfy this call's `matcher`.
    ///
    /// Returns `BambuError::Timeout` if the wall-clock timeout (`command_timeout_secs`)
    /// or message-count safety valve (`POLL_UNTIL_MAX_MESSAGES`) is exceeded. Neither of
    /// these protects against a fully-stalled read on the wire itself: both only run
    /// *after* `poll_wire().await` below has already returned, so a connection that
    /// stalls with zero incoming bytes mid-`await` bypasses them entirely — a real
    /// `Timer` does not help either, since the elapsed-time check is simply never
    /// reached. That protection is a distinct, lower layer: `poll_wire()`
    /// (`src/mqtt/client.rs`) races each low-level read step against `self.timer`
    /// internally, bounding how long a single call below can hang regardless of what
    /// this function's own loop does. See `read_exact_packet`'s doc comment for the
    /// mechanism and the resumability invariant that keeps a timed-out read from
    /// desyncing the stream for the next attempt.
    pub(crate) async fn poll_until<F, T>(&mut self, mut matcher: F) -> Result<T, BambuError>
    where
        F: FnMut(&MqttMessage) -> Option<T>,
    {
        self.ensure_mqtt().await?;

        if let Some(result) = self
            .mqtt
            .as_mut()
            .unwrap()
            .take_pending_matching(&mut matcher)
        {
            return Ok(result);
        }

        let start = self.timer.now_millis();
        let timeout_ms = self.command_timeout_secs * 1000;
        let mut count: usize = 0;

        loop {
            let msg = self.mqtt.as_mut().unwrap().poll_wire(&self.timer).await?;
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
    pub async fn mqtt(&mut self) -> Result<&mut BambuMqttClient<MqttTls::Stream>, BambuError> {
        self.ensure_mqtt().await?;
        Ok(self.mqtt.as_mut().unwrap())
    }
}
