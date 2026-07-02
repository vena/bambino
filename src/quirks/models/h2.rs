//! # H2 Series (H2S, H2D, H2D Pro, H2C) Quirks
//!
//! Manages the properties and kinematic characteristics of the single-nozzle,
//! IDEX, and tool-changer platforms [REF-MOTO-GCODE].
//!
//! Z-axis limits vary by model and current print — multi-nozzle models use the
//! conservative (dual-nozzle) Z limit since the quirks engine doesn't know
//! which nozzle is active at runtime:
//! - H2S: 340mm (single nozzle only)
//! - H2D/H2D Pro: 320mm (conservative; 325mm in single-nozzle mode)
//! - H2C: 320mm (conservative; 325mm with right nozzle only)
//!
//! H2C has 6 Vortek tool-changer hotends + 1 fixed hotend = 7 nozzles.
//! O1C and O1C2 are hardware revisions with identical quirks.

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

pub const H2S_Z_MAX: f32 = 340.0;
pub const H2_DUAL_Z_MAX: f32 = 320.0;
pub const H2_NOZZLE_TEMP_MAX: u16 = 350;
pub const H2_BED_TEMP_MAX: u16 = 120;
pub const H2_CHAMBER_TEMP_MAX: u16 = 65;

pub struct H2SQuirks;
pub struct H2DQuirks;
pub struct H2DProQuirks;
pub struct H2CQuirks;

fn h2_is_door_open(telemetry: &PrinterTelemetry) -> bool {
    telemetry.is_door_open_from_stat()
}

macro_rules! impl_h2_shared {
    ($quirks_type:ty, $nozzle_count:expr, $offset_cal:expr, $z_max:expr) => {
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

            fn supports_nozzle_offset_calibration(&self) -> bool {
                $offset_cal
            }

            fn is_bed_on_z(&self) -> bool {
                true
            }

            fn z_max(&self) -> f32 {
                $z_max
            }

            fn nozzle_temp_max(&self) -> u16 {
                H2_NOZZLE_TEMP_MAX
            }

            fn bed_temp_max(&self) -> u16 {
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

impl_h2_shared!(H2SQuirks, 1, false, H2S_Z_MAX);
impl_h2_shared!(H2DQuirks, 2, true, H2_DUAL_Z_MAX);
impl_h2_shared!(H2DProQuirks, 2, true, H2_DUAL_Z_MAX);
impl_h2_shared!(H2CQuirks, 7, true, H2_DUAL_Z_MAX);
