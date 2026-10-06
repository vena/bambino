use super::merge::Mergeable;
use super::*;

/// Parses a fixture into its `print` object.
fn print(json: &str) -> PrinterTelemetry {
    serde_json::from_str::<TelemetryReport>(json)
        .expect("fixture parses")
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
