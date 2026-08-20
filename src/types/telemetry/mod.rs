//! # State Telemetry Payload Schemas
//!
//! Provides structured, allocation-friendly deserialization models for the
//! local MQTTS Port 8883 state telemetry streams [REF-MQTT-ENV].
//!
//! Supports permissive parsing for platform discrepancies (such as the variable
//! types of `sdcard` presence markers) and implements binary unpacking helpers
//! for composite packed temperatures, home/status flags, and door sensors.
//!
//! ## Architectural Alignment
//! * **Quirks Integration:** Raw elements (e.g., `device.airduct.parts` or `ctc.info.temp`)
//!   are fully parsed into clean schemas to allow model-specific behaviors to be evaluated
//!   via the quirks engine.

pub mod ams;
pub mod device;
pub mod diagnostics;
pub mod report;

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

use serde::{Deserialize, Deserializer, Serialize};

pub use ams::{
    AmsDrySetting, AmsFilamentStep, AmsStatusReport, AmsTray, AmsUnit, FilamentSwitchInlet,
    VirtualTray,
};
pub use device::{
    AirductCollection, AirductModeListEntry, AirductPart, BedInfo, BedTelemetry, DeviceTelemetry,
    ExtToolTelemetry, ExtruderCollection, ExtruderInfo, NozzleCollection, NozzleInfo,
};
pub use diagnostics::{CtcInfo, CtcTelemetry, HmsEntry, IpcamTelemetry};
pub use report::{
    LightReport, NetInfo, PrintPauseList, PrintPausePoint, PrinterTelemetry, SdcardState,
};

pub(crate) const FUN_MQTT_SIGNATURE_REQUIRED: u64 = 0x20000000;

/// Unified top-level telemetry report received from the printer's local MQTT broker.
///
/// Under the over-the-wire schema, updates are typically nested within separate
/// top-level domains depending on which micro-system published the frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryReport {
    /// Telemetry parameters representing the physical printer state machine.
    #[serde(default)]
    pub print: Option<PrinterTelemetry>,

    /// Network and hardware board capability descriptors.
    #[serde(default)]
    pub device: Option<DeviceTelemetry>,

    /// Developer LAN Mode bitmask field (hex string).
    /// Drifts between top-level and `print.fun` depending on firmware version [REF-MQTT-ENV §3.2.1].
    pub fun: Option<String>,
}

impl TelemetryReport {
    /// Returns the bed's (actual, target) temperatures in °C.
    ///
    /// Handles the different wire formats across printer generations automatically:
    /// new-gen composite-packed `device.bed`, pushall-nested `print.device.bed`, and
    /// old-gen direct `bed_temper`/`bed_target_temper` fields. Returns (0, 0) if absent.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let (actual, target) = report.bed_temperatures();
    /// println!("Bed: {}°C (target {}°C)", actual, target);
    /// ```
    pub fn bed_temperatures(&self) -> (u16, u16) {
        let (bed_temper, bed_target_temper) = self
            .print
            .as_ref()
            .map(|print| (print.bed_temper, print.bed_target_temper))
            .unwrap_or((None, None));
        decode_bed_temperatures(self.device(), bed_temper, bed_target_temper)
    }

    /// Returns the `DeviceTelemetry` sub-object, checking both wire locations it can arrive at.
    ///
    /// Mirrors `bed_temperatures()`'s first-found-wins fallback: top-level `device` (incremental
    /// updates) is checked first, falling back to pushall-nested `print.device` (H2/P2/X2
    /// models). Returns `None` if neither location is present. Use this instead of manually
    /// checking both locations for nozzle/extruder/airduct/ctc/ext_tool sub-telemetry.
    pub fn device(&self) -> Option<&DeviceTelemetry> {
        self.device
            .as_ref()
            .or_else(|| self.print.as_ref().and_then(|print| print.device.as_ref()))
    }

    /// Returns the `fun` Developer LAN Mode bitmask, checking both wire locations it can
    /// arrive at.
    ///
    /// Mirrors `device()`'s fallback order — top-level `fun` is checked first,
    /// falling back to `print.fun` [REF-MQTT-ENV §3.2.1]. Prefer this over reading `self.fun`
    /// directly, the same way `device()` is preferred over `self.device`.
    pub fn fun(&self) -> Option<&str> {
        self.fun
            .as_deref()
            .or_else(|| self.print.as_ref().and_then(|print| print.fun.as_deref()))
    }
}

/// Shared bed-temperature decode logic behind [`TelemetryReport::bed_temperatures()`] and [`crate::client::PrinterClient::bed_temperatures()`] — both need the same cross-model unpack (composite-packed new-gen `device.bed` vs. flat old-gen `bed_temper`/ `bed_target_temper`), one sourced from a fresh report, the other from cached scalars.
pub(crate) fn decode_bed_temperatures(
    device: Option<&DeviceTelemetry>,
    bed_temper: Option<f64>,
    bed_target_temper: Option<f64>,
) -> (u16, u16) {
    if let Some(temps) = device.and_then(unpack_bed_telemetry) {
        return temps;
    }
    let actual = bed_temper.unwrap_or(0.0) as u16;
    let target = bed_target_temper.unwrap_or(0.0) as u16;
    (actual, target)
}

fn unpack_bed_telemetry(device: &DeviceTelemetry) -> Option<(u16, u16)> {
    let temp = device.bed.as_ref()?.info.as_ref()?.temp?;
    Some(PrinterTelemetry::unpack_temperature(temp as f64))
}

/// Shared nozzle-temperature decode logic behind [`crate::client::PrinterClient::nozzle_temperatures()`] — ported from the CLI's `bin/bambino-cli/monitor/dashboard.rs` (`populate_nozzle_temps()`), previously the only place this IDEX routing quirk lived.
///
/// Returns one `(id, actual, target)` tuple per nozzle. Prefers `device.extruder.info`
/// (composite-packed per-nozzle temperatures, decoded via [`ExtruderInfo::temperatures()`]).
/// Falls back to the flat `nozzle_temper`/`nozzle_target_temper` fields when absent: a single
/// entry `(0, actual, target)` for a single-nozzle model, or — for a dual-nozzle (IDEX) model
/// with no live extruder temps yet — the wire's undocumented routing quirk: `nozzle_temper` is
/// nozzle 1 (left)'s actual reading and `nozzle_target_temper` is nozzle 0 (right)'s target,
/// each nozzle only getting half of its own reading from the flat fields.
pub fn decode_nozzle_temperatures(
    device: Option<&DeviceTelemetry>,
    nozzle_temper: Option<f64>,
    nozzle_target_temper: Option<f64>,
) -> Vec<(u8, u16, u16)> {
    if let Some(extruder) = device.and_then(|d| d.extruder.as_ref())
        && let Some(info) = extruder.info.as_deref()
        && !info.is_empty()
    {
        return info
            .iter()
            .map(|entry| {
                let (actual, target) = entry.temperatures();
                (entry.id, actual, target)
            })
            .collect();
    }

    // Exclude rack-stored spare nozzles before counting — BambuStudio appends them
    // to the same `nozzle.info` array as installed ones, distinguished only by
    // `NozzleInfo::is_rack_stored()`. Without this, an H2C (single hotend + spare-nozzle
    // rack) misclassifies as IDEX.
    let is_idex = device
        .and_then(|d| d.nozzle.as_ref())
        .map(|n| n.info.iter().flatten().filter(|nz| !nz.is_rack_stored()).count() >= 2)
        .unwrap_or(false);

    let actual = nozzle_temper.unwrap_or(0.0) as u16;
    let target = nozzle_target_temper.unwrap_or(0.0) as u16;

    if is_idex {
        vec![(0, 0, target), (1, actual, 0)]
    } else {
        vec![(0, actual, target)]
    }
}

/// Evaluates Developer LAN Mode from the `fun` hex string [REF-MQTT-ENV §3.2.1].
///
/// Returns `Some(true)` when developer mode is enabled (MQTT signature NOT required),
/// `Some(false)` when disabled, or `None` if the hex string is unparseable.
/// The `fun` field is a variable-length hex string (up to 64 bits). Bit 29
/// (`0x20000000`) is the `MQTT_SIGNATURE_REQUIRED` flag — when clear, developer mode is on.
pub fn is_developer_mode(fun_hex: &str) -> Option<bool> {
    let val = u64::from_str_radix(fun_hex, 16).ok()?;
    Some((val & FUN_MQTT_SIGNATURE_REQUIRED) == 0)
}

/// Custom deserializer mapping various over-the-wire `sdcard` formats to a unified boolean.
///
/// Absorbs standard boolean values, integer indicators (e.g., `1`), and
/// firmware string constants like `"HAS_SDCARD_NORMAL"`.
fn deserialize_permissive_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawSdValue {
        Bool(bool),
        Int(i64),
        String(String),
    }

    match RawSdValue::deserialize(deserializer) {
        Ok(RawSdValue::Bool(b)) => Ok(b),
        Ok(RawSdValue::Int(i)) => Ok(i != 0),
        Ok(RawSdValue::String(s)) => {
            let s_upper = s.to_uppercase();
            Ok(s_upper == "HAS_SDCARD_NORMAL" || s_upper == "TRUE" || s_upper == "1")
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
