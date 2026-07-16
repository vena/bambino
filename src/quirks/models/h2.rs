//! # H2 Series (H2S, H2D, H2D Pro, H2C) Quirks
//!
//! Manages the properties and kinematic characteristics of the single-nozzle,
//! IDEX, and tool-changer platforms [REF-MOTO-GCODE].
//!
//! Z-axis limits vary by model — per `MODEL_MATRIX.csv`'s Build Volume row, Z max does
//! not vary by active nozzle for these three models:
//! - H2S: 340mm (single nozzle only)
//! - H2D/H2D Pro: 325mm
//! - H2C: 325mm
//!
//! H2C has 6 Vortek tool-changer hotends + 1 fixed hotend = 7 nozzles.
//! O1C and O1C2 are hardware revisions with identical quirks.

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

/// H2S build volume Z depth (mm) — single-nozzle-only platform, per `MODEL_MATRIX.csv`'s Build Volume row.
pub const H2S_Z_MAX: f32 = 340.0;
/// Z depth (mm) shared by H2D, H2D Pro, and H2C — does not vary by active nozzle, per `MODEL_MATRIX.csv`'s Build Volume row.
pub const H2_DUAL_Z_MAX: f32 = 325.0;
/// H2S build volume X/Y (mm) — single-nozzle-only platform, per `MODEL_MATRIX.csv`'s Build
/// Volume row (340×320×340mm) (BUG-163).
pub const H2S_X_MAX: f32 = 340.0;
/// See `H2S_X_MAX`'s doc comment.
pub const H2S_Y_MAX: f32 = 320.0;
/// X/Y (mm) shared by H2D, H2D Pro, and H2C — conservative dual-nozzle value (the smaller of
/// each model's single/dual-nozzle profiles), same approach as `H2_DUAL_Z_MAX` (BUG-163).
pub const H2_DUAL_X_MAX: f32 = 300.0;
/// See `H2_DUAL_X_MAX`'s doc comment.
pub const H2_DUAL_Y_MAX: f32 = 320.0;
/// Nozzle temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const H2_NOZZLE_TEMP_MAX: u16 = 350;
/// Bed temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const H2_BED_TEMP_MAX: u16 = 120;
/// Chamber temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.
pub const H2_CHAMBER_TEMP_MAX: u16 = 65;

/// Quirks for the H2S — single-nozzle CoreXY, tallest Z of the H2 family.
pub struct H2SQuirks;
/// Quirks for the H2D — dual-nozzle (IDEX) CoreXY.
pub struct H2DQuirks;
/// Quirks for the H2D Pro — same kinematics as H2D.
pub struct H2DProQuirks;
/// Quirks for the H2C — Vortek tool-changer platform (6 tool-changer nozzles + 1 fixed nozzle).
pub struct H2CQuirks;

fn h2_is_door_open(telemetry: &PrinterTelemetry) -> bool {
    telemetry.is_door_open_from_stat()
}

fn h2_door_sensor_field_present(telemetry: &PrinterTelemetry) -> bool {
    telemetry.stat.is_some()
}

macro_rules! impl_h2_shared {
    ($quirks_type:ty, $nozzle_count:expr, $offset_cal:expr, $z_max:expr, $x_max:expr, $y_max:expr) => {
        impl ModelQuirks for $quirks_type {
            fn uses_plaintext_ftps_data_channel(&self) -> bool {
                false
            }

            fn enforce_ftps_tls_1_2(&self) -> bool {
                false
            }

            fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool {
                h2_is_door_open(telemetry)
            }

            fn door_sensor_field_present(&self, telemetry: &PrinterTelemetry) -> bool {
                h2_door_sensor_field_present(telemetry)
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
                $nozzle_count
            }

            fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition {
                crate::ams::AmsPoolComposition::Independent {
                    max_standard: 4,
                    max_ht: 8,
                }
            }

            fn supports_nozzle_offset_calibration(&self) -> bool {
                $offset_cal
            }

            fn is_bed_on_z(&self) -> bool {
                true
            }

            fn z_max(&self) -> f32 {
                $z_max
            }

            fn x_max(&self) -> f32 {
                $x_max
            }

            fn y_max(&self) -> f32 {
                $y_max
            }

            fn nozzle_temp_max(&self) -> u16 {
                H2_NOZZLE_TEMP_MAX
            }

            fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16 {
                H2_BED_TEMP_MAX
            }

            fn chamber_temp_max(&self) -> u16 {
                H2_CHAMBER_TEMP_MAX
            }

            fn supports_airduct_mode(&self) -> bool {
                true
            }

            fn supports_buzzer(&self) -> bool {
                true
            }

            fn has_chamber_exhaust_fan(&self) -> bool {
                true
            }
        }
    };
}

impl_h2_shared!(H2SQuirks, 1, false, H2S_Z_MAX, H2S_X_MAX, H2S_Y_MAX);
impl_h2_shared!(H2DQuirks, 2, true, H2_DUAL_Z_MAX, H2_DUAL_X_MAX, H2_DUAL_Y_MAX);
impl_h2_shared!(H2DProQuirks, 2, true, H2_DUAL_Z_MAX, H2_DUAL_X_MAX, H2_DUAL_Y_MAX);
impl_h2_shared!(H2CQuirks, 7, true, H2_DUAL_Z_MAX, H2_DUAL_X_MAX, H2_DUAL_Y_MAX);
