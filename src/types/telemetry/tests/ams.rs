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
    assert_eq!(unit.tray.len(), 4);

    assert_eq!(unit.tray[0].tray_type.as_deref(), Some("PLA"));
    assert_eq!(unit.tray[0].state, Some(10));
    assert_eq!(unit.tray[1].state, Some(11));
    assert_eq!(unit.tray[1].tray_type.as_deref(), Some("PETG"));
    // Slot 2: empty (truncated JSON — P1S firmware behavior)
    assert_eq!(unit.tray[2].state, None);
    assert_eq!(unit.tray[2].get_state(), 9);
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

    // BUG-035: call the real accessors instead of hand-rolling the same bit math here — a
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
    // "11002103": bits 0-3 = 3, bits 4-7 = 0, bits 8-11 = 1, bits 22-25 = 4
    let unit = AmsUnit {
        id: "0".into(),
        temp: "26.0".into(),
        humidity: "3".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: vec![],
        info: Some("11002103".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.parse_info(), Some(0x11002103));
    assert_eq!(unit.ams_type(), Some(3));
    assert_eq!(unit.dry_status(), Some(0));
    assert_eq!(unit.extruder_assignment(), Some(1));
    assert_eq!(unit.dry_sub_status(), Some(4));
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
        tray: vec![],
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
fn test_ams_unit_info_accessors_right_extruder() {
    // "2003": bits 0-3 = 3, bits 4-7 = 0, bits 8-11 = 0 (right/main)
    let unit = AmsUnit {
        id: "1".into(),
        temp: "25.0".into(),
        humidity: "4".into(),
        humidity_raw: None,
        dry_time: None,
        dry_setting: None,
        tray: vec![],
        info: Some("2003".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.ams_type(), Some(3));
    assert_eq!(unit.dry_status(), Some(0));
    assert_eq!(unit.extruder_assignment(), Some(0));
    assert_eq!(unit.dry_sub_status(), Some(0));
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
        tray: vec![],
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
        tray: vec![],
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
        tray: vec![],
        info: Some("2053".into()),
        dry_sf_reason: None,
    };
    assert_eq!(unit.ams_type(), Some(3));
    assert_eq!(unit.dry_status(), Some(5));
    assert_eq!(unit.extruder_assignment(), Some(0));
}

#[test]
fn test_ams_status_report_merge_from_preserves_array_on_partial_update() {
    // BUG-091: confirmed via a real P1S wire capture — an incremental `print.ams` push during
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
            tray: vec![],
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
    };

    cached.merge_from(&partial);

    assert_eq!(cached.ams.len(), 1, "unit array must survive a partial push");
    assert_eq!(cached.ams[0].id, "0");
    assert_eq!(cached.tray_tar.as_deref(), Some("3"), "new field applies");
    assert_eq!(cached.tray_now.as_deref(), Some("3"), "untouched field stays cached");
    assert_eq!(cached.ams_exist_bits.as_deref(), Some("1"));
    assert_eq!(cached.version, Some(20));
}

#[test]
fn test_ams_status_report_merge_from_replaces_array_on_full_update() {
    let mut cached = AmsStatusReport {
        ams: vec![AmsUnit {
            id: "0".into(),
            temp: "26.0".into(),
            humidity: "3".into(),
            humidity_raw: None,
            dry_time: None,
            dry_setting: None,
            tray: vec![],
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
    };

    let full = AmsStatusReport {
        ams: vec![AmsUnit {
            id: "1".into(),
            temp: "27.0".into(),
            humidity: "4".into(),
            humidity_raw: None,
            dry_time: None,
            dry_setting: None,
            tray: vec![],
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
    };

    cached.merge_from(&full);

    assert_eq!(cached.ams.len(), 1);
    assert_eq!(cached.ams[0].id, "1", "a non-empty incoming array is authoritative");
}
