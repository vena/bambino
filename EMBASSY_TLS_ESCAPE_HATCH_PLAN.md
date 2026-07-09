# Embassy TLS: Backend Swap (mbedtls-rs) + TLS-1.2-Enforcement Escape Hatch

## Problem

`bambino`'s Embassy (bare-metal, `no_std`+`alloc`) target cannot talk FTPS to P2S or X2D
printers. `ModelQuirks::enforce_ftps_tls_1_2()` returns `true` for those two models — a
workaround for a firmware bug in their embedded vsFTPd, not a real protocol ceiling (see
`src/quirks/models/{p2,x2}.rs` doc comments and `reference/02_ftps.md` §2.1). `BambuFtpsClient`
enforces this by failing closed: `require_tls_1_2_if_enforced()` (`src/ftps/client.rs:288-304`)
errors unless `tls_connector.negotiated_version(stream) == Some(TlsVersion::Tls12)` exactly —
an unconfirmed `None` also rejects. `EmbassyTlsConnector` (`src/io/embassy.rs`) currently wraps
`embedded-tls` 0.19, which only implements TLS 1.3 and hard-codes
`negotiated_version() -> Some(TlsVersion::Tls13)` — it can never pass this check.

Two prior investigations (both closed, summarized here so a clean session doesn't need to
re-derive them):

1. **`mbedtls-rs`** (`github.com/esp-rs/mbedtls-rs`) was investigated as a drop-in replacement
   with real TLS 1.2 support. Its public Rust API (confirmed by reading actual source, not
   just its README) exposes **neither a post-handshake negotiated-version getter nor a
   max-version cap** — `ClientSessionConfig`/`ServerSessionConfig` only have `min_version`
   (a floor; default preset enables both TLS 1.2 *and* 1.3, so a peer that also speaks 1.3 —
   like P2S's vsFTPd — will still often end up negotiating 1.3 unless something forces 1.2
   specifically, and nothing in the public API does). So `mbedtls-rs` **cannot itself satisfy**
   `require_tls_1_2_if_enforced`'s exact-match check either, despite having real 1.2 support
   in the underlying C library. Confirmed no fix on the horizon: checked open issues and
   recent commits, nothing addresses this gap.
2. **`rustls`** was investigated as a further alternative specifically because it natively
   supports both TLS 1.2 and 1.3 (unlike `embedded-tls`) and `bambino` already depends on it
   for `tokio` (`tokio-rustls = "0.26.4"`). That investigation was **dropped without finishing**
   the no_std/crypto-provider/IO-integration research, because the escape hatch decision below
   made its entire premise moot: once verification is bypassed by explicit opt-in regardless of
   backend, rustls's one edge (honest TLS 1.2) stops mattering, and `mbedtls-rs` wins on other
   grounds (hw-accel for ESP32, mature underlying C library, and — confirmed separately, see
   below — correct CA-verification behavior against the printers' actual certs). Do not
   re-open a rustls investigation without new information changing this calculus.

**Given neither candidate backend can honestly satisfy the exact-match TLS-version check, this
plan's resolution is: (a) add an opt-in escape hatch that bypasses the check, accepting a
documented reliability tradeoff, and (b) separately swap Embassy's backend from
`embedded-tls` to `mbedtls-rs` anyway, for reasons independent of that check** (below).

### Important: the backend swap does NOT reduce P2S/X2D-specific transfer failures

Read this before assuming the swap "fixes" or "improves" reliability against P2S/X2D — it
doesn't, and a clean session should not conflate the two tracks below. The P2S firmware bug is
about how the **server** mishandles TLS 1.3 session tickets on the data channel. Both
`embedded-tls` (1.3-only) and `mbedtls-rs` (1.2+1.3 both enabled by default, no way to exclude
1.3 from the offered `ClientHello`) will, in the ordinary case, still end up negotiating TLS 1.3
against a server that also supports it — `mbedtls-rs`'s `min_version` is a floor, not an
exact-version pin, and there is no public API to cap the max. So the swap does not make TLS 1.2
more likely to actually be negotiated with P2S/X2D specifically. The escape hatch's accepted
failure mode (spurious `426`s / retries against those two models, safe because `upload_file`'s
`SIZE` recheck and `download_file`'s exact-`226` check already catch truncation — see below) is
exactly as likely after the swap as before it. The swap's real motivations are orthogonal:
hardware-accelerated crypto for ESP32 (this project's primary practical no_std audience) and
correct certificate verification behavior for every *other* model, addressed next.

## Why the escape hatch is safe to offer (unaffected by which backend is used)

The natural instinct is "never let a caller bypass a fail-closed safety check" — right when the
failure mode is silent corruption, which this isn't, confirmed by reading the transfer code:

- `BambuFtpsClient::upload_file` (`src/ftps/client.rs:490`) unconditionally re-verifies the
  written file's size via `SIZE` regardless of a `226` or transient `426` reply — a bad write
  is caught and surfaced as an error, never silently accepted.
- `BambuFtpsClient::download_file` (`src/ftps/client.rs:570`) requires the control channel's
  final reply to be exactly `226` or it errors — a truncated transfer surfaces as
  `BambuError::ProtocolViolation`, never as silently-short data handed back to the caller.

So bypassing `require_tls_1_2_if_enforced` risks **more failed transfers/retries**, not
corrupted data reaching application code. That's a reliability tradeoff a caller can reasonably
opt into, not a safety hole. Default behavior (fail closed) does not change.

## Why swap to `mbedtls-rs` anyway (independent of the escape hatch)

All of the following are confirmed by reading actual source, not reputation or README prose:

- **Hardware-accelerated crypto for ESP32.** `mbedtls-rs-sys`'s Cargo features
  (`esp32`/`esp32c2`/`c3`/`c5`/`c6`/`h2`/`s2`/`s3`) wire Espressif's crypto peripherals into
  MbedTLS's `_ALT` hooks. `embedded-tls` has no hardware-acceleration story at all — it's
  pure-software RustCrypto-family code today, on every chip. Espressif's `esp-hal`+`embassy`
  (no ESP-IDF/FreeRTOS) is this project's primary practical no_std audience, so this is a real,
  not hypothetical, win.
- **Correct CA-verification against real printer certs.** Per
  `reference/01_network_discovery.md:134-138`, printer certs carry the serial number in CN and
  apparently lack a SAN extension. mbedtls's C source (`espressif/mbedtls` @ `ffb280b`,
  `library/x509_crt.c`, `x509_crt_verify_name`, ~lines 2991-3005) confirmed: it falls back to
  matching Subject CN when no SAN extension is present. `rustls`'s standard verifier is
  believed (not yet confirmed with source — see `TLS_SNI_HOSTNAME_MISMATCH_PLAN.md`'s open
  question) to lack this fallback entirely. `mbedtls-rs`'s `ClientSessionConfig.server_name`
  threads into `mbedtls_ssl_set_hostname()`, which is used as both the SNI value and the CN
  match target — so a consumer who wants to actually verify a printer's cert via a supplied
  CA (rather than the default no-verification mode) gets this correctly, *provided* the caller
  passes the printer's serial, not its IP, as `server_name` — see
  `TLS_SNI_HOSTNAME_MISMATCH_PLAN.md`, a separate, orthogonal plan that fixes this same
  serial-vs-IP mistake across every platform's TLS connector, not just Embassy's.
- **Mature underlying crypto.** MbedTLS the C library is a long-established, widely-audited
  TLS implementation. Only `mbedtls-rs`'s *Rust wrapper* is young (`v0.1.0`) — much lower risk
  than betting on an all-Rust crypto stack's from-scratch implementation.
- **License clean.** `mbedtls-rs`/`mbedtls-rs-sys`: `MIT OR Apache-2.0`. Vendored MbedTLS fork
  (`espressif/mbedtls` @ `ffb280b`): confirmed via its `LICENSE` blob, `Apache-2.0 OR
  GPL-2.0-or-later` — Apache-2.0 alone is fully AGPL-3.0-compatible.
- **IO trait bridge is trivial, not new adapter work.** `mbedtls-rs::Session<'a, T:
  embedded_io_async::Read + Write>` uses the exact same trait family as `bambino`'s `AsyncIo`
  (blanket-impl'd for any `embedded_io_async::Read + Write`, `src/io/mod.rs:156`).
  `EmbassyRawStreamFactory::dial`'s `TcpConnection` return type needs zero adapter code to
  satisfy this bound.
- **Dependency versions already align — confirmed, not assumed.** `bambino`'s `Cargo.toml:117`
  already pins `rand_core = "0.10.1"` (kept alongside a `rand_core_legacy = "0.6.4"` shim
  — `Cargo.toml:123` — that exists *only* to satisfy `embedded-tls` 0.19's older expectation).
  `mbedtls-rs`'s workspace `Cargo.toml` pins the exact same `rand_core = "0.10.1"` and
  `embedded-io`/`embedded-io-async = "0.7"` (`bambino`'s `Cargo.toml:62` also pins
  `embedded-io-async = "0.7.0"`). No RNG or IO adapter shim is needed for the swap — the
  `rand_core_legacy` alias becomes dead weight once `embedded-tls` is removed and should be
  deleted in the same change.
- **RAM is roughly a wash.** `mbedtls-rs-sys` defaults to 16 KiB in + 16 KiB out record
  buffers, matching `bambino`'s current caller-supplied `embedded-tls` buffers (README's
  "Embassy TLS buffers" section: `16384`-byte read/write buffers). Shrinking below default
  forces an on-the-fly rebuild (loses the precompiled-`.a`-lib benefit) — not needed by
  default.
- **Precompiled libs cover more than originally assumed.** Beyond RISC-V (`riscv32imc`,
  `riscv32imac`) and Xtensa ESP32 (`esp32`, `esp32s2`, `esp32s3`), `mbedtls-rs-sys` also ships
  precompiled `.a`s for `thumbv6m-none-eabi` and `thumbv7em-none-eabi` (Cortex-M0/M4 —
  STM32/nRF-class chips), confirmed via `gh api search/code` against the repo.

## Non-goals

- Do not touch `tokio` or `esp-idf` TLS backends. `tokio` already uses `rustls` and gets
  everything it needs; `esp-idf` uses `esp_idf_svc::tls::EspTls` (ESP-IDF's own vendored
  mbedtls), already hardware-accelerated, already correctly does `negotiated_version`
  (`src/io/esp_idf.rs:371-381`) and both TLS versions correctly. No known bug there.
- Do not re-investigate `noxtls`, `embedded-mbedtls`, `mbedtls-rs`'s own version-query/cap gap,
  or `rustls` for Embassy — all closed per the investigations summarized above. Reopen only on
  genuinely new information (e.g. a `mbedtls-rs` release adding the missing API).
- Do not touch `upload_file`'s `SIZE` re-verification or `download_file`'s exact-`226` check —
  these are what makes the escape hatch safe to offer at all.
- Do not fold in `TLS_SNI_HOSTNAME_MISMATCH_PLAN.md`'s serial-vs-IP fix as part of this plan —
  it's an orthogonal, separate bug affecting every platform's `TlsConnector::connect` call
  sites equally, not just Embassy's. It touches the same files (`client/connect.rs`,
  `ftps/client.rs`) as the work below, so implement it as a **separate commit/PR** to keep each
  change reviewable — no actual logical conflict, just avoid tangling unrelated diffs together.
- Do not implement the two tracks below as a single undifferentiated change — keep them
  separable (see "Track ordering" below) so each can be reviewed and (for Track B) hardware-
  tested on its own.

## Track ordering (both tracks are independent — implement in either order, or in parallel)

- **Track A (escape hatch)** works against whichever `TlsConnector` is active — it doesn't care
  whether `EmbassyTlsConnector` wraps `embedded-tls` or `mbedtls-rs`. It can be implemented and
  merged before, after, or alongside Track B.
- **Track B (backend swap)** doesn't depend on Track A existing — `mbedtls-rs`'s
  `negotiated_version` will honestly return `None` (it cannot determine this, same as today's
  hard-coded-but-wrong `Some(Tls13)` from `embedded-tls` — except now honest instead of
  fabricated), so Embassy+P2S/X2D remains blocked without Track A regardless of which backend
  Track B lands. **Track A is what actually unblocks Embassy talking to P2S/X2D at all** — Track
  B alone does not, and should not be described in any commit/PR as fixing that.

## Track A: TLS-1.2-enforcement escape hatch

### A1. Thread a bool through `BambuFtpsClient`

File: `src/ftps/client.rs`.

- Add a new field: `allow_unverified_tls_1_2: bool`, with a doc comment explaining it skips
  `require_tls_1_2_if_enforced`'s rejection and pointing at this plan's "Why the escape hatch
  is safe" section so a future reader doesn't have to re-derive the reasoning.
- Add a new last parameter to `BambuFtpsClient::connect(...)` (currently at line 131:
  `raw_control, tls_connector, data_factory, model, ip, access_code, timer`):
  `allow_unverified_tls_1_2: bool`. Breaking signature change — acceptable pre-1.0 per this
  repo's convention (CLAUDE.md), call it out explicitly in the commit/PR. Update every call
  site (grep `BambuFtpsClient::connect(` — as of this writing: `src/client/connect.rs`'s
  `ensure_ftps()`, plus any direct calls in `tests/ftps_test.rs`/`tests/common/`) to pass
  `false` for unchanged behavior, except the one call site changed in A2.
- Store the parameter into the new field when constructing `Self`.
- Update `require_tls_1_2_if_enforced` (line ~288) to take an extra `allow_unverified: bool`.
  When `true`: `log::warn!` that TLS version enforcement was bypassed by caller configuration,
  then return `Ok(())` unconditionally. When `false`: unchanged behavior. Update its doc
  comment accordingly.
- Update both call sites of `require_tls_1_2_if_enforced`: in `connect()` (line 142), pass the
  local parameter directly; in `open_data_channel()` (line ~324), pass
  `self.allow_unverified_tls_1_2`.

### A2. Wire it through `PrinterClient`

File: `src/client/connect.rs` (struct fields live in `src/client/mod.rs` — grep the
`PrinterClient` struct definition and every `PrinterClient { ... }` constructor literal, e.g.
inside `with_ftps`/`with_camera`/`with_timer`, to thread a new field through, mirroring how
`ftps_port`/`camera_max_frame_size` are already threaded).

- Add `ftps_allow_unverified_tls_1_2: bool` (default `false`) to `PrinterClient`.
- Add `with_ftps_allow_unverified_tls_1_2(mut self, allow: bool) -> Self` (non-consuming,
  mirrors `with_ftps_port` at line 238). Doc comment: state plainly this only matters for the
  `embassy` feature against P2S/X2D — on `tokio`/`esp-idf`, use `force_tls_1_2` instead, since
  those platforms can actually satisfy the check for real.
- Update `ensure_ftps()` (line 250) to pass `self.ftps_allow_unverified_tls_1_2` as the new last
  argument to `BambuFtpsClient::connect(...)` (line 266).

### A3. CLI flag

File: `src/bin/bambino-cli/` — **read `main.rs`'s actual `Cli`/`Command` struct first** to find
the current storage-subcommand flag-parsing shape before writing this; don't guess CLI plumbing
from memory. Add `--allow-unverified-tls-1-2` (clap bool, default `false`), threaded to
`.with_ftps_allow_unverified_tls_1_2(true)` when set. This is additive alongside the existing
`build_unsafe_client_config_with_options(model.quirks().enforce_ftps_tls_1_2())` call at
`storage.rs:63` (that's the tokio-side real `force_tls_1_2` config — unaffected, keep it).

### A4. Tests

File: `tests/ftps_test.rs` (grep for existing tests exercising `enforce_ftps_tls_1_2` against a
mock `TlsConnector` first — thread the new parameter into them rather than duplicating).

- Default (`false`) behavior unchanged: mock model with `enforce_ftps_tls_1_2() == true`, mock
  connector reporting `Some(TlsVersion::Tls13)` (or `None`) — `connect()` still errors.
- Bypass (`true`): same setup — `connect()` succeeds (reaches the login sequence).
- Confirm (likely via existing tests, don't necessarily add new ones) that `upload_file`'s
  `SIZE` recheck and `download_file`'s exact-`226` requirement are unaffected by the flag.

## Track B: Swap Embassy's TLS backend to `mbedtls-rs`

### B1. Add the dependency, remove `embedded-tls`

`Cargo.toml`: add `mbedtls-rs`/`mbedtls-rs-sys` under the `embassy` feature (check current
`mbedtls-rs` crate version on crates.io/its repo at implementation time — this plan read
`v0.1.0` from source, may have moved). Remove `embedded-tls` and the `rand_core_legacy` shim
(`Cargo.toml:106,123` as of this writing) entirely — full replacement, not additive, matching
this repo's stated pre-1.0 preference for clean redesigns over compat shims. This breaks
`EmbassyTlsConnector`'s public constructor shape (today: `EmbassyTlsConnector::new(&config,
rng, &mut read_buf, &mut write_buf)`) — acceptable pre-1.0, call it out explicitly.

### B2. Implement the new connector

File: `src/io/embassy.rs`. Replace `EmbassyTlsConnector`/`EmbassyTlsStream` (or rename —
confirm naming doesn't collide, e.g. keep `EmbassyTlsConnector` as the public name since it's
still Embassy-specific, just backed differently internally) to wrap `mbedtls_rs::Session`:

- Requires one `mbedtls_rs::Tls` instance (MbedTLS only permits one active instance
  program-wide — `Tls::new(rng: &'static mut (dyn CryptoRng + Send))` or
  `Tls::new_local_borrows` for a shorter-lived RNG borrow, per `mbedtls-rs/src/lib.rs`).
  **Decide first**: whether `bambino`'s embassy integration constructs this once at startup
  and holds it for the connector's lifetime (simplest, matches "only one instance" constraint
  naturally) — don't guess a different shape without checking how `EmbassyTlsConnector` is
  currently constructed/held by consumers (check the README's Embassy setup example).
- `connect(host, raw_stream)`: build a `ClientSessionConfig` — `server_name` from `host` (this
  is where `TLS_SNI_HOSTNAME_MISMATCH_PLAN.md`'s fix matters: pass the printer's serial, not
  IP, once that plan lands; until then, whatever `host` value callers pass through today),
  `min_version: TlsVersion::Tls1_2`, `auth_mode` mirroring `bambino`'s existing
  unsafe/verified split (`AuthMode::None` for the no-verification default, `Required` +
  `ca_chain` when a consumer supplies CA certs), then `Session::new(&tls_ref, raw_stream,
  &config)` and `.connect()`.
- `negotiated_version(&self, _stream) -> Option<TlsVersion>`: return `None`, **honestly** — do
  not hard-code a value the crate cannot actually confirm (this is exactly the anti-pattern the
  old `embedded-tls` wrapper had, just wrong in the other direction). Document why: `mbedtls-rs`
  has no public API for this (see Problem section). This is what makes Track A still required
  after this swap.

### B3. Buffer sizing

Keep `mbedtls-rs-sys`'s default 16 KiB in/out record buffers (matches today's `embedded-tls`
16384/16384 caller-supplied buffers — not a regression). Only shrink via the
`ssl-in-content-len-<N>`/`ssl-out-content-len-<N>` Cargo features if a specific
RAM-constrained target needs it, and document that doing so forces an on-the-fly MbedTLS
rebuild (needs `clang`/`cmake`/`ninja`), losing the precompiled-lib benefit for that target.

### B4. Documentation

- `README.md`: replace the "Embassy TLS buffers" section's `embedded-tls`-specific prose with
  the `mbedtls-rs` equivalent. State plainly: (a) this swap does not change P2S/X2D reliability
  (see Problem section's caveat) — Track A is what unblocks that; (b) hw-accel is available on
  ESP32 targets; (c) CA-verified mode now does correct CN-fallback matching against printer
  certs, cross-referencing `TLS_SNI_HOSTNAME_MISMATCH_PLAN.md` for the serial-vs-IP piece.
- `CLAUDE.md`'s "Non-Obvious Type Decisions": add a bullet recording the backend swap, the
  removed `rand_core_legacy` shim, and the still-`None` `negotiated_version` (so a future reader
  doesn't mistake this for a bug and try to hard-code a value).

### B5. Tests

- Confirm compilation and basic construction under `cargo check --no-default-features
  --features embassy --lib`.
- `negotiated_version` returns `None` (test this explicitly — it's a deliberate, documented
  choice, not an oversight, and a future refactor could otherwise "fix" it incorrectly).
- Auth-mode wiring: a config with no CA chain uses `AuthMode::None`; a config with a CA chain
  uses `AuthMode::Required` with the chain installed.
- Full real-handshake testing (TLS 1.2 and 1.3 against a real or mock MbedTLS-compatible peer)
  is limited by what the test harness can simulate in `no_std` — do what's feasible, and rely
  on B6 for the rest.

### B6. Real-hardware verification requirement

This track changes the actual TLS handshake bytes on the wire (new library, new cipher/version
negotiation implementation) — exactly CLAUDE.md's "changing the *shape* of writes/reads on an
already-working wire path" category, which mock tests cannot fully validate. Once B2-B5 pass
their own tests, this needs a real FTPS (and MQTT, if `EmbassyTlsConnector` is shared across
both) connection test over an actual Embassy target before being considered done. **If you are
an agent doing this work and printer credentials are available in your environment: do not run
this verification yourself** — ask the user to run it and report back, per CLAUDE.md's existing
convention. Test against a non-P2S/X2D model first to confirm the swap itself works at all
(e.g. a model with `enforce_ftps_tls_1_2() == false`); P2S/X2D specifically will need Track A's
escape hatch enabled too, and per the Problem section's caveat, may still see transfer failures
even then — that's expected, not a regression.

### B7. Verification gates

```sh
cargo build
cargo test
cargo build --no-default-features --features alloc --lib
cargo check --no-default-features --features embassy --lib
cargo clippy
```

This track is 100% `embassy`-feature-gated code — the `embassy` check is the critical one here,
not optional.

## Definition of done

1. Track A: `BambuFtpsClient::connect`/`PrinterClient::with_ftps_allow_unverified_tls_1_2` exist,
   default behavior provably unchanged, bypass path covered by new tests, CLI flag wired
   (verified against actual `main.rs`, not guessed), README/CLAUDE.md updated.
2. Track B: `embedded-tls`/`rand_core_legacy` removed, `mbedtls-rs`-backed
   `EmbassyTlsConnector` compiles under every gate in B7, `negotiated_version` honestly returns
   `None` (tested), auth-mode wiring tested, README/CLAUDE.md updated with the "doesn't fix
   P2S reliability" caveat stated explicitly.
3. Both tracks merged as separate, independently-reviewable changes (not one tangled diff).
4. A note on what real-hardware test (B6) still needs to happen before Track B is mergeable,
   if not already run.
