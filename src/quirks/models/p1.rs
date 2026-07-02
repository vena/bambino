//! # P1 Series (P1P & P1S CoreXY) Quirks
//!
//! Tracks constraints and kinematic properties of early and enclosed low-power RTOS machines.

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

pub const P1_Z_MAX: f32 = 256.0;
pub const P1_NOZZLE_TEMP_MAX: u16 = 300;
pub const P1_BED_TEMP_MAX: u16 = 100;

pub struct P1Quirks;

impl ModelQuirks for P1Quirks {
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
        true
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
        true
    }

    fn z_max(&self) -> f32 {
        P1_Z_MAX
    }

    fn nozzle_temp_max(&self) -> u16 {
        P1_NOZZLE_TEMP_MAX
    }

    fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16 {
        P1_BED_TEMP_MAX
    }
}
