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

use serde::{Deserialize, Deserializer, Serialize};

pub use ams::{AmsDrySetting, AmsStatusReport, AmsTray, AmsUnit, VirtualTray};
pub use device::{
    AirductCollection, AirductModeListEntry, AirductPart, BedInfo, BedTelemetry, DeviceTelemetry,
    ExtToolTelemetry, ExtruderCollection, ExtruderInfo, NozzleCollection, NozzleInfo,
};
pub use diagnostics::{CtcInfo, CtcTelemetry, HmsEntry, IpcamTelemetry};
pub use report::{LightReport, PrinterTelemetry};

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

    /// Developer LAN Mode bitmask field (hex string). Drifts between top-level
    /// and `print.fun` depending on firmware version [REF-MQTT-ENV §3.2.1].
    pub fun: Option<String>,
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
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
