**bambino > mqtt > commands**

# Module: mqtt::commands

## Contents

**Modules**

- [`ams`](#ams) - AMS-related MQTT command payloads (filament change, drying, RFID scan, settings).
- [`control`](#control) - Print lifecycle commands (pause, resume, stop, speed, skip objects, calibration).
- [`gcode`](#gcode) - G-code dispatch command payload.
- [`hardware`](#hardware) - Hardware control commands (LEDs, fans, airduct mode, buzzer, prompt sound).
- [`print_job`](#print_job) - Print job dispatch (file selection, AMS material mapping, plate/timelapse config).
- [`status`](#status) - Status query commands (pushall, get_version, clean_print_error).

**Functions**

- [`clamp_task_id`](#clamp_task_id) - Clamps a 64-bit transaction or tracking identifier (typically standard UNIX epoch milliseconds) within the strict boundary limits of a 32-bit signed integer (`2147483647`).

---

## Module: ams

AMS-related MQTT command payloads (filament change, drying, RFID scan, settings).



## bambino::mqtt::commands::clamp_task_id

*Function*

Clamps a 64-bit transaction or tracking identifier (typically standard UNIX epoch milliseconds) within the strict boundary limits of a 32-bit signed integer (`2147483647`).

**Why this is critical [REF-MQTT-ENV]:**
The printer's onboard G-code parsing routine clamps subtask identifiers to standard 32-bit
signed integer limits. If a connecting client uses an un-clamped millisecond epoch (13-digit integer),
the memory allocation registers on the motion board will overflow. This causes the printer to lock
indefinitely in an `IDLE` state and reject all subsequent print dispatches.

```rust
fn clamp_task_id(raw_id: u64) -> u32
```



## Module: control

Print lifecycle commands (pause, resume, stop, speed, skip objects, calibration).



## Module: gcode

G-code dispatch command payload.



## Module: hardware

Hardware control commands (LEDs, fans, airduct mode, buzzer, prompt sound).



## Module: print_job

Print job dispatch (file selection, AMS material mapping, plate/timelapse config).



## Module: status

Status query commands (pushall, get_version, clean_print_error).



