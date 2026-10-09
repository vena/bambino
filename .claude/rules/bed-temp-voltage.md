---
paths:
  - "src/quirks/models/x1.rs"
  - "src/client/thermal.rs"
  - "src/client/motion.rs"
---

`ModelQuirks::bed_temp_max` takes `mains_220v: Option<bool>`. Only the `X1C` row (and `X1`, which inherits it via `..X1C`) uses it, through `BedMax::Voltage` in `X1C_BED_TEMP_MAX`: the ceiling is voltage-dependent and inverted (110°C on 220V, 120°C on 110V), and `None` (no `home_flag` seen yet) takes the lower one. Every other row is `BedMax::Flat` and ignores the parameter. Every caller must pass the real region rather than `None` when it has one: `PrinterClient::set_bed_temperature` and `send_gcode` (into `validate_gcode`) both pass `PrinterClient::is_220v_power()`, which reads `TelemetryCache::last_home_flag` via `bits::is_220v` (`HOME_FLAG_POWER_220V`, `home_flag` bit 3).
