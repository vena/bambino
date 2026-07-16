# Rename `BambuError` → `Error`

## Problem

`src/error.rs` defines `pub enum BambuError`, re-exported at `src/lib.rs:103` as
`bambino::BambuError`. This is non-idiomatic: Rust convention for a crate's single
top-level error type is a bare `Error`, disambiguated by path (`bambino::Error`,
same pattern as `std::io::Error`, `reqwest::Error`). The `Bambu`-prefixed name adds
no disambiguation the path doesn't already provide, and risks colliding on
unqualified `use bambino::*` with another Bambu-Lab-adjacent crate that also picked
`BambuError` (plausible in this small ecosystem — pybambu-alikes, Bambu Studio
wrappers).

Crate is pre-1.0 (see `Cargo.toml`), so this is a cheap breaking rename now, expensive
later. This is a pure rename — no behavior change, no new variants, no logic edits.

## Scope

Confirmed via `grep -rc 'BambuError' --include='*.rs' src tests` (34 files, ~300
occurrences total across the whole repo including generated docs):

- `src/error.rs` — the definition itself (enum, doc comments referencing the type by
  name, `Display` impls, `From` impls, `#[cfg_attr(... error(...))]` thiserror
  attributes reference the type only implicitly — fine).
- `src/lib.rs` — `pub use error::BambuError;` (line 103) and one doc-comment usage in
  the crate-level `//!` quick-start example (`Result<(), bambino::BambuError>`).
- All other `src/**/*.rs` and `tests/**/*.rs` hits are call sites: `Result<T, BambuError>`
  return types, `BambuError::Variant` constructions, `matches!(..., BambuError::...)`,
  and doc comments mentioning `BambuError`.
- `CLAUDE.md` (root) — one line under Key Conventions describing the dual `Display`
  impl.
- `src/camera/CLAUDE.md` — one line describing `rtsps::build_rtsps_url`'s return type.
- `README.md` — one prose mention (`Result<String, BambuError>`).
- `07-16-REVIEW.md` — historical review note, references `BambuError::ModelMismatch`.
  Leave as-is: it's a dated record of what was true when written, not living docs.
  Do not edit it as part of this rename (do not rename the finding's contents to match
  post-rename code).

**`docs/` is entirely generated** by `make docs` (`Makefile:45-55`, runs
`cargo docs-md` against `target/doc/bambino.json` then `scripts/strip-doc-noise.py`).
Do not hand-edit any file under `docs/` — regenerating it is the last step of this
plan, not a per-file editing target.

**Do not touch**: `crate::io::SocketError`, `crate::io::TimerError`,
`io::tokio::TokioIoError`, `io::esp_idf::EspIdfIoError` — these are separate
platform-level error types that convert *into* `BambuError`/`Error` via `From` impls;
they are out of scope and keep their existing names.

## Why a plain rename, not a re-export shim

CLAUDE.md's own convention: "Don't use feature flags or backwards-compatibility shims
when you can just change the code." Crate is pre-1.0 with no published consumers on a
registry (no GitHub remote yet, per project memory) — no deprecation window needed.
Do **not** add `pub use error::BambuError as Error` or a `#[deprecated]` type alias.
Rename cleanly.

## Steps

1. **Rename the definition** in `src/error.rs`:
   - `pub enum BambuError` → `pub enum Error`
   - Every `BambuError::Variant` inside the file (the `From` impls, the manual
     `no_std` `Display` impl's `match` arms, the `#[cfg(test)]` module's
     `assert_all_variants_covered` match and `test_display_consistency` /
     `test_from_socket_error` / `test_from_timer_error` /
     `test_protocol_violation_from_static_str` /
     `test_protocol_violation_from_dynamic_string` / `test_bambu_error_is_clone`
     test bodies) → `Error::Variant`.
   - Update the module doc comment (`//! [`BambuError`] is the single error type...`)
     and the type's own doc comment (`/// Unified error type for the \`bambino\`
     crate.`) to say `Error` instead of `BambuError`. Keep the substance of both
     comments — this is a rename, not a rewrite.
   - The two `impl From<...> for BambuError` blocks (from `SocketError`,
     `TimerError`) → `impl From<...> for Error`.

2. **Update the re-export** in `src/lib.rs`:
   - `pub use error::BambuError;` (line 103) → `pub use error::Error;`
   - Fix the crate-level doc example (`async fn example() -> Result<(),
     bambino::BambuError>`) → `bambino::Error`.

3. **Sweep all remaining `src/**/*.rs` and `tests/**/*.rs` call sites.** Every hit is
   one of:
   - `Result<T, BambuError>` / `Result<T, crate::error::BambuError>` in fn signatures
     → `Result<T, Error>` / `Result<T, crate::error::Error>`
   - `BambuError::Variant(...)` construction → `Error::Variant(...)`
   - `matches!(x, BambuError::...)` → `matches!(x, Error::...)`
   - doc comments (`/// ... [`BambuError`] ...`) mentioning the type by name
   - Import lines (`use crate::error::BambuError` or similar, if any — check for
     these specifically, `error.rs`'s own module doesn't import itself but call sites
     might alias it)

   Use a project-wide find/replace of the exact token `BambuError` → `Error` across
   `src/` and `tests/` (word-boundary match — `BambuError` doesn't collide with any
   other identifier in this crate, confirmed no `XBambuError`/`BambuErrorY`-style
   variants exist). A plain `sed -i '' 's/\bBambuError\b/Error/g'` over the file list
   from `grep -rl 'BambuError' --include='*.rs' src tests` is sufficient — there is no
   contextual judgment needed per-occurrence, unlike a typical rename where some hits
   are prose and need rewording.

   File list (34 files) to sweep, from `grep -rl 'BambuError' --include='*.rs' src
   tests`:
   ```
   src/bin/bambino-cli/camera.rs
   src/bin/bambino-cli/connection.rs
   src/bin/bambino-cli/control.rs
   src/bin/bambino-cli/discover.rs
   src/bin/bambino-cli/inspect_cert.rs
   src/bin/bambino-cli/monitor/mod.rs
   src/bin/bambino-cli/probe.rs
   src/bin/bambino-cli/storage.rs
   src/bin/bambino-cli/verify_tls.rs
   src/camera/binary.rs
   src/camera/rtsps.rs
   src/client/ams.rs
   src/client/camera.rs
   src/client/connect.rs
   src/client/hardware.rs
   src/client/mod.rs
   src/client/motion.rs
   src/client/print.rs
   src/client/storage.rs
   src/client/telemetry.rs
   src/client/thermal.rs
   src/diagnostics/kprofile.rs
   src/discovery/mod.rs
   src/error.rs               (already covered in step 1 — sed is idempotent either way)
   src/ftps/client.rs
   src/ftps/protocol.rs
   src/ftps/protocol/tests.rs
   src/io/mod.rs
   src/lib.rs                 (already covered in step 2)
   src/mqtt/client/frame.rs
   src/mqtt/client/mod.rs
   tests/camera_test.rs
   tests/client_test.rs
   tests/ftps_test.rs
   tests/mqtt_test.rs
   ```

4. **Update prose docs** (not code, so not covered by the sed sweep — check each by
   hand, these are short single-line mentions):
   - `CLAUDE.md` line ~62: `- \`BambuError\` has dual \`Display\` impls: ...` →
     `\`Error\``. Keep the rest of the sentence (dual-impl / `test_display_consistency`
     / `ProtocolViolation` content) unchanged — it's still accurate post-rename.
   - `src/camera/CLAUDE.md`: `**\`camera::rtsps::build_rtsps_url\`** returns
     \`Result<String, BambuError>\`, not \`String\` — ...` → `Result<String, Error>`.
   - `README.md` line ~225: `Result<String, BambuError>.` → `Result<String, Error>.`
   - Leave `07-16-REVIEW.md` untouched (see Scope above — it's a historical record).

5. **Regenerate `docs/`** — do not hand-edit anything under `docs/`. Run `make docs`
   (requires `cargo-docs-md` installed per the Makefile comment; if unavailable in
   the executing session, note that in the completion summary rather than hand-editing
   generated files — hand-editing would just be overwritten by the next real `make
   docs` run and isn't a substitute).

6. **Verify.** Run `make check-fast` (per CLAUDE.md: build, test, both no_std/embassy
   feature-gate checks, clippy, in one command). This must pass clean — a plain
   rename should produce zero behavior change, so any compile or test failure means a
   call site was missed by the sed sweep (most likely: a `use` alias like `use
   crate::error::BambuError as SomeOtherName`, which sed's word-boundary match would
   still catch on the `BambuError` token, but re-check if `make check-fast` fails with
   an unresolved-name error rather than trusting the sweep was exhaustive).
   Run this via `ctx_shell`, not `Bash`, per this repo's lean-ctx routing convention —
   `make check-fast` output is large (multi-target build/test/clippy).

7. **Grep for stragglers** before considering this done:
   `grep -rn 'BambuError' .` from repo root, excluding `07-16-REVIEW.md` (intentionally
   left) and `docs/` (regenerated in step 5 — if step 5 was skippable due to missing
   `cargo-docs-md`, `docs/` will still contain stale `BambuError` mentions from the old
   generation; note this explicitly rather than treating it as a missed sweep).

8. **Commit.** Single commit, this is one atomic rename. Suggested subject:
   `rename BambuError to Error`. Body: one line noting the motivation (path-qualified
   naming, `bambino::Error` matches ecosystem convention, avoids `Bambu`-prefix
   collision risk with other Bambu-Lab-adjacent crates) — don't restate the full
   investigation, just the "why" in a sentence or two, per CLAUDE.md's commit-body
   guidance.

## Non-goals

- No new error variants, no behavior change, no splitting `Error` into
  per-subsystem error types (that question was raised and explicitly rejected during
  the investigation that preceded this plan — single flat enum is intentional for this
  no_std/embedded-target crate, keep it).
- No compatibility alias/shim (see "Why a plain rename" above).
- No edits to `io::SocketError`, `io::TimerError`, `TokioIoError`, `EspIdfIoError`, or
  any other type name.
