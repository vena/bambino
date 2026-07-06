//! # X2 Series (X2D CoreXY) Quirks
//!
//! Handles parameters unique to the X2D dual-carriage auxiliary-cooling model.
//!
//! Build volumes: Main Nozzle 256×256×260mm, Aux/Dual 235.5×256×256mm.
//! Z-max uses the conservative aux/dual value (256mm).

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

/// Build volume Z depth (mm) — uses the conservative aux/dual-nozzle value, not the main-nozzle value; see module docs.
pub const X2D_Z_MAX: f32 = 256.0;
/// Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const X2D_NOZZLE_TEMP_MAX: u16 = 300;
/// Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const X2D_BED_TEMP_MAX: u16 = 120;
/// Chamber temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.
pub const X2D_CHAMBER_TEMP_MAX: u16 = 65;

/// Quirks for the X2D dual-carriage, dual-nozzle CoreXY platform.
pub struct X2Quirks;

impl ModelQuirks for X2Quirks {
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
        true
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

    fn z_max(&self) -> f32 {
        X2D_Z_MAX
    }

    fn nozzle_temp_max(&self) -> u16 {
        X2D_NOZZLE_TEMP_MAX
    }

    fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16 {
        X2D_BED_TEMP_MAX
    }

    fn chamber_temp_max(&self) -> u16 {
        X2D_CHAMBER_TEMP_MAX
    }

    fn supports_airduct_mode(&self) -> bool {
        true
    }

    fn has_chamber_exhaust_fan(&self) -> bool {
        true
    }
}
