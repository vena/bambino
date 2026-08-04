//! # A2 Series (A2L Bed-Slinger) Quirks & Coordinates
//!
//! The A2L is a large-format open-frame bed-slinger with a 330×320×325mm build volume.

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

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

/// Quirks for the A2L large-format open-frame bed-slinger.
pub struct A2LQuirks;

impl ModelQuirks for A2LQuirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    fn enforces_ftps_tls_1_2(&self) -> bool {
        false
    }

    fn is_door_open(&self, _telemetry: &PrinterTelemetry) -> bool {
        false
    }

    fn has_door_sensor(&self) -> bool {
        false
    }

    fn camera_protocol(&self) -> CameraProtocol {
        CameraProtocol::BinaryJpeg
    }

    fn ignores_chamber_temperature(&self) -> bool {
        true
    }

    fn has_stg_cur_idle_bug(&self) -> bool {
        false
    }

    fn active_chamber_heater_max_temp_c(&self) -> Option<u16> {


        None


    }

    fn physical_nozzle_count(&self) -> u8 {
        1
    }

    fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition {
        crate::ams::AmsPoolComposition::Shared { max_units: 4 }
    }

    fn supports_nozzle_offset_calibration(&self) -> bool {
        false
    }

    fn is_bed_on_z(&self) -> bool {
        false
    }

    fn z_max(&self) -> f32 {
        A2L_Z_MAX
    }

    fn x_max(&self) -> f32 {
        A2L_X_MAX
    }

    fn y_max(&self) -> f32 {
        A2L_Y_MAX
    }

    fn nozzle_temp_max(&self) -> u16 {
        A2L_NOZZLE_TEMP_MAX
    }

    fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16 {
        A2L_BED_TEMP_MAX
    }

    fn supports_prompt_sound(&self) -> bool {
        true
    }

    fn supports_auxiliary_left_fan(&self) -> bool {
        false
    }
}
