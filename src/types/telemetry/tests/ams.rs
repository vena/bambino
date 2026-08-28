use super::*;

#[test]
fn test_ams_nested_wire_format() {
    let json_data = r#"{
            "print": {
                "ams": {
                    "ams": [
                        {
                            "id": "0",
                            "temp": "26.0",
                            "humidity": "3",
                            "tray": [
                                { "id": "0", "state": 10, "tray_type": "PLA", "tray_color": "FF0000FF", "remain": 85 },
                                { "id": "1", "state": 11, "tray_type": "PETG", "tray_color": "0000FFFF", "remain": 42 },
                                { "id": "2" },
                                { "id": "3", "state": 10, "tray_type": "PLA", "tray_color": "FFFFFFFF", "remain": 100 }
                            ]
                        }
                    ],
                    "ams_exist_bits": "1",
                    "tray_exist_bits": "b",
                    "tray_now": "1",
                    "version": 0
                }
            }
        }"#;

    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let print = report.print.unwrap();
    let ams_status = print.ams.unwrap();

    assert_eq!(ams_status.ams_exist_bits.as_deref(), Some("1"));
    assert_eq!(ams_status.tray_exist_bits.as_deref(), Some("b"));
    assert_eq!(ams_status.tray_now.as_deref(), Some("1"));
    assert_eq!(ams_status.ams.len(), 1);

    let unit = &ams_status.ams[0];
    assert_eq!(unit.id, "0");
    assert_eq!(unit.temp, "26.0");
    assert_eq!(unit.humidity, "3");
    let unit_tray = unit.tray.as_ref().unwrap();
    assert_eq!(unit_tray.len(), 4);

    assert_eq!(unit_tray[0].tray_type.as_deref(), Some("PLA"));
    assert_eq!(unit_tray[0].state, Some(10));
    assert_eq!(unit_tray[1].state, Some(11));
    assert_eq!(unit_tray[1].tray_type.as_deref(), Some("PETG"));
    // Slot 2: empty (truncated JSON — P1S firmware behavior)
    assert_eq!(unit_tray[2].state, None);
    assert_eq!(unit_tray[2].state(), 9);
}

#[test]
fn test_ams_drying_fields() {
    let json_data = r#"{
            "print": {
                "ams": {
                    "ams": [
                        {
                            "id": "0",
                            "temp": "55.0",
                            "humidity": "1",
                            "humidity_raw": "8",
                            "dry_time": 142,
                            "dry_setting": {
                                "dry_temperature": 55,
                                "dry_duration": 480,
                                "dry_filament": "PA-CF"
                            },
                            "tray": []
                        }
                    ]
                }
            }
        }"#;

    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let unit = &report.print.unwrap().ams.unwrap().ams[0];
    assert_eq!(unit.dry_time, Some(142));
    assert_eq!(unit.humidity_raw.as_deref(), Some("8"));
    let dry = unit.dry_setting.as_ref().unwrap();
    assert_eq!(dry.dry_temperature, Some(55));
    assert_eq!(dry.dry_duration, Some(480));
    assert_eq!(dry.dry_filament.as_deref(), Some("PA-CF"));
}

#[test]
fn test_virtual_tray_deserialization() {
    let json_data = r#"{
            "print": {
                "vt_tray": {
                    "id": "254",
                    "tray_type": "PLA",
                    "tray_color": "FF0000FF",
                    "nozzle_temp_max": "220",
                    "nozzle_temp_min": "190",
                    "tray_diameter": "1.75",
                    "remain": 85,
                    "k": 0.02,
                    "n": 1,
                    "cali_idx": -1,
                    "tag_uid": "0000000000000000",
                    "tray_uuid": "00000000000000000000000000000000"
                }
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_data)
        .unwrap()
        .print
        .unwrap();
    let vt = print.vt_tray.unwrap();
    assert_eq!(vt.id.as_deref(), Some("254"));
    assert_eq!(vt.tray_type.as_deref(), Some("PLA"));
    assert_eq!(vt.tray_color.as_deref(), Some("FF0000FF"));
    assert_eq!(vt.nozzle_temp_max.as_deref(), Some("220"));
    assert_eq!(vt.remain, Some(85));
    assert_eq!(vt.cali_idx, Some(-1));
}

#[test]
fn test_virtual_tray_empty() {
    let json_data = r#"{
            "print": {
                "vt_tray": {
                    "id": "254",
                    "tray_type": "",
                    "tray_color": "FFFFFF00",
                    "remain": 0
                }
            }
        }"#;
    let vt = serde_json::from_str::<TelemetryReport>(json_data)
        .unwrap()
        .print
        .unwrap()
        .vt_tray
        .unwrap();
    assert_eq!(vt.tray_type.as_deref(), Some(""));
    assert_eq!(vt.remain, Some(0));
}

#[test]
fn test_hw_switch_state_and_ams_status() {
    let json = r#"{
            "print": {
                "hw_switch_state": 1,
                "ams_status": 768,
                "s_obj": [2, 5, 7]
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.hw_switch_state, Some(1));
    assert_eq!(print.ams_status, Some(768));
    assert_eq!(print.s_obj, Some(vec![2, 5, 7]));
}

#[test]
fn test_vir_slot_deserialization() {
    let json = r#"{
            "print": {
                "vir_slot": [
                    {
                        "id": "254",
                        "tray_type": "PLA",
                        "tray_color": "76D9F4FF",
                        "remain": 0,
                        "cali_idx": -1,
                        "k": 0.02
                    },
                    {
                        "id": "255",
                        "tray_type": "PETG",
                        "tray_color": "FF0000FF",
                        "remain": 50
                    }
                ]
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    let slots = print.vir_slot.unwrap();
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].id.as_deref(), Some("254"));
    assert_eq!(slots[0].tray_type.as_deref(), Some("PLA"));
    assert_eq!(slots[1].id.as_deref(), Some("255"));
    assert_eq!(slots[1].tray_type.as_deref(), Some("PETG"));
    assert_eq!(slots[1].remain, Some(50));
}

#[test]
fn test_ams_status_report_extra_fields() {
    let json = r#"{
            "print": {
                "ams": {
                    "ams": [],
                    "tray_read_done_bits": "ff",
                    "tray_reading_bits": "0",
                    "tray_tar": "3",
                    "insert_flag": true,
                    "power_on_flag": false,
                    "cali_id": 1,
                    "cali_stat": 0
                }
            }
        }"#;
    let ams = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap()
        .ams
        .unwrap();
    assert_eq!(ams.tray_read_done_bits.as_deref(), Some("ff"));
    assert_eq!(ams.tray_reading_bits.as_deref(), Some("0"));
    assert_eq!(ams.tray_tar.as_deref(), Some("3"));
    assert_eq!(ams.insert_flag, Some(true));
    assert_eq!(ams.power_on_flag, Some(false));
    assert_eq!(ams.cali_id, Some(1));
    assert_eq!(ams.cali_stat, Some(0));
}

#[test]
fn test_ams_status_report_calibrate_remain_flag_and_cfs() {
    // calibrate_remain_flag (bool) and cfs (typed Vec<AmsFilamentStep>).
    let json = r#"{
            "print": {
                "ams": {
                    "ams": [],
                    "calibrate_remain_flag": true,
                    "cfs": [2, 9, 5, 7]
                }
            }
        }"#;
    let ams = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap()
        .ams
        .unwrap();
    assert_eq!(ams.calibrate_remain_flag, Some(true));
    assert_eq!(
        ams.cfs,
        Some(vec![
            AmsFilamentStep::HeatNozzle,
            AmsFilamentStep::SwitchExtruder,
            AmsFilamentStep::PushNewFilament,
            AmsFilamentStep::PurgeOldFilament,
        ])
    );
}

#[test]
fn test_ams_filament_step_unknown_value_preserved() {
    let json = r#"{
            "print": {
                "ams": { "ams": [], "cfs": [99] }
            }
        }"#;
    let ams = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap()
        .ams
        .unwrap();
    assert_eq!(ams.cfs, Some(vec![AmsFilamentStep::Unknown(99)]));
}

#[test]
fn test_ams_unit_dry_fan_status() {
    // Bits 18-19 = dry_fan1_status, bits 20-21 = dry_fan2_status.
    // "3c0000" = 0b0011_1100 at bits 16-23: fan1 (bits18-19) = 0b11 = 3, fan2 (bits20-21) = 0b11 = 3.
    let unit = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: Some("3c0000".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.dry_fan1_status(), Some(3));
    assert_eq!(unit.dry_fan2_status(), Some(3));

    let unit_off = AmsUnit {
        info: Some("0".into()),
        ..unit
    };
    assert_eq!(unit_off.dry_fan1_status(), Some(0));
    assert_eq!(unit_off.dry_fan2_status(), Some(0));
}

#[test]
fn test_ams_unit_info_bitmask() {
    let json = r#"{
            "print": {
                "ams": {
                    "ams": [
                        {
                            "id": "0",
                            "temp": "26.0",
                            "humidity": "3",
                            "info": "11002103",
                            "dry_sf_reason": [0, 0, 0, 0],
                            "tray": []
                        }
                    ]
                }
            }
        }"#;
    let unit = &serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap()
        .ams
        .unwrap()
        .ams[0];
    assert_eq!(unit.info.as_deref(), Some("11002103"));
    assert_eq!(unit.dry_sf_reason, Some(vec![0, 0, 0, 0]));

    // Call the real accessors instead of hand-rolling the same bit math here — a
    // regression in ams_type()/extruder_assignment()'s shift/mask constants wouldn't have been
    // caught by this test recomputing the expected value independently.
    assert_eq!(unit.ams_type(), Some(3)); // AMS Lite type
    assert_eq!(unit.extruder_assignment(), Some(1)); // Left/deputy extruder
}

#[test]
fn test_kprofile_ams_linking_fields() {
    let json = r#"{
            "cali_idx": 4,
            "filament_id": "GFA01",
            "nozzle_diameter": "0.4",
            "nozzle_id": "HS00-0.4",
            "extruder_id": 0,
            "name": "PLA Matte",
            "k_value": "0.022000",
            "setting_id": "PF12345678901234567",
            "ams_id": 0,
            "tray_id": 2
        }"#;
    let entry: crate::diagnostics::KProfileEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.ams_id, Some(0));
    assert_eq!(entry.tray_id, Some(2));
}

#[test]
fn test_kprofile_ams_fields_absent() {
    let json = r#"{
            "cali_idx": 4,
            "filament_id": "GFA01",
            "nozzle_diameter": "0.4",
            "nozzle_id": "HS00-0.4",
            "extruder_id": 0,
            "name": "PLA",
            "k_value": "0.022",
            "setting_id": "PF12345678901234567"
        }"#;
    let entry: crate::diagnostics::KProfileEntry = serde_json::from_str(json).unwrap();
    assert!(entry.ams_id.is_none());
    assert!(entry.tray_id.is_none());
}

#[test]
fn test_ams_unit_info_accessors_full_bitmask() {
    // "11002103": bits 0-3 = 3, bits 4-7 = 0, bits 8-11 = 1, bits 22-23 = 0
    let unit = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: Some("11002103".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.parse_info(), Some(0x11002103));
    assert_eq!(unit.ams_type(), Some(3));
    assert_eq!(unit.dry_status(), Some(0));
    assert_eq!(unit.extruder_assignment(), Some(1));
    assert_eq!(unit.dry_sub_status(), Some(0));
}

#[test]
fn test_ams_unit_info_accessors_short_bitmask() {
    // "2103": bits 0-3 = 3, bits 4-7 = 0, bits 8-11 = 1, bits 22-25 = 0
    let unit = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: Some("2103".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.parse_info(), Some(0x2103));
    assert_eq!(unit.ams_type(), Some(3));
    assert_eq!(unit.dry_status(), Some(0));
    assert_eq!(unit.extruder_assignment(), Some(1));
    assert_eq!(unit.dry_sub_status(), Some(0));
}

#[test]
fn test_ams_unit_info_accessors_dry_sub_status_distinct_bits() {
    // "19c0153": type=3 (bits 0-3), dry_status=5 (bits 4-7), extruder=1 (bits 8-11),
    // fan1=3 (bits 18-19), fan2=1 (bits 20-21), dry_sub_status=2 (bits 22-23),
    // plus a nonzero bit 24 outside every known field's mask. Every field gets a distinct
    // nonzero value so a shift/mask regression reading a neighboring field's bits instead
    // of its own would fail here, unlike the all-zero-except-one
    // fixtures elsewhere in this file.
    let unit = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: Some("19c0153".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.ams_type(), Some(3));
    assert_eq!(unit.dry_status(), Some(5));
    assert_eq!(unit.extruder_assignment(), Some(1));
    assert_eq!(unit.dry_fan1_status(), Some(3));
    assert_eq!(unit.dry_fan2_status(), Some(1));
    assert_eq!(unit.dry_sub_status(), Some(2));
}

#[test]
fn test_ams_unit_info_accessors_right_extruder() {
    // "2003": bits 0-3 = 3, bits 4-7 = 0, bits 8-11 = 0 (right/main)
    let unit = AmsUnit {
        id: "1".into(),
        temp: "25.0".into(),
        humidity: "4".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: Some("2003".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.ams_type(), Some(3));
    assert_eq!(unit.dry_status(), Some(0));
    assert_eq!(unit.extruder_assignment(), Some(0));
    assert_eq!(unit.dry_sub_status(), Some(0));
}

#[test]
fn test_filament_switch_inlet_decodes_four_bits_not_two() {
    // bind_switch_in occupies bits 24-27 (BUG-136). Raw 0 => In-B, 1 => In-A.
    let unit = |info: &str| AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: Some(info.into()),
        dry_sf_reason: None,
    };

    // 0x00000E03: bits 24-27 = 0 => In-B, and bits 8-11 = 0xE => unfixed.
    assert_eq!(
        unit("E03").filament_switch_inlet(),
        Some(FilamentSwitchInlet::InB)
    );
    assert!(unit("E03").has_unfixed_extruder());

    // 0x01000E03: bits 24-27 = 1 => In-A.
    assert_eq!(
        unit("1000E03").filament_switch_inlet(),
        Some(FilamentSwitchInlet::InA)
    );

    // 0x04000E03: bits 24-27 = 4 => not bound. A 2-bit read would mask this to 0 and
    // wrongly report In-B — this assertion is the whole point of BUG-136.
    assert_eq!(unit("4000E03").filament_switch_inlet(), None);

    // 0x0F000E03: bits 24-27 = 0xF => not bound. A 2-bit read would mask to 3.
    assert_eq!(unit("F000E03").filament_switch_inlet(), None);

    // A unit wired to a fixed extruder is not unfixed, whatever bits 24-27 hold.
    assert!(!unit("1000103").has_unfixed_extruder());

    // Absent info yields neither.
    let mut bare = unit("E03");
    bare.info = None;
    assert_eq!(bare.filament_switch_inlet(), None);
    assert!(!bare.has_unfixed_extruder());
}

#[test]
fn test_ams_unit_info_uninitialized_extruder() {
    // 0xE in bits 8-11 → extruder_assignment returns None
    let unit = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: Some("E03".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.ams_type(), Some(3));
    assert_eq!(unit.extruder_assignment(), None);
}

#[test]
fn test_ams_unit_info_absent() {
    let unit = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: None,
        dry_sf_reason: None,
    };
    assert_eq!(unit.parse_info(), None);
    assert_eq!(unit.ams_type(), None);
    assert_eq!(unit.dry_status(), None);
    assert_eq!(unit.extruder_assignment(), None);
    assert_eq!(unit.dry_sub_status(), None);
}

#[test]
fn test_ams_unit_info_with_dry_status() {
    // bits 4-7 = 5 → dry_status = 5
    let unit = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: Some("2053".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.ams_type(), Some(3));
    assert_eq!(unit.dry_status(), Some(5));
    assert_eq!(unit.extruder_assignment(), Some(0));
}

#[test]
fn test_ams_status_report_merge_from_preserves_array_on_partial_update() {
    // Confirmed via a real P1S wire capture — an incremental `print.ams` push during
    // a tray-switch can carry only `{"tray_tar": "3"}`, with the unit/tray array (and every
    // other field) entirely absent rather than explicitly emptied.
    let mut cached = AmsStatusReport {
        ams: vec![AmsUnit {
            id: "0".into(),
            temp: "26.0".into(),
            humidity: "3".into(),
            humidity_raw: None,
            dry_time: None,
            dry_setting: None,
            tray: None,
            info: None,
            dry_sf_reason: None,
        }],
        ams_exist_bits: Some("1".into()),
        tray_exist_bits: Some("b".into()),
        tray_is_bbl_bits: None,
        tray_now: Some("3".into()),
        tray_pre: None,
        tray_tar: None,
        version: Some(20),
        tray_read_done_bits: None,
        tray_reading_bits: None,
        insert_flag: None,
        power_on_flag: None,
        cali_id: None,
        cali_stat: None,
        calibrate_remain_flag: None,
        cfs: None,
    };

    let partial = AmsStatusReport {
        ams: vec![],
        ams_exist_bits: None,
        tray_exist_bits: None,
        tray_is_bbl_bits: None,
        tray_now: None,
        tray_pre: None,
        tray_tar: Some("3".into()),
        version: None,
        tray_read_done_bits: None,
        tray_reading_bits: None,
        insert_flag: None,
        power_on_flag: None,
        cali_id: None,
        cali_stat: None,
        calibrate_remain_flag: None,
        cfs: None,
    };

    cached.merge_from(&partial);

    assert_eq!(
        cached.ams.len(),
        1,
        "unit array must survive a partial push"
    );
    assert_eq!(cached.ams[0].id, "0");
    assert_eq!(cached.tray_tar.as_deref(), Some("3"), "new field applies");
    assert_eq!(
        cached.tray_now.as_deref(),
        Some("3"),
        "untouched field stays cached"
    );
    assert_eq!(cached.ams_exist_bits.as_deref(), Some("1"));
    assert_eq!(cached.version, Some(20));
}

#[test]
fn test_ams_status_report_merge_from_preserves_units_not_in_incoming_array() {
    // Was test_..._replaces_array_on_full_update, asserting the opposite of this —
    // rewritten once BambuStudio's DevFilaSystem.cpp confirmed a keyed persistent per-unit
    // map (system->amsList.find(ams_id), never pruned by a push's contents) is the real
    // behavior, not whole-array replacement. A push mentioning only unit "1" must not drop
    // previously-cached unit "0" — it's the same wire-economy principle already
    // confirmed one level up (the ams key itself can be absent), applied one level deeper.
    let mut cached = AmsStatusReport {
        ams: vec![AmsUnit {
            id: "0".into(),
            temp: "26.0".into(),
            humidity: "3".into(),
            humidity_raw: None,
            dry_time: None,
            dry_setting: None,
            tray: None,
            info: None,
            dry_sf_reason: None,
        }],
        ams_exist_bits: None,
        tray_exist_bits: None,
        tray_is_bbl_bits: None,
        tray_now: None,
        tray_pre: None,
        tray_tar: None,
        version: None,
        tray_read_done_bits: None,
        tray_reading_bits: None,
        insert_flag: None,
        power_on_flag: None,
        cali_id: None,
        cali_stat: None,
        calibrate_remain_flag: None,
        cfs: None,
    };

    let partial = AmsStatusReport {
        ams: vec![AmsUnit {
            id: "1".into(),
            temp: "27.0".into(),
            humidity: "4".into(),
            humidity_raw: None,
            dry_time: None,
            dry_setting: None,
            tray: None,
            info: None,
            dry_sf_reason: None,
        }],
        ams_exist_bits: None,
        tray_exist_bits: None,
        tray_is_bbl_bits: None,
        tray_now: None,
        tray_pre: None,
        tray_tar: None,
        version: None,
        tray_read_done_bits: None,
        tray_reading_bits: None,
        insert_flag: None,
        power_on_flag: None,
        cali_id: None,
        cali_stat: None,
        calibrate_remain_flag: None,
        cfs: None,
    };

    cached.merge_from(&partial);

    assert_eq!(
        cached.ams.len(),
        2,
        "unit 0 must survive a unit-1-only push"
    );
    assert!(cached.ams.iter().any(|u| u.id == "0"));
    assert!(cached.ams.iter().any(|u| u.id == "1"));
}

#[test]
fn test_ams_unit_merge_from_preserves_fields_on_absence() {
    // Confirmed against BambuStudio's DevFilaSystem.cpp — dry_time/humidity_raw/
    // dry_sf_reason (and temp/humidity/dry_setting/tray) all preserve on absence within a
    // matched unit, rather than a partial per-unit push nulling out previously-known values.
    let mut cached = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: Some("42".into()),
        dry_time: Some(120),
        dry_setting: Some(AmsDrySetting {
            dry_temperature: Some(55),
            dry_duration: Some(240),
            dry_filament: Some("PA-CF".into()),
        }),
        tray: None,
        info: Some("10001003".into()),
        dry_sf_reason: Some(vec![1, 2]),
    };

    let partial = AmsUnit {
        id: "0".into(),
        temp: "27.0".into(),
        humidity: "4".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: None,
        dry_sf_reason: None,
    };

    cached.merge_from(&partial);

    assert_eq!(cached.temp, "27.0", "temp always takes the incoming value");
    assert_eq!(
        cached.humidity, "4",
        "humidity always takes the incoming value"
    );
    assert_eq!(
        cached.humidity_raw.as_deref(),
        Some("42"),
        "humidity_raw must survive a temp/humidity-only push"
    );
    assert_eq!(cached.dry_time, Some(120), "dry_time must survive");
    assert!(cached.dry_setting.is_some(), "dry_setting must survive");
    assert_eq!(cached.info.as_deref(), Some("10001003"));
    assert_eq!(
        cached.dry_sf_reason,
        Some(vec![1, 2]),
        "dry_sf_reason must survive"
    );
}

#[test]
fn test_p1s_print_sequence_ams_merge_never_regresses() {
    // Regression test: replays a real P1S wire capture (342 incremental pushes across
    // a full print — start through finish — including the exact partial `{"tray_tar":"3"}`-only
    // pushes that proved the bug) through `AmsStatusReport::merge_from` and asserts the merged
    // unit array never drops from non-empty back to empty mid-sequence, the failure mode
    // `merge_from` fixes. Captured via `bambino-cli dump --follow` during a
    // real tray-load on a P1S.
    let capture = include_str!("../../../../tests/mocks/P1S_print_sequence.ndjson");

    let mut merged: Option<AmsStatusReport> = None;
    let mut saw_non_empty = false;

    for line in capture.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let report: TelemetryReport = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("failed to parse capture line: {e}\n{line}"));
        let Some(ams) = report.print.and_then(|p| p.ams) else {
            continue;
        };

        match &mut merged {
            Some(cached) => cached.merge_from(&ams),
            None => merged = Some(ams),
        }

        let unit_count = merged.as_ref().unwrap().ams.len();
        if unit_count > 0 {
            saw_non_empty = true;
        } else if saw_non_empty {
            panic!(
                "AMS unit array regressed to empty after being non-empty earlier in the \
                 sequence — merge_from must preserve it across a partial push"
            );
        }
    }

    assert!(
        saw_non_empty,
        "capture never reported a non-empty AMS unit array — fixture may be stale"
    );
}

#[test]
fn test_ams_tray_merge_from_preserves_fields_on_absence() {
    // Confirmed against BambuStudio's `ParseAmsTrayInfo` — a partial per-tray push (e.g. just
    // `state` changing) must not clobber previously-known fields the push didn't repeat.
    let mut cached = AmsTray {
        id: "0".into(),
        state: Some(11),
        tray_type: Some("PLA".into()),
        tray_color: Some("FF0000FF".into()),
        tag_uid: Some("ABCDEF1234567890".into()),
        tray_uuid: Some("UUID_MOCK".into()),
        remain: Some(80),
        nozzle_temp_max: Some("220".into()),
        nozzle_temp_min: Some("190".into()),
        k: Some(0.02),
        n: Some(1),
        ..Default::default()
    };

    let partial = AmsTray {
        id: "0".into(),
        state: Some(10),
        ..Default::default()
    };

    cached.merge_from(&partial);

    assert_eq!(
        cached.state,
        Some(10),
        "state always takes the incoming value"
    );
    assert_eq!(
        cached.tray_type.as_deref(),
        Some("PLA"),
        "tray_type must survive a state-only push"
    );
    assert_eq!(cached.tray_color.as_deref(), Some("FF0000FF"));
    assert_eq!(
        cached.tag_uid.as_deref(),
        Some("ABCDEF1234567890"),
        "tag_uid must survive absence — deliberately diverges from BambuStudio's literal \
         reset-to-default, see AmsTray::merge_from's doc comment"
    );
    assert_eq!(cached.tray_uuid.as_deref(), Some("UUID_MOCK"));
    assert_eq!(
        cached.remain,
        Some(80),
        "remain must survive absence — same deliberate divergence as tag_uid"
    );
    assert_eq!(cached.nozzle_temp_max.as_deref(), Some("220"));
    assert_eq!(cached.k, Some(0.02));
    assert_eq!(cached.n, Some(1));
}

#[test]
fn test_ams_tray_remain_g_and_filament_setting_id() {
    // Wire keys are `remain_g` and `setting_id` (the latter renamed to
    // `filament_setting_id` in this crate to avoid confusion with `tray_info_idx`).
    let json = r#"{
            "print": {
                "ams": {
                    "ams": [
                        {
                            "id": "0",
                            "temp": "26.0",
                            "humidity": "3",
                            "tray": [
                                { "id": "0", "remain_g": 420, "setting_id": "GFL99" },
                                { "id": "1", "remain_g": -1 },
                                { "id": "2", "remain_g": 0 }
                            ]
                        }
                    ]
                }
            }
        }"#;
    let unit = &serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap()
        .ams
        .unwrap()
        .ams[0];
    let trays = unit.tray.as_ref().unwrap();

    assert_eq!(trays[0].remain_g, Some(420));
    assert_eq!(trays[0].filament_setting_id.as_deref(), Some("GFL99"));
    assert_eq!(trays[0].remaining_weight_grams(), Some(420));

    // -1 sentinel ("not provided by firmware") translates to None via the accessor.
    assert_eq!(trays[1].remain_g, Some(-1));
    assert_eq!(trays[1].remaining_weight_grams(), None);

    // 0 ("confirmed empty") also translates to None, matching BambuStudio's
    // get_filament_remain_weight().
    assert_eq!(trays[2].remain_g, Some(0));
    assert_eq!(trays[2].remaining_weight_grams(), None);
}

#[test]
fn test_ams_tray_remain_g_absent() {
    let tray = AmsTray::default();
    assert_eq!(tray.remain_g, None);
    assert_eq!(tray.remaining_weight_grams(), None);
    assert_eq!(tray.filament_setting_id, None);
}

#[test]
fn test_ams_unit_merge_from_keys_and_prunes_trays() {
    // Finding 1: a tray_id present in both cached and incoming arrays must field-merge (not
    // get wholesale-replaced by whatever subset of fields the incoming push repeats), a new
    // tray_id must be added, and a cached tray_id absent from a *present* incoming array must
    // be pruned — confirmed against `ParseAmsInfo`'s `existing_tray_set`-gated erase loop.
    let mut cached = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: Some(vec![
            AmsTray {
                id: "0".into(),
                state: Some(11),
                tray_type: Some("PLA".into()),
                tray_color: Some("FF0000FF".into()),
                remain: Some(85),
                ..Default::default()
            },
            AmsTray {
                id: "1".into(),
                state: Some(9),
                ..Default::default()
            },
        ]),
        info: None,
        dry_sf_reason: None,
    };

    // Incoming push: tray 0 gets a partial field update (remain only), tray 1 is absent
    // (pruned), tray 2 is new (added).
    let partial = AmsUnit {
        id: "0".into(),
        temp: "27.0".into(),
        humidity: "4".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: Some(vec![
            AmsTray {
                id: "0".into(),
                remain: Some(70),
                ..Default::default()
            },
            AmsTray {
                id: "2".into(),
                state: Some(11),
                tray_type: Some("PETG".into()),
                ..Default::default()
            },
        ]),
        info: None,
        dry_sf_reason: None,
    };

    cached.merge_from(&partial);

    let trays = cached.tray.as_ref().unwrap();
    assert_eq!(
        trays.len(),
        2,
        "tray 1 must be pruned, tray 2 must be added"
    );
    assert!(
        !trays.iter().any(|t| t.id == "1"),
        "tray absent from a present incoming array must be pruned"
    );

    let tray0 = trays.iter().find(|t| t.id == "0").unwrap();
    assert_eq!(tray0.remain, Some(70), "new field applies");
    assert_eq!(
        tray0.tray_type.as_deref(),
        Some("PLA"),
        "fields absent from the incoming partial tray push must survive"
    );
    assert_eq!(tray0.tray_color.as_deref(), Some("FF0000FF"));

    let tray2 = trays.iter().find(|t| t.id == "2").unwrap();
    assert_eq!(tray2.tray_type.as_deref(), Some("PETG"));
}

#[test]
fn test_ams_unit_merge_from_absent_tray_key_leaves_cache_untouched() {
    // `tray: None` (the wire's `tray` key entirely absent from this push) must leave the
    // cached trays untouched — distinct from `tray: Some(vec![])` (key present but empty),
    // which prunes every cached tray. See AmsUnit::tray's doc comment.
    let mut cached = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: Some(vec![AmsTray {
            id: "0".into(),
            tray_type: Some("PLA".into()),
            ..Default::default()
        }]),
        info: None,
        dry_sf_reason: None,
    };

    let partial = AmsUnit {
        id: "0".into(),
        temp: "27.0".into(),
        humidity: "4".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: None,
        info: None,
        dry_sf_reason: None,
    };

    cached.merge_from(&partial);

    assert_eq!(
        cached.tray.as_ref().unwrap().len(),
        1,
        "absent tray key must not touch cached trays"
    );
}

#[test]
fn test_ams_unit_merge_from_present_empty_tray_prunes_all() {
    // `tray: Some(vec![])` — key present but empty — must prune every cached tray, matching
    // `ParseAmsInfo`'s prune loop running (with an empty existing_tray_set) whenever the
    // `tray` key is present at all, even with zero elements.
    let mut cached = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: Some(vec![AmsTray {
            id: "0".into(),
            tray_type: Some("PLA".into()),
            ..Default::default()
        }]),
        info: None,
        dry_sf_reason: None,
    };

    let partial = AmsUnit {
        id: "0".into(),
        temp: "27.0".into(),
        humidity: "4".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: Some(vec![]),
        info: None,
        dry_sf_reason: None,
    };

    cached.merge_from(&partial);

    assert_eq!(
        cached.tray.as_ref().unwrap().len(),
        0,
        "a present-but-empty tray array must prune every cached tray"
    );
}
