//! # X1 Series (X1C, X1E CoreXY) Quirks
//!
//! Implements hardware safety guidelines and thermal parameters for the premium CoreXY platforms.
//! X1C and X1E share all behavior except active chamber heater support (X1E only).

use crate::quirks::ModelQuirks;
use crate::types::PrintTelemetry;

pub struct X1CQuirks;
pub struct X1EQuirks;

fn x1_is_door_open(telemetry: &PrintTelemetry) -> bool {
    telemetry.is_door_open_from_home_flag()
}

impl ModelQuirks for X1CQuirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
        false
    }

    fn is_door_open(&self, telemetry: &PrintTelemetry) -> bool {
        x1_is_door_open(telemetry)
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
        1
    }

    fn supports_nozzle_offset_calibration(&self) -> bool {
        false
    }

    fn is_bed_on_z(&self) -> bool {
        true
    }
}

impl ModelQuirks for X1EQuirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
        false
    }

    fn is_door_open(&self, telemetry: &PrintTelemetry) -> bool {
        x1_is_door_open(telemetry)
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
}
