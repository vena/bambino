# Deep Code Review Plan

Module-by-module review of the `bambino` crate. Detailed write-ups belong in commit messages, not here.

When completing a phase, collapse its section into the completed summary below.

---

## Completed Phases (1–19b)

**Phases 1–18** (Core → Command-Response): 75 fixes, telemetry split into `telemetry/{mod,report,device,ams,diagnostics}.rs`, typed API (`TelemetryEvent`, `VersionInfo`, `ExtrusionCaliGetResponse`), IDEX schema (`ExtruderCollection`/`ExtruderInfo`), platform abstraction (`SecureConnect`, `TimerProvider`), `PrinterClient` command-response with `poll_until()` buffering.

**Phases 19–19b** (Wire Field Completeness): Cross-referenced all `Deserialize` structs against pybambu and Bambuddy. Added ~70 fields total, 3 new sub-structs (`LightReport`, `BedTelemetry`/`BedInfo`, `ExtToolTelemetry`). Unified `AmsTray`/`VirtualTray` field sets. Removed dead `progress` field. Key type decisions preserved in CLAUDE.md: `AmsUnit.info` is hex string, `vir_slot` is separate from `vt_tray`, `fire_ext` is opaque `Value`.

**Deferred from 19b:** `AmsUnit.info` bitmask helper methods (quirks phase), `device.bed` vs `bed_temper` model-aware selection (temperature normalization phase).

---

## Phase 20: Expanded CLI Control Commands

Add commonly useful control commands to the CLI for hardware testing. With Phase 18's typed responses, new commands that query state (e.g. version, calibration status) can use typed return values directly.

- [ ] **`speed <level>`** — Set print speed (silent, standard, sport, ludicrous) via `set_print_speed()`
- [ ] **`clear-error`** — Clear active print error codes via `clear_print_error()`
- [ ] **`airduct <cooling|heating|laser>`** — Switch airduct damper mode via `set_airduct_mode(AirductMode)` (H2/P2S/X2D)
- [ ] **`calibrate <options>`** — Trigger calibration routines via `start_calibration()` (bed-leveling, vibration, motor-noise, nozzle-height, heatbed-thermal)
- [ ] **`ams dry <ams_id> <temp> <time> <rotate_tray> <filament>`** — Start AMS drying cycle via `start_drying()`
- [ ] **`ams dry-stop <ams_id>`** — Stop AMS drying cycle via `stop_drying()`

## Phase 21: Review and Align Reference Docs

Review the reference documentation in `/reference` for alignment with the now expansive typed structs and quirks modules. It's likely that we have discovered and implemented things that were not known or incorrect during the creation of the reference docs. Update the reference docs to fix any errors, confusion, or mis-alignment.

---

## Progress Tracker

| Phase | Module | Status |
|-------|--------|--------|
| 1–19b | Core → Wire Field Completeness | **Complete** |
| 20 | Expanded CLI Commands | Not started |
| 21 | Review and Align Reference Docs | Not Started |