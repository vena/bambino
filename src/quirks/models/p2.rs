//! # P2 Series (P2S CoreXY) Quirks
//!
//! Configures transport parameters, thermal layouts, and camera corrections for the P2S platform.

use crate::quirks::ModelQuirks;
use crate::types::PrintTelemetry;

pub struct P2Quirks;

impl ModelQuirks for P2Quirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
        true
    }

    fn is_door_open(&self, telemetry: &PrintTelemetry) -> bool {
        telemetry.is_door_open_from_stat()
    }

    fn has_door_sensor(&self) -> bool {
        true
    }

    fn camera_stream_port(&self) -> u16 {
        322
    }

    fn ignores_chamber_temperature(&self) -> bool {
        false
    }

    fn has_stg_cur_idle_bug(&self) -> bool {
        false
    }

    fn has_active_chamber_heater(&self) -> bool {
        true
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
}
