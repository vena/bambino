//! # P2 Series (P2S CoreXY) Quirks
//!
//! Configures transport parameters, thermal layouts, and camera corrections for the P2S platform.

use crate::quirks::ModelQuirks;
use crate::types::PrintTelemetry;

#[cfg(not(feature = "std"))]
use alloc::string::String;

/// Forces TLS v1.2 restriction to avoid data channel session-close races
pub fn force_tls_v12_for_ftps() -> bool {
    true
}

/// Constant frame rate camera sync parameters to resolve RTP timestamp freezing bugs [REF-CAM-RTSPS]
pub fn requires_wallclock_rtsp_timestamps() -> bool {
    true
}

pub struct P2Quirks;

impl ModelQuirks for P2Quirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
        true
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
