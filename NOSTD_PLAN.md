# NOSTD_PLAN.md

## Origin

This plan came out of a full `src/io/` architecture review (2026-07-01, pre-1.0, all API
changes in scope). Core conclusion: **the `tokio` target has moved ahead of `embassy` and
`esp-idf` in ways that are no longer just "not implemented yet" — some of the gaps are
structural** (a global buffer singleton that can't support two concurrent TLS sessions, a
sync-only ESP-IDF TLS path, a safety guarantee that's silently tokio-only). This file lays
out phases to close those gaps. Each phase is written to be picked up cold — no prior
conversation needed, just this file and the current code.

**Closed question, no phase needed:** whether `TlsConnector::connect`'s unused `port: u16`
parameter is dead weight that should be dropped. It isn't — `reference/01_network_discovery.md`
confirms every *fixed* protocol port (8883 MQTT, 990 FTPS, 322 RTSPS, 6000 camera, 2021/1990
SSDP) is constant across all models, so Bambu changing a fixed port isn't the risk this
parameter guards against. The real per-connection port variability already exists and already
uses this parameter: `ftps/client.rs`'s `negotiate_passive_port()` gets a fresh ephemeral port
per `PASV` command, and that port is what's passed into `tls_connector.connect(&self.ip, port,
raw_data_socket)` for the data channel. Keep the parameter as-is.

---

## Phase 0 — `TimerProvider::sleep` must be fallible

**Problem.** `TimerProvider::sleep()` returns `()`. On tokio this is honest — `tokio::time::sleep`
cannot fail. On ESP-IDF it is not: `esp-idf-svc`'s `EspAsyncTimer::after()` returns
`Result<(), EspError>` (confirmed against esp-idf-svc docs), and `io/esp_idf.rs`'s current
`TimerProvider` impl papers over that with `.expect("ESP-IDF hardware timer scheduling failed")`
— a panic on what is very plausibly a transient, recoverable condition (e.g. FreeRTOS timer/task
resource exhaustion) on a physical device controlling a heated nozzle and bed. A panic here likely
means a full firmware reset while a print may be running.

**Design decision (already made, not open):** change the trait to return a `Result`, matching how
every other fallible platform operation in this crate is modeled (`SocketError`-style). Tokio and
Embassy impls stay trivially `Ok(())`; only ESP-IDF's impl has a real error to propagate. std-target
code paths already propagate `Result` instead of panicking on recoverable errors — ESP-IDF's timer
should follow the same convention instead of being the one place that panics.

**Tasks:**
1. Add a `TimerError` type to `src/io/mod.rs` (small enum, no_std/alloc-safe, mirrors the shape
   of `SocketError` — start with a single `Other(&'static str)` variant or similar; do not
   over-design this, it exists to replace one panic).
2. Change `TimerProvider::sleep` signature to `async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError>`.
3. Update `TokioTimer::sleep` and `EmbassyTimer::sleep` to wrap their (infallible) calls in `Ok(...)`.
4. Update `EspIdfTimer::sleep` to map the real `EspError` into `TimerError` instead of `.expect()`-panicking.
5. Update every call site (search for `.sleep(` across `src/`) to propagate the new `Result`
   — most will just need a `?` inserted; check `client/mod.rs`'s `poll_until` and any retry/backoff
   loops in `discovery.rs` specifically, since those are the most likely to call `sleep` in a loop
   where an error needs to actually stop the loop rather than be silently ignored.

**Ordering:** fully independent. Do this first — it's small, low-risk, and touches a trait every
other phase's platform code also uses, so landing it early avoids rebasing later phases on top of
a signature change.

---

## Phase 1 — `no_std` address handling and error fidelity

**Problem A — heap allocation and hand-rolled parsing on the hot discovery path.**
`AsyncUdpSocket::send_to`/`recv_from` (`src/io/mod.rs`) take/return `&str`/`String` for addresses.
Concretely:
- `EmbassyUdpSocket::recv_from` (`io/embassy.rs`) heap-allocates a fresh `alloc::string::String`
  via `write!` for *every received datagram* — on a no_std/alloc target, in what is meant to be a
  low-resource-footprint discovery loop.
- `EmbassyUdpSocket`'s `parse_endpoint` hand-rolls an IPv4-only, octet-by-octet string parser to
  turn a `send_to` target string back into a typed `IpEndpoint`. It's IPv6-blind and its only job
  is undoing string formatting that shouldn't have happened in the first place.
- Every implementor (`TokioUdpSocket`, `EmbassyUdpSocket`, `EspIdfUdpSocket`) round-trips through
  a string representation of an address that the underlying platform socket API already hands you
  as a typed value (`std::net::SocketAddr`, `embassy_net::IpEndpoint`).

`core::net::{IpAddr, SocketAddr}` has been stable since Rust 1.77 and requires neither `std` nor
`alloc`. There is no reason left to route through `String` here.

**Problem B — `SocketError::Other(&'static str)` discards real error detail.**
Every unmapped `std::io::Error` collapses to one of a handful of fixed compile-time strings
(`map_std_io_error`'s `other_msg` parameter). The actual OS errno/message is gone by the time
`log::debug!`/`log::trace!` could report it. Contrast this with `TokioIoError` (used one layer up,
at the `embedded_io_async`/`AsyncIo` boundary), which keeps the real `std::io::Error` via
`source()`. Same failure domain, two different fidelity levels depending which trait boundary the
error crossed.

**Design decision — mark decide-first:**
- **Option A (log-at-the-boundary, no API change):** at each `map_std_io_error`/`to_esp_socket_error`
  call site, `log::debug!("{err}")` the real error immediately before discarding it into
  `SocketError::Other`. Zero API surface change, loses nothing that matters for a `Result`-returning
  caller (they only ever match on the `SocketError` variant), keeps the enum `Copy`.
- **Option B (carry detail in the type):** add an alloc-gated variant, e.g.
  `#[cfg(feature = "alloc")] OtherDetailed(alloc::string::String)`, used only where `alloc` is
  available; `Other(&'static str)` remains the no-alloc fallback. More invasive — `SocketError`
  loses its uniform shape across std/no_std, and every match arm gains an alloc-conditional case.

Recommendation: start with Option A. It's a one-line-per-call-site fix that solves the actual
complaint (you can't see what went wrong when debugging) without touching the public error type.
Only revisit Option B if there's a concrete case where callers need to inspect the detail
programmatically, not just log it.

**Tasks:**
1. Change `AsyncUdpSocket::send_to`'s `target: &str` to `target: core::net::SocketAddr` (or
   `&core::net::SocketAddr`) and `recv_from`'s return type from `(usize, String)` to
   `(usize, core::net::SocketAddr)`, in `src/io/mod.rs`.
2. Update `BindableUdpSocket::bind`'s `addr: &str` — decide whether to also switch this to
   `SocketAddr` (cleaner) or leave as `&str` since it's a one-time setup call, not a hot path (low
   stakes either way — pick `SocketAddr` for consistency unless it complicates the `"0.0.0.0:2021"`
   style call sites more than it's worth).
3. Update `TokioUdpSocket`, `EspIdfUdpSocket`, `EmbassyUdpSocket` impls accordingly. Delete
   `parse_endpoint` entirely — `embassy_net::IpEndpoint` should be constructible directly from
   `core::net::SocketAddr`'s octets, or from a small typed conversion, no string round-trip needed.
4. Update `discovery.rs` and any other caller that currently formats/parses these addresses as
   strings.
5. Apply the Option A logging fix at each `SocketError::Other` construction site
   (`io/tokio.rs::to_socket_error`, `io/esp_idf.rs::to_esp_socket_error`, and the shared
   `map_std_io_error` in `io/mod.rs`).

**Ordering:** independent of Phase 0. Independent of Phases 2–5, but do it before Phase 5 (FTPS on
embedded) since Phase 5 adds new Embassy/ESP-IDF network code that should be written against the
typed-address API, not the string one — no sense writing new code against an API you're about to
delete.

---

## Phase 2 — Embassy TLS buffer ownership (remove the global singleton)

**Problem.** `io/embassy.rs` backs every `EmbassyTlsConnector` connection with two **process-wide
static** 16KB buffers (`TLS_READ_BUFFER`, `TLS_WRITE_BUFFER`) guarded by a single `AtomicBool`.
`BufferGuard::acquire()` **panics** if a second `GuardedTlsConnection` is requested while one is
already live — "only one concurrent TLS connection is supported" is enforced at runtime, by
crashing the firmware, not at compile time and not gracefully.

This is not just an efficiency nitpick — it is the direct blocker for Phase 5's Embassy leg. FTPS
(`ftps/client.rs`) holds the control channel's `Tls::Stream` open in `self.control_stream` for the
lifetime of the client, and separately opens a *second* TLS-wrapped data channel per
`list_directory`/`upload_file`/`download_file` call (for every model except the ones where
`model.quirks().uses_plaintext_ftps_data_channel()` is true). That's two live
`GuardedTlsConnection`s at once, structurally, for the common case. With the current design, the
second `connect()` call panics immediately.

**Design constraint.** Embedded RAM is scarce and the whole point of the static-buffer approach was
predictability (fixed allocation, no fragmentation) — a legitimate concern, not to be thrown away
carelessly. The fix is to stop hiding the allocation *decision* inside the crate as a hardcoded
singleton, and instead let the caller (who knows their board's RAM budget) supply the buffer
storage.

**Options — decide first:**
- **Option A: caller-supplied buffers per connector.** `EmbassyTlsConnector::new()` takes
  `&'a mut [u8]` read/write buffer slices as constructor arguments (in addition to `config`/`rng`),
  sized by the caller. Opening N concurrent connections means constructing N connectors, each with
  its own buffer pair — the caller decides N and pays for it explicitly. No statics, no panic, no
  hidden global.
- **Option B: const-generic buffer size, still per-connector-instance (not static).** Same as A but
  `EmbassyTlsConnector<'a, const BUF_SIZE: usize, CipherSuite, Rng>` owns its buffers as
  `[u8; BUF_SIZE]` fields (stack or caller-controlled placement) rather than taking slices. Slightly
  less flexible (size fixed at the type level) but avoids the caller having to manage lifetimes of
  external buffer storage.

Recommendation: Option A. It matches how `embedded-tls`'s own `TlsConnection::new(stream, read_buf,
write_buf)` already wants its buffers passed in — the crate is currently *fighting* that API by
stuffing static buffers behind it instead of threading caller-supplied ones through. Option A is
also strictly more flexible for the FTPS case (control channel and data channel buffers can be
sized differently if useful — e.g. directory listings are small, file transfers are large).

**Tasks:**
1. Remove `SyncUnsafeCell`, `TLS_READ_BUFFER`, `TLS_WRITE_BUFFER`, `TLS_BUFFERS_IN_USE`,
   `BufferGuard`, and the `unsafe impl Sync` block entirely from `io/embassy.rs`.
2. Change `EmbassyTlsConnector::new()` to accept read/write buffer slices (chosen option above).
3. Change `GuardedTlsConnection` (rename it — it no longer guards anything) to just own/borrow the
   `TlsConnection` directly, no guard field.
4. Update the README's Embassy quick-start section (it currently doesn't mention buffer sizing at
   all) to show how to size and pass buffers when constructing `EmbassyTlsConnector`.
5. Add a test or doc-comment making explicit that opening two connectors concurrently is now just
   "costs 2x buffer RAM," not "panics."

**Ordering:** independent of Phase 0/1. Must land before Phase 5's Embassy FTPS work.

---

## Phase 3 — ESP-IDF: real async TLS (decide-first on scope)

**Problem.** `io/esp_idf.rs`'s `EspTlsStream::read`/`write` call raw blocking POSIX `read()`/`write()`
on a socket fd, and `EspIdfSecureConnector::secure_connect` calls `esp_tls_conn_new_sync` — a
blocking handshake. The doc comment already admits this ("adapted to embedded-io-async via
blocking-mode reads/writes... full async integration requires esp-idf-svc socket-async support
which is not yet stable"), but that claim is now out of date: ESP-IDF's own docs confirm
`esp_tls_conn_new_async` exists as the non-blocking counterpart to `esp_tls_conn_new_sync`
(`esp-idf` `protocols/esp_tls.rst`). The building block for a real async connect exists; it's just
not used.

The practical impact: any `TimerProvider`-based timeout wrapped around ESP-IDF network I/O
(exactly the pattern `client/mod.rs`'s `poll_until` and the "chain `.with_timer()` for real
timeouts" guidance in the README describe) cannot preempt a blocked FFI call — the timeout can't
fire mid-handshake or mid-read on this platform, silently, with no warning anywhere in the docs.

**Decide first — how far to take this, two options:**
- **Option A (narrower): make the existing dial-owned model actually async.** Swap
  `esp_tls_conn_new_sync` for `esp_tls_conn_new_async`, and replace the blocking fd `read`/`write`
  in `EspTlsStream` with a non-blocking read/write + a real async wait (e.g. integrate with
  esp-idf-svc's async socket/eventfd primitives, or poll via `EspAsyncTimer` on `EWOULDBLOCK`).
  `SecureConnect` stays as the ESP-IDF abstraction — it's still a "dial your own connection"
  model, just genuinely non-blocking now. Lower risk, keeps `SecureConnect` as a permanent
  third connection pattern alongside `TlsConnector`.
- **Option B (broader): investigate whether ESP-IDF can support wrap-an-existing-stream TLS at
  all**, which would let ESP-IDF implement `TlsConnector` directly instead of `SecureConnect` —
  and would remove `SecureConnect` from the crate's trait surface entirely (tokio and embassy
  don't need it; it exists solely because of this platform's constraint). ESP-IDF's high-level
  `esp_tls_conn_new_sync`/`_async` are dial-style (they establish their own TCP connection as part
  of the call) per the docs pulled for this review — no confirmed high-level API for wrapping an
  already-connected fd was found. This would require dropping to lower-level `mbedtls` FFI
  (ESP-IDF ships the underlying mbedTLS component directly, which historically *can* wrap an
  arbitrary fd/BIO) — meaning more raw unsafe surface in exchange for collapsing three connection
  abstractions down to two. **This needs a spike before committing** — confirm whether
  `esp-idf-svc`/`esp-idf-sys` expose enough of raw mbedTLS to do this safely, and whether it's
  worth the unsafe-code trade for the abstraction simplification, before writing the real
  implementation. Prototype inside `scripts/check-esp-idf.sh`'s Docker image rather than against
  bare `esp-idf-sys` docs — it has the SDK and headers already in place to actually try calls
  against raw mbedTLS instead of guessing at the FFI surface from documentation alone.

Recommendation: do Option A now (concrete, bounded, fixes the "async is a lie" problem this phase
is named for). Treat Option B as a separate, explicitly-scoped spike — do not block Option A on it.

**Tasks (Option A):**
1. Replace `esp_tls_conn_new_sync` with `esp_tls_conn_new_async` in
   `EspIdfSecureConnector::secure_connect`.
2. Replace the raw blocking `read`/`write` syscalls in `EspTlsStream` with non-blocking calls plus
   a genuine async wait on `EWOULDBLOCK`/`EAGAIN` (check what esp-idf-svc offers for async socket
   readiness). Compile-check with `scripts/check-esp-idf.sh esp32c6` (or the relevant chip) — this
   resolves the old "no way to verify without the toolchain" gap by running the check inside the
   matching `espressif/idf-rust` Docker image. That confirms it *builds*; it does not confirm
   correct runtime behavior against real hardware, and the script isn't wired into CI (there is
   none in this repo), so still run it manually and don't treat a clean compile as proof the async
   wait logic is actually correct on-device.

   **Already done (2026-07-01):** the script was run for the first time against `esp32c6`, and
   `io/esp_idf.rs` did not compile as originally written — fixed two pre-existing bugs unrelated
   to this phase's actual scope: `esp_idf_svc::sys::read`/`write` take `usize` for the length arg,
   not `u32`; and `esp_tls_cfg`'s cert fields (`cacert_buf`, `clientcert_buf`, etc.) live behind
   bindgen-generated anonymous unions (`cfg.__bindgen_anon_N.field`), not flat on the struct. Both
   fixed and reconfirmed passing. Don't rediscover these as "new" findings when this phase's actual
   async-I/O work begins — the baseline now compiles clean, so any future compile error in this
   file is from the async rewrite itself, not leftover cruft.
3. Document in the README's platform-targets section that ESP-IDF network I/O did not previously
   respect `TimerProvider`-based timeouts mid-operation, and that this phase fixes it (or, if
   Option A can't fully deliver preemptible I/O, document the remaining limitation honestly instead
   of leaving it unstated).

**Ordering:** independent of Phases 0–2. Its outcome (specifically, whether Option B gets picked up
later) affects how Phase 5's ESP-IDF leg is designed — do this phase before Phase 5.

---

## Phase 4 — Make the TLS-version safety guarantee platform-general

**Problem.** The README claims: if a misconfigured `TlsConnector` negotiates TLS 1.3 on a model
that requires 1.2 (P2S, X2D), `BambuFtpsClient::connect()` returns `ProtocolViolation`
immediately. In reality this only works on tokio:
- `TokioTlsConnector::negotiated_version` overrides the default and reports the real negotiated
  version.
- `EmbassyTlsConnector` never overrides `TlsConnector::negotiated_version` — it silently falls back
  to the trait's default `None`, and `ftps/client.rs::connect()`'s check
  (`if let Some(version) = tls_connector.negotiated_version(...)`) is a no-op when it's `None` — the
  doc comment calls this "best-effort," but the effect is the advertised protection doesn't exist
  on Embassy at all.
- `SecureConnect` (ESP-IDF's trait) has **no version-query method whatsoever** — there's no hook
  to add a check even in principle without extending the trait.

**Tasks:**
1. Add a `negotiated_version` method to `SecureConnect`, mirroring `TlsConnector`'s (default
   `None`, same as the existing trait), so both connection-establishment traits expose the same
   capability. `BambuFtpsClient`'s enforcement check only needs to work for whichever trait its
   `Tls`/`Conn` type parameter actually is at the call site (it's `Tls: TlsConnector<RawIO>` today
   in `ftps/client.rs` specifically — this task is about closing the *capability* gap so it's
   available if/when Phase 5's ESP-IDF FTPS work needs it; note the actual wiring depends on how
   Phase 3's Option A/B decision shapes the ESP-IDF connection story).
2. Implement a real `negotiated_version` for `EmbassyTlsConnector` — investigate `embedded-tls`
   0.19's `TlsConnection`/handshake state for whatever it exposes about the negotiated protocol
   version (it may only ever speak TLS 1.2, in which case the impl is a trivial constant — verify
   this against the crate rather than assuming).
3. Implement `negotiated_version` for whatever ESP-IDF's connector looks like post-Phase-3 —
   investigate whether `esp_tls`/mbedTLS exposes a queryable negotiated protocol version through
   `esp-idf-svc`'s bindings (e.g. via `mbedtls_ssl_get_version`-equivalent) or whether it requires
   raw FFI.
4. Update the README to either (a) confirm the guarantee is now genuinely platform-general, or
   (b) if some platform still can't report the version, say so explicitly instead of stating the
   guarantee unconditionally.

**Ordering:** depends on Phase 3 (ESP-IDF's connection story needs to be settled first, since this
phase's ESP-IDF task is shaped by that decision). Independent of Phases 0–2, but do it before
Phase 5 so FTPS-on-embedded ships with the safety net actually working, not silently degraded like
it is on tokio-adjacent platforms today.

---

## Phase 5 — FTPS on embedded targets (Embassy + ESP-IDF)

**Problem this closes.** FTPS is currently tokio-only in practice: `FtpDataStreamFactory` has
exactly one real implementation, `TokioFtpDataStreamFactory` (plus test/dummy impls) — grep
confirms no Embassy or ESP-IDF factory exists anywhere in the crate, despite `PrinterClient` and
`BambuFtpsClient` being fully generic over `RawIO`/`Tls`/`Factory` as if portability were already
there. It wasn't a simple oversight — the two real platform-specific blockers are:
- **Embassy:** fixed by Phase 2. Before that phase, two concurrent TLS connections (control +
  data channel, which most models need — see Phase 2's problem statement) panic the firmware.
- **ESP-IDF:** `BambuFtpsClient<RawIO, Tls, Factory>` requires `Tls: TlsConnector<RawIO>` — a
  "wrap an existing raw stream" trait. ESP-IDF has never implemented `TlsConnector`, only
  `SecureConnect` ("dial your own connection"). Depending on how Phase 3 resolved (Option A vs B),
  ESP-IDF either still can't satisfy `TlsConnector` at all (Option A world) or gained the ability to
  (Option B world, if the mbedTLS-wrap spike succeeded).

**Decide first, informed by Phase 3's outcome:**
- If Phase 3 stayed on **Option A** (SecureConnect made async, but still dial-only): ESP-IDF cannot
  satisfy `BambuFtpsClient`'s `Tls: TlsConnector<RawIO>` bound as written. Either (a) accept ESP-IDF
  FTPS is not implementable until Phase 3's Option B lands, and scope this phase to Embassy only, or
  (b) redesign `BambuFtpsClient` to be generic over *either* connection-establishment trait (more
  invasive — likely needs an internal enum/adapter so the data-channel connect step can go through
  `SecureConnect::secure_connect(host, port)` instead of `Tls::connect(host, port, raw_stream)` when
  running on ESP-IDF). Recommendation: (a) — ship Embassy FTPS now, treat ESP-IDF FTPS as blocked on
  the Phase 3 spike rather than forcing a `BambuFtpsClient` redesign to route around it.
- If Phase 3 went **Option B** and ESP-IDF gained a real `TlsConnector` impl: ESP-IDF FTPS needs
  only a `FtpDataStreamFactory` impl (raw `std::net::TcpStream`-based, same shape as
  `TokioFtpDataStreamFactory` — ESP-IDF already uses `std::net::UdpSocket` elsewhere in
  `io/esp_idf.rs`, so `std::net::TcpStream` is equally available) plus wiring `Tls` to the new
  ESP-IDF `TlsConnector` impl.

**Tasks (Embassy leg — do this regardless of the ESP-IDF decision above):**
1. Implement an Embassy `FtpDataStreamFactory`: dial a raw TCP connection to the PASV-negotiated
   port using `embassy-net`'s TCP socket. Note `embassy-net::tcp::TcpSocket` needs pre-allocated
   rx/tx buffer slices at construction (same pattern as `EmbassyUdpSocket` already handles for UDP)
   — size these per the same caller-supplied-buffer philosophy landed in Phase 2, not another
   static singleton.
2. Wire an `EmbassyFtpDataStreamFactory` + the (post-Phase-2) `EmbassyTlsConnector` into a
   `PrinterClient::with_ftps(...)` call, and add an integration test or example showing FTPS working
   end-to-end on Embassy (as close to real hardware as the test setup allows — likely against a
   loopback or mock TLS-terminating server, matching however the crate's existing Embassy tests, if
   any, are structured).
3. Update the README's Embassy section to show FTPS usage (currently silent on it), and update the
   "Not yet implemented" / platform-targets sections to reflect the new capability.

**Tasks (ESP-IDF leg — only if Phase 3 delivered Option B, or once it does):**
4. Implement `EspIdfFtpDataStreamFactory` using `std::net::TcpStream`.
5. Wire it with ESP-IDF's `TlsConnector` impl into `PrinterClient::with_ftps(...)`.
6. Same README updates as task 3, for ESP-IDF.

**Ordering:** depends on Phase 2 (Embassy leg) and Phase 3 + Phase 4 (ESP-IDF leg, and Phase 4 for
the TLS-1.2-enforcement quirk to actually mean something on both new platforms). This is the
capstone phase — do it last.
