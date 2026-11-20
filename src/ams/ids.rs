//! # AMS Unit and Slot Ids
//!
//! The id blocks every AMS address falls into, and the predicates over them. Dependency-free on
//! purpose, so the telemetry types, the parsers, the mapping builders and the client all read
//! one statement of the address space instead of each spelling the ranges inline.
//!
//! The blocks, with the global tray ids each resolves to:
//!
//! - standard units `0..=3` (4 slots each, trays `0..=15`)
//! - an AMS Lite on an A2L, physical `16` and normalized `6` (4 slots, trays `24..=27`)
//! - the AMS 3H, unit `32` (9 slots, trays `28..=36`; see [`AMS_3H_UNIT_ID`])
//! - AMS-HT units `128..=135` (1 slot each, trays = the unit id)
//! - external spool holders `254` / `255`
//!
//! See `.claude/rules/ams-lite-on-a2l-unit-id.md` before adding a predicate here: the AMS Lite on
//! an A2L (`16` / `6`) is the unit that rule was written for, and each new block has to be
//! handled in every layer it names.

pub(crate) const AMS_SLOTS_PER_UNIT: u8 = 4;
/// `ams_change_filament` slot (and target) meaning "unload / retract whatever is loaded".
pub(crate) const SLOT_UNLOAD: u8 = 255;
/// The `ams_id`s a caller may pass to an AMS command, as `InvalidArgument` messages state them.
pub(crate) const VALID_AMS_IDS_TEXT: &str =
    "0..=3, 6 or 16 (A2L AMS Lite), 32 (AMS 3H), 128..=135, 254, 255";
/// Reverted from `7` back to `3` — the widening to `7` relied on
/// bambuddy's `ck_ams_id_range` CHECK constraint (0-7), but that range predates bambuddy's
/// own issue #1274 by a month with no cited evidence of a standard unit above id 3; #1274
/// itself only confirms `ams_id=128` (AMS-HT). Three independent sources now agree `3` is
/// correct: user-supplied official Bambu Lab documentation caps standard AMS 2 Pro units at
/// 4 on every product line; BambuStudio's own `DevAms::GetTrayId` (`DevFilaSystem.cpp`)
/// hardcodes AMS-HT's bit-index base offset at `16`, which is only correct if standard units
/// never reach bits 16+ (i.e. never exceed id 3); and pybambu's uncapped `tray_now >> 2`
/// decode doesn't corroborate 8 units either — it's simply unbounded, not evidence of an
/// observed 8th unit.
pub(crate) const AMS_MAX_STANDARD_ID: u8 = 3;
/// AMS-HT unit ids are capped at 135 (8 lettered units, "A"-"H"), not BambuStudio's wider
/// `< 153` bound (`DevFilaSystem.cpp` `GetTrayNameByTrayId`, `CalibUtils.cpp`) —
/// deliberately, not an oversight. That bound is defensive-margin coding, not a confirmed
/// protocol ceiling: bambuddy's actual AMS-HT *operational* logic (not just a storage-layer
/// check) caps at the same 135 bambino uses (`backend/app/api/routes/printers.py`,
/// `backend/app/main.py`'s 8-letter `HT-{A..H}` labeling). The two upstreams disagree
/// with each other here; bambino follows the one with real operational AMS-HT-unit logic
/// behind it. Raise this again only with hardware evidence for a 9th+ AMS-HT unit (id 136+) —
/// re-litigated without new evidence in the 2026-09-08 telemetry review sweep, same conclusion.
pub(crate) const AMS_HT_ID_MIN: u8 = 128;
pub(crate) const AMS_HT_ID_MAX: u8 = 135;
/// The unit id a 4-slot AMS Lite reports when it is attached to an **A2L printer**.
///
/// "AMS Lite" is the unit; "A2L" is the printer it is plugged into, and the pairing is what
/// selects this id. The same physical unit is an A1/A1 mini's *only* possible AMS and takes id
/// 0 there; an A2L can run it alongside up to four shared-pool units already holding ids 0-3
/// (`MODEL_MATRIX.csv`, "AMS Unit Limits"), so it needs an id outside that block and reports
/// 16.
///
/// 16 sits outside every other range (standard 0-3, AMS-HT 128-135, external 254/255), so
/// untranslated it falls through every branch here and resolves to the unmapped sentinel.
///
/// BambuStudio encodes the same pairing as a distinct *unit type* rather than a distinct id —
/// `AMS_LITE_MIXED = 5`, commented "AMS-Lite for N9" (`DeviceCore/DevDefs.h`), N9 being the
/// A2L's dev token — read from the unit's own `info` type nibble. It therefore never reads
/// this id at all; see [`AMS_LITE_ON_A2L_NORMALIZED_ID`] for where the two routes reconverge.
pub(crate) const AMS_LITE_ON_A2L_PHYSICAL_ID: u8 = 16;
/// The id an [A2L-attached AMS Lite](AMS_LITE_ON_A2L_PHYSICAL_ID) is normalized to on ingest,
/// so the standard `ams_id * 4 + slot` formula lands its global tray ids at 24-27.
///
/// 24 is not an arbitrary choice: it is BambuStudio's own `AMS_LITE_MIXED_TRAY_INDEX_OFFSET`
/// (`DeviceCore/DevDefs.h`), applied as `24 + slot_id` in all three of `DevAms::GetTrayId`
/// (`DevFilaSystem.cpp`), `DevMappingUtil::ams_filament_mapping` and
/// `DevMapping.cpp` — none of which consult `ams_id`, because the type already told
/// them which unit this is. bambuddy instead normalizes to the same id 6 this crate uses
/// (`normalize_am_unit_id` in `bambu_mqtt.py`). Two routes, one answer.
///
/// The range collides with nothing — standard units occupy bits/ids 0-15 and AMS-HT 16-23 in
/// `tray_exist_bits`.
pub(crate) const AMS_LITE_ON_A2L_NORMALIZED_ID: u8 = 6;
/// The unit id the H3C's built-in AMS 3H (wire unit type 6, BambuStudio `ZMULTI_A`) reports.
///
/// Sources: ha-bambulab `MOCK-H3C.json` (`"id": "32"`) and bambuddy `utils/tray_ids.py`
/// (`AMS3H_UNIT_ID`). BambuStudio never reads this id: it keys the 3H's tray arithmetic on the
/// unit *type* (`DevAms::GetTrayId`), so helpers here that only have an id key on 32 instead.
/// The AMS-B (type 7, `ZMULTI_B`, tray base 37) is deliberately absent from every id helper: the
/// unit id it reports is not recorded, and inventing one would route a real unit's id onto the
/// wrong tray block.
pub(crate) const AMS_3H_UNIT_ID: u8 = 32;
/// Slots of the AMS 3H, local `0..=8`.
pub(crate) const AMS_3H_SLOTS: u8 = 9;
/// Global tray id and `tray_exist_bits` bit of the AMS 3H's slot 0 (`28 + slot`), BambuStudio's
/// `DevAms::GetTrayId` / `DevAmsTray::get_tray_flag_bit` `ZMULTI_A` branch.
pub(crate) const AMS_3H_TRAY_BASE: u8 = 28;
/// The single-nozzle external spool, and IDEX's right (primary) carriage — BambuStudio's
/// `VIRTUAL_TRAY_MAIN_ID` (`reference/05_materials_ams.md`, "Virtual / External Spool
/// Telemetry" and "Virtual Slot Remapping on Single-Nozzle Platforms"). This is the id an
/// `ams_mapping2` payload must carry for a single-nozzle printer; sending the deputy id
/// instead targets physical AMS tray 0 and produces firmware error `0700_8012`.
pub(crate) const AMS_EXTERNAL_SPOOL_MAIN_ID: u8 = 255;
/// IDEX's left (deputy) carriage — BambuStudio's `VIRTUAL_TRAY_DEPUTY_ID`. Meaningful only on
/// dual-nozzle IDEX machines.
///
/// The previous names had these two roles inverted (`..._ID = 254` / `..._ALT_ID = 255`), which
/// made the "alternate" constant the main id and was the standing trap behind the repeated
/// 254/255 confusion in issues #42, #50, and #56.
pub(crate) const AMS_EXTERNAL_SPOOL_DEPUTY_ID: u8 = 254;

/// Normalizes an AMS unit id reported on the wire into the id this crate addresses it by.
///
/// Only an A2L-attached AMS Lite's physical id 16 is remapped (to 6); every other id passes through
/// untouched, and no other Bambu unit reports id 16, so the remap is self-scoping. Applied on
/// the inbound telemetry boundary so that `tray_exist_bits`, `resolve_global_tray_id` and the
/// mapping builders all agree on one id; the physical 16 is restored only on the outbound wire
/// by [`crate::ams::MaterialSource::to_mapping2_entry`].
///
/// The firmware is internally inconsistent about this unit, which is why one constant cannot
/// cover it: `tray_exist_bits` uses bit base 24 (id 6's position, not id 16's), `tray_now`
/// reports a local slot 0-3, and only `ams_mapping2` and the per-unit commands carry 16.
#[must_use]
pub fn normalize_ams_unit_id(ams_id: u8) -> u8 {
    if ams_id == AMS_LITE_ON_A2L_PHYSICAL_ID {
        AMS_LITE_ON_A2L_NORMALIZED_ID
    } else {
        ams_id
    }
}

/// Returns true for a standard AMS unit id (`0..=3`).
#[must_use]
pub(crate) const fn is_standard_id(ams_id: u8) -> bool {
    ams_id <= AMS_MAX_STANDARD_ID
}

/// Returns true for an AMS-HT unit id (`128..=135`), a single-slot unit.
#[must_use]
pub(crate) const fn is_ams_ht_id(ams_id: u8) -> bool {
    ams_id >= AMS_HT_ID_MIN && ams_id <= AMS_HT_ID_MAX
}

/// Returns true for an A2L-attached AMS Lite under either spelling, physical `16` or normalized `6`.
#[must_use]
pub(crate) const fn is_ams_lite_on_a2l_id(ams_id: u8) -> bool {
    ams_id == AMS_LITE_ON_A2L_PHYSICAL_ID || ams_id == AMS_LITE_ON_A2L_NORMALIZED_ID
}

/// Returns true for the AMS 3H's unit id (`32`).
#[must_use]
pub(crate) const fn is_ams_3h_id(ams_id: u8) -> bool {
    ams_id == AMS_3H_UNIT_ID
}

/// Returns true for an external spool holder id: `254` (deputy) or `255` (main).
///
/// Commands disagree on which pair addresses which holder — `ams_filament_setting` and
/// `extrusion_cali_sel` each have their own rule, documented on the client method that sends
/// them. This only answers whether an id names a holder at all.
#[must_use]
pub(crate) const fn is_external_spool_id(ams_id: u8) -> bool {
    ams_id == AMS_EXTERNAL_SPOOL_DEPUTY_ID || ams_id == AMS_EXTERNAL_SPOOL_MAIN_ID
}

/// Returns true if `ams_id` addresses a physical AMS bus unit: standard, AMS-HT, or an
/// A2L-attached AMS Lite under either spelling, or the AMS 3H. External spools are excluded.
#[must_use]
pub(crate) const fn is_bus_unit_id(ams_id: u8) -> bool {
    is_standard_id(ams_id)
        || is_ams_ht_id(ams_id)
        || is_ams_lite_on_a2l_id(ams_id)
        || is_ams_3h_id(ams_id)
}

/// Returns true for the whole documented `ams_id` address space: a bus unit or an external
/// spool holder.
#[must_use]
pub(crate) const fn is_valid_ams_id(ams_id: u8) -> bool {
    is_bus_unit_id(ams_id) || is_external_spool_id(ams_id)
}

/// The slot count of the bus unit at `ams_id`, or `None` where `ams_id` is not a bus unit.
///
/// Keyed on the id, since that is all most callers hold; the type-keyed answer is
/// [`AmsUnitModel::slot_count`](crate::types::telemetry::AmsUnitModel::slot_count).
#[must_use]
pub(crate) const fn unit_slot_count(ams_id: u8) -> Option<u8> {
    if is_ams_ht_id(ams_id) {
        Some(1)
    } else if is_ams_3h_id(ams_id) {
        Some(AMS_3H_SLOTS)
    } else if is_standard_id(ams_id) || is_ams_lite_on_a2l_id(ams_id) {
        Some(AMS_SLOTS_PER_UNIT)
    } else {
        None
    }
}

/// Returns true if `slot_id` is a slot of the bus unit at `ams_id`: `0..=3` on a standard unit
/// or an A2L-attached AMS Lite, only `0` on a single-slot AMS-HT, `0..=8` on the AMS 3H, nothing
/// elsewhere.
#[must_use]
pub(crate) const fn is_unit_slot(ams_id: u8, slot_id: u8) -> bool {
    match unit_slot_count(ams_id) {
        Some(count) => slot_id < count,
        None => false,
    }
}

/// Translates a caller-supplied `ams_id` into the id the wire carries.
///
/// Only the A2L-attached AMS Lite differs: telemetry normalizes its physical id 16 to 6, but
/// every per-unit command addresses it as 16 with a local `0..=3` slot — confirmed from the
/// firmware's own `ams_mapping2` (`{ams_id: 16, slot_id: 0-3}`, bambuddy `a2l_lite_wire_ids`,
/// `bambu_mqtt.py`), and BambuStudio sends `ams_get_rfid {ams_id: 16}` for the unit.
/// Callers may pass either spelling; every other id passes through untouched.
#[must_use]
pub(crate) const fn wire_ams_id(ams_id: u8) -> u8 {
    if ams_id == AMS_LITE_ON_A2L_NORMALIZED_ID {
        AMS_LITE_ON_A2L_PHYSICAL_ID
    } else {
        ams_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_id_blocks_partition_the_address_space() {
        for id in 0..=u8::MAX {
            let blocks = [
                is_standard_id(id),
                is_ams_ht_id(id),
                is_ams_lite_on_a2l_id(id),
                is_ams_3h_id(id),
                is_external_spool_id(id),
            ];
            assert!(blocks.iter().filter(|&&b| b).count() <= 1, "{id}");
            assert_eq!(is_valid_ams_id(id), blocks.contains(&true), "{id}");
        }
        assert!(is_ams_lite_on_a2l_id(6) && is_ams_lite_on_a2l_id(16));
        assert_eq!(wire_ams_id(6), 16);
        assert_eq!(wire_ams_id(128), 128);
        assert_eq!(wire_ams_id(AMS_3H_UNIT_ID), AMS_3H_UNIT_ID);
        assert!(is_bus_unit_id(AMS_3H_UNIT_ID) && is_valid_ams_id(AMS_3H_UNIT_ID));
        assert_eq!(normalize_ams_unit_id(AMS_3H_UNIT_ID), AMS_3H_UNIT_ID);
    }

    #[test]
    fn test_is_unit_slot() {
        assert!(is_unit_slot(0, 3) && !is_unit_slot(0, 4));
        assert!(is_unit_slot(16, 3) && is_unit_slot(6, 0));
        assert!(is_unit_slot(128, 0) && !is_unit_slot(128, 1));
        assert!(!is_unit_slot(255, 0) && !is_unit_slot(4, 0));
        assert!(is_unit_slot(32, 0) && is_unit_slot(32, 8) && !is_unit_slot(32, 9));
        assert_eq!(unit_slot_count(32), Some(9));
        assert_eq!(unit_slot_count(255), None);
    }
}
