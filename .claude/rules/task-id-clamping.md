---
paths:
  - "src/mqtt/commands/**"
  - "src/diagnostics/kprofile.rs"
  - "src/client/connect.rs"
  - "src/client/mod.rs"
---

All MQTT sequence IDs and task IDs must be clamped to 32-bit signed integer max (`TASK_ID_MAX`). Use `clamp_task_id()` for task IDs.
