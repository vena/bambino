# NOSTD_PLAN.md

## Origin

The previous cycle of this plan (Phases 0-5, closed 2026-07-01) took `src/io/` from
"tokio-only in practice" to genuinely multi-platform: fallible timers, typed addresses,
Embassy TLS buffers owned per-connection instead of a panicking global, real async TLS on
ESP-IDF, a platform-general TLS-version safety guarantee, and FTPS working on Embassy and
ESP-IDF. That history is preserved in git (see `no_std phase 0` through `no_std phase 5`
commits) — this file starts a fresh cycle rather than carrying the old phase writeups
forward.

A follow-up deep review of `src/io/`'s current state (2026-07-02, post-Phase-5) found two
real gaps the previous cycle didn't close, both isolated to ESP-IDF's TLS code. Both are
scoped below as independent, self-contained phases. A third candidate finding — whether
`EmbassyFtpDataStreamFactory::create_data_stream`'s IPv4-only address parsing
(`src/io/embassy.rs`) needed hardening — was resolved as **not a bug**: every real call
site traces back to either a caller-supplied literal IP or SSDP discovery
(`discovery/parser.rs::parse_location`, which only ever extracts a dotted-decimal IPv4
address — Bambu printers don't advertise IPv6 or hostnames), so a hostname/IPv6 input at
that call site is a genuine caller error, not a missing case. Landed directly as a doc
comment (`io/embassy.rs`) plus a README callout in the "Embassy FTPS" section — no phase
needed, already done.

---

## Phase 6 — ESP-IDF TLS-1.2 enforcement: fail closed instead of silently skipping [NOT STARTED]

**Problem.** `BambuFtpsClient::connect()` (`src/ftps/client.rs:72-81`) enforces the
TLS-1.2 requirement on P2S/X2D models like this:

```rust
if model.quirks().enforce_ftps_tls_1_2()
    && let Some(version) = tls_connector.negotiated_version(&control_stream)
    && version != TlsVersion::Tls12
{
    return Err(BambuError::ProtocolViolation(...));
}
```

A `None` from `negotiated_version` skips the whole check — the connection proceeds
unchecked. This is intentional and documented for the case of "platform genuinely cannot
inspect the negotiated version" (the trait doc comment on both `TlsConnector` and
`SecureConnect` calls this out as best-effort-by-design). The bug: ESP-IDF's
`query_negotiated_tls_version` (`src/io/esp_idf.rs:326-351`, used by both
`EspIdfSecureConnector::negotiated_version` and `EspIdfTlsConnector::negotiated_version`)
*can* genuinely query the version — it has a real, working implementation — but also
returns `None` on states that are query *failures*, not platform limitations:

- `ssl_ctx` null (`esp_idf.rs:333-335`)
- `version_ptr` null (`esp_idf.rs:338-340`)
- version string isn't valid UTF-8 (`esp_idf.rs:344`)
- version string doesn't match `"TLSv1.2"`/`"TLSv1.3"` (`esp_idf.rs:349`)

All four are real post-handshake mbedTLS conditions. `ftps/client.rs` has no way to tell
"this platform doesn't support version queries" apart from "this platform's query just
failed" — both collapse to the same `None`, and both currently mean "skip the safety
check" on a P2S/X2D printer with a heated bed and nozzle. That's a fail-open safety net on
exactly the models it exists to protect.

**Design decision (already made — resolved 2026-07-02, do not re-litigate):** fail closed.
Change `ftps/client.rs`'s check so that when `enforce_ftps_tls_1_2()` is true, *any*
non-`Some(Tls12)` result — including `None` — rejects the connection. Chosen over the
alternative (a richer `Result<Option<TlsVersion>, QueryError>` return type threaded through
`TlsConnector`/`SecureConnect` and all three platform impls, to distinguish "unsupported"
from "failed") because that alternative is real API surface for a distinction only
ESP-IDF's implementation can currently produce — no platform today has a legitimate
"cannot query at all" case (Tokio and Embassy both always return `Some`). If a future
platform genuinely can't query the version, revisit then; don't pre-build the distinction
for a case that doesn't exist yet.

**Consequence to accept, not a defect:** this changes semantics for *any* future
`TlsConnector`/`SecureConnect` impl that intentionally returns `None` (there are none
today) — such a platform would now hard-reject TLS-1.2-required models instead of
connecting unchecked. That's the correct trade-off for a firmware controlling a heated
nozzle and bed: refusing to connect beats connecting without the safety check silently
holding.

**Tasks:**
1. In `src/ftps/client.rs`'s `connect()` (`:72-81`), replace the `if ... && let Some(...) &&
   ...` chain with a `match` (or equivalent) that rejects on both `Some(v) if v !=
   TlsVersion::Tls12` and on `None`, when `enforce_ftps_tls_1_2()` is true. Keep the
   existing `Some(TlsVersion::Tls12)` pass-through unchanged.
2. Update the rejection's `ProtocolViolation` message — it currently assumes TLS 1.3 was
   the negotiated version (`"...but TLS 1.3 was negotiated..."`, `:77`). Word it to cover
   both cases (wrong version negotiated, or version could not be determined) rather than
   asserting a specific wrong version that may not be what actually happened.
3. Update the README's TLS-1.2-enforcement paragraph (`README.md`, the paragraph
   referencing "ESP-IDF TLS timeouts" near the "TLS configuration" section) to state the
   fail-closed behavior explicitly: a version-query failure on a TLS-1.2-required model is
   now a hard connection failure, not a silent pass-through.
4. Add or update a test in `tests/ftps_test.rs` covering the `None`-from-`negotiated_version`
   case on a TLS-1.2-required model quirk — the existing
   `test_ftps_tls13_rejected_for_p2s`/`_x2d` tests (per Phase 4's verification notes) use
   `VersionReportingTlsConnector`, which can presumably be configured to return `None`;
   confirm it rejects rather than passes.

**Left out of scope, noted for awareness (not this phase's job):** `list_directory`/
`upload_file`/`download_file` (`ftps/client.rs:145-181` and similar) open a *second* TLS
connection per call for the data channel, and never call `negotiated_version` on it — only
the control channel connection is checked, once, in `connect()`. In practice the data
channel goes through the same `tls_connector`/`Config` as the control channel, so it should
always negotiate the same version, but this hasn't been verified against real ESP-IDF
hardware and isn't checked defensively. If a future review finds the data channel can
diverge from the control channel's negotiated version, that's a separate phase — don't fold
it into this one.

**Ordering:** independent of Phase 7. Small, single-file-plus-tests change — safe to do
first or in either order.

---

## Phase 7 — ESP-IDF TLS: log discarded errors instead of dropping them silently [NOT STARTED]

**Problem.** Four sites in `src/io/esp_idf.rs` discard a real `esp_idf_svc::sys::EspError`
with no logging at all:

- `EspTlsStream::read`'s non-would-block error arm: `Err(_) => return
  Err(embedded_io_async::ErrorKind::Other)` (`esp_idf.rs:245`)
- `EspTlsStream::write`'s equivalent arm (`esp_idf.rs:263`)
- `EspIdfSecureConnector::secure_connect`'s handshake retry loop: `Err(_) => return
  Err(SocketError::ConnectionRefused)` (`esp_idf.rs:294`)
- `EspIdfTlsConnector::connect`'s handshake retry loop, same pattern (`esp_idf.rs:567`)

This directly undercuts the principle the previous plan cycle established in Phase 1
("Option A: log-at-the-boundary before discarding into a generic error" — see
`map_std_io_error` in `src/io/mod.rs`, which does `log::debug!("{other_msg}: {err}")`
before constructing `SocketError::Other`). Phase 1's task list scoped that fix narrowly to
`SocketError::Other` construction sites reached through `map_std_io_error`/
`to_esp_socket_error`/`to_socket_error` — none of these four sites route through those
functions (two produce a different error type, `embedded_io_async::ErrorKind`, entirely;
two produce `SocketError::ConnectionRefused`, a different variant that was never in scope).
Phase 1's own retrospective excused the *static-message* `SocketError::Other("...")` sites
in `secure_connect` ("no discarded errno to surface") — but that reasoning doesn't apply
here, since these four sites do have a real, discarded `EspError` each time.

Practical impact: on ESP-IDF specifically — the platform where physical-device debugging
access is hardest — a TLS handshake or read/write failure surfaces as an opaque
`ConnectionRefused`/`ErrorKind::Other` with no way to tell whether it was a cert mismatch,
a network reset, a timeout, or something else, without attaching a debugger or adding
temporary instrumentation.

**Design decision:** none needed — this is the same "log at the boundary, don't change the
public error type" approach Phase 1 already established and the codebase already uses
elsewhere. No API surface change.

**Tasks:**
1. At `esp_idf.rs:245` (`EspTlsStream::read`'s discard arm), log the real error before
   converting: `Err(e) => { log::debug!("ESP-IDF TLS read failed: {e:?}"); return
   Err(embedded_io_async::ErrorKind::Other); }` (adjust format to whatever `EspError`'s
   `Debug`/`Display` actually renders as useful — check which is more informative; `EspError`
   implements both `Debug` and `Display` via `esp-idf-svc`, prefer `Display` if it includes
   the human-readable ESP-IDF error string, not just the numeric code).
2. Same fix at `esp_idf.rs:263` (`EspTlsStream::write`).
3. Same fix at `esp_idf.rs:294` (`EspIdfSecureConnector::secure_connect`'s non-would-block
   handshake error).
4. Same fix at `esp_idf.rs:567` (`EspIdfTlsConnector::connect`'s non-would-block handshake
   error).
5. Compile-check with `scripts/check-esp-idf.sh esp32c6` (Docker) — this only touches
   logging calls, no new unsafe surface or API change, but confirm it still builds clean
   per this repo's convention of not trusting a stale cache (force with `cargo clean -p
   bambino --target riscv32imac-esp-espidf` inside the container first if re-running on a
   machine where a prior clean run's volumes are still warm).
6. No README change needed — this is an internal debuggability fix, not a behavior or API
   change worth documenting for consumers.

**Ordering:** independent of Phase 6. Small, single-file change, four near-identical edits
— low risk.

---

## Verification (both phases)

Standard gate, same as every phase in the previous cycle:

```sh
cargo build
cargo build --no-default-features --features alloc --lib
cargo check --no-default-features --features embassy --lib
cargo test
cargo clippy --all-targets
scripts/check-esp-idf.sh esp32c6   # required for Phase 7 (esp-idf-gated code); recommended for Phase 6 too since ftps/client.rs isn't esp-idf-gated but its behavior change is ESP-IDF-motivated
```

Only the two pre-existing warnings in `tests/ftps_test.rs`/`tests/client_test.rs`
(`type_complexity`, `while_let_loop` — unrelated to `io/`, confirmed pre-existing as of the
2026-07-01 cycle) are expected from `cargo clippy --all-targets`. Any other new warning is
a regression from these phases' changes.
