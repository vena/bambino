//! Print lifecycle commands (pause, resume, stop, speed, skip objects, calibration).

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::Serialize;

use super::ClampedTaskId;

/// General control payload used for pause, resume, stop, and clean actions.
#[derive(Debug, Clone, Serialize)]
pub struct StandardControlPayload {
    /// Wire command name ("pause", "resume", "stop", etc.), a dynamic string rather than `&'static str`.
    pub command: String,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
}

/// Sends a print lifecycle command (pause, resume, stop) to the printer.
#[derive(Debug, Clone, Serialize)]
pub struct StandardControlRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: StandardControlPayload,
}

impl StandardControlRequest {
    /// Builds a control request for the given lifecycle command string ("pause", "resume", "stop").
    pub fn new(command: &str, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: StandardControlPayload {
                command: String::from(command),
                sequence_id: sequence_id.into().to_string(),
            },
        }
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
    pub sequence_id: String,
}

/// Tells the printer to skip specific objects in a multi-object print.
#[derive(Debug, Clone, Serialize)]
pub struct SkipObjectsRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: SkipObjectsPayload,
}

impl SkipObjectsRequest {
    /// Builds a `skip_objects` request from a list of object indices to skip.
    pub fn new(object_indices: Vec<u32>, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: SkipObjectsPayload {
                command: "skip_objects",
                obj_list: object_indices,
                sequence_id: sequence_id.into().to_string(),
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
    pub sequence_id: String,
}

/// Clears the printer's current error state so it can resume operation.
#[derive(Debug, Clone, Serialize)]
pub struct CleanPrintErrorRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: CleanPrintErrorPayload,
}

impl CleanPrintErrorRequest {
    /// Builds a `clean_print_error` request.
    pub fn new(sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: CleanPrintErrorPayload {
                command: "clean_print_error",
                sequence_id: sequence_id.into().to_string(),
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
    pub sequence_id: String,
}

/// Answers a paused print's error dialog: ignore the fault and resume, or resume/stop naming it.
#[derive(Debug, Clone, Serialize)]
pub struct HmsActionRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: HmsActionPayload,
}

impl HmsActionRequest {
    fn build(
        command: &'static str,
        error_code: u32,
        job_id: &str,
        sequence_id: impl Into<ClampedTaskId>,
    ) -> Self {
        Self {
            print: HmsActionPayload {
                command,
                err: error_code.to_string(),
                param: "reserve",
                job_id: String::from(job_id),
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }

    /// Builds an `ignore` request: skip the next re-check of `error_code` and resume.
    ///
    /// Unlike a plain `resume` ("fixed it, re-check"), this stops a fault such as a wrong build
    /// plate from being re-detected and re-pausing the print a second later (bambuddy #1869).
    pub fn ignore(error_code: u32, job_id: &str, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self::build("ignore", error_code, job_id, sequence_id)
    }

    /// Builds an error-aware `resume` request, the form BambuStudio's error dialog sends.
    pub fn resume(error_code: u32, job_id: &str, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self::build("resume", error_code, job_id, sequence_id)
    }

    /// Builds an error-aware `stop` request, the form BambuStudio's error dialog sends.
    pub fn stop(error_code: u32, job_id: &str, sequence_id: impl Into<ClampedTaskId>) -> Self {
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
    pub sequence_id: String,
}

/// Dismisses a non-pausing warning, once or permanently (BambuStudio `command_hms_idle_ignore`).
#[derive(Debug, Clone, Serialize)]
pub struct IdleIgnoreRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: IdleIgnorePayload,
}

impl IdleIgnoreRequest {
    /// Builds an `idle_ignore` request; `persistent` selects `type: 1` (never show again).
    pub fn new(error_code: u32, persistent: bool, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: IdleIgnorePayload {
                command: "idle_ignore",
                err: error_code.to_string(),
                ignore_type: u8::from(persistent),
                sequence_id: sequence_id.into().to_string(),
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
    pub sequence_id: String,
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
#[derive(Debug, Clone, Serialize)]
pub struct UiopRequest {
    /// The `system` namespace envelope required by the wire protocol.
    pub system: UiopPayload,
}

impl UiopRequest {
    /// Builds a `uiop` request closing the dialog for `error_code`.
    pub fn close_print_error(error_code: u32, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            system: UiopPayload {
                command: "uiop",
                sequence_id: sequence_id.into().to_string(),
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
    pub sequence_id: String,
}

/// Kicks off a calibration routine (vibration compensation, bed leveling, etc.).
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: CalibrationPayload,
}

impl CalibrationRequest {
    /// Builds a `calibration` request from a capability option bitmask.
    pub fn new(option_bitmask: u32, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: CalibrationPayload {
                command: "calibration",
                option: option_bitmask,
                sequence_id: sequence_id.into().to_string(),
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
    pub sequence_id: String,
}

/// Changes the active print speed profile (silent, standard, sport, ludicrous).
#[derive(Debug, Clone, Serialize)]
pub struct PrintSpeedRequest {
    /// The `print` namespace envelope required by the wire protocol.
    pub print: PrintSpeedPayload,
}

impl PrintSpeedRequest {
    /// Builds a `print_speed` request from a stringified speed index.
    pub fn new(speed_index_str: &str, sequence_id: impl Into<ClampedTaskId>) -> Self {
        Self {
            print: PrintSpeedPayload {
                command: "print_speed",
                param: String::from(speed_index_str),
                sequence_id: sequence_id.into().to_string(),
            },
        }
    }
}
