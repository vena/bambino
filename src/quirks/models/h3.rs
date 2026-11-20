//! # H3 Series (H3C) Quirks
//!
//! The H3C is a CoreXY tool changer with a nine-dock hotend rack and no fixed hotend, so every
//! hotend is a rack position (ids `16..=24`, [`ModelQuirks::rack_nozzle_ids`]). Its facts come
//! from BambuStudio's `resources/printers/N10.json` and its `Bambu Lab H3C 0.4 nozzle.json`
//! machine profile, plus ha-bambulab's `MOCK-H3C.json` capture; see `MODEL_MATRIX.csv`.

use crate::ams::AmsPoolComposition;
use crate::camera::CameraProtocol;
use crate::quirks::{
    BedMax, BuildVolume, DoorSensor, DryRule, FEEDRATE_30_MM_S, FEEDRATE_1000_MM_S, ModelQuirks,
    NozzleLayout, SafetyLimits,
};

/// H3C build volume X (mm), `printable_area` of BambuStudio's `Bambu Lab H3C 0.4 nozzle.json`.
pub const H3C_X_MAX: f32 = 330.0;
/// H3C build volume Y (mm), `printable_area` of BambuStudio's `Bambu Lab H3C 0.4 nozzle.json`.
pub const H3C_Y_MAX: f32 = 320.0;
/// H3C build volume Z (mm), `printable_height` of BambuStudio's `Bambu Lab H3C 0.4 nozzle.json`.
pub const H3C_Z_MAX: f32 = 325.0;
/// H3C nozzle temperature ceiling (°C), `nozzle_temp_range` in BambuStudio's `N10.json`.
pub const H3C_NOZZLE_TEMP_MAX: u16 = 350;
/// H3C bed temperature ceiling (°C), `bed_temp_range` in BambuStudio's `N10.json`.
pub const H3C_BED_TEMP_MAX: u16 = 120;
/// H3C chamber heater ceiling (°C), `support_chamber_temp_edit_range` in BambuStudio's `N10.json`.
pub const H3C_CHAMBER_TEMP_MAX: u16 = 65;
/// Rack docks on the H3C, `default_nozzle_count` in BambuStudio's `N10.json`.
pub const H3C_RACK_DOCKS: u8 = 9;

/// H3C: CoreXY tool changer with a nine-dock hotend rack and no fixed hotend.
///
/// - **Feedrates** are the shared 1000 mm/s XY and 30 mm/s Z constants, below the machine
///   profile's `machine_max_speed_x`/`_y` 1400 and `_z` 35, which are not adopted.
/// - **`door`** is presumed `Stat` like the other newer models; no capture with the door open exists.
/// - **AMS pool** is `Independent { 0, 0 }` because external AMS support on the H3C is unrecorded.
///   Its built-in AMS 3H (unit 32) is a separate matter.
/// - **Drying** is `Unstated`: no vendor source states the H3C's drying release, so no firmware
///   version gates it. A reported `fun2` bit 5 decides; unreported, idle remote drying is assumed
///   allowed and drying while printing denied, as for any `Unstated` model.
/// - **`buzzer`** is false: no source records one.
/// - **Fans** are the two auxiliary fans (port 2 right, port 10 left, `N10.json`). `N10.json`
///   has no port-3 fan, so `chamber_exhaust_fan` is false. Its port-15 "Chamber" fan has no
///   `FanTarget`.
pub(crate) const H3C: ModelQuirks = ModelQuirks {
    sliced_for: &["N10"],
    door: DoorSensor::Stat,
    chamber_temperature_sensor: true,
    nozzles: NozzleLayout::Rack {
        docks: H3C_RACK_DOCKS,
        fixed_hotend: false,
    },
    airduct_mode: true,
    store_sent_files: true,
    ai_monitoring: true,
    auxiliary_fan: true,
    auxiliary2_fan: true,
    chamber_exhaust_fan: false,
    auto_flow_calibration: true,
    buzzer: false,
    ..ModelQuirks::new(
        SafetyLimits {
            volume: BuildVolume {
                x: H3C_X_MAX,
                y: H3C_Y_MAX,
                z: H3C_Z_MAX,
            },
            nozzle_temp_max: H3C_NOZZLE_TEMP_MAX,
            bed_temp_max: BedMax::Flat(H3C_BED_TEMP_MAX),
            chamber_heater_temp_max: Some(H3C_CHAMBER_TEMP_MAX),
            bed_on_z: true,
            max_feedrate_xy: FEEDRATE_1000_MM_S,
            max_feedrate_z: FEEDRATE_30_MM_S,
        },
        CameraProtocol::Rtsps,
        // External AMS support on the H3C is unrecorded; the built-in AMS 3H (unit 32) is
        // modelled separately (#668).
        AmsPoolComposition::Independent {
            max_standard: 0,
            max_ht: 0,
        },
        DryRule::Unstated,
    )
};
