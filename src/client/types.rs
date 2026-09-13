//! Client-facing enums and helper types (telemetry events, fan targets, print speed, calibration).

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

/// Enumeration representing target onboard cooling fans [REF-CLIM-FANS].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FanTarget {
    /// Primary part cooling fan (Port 1).
    PartCooling,
    /// Primary left-side auxiliary fan (Port 2).
    AuxiliaryLeft,
    /// Chamber exhaust/filtration fan (Port 3).
    ChamberExhaust,
    /// Secondary left-side auxiliary fan (Port 10, supported on X2D and P2S) [REF-CLIM-FANS].
    ///
    /// Despite the wire port number (M106 `P10`) and read-side airduct id (160) suggesting a
    /// "right" fan, BambuStudio's `DevFan.h` names decoded id 10 `FAN_REMOTE_COOLING_1_IDX` —
    /// a second left-side auxiliary fan, distinct from [`AuxiliaryLeft`](Self::AuxiliaryLeft)'s
    /// primary port-2 fan (`FAN_REMOTE_COOLING_0_IDX`, mirrored into `big_fan1_speed`).
    /// Confirmed against bambuddy's test suite, which titles this fan "P2S/X2D left auxiliary
    /// part cooling fan" throughout (issue #60).
    AuxiliaryLeft2,
}

// Write-side fan port IDs: `hardware.rs::set_fan_speed` uses these as M106 `P` arguments.
pub(crate) const FAN_WRITE_PORT_PART_COOLING: u16 = 1;
pub(crate) const FAN_WRITE_PORT_AUXILIARY_LEFT: u16 = 2;
pub(crate) const FAN_WRITE_PORT_CHAMBER_EXHAUST: u16 = 3;
pub(crate) const FAN_WRITE_PORT_AUXILIARY_LEFT2: u16 = 10;

/// Read-side telemetry port ID for the second left-side auxiliary fan (`device.airduct.parts[id]`), used by `telemetry.rs::auxiliary_left2_fan_speed`.
/// **Different address space from the write-side `FAN_WRITE_PORT_*` constants above** — this is not
/// a typo; write ports are M106 `P` arguments while read ports index the telemetry `airduct.parts`
/// array, and there is no compiler-enforced link between "this `FanTarget` variant" and both of its
/// port numbers today.
pub(crate) const FAN_READ_PORT_AUXILIARY_LEFT2: u32 = 160;

/// Buzzer alarm/attention chime mode for [`super::PrinterClient::set_buzzer_mode`] [REF-MQTT-LIFECYCLE].
/// Supported on models with a physical fire alarm buzzer (H2 series).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuzzerMode {
    /// Silent/disarmed.
    Silent = 0,
    /// Alarm triggered.
    Alarm = 1,
    /// Beeping attention chime.
    Chirp = 2,
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

impl PrintSpeed {
    /// Classifies a raw `spd_lvl` telemetry value (`1`-`4`, matching the same wire values [`PrinterClient::set_print_speed()`](crate::client::PrinterClient::set_print_speed) sends).
    /// Returns `None` for an out-of-range level.
    pub fn from_level(level: u8) -> Option<Self> {
        match level {
            1 => Some(PrintSpeed::Silent),
            2 => Some(PrintSpeed::Standard),
            3 => Some(PrintSpeed::Sport),
            4 => Some(PrintSpeed::Ludicrous),
            _ => None,
        }
    }
}

/// Bitmask flags for selecting hardware calibration routines [REF-MQTT-LIFECYCLE].
///
/// Combine flags with bitwise OR to trigger multiple calibration routines simultaneously
/// (e.g., `CalibrationOption::BED_LEVELING | CalibrationOption::VIBRATION_COMPENSATION`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CalibrationOption(pub u32);

impl CalibrationOption {
    /// Automatic bed mesh leveling.
    pub const BED_LEVELING: Self = Self(2);
    /// Input shaper vibration compensation tuning.
    pub const VIBRATION_COMPENSATION: Self = Self(4);
    /// Motor noise cancellation profiling.
    pub const MOTOR_NOISE_CANCELLATION: Self = Self(8);
    /// First-layer nozzle height calibration.
    pub const NOZZLE_HEIGHT: Self = Self(16);
    /// Heated bed thermal compensation mapping.
    pub const HEATBED_THERMAL: Self = Self(32);
}

impl core::ops::BitOr for CalibrationOption {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
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

/// Decoded classification of the printer's high-level `gcode_state` telemetry field.
///
/// `Unknown` covers both an unrecognized wire value and a missing field — callers
/// needing to tell those apart should inspect the raw `gcode_state` string directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrintStatus {
    /// No print job active or loaded (wire: `"IDLE"`).
    Idle,
    /// Print preparing to start — homing, bed leveling, or priming, physical
    /// motion in progress (wire: `"PREPARE"`).
    Preparing,
    /// Printer is slicing a job on-device, before any physical motion (wire: `"SLICING"`).
    ///
    /// Distinct from [`Preparing`](Self::Preparing): nothing is moving yet. It is still a
    /// busy state — a job is in flight — so treat it like the other active states when
    /// deciding whether the printer can accept new work.
    Slicing,
    /// Print job actively executing (wire: `"RUNNING"`).
    Running,
    /// Print job paused, resumable (wire: `"PAUSE"`).
    Paused,
    /// Print job completed successfully (wire: `"FINISH"`).
    Finished,
    /// Print job aborted by an error condition (wire: `"FAILED"`).
    Failed,
    /// Unrecognized wire value, or `gcode_state` field missing entirely — see the enum's doc comment.
    Unknown,
}

impl PrintStatus {
    /// Classifies a raw `gcode_state` wire value (firmware casing: `"IDLE"`, `"PREPARE"`, `"SLICING"`, `"RUNNING"`, `"PAUSE"`, `"FINISH"`, `"FAILED"` [REF-MQTT-IDLEBUG]).
    pub fn from_gcode_state(state: &str) -> Self {
        match state {
            "IDLE" => PrintStatus::Idle,
            "PREPARE" => PrintStatus::Preparing,
            "SLICING" => PrintStatus::Slicing,
            "RUNNING" => PrintStatus::Running,
            "PAUSE" => PrintStatus::Paused,
            "FINISH" => PrintStatus::Finished,
            "FAILED" => PrintStatus::Failed,
            _ => PrintStatus::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PrintStatus;

    #[test]
    fn test_print_status_from_gcode_state() {
        assert_eq!(PrintStatus::from_gcode_state("IDLE"), PrintStatus::Idle);
        assert_eq!(
            PrintStatus::from_gcode_state("RUNNING"),
            PrintStatus::Running
        );
        assert_eq!(PrintStatus::from_gcode_state("PAUSE"), PrintStatus::Paused);
        assert_eq!(
            PrintStatus::from_gcode_state("FINISH"),
            PrintStatus::Finished
        );
        assert_eq!(PrintStatus::from_gcode_state("FAILED"), PrintStatus::Failed);
        assert_eq!(
            PrintStatus::from_gcode_state("PREPARE"),
            PrintStatus::Preparing
        );
        assert_eq!(
            PrintStatus::from_gcode_state("SLICING"),
            PrintStatus::Slicing
        );
        assert_eq!(PrintStatus::from_gcode_state(""), PrintStatus::Unknown);
    }
}
