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

impl AmsStatusReport {
    /// Merges a freshly-parsed `AmsStatusReport` into `self` field-by-field, instead of
    /// replacing `self` wholesale.
    ///
    /// BUG-091: confirmed via a real P1S wire capture — an incremental `print.ams` push may
    /// carry only a subset of fields (e.g. `{"ams":{"tray_tar":"3"}}` during a tray-switch
    /// sequence), with `ams` (the unit/tray array, `#[serde(default)]`) and every other field
    /// simply absent rather than explicitly emptied. A caller that replaces its cached
    /// `AmsStatusReport` wholesale on any `print.ams: Some(_)` push loses the previously-known
    /// unit array and other fields on every such partial push. Mirrors the "each field
    /// independently keeps its most recently observed value" staleness policy `TelemetryCache`
    /// already documents at the `PrinterTelemetry` level, one layer deeper.
    ///
    /// BUG-098: `ams` itself is now a keyed per-unit merge, not a wholesale array replace —
    /// see the loop body below and `AmsUnit::merge_from`.
    pub(crate) fn merge_from(&mut self, incoming: &AmsStatusReport) {
        if !incoming.ams.is_empty() {
            // BUG-098: keyed per-unit merge, not wholesale replace — confirmed against
            // BambuStudio's own `DevFilaSystem.cpp` (`ParseAmsInfo`), which looks up each
            // unit by `ams_id` in a persistent `amsList` map (`system->amsList.find(ams_id)`)
            // that's never pruned by a push's contents; a unit not mentioned in a given
            // `print.ams.ams` push stays cached exactly as last observed, and a mentioned
            // unit's own fields merge in via `AmsUnit::merge_from` rather than replacing the
            // whole unit.
            for incoming_unit in &incoming.ams {
                match self.ams.iter_mut().find(|u| u.id == incoming_unit.id) {
                    Some(cached_unit) => cached_unit.merge_from(incoming_unit),
                    None => self.ams.push(incoming_unit.clone()),
                }
            }
        }
        if incoming.ams_exist_bits.is_some() {
            self.ams_exist_bits = incoming.ams_exist_bits.clone();
        }
        if incoming.tray_exist_bits.is_some() {
            self.tray_exist_bits = incoming.tray_exist_bits.clone();
        }
        if incoming.tray_is_bbl_bits.is_some() {
            self.tray_is_bbl_bits = incoming.tray_is_bbl_bits.clone();
        }
        if incoming.tray_now.is_some() {
            self.tray_now = incoming.tray_now.clone();
        }
        if incoming.tray_pre.is_some() {
            self.tray_pre = incoming.tray_pre.clone();
        }
        if incoming.tray_tar.is_some() {
            self.tray_tar = incoming.tray_tar.clone();
        }
        if incoming.version.is_some() {
            self.version = incoming.version;
        }
        if incoming.tray_read_done_bits.is_some() {
            self.tray_read_done_bits = incoming.tray_read_done_bits.clone();
        }
        if incoming.tray_reading_bits.is_some() {
            self.tray_reading_bits = incoming.tray_reading_bits.clone();
        }
        if incoming.insert_flag.is_some() {
            self.insert_flag = incoming.insert_flag;
        }
        if incoming.power_on_flag.is_some() {
            self.power_on_flag = incoming.power_on_flag;
        }
        if incoming.cali_id.is_some() {
            self.cali_id = incoming.cali_id;
        }
        if incoming.cali_stat.is_some() {
            self.cali_stat = incoming.cali_stat;
        }
    }
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
    ///
    /// `None` means this push's `tray` key was absent from the wire — leave previously
    /// cached trays untouched. `Some(vec![])` means the key was present but empty, which
    /// (per `AmsUnit::merge_from`) prunes every cached tray for this unit — bambino's
    /// `#[serde(default)]` on `Option<Vec<_>>` gives exactly this absent-vs-present-empty
    /// distinction for free (absent key -> `None` via `Default`, present key -> `Some(_)`
    /// however short), confirmed against BambuStudio's `DevFilaSystem.cpp`
    /// (`ParseAmsInfo`'s `if (j_ams.contains("tray"))` gate around both the per-tray parse
    /// loop and the prune-absent-ids loop).
    #[serde(default)]
    pub tray: Option<Vec<AmsTray>>,

    /// Hex-encoded bitmask: bits 0–3 = AMS type, bits 4–7 = dry_status, bits 8–11 = extruder assignment (IDEX routing).
    #[serde(default)]
    pub info: Option<String>,

    /// Drying failure reason codes per slot (X2D).
    #[serde(default)]
    pub dry_sf_reason: Option<Vec<i32>>,
}

impl AmsUnit {
    /// Merges a freshly-parsed `AmsUnit` into `self` field-by-field, instead of replacing
    /// `self` wholesale.
    ///
    /// BUG-098: confirmed against BambuStudio's own `DevFilaSystem.cpp` (`ParseAmsInfo`,
    /// ~L590-720) — every field here (`humidity_raw`, `dry_time` via `ParseVal`'s no-default
    /// overload, `dry_setting`, `dry_sf_reason`) is gated behind `.contains()` or `ParseVal`'s
    /// no-default overload against a persistent per-unit object, i.e. preserve-on-absence.
    /// `temp`/`humidity` aren't `Option` in this crate's model (they deserialize as required —
    /// a unit object omitting them entirely wouldn't parse as `AmsUnit` at all), so they
    /// always take the incoming value with no merge needed. `dry_time` specifically:
    /// `pybambu`'s own git history (`c517861` "Fix AMS2 updates") shows a hard `KeyError` on
    /// absence was replaced with a naive `.get(..., 0)` default to fix a real crash —
    /// confirming the field can be absent, but its own fix is the same naive-default class
    /// `bambuddy`'s `#1462` documents and corrects; 2 of 3 sources (BambuStudio, `bambuddy`)
    /// agree on preserve-on-absence as the correct handling.
    ///
    /// `tray` is now a keyed per-tray merge (not wholesale array replace), with pruning of
    /// cached tray ids absent from a *present* incoming array — confirmed against
    /// `ParseAmsInfo`'s `if (j_ams.contains("tray"))` block, which both keyed-merges
    /// (`curr_ams->GetTray(tray_id)`, create-or-reuse, field merge via `ParseAmsTrayInfo`) and
    /// prunes any previously-cached `tray_id` not present in `existing_tray_set` after the
    /// loop — but *only* when the `tray` key itself was present in this push (`tray: None`
    /// leaves the cached trays untouched entirely, matching every other field here).
    pub(crate) fn merge_from(&mut self, incoming: &AmsUnit) {
        self.temp = incoming.temp.clone();
        self.humidity = incoming.humidity.clone();
        if incoming.humidity_raw.is_some() {
            self.humidity_raw = incoming.humidity_raw.clone();
        }
        if incoming.dry_time.is_some() {
            self.dry_time = incoming.dry_time;
        }
        if incoming.dry_setting.is_some() {
            self.dry_setting = incoming.dry_setting.clone();
        }
        if let Some(incoming_trays) = &incoming.tray {
            let cached_trays = self.tray.get_or_insert_with(Vec::new);
            for incoming_tray in incoming_trays {
                match cached_trays.iter_mut().find(|t| t.id == incoming_tray.id) {
                    Some(cached_tray) => cached_tray.merge_from(incoming_tray),
                    None => cached_trays.push(incoming_tray.clone()),
                }
            }
            cached_trays.retain(|t| incoming_trays.iter().any(|it| it.id == t.id));
        }
        if incoming.info.is_some() {
            self.info = incoming.info.clone();
        }
        if incoming.dry_sf_reason.is_some() {
            self.dry_sf_reason = incoming.dry_sf_reason.clone();
        }
    }
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
const AMS_UNIT_INFO_DRY_SUB_STATUS_MASK: u64 = 0x3;

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

    /// Drying sub-status from bits 22–23. Bits 24–25 belong to the unrelated `bind_switch_in` field.
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

impl AmsTray {
    /// Merges a freshly-parsed `AmsTray` into `self` field-by-field, instead of replacing
    /// `self` wholesale.
    ///
    /// Confirmed against BambuStudio's `DevFilaSystem.cpp` (`ParseAmsTrayInfo`, ~L743-848) —
    /// every field is gated behind `DevJsonValParser::ParseVal`'s 3-arg (preserve-on-absence)
    /// overload, **except** `tag_uid`, `tray_uuid` (4-arg `ParseVal` with a `"0"` default) and
    /// `remain` (4-arg with a `-1` default), which BambuStudio resets to a fixed default
    /// whenever a push omits them. This merge deliberately does **not** replicate that
    /// reset-on-absence behavior for those three fields: a real P1S wire capture
    /// (`tests/mocks/P1S_print_sequence.ndjson`) shows minimal `{"id":"N"}`-only tray pushes
    /// are routine in normal incremental telemetry (no `tag_uid`/`remain`/anything else
    /// repeated), and applying BambuStudio's literal reset there would wipe a tray's RFID tag
    /// and remaining-percent on every such push — the exact "wholesale clobber on a partial
    /// push" staleness class already fixed at other levels of this tree (BUG-091/097/098).
    /// `tray_info_idx`/`tray_type` are similarly not coupled the way BambuStudio couples them
    /// (both-or-neither, tied to its own `setting_id`-driven `m_fila_type` resolution) — that
    /// coupling is BambuStudio-internal derived-field logic, not a raw preserve/reset merge
    /// rule, so it's out of scope for this intentionally "dumb" field-level merge. `state` has
    /// no BambuStudio counterpart at all (grepped, zero matches in `DevFilaSystem.cpp` for a
    /// tray-level `state` field) — preserved on absence like every field with no confirmed
    /// counterpart elsewhere in this codebase (BUG-097's precedent).
    pub(crate) fn merge_from(&mut self, incoming: &AmsTray) {
        if incoming.state.is_some() {
            self.state = incoming.state;
        }
        if incoming.tray_type.is_some() {
            self.tray_type = incoming.tray_type.clone();
        }
        if incoming.tray_color.is_some() {
            self.tray_color = incoming.tray_color.clone();
        }
        if incoming.tray_info_idx.is_some() {
            self.tray_info_idx = incoming.tray_info_idx.clone();
        }
        if incoming.tag_uid.is_some() {
            self.tag_uid = incoming.tag_uid.clone();
        }
        if incoming.tray_uuid.is_some() {
            self.tray_uuid = incoming.tray_uuid.clone();
        }
        if incoming.remain.is_some() {
            self.remain = incoming.remain;
        }
        if incoming.tray_sub_brands.is_some() {
            self.tray_sub_brands = incoming.tray_sub_brands.clone();
        }
        if incoming.nozzle_temp_max.is_some() {
            self.nozzle_temp_max = incoming.nozzle_temp_max.clone();
        }
        if incoming.nozzle_temp_min.is_some() {
            self.nozzle_temp_min = incoming.nozzle_temp_min.clone();
        }
        if incoming.tray_diameter.is_some() {
            self.tray_diameter = incoming.tray_diameter.clone();
        }
        if incoming.tray_weight.is_some() {
            self.tray_weight = incoming.tray_weight.clone();
        }
        if incoming.tray_id_name.is_some() {
            self.tray_id_name = incoming.tray_id_name.clone();
        }
        if incoming.tray_temp.is_some() {
            self.tray_temp = incoming.tray_temp.clone();
        }
        if incoming.tray_time.is_some() {
            self.tray_time = incoming.tray_time.clone();
        }
        if incoming.drying_temp.is_some() {
            self.drying_temp = incoming.drying_temp.clone();
        }
        if incoming.drying_time.is_some() {
            self.drying_time = incoming.drying_time.clone();
        }
        if incoming.bed_temp.is_some() {
            self.bed_temp = incoming.bed_temp.clone();
        }
        if incoming.bed_temp_type.is_some() {
            self.bed_temp_type = incoming.bed_temp_type.clone();
        }
        if incoming.xcam_info.is_some() {
            self.xcam_info = incoming.xcam_info.clone();
        }
        if incoming.k.is_some() {
            self.k = incoming.k;
        }
        if incoming.n.is_some() {
            self.n = incoming.n;
        }
        if incoming.cali_idx.is_some() {
            self.cali_idx = incoming.cali_idx;
        }
        if incoming.cols.is_some() {
            self.cols = incoming.cols.clone();
        }
        if incoming.ctype.is_some() {
            self.ctype = incoming.ctype;
        }
        if incoming.total_len.is_some() {
            self.total_len = incoming.total_len;
        }
    }
}
