//! # AMS Slicer Mapping & Filament Change Builders
//!
//! Handles translation of slicer-allocated project materials into physical and
//! virtual printer hardware channels [REF-AMS-MAP]. Implements flat `ams_mapping` and
//! structured `ams_mapping2` payload arrays and enforces safety interlocks for single-nozzle
//! external spools [REF-AMS-USEAMS].

#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::parser::{AMS_EXTERNAL_SPOOL_ALT_ID, AMS_EXTERNAL_SPOOL_ID};
use serde::{Deserialize, Serialize};

/// Enumeration of possible physical feed locations for loaded spools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialSource {
    /// Spool loaded inside a standard 4-slot AMS unit.
    StandardAms {
        /// AMS unit index (0-3).
        ams_id: u8,
        /// Tray slot index within the unit (0-3).
        slot_id: u8,
    },
    /// Spool loaded inside a single-slot High-Temperature (AMS-HT) dry-chamber.
    AmsHt {
        /// AMS-HT unit index (128+, per `AmsMapping2Entry::ams_id`'s range note).
        ams_id: u8,
    },
    /// Default virtual external spool holder (used for standard single-nozzle models).
    ExternalSpool,
    /// Left external spool holder (specifically used on dual-nozzle IDEX systems).
    ExternalSpoolLeft,
    /// Right external spool holder (specifically used on dual-nozzle IDEX systems).
    ExternalSpoolRight,
    /// Virtual unmapped placeholder slot (indicates an unused project filament).
    Unmapped,
}

impl MaterialSource {
    /// Computes the flat channel integer value used in standard `ams_mapping` arrays.
    ///
    /// **External Spool Flat-Mapping Restrictions [REF-AMS-MAP]:**
    /// The printer's motion controller rejects absolute external virtual spool IDs (such as
    /// 254 or 255) if passed inside the flat `ams_mapping` array, throwing a `0700_8012`
    /// "Failed to get AMS mapping table" error. Virtual external spools and unused slots
    /// must strictly be mapped to the `-1` (unmapped) sentinel in the flat array.
    ///
    /// BUG-069: `StandardAms`/`AmsHt` fields are public `u8`s a caller can hand-build with an
    /// out-of-range `ams_id`/`slot_id` (unlike `parser.rs`'s inbound-side bounds-checking on
    /// wire data) — validated here the same way, falling back to the `-1` sentinel rather than
    /// producing a bogus flat channel value.
    pub fn flat_channel_id(&self) -> i32 {
        match self {
            MaterialSource::StandardAms { ams_id, slot_id } => {
                if *ams_id <= super::parser::AMS_MAX_STANDARD_ID
                    && *slot_id < super::parser::AMS_SLOTS_PER_UNIT
                {
                    ((*ams_id as i32) * super::parser::AMS_SLOTS_PER_UNIT as i32)
                        + (*slot_id as i32)
                } else {
                    -1
                }
            }
            MaterialSource::AmsHt { ams_id }
                if (super::parser::AMS_HT_ID_MIN..=super::parser::AMS_HT_ID_MAX)
                    .contains(ams_id) =>
            {
                *ams_id as i32
            }
            _ => -1, // External and unmapped slots are strictly mapped to -1
        }
    }

    /// Converts this source location into a structured `ams_mapping2` JSON entry.
    pub fn to_mapping2_entry(&self) -> AmsMapping2Entry {
        match self {
            // BUG-069: same out-of-range validation as flat_channel_id() — an invalid
            // ams_id/slot_id falls back to the same unmapped sentinel entry as
            // MaterialSource::Unmapped, rather than serializing a bogus StandardAms/AmsHt entry.
            MaterialSource::StandardAms { ams_id, slot_id } => {
                if *ams_id <= super::parser::AMS_MAX_STANDARD_ID
                    && *slot_id < super::parser::AMS_SLOTS_PER_UNIT
                {
                    AmsMapping2Entry {
                        ams_id: *ams_id,
                        slot_id: *slot_id,
                    }
                } else {
                    AmsMapping2Entry {
                        ams_id: AMS_EXTERNAL_SPOOL_ALT_ID,
                        slot_id: AMS_EXTERNAL_SPOOL_ALT_ID,
                    }
                }
            }
            MaterialSource::AmsHt { ams_id } => {
                if (super::parser::AMS_HT_ID_MIN..=super::parser::AMS_HT_ID_MAX).contains(ams_id) {
                    AmsMapping2Entry {
                        ams_id: *ams_id,
                        slot_id: 0,
                    }
                } else {
                    AmsMapping2Entry {
                        ams_id: AMS_EXTERNAL_SPOOL_ALT_ID,
                        slot_id: AMS_EXTERNAL_SPOOL_ALT_ID,
                    }
                }
            }
            MaterialSource::ExternalSpool => AmsMapping2Entry {
                ams_id: AMS_EXTERNAL_SPOOL_ALT_ID,
                slot_id: 0,
            },
            MaterialSource::ExternalSpoolLeft => AmsMapping2Entry {
                ams_id: AMS_EXTERNAL_SPOOL_ID,
                slot_id: 0,
            },
            MaterialSource::ExternalSpoolRight => AmsMapping2Entry {
                ams_id: AMS_EXTERNAL_SPOOL_ALT_ID,
                slot_id: 0,
            },
            MaterialSource::Unmapped => AmsMapping2Entry {
                ams_id: AMS_EXTERNAL_SPOOL_ALT_ID,
                slot_id: AMS_EXTERNAL_SPOOL_ALT_ID,
            },
        }
    }
}

/// Structured object detailing unit and slot coordinates within `ams_mapping2` arrays.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmsMapping2Entry {
    /// AMS unit index (0-3 for standard, 128+ for AMS-HT, 254/255 for external/unmapped).
    pub ams_id: u8,
    /// Tray slot index within the unit (0-3 for standard AMS, 0 for single-slot units).
    pub slot_id: u8,
}

/// Computes the flat `ams_mapping` channel value an `AmsMapping2Entry` corresponds to.
///
/// Inverse of `MaterialSource::flat_channel_id`, operating on the already-structured
/// `ams_id`/`slot_id` pair instead of a `MaterialSource` — used by
/// `ProjectFileRequest::from_config` (`mqtt/commands/print_job.rs`) to derive `ams_mapping`
/// from `ams_mapping2` when the caller only supplied the latter via
/// `PrintJobConfig::with_ams_mapping2()`, so the two arrays never go out of sync (BUG-033).
pub fn flat_channel_id_for_entry(entry: &AmsMapping2Entry) -> i32 {
    if entry.ams_id <= super::parser::AMS_MAX_STANDARD_ID {
        (entry.ams_id as i32) * (super::parser::AMS_SLOTS_PER_UNIT as i32) + entry.slot_id as i32
    } else if (super::parser::AMS_HT_ID_MIN..=super::parser::AMS_HT_ID_MAX).contains(&entry.ams_id)
    {
        entry.ams_id as i32
    } else {
        -1 // External and unmapped slots are strictly mapped to -1, same as MaterialSource's rule.
    }
}

/// Builds the flat `ams_mapping` integer array from raw project allocations.
///
/// `allocations` is a slice of `(filament_id, MaterialSource)` pairs where `filament_id`
/// represents the 1-based index (1 to N) of the project material defined in the slicer.
///
/// **Array Length Rule [REF-AMS-MAP]:**
/// The length of the array is governed by the highest filament ID index present in the project,
/// rather than the total count of active spools. If a project uses filament 1 and filament 4,
/// the array must be padded to a length of 4 (elements 0 to 3), using the `-1` sentinel for
/// intermediate unused filament indexes.
pub fn build_ams_mapping(allocations: &[(usize, MaterialSource)]) -> Vec<i32> {
    if allocations.is_empty() {
        return Vec::new();
    }
    let max_id = allocations.iter().map(|(id, _)| *id).max().unwrap_or(1);
    let mut mapping = vec![-1; max_id];

    for (id, source) in allocations {
        if *id > 0 && *id <= max_id {
            mapping[*id - 1] = source.flat_channel_id();
        } else {
            // BUG-070: filament_id is documented as 1-based (1 to N) — id == 0 (or > max_id,
            // unreachable since max_id is derived from this same slice) is a caller bug, not a
            // legitimately-skippable entry. Silently dropping it previously left that project
            // slot permanently unmapped with no operator-visible signal.
            log::warn!(
                "build_ams_mapping: dropping allocation with out-of-range filament_id {id} (valid range is 1..={max_id})"
            );
        }
    }
    mapping
}

/// Builds the structured `ams_mapping2` object array from raw project allocations.
///
/// Symmetrical to `build_ams_mapping`, this array provides detailed physical unit routing
/// parameters to ensure correct material transitions on multi-AMS and IDEX platforms.
pub fn build_ams_mapping2(allocations: &[(usize, MaterialSource)]) -> Vec<AmsMapping2Entry> {
    if allocations.is_empty() {
        return Vec::new();
    }
    let max_id = allocations.iter().map(|(id, _)| *id).max().unwrap_or(1);
    let mut mapping2 = vec![
        AmsMapping2Entry {
            ams_id: AMS_EXTERNAL_SPOOL_ALT_ID,
            slot_id: AMS_EXTERNAL_SPOOL_ALT_ID
        };
        max_id
    ];

    for (id, source) in allocations {
        if *id > 0 && *id <= max_id {
            mapping2[*id - 1] = source.to_mapping2_entry();
        } else {
            // BUG-070: same reasoning as build_ams_mapping's else arm.
            log::warn!(
                "build_ams_mapping2: dropping allocation with out-of-range filament_id {id} (valid range is 1..={max_id})"
            );
        }
    }
    mapping2
}

/// Verifies whether standard expansion systems are active, returning the safe `use_ams` toggle.
///
/// **Mandatory use_ams Override on Single-Nozzle Systems [REF-AMS-USEAMS]:**
/// If a print job is dispatched exclusively from the external spool (meaning all active project
/// filaments map to `ExternalSpool` or are left `Unmapped`), single-nozzle printers require that the
/// `use_ams` command parameter be configured strictly to `false`. Failing to override this parameter
/// causes the printer's execution processor to reject the print task with error `07FF_8012`.
pub fn validate_external_spool_safety(
    is_single_nozzle: bool,
    mapping2: &[AmsMapping2Entry],
) -> bool {
    if !is_single_nozzle {
        // Dual-nozzle systems track this polymorphically using alternate indexing rules,
        // and do not suffer from standard single-nozzle table exceptions.
        return true;
    }

    let mut has_physical_ams = false;
    for entry in mapping2 {
        let is_unmapped =
            entry.ams_id == AMS_EXTERNAL_SPOOL_ALT_ID && entry.slot_id == AMS_EXTERNAL_SPOOL_ALT_ID;
        // Checks both external-spool IDs (254 and 255), matching validate_external_spool_safety_flat's
        // uniform treatment — AmsMapping2Entry's fields are public, so a caller can hand-build
        // an entry with ams_id 254 (normally IDEX-only, via MaterialSource::ExternalSpoolLeft)
        // on a single-nozzle printer; checking only 255 here let that case slip through and
        // dispatch use_ams:true for a non-physical channel, reproducing the 07FF_8012 lockup
        // this function exists to prevent.
        let is_external = (entry.ams_id == AMS_EXTERNAL_SPOOL_ALT_ID
            || entry.ams_id == AMS_EXTERNAL_SPOOL_ID)
            && entry.slot_id == 0;
        if !is_unmapped && !is_external {
            has_physical_ams = true;
            break;
        }
    }

    has_physical_ams
}

/// Per-model AMS unit pool structure (BUG-122), confirmed against `MODEL_MATRIX.csv`'s
/// "AMS Unit Limits" row (user-supplied official Bambu documentation).
///
/// **Known limitation**: AMS Lite units are not independently addressable in this model —
/// they use the same `ams_id` space as standard AMS units — so A1/A1 Mini's "shared pool OR
/// 1 AMS Lite, not combinable" exclusivity and A2L's "shared pool + 1 AMS Lite simultaneously"
/// additive capacity can't be validated from `ams_id`/`slot_id` alone. Both are conservatively
/// modeled as `Shared { max_units: 4 }`, the same as the plain shared-pool models — this may
/// under-count A2L's true capacity by one unit, but never accepts a config that's actually
/// invalid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmsPoolComposition {
    /// Standard AMS and AMS-HT units draw from one combined pool of `max_units` total
    /// (X1C, X1E, P1P, P1S, A1, A1 Mini, A2L).
    Shared {
        /// Maximum combined standard + AMS-HT unit count.
        max_units: u8,
    },
    /// Standard AMS and AMS-HT units draw from independent pools, each with its own cap
    /// (H2C, H2D, H2D Pro, H2S, X2D, P2S).
    Independent {
        /// Maximum standard AMS unit count.
        max_standard: u8,
        /// Maximum AMS-HT unit count.
        max_ht: u8,
    },
}

/// Validates a constructed `ams_mapping2` against the model's actual AMS pool structure
/// (BUG-122). Rejects configs no real hardware combination could serve — e.g. 4 standard +
/// 8 AMS-HT units on a P2S, which only has independent pools of 4 and 4.
///
/// Counts *distinct* `ams_id`s used (not slot allocations) — a config referencing the same
/// unit across multiple slots isn't an extra unit. External-spool and unmapped sentinel
/// entries are ignored, since they don't occupy a physical AMS unit slot.
pub fn validate_ams_pool_composition(
    mapping2: &[AmsMapping2Entry],
    composition: AmsPoolComposition,
) -> bool {
    let mut standard_ids = Vec::new();
    let mut ht_ids = Vec::new();
    for entry in mapping2 {
        if entry.ams_id <= super::parser::AMS_MAX_STANDARD_ID {
            if !standard_ids.contains(&entry.ams_id) {
                standard_ids.push(entry.ams_id);
            }
        } else if (super::parser::AMS_HT_ID_MIN..=super::parser::AMS_HT_ID_MAX)
            .contains(&entry.ams_id)
            && !ht_ids.contains(&entry.ams_id)
        {
            ht_ids.push(entry.ams_id);
        }
    }

    match composition {
        AmsPoolComposition::Shared { max_units } => {
            (standard_ids.len() + ht_ids.len()) as u8 <= max_units
        }
        AmsPoolComposition::Independent {
            max_standard,
            max_ht,
        } => standard_ids.len() as u8 <= max_standard && ht_ids.len() as u8 <= max_ht,
    }
}

/// Flat-array equivalent of `validate_external_spool_safety`, for callers using `PrintJobConfig::with_ams()` (flat `Vec<i32>`) rather than `with_ams_mapping2()`.
pub fn validate_external_spool_safety_flat(is_single_nozzle: bool, ams_mapping: &[i32]) -> bool {
    if !is_single_nozzle {
        return true;
    }
    ams_mapping.iter().any(|&v| v >= 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_ams_mapping_flat() {
        // Project uses Filament 1 and Filament 3
        // Filament 1 mapped to AMS 0 Slot 1 -> Flat Channel (0 * 4) + 1 = 1
        // Filament 3 mapped to External Spool -> Flat Channel strictly -1 [REF-AMS-MAP]
        let allocations = [
            (
                1,
                MaterialSource::StandardAms {
                    ams_id: 0,
                    slot_id: 1,
                },
            ),
            (3, MaterialSource::ExternalSpool),
        ];

        let flat_map = build_ams_mapping(&allocations);
        // Sized to max_id = 3
        assert_eq!(flat_map.len(), 3);
        assert_eq!(flat_map, vec![1, -1, -1]);
    }

    #[test]
    fn test_build_ams_mapping2_structured() {
        let allocations = [
            (
                1,
                MaterialSource::StandardAms {
                    ams_id: 1,
                    slot_id: 2,
                },
            ),
            (2, MaterialSource::ExternalSpool),
            (4, MaterialSource::Unmapped),
        ];

        let mapping2 = build_ams_mapping2(&allocations);
        assert_eq!(mapping2.len(), 4);
        assert_eq!(
            mapping2[0],
            AmsMapping2Entry {
                ams_id: 1,
                slot_id: 2
            }
        );
        assert_eq!(
            mapping2[1],
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 0
            }
        );
        assert_eq!(
            mapping2[2],
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 255
            }
        ); // Padded unmapped
        assert_eq!(
            mapping2[3],
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 255
            }
        ); // Direct unmapped
    }

    #[test]
    fn test_validate_external_spool_safety_single_nozzle() {
        // Case 1: All elements are external/unmapped -> use_ams must override to false
        let mapping_all_external = vec![
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 0,
            }, // External
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 255,
            }, // Unmapped
        ];
        let use_ams_override = validate_external_spool_safety(true, &mapping_all_external);
        assert!(!use_ams_override);

        // Case 2: At least one standard physical AMS unit is mapped -> use_ams stays true
        let mapping_with_ams = vec![
            AmsMapping2Entry {
                ams_id: 0,
                slot_id: 1,
            }, // Physical
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 0,
            }, // External
        ];
        let use_ams_ok = validate_external_spool_safety(true, &mapping_with_ams);
        assert!(use_ams_ok);
    }

    #[test]
    fn test_validate_external_spool_safety_single_nozzle_ams_id_254() {
        // ams_id 254 (AMS_EXTERNAL_SPOOL_ID) is normally only produced by
        // MaterialSource::ExternalSpoolLeft on IDEX builds, but AmsMapping2Entry's fields are
        // public — a caller can hand-build one with 254 on a single-nozzle printer too. Must be
        // treated as external the same as 255, or use_ams would stay true for a non-physical
        // channel and reproduce the 07FF_8012 firmware lockup this function exists to prevent.
        let mapping_all_external = vec![AmsMapping2Entry {
            ams_id: 254,
            slot_id: 0,
        }];
        assert!(!validate_external_spool_safety(true, &mapping_all_external));
    }

    #[test]
    fn test_validate_external_spool_safety_with_ams_ht() {
        // AMS-HT (ams_id 128) counts as physical — use_ams stays true
        let mapping = vec![AmsMapping2Entry {
            ams_id: 128,
            slot_id: 0,
        }];
        assert!(validate_external_spool_safety(true, &mapping));
    }

    #[test]
    fn test_validate_ams_pool_composition_shared_within_limit() {
        // X1C/P1S/A1-style: 4 standard + HT share one pool. 2 standard + 1 HT = 3, within 4.
        let mapping = vec![
            AmsMapping2Entry {
                ams_id: 0,
                slot_id: 0,
            },
            AmsMapping2Entry {
                ams_id: 1,
                slot_id: 0,
            },
            AmsMapping2Entry {
                ams_id: 128,
                slot_id: 0,
            },
        ];
        assert!(validate_ams_pool_composition(
            &mapping,
            AmsPoolComposition::Shared { max_units: 4 }
        ));
    }

    #[test]
    fn test_validate_ams_pool_composition_shared_over_limit() {
        // 4 standard + 1 HT = 5, exceeds a shared pool of 4.
        let mapping = vec![
            AmsMapping2Entry {
                ams_id: 0,
                slot_id: 0,
            },
            AmsMapping2Entry {
                ams_id: 1,
                slot_id: 0,
            },
            AmsMapping2Entry {
                ams_id: 2,
                slot_id: 0,
            },
            AmsMapping2Entry {
                ams_id: 3,
                slot_id: 0,
            },
            AmsMapping2Entry {
                ams_id: 128,
                slot_id: 0,
            },
        ];
        assert!(!validate_ams_pool_composition(
            &mapping,
            AmsPoolComposition::Shared { max_units: 4 }
        ));
    }

    #[test]
    fn test_validate_ams_pool_composition_independent_pools() {
        // P2S-style: independent pools of 4 standard + 4 HT. 4 standard + 4 HT is valid;
        // an unbuildable config (4 standard + 8 HT, this bug's motivating example) is not.
        let valid = vec![
            AmsMapping2Entry {
                ams_id: 0,
                slot_id: 0,
            },
            AmsMapping2Entry {
                ams_id: 128,
                slot_id: 0,
            },
        ];
        assert!(validate_ams_pool_composition(
            &valid,
            AmsPoolComposition::Independent {
                max_standard: 4,
                max_ht: 4,
            }
        ));

        let too_many_ht: Vec<AmsMapping2Entry> = (128..=135)
            .map(|ams_id| AmsMapping2Entry { ams_id, slot_id: 0 })
            .collect();
        assert!(!validate_ams_pool_composition(
            &too_many_ht,
            AmsPoolComposition::Independent {
                max_standard: 4,
                max_ht: 4,
            }
        ));
    }

    #[test]
    fn test_validate_ams_pool_composition_ignores_external_and_repeated_units() {
        // External spool entries and duplicate ams_ids (multiple slots on the same unit)
        // don't count against the pool.
        let mapping = vec![
            AmsMapping2Entry {
                ams_id: 0,
                slot_id: 0,
            },
            AmsMapping2Entry {
                ams_id: 0,
                slot_id: 1,
            },
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 255,
            },
        ];
        assert!(validate_ams_pool_composition(
            &mapping,
            AmsPoolComposition::Shared { max_units: 1 }
        ));
    }

    #[test]
    fn test_validate_external_spool_safety_dual_nozzle_bypasses() {
        // Dual-nozzle always returns true regardless of mapping contents
        let mapping_all_external = vec![AmsMapping2Entry {
            ams_id: 255,
            slot_id: 0,
        }];
        assert!(validate_external_spool_safety(false, &mapping_all_external));
    }

    #[test]
    fn test_validate_external_spool_safety_flat_single_nozzle() {
        // All slots unmapped/external (`-1` sentinel) -> use_ams must override to false
        assert!(!validate_external_spool_safety_flat(true, &[-1, -1]));

        // At least one real physical AMS channel -> use_ams stays true
        assert!(validate_external_spool_safety_flat(true, &[0, -1, 1]));
    }

    #[test]
    fn test_validate_external_spool_safety_flat_dual_nozzle_bypasses() {
        // Dual-nozzle always returns true regardless of mapping contents
        assert!(validate_external_spool_safety_flat(false, &[-1, -1]));
    }

    #[test]
    fn test_build_ams_mapping_ams_ht() {
        let allocations = [(1, MaterialSource::AmsHt { ams_id: 128 })];
        let flat_map = build_ams_mapping(&allocations);
        assert_eq!(flat_map, vec![128]);
    }

    #[test]
    fn test_material_source_out_of_range_rejected() {
        // BUG-069: MaterialSource::StandardAms/AmsHt fields are public u8s a caller can
        // hand-build with an out-of-range value — must fall back to the -1/unmapped sentinel
        // rather than producing a bogus flat channel or wire entry.
        let bad_standard = MaterialSource::StandardAms {
            ams_id: 200,
            slot_id: 0,
        };
        assert_eq!(bad_standard.flat_channel_id(), -1);
        assert_eq!(
            bad_standard.to_mapping2_entry(),
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 255
            }
        );

        let bad_slot = MaterialSource::StandardAms {
            ams_id: 0,
            slot_id: 200,
        };
        assert_eq!(bad_slot.flat_channel_id(), -1);

        let bad_ht = MaterialSource::AmsHt { ams_id: 50 };
        assert_eq!(bad_ht.flat_channel_id(), -1);
        assert_eq!(
            bad_ht.to_mapping2_entry(),
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 255
            }
        );
    }

    #[test]
    fn test_build_ams_mapping_empty() {
        let allocations: [(usize, MaterialSource); 0] = [];
        assert!(build_ams_mapping(&allocations).is_empty());
        assert!(build_ams_mapping2(&allocations).is_empty());
    }

    #[test]
    fn test_build_ams_mapping_idex_external_spools() {
        let allocations = [
            (1, MaterialSource::ExternalSpoolLeft),
            (2, MaterialSource::ExternalSpoolRight),
        ];
        let flat_map = build_ams_mapping(&allocations);
        assert_eq!(flat_map, vec![-1, -1]);

        let mapping2 = build_ams_mapping2(&allocations);
        assert_eq!(
            mapping2[0],
            AmsMapping2Entry {
                ams_id: 254,
                slot_id: 0
            }
        );
        assert_eq!(
            mapping2[1],
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 0
            }
        );
    }
}
