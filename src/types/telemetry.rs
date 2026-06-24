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
    pub total_layers: Option<i32>,

    /// Print job completion percentage (0.0 to 100.0).
    pub progress: Option<f32>,

    /// Estimated remaining duration of the active layer sequence, in seconds.
    pub mc_remaining_time: Option<i32>,

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
    /// May carry packed composite values when actively heating [REF-THER-DECODE].
    pub nozzle_target_temper: Option<u32>,

    /// Hotend actual temperature register.
    ///
    /// May carry packed composite values when actively heating [REF-THER-DECODE].
    pub nozzle_temper: Option<u32>,

    /// Heated build-plate temperature register (actual, target, or composite packed).
    pub bed_temper: Option<u32>,

    /// Active chamber heater or sensor telemetry (actual, target, or composite packed).
    pub chamber_temper: Option<u32>,

    /// Hexadecimal bitmask string representing the physical presence of loaded spools.
    pub tray_exist_bits: Option<String>,

    /// Power status of the printer core logic board.
    #[serde(default)]
    pub power_on_flag: Option<bool>,

    /// Internal identifier or state of the hardware camera module [REF-CAM-RTSPS].
    pub ipcam_dev: Option<String>,

    /// Camera live feed recording status (`"enable"` or `"disable"`) [REF-CAM-RTSPS].
    pub ipcam_record: Option<String>,

    /// Frame-by-layer timelapse recording status (`"enable"` or `"disable"`) [REF-CAM-RTSPS].
    pub timelapse: Option<String>,

    /// AI detection settings (spaghetti detection, first-layer inspection, etc.).
    pub xcam: Option<serde_json::Value>,

    /// AMS expansion bus status container [REF-AMS-DECODE].
    pub ams: Option<AmsStatusReport>,

    /// Slicer-mapped material assignment channels configured during print dispatch [REF-AMS-MAP].
    #[serde(default)]
    pub ams_mapping: Vec<i32>,

    /// Chamber Temperature Controller telemetry mapping [REF-THER-DECODE].
    pub ctc: Option<CtcTelemetry>,
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
    /// Temperature value, typically composite packed or direct Celsius [REF-THER-DECODE].
    pub temp: Option<f32>,
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

/// Material spool state descriptor representing a single physical tray slot.
///
/// **Zero-Warning Tolerant Parsing:**
/// Under standard P1/A1 firmware tracks, removing a physical spool truncates the
/// JSON output to only contain the ID key. Making descriptive keys optional permits
/// safe parsing under empty-slot conditions without triggering serialisation panics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmsTray {
    /// The physical index representing the slot (0 to 3).
    pub id: u8,

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTelemetry {
    /// Structured descriptions representing the active extruder assembly properties.
    pub nozzle: Option<NozzleCollection>,

    /// Nested structures tracking cooling components and climate routing [REF-CLIM-FANS].
    pub airduct: Option<AirductCollection>,
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
    /// Resolves the actual and target values from a composite packed temperature u32 [REF-THER-DECODE].
    ///
    /// If the value is less than or equal to 500, the temperature is returned directly
    /// and the target is assumed to be 0°C. If greater than 500, both fields are extracted.
    pub fn unpack_temperature(raw_val: u32) -> (u16, u16) {
        if raw_val <= TEMP_COMPOSITE_THRESHOLD {
            (raw_val as u16, 0)
        } else {
            let target = (raw_val >> 16) & 0xFFFF;
            let actual = raw_val & 0xFFFF;
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

    /// Evaluates whether the physical front enclosure door is open [REF-NET-DOOR].
    ///
    /// Due to model polymorphism, sensor routing is dependent on the core series:
    /// * **X1 Series**: Monitored via bit 23 (`0x00800000`) of the `home_flag` register.
    /// * **Other Series**: Monitored via bit 23 (`0x00800000`) of the parsed hexadecimal `stat` field.
    pub fn is_door_open(&self, is_x1_series: bool) -> bool {
        if is_x1_series {
            self.home_flag
                .map(|flag| (flag & DOOR_SENSOR_BITMASK) != 0)
                .unwrap_or(false)
        } else {
            self.stat
                .as_ref()
                .and_then(|s| Self::parse_hex_string(s))
                .map(|val| (val & DOOR_SENSOR_BITMASK) != 0)
                .unwrap_or(false)
        }
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
        let (actual, target) = PrintTelemetry::unpack_temperature(6553700);
        assert_eq!(actual, 100);
        assert_eq!(target, 100);

        let (actual_idle, target_idle) = PrintTelemetry::unpack_temperature(35);
        assert_eq!(actual_idle, 35);
        assert_eq!(target_idle, 0);
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
                "ipcam_dev": "1",
                "ipcam_record": "enable",
                "timelapse": "enable"
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let print = report.print.unwrap();
        assert_eq!(print.ipcam_dev.as_deref(), Some("1"));
        assert_eq!(print.ipcam_record.as_deref(), Some("enable"));
        assert_eq!(print.timelapse.as_deref(), Some("enable"));
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
                                { "id": 0, "state": 10, "tray_type": "PLA", "tray_color": "FF0000FF", "remain": 85 },
                                { "id": 1, "state": 11, "tray_type": "PETG", "tray_color": "0000FFFF", "remain": 42 },
                                { "id": 2 },
                                { "id": 3, "state": 10, "tray_type": "PLA", "tray_color": "FFFFFFFF", "remain": 100 }
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
                "ipcam_dev": "1",
                "ipcam_record": "enable",
                "timelapse": "disable",
                "xcam": { "allow_skip_parts": false }
            }
        }"#;

        let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
        let print = report.print.unwrap();
        assert_eq!(print.gcode_state.as_deref(), Some("RUNNING"));
        assert_eq!(print.mc_print_sub_stage, Some(0));
        assert_eq!(print.print_error, Some(0));
        assert!(print.hms.unwrap().is_empty());
        assert_eq!(print.ipcam_record.as_deref(), Some("enable"));
        assert_eq!(print.timelapse.as_deref(), Some("disable"));
    }
}
