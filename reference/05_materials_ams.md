# Chapter 5: Physical Material Expansion (AMS, AMS-HT & Spools)

---

### 5.1 Bus Telemetry & Bitmask Parsing [REF-AMS-DECODE]

The physical printer monitors modular material expansion units connected to its hardware expansion bus. Telemetry updates are broadcast within the `"print"` root envelope under the `"ams"` and `"ams_status"` keys.

#### Per-Model AMS Pool Composition (BUG-122)
The wire-decode boundary constants (`AMS_MAX_STANDARD_ID`, `AMS_HT_ID_MIN`/`AMS_HT_ID_MAX`) are protocol-wide, not model-dependent — every model uses the same bit addressing. What *is* model-dependent is how many units of each type a given machine physically supports, confirmed against `MODEL_MATRIX.csv`'s "AMS Unit Limits" row (user-supplied official Bambu documentation):

*   **Shared pool** (X1C, X1E, P1P, P1S, A1, A1 Mini, A2L): standard AMS and AMS-HT units draw from one combined pool of up to 4 units total.
*   **Independent pools** (H2C, H2D, H2D Pro, H2S, X2D): up to 4 standard AMS units *and* up to 8 AMS-HT units simultaneously, capped separately.
*   **Independent pools, narrower HT cap** (P2S): up to 4 standard AMS units *and* up to 4 AMS-HT units simultaneously (8 units / 20 slots total).

`ModelQuirks::ams_pool_composition()` exposes this per model; `ams::validate_ams_pool_composition()` checks a constructed `ams_mapping2` against it, rejecting configs no real hardware combination could serve (e.g. 4 standard + 8 AMS-HT units on a P2S, which only has independent pools of 4 and 4). A1/A1 Mini's "shared pool OR 1 AMS Lite, not combinable" exclusivity and A2L's "+1 AMS Lite simultaneously" additive capacity aren't modeled precisely; both are conservatively treated as the plain 4-unit shared pool. (This is a capacity-counting gap only — the AMS Lite *is* independently addressable, see "The A2L AMS Lite's Unit ID" below.)

#### Spool Presence Masking
The physical presence of loaded spools across standard expansion units is tracked via a hexadecimal bitmask string:
*   `tray_exist_bits`: A hexadecimal string representing which physical slots contain a spool.

##### Bitmask Evaluation Formulas
To determine if a physical spool is present in a specific standard slot, parse the hexadecimal string `tray_exist_bits` to an integer and evaluate the bitwise representation using standard shift logic. 

Standard physical AMS units (IDs `0` to `3`) each manage 4 physical slots. The global tray bit index is calculated as follows:

```text
shift_standard = (ams_id * 4) + tray_id
```

With the shift index determined, the existence of the physical spool in the slot is verified:

```text
slot_exists = (tray_exist_bits >> shift_standard) & 1
```

*   **AMS-HT Units (IDs 128-135)**: These single-slot, high-temperature dry-chamber units reside on a separate bus address but still occupy a dedicated range in `tray_exist_bits`, immediately following the standard units': `shift_ht = 16 + (ams_id - 128) + slot_id` (BUG-114; confirmed against BambuStudio's `DevAms::GetTrayId` N3S branch, `DevFilaSystem.cpp:833`). Note the standard-unit ID cap above is `0` to `3` (BUG-125), not `0` to `7` — the base offset `16` for AMS-HT only holds if standard units never reach bits 16+.

##### Unit ID of an AMS Lite Attached to an A2L

**"AMS Lite" is the unit; "A2L" is the printer.** The pairing is what matters here, because the same physical AMS Lite reports a different id depending on which printer it is plugged into. On an A1 / A1 mini it is the machine's *only* possible AMS (see the pool table above) and takes id `0`. An A2L can run it alongside up to four shared-pool units already occupying ids `0`-`3`, so there it reports physical unit **id 16**, outside every other range (standard `0`-`3`, AMS-HT `128`-`135`, external `254`/`255`).

BambuStudio encodes this pairing as a distinct *unit type* rather than a distinct id: `AMS_LITE_MIXED = 5`, commented "AMS-Lite for N9" (`DeviceCore/DevDefs.h:61`), N9 being the A2L's dev token, read from the unit's own `info` type nibble. Its tray-id branches for that type ignore `ams_id` entirely, so it never reads the 16 at all.

The firmware is internally inconsistent about this unit, so no single id works everywhere:

| Field | What the A2L uses |
|---|---|
| `tray_exist_bits` | bit base **24** — the position for id 6 (`6*4`), *not* id 16 (which would be bit 64) |
| `tray_now` | a **local** slot `0`-`3`, not a global id |
| `ams_mapping2`, per-unit commands | the **physical** id `16` |
| flat `ams_mapping` | the **local** slot `0`-`3` — not a global channel value at all |

Note the last row: the flat `ams_mapping` array is *not* uniformly "global channel ids". AMS-HT puts its unit id there (`128`-`135`), a regular AMS puts `ams_id*4 + slot`, and the AMS Lite puts a bare local slot. The encoding is per-unit-type.

The resolution is to normalize `16 -> 6` at the telemetry ingest boundary, so global tray ids land at `24`-`27` (colliding with nothing: regular `0`-`15`, AMS-HT `16`-`23` in the bitmask, external `254`/`255`), and to translate back to the physical form only on the outbound wire.

**Verification source:** bambuddy's `a2l_lite_wire_ids()` / `normalize_am_unit_id()` (`bambu_mqtt.py`) marks both of its wire encodings CONFIRMED against the firmware's own mapping — a captured flat `[1]` paired with `ams_mapping2 {"ams_id": 16, "slot_id": 1}`. BambuStudio corroborates the bit-base-24 half independently: `AMS_LITE_MIXED_TRAY_INDEX_OFFSET` is `24` (`DeviceCore/DevDefs.h:93`), applied as `24 + slot_id` in `DevAms::GetTrayId` (`DevFilaSystem.cpp:262-263`), `DevMappingUtil::ams_filament_mapping`, and `DevMapping.cpp:102-104`.

**Not confirmed:** the *global* tray value some commands want (load `target`, `extrusion_cali` `tray_id`). bambuddy extrapolates it as `16*4 + slot` = `64`-`67` and flags it as its single unverified encoding. bambino does not implement that path; a BambuStudio-to-A2L capture of a load or calibration command would settle it.

##### The Printer-Shutdown Telemetry Exception
During printer shutdown routines, the firmware emits a final status update where `tray_exist_bits` evaluates to `0` and the `power_on_flag` boolean is set to `false`. To prevent telemetry parsers from falsely interpreting this final update as a physical spool-removal event, updates where `tray_exist_bits = 0` must be ignored strictly when `power_on_flag` is `false`. Conversely, if `power_on_flag` is `false` but `tray_exist_bits` is non-zero, this represents a valid idle-printer state and changes must be processed normally.

#### Over-the-Wire Slot State Mappings
The physical printer's AMS controller represents spool presence and active routing status using native integer codes in the `"state"` parameter of each tray object.

| Native Integer Code | Protocol State Meaning | Physical Hardware Condition |
| :--- | :--- | :--- |
| **`11`** | Loaded / Active | Spool detected; filament has successfully fed past the hub multiplexer and is routed to the Active Toolhead. |
| **`10`** | Spool Present | Spool detected in the tray and loaded into the local feeder drive, but filament is currently retracted (unloaded from toolhead). |
| **`9`** / **`0`** | Empty Slot | No spool is detected in the tray, or the slot configuration has been cleared. **Does not hold for AMS-HT** — see the AMS-HT exception below. |
| *(Key Absent)* | Empty Slot | **Firmware Exception**: On some firmwares (e.g., P1S, A1 Mini), physically removing a spool omits the `"state"` key entirely (yielding `{id: N}`). Absent state parameters default to `9`. |

##### Symmetrical Absent-Key Empty Slot Signalling (P1S & A1 Mini)
On the P1 and A1 hardware series, when a physical AMS slot is completely empty, the printer emits an extremely truncated JSON object for that tray (e.g., only `{"id": N}`). All other keys (such as `state`, `tray_type`, etc.) are completely omitted. Evaluators must interpret the absence of these descriptive keys as a physical empty-slot indicator, defaulting the tray's state to `9` (Empty).

##### Incremental Telemetry Update Slot Cleansing Rules
When the bitwise presence check indicates a spool has been removed (or when the `"state"` transitions to a non-loaded code like `9`), all stale material telemetry fields (including `tray_type`, `tray_color`, `tray_info_idx`, `tag_uid`, `tray_uuid`, and `remain`) must be explicitly cleared or nullified by the parser. This is required because the printer's incremental telemetry updates often omit configuration keys for inactive slots, causing stale material attributes to persist indefinitely in standard parsers.

Additionally, some models (such as `H2D`) only emit `{id, state}` in incremental updates when a slot is not fully loaded. A transition to state `9` (empty) or `10` (present but retracted), or receiving an empty string for `tray_type`, must be treated as an explicit clearing signal. Without this active sanitization, stale material parameters from previously loaded spools will persist in the state representation.

##### Absent `state` Field
The table above assumes `state` is always present. It is not: some firmware sends a complete tray payload (`tray_info_idx`, `tray_type`, `tray_color`, `remain`) with **no `state` key at all**. An omitted field is not a report of emptiness, and treating it as absent-equivalent scrubs a loaded spool's material data.

When `state` was never reported for a tray, fall back to the filament metadata: a non-empty `tray_info_idx`, or a `tray_type` that is neither empty nor `"Empty"`, means a spool is loaded. This is distinct from the AMS-HT exception below — it is not model-scoped, and it triggers on the field being *absent* rather than on a particular code.

**Verification source:** pybambu tracks a `_state_reported` flag and resolves through `_has_filament_metadata` when `state` was never reported (`models.py:3517-3538`, ha-bambulab PR #2105), using exactly the `tray_info_idx` / `tray_type` test above. `src/ams/parser.rs::clean_stale_tray_data` implements the same gate.

##### AMS-HT State Exception (IDs 128-135)
An AMS-HT is a single-tray high-temperature dry box, not a 4-slot multiplexer: it does not feed filament into a shared buffer, so it has no distinct "loaded past the hub" condition to report as `11`. On a partial power-on frame it reports its **loaded** tray as `state: 9` — the opposite of the standard-AMS meaning tabulated above. Applying the generic `state ∈ {9, 10} → empty` rule to an HT unit therefore wipes a physically present spool on every power-on.

Parsers must not treat state `9` or state `10` alone as a clearing signal when `128 <= ams_id <= 135`. A genuine HT spool removal still clears via the explicit empty-`tray_type` signal and via `tray_exist_bits`, so nothing is lost by the carve-out. State `0` (power-off) is **not** carved out: it is a shutdown artifact covered by the `power_on_flag` rule above, and no HT unit has been observed reporting `0` while loaded — so code `0` is deliberately handled differently from `9` and `10` here despite all three being grouped in the table.

**Verification source:** Bambuddy `bambu_mqtt.py`'s incremental-merge handler skips the state heuristic for `ams_id >= 128`, citing their issue #2594 — a live H2D Pro observation where an HT spool was wiped on every power-on. This independently corroborates the evidence behind bambino issue #27 (commit `3ed570f`), which introduced `clean_stale_tray_data`'s `is_ht` gate without recording the exception here. Bambuddy's carve-out is broader than bambino's original (they skip the heuristic for *any* non-`11` state on HT, treating the HT state field as firmware-variant and unreliable); bambino issue #181 extended the `9` carve-out to `10` on that same evidence, since both states are "spool-present" readings that must not wipe a live HT spool.

**Verification source (BUG-012):** confirmed against two independent reverse-engineering projects, not just this doc's original wording. `pybambu`'s `AMSTrayStateFlags` bitmask model disagreed (treated state 9 as "present but unknown"), but `Bambuddy`'s `bambu_mqtt.py` (`apply_tray_exist_bits`, incremental-merge handler citing issue #784) and `main.py`'s `on_ams_change` (`loaded = cur_state == 11 or (cur_state not in (9, 10) and cur_type.strip())`, citing issue #1322, cross-tested against H2D/A1 Mini/P1S firmware) both treat `state ∈ {9, 10}` as the firmware's explicit "not loaded" signals — matching this doc, not pybambu. `src/ams/parser.rs::clean_stale_tray_data` now clears on both.

#### Multi-AMS Local Index Resolution (`tray_now`)
When multiple standard AMS units or virtual slots are connected, the printer's status stream may report only the local slot ID (0-3) in `tray_now` rather than a global ID.

*   **IDEX / Dual-Nozzle Printers (H2D series)**: Reports only slot numbers `0-3` representing the active tray position on the active extruder's linked AMS. **Preferred resolution (BUG-124)**: decode `device.extruder.info[active].snow` directly — low 8 bits = slot_id, next 8 bits = ams_id (see `ExtruderInfo::current_ams_slot()`) — confirmed as BambuStudio's own preferred method (`DevExterSystem::ParseV2_0`), no extruder-map inversion needed. The `active_extruder` + `ams_extruder_map` inversion described below is a fallback only; `ams_extruder_map`'s own construction from wire data is unconfirmed in this crate.
*   **Single-Nozzle Printers (P2S series)**: Multi-AMS configurations on single-nozzle printers may also report local slot indices in `tray_now`. State trackers must evaluate the MQTT `mapping` array field (refer to Section 5.3) to match the local slot position to the active physical AMS unit.

#### AMS Unit Info Bitmask (`info` Field)
Each AMS unit object in the `print.ams.ams[]` array may include an `"info"` field — a hex-encoded bitmask string (e.g. `"11002103"`). Parse via `u64::from_str_radix(s, 16)`. The bit layout encodes unit metadata and IDEX routing:

| Bit Range | Mask | Field | Values |
| :--- | :--- | :--- | :--- |
| **0–3** | `0xF` | AMS unit type | Which accessory is attached — see the table below. `3` = AMS 2 Pro, **not** AMS Lite (`2`) |
| **4–7** | `0xF0` | Dry status | Drying cycle state |
| **8–11** | `0xF00` | Extruder assignment | `0` = right/main, `1` = left/deputy, `0xE` = uninitialized |
| **22–23** | `0xC00000` | Dry sub-status | Drying sub-state detail |
| **18–19** | `0xC0000` | Dry fan 1 status | Drying fan 1 state (BUG-120; confirmed against BambuStudio's `DevFilaSystem.cpp:696` and independently by `bambu-printer-manager`'s `bambutools.py:685`) |
| **20–21** | `0x300000` | Dry fan 2 status | Drying fan 2 state (BUG-120; `DevFilaSystem.cpp:697`, `bambutools.py:686`) |
| **24–27** | `0xF000000` | `bind_switch_in` | Filament Track Switch inlet this unit feeds. `0` = inlet In-B, `1` = inlet In-A, any other value = not bound |
| **30–31** | `0xC0000000` | Remain-estimate version | Which filament-remaining estimation algorithm the unit reports (`DevAms::RemainEstimateVersion`; `0` = Legacy) |

##### Unit-Type Nibble (bits 0–3)

Bits 0–3 identify which physical accessory is plugged in. BambuStudio casts the nibble straight to its `DevAmsType` enum (`DevFilaSystem.cpp:598`, `type_id = (DevAmsType)DevUtil::get_flag_bits(info, 0, 4)`), enumerated at `DevDefs.h:54-62`:

| Value | Unit | Slots | Dries |
| :--- | :--- | :--- | :--- |
| `0` | External spool / dummy | n/a | no |
| `1` | Original 4-slot AMS | 4 | **no** |
| `2` | AMS Lite (A1 series) | 4 | no |
| `3` | AMS 2 Pro (BambuStudio `N3F`) | 4 | **yes**, 45–65 °C |
| `4` | AMS-HT (BambuStudio `N3S`) | 1 | **yes**, 45–85 °C |
| `5` | AMS Lite for N9 (`AMS_LITE_MIXED`) | varies | no |

**This is the only field that answers "can this unit dry?" — `ams_id` cannot.** Addresses `0..=3` are shared by the original AMS, the AMS Lite and the AMS 2 Pro, and only the last has a heater. A drying command addressed by range rather than unit type reaches heaterless hardware, which acks nothing and leaves `dry_status` at `0`; BambuStudio gates on the type (`Widgets/AMSItem.hpp:255`, `support_drying() { return ams_type == N3S || ams_type == N3F; }`) and so does bambuddy, by module-name prefix (`print_scheduler.py:3976`, `if module_type not in ("n3f", "n3s"): skip`). Exposed here as `AmsUnitModel` (`src/types/telemetry/ams.rs`) with `supports_drying()` and `dry_temp_range()`.

Note that "standard" elsewhere in this chapter — the "Shared pool" bullet in §5.1 above especially — is **pool accounting only** and carries no capability implication. The original AMS and the AMS 2 Pro are both 4-slot units counting against the same pool while differing on whether they have a heater at all.

##### Per-Unit Humidity (`humidity` and `humidity_raw`)

Each unit object in `print.ams.ams[]` carries up to two humidity readings, parsed by BambuStudio at `DevFilaSystem.cpp:677-692`:

*   **`humidity`** — a coarse level `1..=5`, present on every AMS including the original. **`1` is the wettest and `5` the driest** — the direction is inverted relative to what "higher is worse" intuition suggests, and a consumer that reads it as percentage-like gets the meaning backwards. BambuStudio's own level-to-icon mapping proves the direction (`AMSDryControl.cpp:22-38`: `humidity_percent <= 20` maps to level `5`, `> 80` to level `1`), and `DevFilaSystem.h:268` defaults `m_humidity_level = 5` for a unit with no reading. ha-bambulab inverts it for display (`models.py:747-748`, `6 - humidity_index`) with a comment saying the same.
*   **`humidity_raw`** — integer percent, stored separately as BambuStudio's `m_humidity_percent`. Present on the units that also report chamber `temp`, and additionally firmware-gated per printer: ha-bambulab's `Features.AMS_HUMIDITY` requires A1 ≥ 01.06.10.33, P1 ≥ 01.07.50.18, X1 ≥ 01.08.50.18, and reports X1E as unsupported (`pybambu/models.py:275-284`).

##### `bind_switch_in` is four bits, not two (BUG-136)
This field was previously documented here and in two places in `src/` as occupying bits **24–25**. That was wrong: BambuStudio reads `DevUtil::get_flag_bits(info, 24, 4)`, whose implementation is `(value >> start) & ((1 << count) - 1)` — four bits at offset 24, i.e. bits 24–27, mask `0xF000000`.

The value semantics confirm the width independently. Upstream distinguishes `0` (In-B) from `1` (In-A) and treats every other value as "not bound", so a 2-bit read would alias values 4–15 down into 0–3 and silently misreport a binding as a valid inlet.

The field is only meaningful when the extruder-assignment field (bits 8–11) reads `0xE` *and* a Filament Track Switch is installed — that combination is what "not wired to a fixed extruder, routed through the switch instead" looks like on the wire. With no FTS installed, `0xE` simply means uninitialized and this field carries nothing.

**Verification source:** BambuStudio `DevFilaSystem.cpp:598-609` and `DevUtil.cpp:7-14`, read directly; corroborated by bambuddy's independent parser (`c5e00558`, `7a42e0a7`), which uses the same 4-bit extraction. The adjacent dry-sub-status claim (bits 22–23) is unaffected — only the parenthetical about `bind_switch_in` was wrong.

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
*   **`ams/<id>`**: Original AMS unit (e.g., standard CoreXY multi-material systems). The trailing number `<id>` corresponds to the physical `ams_id` (`0` to `3`).
*   **`n3f/<id>`**: AMS 2 Pro unit. The trailing number `<id>` corresponds to the physical `ams_id` (`0` to `3`).
*   **`n3s/<id>`**: AMS-HT dry-chamber unit. The trailing number `<id>` represents the physical single-slot ID, **conventionally** starting at `128` (e.g., `n3s/128`).
*   **`ams_f1/<id>`**: A fourth AMS unit type BambuStudio accepts alongside the three above. **What physical product this designates is not confirmed here** — it is referenced in upstream's AMS-settings UI in a branch alongside a "lite" firmware selection and an `f1` printer AMS type, which suggests a lite/basic variant, but that is inference from surrounding code rather than an established mapping. Treat the prefix as recognized and its product identity as open.

Note the `<id>` ranges above are conventions of each unit type, **not rules a parser should enforce.** BambuStudio's `MachineObject::get_ams_version()` splits on `/`, accepts the four types verbatim, and parses whatever integer follows — it previously special-cased a hardcoded 128+ offset for `n3s` and that offset was removed. A parser that rejects an out-of-range `<id>`, or that infers unit type from the number instead of the prefix, will disagree with upstream.

**Verification source:** BambuStudio `DeviceManager.cpp:941` (the four-type check) and `AMSSetting.cpp:365-372` / `UpgradePanel.cpp:890` for the `ams_f1` usage context, read directly. bambino needs no code change for this — `VersionModule.name` (`src/types/version.rs`) stores the raw string with no prefix dispatch, so an `ams_f1/0` module already deserializes; it simply is not recognizable as an AMS unit by anything reading the list.

---

### 5.2 Spool Presets, Colors & RFID Serialization [REF-AMS-SP_CFG]

For spools equipped with proprietary Bambu Lab RFID tags, the printer automatically scans and populates material characteristics. Generic spools must be configured manually over MQTTS.

#### RFID Tag Serialization
*   `tag_uid`: Unique 16-character hexadecimal string read from the physical RFID tag.
*   `tray_uuid`: Unique 32-character hexadecimal string representing the globally unique ID of the filament spool.

#### Preset Identifiers (`tray_info_idx`)
These are **two separate wire fields**, and conflating them is a real failure mode:

*   `"tray_info_idx"` carries the **short-format** preset ID (e.g. `"GFA01"` for Bambu PLA Matte, `"GFL05"`).
*   `"setting_id"` carries the **full** preset identifier — the long form, such as `"GFSL05_07"`, or the unique randomized id the slicer assigns a custom user preset (`"PF"` followed by 17 numeric digits, e.g. `"PF12345678901234567"`). It is optional and may be omitted entirely.

**A long id does not belong in `tray_info_idx`.** An A1 sent a 19-character `PFUS…` cloud id in that field and stored only its first 8 characters, uppercased, while acking the command as `"success"`; the slot then resolves to Generic and drops out of the calibration table, which is keyed on the same field (bambuddy issue #3003). This is the printer reacting to the wrong field, not a general width limit worth working around — put the long id in `setting_id` and the short code in `tray_info_idx`.

**Verification source:** BambuStudio's `command_ams_filament_settings` (`DeviceManager.cpp:1723-1724`) assigns the two keys from separate arguments — `j["print"]["tray_info_idx"] = filament_id;` and `j["print"]["setting_id"] = setting_id;`. bambuddy's `ams_set_filament_setting` agrees independently, documenting its `tray_info_idx` parameter as "Filament ID short format (e.g. `GFL05`)" against a distinct optional `setting_id` it includes only when non-empty.

#### Color Encoding
Color parameters (`"tray_color"` and `"cols"`) are formatted as 8-character hexadecimal strings representing RRGGBBAA. Empty or unconfigured slots transmit `"00000000"` (zeroed alpha channel), whereas configured filaments use `"RRGGBBAA"` with `"FF"` alpha (e.g., `"FF0000FF"`).

**Outbound hex digits must be uppercase.** The firmware parses a lowercase hex letter in an outbound `tray_color` as `0` and stores the corrupted value. The failure is silent — the `ams_filament_setting` ack echoes the value that was sent and reports `result: "success"`; only the next AMS push status reveals it:

```
sent 09ff00ff -> stored 09000000
sent ff5100ff -> stored 00510000
sent 090000FF -> stored 090000FF   (uppercase survives intact)
```

Measured on a P1S running firmware `01.10.00.00` (bambuddy issue #2987). Normalize at the point the command is assembled, stripping a leading `#` and leaving an empty string empty — bambino does this in `AmsFilamentSettingRequest::new`. Case is **not** to be normalized in `tray_type` or `tray_sub_brands`, where it is meaningful.

---

### 5.3 AMS Slicer Mappings & Filament Changes [REF-AMS-MAP]

Slicers coordinate the mapping between project-defined filaments and the physical hardware channels using two symmetrical array structures inside the `print.project_file` command payload.

#### Flat `ams_mapping` Array
The `"ams_mapping"` parameter is a flat, 1-to-1, forward-mapped JSON array of integers that correlates the filament slots defined in the sliced print job to the physical hardware channels of the printer.

##### Array Mapping Mechanics
*   **Array Length via Maximum ID**: The length of the `ams_mapping` array is determined by the *highest filament ID index* present in the sliced project metadata (`slice_info.config`), not the total count of unique filaments. If a project utilizes filament ID 1 and filament ID 4, the array must be sized to 4.
*   **Forward 0-Indexed Mapping & Padding**: The array positions map sequentially from left to right. Index `i` corresponds directly to the 0-indexed filament slot `i` defined in the sliced project file. Intermediate unused filament IDs must be explicitly padded with the `-1` (unmapped) sentinel.

##### Hardware Channel Identifiers
The integer values within the flat `ams_mapping` array represent absolute physical hardware channels:
*   **`0` to `15`**: Standard AMS channels. Calculated via `(ams_id * 4) + slot_id` (`ams_id` 0-3, `slot_id` 0-3).
*   **`128` to `135`**: Physical single-slot high-temperature AMS-HT units. Global channel ID equals the unit's bus ID (`ams_id`).
*   **`0` to `3` (an A2L-attached AMS Lite only)**: a bare **local** slot index, not a global channel. This unit is the exception to "absolute physical hardware channels" above — see "Unit ID of an AMS Lite Attached to an A2L" in §5.1 for the full per-field encoding table and its verification sources.

    **Verification source:** BambuStudio's `DevMappingUtil::ams_filament_mapping` (`DevMapping.cpp:175`) computes the N3S tray index as `ams_id + tray_id`, yielding 128-135, and that value flows through `FilamentInfo::tray_id` into `mapping_v0_json` — the chain that actually builds this flat array. Corroborated independently by Bambuddy, whose `print_scheduler.py::_global_tray_id` returns `ams_id if ams_id >= 128 else ams_id * 4 + tray_id` and whose `bambu_mqtt.py` puts that `tray_id` straight into `command["print"]["ams_mapping"]`.

    Do not confuse this with the **16-23** range: BambuStudio carries three different N3S index formulas for three different consumers, and `DevAms::GetTrayId` (`DevFilaSystem.cpp:248`) yields `16 + (ams_id - 128) + slot_id` — but that value is only ever used as a `tray_exist_bits` bit index (see §5.1), never as a flat `ams_mapping` channel. `DevFilaSystem::GetTrayIndexMap` uses `tray_index = ams_id` (128-135) for calibration `tray_id` and AMS settings. An upstream pybambu comment asserting 16-23 for the flat slot index traces to a **cloud** `amsDetailMapping` observation, a structure the LAN protocol never carries.
*   **`-1`**: Omit/Unmapped. Mandatory marker for any unused project filament slot or any slot routed to an **External Spool** (non-bus tray).

##### External Spool Flat-Mapping Restrictions
The flat `ams_mapping` array cannot accept absolute external spool IDs (`254` or `255`). To print from an external spool, the flat array must assign `-1` (unmapped) to the respective index, and let the structured `ams_mapping2` array handle specific external routing. Passing virtual external spool IDs directly into the flat array triggers a `"Failed to get AMS mapping table"` exception (such as error `0700_8012` or `07FF_8012`) on the motion board.

#### Structured `ams_mapping2` Array
The `"ams_mapping2"` parameter is a JSON array of structured objects that maintains a direct 1-to-1 index pairing with `"ams_mapping"`. It defines detailed unit and slot routing:

*   **Standard AMS Slot**: `{"ams_id": ams_id, "slot_id": slot_id}` (where `ams_id` is `0` to `3`, and `slot_id` is `0` to `3`).
*   **AMS-HT Slot**: `{"ams_id": ams_id, "slot_id": 0}` (where `ams_id` is `128` to `135`).
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
    *   *Dual-Nozzle IDEX*: Ext-L requires `ams_id: 254` / `tray_id: 254`. Ext-R requires `ams_id: 255` / `tray_id: 254` (never `0` — BUG-117 / BambuStudio DeviceManager.cpp:1667-1693).
2.  **Calibration Profile Binding (`extrusion_cali_sel`)**: Handled by the Main Motion/Extruder MCU. Uses global tray rules.
    *   *Single-Nozzle Platforms*: Requires `"ams_id": 254` and `"tray_id": 254`.
    *   *Dual-Nozzle IDEX*: Ext-L requires `ams_id: 254` / `tray_id: 254`. Ext-R requires `ams_id: 255` / `tray_id: 255`.
        *   *Warning*: Failing to correctly target the `255` address for Ext-R on IDEX machines will mis-route the pressure advance profile to the left carriage (Ext-L) EEPROM, leaving the primary right carriage completely uncalibrated.

##### Virtual Slot Remapping on Single-Nozzle Platforms
Single-nozzle printers report `tray_now = 254` for the external spool on the telemetry channel. However, the slicer-facing configuration and `ams_mapping2` payload must always send `ams_id = 255` (VIRTUAL_TRAY_MAIN_ID) to identify the single external slot. Transmitting `254` during dispatch commands causes the printer's internal lookup to target physical AMS tray 0 instead of the external spool feed, producing a `"Failed to get AMS mapping table"` exception.

#### Filament Load & Unload Commands (ams_change_filament)
Filament loading and unloading sequences are triggered directly by publishing an `"ams_change_filament"` command payload to the request topic.

**`target` derivation (BUG-116)**, confirmed against BambuStudio's `command_ams_change_filament` (`DeviceManager.cpp:1602-1638`): `255` on unload; the `ams_id` itself for any AMS-HT/external-spool unit (`ams_id >= 16`, covers `128`-`135` and `254`/`255`); otherwise the flat global tray ID `(ams_id * 4) + slot_id` for a standard unit. `target` only coincidentally equals `slot_id` when `ams_id == 0` (example 1 below) — the earlier version of this doc generalized that coincidence into a wrong rule, and examples 2/3's `target` values below were wrong for the same reason (both should be `255`, the external-spool `ams_id`, not `slot_id`).

**`extruder_id` (optional): `0` = right/main, `1` = left/deputy.** BambuStudio's `DeviceManager::command_ams_change_filament` takes it as an optional field and omits it unless a **Filament Track Switch** is fitted, and the omission is correct on any printer without one: each AMS is wired to exactly one hotend, and the firmware derives the target from that binding.

With an FTS fitted the situation inverts. Every AMS reports its extruder as "not fixed" (`0xE` in the `info` bitfield — see §5.1's `filament_switch_inlet` notes), each unit is plumbed into one of the switch's two inlets, and from there it can reach *either* hotend. The firmware then has nothing to derive from, and a load or unload command naming neither extruder is **discarded in silence** — on an H2C this presents as load and unload simply doing nothing, with no error and no HMS entry.

Omit the key entirely rather than sending a default when the target hotend is unknown; a wrong explicit value feeds the wrong nozzle. Consider refusing the operation up front when an AMS is unassigned to an inlet, mirroring BambuStudio's `DevFilaSwitch::IsReady`.

**Verification source:** bambuddy commit `9500c046` ("Ask which nozzle to feed when a Filament Track Switch is fitted"), whose `ams_load_filament` attaches `extruder_id` only when the caller supplies one, verified on an H2C-1 with AMS-A slot 3 across all four load/unload combinations.

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
    "target": 255,
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
    "target": 255,
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

**BUG-118**: the field set and shapes below were rewritten to match the real wire protocol — confirmed against BambuStudio's `DevFilaSystem::CtrlAmsStartDryingHour`/`CtrlAmsStopDrying` (`DevFilaSystemCtrl.cpp:18-53`, the sole outbound `ams_filament_drying` constructor in the tree) and independently corroborated by bambuddy's `send_drying_command` (`bambu_mqtt.py:4141-4171`). The earlier version of this doc used `dry_temp`/`dry_time` (minutes) and omitted `humidity`/`cooling_temp`/`close_power_conflict` entirely — none of that matched either source.

**P1-connected AMS is remote-control-blind for drying**: on a P1P/P1S host, the printer's firmware accepts `ams_filament_drying` and acks `result: success`, then silently discards it — no heater/fan activation, `dry_status` telemetry stays `0`. This is documented in Bambu's own P1 manual ("P1S connected AMS drying functions may only be controlled from the P1S screen") and confirmed independently by bambuddy (`fix(drying)`, #2533) and by direct hardware testing against this crate's own drying builder (`PrinterClient::dry(..).send()`)/CLI (`ams dry`) against a P1S — command published successfully, `dry_status` never left `0`. `ModelQuirks::supports_ams_remote_drying()` reports this; `DryingCycle::send()` returns `Error::ModelMismatch` on P1 rather than dispatching a command the firmware will accept-then-drop.

```json
{
  "print": {
    "command": "ams_filament_drying",
    "ams_id": 128,
    "mode": 1,
    "filament": "PA-CF",
    "temp": 55,
    "duration": 8,
    "humidity": 0,
    "rotate_tray": true,
    "cooling_temp": 20,
    "close_power_conflict": false,
    "sequence_id": "40004"
  }
}
```

#### Dryer State Machine & Safety Interlocks
*   **Heater Enablement**: The heater cannot be activated if any slot in the target unit reports a physical status code of `11` (Loaded). Filament must be fully retracted.
*   **`dry_sf_reason` Codes**: an array of independent integer reason codes explaining why a drying cycle will not or did not start. **Not a bitmask** — this doc previously described it as one and listed only codes `1` and `8`, whose reading as bit positions was a coincidence. It is an enumerated code list, which is why this crate parses it as `Option<Vec<i32>>` (`src/types/telemetry/ams.rs`, decoded by `AmsUnit::dry_block_reasons`). BambuStudio's `DevAms::CannotDryReason` (`DevFilaSystem.h:167-179`) is the authoritative enumeration and has ten members; bambuddy's `DRY_SF_REASON_MESSAGES` (`backend/app/services/drying_preflight.py`) agrees on `0`-`8` and omits `10`. `9` is unassigned in both:

    | Code | Meaning | Clears |
    | :--- | :--- | :--- |
    | `0` | Printer is busy | on its own |
    | `1` | Insufficient power: too many AMS drying, or external PSU required | **user** (power) |
    | `2` | AMS is busy | on its own |
    | `3` | Filament is at the AMS outlet, retract it first | **user** (retract) |
    | `4` | AMS is already starting a drying cycle | on its own |
    | `5` | Not supported in 2D mode | on its own |
    | `6` | AMS is already drying | on its own |
    | `7` | AMS firmware is upgrading | on its own |
    | `8` | Plug in the external AMS power adapter to start drying | **user** (power) |
    | `10` | Filament is at the AMS outlet and must be unloaded manually (BambuStudio `FilamentAtAmsOutletManualUnload`) | **user** (manual unload) |

    The "Clears" column is bambuddy's own split (`POWER_REASON_CODES = {1, 8}`, `RETRACT_REASON_CODE = 3`, everything else transient), extended to `10`, which BambuStudio words as requiring a manual unload (`AMSDryControl.cpp:1355-1357`). The split is the part that matters to a consumer: it decides between "retry in a moment" and "surface a message and stop". bambuddy also picks a single `primary_reason_code` to display when the firmware sets several at once.
*   **Drying telemetry is capability-gated upstream.** BambuStudio reads `dry_status`, both dry-fan statuses, `dry_sub_status` and the whole `dry_setting` block only when the printer's own `is_support_remote_dry` bit is set (`DevFilaSystem.cpp:697`), and ha-bambulab restricts `dry_setting` to the P2 series and H2C (`Features.AMS_DRYING_SETTINGS`, `pybambu/models.py:297-301`). This crate parses them unconditionally as `Option`, which is the right shape — but a consumer must not expect them to be present on an X1 or P1.
*   **Remote-drying firmware thresholds.** Whether `ams_filament_drying` is honored over MQTT depends on the host printer and its firmware (`ModelQuirks::ams_remote_drying_support`). Drying *while a print runs* is a separate, narrower capability with its own list (`ModelQuirks::ams_drying_while_printing_support`); the firmware lowers the drying temperature below the printed filament's softening point during a print (the vendor guide's examples: PETG dried while printing PLA clamps to 45 °C, ABS while printing PETG to 55 °C). This crate documents that clamp and does not reimplement it.

    | Model | Idle remote drying | Drying while printing | Source |
    | :--- | :--- | :--- | :--- |
    | H2D | `01.03.00.00` (2026-03-03) | `01.03.00.00` | H2D firmware release history; drying guide |
    | H2D Pro | `01.02.00.00` (2026-04-27) | `01.02.00.00` | H2D Pro firmware release history |
    | H2S | `01.02.00.00` (2026-03-31) | `01.02.00.00` | H2S firmware release history; drying guide |
    | H2C | `01.02.00.00` (2026-06-01) | `01.02.00.00` | H2C firmware release history |
    | P2S | `01.02.00.00` (2026-04-09) | `01.02.00.00` | P2S firmware release history; drying guide |
    | X2D | `01.01.00.00` (2026-04-14, earliest release) | `01.01.00.00` | X2D firmware release history; drying guide |
    | A2L | `01.01.00.00` (2026-06-01, earliest release) | `01.01.00.00` | A2L firmware release history; drying guide |
    | X1C | never | never | drying guide: "P1S/P1P/X1C/A1/A1mini are not supported yet"; X1 `01.09.00.00` is screen-only |
    | P1P / P1S | never | never | P1 `01.08.00.00` is screen-only; drying guide |
    | A1 / A1 Mini | never | never | drying guide |
    | X1E | not stated — allowed | not stated — denied | no entry in any X1E release |

    Firmware release histories live at `https://wiki.bambulab.com/en/<model>/manual/<model>-firmware-release-history`, with three exceptions: H2D Pro is `h2d-pro/manual/firmware-release-history`, and X1/X1C and X1E live under `x1/manual/` as `X1-X1C-firmware-release-history` and `X1E-firmware-release-history`. The drying guide is the wiki page *Filament drying guide for AMS 2 Pro and AMS HT*; its two minimum-firmware lists ("Drying Workflow on Bambu Studio" and "Introduction to Simultaneous Drying and Printing Function") omit H2C and H2D Pro, whose own release notes announce both features — the per-model notes are more specific and more recent.

    **Source order for a firmware threshold**, strongest first: (1) the model's firmware release history, (2) a Bambu feature wiki page with a minimum-firmware table, (3) BambuStudio release notes, (4) BambuStudio and bambuddy source agreeing, (5) a single upstream table. Numbers that failed this check, recorded so they are not reintroduced:

    - `01.02.30.00` (H2D) — BambuStudio 2.5.0's release-note minimum for drying *while printing*; absent from the H2D release history (`01.02.10.00` is followed by `01.03.00.00`). bambuddy's `_DRYING_MIN_FIRMWARE` filed it as the idle threshold.
    - `01.09.00.00` (X1/X1C) — the X1's AMS 2 Pro/HT support release, whose only drying line is "starting the filament drying operation from the printer's screen" — the same sentence P1 `01.08.00.00` carries. bambuddy's `_DRYING_MIN_FIRMWARE` read it as remote drying.
    - `01.11.02.00` (X1C) — bambuddy's `_DRY_WHILE_PRINTING_MIN_FIRMWARE`; that release has no drying entry and the drying guide names the X1C as unsupported.
    - `01.08.50.18` (X1) — ha-bambulab's `Features.AMS_DRYING`; a beta build absent from the public history. A `.50.` segment marks one.
    - `01.01.40.00` (H2S) — BambuStudio 2.5.3's notes; the H2S release history and the drying guide both say `01.02.00.00`.
*   **Dry Duration Unit (`duration`)**: The command's `duration` parameter specifies the drying duration and is expressed in **hours** (e.g., an 8-hour cycle is serialized as `8`) — distinct from the *telemetry* `dry_time` field described below, which counts down in minutes.

##### Telemetry Edge-Triggering and Omitted Fields Quirk
Drying completion is monitored by tracking the falling edge of the `dry_time` parameter (transitioning from a positive integer value representing remaining minutes down to `0`). However, standard status payloads emitted outside of an active drying cycle or during incremental tray updates frequently omit the `dry_time` key entirely. Telemetry parsers must evaluate transitions strictly when `dry_time` is explicitly present in the JSON payload; treating a missing key as a literal `0` value will trigger false "drying complete" events.
