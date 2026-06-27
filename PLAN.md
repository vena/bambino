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

Review the reference documentation in `/reference` for alignment with the now expansive typed structs and quirks modules. It's likely that we have discovered and implemented things that were not known or incorrect during the creation of the reference docs. Update the reference docs to fix any errors, confusion, or mis-alignment.

Note: the reference docs document Bambu Lab 3D printer protocols as a language-agnostic guidebook, they do not document the bambino library API. This project is dual-tract: document the protocols AND build a library to use them. Phase 23 covers library documentation separately.

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
| 22 | Review and Align Reference Docs | Not Started |
| 23 | Rustdoc Library Documentation | Not Started |