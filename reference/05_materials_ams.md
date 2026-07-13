# Chapter 5: Physical Material Expansion (AMS, AMS-HT & Spools)

---

### 5.1 Bus Telemetry & Bitmask Parsing [REF-AMS-DECODE]

The physical printer monitors modular material expansion units connected to its hardware expansion bus. Telemetry updates are broadcast within the `"print"` root envelope under the `"ams"` and `"ams_status"` keys.

#### Spool Presence Masking
The physical presence of loaded spools across standard expansion units is tracked via a hexadecimal bitmask string:
*   `tray_exist_bits`: A hexadecimal string representing which physical slots contain a spool.

##### Bitmask Evaluation Formulas
To determine if a physical spool is present in a specific standard slot, parse the hexadecimal string `tray_exist_bits` to an integer and evaluate the bitwise representation using standard shift logic. 

Standard physical AMS units (IDs `0` to `3`) each manage 4 physical slots. The global tray bit index is calculated as follows:

$$\text{shift\_standard} = (\text{ams\_id} \times 4) + \text{tray\_id}$$

With the shift index determined, the existence of the physical spool in the slot is verified:

$$\text{slot\_exists} = (\text{tray\_exist\_bits} \gg \text{shift\_standard}) \ \& \ 1$$

*   **AMS-HT Units (IDs 128-135)**: These single-slot, high-temperature dry-chamber units reside on a separate addressing scheme. They do not register on the standard `tray_exist_bits` bitmask. Their presence is evaluated directly through their reported slot state parameters in the status payload.

##### The Printer-Shutdown Telemetry Exception
During printer shutdown routines, the firmware emits a final status update where `tray_exist_bits` evaluates to `0` and the `power_on_flag` boolean is set to `false`. To prevent telemetry parsers from falsely interpreting this final update as a physical spool-removal event, updates where `tray_exist_bits = 0` must be ignored strictly when `power_on_flag` is `false`. Conversely, if `power_on_flag` is `false` but `tray_exist_bits` is non-zero, this represents a valid idle-printer state and changes must be processed normally.

#### Over-the-Wire Slot State Mappings
The physical printer's AMS controller represents spool presence and active routing status using native integer codes in the `"state"` parameter of each tray object.

| Native Integer Code | Protocol State Meaning | Physical Hardware Condition |
| :--- | :--- | :--- |
| **`11`** | Loaded / Active | Spool detected; filament has successfully fed past the hub multiplexer and is routed to the Active Toolhead. |
| **`10`** | Spool Present | Spool detected in the tray and loaded into the local feeder drive, but filament is currently retracted (unloaded from toolhead). |
| **`9`** / **`0`** | Empty Slot | No spool is detected in the tray, or the slot configuration has been cleared. |
| *(Key Absent)* | Empty Slot | **Firmware Exception**: On some firmwares (e.g., P1S, A1 Mini), physically removing a spool omits the `"state"` key entirely (yielding `{id: N}`). Absent state parameters default to `9`. |

##### Symmetrical Absent-Key Empty Slot Signalling (P1S & A1 Mini)
On the P1 and A1 hardware series, when a physical AMS slot is completely empty, the printer emits an extremely truncated JSON object for that tray (e.g., only `{"id": N}`). All other keys (such as `state`, `tray_type`, etc.) are completely omitted. Evaluators must interpret the absence of these descriptive keys as a physical empty-slot indicator, defaulting the tray's state to `9` (Empty).

##### Incremental Telemetry Update Slot Cleansing Rules
When the bitwise presence check indicates a spool has been removed (or when the `"state"` transitions to a non-loaded code like `9`), all stale material telemetry fields (including `tray_type`, `tray_color`, `tray_info_idx`, `tag_uid`, `tray_uuid`, and `remain`) must be explicitly cleared or nullified by the parser. This is required because the printer's incremental telemetry updates often omit configuration keys for inactive slots, causing stale material attributes to persist indefinitely in standard parsers.

Additionally, some models (such as `H2D`) only emit `{id, state}` in incremental updates when a slot is not fully loaded. A transition to state `9` (empty) or `10` (present but retracted), or receiving an empty string for `tray_type`, must be treated as an explicit clearing signal. Without this active sanitization, stale material parameters from previously loaded spools will persist in the state representation.

**Verification source (BUG-012):** confirmed against two independent reverse-engineering projects, not just this doc's original wording. `pybambu`'s `AMSTrayStateFlags` bitmask model disagreed (treated state 9 as "present but unknown"), but `Bambuddy`'s `bambu_mqtt.py` (`apply_tray_exist_bits`, incremental-merge handler citing issue #784) and `main.py`'s `on_ams_change` (`loaded = cur_state == 11 or (cur_state not in (9, 10) and cur_type.strip())`, citing issue #1322, cross-tested against H2D/A1 Mini/P1S firmware) both treat `state ∈ {9, 10}` as the firmware's explicit "not loaded" signals — matching this doc, not pybambu. `src/ams/parser.rs::clean_stale_tray_data` now clears on both.

#### Multi-AMS Local Index Resolution (`tray_now`)
When multiple standard AMS units or virtual slots are connected, the printer's status stream may report only the local slot ID (0-3) in `tray_now` rather than a global ID.

*   **IDEX / Dual-Nozzle Printers (H2D series)**: Reports only slot numbers `0-3` representing the active tray position on the active extruder's linked AMS. State trackers must evaluate the `active_extruder` (0 = Right/Main, 1 = Left/Deputy) alongside the `ams_extruder_map` to determine which physical AMS unit has loaded the filament, then calculate the global ID accordingly.
*   **Single-Nozzle Printers (P2S series)**: Multi-AMS configurations on single-nozzle printers may also report local slot indices in `tray_now`. State trackers must evaluate the MQTT `mapping` array field (refer to Section 5.3) to match the local slot position to the active physical AMS unit.

#### AMS Unit Info Bitmask (`info` Field)
Each AMS unit object in the `print.ams.ams[]` array may include an `"info"` field — a hex-encoded bitmask string (e.g. `"11002103"`). Parse via `u64::from_str_radix(s, 16)`. The bit layout encodes unit metadata and IDEX routing:

| Bit Range | Mask | Field | Values |
| :--- | :--- | :--- | :--- |
| **0–3** | `0xF` | AMS unit type | e.g. `3` = AMS Lite |
| **4–7** | `0xF0` | Dry status | Drying cycle state |
| **8–11** | `0xF00` | Extruder assignment | `0` = right/main, `1` = left/deputy, `0xE` = uninitialized |
| **22–23** | `0xC00000` | Dry sub-status | Drying sub-state detail (bits 24–25 belong to the unrelated `bind_switch_in` field) |

The extruder assignment field is used on IDEX platforms to track which extruder carriage an AMS unit is physically wired to. A value of `0xE` indicates the assignment has not been initialized by the firmware.

#### Virtual / External Spool Telemetry (`vt_tray` and `vir_slot`)
External spool holders (filament loaded directly into the extruder without an AMS unit) report their state via two distinct telemetry paths depending on the platform architecture:
*   **Single-Nozzle Platforms (P1S, A1, X1C, H2S, etc.)**: The `print.vt_tray` field contains a single object with the same schema as an AMS tray (`tray_type`, `tray_color`, `tray_info_idx`, `tag_uid`, `tray_uuid`, `remain`, temperature limits, calibration indices, etc.). The virtual tray ID is typically `"254"`.
*   **Dual-Nozzle IDEX Platforms (H2D, H2D Pro, X2D)**: The `print.vir_slot` field contains an array of objects (one per extruder), each using the same schema. Index `0` corresponds to the right/primary external spool, index `1` to the left/deputy external spool.

Both fields are optional — they are only present when the printer has external spool data to report.

#### Combined AMS Status Bitmask (`ams_status`)
The `print.ams_status` field is a 32-bit integer encoding the combined operational state of the AMS expansion bus:
*   **Bits 0–7 (low byte)**: AMS sub-status code.
*   **Bits 8–15**: AMS main status code.

This field provides a high-level summary of the AMS bus state (e.g. idle, feeding, retracting, error) without requiring inspection of individual unit or tray states.

#### Bus Module Firmware & Serial Number Query (get_version Response)
The unique hardware serial numbers and active firmware versions of expansion bus modules are not broadcast in standard telemetry heartbeats. Instead, they are queried over the command channel using the `get_version` request (`[REF-MQTT-LIFECYCLE]`).

The printer returns a structural JSON response over the report topic containing a `"module"` array nested inside an `"info"` root-level object:

```json
{
  "info": {
    "command": "get_version",
    "module": [
      {"name": "ota", "sw_ver": "01.10.00.00", "sn": "01P00A4C2009981"},
      {"name": "esp32", "sw_ver": "01.16.38.70", "sn": "01P00A4C2009981"},
      {"name": "mc", "sw_ver": "00.01.33.24", "sn": "01D000000000001"},
      {"name": "th", "sw_ver": "00.02.09.98", "sn": "01E000000000001"},
      {"name": "n3f/0", "sw_ver": "03.00.21.29", "sn": "19C0FFFFFFFFFFF"}
    ],
    "reason": "",
    "result": "success",
    "sequence_id": "10002"
  }
}
```

The system must map the `"name"` field of each module object using the following naming conventions to identify the physical expansion unit and index:
*   **`ams/<id>`**: Original AMS unit (e.g., standard CoreXY multi-material systems). The trailing number `<id>` corresponds to the physical `ams_id` ($0 \le \text{id} \le 3$).
*   **`n3f/<id>`**: AMS 2 Pro unit. The trailing number `<id>` corresponds to the physical `ams_id` ($0 \le \text{id} \le 3$).
*   **`n3s/<id>`**: AMS-HT dry-chamber unit. The trailing number `<id>` represents the physical single-slot ID, typically starting at $128$ (e.g., `n3s/128`).

---

### 5.2 Spool Presets, Colors & RFID Serialization [REF-AMS-SP_CFG]

For spools equipped with proprietary Bambu Lab RFID tags, the printer automatically scans and populates material characteristics. Generic spools must be configured manually over MQTTS.

#### RFID Tag Serialization
*   `tag_uid`: Unique 16-character hexadecimal string read from the physical RFID tag.
*   `tray_uuid`: Unique 32-character hexadecimal string representing the globally unique ID of the filament spool.

#### Preset Identifiers (`tray_info_idx`)
The `"tray_info_idx"` property contains the short-format preset ID (e.g., `"GFA01"` for Bambu PLA Matte). Custom user presets created in the slicer are assigned a unique, randomized setting ID prefixed with `"PF"` followed by 17 numeric digits (e.g., `"PF12345678901234567"`).

#### Color Encoding
Color parameters (`"tray_color"` and `"cols"`) are formatted as 8-character hexadecimal strings representing RRGGBBAA. Empty or unconfigured slots transmit `"00000000"` (zeroed alpha channel), whereas configured filaments use `"RRGGBBAA"` with `"FF"` alpha (e.g., `"FF0000FF"`).

---

### 5.3 AMS Slicer Mappings & Filament Changes [REF-AMS-MAP]

Slicers coordinate the mapping between project-defined filaments and the physical hardware channels using two symmetrical array structures inside the `print.project_file` command payload.

#### Flat `ams_mapping` Array
The `"ams_mapping"` parameter is a flat, 1-to-1, forward-mapped JSON array of integers that correlates the filament slots defined in the sliced print job to the physical hardware channels of the printer.

##### Array Mapping Mechanics
*   **Array Length via Maximum ID**: The length of the `ams_mapping` array is determined by the *highest filament ID index* present in the sliced project metadata (`slice_info.config`), not the total count of unique filaments. If a project utilizes filament ID 1 and filament ID 4, the array must be sized to 4.
*   **Forward 0-Indexed Mapping & Padding**: The array positions map sequentially from left to right. Index $i$ corresponds directly to the 0-indexed filament slot $i$ defined in the sliced project file. Intermediate unused filament IDs must be explicitly padded with the `-1` (unmapped) sentinel.

##### Hardware Channel Identifiers
The integer values within the flat `ams_mapping` array represent absolute physical hardware channels:
*   **`0` to `103`**: Standard AMS channels. Calculated via $(\text{ams\_id} \times 4) + \text{slot\_id}$.
*   **`128` to `135`**: Physical single-slot high-temperature AMS-HT units. Global channel ID equals the unit's bus ID ($\text{ams\_id}$).
*   **`-1`**: Omit/Unmapped. Mandatory marker for any unused project filament slot or any slot routed to an **External Spool** (non-bus tray).

##### External Spool Flat-Mapping Restrictions
The flat `ams_mapping` array cannot accept absolute external spool IDs (`254` or `255`). To print from an external spool, the flat array must assign `-1` (unmapped) to the respective index, and let the structured `ams_mapping2` array handle specific external routing. Passing virtual external spool IDs directly into the flat array triggers a `"Failed to get AMS mapping table"` exception (such as error `0700_8012` or `07FF_8012`) on the motion board.

#### Structured `ams_mapping2` Array
The `"ams_mapping2"` parameter is a JSON array of structured objects that maintains a direct 1-to-1 index pairing with `"ams_mapping"`. It defines detailed unit and slot routing:

*   **Standard AMS Slot**: `{"ams_id": ams_id, "slot_id": slot_id}` (where $0 \le \text{ams\_id} \le 3$, and $0 \le \text{slot\_id} \le 3$).
*   **AMS-HT Slot**: `{"ams_id": ams_id, "slot_id": 0}` (where $128 \le \text{ams\_id} \le 135$).
*   **Unmapped / Unused Filament**: `{"ams_id": 255, "slot_id": 255}`. *(Note: Do not pass `-1` inside the structured object; it violates 8-bit unsigned bounds)*.
*   **External Spool**:
    *   *Single-Nozzle Printers*: `{"ams_id": 255, "slot_id": 0}`.
    *   *Dual-Nozzle IDEX Printers*: `{"ams_id": 254, "slot_id": 0}` (Left/Deputy) or `{"ams_id": 255, "slot_id": 0}` (Right/Primary).

##### Mandatory use_ams Override on Single-Nozzle Systems [REF-AMS-USEAMS]
On single-nozzle platforms (such as the X1C, P1S, A1, and H2S), if all mapped filaments reside on the external spool (no active spool is routed to a physical AMS unit), the `use_ams` command parameter must be set strictly to `false` in the dispatch payload. If `use_ams: true` is transmitted when printing exclusively from the external spool, the print processor fails to build the material routing table, rejecting the task with error `07FF_8012`.

#### Select Calibration Profile Command (`extrusion_cali_sel`)
To bind a stored pressure advance (K-profile) to an AMS slot, both `"ams_id"` and `"tray_id"` must be transmitted. `"tray_id"` must be formatted as the absolute global tray ID. Furthermore, the `setting_id` field must be strictly omitted to prevent database mislinking.

```json
{
  "print": {
    "command": "extrusion_cali_sel",
    "ams_id": 0,
    "tray_id": 1,
    "cali_idx": 4,
    "filament_id": "GFA01",
    "nozzle_diameter": "0.4",
    "sequence_id": "40003"
  }
}
```

##### Polymorphic External Spool Parameter Rules
When configuring an External Spool, parameter formatting depends on whether the command is for configuration or calibration profile binding:

1.  **Filament Configuration (`ams_filament_setting`)**: Handled by the AMS MCU.
    *   *Single-Nozzle Platforms*: Requires `"ams_id": 255` and `"tray_id": 254`.
    *   *Dual-Nozzle IDEX*: Ext-L requires `ams_id: 254` / `tray_id: 0`. Ext-R requires `ams_id: 255` / `tray_id: 0`.
2.  **Calibration Profile Binding (`extrusion_cali_sel`)**: Handled by the Main Motion/Extruder MCU. Uses global tray rules.
    *   *Single-Nozzle Platforms*: Requires `"ams_id": 254` and `"tray_id": 254`.
    *   *Dual-Nozzle IDEX*: Ext-L requires `ams_id: 254` / `tray_id: 254`. Ext-R requires `ams_id: 255` / `tray_id: 255`.
        *   *Warning*: Failing to correctly target the `255` address for Ext-R on IDEX machines will mis-route the pressure advance profile to the left carriage (Ext-L) EEPROM, leaving the primary right carriage completely uncalibrated.

##### Virtual Slot Remapping on Single-Nozzle Platforms
Single-nozzle printers report `tray_now = 254` for the external spool on the telemetry channel. However, the slicer-facing configuration and `ams_mapping2` payload must always send `ams_id = 255` (VIRTUAL_TRAY_MAIN_ID) to identify the single external slot. Transmitting `254` during dispatch commands causes the printer's internal lookup to target physical AMS tray 0 instead of the external spool feed, producing a `"Failed to get AMS mapping table"` exception.

#### Filament Load & Unload Commands (ams_change_filament)
Filament loading and unloading sequences are triggered directly by publishing an `"ams_change_filament"` command payload to the request topic.

##### 1. Load Filament from standard AMS Slot
Instructs the printer to heat the hotend and feed filament from the designated physical AMS tray to the toolhead.
```json
{
  "print": {
    "command": "ams_change_filament",
    "ams_id": 0,
    "slot_id": 1,
    "target": 1,
    "curr_temp": -1,
    "tar_temp": -1,
    "sequence_id": "40005"
  }
}
```

##### 2. Load Filament from External Spool (Single-Nozzle Platforms)
Instructs single-nozzle printers to load from the virtual external spool.
```json
{
  "print": {
    "command": "ams_change_filament",
    "ams_id": 255,
    "slot_id": 254,
    "target": 254,
    "curr_temp": -1,
    "tar_temp": -1,
    "sequence_id": "40006"
  }
}
```

##### 3. Load Filament from External Spool (Ext-R on Dual-Nozzle IDEX)
Instructs dual-nozzle IDEX printers to heat the designated nozzle carriage and target the right-hand external spool (slot index `0` of the `255` virtual unit).
```json
{
  "print": {
    "command": "ams_change_filament",
    "ams_id": 255,
    "slot_id": 0,
    "target": 0,
    "curr_temp": 215,
    "tar_temp": 215,
    "sequence_id": "40007"
  }
}
```

##### 4. Unload Filament
Initiates the filament cutting and physical extraction sequence, returning the active filament back to its originating feeder channel. The `ams_id` must target the absolute identifier of the physical unit currently in use, or `255` if retracting from an external spool.
```json
{
  "print": {
    "command": "ams_change_filament",
    "ams_id": 0,
    "slot_id": 255,
    "target": 255,
    "curr_temp": 210,
    "tar_temp": 210,
    "sequence_id": "40008"
  }
}
```

---

### 5.4 Dry-Chamber Operations [REF-AMS-DRYER]

Supported AMS units (AMS 2 Pro and AMS-HT) feature built-in heaters and air-recirculation systems to perform in-enclosure filament drying. Operations are initiated by publishing an `ams_filament_drying` payload to the request topic.

```json
{
  "print": {
    "command": "ams_filament_drying",
    "ams_id": 128,
    "mode": 1,
    "dry_temp": 55,
    "dry_time": 480,
    "rotate_tray": true,
    "filament": "PA-CF",
    "sequence_id": "40004"
  }
}
```

#### Dryer State Machine & Safety Interlocks
*   **Heater Enablement**: The heater cannot be activated if any slot in the target unit reports a physical status code of `11` (Loaded). Filament must be fully retracted.
*   **`dry_sf_reason` Flags**: The hardware control board returns a bitmask error list if safety conditions are not met:
    *   `1`: Insufficient input voltage (unable to drive the heater element safely).
    *   `8`: Secondary power plug is disconnected (for dual-plug auxiliary units).
*   **Dry Duration Unit (`dry_time`)**: The `dry_time` parameter specifies the drying duration and **must be expressed in minutes** on the wire (e.g., an 8-hour cycle is serialized as `480`). Transmitting the value in hours (e.g., `8`) will result in the hardware executing an 8-minute cycle.

##### Telemetry Edge-Triggering and Omitted Fields Quirk
Drying completion is monitored by tracking the falling edge of the `dry_time` parameter (transitioning from a positive integer value representing remaining minutes down to `0`). However, standard status payloads emitted outside of an active drying cycle or during incremental tray updates frequently omit the `dry_time` key entirely. Telemetry parsers must evaluate transitions strictly when `dry_time` is explicitly present in the JSON payload; treating a missing key as a literal `0` value will trigger false "drying complete" events.
