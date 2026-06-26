//! # Unified Printer Client Coordinator & Developer API
//!
//! Provides a high-level, platform-agnostic client interface designed to aggregate
//! MQTTS telemetry channels, implicit FTPS storage nodes, and video feeds under a
//! safe, unified controller boundary.
//!
//! ## Architectural Safety Interlocks
//! 1. **Bed-on-Z vs Bed-Slinger Homing [REF-MOTO-GCODE]:** Prevents structural nozzle
//!    collisions by enforcing bare `G28` homing commands on Bed-on-Z platforms (CoreXY),
//!    blocking dangerous partial homing parameters (such as `G28 Z`) that bypass safe parking.
//! 2. **Reference Mode Position Isolation:** Wraps relative movements on the Z-axis in
//!    travel-limit clamps (`M211 S1`) and coordinate push/pop boundaries (`M1002`) to insulate
//!    against mechanical bed crashes.
//! 3. **Chamber Thermal Guards [REF-THER-DECODE]:** Enforces capability checks prior to
//!    dispatching active heated chamber operations (`M141`), rejecting requests on open-frame models.
//! 4. **Auxiliary Fan Safety Routing [REF-CLIM-FANS]:** Directs fan cooling commands dynamically,
//!    handling secondary right-hand auxiliary fan controllers on specialized platforms.

mod ams;
pub mod dummy;
mod hardware;
mod motion;
mod print;
mod storage;
mod thermal;
pub mod types;

pub use dummy::{DummyFactory, DummyRawIo, DummyTimer, DummyTls};
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

/// A high-level, multi-platform coordinator client for Bambu Lab printers.
///
/// This struct wraps an active MQTT session and an optional FTPS file-system client.
/// Type parameters default to dummy implementations to allow lightweight MQTT-only deployment on
/// memory-constrained microcontrollers without violating recursive trait boundaries.
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
    /// Instantiates a standard, lightweight coordinate client wrapping an active MQTT session.
    pub fn new(mqtt_client: BambuMqttClient<IO>, serial: &str, model: BambuModel) -> Self {
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
    /// Instantiates a coordinator client with an integrated timer for command-response timeouts.
    pub fn new_with_timer(
        mqtt_client: BambuMqttClient<IO>,
        timer: Timer,
        serial: &str,
        model: BambuModel,
    ) -> Self {
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
    /// `get_version()` and `get_k_profiles()`.
    pub fn set_command_timeout(&mut self, secs: u64) {
        self.command_timeout_secs = secs;
    }

    /// Pulls the next available telemetry event from the MQTTS channel.
    ///
    /// Drains any internally buffered messages (from command-response round-trips)
    /// before reading from the wire.
    pub async fn poll_telemetry(&mut self) -> Result<TelemetryEvent, BambuError> {
        let msg = if let Some(buffered) = self.pending_messages.pop_front() {
            buffered
        } else {
            self.mqtt.poll_telemetry().await?
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
            self.mqtt.poll_telemetry().await
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
            let msg = self.mqtt.poll_telemetry().await?;
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
}
