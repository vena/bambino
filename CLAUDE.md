# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```sh
cargo build                                          # Default host build (tokio + rustls)
cargo build --bin bambino-cli --features cli         # Build the CLI binary
cargo test                                           # Run all tests
cargo test --lib                                     # Library tests only
cargo test test_name                                 # Single test by name
cargo build --no-default-features --features alloc --lib  # no_std compatibility check (must pass)
cargo check --no-default-features --features embassy --lib  # embassy target check (must pass)
```

`make check-fast` runs all of the above (build, test, both feature-gate checks, clippy) in one command; `make check-esp-idf [CHIP=esp32c6]` wraps `scripts/check-esp-idf.sh`; `make check-all` runs both. `.github/workflows/ci.yml` and `.github/workflows/esp-idf.yml` mirror the same commands but are dormant: this repo has no GitHub remote yet, so they don't run anywhere. `esp-idf.yml` is path-filtered to only fire when esp-idf-gated files change, and its cost is higher on GitHub-hosted runners than locally — the script's Docker-volume caching doesn't survive GitHub's ephemeral VMs, so every run there pays the full cold-build cost, not just the first.

Every change must compile under both the default `tokio` feature set and the `alloc`, `embassy`, and `no_std` library targets. Run `cargo clippy` as part of the verification gate. The `--lib` flag scopes the no_std check to library code only — the CLI is host-only. Use `#[cfg(not(feature = "std"))]` imports from `alloc` (String, Vec, format!) for no_std paths.


The `embassy` feature is not implied by `alloc` alone — `io/embassy.rs` and `#[cfg(feature = "embassy")]` code aren't exercised by the plain no_std/alloc check, so both commands above are required. The `esp-idf` target can't be checked with plain `cargo check` (needs Python/cmake/ninja/the ESP-IDF SDK, `rust-src`, and for Xtensa a forked toolchain) — run `scripts/check-esp-idf.sh [chip]` (default `esp32c6`) instead, which pulls the matching `espressif/idf-rust` Docker image and caches the registry/`rust-src`/`target/` across runs via named volumes. Not wired into CI — run it yourself before touching `src/io/esp_idf.rs`, which uses the safe `esp_idf_svc::tls::{Config, EspTls}` wrapper rather than raw `esp_idf_svc::sys` bindgen. There's no `esp32p4` Docker tag yet (spike before relying on `IDF_TARGET=esp32p4` against the `esp32c6` image); Xtensa tags (`esp32`/`s2`/`s3`) are also unverified by this script so far.

**Mock tests cannot verify wire-level write/read framing — real hardware can.** A mock server reads a stream regardless of how many writes produced it, so it can't distinguish "one write" from "two writes." This is a narrow failure class, not a blanket "test everything on hardware" rule — most wire-code changes (new fields, parsing fixes, validation, constants, error handling) don't need it. **The narrow class that does: changing the *shape* of writes or reads on an already-working wire path** — splitting one write into several (or merging several into one), changing read granularity (byte-at-a-time vs. buffered), or wrapping a read in new timeout/select/race logic. Changes in this class must be physically verified against real hardware before being considered done — passing `cargo test` alone is not sufficient, since mocks read/write a buffer regardless of how many calls produced it. If you're an agent making a change in this narrow class: don't run that verification yourself even if printer credentials (IP/serial/access code) are present in the conversation or environment — ask the user to run the test manually and report the result back. Never write an access code or serial number into a file in this repository (docs, tests, scratch files, commit messages) — treat them the same as any other credential.

**CLI-only dependencies live behind the `cli` feature, not `tokio`.** `crossterm`, `env_logger`, and any future CLI-exclusive dep (e.g. `clap`) must be gated by `cli = ["dep:...", "tokio"]`, never added to the `tokio` feature directly — see the feature comments in `Cargo.toml` for why. `[[bin]] required-features = ["cli"]` in Cargo.toml enforces this at the target level — every file under `src/bin/bambino-cli/` starts with `#![cfg(feature = "cli")]`.

## Architecture

**bambino** is a multi-platform async Rust crate for controlling Bambu Lab 3D printers over LAN. It compiles to three targets from one codebase: host (tokio/rustls), ESP-IDF (std), and bare-metal (embassy/no_std).

### Key Invariants

1. **No direct platform I/O in library code.** All network I/O goes through abstract traits in `src/io/` (`AsyncIo`, `TlsConnector`, `RawStreamFactory`, `AsyncUdpSocket`, `TimerProvider`). Never use `tokio::` or `std::net::` outside `src/io/`. `TlsConnector` wraps an existing raw stream in TLS; `RawStreamFactory` dials a fresh pre-TLS stream to host:port (used for both MQTT's lazy connect and FTPS's data channel). `TimerProvider::now_millis()` provides monotonic clock for platform-agnostic timeouts.

2. **All model-specific behavior goes through the quirks engine.** Access via `model.quirks()` — never match on `BambuModel` variants for behavioral dispatch. Strategy structs live in `src/quirks/models/`.

3. **MQTT commands follow the Payload+Request pattern** (`src/mqtt/commands/` — split into `mod.rs` plus per-category files (`ams.rs`, `control.rs`, `gcode.rs`, `hardware.rs`, `print_job.rs`, `status.rs`) — and `src/diagnostics/kprofile.rs`):
   - A `#[derive(Serialize)]` payload struct with typed fields
   - A wrapper struct with a single `pub print: PayloadType` field (or `pushing:`, `system:`, `info:`)
   - An `impl` block with `pub fn new(...)` constructor

Non-obvious type decisions and behavioral invariants live close to the code they govern, not here:

- Cross-cutting invariants (span multiple non-adjacent `src/` paths) → `.claude/rules/*.md`, each scoped with a `paths:` frontmatter glob.
- Single-directory invariants → a nested `CLAUDE.md` in that directory (currently: `src/types/telemetry/`, `src/camera/`, `src/ftps/`, `src/io/`, `src/mqtt/client/`).
- Only truly global content (this section, build/test commands, architecture overview) stays here.

## Key Conventions

- `serde_json` is used with `default-features = false` — always use `serde_json::to_vec` (not `to_string`) for payloads.
- Library code uses the `log` crate facade (`log::debug!`, `log::trace!`, `log::warn!`) — never `println!`.
- `BambuError` has dual `Display` impls: `thiserror` under `std`, manual under `no_std` (kept in sync by `test_display_consistency`). `ProtocolViolation` uses `Cow<'static, str>`.
- Magic numbers are extracted into named `pub(crate) const` blocks in each module. Use existing constants rather than introducing new literals.
- Protocol specs live in `reference/` as numbered markdown files. Always verify field names and types against reference docs when adding or modifying commands. When external sources (pybambu, Bambuddy, Bambu Studio, wire captures) contradict a reference doc, update the reference doc with the correction and note the verification source.
- Use MODEL_MATRIX.csv to track physical characteristics of printer models. When new information is **confirmed** about a printer model, update MODEL_MATRIX.csv
- When adding public types, modules, traits, or changing conventions: decide the routing *at the time you add the invariant*, don't defer it. A new invariant spanning multiple non-adjacent `src/` paths goes to `.claude/rules/<topic>.md` with a `paths:` glob; one confined to a single directory is a candidate for that directory's nested `CLAUDE.md`; only genuinely global content (applies regardless of which files a session touches) goes in this file. Keep whichever file it lands in concise — document constraints and gotchas, not API summaries. Write real commit bodies for anything that would otherwise become a narrative bullet — the "why," not just the "what" — and delete a completed `*_PLAN.md` file in its own commit with a body describing what shipped and where the resulting invariant lives.
- **Moving, renaming, or splitting a file changes what its `.claude/rules/*.md` globs match — grep `.claude/rules/` for the old path in the same commit and update every glob that references it.** A glob that stops matching doesn't error, it just silently stops loading — worse than never moving the invariant out of root `CLAUDE.md` at all, since root always loads. Same check applies to a nested `<dir>/CLAUDE.md`: if the split moves an invariant's code out of that directory, move the bullet too.
- Full-crate review sweeps require the `deep-review` skill, not ad hoc agent orchestration.
- `BACKLOG.md` entries require the `backlog` skill for rules/severity/next-ID — don't restate its rubric elsewhere.
- **Phases in markdown planning documents must be self-contained.** Each phase must be implementable by a clean session with zero prior conversation context beyond the existing codebase. When adding or altering a phase, inform the next session of what we learned and guide it by spelling out: the problem being solved (not just what to build), design constraints and trade-offs that shape the implementation, ordering dependencies between items, and which items are trivially independent. If a task has a hard design decision, state the options and either pick one or mark it as "decide first." A phase that requires reading git history or guessing at intent is underspecified.
