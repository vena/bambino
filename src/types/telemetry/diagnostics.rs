//! Diagnostic telemetry types (HMS alerts, light reports).

#[cfg(not(feature = "std"))]
use alloc::string::String;

use super::merge::{Mergeable, keep_new, merge_opt};
use super::temps::{HeaterTemps, unpack_temperature};
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

impl Mergeable for CtcTelemetry {
    /// Merges a freshly-parsed `CtcTelemetry` into `self` field-by-field.
    ///
    /// Confirmed against BambuStudio's own `DevChamber::ParseChamberV2_0`
    /// (`src/slic3r/GUI/DeviceCore/DevChamber.cpp`) — it reads `device.ctc.state`
    /// unconditionally the moment `device.ctc` itself is present (no absence guard,
    /// i.e. the official client never expects `state` to arrive independently absent),
    /// but reads `device.ctc.info` behind its own `.contains()` check, i.e. `info` *can*
    /// arrive absent while `state` is present. `self.info` must not be cleared just
    /// because a push carries `ctc.state` without repeating `ctc.info`.
    fn merge_from(&mut self, incoming: &Self) {
        let Self { info, state } = incoming;
        merge_opt(&mut self.info, info);
        keep_new(&mut self.state, state);
    }
}

/// Controller information segment detailing current temperature coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CtcInfo {
    /// Composite-packed integer temperature value [REF-THER-DECODE]; decode with
    /// [`temperatures()`](Self::temperatures).
    pub temp: Option<u32>,

    /// Explicit CTC target temperature (authoritative on new-gen models).
    #[serde(default)]
    pub target: Option<u32>,
}

impl CtcInfo {
    /// The chamber controller's temperatures: `temp` unpacked, with `target` overriding the packed target when present.
    ///
    /// `target` is the authoritative target on new-gen models (bambuddy reads it separately,
    /// `bambu_mqtt.py:2652`); BambuStudio derives both halves from the packed `temp`. `None` when
    /// `temp` is absent.
    #[must_use]
    pub fn temperatures(&self) -> Option<HeaterTemps> {
        let mut temps = unpack_temperature(f64::from(self.temp?));
        if let Some(target) = self.target {
            temps.target = u16::try_from(target).unwrap_or(u16::MAX);
        }
        Some(temps)
    }
}

impl Mergeable for CtcInfo {
    /// Merges a freshly-parsed `CtcInfo` into `self` field-by-field.
    ///
    /// `target` is a real, independently-arriving wire key — `bambuddy`
    /// (`bambu_mqtt.py:2652`, `if "target" in ctc_info:`) explicitly guards it separately
    /// from `temp`. BambuStudio's `DevChamber.cpp` never reads `target` at all (it derives
    /// both actual and target from the single bit-packed `temp` value instead), so it offers
    /// no counter-evidence, but doesn't need to: `self.info` was previously cloned wholesale
    /// whenever `ctc.info` was present at all, which would silently drop a cached `target` on
    /// any push whose `ctc.info` repeats only `temp`.
    fn merge_from(&mut self, incoming: &Self) {
        let Self { temp, target } = incoming;
        keep_new(&mut self.temp, temp);
        keep_new(&mut self.target, target);
    }
}

/// Camera and recording state telemetry, nested as `print.ipcam` on the wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcamTelemetry {
    /// Internal identifier or state of the hardware camera module.
    pub ipcam_dev: Option<String>,

    /// Camera live feed recording status (`"enable"` or `"disable"`); see
    /// [`recording`](Self::recording).
    pub ipcam_record: Option<String>,

    /// Frame-by-layer timelapse recording status (`"enable"` or `"disable"`); see
    /// [`timelapse_enabled`](Self::timelapse_enabled).
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

/// Decodes a wire `"enable"`/`"disable"` toggle; `None` for absent or any other value.
fn enable_flag(value: Option<&str>) -> Option<bool> {
    match value? {
        "enable" => Some(true),
        "disable" => Some(false),
        _ => None,
    }
}

impl IpcamTelemetry {
    /// Whether live-feed recording is on, from `ipcam_record`.
    #[must_use]
    pub fn recording(&self) -> Option<bool> {
        enable_flag(self.ipcam_record.as_deref())
    }

    /// Whether timelapse recording is on, from `timelapse`.
    #[must_use]
    pub fn timelapse_enabled(&self) -> Option<bool> {
        enable_flag(self.timelapse.as_deref())
    }

    /// Whether the TUTK cloud-relay server is on, from `tutk_server`.
    #[must_use]
    pub fn tutk_server_enabled(&self) -> Option<bool> {
        enable_flag(self.tutk_server.as_deref())
    }
}

impl Mergeable for IpcamTelemetry {
    /// Merges a freshly-parsed `IpcamTelemetry` into `self` field-by-field, instead of
    /// replacing `self` wholesale.
    ///
    /// BambuStudio's `parse_json` (`DeviceManager.cpp:3338-3399`) gates every
    /// `ipcam` field behind its own `.contains()` check, same preserve-on-absence pattern
    /// as `CtcTelemetry`/`BedTelemetry`/`ExtToolTelemetry`.
    fn merge_from(&mut self, incoming: &Self) {
        let Self {
            ipcam_dev,
            ipcam_record,
            timelapse,
            mode_bits,
            resolution,
            tutk_server,
            rtsp_url,
        } = incoming;
        keep_new(&mut self.ipcam_dev, ipcam_dev);
        keep_new(&mut self.ipcam_record, ipcam_record);
        keep_new(&mut self.timelapse, timelapse);
        keep_new(&mut self.mode_bits, mode_bits);
        keep_new(&mut self.resolution, resolution);
        keep_new(&mut self.tutk_server, tutk_server);
        keep_new(&mut self.rtsp_url, rtsp_url);
    }
}

/// Raw telemetry entry from the `hms` diagnostic array [REF-DIAG-HMS].
///
/// Each entry represents an active hardware fault or status indication. Use
/// `diagnostics::decode_hms_alert()` to unpack into wiki keys, short-codes, and severity levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HmsEntry {
    /// Packed attribute word encoding module ID, severity, and subsystem address.
    #[serde(default, deserialize_with = "super::deserialize_permissive_hms_u32")]
    pub attr: u32,
    /// Packed code word encoding fault category and error index.
    #[serde(default, deserialize_with = "super::deserialize_permissive_hms_u32")]
    pub code: u32,
    /// Seconds since boot when the alert was raised (confirmed present on X2 only; unverified on H2/P2).
    #[serde(default)]
    pub ts_boot: Option<u64>,
    /// When the alert was raised, as the calendar string `YYYYMMDDHHmmss` (e.g.
    /// `"20260426002648"`) — **not** Unix epoch seconds, despite the wire key `ts_unix`.
    ///
    /// Read from the printer's own clock, which LAN-mode printers don't keep synced, so it is
    /// neither guaranteed UTC nor comparable with host time.
    #[serde(default, rename = "ts_unix")]
    pub ts_local: Option<String>,
}
