**bambino > types > telemetry > report**

# Module: types::telemetry::report

## Contents

**Structs**

- [`LightReport`](#lightreport) - Chamber/work/heatbed light state entry from the `lights_report` array.
- [`PrinterTelemetry`](#printertelemetry) - Core printer state machine telemetry, containing kinematics, thermal targets, auxiliary fan configurations, and connected AMS arrays.

---

## bambino::types::telemetry::report::LightReport

*Struct*

Chamber/work/heatbed light state entry from the `lights_report` array.

**Fields:**
- `node: String` - Light identifier (e.g. "chamber_light", "work_light").
- `mode: String` - Current state (e.g. "on", "off", "flashing").

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Clone**
  - `fn clone(self: &Self) -> LightReport`



## bambino::types::telemetry::report::PrinterTelemetry

*Struct*

Core printer state machine telemetry, containing kinematics, thermal targets, auxiliary fan configurations, and connected AMS arrays.

**Fields:**
- `gcode_state: Option<String>` - High-level execution status of the G-code processor (e.g., "IDLE", "RUNNING", "PAUSE").
- `gcode_file: Option<String>` - Path or parent project file currently loaded for execution.
- `subtask_name: Option<String>` - User-assigned name of the active print queue task.
- `subtask_id: Option<String>` - Hardware-enforced unique 32-bit transaction identifier tracking active jobs.
- `layer_num: Option<i32>` - Active layer progress tracker.
- `total_layers: Option<i32>` - Total layers within the sliced print pipeline.
- `mc_remaining_time: Option<i32>` - Estimated remaining duration of the active layer sequence, in seconds.
- `spd_lvl: Option<u8>` - Active speed profile level (1=Silent, 2=Standard, 3=Sport, 4=Ludicrous).
- `spd_mag: Option<u16>` - Speed magnitude as a percentage of the nominal feedrate.
- `mc_percent: Option<i32>` - Motion controller progress percentage (0–100).
- `mc_print_sub_stage: Option<i32>` - Print sub-stage identifier tracking granular execution phases within the active print stage.
- `mc_print_stage: Option<String>` - Motion controller print stage string.
- `home_flag: Option<u32>` - Kinematics flag field tracking homing states, networking interfaces, and door nodes.
- `stat: Option<String>` - State field used in newer enclosed printer lines to track sensors (e.g., door status hex strings).
- `stg_cur: Option<i32>` - Active print stage. Leveraged by the quirks engine to verify stg_cur idle anomalies [REF-MQTT-IDLEBUG].
- `print_error: Option<u32>` - Active error code register, packed as a 32-bit integer [REF-DIAG-HMS].
- `hms: Option<Vec<super::diagnostics::HmsEntry>>` - Active hardware fault and diagnostic alert entries [REF-DIAG-HMS].
- `sdcard: bool` - Permissive indicator tracking physical MicroSD card insertion.
- `wifi_signal: Option<String>` - Raw wireless network reception scale returned as a formatted string (e.g. "-52dBm").
- `cooling_fan_speed: Option<String>` - On-board part cooling fan speed (represented as discrete steps 0 to 15) [REF-CLIM-FANS].
- `big_fan1_speed: Option<String>` - On-board left-side auxiliary fan speed (represented as discrete steps 0 to 15).
- `big_fan2_speed: Option<String>` - On-board filtration or chamber exhaust fan speed (represented as discrete steps 0 to 15).
- `heatbreak_fan_speed: Option<String>` - On-board toolhead heatbreak fan speed (represented as discrete steps 0 to 15).
- `nozzle_target_temper: Option<f64>` - Hotend target temperature register.
- `nozzle_temper: Option<f64>` - Hotend actual temperature register.
- `bed_temper: Option<f64>` - Heated build-plate temperature register (actual, target, or composite packed).
- `bed_target_temper: Option<f64>` - Explicit bed target temperature. Separate from composite-packed `bed_temper`.
- `chamber_temper: Option<f64>` - Active chamber heater or sensor telemetry (actual, target, or composite packed).
- `tray_exist_bits: Option<String>` - Hexadecimal bitmask string representing the physical presence of loaded spools.
- `power_on_flag: Option<bool>` - Power status of the printer core logic board.
- `ipcam: Option<super::diagnostics::IpcamTelemetry>` - Camera and recording telemetry. Nested as `print.ipcam` on the wire.
- `xcam: Option<serde_json::Value>` - AI detection settings (spaghetti detection, first-layer inspection, etc.).
- `ams: Option<super::ams::AmsStatusReport>` - AMS expansion bus status container [REF-AMS-DECODE].
- `ams_status: Option<i32>` - Combined AMS state bitmask (lower 8 bits = sub status, bits 8–15 = main status).
- `ams_mapping: Vec<i32>` - Slicer-mapped material assignment channels configured during print dispatch [REF-AMS-MAP].
- `vt_tray: Option<super::ams::VirtualTray>` - Virtual/external spool holder state on single-nozzle platforms (P1S, P1P, A1, X1C, H2S).
- `vir_slot: Option<Vec<super::ams::VirtualTray>>` - IDEX external spool holder array. Each entry uses the same schema as `VirtualTray`.
- `device: Option<super::device::DeviceTelemetry>` - Device sub-object nested inside pushall `print` envelope on H2/P2/X2 models.
- `fun: Option<String>` - Developer LAN Mode bitmask field (hex string) nested inside `print` [REF-MQTT-ENV §3.2.1].
- `print_type: Option<String>` - Print source identifier (e.g. `"cloud"`, `"local"`, `"idle"`).
- `lights_report: Option<Vec<LightReport>>` - Chamber/work/heatbed light states array.
- `gcode_file_prepare_percent: Option<String>` - File download progress percentage (sent as string).
- `hw_switch_state: Option<i32>` - Extruder filament sensor state (1 = filament present).
- `s_obj: Option<Vec<i32>>` - Skipped object IDs during selective printing.
- `nozzle_type: Option<String>` - Legacy single-nozzle type string (pre-IDEX models).
- `nozzle_diameter: Option<String>` - Legacy single-nozzle diameter string (pre-IDEX models).
- `fan_gear: Option<u32>` - Fan gear composite bitmask.
- `print_gcode_action: Option<i32>` - G-code action state (H2/X2 models).
- `print_real_action: Option<i32>` - Real action state (H2/X2 models).
- `task_id: Option<String>` - Cloud task identifier.
- `job_id: Option<String>` - Cloud job identifier.
- `remain_time: Option<i32>` - Alternative remaining time field (minutes).
- `cfg: Option<String>` - Hex config bitmask string (bit 18 = AMS Filament Backup).
- `stg: Option<Vec<i32>>` - Calibration stage list.
- `mapping: Option<Vec<i32>>` - IDEX AMS-to-extruder mapping array.
- `gcode_start_time: Option<String>` - Print start timestamp string.
- `cali_version: Option<i32>` - Calibration version identifier.
- `err: Option<String>` - Error string field.
- `fail_reason: Option<String>` - Failure reason description.
- `canvas_id: Option<String>` - Cloud canvas project ID.
- `design_id: Option<String>` - Cloud design ID.
- `model_id: Option<String>` - Cloud model ID.
- `profile_id: Option<String>` - Cloud profile ID.
- `project_id: Option<String>` - Cloud project ID.
- `batch_id: Option<String>` - Cloud batch ID.

**Methods:**

- `fn unpack_temperature(raw_val: f64) -> (u16, u16)` - Resolves the actual and target values from a composite packed temperature [REF-THER-DECODE].
- `fn is_ethernet_active(self: &Self) -> bool` - Evaluates whether the physical printer is connected via wired Ethernet [REF-NET-PORTS].
- `fn is_ethernet_active_via_wifi_signal(self: &Self) -> bool` - Evaluates whether the physical printer is connected via wired Ethernet using the doc-recommended `wifi_signal` sentinel value [REF-NET-PORTS], as an alternative to the disputed `home_flag` bit-18 heuristic in `is_ethernet_active()`.
- `fn is_220v_power(self: &Self) -> bool` - Evaluates whether the printer's mains power supply is wired for the 220V region, based on bit 3 (`0x00000008`) of the `home_flag` register.
- `fn is_door_open_from_home_flag(self: &Self) -> bool` - Reads door sensor state from bit 23 of the `home_flag` register [REF-NET-DOOR].
- `fn is_door_open_from_stat(self: &Self) -> bool` - Reads door sensor state from bit 23 of the parsed hexadecimal `stat` field [REF-NET-DOOR].

**Trait Implementations:**

- **Serialize**
  - `fn serialize<__S>(self: &Self, __serializer: __S) -> _serde::__private228::Result<<__S as >::Ok, <__S as >::Error>`
- **Debug**
  - `fn fmt(self: &Self, f: & mut $crate::fmt::Formatter) -> $crate::fmt::Result`
- **Deserialize**
  - `fn deserialize<__D>(__deserializer: __D) -> _serde::__private228::Result<Self, <__D as >::Error>`
- **Clone**
  - `fn clone(self: &Self) -> PrinterTelemetry`



