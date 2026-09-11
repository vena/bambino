//! Top-level telemetry report envelope (`print` and `device` wire locations).

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use super::ams::{AmsStatusReport, VirtualTray};
use super::device::DeviceTelemetry;
use super::diagnostics::{HmsEntry, IpcamTelemetry};
use super::stage::PrintStage;
use super::xcam::XcamTelemetry;

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

/// One scheduled pause in a running job's pause list.
///
/// Wire keys are single letters (`p`/`t`/`i`/`l`), renamed here to something readable. Every
/// field is `Option` per this module's convention even though BambuStudio's parser requires all
/// four and discards the whole schedule if any is missing — bambino keeps what it can parse and
/// lets the caller decide, rather than dropping a pause list because one point is malformed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrintPausePoint {
    /// Percent complete at which this pause occurs.
    #[serde(rename = "p")]
    pub progress_percent: Option<i32>,

    /// Remaining print time at this pause, in seconds.
    #[serde(rename = "t")]
    pub remaining_time_secs: Option<i32>,

    /// Index of this pause within the job's schedule.
    #[serde(rename = "i")]
    pub pause_index: Option<i32>,

    /// Layer number at which this pause occurs.
    #[serde(rename = "l")]
    pub layer: Option<i32>,
}

/// The pause schedule for a running job, reported as `print.p_list`.
///
/// Lets a consumer see upcoming pauses (typically scheduled filament swaps) before they happen.
/// Nothing in bambino acts on this — it is surfaced so callers can.
///
/// **Field names and semantics come from BambuStudio's parser
/// (`DevPrintTaskInfo.cpp::parsePauseList`), not from a capture taken here.** The abbreviated
/// wire keys in particular have not been confirmed against a real `push_status` from a printer
/// running a job with scheduled pauses — see issue #139.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrintPauseList {
    /// Total number of pauses scheduled for the job.
    pub total: Option<i32>,

    /// The scheduled pauses themselves. Absent and empty are distinct on the wire; both mean
    /// "nothing to show" to a caller.
    pub list: Option<Vec<PrintPausePoint>>,
}

impl PrintPauseList {
    /// Returns the next pending pause — the point with the lowest `pause_index`.
    ///
    /// Points with no `pause_index` are skipped rather than treated as index 0, which would make
    /// a malformed entry masquerade as the next pause. Returns `None` when the list is absent,
    /// empty, or entirely unindexed.
    pub fn next_pause(&self) -> Option<&PrintPausePoint> {
        self.list
            .as_ref()?
            .iter()
            .filter(|p| p.pause_index.is_some())
            .min_by_key(|p| p.pause_index)
    }
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
    ///
    /// Permissive: bambuddy's `_probe_number` coerces this because "firmware is inconsistent
    /// about whether these arrive as ints or as numeric strings". BambuStudio reads it as a
    /// bare `get<int>()` and does not corroborate the string form, so the permissive binding is
    /// defensive rather than confirmed — it costs nothing and cannot fail a frame.
    #[serde(default, deserialize_with = "super::deserialize_permissive_opt_i32")]
    pub layer_num: Option<i32>,

    /// Total layers within the sliced print pipeline.
    /// Wire sends as `total_layer_num`; `total_layers` accepted for compatibility.
    ///
    /// Permissive on the same single-source basis as `layer_num`.
    #[serde(
        alias = "total_layer_num",
        default,
        deserialize_with = "super::deserialize_permissive_opt_i32"
    )]
    pub total_layers: Option<i32>,

    /// Estimated remaining print duration, in **minutes**.
    ///
    /// The wire unit is minutes, not seconds — BambuStudio multiplies by 60 on both parse arms
    /// to reach its own seconds-based `mc_left_time` (`DeviceManager.cpp:3081-3086`), and
    /// bambuddy does the same (`notification_service.py:1163-1169`, "in minutes, convert to
    /// seconds"). Callers wanting seconds must multiply.
    ///
    /// Permissive: BambuStudio branches on `is_string()` here, so the quoted form is real.
    #[serde(default, deserialize_with = "super::deserialize_permissive_opt_i32")]
    pub mc_remaining_time: Option<i32>,

    /// Active speed profile level (1=Silent, 2=Standard, 3=Sport, 4=Ludicrous).
    pub spd_lvl: Option<u8>,

    /// Speed magnitude as a percentage of the nominal feedrate.
    pub spd_mag: Option<u16>,

    /// Motion controller progress percentage (0–100).
    ///
    /// Permissive: BambuStudio branches on `is_string()` (`DeviceManager.cpp:3060-3065`) and
    /// bambuddy coerces via `float()`/`_probe_number`, so the quoted form is confirmed.
    #[serde(default, deserialize_with = "super::deserialize_permissive_opt_i32")]
    pub mc_percent: Option<i32>,

    /// Print sub-stage identifier tracking granular execution phases within the active print stage.
    pub mc_print_sub_stage: Option<i32>,

    /// Motion controller print stage.
    ///
    /// Captures show the quoted form (`"2"`), but BambuStudio parses both an `is_string()` and
    /// an `is_number()` arm, so a bare number must not fail the frame. A numeric wire value is
    /// normalized to its decimal text.
    #[serde(default, deserialize_with = "super::deserialize_permissive_opt_string")]
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

    /// Stage currently executing, drawn from the same ID space as [`Self::stg`]. Leveraged by the quirks engine to verify stg_cur idle anomalies [REF-MQTT-IDLEBUG].
    ///
    /// Emitted in incremental pushes, so it is usable for real-time stage tracking subject to
    /// the [REF-MQTT-IDLEBUG] `gcode_state` gate — A1/P1 firmware reports `0` ("printing") while
    /// genuinely idle, so the value means nothing unless `gcode_state` is `RUNNING` or `PAUSE`.
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

    /// Camera and recording telemetry. Nested as `print.ipcam` on the wire.
    pub ipcam: Option<IpcamTelemetry>,

    /// AI detection settings (spaghetti detection, first-layer inspection, etc.).
    ///
    /// Appears to be pushall-only — no incremental `msg: 1` frame in this repo's captures
    /// carries it. Prefer [`crate::client::PrinterClient::xcam`], which caches and merges it.
    pub xcam: Option<XcamTelemetry>,

    /// AMS expansion bus status container [REF-AMS-DECODE].
    pub ams: Option<AmsStatusReport>,

    /// Schedule of pauses the firmware plans for the running job. Nested as `print.p_list`.
    ///
    /// Present only while a job with scheduled pauses (e.g. filament swaps) is loaded; absent
    /// otherwise, which is not an error.
    pub p_list: Option<PrintPauseList>,

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

    /// Second capability bitfield (hex string), distinct from [`fun`](Self::fun).
    ///
    /// Carries the printer's own firmware capability flags — most importantly bit 5,
    /// remote-dry support. Read via [`TelemetryReport::fun2_bit`] rather than directly:
    /// BambuStudio notes this string "may have infinite length" (`DeviceManager.cpp:4464`) and
    /// reads it with a no-border bit extractor, so it must not be parsed into a fixed-width
    /// integer the way `fun` is.
    #[serde(default)]
    pub fun2: Option<String>,

    /// Print source identifier (e.g. `"cloud"`, `"local"`, `"idle"`).
    #[serde(default)]
    pub print_type: Option<String>,

    /// Chamber/work/heatbed light states array.
    #[serde(default)]
    pub lights_report: Option<Vec<LightReport>>,

    /// File download progress percentage (sent as string).
    #[serde(default)]
    pub gcode_file_prepare_percent: Option<String>,

    /// Legacy main-extruder filament sensor state -- **not** a boolean, and not
    /// per-extruder.
    ///
    /// BambuStudio assigns this value unmodified to `MAIN_EXTRUDER_ID` only
    /// (`DeviceManager.cpp`, `parse_json`) and never bitmask-decodes it, so no
    /// interpretation beyond "non-zero means the main extruder reports filament" is
    /// confirmed. Dual-nozzle hardware (H2S/P2S/X2D-class) is observed sending values
    /// above 1 (`2` and `3` in captured telemetry), so a `== 1` comparison misreads
    /// those models.
    ///
    /// For per-extruder filament state on dual-nozzle models, read
    /// [`ExtruderCollection`](super::device::ExtruderCollection) /
    /// [`ExtruderInfo`](super::device::ExtruderInfo) instead, which model the V2
    /// per-extruder `info` bit field BambuStudio actually uses for the deputy extruder
    /// (`DevExtruderSystem.cpp`, `ExterSystemParser::ParseV2_0`).
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

    /// Stage queue for the run in progress — the stages still to execute, emptied to `[]` at
    /// `FINISH`.
    ///
    /// Emitted in incremental (`msg: 1`) pushes, not only in `pushall` — see [REF-MQTT-IDLEBUG],
    /// which corrects an earlier claim to the contrary. For a standalone `calibration` command
    /// the queue tracks the option bitmask: bed-leveling alone gives `[14, 1]`, bed-leveling
    /// plus vibration compensation gives `[14, 1, 3]` (P1S, firmware `01.10.00.00`). Stage IDs
    /// follow BambuStudio's `get_stage_string` table; decode them with [`Self::stage_queue`].
    ///
    /// Reflects what the firmware actually accepted: unsupported option bits are dropped from
    /// the queue without an error or a failed ack. [`start_calibration()`](crate::client::PrinterClient::start_calibration)
    /// masks those bits up front so a caller does not have to diff this against the request.
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

    /// Which plate of a multi-plate 3MF the current job was sliced for.
    ///
    /// Needed to pull the right plate's metadata — thumbnail, filament list, bed temperature —
    /// out of the project file, since a 3MF's per-plate data is indexed on exactly this. It is
    /// also authoritative over the 3MF's own `slice_info`, which can name a different plate on
    /// a retained or reused archive.
    ///
    /// Firmware sends this as **either a number or a decimal string** — BambuStudio branches on
    /// `is_number()` / `is_string()` for exactly this field (`DeviceManager.cpp:2617-2626`), so
    /// the permissive deserializer is load-bearing rather than defensive: a bare `Option<i32>`
    /// would fail the entire telemetry frame on the string form.
    #[serde(default, deserialize_with = "super::deserialize_permissive_opt_i32")]
    pub plate_idx: Option<i32>,

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
    /// Returns the stage currently executing, decoded, or `None` when it cannot be trusted.
    ///
    /// Applies the [REF-MQTT-IDLEBUG] gate: A1/P1 firmware reports `stg_cur = 0` ("printing")
    /// while genuinely idle, so this returns `None` unless `gcode_state` is `RUNNING` or `PAUSE`.
    /// That gate matters more once the value is typed than it did when it was a bare `i32` — a
    /// [`PrintStage::Printing`] rendered in a UI reads as authoritative. Use
    /// [`Self::current_stage_ungated`] only when you are applying your own gate.
    ///
    /// A `Some(PrintStage::Idle)` during a run is not a bug and not completion: after the last
    /// queued stage finishes, `stg_cur` reads idle for the tail of the run.
    pub fn current_stage(&self) -> Option<PrintStage> {
        let state = self.gcode_state.as_deref()?;
        if !matches!(state, "RUNNING" | "PAUSE") {
            return None;
        }
        self.current_stage_ungated()
    }

    /// Decodes `stg_cur` with no [REF-MQTT-IDLEBUG] gate applied.
    ///
    /// Prefer [`Self::current_stage`]. This exists for callers applying their own state gate;
    /// on an A1 or P1 the raw value is `0` ("printing") when the machine is idle.
    pub fn current_stage_ungated(&self) -> Option<PrintStage> {
        self.stg_cur.map(PrintStage::from_wire)
    }

    /// Decodes the queued stage list, in wire order.
    ///
    /// Needs no state gate — the idle-bug anomaly is specific to `stg_cur`, and an empty or
    /// absent queue is unambiguous. Returns an empty `Vec` when `stg` is absent; the queue also
    /// legitimately empties to `[]` at `FINISH`.
    pub fn stage_queue(&self) -> Vec<PrintStage> {
        self.stg
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|&id| PrintStage::from_wire(id))
            .collect()
    }

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
