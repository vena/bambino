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
The printer publishes both hardware failures and non-error state indications (such as axis homing progress) within the `print_error` and `hms` registers. For `print_error`, non-error state indications are represented by low-word values less than `0x4000` (16384 decimal); only codes with a low 16-bit value $\ge 0x4000$ represent actual faults. For `hms` entries specifically, the check must compare the **full 32-bit `code`** against `0x4000`, not just its low 16 bits (BUG-109 — confirmed against BambuStudio's bundled `resources/hms/hms_en_093.json` fault catalog: 4591/4592 cataloged genuine `hms[]` faults have `code_low < 0x4000`, so a low-word-only check misclassifies nearly every real fault as a non-fault status step).

##### User-Action Echoes
During user-initiated print cancellations, the firmware raises specific confirmation codes (such as `0300_400C` and `0500_400E`) to confirm cancellation has completed. These are status confirmations, not active faults, and must not be treated as actual system errors.

---

### 7.2 Pressure Advance (K-Profile) Calibration [REF-DIAG-KPROF]

The printer's onboard EEPROM database houses user-configured Linear Advance (Pressure Advance) K-factor calibration profiles. These profiles are managed, queried, and loaded via direct MQTTS command schemas.

#### Query Calibration Profiles Database
To retrieve all stored profiles from the machine's database, publish the `"extrusion_cali_get"` command:

```json
{
  "print": {
    "command": "extrusion_cali_get",
    "sequence_id": "50001"
  }
}
```

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
