//! Diagnostic telemetry types (HMS alerts, light reports).

#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

/// Chamber Temperature Controller (CTC) telemetry sub-object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtcTelemetry {
    /// Controller info containing thermal actuals and targets.
    pub info: Option<CtcInfo>,

    /// CTC controller state (0 = idle, 2 = heating).
    #[serde(default)]
    pub state: Option<u32>,
}

/// Controller information segment detailing current temperature coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtcInfo {
    /// Composite-packed integer temperature value [REF-THER-DECODE].
    /// Use `PrinterTelemetry::unpack_temperature()` on this value cast to `f64`.
    pub temp: Option<u32>,

    /// Explicit CTC target temperature (authoritative on new-gen models).
    #[serde(default)]
    pub target: Option<u32>,
}

/// Camera and recording state telemetry, nested as `print.ipcam` on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcamTelemetry {
    /// Internal identifier or state of the hardware camera module.
    pub ipcam_dev: Option<String>,

    /// Camera live feed recording status (`"enable"` or `"disable"`).
    pub ipcam_record: Option<String>,

    /// Frame-by-layer timelapse recording status (`"enable"` or `"disable"`).
    pub timelapse: Option<String>,

    /// Camera mode bitmask.
    pub mode_bits: Option<u32>,

    /// Camera resolution setting.
    pub resolution: Option<String>,

    /// TUTK server status (`"enable"` or `"disable"`).
    pub tutk_server: Option<String>,

    /// RTSP streaming URL (e.g. `"rtsps://192.168.1.64/streaming/live/1"`).
    #[serde(default)]
    pub rtsp_url: Option<String>,
}

/// Raw telemetry entry from the `hms` diagnostic array [REF-DIAG-HMS].
///
/// Each entry represents an active hardware fault or status indication. Use
/// `diagnostics::decode_hms_alert()` to unpack into wiki keys, short-codes, and severity levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HmsEntry {
    /// Packed attribute word encoding module ID, severity, and subsystem address.
    pub attr: u32,
    /// Packed code word encoding fault category and error index.
    pub code: u32,
    /// Seconds since boot when the alert was raised (present on X2/H2/P2 models).
    #[serde(default)]
    pub ts_boot: Option<u64>,
    /// UTC timestamp string when the alert was raised (e.g. `"20260426002648"`).
    #[serde(default)]
    pub ts_unix: Option<String>,
}
