//! # A1 Series (A1 & A1 Mini Bed-Slingers) Quirks & Coordinates
//!
//! Kinematics, safety boundaries, and mechanical constraints of the A1 bed-slinger family
//! [REF-MOTO-GCODE].
//!
//! - A1: 256×256×256mm build volume
//! - A1 Mini: 180×180×180mm build volume

use crate::ams::{AmsLiteSlot, AmsPoolComposition};
use crate::camera::CameraProtocol;
use crate::quirks::{BedMax, BuildVolume, DryRule, ModelQuirks, SafetyLimits};

/// A1 build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.
pub const A1_Z_MAX: f32 = 256.0;
/// A1 Mini build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.
pub const A1_MINI_Z_MAX: f32 = 180.0;
/// Nozzle temperature ceiling (°C) shared by A1 and A1 Mini, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const A1_NOZZLE_TEMP_MAX: u16 = 300;
/// A1 bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const A1_BED_TEMP_MAX: u16 = 100;
/// A1 Mini bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const A1_MINI_BED_TEMP_MAX: u16 = 80;

/// The A1 family's shared row: plaintext FTPS data channel, binary JPEG camera, prompt speaker,
/// no aux fan, no door or chamber sensor.
///
/// **Drying: never.** No known firmware path on the A1 series exposes a remote-dry command. Bambu
/// Lab's *Filament drying guide for AMS 2 Pro and AMS HT* lists A1/A1 mini as "not supported yet"
/// for both remote drying and simultaneous drying and printing, and bambuddy lists both in
/// `_DRYING_UNSUPPORTED_MODELS` (`printer_manager.py`). **Not a hardware limit**: the A1
/// series takes AMS 2 Pro and AMS-HT units from a shared pool of 4 (`MODEL_MATRIX.csv`,
/// `reference/05_materials_ams.md`). The A1 family sends no `fun2`, so the reported-bit stage
/// never engages, but it is honored like every other model's.
const fn a1(side: f32, bed_max: u16) -> ModelQuirks {
    ModelQuirks {
        plaintext_ftps_data_channel: true,
        prompt_sound: true,
        auxiliary_left_fan: false,
        ..ModelQuirks::new(
            SafetyLimits {
                volume: BuildVolume::cube(side),
                nozzle_temp_max: A1_NOZZLE_TEMP_MAX,
                bed_temp_max: BedMax::Flat(bed_max),
                chamber_heater_temp_max: None,
                bed_on_z: false,
            },
            CameraProtocol::BinaryJpeg,
            // A shared pool of 4, or one AMS Lite instead (`MODEL_MATRIX.csv`).
            AmsPoolComposition::Shared {
                max_units: 4,
                ams_lite: AmsLiteSlot::Exclusive,
            },
            DryRule::Never,
        )
    }
}

/// Full-size A1 bed-slinger.
pub(crate) const A1: ModelQuirks = a1(A1_Z_MAX, A1_BED_TEMP_MAX);
/// A1 Mini bed-slinger (same family, smaller build volume and bed ceiling).
pub(crate) const A1_MINI: ModelQuirks = a1(A1_MINI_Z_MAX, A1_MINI_BED_TEMP_MAX);
