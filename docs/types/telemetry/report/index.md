*[bambino](../../../index.md) / [types](../../index.md) / [telemetry](../index.md) / [report](index.md)*

---

# Module `report`

Top-level telemetry report envelope (`print` and `device` wire locations).

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`LightReport`](#lightreport) | struct | Chamber/work/heatbed light state entry from the `lights_report` array. |
| [`NetInfo`](#netinfo) | struct | Network interface state from `print.net` [REF-NET-PORTS]. |
| [`PrinterTelemetry`](#printertelemetry) | struct | Core printer state machine telemetry, containing kinematics, thermal targets, auxiliary fan configurations, and connected AMS arrays. |
| [`SdcardState`](#sdcardstate) | enum | SD-card presence/health state, decoded from `home_flag` bits 8–9 (BUG-123). |

## Types

### `LightReport`

```rust
struct LightReport {
    pub node: String,
    pub mode: String,
}
```

Chamber/work/heatbed light state entry from the `lights_report` array.

#### Fields

- **`node`**: `String`

  Light identifier (e.g. "chamber_light", "work_light").

- **`mode`**: `String`

  Current state (e.g. "on", "off", "flashing").

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

### `PrinterTelemetry`

```rust
struct PrinterTelemetry {
    pub gcode_state: Option<String>,
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
    pub sdcard: bool,
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
    pub tray_exist_bits: Option<String>,
    pub power_on_flag: Option<bool>,
    pub ipcam: Option<super::diagnostics::IpcamTelemetry>,
    pub xcam: Option<serde_json::Value>,
    pub ams: Option<super::ams::AmsStatusReport>,
    pub ams_status: Option<i32>,
    pub ams_mapping: Vec<i32>,
    pub vt_tray: Option<super::ams::VirtualTray>,
    pub vir_slot: Option<Vec<super::ams::VirtualTray>>,
    pub device: Option<super::device::DeviceTelemetry>,
    pub fun: Option<String>,
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
    pub stg: Option<Vec<i32>>,
    pub mapping: Option<Vec<i32>>,
    pub gcode_start_time: Option<String>,
    pub cali_version: Option<i32>,
    pub err: Option<String>,
    pub fail_reason: Option<String>,
    pub canvas_id: Option<String>,
    pub design_id: Option<String>,
    pub model_id: Option<String>,
    pub profile_id: Option<String>,
    pub project_id: Option<String>,
    pub batch_id: Option<String>,
}
```

Core printer state machine telemetry, containing kinematics, thermal targets, auxiliary fan configurations, and connected AMS arrays.

#### Fields

- **`gcode_state`**: `Option<String>`

  High-level execution status of the G-code processor (e.g., "IDLE", "RUNNING", "PAUSE").

- **`gcode_file`**: `Option<String>`

  Path or parent project file currently loaded for execution.

- **`subtask_name`**: `Option<String>`

  User-assigned name of the active print queue task.

- **`subtask_id`**: `Option<String>`

  Hardware-enforced unique 32-bit transaction identifier tracking active jobs.

- **`layer_num`**: `Option<i32>`

  Active layer progress tracker.

- **`total_layers`**: `Option<i32>`

  Total layers within the sliced print pipeline.
  Wire sends as `total_layer_num`; `total_layers` accepted for compatibility.

- **`mc_remaining_time`**: `Option<i32>`

  Estimated remaining duration of the active layer sequence, in seconds.

- **`spd_lvl`**: `Option<u8>`

  Active speed profile level (1=Silent, 2=Standard, 3=Sport, 4=Ludicrous).

- **`spd_mag`**: `Option<u16>`

  Speed magnitude as a percentage of the nominal feedrate.

- **`mc_percent`**: `Option<i32>`

  Motion controller progress percentage (0–100).

- **`mc_print_sub_stage`**: `Option<i32>`

  Print sub-stage identifier tracking granular execution phases within the active print stage.

- **`mc_print_stage`**: `Option<String>`

  Motion controller print stage string.

- **`home_flag`**: `Option<u32>`

  Kinematics flag field tracking homing states, networking interfaces, and door nodes.

- **`stat`**: `Option<String>`

  State field used in newer enclosed printer lines to track sensors (e.g., door status hex strings).

- **`stg_cur`**: `Option<i32>`

  Active print stage. Leveraged by the quirks engine to verify stg_cur idle anomalies [REF-MQTT-IDLEBUG].

- **`print_error`**: `Option<u32>`

  Active error code register, packed as a 32-bit integer [REF-DIAG-HMS].

- **`hms`**: `Option<Vec<super::diagnostics::HmsEntry>>`

  Active hardware fault and diagnostic alert entries [REF-DIAG-HMS].

- **`sdcard`**: `bool`

  Permissive indicator tracking physical MicroSD card insertion.
  
  Evaluated via custom deserializer to absorb structural variations between firmwares.

- **`wifi_signal`**: `Option<String>`

  Raw wireless network reception scale returned as a formatted string (e.g. "-52dBm").

- **`net`**: `Option<NetInfo>`

  Network interface state, nested as `print.net` on the wire (BUG-110).

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
  
  Wire sends both integers and floats depending on model. Use `unpack_temperature()`
  to extract actual/target from composite-packed values [REF-THER-DECODE].

- **`nozzle_temper`**: `Option<f64>`

  Hotend actual temperature register.
  
  Wire sends both integers and floats depending on model [REF-THER-DECODE].

- **`bed_temper`**: `Option<f64>`

  Heated build-plate temperature register (actual, target, or composite packed).

- **`bed_target_temper`**: `Option<f64>`

  Explicit bed target temperature. Separate from composite-packed `bed_temper`.

- **`chamber_temper`**: `Option<f64>`

  Active chamber heater or sensor telemetry (actual, target, or composite packed).

- **`tray_exist_bits`**: `Option<String>`

  Hexadecimal bitmask string representing the physical presence of loaded spools.

- **`power_on_flag`**: `Option<bool>`

  Power status of the printer core logic board.

- **`ipcam`**: `Option<super::diagnostics::IpcamTelemetry>`

  Camera and recording telemetry. Nested as `print.ipcam` on the wire.

- **`xcam`**: `Option<serde_json::Value>`

  AI detection settings (spaghetti detection, first-layer inspection, etc.).

- **`ams`**: `Option<super::ams::AmsStatusReport>`

  AMS expansion bus status container [REF-AMS-DECODE].

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

- **`print_type`**: `Option<String>`

  Print source identifier (e.g. `"cloud"`, `"local"`, `"idle"`).

- **`lights_report`**: `Option<Vec<LightReport>>`

  Chamber/work/heatbed light states array.

- **`gcode_file_prepare_percent`**: `Option<String>`

  File download progress percentage (sent as string).

- **`hw_switch_state`**: `Option<i32>`

  Extruder filament sensor state (1 = filament present).

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

- **`cfg`**: `Option<String>`

  Hex config bitmask string (bit 18 = AMS Filament Backup).

- **`stg`**: `Option<Vec<i32>>`

  Calibration stage list.

- **`mapping`**: `Option<Vec<i32>>`

  IDEX AMS-to-extruder mapping array.

- **`gcode_start_time`**: `Option<String>`

  Print start timestamp string.

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

- **`profile_id`**: `Option<String>`

  Cloud profile ID.

- **`project_id`**: `Option<String>`

  Cloud project ID.

- **`batch_id`**: `Option<String>`

  Cloud batch ID.

#### Implementations

- <span id="printertelemetry-unpack-temperature"></span>`fn unpack_temperature(raw_val: f64) -> (u16, u16)`

  Resolves the actual and target values from a composite packed temperature [REF-THER-DECODE].

- <span id="printertelemetry-is-ethernet-active"></span>`fn is_ethernet_active(&self) -> bool`

  Evaluates whether the physical printer is connected via wired Ethernet [REF-NET-PORTS].

- <span id="printertelemetry-is-ethernet-active-via-wifi-signal"></span>`fn is_ethernet_active_via_wifi_signal(&self) -> bool`

  Evaluates whether the physical printer is connected via wired Ethernet using the `wifi_signal` sentinel value [REF-NET-PORTS], as a fallback for firmware that doesn't populate `print.net.conf`.

- <span id="printertelemetry-is-220v-power"></span>`fn is_220v_power(&self) -> bool`

  Evaluates whether the printer's mains power supply is wired for the 220V region, based on bit 3 (`0x00000008`) of the `home_flag` register.

- <span id="printertelemetry-sdcard-state"></span>`fn sdcard_state(&self) -> Option<SdcardState>` — [`SdcardState`](#sdcardstate)

  Evaluates the SD-card presence/health state from `home_flag` bits 8–9 (BUG-123). See

  [`SdcardState`](#sdcardstate)'s doc comment for verification sources. Returns `None` before any

  telemetry carrying `home_flag` has been observed — distinct from `Some(NoSdcard)`.

- <span id="printertelemetry-is-door-open-from-home-flag"></span>`fn is_door_open_from_home_flag(&self) -> bool`

  Reads door sensor state from bit 23 of the `home_flag` register [REF-NET-DOOR].

- <span id="printertelemetry-is-door-open-from-stat"></span>`fn is_door_open_from_stat(&self) -> bool`

  Reads door sensor state from bit 23 of the parsed hexadecimal `stat` field [REF-NET-DOOR].

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

SD-card presence/health state, decoded from `home_flag` bits 8–9 (BUG-123).

Confirmed against BambuStudio's `MachineObject::parse_json` (`DeviceManager.cpp:1092`:
`m_storage->set_sdcard_state(get_flag_bits(flag, 8, 2))`) and corroborated by pybambu's
`const.py:265-266`/`models.py:3408-3412` (same bits). The `sdcard` boolean field can never
report a degraded state — only this bitmask distinguishes "no card," "normal," "abnormal,"
and "read-only."

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

