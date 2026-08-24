---
paths:
  - "src/mqtt/commands/**"
  - "src/diagnostics/**"
  - "src/client/connect.rs"
  - "src/client/mod.rs"
---

All MQTT sequence IDs and task IDs must be clamped to 32-bit signed integer max (`TASK_ID_MAX`). Un-clamped values overflow the motion board's allocation registers, locking the printer in `IDLE` and making it reject every subsequent print dispatch — see the doc comment on `clamp_task_id` in `src/mqtt/commands/mod.rs`.

**How the invariant is enforced differs by layer, and only one layer is still convention:**

- **Wire-request constructors (`src/mqtt/commands/**`, `src/diagnostics/**`) — type-enforced, nothing to remember.** Every one takes `sequence_id: impl Into<ClampedTaskId>` (`src/mqtt/commands/mod.rs`). `ClampedTaskId` is obtainable only through its clamping `From<u64>` impl, so skipping the clamp is not expressible. Don't audit these for a missing `clamp_task_id()` call and don't add one — take `impl Into<ClampedTaskId>` in any new constructor and the invariant holds by construction. `PrintJobConfig::new` is the deliberate exception: it is a builder, not a wire request, and stores `raw_subtask_id: u64` unclamped until `ProjectFileRequest::from_config` converts it.
- **The client's own sequence counter (`src/client/mod.rs`, `src/client/connect.rs`) — still convention.** `next_sequence_id()` and `ensure_mqtt()`'s wall-clock reseed call `clamp_task_id()` directly, because they maintain a running `u64` counter rather than producing a wire request, and the modulo semantics are load-bearing (continuation across the wraparound, not a reset to a fixed ceiling: `clamp_task_id(TASK_ID_MAX) == 0`). These two call sites are the only place a future edit can still drop the clamp silently.

Historical note, so a reader doesn't re-derive it: BUG-001 was 24 constructors across 7 files each independently remembering to call `clamp_task_id()`, guarded by a regression test that exercised only 2 of them. `ClampedTaskId` was introduced to close that class structurally. This rule previously read "use `clamp_task_id()` for task IDs" full stop, which sent reviews hunting for per-call-site gaps in `commands/`/`diagnostics/` that no longer exist.
