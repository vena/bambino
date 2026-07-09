*[bambino](../../index.md) / [ams](../index.md) / [parser](index.md)*

---

# Module `parser`

# AMS Telemetry & Bitmask Parser

Implements low-level bitwise operations and sanitization logic for parsing
Bambu Lab AMS telemetry reports [REF-AMS-DECODE]. This includes checking spool
presence via hex bitmasks, managing power-down state anomalies, cleansing stale
tray data, and calculating global indexes.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`clean_stale_tray_data`](#clean-stale-tray-data) | fn | Explicitly sanitizes and nullifies telemetry fields when a physical slot becomes empty. |
| [`evaluate_spool_presence`](#evaluate-spool-presence) | fn | Evaluates if a physical spool is present in a specific standard AMS slot. |
| [`resolve_global_tray_id`](#resolve-global-tray-id) | fn | Computes the unique global channel identifier for a given expansion unit and local tray. |
| [`resolve_printing_global_id`](#resolve-printing-global-id) | fn | Resolves the currently printing tray's global ID, accounting for IDEX map translations. |

## Functions

### `clean_stale_tray_data`

```rust
fn clean_stale_tray_data(tray: &mut crate::types::AmsTray)
```

**Types:** [`AmsTray`](../../types/telemetry/ams/index.md#amstray)

Explicitly sanitizes and nullifies telemetry fields when a physical slot becomes empty.

**Incremental Telemetry Update Slot Cleansing Rules [REF-AMS-DECODE]:**
To save network bandwidth, the printer's incremental telemetry pushes omit configuration
parameters (like `tray_type` or `tray_color`) when a spool is extracted. Without active
cleanup on the client side, standard parsers would preserve the material properties of the
previously loaded spool indefinitely.

This routine inspects the tray's state (with 9 representing Empty / Absent) and clears all
stale config keys if empty. It treats an empty `tray_type` string as an explicit clearing signal.

### `evaluate_spool_presence`

```rust
fn evaluate_spool_presence(tray_exist_bits: &str, ams_id: u8, tray_id: u8, power_on_flag: bool) -> Option<bool>
```

Evaluates if a physical spool is present in a specific standard AMS slot.

Standard AMS units contain up to 4 slots. The physical presence is tracked via
a hexadecimal bitmask string (`tray_exist_bits`).

**The Printer-Shutdown Telemetry Exception [REF-AMS-DECODE]:**
During printer shutdown sequences, the firmware often emits a final status packet
where `tray_exist_bits` is `0` and `power_on_flag` is `false`. To prevent downstream
observers from falsely reporting a cascade of physical "spool removed" events,
this evaluator returns `None` strictly when both conditions are met. If `power_on_flag`
is `false` but the parsed bitmask is non-zero, this represents a valid offline state
and is processed normally.

### `resolve_global_tray_id`

```rust
fn resolve_global_tray_id(ams_id: u8, tray_id: u8) -> Option<u8>
```

Computes the unique global channel identifier for a given expansion unit and local tray.

Returns `None` if `ams_id` falls outside all valid ranges (standard 0–3,
AMS-HT 128–135, external 254–255) or if `tray_id >= 4` on the standard path.

The physical mapping aligns as:
* **Standard AMS Slots**: Sized in blocks of 4 per expansion unit: `(ams_id * 4) + tray_id`.
* **AMS-HT Units**: Single-slot systems where the channel ID equals the bus `ams_id` directly.
* **Virtual Spools**: Channels mapped to the external spool holder (ID 254 or 255).

### `resolve_printing_global_id`

```rust
fn resolve_printing_global_id(tray_now: u8, active_extruder: Option<u8>, ams_extruder_map: &[u8]) -> Option<u8>
```

Resolves the currently printing tray's global ID, accounting for IDEX map translations.

**Multi-AMS Local Index Resolution [REF-AMS-DECODE]:**
Multi-extruder platforms (such as the H2D series) emit local slot indexes (0 to 3) inside
their `tray_now` telemetry parameter. To resolve this back to a global index, the client must
inspect the active extruder carriage and correlate it against the `ams_extruder_map` matrix.

