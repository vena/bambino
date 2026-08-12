//! Top-level telemetry report envelope (`print` and `device` wire locations).

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use super::ams::{AmsStatusReport, VirtualTray};
use super::device::DeviceTelemetry;
use super::diagnostics::{HmsEntry, IpcamTelemetry};

/// Chamber/work/heatbed light state entry from the `lights_report` array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightReport {
    /// Light identifier (e.g. "chamber_light", "work_light").
    #[serde(default)]
    pub node: String,
    /// Current state (e.g. "on", "off", "flashing").
    #[serde(default)]
    pub mode: String,
}

/// Core printer state machine telemetry, containing kinematics, thermal targets, auxiliary fan configurations, and connected AMS arrays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterTelemetry {
    /// High-level execution status of the G-code processor (e.g., "IDLE", "RUNNING", "PAUSE").
    pub gcode_state: Option<String>,

    /// Wire command name this frame arrived under (`"push_status"`/`"pushall"` for genuine
    /// telemetry pushes; a command-echo response — e.g. `"extrusion_cali_get"` — shares this
    /// same `print` envelope and can otherwise deserialize as an emptyish telemetry report
    /// [see poll_telemetry's command-echo filter].
    #[serde(default)]
    pub command: Option<String>,

    /// Path or parent project file currently loaded for execution.
    pub gcode_file: Option<String>,

    /// User-assigned name of the active print queue task.
    pub subtask_name: Option<String>,

    /// Hardware-enforced unique 32-bit transaction identifier tracking active jobs.
    pub subtask_id: Option<String>,

    /// Active layer progress tracker.
    pub layer_num: Option<i32>,

    /// Total layers within the sliced print pipeline.
    /// Wire sends as `total_layer_num`; `total_layers` accepted for compatibility.
    #[serde(alias = "total_layer_num")]
    pub total_layers: Option<i32>,

    /// Estimated remaining duration of the active layer sequence, in seconds.
    pub mc_remaining_time: Option<i32>,

    /// Active speed profile level (1=Silent, 2=Standard, 3=Sport, 4=Ludicrous).
    pub spd_lvl: Option<u8>,

    /// Speed magnitude as a percentage of the nominal feedrate.
    pub spd_mag: Option<u16>,

    /// Motion controller progress percentage (0–100).
    pub mc_percent: Option<i32>,

    /// Print sub-stage identifier tracking granular execution phases within the active print stage.
    pub mc_print_sub_stage: Option<i32>,

    /// Motion controller print stage string.
    #[serde(default)]
    pub mc_print_stage: Option<String>,

    /// Kinematics flag field tracking homing states, networking interfaces, and door nodes.
    ///
    /// Transmitted as a signed 32-bit int on the wire [REF-HOMEFLAG]; bit 31 set produces a
    /// negative JSON number that a bare `u32` target rejects, failing the whole telemetry
    /// message's deserialize. Masked into `u32` via `deserialize_signed_as_u32`.
    #[serde(default, deserialize_with = "deserialize_signed_as_u32")]
    pub home_flag: Option<u32>,

    /// State field used in newer enclosed printer lines to track sensors (e.g., door status hex strings).
    pub stat: Option<String>,

    /// Active print stage. Leveraged by the quirks engine to verify stg_cur idle anomalies [REF-MQTT-IDLEBUG].
    pub stg_cur: Option<i32>,

    /// Active error code register, packed as a 32-bit integer [REF-DIAG-HMS].
    pub print_error: Option<u32>,

    /// Active hardware fault and diagnostic alert entries [REF-DIAG-HMS].
    #[serde(default)]
    pub hms: Option<Vec<HmsEntry>>,

    /// Permissive indicator tracking physical MicroSD card insertion.
    ///
    /// Evaluated via custom deserializer to absorb structural variations between firmwares.
    #[serde(deserialize_with = "super::deserialize_permissive_bool", default)]
    pub sdcard: bool,

    /// Raw wireless network reception scale returned as a formatted string (e.g. "-52dBm").
    pub wifi_signal: Option<String>,

    /// Network interface state, nested as `print.net` on the wire.
    #[serde(default)]
    pub net: Option<NetInfo>,

    /// On-board part cooling fan speed (represented as discrete steps 0 to 15) [REF-CLIM-FANS].
    pub cooling_fan_speed: Option<String>,

    /// On-board left-side auxiliary fan speed (represented as discrete steps 0 to 15).
    pub big_fan1_speed: Option<String>,

    /// On-board filtration or chamber exhaust fan speed (represented as discrete steps 0 to 15).
    pub big_fan2_speed: Option<String>,

    /// On-board toolhead heatbreak fan speed (represented as discrete steps 0 to 15).
    pub heatbreak_fan_speed: Option<String>,

    /// Hotend target temperature register.
    ///
    /// Wire sends both integers and floats depending on model. Never composite-packed —
    /// unlike `chamber_temper`, no `unpack_temperature()` call is needed here.
    pub nozzle_target_temper: Option<f64>,

    /// Hotend actual temperature register.
    ///
    /// Wire sends both integers and floats depending on model [REF-THER-DECODE].
    pub nozzle_temper: Option<f64>,

    /// Heated build-plate temperature register (actual value; never composite-packed).
    pub bed_temper: Option<f64>,

    /// Explicit bed target temperature. Separate from composite-packed `bed_temper`.
    pub bed_target_temper: Option<f64>,

    /// Active chamber heater or sensor telemetry (actual, target, or composite packed).
    pub chamber_temper: Option<f64>,

    /// Hexadecimal bitmask string representing the physical presence of loaded spools.
    pub tray_exist_bits: Option<String>,

    /// Power status of the printer core logic board.
    #[serde(default)]
    pub power_on_flag: Option<bool>,

    /// Camera and recording telemetry. Nested as `print.ipcam` on the wire.
    pub ipcam: Option<IpcamTelemetry>,

    /// AI detection settings (spaghetti detection, first-layer inspection, etc.).
    pub xcam: Option<serde_json::Value>,

    /// AMS expansion bus status container [REF-AMS-DECODE].
    pub ams: Option<AmsStatusReport>,

    /// Combined AMS state bitmask (lower 8 bits = sub status, bits 8–15 = main status).
    #[serde(default)]
    pub ams_status: Option<i32>,

    /// Slicer-mapped material assignment channels configured during print dispatch [REF-AMS-MAP].
    #[serde(default)]
    pub ams_mapping: Vec<i32>,

    /// Virtual/external spool holder state on single-nozzle platforms (P1S, P1P, A1, X1C, H2S).
    /// Dual-nozzle IDEX platforms (H2D, H2D Pro, X2D) report `vir_slot` instead [REF-AMS-DECODE].
    pub vt_tray: Option<VirtualTray>,

    /// IDEX external spool holder array. Each entry uses the same schema as `VirtualTray`.
    #[serde(default)]
    pub vir_slot: Option<Vec<VirtualTray>>,

    /// Device sub-object nested inside pushall `print` envelope on H2/P2/X2 models.
    /// Contains CTC, nozzle, and airduct telemetry for enclosed printers.
    pub device: Option<DeviceTelemetry>,

    /// Developer LAN Mode bitmask field (hex string) nested inside `print` [REF-MQTT-ENV §3.2.1].
    pub fun: Option<String>,

    /// Print source identifier (e.g. `"cloud"`, `"local"`, `"idle"`).
    #[serde(default)]
    pub print_type: Option<String>,

    /// Chamber/work/heatbed light states array.
    #[serde(default)]
    pub lights_report: Option<Vec<LightReport>>,

    /// File download progress percentage (sent as string).
    #[serde(default)]
    pub gcode_file_prepare_percent: Option<String>,

    /// Extruder filament sensor state (1 = filament present).
    #[serde(default)]
    pub hw_switch_state: Option<i32>,

    /// Skipped object IDs during selective printing.
    #[serde(default)]
    pub s_obj: Option<Vec<i32>>,

    /// Legacy single-nozzle type string (pre-IDEX models).
    #[serde(default)]
    pub nozzle_type: Option<String>,

    /// Legacy single-nozzle diameter string (pre-IDEX models).
    #[serde(default)]
    pub nozzle_diameter: Option<String>,

    /// Fan gear composite bitmask.
    #[serde(default)]
    pub fan_gear: Option<u32>,

    /// G-code action state (H2/X2 models).
    #[serde(default)]
    pub print_gcode_action: Option<i32>,

    /// Real action state (H2/X2 models).
    #[serde(default)]
    pub print_real_action: Option<i32>,

    /// Cloud task identifier.
    #[serde(default)]
    pub task_id: Option<String>,

    /// Cloud job identifier.
    #[serde(default)]
    pub job_id: Option<String>,

    /// Alternative remaining time field (minutes).
    #[serde(default)]
    pub remain_time: Option<i32>,

    /// Hex config bitmask string (bit 18 = AMS Filament Backup).
    #[serde(default)]
    pub cfg: Option<String>,

    /// Calibration stage list.
    #[serde(default)]
    pub stg: Option<Vec<i32>>,

    /// IDEX AMS-to-extruder mapping array.
    #[serde(default)]
    pub mapping: Option<Vec<i32>>,

    /// Print start timestamp string.
    #[serde(default)]
    pub gcode_start_time: Option<String>,

    /// Calibration version identifier.
    #[serde(default)]
    pub cali_version: Option<i32>,

    /// Error string field.
    #[serde(default)]
    pub err: Option<String>,

    /// Failure reason description.
    #[serde(default)]
    pub fail_reason: Option<String>,

    /// Cloud canvas project ID.
    #[serde(default)]
    pub canvas_id: Option<String>,

    /// Cloud design ID.
    #[serde(default)]
    pub design_id: Option<String>,

    /// Cloud model ID.
    #[serde(default)]
    pub model_id: Option<String>,

    /// Cloud profile ID.
    #[serde(default)]
    pub profile_id: Option<String>,

    /// Cloud project ID.
    #[serde(default)]
    pub project_id: Option<String>,

    /// Cloud batch ID.
    #[serde(default)]
    pub batch_id: Option<String>,
}

/// Network interface state from `print.net` [REF-NET-PORTS].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetInfo {
    /// Bitmask; bit 0 (`0x1`) set means wired Ethernet is the active connection.
    #[serde(default)]
    pub conf: Option<u32>,
}

/// Masks a signed wire value (`home_flag` can carry bit 31 set, read by firmware as negative)
/// into its `u32` bit pattern instead of rejecting it. Mirrors [REF-HOMEFLAG]'s documented
/// `flag & 0xFFFFFFFF` handling.
fn deserialize_signed_as_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<i64> = Option::deserialize(deserializer)?;
    Ok(raw.map(|v| v as u32))
}

pub(crate) const TEMP_COMPOSITE_THRESHOLD: u32 = 500;
pub(crate) const DOOR_SENSOR_BITMASK: u32 = 0x00800000;
pub(crate) const NET_CONF_WIRED_BITMASK: u32 = 0x1;
pub(crate) const POWER_220V_BITMASK: u32 = 0x0000_0008;
pub(crate) const SDCARD_STATE_SHIFT: u32 = 8;
pub(crate) const SDCARD_STATE_MASK: u32 = 0x3;

/// SD-card presence/health state, decoded from `home_flag` bits 8–9.
///
/// Confirmed against BambuStudio's `MachineObject::parse_json` (`DeviceManager.cpp:1092`:
/// `m_storage->set_sdcard_state(get_flag_bits(flag, 8, 2))`) and corroborated by pybambu's
/// `const.py:265-266`/`models.py:3408-3412` (same bits). The `sdcard` boolean field can never
/// report a degraded state — only this bitmask distinguishes "no card," "normal," "abnormal,"
/// and "read-only."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SdcardState {
    /// No SD card physically present.
    NoSdcard,
    /// SD card present and functioning normally.
    Normal,
    /// SD card present but reporting an abnormal/error condition.
    Abnormal,
    /// SD card present but mounted read-only.
    ReadOnly,
}

impl SdcardState {
    fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::NoSdcard,
            1 => Self::Normal,
            2 => Self::Abnormal,
            3 => Self::ReadOnly,
            _ => unreachable!("bits masked to 2 bits, only 0-3 possible"),
        }
    }
}


impl PrinterTelemetry {
    /// Resolves the actual and target values from a composite packed temperature [REF-THER-DECODE].
    ///
    /// Accepts `f64` because the wire sends both integers and floats depending on model.
    /// Values ≤ 500 are direct temperatures (target assumed 0°C). Values > 500 are
    /// composite-packed: upper 16 bits = target, lower 16 bits = actual.
    pub fn unpack_temperature(raw_val: f64) -> (u16, u16) {
        let int_val = raw_val as u32;
        if int_val <= TEMP_COMPOSITE_THRESHOLD {
            (int_val as u16, 0)
        } else {
            let target = (int_val >> 16) & 0xFFFF;
            let actual = int_val & 0xFFFF;
            (actual as u16, target as u16)
        }
    }

    /// Evaluates whether the physical printer is connected via wired Ethernet [REF-NET-PORTS].
    ///
    /// Previously inspected bit 18 (`0x00040000`) of `home_flag`, following a
    /// pybambu-sourced heuristic. Both first-party clients (BambuStudio's
    /// `DevPrintOptions.cpp:26`, OrcaSlicer identically) actually decode that bit as
    /// `is_support_prompt_sound_detection`, unrelated to networking — confirmed wrong, not
    /// merely disputed. Real wired-ethernet state comes from `print.net.conf` bit 0
    /// (`DeviceManager.cpp:3053`: `network_wired = (net.conf & 0x1) != 0`). Returns `false`
    /// (not `None`) when `net`/`net.conf` haven't been observed yet, matching
    /// `is_ethernet_active_via_wifi_signal()`'s existing no-signal-observed convention.
    pub fn is_ethernet_active(&self) -> bool {
        self.net
            .as_ref()
            .and_then(|net| net.conf)
            .map(|conf| (conf & NET_CONF_WIRED_BITMASK) != 0)
            .unwrap_or(false)
    }

    /// Evaluates whether the physical printer is connected via wired Ethernet using the `wifi_signal` sentinel value [REF-NET-PORTS], as a fallback for firmware that doesn't populate `print.net.conf`.
    ///
    /// A printer with no wifi signal to report (i.e. running wired-only) sends a fixed
    /// `wifi_signal` of `"-90dBm"`. Prefer `is_ethernet_active()` — this heuristic is kept
    /// only as a fallback for firmware that doesn't send `net.conf`.
    pub fn is_ethernet_active_via_wifi_signal(&self) -> bool {
        self.wifi_signal.as_deref() == Some("-90dBm")
    }

    /// Evaluates whether the printer's mains power supply is wired for the 220V region, based on bit 3 (`0x00000008`) of the `home_flag` register.
    ///
    /// Used by [`crate::quirks::ModelQuirks::bed_temp_max`] on X1C, where the safe bed
    /// temperature ceiling is genuinely voltage-dependent (110°C @220V, 120°C @110V per the
    /// official spec sheet.
    pub fn is_220v_power(&self) -> bool {
        self.home_flag
            .map(|flag| (flag & POWER_220V_BITMASK) != 0)
            .unwrap_or(false)
    }

    /// Evaluates the SD-card presence/health state from `home_flag` bits 8–9. See
    /// [`SdcardState`]'s doc comment for verification sources. Returns `None` before any
    /// telemetry carrying `home_flag` has been observed — distinct from `Some(NoSdcard)`.
    pub fn sdcard_state(&self) -> Option<SdcardState> {
        self.home_flag
            .map(|flag| SdcardState::from_bits((flag >> SDCARD_STATE_SHIFT) & SDCARD_STATE_MASK))
    }

    /// Reads door sensor state from bit 23 of the `home_flag` register [REF-NET-DOOR].
    ///
    /// Used by X1 series models where the door sensor is wired to the home_flag bitmask.
    pub fn is_door_open_from_home_flag(&self) -> bool {
        self.home_flag
            .map(|flag| (flag & DOOR_SENSOR_BITMASK) != 0)
            .unwrap_or(false)
    }

    /// Reads door sensor state from bit 23 of the parsed hexadecimal `stat` field [REF-NET-DOOR].
    ///
    /// Used by H2, P2, and X2 series models where the door sensor state is encoded in the `stat` string.
    pub fn is_door_open_from_stat(&self) -> bool {
        self.stat
            .as_ref()
            .and_then(|s| Self::parse_hex_string(s))
            .map(|val| (val & DOOR_SENSOR_BITMASK) != 0)
            .unwrap_or(false)
    }

    /// Helper converting raw hexadecimal state strings cleanly into standard numeric values.
    pub(crate) fn parse_hex_string(hex_str: &str) -> Option<u32> {
        let clean = hex_str
            .strip_prefix("0x")
            .or_else(|| hex_str.strip_prefix("0X"))
            .unwrap_or(hex_str);
        u32::from_str_radix(clean, 16).ok()
    }
}
