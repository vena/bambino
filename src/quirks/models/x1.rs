//! # X1 Series (X1, X1C, X1E CoreXY) Quirks
//!
//! Hardware safety limits and thermal parameters for the premium CoreXY platforms. X1C and X1E
//! share everything except active chamber heater support (X1E only), ceilings and drying rule.
//! The plain X1 is the X1C minus a stock auxiliary part-cooling fan — see `X1`.

use crate::ams::{AmsLiteSlot, AmsPoolComposition};
use crate::camera::CameraProtocol;
use crate::quirks::{BedMax, BuildVolume, DoorSensor, DryRule, ModelQuirks, SafetyLimits};

/// Build volume Z depth (mm) shared by X1, X1C and X1E, per `MODEL_MATRIX.csv`'s Build Volume row.
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
/// X1E bed temperature ceiling (°C) — flat, not voltage-dependent like X1C's, per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const X1E_BED_TEMP_MAX: u16 = 110;
/// X1E chamber temperature ceiling (°C) — X1E has an active chamber heater, X1C does not, per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.
pub const X1E_CHAMBER_TEMP_MAX: u16 = 60;

/// X1C's voltage-dependent bed ceiling — see `X1C_BED_TEMP_MAX_220V`'s doc comment.
const X1C_BED_TEMP_MAX: BedMax = BedMax::Voltage {
    v220: X1C_BED_TEMP_MAX_220V,
    v110: X1C_BED_TEMP_MAX_110V,
};

/// The X1C never supports remote AMS drying or drying while printing; a reported `fun2` bit still wins.
///
/// Bambu Lab's *Filament drying guide for AMS 2 Pro and AMS HT* omits the X1C from its Bambu
/// Studio remote-drying firmware list and names it outright in its simultaneous-drying list:
/// "P1S/P1P/X1C/A1/A1mini are not supported yet". No release in the X1/X1C firmware release
/// history (<https://wiki.bambulab.com/en/x1/manual/X1-X1C-firmware-release-history>) through
/// `01.12.00.00` mentions remote drying, and Bambu Lab has stated drying while printing needs
/// hardware the X1 Carbon lacks.
///
/// bambuddy's `_DRYING_MIN_FIRMWARE` lists `01.09.00.00` for X1/X1C. That is the X1's AMS 2 Pro/HT
/// support release (2025-04-29), whose only drying line is "starting the filament drying operation
/// from the printer's screen" — the sentence P1 `01.08.00.00` carries, which marks the P1 as
/// screen-only. Don't restore it. Its `_DRY_WHILE_PRINTING_MIN_FIRMWARE` lists `01.11.02.00`,
/// whose release notes carry no drying entry.
///
/// The X1 and X1C share one firmware line and release history, so the plain X1 takes this rule
/// unchanged.
const X1C_DRY_RULE: DryRule = DryRule::Never;

const X1_AMS_POOL: AmsPoolComposition = AmsPoolComposition::Shared {
    max_units: 4,
    ams_lite: AmsLiteSlot::NotSupported,
};

/// First X1/X1C release with step-loss auto-recovery and Filament Backup.
///
/// BambuStudio's `BL-P001.json`/`BL-P002.json` set `support_auto_recovery_step_loss` and
/// `support_filament_backup` false at `00.00.00.00` and true from `01.01.01.00`; every other
/// model's profile has both from its first release.
const X1C_PRINT_OPTIONS_MIN_FIRMWARE: &str = "01.01.01.00";

/// X1 Carbon: no active chamber heater, voltage-dependent bed ceiling.
pub(crate) const X1C: ModelQuirks = ModelQuirks {
    door: DoorSensor::HomeFlag,
    chamber_temperature_sensor: true,
    print_options_min_firmware: Some(X1C_PRINT_OPTIONS_MIN_FIRMWARE),
    ..ModelQuirks::new(
        SafetyLimits {
            volume: BuildVolume::cube(X1_Z_MAX),
            nozzle_temp_max: X1C_NOZZLE_TEMP_MAX,
            bed_temp_max: X1C_BED_TEMP_MAX,
            chamber_heater_temp_max: None,
            bed_on_z: true,
        },
        CameraProtocol::Rtsps,
        X1_AMS_POOL,
        X1C_DRY_RULE,
    )
};

/// The original, non-Carbon X1: the X1C's limits and rules, without the auxiliary fan.
///
/// BambuStudio's `resources/printers/BL-P002.json` (X1) and `BL-P001.json` (X1 Carbon) differ
/// only in names, model id and serial prefix, and its `Bambu Lab X1 0.4 nozzle` machine profile
/// differs from the X1 Carbon's in no limit field — but it sets `auxiliary_fan` to `0` where the
/// X1 Carbon inherits `1`. The aux fan is therefore treated like the P1P's: not guaranteed
/// present.
pub(crate) const X1: ModelQuirks = ModelQuirks {
    auxiliary_left_fan: false,
    ..X1C
};

/// X1E: active chamber heater, higher nozzle ceiling than X1C, flat bed ceiling.
///
/// Drying is deliberately **not** firmware-gated. No release in the X1E firmware release history
/// (<https://wiki.bambulab.com/en/x1/manual/X1E-firmware-release-history>) through `01.02.00.00`
/// mentions remote drying, but the drying guide doesn't name the X1E as unsupported either, and
/// bambuddy's docstring names X1E among the models that fall through to "allowed"
/// (`printer_manager.py:334`). Extrapolating the X1C rule onto the X1E would invent a restriction
/// no source states.
pub(crate) const X1E: ModelQuirks = ModelQuirks {
    door: DoorSensor::HomeFlag,
    chamber_temperature_sensor: true,
    ..ModelQuirks::new(
        SafetyLimits {
            volume: BuildVolume::cube(X1_Z_MAX),
            nozzle_temp_max: X1E_NOZZLE_TEMP_MAX,
            bed_temp_max: BedMax::Flat(X1E_BED_TEMP_MAX),
            chamber_heater_temp_max: Some(X1E_CHAMBER_TEMP_MAX),
            bed_on_z: true,
        },
        CameraProtocol::Rtsps,
        X1_AMS_POOL,
        DryRule::Unstated,
    )
};
