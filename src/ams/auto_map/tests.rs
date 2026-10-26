#[cfg(not(feature = "std"))]
use alloc::string::ToString;
#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::*;

const RED: &str = "FF0000FF";
const BLUE: &str = "0000FFFF";
const WHITE_TRAY: &str = "FFFFFFFF";

fn tray(slot: u8, tray_type: &str, color: &str, info_idx: &str, remain: Option<i32>) -> AmsTray {
    AmsTray {
        id: slot.to_string(),
        tray_type: Some(tray_type.into()),
        tray_color: Some(color.into()),
        tray_info_idx: Some(info_idx.into()),
        remain,
        ..AmsTray::default()
    }
}

fn unit(id: &str, info: Option<&str>, trays: Vec<AmsTray>) -> AmsUnit {
    AmsUnit {
        id: id.into(),
        info: info.map(Into::into),
        tray: Some(trays),
        ..AmsUnit::default()
    }
}

fn report(units: Vec<AmsUnit>) -> AmsStatusReport {
    AmsStatusReport {
        ams: units,
        ..AmsStatusReport::default()
    }
}

fn filament(id: u32, filament_type: &str, color: &str, idx: &str) -> PlateFilament {
    PlateFilament {
        id,
        filament_type: filament_type.into(),
        color: color.into(),
        tray_info_idx: idx.into(),
    }
}

fn plate(filaments: Vec<PlateFilament>, maps: Option<Vec<u8>>) -> PlateFilaments {
    PlateFilaments {
        plate: 1,
        filaments,
        filament_maps: maps,
    }
}

fn standard(ams_id: u8, slot_id: u8) -> Option<MaterialSource> {
    Some(MaterialSource::StandardAms { ams_id, slot_id })
}

fn sources(result: &AmsMappingResult) -> Vec<Option<MaterialSource>> {
    result.matches.iter().map(|m| m.source).collect()
}

#[test]
fn test_color_distance_known_values() {
    let white = [255, 255, 255, 255];
    let black = [0, 0, 0, 255];
    let red = [255, 0, 0, 255];
    assert_eq!(color_distance(white, white), 0.0);
    // White is L* 100 and black L* 0, both with a and b near 0.
    assert!((color_distance(white, black) - 100.0).abs() < 0.01);
    // sRGB red is about Lab (53.24, 80.09, 67.20) against white (100, 0, 0).
    assert!((color_distance(red, white) - 114.5).abs() < 0.5);
    // Alpha alone: half opacity is 50 on the alpha axis.
    let half = [255, 255, 255, 128];
    assert!((color_distance(white, half) - 49.8).abs() < 0.1);
    // Opaque against transparent adds a full 100 to a 100 lightness gap: sqrt(100^2 + 100^2).
    assert!((color_distance(white, [0, 0, 0, 0]) - 141.42).abs() < 0.05);
}

#[test]
fn test_parse_color_forms() {
    assert_eq!(parse_color("#FF8000"), [255, 128, 0, 255]);
    assert_eq!(parse_color("FF800080"), [255, 128, 0, 128]);
    assert_eq!(parse_color("#FF800080"), [255, 128, 0, 128]);
    for bad in ["", "#FFF", "ZZZZZZ", "+FFFFF", "12345678901"] {
        assert_eq!(parse_color(bad), WHITE, "{bad:?}");
    }
}

#[test]
fn test_tray_filament_type_support_spellings() {
    assert_eq!(tray_filament_type("Sup.PLA", ""), "PLA-S");
    assert_eq!(tray_filament_type("Support W", ""), "PLA-S");
    assert_eq!(tray_filament_type("Sup.PA", ""), "PA-S");
    assert_eq!(tray_filament_type("Support G", ""), "PA-S");
    assert_eq!(tray_filament_type("Sup.ABS", ""), "ABS-S");
    assert_eq!(tray_filament_type("Support", "GFS01"), "PA-S");
    assert_eq!(tray_filament_type("Support", "GFS00"), "PLA-S");
    assert_eq!(tray_filament_type("Support", "other"), "PLA-S");
    assert_eq!(tray_filament_type("PETG", "GFG00"), "PETG");
}

#[test]
fn test_type_mismatch_is_never_picked() {
    let ams = report(vec![unit(
        "0",
        None,
        vec![tray(0, "PETG", RED, "GFG00", None)],
    )]);
    let file = plate(vec![filament(1, "PLA", "#FF0000", "GFA00")], None);
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(sources(&result), [None]);
    assert!(!result.is_complete());
    assert_eq!(result.unmatched().count(), 1);
    assert_eq!(result.mapping2, [AmsMapping2Entry::UNMAPPED]);
}

#[test]
fn test_class_match_beats_colour() {
    // Slot 0 is exactly red but another class; slot 1 is blue with the right class.
    let ams = report(vec![unit(
        "0",
        None,
        vec![
            tray(0, "PLA", RED, "GFA01", None),
            tray(1, "PLA", BLUE, "GFA00", None),
        ],
    )]);
    let file = plate(vec![filament(1, "PLA", "#FF0000", "GFA00")], None);
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(sources(&result), [standard(0, 1)]);
    let distance = result.matches[0].distance.unwrap();
    assert!((10_000.0..11_000.0).contains(&distance), "{distance}");
}

#[test]
fn test_colour_decides_within_a_class_and_without_one() {
    let ams = report(vec![unit(
        "0",
        None,
        vec![
            tray(0, "PLA", BLUE, "GFA00", None),
            tray(1, "PLA", RED, "GFA00", None),
        ],
    )]);
    let file = plate(vec![filament(1, "PLA", "#FF0000", "GFA00")], None);
    assert_eq!(
        sources(&derive_ams_mapping(&file, Some(&ams), &[], false)),
        [standard(0, 1)]
    );
    // No class id on the file: both trays are 20000 plus colour, and colour decides.
    let file = plate(vec![filament(1, "PLA", "#FF0000", "")], None);
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(sources(&result), [standard(0, 1)]);
    assert!(result.matches[0].distance.unwrap() >= 20_000.0);
}

#[test]
fn test_exact_ties_take_the_lowest_tray_without_a_class() {
    let ams = report(vec![unit(
        "0",
        None,
        vec![
            tray(0, "PLA", RED, "", Some(80)),
            tray(1, "PLA", RED, "", Some(20)),
        ],
    )]);
    // Remaining only breaks a tie between exact class matches; the empty ids here aren't one.
    let file = plate(vec![filament(1, "PLA", "#FF0000", "")], None);
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(sources(&result), [standard(0, 0)]);
}

#[test]
fn test_remain_tie_break_prefers_less_filament() {
    let trays = |remains: [Option<i32>; 3]| {
        report(vec![unit(
            "0",
            None,
            remains
                .iter()
                .enumerate()
                .map(|(slot, remain)| tray(slot as u8, "PLA", RED, "GFA00", *remain))
                .collect(),
        )])
    };
    let file = plate(vec![filament(1, "PLA", "#FF0000", "GFA00")], None);
    let pick = |ams: &AmsStatusReport| sources(&derive_ams_mapping(&file, Some(ams), &[], false));
    // Less filament left wins among reporting trays.
    assert_eq!(
        pick(&trays([Some(80), Some(20), Some(50)])),
        [standard(0, 1)]
    );
    // Unknown or empty remain keeps the lowest slot first...
    assert_eq!(pick(&trays([None, Some(-1), Some(0)])), [standard(0, 0)]);
    // ...but a tray that reports some beats one that reports none.
    assert_eq!(
        pick(&trays([Some(-1), Some(40), Some(0)])),
        [standard(0, 1)]
    );
    // A tray reporting 0 is never deliberately preferred.
    assert_eq!(
        pick(&trays([Some(30), Some(0), Some(60)])),
        [standard(0, 0)]
    );
}

/// A candidate for the internal matcher.
fn candidate_for(
    source: MaterialSource,
    color: [u8; 4],
    filament_id: &str,
    setting_id: &str,
) -> Candidate {
    Candidate {
        order: 0,
        source,
        filament_type: "PLA".into(),
        color,
        filament_id: filament_id.into(),
        setting_id: setting_id.into(),
        remain: -1,
    }
}

fn wanted<'a>(id: u32, color: [u8; 4], filament_id: &'a str, setting_id: &'a str) -> Wanted<'a> {
    Wanted {
        id,
        filament_type: "PLA",
        color,
        filament_id,
        setting_id,
    }
}

#[test]
fn test_used_tray_is_reused_only_on_an_exact_match() {
    let red = [255, 0, 0, 255];
    let blue = [0, 0, 255, 255];
    let slot = |n| MaterialSource::StandardAms {
        ams_id: 0,
        slot_id: n,
    };
    let trays = [
        candidate_for(slot(0), red, "GFA00", "PRESET"),
        candidate_for(slot(1), blue, "GFA00", "OTHER"),
    ];

    // Both filaments name tray 0's preset and colour: the second reuses it (distance 0) rather
    // than taking the unused, worse tray 1.
    let a = wanted(1, red, "GFA00", "PRESET");
    let b = wanted(2, red, "GFA00", "PRESET");
    let picks = match_group(&[&a, &b], &trays);
    assert_eq!(picks[0].map(|p| p.0), Some(0));
    assert_eq!(picks[1].map(|p| p.0), Some(0));

    // Without a preset match the second takes the unused tray instead.
    let a = wanted(1, red, "GFA00", "");
    let b = wanted(2, red, "GFA00", "");
    let picks = match_group(&[&a, &b], &trays);
    assert_eq!(picks[0].map(|p| p.0), Some(0));
    assert_eq!(picks[1].map(|p| p.0), Some(1));
}

#[test]
fn test_used_tray_retry_when_nothing_unused_matches() {
    // One PLA tray, two PLA filaments: the second reuses it at its class-plus-colour distance.
    let ams = report(vec![unit(
        "0",
        None,
        vec![tray(2, "PLA", RED, "GFA00", None)],
    )]);
    let file = plate(
        vec![
            filament(1, "PLA", "#FF0000", "GFA00"),
            filament(2, "PLA", "#00FF00", "GFA00"),
            filament(3, "PETG", "#00FF00", "GFG00"),
        ],
        None,
    );
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(sources(&result), [standard(0, 2), standard(0, 2), None]);
    assert_eq!(
        result
            .unmatched()
            .map(|m| m.filament_id)
            .collect::<Vec<_>>(),
        [3]
    );
}

#[test]
fn test_greedy_rounds_take_the_closest_pair_first() {
    // Filament 1 is nearly white and filament 2 exactly white; one white-ish tray and one grey.
    // Round 1 gives the exact pair its tray, so filament 1 gets the other.
    let ams = report(vec![unit(
        "0",
        None,
        vec![
            tray(0, "PLA", WHITE_TRAY, "GFA00", None),
            tray(1, "PLA", "808080FF", "GFA00", None),
        ],
    )]);
    let file = plate(
        vec![
            filament(1, "PLA", "#F0F0F0", "GFA00"),
            filament(2, "PLA", "#FFFFFF", "GFA00"),
        ],
        None,
    );
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(sources(&result), [standard(0, 1), standard(0, 0)]);
}

#[test]
fn test_mapping2_is_sized_by_the_highest_id_with_gaps_unmapped() {
    let ams = report(vec![unit(
        "1",
        None,
        vec![tray(3, "PLA", RED, "GFA00", None)],
    )]);
    let file = plate(
        vec![
            filament(1, "PLA", "#FF0000", "GFA00"),
            filament(4, "ABS", "#FF0000", "GFB00"),
        ],
        None,
    );
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(result.mapping2.len(), 4);
    assert_eq!(
        result.mapping2[0],
        AmsMapping2Entry {
            ams_id: 1,
            slot_id: 3
        }
    );
    assert_eq!(result.mapping2[1], AmsMapping2Entry::UNMAPPED);
    assert_eq!(result.mapping2[2], AmsMapping2Entry::UNMAPPED);
    // The last filament matched nothing but still sizes the array.
    assert_eq!(result.mapping2[3], AmsMapping2Entry::UNMAPPED);
}

#[test]
fn test_no_ams_leaves_everything_unmatched() {
    let file = plate(vec![filament(2, "PLA", "#FF0000", "GFA00")], None);
    let result = derive_ams_mapping(&file, None, &[], false);
    assert_eq!(sources(&result), [None]);
    assert_eq!(result.mapping2, [AmsMapping2Entry::UNMAPPED; 2]);
    let empty = derive_ams_mapping(&plate(Vec::new(), None), None, &[], false);
    assert!(empty.mapping2.is_empty() && empty.is_complete());
}

#[test]
fn test_ids_outside_the_project_range_are_left_out() {
    let ams = report(vec![unit(
        "0",
        None,
        vec![tray(0, "PLA", RED, "GFA00", None)],
    )]);
    let file = plate(
        vec![
            filament(0, "PLA", "#FF0000", "GFA00"),
            filament(33, "PLA", "#FF0000", "GFA00"),
            filament(1, "PLA", "#FF0000", "GFA00"),
        ],
        None,
    );
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(sources(&result), [None, None, standard(0, 0)]);
    assert_eq!(result.mapping2.len(), 1);
}

#[test]
fn test_empty_trays_and_single_nozzle_external_spool_are_not_candidates() {
    let mut emptied = tray(0, "PLA", RED, "GFA00", None);
    emptied.state = Some(9);
    let ams = report(vec![unit("0", None, vec![emptied])]);
    let spool = tray(0, "PLA", RED, "GFA00", None);
    let external = [VirtualTray {
        id: "255".into(),
        ..spool
    }];
    let file = plate(vec![filament(1, "PLA", "#FF0000", "GFA00")], None);
    // The only AMS tray is emptied, and a single-nozzle printer never maps to the spool.
    let result = derive_ams_mapping(&file, Some(&ams), &external, false);
    assert_eq!(sources(&result), [None]);
}

#[test]
fn test_ams_ht_and_ams_lite_on_an_a2l_use_their_own_ids() {
    let ams = report(vec![
        unit("128", None, vec![tray(0, "PA-CF", RED, "GFN03", None)]),
        // Telemetry normalizes the A2L's AMS Lite from physical 16 to 6.
        unit("6", None, vec![tray(2, "PLA", BLUE, "GFA00", None)]),
    ]);
    let file = plate(
        vec![
            filament(1, "PA-CF", "#FF0000", "GFN03"),
            filament(2, "PLA", "#0000FF", "GFA00"),
        ],
        None,
    );
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(
        sources(&result),
        [
            Some(MaterialSource::AmsHt { ams_id: 128 }),
            Some(MaterialSource::AmsLite { slot_id: 2 })
        ]
    );
    // The physical 16 goes back on the wire, with the local slot.
    assert_eq!(
        result.mapping2,
        [
            AmsMapping2Entry {
                ams_id: 128,
                slot_id: 0
            },
            AmsMapping2Entry {
                ams_id: 16,
                slot_id: 2
            }
        ]
    );
    // Unit 16 as a caller might hand-build it is the same unit.
    let raw16 = report(vec![unit(
        "16",
        None,
        vec![tray(1, "PLA", BLUE, "GFA00", None)],
    )]);
    let result = derive_ams_mapping(&file, Some(&raw16), &[], false);
    assert_eq!(
        result.matches[1].source,
        Some(MaterialSource::AmsLite { slot_id: 1 })
    );
}

#[test]
fn test_dual_nozzle_filaments_use_only_their_extruders_units() {
    // Unit 0 is bound to the deputy (info extruder 1), unit 1 to the main (info extruder 0).
    // Both hold an identical PLA tray; the filaments' extruder decides which one each gets.
    let ams = report(vec![
        unit("0", Some("103"), vec![tray(0, "PLA", RED, "GFA00", None)]),
        unit("1", Some("003"), vec![tray(0, "PLA", RED, "GFA00", None)]),
    ]);
    let file = plate(
        vec![
            filament(1, "PLA", "#FF0000", "GFA00"),
            filament(2, "PLA", "#FF0000", "GFA00"),
        ],
        Some(vec![2, 1]),
    );
    let result = derive_ams_mapping(&file, Some(&ams), &[], true);
    assert_eq!(sources(&result), [standard(1, 0), standard(0, 0)]);
}

#[test]
fn test_dual_nozzle_external_spool_only_when_no_unit_is_bound() {
    let ext = |id: &str| VirtualTray {
        id: id.into(),
        ..tray(0, "PLA", RED, "GFA00", None)
    };
    let external = [ext("254"), ext("255")];
    let file = plate(
        vec![
            filament(1, "PLA", "#FF0000", "GFA00"),
            filament(2, "PLA", "#FF0000", "GFA00"),
        ],
        Some(vec![1, 2]),
    );
    // A unit with a loaded PLA tray on the main extruder only: the deputy has no unit, so it
    // takes its external spool; the main side takes the unit and never the spool.
    let ams = report(vec![unit(
        "0",
        Some("003"),
        vec![tray(1, "PLA", BLUE, "GFA00", None)],
    )]);
    let result = derive_ams_mapping(&file, Some(&ams), &external, true);
    assert_eq!(
        sources(&result),
        [Some(MaterialSource::ExternalSpoolLeft), standard(0, 1)]
    );
    assert_eq!(
        result.mapping2[0],
        AmsMapping2Entry {
            ams_id: 254,
            slot_id: 0
        }
    );
    // No AMS at all: each side takes its own external spool.
    let result = derive_ams_mapping(&file, None, &external, true);
    assert_eq!(
        sources(&result),
        [
            Some(MaterialSource::ExternalSpoolLeft),
            Some(MaterialSource::ExternalSpoolRight)
        ]
    );
}

#[test]
fn test_dual_nozzle_filament_switch_unit_reaches_both_extruders() {
    // `E03`: type 3, extruder 0xE; `1000E03` adds a bound switch inlet (bits 24-27).
    let ams = report(vec![
        unit(
            "0",
            Some("1000E03"),
            vec![tray(0, "PLA", RED, "GFA00", None)],
        ),
        // 0xE without an inlet is unbound, which BambuStudio treats as reaching neither.
        unit("1", Some("E03"), vec![tray(0, "PLA", RED, "GFA00", None)]),
    ]);
    let file = plate(
        vec![
            filament(1, "PLA", "#FF0000", "GFA00"),
            filament(2, "PLA", "#FF0000", "GFA00"),
        ],
        Some(vec![1, 2]),
    );
    let result = derive_ams_mapping(&file, Some(&ams), &[], true);
    // Both sides reach unit 0, and the sides don't see each other's picks.
    assert_eq!(sources(&result), [standard(0, 0), standard(0, 0)]);
}

#[test]
fn test_dual_nozzle_without_filament_maps_leaves_filaments_unmatched() {
    let ams = report(vec![unit(
        "0",
        Some("003"),
        vec![tray(0, "PLA", RED, "GFA00", None)],
    )]);
    let file = plate(vec![filament(1, "PLA", "#FF0000", "GFA00")], None);
    let result = derive_ams_mapping(&file, Some(&ams), &[], true);
    assert_eq!(sources(&result), [None]);
    // A value that is neither extruder is left out as well.
    let file = plate(vec![filament(1, "PLA", "#FF0000", "GFA00")], Some(vec![3]));
    assert_eq!(
        sources(&derive_ams_mapping(&file, Some(&ams), &[], true)),
        [None]
    );
}

#[test]
fn test_single_nozzle_ignores_the_units_extruder_binding() {
    // A unit reporting deputy on a single-nozzle printer is still the printer's AMS.
    let ams = report(vec![unit(
        "0",
        Some("103"),
        vec![tray(0, "PLA", RED, "GFA00", None)],
    )]);
    let file = plate(vec![filament(1, "PLA", "#FF0000", "GFA00")], Some(vec![2]));
    let result = derive_ams_mapping(&file, Some(&ams), &[], false);
    assert_eq!(sources(&result), [standard(0, 0)]);
}
