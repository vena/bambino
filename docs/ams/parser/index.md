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
| [`resolve_printing_global_id`](#resolve-printing-global-id) | fn | Resolves the currently printing tray's global ID via `tray_now` + an `ams_extruder_map` inversion, accounting for IDEX map translations. |

## Functions

### `clean_stale_tray_data`

```rust
fn clean_stale_tray_data(tray: &mut crate::types::AmsTray, ams_id: u8)
```

**Types:** [`AmsTray`](../../types/telemetry/ams/index.md#amstray)

Explicitly sanitizes and nullifies telemetry fields when a physical slot becomes empty.

**Incremental Telemetry Update Slot Cleansing Rules [REF-AMS-DECODE]:**
To save network bandwidth, the printer's incremental telemetry pushes omit configuration
parameters (like `tray_type` or `tray_color`) when a spool is extracted. Without active
cleanup on the client side, standard parsers would preserve the material properties of the
previously loaded spool indefinitely.

This routine inspects the tray's state — 9 (`AMS_TRAY_STATE_EMPTY`) meaning Empty/Absent and
10 (`AMS_TRAY_STATE_SPOOL_NOT_FED`) meaning a spool is physically present but not yet fed to
the extruder, both treated as absent-equivalent for stale-data cleansing (verified
against pybambu/Bambuddy — see `AMS_TRAY_STATE_SPOOL_NOT_FED`'s doc comment) — and clears all
stale config keys if either applies. Treats an explicit empty `tray_type` string as a
clearing signal too — but an *absent* `tray_type` (the common incremental-update case,
e.g. a `state: 11` update that simply doesn't repeat `tray_type`) is not, by itself, a
clearing signal. Confirmed against `reference/05_materials_ams.md`'s
Bambuddy cross-check (`on_ams_change`'s `loaded = cur_state == 11 or (cur_state not in
(9, 10) and cur_type.strip())`): state 11 is unconditionally treated as loaded regardless
of whether `tray_type` was repeated in that update, so clearing on absence alone would
wipe a currently-printing tray's material data.

`ams_id` gates the state-9 heuristic: on AMS-HT units (`ams_id` 128-135), state 9 on a
partial power-on frame means *loaded*, not empty — the opposite of its meaning on a
standard 4-slot AMS — so state 9 alone is not treated as a clearing signal for HT units.
Independently corroborated by Bambuddy's incremental-merge handler, which skips the same
heuristic for `ams_id >= 128` after a live H2D Pro wiped an HT spool on every power-on
(their issue #2594); the exception is recorded in `reference/05_materials_ams.md`, which
also explains why `AMS_TRAY_STATE_POWER_OFF` (0) is deliberately *not* gated the same way.

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

**AMS-HT units (IDs 128-135) do participate in `tray_exist_bits`, at a fixed offset**
— BambuStudio's `DevAms::GetTrayId` (`DevFilaSystem.cpp:833`, `GetTrayId`'s N3S
branch) computes the bit index as `16 + (ams_id - 128) + slot_id`, confirmed independently
in OrcaSlicer with an equivalent formula. This reopens and reverses the earlier "AMS-HT
doesn't participate" conclusion, which was based on an incomplete read of BambuStudio's
source.

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

Resolves the currently printing tray's global ID via `tray_now` + an `ams_extruder_map`
inversion, accounting for IDEX map translations.

**Multi-AMS Local Index Resolution [REF-AMS-DECODE]:**
Multi-extruder platforms (such as the H2D series) emit local slot indexes (0 to 3) inside
their `tray_now` telemetry parameter. To resolve this back to a global index, the client must
inspect the active extruder carriage and correlate it against the `ams_extruder_map` matrix.

This is the *fallback* path — prefer
[`crate::client::PrinterClient::printing_tray_global_id`](../../client/index.md#printerclient), which decodes
`ExtruderInfo::current_ams_slot()` (`snow`) directly and needs no `ams_extruder_map` at all.
This function remains unwired in the crate's own client code: `ams_extruder_map`'s
construction from real wire data is itself an unresolved, unconfirmed design question
(no field in this crate's telemetry types currently sources it), and the map can be
genuinely ambiguous (N AMS units per extruder) in ways a flat `&[u8]` array can't express —
a caller with its own confirmed `ams_extruder_map` source may still use this directly.

