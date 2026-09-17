---
paths:
  - "src/test_support.rs"
  - "src/discovery/mod.rs"
  - "src/ftps/protocol/**"
  - "src/mqtt/client/**"
  - "src/camera/binary.rs"
  - "Makefile"
---

`cfg(test)` is true under **every** feature set, so a `#[cfg(test)]` module that names `crate::io::tokio` or `crate::io::TokioIo` without a `feature = "tokio"` guard breaks the entire lib-test build for `alloc`/`embassy`/no_std — not just its own test. That made `pub(crate)` no_std code untestable outright for as long as it went unnoticed, because an integration test (a separate crate) can't reach `pub(crate)` items and `cargo check --lib` never builds test code (#291).

Default test doubles live in `src/test_support.rs` (`#[cfg(test)] mod test_support;`, no platform gate) and compile under `no_std` + `alloc`: `MockTimer` (virtual monotonic clock; `sleep()` returns instantly and advances the clock, so a `sleep`-paced timeout loop terminates deterministically instead of spinning) and `MockIo` (scripted per-`read()` chunks, per-`write()` recording). Reach for these first; a new double belongs there, not in a test module, unless it is genuinely single-use.

Use a real `crate::io::tokio` type only when the test is *about* the tokio backend or asserts **wall-clock** behaviour — a socket that is genuinely `Pending` rather than merely empty, raced against a sleep that really elapses. `MockTimer` cannot model that (its `sleep` is instant in real time, so racing a read against it resolves to "timed out" every call, the same hazard `TimerProvider::has_real_clock` exists for). Gate such a test on `feature = "tokio"` — either an inner `#[cfg(feature = "tokio")] mod async_tests` (as `mqtt/client/frame.rs`, `mqtt/client/mod.rs`, `mqtt/client/pending.rs` and `camera/binary.rs` do) or a sibling module declared `#[cfg(all(test, feature = "tokio"))]` (`ftps/protocol/tests_tokio.rs`).

`#[tokio::test]` itself is **not** the problem and never needs gating: `tokio` is an unconditional dev-dependency, so the attribute compiles under any feature set. Only the feature-gated `crate::io::tokio` module and `crate::io::TokioIo` do.

`make test-embassy-host` runs `cargo test --no-default-features --features "embassy,std" --lib` and is what keeps this from regressing; nothing else in the gate builds test code off the default feature set.
