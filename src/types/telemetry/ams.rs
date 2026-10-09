//! AMS telemetry types (tray slots, units, dry settings, virtual trays).

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::merge::{Mergeable, keep_new, merge_keyed, merge_opt};
use serde::{Deserialize, Deserializer, Serialize};

/// Normalizes `AmsUnit::id` on the way in, so the whole crate addresses one id per unit.
///
/// The only id this rewrites is an A2L-attached AMS Lite's physical 16, which becomes 6 — see
/// [`crate::ams::normalize_ams_unit_id`] for why, and for where the physical id is restored.
/// A non-numeric id is passed through untouched rather than rejected: this field is a
/// `String` on the wire and failing here would discard the entire telemetry frame.
fn deserialize_normalized_ams_unit_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    match raw.trim().parse::<u8>() {
        Ok(id) => {
            let normalized = crate::ams::normalize_ams_unit_id(id);
            if normalized == id {
                Ok(raw)
            } else {
                #[cfg(not(feature = "std"))]
                use alloc::string::ToString;
                Ok(normalized.to_string())
            }
        }
        Err(_) => Ok(raw),
    }
}

/// Per-slot filament-change step code. Mirrors BambuStudio's `DevFilamentStep` enum
/// (`DevDefs.h`) — used to type `AmsStatusReport.cfs`. `CheckPosition` covers both `0x08`
/// wire values (`STEP_CHECK_POSITION`/`STEP_CONFIRM_EXTRUDED` share the same discriminant in
/// the source enum). `Unknown` preserves any other raw value rather than failing to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "i64", into = "i64")]
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

/// Wire code of every named [`AmsFilamentStep`], the single table both conversions read.
const FILAMENT_STEP_CODES: [(i64, AmsFilamentStep); 15] = [
    (0x00, AmsFilamentStep::Idle),
    (0x01, AmsFilamentStep::Pause),
    (0x02, AmsFilamentStep::HeatNozzle),
    (0x03, AmsFilamentStep::CutFilament),
    (0x04, AmsFilamentStep::PullCurrFilament),
    (0x05, AmsFilamentStep::PushNewFilament),
    (0x06, AmsFilamentStep::GrabNewFilament),
    (0x07, AmsFilamentStep::PurgeOldFilament),
    (0x08, AmsFilamentStep::CheckPosition),
    (0x09, AmsFilamentStep::SwitchExtruder),
    (0x0A, AmsFilamentStep::SwitchHotend),
    (0x0B, AmsFilamentStep::AmsFilaCooling),
    (0x0C, AmsFilamentStep::PushSwitcherFila),
    (0x0D, AmsFilamentStep::PullSwitcherFila),
    (0x0E, AmsFilamentStep::SwitcherSwitch),
];

impl From<i64> for AmsFilamentStep {
    fn from(raw: i64) -> Self {
        FILAMENT_STEP_CODES
            .iter()
            .find(|(code, _)| *code == raw)
            .map_or(Self::Unknown(raw), |&(_, step)| step)
    }
}

impl From<AmsFilamentStep> for i64 {
    fn from(step: AmsFilamentStep) -> Self {
        match step {
            AmsFilamentStep::Unknown(raw) => raw,
            named => FILAMENT_STEP_CODES
                .iter()
                .find(|(_, s)| *s == named)
                .map(|&(code, _)| code)
                .expect("every named AmsFilamentStep is in FILAMENT_STEP_CODES"),
        }
    }
}

/// Top-level AMS status wrapper containing the units array and bus-wide metadata [REF-AMS-DECODE].
///
/// On the wire, AMS telemetry is nested as `print.ams.ams[...]` — this struct represents
/// the intermediate `print.ams` object.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    pub tray_tar: Option<String>,

    /// AMS protocol version.
    pub version: Option<i32>,

    /// RFID read completion bitmask (hex string).
    pub tray_read_done_bits: Option<String>,

    /// Active RFID read bitmask (hex string), in the same bit layout as `tray_exist_bits`
    /// ([REF-AMS-DECODE]).
    pub tray_reading_bits: Option<String>,

    /// AMS insertion event flag.
    pub insert_flag: Option<bool>,

    /// AMS unit external power state (distinct from printer power; AMS Pro needs external power for drying).
    pub power_on_flag: Option<bool>,

    /// Calibration tracking ID.
    pub cali_id: Option<i32>,

    /// Calibration tracking status.
    pub cali_stat: Option<i32>,

    /// Whether AMS-side remaining-filament detection is enabled. Confirmed
    /// independently by `bambu-printer-manager` (`bambucommands.py`, `bambutools.py`)
    /// and `OpenBambuAPI/local-printer-api.md` (community protocol spec).
    pub calibrate_remain_flag: Option<bool>,

    /// Per-slot filament-change step codes. Confirmed against BambuStudio's
    /// `DevFilaSystem.cpp` (`GetVal<std::vector<DevFilamentStep>>(jj["ams"], "cfs")`);
    /// consistent with pybambu's `MOCK-X2D.json:184-189` fixture (`"cfs": [2, 9, 5, 7]`).
    pub cfs: Option<Vec<AmsFilamentStep>>,
}

impl AmsStatusReport {
    /// The unit at bus id `ams_id`, as normalized on ingest (an A2L's AMS Lite is `6`).
    #[must_use]
    pub fn unit(&self, ams_id: u8) -> Option<&AmsUnit> {
        self.ams.iter().find(|unit| unit.ams_id() == Some(ams_id))
    }

    /// Slot `slot` of the unit at `ams_id`, if both were reported.
    #[must_use]
    pub fn tray(&self, ams_id: u8, slot: u8) -> Option<&AmsTray> {
        self.unit(ams_id)?
            .tray
            .as_deref()?
            .iter()
            .find(|tray| tray.slot() == Some(slot))
    }
}

impl Mergeable for AmsStatusReport {
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
    fn merge_from(&mut self, incoming: &Self) {
        let Self {
            ams,
            ams_exist_bits,
            tray_exist_bits,
            tray_is_bbl_bits,
            tray_now,
            tray_pre,
            tray_tar,
            version,
            tray_read_done_bits,
            tray_reading_bits,
            insert_flag,
            power_on_flag,
            cali_id,
            cali_stat,
            calibrate_remain_flag,
            cfs,
        } = incoming;
        if !ams.is_empty() {
            // Keyed per-unit merge, not wholesale replace — confirmed against
            // BambuStudio's own `DevFilaSystem.cpp` (`ParseAmsInfo`), which looks up each
            // unit by `ams_id` in a persistent `amsList` map (`system->amsList.find(ams_id)`)
            // that's never pruned by a push's contents; a unit not mentioned in a given
            // `print.ams.ams` push stays cached exactly as last observed, and a mentioned
            // unit's own fields merge in via `AmsUnit::merge_from` rather than replacing the
            // whole unit.
            merge_keyed(&mut self.ams, ams, |u| u.id.clone());
        }
        keep_new(&mut self.ams_exist_bits, ams_exist_bits);
        keep_new(&mut self.tray_exist_bits, tray_exist_bits);
        keep_new(&mut self.tray_is_bbl_bits, tray_is_bbl_bits);
        keep_new(&mut self.tray_now, tray_now);
        keep_new(&mut self.tray_pre, tray_pre);
        keep_new(&mut self.tray_tar, tray_tar);
        keep_new(&mut self.version, version);
        keep_new(&mut self.tray_read_done_bits, tray_read_done_bits);
        keep_new(&mut self.tray_reading_bits, tray_reading_bits);
        keep_new(&mut self.insert_flag, insert_flag);
        keep_new(&mut self.power_on_flag, power_on_flag);
        keep_new(&mut self.cali_id, cali_id);
        keep_new(&mut self.cali_stat, cali_stat);
        keep_new(&mut self.calibrate_remain_flag, calibrate_remain_flag);
        keep_new(&mut self.cfs, cfs);
    }
}

/// Modular standard expansion unit managing up to 4 physical spool slots.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AmsUnit {
    /// Unique index representing the unit position on the physical expansion bus.
    ///
    /// Standard AMS units report 0-3 and AMS-HT units 128-135, both verbatim. The A2L's AMS
    /// Lite reports physical id **16** on the wire and is normalized to **6** here, so that
    /// `tray_exist_bits` (whose bit base for this unit is 24 = `6 * 4`), `resolve_global_tray_id`
    /// and the mapping builders all agree; `MaterialSource::AmsLite` puts the physical 16 back
    /// on the outbound `ams_mapping2`.
    #[serde(deserialize_with = "deserialize_normalized_ams_unit_id")]
    pub id: String,

    /// Ambient temperature inside the expansion enclosure, in degrees Celsius.
    ///
    /// Optional because BambuStudio reads it only when present (`ParseAmsInfo`,
    /// `DevFilaSystem.cpp`): a partial unit push without it must not fail the frame.
    pub temp: Option<String>,

    /// Enclosure climate relative humidity index (1-5 scale). Optional, as for `temp`.
    pub humidity: Option<String>,

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
    /// (per `AmsUnit::merge_from`) prunes every cached tray for this unit — `Option` gives
    /// exactly this absent-vs-present-empty distinction for free (absent key -> `None`,
    /// present key -> `Some(_)` however short), confirmed against BambuStudio's `DevFilaSystem.cpp`
    /// (`ParseAmsInfo`'s `if (j_ams.contains("tray"))` gate around both the per-tray parse
    /// loop and the prune-absent-ids loop).
    pub tray: Option<Vec<AmsTray>>,

    /// Hex-encoded bitmask: bits 0–3 = AMS type, bits 4–7 = dry_status, bits 8–11 = extruder assignment (IDEX routing).
    pub info: Option<String>,

    /// Drying failure reason codes per slot (X2D).
    pub dry_sf_reason: Option<Vec<i32>>,
}

impl Mergeable for AmsUnit {
    /// Merges a freshly-parsed `AmsUnit` into `self` field-by-field, instead of replacing
    /// `self` wholesale.
    ///
    /// Confirmed against BambuStudio's own `DevFilaSystem.cpp` (`ParseAmsInfo`,
    /// ~L590-720) — every field here (`temp`, `humidity`, `humidity_raw`, `dry_time` via
    /// `ParseVal`'s no-default overload, `dry_setting`, `dry_sf_reason`) is gated behind
    /// `.contains()` or `ParseVal`'s no-default overload against a persistent per-unit object,
    /// i.e. preserve-on-absence. `dry_time` specifically:
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
    fn merge_from(&mut self, incoming: &Self) {
        let Self {
            // The caller matched this unit by id before merging.
            id: _,
            temp,
            humidity,
            humidity_raw,
            dry_time,
            dry_setting,
            tray,
            info,
            dry_sf_reason,
        } = incoming;
        keep_new(&mut self.temp, temp);
        keep_new(&mut self.humidity, humidity);
        keep_new(&mut self.humidity_raw, humidity_raw);
        keep_new(&mut self.dry_time, dry_time);
        merge_opt(&mut self.dry_setting, dry_setting);
        if let Some(incoming_trays) = tray {
            let cached_trays = self.tray.get_or_insert_with(Vec::new);
            merge_keyed(cached_trays, incoming_trays, |t| t.id.clone());
            cached_trays.retain(|t| incoming_trays.iter().any(|it| it.id == t.id));
        }
        keep_new(&mut self.info, info);
        keep_new(&mut self.dry_sf_reason, dry_sf_reason);
    }
}

/// Drying cycle configuration embedded within AMS unit telemetry [REF-AMS-DRYER].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AmsDrySetting {
    /// Target drying temperature in degrees Celsius.
    pub dry_temperature: Option<i32>,
    /// Configured drying duration in hours (firmware range 1-24), not minutes.
    ///
    /// Confirmed against BambuStudio, which declares the same field as `dry_hour`
    /// (`DeviceCore/DevFilaSystem.h`, comment "hours") and parses it unscaled
    /// (`DevFilaSystem.cpp`), with a "1-24 h" UI input hint (`AMSDryControl.cpp`);
    /// ha-bambulab likewise exposes it as a `UnitOfTime.HOURS` sensor with the value
    /// taken unmodified off the wire.
    pub dry_duration: Option<i32>,
    /// Filament type string for the active drying profile (e.g. "PA-CF").
    pub dry_filament: Option<String>,
}

impl Mergeable for AmsDrySetting {
    /// Merges a freshly-parsed `AmsDrySetting` into `self` field-by-field, mirroring
    /// `AmsTray::merge_from` -- a partial push (e.g. only `dry_temperature` mid-cycle) must not
    /// clobber cached fields the incoming object omits (issue #57).
    fn merge_from(&mut self, incoming: &Self) {
        let Self {
            dry_temperature,
            dry_duration,
            dry_filament,
        } = incoming;
        keep_new(&mut self.dry_temperature, dry_temperature);
        keep_new(&mut self.dry_duration, dry_duration);
        keep_new(&mut self.dry_filament, dry_filament);
    }
}

/// External spool holder: `vt_tray` on single-nozzle models, each `vir_slot` entry on IDEX.
///
/// Its wire schema is an AMS tray's, so it is the same type, with the same fields, accessors and
/// merge. `id` is the holder's address (`"254"`/`"255"`), or empty if a push omitted it.
pub type VirtualTray = AmsTray;

/// Native state code meaning "slot empty" [REF-AMS-DECODE].
///
/// The tray states and the loaded-spool rule live here, with the data they read, so `types/`
/// depends only on the dependency-free `ams::ids` and not on `ams::parser`.
pub(crate) const AMS_TRAY_STATE_EMPTY: u8 = 9;

/// Native state code reported by a powered-off AMS: always treated as no spool, on every unit
/// type including AMS-HT (`reference/05_materials_ams.md`).
pub(crate) const AMS_TRAY_STATE_POWER_OFF: u8 = 0;

/// True for a `tray_type` that explicitly reports no material: empty, or the literal `"Empty"`.
fn is_blank_type(tray_type: &str) -> bool {
    tray_type.is_empty() || tray_type == "Empty"
}

/// Native state code meaning "spool physically present but not yet fed to the extruder"
/// [REF-AMS-DECODE]. On H2D-generation firmware this is one of the two explicit
/// "not loaded" signals alongside `AMS_TRAY_STATE_EMPTY` — a spool present in state 10 may
/// still have unconfirmed/stale metadata attached, so it's treated as an absent-equivalent
/// state for stale-data cleansing purposes, same as `AMS_TRAY_STATE_EMPTY`. On AMS-HT units
/// (`ams_id` 128-135) neither state 9 nor state 10 is a clearing signal (issue #181) — see
/// `clean_stale_tray_data`. Verified against `pybambu`/`Bambuddy`'s independent
/// reverse-engineering (`bambu_mqtt.py`'s `apply_tray_exist_bits` and incremental-merge
/// handler, `main.py`'s `on_ams_change` — `loaded = cur_state == 11 or (cur_state not in
/// (9, 10) and cur_type.strip())`, cross-tested against H2D, A1 Mini, and P1S firmware,
/// citing upstream issues #784/#1322).
pub(crate) const AMS_TRAY_STATE_SPOOL_NOT_FED: u8 = 10;

/// Material spool state descriptor representing a single physical tray slot.
///
/// On the wire, AMS trays and virtual/external trays (`vt_tray`, `vir_slot`)
/// share the same field schema. All descriptive fields are optional — under
/// standard P1/A1 firmware, removing a spool truncates the JSON to only the ID key.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AmsTray {
    /// The physical index representing the slot (0 to 3), or an external holder's address
    /// (`"254"`/`"255"`) for a [`VirtualTray`]. Sent as a string on the wire; empty if omitted.
    #[serde(default)]
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

    /// Calibration index; -1 means no K profile is selected.
    ///
    /// -1 is not only "never calibrated": an X1C power-cycled mid-print came back with every
    /// tray at -1 while the spools were unchanged (bambuddy #3219).
    pub cali_idx: Option<i32>,

    /// Multi-color columns array (e.g. `["000000FF"]`).
    pub cols: Option<Vec<String>>,

    /// Color type indicator.
    pub ctype: Option<i32>,

    /// Total filament spool length in mm.
    pub total_len: Option<u32>,

    /// Accurate remaining weight in grams, when firmware can resolve it. Distinct
    /// from `remain`'s coarse percentage estimate. Confirmed against BambuStudio's
    /// `DevFilaSystem.cpp`/`.h` (`remain_g`, introduced in commit `31637e013`,
    /// "ENH: support accurate filament remain weight", 2026-06-12) — firmware sends `-1` for
    /// "not provided", preserved here as the raw wire value; use `remaining_weight_grams()`
    /// for the sentinel-translated `Option<u32>`.
    pub remain_g: Option<i32>,

    /// Filament preset ID BambuStudio resolves and prefers for print-preset auto-matching,
    /// distinct from `tray_info_idx`. Wire key is `setting_id`; renamed here to
    /// avoid confusion with `tray_info_idx`'s own doc name collision. Confirmed against
    /// BambuStudio's `DevFilaSystem.cpp` (`filament_setting_id`) and `DevMapping.cpp`
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
/// An enum rather than a bare `u8` because the wire values (`0` = In-B, `1` = In-A) are inverted
/// relative to how the inlets read alphabetically, and every prior attempt to remember that from
/// a bare integer is a bug waiting to happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilamentSwitchInlet {
    /// Inlet In-A. Wire value `1`.
    InA,
    /// Inlet In-B. Wire value `0`.
    InB,
}

/// Minimum drying-chamber temperature (°C) accepted by an AMS unit's built-in heater.
///
/// Applies to both drying units. Confirmed via BambuStudio's own input validation
/// (`AMSDryControl.cpp`, and the field hints at 1442-1445 spelling out `"45-65°C"`
/// for the AMS 2 Pro and `"45-85°C"` for the AMS-HT).
pub(crate) const AMS_DRY_TEMP_MIN: u32 = 45;

/// Maximum drying-chamber temperature (°C) for an AMS-HT (`N3S`) unit's built-in heater.
///
/// Confirmed via Bambu Lab's own wiki (`wiki.bambulab.com/en/ams-ht/Intr-to-ams-ht-workflow-and-features`)
/// and BambuStudio's input validation (`AMSDryControl.cpp`). This is a property of the
/// physical AMS-HT hardware, not the host printer model.
pub(crate) const AMS_HT_DRY_TEMP_MAX: u32 = 85;

/// Maximum drying-chamber temperature (°C) for an AMS 2 Pro (`N3F`) unit's built-in heater.
///
/// Confirmed via Bambu Lab's own wiki (`wiki.bambulab.com/en/ams-2-pro/manual/drying-function`)
/// and BambuStudio's input validation (`AMSDryControl.cpp`). Property of the physical
/// AMS 2 Pro hardware, not the host printer model.
pub(crate) const AMS_STANDARD_DRY_TEMP_MAX: u32 = 65;

/// Which physical AMS accessory is attached, decoded from `info` bits 0–3.
///
/// **A property of the accessory, not of the host printer.** The quirks engine answers questions
/// about the printer; this answers questions about the box plugged into it, and the two are
/// orthogonal. Remote drying in particular needs *both* gates to pass: an AMS that physically has
/// a heater (here) and a printer whose firmware acts on the command rather than acking and
/// discarding it (`ModelQuirks::ams_remote_drying_support`). BambuStudio writes the same pair out
/// longhand at `Widgets/AMSControl.cpp`.
///
/// Do not infer any of this from `ams_id`: `0..=3` is shared by the original AMS, the AMS Lite and
/// the AMS 2 Pro, and only the last of those can dry.
///
/// Wire numbering matches BambuStudio's `DevAmsType` (`DevDefs.h`), which casts these four
/// bits straight to it (`DevFilaSystem.cpp`). bambuddy reaches the same taxonomy by an
/// independent route — the `info` module-name prefix, `"ams"`/`"n3f"`/`"n3s"`
/// (`bambu_mqtt.py`) — and ha-bambulab spells out the full prefix map (`ams/N`,
/// `ams_f1/N`, `n3f/N`, `n3s/N`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmsUnitModel {
    /// External spool / no unit. Wire value `0` (BambuStudio `EXT_SPOOL`).
    ExternalSpool,
    /// The original 4-slot AMS. Wire value `1`. **No drying chamber.**
    Ams,
    /// AMS Lite, as shipped with the A1 series. Wire value `2`. No drying chamber.
    AmsLite,
    /// AMS 2 Pro. Wire value `3` (BambuStudio `N3F`). 4 slots, dries.
    Ams2Pro,
    /// AMS-HT. Wire value `4` (BambuStudio `N3S`). Single slot, dries, higher ceiling.
    AmsHt,
    /// AMS Lite variant for N9. Wire value `5` (BambuStudio `AMS_LITE_MIXED`). No drying chamber.
    AmsLiteMixed,
}

impl AmsUnitModel {
    /// Decodes a raw `info` bits 0–3 value, or `None` for a unit type this crate doesn't know.
    ///
    /// An unknown value is deliberately not folded onto a neighbouring variant — firmware has
    /// added unit types before ([`AmsLiteMixed`](Self::AmsLiteMixed) being the most recent), and
    /// guessing a capability for one is how a drying command reaches a unit that can't dry. Read
    /// [`AmsUnit::ams_type`] for the raw value when this returns `None`.
    #[must_use]
    pub fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::ExternalSpool),
            1 => Some(Self::Ams),
            2 => Some(Self::AmsLite),
            3 => Some(Self::Ams2Pro),
            4 => Some(Self::AmsHt),
            5 => Some(Self::AmsLiteMixed),
            _ => None,
        }
    }

    /// Returns true if this unit has a drying chamber at all.
    ///
    /// True for [`Ams2Pro`](Self::Ams2Pro) and [`AmsHt`](Self::AmsHt) only. The original AMS and
    /// both AMS Lite variants have no heater, so a drying command addressed to one cannot do
    /// anything. Confirmed by BambuStudio (`Widgets/AMSItem.hpp`,
    /// `support_drying() { return ams_type == N3S || ams_type == N3F; }`) and independently by
    /// bambuddy (`print_scheduler.py`, `if module_type not in ("n3f", "n3s"): skip`).
    #[must_use]
    pub fn supports_drying(self) -> bool {
        self.drying_column().is_some()
    }

    /// Which column of a material's drying profile applies to this unit: `0` for the AMS 2 Pro
    /// (BambuStudio `N3F`), `1` for the AMS-HT (`N3S`), `None` without a drying chamber.
    ///
    /// The one exhaustive statement of which units dry; [`supports_drying`](Self::supports_drying)
    /// and [`DryingMaterial`](crate::types::DryingMaterial)'s profile lookups derive from it, and a
    /// test ties [`dry_temp_range`](Self::dry_temp_range) to it.
    pub(crate) const fn drying_column(self) -> Option<usize> {
        match self {
            Self::Ams2Pro => Some(0),
            Self::AmsHt => Some(1),
            Self::ExternalSpool | Self::Ams | Self::AmsLite | Self::AmsLiteMixed => None,
        }
    }

    /// Inclusive `(min, max)` drying-chamber temperature range in °C, or `None` if this unit
    /// cannot dry.
    ///
    /// `(45, 65)` for the AMS 2 Pro and `(45, 85)` for the AMS-HT. **Both bounds are real** —
    /// BambuStudio refuses a temperature below the minimum just as it refuses one above the
    /// maximum (`AMSDryControl.cpp`), so a caller clamping only the ceiling still
    /// publishes values the vendor's own client rejects.
    #[must_use]
    pub fn dry_temp_range(self) -> Option<(u32, u32)> {
        match self {
            Self::Ams2Pro => Some((AMS_DRY_TEMP_MIN, AMS_STANDARD_DRY_TEMP_MAX)),
            Self::AmsHt => Some((AMS_DRY_TEMP_MIN, AMS_HT_DRY_TEMP_MAX)),
            Self::ExternalSpool | Self::Ams | Self::AmsLite | Self::AmsLiteMixed => None,
        }
    }

    /// Spool slots this unit type has, or `None` where the type alone doesn't determine it.
    ///
    /// `1` for the AMS-HT, `4` for the original AMS, AMS Lite and AMS 2 Pro. `None` for
    /// [`ExternalSpool`](Self::ExternalSpool) and [`AmsLiteMixed`](Self::AmsLiteMixed): upstream
    /// has no static answer for those either and falls back to the observed tray count
    /// (BambuStudio `DevAms::GetSlotCount`), so count [`AmsUnit::tray`] rather than trusting a
    /// number invented here.
    #[must_use]
    pub fn slot_count(self) -> Option<u8> {
        match self {
            Self::AmsHt => Some(1),
            Self::Ams | Self::AmsLite | Self::Ams2Pro => Some(crate::ams::ids::AMS_SLOTS_PER_UNIT),
            Self::ExternalSpool | Self::AmsLiteMixed => None,
        }
    }
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

/// Drying-cycle state from `info` bits 4–7, BambuStudio's `DevAms::DryStatus`
/// (`DevFilaSystem.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmsDryStatus {
    /// `0` — not drying.
    Off,
    /// `1` — checking conditions before starting.
    Checking,
    /// `2` — drying.
    Drying,
    /// `3` — cooling down after a cycle.
    Cooling,
    /// `4` — stopping.
    Stopping,
    /// `5` — the cycle hit an error.
    Error,
    /// `6` — the heater could not be stopped (BambuStudio `CannotStopHeatOutofControl`).
    HeaterOutOfControl,
    /// `7` — factory production test (BambuStudio `PrdTesting`).
    ProductionTest,
    /// A value this crate doesn't know, preserved verbatim.
    Other(u8),
}

impl AmsDryStatus {
    fn from_wire(raw: u8) -> Self {
        match raw {
            0 => Self::Off,
            1 => Self::Checking,
            2 => Self::Drying,
            3 => Self::Cooling,
            4 => Self::Stopping,
            5 => Self::Error,
            6 => Self::HeaterOutOfControl,
            7 => Self::ProductionTest,
            other => Self::Other(other),
        }
    }
}

/// Drying sub-state from `info` bits 22–23, BambuStudio's `DevAms::DrySubStatus`
/// (`DevFilaSystem.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmsDrySubStatus {
    /// `0` — idle.
    Off,
    /// `1` — heating.
    Heating,
    /// `2` — dehumidifying.
    Dehumidifying,
    /// A value this crate doesn't know (`3`), preserved verbatim.
    Other(u8),
}

impl AmsDrySubStatus {
    fn from_wire(raw: u8) -> Self {
        match raw {
            0 => Self::Off,
            1 => Self::Heating,
            2 => Self::Dehumidifying,
            other => Self::Other(other),
        }
    }
}

/// State of one drying fan from `info` bits 18–19 or 20–21, BambuStudio's
/// `DevAms::DryFanStatus` (`DevFilaSystem.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmsDryFanStatus {
    /// `0` — off.
    Off,
    /// `1` — on.
    On,
    /// A value this crate doesn't know (`2` or `3`), preserved verbatim.
    Other(u8),
}

impl AmsDryFanStatus {
    fn from_wire(raw: u8) -> Self {
        match raw {
            0 => Self::Off,
            1 => Self::On,
            other => Self::Other(other),
        }
    }
}

impl AmsUnit {
    /// The unit's bus id parsed from [`id`](Self::id), or `None` if it isn't a number.
    #[must_use]
    pub fn ams_id(&self) -> Option<u8> {
        self.id.trim().parse().ok()
    }

    /// Enclosure temperature in °C, from the `temp` string.
    #[must_use]
    pub fn temperature_c(&self) -> Option<f32> {
        self.temp.as_deref()?.trim().parse().ok()
    }

    /// Relative humidity in percent, from `humidity_raw`.
    #[must_use]
    pub fn humidity_percent(&self) -> Option<u8> {
        self.humidity_raw.as_deref()?.trim().parse().ok()
    }

    /// Coarse humidity level `1..=5` from `humidity`, where **`1` is wettest and `5` driest**
    /// (`reference/05_materials_ams.md`, "Per-Unit Humidity").
    #[must_use]
    pub fn humidity_level(&self) -> Option<u8> {
        self.humidity.as_deref()?.trim().parse().ok()
    }

    /// Parses the hex-encoded `info` bitmask string into an integer.
    #[must_use]
    pub fn parse_info(&self) -> Option<u64> {
        self.info.as_deref().and_then(super::bits::hex_u64)
    }

    /// Extracts `(info >> shift) & mask`, every mask here being at most 4 bits wide.
    fn info_field(&self, shift: u32, mask: u64) -> Option<u8> {
        self.parse_info().map(|v| ((v >> shift) & mask) as u8)
    }

    /// Raw AMS unit type from bits 0–3 — e.g. `3` is an AMS 2 Pro, **not** an AMS Lite (`2`).
    ///
    /// Prefer [`unit_model`](Self::unit_model), which decodes this into [`AmsUnitModel`] and
    /// carries the capability accessors. This stays for the one case that cannot serve: reading
    /// a unit type newer than this crate knows about.
    #[must_use]
    pub fn ams_type(&self) -> Option<u8> {
        self.info_field(0, AMS_UNIT_INFO_TYPE_MASK)
    }

    /// Which physical AMS accessory this unit is, decoded from `info` bits 0–3.
    ///
    /// Use this rather than [`ams_type`](Self::ams_type) to ask whether the unit can dry, how
    /// many slots it has, or what temperature range its heater accepts — see [`AmsUnitModel`].
    ///
    /// `None` when `info` is absent from the payload (older firmware omits it entirely) or when
    /// it carries a unit type this crate doesn't know. Both cases mean "don't assume a
    /// capability", which is the safe reading. This accessor is deliberately payload-local: the
    /// `info` module list carries the unit type a second time as a module-name prefix
    /// (`ams_f1/0`, `n3f/0`, `n3s/0`) and BambuStudio falls back to it when the bitmask is
    /// missing, but that lives in a different payload than this one.
    #[must_use]
    pub fn unit_model(&self) -> Option<AmsUnitModel> {
        self.ams_type().and_then(AmsUnitModel::from_wire)
    }

    /// Drying status from bits 4–7.
    #[must_use]
    pub fn dry_status(&self) -> Option<AmsDryStatus> {
        self.info_field(
            AMS_UNIT_INFO_DRY_STATUS_SHIFT,
            AMS_UNIT_INFO_DRY_STATUS_MASK,
        )
        .map(AmsDryStatus::from_wire)
    }

    /// Whether this unit can dry: `None` when its type is unknown (no `info`, or a type newer
    /// than this crate), which is not the same as "can't dry".
    #[must_use]
    pub fn supports_drying(&self) -> Option<bool> {
        self.unit_model().map(AmsUnitModel::supports_drying)
    }

    /// Inclusive drying temperature range in °C, or `None` if the unit can't dry or its type
    /// is unknown — see [`AmsUnitModel::dry_temp_range`].
    #[must_use]
    pub fn dry_temp_range(&self) -> Option<(u32, u32)> {
        self.unit_model().and_then(AmsUnitModel::dry_temp_range)
    }

    /// Extruder assignment from bits 8–11 (0 = right/main, 1 = left/deputy).
    /// Returns `None` when `info` is absent or the value is 0xE (uninitialized).
    #[must_use]
    pub fn extruder_assignment(&self) -> Option<u8> {
        self.info_field(AMS_UNIT_INFO_EXTRUDER_SHIFT, AMS_UNIT_INFO_EXTRUDER_MASK)
            .filter(|&raw| raw != AMS_UNIT_INFO_EXTRUDER_UNINITIALIZED)
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
    /// follows BambuStudio's `DevFilaSystem.cpp`, corroborated by bambuddy (`c5e00558`,
    /// `7a42e0a7`). See issue #137.
    #[must_use]
    pub fn filament_switch_inlet(&self) -> Option<FilamentSwitchInlet> {
        match self.info_field(
            AMS_UNIT_INFO_BIND_SWITCH_IN_SHIFT,
            AMS_UNIT_INFO_BIND_SWITCH_IN_MASK,
        )? {
            AMS_UNIT_INFO_SWITCH_INLET_B => Some(FilamentSwitchInlet::InB),
            AMS_UNIT_INFO_SWITCH_INLET_A => Some(FilamentSwitchInlet::InA),
            _ => None,
        }
    }

    /// True when this unit reports `0xE` ("not wired to a fixed extruder") in bits 8–11.
    ///
    /// Distinguishes the two cases [`extruder_assignment`](Self::extruder_assignment) folds into
    /// `None`: a unit routed through a Filament Track Switch, versus one whose assignment the
    /// firmware simply has not initialized. Pair with
    /// [`filament_switch_inlet`](Self::filament_switch_inlet) to tell them apart — an unbound
    /// `bind_switch_in` alongside `0xE` means uninitialized.
    #[must_use]
    pub fn has_unfixed_extruder(&self) -> bool {
        self.info_field(AMS_UNIT_INFO_EXTRUDER_SHIFT, AMS_UNIT_INFO_EXTRUDER_MASK)
            == Some(AMS_UNIT_INFO_EXTRUDER_UNINITIALIZED)
    }

    /// Drying sub-status from bits 22–23.
    #[must_use]
    pub fn dry_sub_status(&self) -> Option<AmsDrySubStatus> {
        self.info_field(
            AMS_UNIT_INFO_DRY_SUB_STATUS_SHIFT,
            AMS_UNIT_INFO_DRY_SUB_STATUS_MASK,
        )
        .map(AmsDrySubStatus::from_wire)
    }

    /// Dry-fan 1 status from bits 18–19. Confirmed against BambuStudio's
    /// `DevFilaSystem.cpp` (`get_flag_bits(info, 18, 2)`) and independently by
    /// `bambu-printer-manager`'s `bambutools.py`, an exact match.
    #[must_use]
    pub fn dry_fan1_status(&self) -> Option<AmsDryFanStatus> {
        self.info_field(
            AMS_UNIT_INFO_DRY_FAN1_STATUS_SHIFT,
            AMS_UNIT_INFO_DRY_FAN_STATUS_MASK,
        )
        .map(AmsDryFanStatus::from_wire)
    }

    /// Dry-fan 2 status from bits 20–21. Confirmed against BambuStudio's
    /// `DevFilaSystem.cpp` (`get_flag_bits(info, 20, 2)`) and independently by
    /// `bambu-printer-manager`'s `bambutools.py`, an exact match.
    #[must_use]
    pub fn dry_fan2_status(&self) -> Option<AmsDryFanStatus> {
        self.info_field(
            AMS_UNIT_INFO_DRY_FAN2_STATUS_SHIFT,
            AMS_UNIT_INFO_DRY_FAN_STATUS_MASK,
        )
        .map(AmsDryFanStatus::from_wire)
    }

    /// Decodes [`dry_sf_reason`](Self::dry_sf_reason) into typed reasons, in reported order.
    ///
    /// A layer over the raw `Vec<i32>` rather than a replacement for it — the same relationship
    /// [`parse_info`](Self::parse_info) has with the typed `info` accessors. Unrecognized codes
    /// survive as [`DryBlockReason::Other`].
    #[must_use]
    pub fn dry_block_reasons(&self) -> Option<Vec<DryBlockReason>> {
        self.dry_sf_reason.as_ref().map(|codes| {
            codes
                .iter()
                .copied()
                .map(DryBlockReason::from_code)
                .collect()
        })
    }

    /// The single reason worth showing a user when the firmware reports several at once.
    ///
    /// Mirrors bambuddy's `primary_reason_code`: a reason the user has to act on outranks one
    /// that clears on its own, because that is the only case where showing a message beats
    /// retrying silently. Ties break on reported order.
    #[must_use]
    pub fn primary_dry_block_reason(&self) -> Option<DryBlockReason> {
        let reasons = self.dry_block_reasons()?;
        reasons
            .iter()
            .find(|r| r.needs_user_action())
            .or_else(|| reasons.first())
            .copied()
    }
}

/// Why the firmware will not, or did not, start a drying cycle — one entry of `dry_sf_reason`.
///
/// **`dry_sf_reason` is a list of independent codes, not a bitmask.** `reference/05_materials_ams.md`
/// described it as one for a while and listed only `1` and `8`, whose reading as bit positions was
/// a coincidence; the field is an enumerated code list, which is why it deserializes as
/// `Vec<i32>`.
///
/// Codes from BambuStudio's `DevAms::CannotDryReason` (`DevFilaSystem.h`), which has ten
/// members; bambuddy's `DRY_SF_REASON_MESSAGES` (`backend/app/services/drying_preflight.py`)
/// agrees on `0`-`8` and omits `10`. The user-action split is bambuddy's — see
/// [`needs_user_action`](Self::needs_user_action).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DryBlockReason {
    /// `0` — the printer is busy.
    PrinterBusy,
    /// `1` — insufficient power: too many AMS units drying at once, or an external PSU is
    /// required. Needs the user to change something.
    InsufficientPower,
    /// `2` — the AMS is busy.
    AmsBusy,
    /// `3` — filament is sitting at the AMS outlet and must be retracted first. Needs the user.
    FilamentAtOutlet,
    /// `4` — a drying cycle on this AMS is already starting.
    AlreadyStarting,
    /// `5` — not supported in 2D mode.
    Unsupported2dMode,
    /// `6` — the AMS is already drying.
    AlreadyDrying,
    /// `7` — the AMS firmware is upgrading.
    FirmwareUpgrading,
    /// `8` — the external AMS power adapter must be plugged in. Needs the user.
    ExternalPowerRequired,
    /// `10` — filament is at the AMS outlet and must be unloaded by hand before drying.
    ///
    /// Needs the user. BambuStudio's `FilamentAtAmsOutletManualUnload`, whose message asks for a manual
    /// unload (`AMSDryControl.cpp`), unlike `3`, where Studio offers an unload button.
    FilamentAtOutletManualUnload,
    /// A code this crate doesn't know — newer firmware may add reasons, and folding one onto a
    /// neighbouring variant would report a wrong cause with full confidence.
    Other(i32),
}

/// Wire code of every named [`DryBlockReason`], the single table both conversions read.
const DRY_BLOCK_REASON_CODES: [(i32, DryBlockReason); 10] = [
    (0, DryBlockReason::PrinterBusy),
    (1, DryBlockReason::InsufficientPower),
    (2, DryBlockReason::AmsBusy),
    (3, DryBlockReason::FilamentAtOutlet),
    (4, DryBlockReason::AlreadyStarting),
    (5, DryBlockReason::Unsupported2dMode),
    (6, DryBlockReason::AlreadyDrying),
    (7, DryBlockReason::FirmwareUpgrading),
    (8, DryBlockReason::ExternalPowerRequired),
    (10, DryBlockReason::FilamentAtOutletManualUnload),
];

impl DryBlockReason {
    /// Decodes one raw `dry_sf_reason` entry.
    #[must_use]
    pub fn from_code(code: i32) -> Self {
        DRY_BLOCK_REASON_CODES
            .iter()
            .find(|(c, _)| *c == code)
            .map_or(Self::Other(code), |&(_, reason)| reason)
    }

    /// The raw wire code this reason decodes from.
    #[must_use]
    pub fn code(self) -> i32 {
        match self {
            Self::Other(code) => code,
            named => DRY_BLOCK_REASON_CODES
                .iter()
                .find(|(_, r)| *r == named)
                .map(|&(code, _)| code)
                .expect("every named DryBlockReason is in DRY_BLOCK_REASON_CODES"),
        }
    }

    /// Returns true if clearing this needs the user to physically do something.
    ///
    /// This is the distinction that decides a caller's behavior: retry in a moment, or stop and
    /// surface a message. True for [`InsufficientPower`](Self::InsufficientPower) and
    /// [`ExternalPowerRequired`](Self::ExternalPowerRequired) (bambuddy's
    /// `POWER_REASON_CODES = {1, 8}`), for [`FilamentAtOutlet`](Self::FilamentAtOutlet)
    /// (`RETRACT_REASON_CODE = 3`), and for
    /// [`FilamentAtOutletManualUnload`](Self::FilamentAtOutletManualUnload), which bambuddy doesn't
    /// know; every other known reason clears on its own.
    ///
    /// [`Other`](Self::Other) returns `false` — an unknown reason is reported as transient
    /// because that is the reading that keeps a caller retrying rather than permanently refusing
    /// on a code that may be benign.
    #[must_use]
    pub fn needs_user_action(self) -> bool {
        matches!(
            self,
            Self::InsufficientPower
                | Self::ExternalPowerRequired
                | Self::FilamentAtOutlet
                | Self::FilamentAtOutletManualUnload
        )
    }
}

impl AmsTray {
    /// The slot index parsed from [`id`](Self::id), or `None` if it isn't a number.
    #[must_use]
    pub fn slot(&self) -> Option<u8> {
        self.id.trim().parse().ok()
    }

    /// The material abbreviation (`"PLA"`, `"PETG"`, ...), or `None` when `tray_type` is
    /// absent or explicitly blank (empty or `"Empty"`).
    #[must_use]
    pub fn material(&self) -> Option<&str> {
        self.tray_type.as_deref().filter(|t| !is_blank_type(t))
    }

    /// The `RRGGBBAA` `tray_color` decoded to `[r, g, b, a]`, or `None` if absent or malformed.
    #[must_use]
    pub fn color_rgba(&self) -> Option<[u8; 4]> {
        let hex = self.tray_color.as_deref()?;
        if hex.len() != 8 {
            return None;
        }
        u32::from_str_radix(hex, 16).ok().map(u32::to_be_bytes)
    }

    /// `(min, max)` nozzle temperature in °C for the loaded filament, if both were reported.
    #[must_use]
    pub fn nozzle_temp_range(&self) -> Option<(u16, u16)> {
        let parse = |v: &Option<String>| v.as_deref()?.trim().parse().ok();
        Some((parse(&self.nozzle_temp_min)?, parse(&self.nozzle_temp_max)?))
    }

    /// Remaining filament in percent, or `None` for the firmware's `-1` "not calculated"
    /// sentinel or any other out-of-range value.
    #[must_use]
    pub fn remain_percent(&self) -> Option<u8> {
        self.remain
            .and_then(|r| u8::try_from(r).ok())
            .filter(|&r| r <= 100)
    }

    /// Retrieves the raw status code of the spool, defaulting to `9` (Empty) if omitted.
    ///
    /// **Not a loaded/empty answer on its own:** some firmware sends a fully populated tray
    /// with no `state` key, which reads as `9` here. Use [`is_loaded`](Self::is_loaded) to ask
    /// whether a spool is loaded.
    #[must_use]
    pub fn state(&self) -> u8 {
        self.state.unwrap_or(AMS_TRAY_STATE_EMPTY)
    }

    /// True when this tray, in the unit at `ams_id`, holds a loaded spool.
    ///
    /// The same rule [`clean_stale_tray_data`](crate::ams::clean_stale_tray_data) applies before
    /// keeping a tray's material data: a missing `state` with filament metadata is loaded,
    /// states `9`/`10` mean empty except on AMS-HT units (`ams_id` 128-135, where they don't),
    /// and an explicitly blank `tray_type` means empty.
    #[must_use]
    pub fn is_loaded(&self, ams_id: u8) -> bool {
        // An *absent* `state` is not a report of emptiness. Some firmware sends a complete tray
        // payload (`tray_info_idx`, `tray_type`, `tray_color`, `remain`) with no `state` key at
        // all, and treating that as absent-equivalent scrubbed the spool's material data on every
        // `TelemetryCache::sanitized_ams()` call. Fall back to the filament metadata instead —
        // the same fallback pybambu reaches for in `_has_filament_metadata` /
        // `_resolve_loaded_state` (`models.py`), which gates on a `_state_reported`
        // flag and accepts a non-empty `tray_info_idx`, or a `tray_type` that is neither empty
        // nor `"Empty"`, as proof a spool is loaded.
        let has_filament_metadata = self
            .tray_info_idx
            .as_ref()
            .is_some_and(|idx| !idx.is_empty())
            || self.tray_type.as_ref().is_some_and(|t| !is_blank_type(t));

        let is_absent_state = matches!(self.state, Some(AMS_TRAY_STATE_POWER_OFF))
            || (self.state.is_none() && !has_filament_metadata)
            || (!crate::ams::ids::is_ams_ht_id(ams_id)
                && matches!(
                    self.state,
                    Some(AMS_TRAY_STATE_SPOOL_NOT_FED) | Some(AMS_TRAY_STATE_EMPTY)
                ));

        let is_type_cleared = self.tray_type.as_deref().is_some_and(is_blank_type);

        !(is_absent_state || is_type_cleared)
    }

    /// Accurate remaining weight in grams, translating `remain_g`'s raw wire
    /// sentinel to `None`. Mirrors BambuStudio's `DevAmsTray::get_filament_remain_weight()`
    /// (`DevFilaSystem.cpp`): `remain_g < 0` means "not provided by firmware" and
    /// `remain_g == 0` means "confirmed empty," both `None` here; only a positive value is
    /// returned. Does not replicate BambuStudio's percentage-based fallback (`weight * remain
    /// / 100`) when `remain_g` is absent — callers needing that estimate already have
    /// `tray_weight`/`remain` to compute it themselves.
    pub fn remaining_weight_grams(&self) -> Option<u32> {
        self.remain_g.filter(|g| *g > 0).map(|g| g as u32)
    }
}

impl Mergeable for AmsTray {
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
    /// confirmed 3-arg `ParseVal` counterpart (`DevFilaSystem.cpp`).
    fn merge_from(&mut self, incoming: &Self) {
        let Self {
            id,
            state,
            tray_type,
            tray_color,
            tray_info_idx,
            tag_uid,
            tray_uuid,
            remain,
            tray_sub_brands,
            nozzle_temp_max,
            nozzle_temp_min,
            tray_diameter,
            tray_weight,
            tray_id_name,
            tray_temp,
            tray_time,
            drying_temp,
            drying_time,
            bed_temp,
            bed_temp_type,
            xcam_info,
            k,
            n,
            cali_idx,
            cols,
            ctype,
            total_len,
            remain_g,
            filament_setting_id,
        } = incoming;
        // A no-op for an AMS tray, which is matched by id before merging; an external holder's
        // cached copy may have been created from a push that omitted it.
        if !id.is_empty() {
            self.id = id.clone();
        }
        keep_new(&mut self.state, state);
        keep_new(&mut self.tray_type, tray_type);
        keep_new(&mut self.tray_color, tray_color);
        keep_new(&mut self.tray_info_idx, tray_info_idx);
        keep_new(&mut self.tag_uid, tag_uid);
        keep_new(&mut self.tray_uuid, tray_uuid);
        keep_new(&mut self.remain, remain);
        keep_new(&mut self.tray_sub_brands, tray_sub_brands);
        keep_new(&mut self.nozzle_temp_max, nozzle_temp_max);
        keep_new(&mut self.nozzle_temp_min, nozzle_temp_min);
        keep_new(&mut self.tray_diameter, tray_diameter);
        keep_new(&mut self.tray_weight, tray_weight);
        keep_new(&mut self.tray_id_name, tray_id_name);
        keep_new(&mut self.tray_temp, tray_temp);
        keep_new(&mut self.tray_time, tray_time);
        keep_new(&mut self.drying_temp, drying_temp);
        keep_new(&mut self.drying_time, drying_time);
        keep_new(&mut self.bed_temp, bed_temp);
        keep_new(&mut self.bed_temp_type, bed_temp_type);
        keep_new(&mut self.xcam_info, xcam_info);
        keep_new(&mut self.k, k);
        keep_new(&mut self.n, n);
        keep_new(&mut self.cali_idx, cali_idx);
        keep_new(&mut self.cols, cols);
        keep_new(&mut self.ctype, ctype);
        keep_new(&mut self.total_len, total_len);
        keep_new(&mut self.remain_g, remain_g);
        keep_new(&mut self.filament_setting_id, filament_setting_id);
    }
}
