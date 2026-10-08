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

use crate::ams::AmsPoolComposition;
use crate::camera::CameraProtocol;
use crate::quirks::{
    BedMax, BuildVolume, DoorSensor, DryRule, ModelQuirks, NozzleLayout, SafetyLimits,
};

/// H2S build volume Z depth (mm) — single-nozzle-only platform, per `MODEL_MATRIX.csv`'s Build Volume row.
pub const H2S_Z_MAX: f32 = 340.0;
/// Z depth (mm) shared by H2D, H2D Pro, and H2C — does not vary by active nozzle, per `MODEL_MATRIX.csv`'s Build Volume row.
pub const H2_DUAL_Z_MAX: f32 = 325.0;
/// H2S build volume X/Y (mm) — single-nozzle-only platform, per `MODEL_MATRIX.csv`'s Build Volume row (340×320×340mm).
pub const H2S_X_MAX: f32 = 340.0;
/// See `H2S_X_MAX`'s doc comment.
pub const H2S_Y_MAX: f32 = 320.0;
/// X/Y (mm) shared by H2D, H2D Pro, and H2C — conservative dual-nozzle value (the smaller of
/// each model's single/dual-nozzle profiles), same approach as `H2_DUAL_Z_MAX`.
pub const H2_DUAL_X_MAX: f32 = 300.0;
/// See `H2_DUAL_X_MAX`'s doc comment.
pub const H2_DUAL_Y_MAX: f32 = 320.0;
/// Nozzle temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Hot End Temperature row.
pub const H2_NOZZLE_TEMP_MAX: u16 = 350;
/// Bed temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Build Plate Temperature row.
pub const H2_BED_TEMP_MAX: u16 = 120;
/// Chamber temperature ceiling (°C) shared across the H2 family, per `MODEL_MATRIX.csv`'s Max Chamber Temperature row.
pub const H2_CHAMBER_TEMP_MAX: u16 = 65;

/// Firmware release that introduced remote AMS drying, and drying while printing, on the H2D.
///
/// H2D `01.03.00.00` (2026-03-03, <https://wiki.bambulab.com/en/h2d/manual/h2d-firmware-release-history>):
/// "Added support for remotely enabling the drying function" and "Added support for printing
/// while filament is drying". The *Filament drying guide for AMS 2 Pro and AMS HT* gives the same
/// minimum in both of its lists, and bambuddy's `_DRY_WHILE_PRINTING_MIN_FIRMWARE` agrees.
///
/// Later than the H2S/H2C `01.02.00.00` relative to each model's own numbering; that is real, not
/// a slip. Don't restore bambuddy's `_DRYING_MIN_FIRMWARE` value `01.02.30.00`: it is BambuStudio
/// 2.5.0's release-note minimum for drying *while printing*, and no such H2D release exists
/// (`01.02.10.00` is followed by `01.03.00.00`).
pub const H2D_MIN_REMOTE_DRY_FIRMWARE: &str = "01.03.00.00";

/// Firmware release that introduced remote AMS drying, and drying while printing, on the H2S and H2C.
///
/// H2S `01.02.00.00` (2026-03-31, <https://wiki.bambulab.com/en/h2s/manual/h2s-firmware-release-history>)
/// and H2C `01.02.00.00` (2026-06-01, <https://wiki.bambulab.com/en/h2c/manual/h2c-firmware-release-history>)
/// both add remote drying and printing while drying. The drying guide gives the same H2S minimum;
/// it omits the H2C, which is staleness — BambuStudio 2.5.3's notes also name H2C. bambuddy's
/// `_DRYING_MIN_FIRMWARE` agrees. BambuStudio 2.5.3's "01.01.40.00 (H2S)" is outvoted by both
/// vendor pages.
pub const H2S_H2C_MIN_REMOTE_DRY_FIRMWARE: &str = "01.02.00.00";

/// Firmware release that introduced remote AMS drying, and drying while printing, on the H2D Pro.
///
/// H2D Pro `01.02.00.00` (2026-04-27, <https://wiki.bambulab.com/en/h2d-pro/manual/firmware-release-history>):
/// "Added support for remotely enabling the drying function" and printing while drying; no
/// earlier H2D Pro release has either. bambuddy's `_DRYING_MIN_FIRMWARE` omits the H2D Pro rather
/// than contradicting this, and its `_DRY_WHILE_PRINTING_MIN_FIRMWARE` agrees.
pub const H2D_PRO_MIN_REMOTE_DRY_FIRMWARE: &str = "01.02.00.00";

/// The H2 family's shared row: everything but nozzle layout, build volume and drying release.
const fn h2(
    volume: BuildVolume,
    nozzles: NozzleLayout,
    min_dry_firmware: &'static str,
) -> ModelQuirks {
    ModelQuirks {
        door: DoorSensor::Stat,
        chamber_temperature_sensor: true,
        nozzles,
        airduct_mode: true,
        store_sent_files: true,
        ai_monitoring: true,
        buzzer: true,
        chamber_exhaust_fan: true,
        ..ModelQuirks::new(
            SafetyLimits {
                volume,
                nozzle_temp_max: H2_NOZZLE_TEMP_MAX,
                bed_temp_max: BedMax::Flat(H2_BED_TEMP_MAX),
                chamber_heater_temp_max: Some(H2_CHAMBER_TEMP_MAX),
                bed_on_z: true,
            },
            CameraProtocol::Rtsps,
            AmsPoolComposition::Independent {
                max_standard: 4,
                max_ht: 8,
            },
            DryRule::Firmware {
                min: min_dry_firmware,
                in_first_release: false,
            },
        )
    }
}

const H2_DUAL_VOLUME: BuildVolume = BuildVolume {
    x: H2_DUAL_X_MAX,
    y: H2_DUAL_Y_MAX,
    z: H2_DUAL_Z_MAX,
};

/// H2S: single-nozzle CoreXY, tallest Z of the H2 family.
pub(crate) const H2S: ModelQuirks = h2(
    BuildVolume {
        x: H2S_X_MAX,
        y: H2S_Y_MAX,
        z: H2S_Z_MAX,
    },
    NozzleLayout::Single,
    H2S_H2C_MIN_REMOTE_DRY_FIRMWARE,
);
/// H2D: dual-nozzle (IDEX) CoreXY.
pub(crate) const H2D: ModelQuirks = h2(
    H2_DUAL_VOLUME,
    NozzleLayout::Dual,
    H2D_MIN_REMOTE_DRY_FIRMWARE,
);
/// H2D Pro: same kinematics as H2D.
pub(crate) const H2D_PRO: ModelQuirks = h2(
    H2_DUAL_VOLUME,
    NozzleLayout::Dual,
    H2D_PRO_MIN_REMOTE_DRY_FIRMWARE,
);
/// H2C: Vortek tool-changer platform (6 rack nozzles + 1 fixed nozzle).
pub(crate) const H2C: ModelQuirks = h2(
    H2_DUAL_VOLUME,
    NozzleLayout::Rack { nozzles: 7 },
    H2S_H2C_MIN_REMOTE_DRY_FIRMWARE,
);
