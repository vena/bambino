#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::types::{AmsStatusReport, DeviceTelemetry, HmsEntry, VirtualTray};

use super::types::PrintProgress;

/// Cached "last-observed" telemetry values, updated by `PrinterClient::poll_telemetry()`.
/// Each field independently keeps its most recently observed value — a telemetry message
/// that omits a field leaves the previously-cached value in place (see the accessor methods
/// on `PrinterClient` for the public read API over this cache).
#[derive(Debug, Clone, Default)]
pub(crate) struct TelemetryCache {
    pub(crate) last_home_flag: Option<u32>,
    pub(crate) last_gcode_state: Option<String>,
    pub(crate) last_door_open: Option<bool>,
    pub(crate) last_print_error: Option<u32>,
    pub(crate) last_progress: PrintProgress,
    pub(crate) last_bed_temper: Option<f64>,
    pub(crate) last_bed_target_temper: Option<f64>,
    pub(crate) last_device: Option<DeviceTelemetry>,
    pub(crate) last_ams: Option<AmsStatusReport>,
    pub(crate) last_vt_tray: Option<VirtualTray>,
    pub(crate) last_vir_slot: Option<Vec<VirtualTray>>,
    pub(crate) last_nozzle_temper: Option<f64>,
    pub(crate) last_nozzle_target_temper: Option<f64>,
    pub(crate) last_chamber_temper: Option<f64>,
    pub(crate) last_hms: Option<Vec<HmsEntry>>,
    pub(crate) last_cooling_fan_speed: Option<String>,
    pub(crate) last_big_fan1_speed: Option<String>,
    pub(crate) last_big_fan2_speed: Option<String>,
    pub(crate) last_heatbreak_fan_speed: Option<String>,
    pub(crate) last_spd_lvl: Option<u8>,
    pub(crate) last_spd_mag: Option<u16>,
    pub(crate) last_wifi_signal: Option<String>,
}
