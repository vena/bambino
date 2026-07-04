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
mod camera;
pub mod dummy;
mod hardware;
mod motion;
mod print;
mod storage;
mod thermal;
pub mod types;

pub use dummy::{DummyFactory, DummyRawIo, DummyTimer, DummyTls, PreConnected};
#[doc(inline)]
pub use types::{
    CalibrationOption, FanTarget, PrintProgress, PrintSpeed, PrintStatus, TelemetryEvent,
};

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::Serialize;

use core::marker::PhantomData;

use core::future::Future;

use crate::camera::CameraProtocol;
use crate::camera::binary::BambuBinaryCameraStream;
use crate::diagnostics::{
    DecodedHmsAlert, DecodedPrintError, decode_hms_alert, decode_print_error,
};
use crate::error::BambuError;
use crate::ftps::BambuFtpsClient;
use crate::io::{AsyncIo, Raced, RawStreamFactory, SocketError, TimerProvider, TlsConnector, race};
use crate::models::BambuModel;
use crate::mqtt::commands::TASK_ID_MAX;
use crate::mqtt::{BambuMqttClient, MqttMessage};
use crate::quirks::decode_fan_percentage;
use crate::types::telemetry::{decode_bed_temperatures, decode_nozzle_temperatures};
use crate::types::{
    AmsStatusReport, DeviceTelemetry, HmsEntry, PrinterTelemetry, TelemetryReport, VirtualTray,
};

pub(crate) const INITIAL_SEQUENCE_ID: u64 = 10000;
pub(crate) const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 10;
pub(crate) const POLL_UNTIL_MAX_MESSAGES: usize = 200;
/// Default upper bound on `ensure_mqtt()`/`ensure_ftps()`/`ensure_camera()`'s combined
/// dial+connect sequence (`PLAN.md` Phase 12, decision 6) — matches ESP-IDF's pre-existing
/// `DEFAULT_CONNECT_TIMEOUT` (`src/io/esp_idf.rs`). Override via `.with_connect_timeout(secs)`.
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
    CameraRawIO = DummyRawIo,
    CameraTls = DummyTls,
    CameraFactory = DummyFactory,
> where
    MqttRawIO: AsyncIo,
    MqttTls: TlsConnector<MqttRawIO>,
    MqttFactory: RawStreamFactory<MqttRawIO>,
    Timer: TimerProvider,
    FtpsRawIO: AsyncIo,
    FtpsTls: TlsConnector<FtpsRawIO>,
    FtpsFactory: RawStreamFactory<FtpsRawIO>,
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    pub(crate) mqtt: Option<BambuMqttClient<MqttTls::Stream>>,
    pub(crate) ftps: Option<BambuFtpsClient<FtpsRawIO, FtpsTls, FtpsFactory>>,
    pub(crate) ftps_config: Option<(FtpsTls, FtpsFactory)>,
    pub(crate) camera: Option<BambuBinaryCameraStream<CameraTls::Stream>>,
    pub(crate) camera_config: Option<(CameraTls, CameraFactory)>,
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
    pub(crate) last_door_open: Option<bool>,
    pub(crate) last_print_error: Option<u32>,
    pub(crate) last_progress: PrintProgress,
    pub(crate) last_bed_temper: Option<f64>,
    pub(crate) last_bed_target_temper: Option<f64>,
    pub(crate) last_device: Option<DeviceTelemetry>,
    pub(crate) last_ams: Option<AmsStatusReport>,
    pub(crate) last_vt_tray: Option<VirtualTray>,
    pub(crate) last_vir_slot: Option<Vec<VirtualTray>>,
    pub(crate) last_nozzle_temper: Option<f64>,
    pub(crate) last_nozzle_target_temper: Option<f64>,
    pub(crate) last_chamber_temper: Option<f64>,
    pub(crate) last_hms: Option<Vec<HmsEntry>>,
    pub(crate) last_cooling_fan_speed: Option<String>,
    pub(crate) last_big_fan1_speed: Option<String>,
    pub(crate) last_big_fan2_speed: Option<String>,
    pub(crate) last_heatbreak_fan_speed: Option<String>,
    pub(crate) last_spd_lvl: Option<u8>,
    pub(crate) last_spd_mag: Option<u16>,
    pub(crate) last_wifi_signal: Option<String>,
    pub(crate) command_timeout_secs: u64,
    pub(crate) connect_timeout_secs: u64,
    pub(crate) mqtt_port: u16,
    pub(crate) ftps_port: u16,
    pub(crate) camera_port: u16,
    pub(crate) camera_max_frame_size: Option<usize>,
    pub(crate) _mqtt_raw_io: PhantomData<MqttRawIO>,
    pub(crate) _camera_raw_io: PhantomData<CameraRawIO>,
}

impl<MqttRawIO, MqttTls, MqttFactory>
    PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        DummyTimer,
        DummyRawIo,
        DummyTls,
        DummyFactory,
        DummyRawIo,
        DummyTls,
        DummyFactory,
    >
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
            camera: None,
            camera_config: None,
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
            last_door_open: None,
            last_print_error: None,
            last_progress: PrintProgress::default(),
            last_bed_temper: None,
            last_bed_target_temper: None,
            last_device: None,
            last_ams: None,
            last_vt_tray: None,
            last_vir_slot: None,
            last_nozzle_temper: None,
            last_nozzle_target_temper: None,
            last_chamber_temper: None,
            last_hms: None,
            last_cooling_fan_speed: None,
            last_big_fan1_speed: None,
            last_big_fan2_speed: None,
            last_heatbreak_fan_speed: None,
            last_spd_lvl: None,
            last_spd_mag: None,
            last_wifi_signal: None,
            command_timeout_secs: DEFAULT_COMMAND_TIMEOUT_SECS,
            connect_timeout_secs: DEFAULT_CONNECT_TIMEOUT_SECS,
            mqtt_port: crate::mqtt::MQTTS_PORT,
            ftps_port: crate::ftps::FTPS_PORT,
            camera_port: CameraProtocol::BinaryJpeg.default_port(),
            camera_max_frame_size: None,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
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
            camera: None,
            camera_config: None,
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
            last_door_open: None,
            last_print_error: None,
            last_progress: PrintProgress::default(),
            last_bed_temper: None,
            last_bed_target_temper: None,
            last_device: None,
            last_ams: None,
            last_vt_tray: None,
            last_vir_slot: None,
            last_nozzle_temper: None,
            last_nozzle_target_temper: None,
            last_chamber_temper: None,
            last_hms: None,
            last_cooling_fan_speed: None,
            last_big_fan1_speed: None,
            last_big_fan2_speed: None,
            last_heatbreak_fan_speed: None,
            last_spd_lvl: None,
            last_spd_mag: None,
            last_wifi_signal: None,
            command_timeout_secs: DEFAULT_COMMAND_TIMEOUT_SECS,
            connect_timeout_secs: DEFAULT_CONNECT_TIMEOUT_SECS,
            mqtt_port: crate::mqtt::MQTTS_PORT,
            ftps_port: crate::ftps::FTPS_PORT,
            camera_port: CameraProtocol::BinaryJpeg.default_port(),
            camera_max_frame_size: None,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        }
    }
}

impl<
    MqttRawIO,
    MqttTls,
    MqttFactory,
    Timer,
    FtpsRawIO,
    FtpsTls,
    FtpsFactory,
    CameraRawIO,
    CameraTls,
    CameraFactory,
>
    PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        CameraRawIO,
        CameraTls,
        CameraFactory,
    >
where
    MqttRawIO: AsyncIo,
    MqttTls: TlsConnector<MqttRawIO>,
    MqttFactory: RawStreamFactory<MqttRawIO>,
    Timer: TimerProvider,
    FtpsRawIO: AsyncIo,
    FtpsTls: TlsConnector<FtpsRawIO>,
    FtpsFactory: RawStreamFactory<FtpsRawIO>,
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    /// Establishes the MQTT connection if not already connected.
    ///
    /// Short-circuits when `self.mqtt` is already `Some`. Otherwise, dials a raw stream via
    /// `self.mqtt_factory.dial()`, wraps it in TLS via `self.mqtt_tls.connect()`, then calls
    /// `BambuMqttClient::connect()` — the whole dial+TLS+handshake sequence is raced against
    /// `self.connect_timeout_secs`.
    async fn ensure_mqtt(&mut self) -> Result<(), BambuError> {
        if self.mqtt.is_some() {
            return Ok(());
        }
        let mqtt_client =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async {
                let raw = self.mqtt_factory.dial(&self.ip, self.mqtt_port).await?;
                let stream = self.mqtt_tls.connect(&self.ip, raw).await?;
                BambuMqttClient::connect(stream, &self.serial, &self.access_code).await
            })
            .await?;
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
    ) -> PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        NewTimer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        CameraRawIO,
        CameraTls,
        CameraFactory,
    > {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: self.ftps,
            ftps_config: self.ftps_config,
            camera: self.camera,
            camera_config: self.camera_config,
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
            last_door_open: self.last_door_open,
            last_print_error: self.last_print_error,
            last_progress: self.last_progress,
            last_bed_temper: self.last_bed_temper,
            last_bed_target_temper: self.last_bed_target_temper,
            last_device: self.last_device,
            last_ams: self.last_ams,
            last_vt_tray: self.last_vt_tray,
            last_vir_slot: self.last_vir_slot,
            last_nozzle_temper: self.last_nozzle_temper,
            last_nozzle_target_temper: self.last_nozzle_target_temper,
            last_chamber_temper: self.last_chamber_temper,
            last_hms: self.last_hms,
            last_cooling_fan_speed: self.last_cooling_fan_speed,
            last_big_fan1_speed: self.last_big_fan1_speed,
            last_big_fan2_speed: self.last_big_fan2_speed,
            last_heatbreak_fan_speed: self.last_heatbreak_fan_speed,
            last_spd_lvl: self.last_spd_lvl,
            last_spd_mag: self.last_spd_mag,
            last_wifi_signal: self.last_wifi_signal,
            command_timeout_secs: self.command_timeout_secs,
            connect_timeout_secs: self.connect_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
            camera_port: self.camera_port,
            camera_max_frame_size: self.camera_max_frame_size,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
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
        CameraRawIO,
        CameraTls,
        CameraFactory,
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
            camera: self.camera,
            camera_config: self.camera_config,
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
            last_door_open: self.last_door_open,
            last_print_error: self.last_print_error,
            last_progress: self.last_progress,
            last_bed_temper: self.last_bed_temper,
            last_bed_target_temper: self.last_bed_target_temper,
            last_device: self.last_device,
            last_ams: self.last_ams,
            last_vt_tray: self.last_vt_tray,
            last_vir_slot: self.last_vir_slot,
            last_nozzle_temper: self.last_nozzle_temper,
            last_nozzle_target_temper: self.last_nozzle_target_temper,
            last_chamber_temper: self.last_chamber_temper,
            last_hms: self.last_hms,
            last_cooling_fan_speed: self.last_cooling_fan_speed,
            last_big_fan1_speed: self.last_big_fan1_speed,
            last_big_fan2_speed: self.last_big_fan2_speed,
            last_heatbreak_fan_speed: self.last_heatbreak_fan_speed,
            last_spd_lvl: self.last_spd_lvl,
            last_spd_mag: self.last_spd_mag,
            last_wifi_signal: self.last_wifi_signal,
            command_timeout_secs: self.command_timeout_secs,
            connect_timeout_secs: self.connect_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
            camera_port: self.camera_port,
            camera_max_frame_size: self.camera_max_frame_size,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
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
                self.update_telemetry_cache(&report);
                Ok(TelemetryEvent::Report(Box::new(report), msg))
            }
            Err(_) => Ok(TelemetryEvent::Unknown(msg)),
        }
    }

    /// Updates every `last_*` telemetry cache from a freshly-parsed report. A field only
    /// overwrites its cache when present in `report` — a message that omits a field leaves
    /// the previously-cached value in place (staleness is intentional; see the `last_*`
    /// field docs on the struct).
    fn update_telemetry_cache(&mut self, report: &TelemetryReport) {
        if let Some(device) = report.device() {
            self.last_device = Some(device.clone());
        }
        let Some(print) = report.print.as_ref() else {
            return;
        };
        self.update_state_cache(print);
        self.update_progress_cache(print);
        self.update_temperature_cache(print);
        self.update_ams_cache(print);
        self.update_fan_cache(print);
        self.update_speed_and_signal_cache(print);
    }

    fn update_state_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(flag) = print.home_flag {
            self.last_home_flag = Some(flag);
        }
        if let Some(state) = &print.gcode_state {
            self.last_gcode_state = Some(state.clone());
        }
        self.last_door_open = Some(self.model.quirks().is_door_open(print));
        if let Some(print_error) = print.print_error {
            self.last_print_error = Some(print_error);
        }
        if let Some(hms) = &print.hms {
            self.last_hms = Some(hms.clone());
        }
    }

    fn update_progress_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(percent) = print.mc_percent {
            self.last_progress.percent = Some(percent);
        }
        if let Some(remaining) = print.mc_remaining_time {
            self.last_progress.remaining_secs = Some(remaining);
        }
        if let Some(layer_num) = print.layer_num {
            self.last_progress.layer_num = Some(layer_num);
        }
        if let Some(total_layers) = print.total_layers {
            self.last_progress.total_layers = Some(total_layers);
        }
    }

    fn update_temperature_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(bed_temper) = print.bed_temper {
            self.last_bed_temper = Some(bed_temper);
        }
        if let Some(bed_target_temper) = print.bed_target_temper {
            self.last_bed_target_temper = Some(bed_target_temper);
        }
        if let Some(nozzle_temper) = print.nozzle_temper {
            self.last_nozzle_temper = Some(nozzle_temper);
        }
        if let Some(nozzle_target_temper) = print.nozzle_target_temper {
            self.last_nozzle_target_temper = Some(nozzle_target_temper);
        }
        if let Some(chamber_temper) = print.chamber_temper {
            self.last_chamber_temper = Some(chamber_temper);
        }
    }

    fn update_ams_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(ams) = &print.ams {
            self.last_ams = Some(ams.clone());
        }
        if let Some(vt_tray) = &print.vt_tray {
            self.last_vt_tray = Some(vt_tray.clone());
        }
        if let Some(vir_slot) = &print.vir_slot {
            self.last_vir_slot = Some(vir_slot.clone());
        }
    }

    fn update_fan_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(v) = &print.cooling_fan_speed {
            self.last_cooling_fan_speed = Some(v.clone());
        }
        if let Some(v) = &print.big_fan1_speed {
            self.last_big_fan1_speed = Some(v.clone());
        }
        if let Some(v) = &print.big_fan2_speed {
            self.last_big_fan2_speed = Some(v.clone());
        }
        if let Some(v) = &print.heatbreak_fan_speed {
            self.last_heatbreak_fan_speed = Some(v.clone());
        }
    }

    fn update_speed_and_signal_cache(&mut self, print: &PrinterTelemetry) {
        if let Some(spd_lvl) = print.spd_lvl {
            self.last_spd_lvl = Some(spd_lvl);
        }
        if let Some(spd_mag) = print.spd_mag {
            self.last_spd_mag = Some(spd_mag);
        }
        if let Some(wifi_signal) = &print.wifi_signal {
            self.last_wifi_signal = Some(wifi_signal.clone());
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

    /// Returns whether the door was open as of the last-observed telemetry (via
    /// [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// Returns `None` on models without a door sensor (`ModelQuirks::has_door_sensor()`
    /// returns `false`, e.g. A1/A2), regardless of telemetry observed — distinct from
    /// `Some(false)`, which means a sensor-equipped model's telemetry confirms the door is
    /// closed. Also `None` before any telemetry carrying `print` has been observed.
    pub fn door_open(&self) -> Option<bool> {
        if !self.model.quirks().has_door_sensor() {
            return None;
        }
        self.last_door_open
    }

    /// Returns the decoded active print-error fault as of the last-observed `print_error`
    /// telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// `None` covers both "no telemetry carrying `print_error` observed yet" and "the
    /// register reads 0 (no fault)" — both warrant the same caller action, so they are not
    /// distinguished here.
    pub fn active_fault(&self) -> Option<DecodedPrintError> {
        decode_print_error(self.last_print_error?)
    }

    /// Returns the print progress snapshot as of the last-observed telemetry (via
    /// [`poll_telemetry()`](Self::poll_telemetry)). Each field independently tracks its own
    /// "last observed" value — see [`PrintProgress`]'s doc comment.
    pub fn print_progress(&self) -> PrintProgress {
        self.last_progress
    }

    /// Returns the bed's (actual, target) temperatures in °C, decoded from the last-observed
    /// telemetry (via [`poll_telemetry()`](Self::poll_telemetry)). Returns `(0, 0)` before any
    /// telemetry carrying bed temperature has been observed.
    ///
    /// Shares its cross-model decode logic with
    /// [`TelemetryReport::bed_temperatures()`](crate::types::TelemetryReport::bed_temperatures) —
    /// use that method instead if you already have a fresh `TelemetryReport` in hand.
    pub fn bed_temperatures(&self) -> (u16, u16) {
        decode_bed_temperatures(
            self.last_device.as_ref(),
            self.last_bed_temper,
            self.last_bed_target_temper,
        )
    }

    /// Returns the cached AMS/tray status report as of the last-observed telemetry (via
    /// [`poll_telemetry()`](Self::poll_telemetry)). `None` means no telemetry carrying
    /// `print.ams` has been observed yet.
    pub fn ams(&self) -> Option<&AmsStatusReport> {
        self.last_ams.as_ref()
    }

    /// Returns the cached virtual/external spool holder state (single-nozzle models) as of
    /// the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)). `None`
    /// means no telemetry carrying `print.vt_tray` has been observed yet — including on IDEX
    /// models, which send [`vir_slot()`](Self::vir_slot) instead.
    pub fn vt_tray(&self) -> Option<&VirtualTray> {
        self.last_vt_tray.as_ref()
    }

    /// Returns the cached IDEX external spool holder array as of the last-observed telemetry
    /// (via [`poll_telemetry()`](Self::poll_telemetry)). `None` means no telemetry carrying
    /// `print.vir_slot` has been observed yet — including on single-nozzle models, which send
    /// [`vt_tray()`](Self::vt_tray) instead.
    pub fn vir_slot(&self) -> Option<&[VirtualTray]> {
        self.last_vir_slot.as_deref()
    }

    /// Returns the nozzle temperatures as of the last-observed telemetry (via
    /// [`poll_telemetry()`](Self::poll_telemetry)) as `(id, actual, target)` tuples in °C.
    /// Single-nozzle models return one entry (`id` 0); IDEX models return one entry per
    /// physical nozzle. See [`decode_nozzle_temperatures`] for the cross-model decode
    /// (including the undocumented IDEX flat-field routing quirk).
    pub fn nozzle_temperatures(&self) -> Vec<(u8, u16, u16)> {
        decode_nozzle_temperatures(
            self.last_device.as_ref(),
            self.last_nozzle_temper,
            self.last_nozzle_target_temper,
        )
    }

    /// Returns the chamber's (actual, target) temperatures in °C, decoded from the
    /// last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    ///
    /// Returns `None` on models without an active chamber temperature sensor/heater
    /// (`ModelQuirks::ignores_chamber_temperature()` returns `true`, e.g. A1/A1 Mini/A2L/P1P/
    /// P1S) — mirrors `door_open()`'s sensor-capability gate. `Some((0, 0))` before any
    /// telemetry carrying `chamber_temper` has been observed on a chamber-equipped model.
    pub fn chamber_temperature(&self) -> Option<(u16, u16)> {
        if self.model.quirks().ignores_chamber_temperature() {
            return None;
        }
        let raw = self.last_chamber_temper.unwrap_or(0.0);
        Some(PrinterTelemetry::unpack_temperature(raw))
    }

    /// Returns the cached active hardware-alert (HMS) entries as of the last-observed
    /// telemetry (via [`poll_telemetry()`](Self::poll_telemetry)). `None` means no telemetry
    /// carrying `print.hms` has been observed yet.
    pub fn hms(&self) -> Option<&[HmsEntry]> {
        self.last_hms.as_deref()
    }

    /// Returns every cached HMS entry decoded and filtered to genuine faults (mirrors
    /// `active_fault()`'s raw-cache-decode-on-access shape). Empty when nothing is cached or
    /// nothing currently decodes as a genuine fault — there's no caller action that would
    /// differ between those two cases.
    pub fn active_hms_alerts(&self) -> Vec<DecodedHmsAlert> {
        self.last_hms
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|entry| decode_hms_alert(entry.attr, entry.code))
            .filter(|decoded| decoded.is_genuine_fault)
            .collect()
    }

    /// Returns the part-cooling fan speed (Port 1) as a percentage (0-100), decoded from the
    /// last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    pub fn part_cooling_fan_speed(&self) -> Option<u8> {
        self.decode_fan_speed(self.last_cooling_fan_speed.as_deref())
    }

    /// Returns the primary left-side auxiliary fan speed (Port 2) as a percentage (0-100).
    pub fn auxiliary_left_fan_speed(&self) -> Option<u8> {
        self.decode_fan_speed(self.last_big_fan1_speed.as_deref())
    }

    /// Returns the chamber exhaust/filtration fan speed (Port 3) as a percentage (0-100).
    pub fn chamber_exhaust_fan_speed(&self) -> Option<u8> {
        self.decode_fan_speed(self.last_big_fan2_speed.as_deref())
    }

    /// Returns the toolhead heatbreak fan speed as a percentage (0-100). Not independently
    /// controllable (no corresponding `FanTarget` variant/M106 port) — read-only telemetry.
    pub fn heatbreak_fan_speed(&self) -> Option<u8> {
        self.decode_fan_speed(self.last_heatbreak_fan_speed.as_deref())
    }

    /// Returns the X2D/P2S secondary right-side auxiliary fan speed (Port 10,
    /// `FanTarget::AuxiliaryRight`) as a percentage (0-100). Reported at a different wire
    /// location than the other four fans — `device.airduct.parts[id=160].state` — already a
    /// direct percentage, no 0-15 step conversion [REF-CLIM-FANS].
    pub fn auxiliary_right_fan_speed(&self) -> Option<u8> {
        let state = self
            .last_device
            .as_ref()?
            .airduct
            .as_ref()?
            .parts
            .iter()
            .find(|part| part.id == 160)?
            .state?;
        Some(state.clamp(0, 100) as u8)
    }

    fn decode_fan_speed(&self, raw: Option<&str>) -> Option<u8> {
        decode_fan_percentage(raw, self.model.quirks().auxiliary_fan_uses_percentage())
    }

    /// Returns the printer's current print-speed level as of the last-observed telemetry (via
    /// [`poll_telemetry()`](Self::poll_telemetry)). `None` before any telemetry carrying
    /// `spd_lvl` has been observed, or if the observed value is out of the known 1-4 range.
    pub fn print_speed(&self) -> Option<PrintSpeed> {
        PrintSpeed::from_level(self.last_spd_lvl?)
    }

    /// Returns the printer's current print-speed magnitude (percentage of nominal feedrate) as
    /// of the last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    pub fn print_speed_magnitude(&self) -> Option<u16> {
        self.last_spd_mag
    }

    /// Returns the raw wireless signal strength string (e.g. `"-52dBm"`) as of the
    /// last-observed telemetry (via [`poll_telemetry()`](Self::poll_telemetry)).
    pub fn wifi_signal(&self) -> Option<&str> {
        self.last_wifi_signal.as_deref()
    }

    /// Returns whether the printer is on wired Ethernet, per the cached `wifi_signal` sentinel
    /// (mirrors `PrinterTelemetry::is_ethernet_active_via_wifi_signal()` but works between polls
    /// off the cached value, the same way [`is_all_axes_homed()`](Self::is_all_axes_homed) works
    /// off cached `home_flag`).
    pub fn is_ethernet_active_via_wifi_signal(&self) -> bool {
        self.last_wifi_signal.as_deref() == Some("-90dBm")
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

    /// Establishes the camera connection if not already connected.
    ///
    /// Returns `BambuError::ProtocolViolation` immediately for RTSPS models — those use
    /// `camera::rtsps::build_rtsps_url()` instead and have no `PrinterClient`-managed
    /// connection state. Otherwise dials a raw stream via the camera factory, wraps it in
    /// TLS, constructs a `BambuBinaryCameraStream`, and authenticates — the whole sequence is
    /// raced against `self.connect_timeout_secs`, mirroring `ensure_ftps()`.
    async fn ensure_camera(&mut self) -> Result<(), BambuError> {
        if self.model.quirks().camera_protocol() != CameraProtocol::BinaryJpeg {
            return Err(BambuError::ProtocolViolation(
                "This model uses RTSPS for its camera feed — use camera::rtsps::build_rtsps_url() instead"
                    .into(),
            ));
        }
        if self.camera.is_some() {
            return Ok(());
        }
        let (tls, factory) = self.camera_config.take().ok_or_else(|| {
            BambuError::ProtocolViolation(
                "Camera not configured — call .with_camera() or .attach_camera()".into(),
            )
        })?;
        let ip = &self.ip;
        let access_code = &self.access_code;
        let camera_port = self.camera_port;
        let max_frame_size = self.camera_max_frame_size;
        let camera_stream =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async move {
                let raw = factory.dial(ip, camera_port).await?;
                let stream = tls.connect(ip, raw).await?;
                let mut cam = BambuBinaryCameraStream::new(stream);
                if let Some(max) = max_frame_size {
                    cam = cam.with_max_frame_size(max);
                }
                cam.authenticate(access_code).await?;
                Ok::<_, BambuError>(cam)
            })
            .await?;
        self.camera = Some(camera_stream);
        Ok(())
    }

    /// Eagerly establishes the camera connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_camera(&mut self) -> Result<(), BambuError> {
        self.ensure_camera().await
    }

    /// Returns whether the camera session is currently established.
    pub fn camera_connected(&self) -> bool {
        self.camera.is_some()
    }

    /// Configures the binary-JPEG camera for lazy connection on first camera method call.
    ///
    /// Consuming builder — changes the `CameraRawIO`, `CameraTls`, and `CameraFactory` type
    /// parameters. Independent of MQTT's and FTPS's connectors, mirroring `.with_ftps()`.
    pub fn with_camera<NewCameraRawIO, NewCameraTls, NewCameraFactory>(
        self,
        tls: NewCameraTls,
        factory: NewCameraFactory,
    ) -> PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        NewCameraRawIO,
        NewCameraTls,
        NewCameraFactory,
    >
    where
        NewCameraRawIO: AsyncIo,
        NewCameraTls: TlsConnector<NewCameraRawIO>,
        NewCameraFactory: RawStreamFactory<NewCameraRawIO>,
    {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: self.ftps,
            ftps_config: self.ftps_config,
            camera: None,
            camera_config: Some((tls, factory)),
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
            last_door_open: self.last_door_open,
            last_print_error: self.last_print_error,
            last_progress: self.last_progress,
            last_bed_temper: self.last_bed_temper,
            last_bed_target_temper: self.last_bed_target_temper,
            last_device: self.last_device,
            last_ams: self.last_ams,
            last_vt_tray: self.last_vt_tray,
            last_vir_slot: self.last_vir_slot,
            last_nozzle_temper: self.last_nozzle_temper,
            last_nozzle_target_temper: self.last_nozzle_target_temper,
            last_chamber_temper: self.last_chamber_temper,
            last_hms: self.last_hms,
            last_cooling_fan_speed: self.last_cooling_fan_speed,
            last_big_fan1_speed: self.last_big_fan1_speed,
            last_big_fan2_speed: self.last_big_fan2_speed,
            last_heatbreak_fan_speed: self.last_heatbreak_fan_speed,
            last_spd_lvl: self.last_spd_lvl,
            last_spd_mag: self.last_spd_mag,
            last_wifi_signal: self.last_wifi_signal,
            command_timeout_secs: self.command_timeout_secs,
            connect_timeout_secs: self.connect_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
            camera_port: self.camera_port,
            camera_max_frame_size: self.camera_max_frame_size,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        }
    }

    /// Overrides the default camera port (6000, binary-JPEG only).
    pub fn with_camera_port(mut self, port: u16) -> Self {
        self.camera_port = port;
        self
    }

    /// Overrides the default maximum accepted camera frame size (see
    /// `BambuBinaryCameraStream::with_max_frame_size`).
    pub fn with_camera_max_frame_size(mut self, bytes: usize) -> Self {
        self.camera_max_frame_size = Some(bytes);
        self
    }
}
