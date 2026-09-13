//! # X1 Series (X1C, X1E CoreXY) Quirks
//!
//! Implements hardware safety guidelines and thermal parameters for the premium CoreXY platforms.
//! X1C and X1E share all behavior except active chamber heater support (X1E only).

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

/// Build volume Z depth (mm) shared by X1C and X1E, per `MODEL_MATRIX.csv`'s Build Volume row.
pub const X1_Z_MAX: f32 = 256.0;

/// X1C nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const X1C_NOZZLE_TEMP_MAX: u16 = 300;
/// Bed temperature ceiling on a 220V-region unit — confirmed, per the official spec sheet, non-obviously *lower* than the 110V ceiling.
/// Also the conservative default when the mains region is unknown (no `home_flag` telemetry
/// received yet).
pub const X1C_BED_TEMP_MAX_220V: u16 = 110;
/// Bed temperature ceiling on a 110V-region unit.
pub const X1C_BED_TEMP_MAX_110V: u16 = 120;

/// X1E nozzle temperature ceiling (°C) — higher than X1C's, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const X1E_NOZZLE_TEMP_MAX: u16 = 320;
/// X1E bed temperature ceiling (°C) — flat, not voltage-dependent (see `x1e_bed_temp_max`), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const X1E_BED_TEMP_MAX: u16 = 110;
/// X1E chamber temperature ceiling (°C) — X1E has an active chamber heater, X1C does not, per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.
pub const X1E_CHAMBER_TEMP_MAX: u16 = 60;

/// Quirks for the X1C — no active chamber heater, voltage-dependent bed ceiling (see `x1c_bed_temp_max`).
pub struct X1CQuirks;
/// Quirks for the X1E — active chamber heater, higher nozzle ceiling than X1C.
pub struct X1EQuirks;

fn x1_is_door_open(telemetry: &PrinterTelemetry) -> bool {
    telemetry.is_door_open_from_home_flag()
}

fn x1_has_door_sensor_field(telemetry: &PrinterTelemetry) -> bool {
    telemetry.home_flag.is_some()
}

/// X1C's bed ceiling is voltage-dependent — see `X1C_BED_TEMP_MAX_220V`'s doc comment.
/// A free function (not inlined into the macro invocation) since a multi-arm `match` doesn't
/// substitute cleanly as a macro argument without fighting macro hygiene.
fn x1c_bed_temp_max(mains_220v: Option<bool>) -> u16 {
    match mains_220v {
        Some(true) => X1C_BED_TEMP_MAX_220V,
        Some(false) => X1C_BED_TEMP_MAX_110V,
        // Unknown mains region (no home_flag telemetry yet) — fail toward the safer,
        // more conservative ceiling.
        None => X1C_BED_TEMP_MAX_220V,
    }
}

/// X1E's bed ceiling is a flat constant — voltage-independent (its chamber-heater bed ceiling isn't voltage-dependent per the spec, unlike X1C's).
fn x1e_bed_temp_max(_mains_220v: Option<bool>) -> u16 {
    X1E_BED_TEMP_MAX
}

/// X1C never supports remote AMS drying; a reported `fun2` bit still wins.
///
/// Bambu Lab's *Filament drying guide for AMS 2 Pro and AMS HT* omits the X1C from its Bambu
/// Studio remote-drying firmware list and names it outright in its simultaneous-drying list:
/// "P1S/P1P/X1C/A1/A1mini are not supported yet". No release in the X1/X1C firmware release
/// history (<https://wiki.bambulab.com/en/x1/manual/X1-X1C-firmware-release-history>) through
/// `01.12.00.00` mentions remote drying.
///
/// bambuddy's `_DRYING_MIN_FIRMWARE` lists `01.09.00.00` for X1/X1C. That is the X1's AMS 2 Pro/HT
/// support release (2025-04-29), whose only drying line is "starting the filament drying operation
/// from the printer's screen" — the sentence P1 `01.08.00.00` carries, which marks the P1 as
/// screen-only. Don't restore it.
///
/// Kept `fun2`-first rather than a bare `false` because the X1 does send `fun2`
/// (`reference/03_mqtt_telemetry.md`), so a printer that ever reports bit 5 set still wins.
fn x1c_remote_drying_support(ctx: &crate::quirks::QuirkContext) -> crate::quirks::Support {
    crate::quirks::remote_dry_reported_or(ctx, crate::quirks::Support::Inferred(false))
}

/// X1C never supports drying while printing — named as "not supported yet" in the drying guide.
///
/// bambuddy's `_DRY_WHILE_PRINTING_MIN_FIRMWARE` lists `01.11.02.00` for X1C; that release's
/// notes carry no drying entry, and Bambu Lab has stated the feature needs hardware the X1 Carbon
/// lacks.
fn x1c_drying_while_printing_support(_ctx: &crate::quirks::QuirkContext) -> crate::quirks::Support {
    crate::quirks::Support::Inferred(false)
}

/// X1E is deliberately **not** firmware-gated.
///
/// No release in the X1E firmware release history
/// (<https://wiki.bambulab.com/en/x1/manual/X1E-firmware-release-history>) through `01.02.00.00`
/// mentions remote drying, but the drying guide doesn't name the X1E as unsupported either, and
/// bambuddy's docstring names X1E among the models that fall through to "allowed"
/// (`printer_manager.py:334`). Extrapolating the X1C rule onto the X1E would invent a restriction
/// no source states, so this follows the trait default: assume allowed, and let the printer
/// answer `result: "fail"` if it cannot. Drying while printing takes the trait's default deny.
fn x1e_remote_drying_support(ctx: &crate::quirks::QuirkContext) -> crate::quirks::Support {
    crate::quirks::remote_dry_reported_or(ctx, crate::quirks::Support::Assumed(true))
}

fn x1e_drying_while_printing_support(ctx: &crate::quirks::QuirkContext) -> crate::quirks::Support {
    crate::quirks::dry_while_printing_unless_reported_off(
        ctx,
        crate::quirks::Support::Assumed(false),
    )
}

macro_rules! impl_x1_shared {
    ($quirks_type:ty, $chamber_heater_max:expr, $nozzle_max:expr, $bed_max_fn:expr, $remote_dry_fn:expr, $dry_while_printing_fn:expr) => {
        impl ModelQuirks for $quirks_type {
            fn uses_plaintext_ftps_data_channel(&self) -> bool {
                false
            }

            fn enforces_ftps_tls_1_2(&self) -> bool {
                false
            }

            fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool {
                x1_is_door_open(telemetry)
            }

            fn has_door_sensor_field(&self, telemetry: &PrinterTelemetry) -> bool {
                x1_has_door_sensor_field(telemetry)
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

            fn physical_nozzle_count(&self) -> u8 {
                1
            }

            fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition {
                crate::ams::AmsPoolComposition::Shared {
                    max_units: 4,
                    ams_lite: crate::ams::AmsLiteSlot::None,
                }
            }

            fn supports_nozzle_offset_calibration(&self) -> bool {
                false
            }

            fn ams_remote_drying_support(
                &self,
                ctx: &crate::quirks::QuirkContext,
            ) -> crate::quirks::Support {
                $remote_dry_fn(ctx)
            }

            fn ams_drying_while_printing_support(
                &self,
                ctx: &crate::quirks::QuirkContext,
            ) -> crate::quirks::Support {
                $dry_while_printing_fn(ctx)
            }

            fn is_bed_on_z(&self) -> bool {
                true
            }

            fn z_max(&self) -> f32 {
                X1_Z_MAX
            }

            fn x_max(&self) -> f32 {
                X1_Z_MAX
            }

            fn y_max(&self) -> f32 {
                X1_Z_MAX
            }

            fn nozzle_temp_max(&self) -> u16 {
                $nozzle_max
            }

            fn bed_temp_max(&self, mains_220v: Option<bool>) -> u16 {
                $bed_max_fn(mains_220v)
            }

            fn active_chamber_heater_max_temp_c(&self) -> Option<u16> {
                $chamber_heater_max
            }
        }
    };
}

impl_x1_shared!(
    X1CQuirks,
    None,
    X1C_NOZZLE_TEMP_MAX,
    x1c_bed_temp_max,
    x1c_remote_drying_support,
    x1c_drying_while_printing_support
);
impl_x1_shared!(
    X1EQuirks,
    Some(X1E_CHAMBER_TEMP_MAX),
    X1E_NOZZLE_TEMP_MAX,
    x1e_bed_temp_max,
    x1e_remote_drying_support,
    x1e_drying_while_printing_support
);
