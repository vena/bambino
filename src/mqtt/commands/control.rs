//! Print lifecycle commands (pause, resume, stop, speed, skip objects, calibration).

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::Serialize;

/// General control payload used for pause, resume, stop, and clean actions.
#[derive(Debug, Clone, Serialize)]
pub struct StandardControlPayload {
    pub command: String,
    pub sequence_id: String,
}

/// Sends a print lifecycle command (pause, resume, stop) to the printer.
#[derive(Debug, Clone, Serialize)]
pub struct StandardControlRequest {
    pub print: StandardControlPayload,
}

impl StandardControlRequest {
    pub fn new(command: &str, sequence_id: u64) -> Self {
        Self {
            print: StandardControlPayload {
                command: String::from(command),
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Instructs the printer to bypass rendering specific objects within active multi-model jobs.
#[derive(Debug, Clone, Serialize)]
pub struct SkipObjectsPayload {
    pub command: &'static str,
    pub obj_list: Vec<u32>,
    pub sequence_id: String,
}

/// Tells the printer to skip specific objects in a multi-object print.
#[derive(Debug, Clone, Serialize)]
pub struct SkipObjectsRequest {
    pub print: SkipObjectsPayload,
}

impl SkipObjectsRequest {
    pub fn new(object_indices: Vec<u32>, sequence_id: u64) -> Self {
        Self {
            print: SkipObjectsPayload {
                command: "skip_objects",
                obj_list: object_indices,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Clears active error codes from the printer's diagnostic fault register [REF-MQTT-LIFECYCLE].
#[derive(Debug, Clone, Serialize)]
pub struct CleanPrintErrorPayload {
    pub command: &'static str,
    pub sequence_id: String,
}

/// Clears the printer's current error state so it can resume operation.
#[derive(Debug, Clone, Serialize)]
pub struct CleanPrintErrorRequest {
    pub print: CleanPrintErrorPayload,
}

impl CleanPrintErrorRequest {
    pub fn new(sequence_id: u64) -> Self {
        Self {
            print: CleanPrintErrorPayload {
                command: "clean_print_error",
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Triggers automated physical resonance compensation sweeps and chassis alignments.
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationPayload {
    pub command: &'static str,
    /// Calculated 32-bit active target parameter option bitmask [REF-MQTT-LIFECYCLE].
    pub option: u32,
    pub sequence_id: String,
}

/// Kicks off a calibration routine (vibration compensation, bed leveling, etc.).
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationRequest {
    pub print: CalibrationPayload,
}

impl CalibrationRequest {
    pub fn new(option_bitmask: u32, sequence_id: u64) -> Self {
        Self {
            print: CalibrationPayload {
                command: "calibration",
                option: option_bitmask,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Dynamically scales maximum movement velocity and acceleration limits.
#[derive(Debug, Clone, Serialize)]
pub struct PrintSpeedPayload {
    pub command: &'static str,
    /// Target speed scaling index serialized as string:
    /// * `"1"`: Silent Mode (50% limits).
    /// * `"2"`: Standard Mode (100% nominal).
    /// * `"3"`: Sport Mode (124% limits).
    /// * `"4"`: Ludicrous Mode (166% limits).
    pub param: String,
    pub sequence_id: String,
}

/// Changes the active print speed profile (silent, standard, sport, ludicrous).
#[derive(Debug, Clone, Serialize)]
pub struct PrintSpeedRequest {
    pub print: PrintSpeedPayload,
}

impl PrintSpeedRequest {
    pub fn new(speed_index_str: &str, sequence_id: u64) -> Self {
        Self {
            print: PrintSpeedPayload {
                command: "print_speed",
                param: String::from(speed_index_str),
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}
