use super::*;

#[test]
fn test_nozzle_info_standard_keys() {
    let json_data = r#"{
            "device": {
                "nozzle": {
                    "info": [{
                        "id": 0,
                        "diameter": 0.4,
                        "tm": 300,
                        "type": "hardened_steel",
                        "sn": "SN123",
                        "color_m": "FF0000",
                        "fila_id": "GFA01"
                    }]
                }
            }
        }"#;
    let nozzle = &serde_json::from_str::<TelemetryReport>(json_data)
        .unwrap()
        .device
        .unwrap()
        .nozzle
        .unwrap()
        .info
        .unwrap()[0];
    assert_eq!(nozzle.id, 0);
    assert_eq!(nozzle.tm, Some(300));
    assert_eq!(nozzle.nozzle_type.as_deref(), Some("hardened_steel"));
    assert_eq!(nozzle.sn.as_deref(), Some("SN123"));
}

#[test]
fn test_nozzle_info_idex_keys() {
    let json_data = r#"{
            "device": {
                "nozzle": {
                    "info": [{
                        "id": 1,
                        "diameter": 0.6,
                        "max_temp": 350,
                        "type": "stainless_steel",
                        "serial_number": "IDEX-SN-456",
                        "filament_colour": "00FF00",
                        "filament_id": "GFB02"
                    }]
                }
            }
        }"#;
    let nozzle = &serde_json::from_str::<TelemetryReport>(json_data)
        .unwrap()
        .device
        .unwrap()
        .nozzle
        .unwrap()
        .info
        .unwrap()[0];
    assert_eq!(nozzle.id, 1);
    assert_eq!(nozzle.max_temp, Some(350));
    assert_eq!(nozzle.serial_number.as_deref(), Some("IDEX-SN-456"));
    assert_eq!(nozzle.filament_colour.as_deref(), Some("00FF00"));
}

#[test]
fn test_extruder_info_h2d_mock() {
    let json_data = r#"{
            "device": {
                "extruder": {
                    "info": [
                        {
                            "filam_bak": [48],
                            "hnow": 0, "hpre": 0, "htar": 0,
                            "id": 0,
                            "info": 79,
                            "snow": 259, "spre": 259, "star": 259,
                            "stat": 197376,
                            "temp": 16056565
                        },
                        {
                            "filam_bak": [10],
                            "hnow": 1, "hpre": 1, "htar": 1,
                            "id": 1,
                            "info": 8,
                            "snow": 65279, "spre": 65279, "star": 65279,
                            "stat": 0,
                            "temp": 47
                        }
                    ],
                    "state": 2
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let extruder = report.device.unwrap().extruder.unwrap();
    let info = extruder.info.as_ref().unwrap();
    assert_eq!(info.len(), 2);
    assert_eq!(extruder.extruder_count(), 2);
    assert_eq!(extruder.active_extruder_index(), 0);

    // id 0 (right/main): temp 16056565 = 0x00F500F5 → composite packed
    let right = &info[0];
    assert_eq!(right.id, 0);
    let (right_actual, right_target) = right.temperatures();
    assert_eq!(right_actual, 245);
    assert_eq!(right_target, 245);
    assert_eq!(right.filam_bak, vec![48]);
    assert_eq!(right.stat, Some(197376));

    // id 1 (left/deputy): temp 47 → direct (≤ 500)
    let left = &info[1];
    assert_eq!(left.id, 1);
    let (left_actual, left_target) = left.temperatures();
    assert_eq!(left_actual, 47);
    assert_eq!(left_target, 0);
}

#[test]
fn test_extruder_info_x2d_mock() {
    let json_data = r#"{
            "device": {
                "extruder": {
                    "info": [
                        {
                            "filam_bak": [],
                            "hnow": 0, "hpre": 0, "htar": 0,
                            "id": 0,
                            "info": 1176,
                            "snow": 65535, "spre": 65535, "star": 65535,
                            "stat": 0,
                            "temp": 50,
                            "z_bias": 0.0
                        },
                        {
                            "filam_bak": [],
                            "hnow": 1, "hpre": 1, "htar": 1,
                            "id": 1,
                            "info": 1102,
                            "snow": 1, "spre": 1, "star": 1,
                            "stat": 197376,
                            "temp": 16384250,
                            "z_bias": 0.0
                        }
                    ],
                    "state": 33042
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let extruder = report.device.unwrap().extruder.unwrap();
    let info = extruder.info.as_ref().unwrap();
    assert_eq!(info.len(), 2);

    // state 33042: low 4 bits = 2 (count), bits 4-7 = 1 (active = left)
    assert_eq!(extruder.extruder_count(), 2);
    assert_eq!(extruder.active_extruder_index(), 1);

    // id 0: temp 50 (direct, ≤ 500)
    let right = &info[0];
    let (right_actual, right_target) = right.temperatures();
    assert_eq!(right_actual, 50);
    assert_eq!(right_target, 0);
    assert_eq!(right.z_bias, Some(0.0));

    // id 1: temp 16384250 (composite packed, > 500)
    // 16384250 = 0xFA00FA → target = 250, actual = 250
    let left = &info[1];
    let (left_actual, left_target) = left.temperatures();
    assert_eq!(left_target, 250);
    assert_eq!(left_actual, 250);

    // id 0's snow/spre/star are all the unmapped sentinel 0xFFFF (65535) — this
    // extruder isn't routed to any AMS slot.
    assert_eq!(right.current_ams_slot(), None);
    assert_eq!(right.previous_ams_slot(), None);
    assert_eq!(right.target_ams_slot(), None);

    // id 1's snow/spre/star are all 1: slot_id=1 (low byte), ams_id=0 (high byte).
    assert_eq!(left.current_ams_slot(), Some((0, 1)));
    assert_eq!(left.previous_ams_slot(), Some((0, 1)));
    assert_eq!(left.target_ams_slot(), Some((0, 1)));
}

#[test]
fn test_extruder_absent_on_single_nozzle() {
    let json_data = r#"{
            "device": {
                "nozzle": {
                    "info": [{ "id": 0, "diameter": 0.4 }]
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    assert!(report.device.unwrap().extruder.is_none());
}

#[test]
fn test_legacy_nozzle_fields() {
    let json = r#"{
            "print": {
                "nozzle_type": "stainless_steel",
                "nozzle_diameter": "0.4"
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.nozzle_type.as_deref(), Some("stainless_steel"));
    assert_eq!(print.nozzle_diameter.as_deref(), Some("0.4"));
}

#[test]
fn test_nozzle_collection_extra_fields() {
    let json = r#"{
            "device": {
                "nozzle": {
                    "info": [{ "id": 0, "diameter": 0.4 }],
                    "exist": 3,
                    "state": 1,
                    "src_id": 0,
                    "tar_id": 1
                }
            }
        }"#;
    let nozzle = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .device
        .unwrap()
        .nozzle
        .unwrap();
    assert_eq!(nozzle.exist, Some(3));
    assert_eq!(nozzle.state, Some(1));
    assert_eq!(nozzle.src_id, Some(0));
    assert_eq!(nozzle.tar_id, Some(1));
}

#[test]
fn test_nozzle_info_stat_field() {
    let json = r#"{
            "device": {
                "nozzle": {
                    "info": [{ "id": 0, "diameter": 0.4, "stat": 256 }]
                }
            }
        }"#;
    let nozzle = &serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .device
        .unwrap()
        .nozzle
        .unwrap()
        .info
        .unwrap()[0];
    assert_eq!(nozzle.stat, Some(256));
}

#[test]
fn test_decode_nozzle_temperatures_composite_extruder_path() {
    // 4587590 = (70 << 16) | 70, same composite packing as bed/chamber.
    let json = r#"{
            "device": {
                "extruder": {
                    "info": [
                        { "id": 0, "temp": 4587590 },
                        { "id": 1, "temp": 3211296 }
                    ],
                    "state": 18
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    let temps = decode_nozzle_temperatures(report.device(), None, None);
    // 3211296 = (49 << 16) | 32 -> actual=32, target=49
    assert_eq!(temps, vec![(0, 70, 70), (1, 32, 49)]);
}

#[test]
fn test_decode_nozzle_temperatures_single_nozzle_flat_fallback() {
    // No device.extruder and only one nozzle entry -> single (0, actual, target) tuple from
    // the flat print-level fields.
    let json = r#"{
            "device": {
                "nozzle": { "info": [{ "id": 0 }] }
            },
            "print": {
                "nozzle_temper": 210.0,
                "nozzle_target_temper": 220.0
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    let temps = decode_nozzle_temperatures(
        report.device(),
        report.print.as_ref().unwrap().nozzle_temper,
        report.print.as_ref().unwrap().nozzle_target_temper,
    );
    assert_eq!(temps, vec![(0, 210, 220)]);
}

#[test]
fn test_decode_nozzle_temperatures_flat_fallback_above_composite_threshold() {
    // Same guard as the bed-side test: the flat `nozzle_temper`/`nozzle_target_temper` fields
    // are a direct `as u16` cast, never `unpack_temperature`. Every other flat-fallback test
    // stays <= 500, so a change routing these through the composite unpacker would silently
    // decode 600.0 as 0/0 (`src/types/telemetry/CLAUDE.md`).
    let json = r#"{
            "device": {
                "nozzle": { "info": [{ "id": 0 }] }
            },
            "print": {
                "nozzle_temper": 600.0,
                "nozzle_target_temper": 600.0
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    let temps = decode_nozzle_temperatures(
        report.device(),
        report.print.as_ref().unwrap().nozzle_temper,
        report.print.as_ref().unwrap().nozzle_target_temper,
    );
    assert_eq!(temps, vec![(0, 600, 600)]);
}

#[test]
fn test_decode_nozzle_temperatures_idex_swapped_fallback() {
    // IDEX model (2 nozzle.info entries) with no live device.extruder telemetry yet — the
    // undocumented wire routing quirk: nozzle_temper is nozzle 1's actual, and
    // nozzle_target_temper is nozzle 0's target, each nozzle getting only half of its own
    // reading from the flat fields.
    let json = r#"{
            "device": {
                "nozzle": { "info": [{ "id": 0 }, { "id": 1 }] }
            },
            "print": {
                "nozzle_temper": 210.0,
                "nozzle_target_temper": 220.0
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    let temps = decode_nozzle_temperatures(
        report.device(),
        report.print.as_ref().unwrap().nozzle_temper,
        report.print.as_ref().unwrap().nozzle_target_temper,
    );
    assert_eq!(temps, vec![(0, 0, 220), (1, 210, 0)]);
}

#[test]
fn test_decode_nozzle_temperatures_h2c_rack_nozzle_not_misclassified_as_idex() {
    // A single installed nozzle (id 0) plus a rack-stored spare (id 16 — high
    // nibble 1 flags rack storage per BambuStudio's get_hex_bits(id, 1) == 1) must not be
    // misclassified as IDEX — H2C has one hotend and a spare-nozzle rack.
    let json = r#"{
            "device": {
                "nozzle": { "info": [{ "id": 0 }, { "id": 16 }] }
            },
            "print": {
                "nozzle_temper": 210.0,
                "nozzle_target_temper": 220.0
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    let temps = decode_nozzle_temperatures(
        report.device(),
        report.print.as_ref().unwrap().nozzle_temper,
        report.print.as_ref().unwrap().nozzle_target_temper,
    );
    assert_eq!(temps, vec![(0, 210, 220)], "must resolve as single-nozzle, not IDEX");
}
