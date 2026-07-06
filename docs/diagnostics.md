**bambino > diagnostics**

# Module: diagnostics

## Contents

**Modules**

- [`hms`](#hms) - # HMS Diagnostic Telemetry Parsing & Unpacking Engine
- [`kprofile`](#kprofile) - # Linear Advance (Pressure Advance / K-Profile) Calibration Database Builders

---

## Module: hms

# HMS Diagnostic Telemetry Parsing & Unpacking Engine

Provides mathematical decoders to unpack physical printer hardware fault codes,
warning levels, and operational alerts from telemetry status streams [REF-DIAG-HMS].

This module parses:
1. The 32-bit `print_error` register into short-code formats.
2. The `hms` array containing active telemetry blocks (`attr` and `code`) into
   both 16-character Wiki slugs and 8-character local short-codes.

## Technical Specifications
* **Fault Isolation**: Filters out non-error statuses (low 16-bit word < `0x4000`)
  and user action confirmation echoes (such as user-initiated cancellation events)
  to isolate genuine hardware failures from routine system state updates.



## Module: kprofile

# Linear Advance (Pressure Advance / K-Profile) Calibration Database Builders

Exposes command serialization schemas and validation checks to manage stored
pressure-advance calibration profiles on the printer's onboard EEPROM [REF-DIAG-KPROF].

## Structural Guidelines & Constraints
* **Setting ID Validation**: Enforces the 19-character numeric `setting_id` boundary
  (`"PF"` followed by exactly 17 decimal digits) to prevent memory table corruption in the local
  EEPROM partition database.
* **Polymorphic Deletions**: Separates deletion schemas cleanly between standard single-nozzle
  platforms (keyed on `setting_id`) and dual-nozzle IDEX platforms (keyed on coordinate/carriage parameters).



