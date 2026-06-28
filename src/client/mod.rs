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

pub use dummy::{DummyFactory, DummyRawIo, DummyTimer, DummyTls};
#[doc(inline)]
pub use types::{CalibrationOption, FanTarget, PrintSpeed, TelemetryEvent};

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;
#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(not(feature = "std"))]
use alloc::string::String;

#[cfg(feature = "std")]
use std::collections::VecDeque;

use serde::Serialize;

use crate::error::BambuError;
use crate::ftps::{BambuFtpsClient, FtpDataStreamFactory};
use crate::io::{AsyncIo, TimerProvider, TlsConnector};
use crate::models::BambuModel;
use crate::mqtt::commands::TASK_ID_MAX;
use crate::mqtt::{BambuMqttClient, MqttMessage};
use crate::types::TelemetryReport;

pub(crate) const INITIAL_SEQUENCE_ID: u64 = 10000;
pub(crate) const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 10;
pub(crate) const POLL_UNTIL_MAX_MESSAGES: usize = 200;

/// High-level client for controlling a Bambu Lab printer.
///
/// Wraps an active [`BambuMqttClient`] session and optionally a [`BambuFtpsClient`] for
/// SD card access. The type parameters default to [`DummyTimer`]/[`DummyTls`]/etc. so
/// you can create an MQTT-only client without specifying the FTPS generics.
pub struct PrinterClient<
    IO,
    Timer = DummyTimer,
    RawIO = DummyRawIo,
    Tls = DummyTls,
    Factory = DummyFactory,
> where
    IO: AsyncIo,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
    pub(crate) mqtt: BambuMqttClient<IO>,
    pub(crate) ftps: Option<BambuFtpsClient<RawIO, Tls, Factory>>,
    pub(crate) timer: Timer,
    pub(crate) serial: String,
    pub(crate) model: BambuModel,
    pub(crate) sequence_counter: u64,
    pub(crate) k_profile_primed: bool,
    pub(crate) pending_messages: VecDeque<MqttMessage>,
    pub(crate) command_timeout_secs: u64,
}

impl<IO> PrinterClient<IO, DummyTimer, DummyRawIo, DummyTls, DummyFactory>
where
    IO: AsyncIo,
{
    /// Creates an MQTT-only client with no FTPS or timer support.
    ///
    /// This is the simplest way to get started. Without a [`TimerProvider`], command-response
    /// methods like [`get_version()`](Self::get_version) rely on a message-count safety valve
    /// instead of wall-clock timeouts — fine for most use cases.
    pub fn new(mut mqtt_client: BambuMqttClient<IO>, serial: &str, model: BambuModel) -> Self {
        mqtt_client.owned_by_printerclient = true;
        Self {
            mqtt: mqtt_client,
            ftps: None,
            timer: DummyTimer,
            serial: String::from(serial),
            model,
            sequence_counter: INITIAL_SEQUENCE_ID,
            k_profile_primed: false,
            pending_messages: VecDeque::new(),
            command_timeout_secs: DEFAULT_COMMAND_TIMEOUT_SECS,
        }
    }
}

impl<IO, Timer> PrinterClient<IO, Timer, DummyRawIo, DummyTls, DummyFactory>
where
    IO: AsyncIo,
    Timer: TimerProvider,
{
    /// Creates a client with a [`TimerProvider`] for wall-clock command-response timeouts.
    ///
    /// Use this when you need reliable timeouts on methods like [`get_version()`](Self::get_version)
    /// and [`get_k_profiles()`](Self::get_k_profiles).
    pub fn new_with_timer(
        mut mqtt_client: BambuMqttClient<IO>,
        timer: Timer,
        serial: &str,
        model: BambuModel,
    ) -> Self {
        mqtt_client.owned_by_printerclient = true;
        Self {
            mqtt: mqtt_client,
            ftps: None,
            timer,
            serial: String::from(serial),
            model,
            sequence_counter: INITIAL_SEQUENCE_ID,
            k_profile_primed: false,
            pending_messages: VecDeque::new(),
            command_timeout_secs: DEFAULT_COMMAND_TIMEOUT_SECS,
        }
    }
}

impl<IO, Timer, RawIO, Tls, Factory> PrinterClient<IO, Timer, RawIO, Tls, Factory>
where
    IO: AsyncIo,
    Timer: TimerProvider,
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: FtpDataStreamFactory<RawIO>,
{
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
        let msg = if let Some(buffered) = self.pending_messages.pop_front() {
            buffered
        } else {
            self.mqtt.poll_message().await?
        };
        match serde_json::from_slice::<TelemetryReport>(&msg.payload) {
            Ok(report) => Ok(TelemetryEvent::Report(Box::new(report), msg)),
            Err(_) => Ok(TelemetryEvent::Unknown(msg)),
        }
    }

    /// Pulls the next raw MQTT message without deserialization.
    ///
    /// Drains any internally buffered messages before reading from the wire.
    pub async fn poll_raw(&mut self) -> Result<MqttMessage, BambuError> {
        if let Some(buffered) = self.pending_messages.pop_front() {
            Ok(buffered)
        } else {
            self.mqtt.poll_message().await
        }
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
        let start = self.timer.now_millis();
        let timeout_ms = self.command_timeout_secs * 1000;
        let mut count: usize = 0;

        loop {
            let msg = self.mqtt.poll_message().await?;
            if let Some(result) = matcher(&msg) {
                return Ok(result);
            }
            self.pending_messages.push_back(msg);
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
        let payload = serde_json::to_vec(request).map_err(|_| BambuError::SerializationError)?;
        self.mqtt.publish_command(&payload).await
    }

    /// Requests a full state dump from the printer [REF-MQTT-LIFECYCLE].
    pub async fn request_pushall(&mut self) -> Result<u16, BambuError> {
        let seq = self.next_sequence_id();
        let req = crate::mqtt::PushAllRequest::new(seq);
        self.publish_request(&req).await
    }

    /// Dispatches a PINGREQ keep-alive frame to maintain connection liveness.
    pub async fn send_ping(&mut self) -> Result<(), BambuError> {
        self.mqtt.send_ping().await
    }

    /// Returns a reference to the printer's unique hardware serial number.
    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// Returns the resolved printer hardware model.
    pub fn model(&self) -> BambuModel {
        self.model
    }

    /// Returns direct access to the underlying [`BambuMqttClient`].
    ///
    /// Use this for sending custom MQTT payloads, managing zombie detection via
    /// [`tick_zombie_check()`](BambuMqttClient::tick_zombie_check), or inspecting
    /// in-flight state — anything that [`PrinterClient`] doesn't expose directly.
    ///
    /// Calling [`poll_telemetry()`](BambuMqttClient::poll_telemetry) on the returned
    /// client will log a warning — use [`PrinterClient::poll_telemetry()`](Self::poll_telemetry)
    /// or [`poll_raw()`](Self::poll_raw) instead to keep the internal message buffer
    /// consistent.
    pub fn mqtt(&mut self) -> &mut BambuMqttClient<IO> {
        &mut self.mqtt
    }
}
