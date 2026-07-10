//! AMS telemetry types (tray slots, units, dry settings, virtual trays).

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

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

    /// Target tray index.
    #[serde(default)]
    pub tray_tar: Option<String>,

    /// AMS protocol version.
    pub version: Option<i32>,

    /// RFID read completion bitmask (hex string).
    #[serde(default)]
    pub tray_read_done_bits: Option<String>,

    /// Active RFID read bitmask (hex string).
    #[serde(default)]
    pub tray_reading_bits: Option<String>,

    /// AMS insertion event flag.
    #[serde(default)]
    pub insert_flag: Option<bool>,

    /// AMS unit external power state (distinct from printer power; AMS Pro needs external power for drying).
    #[serde(default)]
    pub power_on_flag: Option<bool>,

    /// Calibration tracking ID.
    #[serde(default)]
    pub cali_id: Option<i32>,

    /// Calibration tracking status.
    #[serde(default)]
    pub cali_stat: Option<i32>,
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

    /// Hex-encoded bitmask: bits 0–3 = AMS type, bits 4–7 = dry_status, bits 8–11 = extruder assignment (IDEX routing).
    #[serde(default)]
    pub info: Option<String>,

    /// Drying failure reason codes per slot (X2D).
    #[serde(default)]
    pub dry_sf_reason: Option<Vec<i32>>,
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

/// Virtual/external spool holder telemetry.
/// Represents the filament loaded directly into the extruder without going through an AMS unit.
///
/// On the wire, this shares the same schema as `AmsTray` — both physical AMS trays
/// and virtual/external spool holders use the same field set.
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

/// Native state code meaning "slot empty" [REF-AMS-DECODE].
/// Lives here (not in `ams::parser`) since `AmsTray::get_state()` is a pure data accessor and
/// `types/` must not depend on business-logic modules.
pub(crate) const AMS_TRAY_STATE_EMPTY: u8 = 9;

/// Native state code meaning "spool physically present but not yet fed to the extruder"
/// [REF-AMS-DECODE] (BUG-012). On H2D-generation firmware this is one of the two explicit
/// "not loaded" signals alongside `AMS_TRAY_STATE_EMPTY` — a spool present in state 10 may
/// still have unconfirmed/stale metadata attached, so it's treated as an absent-equivalent
/// state for stale-data cleansing purposes, same as `AMS_TRAY_STATE_EMPTY`. Verified against
/// `pybambu`/`Bambuddy`'s independent reverse-engineering (`bambu_mqtt.py`'s
/// `apply_tray_exist_bits` and incremental-merge handler, `main.py`'s `on_ams_change` —
/// `loaded = cur_state == 11 or (cur_state not in (9, 10) and cur_type.strip())`, cross-tested
/// against H2D, A1 Mini, and P1S firmware, citing upstream issues #784/#1322).
pub(crate) const AMS_TRAY_STATE_SPOOL_NOT_FED: u8 = 10;

/// Material spool state descriptor representing a single physical tray slot.
///
/// On the wire, AMS trays and virtual/external trays (`vt_tray`, `vir_slot`)
/// share the same field schema. All descriptive fields are optional — under
/// standard P1/A1 firmware, removing a spool truncates the JSON to only the ID key.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

    /// Sub-brand or variant string (e.g. "PLA Matte", "Support for PLA").
    pub tray_sub_brands: Option<String>,

    /// Maximum nozzle temperature for the loaded filament (sent as string).
    pub nozzle_temp_max: Option<String>,

    /// Minimum nozzle temperature for the loaded filament (sent as string).
    pub nozzle_temp_min: Option<String>,

    /// Filament diameter in mm (sent as string, e.g. `"1.75"`).
    pub tray_diameter: Option<String>,

    /// Spool net weight in grams (sent as string).
    pub tray_weight: Option<String>,

    /// Filament preset display name (e.g. "S02-W0", "A01-K1").
    pub tray_id_name: Option<String>,

    /// Filament drying temperature (sent as string). Newer firmware uses `drying_temp`.
    pub tray_temp: Option<String>,

    /// Filament drying time (sent as string). Newer firmware uses `drying_time`.
    pub tray_time: Option<String>,

    /// Drying temperature on newer firmware (alias for `tray_temp`).
    pub drying_temp: Option<String>,

    /// Drying time on newer firmware (alias for `tray_time`).
    pub drying_time: Option<String>,

    /// Per-tray bed temperature setting (sent as string).
    pub bed_temp: Option<String>,

    /// Bed temperature type/profile (sent as string).
    pub bed_temp_type: Option<String>,

    /// XCam inspection info hex string.
    pub xcam_info: Option<String>,

    /// Flow rate calibration K factor.
    pub k: Option<f64>,

    /// Flow rate calibration N factor.
    pub n: Option<i32>,

    /// Calibration index (-1 if uncalibrated).
    pub cali_idx: Option<i32>,

    /// Multi-color columns array (e.g. `["000000FF"]`).
    #[serde(default)]
    pub cols: Option<Vec<String>>,

    /// Color type indicator.
    pub ctype: Option<i32>,

    /// Total filament spool length in mm.
    pub total_len: Option<u32>,
}

const AMS_UNIT_INFO_TYPE_MASK: u64 = 0xF;
const AMS_UNIT_INFO_DRY_STATUS_SHIFT: u32 = 4;
const AMS_UNIT_INFO_DRY_STATUS_MASK: u64 = 0xF;
const AMS_UNIT_INFO_EXTRUDER_SHIFT: u32 = 8;
const AMS_UNIT_INFO_EXTRUDER_MASK: u64 = 0xF;
const AMS_UNIT_INFO_EXTRUDER_UNINITIALIZED: u8 = 0xE;
const AMS_UNIT_INFO_DRY_SUB_STATUS_SHIFT: u32 = 22;
const AMS_UNIT_INFO_DRY_SUB_STATUS_MASK: u64 = 0xF;

impl AmsUnit {
    /// Parses the hex-encoded `info` bitmask string into an integer.
    pub fn parse_info(&self) -> Option<u64> {
        self.info
            .as_ref()
            .and_then(|s| u64::from_str_radix(s, 16).ok())
    }

    /// AMS unit type from bits 0–3 (e.g. 3 = AMS Lite).
    pub fn ams_type(&self) -> Option<u8> {
        self.parse_info()
            .map(|v| (v & AMS_UNIT_INFO_TYPE_MASK) as u8)
    }

    /// Drying status from bits 4–7.
    pub fn dry_status(&self) -> Option<u8> {
        self.parse_info()
            .map(|v| ((v >> AMS_UNIT_INFO_DRY_STATUS_SHIFT) & AMS_UNIT_INFO_DRY_STATUS_MASK) as u8)
    }

    /// Extruder assignment from bits 8–11 (0 = right/main, 1 = left/deputy).
    /// Returns `None` when `info` is absent or the value is 0xE (uninitialized).
    pub fn extruder_assignment(&self) -> Option<u8> {
        self.parse_info().and_then(|v| {
            let raw = ((v >> AMS_UNIT_INFO_EXTRUDER_SHIFT) & AMS_UNIT_INFO_EXTRUDER_MASK) as u8;
            if raw == AMS_UNIT_INFO_EXTRUDER_UNINITIALIZED {
                None
            } else {
                Some(raw)
            }
        })
    }

    /// Drying sub-status from bits 22–25.
    pub fn dry_sub_status(&self) -> Option<u8> {
        self.parse_info().map(|v| {
            ((v >> AMS_UNIT_INFO_DRY_SUB_STATUS_SHIFT) & AMS_UNIT_INFO_DRY_SUB_STATUS_MASK) as u8
        })
    }
}

impl AmsTray {
    /// Retrieves the status code of the spool, defaulting to `9` (Empty) if omitted.
    ///
    /// This handles symmetrical empty slots safely on standard P1S and A1 Mini lines.
    pub fn get_state(&self) -> u8 {
        self.state.unwrap_or(AMS_TRAY_STATE_EMPTY)
    }
}
