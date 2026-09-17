# embassy-hw-probe

A bare-metal Rust application that runs `bambino`'s **embassy** backend on a
real ESP32-C6, against a real printer. It is not part of the `bambino` crate:
separate cargo package, never a dependency of `bambino`, excluded from the
published crate via the root `Cargo.toml`'s `exclude` entry.

## Why it exists

`bambino` compiles to three targets — host (tokio), ESP-IDF (std), and
bare-metal (embassy/no_std). Two of those have run on real hardware. The
embassy backend never has: its verification to date is mock tests plus
`tests/embassy_tls_version_test.rs`, which compiles the real
`EmbassyTlsConnector` on the host and drives a loopback handshake against a
rustls server. That proves the `mbedtls-rs` wiring and the `negotiated_version`
mapping, and nothing about the embedded stack — no esp-hal, no esp-radio, no
embassy-net, no printer.

This is the harness that closes that gap. See
[issue #292](https://github.com/vena/bambino/issues/292).

## Why not reuse `esp32-hw-probe`

[`esp32-hw-probe/`](../esp32-hw-probe) is the equivalent harness for the ESP-IDF
backend, and the two cannot be one package. It builds for
`riscv32imac-esp-espidf` against `bambino`'s `esp-idf` feature, which is
std-on and needs the ESP-IDF SDK. This one builds for
`riscv32imac-unknown-none-elf` against the `embassy` feature, which is no_std
and brings its own stack (esp-hal + esp-radio + embassy-net). Different target
triple, different runtime, mutually exclusive feature sets. The same ESP32-C6
board runs both.

## What it does

`src/main.rs` runs six stages in order and logs each one with the heap
headroom at that point. Stages 0-2 are stack bring-up and the low-level I/O
traits; stage 3 onward drives the same public API a consumer would use.

| Stage | What it proves |
|-------|----------------|
| 0 | Wi-Fi associates and embassy-net gets a DHCP lease |
| 1 | `EmbassyRawStreamFactory` dials the printer (plain TCP, pre-TLS) |
| 2 | `EmbassyTlsConnector` completes a real handshake; `negotiated_version` reports it |
| 3 | `PrinterClient::connect_mqtt` plus one decoded telemetry event |
| 4 | `FtpsClient::connect` plus one `list_directory` |
| 5 | `EmbassyTimer` is monotonic and `sleep` paces correctly |

Most of the code is bring-up that runs *before* any `bambino` code does, which
is why the stage number matters more than the final line: it tells you whether
the crate under test was even reached.

Three TLS connectors share one `mbedtls_rs::Tls` instance here (MQTT, FTPS
control, FTPS data), which is exactly the case `EmbassyTlsConnector`'s
`TlsReference` design exists for — MbedTLS permits only one instance per
process.

`bambino` is a path dependency on purpose: a probe should drive the *shipped*
types, because a reimplementation of them in this file can pass while the real
code still fails.

## Running it

```sh
cp .env.example .env       # then fill in SSID, password, printer IP, serial, access code
cargo run --release        # runner is `espflash flash --monitor`
```

Requires a board connected over USB and
`rustup target add riscv32imac-unknown-none-elf`. No Docker and no ESP-IDF SDK —
that is a difference from `esp32-hw-probe`, not an omission here.

`.env` is gitignored and its values are compiled into the binary by `build.rs`.
The serial and the access code are credentials: don't paste an unscrubbed run
log into the repo.

The probe loops forever after its last stage (standard bare-metal convention —
`main` never returns), so the monitor won't exit on its own; Ctrl-C detaches it
without resetting the board. Pipe through `tee` to keep the transcript:

```sh
cargo espflash flash --release --monitor 2>&1 | tee run.log
```

## Build-only check

From the repo root:

```sh
make check-embassy-probe
```

Builds this crate for the bare-metal target with placeholder credentials. It is
a build rather than a check because MbedTLS's X.509 code references `memchr`,
which the bare-metal sysroot does not provide — a link-time failure that
`cargo check` cannot see (hence the `tinyrlibc` dependency).

## Memory

`mbedtls-rs` allocates 16 KiB in + 16 KiB out per TLS session by default, and
three sessions are live by stage 4. That, not CPU, is the first thing expected
to run out on a C6. `src/main.rs`'s heap sizes are the knob; the
`ssl-in-content-len-<N>`/`ssl-out-content-len-<N>` features on `mbedtls-rs` are
the other. Numbers measured here belong back in the `mbedtls-rs` dependency
comment in the root `Cargo.toml`.

## Versions

The esp-hal/esp-rtos/esp-radio/esp-sync versions in `Cargo.toml` are one
coherent set fixed by upstream's own requirements, not preferences — see the
comments there before upgrading any of them. Hardware-accelerated crypto
(`mbedtls-rs`'s `esp32c6` feature) is deliberately off because enabling it
forces a prerelease esp-radio; timing measured here is a software-crypto floor.

See [`CLAUDE.md`](CLAUDE.md) in this directory for the same details in the form
the agent tooling consumes.
