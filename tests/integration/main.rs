//! # Integration test suite (single binary)
//!
//! Every file here is a module of *one* test target, not a test crate of its own.
//! Cargo treats each top-level `tests/*.rs` file as a separate integration-test
//! crate, which means a separate executable: 12 files produced 12 binaries.
//!
//! **Why one binary:** the binary count, not the test count, was dominating the
//! local verification gate. On macOS the first execution of a freshly linked
//! executable pays a fixed ~31s of OS validation (measured: running a test binary
//! with `--list`, which prints test names and runs nothing, took 29-62s on its
//! first run and 0.00-0.01s on its second). `make check-fast` relinks the test
//! binaries on every commit that touches `src/`, so that penalty was paid once
//! per binary, every time -- ~442s of a ~613s gate, against 34s of actual
//! compilation and 32s of actual test execution. Collapsing 12 binaries into 1
//! leaves the lib test binary as the only other executable. See issue #245.
//!
//! Consolidating is safe here because the mocks are in-memory: `common/` builds
//! its fakes on `tokio::io::DuplexStream`, so there are no listening sockets, no
//! fixed ports, and no process-global state (no env vars, no logger init, no
//! statics) for tests to contend over now that they share a process.
//!
//! **Adding a test file:** drop it in this directory and add a `mod` line below.
//! A new top-level `tests/*.rs` file would silently become its own binary again
//! and reintroduce the penalty one binary at a time.
//!
//! Inside these modules, shared helpers are reached as `crate::common::...` --
//! not `common::...`, which only resolves from a crate root.

mod common;

mod camera_test;
mod client_core_test;
mod client_gcode_test;
mod client_negative_test;
mod client_reconnect_test;
mod client_session_test;
mod client_telemetry_cache_test;
mod client_version_test;
mod drying_capability_matrix_test;
mod ftps_test;
mod mqtt_test;
mod telemetry_replay_test;
