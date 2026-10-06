//! Print lifecycle commands (pause, resume, stop, speed, skip objects, calibration).

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::Serialize;

use super::ClampedTaskId;
use crate::types::control::PrintSpeed;

/// A print-lifecycle command that carries nothing but its name and `sequence_id` [REF-MQTT-LIFECYCLE].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StandardCommand {
    /// Pause the running job (`pause`).
    Pause,
    /// Resume a paused job (`resume`).
    Resume,
    /// Stop the job (`stop`).
    Stop,
    /// Re-read the nozzle information (`refresh_nozzle`).
    RefreshNozzle,
    /// Turn off air purification (`close_air_filt`).
    CloseAirFilter,
    /// The error dialog's "stop drying" (`auto_stop_ams_dry`).
    AutoStopAmsDry,
}

impl StandardCommand {
    /// The wire command name.
    #[must_use]
    pub const fn as_wire(self) -> &'static str {
        match self {
            StandardCommand::Pause => "pause",
            StandardCommand::Resume => "resume",
            StandardCommand::Stop => "stop",
            StandardCommand::RefreshNozzle => "refresh_nozzle",
            StandardCommand::CloseAirFilter => "close_air_filt",
            StandardCommand::AutoStopAmsDry => "auto_stop_ams_dry",
        }
    }
}

/// General control payload used for pause, resume, stop and the other name-only commands.
#[derive(Debug, Clone, Serialize)]
pub struct StandardControlPayload {
    /// Wire command name — see [`StandardCommand`].
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Sends a name-only print lifecycle command (pause, resume, stop, ...) to the printer.
pub type StandardControlRequest = super::Print<StandardControlPayload>;

impl StandardControlRequest {
    /// Builds a request for `command`.
    pub fn new(command: StandardCommand, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: StandardControlPayload {
                command: command.as_wire(),
                sequence_id: sequence_id.into(),
            },
        }
    }

    /// Builds a `pause` request.
    pub fn pause(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self::new(StandardCommand::Pause, sequence_id)
    }

    /// Builds a `resume` request.
    pub fn resume(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self::new(StandardCommand::Resume, sequence_id)
    }

    /// Builds a `stop` request.
    pub fn stop(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self::new(StandardCommand::Stop, sequence_id)
    }
}

/// Instructs the printer to bypass rendering specific objects within active multi-model jobs.
#[derive(Debug, Clone, Serialize)]
pub struct SkipObjectsPayload {
    /// Wire command name, always `"skip_objects"`.
    pub command: &'static str,
    /// List of object indices (as sliced) to skip rendering.
    pub obj_list: Vec<u32>,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Tells the printer to skip specific objects in a multi-object print.
pub type SkipObjectsRequest = super::Print<SkipObjectsPayload>;

impl SkipObjectsRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "skip_objects";

    /// Builds a `skip_objects` request from a list of object indices to skip.
    pub fn new(object_indices: Vec<u32>, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: SkipObjectsPayload {
                command: Self::COMMAND,
                obj_list: object_indices,
                sequence_id: sequence_id.into(),
            },
        }
    }
}

/// Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].
#[derive(Debug, Clone, Serialize)]
pub struct CleanPrintErrorPayload {
    /// Wire command name, always `"clean_print_error"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Clears the printer's current error state so it can resume operation.
pub type CleanPrintErrorRequest = super::Print<CleanPrintErrorPayload>;

impl CleanPrintErrorRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "clean_print_error";

    /// Builds a `clean_print_error` request.
    pub fn new(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: CleanPrintErrorPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
            },
        }
    }
}

/// Error-dialog action carrying the fault it answers [REF-MQTT-LIFECYCLE].
///
/// One shape serves three commands, per BambuStudio's `command_hms_ignore`,
/// `command_hms_resume` and `command_hms_stop` (`DeviceManager.cpp`): `err`, `param: "reserve"`
/// and `job_id` alongside the command name. `err` is the code in *decimal* — BambuStudio passes
/// `std::to_string(m_error_code)` (`DeviceErrorDialog.cpp`); only `uiop` uses 8-digit hex.
#[derive(Debug, Clone, Serialize)]
pub struct HmsActionPayload {
    /// Wire command name: `"ignore"`, `"resume"` or `"stop"`.
    pub command: &'static str,
    /// The `print_error` code being answered, as a decimal string.
    pub err: String,
    /// Always `"reserve"`.
    pub param: &'static str,
    /// The current job's `job_id`, or empty when unknown (bambuddy sends `""` then).
    pub job_id: String,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Answers a paused print's error dialog: ignore the fault and resume, or resume/stop naming it.
pub type HmsActionRequest = super::Print<HmsActionPayload>;

impl HmsActionRequest {
    fn build(
        command: &'static str,
        error_code: u32,
        job_id: Option<&str>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            print: HmsActionPayload {
                command,
                err: error_code.to_string(),
                param: "reserve",
                job_id: String::from(job_id.unwrap_or_default()),
                sequence_id: sequence_id.into(),
            },
        }
    }

    /// Builds an `ignore` request: skip the next re-check of `error_code` and resume.
    ///
    /// Unlike a plain `resume` ("fixed it, re-check"), this stops a fault such as a wrong build
    /// plate from being re-detected and re-pausing the print a second later (bambuddy #1869).
    ///
    /// `job_id` is the running job's id, or `None` when no telemetry has carried one yet.
    pub fn ignore(
        error_code: u32,
        job_id: Option<&str>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self::build("ignore", error_code, job_id, sequence_id)
    }

    /// Builds an error-aware `resume` request, the form BambuStudio's error dialog sends.
    pub fn resume(
        error_code: u32,
        job_id: Option<&str>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self::build("resume", error_code, job_id, sequence_id)
    }

    /// Builds an error-aware `stop` request, the form BambuStudio's error dialog sends.
    pub fn stop(
        error_code: u32,
        job_id: Option<&str>,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self::build("stop", error_code, job_id, sequence_id)
    }
}

/// Dismisses a warning without resuming anything [REF-MQTT-LIFECYCLE].
#[derive(Debug, Clone, Serialize)]
pub struct IdleIgnorePayload {
    /// Wire command name, always `"idle_ignore"`.
    pub command: &'static str,
    /// The `print_error` code being dismissed, as a decimal string.
    pub err: String,
    /// `0` dismisses this occurrence; `1` suppresses the same warning permanently.
    #[serde(rename = "type")]
    pub ignore_type: u8,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Dismisses a non-pausing warning, once or permanently (BambuStudio `command_hms_idle_ignore`).
pub type IdleIgnoreRequest = super::Print<IdleIgnorePayload>;

/// How long an `idle_ignore` dismissal lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdleIgnoreScope {
    /// Dismiss this occurrence only (`type: 0`).
    Once,
    /// Never show this warning again (`type: 1`).
    Permanent,
}

impl IdleIgnoreRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "idle_ignore";

    /// Builds an `idle_ignore` request dismissing `error_code` for `scope`.
    pub fn new(
        error_code: u32,
        scope: IdleIgnoreScope,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            print: IdleIgnorePayload {
                command: Self::COMMAND,
                err: error_code.to_string(),
                ignore_type: match scope {
                    IdleIgnoreScope::Once => 0,
                    IdleIgnoreScope::Permanent => 1,
                },
                sequence_id: sequence_id.into(),
            },
        }
    }
}

/// Closes the printer's on-screen `print_error` dialog [REF-MQTT-LIFECYCLE].
#[derive(Debug, Clone, Serialize)]
pub struct UiopPayload {
    /// Wire command name, always `"uiop"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
    /// UI element family, always `"print_error"`.
    pub name: &'static str,
    /// Always `"close"`.
    pub action: &'static str,
    /// Sender: `0` the printer's own UI, `1` BambuStudio. bambino identifies as `1`.
    pub source: u8,
    /// Always `"dialog"`.
    #[serde(rename = "type")]
    pub ui_type: &'static str,
    /// The `print_error` code whose dialog to close, as 8 uppercase hex digits.
    pub err: String,
}

/// Closes the error dialog on the printer's screen (BambuStudio `command_clean_print_error_uiop`).
///
/// Separate from [`CleanPrintErrorRequest`], which clears the error latch: BambuStudio sends
/// this once whenever its own copy of the dialog closes.
pub type UiopRequest = super::System<UiopPayload>;

impl UiopRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "uiop";

    /// Builds a `uiop` request closing the dialog for `error_code`.
    pub fn close_print_error(error_code: u32, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            system: UiopPayload {
                command: Self::COMMAND,
                sequence_id: sequence_id.into(),
                name: "print_error",
                action: "close",
                source: 1,
                ui_type: "dialog",
                err: format!("{error_code:08X}"),
            },
        }
    }
}

/// Triggers automated physical resonance compensation sweeps and chassis alignments.
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationPayload {
    /// Wire command name, always `"calibration"`.
    pub command: &'static str,
    /// Calculated 32-bit active target parameter option bitmask [REF-MQTT-LIFECYCLE].
    pub option: u32,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Kicks off a calibration routine (vibration compensation, bed leveling, etc.).
pub type CalibrationRequest = super::Print<CalibrationPayload>;

impl CalibrationRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "calibration";

    /// Builds a `calibration` request from a capability option bitmask.
    pub fn new(option_bitmask: u32, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: CalibrationPayload {
                command: Self::COMMAND,
                option: option_bitmask,
                sequence_id: sequence_id.into(),
            },
        }
    }
}

/// Dynamically scales maximum movement velocity and acceleration limits.
#[derive(Debug, Clone, Serialize)]
pub struct PrintSpeedPayload {
    /// Wire command name, always `"print_speed"`.
    pub command: &'static str,
    /// Target speed scaling index serialized as string:
    /// * `"1"`: Silent Mode (50% limits).
    /// * `"2"`: Standard Mode (100% nominal).
    /// * `"3"`: Sport Mode (124% limits).
    /// * `"4"`: Ludicrous Mode (166% limits).
    pub param: String,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: ClampedTaskId,
}

/// Changes the active print speed profile (silent, standard, sport, ludicrous).
pub type PrintSpeedRequest = super::Print<PrintSpeedPayload>;

impl PrintSpeedRequest {
    /// Wire command name.
    pub const COMMAND: &'static str = "print_speed";

    /// Builds a `print_speed` request for `speed`.
    pub fn new(speed: PrintSpeed, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: PrintSpeedPayload {
                command: Self::COMMAND,
                param: speed.level().to_string(),
                sequence_id: sequence_id.into(),
            },
        }
    }
}
