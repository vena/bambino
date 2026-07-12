use super::*;

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
fn test_is_220v_power_from_home_flag() {
    let json_220v = r#"{ "print": { "home_flag": 8 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_220v)
        .expect("valid json")
        .print
        .expect("print present");
    assert!(print.is_220v_power());

    let json_110v = r#"{ "print": { "home_flag": 0 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_110v)
        .expect("valid json")
        .print
        .expect("print present");
    assert!(!print.is_220v_power());

    let json_missing = r#"{ "print": {} }"#;
    let print = serde_json::from_str::<TelemetryReport>(json_missing)
        .expect("valid json")
        .print
        .expect("print present");
    assert!(!print.is_220v_power());
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

#[test]
fn test_device_telemetry_merge_from_preserves_absent_sub_objects() {
    // BUG-093: a `device` push touching only `ctc` must not wipe the previously-cached
    // `nozzle`/`extruder`/`airduct` sub-objects sitting alongside it.
    let mut cached = DeviceTelemetry {
        nozzle: Some(NozzleCollection {
            info: vec![NozzleInfo {
                id: 0,
                diameter: Some(0.4),
                tm: None,
                max_temp: None,
                nozzle_type: None,
                wear: None,
                serial_number: None,
                sn: None,
                filament_colour: None,
                color_m: None,
                filament_id: None,
                fila_id: None,
                stat: None,
            }],
            exist: Some(1),
            state: None,
            src_id: None,
            tar_id: None,
        }),
        extruder: None,
        airduct: None,
        ctc: None,
        bed: None,
        ext_tool: None,
        fire_ext: None,
        bed_temp: None,
    };

    let partial = DeviceTelemetry {
        nozzle: None,
        extruder: None,
        airduct: None,
        ctc: Some(CtcTelemetry {
            info: None,
            state: Some(2),
        }),
        bed: None,
        ext_tool: None,
        fire_ext: None,
        bed_temp: None,
    };

    cached.merge_from(&partial);

    assert!(
        cached.nozzle.is_some(),
        "nozzle must survive a ctc-only partial push"
    );
    assert_eq!(cached.nozzle.as_ref().unwrap().info.len(), 1);
    assert_eq!(cached.ctc.unwrap().state, Some(2), "new field applies");
}

#[test]
fn test_nozzle_collection_merge_from_preserves_info_on_absence() {
    // BUG-094: confirmed via pybambu/bambuddy — device.nozzle.info can be absent while
    // sibling nozzle fields (e.g. exist) change.
    let mut cached = NozzleCollection {
        info: vec![NozzleInfo {
            id: 0,
            diameter: Some(0.4),
            tm: None,
            max_temp: None,
            nozzle_type: None,
            wear: None,
            serial_number: None,
            sn: None,
            filament_colour: None,
            color_m: None,
            filament_id: None,
            fila_id: None,
            stat: None,
        }],
        exist: Some(1),
        state: None,
        src_id: None,
        tar_id: None,
    };

    let partial = NozzleCollection {
        info: vec![],
        exist: Some(3),
        state: None,
        src_id: None,
        tar_id: None,
    };

    cached.merge_from(&partial);

    assert_eq!(cached.info.len(), 1, "info array must survive a partial push");
    assert_eq!(cached.exist, Some(3), "new field applies");
}

#[test]
fn test_extruder_collection_merge_from_preserves_info_on_absence() {
    let mut cached = ExtruderCollection {
        info: vec![ExtruderInfo {
            id: 0,
            temp: Some(16056565),
            snow: None,
            spre: None,
            star: None,
            hnow: None,
            hpre: None,
            htar: None,
            stat: None,
            info: None,
            filam_bak: vec![],
            z_bias: None,
        }],
        state: Some(2),
    };

    let partial = ExtruderCollection {
        info: vec![],
        state: Some(3),
    };

    cached.merge_from(&partial);

    assert_eq!(cached.info.len(), 1, "info array must survive a partial push");
    assert_eq!(cached.state, Some(3), "new field applies");
}

#[test]
fn test_airduct_collection_merge_from_preserves_fields_independently() {
    // BUG-094: confirmed via bambuddy — device.airduct.modeCur can change while parts/modeList
    // are absent from that same push, and vice versa.
    let mut cached = AirductCollection {
        parts: vec![AirductPart {
            id: 160,
            state: Some(50),
        }],
        mode_cur: Some(0),
        mode_list: vec![AirductModeListEntry { mode_id: 0 }],
    };

    let partial = AirductCollection {
        parts: vec![],
        mode_cur: Some(1),
        mode_list: vec![],
    };

    cached.merge_from(&partial);

    assert_eq!(cached.parts.len(), 1, "parts must survive a mode_cur-only push");
    assert_eq!(cached.mode_list.len(), 1, "mode_list must survive a mode_cur-only push");
    assert_eq!(cached.mode_cur, Some(1), "new field applies");
}

#[test]
fn test_ctc_telemetry_merge_from_preserves_info_on_absence() {
    // BUG-096: confirmed via BambuStudio's DevChamber::ParseChamberV2_0 — device.ctc.info can
    // be absent while device.ctc.state is present (and changes) in the same push.
    let mut cached = CtcTelemetry {
        info: Some(CtcInfo {
            temp: Some(1900000),
            target: Some(30),
        }),
        state: Some(0),
    };

    let partial = CtcTelemetry {
        info: None,
        state: Some(2),
    };

    cached.merge_from(&partial);

    assert!(cached.info.is_some(), "info must survive a state-only push");
    assert_eq!(cached.info.unwrap().target, Some(30));
    assert_eq!(cached.state, Some(2), "new field applies");
}

#[test]
fn test_ctc_info_merge_from_preserves_target_on_absence() {
    // target is a real, independently-arriving wire key per bambuddy's own
    // `if "target" in ctc_info:` guard — a ctc.info push repeating only temp must not wipe
    // a previously-cached target.
    let mut cached = CtcInfo {
        temp: Some(1900000),
        target: Some(30),
    };

    let partial = CtcInfo {
        temp: Some(2000000),
        target: None,
    };

    cached.merge_from(&partial);

    assert_eq!(cached.temp, Some(2000000), "new field applies");
    assert_eq!(cached.target, Some(30), "target must survive a temp-only push");
}

#[test]
fn test_bed_telemetry_merge_from_preserves_info_on_absence() {
    // BUG-095: confirmed via BambuStudio's json_diff::restore_objects generic reconstruction
    // layer — device.bed.info can be absent while device.bed.state is present (and changes)
    // in the same push, same shape as BUG-096's device.ctc.info/.state.
    let mut cached = BedTelemetry {
        info: Some(BedInfo { temp: Some(1900000) }),
        state: Some(0),
    };

    let partial = BedTelemetry {
        info: None,
        state: Some(2),
    };

    cached.merge_from(&partial);

    assert!(cached.info.is_some(), "info must survive a state-only push");
    assert_eq!(cached.info.unwrap().temp, Some(1900000));
    assert_eq!(cached.state, Some(2), "new field applies");
}

#[test]
fn test_ext_tool_telemetry_merge_from_preserves_fields_independently() {
    // BUG-097: confirmed via BambuStudio's DevExtensionToolParser::ParseV2_0 — mount_3d/calib
    // (ParseVal with current-value default) and type/tool_type (unrecognized/absent falls
    // through without writing) all preserve on absence.
    let mut cached = ExtToolTelemetry {
        mount: Some(1),
        tool_type: Some("LB00".into()),
        calib: Some(1),
        low_prec: Some(false),
        th_temp: Some(45),
        mount_3d: Some(0),
    };

    let partial = ExtToolTelemetry {
        mount: None,
        tool_type: None,
        calib: None,
        low_prec: None,
        th_temp: Some(50),
        mount_3d: None,
    };

    cached.merge_from(&partial);

    assert_eq!(cached.mount, Some(1), "mount must survive a th_temp-only push");
    assert_eq!(cached.tool_type.as_deref(), Some("LB00"));
    assert_eq!(cached.calib, Some(1));
    assert_eq!(cached.low_prec, Some(false));
    assert_eq!(cached.th_temp, Some(50), "new field applies");
    assert_eq!(cached.mount_3d, Some(0));
}
