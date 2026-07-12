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
fn test_p1s_wire_capture_end_to_end() {
    let json_data = include_str!("../../../../tests/mocks/P1S.json");
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
    let unit_tray = unit.tray.as_ref().unwrap();
    assert_eq!(unit_tray.len(), 4);
    assert_eq!(unit_tray[0].id, "0");
    assert_eq!(unit_tray[3].id, "3");

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
fn test_progress_field_removed() {
    // BUG-067: the legacy "progress" wire field was removed from PrintTelemetry — assert a
    // stray "progress" key in incoming JSON is silently ignored on deserialize rather than
    // erroring, instead of the previous verbatim duplicate of test_mc_percent_deserialization,
    // which asserted nothing about "progress" at all.
    let json = r#"{ "print": { "mc_percent": 75, "progress": 75 } }"#;
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
