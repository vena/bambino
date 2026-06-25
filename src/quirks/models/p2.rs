//! # P2 Series (P2S CoreXY) Quirks
//!
//! Configures transport parameters, thermal layouts, and camera corrections for the P2S platform.

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

pub const P2S_Z_MAX: f32 = 256.0;
pub const P2S_NOZZLE_TEMP_MAX: u16 = 300;
pub const P2S_BED_TEMP_MAX: u16 = 110;

pub struct P2Quirks;

impl ModelQuirks for P2Quirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
        true
    }

    fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool {
        telemetry.is_door_open_from_stat()
    }

    fn has_door_sensor(&self) -> bool {
        true
    }

    fn camera_protocol(&self) -> CameraProtocol {
        CameraProtocol::Rtsps
    }

    fn ignores_chamber_temperature(&self) -> bool {
        false
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
        true
    }

    fn requires_wallclock_rtsp_timestamps(&self) -> bool {
        true
    }

    fn supports_auxiliary_right_fan(&self) -> bool {
        true
    }

    fn auxiliary_fan_uses_percentage(&self) -> bool {
        true
    }

    fn z_max(&self) -> f32 {
        P2S_Z_MAX
    }

    fn nozzle_temp_max(&self) -> u16 {
        P2S_NOZZLE_TEMP_MAX
    }

    fn bed_temp_max(&self) -> u16 {
        P2S_BED_TEMP_MAX
    }
}
