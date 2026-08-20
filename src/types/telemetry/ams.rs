//! AMS telemetry types (tray slots, units, dry settings, virtual trays).

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// Per-slot filament-change step code. Mirrors BambuStudio's `DevFilamentStep` enum
/// (`DevDefs.h:64`) — used to type `AmsStatusReport.cfs`. `CheckPosition` covers both `0x08`
/// wire values (`STEP_CHECK_POSITION`/`STEP_CONFIRM_EXTRUDED` share the same discriminant in
/// the source enum). `Unknown` preserves any other raw value rather than failing to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AmsFilamentStep {
    /// No filament-change activity in progress.
    Idle,
    /// Change sequence paused.
    Pause,
    /// Heating the nozzle before the change.
    HeatNozzle,
    /// Cutting the current filament.
    CutFilament,
    /// Retracting the current filament out of the toolhead.
    PullCurrFilament,
    /// Feeding the new filament toward the toolhead.
    PushNewFilament,
    /// Grabbing the new filament at the AMS slot.
    GrabNewFilament,
    /// Purging leftover old filament from the nozzle.
    PurgeOldFilament,
    /// Verifying filament position (wire value `0x08`, shared with `STEP_CONFIRM_EXTRUDED`).
    CheckPosition,
    /// Switching to a different extruder (IDEX).
    SwitchExtruder,
    /// Switching to a different hotend (tool-changer).
    SwitchHotend,
    /// Cooling the filament inside the AMS unit.
    AmsFilaCooling,
    /// Pushing filament into the tool-changer switcher.
    PushSwitcherFila,
    /// Pulling filament out of the tool-changer switcher.
    PullSwitcherFila,
    /// Switching the tool-changer's active position.
    SwitcherSwitch,
    /// Any wire value not covered by a named variant, preserved verbatim.
    Unknown(i64),
}

impl From<i64> for AmsFilamentStep {
    fn from(raw: i64) -> Self {
        match raw {
            0x00 => Self::Idle,
            0x01 => Self::Pause,
            0x02 => Self::HeatNozzle,
            0x03 => Self::CutFilament,
            0x04 => Self::PullCurrFilament,
            0x05 => Self::PushNewFilament,
            0x06 => Self::GrabNewFilament,
            0x07 => Self::PurgeOldFilament,
            0x08 => Self::CheckPosition,
            0x09 => Self::SwitchExtruder,
            0x0A => Self::SwitchHotend,
            0x0B => Self::AmsFilaCooling,
            0x0C => Self::PushSwitcherFila,
            0x0D => Self::PullSwitcherFila,
            0x0E => Self::SwitcherSwitch,
            other => Self::Unknown(other),
        }
    }
}

impl From<AmsFilamentStep> for i64 {
    fn from(step: AmsFilamentStep) -> Self {
        match step {
            AmsFilamentStep::Idle => 0x00,
            AmsFilamentStep::Pause => 0x01,
            AmsFilamentStep::HeatNozzle => 0x02,
            AmsFilamentStep::CutFilament => 0x03,
            AmsFilamentStep::PullCurrFilament => 0x04,
            AmsFilamentStep::PushNewFilament => 0x05,
            AmsFilamentStep::GrabNewFilament => 0x06,
            AmsFilamentStep::PurgeOldFilament => 0x07,
            AmsFilamentStep::CheckPosition => 0x08,
            AmsFilamentStep::SwitchExtruder => 0x09,
            AmsFilamentStep::SwitchHotend => 0x0A,
            AmsFilamentStep::AmsFilaCooling => 0x0B,
            AmsFilamentStep::PushSwitcherFila => 0x0C,
            AmsFilamentStep::PullSwitcherFila => 0x0D,
            AmsFilamentStep::SwitcherSwitch => 0x0E,
            AmsFilamentStep::Unknown(other) => other,
        }
    }
}

impl Serialize for AmsFilamentStep {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i64(i64::from(*self))
    }
}

impl<'de> Deserialize<'de> for AmsFilamentStep {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        i64::deserialize(deserializer).map(AmsFilamentStep::from)
    }
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

    /// Whether AMS-side remaining-filament detection is enabled. Confirmed
    /// independently by `bambu-printer-manager` (`bambucommands.py:180`, `bambutools.py:90`)
    /// and `OpenBambuAPI/local-printer-api.md:317` (community protocol spec).
    #[serde(default)]
    pub calibrate_remain_flag: Option<bool>,

    /// Per-slot filament-change step codes. Confirmed against BambuStudio's
    /// `DevFilaSystem.cpp:507-508` (`GetVal<std::vector<DevFilamentStep>>(jj["ams"], "cfs")`);
    /// consistent with pybambu's `MOCK-X2D.json:184-189` fixture (`"cfs": [2, 9, 5, 7]`).
    #[serde(default)]
    pub cfs: Option<Vec<AmsFilamentStep>>,
}

impl AmsStatusReport {
    /// Merges a freshly-parsed `AmsStatusReport` into `self` field-by-field, instead of
    /// replacing `self` wholesale.
    ///
    /// Confirmed via a real P1S wire capture — an incremental `print.ams` push may
    /// carry only a subset of fields (e.g. `{"ams":{"tray_tar":"3"}}` during a tray-switch
    /// sequence), with `ams` (the unit/tray array, `#[serde(default)]`) and every other field
    /// simply absent rather than explicitly emptied. A caller that replaces its cached
    /// `AmsStatusReport` wholesale on any `print.ams: Some(_)` push loses the previously-known
    /// unit array and other fields on every such partial push. Mirrors the "each field
    /// independently keeps its most recently observed value" staleness policy `TelemetryCache`
    /// already documents at the `PrinterTelemetry` level, one layer deeper.
    ///
    /// `ams` itself is a keyed per-unit merge, not a wholesale array replace —
    /// see the loop body below and `AmsUnit::merge_from`.
    pub(crate) fn merge_from(&mut self, incoming: &AmsStatusReport) {
        if !incoming.ams.is_empty() {
            // Keyed per-unit merge, not wholesale replace — confirmed against
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
        if incoming.calibrate_remain_flag.is_some() {
            self.calibrate_remain_flag = incoming.calibrate_remain_flag;
        }
        if incoming.cfs.is_some() {
            self.cfs = incoming.cfs.clone();
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
    /// Confirmed against BambuStudio's own `DevFilaSystem.cpp` (`ParseAmsInfo`,
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
        if let Some(incoming_dry) = &incoming.dry_setting {
            match &mut self.dry_setting {
                Some(cached_dry) => cached_dry.merge_from(incoming_dry),
                None => self.dry_setting = Some(incoming_dry.clone()),
            }
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

impl AmsDrySetting {
    /// Merges a freshly-parsed `AmsDrySetting` into `self` field-by-field, mirroring
    /// `AmsTray::merge_from` -- a partial push (e.g. only `dry_temperature` mid-cycle) must not
    /// clobber cached fields the incoming object omits (issue #57).
    pub(crate) fn merge_from(&mut self, incoming: &AmsDrySetting) {
        if incoming.dry_temperature.is_some() {
            self.dry_temperature = incoming.dry_temperature;
        }
        if incoming.dry_duration.is_some() {
            self.dry_duration = incoming.dry_duration;
        }
        if incoming.dry_filament.is_some() {
            self.dry_filament = incoming.dry_filament.clone();
        }
    }
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

impl VirtualTray {
    /// Merges a freshly-parsed `VirtualTray` into `self` field-by-field, mirroring
    /// `AmsTray::merge_from` -- `VirtualTray` shares `AmsTray`'s wire schema, and BambuStudio's
    /// preserve-on-absence `ParseVal` behavior applies here too. A partial id-only push (the
    /// same shape routine `vt_tray` deltas use) must not wipe cached `tray_type`/`tray_color`/
    /// etc. that the printer never actually cleared (issue #43).
    pub(crate) fn merge_from(&mut self, incoming: &VirtualTray) {
        if incoming.id.is_some() {
            self.id = incoming.id.clone();
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
        if incoming.tray_temp.is_some() {
            self.tray_temp = incoming.tray_temp.clone();
        }
        if incoming.tray_time.is_some() {
            self.tray_time = incoming.tray_time.clone();
        }
        if incoming.bed_temp.is_some() {
            self.bed_temp = incoming.bed_temp.clone();
        }
        if incoming.bed_temp_type.is_some() {
            self.bed_temp_type = incoming.bed_temp_type.clone();
        }
        if incoming.tag_uid.is_some() {
            self.tag_uid = incoming.tag_uid.clone();
        }
        if incoming.tray_uuid.is_some() {
            self.tray_uuid = incoming.tray_uuid.clone();
        }
        if incoming.tray_id_name.is_some() {
            self.tray_id_name = incoming.tray_id_name.clone();
        }
        if incoming.xcam_info.is_some() {
            self.xcam_info = incoming.xcam_info.clone();
        }
        if incoming.remain.is_some() {
            self.remain = incoming.remain;
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
    }
}

/// Native state code meaning "slot empty" [REF-AMS-DECODE].
/// Lives here (not in `ams::parser`) since `AmsTray::state()` is a pure data accessor and
/// `types/` must not depend on business-logic modules.
pub(crate) const AMS_TRAY_STATE_EMPTY: u8 = 9;

/// Native state code meaning "spool physically present but not yet fed to the extruder"
/// [REF-AMS-DECODE]. On H2D-generation firmware this is one of the two explicit
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

    /// Accurate remaining weight in grams, when firmware can resolve it. Distinct
    /// from `remain`'s coarse percentage estimate. Confirmed against BambuStudio's
    /// `DevFilaSystem.cpp:800`/`.h:73` (`remain_g`, introduced in commit `31637e013`,
    /// "ENH: support accurate filament remain weight", 2026-06-12) — firmware sends `-1` for
    /// "not provided", preserved here as the raw wire value; use `remaining_weight_grams()`
    /// for the sentinel-translated `Option<u32>`.
    pub remain_g: Option<i32>,

    /// Filament preset ID BambuStudio resolves and prefers for print-preset auto-matching,
    /// distinct from `tray_info_idx`. Wire key is `setting_id`; renamed here to
    /// avoid confusion with `tray_info_idx`'s own doc name collision. Confirmed against
    /// BambuStudio's `DevFilaSystem.cpp:801` (`filament_setting_id`) and `DevMapping.cpp`
    /// (commit `d1f121d26`, 2026-06-09), which prefers this field over the coarser
    /// `filament_id` when auto-matching a spool to a slicer preset.
    #[serde(rename = "setting_id")]
    pub filament_setting_id: Option<String>,
}

/// Which Filament Track Switch inlet an AMS unit feeds through.
///
/// The FTS is an accessory that lets one AMS feed either printer nozzle through a shared switch,
/// instead of being wired to a fixed extruder. A unit routed this way reports `0xE`
/// ("not fixed") for its extruder assignment, and the inlet below is the only thing that says
/// which physical nozzle it actually reaches.
///
/// Deliberately not `Copy`-cheap-`u8` — the wire values (`0` = In-B, `1` = In-A) are inverted
/// relative to how the inlets read alphabetically, and every prior attempt to remember that from
/// a bare integer is a bug waiting to happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilamentSwitchInlet {
    /// Inlet In-A. Wire value `1`.
    InA,
    /// Inlet In-B. Wire value `0`.
    InB,
}

const AMS_UNIT_INFO_TYPE_MASK: u64 = 0xF;
const AMS_UNIT_INFO_DRY_STATUS_SHIFT: u32 = 4;
const AMS_UNIT_INFO_DRY_STATUS_MASK: u64 = 0xF;
const AMS_UNIT_INFO_EXTRUDER_SHIFT: u32 = 8;
const AMS_UNIT_INFO_EXTRUDER_MASK: u64 = 0xF;
const AMS_UNIT_INFO_EXTRUDER_UNINITIALIZED: u8 = 0xE;
const AMS_UNIT_INFO_DRY_SUB_STATUS_SHIFT: u32 = 22;
const AMS_UNIT_INFO_DRY_SUB_STATUS_MASK: u64 = 0x3;
const AMS_UNIT_INFO_DRY_FAN1_STATUS_SHIFT: u32 = 18;
const AMS_UNIT_INFO_DRY_FAN2_STATUS_SHIFT: u32 = 20;
const AMS_UNIT_INFO_DRY_FAN_STATUS_MASK: u64 = 0x3;
const AMS_UNIT_INFO_BIND_SWITCH_IN_SHIFT: u32 = 24;
/// Four bits wide, not two — see [`AmsUnit::filament_switch_inlet`] for why that matters.
const AMS_UNIT_INFO_BIND_SWITCH_IN_MASK: u64 = 0xF;
/// `bind_switch_in` value for the Filament Track Switch's In-B inlet.
const AMS_UNIT_INFO_SWITCH_INLET_B: u8 = 0;
/// `bind_switch_in` value for the Filament Track Switch's In-A inlet.
const AMS_UNIT_INFO_SWITCH_INLET_A: u8 = 1;

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

    /// Filament Track Switch inlet this unit feeds, decoded from `bind_switch_in` (bits 24–27).
    ///
    /// Returns [`FilamentSwitchInlet::InB`] for `0` and [`FilamentSwitchInlet::InA`] for `1`;
    /// `None` for `info` absent, or any other value, which upstream treats as "not bound".
    ///
    /// **Only meaningful when [`extruder_assignment`](Self::extruder_assignment) returns `None`
    /// because the raw field is `0xE`.** An AMS wired to a fixed extruder reports that extruder
    /// directly and this field carries nothing; `0xE` means "not fixed", and when a Filament
    /// Track Switch is installed, this is the only way to recover which physical nozzle the unit
    /// actually feeds. That matters beyond display: BambuStudio uses the resolved inlet to pick
    /// the K-profile for the feeding nozzle. Note `extruder_assignment` collapses `0xE` into
    /// `None` and cannot distinguish "uninitialized" from "routed through a switch", so a caller
    /// wanting that distinction must consult this method as well.
    ///
    /// The field is four bits, not the two this crate documented before BUG-136 — a 2-bit read
    /// aliases values 4–15 into 0–3 and reports a valid inlet for a unit that has none.
    ///
    /// **Unverified against hardware.** No Filament Track Switch has been available; the decode
    /// follows BambuStudio's `DevFilaSystem.cpp:598-609`, corroborated by bambuddy (`c5e00558`,
    /// `7a42e0a7`). See issue #137.
    pub fn filament_switch_inlet(&self) -> Option<FilamentSwitchInlet> {
        self.parse_info().and_then(|v| {
            let raw = ((v >> AMS_UNIT_INFO_BIND_SWITCH_IN_SHIFT)
                & AMS_UNIT_INFO_BIND_SWITCH_IN_MASK) as u8;
            match raw {
                AMS_UNIT_INFO_SWITCH_INLET_B => Some(FilamentSwitchInlet::InB),
                AMS_UNIT_INFO_SWITCH_INLET_A => Some(FilamentSwitchInlet::InA),
                _ => None,
            }
        })
    }

    /// True when this unit reports `0xE` ("not wired to a fixed extruder") in bits 8–11.
    ///
    /// Distinguishes the two cases [`extruder_assignment`](Self::extruder_assignment) folds into
    /// `None`: a unit routed through a Filament Track Switch, versus one whose assignment the
    /// firmware simply has not initialized. Pair with
    /// [`filament_switch_inlet`](Self::filament_switch_inlet) to tell them apart — an unbound
    /// `bind_switch_in` alongside `0xE` means uninitialized.
    pub fn has_unfixed_extruder(&self) -> bool {
        self.parse_info().is_some_and(|v| {
            ((v >> AMS_UNIT_INFO_EXTRUDER_SHIFT) & AMS_UNIT_INFO_EXTRUDER_MASK) as u8
                == AMS_UNIT_INFO_EXTRUDER_UNINITIALIZED
        })
    }

    /// Drying sub-status from bits 22–23.
    pub fn dry_sub_status(&self) -> Option<u8> {
        self.parse_info().map(|v| {
            ((v >> AMS_UNIT_INFO_DRY_SUB_STATUS_SHIFT) & AMS_UNIT_INFO_DRY_SUB_STATUS_MASK) as u8
        })
    }

    /// Dry-fan 1 status from bits 18–19. Confirmed against BambuStudio's
    /// `DevFilaSystem.cpp:696` (`get_flag_bits(info, 18, 2)`) and independently by
    /// `bambu-printer-manager`'s `bambutools.py:685`, an exact match.
    pub fn dry_fan1_status(&self) -> Option<u8> {
        self.parse_info().map(|v| {
            ((v >> AMS_UNIT_INFO_DRY_FAN1_STATUS_SHIFT) & AMS_UNIT_INFO_DRY_FAN_STATUS_MASK) as u8
        })
    }

    /// Dry-fan 2 status from bits 20–21. Confirmed against BambuStudio's
    /// `DevFilaSystem.cpp:697` (`get_flag_bits(info, 20, 2)`) and independently by
    /// `bambu-printer-manager`'s `bambutools.py:686`, an exact match.
    pub fn dry_fan2_status(&self) -> Option<u8> {
        self.parse_info().map(|v| {
            ((v >> AMS_UNIT_INFO_DRY_FAN2_STATUS_SHIFT) & AMS_UNIT_INFO_DRY_FAN_STATUS_MASK) as u8
        })
    }
}

impl AmsTray {
    /// Retrieves the status code of the spool, defaulting to `9` (Empty) if omitted.
    ///
    /// This handles symmetrical empty slots safely on standard P1S and A1 Mini lines.
    pub fn state(&self) -> u8 {
        self.state.unwrap_or(AMS_TRAY_STATE_EMPTY)
    }
}

impl AmsTray {
    /// Accurate remaining weight in grams, translating `remain_g`'s raw wire
    /// sentinel to `None`. Mirrors BambuStudio's `DevAmsTray::get_filament_remain_weight()`
    /// (`DevFilaSystem.cpp:116-124`): `remain_g < 0` means "not provided by firmware" and
    /// `remain_g == 0` means "confirmed empty," both `None` here; only a positive value is
    /// returned. Does not replicate BambuStudio's percentage-based fallback (`weight * remain
    /// / 100`) when `remain_g` is absent — callers needing that estimate already have
    /// `tray_weight`/`remain` to compute it themselves.
    pub fn remaining_weight_grams(&self) -> Option<u32> {
        self.remain_g.filter(|g| *g > 0).map(|g| g as u32)
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
    /// push" staleness class already fixed at other levels of this tree.
    /// `tray_info_idx`/`tray_type` are similarly not coupled the way BambuStudio couples them
    /// (both-or-neither, tied to its own `setting_id`-driven `m_fila_type` resolution) — that
    /// coupling is BambuStudio-internal derived-field logic, not a raw preserve/reset merge
    /// rule, so it's out of scope for this intentionally "dumb" field-level merge. `state` has
    /// no BambuStudio counterpart at all (grepped, zero matches in `DevFilaSystem.cpp` for a
    /// tray-level `state` field) — preserved on absence like every field with no confirmed
    /// counterpart elsewhere in this codebase. `remain_g`/
    /// `filament_setting_id` preserve-on-absence like every other field with a
    /// confirmed 3-arg `ParseVal` counterpart (`DevFilaSystem.cpp:800-801`).
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
        if incoming.remain_g.is_some() {
            self.remain_g = incoming.remain_g;
        }
        if incoming.filament_setting_id.is_some() {
            self.filament_setting_id = incoming.filament_setting_id.clone();
        }
    }
}
