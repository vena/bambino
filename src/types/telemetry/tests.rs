use super::*;

#[test]
fn test_temperature_unpacking_composite() {
    let (actual, target) = PrinterTelemetry::unpack_temperature(6553700.0);
    assert_eq!(actual, 100);
    assert_eq!(target, 100);

    let (actual_idle, target_idle) = PrinterTelemetry::unpack_temperature(35.0);
    assert_eq!(actual_idle, 35);
    assert_eq!(target_idle, 0);

    // Fractional temps from P1S/A1 models — truncated to integer
    let (actual_frac, target_frac) = PrinterTelemetry::unpack_temperature(27.625);
    assert_eq!(actual_frac, 27);
    assert_eq!(target_frac, 0);
}

#[test]
fn test_airduct_deserialization() {
    let json_data = r#"{
            "device": {
                "airduct": {
                    "parts": [
                        { "id": 160, "state": 85 }
                    ]
                }
            }
        }"#;

    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let device = report.device.unwrap();
    let airduct = device.airduct.unwrap();
    assert_eq!(airduct.parts.len(), 1);
    assert_eq!(airduct.parts[0].id, 160);
    assert_eq!(airduct.parts[0].state, Some(85));
}

#[test]
fn test_print_error_deserialization() {
    let json_data = r#"{
            "print": {
                "print_error": 83902476
            }
        }"#;

    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let print = report.print.unwrap();
    assert_eq!(print.print_error, Some(83902476));
}

#[test]
fn test_hms_array_deserialization() {
    let json_data = r#"{
            "print": {
                "hms": [
                    { "attr": 50331904, "code": 65543 },
                    { "attr": 83886336, "code": 81924 }
                ]
            }
        }"#;

    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let print = report.print.unwrap();
    let hms = print.hms.unwrap();
    assert_eq!(hms.len(), 2);
    assert_eq!(hms[0].attr, 50331904);
    assert_eq!(hms[0].code, 65543);
    assert_eq!(hms[1].attr, 83886336);
    assert_eq!(hms[1].code, 81924);
}

#[test]
fn test_hms_absent_vs_empty() {
    let absent = r#"{ "print": {} }"#;
    let report: TelemetryReport = serde_json::from_str(absent).unwrap();
    assert!(report.print.unwrap().hms.is_none());

    let empty = r#"{ "print": { "hms": [] } }"#;
    let report: TelemetryReport = serde_json::from_str(empty).unwrap();
    let hms = report.print.unwrap().hms.unwrap();
    assert!(hms.is_empty());
}

#[test]
fn test_camera_fields_deserialization() {
    let json_data = r#"{
            "print": {
                "ipcam": {
                    "ipcam_dev": "1",
                    "ipcam_record": "enable",
                    "timelapse": "enable",
                    "mode_bits": 3,
                    "resolution": "",
                    "tutk_server": "disable"
                }
            }
        }"#;

    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let ipcam = report.print.unwrap().ipcam.unwrap();
    assert_eq!(ipcam.ipcam_dev.as_deref(), Some("1"));
    assert_eq!(ipcam.ipcam_record.as_deref(), Some("enable"));
    assert_eq!(ipcam.timelapse.as_deref(), Some("enable"));
    assert_eq!(ipcam.mode_bits, Some(3));
    assert_eq!(ipcam.tutk_server.as_deref(), Some("disable"));
}

#[test]
fn test_xcam_deserialization() {
    let json_data = r#"{
            "print": {
                "xcam": {
                    "first_layer_inspector": true,
                    "spaghetti_detector": false
                }
            }
        }"#;

    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let print = report.print.unwrap();
    let xcam = print.xcam.unwrap();
    assert_eq!(xcam["first_layer_inspector"], true);
    assert_eq!(xcam["spaghetti_detector"], false);
}

#[test]
fn test_mc_print_sub_stage_deserialization() {
    let json_data = r#"{
            "print": {
                "mc_print_sub_stage": 3
            }
        }"#;

    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let print = report.print.unwrap();
    assert_eq!(print.mc_print_sub_stage, Some(3));
}

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
fn test_full_telemetry_with_diagnostics() {
    let json_data = r#"{
            "print": {
                "gcode_state": "RUNNING",
                "mc_print_sub_stage": 0,
                "print_error": 0,
                "hms": [],
                "ipcam": {
                    "ipcam_dev": "1",
                    "ipcam_record": "enable",
                    "timelapse": "disable"
                },
                "xcam": { "allow_skip_parts": false }
            }
        }"#;

    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let print = report.print.unwrap();
    assert_eq!(print.gcode_state.as_deref(), Some("RUNNING"));
    assert_eq!(print.mc_print_sub_stage, Some(0));
    assert_eq!(print.print_error, Some(0));
    assert!(print.hms.unwrap().is_empty());
    let ipcam = print.ipcam.unwrap();
    assert_eq!(ipcam.ipcam_record.as_deref(), Some("enable"));
    assert_eq!(ipcam.timelapse.as_deref(), Some("disable"));
}

#[test]
fn test_door_open_from_home_flag() {
    let json_open = r#"{ "print": { "home_flag": 8388608 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_open)
        .expect("valid json")
        .print
        .expect("print present");
    assert!(print.is_door_open_from_home_flag());

    let json_closed = r#"{ "print": { "home_flag": 0 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_closed)
        .expect("valid json")
        .print
        .expect("print present");
    assert!(!print.is_door_open_from_home_flag());
}

#[test]
fn test_door_open_from_stat() {
    let json_open = r#"{ "print": { "stat": "0x00800000" } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_open)
        .expect("valid json")
        .print
        .expect("print present");
    assert!(print.is_door_open_from_stat());

    let json_closed = r#"{ "print": { "stat": "0x00000000" } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_closed)
        .expect("valid json")
        .print
        .expect("print present");
    assert!(!print.is_door_open_from_stat());
}

#[test]
fn test_door_open_missing_fields() {
    let json_empty = r#"{ "print": {} }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_empty)
        .expect("valid json")
        .print
        .expect("print present");
    assert!(!print.is_door_open_from_home_flag());
    assert!(!print.is_door_open_from_stat());
}

#[test]
fn test_p1s_wire_capture_end_to_end() {
    let json_data = include_str!("../../../tests/mocks/P1S.json");
    let report: TelemetryReport =
        serde_json::from_str(json_data).expect("P1S wire capture must deserialize");
    let print = report.print.expect("print present");

    assert_eq!(print.gcode_state.as_deref(), Some("FINISH"));
    assert_eq!(
        print.subtask_name.as_deref(),
        Some("8_Minute_Print_Multi-Fit_Cardboard_Spool_Ring")
    );
    assert_eq!(print.layer_num, Some(27));
    assert_eq!(print.total_layers, Some(27));
    assert_eq!(print.mc_percent, Some(100));
    assert_eq!(print.mc_remaining_time, Some(0));
    assert_eq!(print.home_flag, Some(6374672));
    assert_eq!(print.stg_cur, Some(0));
    assert_eq!(print.print_error, Some(0));
    assert!(print.hms.unwrap().is_empty());
    assert!(print.sdcard);
    assert_eq!(print.wifi_signal.as_deref(), Some("-41dBm"));

    // Fix A: float temps deserialize correctly
    assert!((print.bed_temper.unwrap() - 27.625).abs() < 0.001);
    assert!((print.nozzle_temper.unwrap() - 29.46875).abs() < 0.001);
    assert_eq!(print.nozzle_target_temper.unwrap() as u32, 0);
    assert_eq!(print.chamber_temper.unwrap() as u32, 5);

    // Fix E: bed_target_temper
    assert_eq!(print.bed_target_temper.unwrap() as u32, 0);

    // Fix H: total_layer_num alias
    assert_eq!(print.total_layers, Some(27));

    // Fix G: nested ipcam
    let ipcam = print.ipcam.expect("ipcam present");
    assert_eq!(ipcam.ipcam_dev.as_deref(), Some("1"));
    assert_eq!(ipcam.ipcam_record.as_deref(), Some("disable"));
    assert_eq!(ipcam.timelapse.as_deref(), Some("disable"));
    assert_eq!(ipcam.mode_bits, Some(3));

    // Fix F: AMS tray IDs are strings
    let ams = print.ams.expect("ams present");
    assert_eq!(ams.ams.len(), 1);
    let unit = &ams.ams[0];
    assert_eq!(unit.id, "0");
    assert_eq!(unit.tray.len(), 4);
    assert_eq!(unit.tray[0].id, "0");
    assert_eq!(unit.tray[3].id, "3");

    // Fix D: vt_tray
    let vt = print.vt_tray.expect("vt_tray present");
    assert_eq!(vt.id.as_deref(), Some("254"));
    assert_eq!(vt.tray_color.as_deref(), Some("FFFFFF00"));
    assert_eq!(vt.remain, Some(0));
    assert!((vt.k.unwrap() - 0.02).abs() < 0.001);
    assert_eq!(vt.cali_idx, Some(-1));
}

#[test]
fn test_temperature_fields_accept_float_and_int() {
    let json_float = r#"{ "print": { "bed_temper": 27.625, "nozzle_temper": 29.46875 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_float)
        .unwrap()
        .print
        .unwrap();
    assert!((print.bed_temper.unwrap() - 27.625).abs() < 0.001);
    assert!((print.nozzle_temper.unwrap() - 29.46875).abs() < 0.001);

    let json_int = r#"{ "print": { "bed_temper": 100, "nozzle_temper": 40 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_int)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.bed_temper.unwrap() as u32, 100);
    assert_eq!(print.nozzle_temper.unwrap() as u32, 40);
}

#[test]
fn test_temperature_boundary_500_and_501() {
    let (actual, target) = PrinterTelemetry::unpack_temperature(500.0);
    assert_eq!(actual, 500);
    assert_eq!(target, 0);

    // 501 = 0x000001F5 → actual=501, target=0 (but > threshold so unpacked)
    let (actual, target) = PrinterTelemetry::unpack_temperature(501.0);
    assert_eq!(actual, 501);
    assert_eq!(target, 0);

    // Real composite: target=60, actual=48 → (60 << 16) | 48 = 3932208
    let (actual, target) = PrinterTelemetry::unpack_temperature(3932208.0);
    assert_eq!(actual, 48);
    assert_eq!(target, 60);
}

#[test]
fn test_ctc_info_deserialization_composite() {
    let json_data = r#"{
            "device": {
                "ctc": {
                    "info": { "temp": 3932208 }
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let ctc = report.device.unwrap().ctc.unwrap();
    let temp = ctc.info.unwrap().temp.unwrap();
    let (actual, target) = PrinterTelemetry::unpack_temperature(temp as f64);
    assert_eq!(actual, 48);
    assert_eq!(target, 60);
}

#[test]
fn test_ctc_info_deserialization_direct() {
    let json_data = r#"{
            "device": {
                "ctc": {
                    "info": { "temp": 35 }
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let temp = report
        .device
        .unwrap()
        .ctc
        .unwrap()
        .info
        .unwrap()
        .temp
        .unwrap();
    let (actual, target) = PrinterTelemetry::unpack_temperature(temp as f64);
    assert_eq!(actual, 35);
    assert_eq!(target, 0);
}

#[test]
fn test_device_nesting_in_pushall() {
    let json_data = r#"{
            "print": {
                "gcode_state": "IDLE",
                "device": {
                    "ctc": {
                        "info": { "temp": 3932208 }
                    },
                    "nozzle": {
                        "info": [{ "id": 0, "diameter": 0.4 }]
                    },
                    "airduct": {
                        "parts": [{ "id": 160, "state": 50 }]
                    }
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let print = report.print.unwrap();
    let device = print.device.expect("device nested in print");
    let ctc_temp = device.ctc.unwrap().info.unwrap().temp.unwrap();
    assert_eq!(ctc_temp, 3932208);
    assert_eq!(device.nozzle.unwrap().info[0].id, 0);
    assert_eq!(device.airduct.unwrap().parts[0].state, Some(50));
}

#[test]
fn test_device_incremental_top_level() {
    let json_data = r#"{
            "device": {
                "nozzle": {
                    "info": [{ "id": 0, "diameter": 0.4 }, { "id": 1, "diameter": 0.6 }]
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    assert!(report.print.is_none());
    let device = report.device.unwrap();
    let nozzles = &device.nozzle.unwrap().info;
    assert_eq!(nozzles.len(), 2);
    assert_eq!(nozzles[1].id, 1);
}

#[test]
fn test_deserialize_permissive_bool_variants() {
    // Bool true
    let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": true } }"#).unwrap();
    assert!(r.print.unwrap().sdcard);

    // Bool false
    let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": false } }"#).unwrap();
    assert!(!r.print.unwrap().sdcard);

    // Int 1
    let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": 1 } }"#).unwrap();
    assert!(r.print.unwrap().sdcard);

    // Int 0
    let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": 0 } }"#).unwrap();
    assert!(!r.print.unwrap().sdcard);

    // String "HAS_SDCARD_NORMAL"
    let r: TelemetryReport =
        serde_json::from_str(r#"{ "print": { "sdcard": "HAS_SDCARD_NORMAL" } }"#).unwrap();
    assert!(r.print.unwrap().sdcard);

    // String "TRUE"
    let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": "TRUE" } }"#).unwrap();
    assert!(r.print.unwrap().sdcard);

    // String "1"
    let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": "1" } }"#).unwrap();
    assert!(r.print.unwrap().sdcard);

    // String other → false
    let r: TelemetryReport = serde_json::from_str(r#"{ "print": { "sdcard": "nope" } }"#).unwrap();
    assert!(!r.print.unwrap().sdcard);

    // Missing → default false
    let r: TelemetryReport = serde_json::from_str(r#"{ "print": {} }"#).unwrap();
    assert!(!r.print.unwrap().sdcard);
}

#[test]
fn test_deserialize_permissive_bool_malformed_shape_is_error() {
    // A malformed `sdcard` value (an object, not a bool/int/string) must be a hard parse error,
    // not silently coerced to `false` — that would be indistinguishable from a legitimately
    // absent/false field.
    let result: Result<TelemetryReport, _> =
        serde_json::from_str(r#"{ "print": { "sdcard": {} } }"#);
    assert!(
        result.is_err(),
        "expected malformed sdcard shape to be a deserialization error, got {:?}",
        result.map(|r| r.print.map(|p| p.sdcard))
    );
}

#[test]
fn test_parse_hex_string_variants() {
    assert_eq!(
        PrinterTelemetry::parse_hex_string("0x00800000"),
        Some(0x00800000)
    );
    assert_eq!(
        PrinterTelemetry::parse_hex_string("0X00800000"),
        Some(0x00800000)
    );
    assert_eq!(
        PrinterTelemetry::parse_hex_string("00800000"),
        Some(0x00800000)
    );
    assert_eq!(PrinterTelemetry::parse_hex_string("ff"), Some(0xff));
    assert_eq!(PrinterTelemetry::parse_hex_string("zzzz"), None);
    assert_eq!(PrinterTelemetry::parse_hex_string(""), None);
}

#[test]
fn test_ethernet_active_bitmask() {
    let json = r#"{ "print": { "home_flag": 262144 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert!(print.is_ethernet_active());

    let json_off = r#"{ "print": { "home_flag": 0 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_off)
        .unwrap()
        .print
        .unwrap();
    assert!(!print.is_ethernet_active());

    let json_missing = r#"{ "print": {} }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_missing)
        .unwrap()
        .print
        .unwrap();
    assert!(!print.is_ethernet_active());
}

#[test]
fn test_ethernet_active_via_wifi_signal() {
    let json = r#"{ "print": { "wifi_signal": "-90dBm" } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert!(print.is_ethernet_active_via_wifi_signal());

    let json_off = r#"{ "print": { "wifi_signal": "-52dBm" } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_off)
        .unwrap()
        .print
        .unwrap();
    assert!(!print.is_ethernet_active_via_wifi_signal());

    let json_missing = r#"{ "print": {} }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_missing)
        .unwrap()
        .print
        .unwrap();
    assert!(!print.is_ethernet_active_via_wifi_signal());
}

#[test]
fn test_power_on_flag_deserialization() {
    let json_true = r#"{ "print": { "power_on_flag": true } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_true)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.power_on_flag, Some(true));

    let json_false = r#"{ "print": { "power_on_flag": false } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_false)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.power_on_flag, Some(false));

    let json_missing = r#"{ "print": {} }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_missing)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.power_on_flag, None);
}

#[test]
fn test_total_layer_num_alias() {
    // Wire name: total_layer_num
    let json = r#"{ "print": { "total_layer_num": 42 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.total_layers, Some(42));

    // Legacy name still works
    let json2 = r#"{ "print": { "total_layers": 99 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json2)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.total_layers, Some(99));
}

#[test]
fn test_mc_percent_deserialization() {
    let json = r#"{ "print": { "mc_percent": 100 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.mc_percent, Some(100));
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
        .info[0];
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
        .info[0];
    assert_eq!(nozzle.id, 1);
    assert_eq!(nozzle.max_temp, Some(350));
    assert_eq!(nozzle.serial_number.as_deref(), Some("IDEX-SN-456"));
    assert_eq!(nozzle.filament_colour.as_deref(), Some("00FF00"));
}

#[test]
fn test_fun_field_deserialization_top_level() {
    let json = r#"{ "fun": "3EC1AFFF9CFF" }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.fun.as_deref(), Some("3EC1AFFF9CFF"));
}

#[test]
fn test_fun_field_deserialization_nested_in_print() {
    let json = r#"{ "print": { "fun": "1AFFF9CFF" } }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.print.unwrap().fun.as_deref(), Some("1AFFF9CFF"));
}

#[test]
fn test_is_developer_mode() {
    // Bit 0x20000000 SET → signature required → developer mode OFF
    assert_eq!(is_developer_mode("3EC1AFFF9CFF"), Some(false));
    // Bit 0x20000000 CLEAR → developer mode ON
    assert_eq!(is_developer_mode("3EC18FFF9CFF"), Some(true));
    // Short value with bit clear
    assert_eq!(is_developer_mode("0"), Some(true));
    // Exact bit value
    assert_eq!(is_developer_mode("20000000"), Some(false));
    // Invalid hex
    assert_eq!(is_developer_mode("zzzz"), None);
    // Empty string
    assert_eq!(is_developer_mode(""), None);
}

#[test]
fn test_is_developer_mode_real_mock_values() {
    // From pybambu MOCK-H2D.json
    assert_eq!(is_developer_mode("1AFFF9CFF"), Some(false));
    // From pybambu MOCK-P2S.json
    assert_eq!(is_developer_mode("60029FD1A3FF9CB7"), Some(false));
    // From pybambu MOCK-X2D.json
    assert_eq!(is_developer_mode("40029FD1B30F9CB7"), Some(false));
}

#[test]
fn test_airduct_mode_telemetry() {
    let json_data = r#"{
            "device": {
                "airduct": {
                    "parts": [{ "id": 160, "state": 50 }],
                    "modeCur": 1,
                    "modeList": [
                        { "modeId": 0 },
                        { "modeId": 1 }
                    ]
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let airduct = report.device.unwrap().airduct.unwrap();
    assert_eq!(airduct.mode_cur, Some(1));
    assert_eq!(airduct.mode_list.len(), 2);
    assert_eq!(airduct.mode_list[0].mode_id, 0);
    assert_eq!(airduct.mode_list[1].mode_id, 1);
}

#[test]
fn test_airduct_mode_telemetry_with_laser() {
    let json_data = r#"{
            "device": {
                "airduct": {
                    "parts": [],
                    "modeCur": 0,
                    "modeList": [
                        { "modeId": 0 },
                        { "modeId": 1 },
                        { "modeId": 2 }
                    ]
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let airduct = report.device.unwrap().airduct.unwrap();
    assert_eq!(airduct.mode_cur, Some(0));
    assert_eq!(airduct.mode_list.len(), 3);
    assert_eq!(airduct.mode_list[2].mode_id, 2);
}

#[test]
fn test_airduct_mode_absent() {
    let json_data = r#"{
            "device": {
                "airduct": {
                    "parts": [{ "id": 160, "state": 85 }]
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json_data).unwrap();
    let airduct = report.device.unwrap().airduct.unwrap();
    assert_eq!(airduct.mode_cur, None);
    assert!(airduct.mode_list.is_empty());
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
    assert_eq!(extruder.info.len(), 2);
    assert_eq!(extruder.extruder_count(), 2);
    assert_eq!(extruder.active_extruder_index(), 0);

    // id 0 (right/main): temp 16056565 = 0x00F500F5 → composite packed
    let right = &extruder.info[0];
    assert_eq!(right.id, 0);
    let (right_actual, right_target) = right.temperatures();
    assert_eq!(right_actual, 245);
    assert_eq!(right_target, 245);
    assert_eq!(right.filam_bak, vec![48]);
    assert_eq!(right.stat, Some(197376));

    // id 1 (left/deputy): temp 47 → direct (≤ 500)
    let left = &extruder.info[1];
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
    assert_eq!(extruder.info.len(), 2);

    // state 33042: low 4 bits = 2 (count), bits 4-7 = 1 (active = left)
    assert_eq!(extruder.extruder_count(), 2);
    assert_eq!(extruder.active_extruder_index(), 1);

    // id 0: temp 50 (direct, ≤ 500)
    let right = &extruder.info[0];
    let (right_actual, right_target) = right.temperatures();
    assert_eq!(right_actual, 50);
    assert_eq!(right_target, 0);
    assert_eq!(right.z_bias, Some(0.0));

    // id 1: temp 16384250 (composite packed, > 500)
    // 16384250 = 0xFA00FA → target = 250, actual = 250
    let left = &extruder.info[1];
    let (left_actual, left_target) = left.temperatures();
    assert_eq!(left_target, 250);
    assert_eq!(left_actual, 250);
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
fn test_lights_report_deserialization() {
    let json = r#"{
            "print": {
                "lights_report": [
                    { "node": "chamber_light", "mode": "on" },
                    { "node": "work_light", "mode": "flashing" },
                    { "node": "chamber_light2", "mode": "off" }
                ]
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    let lights = print.lights_report.unwrap();
    assert_eq!(lights.len(), 3);
    assert_eq!(lights[0].node, "chamber_light");
    assert_eq!(lights[0].mode, "on");
    assert_eq!(lights[1].node, "work_light");
    assert_eq!(lights[1].mode, "flashing");
}

#[test]
fn test_print_type_and_action_fields() {
    let json = r#"{
            "print": {
                "print_type": "local",
                "print_gcode_action": 0,
                "print_real_action": 0,
                "mc_print_stage": "2",
                "fan_gear": 5373952
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.print_type.as_deref(), Some("local"));
    assert_eq!(print.print_gcode_action, Some(0));
    assert_eq!(print.print_real_action, Some(0));
    assert_eq!(print.mc_print_stage.as_deref(), Some("2"));
    assert_eq!(print.fan_gear, Some(5373952));
}

#[test]
fn test_job_identifiers_and_timing() {
    let json = r#"{
            "print": {
                "task_id": "9012",
                "job_id": "0",
                "remain_time": 549,
                "gcode_start_time": "1681479206",
                "gcode_file_prepare_percent": "100",
                "cali_version": 0
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.task_id.as_deref(), Some("9012"));
    assert_eq!(print.job_id.as_deref(), Some("0"));
    assert_eq!(print.remain_time, Some(549));
    assert_eq!(print.gcode_start_time.as_deref(), Some("1681479206"));
    assert_eq!(print.gcode_file_prepare_percent.as_deref(), Some("100"));
    assert_eq!(print.cali_version, Some(0));
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
fn test_cfg_stg_mapping_fields() {
    let json = r#"{
            "print": {
                "cfg": "3C5FDAD9",
                "stg": [0, 1, 2, 3, 4, 5, 6, 7],
                "mapping": [1]
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.cfg.as_deref(), Some("3C5FDAD9"));
    assert_eq!(print.stg, Some(vec![0, 1, 2, 3, 4, 5, 6, 7]));
    assert_eq!(print.mapping, Some(vec![1]));
}

#[test]
fn test_error_and_failure_fields() {
    let json = r#"{
            "print": {
                "err": "0",
                "fail_reason": "0"
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.err.as_deref(), Some("0"));
    assert_eq!(print.fail_reason.as_deref(), Some("0"));
}

#[test]
fn test_cloud_project_ids() {
    let json = r#"{
            "print": {
                "design_id": "467269",
                "model_id": "US1fccd3bfcb9084",
                "profile_id": "731239480",
                "project_id": "904240393"
            }
        }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.design_id.as_deref(), Some("467269"));
    assert_eq!(print.model_id.as_deref(), Some("US1fccd3bfcb9084"));
    assert_eq!(print.profile_id.as_deref(), Some("731239480"));
    assert_eq!(print.project_id.as_deref(), Some("904240393"));
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
fn test_bed_telemetry_composite_packed() {
    let json = r#"{
            "device": {
                "bed": {
                    "info": { "temp": 4587590 },
                    "state": 2
                },
                "bed_temp": 4587590
            }
        }"#;
    let device = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .device
        .unwrap();
    let bed = device.bed.unwrap();
    assert_eq!(bed.state, Some(2));
    let temp = bed.info.unwrap().temp.unwrap();
    let (actual, target) = PrinterTelemetry::unpack_temperature(temp as f64);
    assert_eq!(actual, 70);
    assert_eq!(target, 70);
    assert_eq!(device.bed_temp, Some(4587590));
}

#[test]
fn test_ext_tool_laser_mounted() {
    let json = r#"{
            "device": {
                "ext_tool": {
                    "calib": 0,
                    "low_prec": false,
                    "mount": 1,
                    "th_temp": 29,
                    "type": "LB00"
                }
            }
        }"#;
    let device = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .device
        .unwrap();
    let ext_tool = device.ext_tool.unwrap();
    assert_eq!(ext_tool.mount, Some(1));
    assert_eq!(ext_tool.tool_type.as_deref(), Some("LB00"));
    assert_eq!(ext_tool.calib, Some(0));
    assert_eq!(ext_tool.low_prec, Some(false));
    assert_eq!(ext_tool.th_temp, Some(29));
}

#[test]
fn test_ext_tool_not_mounted() {
    let json = r#"{
            "device": {
                "ext_tool": { "mount": 0 }
            }
        }"#;
    let ext_tool = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .device
        .unwrap()
        .ext_tool
        .unwrap();
    assert_eq!(ext_tool.mount, Some(0));
    assert!(ext_tool.tool_type.is_none());
}

#[test]
fn test_fire_ext_opaque_value() {
    let json = r#"{
            "device": {
                "fire_ext": { "status": 1, "alarm": false }
            }
        }"#;
    let device = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .device
        .unwrap();
    assert!(device.fire_ext.is_some());
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
        .info[0];
    assert_eq!(nozzle.stat, Some(256));
}

#[test]
fn test_ctc_state_and_target() {
    let json = r#"{
            "device": {
                "ctc": {
                    "info": { "temp": 38, "target": 45 },
                    "state": 2
                }
            }
        }"#;
    let ctc = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .device
        .unwrap()
        .ctc
        .unwrap();
    assert_eq!(ctc.state, Some(2));
    assert_eq!(ctc.info.as_ref().unwrap().temp, Some(38));
    assert_eq!(ctc.info.as_ref().unwrap().target, Some(45));
}

#[test]
fn test_ctc_state_idle() {
    let json = r#"{
            "device": {
                "ctc": {
                    "info": { "temp": 38 },
                    "state": 0
                }
            }
        }"#;
    let ctc = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .device
        .unwrap()
        .ctc
        .unwrap();
    assert_eq!(ctc.state, Some(0));
    assert!(ctc.info.as_ref().unwrap().target.is_none());
}

#[test]
fn test_ipcam_rtsp_url() {
    let json = r#"{
            "print": {
                "ipcam": {
                    "ipcam_dev": "1",
                    "ipcam_record": "enable",
                    "timelapse": "disable",
                    "rtsp_url": "rtsps://192.168.1.64/streaming/live/1"
                }
            }
        }"#;
    let ipcam = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap()
        .ipcam
        .unwrap();
    assert_eq!(
        ipcam.rtsp_url.as_deref(),
        Some("rtsps://192.168.1.64/streaming/live/1")
    );
}

#[test]
fn test_ipcam_rtsp_url_disabled() {
    let json = r#"{
            "print": {
                "ipcam": {
                    "rtsp_url": "disable"
                }
            }
        }"#;
    let ipcam = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap()
        .ipcam
        .unwrap();
    assert_eq!(ipcam.rtsp_url.as_deref(), Some("disable"));
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

    // Verify bitmask parsing: bits 0-3 = AMS type, bits 8-11 = extruder assignment
    let info_val = u64::from_str_radix("11002103", 16).unwrap();
    let ams_type = (info_val & 0xF) as u8;
    let extruder_id = ((info_val >> 8) & 0xF) as u8;
    assert_eq!(ams_type, 3); // AMS Lite type
    assert_eq!(extruder_id, 1); // Left/deputy extruder
}

#[test]
fn test_version_module_extra_fields() {
    let json = r#"{
            "name": "ap2",
            "hw_ver": "AP04",
            "sw_ver": "00.01.02.03",
            "sn": "00W000000000001",
            "project_name": "C12",
            "loader_ver": "00.00.02.04",
            "ota_ver": "01.00.00.00",
            "flag": 0
        }"#;
    let module: crate::types::version::VersionModule = serde_json::from_str(json).unwrap();
    assert_eq!(module.project_name.as_deref(), Some("C12"));
    assert_eq!(module.loader_ver.as_deref(), Some("00.00.02.04"));
    assert_eq!(module.ota_ver.as_deref(), Some("01.00.00.00"));
    assert_eq!(module.flag, Some(0));
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
fn test_progress_field_removed() {
    let json = r#"{ "print": { "mc_percent": 75 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert_eq!(print.mc_percent, Some(75));
}

#[test]
fn test_new_fields_absent_by_default() {
    let json = r#"{ "print": {} }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    assert!(print.print_type.is_none());
    assert!(print.lights_report.is_none());
    assert!(print.hw_switch_state.is_none());
    assert!(print.ams_status.is_none());
    assert!(print.s_obj.is_none());
    assert!(print.vir_slot.is_none());
    assert!(print.fan_gear.is_none());
    assert!(print.task_id.is_none());
    assert!(print.cfg.is_none());
    assert!(print.stg.is_none());
    assert!(print.mapping.is_none());
    assert!(print.err.is_none());
    assert!(print.fail_reason.is_none());
}

#[test]
fn test_h2d_pushall_comprehensive() {
    let json = r#"{
            "print": {
                "gcode_state": "RUNNING",
                "print_type": "local",
                "lights_report": [
                    { "node": "chamber_light", "mode": "on" },
                    { "node": "work_light", "mode": "off" },
                    { "node": "chamber_light2", "mode": "on" }
                ],
                "hw_switch_state": 1,
                "ams_status": 768,
                "task_id": "9012",
                "fan_gear": 5373952,
                "cfg": "3C5FDAD9",
                "stg": [0, 1, 2, 3],
                "vir_slot": [
                    { "id": "254", "tray_type": "PLA", "tray_color": "76D9F4FF", "remain": 0 },
                    { "id": "255", "tray_type": "", "tray_color": "FFFFFF00", "remain": 0 }
                ],
                "ams": {
                    "ams": [
                        { "id": "0", "temp": "26.0", "humidity": "3", "info": "2103", "tray": [] },
                        { "id": "1", "temp": "25.0", "humidity": "4", "info": "2003", "tray": [] }
                    ],
                    "tray_read_done_bits": "ff",
                    "tray_tar": "3",
                    "insert_flag": true,
                    "power_on_flag": false
                },
                "device": {
                    "bed": { "info": { "temp": 4587590 }, "state": 2 },
                    "ext_tool": { "mount": 1, "type": "LB00", "th_temp": 29 },
                    "ctc": { "info": { "temp": 38 }, "state": 0 }
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    let print = report.print.unwrap();

    assert_eq!(print.print_type.as_deref(), Some("local"));
    assert_eq!(print.lights_report.as_ref().unwrap().len(), 3);
    assert_eq!(print.hw_switch_state, Some(1));
    assert_eq!(print.ams_status, Some(768));
    assert_eq!(print.fan_gear, Some(5373952));

    let slots = print.vir_slot.as_ref().unwrap();
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].tray_type.as_deref(), Some("PLA"));

    let ams = print.ams.as_ref().unwrap();
    assert_eq!(ams.tray_read_done_bits.as_deref(), Some("ff"));
    assert_eq!(ams.insert_flag, Some(true));
    assert_eq!(ams.ams[0].info.as_deref(), Some("2103"));
    assert_eq!(ams.ams[1].info.as_deref(), Some("2003"));

    let device = print.device.unwrap();
    let bed = device.bed.unwrap();
    assert_eq!(bed.state, Some(2));
    let bed_temp = bed.info.unwrap().temp.unwrap();
    let (actual, target) = PrinterTelemetry::unpack_temperature(bed_temp as f64);
    assert_eq!(actual, 70);
    assert_eq!(target, 70);

    let ext_tool = device.ext_tool.unwrap();
    assert_eq!(ext_tool.tool_type.as_deref(), Some("LB00"));
}

// --- Phase 21: AmsUnit bitmask accessor tests ---

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

// --- Phase 21: bed_temperatures() accessor tests ---

#[test]
fn test_bed_temperatures_new_gen_top_level() {
    // 4587590 = (70 << 16) | 70
    let json = r#"{
            "device": {
                "bed": { "info": { "temp": 4587590 }, "state": 2 }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.bed_temperatures(), (70, 70));
}

#[test]
fn test_bed_temperatures_new_gen_nested_in_print() {
    let json = r#"{
            "print": {
                "device": {
                    "bed": { "info": { "temp": 3932261 }, "state": 2 }
                }
            }
        }"#;
    // 3932261 = (60 << 16) | 0x0065 = 3932261 → actual=101, target=60? No...
    // Let me calculate: (60 << 16) | 55 = 3932215. Let me use 60/55: (60 << 16) | 55 = 3932215
    // Actually let me just use a known value: (60 << 16) | 55 = 0x3C0037 = 3932215
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    let (actual, target) = report.bed_temperatures();
    // 3932261 = 0x3C0065 → actual = 0x65 = 101, target = 0x3C = 60
    assert_eq!(actual, 101);
    assert_eq!(target, 60);
}

#[test]
fn test_bed_temperatures_old_gen_direct() {
    let json = r#"{
            "print": {
                "bed_temper": 55.5,
                "bed_target_temper": 60.0
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.bed_temperatures(), (55, 60));
}

#[test]
fn test_bed_temperatures_both_present_new_gen_wins() {
    // When both top-level device.bed and print.bed_temper exist, device.bed wins
    let json = r#"{
            "device": {
                "bed": { "info": { "temp": 4587590 }, "state": 2 }
            },
            "print": {
                "bed_temper": 25.0,
                "bed_target_temper": 0.0
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.bed_temperatures(), (70, 70));
}

#[test]
fn test_bed_temperatures_neither_present() {
    let json = r#"{ "print": {} }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.bed_temperatures(), (0, 0));
}

#[test]
fn test_bed_temperatures_empty_report() {
    let json = r#"{}"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert_eq!(report.bed_temperatures(), (0, 0));
}

// --- device() accessor tests (mirrors bed_temperatures()'s fallback pattern) ---

#[test]
fn test_device_top_level() {
    let json = r#"{
            "device": {
                "bed": { "info": { "temp": 4587590 }, "state": 2 }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert!(report.device().is_some());
}

#[test]
fn test_device_nested_in_print() {
    let json = r#"{
            "print": {
                "device": {
                    "bed": { "info": { "temp": 4587590 }, "state": 2 }
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert!(report.device().is_some());
}

#[test]
fn test_device_both_present_top_level_wins() {
    let json = r#"{
            "device": {
                "airduct": { "parts": [{ "id": 1, "state": 1 }] }
            },
            "print": {
                "device": {
                    "airduct": { "parts": [{ "id": 2, "state": 2 }] }
                }
            }
        }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    let device = report.device().unwrap();
    assert_eq!(device.airduct.as_ref().unwrap().parts[0].id, 1);
}

#[test]
fn test_device_neither_present() {
    let json = r#"{ "print": {} }"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert!(report.device().is_none());
}

#[test]
fn test_device_empty_report() {
    let json = r#"{}"#;
    let report: TelemetryReport = serde_json::from_str(json).unwrap();
    assert!(report.device().is_none());
}
