//! # H2 Series (H2S, H2D, H2C IDEX and Tool Changer platforms) Quirks
//!
//! Manages the properties and kinematic characteristics of the IDEX and
//! tool-changer platforms [REF-MOTO-GCODE].

use crate::quirks::ModelQuirks;
use crate::types::PrintTelemetry;

#[cfg(not(feature = "std"))]
use alloc::string::String;

pub struct H2Quirks;

impl ModelQuirks for H2Quirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
        false
    }

    fn is_door_open(&self, telemetry: &PrintTelemetry) -> bool {
        telemetry.is_door_open(false)
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
        // Overridden dynamically based on carriage count parameters in parent loops
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

    fn is_unsafe_homing_command(&self, gcode: &str) -> bool {
        let clean = gcode.to_uppercase();
        clean.contains("G28") && (clean.contains('Z') || clean.contains('X') || clean.contains('Y'))
    }

    fn relative_z_move_gcode(&self, _distance: f32, _feedrate: u32) -> String {
        String::from(super::super::DEFAULT_Z_MOVE_GCODE)
    }

    fn is_unsupported_command(&self, _command: &str) -> bool {
        false
    }
}
