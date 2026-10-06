//! Client-facing helper types (telemetry events, print progress, calibration options).
//!
//! The control enums `PrinterClient` takes (`FanTarget`, `PrintSpeed`, ...) live in
//! [`crate::types::control`] and are re-exported from [`crate::client`].

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;

#[allow(unused_imports)] // doc links only
use super::command::CommandOutcome;
use super::command::CommandResolution;
use crate::mqtt::MqttMessage;
use crate::types::TelemetryReport;

/// Typed telemetry event from the printer's MQTT channel.
///
/// The library deserializes wire payloads into structured types so consumers don't
/// have to reimplement JSON parsing and model-quirk handling. Raw access is always
/// available via [`into_raw`](TelemetryEvent::into_raw).
#[derive(Debug, Clone)]
pub enum TelemetryEvent {
    /// State telemetry update (print status, device hardware, or both).
    Report(Box<TelemetryReport>, MqttMessage),
    /// The terminal outcome of a command this client published.
    ///
    /// Carries the echo that decided it, or `None` for an outcome no message produced
    /// ([`CommandOutcome::TimedOut`], [`CommandOutcome::ConnectionLost`]).
    Command(CommandResolution, Option<MqttMessage>),
    /// Payload that didn't match any known telemetry structure, including command echoes for
    /// `sequence_id`s this client did not send (other clients share the report topic).
    Unknown(MqttMessage),
}

impl TelemetryEvent {
    /// Consumes the event and returns the underlying raw MQTT message, if one produced it.
    ///
    /// `None` only for a [`Command`](Self::Command) outcome that no message produced.
    pub fn into_raw(self) -> Option<MqttMessage> {
        match self {
            Self::Report(_, raw) | Self::Unknown(raw) => Some(raw),
            Self::Command(_, raw) => raw,
        }
    }

    /// Returns a reference to the underlying raw MQTT message, if one produced it.
    ///
    /// `None` only for a [`Command`](Self::Command) outcome that no message produced.
    pub fn raw(&self) -> Option<&MqttMessage> {
        match self {
            Self::Report(_, raw) | Self::Unknown(raw) => Some(raw),
            Self::Command(_, raw) => raw.as_ref(),
        }
    }

    /// Returns the typed report if this is a `Report` variant.
    pub fn report(&self) -> Option<&TelemetryReport> {
        match self {
            Self::Report(report, _) => Some(report),
            Self::Command(..) | Self::Unknown(_) => None,
        }
    }

    /// Returns the command resolution if this is a `Command` variant.
    pub fn command(&self) -> Option<&CommandResolution> {
        match self {
            Self::Command(resolution, _) => Some(resolution),
            Self::Report(..) | Self::Unknown(_) => None,
        }
    }
}

/// Cached print-progress snapshot as of the last-observed telemetry carrying any of these fields (via [`poll_telemetry()`](crate::client::PrinterClient::poll_telemetry)).
///
/// Bundled into one struct rather than four separate cached scalars (unlike `home_flag`/
/// `gcode_state`/`is_door_open`/`print_error`, which answer four independent questions) because
/// `mc_percent`, `mc_remaining_time`, `layer_num`, and `total_layers` are always consumed
/// together as one "how's the print going" question. Each field updates independently and
/// keeps its last-observed value across a telemetry message that omits it — a `None` field
/// means "never observed," not "printer reports zero/none."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PrintProgress {
    /// Motion controller progress percentage (0-100).
    pub percent: Option<i32>,
    /// Estimated remaining print duration, in seconds.
    ///
    /// Converted on ingest from `mc_remaining_time`, which the wire reports in **minutes**.
    pub remaining_secs: Option<i32>,
    /// Active layer progress tracker.
    pub layer_num: Option<i32>,
    /// Total layers within the sliced print pipeline.
    pub total_layers: Option<i32>,
}
