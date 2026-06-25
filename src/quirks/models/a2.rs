//! # A2 Series (A2L Bed-Slinger) Quirks & Coordinates
//!
//! The A2L is a large-format open-frame bed-slinger with a 330×320×325mm build volume.

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

pub const A2L_Z_MAX: f32 = 325.0;
pub const A2L_NOZZLE_TEMP_MAX: u16 = 300;
pub const A2L_BED_TEMP_MAX: u16 = 80;

pub struct A2LQuirks;

impl ModelQuirks for A2LQuirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
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

    fn has_active_chamber_heater(&self) -> bool {
        false
    }

    fn physical_nozzle_count(&self) -> u8 {
        1
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

    fn nozzle_temp_max(&self) -> u16 {
        A2L_NOZZLE_TEMP_MAX
    }

    fn bed_temp_max(&self) -> u16 {
        A2L_BED_TEMP_MAX
    }
}
