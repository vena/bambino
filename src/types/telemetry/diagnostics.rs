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

impl CtcTelemetry {
    /// Merges a freshly-parsed `CtcTelemetry` into `self` field-by-field.
    ///
    /// Confirmed against BambuStudio's own `DevChamber::ParseChamberV2_0`
    /// (`src/slic3r/GUI/DeviceCore/DevChamber.cpp`) — it reads `device.ctc.state`
    /// unconditionally the moment `device.ctc` itself is present (no absence guard,
    /// i.e. the official client never expects `state` to arrive independently absent),
    /// but reads `device.ctc.info` behind its own `.contains()` check, i.e. `info` *can*
    /// arrive absent while `state` is present. `self.info` must not be cleared just
    /// because a push carries `ctc.state` without repeating `ctc.info`.
    pub(crate) fn merge_from(&mut self, incoming: &CtcTelemetry) {
        match (&mut self.info, &incoming.info) {
            (Some(cached), Some(new)) => cached.merge_from(new),
            (None, Some(new)) => self.info = Some(new.clone()),
            _ => {}
        }
        if incoming.state.is_some() {
            self.state = incoming.state;
        }
    }
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

impl CtcInfo {
    /// Merges a freshly-parsed `CtcInfo` into `self` field-by-field.
    ///
    /// `target` is a real, independently-arriving wire key — `bambuddy`
    /// (`bambu_mqtt.py:2652`, `if "target" in ctc_info:`) explicitly guards it separately
    /// from `temp`. BambuStudio's `DevChamber.cpp` never reads `target` at all (it derives
    /// both actual and target from the single bit-packed `temp` value instead), so it offers
    /// no counter-evidence, but doesn't need to: `self.info` was previously cloned wholesale
    /// whenever `ctc.info` was present at all, which would silently drop a cached `target` on
    /// any push whose `ctc.info` repeats only `temp`.
    pub(crate) fn merge_from(&mut self, incoming: &CtcInfo) {
        if incoming.temp.is_some() {
            self.temp = incoming.temp;
        }
        if incoming.target.is_some() {
            self.target = incoming.target;
        }
    }
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

impl IpcamTelemetry {
    /// Merges a freshly-parsed `IpcamTelemetry` into `self` field-by-field, instead of
    /// replacing `self` wholesale.
    ///
    /// BambuStudio's `parse_json` (`DeviceManager.cpp:3338-3399`) gates every
    /// `ipcam` field behind its own `.contains()` check, same preserve-on-absence pattern
    /// as `CtcTelemetry`/`BedTelemetry`/`ExtToolTelemetry`.
    pub(crate) fn merge_from(&mut self, incoming: &IpcamTelemetry) {
        if incoming.ipcam_dev.is_some() {
            self.ipcam_dev = incoming.ipcam_dev.clone();
        }
        if incoming.ipcam_record.is_some() {
            self.ipcam_record = incoming.ipcam_record.clone();
        }
        if incoming.timelapse.is_some() {
            self.timelapse = incoming.timelapse.clone();
        }
        if incoming.mode_bits.is_some() {
            self.mode_bits = incoming.mode_bits;
        }
        if incoming.resolution.is_some() {
            self.resolution = incoming.resolution.clone();
        }
        if incoming.tutk_server.is_some() {
            self.tutk_server = incoming.tutk_server.clone();
        }
        if incoming.rtsp_url.is_some() {
            self.rtsp_url = incoming.rtsp_url.clone();
        }
    }
}

/// Raw telemetry entry from the `hms` diagnostic array [REF-DIAG-HMS].
///
/// Each entry represents an active hardware fault or status indication. Use
/// `diagnostics::decode_hms_alert()` to unpack into wiki keys, short-codes, and severity levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HmsEntry {
    /// Packed attribute word encoding module ID, severity, and subsystem address.
    #[serde(default, deserialize_with = "deserialize_permissive_hms_u32")]
    pub attr: u32,
    /// Packed code word encoding fault category and error index.
    #[serde(default, deserialize_with = "deserialize_permissive_hms_u32")]
    pub code: u32,
    /// Seconds since boot when the alert was raised (confirmed present on X2 only; unverified on H2/P2).
    #[serde(default)]
    pub ts_boot: Option<u64>,
    /// UTC timestamp string when the alert was raised (e.g. `"20260426002648"`).
    #[serde(default)]
    pub ts_unix: Option<String>,
}

/// Permissively decodes an `HmsEntry.attr`/`.code` wire value.
///
/// BambuStudio's `ParseHMSItems` (`DevHMS.cpp:42-61`) pushes a default-zeroed
/// item on a malformed entry rather than aborting the whole message; bambuddy
/// (`bambu_mqtt.py:2756-2761`) additionally tolerates hex-string `attr`/`code` values.
/// Accepts a plain integer or a `0x`/`0X`-prefixed hex string; any other shape (including a
/// non-prefixed decimal string) defaults to `0` instead of failing the entire telemetry
/// message's deserialize.
fn deserialize_permissive_hms_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum RawHmsValue {
        Int(u32),
        Str(String),
    }

    Ok(match RawHmsValue::deserialize(deserializer) {
        Ok(RawHmsValue::Int(i)) => i,
        Ok(RawHmsValue::Str(s)) => s
            .strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .unwrap_or(0),
        Err(_) => 0,
    })
}
