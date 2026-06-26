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
    StandardAms { ams_id: u8, slot_id: u8 },
    /// Spool loaded inside a single-slot High-Temperature (AMS-HT) dry-chamber.
    AmsHt { ams_id: u8 },
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
    pub fn flat_channel_id(&self) -> i32 {
        match self {
            MaterialSource::StandardAms { ams_id, slot_id } => {
                ((*ams_id as i32) * 4) + (*slot_id as i32)
            }
            MaterialSource::AmsHt { ams_id } => *ams_id as i32,
            _ => -1, // External and unmapped slots are strictly mapped to -1
        }
    }

    /// Converts this source location into a structured `ams_mapping2` JSON entry.
    pub fn to_mapping2_entry(&self) -> AmsMapping2Entry {
        match self {
            MaterialSource::StandardAms { ams_id, slot_id } => AmsMapping2Entry {
                ams_id: *ams_id,
                slot_id: *slot_id,
            },
            MaterialSource::AmsHt { ams_id } => AmsMapping2Entry {
                ams_id: *ams_id,
                slot_id: 0,
            },
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
    pub ams_id: u8,
    pub slot_id: u8,
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
        let is_external = entry.ams_id == AMS_EXTERNAL_SPOOL_ALT_ID && entry.slot_id == 0;
        if !is_unmapped && !is_external {
            has_physical_ams = true;
            break;
        }
    }

    has_physical_ams
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
    fn test_validate_external_spool_safety_with_ams_ht() {
        // AMS-HT (ams_id 128) counts as physical — use_ams stays true
        let mapping = vec![AmsMapping2Entry {
            ams_id: 128,
            slot_id: 0,
        }];
        assert!(validate_external_spool_safety(true, &mapping));
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
    fn test_build_ams_mapping_ams_ht() {
        let allocations = [(1, MaterialSource::AmsHt { ams_id: 128 })];
        let flat_map = build_ams_mapping(&allocations);
        assert_eq!(flat_map, vec![128]);
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
