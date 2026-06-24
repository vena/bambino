//! # X2 Series (X2D CoreXY) Quirks
//!
//! Handles parameters unique to the X2D dual-carriage auxiliary-cooling model.

use crate::quirks::ModelQuirks;
use crate::types::PrintTelemetry;

pub struct X2Quirks;

impl ModelQuirks for X2Quirks {
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
        false
    }

    fn physical_nozzle_count(&self) -> u8 {
        2
    }

    fn supports_nozzle_offset_calibration(&self) -> bool {
        true
    }

    fn is_bed_on_z(&self) -> bool {
        true
    }

    fn supports_auxiliary_right_fan(&self) -> bool {
        true
    }

    fn auxiliary_fan_uses_percentage(&self) -> bool {
        true
    }
}
