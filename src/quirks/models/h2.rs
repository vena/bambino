//! # H2 Series (H2S, H2D, H2D Pro, H2C) Quirks
//!
//! Manages the properties and kinematic characteristics of the single-nozzle,
//! IDEX, and tool-changer platforms [REF-MOTO-GCODE].

use crate::quirks::ModelQuirks;
use crate::types::PrintTelemetry;

pub struct H2SQuirks;
pub struct H2DQuirks;
pub struct H2DProQuirks;
pub struct H2CQuirks;

fn h2_is_door_open(telemetry: &PrintTelemetry) -> bool {
    telemetry.is_door_open_from_stat()
}

macro_rules! impl_h2_shared {
    ($quirks_type:ty, $nozzle_count:expr, $offset_cal:expr) => {
        impl ModelQuirks for $quirks_type {
            fn uses_plaintext_ftps_data_channel(&self) -> bool {
                false
            }

            fn enforce_ftps_tls_1_2(&self) -> bool {
                false
            }

            fn is_door_open(&self, telemetry: &PrintTelemetry) -> bool {
                h2_is_door_open(telemetry)
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
                $nozzle_count
            }

            fn supports_nozzle_offset_calibration(&self) -> bool {
                $offset_cal
            }

            fn is_bed_on_z(&self) -> bool {
                true
            }
        }
    };
}

impl_h2_shared!(H2SQuirks, 1, false);
impl_h2_shared!(H2DQuirks, 2, true);
impl_h2_shared!(H2DProQuirks, 2, true);
impl_h2_shared!(H2CQuirks, 7, true);
