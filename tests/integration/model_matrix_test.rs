//! `MODEL_MATRIX.csv` and the quirks rows state the same physical facts; this keeps them equal.
//!
//! The CSV is where confirmed model facts are recorded, and the quirks constants cite it, but an
//! edit to either side doesn't touch the other. Every row this test understands is compared for
//! every supported model; an `Unconfirmed` cell is skipped, not treated as a mismatch.

use bambino::PrinterModel;
use bambino::camera::CameraProtocol;
use bambino::models::supported_models;
use bambino::quirks::{BuildVolume, DoorSensor, ModelQuirks, QuirkContext};

const MATRIX: &str = include_str!("../../MODEL_MATRIX.csv");

/// Parses RFC 4180-style CSV: quoted fields may hold commas and newlines, `""` escapes a quote.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => row.push(core::mem::take(&mut field)),
            ('\n', false) => {
                row.push(core::mem::take(&mut field));
                rows.push(core::mem::take(&mut row));
            }
            ('\r', false) => {}
            (c, _) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

/// The CSV cell for (`row`, `model`), or `None` when it is `Unconfirmed`.
fn cell<'a>(matrix: &'a [Vec<String>], row: &str, model: PrinterModel) -> Option<&'a str> {
    let col = matrix[0]
        .iter()
        .position(|h| h == model.display_name())
        .unwrap_or_else(|| panic!("no MODEL_MATRIX.csv column for {model}"));
    let line = matrix
        .iter()
        .find(|r| r[0] == row)
        .unwrap_or_else(|| panic!("no MODEL_MATRIX.csv row {row:?}"));
    let value = line[col].trim();
    (!value.starts_with("Unconfirmed")).then_some(value)
}

/// Leading integer of a cell like `"300°C"` or `"110°C (…)"`.
fn leading_u16(value: &str) -> u16 {
    let digits: String = value.chars().take_while(char::is_ascii_digit).collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("no leading number in {value:?}"))
}

/// `"Yes"` → true; `"No"`, `"N/A"` and `"Optional…"` (not guaranteed present) → false.
fn yes(value: &str) -> bool {
    value.starts_with("Yes")
}

/// The build volume the quirks use: the plain `X*Y*Z` cell, or its `Dual Nozzle:` line on a
/// multi-nozzle model (the conservative envelope every nozzle can reach).
fn build_volume(value: &str) -> BuildVolume {
    let line = value
        .lines()
        .find(|l| l.starts_with("Dual Nozzle:"))
        .map_or(value, |l| l.trim_start_matches("Dual Nozzle:"));
    let dims: Vec<f32> = line
        .trim()
        .trim_end_matches(" mm³")
        .split('*')
        .map(|d| {
            d.parse()
                .unwrap_or_else(|_| panic!("bad dimension in {value:?}"))
        })
        .collect();
    BuildVolume {
        x: dims[0],
        y: dims[1],
        z: dims[2],
    }
}

/// `"2 Slots, up to 7 active nozzles"` → 7, else the leading count.
fn nozzle_count(value: &str) -> u8 {
    match value.split_once("up to ") {
        Some((_, rest)) => leading_u16(rest) as u8,
        None => leading_u16(value) as u8,
    }
}

fn check(model: PrinterModel, q: &ModelQuirks, matrix: &[Vec<String>]) {
    let get = |row| cell(matrix, row, model);

    if let Some(v) = get("Build Volume") {
        assert_eq!(q.build_volume(), build_volume(v), "{model} build volume");
    }
    if let Some(v) = get("# Nozzles") {
        assert_eq!(
            q.physical_nozzle_count(),
            nozzle_count(v),
            "{model} nozzles"
        );
    }
    if let Some(v) = get("Max Hot End Temperature") {
        assert_eq!(q.nozzle_temp_max(), leading_u16(v), "{model} nozzle max");
    }
    if let Some(v) = get("Max Build Plate Temperature") {
        // The cell's leading value is the ceiling with the mains region unknown.
        assert_eq!(q.bed_temp_max(None), leading_u16(v), "{model} bed max");
    }
    if let (Some(heater), Some(max)) = (get("Chamber Heater"), get("Max Chamber Temperature")) {
        let expected = yes(heater).then(|| leading_u16(max));
        assert_eq!(
            q.chamber_heater_temp_max(),
            expected,
            "{model} chamber heater"
        );
    }
    if let Some(v) = get("Door Sensor") {
        assert_eq!(
            q.door_sensor() != DoorSensor::None,
            yes(v),
            "{model} door sensor"
        );
    }
    if let Some(v) = get("Aux Part Cooling Fan") {
        assert_eq!(q.has_auxiliary_left_fan(), yes(v), "{model} aux fan");
    }
    if let Some(v) = get("Chamber Exhaust Fan") {
        assert_eq!(q.has_chamber_exhaust_fan(), yes(v), "{model} exhaust fan");
    }
    if let Some(v) = get("Airduct Damper Control (Adaptive Airflow System)") {
        assert_eq!(q.supports_airduct_mode(), yes(v), "{model} airduct");
    }
    if let Some(v) = get("Prompt Sound (Speaker)") {
        // The model rule, before any report: an empty context.
        let support = q.prompt_sound_support(&QuirkContext::empty());
        assert_eq!(support.is_supported(), yes(v), "{model} prompt sound");
    }
    if let Some(v) = get("Fire Alarm Buzzer") {
        assert_eq!(q.has_buzzer(), yes(v), "{model} buzzer");
    }
    if let Some(v) = get("Video Streaming Protocol") {
        let expected = if v == "RTSPS" {
            CameraProtocol::Rtsps
        } else {
            CameraProtocol::BinaryJpeg
        };
        assert_eq!(q.camera_protocol(), expected, "{model} camera");
    }
}

#[test]
fn test_model_matrix_matches_quirks() {
    let matrix = parse_csv(MATRIX);
    let mut checked = 0;
    for model in supported_models() {
        check(model, model.quirks(), &matrix);
        checked += 1;
    }
    assert_eq!(
        checked,
        matrix[0].len() - 1,
        "a CSV column has no supported model"
    );
}
