# embassy-hw-probe

A bare-metal Rust application that runs `bambino`'s **embassy** backend on a
real ESP32-C6, against a real printer. It is not part of the `bambino` crate:
separate cargo package, never a dependency of `bambino`, excluded from the
published crate via the root `Cargo.toml`'s `exclude` entry.

## Why it exists

`bambino` compiles to three targets — host (tokio), ESP-IDF (std), and
bare-metal (embassy/no_std). Until this harness, the embassy backend had never
run on real hardware: its verification was mock tests plus
`tests/embassy_tls_version_test.rs`, which compiles the real
`EmbassyTlsConnector` on the host and drives a loopback handshake against a
rustls server. That proves the `mbedtls-rs` wiring and the `negotiated_version`
mapping, and nothing about the embedded stack — no esp-hal, no esp-radio, no
embassy-net, no printer.

This harness closed that gap ([issue #292](https://github.com/vena/bambino/issues/292))
and is now where every hardware question about the embassy backend gets
answered.

## Why not reuse `esp32-hw-probe`

[`esp32-hw-probe/`](../esp32-hw-probe) is the equivalent harness for the ESP-IDF
backend, and the two cannot be one package. It builds for
`riscv32imac-esp-espidf` against `bambino`'s `esp-idf` feature, which is
std-on and needs the ESP-IDF SDK. This one builds for
`riscv32imac-unknown-none-elf` against the `embassy` feature, which is no_std
and brings its own stack (esp-hal + esp-radio + embassy-net). Different target
triple, different runtime, mutually exclusive feature sets. The same ESP32-C6
board runs both.

## What's in `src/main.rs`

Only the **current** investigation; its doc comment says what it measures and
how to read the result. `git log -- embassy-hw-probe/src/main.rs` is the record
of earlier ones. To start a new one, keep the bring-up (heap, `esp_rtos::start`,
Wi-Fi, embassy-net, TRNG, the single `mbedtls_rs::Tls`) and the `custom_halt`
function, and replace the rest.

The first investigation (#292) ran six stages: Wi-Fi and DHCP; a raw
`EmbassyRawStreamFactory` dial; a real `EmbassyTlsConnector` handshake with
`negotiated_version`; `PrinterClient::connect_mqtt` plus one telemetry event;
`FtpsClient::connect` plus one `list_directory`; and `EmbassyTimer` pacing. It
is the reference for how to construct each of those on this stack.

Any number of TLS connectors share the one `mbedtls_rs::Tls` instance, which is
the case `EmbassyTlsConnector`'s `TlsReference` design exists for: MbedTLS
permits only one instance per process.

`bambino` is a path dependency on purpose: a probe should drive the *shipped*
types, because a reimplementation of them in this file can pass while the real
code still fails.

## Re-running the socket teardown check

The root `Cargo.toml` warning above `embassy-net` asks for this before any
embassy-net or smoltcp change. bambino must close every TCP connection cleanly,
with a FIN, and only hardware shows whether a stack change still lets it.

The check is the probe committed as "probe: hardware check of bambino's embassy
socket pool" (`git log --grep 'embassy socket pool' -- embassy-hw-probe/`).
Restore its `src/main.rs`, `Cargo.toml` and `.cargo/config.toml`, run it, then
put the current investigation back. The `src/main.rs` doc comment has the
stages, the pass conditions and how to run it.

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

The probe loops forever once it finishes (standard bare-metal convention —
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

`mbedtls-rs` allocates 16 KiB in + 16 KiB out per TLS session by default, and a
C6 at the heap sizes in `src/main.rs` fits two live sessions, not three
(measured; see `src/io/CLAUDE.md`). Memory, not CPU, is the first thing to run
out. `src/main.rs`'s heap sizes are the knob; the
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
