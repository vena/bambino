//! # P1 Series (P1P & P1S CoreXY) Quirks
//!
//! Tracks constraints and kinematic properties of early and enclosed low-power RTOS machines.

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

/// Build volume Z depth (mm) shared by P1P and P1S, per `MODEL_MATRIX.csv`'s Build Volume row.
pub const P1_Z_MAX: f32 = 256.0;
/// Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const P1_NOZZLE_TEMP_MAX: u16 = 300;
/// Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const P1_BED_TEMP_MAX: u16 = 100;

/// Quirks for the P1P CoreXY platform.
pub struct P1PQuirks;
/// Quirks for the P1S CoreXY platform (same family, enclosed, guaranteed aux fan).
pub struct P1SQuirks;

macro_rules! impl_p1_shared {
    ($quirks_type:ty, $supports_auxiliary_left_fan:expr) => {
        impl ModelQuirks for $quirks_type {
            fn uses_plaintext_ftps_data_channel(&self) -> bool {
                false
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

            fn active_chamber_heater_max_temp_c(&self) -> Option<u16> {
                None
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

            /// Screen-only: the firmware acks `ams_filament_drying` `result: success` and silently discards it.
            ///
            /// P1 firmware `01.08.00.00` (2025-04-29, P1P/P1S firmware release history) says drying
            /// starts "from the printer's screen" and no later P1 release adds remote drying; the
            /// *Filament drying guide for AMS 2 Pro and AMS HT* lists P1S/P1P as unsupported. Also
            /// the P1 manual, bambuddy's `_DRYING_SCREEN_ONLY_MODELS` citing its #2533, and direct
            /// hardware testing on a P1S.
            ///
            /// **In practice this always returns `false`.** The `fun2` branch exists for
            /// consistency with every other implementation, but the P1 family sends no `fun2`
            /// at all (`reference/03_mqtt_telemetry.md`), so no P1 can currently reach it. It
            /// is not a live self-healing path, and a firmware release adding remote drying
            /// would have to start emitting `fun2` for it to engage.
            fn ams_remote_drying_support(
                &self,
                ctx: &crate::quirks::QuirkContext,
            ) -> crate::quirks::Support {
                crate::quirks::remote_dry_reported_or(ctx, crate::quirks::Support::Inferred(false))
            }

            /// Never supports drying while printing.
            ///
            /// The drying guide names P1S/P1P as "not supported yet" for simultaneous drying and
            /// printing.
            fn ams_drying_while_printing_support(
                &self,
                _ctx: &crate::quirks::QuirkContext,
            ) -> crate::quirks::Support {
                crate::quirks::Support::Inferred(false)
            }

            fn is_bed_on_z(&self) -> bool {
                true
            }

            fn z_max(&self) -> f32 {
                P1_Z_MAX
            }

            fn x_max(&self) -> f32 {
                P1_Z_MAX
            }

            fn y_max(&self) -> f32 {
                P1_Z_MAX
            }

            fn nozzle_temp_max(&self) -> u16 {
                P1_NOZZLE_TEMP_MAX
            }

            fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16 {
                P1_BED_TEMP_MAX
            }

            /// `MODEL_MATRIX.csv`'s Aux Part Cooling Fan row lists P1P as `Optional`
            /// (not guaranteed present) vs. P1S's `Yes` — the shared `P1Quirks` struct this
            /// split from couldn't distinguish the two and unconditionally reported `true`,
            /// which would over-report support on a P1P without the physical fan installed.
            fn supports_auxiliary_left_fan(&self) -> bool {
                $supports_auxiliary_left_fan
            }
        }
    };
}

impl_p1_shared!(P1PQuirks, false);
impl_p1_shared!(P1SQuirks, true);
