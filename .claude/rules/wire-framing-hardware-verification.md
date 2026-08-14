---
paths:
  - "src/ftps/**"
  - "src/mqtt/client/**"
  - "src/camera/**"
---

Mock tests cannot verify wire-level write/read framing — real hardware can. A mock server reads a stream regardless of how many writes produced it, so it can't distinguish "one write" from "two writes." This is a narrow failure class, not a blanket "test everything on hardware" rule — most wire-code changes (new fields, parsing fixes, validation, constants, error handling) don't need it. **The narrow class that does: changing the *shape* of writes or reads on an already-working wire path** — splitting one write into several (or merging several into one), changing read granularity (byte-at-a-time vs. buffered), or wrapping a read in new timeout/select/race logic. Changes in this class must be physically verified against real hardware before being considered done — passing `cargo test` alone is not sufficient, since mocks read/write a buffer regardless of how many calls produced it. If you're an agent making a change in this narrow class: don't run that verification yourself even if printer credentials (IP/serial/access code) are present in the conversation or environment — ask the user to run the test manually and report the result back.

For ESP-IDF-specific hardware questions that aren't about wire framing (resource exhaustion, timing, anything else `scripts/check-esp-idf.sh`'s static check can't observe) — same "don't self-verify, ask the user" rule applies, but there's a reusable flashable test harness already scaffolded for it: see `src/io/CLAUDE.md` and `esp32-hw-probe/CLAUDE.md`. Don't build a fresh `*_PLAN.md`-driven scaffold from scratch for a new ESP-IDF hardware investigation — swap that harness's `src/main.rs` body instead.
