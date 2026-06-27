# Deep Code Review Plan

Module-by-module review of the `bambino` crate. Detailed write-ups belong in commit messages, not here.

When completing a phase, collapse its section into the completed summary below.

---

## Completed Phases (1–21)

**Phases 1–18** (Core → Command-Response): 75 fixes, telemetry split into `telemetry/{mod,report,device,ams,diagnostics}.rs`, typed API (`TelemetryEvent`, `VersionInfo`, `ExtrusionCaliGetResponse`), IDEX schema (`ExtruderCollection`/`ExtruderInfo`), platform abstraction (`SecureConnect`, `TimerProvider`), `PrinterClient` command-response with `poll_until()` buffering.

**Phases 19–19b** (Wire Field Completeness): Cross-referenced all `Deserialize` structs against pybambu and Bambuddy. Added ~70 fields total, 3 new sub-structs (`LightReport`, `BedTelemetry`/`BedInfo`, `ExtToolTelemetry`). Unified `AmsTray`/`VirtualTray` field sets. Removed dead `progress` field. Key type decisions preserved in CLAUDE.md: `AmsUnit.info` is hex string, `vir_slot` is separate from `vt_tray`, `fire_ext` is opaque `Value`.

**Phase 20** (Expanded CLI Control Commands): Added 6 new control actions to the CLI, all routing through `PrinterClient` library methods: `speed` (print speed levels), `clear-error`, `airduct` (damper mode), `calibrate` (bitmask-combined routines), `ams dry` (drying cycle), `ams dry-stop`. Updated README and CLI help text.

**Phase 21** (Telemetry Accessor Methods): Added `impl AmsUnit` bitmask helpers (`parse_info`, `ams_type`, `dry_status`, `extruder_assignment`, `dry_sub_status`) to decode the hex-encoded `info` field. Added `TelemetryReport::bed_temperatures()` cascade accessor that checks `device.bed` (new-gen composite-packed) first, then `print.device.bed` (pushall-nested), falling back to old-gen `bed_temper`/`bed_target_temper`. Option A chosen — no quirks dependency needed since field presence is the model signal.

---

## Phase 22: Review and Align Reference Docs

Cross-referenced all 7 reference chapters against the typed structs, quirks engine, MQTT command builders, and MODEL_MATRIX.csv (wiki-confirmed specs). The reference docs document Bambu Lab 3D printer protocols as a language-agnostic guidebook — they do not document the bambino library API. Phase 23 covers library documentation separately.

### Completed (22a): Serial Prefix Corrections

Updated `resolve_model()` and Section 1.5 with wiki-confirmed serial prefixes. Each H2-series model has a distinct prefix — the "H2S/H2D collision rule" was invalid and has been removed.

- `094` → H2D only (was shared by all H2 models)
- `093` → H2S (new)
- `239` → H2D Pro (new, was incorrectly mapped to H2D)
- `31B` → H2C (new)

### Completed (22b): Reference Doc Alignment

All 20 discrepancies between the reference docs and the codebase have been resolved. Changes by chapter:

- **Chapter 2**: Added `TYPE I` to handshake command sequence; added RETR download pipeline documentation.
- **Chapter 3**: Fixed example JSON (`progress` → `mc_percent`); added `spd_lvl`/`spd_mag`/`mc_percent`/`stat`/`lights_report`/`print_type` field docs; documented dual-location `DeviceTelemetry`; added H2S to door sensor routing; added P2S to secondary aux fan list; added X2D to IDEX and A2L to Standard nozzle key categories; added laser mode to `set_airduct`; fixed prompt sound support list (A1/A1 Mini/A2L); fixed buzzer support list (H2S/H2D/H2D Pro/H2C).
- **Chapter 4**: Documented `device.bed` wire path cascade (new-gen vs old-gen); documented `device.extruder.state` bitmask; added `device.ext_tool` laser/cutter mount telemetry.
- **Chapter 5**: Documented `AmsUnit.info` hex bitmask; documented `vt_tray`/`vir_slot` telemetry paths; documented `ams_status` combined state bitmask; removed FTS from index (requires further research).
- **Chapter 6**: Added A2L to Port 6000 model list; documented `rtsp_url` telemetry field.
- **Chapter 7**: Documented `ts_boot`/`ts_unix` optional timestamp fields on HMS entries.
- **Index (`00_index.md`)**: Updated all affected section descriptions.

---

## Phase 23: Rustdoc Library Documentation

### Problem

The library has extensive doc comments on most public types and methods, but coverage is uneven across modules and hasn't been audited holistically for rustdoc output quality. For `bambino` to be usable as a standalone crate, the generated docs need to tell a coherent story — not just describe individual items, but guide consumers through the API surface.

### Approach

Run `cargo doc --no-deps` and audit the rendered output, not just the source comments. Rustdoc has its own concerns beyond `///` coverage: re-export visibility, module-level narrative, cross-linking, and example snippets.

### Items

- [ ] **Crate-level docs** (`lib.rs` `//!`): Expand beyond the current blurb. Add a quick-start usage example showing `PrinterClient` connection and telemetry polling. Mention the three compilation targets (host/ESP-IDF/bare-metal) and which feature flags control them.
- [ ] **Module-level docs** (`//!`): Audit every `pub mod` for a module-level doc comment that explains the module's role and key types. Modules like `ams`, `camera`, `discovery`, `diagnostics`, `ftps`, `mqtt`, `quirks`, and `client` should each have a short narrative.
- [ ] **Public item coverage**: Run `cargo doc --no-deps 2>&1` and fix any missing-doc warnings. Ensure all `pub` structs, enums, traits, methods, and functions have `///` comments. Focus on items that appear in the rendered docs — internal `pub(crate)` items are lower priority.
- [ ] **Re-export docs**: Check that `pub use` re-exports in `types/mod.rs` and `lib.rs` render clearly in rustdoc. Add `#[doc(inline)]` where re-exports should show full docs at the re-export site rather than linking to the source module.
- [ ] **Cross-references**: Convert prose references to other types (e.g. "see `PrinterTelemetry`") into rustdoc links (`` [`PrinterTelemetry`] ``) so they're clickable in the rendered output.
- [ ] **Example snippets**: Add `# Examples` sections to the most-used entry points: `PrinterClient::new()`, `poll_telemetry()`, `bed_temperatures()`, `send_gcode()`, key command methods. These don't need to compile (use `no_run` or `ignore`) but should show realistic usage patterns.
- [ ] **Verification gate**: `cargo doc --no-deps` must complete with zero warnings.

---

## Progress Tracker

| Phase | Module | Status |
|-------|--------|--------|
| 1–21 | Core → Telemetry Accessors | **Complete** |
| 22a | Serial Prefix Corrections | **Complete** |
| 22b | Reference Doc Alignment (20 items) | **Complete** |
| 23 | Rustdoc Library Documentation | Not Started |