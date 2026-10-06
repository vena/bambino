---
paths:
  - "src/quirks/models/x1.rs"
  - "src/client/thermal.rs"
---

`ModelQuirks::bed_temp_max` takes `mains_220v: Option<bool>` (breaking, pre-1.0) — only the `X1C` and `X1` rows use it (both via `BedMax::Voltage` in `X1C_BED_TEMP_MAX`): bed ceiling is voltage-dependent and inverted (110°C on 220V, 120°C on 110V). `None` (no `home_flag` observed yet) clamps to the lower of the two, 110°C. Every other model ignores the parameter. `PrinterClient::set_bed_temperature` computes `mains_220v` from `TelemetryCache::last_home_flag` via `POWER_220V_BITMASK` (`home_flag` bit 3).
