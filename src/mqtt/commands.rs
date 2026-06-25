//! # MQTT Command Payloads & Serialization Builders
//!
//! Provides the concrete data structures and serialization wrappers required to control
//! physical Bambu Lab printers over MQTTS Port 8883 [REF-MQTT-LIFECYCLE].
//!
//! Handles complex polymorphic rules such as the string-vs-array mapping schemas for the
//! `ams_mapping` parameter, and enforces safety bounds on task identities.
//!
//! ## Architectural Alignment
//! * **Polymorphic Mapping Rules [REF-MQTT-LIFECYCLE]:** Handles conditional typing for
//!   material mappings, where inactive AMS sessions must present as empty strings while active
//!   sessions require integer arrays.
//! * **Task-ID Overflow Prevention [REF-MQTT-ENV]:** Clamps all generated sequence identifiers
//!   to 32-bit signed integer limits to prevent memory allocation overflows on hardware boards.

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::Serialize;

use crate::models::BambuModel;

pub(crate) const TASK_ID_MAX: u64 = i32::MAX as u64;

/// Clamps a 64-bit transaction or tracking identifier (typically standard UNIX epoch
/// milliseconds) within the strict boundary limits of a 32-bit signed integer (`2147483647`).
///
/// **Why this is critical [REF-MQTT-ENV]:**
/// The printer's onboard G-code parsing routine clamps subtask identifiers to standard 32-bit
/// signed integer limits. If a connecting client uses an un-clamped millisecond epoch (13-digit integer),
/// the memory allocation registers on the motion board will overflow. This causes the printer to lock
/// indefinitely in an `IDLE` state and reject all subsequent print dispatches.
pub fn clamp_task_id(raw_id: u64) -> u32 {
    (raw_id % TASK_ID_MAX) as u32
}

// ============================================================================
// 1. Status & Information Queries
// ============================================================================

/// Payload schema to trigger a complete state dump ("pushall") from the printer.
#[derive(Debug, Clone, Serialize)]
pub struct PushAllPayload {
    pub command: &'static str,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PushAllRequest {
    pub pushing: PushAllPayload,
}

impl PushAllRequest {
    pub fn new(sequence_id: u64) -> Self {
        Self {
            pushing: PushAllPayload {
                command: "pushall",
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Payload schema to retrieve hardware/firmware version strings from the expansion bus.
#[derive(Debug, Clone, Serialize)]
pub struct GetVersionPayload {
    pub command: &'static str,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetVersionRequest {
    pub info: GetVersionPayload,
}

impl GetVersionRequest {
    pub fn new(sequence_id: u64) -> Self {
        Self {
            info: GetVersionPayload {
                command: "get_version",
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

// ============================================================================
// 2. Structural G-Code Enveloping
// ============================================================================

/// Queues raw G-code strings directly to the printer's motion execution controller.
///
/// Under the Bambu protocol specification, physical moves, manual extrusions, and
/// temperature targets are issued by packing standard G-code lines into this wrapper.
#[derive(Debug, Clone, Serialize)]
pub struct GCodePayload {
    pub command: &'static str,
    pub param: String,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GCodeRequest {
    pub print: GCodePayload,
}

impl GCodeRequest {
    /// Creates a request envelope wrapping a raw G-code payload.
    ///
    /// **Execution Note:** The raw G-code string is strictly appended with a newline character (`\n`)
    /// to ensure the physical controller's stream parser identifies the end-of-command boundary.
    pub fn new(gcode_line: &str, sequence_id: u64) -> Self {
        let mut param = String::from(gcode_line);
        if !param.ends_with('\n') {
            param.push('\n');
        }
        Self {
            print: GCodePayload {
                command: "gcode_line",
                param,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

// ============================================================================
// 3. Print Queue Lifecycle Management
// ============================================================================

/// General control payload used for pause, resume, stop, and clean actions.
#[derive(Debug, Clone, Serialize)]
pub struct StandardControlPayload {
    pub command: String,
    pub sequence_id: String,
}

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

// ============================================================================
// 4. Submit Print Job (project_file Dispatch)
// ============================================================================

/// Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE].
///
/// Replaces the positional parameter list on `start_print()` and `ProjectFileRequest::new()`
/// with named fields and sensible defaults for calibration flags.
#[derive(Debug, Clone)]
pub struct PrintJobConfig {
    pub job_filename: String,
    pub plate_gcode_path: String,
    pub subtask_name: String,
    pub raw_subtask_id: u64,
    pub bed_type: String,
    pub bed_leveling: bool,
    pub run_flow_calibration: bool,
    pub run_vibration_compensation: bool,
    pub timelapse: bool,
    pub layer_inspect: bool,
    /// `None` defers to the quirks engine default in `PrinterClient::start_print()`.
    pub nozzle_offset_cali: Option<bool>,
    pub use_ams: bool,
    pub ams_mapping: Vec<i32>,
    pub ams_mapping2: Option<Vec<ProjectAmsMapping2Entry>>,
}

impl PrintJobConfig {
    pub fn new(
        job_filename: &str,
        plate_gcode_path: &str,
        subtask_name: &str,
        raw_subtask_id: u64,
        bed_type: &str,
    ) -> Self {
        Self {
            job_filename: String::from(job_filename),
            plate_gcode_path: String::from(plate_gcode_path),
            subtask_name: String::from(subtask_name),
            raw_subtask_id,
            bed_type: String::from(bed_type),
            bed_leveling: true,
            run_flow_calibration: true,
            run_vibration_compensation: true,
            timelapse: true,
            layer_inspect: true,
            nozzle_offset_cali: None,
            use_ams: false,
            ams_mapping: Vec::new(),
            ams_mapping2: None,
        }
    }

    pub fn with_ams(mut self, mapping: Vec<i32>) -> Self {
        self.use_ams = true;
        self.ams_mapping = mapping;
        self
    }

    pub fn with_ams_mapping2(mut self, mapping2: Vec<ProjectAmsMapping2Entry>) -> Self {
        self.ams_mapping2 = Some(mapping2);
        self
    }

    pub fn bed_leveling(mut self, enabled: bool) -> Self {
        self.bed_leveling = enabled;
        self
    }

    pub fn flow_calibration(mut self, enabled: bool) -> Self {
        self.run_flow_calibration = enabled;
        self
    }

    pub fn vibration_compensation(mut self, enabled: bool) -> Self {
        self.run_vibration_compensation = enabled;
        self
    }

    pub fn timelapse(mut self, enabled: bool) -> Self {
        self.timelapse = enabled;
        self
    }

    pub fn layer_inspect(mut self, enabled: bool) -> Self {
        self.layer_inspect = enabled;
        self
    }

    pub fn nozzle_offset_calibration(mut self, enabled: bool) -> Self {
        self.nozzle_offset_cali = Some(enabled);
        self
    }
}

/// Represents the conditional, polymorphic typing needed for the `ams_mapping` key [REF-MQTT-LIFECYCLE].
///
/// **The Polymorphic Mapping Rule:**
/// * When `use_ams` is `false` (external spool mode), the key must serialize to an empty string `""`.
/// * When `use_ams` is `true` (AMS active mode), the key must serialize as an integer array (e.g. `[0, -1, 1]`).
///
/// Utilizing an untagged enum ensures standard JSON compliance across all execution profiles.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum AmsMappingTable {
    Inactive(String),
    Active(Vec<i32>),
}

/// Represents the detailed material and nozzle path pairing entries used inside the structured `ams_mapping2` array.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ProjectAmsMapping2Entry {
    pub ams_id: u8,
    pub slot_id: u8,
}

/// Payload layout to submit and execute a physical `.3mf` print from MicroSD card storage.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectFilePayload {
    pub command: &'static str,
    pub sequence_id: String,
    /// Target file path of the internal sliced plate payload (e.g. "Metadata/plate_1.gcode").
    pub param: String,
    /// User-friendly label associated with the print queue task.
    pub subtask_name: String,
    /// Unique 32-bit tracking identifier (Clamped to prevent overflow lockups).
    pub subtask_id: String,
    /// Sliced compilation container file path residing on the SD card (e.g., "job.3mf").
    pub file: String,
    /// Connection endpoint directory scheme (Must use `ftp://` for local loopback parsing) [REF-MQTT-LIFECYCLE].
    pub url: String,
    pub timelapse: bool,
    pub bed_type: String,
    pub bed_leveling: bool,
    /// Controls dynamic flow calibration. Expressed as an integer: `1` for active, `0` for bypass.
    pub extrude_cali_flag: i32,
    /// Active nozzle offset verification flag (Used primarily on IDEX and tool-changers).
    pub nozzle_offset_cali: i32,
    pub vibration_cali: bool,
    pub layer_inspect: bool,
    /// Triggers physical AMS multiplexer material routing. Must strictly be serialized as a boolean.
    pub use_ams: bool,
    /// Polymorphic representation enforcing empty strings on external spools vs integer arrays on standard channels.
    pub ams_mapping: AmsMappingTable,
    /// Structured sub-mappings for advanced material and multi-AMS routing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ams_mapping2: Option<Vec<ProjectAmsMapping2Entry>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectFileRequest {
    pub print: ProjectFilePayload,
}

impl ProjectFileRequest {
    /// Constructs a print job request from a `PrintJobConfig`, model, and sequence ID.
    ///
    /// When `nozzle_offset_cali` is `None`, defaults to the model's quirks-engine value
    /// via `supports_nozzle_offset_calibration()` — enabling it automatically on IDEX
    /// and tool-changer platforms.
    ///
    /// **Polymorphic Warning [REF-MQTT-LIFECYCLE]:**
    /// `use_ams` is serialized strictly as a JSON boolean. On dual-nozzle IDEX systems,
    /// serializing this field as an integer (e.g., `1` / `0`) causes the printer's JSON engine
    /// to treat the value as the physical carriage index (Target nozzle 1) instead of material
    /// routing parameters.
    pub fn from_config(config: &PrintJobConfig, sequence_id: u64, model: BambuModel) -> Self {
        let url = format!("ftp://{}", config.job_filename);

        let mapping = if config.use_ams {
            AmsMappingTable::Active(config.ams_mapping.clone())
        } else {
            AmsMappingTable::Inactive(String::new())
        };

        let nozzle_offset = config
            .nozzle_offset_cali
            .unwrap_or_else(|| model.quirks().supports_nozzle_offset_calibration());

        Self {
            print: ProjectFilePayload {
                command: "project_file",
                sequence_id: sequence_id.to_string(),
                param: config.plate_gcode_path.clone(),
                subtask_name: config.subtask_name.clone(),
                subtask_id: clamp_task_id(config.raw_subtask_id).to_string(),
                file: config.job_filename.clone(),
                url,
                timelapse: config.timelapse,
                bed_type: config.bed_type.clone(),
                bed_leveling: config.bed_leveling,
                extrude_cali_flag: if config.run_flow_calibration { 1 } else { 0 },
                nozzle_offset_cali: if nozzle_offset { 1 } else { 0 },
                vibration_cali: config.run_vibration_compensation,
                layer_inspect: config.layer_inspect,
                use_ams: config.use_ams,
                ams_mapping: mapping,
                ams_mapping2: config.ams_mapping2.clone(),
            },
        }
    }
}

// ============================================================================
// 5. Hardware Subsystem & Climate Control Commands
// ============================================================================

/// Chamber illumination and toolhead LED control configurations.
#[derive(Debug, Clone, Serialize)]
pub struct LedCtrlPayload {
    pub command: &'static str,
    pub sequence_id: String,
    /// Targets specific physical fixtures (e.g. "chamber_light", "chamber_light2").
    pub led_node: String,
    /// Mode state transitions (e.g., "on", "off", "flashing").
    pub led_mode: String,
    pub led_on_time: u32,
    pub led_off_time: u32,
    pub loop_times: u32,
    pub interval_time: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LedCtrlRequest {
    pub system: LedCtrlPayload,
}

impl LedCtrlRequest {
    pub fn new(led_node: &str, turn_on: bool, sequence_id: u64) -> Self {
        Self {
            system: LedCtrlPayload {
                command: "ledctrl",
                sequence_id: sequence_id.to_string(),
                led_node: String::from(led_node),
                led_mode: String::from(if turn_on { "on" } else { "off" }),
                led_on_time: 0,
                led_off_time: 0,
                loop_times: 0,
                interval_time: 0,
            },
        }
    }
}

/// Redirects internal climate airflows using active damper deflection plates.
#[derive(Debug, Clone, Serialize)]
pub struct AirductPayload {
    pub command: &'static str,
    /// `0` represents cooling mode (recirculation), `1` represents heating mode (exhaust) [REF-MQTT-LIFECYCLE].
    #[serde(rename = "modeId")]
    pub mode_id: i32,
    pub submode: i32,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AirductRequest {
    pub print: AirductPayload,
}

impl AirductRequest {
    pub fn new(recirculate_air: bool, sequence_id: u64) -> Self {
        Self {
            print: AirductPayload {
                command: "set_airduct",
                mode_id: if recirculate_air { 0 } else { 1 },
                submode: -1,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Controls structural notification sound output via speakers (Supported on A1 and H2D series only).
#[derive(Debug, Clone, Serialize)]
pub struct PromptSoundPayload {
    pub command: &'static str,
    pub sound_enable: bool,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PromptSoundRequest {
    pub print: PromptSoundPayload,
}

impl PromptSoundRequest {
    pub fn new(enable: bool, sequence_id: u64) -> Self {
        Self {
            print: PromptSoundPayload {
                command: "print_option",
                sound_enable: enable,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Modifies active alarm or attention chime parameters on the printer cabinet buzzer module.
#[derive(Debug, Clone, Serialize)]
pub struct BuzzerPayload {
    pub command: &'static str,
    /// Alarm state representation: `0` (Silent), `1` (Alarm), `2` (Chirp/Beep) [REF-MQTT-LIFECYCLE].
    pub mode: i32,
    pub reason: &'static str,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BuzzerRequest {
    pub print: BuzzerPayload,
}

impl BuzzerRequest {
    pub fn new(mode_code: i32, sequence_id: u64) -> Self {
        Self {
            print: BuzzerPayload {
                command: "buzzer_ctrl",
                mode: mode_code,
                reason: "",
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

// ============================================================================
// 6. Filament Configuration, Scanning & Feeding (AMS Control)
// ============================================================================

/// Overwrites physical attributes or custom slicer presets assigned to a specific tray.
#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentSettingPayload {
    pub command: &'static str,
    pub sequence_id: String,
    pub ams_id: i32,
    pub tray_id: i32,
    /// Standard filament preset index code (e.g. "GFL05" / "PF12345678901234567") [REF-AMS-SP_CFG].
    pub tray_info_idx: String,
    pub tray_type: String,
    pub tray_sub_brands: String,
    /// Structural hexadecimal color in RRGGBBAA format (e.g., "FFFF00FF").
    pub tray_color: String,
    pub nozzle_temp_min: u32,
    pub nozzle_temp_max: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentSettingRequest {
    pub print: AmsFilamentSettingPayload,
}

impl AmsFilamentSettingRequest {
    /// Creates a request payload to update slot parameters.
    ///
    /// **Polymorphic Tray Rule [REF-MQTT-LIFECYCLE]:**
    /// For standard physical slots, `ams_id` matches the expansion unit index (0-3).
    /// For the single-nozzle external spool slot, `ams_id` must strictly be set to `255`
    /// and `tray_id` must strictly be set to `254` to prevent command rejection.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ams_id: i32,
        tray_id: i32,
        preset_code: &str,
        material_type: &str,
        sub_brands: Option<&str>,
        color_hex: &str,
        temp_min: u32,
        temp_max: u32,
        sequence_id: u64,
    ) -> Self {
        let tray_sub_brands = match sub_brands {
            Some(s) => String::from(s),
            None => format!("{} Basic", material_type),
        };
        Self {
            print: AmsFilamentSettingPayload {
                command: "ams_filament_setting",
                sequence_id: sequence_id.to_string(),
                ams_id,
                tray_id,
                tray_info_idx: String::from(preset_code),
                tray_type: String::from(material_type),
                tray_sub_brands,
                tray_color: String::from(color_hex),
                nozzle_temp_min: temp_min,
                nozzle_temp_max: temp_max,
            },
        }
    }
}

/// Commands standard AMS controllers to resume, pause, or reset physical material feeds.
#[derive(Debug, Clone, Serialize)]
pub struct AmsControlPayload {
    pub command: &'static str,
    /// Target physical operation (e.g., "resume", "pause").
    pub param: String,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmsControlRequest {
    pub print: AmsControlPayload,
}

impl AmsControlRequest {
    pub fn new(operation: &str, sequence_id: u64) -> Self {
        Self {
            print: AmsControlPayload {
                command: "ams_control",
                param: String::from(operation),
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Triggers physical filament feeder movement to scan proprietary RFID tag properties.
#[derive(Debug, Clone, Serialize)]
pub struct AmsGetRfidPayload {
    pub command: &'static str,
    pub ams_id: i32,
    pub slot_id: i32,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmsGetRfidRequest {
    pub print: AmsGetRfidPayload,
}

impl AmsGetRfidRequest {
    pub fn new(ams_id: i32, slot_id: i32, sequence_id: u64) -> Self {
        Self {
            print: AmsGetRfidPayload {
                command: "ams_get_rfid",
                ams_id,
                slot_id,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Triggers filament load or unload sequences on physical AMS units or virtual external spools [REF-AMS-MAP].
#[derive(Debug, Clone, Serialize)]
pub struct AmsChangeFilamentPayload {
    pub command: &'static str,
    pub ams_id: i32,
    pub slot_id: i32,
    /// Load/unload destination (1 = toolhead load, 255 = unload/retract).
    pub target: i32,
    /// Current nozzle temperature (-1 = let firmware decide).
    pub curr_temp: i32,
    /// Target nozzle temperature (-1 = let firmware decide).
    pub tar_temp: i32,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmsChangeFilamentRequest {
    pub print: AmsChangeFilamentPayload,
}

impl AmsChangeFilamentRequest {
    pub fn new(
        ams_id: i32,
        slot_id: i32,
        target: i32,
        curr_temp: i32,
        tar_temp: i32,
        sequence_id: u64,
    ) -> Self {
        Self {
            print: AmsChangeFilamentPayload {
                command: "ams_change_filament",
                ams_id,
                slot_id,
                target,
                curr_temp,
                tar_temp,
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

/// Initiates or terminates dry-chamber heating cycles on AMS 2 Pro and AMS-HT units [REF-AMS-DRYER].
#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentDryingPayload {
    pub command: &'static str,
    pub ams_id: i32,
    /// 1 = start drying, 0 = stop drying.
    pub mode: i32,
    pub dry_temp: u32,
    /// Duration in **minutes** (e.g., 8-hour cycle = 480).
    pub dry_time: u32,
    pub rotate_tray: bool,
    pub filament: String,
    pub sequence_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AmsFilamentDryingRequest {
    pub print: AmsFilamentDryingPayload,
}

impl AmsFilamentDryingRequest {
    pub fn new(
        ams_id: i32,
        mode: i32,
        dry_temp: u32,
        dry_time: u32,
        rotate_tray: bool,
        filament: &str,
        sequence_id: u64,
    ) -> Self {
        Self {
            print: AmsFilamentDryingPayload {
                command: "ams_filament_drying",
                ams_id,
                mode,
                dry_temp,
                dry_time,
                rotate_tray,
                filament: String::from(filament),
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

// ============================================================================
// 7. Physical Self-Tests & Operational Performance Scaling
// ============================================================================

/// Triggers automated physical resonance compensation sweeps and chassis alignments.
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationPayload {
    pub command: &'static str,
    /// Calculated 32-bit active target parameter option bitmask [REF-MQTT-LIFECYCLE].
    pub option: u32,
    pub sequence_id: String,
}

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

#[derive(Debug, Clone, Serialize)]
pub struct PrintSpeedRequest {
    pub print: PrintSpeedPayload,
}

impl PrintSpeedRequest {
    pub(crate) fn new(speed_index_str: &str, sequence_id: u64) -> Self {
        Self {
            print: PrintSpeedPayload {
                command: "print_speed",
                param: String::from(speed_index_str),
                sequence_id: sequence_id.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_modulo_math() {
        let raw_epoch: u64 = 1718626458000;
        let clamped = clamp_task_id(raw_epoch);
        assert!(clamped <= i32::MAX as u32);
    }

    #[test]
    fn test_ams_mapping_polymorphism_inactive() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        );
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""ams_mapping":"""#));
    }

    #[test]
    fn test_ams_mapping_polymorphism_active() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .with_ams(vec![0, -1, 1]);
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::P1S);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""ams_mapping":[0,-1,1]"#));
    }

    #[test]
    fn test_nozzle_offset_cali_quirks_default_idex() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        );
        assert!(config.nozzle_offset_cali.is_none());

        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::X2D);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""nozzle_offset_cali":1"#));
    }

    #[test]
    fn test_nozzle_offset_cali_quirks_default_single_nozzle() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        );
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::P1S);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""nozzle_offset_cali":0"#));
    }

    #[test]
    fn test_nozzle_offset_cali_explicit_override() {
        let config = PrintJobConfig::new(
            "job.3mf",
            "Metadata/plate_1.gcode",
            "Test Print",
            12345,
            "textured",
        )
        .nozzle_offset_calibration(false);
        let req = ProjectFileRequest::from_config(&config, 5000, BambuModel::X2D);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""nozzle_offset_cali":0"#));
    }

    #[test]
    fn test_ams_change_filament_load_json() {
        let req = AmsChangeFilamentRequest::new(0, 1, 1, -1, -1, 40005);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"ams_change_filament"#));
        assert!(json.contains(r#""ams_id":0"#));
        assert!(json.contains(r#""slot_id":1"#));
        assert!(json.contains(r#""target":1"#));
        assert!(json.contains(r#""curr_temp":-1"#));
        assert!(json.contains(r#""tar_temp":-1"#));
    }

    #[test]
    fn test_ams_change_filament_unload_json() {
        let req = AmsChangeFilamentRequest::new(0, 255, 255, 210, 210, 40008);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""slot_id":255"#));
        assert!(json.contains(r#""target":255"#));
        assert!(json.contains(r#""curr_temp":210"#));
    }

    #[test]
    fn test_ams_filament_drying_json() {
        let req = AmsFilamentDryingRequest::new(128, 1, 55, 480, true, "PA-CF", 40004);
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"ams_filament_drying"#));
        assert!(json.contains(r#""ams_id":128"#));
        assert!(json.contains(r#""mode":1"#));
        assert!(json.contains(r#""dry_temp":55"#));
        assert!(json.contains(r#""dry_time":480"#));
        assert!(json.contains(r#""rotate_tray":true"#));
        assert!(json.contains(r#""filament":"PA-CF""#));
    }

    #[test]
    fn test_clean_print_error_json() {
        let req = CleanPrintErrorRequest::new(20010);
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(
            json,
            r#"{"print":{"command":"clean_print_error","sequence_id":"20010"}}"#
        );
    }
}
