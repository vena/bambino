//! # A2 Series (A2L Bed-Slinger) Quirks & Coordinates
//!
//! The A2L is a large-format open-frame bed-slinger with a 330×320×325mm build volume.

use crate::ams::{AmsLiteSlot, AmsPoolComposition};
use crate::camera::CameraProtocol;
use crate::quirks::{BedMax, BuildVolume, DryRule, ModelQuirks, SafetyLimits};

/// A2L build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm).
pub const A2L_Z_MAX: f32 = 325.0;
/// A2L build volume X width (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm).
pub const A2L_X_MAX: f32 = 330.0;
/// A2L build volume Y depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row (330×320×325mm).
pub const A2L_Y_MAX: f32 = 320.0;
/// Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const A2L_NOZZLE_TEMP_MAX: u16 = 300;
/// Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const A2L_BED_TEMP_MAX: u16 = 80;

/// A2L large-format open-frame bed-slinger.
///
/// **Drying: always.** Its earliest published release, A2L `01.01.00.00` (2026-06-01,
/// <https://wiki.bambulab.com/en/a2l/manual/a2l-firmware-release-history>), "Added support for
/// remote activation of filament drying" and "Print While Drying". The *Filament drying guide for
/// AMS 2 Pro and AMS HT* gives the same minimum in both lists, as does bambuddy's
/// `_DRY_WHILE_PRINTING_MIN_FIRMWARE`.
pub(crate) const A2L: ModelQuirks = ModelQuirks {
    prompt_sound: true,
    auxiliary_left_fan: false,
    ..ModelQuirks::new(
        SafetyLimits {
            volume: BuildVolume {
                x: A2L_X_MAX,
                y: A2L_Y_MAX,
                z: A2L_Z_MAX,
            },
            nozzle_temp_max: A2L_NOZZLE_TEMP_MAX,
            bed_temp_max: BedMax::Flat(A2L_BED_TEMP_MAX),
            chamber_heater_temp_max: None,
            bed_on_z: false,
        },
        CameraProtocol::BinaryJpeg,
        // A shared pool of 4 plus one AMS Lite at the same time (`MODEL_MATRIX.csv`).
        AmsPoolComposition::Shared {
            max_units: 4,
            ams_lite: AmsLiteSlot::Additive,
        },
        DryRule::Always,
    )
};
