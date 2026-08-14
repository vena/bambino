# esp32-hw-probe

Reusable ESP-IDF Rust app scaffolding for hardware questions that
`src/io/esp_idf.rs`'s mock tests can't answer — see
`.claude/rules/wire-framing-hardware-verification.md` in the repo root for why
this class of question exists. Excluded from `bambino`'s published crate via
its `Cargo.toml` `exclude` entry — never a dependency of `bambino` itself.

**To reuse for a new investigation:** replace `src/main.rs`'s body with the new
test logic (keep the `esp_idf_svc::sys::link_patches()` / logger-init
boilerplate at the top). Reach for `bambino` itself — it's a path dependency
here, so a probe can drive the shipped type rather than a copy of it, and a
copy is exactly what can pass while the real code still fails. Note that
`&self`-taking types holding a `RefCell` (e.g. `EspIdfTimer`) aren't `Sync`, so
"two callers at once" has to mean two futures on one executor, not two threads;
`embassy-futures`' `join`/`select` are already dependencies for that.

Don't accumulate old investigations' logic here —
`git log -- esp32-hw-probe/src/main.rs` is the record of what's been tested
before (e.g. the BUG-051 timer-exhaustion stress test), the file itself should
only ever hold the *current* investigation.

**Retargeting chips:** edit `.cargo/config.toml`'s `[build] target` (the target
triple, not just an env var) and `[env] MCU`:

| Chip | `target` | `MCU` |
|------|----------|-------|
| ESP32-C6 | `riscv32imac-esp-espidf` | `esp32c6` |
| ESP32-C3 | `riscv32imc-esp-espidf` | `esp32c3` |

`sdkconfig.defaults` doesn't need touching for a chip retarget — it only holds
task/stack-size overrides, not the target selection.

**Flash and run:** `cd esp32-hw-probe && cargo espflash flash --release --monitor`
(board connected over USB; check `espflash --help` if the exact flag differs
from the installed `espflash`/`cargo-espflash` version). The app spins forever
after its test loop finishes (standard ESP-IDF convention — `main` isn't meant
to return), so the monitor won't exit on its own; Ctrl-C to detach once you see
the loop's final summary line, this only detaches the monitor and doesn't reset
the board. Pipe through `tee` to capture output to a file, since these logs can
get long: `cargo espflash flash --release --monitor 2>&1 | tee run.log`.
