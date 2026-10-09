---
paths:
  - "src/ftps/**"
  - "src/mqtt/client/**"
  - "src/camera/**"
---

Mock tests cannot verify wire-level write/read framing — real hardware can. A mock server reads a stream regardless of how many writes produced it, so it can't distinguish "one write" from "two writes." This is a narrow failure class, not a blanket "test everything on hardware" rule — most wire-code changes (new fields, parsing fixes, validation, constants, error handling) don't need it. **The narrow class that does: changing the *shape* of writes or reads on an already-working wire path** — splitting one write into several (or merging several into one), changing read granularity (byte-at-a-time vs. buffered), or wrapping a read in new timeout/select/race logic. Changes in this class must be physically verified against real hardware before being considered done — passing `cargo test` alone is not sufficient, since mocks read/write a buffer regardless of how many calls produced it. If you're an agent making a change in this narrow class: don't run that verification yourself even if printer credentials (IP/serial/access code) are present in the conversation or environment — ask the user to run the test manually and report the result back.

Embedded hardware questions that aren't about wire framing (resource exhaustion, timing, anything a static check can't observe) follow the same "don't self-verify, ask the user" rule, and already have reusable flashable harnesses: `esp32-hw-probe/` for ESP-IDF and `embassy-hw-probe/` for bare-metal Embassy (each has its own `CLAUDE.md`). Don't build a fresh scaffold for a new investigation; swap the harness's `src/main.rs` instead.
