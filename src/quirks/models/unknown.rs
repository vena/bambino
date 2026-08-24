//! # Unrecognized Model Fallback Quirks
//!
//! Strategy used for [`PrinterModel::Unknown`](crate::PrinterModel::Unknown) — a printer
//! whose model string this crate does not recognize (a new SKU, a malformed SSDP `DevModel`
//! header, or a firmware that reports an unexpected token).
//!
//! Physical limits here are the **floor of the entire supported family**, not any one model's
//! values: an unrecognized machine could be any of them, so every ceiling has to be one no
//! shipping model would exceed. This is why the fallback is not simply X1C's strategy — X1C's
//! bed ceiling is voltage-dependent and rises to 120 °C on a 110 V unit, 40 °C past the real
//! ceiling of the entry-level models an unrecognized printer might well be.
//!
//! Connection-layer behavior (FTPS data-channel encryption, TLS 1.2 enforcement, camera
//! protocol) keeps the X1-series values, since those are interop choices rather than physical
//! safety ceilings and the X1 settings are the ones that reach the widest set of hosts.

use crate::camera::CameraProtocol;
use crate::quirks::ModelQuirks;
use crate::types::PrinterTelemetry;

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
pub struct UnknownQuirks;

impl ModelQuirks for UnknownQuirks {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        false
    }

    fn enforces_ftps_tls_1_2(&self) -> bool {
        false
    }

    /// Always false — `home_flag` bit assignments are only known for recognized models, so
    /// reading a door state out of an unrecognized machine's flags would be fabricating a
    /// sensor reading rather than reporting one.
    fn is_door_open(&self, _telemetry: &PrinterTelemetry) -> bool {
        false
    }

    fn has_door_sensor(&self) -> bool {
        false
    }

    fn camera_protocol(&self) -> CameraProtocol {
        CameraProtocol::Rtsps
    }

    fn ignores_chamber_temperature(&self) -> bool {
        true
    }

    /// Assumes the bug is present — treating a real `stg_cur` idle report as suspect costs a
    /// redundant state check, while missing the bug reports a running print as finished.
    fn has_stg_cur_idle_bug(&self) -> bool {
        true
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

    /// True so axis-constrained `G28` is rejected — a bed-slinger tolerates the homing variants
    /// a bed-on-Z machine crashes on, so assuming bed-on-Z is the direction that cannot break
    /// hardware.
    fn is_bed_on_z(&self) -> bool {
        true
    }

    fn z_max(&self) -> f32 {
        UNKNOWN_AXIS_MAX
    }

    fn x_max(&self) -> f32 {
        UNKNOWN_AXIS_MAX
    }

    fn y_max(&self) -> f32 {
        UNKNOWN_AXIS_MAX
    }

    fn nozzle_temp_max(&self) -> u16 {
        UNKNOWN_NOZZLE_TEMP_MAX
    }

    fn bed_temp_max(&self, _mains_220v: Option<bool>) -> u16 {
        UNKNOWN_BED_TEMP_MAX
    }

    /// `None` — an unrecognized machine gets no active chamber heater, so `M141` is refused
    /// rather than sent to a printer that may have no heater to receive it.
    fn active_chamber_heater_max_temp_c(&self) -> Option<u16> {
        None
    }
}
