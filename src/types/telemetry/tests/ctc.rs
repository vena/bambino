use super::*;

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
