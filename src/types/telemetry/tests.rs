use super::merge::Mergeable;
use super::*;

/// Parses a fixture as a whole report.
fn parse_report(json: &str) -> TelemetryReport {
    serde_json::from_str(json).expect("fixture parses")
}

/// Parses a fixture into its `print` object.
fn parse_print(json: &str) -> PrinterTelemetry {
    parse_report(json)
        .print
        .expect("fixture has a print object")
}

/// A heater reading, for comparing against the `Option<HeaterTemps>` accessors.
fn temps(actual: u16, target: u16) -> Option<HeaterTemps> {
    Some(HeaterTemps { actual, target })
}

/// `(id, actual, target)` per nozzle, for compact comparisons.
fn nozzle_tuples(temps: &[NozzleTemps]) -> Vec<(u8, u16, u16)> {
    temps.iter().map(|t| (t.id, t.actual, t.target)).collect()
}

#[path = "tests/ams.rs"]
mod ams;
#[path = "tests/bed.rs"]
mod bed;
#[path = "tests/ctc.rs"]
mod ctc;
#[path = "tests/device.rs"]
mod device;
#[path = "tests/fun_field.rs"]
mod fun_field;
#[path = "tests/misc.rs"]
mod misc;
#[path = "tests/nozzle.rs"]
mod nozzle;

#[test]
fn test_print_key_count_counts_only_the_print_object() {
    let payload = br#"{"print":{"home_flag":1,"cfg":"0","nested":{"a":1,"b":2}},"info":{"x":1}}"#;
    assert_eq!(print_key_count(payload), Some(3));
    assert_eq!(print_key_count(br#"{"system":{"a":1}}"#), None);
    assert_eq!(print_key_count(b"not json"), None);
}
