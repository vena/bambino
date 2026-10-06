//! # P1 Series (P1P & P1S CoreXY) Quirks
//!
//! Constraints and kinematic properties of the early and enclosed low-power RTOS machines.

use crate::ams::{AmsLiteSlot, AmsPoolComposition};
use crate::camera::CameraProtocol;
use crate::quirks::{BedMax, BuildVolume, DryRule, ModelQuirks, SafetyLimits};

/// Build volume Z depth (mm) shared by P1P and P1S, per `MODEL_MATRIX.csv`'s Build Volume row.
pub const P1_Z_MAX: f32 = 256.0;
/// Nozzle temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const P1_NOZZLE_TEMP_MAX: u16 = 300;
/// Bed temperature ceiling (°C), per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const P1_BED_TEMP_MAX: u16 = 100;

/// P1S: enclosed CoreXY, guaranteed aux fan.
///
/// **Drying: never — screen-only.** The firmware acks `ams_filament_drying` `result: success` and
/// silently discards it. P1 firmware `01.08.00.00` (2025-04-29, P1P/P1S firmware release history)
/// says drying starts "from the printer's screen" and no later P1 release adds remote drying; the
/// *Filament drying guide for AMS 2 Pro and AMS HT* lists P1S/P1P as unsupported for both remote
/// drying and drying while printing. Also the P1 manual, bambuddy's `_DRYING_SCREEN_ONLY_MODELS`
/// citing its #2533, and direct hardware testing on a P1S. The P1 family sends no `fun2`
/// (`reference/03_mqtt_telemetry.md`), so the reported-bit stage never engages in practice.
pub(crate) const P1S: ModelQuirks = ModelQuirks::new(
    SafetyLimits {
        volume: BuildVolume::cube(P1_Z_MAX),
        nozzle_temp_max: P1_NOZZLE_TEMP_MAX,
        bed_temp_max: BedMax::Flat(P1_BED_TEMP_MAX),
        chamber_heater_temp_max: None,
        bed_on_z: true,
    },
    CameraProtocol::BinaryJpeg,
    AmsPoolComposition::Shared {
        max_units: 4,
        ams_lite: AmsLiteSlot::NotSupported,
    },
    DryRule::Never,
);

/// P1P: the P1S without a guaranteed aux fan.
///
/// `MODEL_MATRIX.csv`'s Aux Part Cooling Fan row lists P1P as `Optional` (not guaranteed present)
/// vs. P1S's `Yes`; reporting `true` would over-report on a P1P without the fan installed.
pub(crate) const P1P: ModelQuirks = ModelQuirks {
    auxiliary_left_fan: false,
    ..P1S
};
