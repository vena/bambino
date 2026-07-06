# Pre-Release Review — Implementation Plan

Fixes every finding in `PRE_RELEASE_REVIEW.md`, grouped into phases by severity. Each
phase's sub-items are independent of each other (different files/functions) unless a
sub-item explicitly says otherwise — a session can pick up any subset of a phase, or run
phases out of order, except where a dependency is called out.

## Ground rules for every phase

- Verify with `make check-fast` (build + test + `alloc`/`embassy` lib checks + clippy)
  after every change. This does **not** yet cover the CLI binary — see Phase 2.3, which
  fixes that gap; until it lands, also manually run
  `cargo build --bin bambino-cli --features cli` and `cargo clippy --bin bambino-cli
  --features cli` for any change touching `src/bin/bambino-cli/`.
- Changes to `src/io/esp_idf.rs` need `scripts/check-esp-idf.sh` (Docker-based, ~5.5 min
  cold) in addition to `make check-fast`, since plain `cargo check` cannot compile the
  `esp-idf` feature at all (needs the ESP-IDF SDK toolchain). Run it yourself before
  calling an ESP-IDF change done.
- Per `CLAUDE.md`'s hardware-verification rule: **do not** self-verify against real
  printer/ESP32 hardware even if credentials are present in your environment — ask the
  user to run the test and report back. This applies narrowly to changes that alter the
  *shape* of wire reads/writes or wrap an existing read/connect in new timeout/retry
  logic. In this plan, that's **Phase 1.2** (ESP-IDF non-blocking connect) and
  **Phase 3.2** (FTPS per-operation timeouts) — both change how/when bytes are read from
  a live socket. Everything else in this plan (validation, clamping, doc fixes, dedup,
  new quirks methods) is not in that class and needs no hardware pass.
- Never write real access codes/serials into files, tests, or commits (existing repo
  convention).
- `serde_json::to_vec` (not `to_string`) for any new payload serialization; `log::` macros
  not `println!` in library code (CLI code under `src/bin/` may use `println!`).

---

## Phase 1 — Critical

### 1.1 RTSPS URL userinfo-injection via unvalidated `ip` (`src/camera/rtsps.rs`)

**File:** `src/camera/rtsps.rs`, function `build_rtsps_url` (currently ~line 65).

**Bug:** `access_code` is validated (non-empty ASCII alphanumeric) before being
interpolated into `format!("rtsps://bblp:{}@{}:322/streaming/live/1", access_code, ip)`,
but `ip` is not. If `ip` contains an `@` (e.g. it came from an SSDP/mDNS discovery
response on an untrusted LAN, spoofable by any device on the subnet), standard
userinfo/host-splitting URL parsing treats everything up to the *last* `@` as userinfo —
an `ip` like `"1.2.3.4@attacker.example.com"` produces a URL whose host resolves to
`attacker.example.com`, and the media client sends the LAN access code there.

**Fix:**
1. In `build_rtsps_url`, after the existing `access_code` check, validate `ip`. Two
   acceptable approaches — prefer (a):
   - (a) Parse `ip` with `core::net::IpAddr::from_str` (or `.parse::<IpAddr>()` if `std`
     is available; `core::net::IpAddr` implements `FromStr` under `no_std` too via the
     `core` re-export used elsewhere in this crate, e.g. `src/io/mod.rs`'s
     `core::net::SocketAddr` usage) and reject with `BambuError::ProtocolViolation` if it
     doesn't parse. This is the strictest fix and matches "genuine printer IPs are always
     valid IPv4/IPv6 addresses" — same reasoning `build_rtsps_url` already applies to
     `access_code`.
   - (b) If some caller legitimately needs to pass a hostname (not just a literal IP) —
     check call sites first via `ctx_search`/grep for `build_rtsps_url(` before assuming
     (a) is safe) — fall back to rejecting structural URL characters instead:
     `@`, `/`, `\`, whitespace, and control characters. Only take this path if (a) breaks
     a real caller.
2. Add unit tests mirroring the existing `access_code` rejection tests:
   `test_build_rtsps_url_rejects_ip_with_embedded_at`,
   `test_build_rtsps_url_rejects_non_ip_hostname` (if you took approach (a)) — assert
   `Err(BambuError::ProtocolViolation(_))` for `ip = "1.2.3.4@attacker.example.com"` and
   confirm the existing `test_build_rtsps_url` (valid IP) still passes unchanged.
3. Do **not** touch `rewrite_rtsp_request_uri` in this sub-item — its lower-severity twin
   bug is Phase 4.8 (no credential involved there, lower urgency, and it has different
   call-site constraints — see that item).

**Acceptance:** `build_rtsps_url("1.2.3.4@evil.com", "12345678")` returns
`Err`. `build_rtsps_url("192.168.1.150", "12345678")` still returns the exact same
`Ok` string as before.

---

### 1.2 ESP-IDF `dial()` blocks the whole task, defeating `connect_timeout_secs` (`src/io/esp_idf.rs`)

**Files:** `src/io/esp_idf.rs` — `EspIdfTcpStream::connect` (currently ~line 439) and
`EspIdfRawStreamFactory::dial` (currently ~line 620).

**Bug:** `dial()` is declared `async fn` but its body is one call to synchronous
`std::net::TcpStream::connect((host, port))` with zero `.await` points. `race()`
(`src/io/mod.rs`) cooperatively polls two futures on the same task via `poll_fn`; a future
with no internal yield point runs to completion the instant it's first polled — it never
gives the `timer.sleep(connect_timeout_secs)` half of `race_against_connect_timeout`
(`src/client/connect.rs`) a chance to run. A printer that's off, on another subnet, or
behind a silent packet-dropping firewall hangs the whole task for however long the
underlying OS/lwIP connect takes — not the documented `connect_timeout_secs`. This
silently breaks the `connect_timeout_secs` guarantee for `ensure_mqtt()`/`ensure_ftps()`/
`ensure_camera()` on ESP-IDF specifically (Embassy openly documents having no built-in
connect timeout; this gap is undocumented and contradicts a documented guarantee).

**Design decision — read before coding:** the crate has an established two-part pattern
for exactly this problem, already used by `EspIdfTlsConnector::connect` (same file,
handshake phase): (1) put the socket into non-blocking mode, (2) initiate the operation,
(3) loop: check for completion, and if not yet complete, `.await` a short
`EspIdfTimer::sleep(TLS_POLL_INTERVAL)` before retrying. That `.await` inside the loop is
what gives `race()` a real yield point, letting the outer `connect_timeout_secs` race
actually preempt it. Apply the same shape to the TCP `connect()` step itself, not just the
TLS handshake that currently follows it.

**Concrete implementation approach (recommended):**
1. `std::net::TcpStream` cannot be un-blocked mid-`connect()` — the nonblocking flag must
   be set *before* `connect()` is called, on a not-yet-connected socket, which
   `std::net::TcpStream::connect()`'s all-in-one API doesn't allow. Add the `socket2`
   crate as a new optional dependency gated by the `esp-idf` feature only (mirrors how
   `esp-idf-svc` itself is gated — add `socket2 = { version = "0.5", optional = true }`
   to `[dependencies]` in `Cargo.toml`, then `esp-idf = ["dep:esp-idf-svc", "dep:socket2",
   "std"]`). Verify the current `socket2` major version via the `find-docs`/context7 skill
   before pinning — do not guess the version from training data.
2. In `EspIdfTcpStream::connect` (or a new free function it delegates to), replace the
   direct `TcpStream::connect` call with:
   - Build a `socket2::Socket` for the resolved address family (`Domain::for_address` on
     the first resolved `SocketAddr` from `(host, port).to_socket_addrs()` — resolve the
     hostname manually since `socket2::Socket` connects to a `SockAddr`, not a
     `(host, port)` tuple).
   - `socket.set_nonblocking(true)?`.
   - Call `socket.connect(&addr.into())`. On a non-blocking socket this returns
     immediately with `Err` where `.kind() == WouldBlock` (or raw `EINPROGRESS` depending
     on how `socket2` surfaces it — check `socket2`'s docs via `find-docs`) instead of
     completing synchronously.
   - Loop: call `socket.take_error()` (returns `Ok(None)` if the connect completed
     successfully, `Ok(Some(e))` if it failed, and you'll need to also handle the
     "still in progress" case — check `socket2`'s recommended poll idiom via its docs,
     historically this is done by attempting to write 0 bytes or checking `SO_ERROR` via
     `take_error()` after a writability check). Between attempts, `.await` an
     `EspIdfTimer::sleep(TLS_POLL_INTERVAL)` (reuse the existing constant — 20ms — no new
     poll-interval constant needed).
   - On success, convert `socket2::Socket` into `std::net::TcpStream` via
     `std::net::TcpStream::from(socket)` (socket2 provides this `From` impl on Unix-like
     targets, which ESP-IDF's std target is) and wrap it in `EspIdfTcpStream(Some(stream))`
     as before.
   - Do **not** add an internal timeout/budget to this loop — leave it looping until
     success or a real error. Bounding is already handled by the *outer*
     `race_against_connect_timeout` in `ensure_mqtt()`/`ensure_ftps()`/`ensure_camera()`,
     which can now actually preempt this future because it has real `.await` points. This
     matches the plain (non-connector-owned) design `RawStreamFactory::dial` has on every
     other platform (`TokioRawStreamFactory::dial`, `EmbassyRawStreamFactory::dial` — check
     the latter for the exact shape) — none of them carry their own timeout either.
   - **Optional, for parity with `EspIdfTlsConnector`'s existing two-layer timeout
     precedent** (documented in `CLAUDE.md` under "Connect-phase timeouts: two layers, not
     one"): also give `EspIdfRawStreamFactory` its own `connect_timeout: Duration` field
     (default `DEFAULT_CONNECT_TIMEOUT`, already defined in this file) plus a
     `.with_connect_timeout(d)` builder, for direct (non-`PrinterClient`) consumers of this
     factory who want a bound without going through `PrinterClient`. Not required for
     correctness (the critical bug is fixed either way) — do this only if time allows.
3. Map every `socket2`/`io::Error` failure path through the existing
   `to_esp_socket_error`/`map_std_io_error` helpers, not a new ad hoc mapping.
4. Add a unit test if feasible under `#[cfg(feature = "esp-idf")]` — likely limited value
   since `esp-idf-svc` types can't run outside the ESP-IDF Docker toolchain; a `cargo
   check --features esp-idf` won't compile at all without the SDK. Focus verification
   effort on `scripts/check-esp-idf.sh esp32c6` instead, and note in your summary that the
   *connect-timeout-actually-fires* behavior (the whole point of this fix) is only
   provable on real ESP32 hardware or in a from-scratch integration test inside the Docker
   toolchain — flag this to the user for manual confirmation per the ground rules above.

**Fallback if `socket2` doesn't build under the ESP-IDF target:** fall back to raw
`libc`-style syscalls via `esp_idf_svc::sys` (the vendored ESP-IDF bindgen bindings
already used elsewhere in this file, e.g. `esp_idf_svc::sys::EWOULDBLOCK`) — `connect()`,
`fcntl()`/`ioctl(FIONBIO)` for non-blocking, `getsockopt(SO_ERROR)` for completion
checking, on the raw fd obtained via `std::os::fd::AsRawFd`. This is more code but has no
new-dependency risk. Only go this route if `socket2` genuinely fails to compile —
verified by actually trying it first, not by assumption.

**Acceptance:** `scripts/check-esp-idf.sh esp32c6` passes. `make check-fast` still passes
(this change is entirely behind `#[cfg(feature = "esp-idf")]`, so it must not affect the
default/tokio build at all — if `Cargo.toml`'s `esp-idf` feature line is touched, double
check `cargo build` with default features still succeeds with no new unused-optional-dep
warnings).

---

## Phase 2 — High severity, independent (transport / MQTT / build tooling)

### 2.1 ESP-IDF has no TLS-1.2-forcing option (`src/io/esp_idf.rs`)

**Files:** `src/io/esp_idf.rs` (`EspIdfTlsCerts`, `build_tls_config`, `EspIdfTlsConnector`).

**Bug:** `esp_idf_svc::tls::Config` (as vendored, `esp-idf-svc-0.52.1/src/tls.rs`) has no
min/max TLS version field, and `build_tls_config` never attempts to constrain the
negotiated version. `ftps/client.rs`'s `require_tls_1_2_if_enforced` fail-closed guard
rejects any FTPS connection where `model.quirks().enforce_ftps_tls_1_2()` is true (several
models per `MODEL_MATRIX.csv`: p1, p2, x2, a1, a2, h2, one x1 variant) unless the
negotiated version is exactly TLS 1.2. On ESP-IDF, if the printer's vsFTPd
offers/prefers TLS 1.3, there is currently no way to force TLS 1.2 — the connection is
permanently rejected. Compare with `src/io/tokio.rs`'s
`build_unsafe_client_config_with_options(force_tls_1_2: bool)` /
`build_verified_client_config_with_options(..., force_tls_1_2: bool)`, which already solve
this for the tokio backend by restricting `rustls`'s `with_protocol_versions()` call.

**Fix:**
1. Check first whether `esp_idf_svc::tls::Config` (0.52.1, as already vendored in this
   repo's lockfile) exposes *any* version-constraint knob under a different field name —
   re-read the actual vendored source at
   `esp-idf-svc-0.52.1/src/tls.rs` (find its path via `cargo metadata` or
   `find ~/.cargo -path '*esp-idf-svc-0.52.1/src/tls.rs'`) rather than trusting the
   review's claim blindly; APIs can have version-specific fields under different names
   (e.g. `min_tls_version`, or a raw `mbedtls_ssl_config` escape hatch). If a genuine knob
   exists, use it directly — much simpler than the fallback below.
2. If no such knob exists in the safe wrapper: `esp_idf_svc::tls::Config` almost certainly
   still exposes a way to reach the raw `mbedtls_ssl_config` (either a field or via
   `EspTls::context_handle()`, already used in this file by
   `query_negotiated_tls_version` to call `esp_tls_get_ssl_context`). mbedTLS's C API for
   this is `mbedtls_ssl_conf_min_version`/`mbedtls_ssl_conf_max_version` — check whether
   `esp_idf_svc::sys` exposes these bindgen'd functions (it should, since `sys` is a raw
   bindgen crate over the whole ESP-IDF C surface). If so, add a `force_tls_1_2: bool`
   field to `EspIdfTlsCerts` (and thread it through `EspIdfTlsConnector::new()`/
   `with_certs()`/a new `.with_force_tls_1_2(bool)` non-consuming builder, mirroring
   `io/tokio.rs`'s options-suffixed function pattern), and inside
   `EspIdfTlsConnector::connect` — **after** `EspTls::adopt()` but **before** the
   `negotiate()` loop — call `mbedtls_ssl_conf_max_version`/`min_version` on the context
   obtained the same way `query_negotiated_tls_version` does, pinning to TLS 1.2 when the
   flag is set. This mirrors Embassy's own documented approach (check
   `src/io/embassy.rs`'s existing `force_tls_1_2`-equivalent handling of `embedded-tls`, if
   any, for a consistent naming/shape precedent before inventing new names).
3. If mbedTLS's raw config truly isn't reachable at all (verify by attempting to compile
   against the real bindgen output via `scripts/check-esp-idf.sh` — don't assume), the
   remaining honest option is to document the limitation explicitly on
   `EspIdfTlsConnector`'s doc comment (mirroring how `src/io/embassy.rs` already documents
   "no built-in connect timeout" as an accepted platform gap) rather than silently doing
   nothing — at minimum this closes the "Embassy documents its gap, ESP-IDF doesn't" half
   of the finding. Only fall back to documentation-only if the raw-config path is
   genuinely unreachable; prefer a real fix.
4. Add a unit test analogous to `io/tokio.rs`'s
   `test_build_verified_client_config_with_options_tls12` if the new code path is testable
   outside the ESP-IDF Docker toolchain (likely only the builder/field-plumbing is
   testable on host; the actual mbedTLS call is not). If not testable on host, rely on
   `scripts/check-esp-idf.sh` for compilation, and flag to the user that a live TLS-1.3
   printer test is the only way to prove the constraint actually holds (again, not
   something to self-verify against real hardware — ask the user).

**Acceptance:** `scripts/check-esp-idf.sh esp32c6` passes with the new option compiled in.
Either a working `force_tls_1_2` path exists on ESP-IDF, or the limitation is explicitly
documented on `EspIdfTlsConnector` (not silently absent).

---

### 2.2 `ams_mapping2` serializes even when computed `use_ams` is false (`src/mqtt/commands/print_job.rs`)

**File:** `src/mqtt/commands/print_job.rs` — `PrintJobConfig::with_ams_mapping2` (currently
~line 76) and `ProjectFileRequest::from_config` (currently ~line 178).

**Bug:** Two related defects in the same area:
1. `with_ams_mapping2()` sets `self.ams_mapping2 = Some(mapping2)` but never sets
   `self.use_ams = true` — unlike `with_ams()`, which does set `use_ams = true`. A caller
   using only `.with_ams_mapping2(...)` (no `.with_ams(...)`) gets `config.use_ams == false`
   going into `from_config`.
2. In `from_config`, the computed `use_ams` local (which folds in
   `validate_external_spool_safety`) correctly drives `ams_mapping`'s
   `Active`/`Inactive` collapse, but `ams_mapping2` is passed through unconditionally via
   `ams_mapping2: config.ams_mapping2.clone()` regardless of the computed `use_ams` value.
   If the safety interlock trips (`validate_external_spool_safety` returns `false`) or a
   caller hits bug (1) above, the printer receives
   `{"use_ams":false,"ams_mapping":"","ams_mapping2":[...]}` — internally contradictory,
   the same shape the reference doc documents as causing firmware error
   `0700_8012 "Failed to get AMS mapping table"`.

**Fix:**
1. In `from_config`, change the final struct literal's `ams_mapping2` field from
   `config.ams_mapping2.clone()` to a value gated on the computed `use_ams`:
   ```rust
   ams_mapping2: if use_ams { config.ams_mapping2.clone() } else { None },
   ```
   Since the field is already `#[serde(skip_serializing_if = "Option::is_none")]`, this
   makes the key vanish entirely from the wire payload when AMS is inactive — consistent
   with `ams_mapping` collapsing to `Inactive("")`.
2. Additionally, for symmetry with `with_ams()` and so a bare `.with_ams_mapping2(...)`
   call (no `.with_ams(...)`) does the intuitive thing, set `self.use_ams = true` inside
   `with_ams_mapping2()` too:
   ```rust
   pub fn with_ams_mapping2(mut self, mapping2: Vec<AmsMapping2Entry>) -> Self {
       self.use_ams = true;
       self.ams_mapping2 = Some(mapping2);
       self
   }
   ```
   This is the smaller, more localized half of the fix; step 1 alone already prevents the
   contradictory-payload bug even without step 2, but do both — the review explicitly asks
   for "and/or", and both changes are cheap and consistent with existing conventions.
3. Add regression tests in `src/mqtt/commands/mod.rs`'s `#[cfg(test)]` block (alongside the
   existing `test_ams_mapping_polymorphism_*` tests), e.g.:
   - `test_ams_mapping2_omitted_when_use_ams_false`: build a `PrintJobConfig` via
     `.with_ams_mapping2(vec![...])` only (no `.with_ams`), call `from_config`, assert the
     serialized JSON does **not** contain `"ams_mapping2"` at all if your fix routes
     through the safety interlock in a way that still yields `use_ams == false` (e.g. via
     an all-external-spool `ams_mapping2`, mirroring
     `test_ams_mapping_all_external_spool_overrides_use_ams_false_single_nozzle`'s
     pattern) — or, simpler, assert `use_ams` is `true` and `ams_mapping2` **is** present
     when only `.with_ams_mapping2()` is used with no interlock trip (this directly tests
     fix step 2).
   - `test_ams_mapping2_dropped_when_safety_interlock_trips`: construct a config with
     `.with_ams_mapping2(...)` where `validate_external_spool_safety` should trip and force
     `use_ams` to `false` (reuse whatever fixture the existing
     `test_ams_mapping_all_external_spool_overrides_use_ams_false_single_nozzle` test
     uses, adapted to populate `ams_mapping2` instead of/alongside `ams_mapping`), assert
     the serialized JSON contains `"use_ams":false"` and does **not** contain
     `"ams_mapping2"`.

**Acceptance:** No code path can produce `use_ams:false` alongside a present
`ams_mapping2` array. `cargo test` passes including the two new tests.

---

### 2.3 `make check-fast` never builds/lints the CLI binary (`Makefile`)

**File:** `Makefile` (`check-fast` target).

**Bug:** `check-fast` runs `cargo build` (default features — excludes `cli`), `cargo
test`, the `alloc`/`embassy` lib checks, and `cargo clippy` (also default features). None
of these compile `src/bin/bambino-cli/` or its `cli`-gated dependencies
(`crossterm`, `clap`, `env_logger`, `time`). `.github/workflows/ci.yml` just invokes `make
check-fast`, so it inherits the same gap (though note: this repo has no GitHub remote yet,
so that workflow doesn't currently run anywhere — see `CLAUDE.md`). `CLAUDE.md` itself
documents `cargo build --bin bambino-cli --features cli` as a required command, but
`check-fast`'s own docstring claims it "runs all of the above... in one command," which is
false today.

**Fix:** Add the missing build and clippy invocations to `check-fast`:
```makefile
check-fast:
	cargo build
	cargo build --bin bambino-cli --features cli
	cargo test
	cargo build --no-default-features --features alloc --lib
	cargo check --no-default-features --features embassy --lib
	cargo clippy
	cargo clippy --bin bambino-cli --features cli
```
Order matters only cosmetically (fail-fast on the cheapest checks first is fine); keep the
existing plain `cargo build`/`cargo clippy` calls too since default-feature library
consumers must still work without `cli`. Do **not** add `cargo test --features cli`
unless `src/bin/bambino-cli/` actually has tests to run (check first — if there are none,
adding it just wastes time recompiling the binary a second time for no test coverage
gain).

**Acceptance:** Run `make check-fast` locally end-to-end once after the edit — it must
still pass, and must now fail if you temporarily break something under
`src/bin/bambino-cli/` (spot-check this by introducing and then reverting a trivial
compile error in `src/bin/bambino-cli/main.rs` to confirm the new line actually catches
it, then revert).

---

## Phase 3 — High severity, FTPS hang/OOM protection

### 3.1 `read_to_eof` has no maximum size cap (`src/ftps/protocol.rs`)

Do this sub-item **before** 3.2 — it's small, independent, and unblocks the more invasive
3.2 without being entangled in its design decision.

**File:** `src/ftps/protocol.rs`, function `read_to_eof` (currently ~line 259).

**Bug:** `read_to_eof` accumulates into an unbounded `Vec<u8>` via
`out.extend_from_slice(&chunk[..n])` with no maximum-size check, unlike camera's
`CAMERA_FRAME_MAX_SIZE` guard (whose doc comment explicitly warns that unbounded
allocation triggers an uncatchable `alloc_error_handler` abort on `no_std`/Embassy
targets, not a recoverable `Result`). Used by `list_directory`'s listing payload and
`download_file`'s file payload. A misbehaving printer, a MITM, or a very large
timelapse/listing response that never sends EOF grows the buffer without bound.

**Fix:**
1. Add a new constant near the other `FTPS_*`/`FTP_*` constants in this file:
   ```rust
   /// Maximum bytes accepted from a single FTPS data-channel transfer (`list_directory`'s
   /// listing payload, `download_file`'s file payload) before `read_to_eof` aborts with
   /// `ProtocolViolation` rather than growing `out` without bound. Mirrors
   /// `CAMERA_FRAME_MAX_SIZE`'s rationale (`src/camera/binary.rs`) — unbounded allocation
   /// on a no_std/Embassy target hits the uncatchable `alloc_error_handler` abort, not a
   /// recoverable `Result`. Chosen generously for legitimate large downloads (multi-hundred-MB
   /// timelapse videos) while still bounding worst case; override reasoning if a real caller
   /// needs larger files — there is currently no builder to raise it (see below).
   pub(crate) const FTPS_MAX_TRANSFER_BYTES: usize = 512 * 1024 * 1024; // 512 MiB
   ```
   Pick the exact number by checking whether this crate/its docs mention a realistic
   maximum timelapse or `.3mf`/gcode file size anywhere (grep `reference/*.md` for
   "timelapse" or file-size mentions) before committing to 512 MiB — adjust to match real
   documented limits if you find a more authoritative number.
2. In `read_to_eof`, check `out.len()` (or `out.len() + n`) against the cap on each
   iteration and return `Err(BambuError::ProtocolViolation("FTPS transfer exceeds maximum
   accepted size".into()))` before extending past it — mirror the exact pattern
   `BambuBinaryCameraStream::read_next_frame_with_timer` uses for its `max_frame_size`
   check (`src/camera/binary.rs`).
3. Decide whether this needs to be caller-configurable like camera's
   `with_max_frame_size` (embedded targets may want a much smaller cap given tighter
   memory budgets — no_std/Embassy is a real target for this crate per `CLAUDE.md`). If
   you add configurability, follow the exact existing convention: a non-consuming
   `.with_max_transfer_size(bytes)` builder on `BambuFtpsClient`, defaulting to
   `FTPS_MAX_TRANSFER_BYTES`, storing it in a new `max_transfer_bytes: usize` field
   threaded through `connect()`. If you skip configurability for now (acceptable — a flat
   constant is still a strict improvement over "no cap at all"), state clearly in your
   summary that this is a fixed constant, not yet configurable, so a future session
   doesn't need to rediscover that.
4. Add unit tests mirroring camera's oversized-frame test shape:
   `test_read_to_eof_rejects_oversized_transfer` — feed a mock stream that keeps
   returning nonzero reads past the cap, assert `Err(BambuError::ProtocolViolation(_))`.

**Acceptance:** `list_directory`/`download_file` against a mock stream that never sends
EOF and exceeds the cap return a clean error instead of growing memory unbounded.
Existing `tests/ftps_test.rs` integration tests still pass unmodified (real transfers are
far under the cap).

---

### 3.2 No FTPS operation has a per-read wall-clock deadline (`src/ftps/protocol.rs`, `src/ftps/client.rs`)

**Files:** `src/ftps/protocol.rs` (`read_line_raw`, `read_to_eof`), `src/ftps/client.rs`
(`BambuFtpsClient` struct and every method).

**Bug:** `read_line_raw` and `read_to_eof` call `stream.read(&mut chunk).await` directly,
never through `io::read_chunk`/`race` the way MQTT's `poll_wire` and camera's
`read_next_frame_with_timer` do. `BambuFtpsClient` has no `Timer` type parameter at all.
If a printer stalls mid-transfer (firmware hang during microSD flush, after `150`/`125`
but before `226`), `list_directory`/`upload_file`/`download_file`/`get_available_space`
block the calling task indefinitely — `PrinterClient::connect_timeout_secs` only bounds
the one-time dial+login sequence (`ensure_ftps()`), not any of these post-connect calls.

**This is a "decide first" item — read this whole section before writing code.** The
design constraint that makes this harder than MQTT's/camera's equivalent fix: MQTT and
camera each own a single long-lived object (`BambuMqttClient`, `BambuBinaryCameraStream`)
that `PrinterClient` always mediates access to via methods that already receive
`&self.timer` at the call site (`poll_wire(&self.timer)`,
`read_next_frame_with_timer(..., &self.timer, ...)`). FTPS is different:
`PrinterClient::storage()` (`src/client/storage.rs`) hands the **caller** direct
`&mut BambuFtpsClient` access — `PrinterClient` does not mediate `list_directory`/
`upload_file`/etc. calls itself. So there is no call site inside `PrinterClient` where
`&self.timer` could be threaded through to an FTPS method the way it is for MQTT/camera —
`BambuFtpsClient` needs to **own** a timer instance itself, for the entire lifetime of the
client, independent of whatever borrowed `PrinterClient` handed it out.

**Two design options for how `BambuFtpsClient` acquires that owned timer:**

- **Option A — reuse `PrinterClient`'s `Timer` type param, require `Clone`.** Add a
  `Clone` bound to `Timer: TimerProvider` wherever `PrinterClient::ensure_ftps()` builds a
  `BambuFtpsClient`, and pass `self.timer.clone()` into `BambuFtpsClient::connect()`.
  Rejected as the primary recommendation: `EspIdfTimer` wraps
  `RefCell<esp_idf_svc::timer::EspAsyncTimer>`, which is not `Clone` (and can't cheaply be
  made so — a real `Clone` impl would need to fallibly construct a brand new
  `EspTimerService`/`EspAsyncTimer`, and `Clone::clone` can't return `Result`). Making it
  `Clone` would require wrapping the inner timer in `Rc<RefCell<...>>` — a real refactor of
  a type that ships today, for a feature (FTPS timeouts) that doesn't strictly need it.
- **Option B (recommended) — give `BambuFtpsClient` its own independent `Timer` type
  parameter, constructed fresh, no `Clone` bound anywhere.** This exactly matches
  existing precedent already in this codebase: `EspIdfTlsConnector::connect` already
  constructs a brand new `EspIdfTimer::new()` internally (`src/io/esp_idf.rs`) rather than
  reusing one passed in from outside, and `TokioTimer::new()`/`EmbassyTimer` (unit struct)
  are trivially constructed fresh anywhere too. Add a new type parameter to
  `BambuFtpsClient<RawIO, Tls, Factory, FtpsTimer = DummyTimer>`, add a `timer: FtpsTimer`
  field, thread it through `connect(raw_control, tls_connector, data_factory, model, ip,
  access_code, timer)` (new trailing parameter), and require the caller building a
  `PrinterClient` to supply one at `.with_ftps(tls, factory, timer)` — a signature change
  to that existing consuming builder (acceptable pre-1.0 breaking change, same category as
  the other signature changes already documented in `CLAUDE.md`). Update
  `PrinterClient`'s own type parameter list to add a matching `FtpsTimer: TimerProvider`
  slot (defaulted to `DummyTimer`, mirroring how `FtpsRawIO`/`FtpsTls`/`FtpsFactory` are
  already defaulted), and update every method in `src/client/connect.rs`/`storage.rs` that
  names the full `PrinterClient<...>` parameter list.

Go with **Option B**. It has a wider blast radius (touches `PrinterClient`'s type
parameter list, which several files reference explicitly), but no `Clone` bound landmine
and no risk of silently breaking `EspIdfTimer` semantics. Do the full sweep in one pass —
partial application (e.g. only `BambuFtpsClient` changed, `PrinterClient` not updated to
match) leaves the crate non-compiling.

**Implementation steps:**
1. `src/ftps/protocol.rs`: change `read_line_raw` and `read_to_eof` to accept
   `timer: &T` and `deadline_ms: Option<u64>` parameters (exact shape of
   `io::read_chunk`'s own signature) and route their `stream.read(...)` calls through
   `crate::io::read_chunk` instead of calling `stream.read` directly. Add a
   `FTPS_READ_TIMEOUT_SECS` constant (`pub(crate)`, suggest 30s to match
   `MQTT_READ_TIMEOUT_SECS`/`CAMERA_READ_TIMEOUT_SECS` for consistency, though FTPS
   transfers can legitimately be slower for large files — consider whether a longer
   default, e.g. 60s, is more appropriate given `upload_file`'s own doc comment already
   mentions waiting "up to 300 seconds for the `226` transfer confirmation" for microSD
   flush latency; if you use a shorter per-*chunk* deadline than that 300s figure, make
   sure the *chunk*-level deadline resets on every partial read the way MQTT's
   `MQTT_READ_TIMEOUT_SECS` does per read-step, not per whole logical transfer — a stalled
   printer producing zero bytes should time out well under 300s, but a slow-but-live
   transfer trickling in a few bytes every few seconds should not).
2. `read_response` (which calls `read_line_raw` in a loop) needs the same `timer`/
   `deadline_ms` threading — update its signature and every call site.
3. `src/ftps/client.rs`: add the `FtpsTimer` type parameter to `BambuFtpsClient` as
   described in Option B above. Store `timer: FtpsTimer`. In every method that currently
   calls `read_response(...)`/`read_to_eof(...)`, compute a fresh deadline the same way
   `read_exact_packet` does (`let deadline_ms = if timer.has_real_clock() {
   Some(timer.now_millis().saturating_add(budget_ms)) } else { None };`) and pass it
   through. Respect `has_real_clock()` exactly like every other timeout in this crate —
   `DummyTimer` (the default) must continue to behave exactly as it does today
   (unbounded), so existing callers not using `PrinterClient` see zero behavior change.
4. `write_command` (`src/ftps/protocol.rs`) is a write, not a read — camera's
   `authenticate()` (Phase 4.9) has the same "writes have no deadline" gap; consider
   whether to fix both in the same pass for consistency, but they're independently
   assignable — don't block this item on that one.
5. Update `src/client/connect.rs`'s `ensure_ftps()` to pass a freshly-constructed
   `FtpsTimer` instance into `BambuFtpsClient::connect(...)` — where does that instance
   come from? `PrinterClient::with_ftps(tls, factory, timer)`'s new third parameter
   (supplied by the `PrinterClient` builder caller, stored in `ftps_config` alongside
   `tls`/`factory` as a 3-tuple instead of a 2-tuple) is the source; `ensure_ftps()` just
   moves it out of `self.ftps_config.take()` the same way it already does for `tls`/
   `factory`.
6. Update every call site across the crate that names `PrinterClient<...>`'s full type
   parameter list (grep for `PrinterClient<` — expect hits in `src/client/{mod,connect,
   storage,ams,camera,hardware,motion,print,telemetry,thermal}.rs`, and possibly
   `src/client/dummy.rs`/`src/client/types.rs`) to add the new `FtpsTimer` slot with its
   `DummyTimer` default preserved. Also update every call site in
   `src/bin/bambino-cli/storage.rs` (`printer.with_ftps(ftps_tls, TokioRawStreamFactory)`)
   to pass a `TokioTimer::new()` as the new third argument, and any test helper in
   `tests/` that constructs a `BambuFtpsClient` or a `PrinterClient::with_ftps(...)`
   directly.
7. Add regression tests mirroring MQTT's/camera's stalled-connection tests exactly:
   `test_read_line_raw_stalled_connection_times_out` (or at the `read_response` level) and
   a resume-without-losing-bytes test if you made `read_line_raw`'s partial-line state
   resumable across a timeout the way `FrameReadState`/`CameraFrameReadState` are — check
   whether `read_line_raw`'s existing `fill_buf`-carryover design already gives you
   resumability "for free" (bytes not yet consumed into a line stay in `fill_buf` across
   calls) or whether a timed-out `read_chunk` call mid-fill can lose the partial chunk (it
   should not, since `read_chunk` itself is one-`read()`-step-at-a-time and only returns
   already-completed data or a clean timeout — verify this reasoning against
   `read_chunk`'s own doc comment in `src/io/mod.rs` before assuming it "just works").

**Acceptance:** A mock stream that stalls with zero bytes after a `150`/`125` reply
causes `list_directory`/`upload_file`/`download_file` to return
`BambuError::NetworkError(SocketError::TimedOut)` within roughly `FTPS_READ_TIMEOUT_SECS`,
not hang forever. `DummyTimer`-based callers (anyone not using `PrinterClient` or not
calling `.with_timer()`) see no behavior change. Flag to the user per the ground rules
that this needs a real-hardware pass before being considered fully done (it changes read
granularity/timeout behavior on an already-working wire path).

---

## Phase 4 — Medium severity (independent, can be split across sessions freely)

### 4.1 Duplicated UDP multicast/broadcast setup (`src/io/tokio.rs` vs `src/io/esp_idf.rs`)

Extract a shared helper in `src/io/mod.rs` (guarded `#[cfg(feature = "std")]`, since both
call sites are std-based):
```rust
#[cfg(feature = "std")]
pub(crate) fn configure_std_udp_socket(socket: &std::net::UdpSocket) -> Result<(), SocketError> {
    if let Err(e) = socket.set_broadcast(true) {
        log::debug!("configure_std_udp_socket: set_broadcast failed: {e}");
    }
    let multiaddr = std::net::Ipv4Addr::new(239, 255, 255, 250);
    let interface = std::net::Ipv4Addr::new(0, 0, 0, 0);
    if let Err(e) = socket.join_multicast_v4(&multiaddr, &interface) {
        log::debug!("configure_std_udp_socket: join_multicast_v4 failed: {e}");
    }
    socket.set_nonblocking(true).map_err(|e| map_std_io_error(e, "failed to set UDP socket non-blocking"))
}
```
Replace the duplicated block in `TokioUdpSocket::bind` (`src/io/tokio.rs`) and
`EspIdfUdpSocket::bind` (`src/io/esp_idf.rs`) with a call to this helper. Preserve each
call site's existing post-helper steps (Tokio's `UdpSocket::from_std` conversion,
ESP-IDF's `EspIdfTimer::new()` construction) unchanged — only the setup block moves.
Verify `make check-fast` and `scripts/check-esp-idf.sh` both still pass.

### 4.2 Duplicated `embedded_io_async::Error::kind()` mapping (`TokioIoError` vs `EspIdfIoError`)

Both `src/io/tokio.rs::TokioIoError::kind` and `src/io/esp_idf.rs::EspIdfIoError::kind`
have an identical match over `std::io::ErrorKind`. Extract a shared
`pub(crate) fn map_io_error_kind(kind: std::io::ErrorKind) -> embedded_io_async::ErrorKind`
in `src/io/mod.rs` (guarded `#[cfg(feature = "std")]`) with the match body, and have both
`impl embedded_io_async::Error for ...::kind()` delegate to it (`map_io_error_kind(self.0.kind())`).

### 4.3 Duplicated WouldBlock-retry loop shape in `EspTlsStream::read`/`write` (`src/io/esp_idf.rs`)

The `read`/`write` impls for `EspTlsStream` are structurally identical (loop, match on
`self.tls.{read,write}(buf)`, same `is_would_block`/sleep/error-log arms), differing only
in which `EspTls` method is called and the log message text. Factor into one private
helper, e.g.:
```rust
async fn retry_on_would_block<S, F>(
    timer: &EspIdfTimer,
    op_name: &str,
    mut op: F,
) -> Result<usize, embedded_io_async::ErrorKind>
where
    F: FnMut() -> Result<usize, ::esp_idf_svc::sys::EspError>,
{
    loop {
        match op() {
            Ok(n) => return Ok(n),
            Err(e) if is_would_block(&e) => {
                timer.sleep(TLS_POLL_INTERVAL).await
                    .map_err(|_| embedded_io_async::ErrorKind::Other)?;
            }
            Err(e) => {
                log::debug!("ESP-IDF TLS {op_name} failed: {e}");
                return Err(embedded_io_async::ErrorKind::Other);
            }
        }
    }
}
```
Call it from both `read` and `write` with a closure capturing `self.tls.read(buf)` /
`self.tls.write(buf)` respectively (watch for the double-mutable-borrow issue — `self.tls`
and `self.timer` are both fields of `self`, so the helper likely needs to be a free
function taking `&mut EspTls<S>` and `&EspIdfTimer` separately rather than `&mut self`,
to avoid borrow-checker conflicts inside the closure). Verify with `scripts/check-esp-idf.sh`.

### 4.4 MQTT command constructors don't clamp `sequence_id` (all files under `src/mqtt/commands/`)

**Bug:** Every `pub fn new(..., sequence_id: u64)` constructor across `ams.rs`,
`control.rs`, `gcode.rs`, `hardware.rs`, `print_job.rs`, `status.rs` stores
`sequence_id.to_string()` directly with no clamping, unlike `subtask_id` (clamped via
`clamp_task_id()` in `print_job.rs`). The only in-repo caller
(`PrinterClient::next_sequence_id()`, `src/client/mod.rs`) already clamps before calling
these constructors, so the bug isn't reachable internally today — but every one of these
`new()` functions is `pub`, re-exported from `src/mqtt/mod.rs`, and thus public API an
external consumer can call directly with an unclamped value (e.g. a raw epoch-millisecond
`u64`), reproducing the documented 32-bit-overflow firmware lockup.

**Fix:** In every constructor listed above (`ams.rs`: `AmsFilamentSettingRequest::new`,
`AmsControlRequest::new`, `AmsGetRfidRequest::new`, `AmsChangeFilamentRequest::new`,
`AmsFilamentDryingRequest::new`; `control.rs`: `StandardControlRequest::new`,
`SkipObjectsRequest::new`, `CleanPrintErrorRequest::new`, `CalibrationRequest::new`,
`PrintSpeedRequest::new`; `gcode.rs`: `GCodeRequest::new`; `hardware.rs`:
`LedCtrlRequest::new`, `AirductRequest::new`, `PromptSoundRequest::new`,
`BuzzerRequest::new`; `status.rs`: `PushAllRequest::new`, `GetVersionRequest::new`),
replace `sequence_id.to_string()` with `crate::mqtt::commands::clamp_task_id(sequence_id).to_string()`
(the function already exists and is `pub`, just not applied at these call sites — it's
already imported via `super::clamp_task_id` in `print_job.rs`; add the same import where
missing). This is a purely internal, non-breaking change: `clamp_task_id` on an
already-clamped value (the only thing internal callers pass today) is a no-op
(`x % TASK_ID_MAX == x` when `x <= i32::MAX`), so no existing test's expected JSON output
changes.

**Acceptance:** Add one test (e.g. in `src/mqtt/commands/mod.rs`'s existing test module) that
calls one of these constructors — e.g. `GCodeRequest::new("G28", u64::MAX)` — directly with
an unclamped huge value and asserts the serialized `sequence_id` string is `<=
i32::MAX`'s string form, proving the clamp is applied even for external/direct callers.
Existing tests (which all pass already-small sequence IDs like `10001`) must still pass
byte-for-byte since clamping a small value is a no-op.

### 4.5 `tick_zombie_check` has zero production call sites (`bambino-cli`)

**File:** `src/bin/bambino-cli/monitor/mod.rs` (the `run` loop, ~line 71-117 based on the
existing `send_ping()` call at line 117).

**Bug:** `BambuMqttClient::tick_zombie_check(&mut self, elapsed_secs: u32)`
(`src/mqtt/client/mod.rs`) correctly detects write-zombie and stale-connection states, but
nothing in `bambino-cli` — the only shipped application built on this library — ever
calls it. The monitor loop calls `printer.send_ping()` on a timer but never
`tick_zombie_check`, so this safety-critical detection is inert end-to-end.

**Fix:**
1. Read the full monitor loop in `src/bin/bambino-cli/monitor/mod.rs` around the existing
   `send_ping()` call to understand its exact timing/loop structure (what tracks elapsed
   time between iterations, what the loop's tick interval is) before editing — don't guess
   the shape.
2. `tick_zombie_check` is a method on `BambuMqttClient`, not `PrinterClient` — reach it via
   `printer.mqtt().await?.tick_zombie_check(elapsed_secs)`, mirroring how
   `PrinterClient::mqtt()` is documented as the escape hatch for exactly this kind of
   direct-access need (see its doc comment in `src/client/mod.rs`).
3. Call it once per loop tick (same tick interval `send_ping()` already uses, or the
   underlying poll-loop's iteration rate — match whatever `elapsed_secs` value is
   accurate for the loop's actual timing, don't hardcode a guessed constant if the loop
   already tracks real elapsed time via a `Timer`).
4. On `Err(BambuError::Timeout)` from `tick_zombie_check`, decide the right CLI behavior —
   likely: log a warning (`log::warn!`) and either force a reconnect (if the monitor loop
   has reconnect logic already — check for one) or exit the loop with a clear message.
   Match whatever the monitor loop's existing error-handling convention is for other fatal
   MQTT errors in the same loop (check how it currently handles `send_ping()`'s own
   `Err` case at line 117 for the precedent to follow).

**Acceptance:** `cargo build --bin bambino-cli --features cli` passes. Manually trace
through the logic to confirm `tick_zombie_check` is now reachable on every loop
iteration — a live test (stalling a real MQTT connection to a printer) is not required for
this fix since it's pure control-flow wiring, not a wire-format change, but mention in
your summary that end-to-end zombie-detection behavior is easiest to confirm by briefly
unplugging a printer mid-session.

### 4.6 `validate_ftp_path` doesn't reject path traversal (`src/ftps/protocol.rs`)

**File:** `src/ftps/protocol.rs`, `validate_ftp_path` (currently ~line 249).

**Bug:** Only rejects `\r`/`\n`/`\0`. Nothing prevents a caller-supplied path containing
`..` from escaping the intended directory root via `delete_file`/`rename_file`/
`remove_directory`/`upload_file` (overwrite). Impact is bounded by the printer's own
vsFTPd sandboxing (unknown/unverified from this client's side), but defense in depth costs
little here.

**Fix:** Add a `..` path-segment check to `validate_ftp_path` — reject if any
`/`-or-`\`-delimited segment of `path` is exactly `..` (not merely "contains the substring
`..`", which would also reject legitimate filenames like `my..file.gcode` if such a thing
is realistic; check filesystem convention here — vsFTPd runs on a POSIX-like path space,
so segment-wise `..` matching is the correct semantics, same as any other traversal
guard). Implementation sketch:
```rust
if path.split(['/', '\\']).any(|segment| segment == "..") {
    return Err(BambuError::ProtocolViolation(
        "FTP path contains a '..' path traversal segment".into(),
    ));
}
```
Add this check alongside the existing CR/LF/NUL check in the same function (single early
return point, or two separate checks — either is fine). Add unit tests:
`test_validate_ftp_path_rejects_traversal_segment` (e.g. `"../../etc/passwd"`,
`"foo/../bar"`) and confirm `test_valid_pasv_response`-style legitimate paths (e.g.
`"/cache/model..with..dots.3mf"` if you want to confirm the substring-vs-segment
distinction) still pass. Also re-check `parse_unix_listing` (`src/ftps/parser.rs`), which
already calls `validate_ftp_path` on parsed filenames (line ~127) — confirm your stricter
check doesn't now spuriously reject a real (if unusual) filename containing literal `..`
as a substring but not as a whole path segment (e.g. `"my..cool..file.gcode"` — this must
still pass since no segment there is exactly `".."`).

### 4.7 Duplicated secure/plaintext data-channel branch across `list_directory`/`upload_file`/`download_file` (`src/ftps/client.rs`)

**Bug:** Each of the three methods repeats an almost-identical ~40-line "connect data
socket → re-check TLS 1.2 if enforced → poison-on-error → transfer" branch, and
`upload_file`'s chunked-write loop is duplicated verbatim between its secure and
plaintext branches. `CLAUDE.md` explicitly calls out this exact shape of duplication as
the root cause of the `write_command` regression (commit `6385019`) — a future fix
applied to one branch and missed in its sibling would silently reintroduce that failure
class, and mocks would still pass since they can't distinguish branch-level duplication
bugs from correct code.

**Fix:** Extract a shared helper that wraps the raw data socket in TLS (or not) and
re-checks the TLS-1.2 enforcement, returning a boxed/enum-wrapped stream the three callers
can treat uniformly. Since `Tls::Stream` and the raw `RawIO` are different concrete types
(one wrapped in TLS, one not), you cannot return "either" from a function without an enum
wrapper or dynamic dispatch. Recommended shape:
```rust
enum DataChannel<RawIO, TlsStream> {
    Plain(RawIO),
    Secure(TlsStream),
}

impl<RawIO: AsyncIo, TlsStream: AsyncIo> embedded_io_async::ErrorType for DataChannel<RawIO, TlsStream> { /* ... */ }
impl<RawIO: AsyncIo, TlsStream: AsyncIo> embedded_io_async::Read for DataChannel<RawIO, TlsStream> { /* delegate to whichever variant */ }
impl<RawIO: AsyncIo, TlsStream: AsyncIo> embedded_io_async::Write for DataChannel<RawIO, TlsStream> { /* delegate */ }
```
Then a single async method on `BambuFtpsClient`:
```rust
async fn open_data_channel(
    &mut self,
    raw_data_socket: RawIO,
) -> Result<DataChannel<RawIO, Tls::Stream>, BambuError> {
    if self.model.quirks().uses_plaintext_ftps_data_channel() {
        return Ok(DataChannel::Plain(raw_data_socket));
    }
    let secure = match self.tls_connector.connect(&self.ip, raw_data_socket).await {
        Ok(s) => s,
        Err(e) => { self.poisoned = true; return Err(e.into()); }
    };
    if let Err(e) = Self::require_tls_1_2_if_enforced(&self.tls_connector, &secure, self.model) {
        self.poisoned = true;
        return Err(e);
    }
    Ok(DataChannel::Secure(secure))
}
```
Replace the duplicated branches in `list_directory`/`download_file` (which both just
`read_to_eof` from the resulting channel) and `upload_file` (which writes+flushes to it —
its chunked-write loop then becomes a single loop over the `DataChannel` enum, not two
copies) with a call to `open_data_channel` followed by one shared transfer call. This is a
meaningful refactor — go carefully, and make sure `tests/ftps_test.rs`'s existing
plaintext-vs-secure test coverage (check for `FailingDataTlsConnector` or similar mocks
mentioned in `CLAUDE.md`) still exercises both branches through the new unified path, not
just one.

**Acceptance:** All three methods route through the shared helper. Existing
`tests/ftps_test.rs` tests pass unmodified (behavior must be identical, this is a pure
refactor). `cargo clippy` clean (watch for the enum needing `#[allow(dead_code)]` on an
unused variant only if one platform truly never constructs it — shouldn't happen here
since both variants are always reachable based on `model.quirks()`).

### 4.8 `rewrite_rtsp_request_uri`'s `printer_ip` is unvalidated (`src/camera/rtsps.rs`)

Lower-severity twin of Phase 1.1 — no credential is involved here (this function never
sees the access code), but a `printer_ip` containing `@` or `/` can still redirect the
proxy's outbound connection or produce a malformed URI. Before changing anything, check
this function's call sites (grep for `rewrite_rtsp_request_uri(`) to confirm where
`printer_ip` originates in practice — if it's always the same trusted config value the
caller already validated elsewhere (e.g. already passed through `build_rtsps_url`'s new
`IpAddr` check from Phase 1.1), the fix here may just be a doc-comment note rather than a
runtime check, to avoid revalidating the same value twice per request in a hot path
(RTSP proxies rewrite URIs per-request). If call sites can't guarantee that, apply the
same `IpAddr::parse` (or structural-character-rejection) validation used in Phase 1.1,
returning... note this function currently returns `String`, not `Result` — changing its
signature to `Result<String, BambuError>` is a breaking API change; weigh whether that's
warranted given the lower severity, or whether documenting the precondition ("caller must
validate printer_ip before calling — see build_rtsps_url") is the more proportionate fix.
State your choice and reasoning in your summary either way.

### 4.9 Camera `authenticate()` write has no deadline (`src/camera/binary.rs`)

**File:** `src/camera/binary.rs`, `BambuBinaryCameraStream::authenticate` (currently
~line 153).

**Bug:** `write_all`/`flush` in `authenticate()` have no deadline, unlike the read side
(`read_next_frame_with_timer`). If the printer never drains its TCP receive buffer during
handshake, `authenticate()` can hang forever.

**Fix:** This crate has no existing "bounded write" helper (`read_chunk` is read-only).
Two options:
- (a) Race the whole `write_all`+`flush` sequence against `timer.sleep(...)` using the
  existing `race()` combinator directly (no new helper needed, since a write, unlike a
  read, has no "resume mid-write" requirement here — an 80-byte handshake packet is small
  enough that losing/retrying the whole write on timeout is an acceptable simplification,
  unlike MQTT/camera reads which must preserve partial progress). Add a
  `authenticate_with_timer<T: TimerProvider>(&mut self, access_code: &str, timer: &T,
  budget_ms: u64)` following the exact naming/delegation convention already established by
  `read_next_frame`/`read_next_frame_with_timer` in this same file, with the public
  `authenticate()` delegating to it under `DummyTimer` (zero behavior change for existing
  callers).
- (b) Skip this for now if `write_all`'s default `embedded_io_async::Write::write_all`
  implementation already loops calling the underlying `write()`, similar to how
  `read_exact`'s default impl does for reads — if so, consider whether a generic
  `write_chunk` helper (a write-side sibling of `read_chunk`) belongs in `src/io/mod.rs`
  instead, for reuse by both this fix and a future FTPS write-deadline fix (`write_command`
  in `src/ftps/protocol.rs` has the same gap, noted but out of scope in Phase 3.2's step 4).
  If you go this route, build the shared `write_chunk` first, since it's the more durable
  fix and directly reusable.

Either way: `PrinterClient::ensure_camera()` (`src/client/connect.rs`) already calls
`cam.authenticate(access_code).await?` inside a `race_against_connect_timeout(...)` block —
confirm whether that outer race already provides adequate protection today (it does, for
callers going through `PrinterClient`) before treating this as urgent; the gap is
specifically for direct (non-`PrinterClient`) users of `BambuBinaryCameraStream`. If the
`PrinterClient` path is already covered, scope this fix to just adding the
`_with_timer` variant for direct-consumer parity, matching the read-side precedent, rather
than treating it as blocking.

### 4.10 No bounds validation on AMS addressing parameters (`src/client/ams.rs`)

**File:** `src/client/ams.rs` — `change_filament` (~line 53), `scan_rfid` (~line 108),
`select_k_profile` (~line 124).

**Bug:** None of `change_filament`'s `ams_id`/`slot_id`/`target` (documented valid values
`{0..3, 255}` / `{0..3, 254}` / `{1, 255}` per this method's own doc comment),
`scan_rfid`'s `ams_id`/`slot_id`, or `select_k_profile`'s `ams_id`/`tray_id` (documented
valid combos exactly `{254,254}` or `{255,255}` per the IDEX cheat-sheet already written
in `select_k_profile`'s doc comment) are validated before serialization. Every other
hazardous parameter elsewhere in the client goes through a quirks-based guard (fan
targets in `hardware.rs`, chamber heater in `thermal.rs`, homing in `motion.rs`) — these
three methods don't, despite `select_k_profile`'s own doc comment explicitly calling out
the IDEX Ext-R mis-routing hazard ("targeting the wrong address for Ext-R on IDEX machines
mis-routes the pressure advance profile to the left carriage").

**Fix:**
1. For `change_filament`, validate before building the request:
   ```rust
   let ams_valid = (0..=3).contains(&ams_id) || ams_id == 255;
   let slot_valid = (0..=3).contains(&slot_id) || slot_id == 254;
   let target_valid = target == 1 || target == 255;
   if !ams_valid || !slot_valid || !target_valid {
       return Err(BambuError::ProtocolViolation(
           "invalid AMS addressing parameters for change_filament".into(),
       ));
   }
   ```
   Adjust the exact ranges to match the doc comment precisely (re-read it before coding —
   don't paraphrase from this plan; the doc comment is the source of truth and may have
   nuances like AMS-HT's `128..=135` range documented elsewhere in `src/ams/parser.rs`'s
   `AMS_HT_ID_MIN`/`AMS_HT_ID_MAX` constants — check whether `change_filament`'s valid
   `ams_id` range should also include AMS-HT IDs, since `ams/parser.rs` clearly treats
   128-135 as valid elsewhere in this codebase. If AMS-HT units are addressable via this
   command too, the validation must include that range or it will incorrectly reject valid
   calls).
2. For `scan_rfid`, apply the same range check for `ams_id`/`slot_id` (check the reference
   doc `reference/05_materials_ams.md` or wherever `[REF-AMS-MAP]` points for the
   documented valid ranges for this specific command — don't assume they're identical to
   `change_filament`'s).
3. For `select_k_profile`, validate against the exact documented combos already spelled
   out in its own doc comment: single-nozzle `(254, 254)`; IDEX Ext-L `(254, 254)`; IDEX
   Ext-R `(255, 255)`. Since the valid set is a small fixed list of pairs (not a
   range), validate as:
   ```rust
   let valid_combo = matches!((ams_id, tray_id), (254, 254) | (255, 255));
   if !valid_combo {
       return Err(BambuError::ProtocolViolation(
           "invalid ams_id/tray_id combination for select_k_profile — must be (254,254) or (255,255) per the IDEX addressing cheat-sheet".into(),
       ));
   }
   ```
   Double-check this doesn't reject a legitimate non-external-spool K-profile selection —
   re-read `select_k_profile`'s doc comment fully; if it only documents the *external
   spool* addressing cheat-sheet and there's a separate valid range for genuine AMS-tray
   K-profile selection (seems likely, since K-profiles apply to AMS trays too, not just
   external spools), the validation needs to cover that broader case, not just the two
   external-spool pairs called out in the excerpt this plan saw. Investigate
   `reference/`'s `[REF-AMS-MAP]` doc before finalizing this specific check — this is the
   one sub-item in this phase most likely to need broader validation than what's shown
   above.
4. Add unit/integration tests for each method covering one valid and one invalid input
   (e.g. `test_change_filament_rejects_invalid_ams_id`, `test_select_k_profile_rejects_invalid_combo`).

**Acceptance:** Each of the three methods returns `BambuError::ProtocolViolation` for
documented-invalid inputs and still succeeds (dispatches normally) for every documented-valid
combination — verify against existing tests in `tests/` that already exercise valid calls
to these methods, to make sure the new validation doesn't reject something previously
accepted.

### 4.11 `start_drying` has no temperature/time ceiling (`src/client/ams.rs`, `src/quirks/mod.rs`)

**Files:** `src/client/ams.rs` (`start_drying`, ~line 75), `src/quirks/mod.rs`
(`ModelQuirks` trait), and every file under `src/quirks/models/`.

**Bug:** `dry_temp`/`dry_time` have zero ceiling enforcement — no
`ams_dry_temp_max`-equivalent quirks method exists (confirmed: no hits for
`dry_temp_max`/`ams_dry_temp_max` anywhere under `src/quirks/`), unlike every other
heater-setting method in `thermal.rs` (`set_bed_temperature`/`set_nozzle_temperature`/
`set_chamber_temperature`), which all clamp via `model.quirks()`.

**Fix:**
1. Add a new method to the `ModelQuirks` trait in `src/quirks/mod.rs`, following the exact
   shape of `chamber_temp_max` (a `u16`-returning method with a default of `0` meaning
   "not supported/no drying capability", since not every model has an AMS-HT/AMS 2 Pro
   drying unit attached — though note this is really a property of the *attached AMS
   unit*, not the *printer model* — see the design note below before assuming a flat
   per-model constant is even the right shape):
   ```rust
   /// Returns the maximum safe AMS drying-chamber temperature in °C, or `0` if this
   /// model has no documented AMS drying ceiling to enforce. Unlike bed/nozzle/chamber
   /// heater ceilings, this is a property of the *attached AMS unit* (AMS-HT, AMS 2 Pro),
   /// not the printer itself — the default `0` reflects "no known ceiling for this
   /// printer+AMS combination", not "drying is unsupported".
   fn ams_dry_temp_max(&self) -> u16 {
       0
   }
   ```
   **Design note — investigate before committing to this shape:** check
   `reference/05_materials_ams.md` (or wherever `[REF-AMS-DRYER]` points) for whether the
   drying temperature ceiling is genuinely a fixed hardware constant (e.g. "65°C max on
   all AMS-HT units regardless of host printer") or varies by *host printer model* the way
   bed temp does. If it's a property of the AMS unit hardware and not the host printer,
   the more correct fix is a flat, non-quirks-dispatched constant (e.g.
   `pub const AMS_HT_DRY_TEMP_MAX: u16 = 65;` near `start_drying`) rather than a new trait
   method every `ModelQuirks` impl must implement/override. Only add the trait method if
   you find real evidence different printer models pair with AMS units carrying genuinely
   different documented ceilings. Default to the simpler flat-constant fix unless the
   reference docs say otherwise — this avoids a many-file edit (every `models/*.rs` file)
   for a ceiling that likely doesn't vary by printer model at all.
2. In `start_drying` (`src/client/ams.rs`), clamp `dry_temp` (and validate `dry_time`
   against a sane maximum — check the reference doc for a documented maximum drying
   duration too, e.g. the existing doc comment's own example of "480 for an 8-hour cycle"
   suggests there may be a real firmware-enforced ceiling worth matching) using
   whichever mechanism you chose in step 1, following the exact
   clamp-and-`log::warn!` pattern already used in `thermal.rs`:
   ```rust
   let max_temp = /* quirks call or flat constant */;
   let dry_temp = if dry_temp > max_temp {
       log::warn!("AMS dry temperature {}°C exceeds maximum {}°C, clamping", dry_temp, max_temp);
       max_temp
   } else {
       dry_temp
   };
   ```
3. Add a unit/integration test confirming an out-of-range `dry_temp` gets clamped in the
   dispatched payload (check the JSON via a mock, mirroring existing AMS command tests in
   `src/mqtt/commands/mod.rs`'s test module).

### 4.12 `clean_stale_tray_data` doesn't clear `tray_temp`/`tray_time`/`drying_temp`/`drying_time` (`src/ams/parser.rs`)

**File:** `src/ams/parser.rs`, `clean_stale_tray_data` (currently ~line 80).

**Bug:** Clears `tray_type`/`tray_color`/`tray_info_idx`/`tag_uid`/`tray_uuid`/`remain`/
`tray_sub_brands`/`nozzle_temp_max`/`nozzle_temp_min`/`tray_diameter`/`tray_weight`/
`tray_id_name`/`xcam_info`/`k`/`n`/`cali_idx`/`cols`/`ctype`/`total_len`/`bed_temp`/
`bed_temp_type` on an empty/absent tray, but never clears `tray_temp`, `tray_time`,
`drying_temp`, `drying_time` (all present as `Option<String>` fields on `AmsTray`,
`src/types/telemetry/ams.rs` — confirmed by direct read, currently defined right after
`bed_temp_type`). A spool with a configured drying profile that's removed and replaced
with a spool lacking drying config leaves the *previous* spool's stale drying
temp/time cached client-side (the incremental telemetry update omits drying keys for the
new tray), which can show a phantom drying countdown in a UI.

**Fix:** Add the four missing field resets to the same `if is_absent_state ||
is_type_cleared { ... }` block in `clean_stale_tray_data`:
```rust
tray.tray_temp = None;
tray.tray_time = None;
tray.drying_temp = None;
tray.drying_time = None;
```
Insert them anywhere in the existing reset list (grouping near `bed_temp`/`bed_temp_type`
makes sense since they're adjacent on the struct, but ordering has no functional effect).

**Test:** Add `test_clean_stale_tray_data_clears_drying_fields` — construct an `AmsTray`
with `state: Some(9)` (or any absent-triggering state) and all four fields populated
(`tray_temp: Some("50".into())`, `tray_time: Some("240".into())`,
`drying_temp: Some("55".into())`, `drying_time: Some("480".into())`), call
`clean_stale_tray_data`, assert all four are `None` afterward. Also extend one of the
existing `test_clean_stale_tray_data_*` tests (e.g. `test_clean_stale_tray_data_state_9`)
to populate and assert these fields too, so future regressions on this exact bug are
caught even if someone only runs the "main" test.

---

## Phase 5 — Low severity cleanup (all independent, safe to batch into one or more PRs)

Each of these is a small, self-contained, low-risk change. Group them however is
convenient — by file, by theme, or all in one pass — since none has cross-dependencies on
another Phase 5 item or on Phases 1-4.

### 5.1 `src/io/esp_idf.rs:73,87,102` — redundant closures

`.map_err(|e| to_esp_socket_error(e))` → `.map_err(to_esp_socket_error)` at all three call
sites (inside `EspIdfUdpSocket::bind`). `cargo clippy` (part of the project's own gate)
flags `clippy::redundant_closure` here — this may already surface as a clippy failure once
Phase 2.3's CLI clippy gate is added; fix regardless. Verify with `scripts/check-esp-idf.sh`
since this file only compiles under the `esp-idf` feature.

### 5.2 `src/io/esp_idf.rs` — `EspTlsStream::flush` no-op has no explaining comment

Add a doc comment above the `async fn flush` no-op body explaining *why* it's a no-op
(the crate's own architecture doc for this file suggests mbedTLS/`esp_tls` writes are
unbuffered past the socket write — confirm this is actually true by checking whether
`EspTls::write` calls straight through to the socket with no internal buffering, per the
vendored `esp-idf-svc` source, before writing the comment as fact rather than assumption).
One line is enough: `// esp_tls writes go straight to the socket with no internal
buffering (confirmed via esp-idf-svc source) — nothing to flush.` Do not guess; verify
first, since an incorrect "verified" comment is worse than an honest "unverified, assumed"
one.

### 5.3 `LedCtrlRequest::new` only exposes on/off, not "flashing" mode (`src/mqtt/commands/hardware.rs`)

The wire protocol supports a flashing `led_mode` with nonzero on/off/loop/interval timing
(per the reference doc `[REF-MQTT-LIFECYCLE]` this file already cites), but
`LedCtrlPayload` already has all four fields (`led_on_time`/`led_off_time`/`loop_times`/
`interval_time`) — `new()` just always zeroes them. Add a second constructor,
e.g. `LedCtrlRequest::new_flashing(led_node: &str, on_time: u32, off_time: u32, loop_times:
u32, interval_time: u32, sequence_id: u64) -> Self`, setting `led_mode:
String::from("flashing")` and the four timing fields from parameters. Keep `new()`
unchanged (still the simple on/off constructor). Add a test mirroring
`test_led_ctrl_request_json`.

### 5.4 `AmsMappingTable` not re-exported from `src/mqtt/mod.rs`

`src/mqtt/mod.rs`'s `pub use commands::{...}` list omits `AmsMappingTable` even though
it's a field type on the already-re-exported `ProjectFilePayload::ams_mapping` (via
`print_job::AmsMappingTable`, already re-exported one level down in
`src/mqtt/commands/mod.rs`'s own `pub use print_job::{AmsMappingTable, ...}`). Add
`AmsMappingTable` to the `pub use commands::{...}` list in `src/mqtt/mod.rs` alongside its
sibling `PrintJobConfig`/`ProjectFileRequest`.

### 5.5 `write_frame` collapses every I/O error to `ConnectionAborted` (`src/mqtt/client/mod.rs`)

`write_frame` (~line 98) maps every `write_all`/`flush` failure to the same
`SocketError::ConnectionAborted`, discarding the real underlying error — inconsistent with
the precision this crate applies elsewhere (e.g. ESP-IDF's `map_esp_tls_connect_error`).
Since `write_frame` operates over the generic `AsyncIo`/`embedded_io_async::Write` trait
(not `std::io::Error` directly), there's no `map_std_io_error`-style helper to reuse
directly here — the underlying error type is whatever `IO::Error` is (an
`embedded_io_async::Error`). Consider whether `embedded_io_async::Error::kind()` (already
implemented by every platform's IO error wrapper) can be inspected here to produce a more
specific `SocketError` via a small local match, similar in spirit to
`map_std_io_error`/`map_io_error_kind` from Phase 4.2 but operating on
`embedded_io_async::ErrorKind` instead of `std::io::ErrorKind`. This is lower priority and
somewhat speculative — if the mapping can't meaningfully improve (e.g. if the generic
`AsyncIo` bound genuinely can't distinguish timeout from reset at this layer), it's
acceptable to leave as-is and just note that limitation in a doc comment instead of
forcing a fix.

### 5.6 `validate_ftp_path` doesn't reject leading `-` or other control chars (`src/ftps/protocol.rs`, `src/ftps/parser.rs`)

Beyond CR/LF/NUL (existing) and `..` traversal (Phase 4.6), some FTP daemons interpret a
leading-dash filename as a flag argument, and non-CR/LF C0/DEL control characters can
smuggle ANSI escapes into a filename a caller later prints/logs. Add to
`validate_ftp_path`: reject `path` (or, more precisely, the final path *segment* — a
leading-dash directory component earlier in the path is not the same hazard) starting with
`-`, and reject any byte `< 0x20` (beyond the already-checked `\r`/`\n`/`\0`) or `0x7F`
(DEL). Since `parse_unix_listing` (`src/ftps/parser.rs`) already calls this same function
on parsed names, this fix automatically covers both the "outgoing command" and "incoming
listing" defense-in-depth cases the review calls out — no separate change needed in
`parser.rs`. Add tests for both new rejection cases.

### 5.7 `build_handshake_packet` doesn't validate ASCII-alphanumeric like `build_rtsps_url` does (`src/camera/binary.rs`)

Currently only checks `access_code.len() <= 32`. Not independently exploitable (copied
into a fixed-width binary field, never interpolated into text), but inconsistent with
`rtsps.rs::build_rtsps_url`'s stricter check on the same conceptual credential. Add the
same `is_ascii_alphanumeric()` check for consistency:
```rust
if !access_code.chars().all(|c| c.is_ascii_alphanumeric()) {
    return Err(BambuError::ProtocolViolation(
        "access_code must be ASCII alphanumeric".into(),
    ));
}
```
Add before the existing length check (or after — order doesn't matter functionally).
Update/add a test mirroring `test_build_rtsps_url_rejects_non_alphanumeric_access_code`.
Check `tests/` for any existing camera integration test that passes a non-alphanumeric
access code expecting success — unlikely, but confirm nothing breaks.

### 5.8 Stale doc-comment references to `src/mqtt/client.rs` (`src/client/mod.rs:306`, `src/client/motion.rs`)

That file no longer exists — it was split into `src/mqtt/client/{mod,codec,frame,pending}.rs`
in commit `b133c9d`'s cleanup pass, which missed these two remaining references (one seen
directly in `motion.rs`'s `wait_for_homing` doc comment during this plan's research:
"the underlying `BambuMqttClient::poll_wire()` (`src/mqtt/client.rs`)"). Grep for
`mqtt/client.rs` (as a literal string, not `mqtt/client/`) across `src/` to find every
remaining stale reference — there may be more than the two the review names, since this
was a broad rename. Update each to the correct current path (`src/mqtt/client/mod.rs` if
referring to the struct/connect logic, or the specific submodule if referring to something
that moved further, e.g. `src/mqtt/client/frame.rs` for `poll_wire`'s per-read-deadline
mechanism specifically).

### 5.9 `set_command_timeout(0)` doc gap (`src/client/mod.rs`)

The doc comment on `set_command_timeout` doesn't state that `secs = 0` disables the
wall-clock timeout entirely in `poll_until` (falls back to the 200-message
`POLL_UNTIL_MAX_MESSAGES` cap with no time bound) — re-read `poll_until`'s own loop
(`if timeout_ms > 0 && elapsed >= timeout_ms` — confirms `0` is treated as "no time bound"
by design, not a bug) and add one sentence to `set_command_timeout`'s doc comment stating
this explicitly, e.g.: "Passing `0` disables the wall-clock timeout entirely — commands
then rely solely on the 200-message safety valve, not immediate timeout." This is a
documentation-only fix; no behavior change.

### 5.10 Repeated `next_sequence_id()` → build → `publish_request()` triplet (`src/client/{mod,ams,hardware,print}.rs`)

Roughly 15 call sites across these files repeat the same three-line shape. Consider adding
a small internal helper on `PrinterClient`, e.g.:
```rust
pub(crate) async fn dispatch<T: Serialize>(
    &mut self,
    build: impl FnOnce(u64) -> T,
) -> Result<u16, BambuError> {
    let seq = self.next_sequence_id();
    let req = build(seq);
    self.publish_request(&req).await
}
```
And convert call sites like:
```rust
let seq = self.next_sequence_id();
let req = crate::mqtt::AmsChangeFilamentRequest::new(ams_id, slot_id, target, curr_temp, tar_temp, seq);
self.publish_request(&req).await
```
into:
```rust
self.dispatch(|seq| crate::mqtt::AmsChangeFilamentRequest::new(ams_id, slot_id, target, curr_temp, tar_temp, seq)).await
```
This is a pure DRY cleanup — apply it consistently across every call site in `mod.rs`
(`request_pushall`), `ams.rs`, `hardware.rs`, `print.rs` in one pass so the codebase
doesn't end up with two competing conventions. Double-check `motion.rs`, `thermal.rs` too
(not named in the original finding, but check whether they have the same shape and would
benefit — consistency across the whole `client/` module is more valuable than narrowly
matching only the files the review named). Skip any call site whose shape genuinely
differs (e.g. `select_k_profile`'s builder needs more than just `seq` captured — the
closure approach handles that fine actually, since closures can capture arbitrary
surrounding locals, so this should apply almost everywhere).

### 5.11 Repeated clamp-and-warn block in `thermal.rs`

`set_bed_temperature`/`set_nozzle_temperature`/`set_chamber_temperature`
(`src/client/thermal.rs`) each repeat an identical clamp block differing only in the label
string. Extract:
```rust
fn clamp_temp(value: u16, max: u16, label: &str) -> u16 {
    if value > max {
        log::warn!("{} temperature {}°C exceeds model max {}°C, clamping", label, value, max);
        max
    } else {
        value
    }
}
```
(free function or a private method on `PrinterClient` — free function is simpler since it
needs no `self`). Replace all three call sites. Do this in the same pass as Phase 4.11's
`start_drying` clamp if convenient — that one is a very similar shape and could reuse this
same helper (call it from `client/ams.rs` too via a shared visible location, e.g.
`pub(crate)` in `client/mod.rs` or a small new `client/util.rs`, rather than duplicating
the helper itself).

### 5.12 Fan port IDs are unlinked magic numbers (`src/client/hardware.rs` write-side vs `src/client/telemetry.rs` read-side)

Write side (`hardware.rs::set_fan_speed`) uses inline `1`/`2`/`3`/`10` for
`FanTarget::{PartCooling,AuxiliaryLeft,ChamberExhaust,AuxiliaryRight}`; read side
(`telemetry.rs::auxiliary_right_fan_speed`) separately hardcodes `160` for the same
logical fan's telemetry-side port ID (different address space — write ports are M106 `P`
arguments, read ports are `device.airduct.parts[id]` — so these are *not* meant to be the
literal same number, but there's no compiler-enforced link between "this FanTarget variant"
and "these two numbers" today). Add named constants co-located with the `FanTarget` enum
definition (`src/client/types.rs` — verify exact location first), e.g.:
```rust
pub(crate) const FAN_WRITE_PORT_PART_COOLING: u16 = 1;
pub(crate) const FAN_WRITE_PORT_AUXILIARY_LEFT: u16 = 2;
pub(crate) const FAN_WRITE_PORT_CHAMBER_EXHAUST: u16 = 3;
pub(crate) const FAN_WRITE_PORT_AUXILIARY_RIGHT: u16 = 10;
pub(crate) const FAN_READ_PORT_AUXILIARY_RIGHT: u16 = 160;
```
and reference them from both `hardware.rs` and `telemetry.rs`, with a doc comment on
`FAN_READ_PORT_AUXILIARY_RIGHT` explicitly noting it is a *different address space* from
the write-side ports, not a typo. This doesn't create a compiler-enforced link (the review
correctly notes there isn't a real fix for that without a bigger structural change — a
lookup table keyed by `FanTarget` mapping to both a write port and read port would truly
close the gap, but is a bigger change than this Low-severity item warrants) — the constant
extraction at least makes both sides greppable/discoverable together and documents the
relationship, which is the proportionate fix here.

### 5.13 `set_buzzer_mode` takes a raw unvalidated `i32` (`src/client/hardware.rs`)

Every sibling setter in this file is typed (`FanTarget` enum, `AirductMode` enum) except
this one, which takes `mode_code: i32` with "0/1/2" documented only in a comment. Add a
proper enum:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuzzerMode {
    Silent = 0,
    Alarm = 1,
    Chirp = 2,
}
```
in `src/client/types.rs` (co-located with `FanTarget`/other client-facing enums — verify
exact location), re-export it the same way `FanTarget` is re-exported from `client/mod.rs`,
and change `set_buzzer_mode`'s signature to `pub async fn set_buzzer_mode(&mut self, mode:
BuzzerMode) -> Result<u16, BambuError>`, passing `mode as i32` into
`BuzzerRequest::new(...)`. This is a breaking signature change (pre-1.0, consistent with
other breaking changes already made in this crate per `CLAUDE.md`) — update the one CLI
call site in `src/bin/bambino-cli/control.rs` (currently there's no `Buzzer` variant in
`ControlAction` visible in the file this plan read — confirm whether buzzer control is
exposed via the CLI at all; if not, this is a library-only change with no CLI call site to
update).

### 5.14 `set_nozzle_temperature`'s `nozzle_id` isn't validated against `physical_nozzle_count()` (`src/client/thermal.rs`)

`nozzle_id: u8` is never checked against `ModelQuirks::physical_nozzle_count()` (confirmed
unreferenced under `src/client/` via search). Add a guard at the top of
`set_nozzle_temperature`:
```rust
if nozzle_id >= self.model.quirks().physical_nozzle_count() {
    return Err(BambuError::ModelMismatch(
        "nozzle_id exceeds this model's physical nozzle count".into(),
    ));
}
```
Add a test confirming e.g. `nozzle_id = 1` is rejected on a single-nozzle model
(`BambuModel::P1S` or similar) and accepted on an IDEX model (`BambuModel::H2D`/`X2D`).
Watch for tool-changer models (H2C, `physical_nozzle_count() == 7`) — confirm the guard's
semantics (`nozzle_id` as a 0-indexed carriage/tool index up to `count - 1`) actually match
how H2C addresses its 7 nozzles in practice before assuming a flat `0..count` range is
correct — check `reference/`'s G-code doc (`[REF-MOTO-GCODE]`) for H2C's actual addressing
scheme if one exists, since a 6-tool-changer-plus-1-fixed setup might not use a simple
linear index.

### 5.15 `X1CQuirks`/`X1EQuirks` hand-duplicated instead of macro-shared (`src/quirks/models/x1.rs`)

Unlike `a1.rs`/`h2.rs`, which share near-identical variants via `macro_rules!`
(`impl_a1_shared!`/`impl_h2_shared!`), `x1.rs` duplicates the full `impl ModelQuirks`
block for both structs, differing in only 4 of 13 methods (`has_active_chamber_heater`,
`nozzle_temp_max`, `bed_temp_max`, `chamber_temp_max` — verified by direct comparison
during this plan's research). Values are correct — this is pure DRY/consistency. Convert
to the same macro pattern as `a1.rs`:
```rust
macro_rules! impl_x1_shared {
    ($quirks_type:ty, $has_chamber_heater:expr, $nozzle_max:expr, $bed_max_fn:expr, $chamber_max:expr) => {
        impl ModelQuirks for $quirks_type {
            fn uses_plaintext_ftps_data_channel(&self) -> bool { false }
            fn enforce_ftps_tls_1_2(&self) -> bool { false }
            fn is_door_open(&self, telemetry: &PrinterTelemetry) -> bool { x1_is_door_open(telemetry) }
            fn has_door_sensor(&self) -> bool { true }
            fn camera_protocol(&self) -> CameraProtocol { CameraProtocol::Rtsps }
            fn ignores_chamber_temperature(&self) -> bool { false }
            fn has_stg_cur_idle_bug(&self) -> bool { false }
            fn has_active_chamber_heater(&self) -> bool { $has_chamber_heater }
            fn physical_nozzle_count(&self) -> u8 { 1 }
            fn supports_nozzle_offset_calibration(&self) -> bool { false }
            fn is_bed_on_z(&self) -> bool { true }
            fn z_max(&self) -> f32 { X1_Z_MAX }
            fn nozzle_temp_max(&self) -> u16 { $nozzle_max }
            fn bed_temp_max(&self, mains_220v: Option<bool>) -> u16 { $bed_max_fn(mains_220v) }
            fn chamber_temp_max(&self) -> u16 { $chamber_max }
        }
    };
}
```
`bed_temp_max` is awkward to macro-parameterize since X1C's body is a real 3-arm `match`
and X1E's is a flat constant ignoring the parameter — passing a closure-like
`$bed_max_fn` expression that gets called as `$bed_max_fn(mains_220v)` works if you define
two small free functions (`x1c_bed_temp_max(mains_220v: Option<bool>) -> u16` and
`x1e_bed_temp_max(_: Option<bool>) -> u16`) above the macro and pass their names as the
macro argument, rather than trying to inline the `match` into the macro invocation
directly (messier but avoids fighting macro hygiene over a multi-arm match literal as an
argument). Verify `cargo test` output for `test_x1c_quirks`/`test_x1e_quirks`
(`src/quirks/mod.rs`'s existing tests) is byte-identical before/after — this must be a
pure refactor with zero behavior change.

### 5.16 Reference doc omits A2L on port 6000 (`reference/01_network_discovery.md:122`)

Table row currently reads "A1, A1 Mini, P1P, P1S" for the binary-JPEG camera port, omitting
A2L — but `MODEL_MATRIX.csv` and `A2LQuirks::camera_protocol()`
(`src/quirks/models/a2.rs`) both agree A2L uses this same protocol. Code is correct; the
doc predates A2L. Update the table cell to "A1, A1 Mini, A2L, P1P, P1S" — per this
project's own stated convention ("When external sources... contradict a reference doc,
update the reference doc with the correction and note the verification source" —
`CLAUDE.md`), add a short inline note citing `src/quirks/models/a2.rs`'s
`camera_protocol()` as the verification source for the correction.

### 5.17 `Table::write_to`'s `separator_width` underflows on empty headers (`src/bin/bambino-cli/table.rs`)

`(col_count - 1) * 3` is a non-saturating `usize` subtraction — `Table::new(vec![])`
(empty header vector) underflows: panics in debug, produces a huge wraparound value fed
into a `format!` repeat-count in release. Currently unreachable (every call site in this
codebase passes non-empty headers), but `Table` is `pub` and this is a latent landmine for
a future caller. Fix with `saturating_sub`:
```rust
let separator_width: usize = widths.iter().sum::<usize>() + col_count.saturating_sub(1) * 3;
```
Add a regression test: `Table::new(vec![]).write_to(&mut some_buffer)` must not panic
(assert it produces *some* output without crashing — exact content doesn't matter much for
a genuinely-degenerate empty-header table, just that it doesn't panic/OOM).

### 5.18 1 GiB magic number duplicated three times (`src/bin/bambino-cli/storage.rs`)

`1_073_741_824` appears as a locally-scoped `const MAX_UPLOAD_BYTES` inside the `Upload`
match arm and again as two bare literals inside `format_size`. Extract one top-level
`const GIBIBYTE_BYTES: u64 = 1_073_741_824;` (or reuse/rename `MAX_UPLOAD_BYTES` to a
clearly-named `pub(crate) const BYTES_PER_GIB: u64 = 1_073_741_824;` at module scope) and
reference it from both `MAX_UPLOAD_BYTES`'s definition (`= BYTES_PER_GIB` if you keep both
names for semantic clarity — one for "the upload ceiling," one for "the unit conversion
factor," even though they're numerically identical today) and `format_size`'s two literal
sites.

### 5.19 Repeated "Dispatching.../call/...published successfully" triplet (`src/bin/bambino-cli/control.rs`)

16+ `ControlAction` match arms repeat the same three-line shape (`println!("Dispatching
...")`, the actual client call with `?`, `println!("... published successfully.")`).
Collapse into a small local helper, e.g.:
```rust
async fn dispatch<T>(
    before_msg: &str,
    after_msg: &str,
    fut: impl std::future::Future<Output = Result<T, BambuError>>,
) -> Result<T, BambuError> {
    println!("{before_msg}");
    let result = fut.await?;
    println!("{after_msg}");
    Ok(result)
}
```
and convert each arm, e.g.:
```rust
ControlAction::Home => {
    dispatch("Dispatching safe homing command macro...", "Homing command published successfully.", client.home_axes(false)).await?;
}
```
Apply consistently across every arm that fits this exact shape — skip `GcodeRaw` (has the
confirmation-prompt branch in between, doesn't fit the simple triplet) and any arm with
genuinely different pre/post logic (e.g. `Calibrate`'s options-building loop beforehand).
This is a pure readability cleanup with no behavior change — diff carefully to confirm the
printed messages are byte-identical to before.

### 5.20 README.md doesn't document `gcode-raw --unsafe` (README.md)

Confirmed via search: README.md's Usage block has no mention of `--unsafe`/
`bypass_safety`, unlike every other control action, which are documented. Add a line to
the `gcode-raw` usage example showing the `--unsafe` flag and what it does (skips the
interactive "type yes to confirm" safety prompt — see `control.rs`'s `GcodeRaw` match arm
for the exact behavior to describe accurately).

### 5.21 `move` action silently truncates multi-character axis input (`src/bin/bambino-cli/control.rs`)

`axis.chars().next()` takes only the first character of the `axis: String` clap argument —
`move xy 10` is silently treated as `move x 10` instead of erroring on the unexpected
extra character. Fix by validating the full string is exactly one character (and one of
`x`/`y`/`z`, case-insensitive) before extracting it:
```rust
if axis.len() != 1 {
    return Err(BambuError::ProtocolViolation(
        format!("Invalid axis: '{}' (expected a single character X, Y, or Z)", axis).into(),
    ));
}
let axis_char = axis.chars().next().unwrap(); // safe: len() == 1 checked above
```
(Keep the existing downstream validation, if any, that further restricts to X/Y/Z —
`move_relative` may already reject other letters; check before assuming this needs a
second validation layer.) Add a test/manual check: `move xy 10` must now return an error
instead of silently moving X.

---

## Summary checklist for whoever picks this up

- [ ] Phase 1.1 — RTSPS `ip` injection (critical, small)
- [ ] Phase 1.2 — ESP-IDF blocking dial (critical, large, needs `scripts/check-esp-idf.sh`
      + eventual real-hardware confirmation)
- [ ] Phase 2.1 — ESP-IDF force-TLS-1.2 (high, needs `scripts/check-esp-idf.sh`)
- [ ] Phase 2.2 — `ams_mapping2`/`use_ams` gating bug (high, small)
- [ ] Phase 2.3 — `make check-fast` CLI gap (high, trivial)
- [ ] Phase 3.1 — FTPS `read_to_eof` size cap (high, small, do before 3.2)
- [ ] Phase 3.2 — FTPS per-operation timeouts (high, large, "decide first" resolved to
      Option B above, needs eventual real-hardware confirmation)
- [ ] Phase 4.1-4.3 — io/ dedup (medium, mechanical)
- [ ] Phase 4.4 — MQTT constructor sequence_id clamping (medium, mechanical, many files)
- [ ] Phase 4.5 — `tick_zombie_check` CLI wiring (medium, small)
- [ ] Phase 4.6 — FTP path traversal validation (medium, small)
- [ ] Phase 4.7 — FTPS client branch dedup (medium, moderate refactor)
- [ ] Phase 4.8 — `rewrite_rtsp_request_uri` ip validation (medium, small, investigate
      call sites first)
- [ ] Phase 4.9 — camera `authenticate()` write deadline (medium, small-moderate)
- [ ] Phase 4.10 — AMS addressing bounds validation (medium, small, verify docs first)
- [ ] Phase 4.11 — `start_drying` ceiling (medium, small, investigate flat-const-vs-quirks
      shape first)
- [ ] Phase 4.12 — `clean_stale_tray_data` missing fields (medium, trivial)
- [ ] Phase 5.1-5.21 — low severity batch (all independent, safe to do in any order/grouping)

Every phase above states its own acceptance criteria — use those, plus `make check-fast`,
as the definition of done for each item.
