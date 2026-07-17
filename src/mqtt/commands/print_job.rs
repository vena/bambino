//! Print job dispatch (file selection, AMS material mapping, plate/timelapse config).

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::Serialize;

use crate::ams::mapping::{AmsMapping2Entry, flat_channel_id_for_entry};
use crate::ams::{is_external_spool_safety_valid, is_external_spool_safety_valid_flat};
use crate::models::PrinterModel;

use super::ClampedTaskId;

/// Structured configuration for submitting a print job [REF-MQTT-LIFECYCLE].
///
/// Replaces the positional parameter list on `start_print()` and `ProjectFileRequest::new()`
/// with named fields and sensible defaults for calibration flags.
#[derive(Debug, Clone)]
pub struct PrintJobConfig {
    /// Filename of the `.3mf` file on SD card storage (e.g. "job.3mf").
    pub job_filename: String,
    /// Sliced plate gcode path inside the `.3mf` (e.g. "Metadata/plate_1.gcode").
    pub plate_gcode_path: String,
    /// User-friendly label for the print queue task.
    pub subtask_name: String,
    /// Unique 32-bit tracking identifier before clamping (see `ClampedTaskId`).
    pub raw_subtask_id: u64,
    /// Bed plate type (e.g. "textured", "smooth").
    pub bed_type: String,
    /// Whether to run automatic bed leveling before the print.
    pub bed_leveling: bool,
    /// Whether to run dynamic flow calibration before the print.
    pub run_flow_calibration: bool,
    /// Whether to run vibration compensation calibration before the print.
    pub run_vibration_compensation: bool,
    /// Whether timelapse capture is enabled.
    pub timelapse: bool,
    /// Whether to run first-layer inspection during the print.
    pub layer_inspect: bool,
    /// `None` defers to the quirks engine default in `PrinterClient::start_print()`.
    pub nozzle_offset_cali: Option<bool>,
    /// Whether to route filament through the AMS rather than an external spool.
    pub use_ams: bool,
    /// Flat AMS slot mapping (one entry per plate object, -1 = no AMS slot).
    pub ams_mapping: Vec<i32>,
    /// Structured per-nozzle AMS mapping; takes precedence over `ams_mapping` when set.
    pub ams_mapping2: Option<Vec<AmsMapping2Entry>>,
}

impl PrintJobConfig {
    /// Builds a job config with calibration flags defaulted on and AMS disabled.
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

    /// Enables AMS and sets the flat slot-mapping array (`ams_mapping`).
    #[must_use]
    pub fn with_ams(mut self, mapping: Vec<i32>) -> Self {
        self.use_ams = true;
        self.ams_mapping = mapping;
        self
    }

    /// Enables AMS with structured per-nozzle sub-mappings (`ams_mapping2`).
    #[must_use]
    pub fn with_ams_mapping2(mut self, mapping2: Vec<AmsMapping2Entry>) -> Self {
        self.use_ams = true;
        self.ams_mapping2 = Some(mapping2);
        self
    }

    /// Enables or disables automatic bed leveling for this job.
    pub fn bed_leveling(mut self, enabled: bool) -> Self {
        self.bed_leveling = enabled;
        self
    }

    /// Enables or disables flow calibration for this job.
    pub fn flow_calibration(mut self, enabled: bool) -> Self {
        self.run_flow_calibration = enabled;
        self
    }

    /// Enables or disables vibration compensation calibration for this job.
    pub fn vibration_compensation(mut self, enabled: bool) -> Self {
        self.run_vibration_compensation = enabled;
        self
    }

    /// Enables or disables timelapse capture for this job.
    pub fn timelapse(mut self, enabled: bool) -> Self {
        self.timelapse = enabled;
        self
    }

    /// Enables or disables first-layer inspection for this job.
    pub fn layer_inspect(mut self, enabled: bool) -> Self {
        self.layer_inspect = enabled;
        self
    }

    /// Overrides the model's default nozzle-offset-calibration behavior for this job.
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
    /// External-spool mode: serializes to an empty string.
    Inactive(String),
    /// AMS active mode: serializes to an integer slot-mapping array.
    Active(Vec<i32>),
}

/// Payload layout to submit and execute a physical `.3mf` print from MicroSD card storage.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectFilePayload {
    /// Wire command name, always `"project_file"`.
    pub command: &'static str,
    /// Request sequence ID, serialized as a string on the wire.
    pub sequence_id: String,
    /// Target file path of the internal sliced plate payload (e.g. "Metadata/plate_1.gcode").
    pub param: String,
    /// User-friendly label associated with the print queue task.
    pub subtask_name: String,
    /// Unique 32-bit tracking identifier (Clamped to prevent overflow lockups).
    pub subtask_id: String,
    /// Dynamic flow (pressure advance) calibration flag, duplicating `extrude_cali_flag` under
    /// its own key. bambuddy cites a real production incident (#1478) where a
    /// consumer relying on the wrong one of these two calibration flags silently skipped
    /// calibration — both are sent so no observer can pick the wrong field.
    pub flow_cali: bool,
    /// Slicer preset profile ID. Always `"0"` — confirmed against bambuddy and pybambu, both
    /// of which hardcode this value; no observed non-zero case.
    pub profile_id: String,
    /// Per-submission project tracking ID. Set equal to `subtask_id` — bambuddy's
    /// `send_start_print_command` (`bambu_mqtt.py:3721-3781`) mints one fresh ID per
    /// submission and reuses it for `subtask_id`/`project_id`/`task_id` alike; bambino's
    /// `subtask_id` already carries the same "fresh per submission" contract via its own doc
    /// comment, so reusing it here satisfies the same invariant bambuddy's fix relies on
    /// (avoiding the task-continuation firmware bug, #1042/#1011) without inventing a second
    /// ID-minting mechanism.
    pub project_id: String,
    /// Per-submission task tracking ID. See `project_id`'s doc comment — same value, same
    /// reasoning.
    pub task_id: String,
    /// Sliced compilation container file path residing on the SD card (e.g., "job.3mf").
    pub file: String,
    /// Connection endpoint directory scheme (Must use `ftp://` for local loopback parsing) [REF-MQTT-LIFECYCLE].
    pub url: String,
    /// Whether timelapse capture is enabled for this job.
    pub timelapse: bool,
    /// Bed plate type used for the print (e.g. "textured", "smooth").
    pub bed_type: String,
    /// Whether to run automatic bed leveling before the print.
    pub bed_leveling: bool,
    /// Controls dynamic flow calibration. Expressed as an integer: `1` for active, `0` for bypass.
    pub extrude_cali_flag: i32,
    /// Active nozzle offset verification flag (Used primarily on IDEX and tool-changers).
    pub nozzle_offset_cali: i32,
    /// Whether vibration compensation calibration ran as part of this job.
    pub vibration_cali: bool,
    /// Whether layer inspection (first-layer scan) ran as part of this job.
    pub layer_inspect: bool,
    /// Triggers physical AMS multiplexer material routing. Must strictly be serialized as a boolean.
    pub use_ams: bool,
    /// Polymorphic representation enforcing empty strings on external spools vs integer arrays on standard channels.
    pub ams_mapping: AmsMappingTable,
    /// Structured sub-mappings for advanced material and multi-AMS routing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ams_mapping2: Option<Vec<AmsMapping2Entry>>,
}

/// Submits a `.3mf` print job from the SD card for execution.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectFileRequest {
    /// The `print` namespace envelope required by the wire protocol.
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
    pub fn from_config(
        config: &PrintJobConfig,
        sequence_id: impl Into<ClampedTaskId>,
        model: PrinterModel,
    ) -> Self {
        let url = format!("ftp://{}", config.job_filename);

        let is_single_nozzle = model.quirks().physical_nozzle_count() == 1;
        let use_ams = config.use_ams
            && match &config.ams_mapping2 {
                Some(mapping2) => is_external_spool_safety_valid(is_single_nozzle, mapping2),
                None => is_external_spool_safety_valid_flat(is_single_nozzle, &config.ams_mapping),
            };
        // Derive the flat array from ams_mapping2 whenever it's the active source,
        // instead of trusting config.ams_mapping — with_ams_mapping2() alone never touches
        // ams_mapping, so a caller who only calls that builder previously got a populated
        // ams_mapping2 paired with an empty ams_mapping, breaking the documented 1:1 index
        // pairing the firmware relies on [REF-AMS-MAP].
        let flat_mapping: Vec<i32> = match &config.ams_mapping2 {
            Some(mapping2) => mapping2.iter().map(flat_channel_id_for_entry).collect(),
            None => config.ams_mapping.clone(),
        };
        let mapping = if use_ams {
            AmsMappingTable::Active(flat_mapping)
        } else {
            AmsMappingTable::Inactive(String::new())
        };

        let nozzle_offset = config
            .nozzle_offset_cali
            .unwrap_or_else(|| model.quirks().supports_nozzle_offset_calibration());

        // subtask_id/project_id/task_id all share one value — bambuddy mints a
        // single fresh ID per submission and reuses it for all three; see ProjectFilePayload's
        // `project_id` doc comment for why reusing bambino's own subtask_id here is equivalent.
        let submission_id = ClampedTaskId::from(config.raw_subtask_id).to_string();

        Self {
            print: ProjectFilePayload {
                command: "project_file",
                sequence_id: sequence_id.into().to_string(),
                param: config.plate_gcode_path.clone(),
                subtask_name: config.subtask_name.clone(),
                subtask_id: submission_id.clone(),
                flow_cali: config.run_flow_calibration,
                profile_id: String::from("0"),
                project_id: submission_id.clone(),
                task_id: submission_id,
                file: config.job_filename.clone(),
                url,
                timelapse: config.timelapse,
                bed_type: config.bed_type.clone(),
                bed_leveling: config.bed_leveling,
                extrude_cali_flag: if config.run_flow_calibration { 1 } else { 0 },
                nozzle_offset_cali: if nozzle_offset { 1 } else { 0 },
                vibration_cali: config.run_vibration_compensation,
                layer_inspect: config.layer_inspect,
                use_ams,
                ams_mapping: mapping,
                // Gated on the *computed* `use_ams` (not `config.use_ams`) so a tripped
                // `is_external_spool_safety_valid` interlock can never leave the wire payload
                // internally contradictory (`use_ams: false` alongside a populated
                // `ams_mapping2` array) — see [REF-MQTT-LIFECYCLE] for the firmware error
                // (`0700_8012`) this shape causes.
                ams_mapping2: if use_ams {
                    config.ams_mapping2.clone()
                } else {
                    None
                },
            },
        }
    }
}
