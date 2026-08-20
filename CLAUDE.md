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

`make check-fast` runs all of the above (build, test, both feature-gate checks, clippy) in one command; `make check-esp-idf [CHIP=esp32c6]` wraps `scripts/check-esp-idf.sh`; `make check-all` runs both. `.github/workflows/ci.yml` and `.github/workflows/esp-idf.yml` mirror the same commands and run live on every push to `main` (confirmed via `gh run list`) — this repo has a GitHub remote (`origin` → `github.com/vena/bambino`) and CI is active, not dormant.

Run `make install-hooks` once after cloning — it installs a pre-commit hook (source in `scripts/hooks/`) that runs `make check-fast` and rejects the commit on failure. **Docs-only commits skip the cargo gate** — the hook matches every staged path against an allowlist of known-inert paths (`*.md`, `.claude/`, `docs/`, `reference/`, `.github/`, `LICENSE`, `.gitignore`, `MODEL_MATRIX.csv`) and runs the full gate the moment one path falls outside it. It's an allowlist, not a denylist, so an unrecognized new file type fails safe (runs the gate) rather than slipping through unchecked. `tests/mocks/*.json`/`.ndjson` are deliberately *not* inert — tests deserialize them, so a fixture edit can fail the suite. `.git/hooks/` isn't tracked by git, so this is what makes the gate survive a fresh clone before a first push; CI is the backstop after that.

The `esp-idf` target needs `scripts/check-esp-idf.sh [chip]` instead of plain `cargo check` — see `src/io/CLAUDE.md` for the toolchain/Docker details.

## Verification Gate

Every change must compile under both the default `tokio` feature set and the `alloc`, `embassy`, and `no_std` library targets. Run `cargo clippy` as part of the verification gate. The `--lib` flag scopes the no_std check to library code only — the CLI is host-only. Use `#[cfg(not(feature = "std"))]` imports from `alloc` (String, Vec, format!) for no_std paths.

The `embassy` feature is not implied by `alloc` alone — `io/embassy.rs` and `#[cfg(feature = "embassy")]` code aren't exercised by the plain no_std/alloc check, so both commands above are required.

**Mock tests cannot verify wire-level write/read framing changes — see `.claude/rules/wire-framing-hardware-verification.md`.**

Run `make check-fast`/`make check-all`/`git commit`/ALL shell commands through `ctx_shell`, not `Bash`, when lean-ctx is connected — their output (multi-target cargo build/test/clippy, ESP-IDF check) is large and repetitive, and global CLAUDE.md's lean-ctx section routes all shell commands through `ctx_shell` — `git commit` firing the pre-commit hook is just the case where forgetting this is costliest, not a special exception to some other rule.

This section was kept separate from the raw command list above so the CI-live transition wouldn't tangle the two together — that transition has now happened (see Build & Test Commands above).

## Architecture

**bambino** is a multi-platform async Rust crate for controlling Bambu Lab 3D printers over LAN. It compiles to three targets from one codebase: host (tokio/rustls), ESP-IDF (std), and bare-metal (embassy/no_std).

### Key Invariants

1. **No direct platform I/O in library code.** All network I/O goes through abstract traits in `src/io/` (`AsyncIo`, `TlsConnector`, `RawStreamFactory`, `AsyncUdpSocket`, `TimerProvider`). Never use `tokio::` or `std::net::` outside `src/io/`. `TlsConnector` wraps an existing raw stream in TLS; `RawStreamFactory` dials a fresh pre-TLS stream to host:port (used for both MQTT's lazy connect and FTPS's data channel). `TimerProvider::now_millis()` provides monotonic clock for platform-agnostic timeouts.

2. **All model-specific behavior goes through the quirks engine.** Access via `model.quirks()` — never match on `BambuModel` variants for behavioral dispatch. Strategy structs live in `src/quirks/models/`.

3. **MQTT commands follow the Payload+Request pattern** (`src/mqtt/commands/` — split into `mod.rs` plus per-category files (`ams.rs`, `control.rs`, `gcode.rs`, `hardware.rs`, `print_job.rs`, `status.rs`) — and `src/diagnostics/kprofile.rs`):
   - A `#[derive(Serialize)]` payload struct with typed fields
   - A wrapper struct with a single `pub print: PayloadType` field (or `pushing:`, `system:`, `info:`)
   - An `impl` block with `pub fn new(...)` constructor

### Where Other Invariants Live

Non-obvious type decisions and behavioral invariants live close to the code they govern, not here — these aren't Key Invariants themselves, this is a routing note for where to find them:

- Cross-cutting invariants (span multiple non-adjacent `src/` paths) → `.claude/rules/*.md`, each scoped with a `paths:` frontmatter glob.
- Single-directory invariants → a nested `CLAUDE.md` in that directory (currently: `src/types/telemetry/`, `src/camera/`, `src/ftps/`, `src/io/`, `src/mqtt/client/`).
- Only truly global content (Key Invariants above, build/test commands, verification gate, architecture overview) stays here.

## Key Conventions

- `serde_json` is used with `default-features = false` — always use `serde_json::to_vec` (not `to_string`) for payloads.
- Library code uses the `log` crate facade (`log::debug!`, `log::trace!`, `log::warn!`) — never `println!`.
- `Error` has dual `Display` impls: `thiserror` under `std`, manual under `no_std` (kept in sync by `test_display_consistency`). `ProtocolViolation` uses `Cow<'static, str>`.
- Magic numbers are extracted into named `pub(crate) const` blocks in each module. Use existing constants rather than introducing new literals.
- Protocol specs live in `reference/` as numbered markdown files. Always verify field names and types against reference docs when adding or modifying commands — use `ctx_compose` for this cross-file check (reference doc + command file together) rather than reading each in full. When external sources (pybambu, Bambuddy, Bambu Studio, wire captures) contradict a reference doc, update the reference doc with the correction and note the verification source.
- Use MODEL_MATRIX.csv to track physical characteristics of printer models. When new information is **confirmed** about a printer model, update MODEL_MATRIX.csv
- CLI-only dependencies live behind the `cli` feature, not `tokio`. `crossterm`, `env_logger`, and any future CLI-exclusive dep (e.g. `clap`) must be gated by `cli = ["dep:...", "tokio"]`, never added to the `tokio` feature directly — see the feature comments in `Cargo.toml` for why. `[[bin]] required-features = ["cli"]` in Cargo.toml enforces this at the target level — every file under `src/bin/bambino-cli/` starts with `#![cfg(feature = "cli")]`.
- Never write an access code or serial number into a file in this repository (docs, tests, scratch files, commit messages) — treat them the same as any other credential.
- When adding public types, modules, traits, or changing conventions: decide the routing *at the time you add the invariant*, don't defer it. A new invariant spanning multiple non-adjacent `src/` paths goes to `.claude/rules/<topic>.md` with a `paths:` glob; one confined to a single directory is a candidate for that directory's nested `CLAUDE.md`; only genuinely global content (applies regardless of which files a session touches) goes in this file. Keep whichever file it lands in concise — document constraints and gotchas, not API summaries.
- Write real commit bodies for anything that would otherwise become a narrative bullet — the "why," not just the "what" — and delete a completed `*_PLAN.md` file in its own commit with a body describing what shipped and where the resulting invariant lives.
- Moving, renaming, or splitting a file changes what its `.claude/rules/*.md` globs match — `ctx_search` (not raw grep) `.claude/rules/` for the old path in the same commit and update every glob that references it. A glob that stops matching doesn't error, it just silently stops loading — worse than never moving the invariant out of root `CLAUDE.md` at all, since root always loads. Same check applies to a nested `<dir>/CLAUDE.md`: if the split moves an invariant's code out of that directory, move the bullet too.
- Full-crate review sweeps require the `deep-review` skill, not ad hoc agent orchestration. `deep-review` only stages findings in a dated review file — it never files GitHub Issues itself; run `triage-review` separately to convert a completed sweep into issues.
- Bug/finding tracking lives on GitHub Issues, not a file in this repo — the `backlog` skill has the issue format, label schema, and release bar; don't restate its rubric elsewhere.
- Phases in markdown planning documents must be self-contained — implementable by a clean session with zero prior conversation context beyond the existing codebase. When adding or altering a phase, briefly inform the next session of what we learned and guide it by spelling out: the problem being solved (not just what to build), design constraints and trade-offs that shape the implementation, ordering dependencies between items, and which items are trivially independent. If a task has a hard design decision, state the options and either pick one or mark it as "decide first." A phase that requires the executing session to reconstruct missing rationale or scope by reading git history or guessing at intent is underspecified.
- A prior commit/fix/doc comment being landed doesn't make it permanently correct — re-verify against new evidence (a newly-consulted source, new hardware access, a newer upstream commit) before citing old work as settled precedent, especially in a plan doc.
