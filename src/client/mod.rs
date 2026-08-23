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
mod connect;
pub mod dummy;
mod hardware;
mod motion;
mod print;
mod storage;
mod telemetry;
mod thermal;
pub mod types;

pub use dummy::{DummyFactory, DummyRawIo, DummyTimer, DummyTls, PreConnected};
#[doc(inline)]
pub use types::{
    BuzzerMode, CalibrationOption, FanTarget, PrintProgress, PrintSpeed, PrintStatus,
    TelemetryEvent,
};

#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::Serialize;

use core::marker::PhantomData;

use crate::camera::CameraProtocol;
use crate::camera::binary::BinaryCameraStream;
use crate::error::Error;
use crate::ftps::FtpsClient;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::identity::PrinterIdentity;
use crate::models::PrinterModel;
use crate::mqtt::{MqttClient, MqttMessage};

pub(crate) const INITIAL_SEQUENCE_ID: u64 = 10000;
pub(crate) const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 10;
pub(crate) const POLL_UNTIL_MAX_MESSAGES: usize = 200;
/// Default upper bound on `ensure_mqtt()`/`ensure_ftps()`/`ensure_camera()`'s combined dial+connect sequence — matches ESP-IDF's pre-existing `DEFAULT_CONNECT_TIMEOUT` (`src/io/esp_idf.rs`).
/// Override via `.with_connect_timeout(secs)`.
pub(crate) const DEFAULT_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Clamps `value` to `max`, logging a warning if it was reduced.
/// Shared by every model-ceiling-clamped heater-setting method in `thermal.rs`
/// (bed/nozzle/chamber), which previously repeated this identical clamp-and-warn block three times,
/// differing only in `label`.
pub(crate) fn clamp_temp(value: u16, max: u16, label: &str) -> u16 {
    if value > max {
        log::warn!(
            "{} temperature {}°C exceeds model max {}°C, clamping",
            label,
            value,
            max
        );
        max
    } else {
        value
    }
}

/// High-level client for controlling a Bambu Lab printer.
///
/// Wraps an MQTT session (connected or lazy) and optionally a [`FtpsClient`] for
/// SD card access. `MqttRawIO`/`MqttTls`/`MqttFactory` are MQTT's [`TlsConnector`]+
/// [`RawStreamFactory`] pair (mandatory — every `PrinterClient` needs MQTT);
/// `FtpsRawIO`/`FtpsTls`/`FtpsFactory` are FTPS's independent pair (defaulted, configured via
/// [`.with_ftps()`](Self::with_ftps)). Use [`PreConnected`] for both MQTT slots when wrapping
/// an already-connected [`MqttClient`] (see [`from_mqtt()`](Self::from_mqtt)), or a
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
    FtpsTimer = DummyTimer,
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
    FtpsTimer: TimerProvider,
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    pub(crate) mqtt: Option<MqttClient<MqttTls::Stream>>,
    pub(crate) ftps: Option<FtpsClient<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>>,
    pub(crate) ftps_config: Option<(FtpsTls, FtpsFactory, FtpsTimer)>,
    pub(crate) camera: Option<BinaryCameraStream<CameraTls::Stream>>,
    pub(crate) camera_config: Option<(CameraTls, CameraFactory)>,
    pub(crate) mqtt_tls: MqttTls,
    pub(crate) mqtt_factory: MqttFactory,
    pub(crate) timer: Timer,
    pub(crate) identity: PrinterIdentity,
    pub(crate) sequence_counter: u64,
    pub(crate) k_profile_primed: bool,
    pub(crate) cache: telemetry::TelemetryCache,
    pub(crate) command_timeout_secs: u64,
    pub(crate) connect_timeout_secs: u64,
    pub(crate) mqtt_port: u16,
    pub(crate) ftps_port: u16,
    /// Bypasses `FtpsClient`'s TLS-1.2-enforcement rejection for P2S/X2D when set —
    /// see `src/ftps/CLAUDE.md` and `src/io/CLAUDE.md`. Only meaningful for the `embassy`
    /// feature; on `tokio`/`esp-idf`, use `force_tls_1_2` on the `TlsConnector` instead.
    /// Default `false`.
    pub(crate) ftps_allow_unverified_tls_1_2: bool,
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
        DummyTimer,
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
    /// [`.with_ftps(tls, factory, timer)`](Self::with_ftps)'s call shape — `factory.dial()` opens the
    /// raw TCP socket, then `tls.connect()` wraps it in TLS.
    ///
    /// Without a [`TimerProvider`], command-response methods like
    /// [`get_version()`](Self::get_version) rely on a message-count safety valve
    /// instead of wall-clock timeouts. Chain [`.with_timer()`](Self::with_timer)
    /// for real timeouts.
    pub fn new(
        tls: MqttTls,
        factory: MqttFactory,
        identity: PrinterIdentity,
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
            identity,
            sequence_counter: INITIAL_SEQUENCE_ID,
            k_profile_primed: false,
            cache: telemetry::TelemetryCache::default(),
            command_timeout_secs: DEFAULT_COMMAND_TIMEOUT_SECS,
            connect_timeout_secs: DEFAULT_CONNECT_TIMEOUT_SECS,
            mqtt_port: crate::mqtt::MQTTS_PORT,
            ftps_port: crate::ftps::FTPS_PORT,
            ftps_allow_unverified_tls_1_2: false,
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
        DummyTimer,
        DummyRawIo,
        DummyTls,
        DummyFactory,
    >
where
    IO: AsyncIo,
{
    /// Wraps an already-connected [`MqttClient`] in a `PrinterClient`.
    ///
    /// Use this when you have a pre-established MQTT session (tests, Embassy,
    /// or any context where the caller manages the connection). The resulting client uses
    /// [`PreConnected`] for both the MQTT `Tls` and `Factory` slots — `ensure_mqtt()`
    /// short-circuits on `self.mqtt.is_some()` before either is ever called, so
    /// `PreConnected`'s `RawStreamFactory::dial` (which returns
    /// [`SocketError::NotConnected`](crate::io::SocketError::NotConnected)) is unreachable in
    /// practice.
    pub fn from_mqtt(mqtt_client: MqttClient<IO>, model: PrinterModel) -> Self {
        let serial = String::from(mqtt_client.serial());
        Self {
            mqtt: Some(mqtt_client),
            ftps: None,
            ftps_config: None,
            camera: None,
            camera_config: None,
            mqtt_tls: PreConnected(PhantomData),
            mqtt_factory: PreConnected(PhantomData),
            timer: DummyTimer,
            identity: PrinterIdentity {
                serial,
                ip: String::new(),
                access_code: String::new(),
                model,
            },
            sequence_counter: INITIAL_SEQUENCE_ID,
            k_profile_primed: false,
            cache: telemetry::TelemetryCache::default(),
            command_timeout_secs: DEFAULT_COMMAND_TIMEOUT_SECS,
            connect_timeout_secs: DEFAULT_CONNECT_TIMEOUT_SECS,
            mqtt_port: crate::mqtt::MQTTS_PORT,
            ftps_port: crate::ftps::FTPS_PORT,
            ftps_allow_unverified_tls_1_2: false,
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
    FtpsTimer,
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
        FtpsTimer,
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
    FtpsTimer: TimerProvider,
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    /// Increments and returns the next transaction/sequence identifier tracking commands.
    ///
    /// Wraps via `clamp_task_id()` (32-bit signed integer limit) to stay within firmware
    /// parsing constraints [REF-MQTT-ENV] — on overflow this continues as
    /// `(sequence_counter + 1) % TASK_ID_MAX` rather than resetting to
    /// `INITIAL_SEQUENCE_ID`, so a session never revisits the same starting value mid-flight.
    pub fn next_sequence_id(&mut self) -> u64 {
        self.sequence_counter =
            crate::mqtt::commands::clamp_task_id(self.sequence_counter + 1) as u64;
        self.sequence_counter
    }

    /// Sets the timeout (in seconds) used by command-response methods like [`get_version()`](Self::get_version) and [`get_k_profiles()`](Self::get_k_profiles).
    ///
    /// Passing `0` disables the wall-clock timeout entirely — commands then rely solely on
    /// the 200-message safety valve (`POLL_UNTIL_MAX_MESSAGES`), not immediate timeout.
    pub fn set_command_timeout(&mut self, secs: u64) {
        self.command_timeout_secs = secs;
    }

    /// Polls the MQTT stream until `matcher` returns `Some(T)`, buffering non-matching messages for later retrieval via `poll_telemetry()` / `poll_raw()`.
    ///
    /// Checks previously-buffered messages (stashed by an earlier `poll_until()` call)
    /// for a match before reading from the wire — a leftover message from a prior
    /// request-response round-trip may already satisfy this call's `matcher`.
    ///
    /// Returns `Error::Timeout` if the wall-clock timeout (`command_timeout_secs`)
    /// or message-count safety valve (`POLL_UNTIL_MAX_MESSAGES`) is exceeded. Neither of
    /// these protects against a fully-stalled read on the wire itself: both only run
    /// *after* `poll_wire().await` below has already returned, so a connection that
    /// stalls with zero incoming bytes mid-`await` bypasses them entirely — a real
    /// `Timer` does not help either, since the elapsed-time check is simply never
    /// reached. That protection is a distinct, lower layer: `poll_wire()`
    /// (`src/mqtt/client/mod.rs`) races each low-level read step against `self.timer`
    /// internally, bounding how long a single call below can hang regardless of what
    /// this function's own loop does. See `read_exact_packet`'s doc comment for the
    /// mechanism and the resumability invariant that keeps a timed-out read from
    /// desyncing the stream for the next attempt.
    pub(crate) async fn poll_until<F, T>(&mut self, mut matcher: F) -> Result<T, Error>
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
                return Err(Error::Timeout);
            }
            let elapsed = self.timer.now_millis().wrapping_sub(start);
            if timeout_ms > 0 && elapsed >= timeout_ms {
                return Err(Error::Timeout);
            }
        }
    }

    /// Serializes a request struct and publishes it to the printer's MQTT command channel.
    pub(crate) async fn publish_request<T: Serialize>(
        &mut self,
        request: &T,
    ) -> Result<u16, Error> {
        self.ensure_mqtt().await?;
        let payload = serde_json::to_vec(request).map_err(|_| Error::Serialization)?;
        self.mqtt
            .as_mut()
            .unwrap()
            .publish_command_with_timer(&payload, &self.timer)
            .await
    }

    /// Collapses the repeated `next_sequence_id()` → build request → `publish_request()` triplet used by nearly every command-dispatching method in this module.
    /// `build` receives the freshly-clamped sequence ID and constructs the request struct; closures can
    /// capture whatever other locals a given command needs beyond `seq`.
    pub(crate) async fn dispatch<T: Serialize>(
        &mut self,
        build: impl FnOnce(u64) -> T,
    ) -> Result<u16, Error> {
        let seq = self.next_sequence_id();
        let req = build(seq);
        self.publish_request(&req).await
    }

    /// Requests a full state dump from the printer [REF-MQTT-LIFECYCLE].
    pub async fn request_pushall(&mut self) -> Result<u16, Error> {
        self.dispatch(crate::mqtt::PushAllRequest::new).await
    }

    /// Dispatches a PINGREQ keep-alive frame to maintain connection liveness.
    pub async fn send_ping(&mut self) -> Result<(), Error> {
        self.ensure_mqtt().await?;
        self.mqtt.as_mut().unwrap().send_ping_with_timer(&self.timer).await
    }

    /// Returns a reference to the printer's unique hardware serial number.
    pub fn serial(&self) -> &str {
        &self.identity.serial
    }

    /// Returns the resolved printer hardware model.
    pub fn model(&self) -> PrinterModel {
        self.identity.model
    }

    /// Returns direct access to the underlying [`MqttClient`], auto-connecting if needed.
    ///
    /// Use this for sending custom MQTT payloads, managing zombie detection via
    /// [`tick_zombie_check()`](MqttClient::tick_zombie_check), or inspecting
    /// in-flight state — anything that [`PrinterClient`] doesn't expose directly.
    ///
    /// Pipelining multiple commands through this handle before awaiting a response forfeits
    /// write-zombie coverage beyond the first outstanding command: `tick_zombie_check()` tracks
    /// only one armed `(sequence_id, elapsed_secs)` pair at a time, so a second `publish_command`
    /// issued while the first is still unanswered gets no tracking of its own — if the broker
    /// acks the first but silently drops the second, the second can hang forever undetected.
    /// The default [`PrinterClient`] request flow awaits each command in turn and isn't affected.
    pub async fn mqtt(&mut self) -> Result<&mut MqttClient<MqttTls::Stream>, Error> {
        self.ensure_mqtt().await?;
        Ok(self.mqtt.as_mut().unwrap())
    }
}
