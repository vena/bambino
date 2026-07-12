//! # AMS Telemetry & Bitmask Parser
//!
//! Implements low-level bitwise operations and sanitization logic for parsing
//! Bambu Lab AMS telemetry reports [REF-AMS-DECODE]. This includes checking spool
//! presence via hex bitmasks, managing power-down state anomalies, cleansing stale
//! tray data, and calculating global indexes.

use crate::types::AmsTray;
use crate::types::telemetry::ams::{AMS_TRAY_STATE_EMPTY, AMS_TRAY_STATE_SPOOL_NOT_FED};

pub(crate) const AMS_SLOTS_PER_UNIT: u8 = 4;
/// Confirmed against `bambuddy/backend/app/models/spoolman_slot_assignment.py`'s
/// `ck_ams_id_range` CHECK constraint (0-7, 8 units) — widened there in bambuddy's own
/// issue #1274 because real H2C/H2D hardware exceeded a 4-unit cap. `pybambu`'s
/// `tray_now >> 2` decode derives the AMS index dynamically with no hardcoded cap.
/// Previously `3` (4 units), which silently misclassified units 4-7 as non-standard/external.
pub(crate) const AMS_MAX_STANDARD_ID: u8 = 7;
pub(crate) const AMS_HT_ID_MIN: u8 = 128;
pub(crate) const AMS_HT_ID_MAX: u8 = 135;
pub(crate) const AMS_EXTERNAL_SPOOL_ID: u8 = 254;
pub(crate) const AMS_EXTERNAL_SPOOL_ALT_ID: u8 = 255;
pub(crate) const AMS_TRAY_STATE_POWER_OFF: u8 = 0;

/// Evaluates if a physical spool is present in a specific standard AMS slot.
///
/// Standard AMS units contain up to 4 slots. The physical presence is tracked via
/// a hexadecimal bitmask string (`tray_exist_bits`).
///
/// **The Printer-Shutdown Telemetry Exception [REF-AMS-DECODE]:**
/// During printer shutdown sequences, the firmware often emits a final status packet
/// where `tray_exist_bits` is `0` and `power_on_flag` is `false`. To prevent downstream
/// observers from falsely reporting a cascade of physical "spool removed" events,
/// this evaluator returns `None` strictly when both conditions are met. If `power_on_flag`
/// is `false` but the parsed bitmask is non-zero, this represents a valid offline state
/// and is processed normally.
///
/// **AMS-HT units (IDs 128-135) don't participate in `tray_exist_bits` at all**
/// (per `reference/05_materials_ams.md` §5.1) — this function returns `None` for that
/// range rather than guessing, so callers must consult the tray's `state` field instead.
pub fn evaluate_spool_presence(
    tray_exist_bits: &str,
    ams_id: u8,
    tray_id: u8,
    power_on_flag: bool,
) -> Option<bool> {
    // Strip optional hex prefixes prior to radix conversions
    let clean_bits = tray_exist_bits
        .strip_prefix("0x")
        .or_else(|| tray_exist_bits.strip_prefix("0X"))
        .unwrap_or(tray_exist_bits);

    let parsed_mask = u32::from_str_radix(clean_bits, 16).ok()?;

    // Evaluate the shutdown exception boundary
    if parsed_mask == 0 && !power_on_flag {
        return None;
    }

    // High-temperature AMS-HT units (IDs 128-135) reside on their own bus addresses
    // and do not participate in standard bitwise exists strings — presence must come
    // from tray state instead, so report unknown rather than hardcoding a guess.
    if (AMS_HT_ID_MIN..=AMS_HT_ID_MAX).contains(&ams_id) {
        return None;
    }

    // Reject ams_id values outside the standard AMS range before computing the shift
    // amount below (mirrors resolve_global_tray_id's bounds check in this same file) —
    // otherwise an out-of-range ams_id produces a shift amount >= 32, which panics in
    // debug builds and silently returns a wrong result in release builds.
    if ams_id > AMS_MAX_STANDARD_ID || tray_id >= AMS_SLOTS_PER_UNIT {
        return None;
    }

    let shift_standard = (ams_id as u32 * AMS_SLOTS_PER_UNIT as u32) + tray_id as u32;
    let slot_exists = ((parsed_mask >> shift_standard) & 1) == 1;

    Some(slot_exists)
}

/// Explicitly sanitizes and nullifies telemetry fields when a physical slot becomes empty.
///
/// **Incremental Telemetry Update Slot Cleansing Rules [REF-AMS-DECODE]:**
/// To save network bandwidth, the printer's incremental telemetry pushes omit configuration
/// parameters (like `tray_type` or `tray_color`) when a spool is extracted. Without active
/// cleanup on the client side, standard parsers would preserve the material properties of the
/// previously loaded spool indefinitely.
///
/// This routine inspects the tray's state — 9 (`AMS_TRAY_STATE_EMPTY`) meaning Empty/Absent and
/// 10 (`AMS_TRAY_STATE_SPOOL_NOT_FED`) meaning a spool is physically present but not yet fed to
/// the extruder, both treated as absent-equivalent for stale-data cleansing (BUG-012, verified
/// against pybambu/Bambuddy — see `AMS_TRAY_STATE_SPOOL_NOT_FED`'s doc comment) — and clears all
/// stale config keys if either applies. It treats an empty `tray_type` string as an explicit
/// clearing signal too.
pub fn clean_stale_tray_data(tray: &mut AmsTray) {
    let is_absent_state = matches!(
        tray.state,
        Some(AMS_TRAY_STATE_EMPTY)
            | Some(AMS_TRAY_STATE_SPOOL_NOT_FED)
            | Some(AMS_TRAY_STATE_POWER_OFF)
            | None
    );

    let is_type_cleared = tray
        .tray_type
        .as_ref()
        .map(|t| t.is_empty() || t == "Empty")
        .unwrap_or(true);

    if is_absent_state || is_type_cleared {
        // Enforce clean state representation by resetting optional keys
        tray.tray_type = None;
        tray.tray_color = None;
        tray.tray_info_idx = None;
        tray.tag_uid = None;
        tray.tray_uuid = None;
        tray.remain = Some(-1);
        tray.tray_sub_brands = None;
        tray.nozzle_temp_max = None;
        tray.nozzle_temp_min = None;
        tray.tray_diameter = None;
        tray.tray_weight = None;
        tray.tray_id_name = None;
        tray.xcam_info = None;
        tray.k = None;
        tray.n = None;
        tray.cali_idx = None;
        tray.cols = None;
        tray.ctype = None;
        tray.total_len = None;
        tray.bed_temp = None;
        tray.bed_temp_type = None;
        tray.tray_temp = None;
        tray.tray_time = None;
        tray.drying_temp = None;
        tray.drying_time = None;

        // Standardize absent state representation to 9
        if tray.state.is_none() {
            tray.state = Some(AMS_TRAY_STATE_EMPTY);
        }
    }
}

/// Computes the unique global channel identifier for a given expansion unit and local tray.
///
/// Returns `None` if `ams_id` falls outside all valid ranges (standard 0–3,
/// AMS-HT 128–135, external 254–255) or if `tray_id >= 4` on the standard path.
///
/// The physical mapping aligns as:
/// * **Standard AMS Slots**: Sized in blocks of 4 per expansion unit: `(ams_id * 4) + tray_id`.
/// * **AMS-HT Units**: Single-slot systems where the channel ID equals the bus `ams_id` directly.
/// * **Virtual Spools**: Channels mapped to the external spool holder (ID 254 or 255).
pub fn resolve_global_tray_id(ams_id: u8, tray_id: u8) -> Option<u8> {
    let is_ht = (AMS_HT_ID_MIN..=AMS_HT_ID_MAX).contains(&ams_id);
    let is_external = ams_id == AMS_EXTERNAL_SPOOL_ID || ams_id == AMS_EXTERNAL_SPOOL_ALT_ID;

    if is_ht || is_external {
        Some(ams_id)
    } else if ams_id <= AMS_MAX_STANDARD_ID && tray_id < AMS_SLOTS_PER_UNIT {
        Some(ams_id * AMS_SLOTS_PER_UNIT + tray_id)
    } else {
        None
    }
}

/// Resolves the currently printing tray's global ID, accounting for IDEX map translations.
///
/// **Multi-AMS Local Index Resolution [REF-AMS-DECODE]:**
/// Multi-extruder platforms (such as the H2D series) emit local slot indexes (0 to 3) inside
/// their `tray_now` telemetry parameter. To resolve this back to a global index, the client must
/// inspect the active extruder carriage and correlate it against the `ams_extruder_map` matrix.
pub fn resolve_printing_global_id(
    tray_now: u8,
    active_extruder: Option<u8>,
    ams_extruder_map: &[u8],
) -> Option<u8> {
    let extruder = active_extruder?;
    let ams_id = ams_extruder_map.get(extruder as usize)?;
    resolve_global_tray_id(*ams_id, tray_now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_spool_presence_standard() {
        // Hex "f" is binary 1111 -> slots 0, 1, 2, 3 all present on AMS 0
        assert_eq!(evaluate_spool_presence("0xf", 0, 0, true), Some(true));
        assert_eq!(evaluate_spool_presence("f", 0, 3, true), Some(true));

        // Hex "2" is binary 0010 -> slot 1 present, slot 0 absent on AMS 0
        assert_eq!(evaluate_spool_presence("0x2", 0, 0, true), Some(false));
        assert_eq!(evaluate_spool_presence("0x2", 0, 1, true), Some(true));
    }

    #[test]
    fn test_evaluate_spool_presence_ams_ht_returns_none() {
        // BUG-015: AMS-HT units (128-135) don't participate in tray_exist_bits — this
        // must report unknown (None), not hardcode Some(true), so callers fall back to
        // consulting the tray's own `state` field for real presence.
        assert_eq!(evaluate_spool_presence("f", 128, 0, true), None);
        assert_eq!(evaluate_spool_presence("f", 135, 0, true), None);
        assert_eq!(evaluate_spool_presence("0", 130, 0, true), None);
    }

    #[test]
    fn test_shutdown_telemetry_exception() {
        // Under normal running, zero mask evaluates to absent
        assert_eq!(evaluate_spool_presence("0", 0, 0, true), Some(false));

        // If shutdown occurs (power_on_flag = false, mask = 0), ignore the update (None)
        assert_eq!(evaluate_spool_presence("0", 0, 0, false), None);

        // If mask is non-zero during offline power down, process changes normally
        assert_eq!(evaluate_spool_presence("2", 0, 1, false), Some(true));
    }

    #[test]
    fn test_clean_stale_tray_data_state_9() {
        let mut tray = AmsTray {
            id: "0".into(),
            state: Some(9),
            tray_type: Some("PLA".into()),
            tray_color: Some("FFFFFFFF".into()),
            tray_info_idx: Some("GFA01".into()),
            tag_uid: Some("ABCDEF1234567890".into()),
            tray_uuid: Some("UUID_SOME_MOCK_VAL".into()),
            remain: Some(85),
            tray_temp: Some("50".into()),
            tray_time: Some("240".into()),
            drying_temp: Some("55".into()),
            drying_time: Some("480".into()),
            ..Default::default()
        };

        clean_stale_tray_data(&mut tray);

        assert_eq!(tray.tray_type, None);
        assert_eq!(tray.tray_color, None);
        assert_eq!(tray.tag_uid, None);
        assert_eq!(tray.remain, Some(-1));
        assert_eq!(tray.tray_temp, None);
        assert_eq!(tray.tray_time, None);
        assert_eq!(tray.drying_temp, None);
        assert_eq!(tray.drying_time, None);
    }

    #[test]
    fn test_clean_stale_tray_data_clears_drying_fields() {
        // Phase 4.12 regression: a spool with a configured drying profile that's removed and
        // replaced with a spool lacking drying config must not leave the *previous* spool's
        // stale drying temp/time cached client-side (which could show a phantom drying
        // countdown in a UI).
        let mut tray = AmsTray {
            id: "0".into(),
            state: Some(9),
            tray_temp: Some("50".into()),
            tray_time: Some("240".into()),
            drying_temp: Some("55".into()),
            drying_time: Some("480".into()),
            ..Default::default()
        };

        clean_stale_tray_data(&mut tray);

        assert_eq!(tray.tray_temp, None);
        assert_eq!(tray.tray_time, None);
        assert_eq!(tray.drying_temp, None);
        assert_eq!(tray.drying_time, None);
    }

    #[test]
    fn test_clean_stale_tray_data_empty_string() {
        let mut tray = AmsTray {
            id: "1".into(),
            state: Some(10),            // Supposedly present but retracted
            tray_type: Some("".into()), // Empty type constitutes a clearing signal
            tray_color: Some("FFFFFFFF".into()),
            tray_info_idx: None,
            tag_uid: None,
            tray_uuid: None,
            remain: Some(100),
            ..Default::default()
        };

        clean_stale_tray_data(&mut tray);

        assert_eq!(tray.tray_color, None);
        assert_eq!(tray.remain, Some(-1));
    }

    #[test]
    fn test_resolve_global_tray_id_standard() {
        assert_eq!(resolve_global_tray_id(0, 0), Some(0));
        assert_eq!(resolve_global_tray_id(0, 3), Some(3));
        assert_eq!(resolve_global_tray_id(1, 2), Some(6));
        assert_eq!(resolve_global_tray_id(2, 0), Some(8));
        assert_eq!(resolve_global_tray_id(3, 3), Some(15));
    }

    #[test]
    fn test_resolve_global_tray_id_ams_ht() {
        assert_eq!(resolve_global_tray_id(128, 0), Some(128));
        assert_eq!(resolve_global_tray_id(135, 0), Some(135));
    }

    #[test]
    fn test_resolve_global_tray_id_external_spool() {
        // IDEX left external spool (ams_id 254, slot 0)
        assert_eq!(resolve_global_tray_id(254, 0), Some(254));
        // IDEX right / single-nozzle external spool (ams_id 255, slot 0)
        assert_eq!(resolve_global_tray_id(255, 0), Some(255));
        // Single-nozzle telemetry reports tray_now=254
        assert_eq!(resolve_global_tray_id(254, 254), Some(254));
    }

    #[test]
    fn test_resolve_global_tray_id_invalid() {
        // ams_id outside valid ranges
        assert_eq!(resolve_global_tray_id(8, 0), None);
        assert_eq!(resolve_global_tray_id(64, 0), None);
        assert_eq!(resolve_global_tray_id(127, 0), None);
        assert_eq!(resolve_global_tray_id(136, 0), None);
        // tray_id out of range on standard path
        assert_eq!(resolve_global_tray_id(0, 4), None);
        assert_eq!(resolve_global_tray_id(3, 255), None);
    }

    #[test]
    fn test_resolve_printing_global_id_idex_standard_ams() {
        let ams_extruder_map = [0u8, 1u8];
        assert_eq!(
            resolve_printing_global_id(2, Some(1), &ams_extruder_map),
            Some(6)
        );
        assert_eq!(
            resolve_printing_global_id(0, Some(0), &ams_extruder_map),
            Some(0)
        );
    }

    #[test]
    fn test_resolve_printing_global_id_idex_external_spool() {
        // Extruder 0 → AMS 0 (standard), Extruder 1 → external spool right (255)
        let ams_extruder_map = [0u8, 255u8];
        assert_eq!(
            resolve_printing_global_id(0, Some(1), &ams_extruder_map),
            Some(255)
        );
    }

    #[test]
    fn test_resolve_printing_global_id_no_extruder() {
        let ams_extruder_map = [0u8, 1u8];
        assert_eq!(resolve_printing_global_id(0, None, &ams_extruder_map), None);
    }

    #[test]
    fn test_resolve_printing_global_id_out_of_bounds() {
        let ams_extruder_map = [0u8];
        assert_eq!(
            resolve_printing_global_id(0, Some(5), &ams_extruder_map),
            None
        );
    }

    #[test]
    fn test_evaluate_spool_presence_ams_id_out_of_range() {
        // ams_id outside both the standard (0-7) and AMS-HT (128-135) ranges must not
        // panic or wrap into a bogus shift amount — it should cleanly report None.
        assert_eq!(evaluate_spool_presence("f", 200, 0, true), None);
        assert_eq!(evaluate_spool_presence("f", 8, 0, true), None);
        assert_eq!(evaluate_spool_presence("f", 127, 0, true), None);
        assert_eq!(evaluate_spool_presence("f", 255, 0, true), None);
    }

    #[test]
    fn test_evaluate_spool_presence_tray_id_out_of_range() {
        // BUG-014: a valid ams_id (0-3) with tray_id >= 4 must not reach the bit-shift —
        // tray_id comes straight off the wire, so a malformed packet must not panic
        // (debug) or silently wrap into a bogus shift amount (release).
        assert_eq!(evaluate_spool_presence("f", 0, 4, true), None);
        assert_eq!(evaluate_spool_presence("f", 3, 32, true), None);
        assert_eq!(evaluate_spool_presence("f", 0, 255, true), None);
    }

    #[test]
    fn test_evaluate_spool_presence_multi_ams() {
        // Hex "ff10" = binary ...1111_1111_0001_0000
        // AMS 0 slots: bits 0-3 = 0000 (none present)
        // AMS 1 slots: bits 4-7 = 0001 (slot 0 present)
        // AMS 2 slots: bits 8-11 = 1111 (all present)
        // AMS 3 slots: bits 12-15 = 1111 (all present)
        assert_eq!(evaluate_spool_presence("ff10", 0, 0, true), Some(false));
        assert_eq!(evaluate_spool_presence("ff10", 1, 0, true), Some(true));
        assert_eq!(evaluate_spool_presence("ff10", 1, 1, true), Some(false));
        assert_eq!(evaluate_spool_presence("ff10", 2, 0, true), Some(true));
        assert_eq!(evaluate_spool_presence("ff10", 2, 3, true), Some(true));
    }

    #[test]
    fn test_clean_stale_tray_data_state_10_with_type_clears() {
        // BUG-012: state 10 (spool present but not yet fed to the extruder) with a populated
        // tray_type used to be treated as "keep" — this locked in the wrong behavior. Verified
        // against pybambu/Bambuddy's independent reverse-engineering (see
        // AMS_TRAY_STATE_SPOOL_NOT_FED's doc comment): state 10 is one of the firmware's two
        // explicit "not loaded" signals (alongside state 9) regardless of what stale metadata
        // is still attached, so it must clear here too.
        let mut tray = AmsTray {
            id: "0".into(),
            state: Some(10),
            tray_type: Some("PLA".into()),
            tray_color: Some("FF0000FF".into()),
            tray_info_idx: Some("GFA01".into()),
            tag_uid: None,
            tray_uuid: None,
            remain: Some(85),
            ..Default::default()
        };

        clean_stale_tray_data(&mut tray);

        assert_eq!(tray.tray_type, None);
        assert_eq!(tray.tray_color, None);
        assert_eq!(tray.remain, Some(-1));
    }

    #[test]
    fn test_clean_stale_tray_data_state_10_without_type() {
        // State 10 with absent tray_type — H2D incremental case, should clear
        let mut tray = AmsTray {
            id: "0".into(),
            state: Some(10),
            tray_type: None,
            tray_color: Some("FF0000FF".into()),
            tray_info_idx: None,
            tag_uid: None,
            tray_uuid: None,
            remain: Some(85),
            ..Default::default()
        };

        clean_stale_tray_data(&mut tray);

        assert_eq!(tray.tray_color, None);
        assert_eq!(tray.remain, Some(-1));
    }

    #[test]
    fn test_clean_stale_tray_data_none_state_defaults_to_9() {
        let mut tray = AmsTray {
            id: "2".into(),
            state: None,
            tray_type: None,
            tray_color: None,
            tray_info_idx: None,
            tag_uid: None,
            tray_uuid: None,
            remain: None,
            ..Default::default()
        };

        clean_stale_tray_data(&mut tray);

        assert_eq!(tray.state, Some(AMS_TRAY_STATE_EMPTY));
        assert_eq!(tray.remain, Some(-1));
    }
}
