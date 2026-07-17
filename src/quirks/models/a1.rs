//! # A1 Series (A1 & A1 Mini Bed-Slingers) Quirks & Coordinates
//!
//! Handles the kinematics, safety boundaries, and mechanical constraints of the
//! A1 bed-slinger family [REF-MOTO-GCODE].
//!
//! - A1: 256×256×256mm build volume
//! - A1 Mini: 180×180×180mm build volume

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

/// A1 build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.
pub const A1_Z_MAX: f32 = 256.0;
/// A1 Mini build volume Z depth (mm), per `MODEL_MATRIX.csv`'s Build Volume row.
pub const A1_MINI_Z_MAX: f32 = 180.0;
/// Nozzle temperature ceiling (°C) shared by A1 and A1 Mini, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const A1_NOZZLE_TEMP_MAX: u16 = 300;
/// A1 bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const A1_BED_TEMP_MAX: u16 = 100;
/// A1 Mini bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const A1_MINI_BED_TEMP_MAX: u16 = 80;

/// Quirks for the full-size A1 bed-slinger.
pub struct A1Quirks;
/// Quirks for the A1 Mini bed-slinger (same family, smaller build volume/bed ceiling).
pub struct A1MiniQuirks;

macro_rules! impl_a1_shared {
    ($quirks_type:ty, $z_max:expr, $bed_max:expr) => {
        impl ModelQuirks for $quirks_type {
            fn uses_plaintext_ftps_data_channel(&self) -> bool {
                true
            }

            fn enforces_ftps_tls_1_2(&self) -> bool {
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

            fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition {
                crate::ams::AmsPoolComposition::Shared { max_units: 4 }
            }

            fn supports_nozzle_offset_calibration(&self) -> bool {
                false
            }

            fn is_bed_on_z(&self) -> bool {
                false
            }

            fn z_max(&self) -> f32 {
                $z_max
            }

            fn x_max(&self) -> f32 {
                $z_max
            }

            fn y_max(&self) -> f32 {
                $z_max
            }

            fn nozzle_temp_max(&self) -> u16 {
                A1_NOZZLE_TEMP_MAX
            }

            fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16 {
                $bed_max
            }

            fn supports_prompt_sound(&self) -> bool {
                true
            }

            fn supports_auxiliary_left_fan(&self) -> bool {
                false
            }
        }
    };
}

impl_a1_shared!(A1Quirks, A1_Z_MAX, A1_BED_TEMP_MAX);
impl_a1_shared!(A1MiniQuirks, A1_MINI_Z_MAX, A1_MINI_BED_TEMP_MAX);
