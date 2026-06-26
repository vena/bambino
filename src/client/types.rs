#[cfg(not(feature = "std"))]
use alloc::boxed::Box;

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
    /// Payload that didn't match any known telemetry structure.
    Unknown(MqttMessage),
}

impl TelemetryEvent {
    /// Consumes the event and returns the underlying raw MQTT message.
    pub fn into_raw(self) -> MqttMessage {
        match self {
            Self::Report(_, raw) => raw,
            Self::Unknown(raw) => raw,
        }
    }

    /// Returns a reference to the underlying raw MQTT message.
    pub fn raw(&self) -> &MqttMessage {
        match self {
            Self::Report(_, raw) | Self::Unknown(raw) => raw,
        }
    }

    /// Returns the typed report if this is a `Report` variant.
    pub fn report(&self) -> Option<&TelemetryReport> {
        match self {
            Self::Report(report, _) => Some(report),
            Self::Unknown(_) => None,
        }
    }
}

/// Enumeration representing target onboard cooling fans [REF-CLIM-FANS].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FanTarget {
    /// Primary part cooling fan (Port 1).
    PartCooling,
    /// Primary left-side auxiliary fan (Port 2).
    AuxiliaryLeft,
    /// Chamber exhaust/filtration fan (Port 3).
    ChamberExhaust,
    /// Secondary right-side auxiliary fan (Port 10, specifically supported on X2D).
    AuxiliaryRight,
}

/// Velocity and acceleration scaling presets for active print jobs [REF-MQTT-LIFECYCLE].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintSpeed {
    /// 50% max acceleration and feedrate limits.
    Silent = 1,
    /// 100% nominal feedrate limit.
    Standard = 2,
    /// 124% nominal feedrate limit.
    Sport = 3,
    /// 166% nominal feedrate limit.
    Ludicrous = 4,
}

/// Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].
///
/// Combine flags with bitwise OR to trigger multiple calibration routines simultaneously
/// (e.g., `CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CalibrationOption(pub u32);

impl CalibrationOption {
    pub const BED_LEVELING: Self = Self(2);
    pub const VIBRATION_COMPENSATION: Self = Self(4);
    pub const MOTOR_NOISE_CANCELLATION: Self = Self(8);
    pub const NOZZLE_HEIGHT: Self = Self(16);
    pub const HEATBED_THERMAL: Self = Self(32);
}

impl core::ops::BitOr for CalibrationOption {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
