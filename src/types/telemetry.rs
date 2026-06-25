//! # State Telemetry Payload Schemas
//!
//! Provides structured, allocation-friendly deserialization models for the
//! local MQTTS Port 8883 state telemetry streams [REF-MQTT-ENV].
//!
//! Supports permissive parsing for platform discrepancies (such as the variable
//! types of `sdcard` presence markers) and implements binary unpacking helpers
//! for composite packed temperatures, home/status flags, and door sensors.
//!
//! ## Architectural Alignment
//! * **Quirks Integration:** Raw elements (e.g., `device.airduct.parts` or `ctc.info.temp`)
//!   are fully parsed into clean schemas to allow model-specific behaviors to be evaluated
//!   via the quirks engine.

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::{Deserialize, Deserializer, Serialize};

/// Unified top-level telemetry report received from the printer's local MQTT broker.
///
/// Under the over-the-wire schema, updates are typically nested within separate
/// top-level domains depending on which micro-system published the frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryReport {
    /// Telemetry parameters representing the physical printer state machine.
    #[serde(default)]
    pub print: Option<PrintTelemetry>,

    /// Network and hardware board capability descriptors.
    #[serde(default)]
    pub device: Option<DeviceTelemetry>,
}

/// Core printer state machine telemetry, containing kinematics, thermal targets,
/// auxiliary fan configurations, and connected AMS arrays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintTelemetry {
    /// High-level execution status of the G-code processor (e.g., "IDLE", "RUNNING", "PAUSE").
    pub gcode_state: Option<String>,

    /// Path or parent project file currently loaded for execution.
    pub gcode_file: Option<String>,

    /// User-assigned name of the active print queue task.
    pub subtask_name: Option<String>,

    /// Hardware-enforced unique 32-bit transaction identifier tracking active jobs.
    pub subtask_id: Option<String>,

    /// Active layer progress tracker.
    pub layer_num: Option<i32>,

    /// Total layers within the sliced print pipeline.
    /// Wire sends as `total_layer_num`; `total_layers` accepted for compatibility.
    #[serde(alias = "total_layer_num")]
    pub total_layers: Option<i32>,

    /// Print job completion percentage (0.0 to 100.0).
    pub progress: Option<f32>,

    /// Estimated remaining duration of the active layer sequence, in seconds.
    pub mc_remaining_time: Option<i32>,

    /// Motion controller progress percentage (0–100). Present on idle and active prints;
    /// `progress` may only appear during active prints.
    pub mc_percent: Option<i32>,

    /// Print sub-stage identifier tracking granular execution phases within the active print stage.
    pub mc_print_sub_stage: Option<i32>,

    /// Kinematics flag field tracking homing states, networking interfaces, and door nodes.
    pub home_flag: Option<u32>,

    /// State field used in newer enclosed printer lines to track sensors (e.g., door status hex strings).
    pub stat: Option<String>,

    /// Active print stage. Leveraged by the quirks engine to verify stg_cur idle anomalies [REF-MQTT-IDLEBUG].
    pub stg_cur: Option<i32>,

    /// Active error code register, packed as a 32-bit integer [REF-DIAG-HMS].
    pub print_error: Option<u32>,

    /// Active hardware fault and diagnostic alert entries [REF-DIAG-HMS].
    #[serde(default)]
    pub hms: Option<Vec<HmsEntry>>,

    /// Permissive indicator tracking physical MicroSD card insertion.
    ///
    /// Evaluated via custom deserializer to absorb structural variations between firmwares.
    #[serde(deserialize_with = "deserialize_permissive_bool", default)]
    pub sdcard: bool,

    /// Raw wireless network reception scale returned as a formatted string (e.g. "-52dBm").
    pub wifi_signal: Option<String>,

    /// On-board part cooling fan speed (represented as discrete steps 0 to 15) [REF-CLIM-FANS].
    pub cooling_fan_speed: Option<String>,

    /// On-board left-side auxiliary fan speed (represented as discrete steps 0 to 15).
    pub big_fan1_speed: Option<String>,

    /// On-board filtration or chamber exhaust fan speed (represented as discrete steps 0 to 15).
    pub big_fan2_speed: Option<String>,

    /// On-board toolhead heatbreak fan speed (represented as discrete steps 0 to 15).
    pub heatbreak_fan_speed: Option<String>,

    /// Hotend target temperature register.
    ///
    /// Wire sends both integers and floats depending on model. Use `unpack_temperature()`
    /// to extract actual/target from composite-packed values [REF-THER-DECODE].
    pub nozzle_target_temper: Option<f64>,

    /// Hotend actual temperature register.
    ///
    /// Wire sends both integers and floats depending on model [REF-THER-DECODE].
    pub nozzle_temper: Option<f64>,

    /// Heated build-plate temperature register (actual, target, or composite packed).
    pub bed_temper: Option<f64>,

    /// Explicit bed target temperature. Separate from composite-packed `bed_temper`.
    pub bed_target_temper: Option<f64>,

    /// Active chamber heater or sensor telemetry (actual, target, or composite packed).
    pub chamber_temper: Option<f64>,

    /// Hexadecimal bitmask string representing the physical presence of loaded spools.
    pub tray_exist_bits: Option<String>,

    /// Power status of the printer core logic board.
    #[serde(default)]
    pub power_on_flag: Option<bool>,

    /// Camera and recording telemetry. Nested as `print.ipcam` on the wire.
    pub ipcam: Option<IpcamTelemetry>,

    /// AI detection settings (spaghetti detection, first-layer inspection, etc.).
    pub xcam: Option<serde_json::Value>,

    /// AMS expansion bus status container [REF-AMS-DECODE].
    pub ams: Option<AmsStatusReport>,

    /// Slicer-mapped material assignment channels configured during print dispatch [REF-AMS-MAP].
    #[serde(default)]
    pub ams_mapping: Vec<i32>,

    /// Virtual/external spool holder state. Present on P1S, P1P, A1, H2D, X1C.
    pub vt_tray: Option<VirtualTray>,

    /// Device sub-object nested inside pushall `print` envelope on H2/P2/X2 models.
    /// Contains CTC, nozzle, and airduct telemetry for enclosed printers.
    pub device: Option<DeviceTelemetry>,
}

/// Chamber Temperature Controller (CTC) telemetry sub-object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtcTelemetry {
    /// Controller info containing thermal actuals and targets.
    pub info: Option<CtcInfo>,
}

/// Controller information segment detailing current temperature coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtcInfo {
    /// Composite-packed integer temperature value [REF-THER-DECODE].
    /// Use `PrintTelemetry::unpack_temperature()` on this value cast to `f64`.
    pub temp: Option<u32>,
}

/// Camera and recording state telemetry, nested as `print.ipcam` on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcamTelemetry {
    /// Internal identifier or state of the hardware camera module.
    pub ipcam_dev: Option<String>,

    /// Camera live feed recording status (`"enable"` or `"disable"`).
    pub ipcam_record: Option<String>,

    /// Frame-by-layer timelapse recording status (`"enable"` or `"disable"`).
    pub timelapse: Option<String>,

    /// Camera mode bitmask.
    pub mode_bits: Option<u32>,

    /// Camera resolution setting.
    pub resolution: Option<String>,

    /// TUTK server status (`"enable"` or `"disable"`).
    pub tutk_server: Option<String>,
}

/// Raw telemetry entry from the `hms` diagnostic array [REF-DIAG-HMS].
///
/// Each entry represents an active hardware fault or status indication. Use
/// `diagnostics::decode_hms_alert()` to unpack into wiki keys, short-codes, and severity levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HmsEntry {
    /// Packed attribute word encoding module ID, severity, and subsystem address.
    pub attr: u32,
    /// Packed code word encoding fault category and error index.
    pub code: u32,
}

/// Top-level AMS status wrapper containing the units array and bus-wide metadata [REF-AMS-DECODE].
///
/// On the wire, AMS telemetry is nested as `print.ams.ams[...]` — this struct represents
/// the intermediate `print.ams` object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmsStatusReport {
    /// Array of connected AMS units on the expansion bus.
    #[serde(default)]
    pub ams: Vec<AmsUnit>,

    /// Hexadecimal bitmask string indicating which AMS units are physically present.
    pub ams_exist_bits: Option<String>,

    /// Hexadecimal bitmask string indicating which tray slots contain a physical spool.
    pub tray_exist_bits: Option<String>,

    /// Hexadecimal bitmask string indicating which trays contain Bambu Lab branded spools.
    pub tray_is_bbl_bits: Option<String>,

    /// Index of the currently active tray feeding filament to the toolhead.
    pub tray_now: Option<String>,

    /// Index of the previously active tray.
    pub tray_pre: Option<String>,

    /// AMS protocol version.
    pub version: Option<i32>,
}

/// Modular standard expansion unit managing up to 4 physical spool slots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmsUnit {
    /// Unique index representing the unit position on the physical expansion bus (0 to 3).
    pub id: String,

    /// Ambient temperature inside the expansion enclosure, in degrees Celsius.
    pub temp: String,

    /// Enclosure climate relative humidity index (1-5 scale).
    pub humidity: String,

    /// Actual relative humidity percentage (1-100) from the onboard sensor.
    /// Sent as a string on the wire (e.g., `"17"`).
    pub humidity_raw: Option<String>,

    /// Remaining drying time in minutes during an active dry cycle [REF-AMS-DRYER].
    /// Sent as an integer on the wire but may vary by firmware.
    pub dry_time: Option<u32>,

    /// Drying configuration settings (target temperature, duration, filament type).
    pub dry_setting: Option<AmsDrySetting>,

    /// Trays / spool slots configured inside the designated unit.
    #[serde(default)]
    pub tray: Vec<AmsTray>,
}

/// Drying cycle configuration embedded within AMS unit telemetry [REF-AMS-DRYER].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmsDrySetting {
    /// Target drying temperature in degrees Celsius.
    pub dry_temperature: Option<i32>,
    /// Configured drying duration in minutes.
    pub dry_duration: Option<i32>,
    /// Filament type string for the active drying profile (e.g. "PA-CF").
    pub dry_filament: Option<String>,
}

/// Virtual/external spool holder telemetry. Represents the filament loaded
/// directly into the extruder without going through an AMS unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtualTray {
    /// Virtual tray ID (typically `"254"`).
    pub id: Option<String>,

    /// Material class abbreviation (e.g. "PLA", "PETG"). Empty when no filament loaded.
    pub tray_type: Option<String>,

    /// RRGGBBAA hexadecimal color string.
    pub tray_color: Option<String>,

    /// Slicer filament preset index.
    pub tray_info_idx: Option<String>,

    /// Sub-brand or variant string.
    pub tray_sub_brands: Option<String>,

    /// Maximum nozzle temperature for the loaded filament (sent as string).
    pub nozzle_temp_max: Option<String>,

    /// Minimum nozzle temperature for the loaded filament (sent as string).
    pub nozzle_temp_min: Option<String>,

    /// Filament diameter in mm (sent as string, e.g. `"1.75"`).
    pub tray_diameter: Option<String>,

    /// Spool net weight in grams (sent as string).
    pub tray_weight: Option<String>,

    /// Filament temperature setting (sent as string).
    pub tray_temp: Option<String>,

    /// Filament print time accumulator (sent as string).
    pub tray_time: Option<String>,

    /// Bed temperature setting (sent as string).
    pub bed_temp: Option<String>,

    /// Bed temperature type/profile (sent as string).
    pub bed_temp_type: Option<String>,

    /// 16-character hexadecimal RFID tag UID.
    pub tag_uid: Option<String>,

    /// 32-character globally unique filament spool ID.
    pub tray_uuid: Option<String>,

    /// Filament preset display name.
    pub tray_id_name: Option<String>,

    /// XCam inspection info hex string.
    pub xcam_info: Option<String>,

    /// Remaining filament percentage (0–100, or 0 if unknown).
    pub remain: Option<i32>,

    /// Flow rate calibration K factor.
    pub k: Option<f64>,

    /// Flow rate calibration N factor.
    pub n: Option<i32>,

    /// Calibration index (-1 if uncalibrated).
    pub cali_idx: Option<i32>,
}

/// Material spool state descriptor representing a single physical tray slot.
///
/// **Zero-Warning Tolerant Parsing:**
/// Under standard P1/A1 firmware tracks, removing a physical spool truncates the
/// JSON output to only contain the ID key. Making descriptive keys optional permits
/// safe parsing under empty-slot conditions without triggering serialisation panics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmsTray {
    /// The physical index representing the slot (0 to 3). Sent as a string on the wire.
    pub id: String,

    /// The native state code representing filament routing status [REF-AMS-DECODE].
    pub state: Option<u8>,

    /// Material class abbreviation (e.g. "PLA", "PETG", "PA-CF").
    pub tray_type: Option<String>,

    /// RRGGBBAA hexadecimal color string defining the filament profile.
    pub tray_color: Option<String>,

    /// Short or unique customized preset index matching slicer calibrations.
    pub tray_info_idx: Option<String>,

    /// 16-character hexadecimal RFID tag UID, if reading a native spool.
    pub tag_uid: Option<String>,

    /// 32-character globally unique ID of the filament spool.
    pub tray_uuid: Option<String>,

    /// Remaining filament volume percentage (or -1 if uncalculated).
    pub remain: Option<i32>,
}

/// Device hardware state properties containing physical tooling descriptions.
///
/// Appears at two locations on the wire:
/// - Top-level `{"device": {...}}` for incremental updates (e.g., `push_alt_nozzle_info`)
/// - Nested inside `{"print": {"device": {...}}}` for pushall on H2/P2/X2 models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTelemetry {
    /// Structured descriptions representing the active extruder assembly properties.
    pub nozzle: Option<NozzleCollection>,

    /// Nested structures tracking cooling components and climate routing [REF-CLIM-FANS].
    pub airduct: Option<AirductCollection>,

    /// Chamber Temperature Controller telemetry [REF-THER-DECODE].
    pub ctc: Option<CtcTelemetry>,
}

/// Wrap block holding nozzle characteristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NozzleCollection {
    /// Polymorphic array representing active carriages and tool configurations.
    #[serde(default)]
    pub info: Vec<NozzleInfo>,
}

/// Dynamic extruder nozzle details.
///
/// Integrates both legacy abbreviated keys (standard platforms) and descriptive keys
/// (IDEX platforms) to provide unified schema matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NozzleInfo {
    /// Extruder carriage index (0 = Right/Main, 1 = Left/Deputy) or storage rack index.
    pub id: u8,

    /// Nozzle orifice diameter in millimeters (e.g. 0.4).
    pub diameter: Option<f32>,

    /// Target maximum temperature (Standard Platform abbreviated representation).
    pub tm: Option<u32>,

    /// Target maximum temperature (IDEX Platform verbose representation).
    pub max_temp: Option<u32>,

    /// Core physical nozzle composition or tool type designation.
    #[serde(rename = "type")]
    pub nozzle_type: Option<String>,

    /// Normalized physical wear tracker value.
    pub wear: Option<u32>,

    /// Hotend manufacturer serial number (verbose IDEX platform representation).
    pub serial_number: Option<String>,

    /// Hotend manufacturer serial number (standard platform abbreviated representation).
    pub sn: Option<String>,

    /// Physical filament color hex code loaded into the extruder.
    pub filament_colour: Option<String>,

    /// Abbreviated filament color hex code.
    pub color_m: Option<String>,

    /// Filament preset calibration index.
    pub filament_id: Option<String>,

    /// Abbreviated filament preset calibration index.
    pub fila_id: Option<String>,
}

/// Climate parts collection nested within `device` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirductCollection {
    /// Array of active climate routing nodes (heaters, dampers, supplementary fans) [REF-CLIM-FANS].
    #[serde(default)]
    pub parts: Vec<AirductPart>,
}

/// Represents an individual auxiliary routing component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirductPart {
    /// Part index matching hardware configurations (e.g., `160` for the right auxiliary fan).
    pub id: u32,

    /// The active operating speed percentage ($0$ to $100$) or damper direction flag.
    pub state: Option<i32>,
}

// ============================================================================
// Unpacking Helpers and Structural Evaluation Functions
// ============================================================================

pub(crate) const TEMP_COMPOSITE_THRESHOLD: u32 = 500;
pub(crate) const DOOR_SENSOR_BITMASK: u32 = 0x00800000;
pub(crate) const ETHERNET_ACTIVE_BITMASK: u32 = 0x00040000;

impl PrintTelemetry {
    /// Resolves the actual and target values from a composite packed temperature [REF-THER-DECODE].
    ///
    /// Accepts `f64` because the wire sends both integers and floats depending on model.
    /// Values ≤ 500 are direct temperatures (target assumed 0°C). Values > 500 are
    /// composite-packed: upper 16 bits = target, lower 16 bits = actual.
    pub fn unpack_temperature(raw_val: f64) -> (u16, u16) {
        let int_val = raw_val as u32;
        if int_val <= TEMP_COMPOSITE_THRESHOLD {
            (int_val as u16, 0)
        } else {
            let target = (int_val >> 16) & 0xFFFF;
            let actual = int_val & 0xFFFF;
            (actual as u16, target as u16)
        }
    }

    /// Evaluates whether the physical printer is connected via wired Ethernet [REF-NET-PORTS].
    ///
    /// Inspects bit 18 (`0x00040000`) of the `home_flag` register.
    pub fn is_ethernet_active(&self) -> bool {
        self.home_flag
            .map(|flag| (flag & ETHERNET_ACTIVE_BITMASK) != 0)
            .unwrap_or(false)
    }

    /// Reads door sensor state from bit 23 of the `home_flag` register [REF-NET-DOOR].
    ///
    /// Used by X1 series models where the door sensor is wired to the home_flag bitmask.
    pub fn is_door_open_from_home_flag(&self) -> bool {
        self.home_flag
            .map(|flag| (flag & DOOR_SENSOR_BITMASK) != 0)
            .unwrap_or(false)
    }

    /// Reads door sensor state from bit 23 of the parsed hexadecimal `stat` field [REF-NET-DOOR].
    ///
    /// Used by H2, P2, and X2 series models where the door sensor state is encoded in the `stat` string.
    pub fn is_door_open_from_stat(&self) -> bool {
        self.stat
            .as_ref()
            .and_then(|s| Self::parse_hex_string(s))
            .map(|val| (val & DOOR_SENSOR_BITMASK) != 0)
            .unwrap_or(false)
    }

    /// Helper converting raw hexadecimal state strings cleanly into standard numeric values.
    fn parse_hex_string(hex_str: &str) -> Option<u32> {
        let clean = hex_str
            .strip_prefix("0x")
            .or_else(|| hex_str.strip_prefix("0X"))
            .unwrap_or(hex_str);
        u32::from_str_radix(clean, 16).ok()
    }
}

impl AmsTray {
    /// Retrieves the status code of the spool, defaulting to `9` (Empty) if omitted.
    ///
    /// This handles symmetrical empty slots safely on standard P1S and A1 Mini lines.
    pub fn get_state(&self) -> u8 {
        self.state
            .unwrap_or(crate::ams::parser::AMS_TRAY_STATE_EMPTY)
    }
}

// ============================================================================
// Custom Permissive Boolean Deserializer
// ============================================================================

/// Custom deserializer mapping various over-the-wire `sdcard` formats to a unified boolean.
///
/// Absorbs standard boolean values, integer indicators (e.g., `1`), and
/// firmware string constants like `"HAS_SDCARD_NORMAL"`.
fn deserialize_permissive_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawSdValue {
        Bool(bool),
        Int(i64),
        String(String),
    }

    match RawSdValue::deserialize(deserializer) {
        Ok(RawSdValue::Bool(b)) => Ok(b),
        Ok(RawSdValue::Int(i)) => Ok(i != 0),
        Ok(RawSdValue::String(s)) => {
            let s_upper = s.to_uppercase();
            Ok(s_upper == "HAS_SDCARD_NORMAL" || s_upper == "TRUE" || s_upper == "1")
        }
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_unpacking_composite() {
        let (actual, target) = PrintTelemetry::unpack_temperature(6553700.0);
        assert_eq!(actual, 100);
        assert_eq!(target, 100);

        let (actual_idle, target_idle) = PrintTelemetry::unpack_temperature(35.0);
        assert_eq!(actual_idle, 35);
        assert_eq!(target_idle, 0);

        // Fractional temps from P1S/A1 models — truncated to integer
        let (actual_frac, target_frac) = PrintTelemetry::unpack_temperature(27.625);
        assert_eq!(actual_frac, 27);
        assert_eq!(target_frac, 0);
    }

    #[test]
    fn test_airduct_deserialization() {
        let json_data = r#"{
            "device": {
                "airduct": {
                    "parts": [
                        { "id": 160, "state": 85 }
                    ]
                }
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let device = report.device.unwrap();
        let airduct = device.airduct.unwrap();
        assert_eq!(airduct.parts.len(), 1);
        assert_eq!(airduct.parts[0].id, 160);
        assert_eq!(airduct.parts[0].state, Some(85));
    }

    #[test]
    fn test_print_error_deserialization() {
        let json_data = r#"{
            "print": {
                "print_error": 83902476
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let print = report.print.unwrap();
        assert_eq!(print.print_error, Some(83902476));
    }

    #[test]
    fn test_hms_array_deserialization() {
        let json_data = r#"{
            "print": {
                "hms": [
                    { "attr": 50331904, "code": 65543 },
                    { "attr": 83886336, "code": 81924 }
                ]
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let print = report.print.unwrap();
        let hms = print.hms.unwrap();
        assert_eq!(hms.len(), 2);
        assert_eq!(hms[0].attr, 50331904);
        assert_eq!(hms[0].code, 65543);
        assert_eq!(hms[1].attr, 83886336);
        assert_eq!(hms[1].code, 81924);
    }

    #[test]
    fn test_hms_absent_vs_empty() {
        let absent = r#"{ "print": {} }"#;
        let report: TelemetryReport = serde_json::from_str(absent).unwrap();
        assert!(report.print.unwrap().hms.is_none());

        let empty = r#"{ "print": { "hms": [] } }"#;
        let report: TelemetryReport = serde_json::from_str(empty).unwrap();
        let hms = report.print.unwrap().hms.unwrap();
        assert!(hms.is_empty());
    }

    #[test]
    fn test_camera_fields_deserialization() {
        let json_data = r#"{
            "print": {
                "ipcam": {
                    "ipcam_dev": "1",
                    "ipcam_record": "enable",
                    "timelapse": "enable",
                    "mode_bits": 3,
                    "resolution": "",
                    "tutk_server": "disable"
                }
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let ipcam = report.print.unwrap().ipcam.unwrap();
        assert_eq!(ipcam.ipcam_dev.as_deref(), Some("1"));
        assert_eq!(ipcam.ipcam_record.as_deref(), Some("enable"));
        assert_eq!(ipcam.timelapse.as_deref(), Some("enable"));
        assert_eq!(ipcam.mode_bits, Some(3));
        assert_eq!(ipcam.tutk_server.as_deref(), Some("disable"));
    }

    #[test]
    fn test_xcam_deserialization() {
        let json_data = r#"{
            "print": {
                "xcam": {
                    "first_layer_inspector": true,
                    "spaghetti_detector": false
                }
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let print = report.print.unwrap();
        let xcam = print.xcam.unwrap();
        assert_eq!(xcam["first_layer_inspector"], true);
        assert_eq!(xcam["spaghetti_detector"], false);
    }

    #[test]
    fn test_mc_print_sub_stage_deserialization() {
        let json_data = r#"{
            "print": {
                "mc_print_sub_stage": 3
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let print = report.print.unwrap();
        assert_eq!(print.mc_print_sub_stage, Some(3));
    }

    #[test]
    fn test_ams_nested_wire_format() {
        let json_data = r#"{
            "print": {
                "ams": {
                    "ams": [
                        {
                            "id": "0",
                            "temp": "26.0",
                            "humidity": "3",
                            "tray": [
                                { "id": "0", "state": 10, "tray_type": "PLA", "tray_color": "FF0000FF", "remain": 85 },
                                { "id": "1", "state": 11, "tray_type": "PETG", "tray_color": "0000FFFF", "remain": 42 },
                                { "id": "2" },
                                { "id": "3", "state": 10, "tray_type": "PLA", "tray_color": "FFFFFFFF", "remain": 100 }
                            ]
                        }
                    ],
                    "ams_exist_bits": "1",
                    "tray_exist_bits": "b",
                    "tray_now": "1",
                    "version": 0
                }
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let print = report.print.unwrap();
        let ams_status = print.ams.unwrap();

        assert_eq!(ams_status.ams_exist_bits.as_deref(), Some("1"));
        assert_eq!(ams_status.tray_exist_bits.as_deref(), Some("b"));
        assert_eq!(ams_status.tray_now.as_deref(), Some("1"));
        assert_eq!(ams_status.ams.len(), 1);

        let unit = &ams_status.ams[0];
        assert_eq!(unit.id, "0");
        assert_eq!(unit.temp, "26.0");
        assert_eq!(unit.humidity, "3");
        assert_eq!(unit.tray.len(), 4);

        assert_eq!(unit.tray[0].tray_type.as_deref(), Some("PLA"));
        assert_eq!(unit.tray[0].state, Some(10));
        assert_eq!(unit.tray[1].state, Some(11));
        assert_eq!(unit.tray[1].tray_type.as_deref(), Some("PETG"));
        // Slot 2: empty (truncated JSON — P1S firmware behavior)
        assert_eq!(unit.tray[2].state, None);
        assert_eq!(unit.tray[2].get_state(), 9);
    }

    #[test]
    fn test_ams_drying_fields() {
        let json_data = r#"{
            "print": {
                "ams": {
                    "ams": [
                        {
                            "id": "0",
                            "temp": "55.0",
                            "humidity": "1",
                            "humidity_raw": "8",
                            "dry_time": 142,
                            "dry_setting": {
                                "dry_temperature": 55,
                                "dry_duration": 480,
                                "dry_filament": "PA-CF"
                            },
                            "tray": []
                        }
                    ]
                }
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let unit = &report.print.unwrap().ams.unwrap().ams[0];
        assert_eq!(unit.dry_time, Some(142));
        assert_eq!(unit.humidity_raw.as_deref(), Some("8"));
        let dry = unit.dry_setting.as_ref().unwrap();
        assert_eq!(dry.dry_temperature, Some(55));
        assert_eq!(dry.dry_duration, Some(480));
        assert_eq!(dry.dry_filament.as_deref(), Some("PA-CF"));
    }

    #[test]
    fn test_full_telemetry_with_diagnostics() {
        let json_data = r#"{
            "print": {
                "gcode_state": "RUNNING",
                "mc_print_sub_stage": 0,
                "print_error": 0,
                "hms": [],
                "ipcam": {
                    "ipcam_dev": "1",
                    "ipcam_record": "enable",
                    "timelapse": "disable"
                },
                "xcam": { "allow_skip_parts": false }
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let print = report.print.unwrap();
        assert_eq!(print.gcode_state.as_deref(), Some("RUNNING"));
        assert_eq!(print.mc_print_sub_stage, Some(0));
        assert_eq!(print.print_error, Some(0));
        assert!(print.hms.unwrap().is_empty());
        let ipcam = print.ipcam.unwrap();
        assert_eq!(ipcam.ipcam_record.as_deref(), Some("enable"));
        assert_eq!(ipcam.timelapse.as_deref(), Some("disable"));
    }

    #[test]
    fn test_door_open_from_home_flag() {
        let json_open = r#"{ "print": { "home_flag": 8388608 } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_open)
            .expect("valid json")
            .print
            .expect("print present");
        assert!(print.is_door_open_from_home_flag());

        let json_closed = r#"{ "print": { "home_flag": 0 } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_closed)
            .expect("valid json")
            .print
            .expect("print present");
        assert!(!print.is_door_open_from_home_flag());
    }

    #[test]
    fn test_door_open_from_stat() {
        let json_open = r#"{ "print": { "stat": "0x00800000" } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_open)
            .expect("valid json")
            .print
            .expect("print present");
        assert!(print.is_door_open_from_stat());

        let json_closed = r#"{ "print": { "stat": "0x00000000" } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_closed)
            .expect("valid json")
            .print
            .expect("print present");
        assert!(!print.is_door_open_from_stat());
    }

    #[test]
    fn test_door_open_missing_fields() {
        let json_empty = r#"{ "print": {} }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_empty)
            .expect("valid json")
            .print
            .expect("print present");
        assert!(!print.is_door_open_from_home_flag());
        assert!(!print.is_door_open_from_stat());
    }

    #[test]
    fn test_p1s_wire_capture_end_to_end() {
        let json_data = include_str!("../../tests/mocks/P1S.json");
        let report: TelemetryReport =
            serde_json::from_str(json_data).expect("P1S wire capture must deserialize");
        let print = report.print.expect("print present");

        assert_eq!(print.gcode_state.as_deref(), Some("FINISH"));
        assert_eq!(
            print.subtask_name.as_deref(),
            Some("8_Minute_Print_Multi-Fit_Cardboard_Spool_Ring")
        );
        assert_eq!(print.layer_num, Some(27));
        assert_eq!(print.total_layers, Some(27));
        assert_eq!(print.mc_percent, Some(100));
        assert_eq!(print.mc_remaining_time, Some(0));
        assert_eq!(print.home_flag, Some(6374672));
        assert_eq!(print.stg_cur, Some(0));
        assert_eq!(print.print_error, Some(0));
        assert!(print.hms.unwrap().is_empty());
        assert!(print.sdcard);
        assert_eq!(print.wifi_signal.as_deref(), Some("-41dBm"));

        // Fix A: float temps deserialize correctly
        assert!((print.bed_temper.unwrap() - 27.625).abs() < 0.001);
        assert!((print.nozzle_temper.unwrap() - 29.46875).abs() < 0.001);
        assert_eq!(print.nozzle_target_temper.unwrap() as u32, 0);
        assert_eq!(print.chamber_temper.unwrap() as u32, 5);

        // Fix E: bed_target_temper
        assert_eq!(print.bed_target_temper.unwrap() as u32, 0);

        // Fix H: total_layer_num alias
        assert_eq!(print.total_layers, Some(27));

        // Fix G: nested ipcam
        let ipcam = print.ipcam.expect("ipcam present");
        assert_eq!(ipcam.ipcam_dev.as_deref(), Some("1"));
        assert_eq!(ipcam.ipcam_record.as_deref(), Some("disable"));
        assert_eq!(ipcam.timelapse.as_deref(), Some("disable"));
        assert_eq!(ipcam.mode_bits, Some(3));

        // Fix F: AMS tray IDs are strings
        let ams = print.ams.expect("ams present");
        assert_eq!(ams.ams.len(), 1);
        let unit = &ams.ams[0];
        assert_eq!(unit.id, "0");
        assert_eq!(unit.tray.len(), 4);
        assert_eq!(unit.tray[0].id, "0");
        assert_eq!(unit.tray[3].id, "3");

        // Fix D: vt_tray
        let vt = print.vt_tray.expect("vt_tray present");
        assert_eq!(vt.id.as_deref(), Some("254"));
        assert_eq!(vt.tray_color.as_deref(), Some("FFFFFF00"));
        assert_eq!(vt.remain, Some(0));
        assert!((vt.k.unwrap() - 0.02).abs() < 0.001);
        assert_eq!(vt.cali_idx, Some(-1));
    }

    #[test]
    fn test_temperature_fields_accept_float_and_int() {
        let json_float = r#"{ "print": { "bed_temper": 27.625, "nozzle_temper": 29.46875 } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_float)
            .unwrap()
            .print
            .unwrap();
        assert!((print.bed_temper.unwrap() - 27.625).abs() < 0.001);
        assert!((print.nozzle_temper.unwrap() - 29.46875).abs() < 0.001);

        let json_int = r#"{ "print": { "bed_temper": 100, "nozzle_temper": 40 } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_int)
            .unwrap()
            .print
            .unwrap();
        assert_eq!(print.bed_temper.unwrap() as u32, 100);
        assert_eq!(print.nozzle_temper.unwrap() as u32, 40);
    }

    #[test]
    fn test_temperature_boundary_500_and_501() {
        let (actual, target) = PrintTelemetry::unpack_temperature(500.0);
        assert_eq!(actual, 500);
        assert_eq!(target, 0);

        // 501 = 0x000001F5 → actual=501, target=0 (but > threshold so unpacked)
        let (actual, target) = PrintTelemetry::unpack_temperature(501.0);
        assert_eq!(actual, 501);
        assert_eq!(target, 0);

        // Real composite: target=60, actual=48 → (60 << 16) | 48 = 3932208
        let (actual, target) = PrintTelemetry::unpack_temperature(3932208.0);
        assert_eq!(actual, 48);
        assert_eq!(target, 60);
    }

    #[test]
    fn test_ctc_info_deserialization_composite() {
        let json_data = r#"{
            "device": {
                "ctc": {
                    "info": { "temp": 3932208 }
                }
            }
        }"#;
        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let ctc = report.device.unwrap().ctc.unwrap();
        let temp = ctc.info.unwrap().temp.unwrap();
        let (actual, target) = PrintTelemetry::unpack_temperature(temp as f64);
        assert_eq!(actual, 48);
        assert_eq!(target, 60);
    }

    #[test]
    fn test_ctc_info_deserialization_direct() {
        let json_data = r#"{
            "device": {
                "ctc": {
                    "info": { "temp": 35 }
                }
            }
        }"#;
        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let temp = report
            .device
            .unwrap()
            .ctc
            .unwrap()
            .info
            .unwrap()
            .temp
            .unwrap();
        let (actual, target) = PrintTelemetry::unpack_temperature(temp as f64);
        assert_eq!(actual, 35);
        assert_eq!(target, 0);
    }

    #[test]
    fn test_device_nesting_in_pushall() {
        let json_data = r#"{
            "print": {
                "gcode_state": "IDLE",
                "device": {
                    "ctc": {
                        "info": { "temp": 3932208 }
                    },
                    "nozzle": {
                        "info": [{ "id": 0, "diameter": 0.4 }]
                    },
                    "airduct": {
                        "parts": [{ "id": 160, "state": 50 }]
                    }
                }
            }
        }"#;
        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let print = report.print.unwrap();
        let device = print.device.expect("device nested in print");
        let ctc_temp = device.ctc.unwrap().info.unwrap().temp.unwrap();
        assert_eq!(ctc_temp, 3932208);
        assert_eq!(device.nozzle.unwrap().info[0].id, 0);
        assert_eq!(device.airduct.unwrap().parts[0].state, Some(50));
    }

    #[test]
    fn test_device_incremental_top_level() {
        let json_data = r#"{
            "device": {
                "nozzle": {
                    "info": [{ "id": 0, "diameter": 0.4 }, { "id": 1, "diameter": 0.6 }]
                }
            }
        }"#;
        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        assert!(report.print.is_none());
        let device = report.device.unwrap();
        let nozzles = &device.nozzle.unwrap().info;
        assert_eq!(nozzles.len(), 2);
        assert_eq!(nozzles[1].id, 1);
    }

    #[test]
    fn test_deserialize_permissive_bool_variants() {
        // Bool true
        let r: TelemetryReport =
            serde_json::from_str(r#"{ "print": { "sdcard": true } }"#).unwrap();
        assert!(r.print.unwrap().sdcard);

        // Bool false
        let r: TelemetryReport =
            serde_json::from_str(r#"{ "print": { "sdcard": false } }"#).unwrap();
        assert!(!r.print.unwrap().sdcard);

        // Int 1
        let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": 1 } }"#).unwrap();
        assert!(r.print.unwrap().sdcard);

        // Int 0
        let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": 0 } }"#).unwrap();
        assert!(!r.print.unwrap().sdcard);

        // String "HAS_SDCARD_NORMAL"
        let r: TelemetryReport =
            serde_json::from_str(r#"{ "print": { "sdcard": "HAS_SDCARD_NORMAL" } }"#).unwrap();
        assert!(r.print.unwrap().sdcard);

        // String "TRUE"
        let r: TelemetryReport =
            serde_json::from_str(r#"{ "print": { "sdcard": "TRUE" } }"#).unwrap();
        assert!(r.print.unwrap().sdcard);

        // String "1"
        let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": "1" } }"#).unwrap();
        assert!(r.print.unwrap().sdcard);

        // String other → false
        let r: TelemetryReport =
            serde_json::from_str(r#"{ "print": { "sdcard": "nope" } }"#).unwrap();
        assert!(!r.print.unwrap().sdcard);

        // Missing → default false
        let r: TelemetryReport = serde_json::from_str(r#"{ "print": {} }"#).unwrap();
        assert!(!r.print.unwrap().sdcard);
    }

    #[test]
    fn test_parse_hex_string_variants() {
        assert_eq!(
            PrintTelemetry::parse_hex_string("0x00800000"),
            Some(0x00800000)
        );
        assert_eq!(
            PrintTelemetry::parse_hex_string("0X00800000"),
            Some(0x00800000)
        );
        assert_eq!(
            PrintTelemetry::parse_hex_string("00800000"),
            Some(0x00800000)
        );
        assert_eq!(PrintTelemetry::parse_hex_string("ff"), Some(0xff));
        assert_eq!(PrintTelemetry::parse_hex_string("zzzz"), None);
        assert_eq!(PrintTelemetry::parse_hex_string(""), None);
    }

    #[test]
    fn test_ethernet_active_bitmask() {
        let json = r#"{ "print": { "home_flag": 262144 } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json)
            .unwrap()
            .print
            .unwrap();
        assert!(print.is_ethernet_active());

        let json_off = r#"{ "print": { "home_flag": 0 } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_off)
            .unwrap()
            .print
            .unwrap();
        assert!(!print.is_ethernet_active());

        let json_missing = r#"{ "print": {} }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_missing)
            .unwrap()
            .print
            .unwrap();
        assert!(!print.is_ethernet_active());
    }

    #[test]
    fn test_power_on_flag_deserialization() {
        let json_true = r#"{ "print": { "power_on_flag": true } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_true)
            .unwrap()
            .print
            .unwrap();
        assert_eq!(print.power_on_flag, Some(true));

        let json_false = r#"{ "print": { "power_on_flag": false } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_false)
            .unwrap()
            .print
            .unwrap();
        assert_eq!(print.power_on_flag, Some(false));

        let json_missing = r#"{ "print": {} }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_missing)
            .unwrap()
            .print
            .unwrap();
        assert_eq!(print.power_on_flag, None);
    }

    #[test]
    fn test_total_layer_num_alias() {
        // Wire name: total_layer_num
        let json = r#"{ "print": { "total_layer_num": 42 } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json)
            .unwrap()
            .print
            .unwrap();
        assert_eq!(print.total_layers, Some(42));

        // Legacy name still works
        let json2 = r#"{ "print": { "total_layers": 99 } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json2)
            .unwrap()
            .print
            .unwrap();
        assert_eq!(print.total_layers, Some(99));
    }

    #[test]
    fn test_mc_percent_deserialization() {
        let json = r#"{ "print": { "mc_percent": 100 } }"#;
        let print = serde_json::from_str::<TelemetryReport>(json)
            .unwrap()
            .print
            .unwrap();
        assert_eq!(print.mc_percent, Some(100));
    }

    #[test]
    fn test_virtual_tray_deserialization() {
        let json_data = r#"{
            "print": {
                "vt_tray": {
                    "id": "254",
                    "tray_type": "PLA",
                    "tray_color": "FF0000FF",
                    "nozzle_temp_max": "220",
                    "nozzle_temp_min": "190",
                    "tray_diameter": "1.75",
                    "remain": 85,
                    "k": 0.02,
                    "n": 1,
                    "cali_idx": -1,
                    "tag_uid": "0000000000000000",
                    "tray_uuid": "00000000000000000000000000000000"
                }
            }
        }"#;
        let print = serde_json::from_str::<TelemetryReport>(json_data)
            .unwrap()
            .print
            .unwrap();
        let vt = print.vt_tray.unwrap();
        assert_eq!(vt.id.as_deref(), Some("254"));
        assert_eq!(vt.tray_type.as_deref(), Some("PLA"));
        assert_eq!(vt.tray_color.as_deref(), Some("FF0000FF"));
        assert_eq!(vt.nozzle_temp_max.as_deref(), Some("220"));
        assert_eq!(vt.remain, Some(85));
        assert_eq!(vt.cali_idx, Some(-1));
    }

    #[test]
    fn test_virtual_tray_empty() {
        let json_data = r#"{
            "print": {
                "vt_tray": {
                    "id": "254",
                    "tray_type": "",
                    "tray_color": "FFFFFF00",
                    "remain": 0
                }
            }
        }"#;
        let vt = serde_json::from_str::<TelemetryReport>(json_data)
            .unwrap()
            .print
            .unwrap()
            .vt_tray
            .unwrap();
        assert_eq!(vt.tray_type.as_deref(), Some(""));
        assert_eq!(vt.remain, Some(0));
    }

    #[test]
    fn test_nozzle_info_standard_keys() {
        let json_data = r#"{
            "device": {
                "nozzle": {
                    "info": [{
                        "id": 0,
                        "diameter": 0.4,
                        "tm": 300,
                        "type": "hardened_steel",
                        "sn": "SN123",
                        "color_m": "FF0000",
                        "fila_id": "GFA01"
                    }]
                }
            }
        }"#;
        let nozzle = &serde_json::from_str::<TelemetryReport>(json_data)
            .unwrap()
            .device
            .unwrap()
            .nozzle
            .unwrap()
            .info[0];
        assert_eq!(nozzle.id, 0);
        assert_eq!(nozzle.tm, Some(300));
        assert_eq!(nozzle.nozzle_type.as_deref(), Some("hardened_steel"));
        assert_eq!(nozzle.sn.as_deref(), Some("SN123"));
    }

    #[test]
    fn test_nozzle_info_idex_keys() {
        let json_data = r#"{
            "device": {
                "nozzle": {
                    "info": [{
                        "id": 1,
                        "diameter": 0.6,
                        "max_temp": 350,
                        "type": "stainless_steel",
                        "serial_number": "IDEX-SN-456",
                        "filament_colour": "00FF00",
                        "filament_id": "GFB02"
                    }]
                }
            }
        }"#;
        let nozzle = &serde_json::from_str::<TelemetryReport>(json_data)
            .unwrap()
            .device
            .unwrap()
            .nozzle
            .unwrap()
            .info[0];
        assert_eq!(nozzle.id, 1);
        assert_eq!(nozzle.max_temp, Some(350));
        assert_eq!(nozzle.serial_number.as_deref(), Some("IDEX-SN-456"));
        assert_eq!(nozzle.filament_colour.as_deref(), Some("00FF00"));
    }
}
