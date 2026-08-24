# esp32-hw-probe

A small, reusable ESP-IDF Rust application used to answer questions about
`bambino` that **only real hardware can answer**. It is not part of the
`bambino` crate: it is a separate cargo package, it is never a dependency of
`bambino`, and it is excluded from the published crate via the root
`Cargo.toml`'s `exclude` entry.

## Why it exists

`bambino` is a `no_std`-capable async crate that targets host (tokio), ESP-IDF,
and bare-metal (embassy) from one codebase. Most of it is verified with mock
transports in `cargo test`, but a mock is only as accurate as our assumptions
about the wire and about the platform underneath it. Two classes of question
fall straight through that net:

1. **Wire-level framing.** Whether a write is split across TCP segments, how a
   real printer's firmware responds to a particular byte sequence, and where a
   read boundary actually lands are properties of the peer and the network
   stack — not of the mock. See
   [`.claude/rules/wire-framing-hardware-verification.md`](../.claude/rules/wire-framing-hardware-verification.md)
   for the standing rule that changes in this class require hardware
   verification before they can be called done.
2. **Platform runtime behavior.** Timer exhaustion, task stack sizes, mbedTLS
   memory pressure, `RefCell`/`Sync` constraints on shared ESP-IDF handles —
   these only show up on the chip, under the real ESP-IDF scheduler.

This directory is the standing harness for both: flash it, watch the serial
log, get an answer. Keeping it checked in means the next investigation starts
from a working ESP-IDF build rather than from `cargo generate` and an evening
of toolchain setup.

## What's in `src/main.rs`

Only the **current** investigation. It is deliberately not an accumulating test
suite — `git log -- esp32-hw-probe/src/main.rs` is the record of what has been
probed before (e.g. the BUG-051 timer-exhaustion stress test). To start a new
investigation, replace the body of `main` and keep the
`esp_idf_svc::sys::link_patches()` / logger-init boilerplate at the top.

`bambino` is a path dependency here on purpose: a probe should drive the
*shipped* type, because a reimplementation of it in this file can pass while
the real code still fails.

## Running it

```sh
cd esp32-hw-probe
cargo espflash flash --release --monitor 2>&1 | tee run.log
```

Requires a board connected over USB. The app spins forever after its test loop
finishes (standard ESP-IDF convention — `main` isn't meant to return), so the
monitor won't exit on its own; Ctrl-C detaches the monitor without resetting
the board.

To retarget a different chip, edit `.cargo/config.toml`'s `[build] target` and
`[env] MCU`:

| Chip | `target` | `MCU` |
|------|----------|-------|
| ESP32-C6 | `riscv32imac-esp-espidf` | `esp32c6` |
| ESP32-C3 | `riscv32imc-esp-espidf` | `esp32c3` |

`sdkconfig.defaults` holds task/stack-size overrides only and does not need
touching for a retarget.

See [`CLAUDE.md`](CLAUDE.md) in this directory for the same details in the form
the agent tooling consumes.
