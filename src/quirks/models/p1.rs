//! # P1 Series (P1P & P1S CoreXY) Quirks
//!
//! Tracks constraints and kinematic properties of early and enclosed low-power RTOS machines.

use crate::quirks::ModelQuirks;
use crate::types::PrintTelemetry;

#[cfg(not(feature = "std"))]
use alloc::string::String;

/// Standard post-boot socket preparation delay, in seconds
pub const POST_BOOT_CONNECT_DELAY: u64 = 25;

/// Connection handshake timeout limits specifically configured for low-resource ESP32 platforms
pub const CRYPTO_HANDSHAKE_TIMEOUT_MS: u64 = 5000;

pub struct P1Quirks;

impl ModelQuirks for P1Quirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
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
        true
    }

    fn is_unsafe_homing_command(&self, gcode: &str) -> bool {
        let clean = gcode.to_uppercase();
        clean.contains("G28") && (clean.contains('Z') || clean.contains('X') || clean.contains('Y'))
    }

    fn relative_z_move_gcode(&self, _distance: f32, _feedrate: u32) -> String {
        String::from(super::super::DEFAULT_Z_MOVE_GCODE)
    }

    /// Evaluates if the specified command string is unsupported or ignored on the target model.
    ///
    /// **Why get_version is allowed here:**
    /// Previously, a gate blocked `get_version` on the P1 series due to assumptions about lack
    /// of support. We are removing this restriction to verify if the P1S actually processes
    /// and responds to expansion bus version queries under active LAN/Developer modes.
    fn is_unsupported_command(&self, _command: &str) -> bool {
        false
    }
}
