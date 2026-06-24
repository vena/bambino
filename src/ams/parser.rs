//! # AMS Telemetry & Bitmask Parser
//!
//! Implements low-level bitwise operations and sanitization logic for parsing
//! Bambu Lab AMS telemetry reports [REF-AMS-DECODE]. This includes checking spool
//! presence via hex bitmasks, managing power-down state anomalies, cleansing stale
//! tray data, and calculating global indexes.

use crate::types::AmsTray;

pub(crate) const AMS_SLOTS_PER_UNIT: u8 = 4;
pub(crate) const AMS_HT_ID_MIN: u8 = 128;
pub(crate) const AMS_HT_ID_MAX: u8 = 135;
pub(crate) const AMS_EXTERNAL_SPOOL_ID: u8 = 254;
pub(crate) const AMS_EXTERNAL_SPOOL_ALT_ID: u8 = 255;
pub(crate) const AMS_TRAY_STATE_EMPTY: u8 = 9;
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
    // and do not participate in standard bitwise exists strings.
    if (AMS_HT_ID_MIN..=AMS_HT_ID_MAX).contains(&ams_id) {
        return Some(true);
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
/// This routine inspects the tray's state (with 9 representing Empty / Absent) and clears all
/// stale config keys if empty. It treats an empty `tray_type` string as an explicit clearing signal.
pub fn clean_stale_tray_data(tray: &mut AmsTray) {
    let is_absent_state = matches!(
        tray.state,
        Some(AMS_TRAY_STATE_EMPTY) | Some(AMS_TRAY_STATE_POWER_OFF) | None
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

        // Standardize absent state representation to 9
        if tray.state.is_none() {
            tray.state = Some(AMS_TRAY_STATE_EMPTY);
        }
    }
}

/// Computes the unique global channel identifier for a given expansion unit and local tray.
///
/// The physical mapping aligns as:
/// * **Standard AMS Slots**: Sized in blocks of 4 per expansion unit: `(ams_id * 4) + tray_id`.
/// * **AMS-HT Units**: Single-slot systems where the channel ID equals the bus `ams_id` directly.
/// * **Virtual Spools**: Channels mapped to the external spool holder (ID 254 or 255).
pub fn resolve_global_tray_id(ams_id: u8, tray_id: u8) -> u8 {
    if (AMS_HT_ID_MIN..=AMS_HT_ID_MAX).contains(&ams_id) {
        ams_id
    } else if ams_id == AMS_EXTERNAL_SPOOL_ID || ams_id == AMS_EXTERNAL_SPOOL_ALT_ID {
        tray_id
    } else {
        (ams_id * AMS_SLOTS_PER_UNIT) + tray_id
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
    if let Some(extruder) = active_extruder {
        let idx = extruder as usize;
        if idx < ams_extruder_map.len() {
            let ams_id = ams_extruder_map[idx];
            return Some(resolve_global_tray_id(ams_id, tray_now));
        }
    }
    None
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
            id: 0,
            state: Some(9),
            tray_type: Some("PLA".into()),
            tray_color: Some("FFFFFFFF".into()),
            tray_info_idx: Some("GFA01".into()),
            tag_uid: Some("ABCDEF1234567890".into()),
            tray_uuid: Some("UUID_SOME_MOCK_VAL".into()),
            remain: Some(85),
        };

        clean_stale_tray_data(&mut tray);

        assert_eq!(tray.tray_type, None);
        assert_eq!(tray.tray_color, None);
        assert_eq!(tray.tag_uid, None);
        assert_eq!(tray.remain, Some(-1));
    }

    #[test]
    fn test_clean_stale_tray_data_empty_string() {
        let mut tray = AmsTray {
            id: 1,
            state: Some(10),            // Supposedly present but retracted
            tray_type: Some("".into()), // Empty type constitutes a clearing signal
            tray_color: Some("FFFFFFFF".into()),
            tray_info_idx: None,
            tag_uid: None,
            tray_uuid: None,
            remain: Some(100),
        };

        clean_stale_tray_data(&mut tray);

        assert_eq!(tray.tray_color, None);
        assert_eq!(tray.remain, Some(-1));
    }

    #[test]
    fn test_resolve_global_tray_id() {
        // Standard physical AMS 1, slot 2 -> (1 * 4) + 2 = 6
        assert_eq!(resolve_global_tray_id(1, 2), 6);

        // Standard physical AMS 2, slot 0 -> (2 * 4) + 0 = 8
        assert_eq!(resolve_global_tray_id(2, 0), 8);

        // High-temperature dry-chamber (AMS-HT) ID 128
        assert_eq!(resolve_global_tray_id(128, 0), 128);

        // Virtual spool target 254
        assert_eq!(resolve_global_tray_id(254, 254), 254);
    }

    #[test]
    fn test_resolve_printing_global_id_idex() {
        // Active extruder is Deputy (1), ams_extruder_map routes extruder 1 to ams_id 1
        let ams_extruder_map = [0u8, 1u8];
        let global_id = resolve_printing_global_id(2, Some(1), &ams_extruder_map);
        // (1 * 4) + 2 = 6
        assert_eq!(global_id, Some(6));
    }
}
