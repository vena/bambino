# Chapter 3: Local Broker Control & State Telemetry

---

### 3.1 Network Boundary & Interface Parameters [REF-MQTT-CONN]

Local broker interaction occurs via MQTT over TLS (MQTTS) on Port `8883`. The local broker handles direct telemetry publishing and is the gateway for commands written to the printer's state machines.

#### Connection Parameters
*   **Host Port**: `8883`
*   **MQTT Authentication**:
    *   **Username**: `bblp`
    *   **Password**: `<access_code>` (The uppercase 8-character LAN access code)
*   **Topic Topology**:
    *   **Publish Topic (Command channel)**: `device/{serial_number}/request`
    *   **Subscription Topic (Status channel)**: `device/{serial_number}/report`
*   **Quality of Service (QoS)**: QoS `1` (At Least Once) is mandatory for outbound requests. The local broker ignores QoS `0` payloads when processing high-volume status broadcasts.
*   **Clean Session Protocol**: Connections must negotiate Clean Session = True. Re-establishing a connection with a dirty session can trigger a re-delivery of duplicate QoS 1 commands, resulting in memory heap corruption or SD card read-write faults on the printer.
*   **Client ID Uniqueness**: Each connection must use a unique MQTT client ID. If a prior connection's TCP socket has not fully torn down, a reconnect using the same client ID forces the broker to disconnect the stale session, potentially losing in-flight QoS 1 messages. Appending a monotonic counter or random suffix to the client ID (e.g., `bambino_{serial}_{counter}`) avoids stale session conflicts.
*   **Connection Staleness Monitoring**: The printer's broker does not reliably deliver TCP-level connection reset notifications when the physical network link degrades. Clients must implement a general liveness monitor that triggers a forced reconnect if no packets of any kind (PUBLISH, PUBACK, PINGRESP) are received within 60 seconds. This is independent of the write-channel zombie detection described in [REF-MQTT-ZOMBIE].
*   **Broker In-Flight Limits & PUBACK Latency**: The printer's onboard broker exhibits substantial latency in returning PUBACK confirmations when processing multiple consecutive QoS 1 command payloads. To prevent command queue saturation and transaction stalls on the secure channel, the connecting peer must allow at least 200 concurrent unacknowledged in-flight packets before blocking transmission.
*   **Request Topic Subscription Rejection**: Some printer MQTT brokers (such as the P1S and A1 series) reject active client subscriptions to the request topic (`device/{serial_number}/request`) by abruptly terminating the TCP connection. Clients must restrict subscriptions exclusively to the report topic.

---

### 3.2 State Telemetry Payload Schema (The Read Stream) [REF-MQTT-ENV]

Status telemetry structures, string emission anomalies, and task-ID overflow limits are processed via the primary status channel.

**`plate_idx`** identifies which plate of a multi-plate 3MF the current job was sliced for. A consumer needs it to pull the right plate's metadata — thumbnail, filament list, bed temperature — out of the project file, since a 3MF indexes per-plate data on exactly this (`Metadata/plate_<N>.png` and friends).

**It is authoritative over the 3MF's own `slice_info`.** A printer can retain or reuse a 3MF whose embedded `slice_info` names a different plate, so a consumer that trusts the archive over this field renders a stale thumbnail. Prefer `plate_idx` whenever it is present and non-zero. *(Verification source: ha-bambulab PR #2067, whose cover-image path takes `plate_idx` over the archive's index metadata for exactly this reason.)*

**`subtask_name` also carries the printer's own internal jobs.** A consumer asking "is this a real user print?" must filter these four names out:

*   `auto_cali_for_user`
*   `auto_pa_line_calib_mode`
*   `pa_line_calib_mode` — manual flow dynamics
*   `pa_pattern_calib_mode` — manual flow dynamics

The last two are the ones most easily missed: bare subtask names with no `/usr/` path and **no `auto_` prefix**, so a filter keyed on the prefix or the path lets them through as user prints. Match the four **exactly**, after normalizing path, suffix and case — never on a `pa_` stem, or a user's own `pa_bracket.3mf` gets swallowed with them. *(Verification source: bambuddy issue #2957 and its follow-ups.)*

```json
{
  "print": {
    "gcode_state": "RUNNING",
    "gcode_file": "/Metadata/plate_1.gcode.3mf",
    "subtask_name": "MyPrintJob",
    "subtask_id": "14852932",
    "plate_idx": 2,
    "mc_print_sub_stage": 0,
    "mc_percent": 10,
    "layer_num": 12,
    "total_layer_num": 120,
    "mc_remaining_time": 3600,
    "spd_lvl": 2,
    "spd_mag": 100,
    "home_flag": 131072,
    "sdcard": true,
    "wifi_signal": "-52dBm",
    "print_type": "local",
    "lights_report": [{"node": "chamber_light", "mode": "on"}]
  }
}
```

#### Dual-Location Device Telemetry
The `device` sub-object containing nozzle, extruder, airduct, CTC (chamber temperature controller), bed, and ext_tool telemetry appears at two distinct wire locations:
*   **Top-level `{"device": {...}}`**: Sent as incremental updates (e.g., nozzle info pushes, airduct state changes).
*   **Nested `{"print": {"device": {...}}}`**: Included inside `pushall` responses on H2, P2, and X2 series models.

Both locations use the identical schema. Clients must merge data from both paths into a unified device state representation.

#### Telemetry Field Specifics
1.  **gcode_state**: Indicates the high-level operational state of the printer (such as `IDLE`, `RUNNING`, `PAUSE`, `FINISH`, `FAILED`).
2.  **mc_percent**: Motion controller progress percentage (integer, 0 to 100). This is the authoritative progress field.
3.  **spd_lvl / spd_mag**: Active speed profile level (`1`=Silent, `2`=Standard, `3`=Sport, `4`=Ludicrous) and speed magnitude as a percentage of the nominal feedrate.
4.  **stat**: Hexadecimal bitmask string used on P2S, X2D, and H2 series models to track sensor states such as the enclosure door open sensor (bit 23). See §3.2.1 for door sensor routing details.
5.  **lights_report**: Array of `{"node": "<strip_name>", "mode": "<on|off|flashing>"}` objects reporting the current state of chamber, work, and heatbed light strips.
6.  **print_type**: Print source identifier string (`"cloud"`, `"local"`, `"idle"`, `"system"`).
7.  **sdcard**: The top-level `sdcard` field is a permissive truthy check covering boolean, integer, or string (`HAS_SDCARD_NORMAL`) formats depending on firmware. The underlying `home_flag` bit 8 and 9 presence signals are unreliable because periodic heartbeat pushes clear these bits even when a card is physically present.
8.  **gcode_file Emission Anomaly**: The `gcode_file` property does not strictly guarantee a `.gcode` path. On certain firmwares (such as P1S `01.10.00.00`), the printer transmits the parent `.3mf` filename instead of the specific sliced plate `.gcode` path.
9.  **total_layer_num**: Total layers within the sliced print pipeline. Some firmware versions send this as `total_layers` instead — clients should accept both keys.
10.  **gcode_start_time**: Unix epoch string for when the current job started. Never observed in a complete local (`print_type: "local"`) print lifecycle capture on a P1S (FAILED→IDLE→PREPARE→RUNNING→FINISH, zero occurrences) — may be cloud-print-only or otherwise absent on LAN-dispatched jobs. Unconfirmed either way; treat its absence as expected on local prints.
11.  **A1/P1 Series `stg_cur = 0` Idle Bug [REF-MQTT-IDLEBUG]**: The firmware on the A1, A1 Mini, and P1 series exhibits an idle reporting anomaly where `stg_cur = 0` (which normally maps to `"Printing"`) is transmitted in telemetry even when the machine is in an idle, non-printing state. Wire-state evaluation models must ignore the `stg_cur` value when the high-level `gcode_state` is not `RUNNING` or `PAUSE`. **`stg_cur`/`stg` are emitted in incremental telemetry when a stage is actually queued — a previous claim to the contrary here was wrong and is corrected; do not trust older notes on this.** The earlier claim (that P1S sends both only in the one-time `pushall` dump) was generalized from `probe_report-from_unhomed.json` and `probe_report-home_axes_repeat-from_homed.json`, captured during `home_axes`/`home_axes_repeat` probe runs — runs that queue no stage at all, so both fields were absent for want of content, not by protocol rule. Direct refutation: a standalone `calibration` run on a P1S (firmware `01.10.00.00`) emits `stg` and `stg_cur` in `msg: 1` incremental `print` updates throughout, in a capture containing **zero** `msg: 0` frames. See "Hardware Calibration Routines" below for the full stage sequence. The idle-bug pollution itself remains real and was directly observed in a `pushall` dump (`stg_cur=0` while `gcode_state="IDLE"`) — that is what the `gcode_state` gate above is for, and it is unaffected by this correction. pybambu's `CURRENT_STAGE_IDS` table (`const.py`) documents per-code semantics for `stg_cur` (e.g. `4`="changing_filament", `13`="homing_toolhead") and does resolve the `mc_print_sub_stage` ambiguity in [REF-MOTO-HOME]. Real-time activity detection on `stg_cur` is therefore viable, subject to the `gcode_state` gate; treat the stage-ID semantics as still unverified on non-P1S models.

#### Task Tracking & Sequence Boundaries
When generating identity fields for command payloads and print job submissions (such as `task_id` and `subtask_id`), values must strictly conform to hardware parser limits.
*   **32-Bit Signed Integer Overflow Hazard**: The embedded firmware strictly clamps task identity parameters to a 32-bit signed integer maximum (`2147483647`). If a client utilizes standard Epoch milliseconds (a 13-digit integer) to generate unique tracking IDs, the value will overflow the 32-bit limit. This causes the printer to permanently lock into an `IDLE` state, treating every new print dispatch as an illegal continuation of a previously aborted job. Unique sequence IDs must be mathematically constrained within the signed 32-bit integer boundary (e.g., using `Epoch ms % 2147483647`).

---

### 3.2.1 Model-Specific Telemetry Polymorphism & Bitmasks

Decoded parameters within the status stream vary conditionally based on the discovered hardware model profile and firmware version track.

#### `home_flag` Bitmask Reference [REF-HOMEFLAG]

The `home_flag` integer field is a packed bitmask encoding printer hardware state. It is transmitted as a signed 32-bit integer (negative values on the wire must be masked to unsigned via `flag & 0xFFFFFFFF`). Verification sources: OrcaSlicer (`DeviceManager.cpp`), pybambu (`const.py` `Home_Flag_Values`), wire captures from P1S probe runs.

| Bit | Mask | Field | Description |
| :--- | :--- | :--- | :--- |
| 0 | `0x00000001` | X axis homed | Set when the X axis has been homed via `G28` |
| 1 | `0x00000002` | Y axis homed | Set when the Y axis has been homed via `G28` |
| 2 | `0x00000004` | Z axis homed | Set when the Z axis has been homed via `G28` |
| 3 | `0x00000008` | 220V power | Printer is on a 220V power supply |
| 4 | `0x00000010` | XCam auto-recovery step loss | XCam step-loss auto-recovery enabled |
| 5 | `0x00000020` | Camera recording | Timelapse/recording is active |
| 7 | `0x00000080` | AMS calibrate remaining | AMS remaining filament calibration enabled |
| 8–9 | `0x00000300` | SD card state | 2-bit value (`get_flag_bits(flag, 8, 2)`): `0`=no card, `1`=normal, `2`=abnormal, `3`=read-only (BUG-123; confirmed against BambuStudio's `DeviceManager.cpp:1092` and pybambu's `const.py:265-266`/`models.py:3408-3412` — the top-level `sdcard` boolean can never report a degraded state, only this bitmask can) |
| 10 | `0x00000400` | AMS auto-switch | AMS automatic filament switching enabled |
| 15 | `0x00008000` | Supports flow calibration | Hardware supports flow calibration (false on H2D despite firmware reporting — OrcaSlicer overrides) |
| 16 | `0x00010000` | Supports PA calibration | Hardware supports pressure advance calibration (false on P1 series despite firmware reporting — OrcaSlicer overrides) |
| 17 | `0x00020000` | XCam prompt sound enabled | XCam prompt sound is currently enabled |
| 18 | `0x00040000` | Supports prompt sound | Hardware supports prompt sound configuration. Note: pybambu incorrectly labels this as "wired network" |
| 19 | `0x00080000` | Supports filament tangle detect | Hardware supports tangle detection (model-specific: requires XCam AI engine) |
| 20 | `0x00100000` | Filament tangle detect enabled | Filament tangle detection is currently enabled |
| 21 | `0x00200000` | Supports motor noise calibration | Hardware supports motor noise calibration |
| 22 | `0x00400000` | Supports user presets | Hardware supports user-defined presets |
| 23 | `0x00800000` | Door open | Enclosure door open — X1 family only (see §3.2.1 Door Sensor Routing) |
| 24 | `0x01000000` | Nozzle blob detect enabled | Nozzle blob detection is currently enabled (model-specific: requires XCam AI engine) |
| 25 | `0x02000000` | Supports nozzle blob detect | Hardware supports nozzle blob detection |
| 26 | `0x04000000` | `installed_plus` | Purpose unknown — present in OrcaSlicer and pybambu (`INSTALLED_PLUS`) but not publicly documented. OrcaSlicer references it as `is_support_p1s_plus`, suggesting P1S-specific |
| 27 | `0x08000000` | `supported_plus` | Purpose unknown — paired with bit 26 in OrcaSlicer and pybambu (`SUPPORTED_PLUS`) |
| 28 | `0x10000000` | Air print (spaghetti) status | Air print / spaghetti detection is currently enabled (model-specific: requires XCam AI engine; disabled on AMS-HT firmware) |
| 29 | `0x20000000` | Supports air print detect | Hardware supports air print / spaghetti detection |

The "supports" vs "enabled" bit pairs (e.g., 19/20, 25/24, 29/28) follow a pattern where the higher bit indicates hardware capability and the lower bit indicates user-toggled state. Detection features (tangle, blob, air print) are model-specific — they require XCam AI hardware present on X1, P1S, and newer platforms. See `MODEL_MATRIX.md` for per-model capability availability.

**Axis homing state (bits 0–2):** During a `G28` homing sequence, bits are set progressively as each axis completes. On a P1S the observed sequence is: all three bits clear (unhomed) → bits 0–1 set (X and Y homed, Z pending) → all three bits set (fully homed). The firmware does not reject motion gcode when axes are unhomed — the motion controller executes regardless. Clients must check bits 0–2 before dispatching motion commands and block or warn at the application layer (matching OrcaSlicer's behavior). See [REF-MOTO-GCODE] for homing safety constraints.

#### Wired Ethernet Detection (`net.conf`) [REF-NET-CONF]
The confirmed-authoritative source for wired-Ethernet state is bit 0 of `print.net.conf` — confirmed directly against BambuStudio source (`DeviceManager.cpp:3053`: `network_wired = (net.conf & 0x1) != 0`; identical in OrcaSlicer):

```json
"net": { "conf": 1 }
```

`conf & 0x1 != 0` means wired Ethernet is the active connection. See the Wi-Fi signal sentinel below for a fallback when `net.conf` isn't present.

#### Wired Ethernet Wi-Fi Signal Sentinel [REF-NET-PORTS]
For Ethernet-equipped models (`X1E`, `X2D`, `P2S`, or `H2D Pro`), when the machine is connected via a physical Ethernet cable and Wi-Fi is disabled, the printer transmits a static sentinel value:

```json
"wifi_signal": "-90dBm"
```

This represents an active Wired Ethernet Mode rather than a degraded Wi-Fi link. This sentinel is a fallback signal only — the confirmed-authoritative source is `print.net.conf` bit 0 (`network_wired = (net.conf & 0x1) != 0`, see [REF-NET-CONF]); pybambu's `home_flag` bit 18 (`0x00040000`) attribution was checked directly against both BambuStudio and OrcaSlicer source (BUG-110) and confirmed wrong — that bit is `is_support_prompt_sound_detection`, unrelated to networking.

#### Enclosure Door Open Sensor Routing [REF-NET-DOOR]
The active state of the front enclosure door is tracked via bit 23 (`0x00800000`) of a telemetry status field. This diagnostic function is physically restricted to enclosed models equipped with an electronic door sensor switch (`X1`, `X1C`, `X1E`, `X2D`, `P2S`, `H2C`, `H2D`, `H2D Pro`, `H2S`). Open-frame bed-slingers and non-sensor models (`P1P`, `P1S`, `A1`, `A1 Mini`) do not support physical door sensing on the hardware bus. For supported sensor-equipped models, routing is model-dependent:
*   X1 Series (X1, X1C, X1E): Monitored via bit 23 of `home_flag`.
*   Other Sensor Families (P2S, X2D, H2C, H2D, H2D Pro, H2S): Monitored via bit 23 of the `stat` field (hex string) nested inside the `"print"` status object (e.g., `payload["print"]["stat"]`).

Supported sensor-equipped architectures share the same bitmask (`0x00800000`) but live in different telemetry fields. For unmonitored models, this bit remains static or fluctuates meaninglessly and must be ignored.

#### Divergent Nozzle Info Telemetry Keys [REF-NOZZLE-KEYS]
The structured array nested within `device.nozzle.info` contains nozzle characteristics that programmatically vary based on the physical extruder and carriage architecture. Evaluators must map these keys conditionally based on the active model prefix:

*   **Standard Platforms (X1, P1, A1, A2L, P2S, H2S)**:
    These models utilize standard abbreviated telemetry keys inside the `device.nozzle.info` array:
    *   `"diameter"`: Nozzle diameter (e.g. `0.4`).
    *   `"id"`: Extruder/hotend ID (always `0` on single-nozzle configurations).
    *   `"tm"`: Target maximum temperature.
    *   `"type"`: Nozzle material/type code (e.g. `"HS01"`).
    *   `"wear"`: Nozzle wear index.

*   **Dual-Nozzle IDEX Platforms (H2D, H2D Pro, X2D)**:
    These models support dual independent extruders (ID `0` for Right/Main, `1` for Left/Deputy). They expand the telemetry dictionary with alternative descriptive keys:
    *   `"diameter"`: Nozzle diameter.
    *   `"id"`: Extruder carriage ID (`0` or `1`).
    *   `"max_temp"`: Target maximum temperature. *(Note: This replaces the standard `"tm"` key)*.
    *   `"serial_number"`: Unique serial number of the hotend assembly. *(Note: This replaces the standard `"sn"` key)*.
    *   `"filament_colour"`: Hex color of the loaded filament (RRGGBBAA format). *(Note: This replaces the standard `"color_m"` key)*.
    *   `"filament_id"`: Filament profile preset identifier. *(Note: This replaces the standard `"fila_id"` key)*.

*   **Vortek Tool Changer Systems (H2C)**:
    These models utilize an array representing both the active carriage and the passive storage rack slots (slots `16` to `21`). They use standard abbreviations:
    *   `"diameter"`: Nozzle diameter.
    *   `"id"`: Carriage ID (`0` or `1`) or rack slot index (`16` to `21`).
    *   `"tm"`: Target maximum temperature.
    *   `"sn"`: Unique serial number of the hotend assembly.
    *   `"color_m"`: Hex color of the loaded filament.
    *   `"fila_id"`: Filament profile preset identifier.
    *   `"p_t"`: Cumulative print time **in seconds** for this individual hotend. A wear/usage counter tied to the physical hotend rather than the position it occupies, which is what makes it meaningful on a rack machine where hotends are swapped between slots. Divide by 3600 for hours. *(Verification source: ha-bambulab PR #2094, which surfaces it as an hours sensor.)*

##### Empty Hotend Reporting and Stale `diameter`
An **empty mounted position is still reported** in `nozzle_info`, and it keeps the **diameter of the nozzle it last held**. Measured on an idle H2C, an unoccupied position read `diameter: 0.4`, `max_temp: 0`, serial `"N/A"`. Two consequences:

1.  `diameter` cannot be trusted without first establishing that a hotend is actually present.
2.  Presence has to be **stated**, not inferred from one field. The serial must be the firmware's explicit `"N/A"` **and** the temperature rating must be absent or zero. Either check alone gives a wrong answer on some model, because a firmware that reports neither field normalizes to the same shape as an empty hotend.

`stat` is **not** usable for this — it read `0` on every entry, occupied and empty alike.

An empty **rack dock** is a different case: it is absent from the payload entirely, so an id in `16`-`21` already implies a nozzle is in it. The rule above is about a mounted position, not a rack slot.

bambino implements the two-field test as `NozzleInfo::is_installed()`. *(Verification source: bambuddy issue #2885.)*

##### `type` Carries Two Vocabularies by Generation
The `"type"` key (bambino's `NozzleInfo::nozzle_type`) reports the nozzle **material** on legacy printers (e.g. `"hardened_steel"`) but a **flow code** on H2-generation printers (`"HH"` high flow / `"HS"` standard, plus a hardware-variant digit pair). Do not assume one and parse the other. This is the same flow-code vocabulary as a K-profile entry's `nozzle_id` — see §7.2 of `reference/07_diagnostics_hms.md` for its encoding and comparison rule.

#### Fan Speed Telemetry Key Mapping [REF-CLIM-FANS]
On-board cooling fan speeds are represented in telemetry via the following JSON keys:
*   `cooling_fan_speed`: Part cooling fan speed.
*   `big_fan1_speed`: Auxiliary cooling fan speed (represents the primary left-side auxiliary fan).
*   `big_fan2_speed`: Chamber exhaust or filtration fan speed.
*   `heatbreak_fan_speed`: Toolhead heatbreak/hotend fan speed.
*   `device.airduct.parts`: On models with a second left-side auxiliary fan (`X2D` and `P2S`), its speed is nested inside this array within the object matching `"id": 160`. The `"state"` parameter of this object holds the speed value directly as an integer percentage (`0` to `100`) and does not require the 0–15 step conversion used by the other fan telemetry keys. Despite the wire port number and this id suggesting a "right" fan, BambuStudio's `DevFan.h` names decoded id 10 `FAN_REMOTE_COOLING_1_IDX` — a second left-side auxiliary fan, distinct from the primary `FAN_REMOTE_COOLING_0_IDX` (port 2, mirrored into `big_fan1_speed`); confirmed against bambuddy's test suite, which titles this fan "P2S/X2D left auxiliary part cooling fan" throughout (issue #60).

#### Developer LAN Mode Bitmask Evaluation
Developer LAN Mode is evaluated via the `fun` telemetry field bit `0x20000000` (which represents the `MQTT_SIGNATURE_REQUIRED` flag). The boolean evaluation is inverted:
*   **`True` (1)**: MQTT Signature/Encryption is **Required** (LAN Developer Mode is **OFF / disabled**).
*   **`False` (0)**: MQTT Signature/Encryption is **Not Required** (LAN Developer Mode is **ON / enabled**).

Depending on the active firmware track and message type, the `"fun"` key drifts within the JSON telemetry payload. On certain firmware versions, it is provided directly at the root level (`payload["fun"]`), whereas on others it is nested within the `"print"` object (`payload["print"]["fun"]`). Evaluating systems must sequentially inspect both JSON paths to retrieve the field.

#### A1 and P1 Series Hardware Probing Protocol
On ESP32-based RTOS hardware lines (P1 and A1 series) where the `fun` field is omitted from telemetry, LAN Developer Mode is verified by attempting a non-destructive publish transaction.

##### Probe Execution Mechanics
The client publishes a command to update the configuration of the virtual external spool slot (re-sending its current configuration parameters, or sending a harmless reset command if empty):
```json
{
  "print": {
    "command": "ams_filament_setting",
    "ams_id": 255,
    "tray_id": 254,
    "tray_info_idx": "GFL99",
    "tray_type": "PLA",
    "tray_color": "00000000",
    "sequence_id": "20000"
  }
}
```
The client then monitors the report topic for the response payload bearing the corresponding `sequence_id` and evaluates the transaction result:
*   **Success** (`result: "success"`): The command channel is active, and LAN Developer Mode is **ON (enabled)**.
*   **Fail** (`result: "failed"` and `reason: "mqtt message verify failed"`): The command is rejected because cryptographic signatures are required, indicating LAN Developer Mode is **OFF (disabled)**.

---

### 3.3 Over-the-Wire Control Command Schema (The Write Stream) [REF-MQTT-LIFECYCLE]

All outbound control payloads are written to `device/{serial_number}/request`. Commands must follow the standardized schemas below.

#### Status Push Initialization
Triggers the printer to emit a complete `"pushall"` state dump on the report topic.
```json
{
  "pushing": {
    "command": "pushall",
    "sequence_id": "20001"
  }
}
```

##### P1/A1 Series Rate Limit
On ESP32-based RTOS hardware lines (P1P, P1S, A1, A1 Mini), the `pushall` command imposes significant processing overhead on the constrained network processor. Issuing this command more frequently than once every 5 minutes can cause observable lag, delayed telemetry broadcasts, and degraded command responsiveness. Clients targeting these platforms should send `pushall` only once at connection establishment and rely on the incremental partial-update stream for ongoing state tracking.

##### Telemetry Update Granularity
The behavior of the report topic stream differs by hardware family:
*   **X1 Series (X1, X1C, X1E, X2D)**: Each automatic report broadcast contains the complete printer state. Clients may treat every incoming message as a full snapshot.
*   **P1/A1 Series (P1P, P1S, A1, A1 Mini)**: Automatic report broadcasts contain only fields that have changed since the previous transmission. Clients must accumulate state across messages to maintain a complete picture, merging each partial update into a persistent state map.

#### Module Version Request
Requests the system partition and hardware expansion bus module firmware versions.
```json
{
  "info": {
    "command": "get_version",
    "sequence_id": "20002"
  }
}
```

#### Print Job Status Control
Controls the state of the active print queue on the physical machine.

##### Pause Print Job
```json
{
  "print": {
    "command": "pause",
    "sequence_id": "20003"
  }
}
```

##### Resume Print Job
```json
{
  "print": {
    "command": "resume",
    "sequence_id": "20004"
  }
}
```

##### Stop/Abort Print Job
```json
{
  "print": {
    "command": "stop",
    "sequence_id": "20005"
  }
}
```

##### Skip Objects During Print
Instructs the printer to bypass the physical execution of specific printed objects (identified by their sliced metadata indices) during an active print job.
```json
{
  "print": {
    "command": "skip_objects",
    "obj_list": [1, 2],
    "sequence_id": "20006"
  }
}
```

##### G-Code Command Queue Wrapper (`gcode_line`) [REF-MOTO-GCODE]
Queues a raw G-code string directly to the printer's motion-controller execution buffer.
```json
{
  "print": {
    "command": "gcode_line",
    "param": "<RAW_GCODE_STRING>\n",
    "sequence_id": "20007"
  }
}
```

##### Submit Print Job (project_file)
Initiates print execution of a sliced `.3mf` file currently residing on the MicroSD card.

**BUG-119**: `flow_cali`/`profile_id`/`project_id`/`task_id` below were previously missing from bambino's payload entirely — bambuddy cites a real production incident (#1478) where a consumer relying on the wrong one of `flow_cali`/`extrude_cali_flag` silently skipped calibration, and a task-continuation firmware bug (#1042/#1011) requiring a fresh `project_id`/`task_id` per submission (not hardcoded `"0"`). `subtask_id`/`project_id`/`task_id` share one value, minted fresh per submission.
```json
{
  "print": {
    "command": "project_file",
    "sequence_id": "20008",
    "param": "Metadata/plate_1.gcode",
    "subtask_name": "My Print Job",
    "subtask_id": "14852932",
    "flow_cali": true,
    "profile_id": "0",
    "project_id": "14852932",
    "task_id": "14852932",
    "file": "job.3mf",
    "url": "ftp://job.3mf",
    "timelapse": true,
    "bed_type": "auto",
    "bed_leveling": true,
    "auto_bed_leveling": 1,
    "extrude_cali_flag": 1,
    "nozzle_offset_cali": 0,
    "vibration_cali": true,
    "layer_inspect": true,
    "use_ams": true,
    "ams_mapping": [-1, 0, 1]
  }
}
```

###### Calibration and Leveling Fields
Standard printer configuration toggles (`timelapse`, `bed_leveling`, `vibration_cali`, and `layer_inspect`) are evaluated as raw **JSON booleans** (`true` or `false`) for every model family. Real-world captures indicate that serializing these fields as integers (e.g., `1` or `0`) can disrupt the firmware's local calibration loops, causing certain architectures (such as the single-nozzle `H2S`) to bypass flow calibration entirely. `bed_leveling` in particular has no tri-state — it is exactly "on"/"off", nothing else.

###### Bed Leveling Auto Companion Field (`auto_bed_leveling`)
BambuStudio pairs the strict-boolean `bed_leveling` field above with a separate integer field, `auto_bed_leveling`, that carries the tri-state intent — confirmed against BambuStudio source (`bambu_networking.hpp`'s `PrintParams::auto_bed_leveling` member and `PrintJob.cpp`'s `params.auto_bed_leveling = this->auto_bed_leveling;`) and a bambuddy wire capture (`bambu_mqtt.py`'s `start_print()`, which sends `"bed_leveling": bed_levelling == "on"` alongside `"auto_bed_leveling": bed_level_int`):
*   `0`: Skip bed leveling (`bed_leveling` is also `false`).
*   `1`: Force bed leveling every print (`bed_leveling` is also `true`).
*   `2`: Auto — firmware runs it only if the bed wasn't leveled recently (`bed_leveling` stays `false`; only this field carries Auto).

This two-field shape is why bed_leveling itself must never become tri-state: the earlier ask to encode Auto directly into `bed_leveling` as an integer would collide with the strict-boolean requirement documented above and the H2S flow-calibration regression it was fixed for.

###### Calibration Execution Flag (`extrude_cali_flag`)
The `extrude_cali_flag` parameter governs dynamic flow calibration and must be serialized as an integer:
*   `1`: Execute/run calibration.
*   `0`: Skip calibration.
*   `2`: Auto — firmware runs it only if flow wasn't calibrated recently (confirmed against BambuStudio's `SelectMachine.cpp` `getValueInt()` tri-state encoding and bambuddy's `_tristate_wire = {"off": 0, "on": 1, "auto": 2}` map).

###### Nozzle Offset Calibration Flag (`nozzle_offset_cali`)
The `nozzle_offset_cali` parameter governs pre-print physical nozzle alignment and is exposed on multi-nozzle carriage platforms (`H2D`, `H2D Pro`, `H2C`, and `X2D`). It must be serialized as an integer:
*   `1`: Execute/run nozzle offset calibration.
*   `0`: Skip calibration.
*   `2`: Auto — firmware runs it only if nozzle offsets weren't calibrated recently (same tri-state encoding as `extrude_cali_flag`; see `CalibUtils.cpp`'s `nozzle_offset_cali = ... ? 2 : 0` default).

On single-nozzle architectures, this field resolves to `0` to prevent the firmware from initiating sensor checks for non-existent secondary hardware carriages.

###### Network Path Rule (`url`)
When submitting direct local network print jobs, the `"url"` parameter universally utilizes the `ftp://` or `ftp:///` scheme across all current hardware series (e.g., `"ftp://job.3mf"`). The printer's onboard print processor automatically resolves this scheme over its local FTP server loop. Cloud-brokered formats such as `/mnt/sdcard/` or `file:///mnt/sdcard/` are exclusively evaluated during remote cloud execution and are rejected by the local command parser when dispatching via the local broker.

###### Inbound `project_file` Response (Screen-Started Prints)
The schemes above are the **outbound** rule. There is also an **inbound** `project_file` response, published unsolicited by the printer on the *report* topic roughly two seconds before `gcode_state` reaches `PREPARE`, carrying `url` and `result`. It fires for prints started from the printer's own touchscreen — which publish nothing on the request topic — as well as for dispatch echoes. It is the only announcement a LAN client gets that a screen-started print is about to begin, and where its file lives.

Gate on `result == "SUCCESS"` **and** a non-empty `url`. Two internal-storage URL shapes appear here and mean different things:

*   `file:///userdata/model/history/<name>.gcode.3mf` — a print of a file that was **already on the printer**: a touchscreen re-print, a Handy start, or a Studio send-to-storage printed later. Port 990 does **not** serve `/userdata/`, so this file is unreachable over FTPS. Note the exclusion is the *root*, not the scheme — a `file://` URL under another root is reachable, e.g. `file:///media/usb0/<name>.gcode.3mf` was measured listable and downloadable over FTPS on an H2D.
*   `brtc://emmc/<name>` — a *dispatch* that chose internal storage. Distinct from the above in cause rather than verdict: a slicer sent the file and the printer filed it where port 990 cannot serve it, which an operator can change by sending it elsewhere. There is no such choice behind a `/userdata/` re-print.

On a **screen start this frame is the only place `ams_mapping` appears** — no slicer ever sent one — so a consumer tracking filament mapping must read it here or not at all. Treat it as a gap-fill: when the request topic already carried this print's mapping, that copy is the slicer's own, and the report-topic echo can arrive without the field.

Related caveat from the same source: an H2S reports `sdcard: true` for its internal eMMC, so **the `sdcard` flag is not a proxy for FTPS reachability**. A consumer deciding whether to sweep FTPS for a 3MF needs both facts.

*(Verification source: bambuddy issue #1820.)*

###### Polymorphic Schema Rule (`ams_mapping`)
The `"ams_mapping"` field varies conditionally based on the operating mode:
*   **AMS Inactive / External Spool Mode (`"use_ams"`: false)**: Must be configured strictly as an empty string (`"ams_mapping"`: "").
*   **AMS Active Mode (`"use_ams"`: true)**: Must be formatted strictly as a **raw JSON array of integers** (e.g., `"ams_mapping"`: [-1, 0, 1]), containing 1-to-1 mappings of the sliced project filaments.
        *   *Array Type Requirement*: Flat mapping must use -1 for unmapped or external/virtual spools; the firmware does not accept raw virtual tray IDs like 254/255 in the flat array, which would cause the print initialization to fail with `0700_8012` "Failed to get AMS mapping table".
        *   *Sub-mapping Detail (`ams_mapping2`)*: For detailed nozzle and material routing, the printer relies on structural extensions which map physical AMS slots or external feeders to corresponding extruder positions.

###### Tool-Changer Nozzle Routing (`nozzle_mapping`, H2C only)
On a nozzle-rack model the `project_file` payload carries a `nozzle_mapping` array selecting which physical nozzle prints each filament slot. Without it, firmware picks the nozzle itself, which is the bug class this field exists to prevent: printing from a carriage that was not the one levelled.

**Two overlapping namespaces — this is the whole hazard.** Extruder indices and physical nozzle IDs are different numbering schemes that collide at low values:

| Carriage | Extruder index | Physical nozzle ID |
| :--- | :--- | :--- |
| Fixed hotend | `1` | `1` |
| Rack position *n* (1-based) | `0` | `15 + n`, i.e. `16`–`21` |

Passing an extruder index where a physical ID belongs is silently wrong rather than obviously wrong. Observed consequences upstream: the printer levels with one nozzle and prints with another millimetres off the bed, or rejects the job outright as a hotend mismatch (HMS `0500-4047`).

*   **The array is a fixed 32 entries**, padded with `-1` for slots the plate does not print — *not* the plate's filament count.
*   **Omission is the correct failure mode.** BambuStudio omits `nozzle_mapping` entirely for a plate sliced for the fixed hotend only, and any client that cannot resolve the routing with confidence should do the same rather than guess. Firmware auto-pick is suboptimal; a wrong physical ID is destructive.

**Two prior claims here were wrong and are corrected above — re-read this table rather than trusting older notes.** Upstream initially had the extruder polarity inverted (fixed = `0`, rack = `1`), which dispatched a plate to the carriage that had not been levelled and printed its first layer in mid-air; it was corrected to fixed = `1`, rack = `0` (bambuddy `45dc139c`, 2026-08-14). Upstream also briefly derived the array length from the plate's filament count on the strength of a single 3-entry capture, then reverted to a fixed 32 after observing a real 3-filament project dispatch as 32 entries (`[16, 1, 18, -1 ×29]`, captured 2026-08-13); the 3-entry capture had been a calibration job, so its length tracked what Studio was doing rather than the filament count.

**Verification source:** bambuddy's hardware-measured constants (`3954d3a7`, `45dc139c`, `dfeac792`, `ec26cba9`), cross-checked against their `ams_extruder_map` telemetry and native BambuStudio dispatch captures on the maintainer's H2C. **Not verified here — no H2C is available.** bambino implements this in `resolve_rack_nozzle_mapping`, gated by `ModelQuirks::uses_nozzle_rack()`.

###### Deliberately Omitted Fields (`md5`, `cfg`, `extrude_cali_manual_mode`)
Upstream bambuddy sends three fields in `project_file` that bambino does not, all as fixed defaults: `"md5": ""`, `"cfg": "0"`, and `"extrude_cali_manual_mode": 0`. **The omission is deliberate, not an oversight — do not "fix" it without new evidence.**

*   Prints submitted by bambino succeed on a P1S without them, which is the only direct evidence available about whether firmware requires them. It does not.
*   All three are constants in upstream, never varying with job configuration. Nothing computes an actual MD5; the field is sent empty.
*   BambuStudio's own payload cannot be consulted. `project_file` is assembled inside the closed-source `bambu_networking` library — the open tree only marshals values into `BBL::PrintParams` (`slic3r/Utils/bambu_networking.hpp`, `GUI/Jobs/PrintJob.cpp`) and hands them over. Reading BambuStudio's source cannot answer which fields it puts on the wire.
*   Observing BambuStudio's traffic was investigated and ruled out as disproportionate: the `request` topic is not subscribable (see "Topic Access Control" below), BambuStudio's own logs are encrypted (`*_enc.log`), and it verifies the printer's TLS certificate — so a capture requires installing a custom CA into the slicer's trust store plus a TLS-terminating relay. That is a reasonable thing to build for general protocol capture, but not to confirm three inert constants.

If a capture ever *does* become available, check these three first and update this note.

###### Model Gate (`vibration_cali`)
`vibration_cali` is sent on every model **except P2S**, where it must be forced to `false` regardless of the caller's request. The P2S does not run vibration compensation the way the X1/P1 series does.

*   Note `N7` and `P2S` are the same machine — `N7` is the internal/SSDP code (BambuStudio ships both `resources/printers/N7.json` and `resources/profiles/BBL/machine/Bambu Lab P2S.json`). Upstream's check reads `("P2S", "N7")` because it matches on a name string that may arrive in either form; a decode that already resolves to a model enum needs only the one variant.
*   In this crate the gate lives in the quirks engine (`PrinterQuirks::supports_vibration_compensation`), not as an inline model match, per the quirks invariant in root `CLAUDE.md`. Upstream instead overwrites the field after building the payload.

**Verification source — weaker than the rest of this document, read the caveat.** This rests on bambuddy commit `be18ebb3` ("Fix P2S printer support - disable vibration_cali and fix FTP SSL") and nothing else. BambuStudio cannot corroborate it: its printer profiles carry 30+ `support_*` capability keys and **none** concerns vibration (verified across `BL-P001`, `C11`, `C12`, `N1`, `N2S`, `N7`, `N9`, `O1D`), and its own vibration checkbox (`Calibration.cpp:57`) is ungated by model. What raises confidence is that the *other* half of that same commit is the P2S FTPS TLS-1.3 quirk, independently confirmed here and implemented in `src/quirks/models/p2.rs` — the contributor was demonstrably working from a real P2S. That is corroboration of the source, not of this claim.

No P2S has been available to verify this directly. See issue #133; confirm before treating it as settled.

###### Polymorphic Typing Rule (`use_ams`)
The `"use_ams"` parameter must strictly be serialized as a JSON boolean (`true` or `false`). On dual-nozzle IDEX printers (such as the H2D and H2D Pro), the printer's onboard JSON parser processes this field polymorphically. If the `use_ams` field is serialized on the wire as an integer (`1` or `0`) instead of a boolean, the parser interprets the value as the physical nozzle carriage index (`1` = Left/Deputy nozzle) rather than a boolean material routing flag.

#### Enclosure LED Lighting Control (`ledctrl`)
Controls the activation mode and flashing cycles of the internal LEDs.
```json
{
  "system": {
    "sequence_id": "30008",
    "command": "ledctrl",
    "led_node": "chamber_light",
    "led_mode": "on",
    "led_on_time": 500,
    "led_off_time": 500,
    "loop_times": 0,
    "interval_time": 0
  }
}
```
*   `led_node`: Targets specific hardware strips. Can be `"chamber_light"` (Left Side / Primary), `"chamber_light2"` (Right Side / Secondary), or `"work_light"` (toolhead LED, X1 only).
*   `led_mode`: Set to `"on"`, `"off"`, or `"flashing"`.
*   `loop_times` and `interval_time`: Must be configured strictly as `0` if `led_mode` is `"on"` or `"off"`.

#### Scheduled Pause List (`print.p_list`)
While a job with scheduled pauses is loaded (typically filament swaps), `push_status` carries a `p_list` object describing them. It lets a consumer show upcoming pauses before they happen.

```json
"p_list": {
  "total": 2,
  "list": [
    { "p": 45, "t": 3600, "i": 0, "l": 120 }
  ]
}
```

*   `total` — number of pauses scheduled for the job.
*   `list[]` — the pauses themselves, with single-letter keys: `p` = percent complete at the pause, `t` = remaining print time at that point (seconds), `i` = pause index within the schedule, `l` = layer number.
*   The array is **not ordered by `i`**. Resolve "next pause" by minimum `i`, not by array position.
*   Absent whenever no job with pauses is loaded, which is not an error.

BambuStudio's parser requires all four keys on every point and discards the entire schedule if any is missing. bambino is deliberately more tolerant — it keeps whatever parsed and treats a point with no `i` as unorderable rather than as index 0 — so a single malformed entry cannot blank the list.

**Verification source:** BambuStudio `DevPrintTaskInfo.cpp::parsePauseList` (commit `7cbd2e27`), read directly. **Not confirmed against a capture taken here** — the abbreviated key meanings in particular are upstream's naming, not an observed wire sample. See issue #139.

#### Topic Access Control (`request` is not observable)
Each printer exposes two MQTT topics: `device/<serial>/report`, which the printer publishes telemetry to, and `device/<serial>/request`, which clients publish commands to. A natural way to capture what another client (e.g. BambuStudio) sends would be to authenticate as a second client and subscribe to `request`. **This does not work on a P1S.**

Observed behaviour: the broker accepts the CONNECT normally (CONNACK return code `0`), the client writes its SUBSCRIBE for `device/<serial>/request`, and the broker then **resets the TCP connection** while the client awaits SUBACK. It does *not* return the `0x80` failure SUBACK that MQTT 3.1.1 §3.9.3 defines for a refused subscription.

*   Authentication is not the issue — the same credentials connect fine and the CONNACK succeeds. Only the subscription is refused.
*   A client must therefore treat **both** a failure SUBACK and a mid-handshake connection reset as "subscription refused". Handling only the spec-defined rejection misses the case that actually occurs.
*   Consequence: bambino cannot observe another client's requests from the side. Capturing them requires a proxy between that client and the printer, or standing up an endpoint the client connects to instead.

Only P1S has been tested. Whether other models refuse the same way, refuse cleanly, or permit the subscription is unknown.

**Do not re-attempt this approach without new evidence.** A `bambino-cli sniff` subcommand was written to do exactly this and removed once the P1S refused it — the finding is the artifact worth keeping, not the code. If a future model is found to permit the subscription, the implementation is recoverable from git history (search the log for `sniff`).

**Verification source:** direct observation against a P1S (issue #134) — CONNACK return code `0`, SUBSCRIBE written, TCP reset while awaiting SUBACK.

#### Access Code Readback (`get_access_code`)
Asks the printer to report its own current LAN access code over an already-authenticated session. This is distinct from the access code a client supplies to connect: it lets a connected client re-read the value, which is how it notices that a rotated code has invalidated its cached credential.

Request:
```json
{
  "system": {
    "sequence_id": "30009",
    "command": "get_access_code"
  }
}
```

Response:
```json
{
  "system": {
    "command": "get_access_code",
    "access_code": "<code>",
    "result": "success",
    "reason": "success",
    "sequence_id": "30009"
  }
}
```

*   The reply is `system`-wrapped and echoes the request's `sequence_id`, so it is correlatable; `get_access_code` is on `ACK_CORRELATED_COMMANDS` on that basis.
*   `result` and `reason` are present on the wire even though BambuStudio's own client does not read them — do not assume a reply consisting of `access_code` alone.
*   **Handle the returned value as a credential.** It must never be logged or written to a file in this repository; `bambino-cli ack-probe` masks it before writing its report for exactly this reason.

**Verification source:** observed directly on a P1S via `bambino-cli ack-probe -t get_access_code` (issue #140) — `verdict: ack_correlated`, 14 ms round trip, `result: "success"`. The command's existence was first identified in BambuStudio commit `1678b5ac` (`MachineObject::request_access_code`, refined by `5cc7ed4f`); the `result`/`reason` fields above come from the local capture, not from upstream. Not yet re-verified on any other model.

#### Airduct AC Mode Selection (`set_airduct`)
Controls the target routing of the internal air circulation flaps.
```json
{
  "print": {
    "command": "set_airduct",
    "modeId": 0,
    "submode": -1,
    "sequence_id": "30007"
  }
}
```
*   `modeId`: `0` = cooling mode (recirculation dampers close, hot air routes through exhaust), `1` = heating mode (exhaust flaps close, enclosure seals for heat retention), `2` = laser mode (airflow configured for laser engraving module operation).

#### Sound & Alerting Commands

##### Configure Prompt Sound Enablement (`print_option`)
Configures whether the printer's onboard speakers emit structural sound notifications during user-facing events. Supported on: `A1`, `A1 Mini`, `A2L`.
```json
{
  "print": {
    "command": "print_option",
    "sound_enable": true,
    "sequence_id": "30014"
  }
}
```

##### Configure Enclosure Buzzer Mode (`buzzer_ctrl`)
Controls the operating behavior of the physical fire alarm buzzer module. Supported on: `H2S`, `H2D`, `H2D Pro`, `H2C`.
```json
{
  "print": {
    "command": "buzzer_ctrl",
    "mode": 0,
    "reason": "",
    "sequence_id": "30015"
  }
}
```
*   `mode`: Integer code representing the target alarm state:
    *   `0`: Silent (disarmed)
    *   `1`: Alarm (triggered)
    *   `2`: Beeping (attention required)

#### Material Settings & Configuration
Modifies filament and preset attributes inside physical or virtual storage slots.

##### Set Tray Filament Attributes (ams_filament_setting)
```json
{
  "print": {
    "command": "ams_filament_setting",
    "sequence_id": "20009",
    "ams_id": 255,
    "tray_id": 254,
    "tray_info_idx": "GFL05",
    "tray_type": "PLA",
    "tray_sub_brands": "PLA Basic",
    "tray_color": "FFFF00FF",
    "nozzle_temp_min": 190,
    "nozzle_temp_max": 230
  }
}
```

###### Polymorphic Tray ID Rule (`tray_id`)
The directory parameters inside `"ams_filament_setting"` behave polymorphically based on the targeted unit and printer architecture:
*   **Standard AMS Units (`0` to `3`)**: `tray_id` strictly represents the local slot index (`0` to `3`) within the designated unit.
*   **Virtual External Spool (`255` / `254`)**:
    *   **Single-Nozzle Printers (X1, P1, A1 series)**: When targeting the virtual external spool, `ams_id` must be set to `255` and `tray_id` must be set to `254`. Transmitting a local slot index (such as `0`) alongside an `ams_id` of `255` causes the printer's local broker (most notably on the `P1S` track) to reject the payload and return a failure response (`result: "fail"`).
    *   **Dual-Nozzle IDEX Printers (H2D, X2D, H2C series)**: The external spools are mapped as independent virtual units. Both the left external spool (Ext-L, `ams_id: 254`) and the right external spool (Ext-R, `ams_id: 255`) must be configured with `tray_id: 254` (BUG-117; confirmed against BambuStudio's `command_ams_filament_settings`, `DeviceManager.cpp:1667-1693` — `tag_ams_id` `254` or `255` both map to `tag_tray_id = 254`, never `0`).

##### AMS Physical Control (ams_control)
Resumes, pauses, or resets material changes and active physical operations inside the expansion bus feed system.
```json
{
  "print": {
    "command": "ams_control",
    "param": "resume",
    "sequence_id": "20011"
  }
}
```

##### Trigger Physical RFID Scan (ams_get_rfid)
Commands the designated AMS unit to physically advance the filament in a targeted slot to its internal reader node and execute a passive RFID tag inventory sweep.
```json
{
  "print": {
    "command": "ams_get_rfid",
    "ams_id": 0,
    "slot_id": 1,
    "sequence_id": "20012"
  }
}
```

#### Physical Calibration Controls

##### Hardware Calibration Routines (calibration)
Triggers automatic physical self-test, structural, and alignment calibrations on the physical machine chassis.
```json
{
  "print": {
    "command": "calibration",
    "option": 6,
    "sequence_id": "20013"
  }
}
```

###### Option Bitmask Calculation
The `"option"` value is a 32-bit integer built by evaluating active calibration requests against a strict bitwise parameter mapping:

| Active Bit | Decimal Value | Calibration Target |
| :--- | :--- | :--- |
| **Bit 0** | `1` | Micro-Camera (xcam) Calibration *(Internal)* |
| **Bit 1** | `2` | Auto Bed Leveling |
| **Bit 2** | `4` | Vibration Compensation (Resonance Sweep) |
| **Bit 3** | `8` | Motor Noise Cancellation |
| **Bit 4** | `16` | Nozzle Height/Tilt Calibration (IDEX/Dual-Nozzle only) |
| **Bit 5** | `32` | Heatbed Leveling & Thermal Profile Calibration |
| **Bit 6** | `64` | Nozzle Clumping Position Calibration *(Internal)* |

###### Observed Run Telemetry

**Verification source: direct wire capture, P1S firmware `01.10.00.00`, two runs (`option: 2` and `option: 6`).** This supersedes the earlier assumption that a standalone calibration reports nothing observable.

The command is acknowledged on the report topic before the run starts:

```json
{"command": "calibration", "option": 2, "result": "success", "reason": "success"}
```

During the run the printer reports as if executing a print job, with these distinguishing marks:

*   `print_type` becomes `"system"` (not `"local"`/`"idle"`), and `subtask_name` is `"auto_cali_for_user_param.gcode"`. Together these are the reliable way to tell a calibration sweep from a user print.
*   `gcode_state` walks `IDLE` → `RUNNING` → `FINISH`; `mc_print_stage` is `"2"` during the run and `"1"` at finish.
*   `layer_num`/`total_layer_num` are `0` — meaningless here, do not surface them.

**`mc_percent` is a single aggregate ramp over the whole sweep, not per-routine.** It climbs monotonically `0 → 100` with no reset at a routine boundary, and a single-routine run (`option: 2`) covers the same full `0 → 100` range as a two-routine run (`option: 6`). A consumer can therefore drive one progress bar from it, but cannot derive per-routine progress. `mc_remaining_time` populates alongside it and counts down in **minutes** (`26 → 0` on both runs).

**Per-routine boundaries come from `stg`/`stg_cur`, which are emitted in `msg: 1` incremental updates** (see the correction in the numbered notes above — an earlier claim that these were `pushall`-only was wrong). `stg` is the queued stage list for the run and `stg_cur` is the stage currently executing; `stg` empties to `[]` at `FINISH`. The queue corresponds directly to the option bitmask:

| `option` | Routines requested | `stg` | `stg_cur` sequence |
| :--- | :--- | :--- | :--- |
| `2` | Auto Bed Leveling | `[14, 1]` | `255` → `14` → `1` → `0` |
| `6` | Auto Bed Leveling + Vibration Compensation | `[14, 1, 3]` | `255` → `14` → `1` → `3` → `0` |

Setting bit 2 (Vibration Compensation) added exactly one stage, `3`, to the queue. Stage IDs match pybambu's `CURRENT_STAGE_IDS` (`const.py`): `14` = `cleaning_nozzle_tip` (an unrequested preamble the firmware always queues first), `1` = `auto_bed_leveling`, `3` = `sweeping_xy_mech_mode` (annotated upstream as vibration compensation), `255` = idle in the P1 encoding, `0` = `printing`. Note `stg_cur` reaches `0` near 99% while `gcode_state` is still `RUNNING`, so the [REF-MQTT-IDLEBUG] gate correctly leaves it readable there.

Unverified beyond this: only bits 1 and 2 have been exercised, and only on a P1S. Bit 4 is gated to IDEX/dual-nozzle hardware, so it will not produce a stage on a single-nozzle machine.

#### Printer Motion & Operation Parameters

##### Configure Print Feed Speed Level (print_speed)
Dynamically scales maximum velocity and acceleration envelopes during an active print task.
```json
{
  "print": {
    "command": "print_speed",
    "param": "2",
    "sequence_id": "20014"
  }
}
```

###### Speed Parameters Schema
The `"param"` field represents speed scaling targets and must be serialized on the wire as a string containing one of the following scale keys:
*   `"1"`: Silent Mode (50% max acceleration and feedrate limits).
*   `"2"`: Standard Mode (100% nominal feedrate limit).
*   `"3"`: Sport Mode (124% nominal feedrate limit).
*   `"4"`: Ludicrous Mode (166% nominal feedrate limit).

#### Diagnostic Fault Management
Clears active error codes from the printer's active status state.

##### Clear Active Error
```json
{
  "print": {
    "command": "clean_print_error",
    "sequence_id": "20010"
  }
}
```

#### Command Acknowledgment Envelope [REF-MQTT-ACK]

All commands published to the request topic produce an acknowledgment response on the report topic. The ack echoes the command name and the client's `sequence_id`, enabling correlation. The envelope varies by command family:

**Print commands** (gcode_line, pause, resume, stop, clean_print_error, calibration, print_speed, etc.):
```json
{
  "print": {
    "command": "<echoed_command_name>",
    "param": "<echoed_param_if_present>",
    "reason": "success",
    "result": "success",
    "sequence_id": "<echoed_sequence_id>"
  }
}
```

**System commands** (ledctrl):
```json
{
  "system": {
    "command": "ledctrl",
    "led_node": "<echoed>",
    "led_mode": "<echoed>",
    "led_on_time": 0,
    "led_off_time": 0,
    "loop_times": 0,
    "interval_time": 0,
    "reason": "success",
    "result": "success",
    "sequence_id": "<echoed_sequence_id>"
  }
}
```

**Observed behavior (P1S, firmware 2025):** All tested commands return `result: "success"` regardless of whether the command had a meaningful effect. This includes motion commands when axes are unhomed, pause/resume/stop when no print is active, and clearing errors when none exist. The ack confirms command *receipt and dispatch*, not successful *execution*. Clients must not treat `result: "success"` as confirmation that the intended physical action occurred.

The printer's own incremental `push_status` telemetry uses an independent `sequence_id` counter (starting from low values like `0` or `1`), separate from the client's command sequence IDs. This makes correlation unambiguous — command acks carry the client's high-value sequence IDs, while background telemetry carries the printer's own counter.

###### Ack Correlation Confirmed by Wire Capture (P1S, firmware 2025)

A `bambino-cli ack-probe` run (issue #26) published each of the following with a client-chosen `sequence_id` and captured the response stream for 5s. All eight echoed that exact `sequence_id` back inside a `print` wrapper carrying the same `command` name, 13–57ms after publish, with background `push_status` traffic interleaved in six of the eight windows:

| Command | Ack latency | `result` |
| :--- | :--- | :--- |
| `ams_control` | 13ms | `success` |
| `ams_get_rfid` | 22ms | `success` |
| `skip_objects` | 35ms | `success` |
| `set_airduct` | 50ms | `success` |
| `print_option` | 16ms | `success` |
| `buzzer_ctrl` | 15ms | `success` |
| `ams_change_filament` | 29ms | `success` |
| `project_file` | 57ms | `success` |

This extends the "success regardless of effect" observation above in an important direction: `set_airduct` and `buzzer_ctrl` address hardware a P1S does not physically have (no chamber airduct damper, no fire-alarm buzzer), and `project_file` named a `.3mf` that does not exist on the SD card. All three still acked `result: "success"`. The ack therefore does not confirm feature support or file existence, only that the command reached the broker.

`project_file`'s ack echoes the entire submitted payload back verbatim (every flag, `task_id`/`subtask_id`/`project_id`, and the derived `url`), unlike the other commands' compact `command`+`reason`+`result`+`sequence_id` envelope.

Not confirmed by this run: whether any of these behave the same on other models. The capture is P1S-only.

---

### 3.4 Mechanical & Firmware Quirks

#### Keep-Alive Socket Zombies [REF-MQTT-ZOMBIE]
Under certain conditions, the broker on P1P, P1S, and A1 series printers enters a state where the TCP socket remains established and publishes telemetry updates, but completely ignores or discards incoming control commands published to the `request` topic. This write-channel silent failure can only be detected on the wire by verifying that the printer publishes a corresponding state change or response payload on the `report` topic within 10 seconds of a write transaction.

#### Local QoS 1 Queue Replay Errors [REF-MQTT-REPLAY]
When re-establishing an MQTTS connection with Clean Session = False, the broker's QoS 1 queue replay mechanism may retransmit previously unacknowledged control commands (such as `project_file`) on the command topic. Replaying a print job payload (`project_file`) while a print is already actively running causes the motion-controller board to throw error `0500_4003` (MicroSD Read/Write Exception) and halt execution.
