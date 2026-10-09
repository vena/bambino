*[bambino](../../../index.md) / [types](../../index.md) / [telemetry](../index.md) / [report](index.md)*

---

# Module `report`

Top-level telemetry report envelope (`print` and `device` wire locations).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`LightReport`](#lightreport) | struct | Chamber/work/heatbed light state entry from the `lights_report` array. |
| [`NetInfo`](#netinfo) | struct | Network interface state from `print.net` [REF-NET-PORTS]. |
| [`PrintPauseList`](#printpauselist) | struct | The pause schedule for a running job, reported as `print.p_list`. |
| [`PrintPausePoint`](#printpausepoint) | struct | One scheduled pause in a running job's pause list. |
| [`PrinterTelemetry`](#printertelemetry) | struct | Core printer state machine telemetry, containing kinematics, thermal targets, auxiliary fan configurations, and connected AMS arrays. |
| [`SdcardState`](#sdcardstate) | enum | SD-card presence/health state, decoded from a two-bit field (`aux` bits 12–13 or `home_flag` bits 8–9). |

## Types

### `LightReport`

```rust
struct LightReport {
    pub node: Option<String>,
    pub mode: Option<String>,
}
```

Chamber/work/heatbed light state entry from the `lights_report` array.

#### Fields

- **`node`**: `Option<String>`

  Light identifier (e.g. "chamber_light", "work_light").

- **`mode`**: `Option<String>`

  Current state (e.g. "on", "off", "flashing"); see [`is_on`](#lightreport).

#### Implementations

- <span id="lightreport-led-node"></span>`fn led_node(&self) -> Option<LedNode>` — [`LedNode`](../../control/index.md#lednode)

  The fixture this entry reports, or `None` for an absent or unrecognized node.

- <span id="lightreport-light-mode"></span>`fn light_mode(&self) -> Option<LightMode>` — [`LightMode`](../../control/index.md#lightmode)

  The fixture's mode, or `None` for an absent or unrecognized mode.

- <span id="lightreport-is-on"></span>`fn is_on(&self) -> Option<bool>`

  Whether the light is lit: `true` for `"on"` and `"flashing"`, `false` for `"off"`,
  `None` when the mode is absent or unrecognized.

#### Trait Implementations

##### `impl Clone for LightReport`

- <span id="lightreport-clone"></span>`fn clone(&self) -> LightReport` — [`LightReport`](#lightreport)

##### `impl Debug for LightReport`

- <span id="lightreport-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for LightReport`

- <span id="lightreport-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for LightReport`

##### `impl Serialize for LightReport`

- <span id="lightreport-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `NetInfo`

```rust
struct NetInfo {
    pub conf: Option<u32>,
}
```

Network interface state from `print.net` [REF-NET-PORTS].

#### Fields

- **`conf`**: `Option<u32>`

  Bitmask; bit 0 (`0x1`) set means wired Ethernet is the active connection.

#### Trait Implementations

##### `impl Clone for NetInfo`

- <span id="netinfo-clone"></span>`fn clone(&self) -> NetInfo` — [`NetInfo`](#netinfo)

##### `impl Debug for NetInfo`

- <span id="netinfo-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for NetInfo`

- <span id="netinfo-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for NetInfo`

##### `impl Serialize for NetInfo`

- <span id="netinfo-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrintPauseList`

```rust
struct PrintPauseList {
    pub total: Option<i32>,
    pub list: Option<Vec<PrintPausePoint>>,
}
```

The pause schedule for a running job, reported as `print.p_list`.

Lets a consumer see upcoming pauses (typically scheduled filament swaps) before they happen.
Nothing in bambino acts on this — it is surfaced so callers can.

**Field names and semantics come from BambuStudio's parser
(`DevPrintTaskInfo.cpp::parsePauseList`), not from a capture taken here.** The abbreviated
wire keys in particular have not been confirmed against a real `push_status` from a printer
running a job with scheduled pauses — see issue #139.

#### Fields

- **`total`**: `Option<i32>`

  Total number of pauses scheduled for the job.

- **`list`**: `Option<Vec<PrintPausePoint>>`

  The scheduled pauses themselves. Absent and empty are distinct on the wire; both mean
  "nothing to show" to a caller.

#### Implementations

- <span id="printpauselist-next-pause"></span>`fn next_pause(&self) -> Option<&PrintPausePoint>` — [`PrintPausePoint`](#printpausepoint)

  Returns the next pending pause — the point with the lowest `pause_index`.

  Points with no `pause_index` are skipped rather than treated as index 0, which would make
  a malformed entry masquerade as the next pause. Returns `None` when the list is absent,
  empty, or entirely unindexed.

#### Trait Implementations

##### `impl Clone for PrintPauseList`

- <span id="printpauselist-clone"></span>`fn clone(&self) -> PrintPauseList` — [`PrintPauseList`](#printpauselist)

##### `impl Debug for PrintPauseList`

- <span id="printpauselist-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for PrintPauseList`

- <span id="printpauselist-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for PrintPauseList`

##### `impl Eq for PrintPauseList`

##### `impl PartialEq for PrintPauseList`

- <span id="printpauselist-partialeq-eq"></span>`fn eq(&self, other: &PrintPauseList) -> bool` — [`PrintPauseList`](#printpauselist)

##### `impl Serialize for PrintPauseList`

- <span id="printpauselist-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrintPausePoint`

```rust
struct PrintPausePoint {
    pub progress_percent: Option<i32>,
    pub remaining_time_secs: Option<i32>,
    pub pause_index: Option<i32>,
    pub layer: Option<i32>,
}
```

One scheduled pause in a running job's pause list.

Wire keys are single letters (`p`/`t`/`i`/`l`), renamed here to something readable. Every
field is `Option` per this module's convention even though BambuStudio's parser requires all
four and discards the whole schedule if any is missing — bambino keeps what it can parse and
lets the caller decide, rather than dropping a pause list because one point is malformed.

#### Fields

- **`progress_percent`**: `Option<i32>`

  Percent complete at which this pause occurs.

- **`remaining_time_secs`**: `Option<i32>`

  Remaining print time at this pause, in seconds.

- **`pause_index`**: `Option<i32>`

  Index of this pause within the job's schedule.

- **`layer`**: `Option<i32>`

  Layer number at which this pause occurs.

#### Trait Implementations

##### `impl Clone for PrintPausePoint`

- <span id="printpausepoint-clone"></span>`fn clone(&self) -> PrintPausePoint` — [`PrintPausePoint`](#printpausepoint)

##### `impl Debug for PrintPausePoint`

- <span id="printpausepoint-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for PrintPausePoint`

- <span id="printpausepoint-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for PrintPausePoint`

##### `impl Eq for PrintPausePoint`

##### `impl PartialEq for PrintPausePoint`

- <span id="printpausepoint-partialeq-eq"></span>`fn eq(&self, other: &PrintPausePoint) -> bool` — [`PrintPausePoint`](#printpausepoint)

##### `impl Serialize for PrintPausePoint`

- <span id="printpausepoint-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `PrinterTelemetry`

```rust
struct PrinterTelemetry {
    pub gcode_state: Option<String>,
    pub command: Option<String>,
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub subtask_id: Option<String>,
    pub layer_num: Option<i32>,
    pub total_layers: Option<i32>,
    pub mc_remaining_time: Option<i32>,
    pub spd_lvl: Option<u8>,
    pub spd_mag: Option<u16>,
    pub mc_percent: Option<i32>,
    pub mc_print_sub_stage: Option<i32>,
    pub mc_print_stage: Option<String>,
    pub home_flag: Option<u32>,
    pub stat: Option<String>,
    pub stg_cur: Option<i32>,
    pub print_error: Option<u32>,
    pub hms: Option<Vec<super::diagnostics::HmsEntry>>,
    pub sdcard: Option<bool>,
    pub wifi_signal: Option<String>,
    pub net: Option<NetInfo>,
    pub cooling_fan_speed: Option<String>,
    pub big_fan1_speed: Option<String>,
    pub big_fan2_speed: Option<String>,
    pub heatbreak_fan_speed: Option<String>,
    pub nozzle_target_temper: Option<f64>,
    pub nozzle_temper: Option<f64>,
    pub bed_temper: Option<f64>,
    pub bed_target_temper: Option<f64>,
    pub chamber_temper: Option<f64>,
    pub ipcam: Option<super::diagnostics::IpcamTelemetry>,
    pub xcam: Option<super::xcam::XcamTelemetry>,
    pub ams: Option<super::ams::AmsStatusReport>,
    pub p_list: Option<PrintPauseList>,
    pub ams_status: Option<i32>,
    pub ams_mapping: Vec<i32>,
    pub vt_tray: Option<super::ams::VirtualTray>,
    pub vir_slot: Option<Vec<super::ams::VirtualTray>>,
    pub device: Option<super::device::DeviceTelemetry>,
    pub fun: Option<String>,
    pub fun2: Option<String>,
    pub print_type: Option<String>,
    pub lights_report: Option<Vec<LightReport>>,
    pub gcode_file_prepare_percent: Option<String>,
    pub hw_switch_state: Option<i32>,
    pub s_obj: Option<Vec<i32>>,
    pub nozzle_type: Option<String>,
    pub nozzle_diameter: Option<String>,
    pub fan_gear: Option<u32>,
    pub print_gcode_action: Option<i32>,
    pub print_real_action: Option<i32>,
    pub task_id: Option<String>,
    pub job_id: Option<String>,
    pub remain_time: Option<i32>,
    pub cfg: Option<String>,
    pub aux: Option<String>,
    pub flag3: Option<u32>,
    pub stg: Option<Vec<i32>>,
    pub mapping: Option<Vec<i32>>,
    pub gcode_start_time: Option<String>,
    pub cali_version: Option<i32>,
    pub err: Option<String>,
    pub fail_reason: Option<String>,
    pub canvas_id: Option<String>,
    pub design_id: Option<String>,
    pub model_id: Option<String>,
    pub plate_idx: Option<i32>,
    pub profile_id: Option<String>,
    pub project_id: Option<String>,
    pub batch_id: Option<String>,
}
```

Core printer state machine telemetry, containing kinematics, thermal targets, auxiliary fan configurations, and connected AMS arrays.

Most fields mirror the wire as-is. Where a decoded accessor exists, prefer it over reading the
raw field and re-implementing the decode:

| Instead of the raw field | Use |
| :--- | :--- |
| `stg_cur`, `stg` | [`current_stage`](#printertelemetry) (gated on `gcode_state` [REF-MQTT-IDLEBUG]), [`stage_queue`](#printertelemetry) |
| `aux` bits 12–13, `home_flag` bits 8–9, `sdcard` | [`sdcard_status`](#printertelemetry) |
| `gcode_state` | [`print_status`](#printertelemetry) |
| `home_flag` bit 23, `stat` | [`is_door_open_from_home_flag`](#printertelemetry), [`is_door_open_from_stat`](#printertelemetry) |
| `home_flag` bit 3 | [`is_220v_power`](#printertelemetry) |
| `net.conf`, `wifi_signal` | [`is_ethernet_active`](#printertelemetry), with [`is_ethernet_active_via_wifi_signal`](#printertelemetry) as the fallback |
| `gcode_start_time` | [`gcode_start_time_secs`](#printertelemetry) |
| `chamber_temper` (packed) | [`chamber_temperatures`](#printertelemetry) |
| `bed_temper` / `device.bed` | [`TelemetryReport::bed_temperatures`](../index.md#telemetryreport) |
| `nozzle_temper` / `device.extruder` | [`TelemetryReport::nozzle_temperatures`](../index.md#telemetryreport) |
| `*_fan_speed` | [`TelemetryReport::fan_percent`](../index.md#telemetryreport) |
| `device`, `fun`, `fun2` | [`TelemetryReport::device`](../index.md#telemetryreport), [`fun`](../index.md#telemetryreport), [`fun2_bit`](../index.md#telemetryreport) (both wire locations) |
| `ipcam.*` toggles | [`IpcamTelemetry::recording`](../diagnostics/index.md#ipcamtelemetry), [`timelapse_enabled`](../diagnostics/index.md#ipcamtelemetry) |
| `lights_report[]` | [`LightReport::is_on`](#lightreport), [`LightReport::led_node`](#lightreport), [`LightReport::light_mode`](#lightreport) |
| `xcam.cfg`, `xcam.halt_print_sensitivity` | the `XcamTelemetry` detector accessors, [`halt_print_sensitivity_level`](../xcam/index.md#xcamtelemetry) |

#### Fields

- **`gcode_state`**: `Option<String>`

  High-level execution status of the G-code processor (e.g., "IDLE", "RUNNING", "PAUSE").

- **`command`**: `Option<String>`

  Wire command name this frame arrived under (`"push_status"`/`"pushall"` for genuine
  telemetry pushes; a command-echo response — e.g. `"extrusion_cali_get"` — shares this
  same `print` envelope and can otherwise deserialize as an emptyish telemetry report
  [see poll_telemetry's command-echo filter].

- **`gcode_file`**: `Option<String>`

  Path or parent project file currently loaded for execution.

- **`subtask_name`**: `Option<String>`

  User-assigned name of the active print queue task.

- **`subtask_id`**: `Option<String>`

  Hardware-enforced unique 32-bit transaction identifier tracking active jobs.

- **`layer_num`**: `Option<i32>`

  Active layer progress tracker.
  
  Permissive: bambuddy's `_probe_number` coerces this because "firmware is inconsistent
  about whether these arrive as ints or as numeric strings". BambuStudio reads it as a
  bare `get<int>()` and does not corroborate the string form, so the permissive binding is
  defensive rather than confirmed — it costs nothing and cannot fail a frame.

- **`total_layers`**: `Option<i32>`

  Total layers within the sliced print pipeline.
  Wire sends as `total_layer_num`; `total_layers` accepted for compatibility.
  
  Permissive on the same single-source basis as `layer_num`.

- **`mc_remaining_time`**: `Option<i32>`

  Estimated remaining print duration, in **minutes**.
  
  The wire unit is minutes, not seconds — BambuStudio multiplies by 60 on both parse arms
  to reach its own seconds-based `mc_left_time` (`DeviceManager.cpp`), and
  bambuddy does the same (`notification_service.py`, "in minutes, convert to
  seconds"). Callers wanting seconds must multiply.
  
  Permissive: BambuStudio branches on `is_string()` here, so the quoted form is real.

- **`spd_lvl`**: `Option<u8>`

  Active speed profile level (1=Silent, 2=Standard, 3=Sport, 4=Ludicrous).

- **`spd_mag`**: `Option<u16>`

  Speed magnitude as a percentage of the nominal feedrate.

- **`mc_percent`**: `Option<i32>`

  Motion controller progress percentage (0–100).
  
  Permissive: BambuStudio branches on `is_string()` (`DeviceManager.cpp`) and
  bambuddy coerces via `float()`/`_probe_number`, so the quoted form is confirmed.

- **`mc_print_sub_stage`**: `Option<i32>`

  Print sub-stage identifier tracking granular execution phases within the active print stage.

- **`mc_print_stage`**: `Option<String>`

  Motion controller print stage.
  
  Captures show the quoted form (`"2"`), but BambuStudio parses both an `is_string()` and
  an `is_number()` arm, so a bare number must not fail the frame. A numeric wire value is
  normalized to its decimal text.

- **`home_flag`**: `Option<u32>`

  Kinematics flag field tracking homing states, networking interfaces, and door nodes.
  
  Transmitted as a signed 32-bit int on the wire [REF-HOMEFLAG]; bit 31 set produces a
  negative JSON number that a bare `u32` target rejects, failing the whole telemetry
  message's deserialize. Masked into `u32` via `deserialize_permissive_opt_flags`.

- **`stat`**: `Option<String>`

  State field used in newer enclosed printer lines to track sensors (e.g., door status hex strings).

- **`stg_cur`**: `Option<i32>`

  Stage currently executing, drawn from the same ID space as [`Self::stg`](#printertelemetry).
  
  Reads `0` ("printing") while genuinely idle on A1/P1 firmware [REF-MQTT-IDLEBUG], so the
  stage accessor gates it on `gcode_state` for every model rather than per-model quirk.
  
  Emitted in incremental pushes, so it is usable for real-time stage tracking subject to
  the [REF-MQTT-IDLEBUG] `gcode_state` gate — A1/P1 firmware reports `0` ("printing") while
  genuinely idle, so the value means nothing unless `gcode_state` is `RUNNING` or `PAUSE`.

- **`print_error`**: `Option<u32>`

  Active error code register, packed as a 32-bit integer [REF-DIAG-HMS].

- **`hms`**: `Option<Vec<super::diagnostics::HmsEntry>>`

  Active hardware fault and diagnostic alert entries [REF-DIAG-HMS].

- **`sdcard`**: `Option<bool>`

  Top-level MicroSD presence flag, sent as a bool, an integer or a string such as `"HAS_SDCARD_NORMAL"`.
  
  `None` when this frame doesn't carry it — distinct from `Some(false)`, "no card". It
  cannot report a degraded card; prefer [`sdcard_status`](#printertelemetry), which reads
  the richer bit fields first.

- **`wifi_signal`**: `Option<String>`

  Raw wireless network reception scale returned as a formatted string (e.g. "-52dBm").

- **`net`**: `Option<NetInfo>`

  Network interface state, nested as `print.net` on the wire.

- **`cooling_fan_speed`**: `Option<String>`

  On-board part cooling fan speed (represented as discrete steps 0 to 15) [REF-CLIM-FANS].

- **`big_fan1_speed`**: `Option<String>`

  On-board left-side auxiliary fan speed (represented as discrete steps 0 to 15).

- **`big_fan2_speed`**: `Option<String>`

  On-board filtration or chamber exhaust fan speed (represented as discrete steps 0 to 15).

- **`heatbreak_fan_speed`**: `Option<String>`

  On-board toolhead heatbreak fan speed (represented as discrete steps 0 to 15).

- **`nozzle_target_temper`**: `Option<f64>`

  Hotend target temperature register.
  
  Wire sends both integers and floats depending on model. Never composite-packed —
  unlike `chamber_temper`, no unpacking is needed here.

- **`nozzle_temper`**: `Option<f64>`

  Hotend actual temperature register.
  
  Wire sends both integers and floats depending on model [REF-THER-DECODE].

- **`bed_temper`**: `Option<f64>`

  Heated build-plate temperature register (actual value; never composite-packed).

- **`bed_target_temper`**: `Option<f64>`

  Bed target temperature, sent alongside `bed_temper` (old-gen models; never composite-packed).

- **`chamber_temper`**: `Option<f64>`

  Active chamber heater or sensor telemetry (actual, target, or composite packed).

- **`ipcam`**: `Option<super::diagnostics::IpcamTelemetry>`

  Camera and recording telemetry. Nested as `print.ipcam` on the wire.

- **`xcam`**: `Option<super::xcam::XcamTelemetry>`

  AI detection settings (spaghetti detection, first-layer inspection, etc.).
  
  Appears to be pushall-only — no incremental `msg: 1` frame in this repo's captures
  carries it. Prefer [`xcam`](../xcam/index.md), which caches and merges it.

- **`ams`**: `Option<super::ams::AmsStatusReport>`

  AMS expansion bus status container [REF-AMS-DECODE].

- **`p_list`**: `Option<PrintPauseList>`

  Schedule of pauses the firmware plans for the running job. Nested as `print.p_list`.
  
  Present only while a job with scheduled pauses (e.g. filament swaps) is loaded; absent
  otherwise, which is not an error.

- **`ams_status`**: `Option<i32>`

  Combined AMS state bitmask (lower 8 bits = sub status, bits 8–15 = main status).

- **`ams_mapping`**: `Vec<i32>`

  Slicer-mapped material assignment channels configured during print dispatch [REF-AMS-MAP].

- **`vt_tray`**: `Option<super::ams::VirtualTray>`

  Virtual/external spool holder state on single-nozzle platforms (P1S, P1P, A1, X1C, H2S).
  Dual-nozzle IDEX platforms (H2D, H2D Pro, X2D) report `vir_slot` instead [REF-AMS-DECODE].

- **`vir_slot`**: `Option<Vec<super::ams::VirtualTray>>`

  IDEX external spool holder array. Each entry uses the same schema as `VirtualTray`.

- **`device`**: `Option<super::device::DeviceTelemetry>`

  Device sub-object nested inside pushall `print` envelope on H2/P2/X2 models.
  Contains CTC, nozzle, and airduct telemetry for enclosed printers.

- **`fun`**: `Option<String>`

  Developer LAN Mode bitmask field (hex string) nested inside `print` [REF-MQTT-ENV §3.2.1].

- **`fun2`**: `Option<String>`

  Second capability bitfield (hex string), distinct from [`fun`](#printertelemetry).
  
  Carries the printer's own firmware capability flags — most importantly bit 5,
  remote-dry support. Read via [`fun2_bit`](../index.md#telemetryreport) rather than directly:
  BambuStudio notes this string "may have infinite length" (`DeviceManager.cpp`) and
  reads it with a no-border bit extractor, so it must not be parsed into a fixed-width
  integer the way `fun` is.

- **`print_type`**: `Option<String>`

  Print source identifier (e.g. `"cloud"`, `"local"`, `"idle"`).

- **`lights_report`**: `Option<Vec<LightReport>>`

  Chamber/work/heatbed light states array.

- **`gcode_file_prepare_percent`**: `Option<String>`

  File download progress percentage (sent as string).

- **`hw_switch_state`**: `Option<i32>`

  Legacy main-extruder filament sensor state -- **not** a boolean, and not
  per-extruder.
  
  BambuStudio assigns this value unmodified to `MAIN_EXTRUDER_ID` only
  (`DeviceManager.cpp`, `parse_json`) and never bitmask-decodes it, so no
  interpretation beyond "non-zero means the main extruder reports filament" is
  confirmed. Dual-nozzle hardware (H2S/P2S/X2D-class) is observed sending values
  above 1 (`2` and `3` in captured telemetry), so a `== 1` comparison misreads
  those models.
  
  For per-extruder filament state on dual-nozzle models, read
  [`ExtruderCollection`](../device/index.md) /
  [`ExtruderInfo`](../device/index.md) instead, which model the V2
  per-extruder `info` bit field BambuStudio actually uses for the deputy extruder
  (`DevExtruderSystem.cpp`, `ExterSystemParser::ParseV2_0`).

- **`s_obj`**: `Option<Vec<i32>>`

  Skipped object IDs during selective printing.

- **`nozzle_type`**: `Option<String>`

  Legacy single-nozzle type string (pre-IDEX models).

- **`nozzle_diameter`**: `Option<String>`

  Legacy single-nozzle diameter string (pre-IDEX models).

- **`fan_gear`**: `Option<u32>`

  Fan gear composite bitmask.

- **`print_gcode_action`**: `Option<i32>`

  G-code action state (H2/X2 models).

- **`print_real_action`**: `Option<i32>`

  Real action state (H2/X2 models).

- **`task_id`**: `Option<String>`

  Cloud task identifier.

- **`job_id`**: `Option<String>`

  Cloud job identifier.

- **`remain_time`**: `Option<i32>`

  Alternative remaining time field (minutes).
  
  Prefer [`mc_remaining_time`](#printertelemetry): it is the field BambuStudio reads
  for the ETA (`DeviceManager.cpp`) and the one `PrinterClient::print_progress`
  tracks. This one is kept for completeness; nothing in either upstream client prefers it.

- **`cfg`**: `Option<String>`

  Hex config bitmask string of user-facing printer settings [REF-MQTT-TELEMETRY].
  
  Bit 18 is AMS Filament Backup (auto-refill), confirmed against both upstreams:
  BambuStudio decodes it as `SetAutoRefillEnabled(get_flag_bits(cfg, 18))`
  (`DeviceManager.cpp`, the `/*cfg*/` block), and `DevFilaSystem::CanShowFilamentBackup()`
  gates the "Filament Backup" UI on that same `IsAutoRefillEnabled()` — the auto-refill
  flag and the feature's user-facing name are the same thing. bambuddy reads the identical
  position in `parse_ams_filament_backup_from_cfg` (`services/bambu_mqtt.py`).
  
  P1P, P1S, A1 and A1 Mini omit `cfg` entirely, so absent is not "off" — hence `Option`.
  Those families carry Filament Backup in [`home_flag`](#printertelemetry) bit 10 instead
  (BambuStudio's `parse_home_flag`; bambuddy #3259).
  
  For decoded settings, use the getters on `PrinterClient` (`prompt_sound_enabled`,
  `filament_backup_enabled`, `door_open_check`, ...): they prefer `cfg`, fall back to
  `home_flag`, and cache across frames, since one frame can't tell `cfg` absent from
  unsent.

- **`aux`**: `Option<String>`

  Auxiliary state hex string, sent only by firmware using BambuStudio's "np" payload format.
  
  Its *presence* is one quarter of BambuStudio's `check_enable_np` probe
  (`DeviceManager.cpp`) — see [`Self::reports_np_format`](#printertelemetry). BambuStudio reads it
  as a string (`DeviceManager.cpp`).

- **`flag3`**: `Option<u32>`

  Third capability bitfield.
  
  Bit 9 is BambuStudio's `is_enable_ams_np`, the AMS-side "np" flag
  (`DeviceManager.cpp`), read alongside the `cfg`/`fun`/`aux`/`stat` probe — see
  [`Self::reports_np_format`](#printertelemetry). Masked into `u32` like [`home_flag`](#printertelemetry).

- **`stg`**: `Option<Vec<i32>>`

  Stage queue for the run in progress — the stages still to execute, emptied to `[]` at
  `FINISH`.
  
  Emitted in incremental (`msg: 1`) pushes, not only in `pushall` — see [REF-MQTT-IDLEBUG],
  which corrects an earlier claim to the contrary. For a standalone `calibration` command
  the queue tracks the option bitmask: bed-leveling alone gives `[14, 1]`, bed-leveling
  plus vibration compensation gives `[14, 1, 3]` (P1S, firmware `01.10.00.00`). Stage IDs
  follow BambuStudio's `get_stage_string` table; decode them with [`Self::stage_queue`](#printertelemetry).
  
  Reflects what the firmware actually accepted: unsupported option bits are dropped from
  the queue without an error or a failed ack. [`start_calibration()`](../../../client/index.md#printerclient)
  masks those bits up front so a caller does not have to diff this against the request.

- **`mapping`**: `Option<Vec<i32>>`

  Per-filament AMS tray mapping of the running print.
  
  Each entry is `ams_id * 256 + slot_id`, and `65535` means unmapped (BambuStudio
  `DeviceManager.cpp`). It is task-level state: an idle H2 keeps reporting the
  previous print's mapping, so it is meaningful only while a print runs.

- **`gcode_start_time`**: `Option<String>`

  Print start time as Unix epoch seconds in a decimal string (e.g. `"1681479206"`); see
  [`gcode_start_time_secs`](#printertelemetry).
  
  Read from the printer's own clock, which LAN-mode printers don't keep synced. Not seen in
  local-print captures; see `reference/03_mqtt_telemetry.md`.

- **`cali_version`**: `Option<i32>`

  Calibration version identifier.

- **`err`**: `Option<String>`

  Error string field.

- **`fail_reason`**: `Option<String>`

  Failure reason description.

- **`canvas_id`**: `Option<String>`

  Cloud canvas project ID.

- **`design_id`**: `Option<String>`

  Cloud design ID.

- **`model_id`**: `Option<String>`

  Cloud model ID.

- **`plate_idx`**: `Option<i32>`

  Which plate of a multi-plate 3MF the current job was sliced for.
  
  Needed to pull the right plate's metadata — thumbnail, filament list, bed temperature —
  out of the project file, since a 3MF's per-plate data is indexed on exactly this. It is
  also authoritative over the 3MF's own `slice_info`, which can name a different plate on
  a retained or reused archive.
  
  Firmware sends this as **either a number or a decimal string** — BambuStudio branches on
  `is_number()` / `is_string()` for exactly this field (`DeviceManager.cpp`), so
  the permissive deserializer is load-bearing rather than defensive: a bare `Option<i32>`
  would fail the entire telemetry frame on the string form.

- **`profile_id`**: `Option<String>`

  Cloud profile ID.

- **`project_id`**: `Option<String>`

  Cloud project ID.

- **`batch_id`**: `Option<String>`

  Cloud batch ID.

#### Implementations

- <span id="printertelemetry-gcode-start-time-secs"></span>`fn gcode_start_time_secs(&self) -> Option<u64>`

  [`gcode_start_time`](#printertelemetry) parsed to epoch seconds.

- <span id="printertelemetry-reports-np-format"></span>`fn reports_np_format(&self) -> bool`

  Returns true if this frame shows the firmware uses BambuStudio's "np" payload format.

  This is not the MQTT version — the transport is MQTT 3.1.1 on every printer. It is a
  firmware-side difference in which JSON fields and commands the printer understands.
  BambuStudio tracks it as `is_enable_np` / `is_enable_ams_np` and never expands "np"; the
  name here follows it rather than guessing.

  Mirrors BambuStudio's selector (`StatusPanel.cpp`,
  `obj->is_enable_np || obj->is_enable_ams_np`): either `cfg`, `fun`, `aux` and `stat` are
  all present (`check_enable_np`, `DeviceManager.cpp`), or `flag3` bit 9 is set
  (`DeviceManager.cpp`). `false` means this frame didn't show it, which on a partial
  frame is not proof the firmware lacks it.

- <span id="printertelemetry-current-stage"></span>`fn current_stage(&self) -> Option<PrintStage>` — [`PrintStage`](../stage/index.md#printstage)

  Returns the stage currently executing, decoded, or `None` when it cannot be trusted.

  Applies the [REF-MQTT-IDLEBUG] gate: A1/P1 firmware reports `stg_cur = 0` ("printing")
  while genuinely idle, so this returns `None` unless `gcode_state` is `RUNNING` or `PAUSE`.
  That gate matters more once the value is typed than it did when it was a bare `i32` — a
  [`PrintStage::Printing`](../stage/index.md#printstage) rendered in a UI reads as authoritative. Use
  [`Self::current_stage_ungated`](#printertelemetry) only when you are applying your own gate.

  A `Some(PrintStage::Idle)` during a run is not a bug and not completion: after the last
  queued stage finishes, `stg_cur` reads idle for the tail of the run.

- <span id="printertelemetry-print-status"></span>`fn print_status(&self) -> Option<PrintStatus>` — [`PrintStatus`](../../control/index.md#printstatus)

  Classifies `gcode_state`; `None` when this frame doesn't carry it.

- <span id="printertelemetry-current-stage-ungated"></span>`fn current_stage_ungated(&self) -> Option<PrintStage>` — [`PrintStage`](../stage/index.md#printstage)

  Decodes `stg_cur` with no [REF-MQTT-IDLEBUG] gate applied.

  Prefer [`Self::current_stage`](#printertelemetry). This exists for callers applying their own state gate;
  on an A1 or P1 the raw value is `0` ("printing") when the machine is idle.

- <span id="printertelemetry-stage-queue"></span>`fn stage_queue(&self) -> Vec<PrintStage>` — [`PrintStage`](../stage/index.md#printstage)

  Decodes the queued stage list, in wire order.

  Needs no state gate — the idle-bug anomaly is specific to `stg_cur`, and an empty or
  absent queue is unambiguous. Returns an empty `Vec` when `stg` is absent; the queue also
  legitimately empties to `[]` at `FINISH`.

- <span id="printertelemetry-chamber-temperatures"></span>`fn chamber_temperatures(&self) -> Option<HeaterTemps>` — [`HeaterTemps`](../../../client/index.md#heatertemps)

  The chamber's temperatures, unpacked from `chamber_temper`; `None` when this frame doesn't carry it.

  Values above 500 are composite-packed `(target << 16) | actual` on models with an active
  chamber heater; at or below 500 the value is the reading itself with target `0`, which is
  what every model without a heater sends, so the unpack is correct on both.

- <span id="printertelemetry-is-ethernet-active"></span>`fn is_ethernet_active(&self) -> bool`

  Evaluates whether the physical printer is connected via wired Ethernet [REF-NET-PORTS].

  Previously inspected bit 18 (`0x00040000`) of `home_flag`, following a
  pybambu-sourced heuristic. Both first-party clients (BambuStudio's
  `DevPrintOptions.cpp`, OrcaSlicer identically) actually decode that bit as
  `is_support_prompt_sound_detection`, unrelated to networking — confirmed wrong, not
  merely disputed. Real wired-ethernet state comes from `print.net.conf` bit 0
  (`DeviceManager.cpp`: `network_wired = (net.conf & 0x1) != 0`). Returns `false`
  (not `None`) when `net`/`net.conf` haven't been observed yet, matching
  `is_ethernet_active_via_wifi_signal()`'s existing no-signal-observed convention.

- <span id="printertelemetry-is-ethernet-active-via-wifi-signal"></span>`fn is_ethernet_active_via_wifi_signal(&self) -> bool`

  Evaluates whether the physical printer is connected via wired Ethernet using the `wifi_signal` sentinel value [REF-NET-PORTS], as a fallback for firmware that doesn't populate `print.net.conf`.

  A printer with no wifi signal to report (i.e. running wired-only) sends a fixed
  `wifi_signal` of `"-90dBm"`. Prefer `is_ethernet_active()` — this heuristic is kept
  only as a fallback for firmware that doesn't send `net.conf`.

- <span id="printertelemetry-is-220v-power"></span>`fn is_220v_power(&self) -> bool`

  Evaluates whether the printer's mains power supply is wired for the 220V region, based on bit 3 (`0x00000008`) of the `home_flag` register.

  Used by [`crate::quirks::ModelQuirks::bed_temp_max`](../../../quirks/index.md#modelquirks) on X1C, where the safe bed
  temperature ceiling is genuinely voltage-dependent (110°C @220V, 120°C @110V per the
  official spec sheet.

- <span id="printertelemetry-sdcard-state"></span>`fn sdcard_state(&self) -> Option<SdcardState>` — [`SdcardState`](#sdcardstate)

  Evaluates the SD-card presence/health state from `home_flag` bits 8–9 alone. See
  [`SdcardState`](#sdcardstate)'s doc comment for verification sources, and prefer
  [`sdcard_status`](#printertelemetry), which also reads `aux` and the `sdcard` flag.
  Returns `None` when this frame doesn't carry `home_flag` — distinct from `Some(NoSdcard)`.

- <span id="printertelemetry-sdcard-status"></span>`fn sdcard_status(&self) -> Option<SdcardState>` — [`SdcardState`](#sdcardstate)

  The SD-card state from whichever signal this frame carries, in BambuStudio's precedence.

  BambuStudio reads all three and lets the later one win (`DeviceManager.cpp`): the
  top-level `sdcard` flag (`DevStorage::ParseV1_0`), then `home_flag` bits 8–9
  (`parse_home_flag`, `:1075`), then — on "np" firmware — `aux` bits 12–13 (`:4514`). This
  returns the first present of `aux`, `home_flag`, `sdcard`, which is the same answer.

  bambuddy reads only `sdcard`, saying heartbeat pushes clear `home_flag` bits 8–9 with a
  card inserted (`bambu_mqtt.py`). BambuStudio, the authoritative source, reads the
  bits on every frame that carries `home_flag`, so they are followed here; see
  `reference/03_mqtt_telemetry.md` for the disagreement.

- <span id="printertelemetry-is-door-open-from-home-flag"></span>`fn is_door_open_from_home_flag(&self) -> bool`

  Reads door sensor state from bit 23 of the `home_flag` register [REF-NET-DOOR].

  Used by X1 series models where the door sensor is wired to the home_flag bitmask.

- <span id="printertelemetry-is-door-open-from-stat"></span>`fn is_door_open_from_stat(&self) -> bool`

  Reads door sensor state from bit 23 of the parsed hexadecimal `stat` field [REF-NET-DOOR].

  Used by H2, P2, and X2 series models where the door sensor state is encoded in the `stat` string.

- <span id="printertelemetry-door-state"></span>`fn door_state(&self, sensor: crate::quirks::DoorSensor) -> Option<bool>` — [`DoorSensor`](../../../quirks/index.md#doorsensor)

  Reads the door state from wherever `sensor` says this model reports it [REF-NET-DOOR].

  `Some(true)` open, `Some(false)` closed, `None` when the model has no sensor or this frame
  doesn't carry a readable field — never a guess of "closed". Get `sensor` from
  [`ModelQuirks::door_sensor`](../../../quirks/index.md#modelquirks).

#### Trait Implementations

##### `impl Clone for PrinterTelemetry`

- <span id="printertelemetry-clone"></span>`fn clone(&self) -> PrinterTelemetry` — [`PrinterTelemetry`](#printertelemetry)

##### `impl Debug for PrinterTelemetry`

- <span id="printertelemetry-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Deserialize<'de> for PrinterTelemetry`

- <span id="printertelemetry-deserialize"></span>`fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`

##### `impl DeserializeOwned for PrinterTelemetry`

##### `impl Serialize for PrinterTelemetry`

- <span id="printertelemetry-serialize"></span>`fn serialize<__S>(&self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`

### `SdcardState`

```rust
enum SdcardState {
    NoSdcard,
    Normal,
    Abnormal,
    ReadOnly,
}
```

SD-card presence/health state, decoded from a two-bit field (`aux` bits 12–13 or `home_flag` bits 8–9).

Confirmed against BambuStudio's `MachineObject::parse_home_flag`
(`DeviceManager.cpp`: `m_storage->set_sdcard_state(get_flag_bits(flag, 8, 2))`) and
corroborated by pybambu's `const.py`/`models.py` (same bits). The `sdcard`
boolean field can never report a degraded state — only the bit fields distinguish "no card,"
"normal," "abnormal," and "read-only."

#### Variants

- **`NoSdcard`**

  No SD card physically present.

- **`Normal`**

  SD card present and functioning normally.

- **`Abnormal`**

  SD card present but reporting an abnormal/error condition.

- **`ReadOnly`**

  SD card present but mounted read-only.

#### Trait Implementations

##### `impl Clone for SdcardState`

- <span id="sdcardstate-clone"></span>`fn clone(&self) -> SdcardState` — [`SdcardState`](#sdcardstate)

##### `impl Copy for SdcardState`

##### `impl Debug for SdcardState`

- <span id="sdcardstate-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for SdcardState`

##### `impl Hash for SdcardState`

- <span id="sdcardstate-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for SdcardState`

- <span id="sdcardstate-partialeq-eq"></span>`fn eq(&self, other: &SdcardState) -> bool` — [`SdcardState`](#sdcardstate)

