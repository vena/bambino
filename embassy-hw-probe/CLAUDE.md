# embassy-hw-probe

Flashable harness for hardware questions about `src/io/embassy.rs` — the one
backend that has never run on real hardware (GitHub issue #292). Excluded from
`bambino`'s published crate via the root `Cargo.toml` `exclude` entry; never a
dependency of `bambino` itself.

**This is not `esp32-hw-probe` retargeted, and must not become that.** The two
cannot be one package: `esp32-hw-probe` builds for `riscv32imac-esp-espidf`
against `bambino`'s `esp-idf` feature (std, ESP-IDF SDK), this one builds for
`riscv32imac-unknown-none-elf` against the `embassy` feature (no_std, esp-hal +
esp-radio + embassy-net). Different target triple, different runtime, mutually
exclusive feature sets. Same board though — one ESP32-C6 runs both.

**To reuse for a new investigation:** replace the stage bodies in
`src/main.rs`, keeping the bring-up above stage 1 (heap, `esp_rtos::start`,
Wi-Fi, embassy-net, TRNG, the single `Tls`). That bring-up is most of the file
and is the part that took the work; the stages are the cheap part. Same
convention as `esp32-hw-probe`: the file holds only the *current*
investigation, and `git log -- embassy-hw-probe/src/main.rs` is the record of
what has been probed before.

**Version pinning is not free choice.** esp-rtos 0.3 and esp-radio 0.18 (the
newest stable releases of each) both pin `esp-hal ~1.1`, and the graph has to
land on one `esp-sync` minor, which is what fixes esp-alloc/esp-println/
esp-backtrace/esp-bootloader-esp-idf at the versions in `Cargo.toml`. Bumping
one of them alone produces a resolver error, not a duplicate build. The
comments in `Cargo.toml` carry the details — read them before upgrading
anything.

`mbedtls-rs`'s `esp32c6` feature (hardware-accelerated crypto) is deliberately
**off**: it pulls `esp-hal ~1.2`, which forces esp-rtos 0.4 + esp-radio
1.0.0-beta. Any handshake timing measured here is therefore a software-crypto
floor and is *not* comparable to the ESP-IDF probe's accelerated figures in
`src/io/CLAUDE.md`. Revisit when esp-radio 1.0 ships.

**Build gate:** `make check-embassy-probe` (from the repo root) builds this for
the bare-metal target with placeholder credentials. It needs
`rustup target add riscv32imac-unknown-none-elf` and nothing else — no Docker,
no ESP-IDF SDK. It is a `cargo build`, not a `check`, on purpose: MbedTLS's
X.509 code references `memchr`, which the bare-metal sysroot does not supply, so
that class of failure appears only at link time (hence the `tinyrlibc`
dependency).

**Credentials:** copy `.env.example` to `.env` (gitignored) and fill it in.
`build.rs` compiles the values in via `env!`; all five keys are required here,
unlike `esp32-hw-probe` where the access code is optional, because this probe's
MQTT and FTPS stages authenticate. The serial and access code are credentials —
never paste a run's log into the repo without scrubbing them.

**Flash and run:** `cd embassy-hw-probe && cargo run --release` (the target's
`runner` is `espflash flash --monitor`), or `cargo espflash flash --release
--monitor 2>&1 | tee run.log`. The probe loops forever after its last stage, so
the monitor won't exit on its own; Ctrl-C detaches it without resetting the
board.

**Retargeting chips:** edit `.cargo/config.toml`'s `[build] target` and swap the
`esp32c6` feature on `esp-hal`, `esp-rtos`, `esp-radio`, `esp-backtrace`,
`esp-bootloader-esp-idf` and `esp-println` together — they must all name the
same chip.

| Chip | `target` | feature |
|------|----------|---------|
| ESP32-C6 | `riscv32imac-unknown-none-elf` | `esp32c6` |
| ESP32-C3 | `riscv32imc-unknown-none-elf` | `esp32c3` |

`mbedtls-rs-sys` ships prebuilt libraries for both of those triples; a triple it
doesn't cover would need a C toolchain to build MbedTLS on the fly.

**Don't self-verify.** The same rule as
`.claude/rules/wire-framing-hardware-verification.md` applies to everything this
probe measures: an agent that edits this file has not run it. Hand the transcript
to the user and let the results come back from a real board.
