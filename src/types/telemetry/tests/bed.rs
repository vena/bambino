use super::*;

#[test]
fn test_chamber_temper_composite_packed_via_unpack_temperature() {
    // No test previously exercised a real chamber_temper wire value through
    // unpack_temperature() specifically — only generic composite values. Deserializes an
    // actual print.chamber_temper field carrying a composite-packed (>500) value and confirms
    // it decodes correctly: target=45, actual=38 -> (45 << 16) | 38 = 2949158.
    let json = r#"{ "print": { "chamber_temper": 2949158.0 } }"#;
    let print = serde_json::from_str::<TelemetryReport>(json)
        .unwrap()
        .print
        .unwrap();
    let (actual, target) = PrinterTelemetry::unpack_temperature(print.chamber_temper.unwrap());
    assert_eq!(actual, 38);
    assert_eq!(target, 45);
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
fn test_bed_temperatures_ignores_device_bed_temp() {
    // device.bed_temp is confirmed-redundant wire data that
    // decode_bed_temperatures() must never fall back to. Set it to a value that would decode
    // to a visibly different (10, 20) pair if it were consulted, and confirm the result still
    // comes from device.bed.info.temp (70, 70) instead.
    let json = r#"{
            "device": {
                "bed": { "info": { "temp": 4587590 }, "state": 2 },
                "bed_temp": 655380
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
