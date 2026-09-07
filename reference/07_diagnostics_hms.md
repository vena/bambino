# Chapter 7: Diagnostic Mapping & Calibration Profiles

---

### 7.1 HMS Telemetry Decoding [REF-DIAG-HMS]

The physical printer communicates active hardware faults, operational warnings, and diagnostic status logs using two main pathways inside the `"print"` telemetry report envelope: the `"hms"` array of objects and the `"print_error"` 32-bit integer register.

#### 1. The `"print_error"` Register
The `"print_error"` field contains a single 32-bit packed integer representing the primary system execution error. To convert this integer into a Unified HMS Key, the following normalization is executed:

1. Read the 32-bit decimal integer (e.g., `83902476`).
2. Convert the integer to an 8-character uppercase hexadecimal string, padded with leading zeros (e.g., `"0500400C"`).
3. Split the string into two 4-character blocks separated by an underscore (e.g., `"0500_400C"`).

#### 2. The `"hms"` Telemetry Array
The `"hms"` array contains active system faults represented as objects with `"attr"` and `"code"` keys. On X2, H2, and P2 series models, each entry may also include optional timestamp fields:
*   `ts_boot`: Seconds since boot when the alert was raised (`u64`).
*   `ts_unix`: UTC timestamp string when the alert was raised (e.g. `"20260426002648"`, format `YYYYMMDDHHmmss`).

To convert the `attr` and `code` fields into a standard 16-character wiki troubleshooting key (`MMMM_MMMM_CCCC_CCCC`), apply the following binary unpacking steps:

```python
attr_high = (attr >> 16) & 0xFFFF
attr_low = attr & 0xFFFF
code_high = (code >> 16) & 0xFFFF
code_low = code & 0xFFFF

unified_hms_key = f"{attr_high:04X}_{attr_low:04X}_{code_high:04X}_{code_low:04X}"
```

*Example*: If `attr` = `50331904` and `code` = `65543`:
*   `attr_high` = `(50331904 >> 16) & 0xFFFF` = `768` -> `"0300"`
*   `attr_low`  = `50331904 & 0xFFFF` = `256` -> `"0100"`
*   `code_high` = `(65543 >> 16) & 0xFFFF` = `1` -> `"0001"`
*   `code_low`  = `65543 & 0xFFFF` = `7` -> `"0007"`
*   **Resulting Wiki Slug**: `0300_0100_0001_0007`

##### Wiki Slug Delimiter Domain Boundary
Troubleshooting routing paths on the official wiki are resolved strictly using underscore-delimited formats (`MMMM_MMMM_CCCC_CCCC`). Hyphenated formats are not resolved by the server.

#### 3. Community / Local 8-Character Short-Code Format
To facilitate local lookup of error descriptions without querying remote wiki services, and to match the raw error formats displayed on the physical machine's LCD screen (e.g. `[0500-400E]`), diagnostic parsers construct an abbreviated 8-character short-code:

*   **From the `print_error` Register**:
    The split 4-character blocks are concatenated directly with an underscore separator:
    ```text
    short_code = f"{module:04X}_{error:04X}"
    ```
    *(Example: `83902476` decimal -> `0x0500400C` hex -> `"0500_400C"`)*

*   **From the `hms` Telemetry Array**:
    The high-word (16 bits) of the `attr` parameter and the low-word (16 bits) of the `code` parameter are unpacked and joined with an underscore separator:
    ```text
    short_code = f"{(attr >> 16) & 0xFFFF:04X}_{code & 0xFFFF:04X}"
    ```
    *(Example: `attr = 50331904` (0x03000100) and `code = 65543` (0x00010007) -> `"0300_0007"`)*

#### Severity Scale & Module Identification
The severity level of the diagnostic alert is extracted from the high 16 bits of the `code`
parameter (BUG-108 — not `attr`; confirmed against BambuStudio's `parse_hms_info`,
`DevHMS.cpp:7-25`, identical in OrcaSlicer, and pybambu's `get_HMS_severity`):

```python
severity = (code >> 16) & 0xFFFF
```

*   **`1`**: Fatal Error (Immediate execution halt required).
*   **`2`**: Serious Alert (Requires user attention before resuming).
*   **`3`**: Warning (Non-blocking warning or environmental notice).
*   **`4`**: Information / Prompt (Routine user prompt or action confirmation).

The source hardware module is identified by the fourth byte of the `attr` parameter:

```python
module_id = (attr >> 24) & 0xFF
```

#### Real Hardware Faults vs. Non-Error Status Codes
The printer publishes both hardware failures and non-error state indications (such as axis homing progress) within the `print_error` and `hms` registers. For `print_error`, non-error state indications are represented by low-word values less than `0x4000` (16384 decimal); only codes with a low 16-bit value `>= 0x4000` represent actual faults. For `hms` entries specifically, the check must compare the **full 32-bit `code`** against `0x4000`, not just its low 16 bits (BUG-109 — confirmed against BambuStudio's bundled `resources/hms/hms_en_093.json` fault catalog: 4591/4592 cataloged genuine `hms[]` faults have `code_low < 0x4000`, so a low-word-only check misclassifies nearly every real fault as a non-fault status step).

##### User-Action Echoes
During user-initiated print cancellations, the firmware raises specific confirmation codes (such as `0300_400C` and `0500_400E`) to confirm cancellation has completed. These are status confirmations, not active faults, and must not be treated as actual system errors.

##### `0500_0500_0001_0007` — Control Commands Silently Refused

Firmware from roughly `01.08.03.00beta` / `01.08.05.00` onward can reject a control command whose authorization it cannot verify, and reports it as an HMS entry with `attr = 0x05000500`, `code = 0x00010007`.

The failure mode is nasty for a client, because **queries keep answering while control commands are dropped**. `get_version`, `extrusion_cali_get` and `pushall` all respond normally, so the connection looks healthy and the printer looks idle — while `project_file`, `gcode_line` and `ams_change_filament` are discarded in silence. No amount of waiting or re-uploading changes it, and a consumer polling telemetry sees nothing wrong.

**The 16-character form is load-bearing here.** The meaning lives in `attr`'s low half (`0500`) and `code`'s high half (`0001`):

```
16-char (MMMM_MMMM_CCCC_CCCC):  0500 0500 0001 0007   <- identifies the condition
 8-char (MMMM_CCCC):            0500      0007        <- matches nothing in any catalog
```

The 8-character LCD short code collapses it to `0500_0007`, which appears in no published HMS catalog. Anyone triaging from the short code alone will find nothing and conclude the entry is spurious. Use the 16-character key — `decode_hms_alert` (`src/diagnostics/hms.rs`) produces both, and `DecodedHmsAlert` exposes the 16-character form alongside the 8.

*(Verification source: bambuddy, `backend/app/services/bambu_mqtt.py:705` — the constant, with the attr/code split explained in the comment above it — and their scheduler's `_mqtt_commands_rejected`, which acts on it. Source issue: bambuddy #2732. **The firmware version boundary is theirs and has not been independently checked here**; record it as reported, not established.)*

---

### 7.2 Pressure Advance (K-Profile) Calibration [REF-DIAG-KPROF]

The printer's onboard EEPROM database houses user-configured Linear Advance (Pressure Advance) K-factor calibration profiles. These profiles are managed, queried, and loaded via direct MQTTS command schemas.

#### Query Calibration Profiles Database
To retrieve all stored profiles from the machine's database, publish the `"extrusion_cali_get"` command:

```json
{
  "print": {
    "command": "extrusion_cali_get",
    "filament_id": "",
    "nozzle_diameter": "0.4",
    "sequence_id": "50001"
  }
}
```

**A response is the complete table for exactly one nozzle diameter, and it echoes the *requested* diameter rather than reflecting installed hardware.** The bare request shape (`command` + `sequence_id` only) is accepted, but on a dual-diameter machine its reply covers whichever single diameter the firmware picks — a partial table that looks complete to the caller, with no way to ask for the rest. Query once per fitted diameter and merge. `filament_id` scopes the query to one preset; upstream sends an empty string for "all filaments".

Two consequences worth stating separately, both of which bambino already handles:

*   Match responses on `sequence_id`. The report topic is shared, so an unmatched read can pick up BambuStudio's response to its own query.
*   Do **not** feed the response's `nozzle_diameter` into live nozzle state. It is the echo of what was asked for, and folding it back in clobbers the real installed nozzle size (bambuddy #2663 — theirs typically left `0.8`, the last diameter probed).

*(Verification source: bambuddy issue #2854. Their client buckets responses per diameter, queries only the fitted diameters, and deliberately keeps the response out of nozzle state.)*

#### Resolving a Slot to a Profile

The two facts below are exactly the traps a consumer falls into, and neither is inferable from the payload shapes above. bambino does no slot-to-K resolution itself, so nothing in the crate depends on them — but this is where a consumer would look.

**1. `cali_idx` is not uniquely keyed per nozzle.** Both of these occur on real hardware:

*   Two profiles share a `cali_idx` and differ only by `extruder_id`. Measured on an H2C: index 16 = left, black PLA, K=0.018; index 15 = right, K=0.020.
*   One profile is what slots on *both* extruders point at. An X2D with one AMS 2 Pro per hotend filed every entry under a single extruder, so the second AMS's slots referenced the first's entries.

Upstream's resolution rule: prefer a profile filed under the slot's own `extruder_id`; if that extruder appears nowhere in the table, match on `cali_idx` alone and accept only when every candidate agrees on one `k_value`. BambuStudio is looser — `CalibUtils::get_pa_k_n_value_by_cali_idx` matches `cali_idx` and nothing else.

**2. `nozzle_id` encodes flow type, and is empty on some models.**

*   `HH00-0.4` = high flow, `HS00-0.4` = standard. A printer can hold both for one diameter — an H2D was observed with 102 high-flow entries against 6 standard ones — and the same filament reads a different K through each.
*   The **fitted** nozzle reports `HH01`, not `HH00`. Comparison must be on the first **two** characters; the trailing digits are a hardware variant that the calibration table normalizes to `00`.
*   An **X1C declares an empty `nozzle_id` on every profile** (probed live: all eight, against a four-digit `cali_idx` and a populated `setting_id`). A model-capability flag is therefore the wrong thing to gate on — handle the emptiness directly.

Note this is the *same* flow-code vocabulary as `NozzleInfo`'s `type` key on H2-generation printers, but `type` reports nozzle **material** on legacy printers — see §3's note on that key.

*(Verification sources: bambuddy issue #3044 and commit `e5a18bf5`.)*

#### Calibration Profiles Database Telemetry Schema (The Read Stream)
The printer returns the complete onboard profile list over the report topic (`device/{serial_number}/report`). Parsers must inspect the payload to extract the `"filaments"` array nested inside the query response envelope:

```json
{
  "print": {
    "command": "extrusion_cali_get",
    "sequence_id": "50001",
    "nozzle_diameter": "0.4",
    "filaments": [
      {
        "cali_idx": 4,
        "filament_id": "GFA01",
        "nozzle_diameter": "0.4",
        "nozzle_id": "HS00-0.4",
        "extruder_id": 0,
        "name": "My Custom PLA Matte",
        "k_value": "0.022000",
        "n_coef": "0.000000",
        "setting_id": "PF12345678901234567"
      }
    ]
  }
}
```

Single-nozzle firmware may omit the per-entry `"nozzle_diameter"` field inside each
`"filaments"` object shown above, setting it only once at the envelope level (as in the
example). Parsers must fall back to the envelope's `"nozzle_diameter"` when the per-entry
field is absent, rather than treating a filament entry without it as malformed.

#### Create or Edit a Calibration Profile
To save or overwrite a specific K-value profile slot, publish an `"extrusion_cali_set"` command containing a nested `"filaments"` array:

```json
{
  "print": {
    "command": "extrusion_cali_set",
    "filaments": [
      {
        "cali_idx": -1,
        "filament_id": "GFA01",
        "nozzle_diameter": "0.4",
        "nozzle_id": "HS00-0.4",
        "extruder_id": 0,
        "name": "My Custom PLA Matte",
        "k_value": "0.022000",
        "setting_id": "PF12345678901234567"
      }
    ],
    "sequence_id": "50002"
  }
}
```

##### Dual-Nozzle Multi-Profile Calibration
On IDEX platforms (such as the `H2D`), the `"filaments"` array inside `"extrusion_cali_set"` may carry multiple structured objects to commit calibration constants for both primary (Right - `extruder_id: 0`) and secondary (Left - `extruder_id: 1`) carriages in a single MQTTS write transaction.

##### Calibration Setting ID Boundary Rule
The `"setting_id"` parameter inside K-profile calibration payloads (`extrusion_cali_set` and `extrusion_cali_del`) must conform strictly to a 19-character numeric string format consisting of the `"PF"` header prefix followed by exactly 17 numeric digits (e.g., `"PF12345678901234567"`). Alphanumeric setting ID formats (such as `"PFUS9be9e18f81828a"`) are strictly reserved for slicer-side filament presets (`ams_filament_setting` / `tray_info_idx` mappings). Transmitting an alphanumeric setting ID inside K-profile operations will result in execution failure or local EEPROM table corruption.

#### Delete a Calibration Profile
Because single-carriage and dual-carriage (IDEX) models manage their EEPROM databases differently, deletions must be executed using separate, mutually exclusive command schemas:

##### Schema A: Standard Single-Nozzle Deletion (X1, P1, A1, P2S, H2S)
The database on single-nozzle platforms is globally keyed on `"setting_id"`. The deletion schema must mirror the nested `"filaments"` array structure used during profile creation:

```json
{
  "print": {
    "command": "extrusion_cali_del",
    "filaments": [
      {
        "cali_idx": 4,
        "filament_id": "GFA01",
        "nozzle_diameter": "0.4",
        "nozzle_id": "HS00-0.4",
        "setting_id": "PF12345678901234567"
      }
    ],
    "sequence_id": "50003"
  }
}
```

##### Schema B: Dual-Nozzle IDEX Deletion (H2D, X2D, H2C)
The database on IDEX platforms is keyed by physical carriage coordinate parameters. Deletions target these fields within a nested `"filaments"` array, identical to the structure used by Schema A:

```json
{
  "print": {
    "command": "extrusion_cali_del",
    "filaments": [
      {
        "nozzle_diameter": "0.4",
        "nozzle_id": "HS00-0.4",
        "extruder_id": 0
      }
    ],
    "sequence_id": "50004"
  }
}
```

---

### 7.3 Mechanical & Firmware Quirks

#### K-Profile Request Priming
The firmware's command processor ignores the initial `"extrusion_cali_get"` command received immediately after MQTTS connection establishment. Retrieving the profile database requires sending a dummy `"extrusion_cali_get"` payload first as a priming command.
