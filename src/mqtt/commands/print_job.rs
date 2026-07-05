//! Print job dispatch (file selection, AMS material mapping, plate/timelapse config).

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::Serialize;

use crate::ams::mapping::AmsMapping2Entry;
use crate::ams::{validate_external_spool_safety, validate_external_spool_safety_flat};
use crate::models::BambuModel;

use super::clamp_task_id;

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
    pub ams_mapping2: Option<Vec<AmsMapping2Entry>>,
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

    pub fn with_ams_mapping2(mut self, mapping2: Vec<AmsMapping2Entry>) -> Self {
        self.use_ams = true;
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
    pub ams_mapping2: Option<Vec<AmsMapping2Entry>>,
}

/// Submits a `.3mf` print job from the SD card for execution.
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

        let is_single_nozzle = model.quirks().physical_nozzle_count() == 1;
        let use_ams = config.use_ams
            && match &config.ams_mapping2 {
                Some(mapping2) => validate_external_spool_safety(is_single_nozzle, mapping2),
                None => validate_external_spool_safety_flat(is_single_nozzle, &config.ams_mapping),
            };
        let mapping = if use_ams {
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
                use_ams,
                ams_mapping: mapping,
                // Gated on the *computed* `use_ams` (not `config.use_ams`) so a tripped
                // `validate_external_spool_safety` interlock can never leave the wire payload
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
