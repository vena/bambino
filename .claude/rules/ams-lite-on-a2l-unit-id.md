---
paths:
  - "src/ams/**"
  - "src/client/ams.rs"
  - "src/client/drying.rs"
  - "src/mqtt/commands/ams.rs"
  - "src/mqtt/commands/print_job.rs"
  - "src/diagnostics/kprofile.rs"
  - "src/types/telemetry/ams.rs"
  - "src/types/telemetry/device.rs"
  - "src/bin/bambino-cli/control.rs"
---

An AMS Lite attached to an A2L printer is the one AMS unit outside the `0..=3` / `128..=135` / `254..=255` id blocks. Anything that validates, ranges over, matches on, or derives arithmetic from an `ams_id`, `tray_id` or global tray index must handle it. Missed five times so far (#193, #221, #258, #259, #271), each in a layer the previous fix didn't touch.

- **Name the pairing, not a product.** A2L is a printer; AMS Lite is an AMS unit. Write "AMS Lite on an A2L" / "A2L-attached AMS Lite", never "A2L AMS Lite" or "A2L Lite" — there is no such product, and the same unit takes id 0 on an A1. The shorthand comes from bambuddy's `A2L_LITE_*` constants and "A2L AMS-Lite" comments; it spread through issue titles (#193, #221, #258, #259, #270, #271) that later sessions copied. Don't reuse upstream identifiers as prose.
- **Two spellings.** The wire carries physical id `16` (`AMS_LITE_ON_A2L_PHYSICAL_ID`) with a **local** `0..=3` slot, confirmed from the firmware's own `ams_mapping2` (bambuddy `a2l_lite_wire_ids`, `bambu_mqtt.py:142-163`). Telemetry ingest normalizes it to `6` (`AMS_LITE_ON_A2L_NORMALIZED_ID`, `normalize_ams_unit_id`), so the crate's internal spelling is `6`.
- **Client methods accept both** and translate to `16` at dispatch: `is_valid_ams_id` / `is_valid_ams_bus_unit_id` for validation, `wire_ams_id` for the outbound id (`src/client/ams.rs`). A lookup against cached telemetry normalizes first (`cached_ams_unit_model`). Raw `*Request::new` builders take the wire form as given.
- **Global tray id is `24..=27`** (`AMS_LITE_ON_A2L_GLOBAL_TRAY_IDS`), from BambuStudio's `GetTrayIndexMap` (`DevFilaSystem.cpp:367-373`), which fills `extrusion_cali_sel`'s `tray_id`. bambuddy's `64..=67` is its own unconfirmed extrapolation — don't adopt it.
- **BambuStudio has three tray-index formulas; don't mix them up** (settled in #210, re-derived by mistake since). `GetTrayIndexMap` (calibration, AMS settings): AMS-HT = `ams_id` (128-135). `ams_filament_mapping` (flat `ams_mapping`): also 128-135. `DevAms::GetTrayId`: AMS-HT = `16 + (ams_id - 128)`, but only as the `tray_exist_bits` bit index. bambuddy's calibration code disagrees with itself on AMS-HT (local slot in `extrusion_cali_sel`, `(ams_id-128)*4 + slot` in `extrusion_cali_set`); BambuStudio wins.
- **Flat `ams_mapping` channel is the bare local slot**, not a global id (`src/ams/mapping.rs`).
- **`ams_change_filament` `target` is `16`**, via BambuStudio's `ams_id >= 16 → target = ams_id` branch (`DeviceManager.cpp:1644-1663`).
- `test_ams_commands_address_a2l_ams_lite` (`tests/integration/client_negative_test.rs`) covers every public `ams_id`-taking client entry point — add any new one to it.
- Unrelated `>= 16` checks exist (e.g. nozzle ids in `bin/bambino-cli/monitor/dashboard.rs`); confirm the id space before treating one as a hit.
