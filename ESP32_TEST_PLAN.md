# ESP32 Timer Stress Test — Plan (BUG-051)

## What this resolves

`BACKLOG.md` BUG-051 (`needs-verification`): `EspIdfTcpStream::connect()`
(`src/io/esp_idf.rs:492`) and `EspIdfTlsConnector::connect()` (`src/io/esp_idf.rs:692`) each
allocate their own `EspIdfTimer` — and therefore their own
`esp_idf_svc::timer::EspTimerService::<Task>::new()` — instead of sharing one across a
connect sequence. Every `esp_timer` handle is a real FreeRTOS/ESP-IDF kernel resource with a
platform-enforced cap (`CONFIG_ESP_TIMER_TASK_STACK_SIZE`-adjacent limits, exact cap varies by
`sdkconfig`). The question BUG-051 asks: does a connection-heavy workload (FTPS opening a
fresh data channel per transfer, MQTT reconnecting repeatedly) ever exhaust that cap, or does
each `EspTimerService` get cleanly freed on drop with no leak/contention in practice? This
can only be answered by watching real allocation/drop behavior on real hardware — mock tests
have no access to the underlying ESP-IDF timer service at all.

**This plan produces a standalone test binary, not a change to `bambino` itself.** Nothing in
`src/` changes as part of this plan — only if Phase 4's results confirm a real problem does a
follow-up (sharing the timer) get scoped, and that's out of this plan's scope on purpose (no
point designing a fix for a problem not yet confirmed to exist).

## Ground rules for every phase

- Per `.claude/rules/wire-framing-hardware-verification.md`'s pattern (this is the same class
  of thing even though it's resource exhaustion, not wire framing): **whoever executes Phase
  3 onward must not claim results without actually running them on hardware.** If you are an
  agent session picking this up, do Phases 1-2 (they're pure code, no hardware needed), then
  stop and hand Phase 3+ back to the user with clear flash/run instructions — don't simulate
  or guess at what the serial monitor would show.
- The user has both an **ESP32-C6** and an **ESP32-C3** available to test on. Run the full
  Phase 3-4 cycle on both — BUG-051's note about "more powerful AP controllers" (X1/H2-series,
  which use a different, more capable SoC than the C3/C6 this crate currently targets) doesn't
  apply here; both boards are the same RISC-V/`esp_timer` architecture this bug is actually
  about, so testing both is about chip-to-chip variance in the timer slot cap, not a materially
  different question.
- Test design deliberately avoids WiFi/networking entirely (see Phase 2) — this isolates the
  actual resource in question (`esp_timer` handle allocation) without the added scaffolding
  and failure surface of joining a LAN, dialing a real or mock printer, etc. If Phase 4's
  results are ambiguous, a networked version exercising the *actual* `EspIdfTcpStream::connect`
  / `EspIdfTlsConnector::connect` code paths is the fallback — see Phase 5 (optional).
- New scaffolding lives in a new top-level directory, `esp32-hw-probe/`, as its own
  independent Cargo package (**not** a `[[bin]]` inside `bambino`'s own `Cargo.toml`) — see
  Phase 1 for why.
- **This scaffolding must never reach a `bambino` consumer, but it does stay in the repo.**
  It's a reusable diagnostic harness for `src/io/esp_idf.rs` hardware questions in general
  (BUG-051 is just the first tenant), not a one-off deleted after use — see `esp32-hw-probe/`'s
  own `README.md` (Phase 1 step 7) for the reuse convention. A library consumer who doesn't
  need it (or already has their own ESP-IDF app scaffolding) should still never get it bundled
  into what they pull from crates.io, though — that's a separate concern from whether it stays
  checked into the repo, and is handled purely at publish time: `esp32-hw-probe/` is a
  git-tracked subdirectory of `bambino`'s own repo root, so `cargo package`/`cargo publish`
  would bundle it into the published tarball by default (cargo's default packaging scope is
  "everything git-tracked under the crate root," independent of Cargo *workspace* membership
  — a nested `Cargo.toml` doesn't exclude itself). Phase 1 step 1 adds a permanent `exclude`
  entry to `bambino`'s own `Cargo.toml` before the directory is ever populated — this entry is
  not removed once BUG-051 closes, since the directory itself isn't going away.

---

## Phase 1 — Scaffold a flashable ESP-IDF Rust binary

**No hardware needed for this phase.** Can be done and verified (via `cargo build`, not
flashing) by a clean session with no prior context.

### Why a separate package, not a `bambino` `[[bin]]`

`bambino`'s existing `[[bin]]` (`bambino-cli`) requires the `cli` feature, which requires
`tokio` — incompatible with `esp-idf` (a different, mutually-exclusive runtime target). A real
flashable ESP-IDF app also needs infrastructure `bambino`'s `Cargo.toml` doesn't have and
shouldn't gain just for a one-off test: `esp-idf-sys` as a *build-time* dependency (not just
`esp-idf-svc`, already present), a `build.rs` invoking `embuild`, `sdkconfig.defaults`, and a
`.cargo/config.toml` target/linker configuration specific to the ESP-IDF app-image format
(different from a plain `cargo check` of library code, which is all `scripts/check-esp-idf.sh`
currently does). Keeping this in its own package avoids entangling any of that with
`bambino`'s own build.

### Steps

1. **Before creating the directory**, add the publish-exclusion entry to `bambino`'s own
   `Cargo.toml` `[package]` section (currently has no `exclude`/`include` field at all — add
   one):
   ```toml
   exclude = ["esp32-hw-probe/"]
   ```
   Commit this on its own, or as the first change in the same commit that adds the directory
   — either way, it must land no later than the directory itself, never after.
2. Confirm the local machine has the ESP Rust toolchain installed natively (not just Docker —
   flashing over USB from inside a Docker container is a separate can of worms, especially on
   macOS, and not worth solving for a one-off test):
   ```sh
   cargo install espup --locked  # if not already installed
   espup install                 # installs the esp Rust toolchain + exports
   . $HOME/export-esp.sh         # or wherever espup printed it, needed in every new shell
   cargo install espflash --locked
   ```
   If `espup`/`espflash` are already present (check `espflash --version`), skip straight to
   step 3.
3. Generate the project via the standard template rather than hand-writing scaffolding from
   scratch (less error-prone, matches what any other ESP-IDF Rust project looks like):
   ```sh
   cargo install cargo-generate --locked   # if not already installed
   cargo generate esp-rs/esp-idf-template cargo
   ```
   When prompted: project name `esp32-hw-probe`, place it at the repo root
   (`/Users/vena/Documents/Projects/Personal/bambino/esp32-hw-probe/`), MCU `esp32c6` for
   the first pass (Phase 3 repeats the whole cycle with `esp32c3` — either regenerate with
   `esp32c3` selected, or see step 5's note on retargeting the same project instead of
   generating twice).
4. Add `esp-idf-svc` as a dependency in `esp32-hw-probe/Cargo.toml` matching the same
   version `bambino`'s own `Cargo.toml` pins (`0.52.1` — check `Cargo.toml`'s
   `esp-idf-svc = { version = "0.52.1", ... }` line for the current value before assuming
   it's unchanged since this plan was written).
5. To retarget between `esp32c6` and `esp32c3` without regenerating the whole project: edit
   `esp32-hw-probe/.cargo/config.toml`'s `[build] target` line (`riscv32imac-esp-espidf`
   for C6, `riscv32imc-esp-espidf` for C3 — note the target triple itself differs, not just an
   env var, per `scripts/check-esp-idf.sh`'s existing `CHIP`→`TARGET` mapping) and
   `sdkconfig.defaults`' `CONFIG_IDF_TARGET` line.
6. Verify the generated template builds as-is before touching any test logic:
   ```sh
   cd esp32-hw-probe && cargo build --release
   ```
   This alone needs no hardware — just confirms the toolchain/scaffolding is sound. If it
   fails, fix the scaffolding here before moving to Phase 2; don't debug template issues mixed
   in with test-logic issues later.
7. Add `esp32-hw-probe/README.md` establishing the reuse convention referenced in the ground
   rules above, since this harness is meant to outlive BUG-051:
   ```markdown
   # esp32-hw-probe

   Reusable ESP-IDF Rust app scaffolding for hardware questions that
   `src/io/esp_idf.rs`'s mock tests can't answer — see
   `.claude/rules/wire-framing-hardware-verification.md` in the repo root for why
   this class of question exists. Excluded from `bambino`'s published crate via
   its `Cargo.toml` `exclude` entry — never a dependency of `bambino` itself.

   **To reuse for a new investigation:** replace `src/main.rs`'s body with the new
   test logic (keep the `esp_idf_svc::sys::link_patches()` / logger-init
   boilerplate at the top). Don't accumulate old investigations' logic here —
   `git log -- esp32-hw-probe/src/main.rs` is the record of what's been tested
   before, the file itself should only ever hold the *current* investigation.
   Retarget chips via `.cargo/config.toml`'s `target` line and
   `sdkconfig.defaults`' `CONFIG_IDF_TARGET` (see `ESP32_TEST_PLAN.md`'s git
   history for the BUG-051 investigation this was built for, and the pattern to
   follow for the next one — that plan doc itself gets deleted once its
   investigation closes, per repo convention, so check history rather than
   expecting to find it checked in).
   ```

---

## Phase 2 — Write the stress-test logic

**No hardware needed for this phase either** — `cargo build --release` is the acceptance
check; running it is Phase 3.

Replace the template's generated `src/main.rs` body (keep its `esp_idf_svc::sys::link_patches()`
/ logger-init boilerplate at the top — every ESP-IDF Rust app needs that) with:

1. A loop running a large, fixed iteration count — start with `10_000` (order-of-magnitude
   above any realistic single print-job session's FTPS transfer count, per BUG-051's own
   "connection-heavy workloads" framing; adjust upward in Phase 4 if 10,000 completes cleanly
   and you want more confidence margin).
2. **Mirror the real allocation shape from `src/io/esp_idf.rs`, not a simplified one-timer
   loop** — BUG-051 is specifically about *two* separate `EspTimerService`s per connect
   sequence (one in `EspIdfTcpStream::connect`, dropped when `EspTls::adopt` consumes the raw
   stream; one in `EspIdfTlsConnector::connect`, held for the handshake). Each loop iteration
   should therefore:
   - Create `EspTimerService::<Task>::new()` (timer A), call `.timer_async()`, hold it briefly
     (a short `.after(Duration::from_millis(1)).await` mirrors real dial-polling), then drop
     it explicitly (`drop(timer_a)`) — mirroring the dial-phase timer's lifetime.
   - Create a second `EspTimerService::<Task>::new()` (timer B) the same way, held slightly
     longer (mirrors the handshake-phase timer persisting on `EspTlsStream`), then drop it too.
   - This can use `esp_idf_svc::timer::EspTimerService`/`EspAsyncTimer` directly — no need to
     depend on `bambino` itself from this package; duplicating the ~15 lines this needs keeps
     `esp32-hw-probe` fully independent of `bambino`'s own build.
3. **Log every iteration's outcome immediately, not just at the end** — `log::info!("iter
   {i}: ok")` on success, and on any `Result::Err` from either `EspTimerService::new()` call,
   `log::error!("iter {i}: FAILED: {e:?}")` and break the loop rather than panicking. The
   point is to know *which* iteration failed (if any), not just that the run as a whole didn't
   reach 10,000 — a panic would still show up in the serial monitor but loses the clean
   "reached iteration N before failing" signal.
4. Print a final summary line (`log::info!("completed {i} iterations, {failures} failures")`)
   so Phase 4's read of the serial monitor output doesn't require scrolling back through
   10,000 "ok" lines to find the answer.
5. After the loop, spin forever (`loop { esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000); }`)
   rather than letting `main()` return — matches every ESP-IDF Rust example's convention (an
   ESP-IDF app returning from `main` is undefined-ish behavior, not a clean exit).

**Acceptance for this phase:** `cargo build --release` succeeds, no hardware involved yet.

---

## Phase 3 — Flash and run (user-executed only, not an agent session)

Repeat this entire phase twice: once with the project targeting `esp32c6`, once retargeted to
`esp32c3` (Phase 1 step 5 covers how to retarget without regenerating).

1. Connect the board over USB.
2. `cargo espflash flash --release --monitor` (or `espflash flash --release --monitor
   --target-app-partition` depending on the exact `cargo-espflash` vs `espflash` CLI installed
   — check `espflash --help` if the exact flag differs from this) from inside
   `esp32-hw-probe/`. This flashes and immediately opens the serial monitor so the log
   output from Phase 2 is visible live.
3. Let it run to completion (10,000 iterations at ~1-2ms each should finish in well under a
   minute; adjust your wait accordingly if you raised the iteration count).
4. Save the full serial monitor output (redirect `--monitor`'s output to a file, or copy the
   terminal scrollback) — Phase 4 needs the actual failure point (if any), not just a
   pass/fail summary.
5. Power-cycle the board and repeat once more per chip (BUG-042's investigation found that a
   single run can miss state that only shows up after a fresh boot — cheap insurance here
   given how quick each run is).

---

## Phase 4 — Interpret results and close out BUG-051

Back in a normal (non-hardware) session, with the saved serial monitor output in hand:

- **Either way, once BUG-051 itself is resolved** (Wontfix or a scoped follow-up per the two
  outcomes below): delete `ESP32_TEST_PLAN.md` (matches the completed-`*_PLAN.md` convention
  in `CLAUDE.md`'s Key Conventions) — but **do not delete `esp32-hw-probe/`**, it's a kept
  reusable harness (see the ground rules above and its own `README.md`). Instead, reset
  `esp32-hw-probe/src/main.rs` to a minimal placeholder (a comment pointing at
  `esp32-hw-probe/README.md` and `git log` for prior investigations, no live test logic) so
  the next investigation starts from a clean slate rather than inheriting BUG-051's loop. The
  `Cargo.toml` `exclude` entry from Phase 1 step 1 stays permanently, unlike the directory's
  contents.
- **If both boards complete all iterations with zero failures across both runs each:** this
  confirms BUG-051's concern doesn't materialize in practice — `EspTimerService::new()`/drop
  cycles clean up correctly and don't exhaust the slot cap even at 10,000+ iterations, an order
  of magnitude beyond realistic usage. Move BUG-051 from `Open`/`needs-verification` to
  `Wontfix` in `BACKLOG.md`, with a one-line reason citing this test (iteration count, chips
  tested, zero failures) — no code change to `bambino` itself is warranted.
- **If either board fails at some iteration count:** BUG-051 is confirmed real. Reassign it
  from `needs-verification` to a real severity (Sev3 unless the failure mode is worse than
  "connect eventually fails" — re-read `backlog` skill's severity rubric before picking), and
  record the failing iteration count and exact error per chip in `BACKLOG.md`'s `Detail`
  column. Scope a follow-up fix as a **new**, separate piece of work (share one `EspIdfTimer`
  across `EspIdfTcpStream::connect` + `EspIdfTlsConnector::connect` for a given connect
  sequence, threading it through instead of each allocating its own) — don't design that fix
  speculatively as part of this plan; do it once the failure mode is known, since the exact
  fix shape may depend on what actually fails (e.g. if only the dial-phase timer's short lease
  ever exhausts the cap, a smaller/different fix than "share both" might suffice).
- **If results differ between the two chips:** note the chip-specific difference explicitly
  in `BACKLOG.md` — e.g. it may turn out the C3's smaller default `sdkconfig` timer-task stack
  size hits the cap sooner than the C6's. Don't average or pick one chip's result as
  representative of both.

---

## Phase 5 — Optional: networked version, only if Phase 4 is ambiguous

Not needed if Phase 4 gives a clean answer either way. If the standalone timer-loop test
completes cleanly but a real-world FTPS-heavy session still seems to misbehave in the field
(some other symptom BUG-051 didn't originally predict), a version of this test that actually
joins the LAN and calls `bambino`'s real `EspIdfTcpStream::connect`/`EspIdfTlsConnector::connect`
in a loop against a real reachable service would be the next-most-faithful test — at that
point `esp32-hw-probe` would need to depend on `bambino` itself (with the `esp-idf`
feature) plus WiFi credentials and a target IP, which is why Phase 1-4 deliberately don't
build that from the start: it's substantially more scaffolding for a question the simpler
timer-only test can very likely already answer on its own.
