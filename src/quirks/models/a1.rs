//! # A1 Series (A1 & A1 Mini Bed-Slingers) Quirks & Coordinates
//!
//! Handles the kinematics, safety boundaries, and mechanical constraints of the
//! bed-slinger family [REF-MOTO-GCODE].

use crate::quirks::ModelQuirks;
use crate::types::PrintTelemetry;

#[cfg(not(feature = "std"))]
use alloc::string::String;

/// Maximum physical workspace dimension boundaries (in millimeters)
pub const MAX_X: f32 = 256.0;
pub const MAX_Y: f32 = 256.0;
pub const MAX_Z: f32 = 256.0;

/// Mini model specific workspace boundaries
pub const MINI_MAX_X: f32 = 180.0;
pub const MINI_MAX_Y: f32 = 180.0;
pub const MINI_MAX_Z: f32 = 180.0;

pub struct A1Quirks;

impl ModelQuirks for A1Quirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        true
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
        false
    }

    fn is_door_open(&self, _telemetry: &PrintTelemetry) -> bool {
        false
    }

    fn has_door_sensor(&self) -> bool {
        false
    }

    fn camera_stream_port(&self) -> u16 {
        6000
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
        false
    }

    fn is_unsafe_homing_command(&self, _gcode: &str) -> bool {
        false
    }

    fn relative_z_move_gcode(&self, distance: f32, _feedrate: u32) -> String {
        let limit = MAX_Z;
        if distance > limit || distance < -limit {
            return String::new();
        }
        String::from("M211 S1\nM1002 push_ref_mode\nG91\nG0 Z10.00 F3000\nG90\nM1002 pop_ref_mode")
    }

    /// Evaluates if the specified command string is unsupported or ignored on the target model.
    ///
    /// **Why get_version is allowed here:**
    /// Similar to the P1 series, we are lifting the restriction on `get_version` for the A1
    /// series to allow diagnostic evaluation and verification of device replies.
    fn is_unsupported_command(&self, _command: &str) -> bool {
        false
    }
}
