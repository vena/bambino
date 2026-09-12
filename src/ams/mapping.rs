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

use super::parser::{AMS_EXTERNAL_SPOOL_DEPUTY_ID, AMS_EXTERNAL_SPOOL_MAIN_ID};
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
        /// AMS-HT unit index (128-135, per `AmsMapping2Entry::ams_id`'s range note).
        ams_id: u8,
    },
    /// Spool loaded inside the A2L's 4-slot AMS Lite.
    ///
    /// Kept separate from [`MaterialSource::StandardAms`] because this unit's two wire
    /// encodings do not follow the standard unit's: the flat `ams_mapping` array carries the
    /// bare **local** slot rather than a global `ams_id * 4 + slot` channel, and `ams_mapping2`
    /// carries the **physical** unit id 16 rather than the normalized 6 the rest of the crate
    /// addresses it by. A `StandardAms { ams_id: 6, .. }` would get both wrong.
    AmsLite {
        /// Tray slot index within the unit (0-3).
        slot_id: u8,
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
    /// `StandardAms`/`AmsHt` fields are public `u8`s a caller can hand-build with an
    /// out-of-range `ams_id`/`slot_id` (unlike `parser.rs`'s inbound-side bounds-checking on
    /// wire data) — validated here the same way, falling back to the `-1` sentinel rather than
    /// producing a bogus flat channel value.
    #[must_use]
    pub fn flat_channel_id(&self) -> i32 {
        match self {
            MaterialSource::StandardAms { ams_id, slot_id }
                if *ams_id <= super::parser::AMS_MAX_STANDARD_ID
                    && *slot_id < super::parser::AMS_SLOTS_PER_UNIT =>
            {
                ((*ams_id as i32) * super::parser::AMS_SLOTS_PER_UNIT as i32) + (*slot_id as i32)
            }
            MaterialSource::AmsHt { ams_id }
                if (super::parser::AMS_HT_ID_MIN..=super::parser::AMS_HT_ID_MAX)
                    .contains(ams_id) =>
            {
                *ams_id as i32
            }
            // The flat array's encoding is per-unit-type, not uniformly "global channel id":
            // AMS Lite puts a bare local slot 0-3 here. CONFIRMED by bambuddy against the
            // firmware's own mapping — a captured flat `[1]` paired with an `ams_mapping2`
            // entry of `{"ams_id": 16, "slot_id": 1}`.
            MaterialSource::AmsLite { slot_id } if *slot_id < super::parser::AMS_SLOTS_PER_UNIT => {
                *slot_id as i32
            }
            _ => -1, // External and unmapped slots are strictly mapped to -1
        }
    }

    /// Converts this source location into a structured `ams_mapping2` JSON entry.
    #[must_use]
    pub fn to_mapping2_entry(&self) -> AmsMapping2Entry {
        match self {
            // Same out-of-range validation as flat_channel_id() — an invalid
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
                        ams_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
                        slot_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
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
                        ams_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
                        slot_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
                    }
                }
            }
            // `ams_mapping2` is the one place an A2L-attached AMS Lite's *physical* id 16 goes back on
            // the wire, paired with a local slot. CONFIRMED against the firmware's own mapping
            // (bambuddy's `a2l_lite_wire_ids`) and BambuStudio, which writes the unreduced
            // `ams_id` into its mapping entry (`DevMapping.cpp:88-89`).
            MaterialSource::AmsLite { slot_id } => {
                if *slot_id < super::parser::AMS_SLOTS_PER_UNIT {
                    AmsMapping2Entry {
                        ams_id: super::parser::AMS_LITE_ON_A2L_PHYSICAL_ID,
                        slot_id: *slot_id,
                    }
                } else {
                    AmsMapping2Entry {
                        ams_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
                        slot_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
                    }
                }
            }
            MaterialSource::ExternalSpool => AmsMapping2Entry {
                ams_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
                slot_id: 0,
            },
            MaterialSource::ExternalSpoolLeft => AmsMapping2Entry {
                ams_id: AMS_EXTERNAL_SPOOL_DEPUTY_ID,
                slot_id: 0,
            },
            MaterialSource::ExternalSpoolRight => AmsMapping2Entry {
                ams_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
                slot_id: 0,
            },
            MaterialSource::Unmapped => AmsMapping2Entry {
                ams_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
                slot_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
            },
        }
    }
}

/// Structured object detailing unit and slot coordinates within `ams_mapping2` arrays.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AmsMapping2Entry {
    /// AMS unit index (0-3 for standard, 16 for an A2L-attached AMS Lite, 128-135 for AMS-HT,
    /// 254/255 for external/unmapped).
    ///
    /// Id 16 is a real physical unit, not a sentinel — omitting it here is the exact footgun
    /// [`AmsEntryKind`] warns about, since a caller reading this field in isolation could add
    /// defensive rejection that breaks A2L support (issue #221). Classify with
    /// [`classify_mapping2_entry`] rather than re-deriving the ranges.
    pub ams_id: u8,
    /// Tray slot index within the unit (0-3 for standard AMS, 0 for single-slot units).
    pub slot_id: u8,
}

/// What kind of feed location a raw [`AmsMapping2Entry`]'s `ams_id`/`slot_id` pair names.
///
/// Single place the "which `ams_id`s are real physical units" rule lives. `MaterialSource`'s
/// own methods can rely on the enum variant to tell them, but every function that instead
/// re-derives physical-ness from a hand-built `AmsMapping2Entry` has to reproduce the same
/// range checks — and each one independently missed an A2L-attached AMS Lite's physical id 16 when
/// it was added (issue #221). Route those through [`classify_mapping2_entry`] so a new one
/// cannot omit a unit type by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmsEntryKind {
    /// Standard 4-slot AMS unit, `ams_id` 0-3 with an in-range `slot_id`.
    Standard,
    /// Single-slot AMS-HT unit, `ams_id` 128-135.
    Ht,
    /// The A2L's 4-slot AMS Lite, carried on the wire as physical unit id 16.
    AmsLite,
    /// One of the two external-spool sentinel ids (254/255), including the
    /// `{255, 255}` unmapped sentinel.
    External,
    /// Neither a validly-ranged physical unit nor a recognized sentinel.
    Invalid,
}

/// Classifies a raw `ams_mapping2` entry by the kind of feed location it names.
///
/// `slot_id` is range-checked for the multi-slot unit types (standard and AMS Lite) but
/// ignored for AMS-HT, which is single-slot and encodes nothing in the field, and for the
/// external sentinels, whose `slot_id` is `0` or `255` depending on whether the entry means
/// "external spool" or "unmapped".
#[must_use]
pub fn classify_mapping2_entry(entry: &AmsMapping2Entry) -> AmsEntryKind {
    if entry.ams_id == AMS_EXTERNAL_SPOOL_MAIN_ID || entry.ams_id == AMS_EXTERNAL_SPOOL_DEPUTY_ID {
        AmsEntryKind::External
    } else if entry.ams_id <= super::parser::AMS_MAX_STANDARD_ID
        && entry.slot_id < super::parser::AMS_SLOTS_PER_UNIT
    {
        AmsEntryKind::Standard
    } else if (super::parser::AMS_HT_ID_MIN..=super::parser::AMS_HT_ID_MAX).contains(&entry.ams_id)
    {
        AmsEntryKind::Ht
    } else if entry.ams_id == super::parser::AMS_LITE_ON_A2L_PHYSICAL_ID
        && entry.slot_id < super::parser::AMS_SLOTS_PER_UNIT
    {
        AmsEntryKind::AmsLite
    } else {
        AmsEntryKind::Invalid
    }
}

/// Computes the flat `ams_mapping` channel value an `AmsMapping2Entry` corresponds to.
///
/// Inverse of `MaterialSource::flat_channel_id`, operating on the already-structured
/// `ams_id`/`slot_id` pair instead of a `MaterialSource` — keeps `ams_mapping` and
/// `ams_mapping2` from going out of sync when only the latter was supplied.
#[must_use]
pub fn flat_channel_id_for_entry(entry: &AmsMapping2Entry) -> i32 {
    match classify_mapping2_entry(entry) {
        AmsEntryKind::Standard => {
            (entry.ams_id as i32) * (super::parser::AMS_SLOTS_PER_UNIT as i32)
                + entry.slot_id as i32
        }
        AmsEntryKind::Ht => entry.ams_id as i32,
        // The flat array's encoding is per-unit-type, not uniformly "global channel id": the
        // AMS Lite puts a bare local slot 0-3 here, mirroring `MaterialSource::flat_channel_id`'s
        // `AmsLite` arm. Without this, an `ams_mapping2` entry of `{"ams_id": 16, ...}` paired
        // with a `-1` flat element described the same filament two contradictory ways in one
        // MQTT command.
        AmsEntryKind::AmsLite => entry.slot_id as i32,
        // External and unmapped slots are strictly mapped to -1, same as MaterialSource's rule.
        AmsEntryKind::External | AmsEntryKind::Invalid => -1,
    }
}

/// Largest `filament_id` the mapping builders will size an output array from.
///
/// The array length is driven by the highest filament id in the project (see the Array Length
/// Rule below), so without a ceiling a single allocation carrying a huge id — `usize::MAX`, or
/// any large value read out of an untrusted slicer project file — would size the allocation from
/// caller-supplied data. On `alloc`/`embassy` that is an OOM abort in a fixed heap, not a
/// recoverable error.
///
/// The ceiling is the physical one: 16 flat channels (4 standard units × 4 slots) plus every id
/// an AMS-HT configuration can add. H2C/H2D/H2D Pro/H2S/X2D report
/// `AmsPoolComposition::Independent { max_standard: 4, max_ht: 8 }`, so the real maximum is 24,
/// not the 20 this once assumed — the earlier value silently dropped ids 21-24 on a project
/// using all 4 standard units plus all 8 AMS-HT units. Derived from the parser's id ranges
/// rather than written as a literal so a future range change carries through here.
///
/// This is deliberately a single crate-wide cap rather than a per-model one: the mapping
/// builders take raw project allocations with no `PrinterModel` in hand, so the quirks engine
/// is not reachable from here. It bounds an allocation sized from untrusted input; a model that
/// physically has fewer channels rejects the surplus ids on its own side.
///
/// Allocations above it are dropped through the same `log::warn!` path as any other
/// out-of-range filament id.
pub(crate) const AMS_MAX_PROJECT_FILAMENTS: usize =
    (super::parser::AMS_MAX_STANDARD_ID as usize + 1) * super::parser::AMS_SLOTS_PER_UNIT as usize
        + (super::parser::AMS_HT_ID_MAX - super::parser::AMS_HT_ID_MIN + 1) as usize;

/// Builds the flat `ams_mapping` integer array from raw project allocations.
///
/// `allocations` is a slice of `(filament_id, MaterialSource)` pairs where `filament_id`
/// represents the 1-based index (1 to N) of the project material defined in the slicer.
/// Ids above the physical ceiling of `AMS_MAX_PROJECT_FILAMENTS` (16 flat channels plus the 8
/// an AMS-HT configuration adds) are dropped with a warning rather than sizing the output array.
///
/// **Array Length Rule [REF-AMS-MAP]:**
/// The length of the array is governed by the highest filament ID index present in the project,
/// rather than the total count of active spools. If a project uses filament 1 and filament 4,
/// the array must be padded to a length of 4 (elements 0 to 3), using the `-1` sentinel for
/// intermediate unused filament indexes.
#[must_use]
pub fn build_ams_mapping(allocations: &[(usize, MaterialSource)]) -> Vec<i32> {
    if allocations.is_empty() {
        return Vec::new();
    }
    let max_id = allocations
        .iter()
        .map(|(id, _)| *id)
        .max()
        .unwrap_or(1)
        .min(AMS_MAX_PROJECT_FILAMENTS);
    let mut mapping = vec![-1; max_id];

    for (id, source) in allocations {
        if *id > 0 && *id <= max_id {
            mapping[*id - 1] = source.flat_channel_id();
        } else {
            // filament_id is documented as 1-based (1 to N), so id == 0 is a caller bug. The
            // `> max_id` half is live now that max_id is capped at AMS_MAX_PROJECT_FILAMENTS:
            // it is what keeps an absurd caller-supplied id from sizing the allocation.
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
#[must_use]
pub fn build_ams_mapping2(allocations: &[(usize, MaterialSource)]) -> Vec<AmsMapping2Entry> {
    if allocations.is_empty() {
        return Vec::new();
    }
    let max_id = allocations
        .iter()
        .map(|(id, _)| *id)
        .max()
        .unwrap_or(1)
        .min(AMS_MAX_PROJECT_FILAMENTS);
    let mut mapping2 = vec![
        AmsMapping2Entry {
            ams_id: AMS_EXTERNAL_SPOOL_MAIN_ID,
            slot_id: AMS_EXTERNAL_SPOOL_MAIN_ID
        };
        max_id
    ];

    for (id, source) in allocations {
        if *id > 0 && *id <= max_id {
            mapping2[*id - 1] = source.to_mapping2_entry();
        } else {
            // Same reasoning as build_ams_mapping's else arm.
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
#[must_use]
pub fn is_external_spool_safety_valid(
    is_single_nozzle: bool,
    mapping2: &[AmsMapping2Entry],
) -> bool {
    if !is_single_nozzle {
        // Dual-nozzle systems track this polymorphically using alternate indexing rules,
        // and do not suffer from standard single-nozzle table exceptions.
        return true;
    }

    // Only a validly-ranged physical unit counts. Both external-spool IDs (254 and 255) are
    // excluded, matching is_external_spool_safety_valid_flat's uniform treatment —
    // AmsMapping2Entry's fields are public, so a caller can hand-build an entry with ams_id 254
    // (normally IDEX-only, via MaterialSource::ExternalSpoolLeft) on a single-nozzle printer.
    // Treating that as physical would dispatch use_ams:true for a non-physical channel,
    // reproducing the 07FF_8012 lockup this function exists to prevent; so would an
    // unconstrained fallthrough for garbage ids. The A2L's AMS Lite *is* physical — omitting it
    // forced use_ams off for a job fed exclusively from it, which the printer then rejects with
    // the same 07FF_8012 per [REF-AMS-USEAMS].
    mapping2.iter().any(|entry| {
        matches!(
            classify_mapping2_entry(entry),
            AmsEntryKind::Standard | AmsEntryKind::Ht | AmsEntryKind::AmsLite
        )
    })
}

/// Per-model AMS unit pool structure, confirmed against `MODEL_MATRIX.csv`'s
/// "AMS Unit Limits" row (user-supplied official Bambu documentation).
///
/// **Known limitation**: this enum still cannot express A1/A1 Mini's "shared pool OR 1 AMS
/// Lite, not combinable" exclusivity, or A2L's "shared pool + 1 AMS Lite simultaneously"
/// additive capacity. Both are conservatively modeled as `Shared { max_units: 4 }`, the same
/// as the plain shared-pool models — this may under-count A2L's true capacity by one unit, but
/// never accepts a config that's actually invalid.
///
/// This is a *capacity-counting* gap only. The addressing gap it used to describe — "AMS Lite
/// units are not independently addressable ... they use the same `ams_id` space as standard AMS
/// units" — is fixed: an A2L-attached AMS Lite reports physical unit id 16, which
/// [`crate::ams::normalize_ams_unit_id`] maps to 6 on ingest, and
/// [`MaterialSource::AmsLite`] addresses its slots with their own wire encodings.
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

/// Validates a constructed `ams_mapping2` against the model's actual AMS pool structure.
/// Rejects configs no real hardware combination could serve — e.g. 4 standard +
/// 8 AMS-HT units on a P2S, which only has independent pools of 4 and 4.
///
/// Counts *distinct* `ams_id`s used (not slot allocations) — a config referencing the same
/// unit across multiple slots isn't an extra unit. External-spool and unmapped sentinel
/// entries are ignored, since they don't occupy a physical AMS unit slot.
#[must_use]
pub fn is_ams_pool_composition_valid(
    mapping2: &[AmsMapping2Entry],
    composition: AmsPoolComposition,
) -> bool {
    let mut standard_ids = Vec::new();
    let mut ht_ids = Vec::new();
    for entry in mapping2 {
        match classify_mapping2_entry(entry) {
            // An A2L-attached AMS Lite is counted in the standard bucket rather than an additive one of
            // its own. `AmsPoolComposition` has no axis for A2L's "shared pool + 1 AMS Lite
            // simultaneously" capacity (see this enum's known-limitation note), so folding the
            // Lite into the shared count keeps the conservative stance documented there: it may
            // under-count A2L by one unit, but never accepts a config real hardware can't serve.
            // What it must not do is what it did before — hard-reject an otherwise valid
            // mapping just because it contains a legitimate id-16 entry.
            AmsEntryKind::Standard | AmsEntryKind::AmsLite => {
                if !standard_ids.contains(&entry.ams_id) {
                    standard_ids.push(entry.ams_id);
                }
            }
            AmsEntryKind::Ht => {
                if !ht_ids.contains(&entry.ams_id) {
                    ht_ids.push(entry.ams_id);
                }
            }
            // External/unmapped sentinels don't occupy a physical unit slot; anything else is a
            // malformed ams_id/slot_id pair — reject exhaustively rather than silently ignoring
            // it like a legitimate external entry.
            AmsEntryKind::External => {}
            AmsEntryKind::Invalid => return false,
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

/// Flat-array equivalent of `is_external_spool_safety_valid`, for callers using `PrintJobConfig::with_ams()` (flat `Vec<i32>`) rather than `with_ams_mapping2()`.
#[must_use]
pub fn is_external_spool_safety_valid_flat(is_single_nozzle: bool, ams_mapping: &[i32]) -> bool {
    if !is_single_nozzle {
        return true;
    }
    let max_standard_channel = (super::parser::AMS_MAX_STANDARD_ID as i32 + 1)
        * super::parser::AMS_SLOTS_PER_UNIT as i32
        - 1;
    ams_mapping.iter().any(|&v| {
        (0..=max_standard_channel).contains(&v)
            || (super::parser::AMS_HT_ID_MIN as i32..=super::parser::AMS_HT_ID_MAX as i32)
                .contains(&v)
    })
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
        let use_ams_override = is_external_spool_safety_valid(true, &mapping_all_external);
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
        let use_ams_ok = is_external_spool_safety_valid(true, &mapping_with_ams);
        assert!(use_ams_ok);
    }

    #[test]
    fn test_validate_external_spool_safety_single_nozzle_ams_id_254() {
        // ams_id 254 (AMS_EXTERNAL_SPOOL_DEPUTY_ID) is normally only produced by
        // MaterialSource::ExternalSpoolLeft on IDEX builds, but AmsMapping2Entry's fields are
        // public — a caller can hand-build one with 254 on a single-nozzle printer too. Must be
        // treated as external the same as 255, or use_ams would stay true for a non-physical
        // channel and reproduce the 07FF_8012 firmware lockup this function exists to prevent.
        let mapping_all_external = vec![AmsMapping2Entry {
            ams_id: 254,
            slot_id: 0,
        }];
        assert!(!is_external_spool_safety_valid(true, &mapping_all_external));
    }

    #[test]
    fn test_validate_external_spool_safety_with_ams_ht() {
        // AMS-HT (ams_id 128) counts as physical — use_ams stays true
        let mapping = vec![AmsMapping2Entry {
            ams_id: 128,
            slot_id: 0,
        }];
        assert!(is_external_spool_safety_valid(true, &mapping));
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
        assert!(is_ams_pool_composition_valid(
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
        assert!(!is_ams_pool_composition_valid(
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
        assert!(is_ams_pool_composition_valid(
            &valid,
            AmsPoolComposition::Independent {
                max_standard: 4,
                max_ht: 4,
            }
        ));

        let too_many_ht: Vec<AmsMapping2Entry> = (128..=135)
            .map(|ams_id| AmsMapping2Entry { ams_id, slot_id: 0 })
            .collect();
        assert!(!is_ams_pool_composition_valid(
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
        assert!(is_ams_pool_composition_valid(
            &mapping,
            AmsPoolComposition::Shared { max_units: 1 }
        ));
    }

    #[test]
    fn test_a2l_attached_ams_lite_entry_recognized_by_raw_entry_consumers() {
        // Issue #221: three functions re-derived "is this a physical AMS unit" from a raw
        // AmsMapping2Entry and each missed an A2L-attached AMS Lite's physical id 16, which only
        // MaterialSource's own methods handled. They now share classify_mapping2_entry.
        let lite = AmsMapping2Entry {
            ams_id: super::super::parser::AMS_LITE_ON_A2L_PHYSICAL_ID,
            slot_id: 1,
        };
        assert_eq!(classify_mapping2_entry(&lite), AmsEntryKind::AmsLite);

        // Flat array carries the bare local slot, matching MaterialSource::flat_channel_id's
        // AmsLite arm — not the -1 unmapped sentinel it used to emit, which contradicted the
        // ams_mapping2 entry describing the same filament in the same command.
        assert_eq!(flat_channel_id_for_entry(&lite), 1);
        assert_eq!(
            flat_channel_id_for_entry(&lite),
            MaterialSource::AmsLite { slot_id: 1 }.flat_channel_id()
        );

        // A job fed exclusively from the AMS Lite is a physical-AMS job: use_ams must stay on,
        // or the printer rejects the task with 07FF_8012 per [REF-AMS-USEAMS].
        assert!(is_external_spool_safety_valid(true, &[lite.clone()]));

        // And the pool validator must not hard-reject a mapping containing it.
        assert!(is_ams_pool_composition_valid(
            &[lite],
            AmsPoolComposition::Shared { max_units: 4 }
        ));
    }

    #[test]
    fn test_validate_ams_pool_composition_rejects_malformed_slot_id() {
        // A standard ams_id paired with an out-of-range slot_id is malformed, and is rejected
        // the same way an out-of-range ams_id is — the classifier applies one rule to both.
        let mapping = vec![AmsMapping2Entry {
            ams_id: 0,
            slot_id: 9,
        }];
        assert!(!is_ams_pool_composition_valid(
            &mapping,
            AmsPoolComposition::Shared { max_units: 4 }
        ));
    }

    #[test]
    fn test_validate_external_spool_safety_dual_nozzle_bypasses() {
        // Dual-nozzle always returns true regardless of mapping contents
        let mapping_all_external = vec![AmsMapping2Entry {
            ams_id: 255,
            slot_id: 0,
        }];
        assert!(is_external_spool_safety_valid(false, &mapping_all_external));
    }

    #[test]
    fn test_validate_external_spool_safety_flat_single_nozzle() {
        // All slots unmapped/external (`-1` sentinel) -> use_ams must override to false
        assert!(!is_external_spool_safety_valid_flat(true, &[-1, -1]));

        // At least one real physical AMS channel -> use_ams stays true
        assert!(is_external_spool_safety_valid_flat(true, &[0, -1, 1]));
    }

    #[test]
    fn test_validate_external_spool_safety_flat_dual_nozzle_bypasses() {
        // Dual-nozzle always returns true regardless of mapping contents
        assert!(is_external_spool_safety_valid_flat(false, &[-1, -1]));
    }

    #[test]
    fn test_build_ams_mapping_ams_ht() {
        let allocations = [(1, MaterialSource::AmsHt { ams_id: 128 })];
        let flat_map = build_ams_mapping(&allocations);
        assert_eq!(flat_map, vec![128]);
    }

    #[test]
    fn test_build_ams_mapping_keeps_all_24_channels_of_a_full_h2d_pool() {
        // The cap used to be 20, derived from an assumption of at most 4 AMS-HT units. H2C,
        // H2D, H2D Pro, H2S and X2D all report Independent { max_standard: 4, max_ht: 8 }, so
        // a project using every unit reaches filament id 24 and ids 21-24 were silently
        // dropped — and max_id.min(cap) shrank the array below what the project needed.
        assert_eq!(AMS_MAX_PROJECT_FILAMENTS, 24);

        let mut allocations = Vec::new();
        for ams_id in 0..=3u8 {
            for slot_id in 0..4u8 {
                allocations.push((
                    allocations.len() + 1,
                    MaterialSource::StandardAms { ams_id, slot_id },
                ));
            }
        }
        for ams_id in 128..=135u8 {
            allocations.push((allocations.len() + 1, MaterialSource::AmsHt { ams_id }));
        }

        let flat_map = build_ams_mapping(&allocations);
        assert_eq!(flat_map.len(), 24, "no filament id may be dropped");
        assert_eq!(flat_map[15], 15, "last standard flat channel");
        assert_eq!(flat_map[16], 128, "first AMS-HT id");
        assert_eq!(flat_map[23], 135, "eighth AMS-HT id");
        assert!(
            !flat_map.contains(&-1),
            "every allocation is valid, so nothing should map to the unmapped sentinel"
        );

        assert_eq!(build_ams_mapping2(&allocations).len(), 24);
    }

    #[test]
    fn test_material_source_out_of_range_rejected() {
        // MaterialSource::StandardAms/AmsHt fields are public u8s a caller can
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

        // An A2L-attached AMS Lite's two wire encodings differ from a standard unit's: the flat array
        // carries the bare local slot, `ams_mapping2` the physical unit id 16.
        for slot_id in 0..4u8 {
            let lite = MaterialSource::AmsLite { slot_id };
            assert_eq!(
                lite.flat_channel_id(),
                slot_id as i32,
                "AMS Lite's flat channel is the bare local slot, not a global ams_id*4+slot"
            );
            assert_eq!(
                lite.to_mapping2_entry(),
                AmsMapping2Entry {
                    ams_id: 16,
                    slot_id
                },
                "ams_mapping2 carries the physical unit id 16"
            );
        }
        // Out-of-range slots fall back to the same sentinels as every other source.
        let bad_lite = MaterialSource::AmsLite { slot_id: 4 };
        assert_eq!(bad_lite.flat_channel_id(), -1);
        assert_eq!(
            bad_lite.to_mapping2_entry(),
            AmsMapping2Entry {
                ams_id: 255,
                slot_id: 255
            }
        );

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
