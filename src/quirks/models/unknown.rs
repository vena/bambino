//! # Unrecognized Model Fallback Quirks
//!
//! Row used for [`PrinterModel::Unknown`](crate::PrinterModel::Unknown) — a printer
//! whose model string this crate does not recognize (a new SKU, a malformed SSDP `DevModel`
//! header, or a firmware that reports an unexpected token).
//!
//! Physical limits here are the **floor of the entire supported family**, not any one model's
//! values: an unrecognized machine could be any of them, so every ceiling has to be one no
//! shipping model would exceed. This is why the fallback is not simply X1C's row — X1C's
//! bed ceiling is voltage-dependent and rises to 120 °C on a 110 V unit, 40 °C past the real
//! ceiling of the entry-level models an unrecognized printer might well be.
//!
//! Connection-layer behavior (FTPS data-channel encryption, TLS 1.2 enforcement, camera
//! protocol) keeps the X1-series values, since those are interop choices rather than physical
//! safety ceilings and the X1 settings are the ones that reach the widest set of hosts.

use crate::ams::{AmsLiteSlot, AmsPoolComposition};
use crate::camera::CameraProtocol;
use crate::quirks::{BedMax, BuildVolume, DryRule, ModelQuirks, SafetyLimits};

/// Travel ceiling (mm), applied to all three axes — the smallest build volume in the family
/// (A1 Mini), per `MODEL_MATRIX.csv`'s Build Volume row.
pub const UNKNOWN_AXIS_MAX: f32 = 180.0;
/// Nozzle temperature ceiling (°C) — the lowest hot-end ceiling in the family, per
/// `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const UNKNOWN_NOZZLE_TEMP_MAX: u16 = 300;
/// Bed temperature ceiling (°C) — the lowest build-plate ceiling in the family (A1 Mini / A2L),
/// per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row. Flat, never voltage-dependent: the
/// mains region of an unrecognized machine says nothing about which model it is.
pub const UNKNOWN_BED_TEMP_MAX: u16 = 80;

/// Conservative quirks for an unrecognized printer model — see the module docs.
///
/// No door sensor: `home_flag`/`stat` bit assignments are only known for recognized models, so
/// reading a door state out of an unrecognized machine would fabricate a sensor reading. No
/// active chamber heater, so `M141` is refused rather than sent to a printer that may have none.
/// Bed-on-Z so axis-constrained `G28` is rejected — a bed-slinger tolerates the homing variants
/// a bed-on-Z machine crashes on, so assuming bed-on-Z is the direction that cannot break
/// hardware.
pub(crate) const UNKNOWN: ModelQuirks = ModelQuirks::new(
    SafetyLimits {
        volume: BuildVolume::cube(UNKNOWN_AXIS_MAX),
        nozzle_temp_max: UNKNOWN_NOZZLE_TEMP_MAX,
        bed_temp_max: BedMax::Flat(UNKNOWN_BED_TEMP_MAX),
        chamber_heater_temp_max: None,
        bed_on_z: true,
    },
    CameraProtocol::Rtsps,
    AmsPoolComposition::Shared {
        max_units: 4,
        ams_lite: AmsLiteSlot::NotSupported,
    },
    DryRule::Unstated,
);
