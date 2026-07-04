# bambino — `src/mqtt/client.rs` Refactor

**Important:** Before starting any phase, read this document in its entirety, then read `src/mqtt/client.rs` in its entirety. Read the relevant sections of `CLAUDE.md` (the `PrinterClient` refactor bullets under "Non-Obvious Type Decisions" describe the precedent this plan follows). Do not apply generic software engineering heuristics without grounding them in what this file actually does.

**Pre-release:** This library has not been released. All internal APIs are on the table — `pub(crate)` items can be freely renamed/moved/split. There are no external consumers of `crate::mqtt::client`'s internals to preserve compatibility for (confirmed by grep — see "Verified: no external breakage" below). The one true public surface, `crate::mqtt::{BambuMqttClient, MqttMessage}` re-exported from `src/mqtt/mod.rs`, must keep working identically; nothing in this plan changes it.

**Why this refactor:** `src/mqtt/client.rs` is 1284 lines, mixing four genuinely separate concerns in one file: wire-format encoding, resumable frame reading, session/connection lifecycle, and pending-message buffer management, plus ~440 lines of tests. This is the same shape of problem `src/client/mod.rs` had before it was split into `src/client/{ams,camera,connect,dummy,hardware,motion,print,storage,telemetry,thermal,types}.rs` (see recent commits `861ce00`, `fae1392`, `c62ef05`). This plan applies the same technique to `src/mqtt/client.rs`, plus fixes two DRY violations and one real test-quality bug found during review.

**When completing a phase:** Update this document marking the phase complete before starting the next one. If a phase reveals something the next phase's implementer needs to know that isn't derivable from reading the code, add a bullet under that phase's "Findings" heading. Do not remove completed phases from this document (unlike the old root `PLAN.md` convention) — this file is deleted entirely once all phases are done, not trimmed phase-by-phase.

**Verification gate for every phase:** `cargo build`, `cargo test`, `cargo build --no-default-features --features alloc --lib`, `cargo check --no-default-features --features embassy --lib`, `cargo clippy --all-targets -- -D warnings` (with and without `--features cli` where relevant — this refactor doesn't touch CLI code, so `--features cli` clippy is optional here but still run the default one). All must pass before moving to the next phase. Do not skip the `alloc`/`embassy` checks — this file has `#[cfg(not(feature = "std"))]` branches at the top that are easy to silently break by adding a new file that forgets them.

---

## Verified: no external breakage

Grepped `src`, `tests` for any reference to `mqtt::client::*` internals (packet consts, `encode_*`, `FrameReadState`, `read_exact_packet`) from outside `src/mqtt/client.rs` itself: **none exist**. `tests/common/mock_mqtt.rs` has its own independent copies of the packet-type/header constants and its own `encode_remaining_length` (it's a separate test-support crate target and cannot see `pub(crate)` items) — this is pre-existing duplication across the prod/test boundary, out of scope for this refactor, do not try to unify it. `src/camera/binary.rs` and `src/client/connect.rs` reference `read_exact_packet`/`FrameReadState`/`MQTT_READ_TIMEOUT_SECS` only in doc comments (prose, not code) — those comments should be updated if file paths change (see Phase 5) but nothing there depends on the internal module layout compiling a particular way.

This means every phase below is a pure internal reorganization of one file into a directory module. `src/mqtt/mod.rs`'s `pub mod client;` line and its `pub use client::{BambuMqttClient, MqttMessage};` re-export do not need to change — `src/mqtt/client.rs` → `src/mqtt/client/mod.rs` is a transparent rename as far as the module path `crate::mqtt::client` is concerned.

---

## Target end state

```
src/mqtt/client/
  mod.rs      — BambuMqttClient struct, MqttMessage, CONNECTION_COUNTER,
                connect(), publish_command(), poll_telemetry()/
                poll_telemetry_with_timer()/poll_wire(), send_ping(),
                tick_zombie_check(), get_in_flight_count().
                Owns the write_frame() helper (Phase 4) and re-exports
                whatever codec.rs/frame.rs items it needs internally.
  codec.rs    — Packet-type consts, header-byte consts, MQTT_KEEP_ALIVE_SECS,
                encode_remaining_length, encode_connect, encode_subscribe,
                encode_publish_qos1, encode_puback, encode_pingreq.
                Pure, stateless, no BambuMqttClient dependency.
  frame.rs    — FrameReadState, read_exact_packet, MQTT_READ_TIMEOUT_SECS,
                MQTT_MAX_PAYLOAD_BYTES. Its own #[cfg(test)] mod with the
                4 tests that exercise read_exact_packet directly.
  pending.rs  — A second impl<IO: AsyncIo> BambuMqttClient<IO> block holding
                message_size(), push_pending(), take_pending_matching(),
                MQTT_PENDING_BUFFER_MAX_BYTES. Its own #[cfg(test)] mod with
                the pending-buffer tests and the test_client() fixture they need.
```

Each file keeps the tests that exercise *its* private items — do not try to consolidate all tests into one place, and do not move any of these tests into `tests/mqtt_test.rs` (that file only exercises the public `connect()`/`poll_telemetry()` surface through a real mock broker; the tests being relocated here poke private fields/functions that an external test-crate target cannot see).

---

## Phase 1: Extract `codec.rs` (packet encoding)

**What moves:** `encode_remaining_length`, `encode_connect`, `encode_subscribe`, `encode_publish_qos1`, `encode_puback`, `encode_pingreq`, and these consts: `PACKET_TYPE_CONNACK`, `PACKET_TYPE_PUBLISH`, `PACKET_TYPE_PUBACK`, `PACKET_TYPE_SUBACK`, `PACKET_TYPE_PINGRESP`, `HEADER_CONNECT`, `HEADER_SUBSCRIBE`, `HEADER_PUBLISH_QOS1`, `HEADER_PUBACK`, `HEADER_PINGREQ`, `MQTT_KEEP_ALIVE_SECS`. Currently lines 40–183 of `src/mqtt/client.rs`.

**Why this boundary:** these functions take primitive args (`&str`, `u16`, `&[u8]`) and return `Vec<u8>` — zero coupling to `BambuMqttClient` or `AsyncIo`. Purest possible split, do this first to build confidence before touching anything stateful.

**Steps:**
1. Create `src/mqtt/client/` directory. Git-move `src/mqtt/client.rs` to `src/mqtt/client/mod.rs` (use `git mv` so history follows the file).
2. Create `src/mqtt/client/codec.rs`. Move the listed functions and consts into it. All items stay `pub(crate)` (functions currently have no visibility modifier — they're private `fn`; check whether anything in Phase 2–4's target files needs to call them, which it does — `mod.rs` calls `encode_connect`/`encode_subscribe` from `connect()`, `encode_publish_qos1` from `publish_command()`, `encode_puback` from `poll_wire()`, `encode_pingreq` from `send_ping()`. Make the encode functions `pub(super)` or `pub(crate)` — `pub(crate)` is simpler and matches the existing convention for the consts in this file).
3. In `src/mqtt/client/mod.rs`, add `mod codec;` and `use codec::{encode_connect, encode_subscribe, encode_publish_qos1, encode_puback, encode_pingreq, HEADER_CONNECT, ...};` (import exactly what's used — let the compiler tell you via unused-import warnings on `cargo build`).
4. `MQTT_MAX_PAYLOAD_BYTES`, `MQTT_IN_FLIGHT_LIMIT`, `MQTT_ZOMBIE_TIMEOUT_SECS`, `MQTT_STALE_CONNECTION_SECS`, `MQTT_PENDING_BUFFER_MAX_BYTES`, `MQTT_READ_TIMEOUT_SECS` stay in `mod.rs` for now — Phase 2 and Phase 3 will relocate the ones that belong with `frame.rs`/`pending.rs`. Don't move more than this phase's list in one step.
5. `cargo build` — fix any visibility/import errors. Run the full verification gate.

**Findings:** `mod.rs`'s test module has a nested `mod async_tests` that calls `encode_remaining_length` directly (unqualified, relying on the old flat namespace via `use super::super::*`). Since `codec` is a private `mod` in `mod.rs`, that glob import no longer brings the function into scope. Fixed by adding an explicit `use crate::mqtt::client::codec::encode_remaining_length;` inside `async_tests` — private items are visible to descendant modules, so this compiles. This same test (`test_read_exact_packet_oom_guard`) is scheduled to move to `frame.rs` in Phase 2, at which point this import moves with it (frame.rs will need its own `use super::codec::encode_remaining_length;` or similar).

**Status:** Complete

---

## Phase 2: Extract `frame.rs` (resumable frame reader)

**What moves:** `FrameReadState` (the enum), `read_exact_packet` (the function), `MQTT_READ_TIMEOUT_SECS`, `MQTT_MAX_PAYLOAD_BYTES`. Currently lines 54–63 (the two consts) and 185–320 of the original file.

**What stays in `mod.rs`:** `BambuMqttClient`'s `read_state: FrameReadState` field, and every *call site* of `read_exact_packet` (`connect()`'s CONNACK/SUBACK reads, `poll_wire()`'s main loop) — those are session-lifecycle logic, not frame-reading logic, and belong with the struct.

**Why this boundary:** `read_exact_packet` and `FrameReadState` together form one cohesive, independently-testable unit: "read one MQTT frame off an `AsyncIo` stream, resumable across timeouts." It has zero dependency on `BambuMqttClient`'s fields — it's a free function taking `&mut IO`, `&mut FrameReadState`, `&T: TimerProvider`, and a `budget_ms`. Read its existing doc comment (lines 211–233 of the original file) closely before moving it — the correctness invariant described there (never lose bytes already read across a timeout) must not be disturbed by the move, and the doc comment should move with the code verbatim.

**Tests to relocate:** these 4 tests currently under `#[cfg(feature = "tokio")] mod async_tests` (search for their names in the original file) call `read_exact_packet` directly and construct `FrameReadState::default()` directly — they belong in `frame.rs`, not `mod.rs`:
- `test_read_exact_packet_oom_guard`
- `test_read_exact_packet_malformed_remaining_length`
- `test_read_exact_packet_stalled_connection_times_out`
- `test_read_exact_packet_resumes_after_timeout_without_losing_bytes`

Also relocate the DRY fix from Phase 6 here if you're doing phases out of order — but do Phase 6 after this phase completes, in order, to keep each phase's diff reviewable.

**Steps:**
1. Create `src/mqtt/client/frame.rs`. Move `FrameReadState`, `read_exact_packet`, `MQTT_READ_TIMEOUT_SECS`, `MQTT_MAX_PAYLOAD_BYTES` into it, doc comments intact.
2. `read_exact_packet` needs `read_chunk` from `crate::io` — check the existing `use crate::io::{AsyncIo, SocketError, TimerProvider, read_chunk};` import at the top of the original file and bring exactly what `frame.rs` needs into its own `use` block. `mod.rs` will need a separate, likely smaller, `use crate::io::{...}` for what it still uses directly (`AsyncIo`, `SocketError` at minimum for its own `write_all`/`flush` error mapping).
3. Make `FrameReadState` and `read_exact_packet` `pub(crate)` (or `pub(super)` if you're confident nothing outside `mqtt::client` needs them — grep confirmed nothing outside this file uses them today, but `pub(crate)` is what the rest of this file's items already use, stay consistent).
4. In `mod.rs`, add `mod frame;` and `use frame::{FrameReadState, read_exact_packet, MQTT_READ_TIMEOUT_SECS};`.
5. Move the 4 tests listed above into a `#[cfg(test)] mod tests { ... }` at the bottom of `frame.rs`, under the same `#[cfg(feature = "tokio")] mod async_tests` nesting structure the original file uses (check how `mod tests` / `mod async_tests` are nested in the original — preserve that structure, just in the new file). These tests use `crate::io::TokioIo`, `crate::io::tokio::TokioTimer`, `tokio::io::{AsyncReadExt, AsyncWriteExt}` — bring those imports along.
6. Run the full verification gate.

**Findings:** After the move, `mod.rs` no longer uses the `alloc::vec` macro import directly (only `alloc::vec::Vec` the type) — the `#[cfg(not(feature = "std"))] use alloc::vec;` line became a dead import once `read_exact_packet`'s body (the only `vec![...]` call site in this file) moved to `frame.rs`, and only showed up as a warning under `--features alloc`/`embassy` builds (tests, which still use `vec!`, aren't compiled in those configs). Removed it. Also: `test_read_exact_packet_oom_guard`/`_malformed_remaining_length` no longer need `DummyTimer` imported via the old `super::super::*` glob — `frame.rs`'s own test module imports `crate::client::dummy::DummyTimer` directly since `frame.rs` doesn't otherwise use it outside tests.

**Status:** Complete

---

## Phase 3: Extract `pending.rs` (pending-message buffer)

**What moves:** `message_size`, `push_pending`, `take_pending_matching` (all currently methods on `impl<IO: AsyncIo> BambuMqttClient<IO>` in the original file), plus `MQTT_PENDING_BUFFER_MAX_BYTES`.

**What stays in `mod.rs`:** the `pending_messages: VecDeque<MqttMessage>` and `pending_bytes: usize` *fields* on the struct (struct definition doesn't move, only the methods operating on it), and the two call sites in `poll_telemetry_with_timer()` (drains `pending_messages.pop_front()`) — that's lifecycle logic, not buffer-management logic, leave it in `mod.rs`.

**Why this boundary, and why it's safe to split methods across files:** Rust allows multiple `impl` blocks for the same type across different files in the same crate — `src/client/mod.rs`'s `PrinterClient` already does exactly this (its methods are split across `ams.rs`, `motion.rs`, `storage.rs`, etc., each with its own `impl<...> PrinterClient<...> { }` block). `pending.rs` will have `impl<IO: AsyncIo> BambuMqttClient<IO> { fn message_size(...) {...} pub(crate) fn push_pending(...) {...} pub(crate) fn take_pending_matching(...) {...} }` — no different in kind from what this codebase already does for `PrinterClient`.

**Tests to relocate:**
- `test_push_pending_evicts_oldest_beyond_max_bytes`
- `test_take_pending_matching_removes_only_the_match`
- `test_take_pending_matching_returns_none_when_no_match`
- The `test_client()` fixture function these three tests depend on (constructs a `BambuMqttClient` by hand with all-default fields, bypassing `connect()`'s handshake — it must move with them since nothing else uses it after Phases 2–3 are done; double check that assumption by grepping for `test_client()` call sites before deleting it from `mod.rs`).

**Steps:**
1. Create `src/mqtt/client/pending.rs`. Add `use super::{BambuMqttClient, MqttMessage};` and `use crate::io::AsyncIo;` (adjust based on what the compiler says is actually needed).
2. Move `message_size`, `push_pending`, `take_pending_matching`, `MQTT_PENDING_BUFFER_MAX_BYTES` into a new `impl<IO: AsyncIo> BambuMqttClient<IO>` block in `pending.rs`. Doc comments move with them verbatim.
3. In `mod.rs`, add `mod pending;` — no re-export needed for the methods themselves (they're called as `self.push_pending(...)` etc. from `src/client/*.rs` and `tests/`, which works automatically once the `impl` block exists anywhere in the same crate reachable via the type). Do check whether `MQTT_PENDING_BUFFER_MAX_BYTES` is referenced from `mod.rs` or elsewhere and add `use pending::MQTT_PENDING_BUFFER_MAX_BYTES;` if so.
4. Move the 3 tests + `test_client()` fixture into `pending.rs`'s own `#[cfg(feature = "tokio")] mod async_tests` (or plain `#[cfg(test)] mod tests` if they don't actually need tokio — check: `push_pending`/`take_pending_matching` are synchronous methods, the existing tests are plain `#[test]`, not `#[tokio::test]` — but `test_client()` constructs `TokioIo<std::io::Cursor<Vec<u8>>>` which needs the `tokio` feature to compile, so keep the same `#[cfg(feature = "tokio")]` gating the original file used).
5. Run the full verification gate.

**Findings:** `message_size` had no visibility modifier in the original file (private, but callable from `poll_telemetry_with_timer` since both lived in the same `impl` block in the same file). Once moved to `pending.rs`, `mod.rs`'s `poll_telemetry_with_timer` (which calls `Self::message_size(&buffered)`) needed it visible from the parent module — Rust privacy makes private items visible to their defining module and descendants only, not ancestors. Made it `pub(crate)`, matching `push_pending`/`take_pending_matching`'s existing visibility for consistency. Confirmed via grep that `test_client()` had exactly the 3 call sites the plan expected (all in the tests being relocated) before deleting it from `mod.rs`.

**Status:** Complete

---

## Phase 4: DRY fix — extract `write_frame` helper

**Problem:** the pattern

```rust
self.stream.write_all(&packet).await.map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
self.stream.flush().await.map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
```

(or the equivalent on a local `stream` variable rather than `self.stream`, inside `connect()` before `Self` exists) appears 5 times in `mod.rs` after Phases 1–3: twice in `connect()` (CONNECT packet, SUBSCRIBE packet), once in `publish_command()`, once in `poll_wire()`'s QoS-1 PUBACK send, once in `send_ping()`.

**Fix:** add a free function (not a method, since `connect()` calls it before `Self` is constructed) in `mod.rs`:

```rust
async fn write_frame<IO: AsyncIo>(stream: &mut IO, packet: &[u8]) -> Result<(), BambuError> {
    stream
        .write_all(packet)
        .await
        .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
    stream
        .flush()
        .await
        .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))
}
```

Replace all 5 call sites. In `connect()`, call as `write_frame(&mut stream, &connect_pkt).await?;`. In the methods on `BambuMqttClient`, call as `write_frame(&mut self.stream, &packet).await?;`.

**Steps:**
1. Add `write_frame` near the top of `mod.rs` (right after the `impl` block's opening, or as a private free function above `impl<IO: AsyncIo> BambuMqttClient<IO>` — either is fine, pick whichever reads better once you see the file).
2. Replace all 5 call sites. Delete the now-dead duplicated `map_err` chains.
3. Run the full verification gate — this is a pure refactor, no behavior change, so `cargo test` output must be byte-identical in pass/fail terms to before this phase.

**Findings:** Straightforward, no surprises. `cargo test` pass/fail counts (275 lib, 54+14+1 integration, 8 ignored doctests) are identical to the Phase 3 gate, confirming no behavior change.

**Status:** Complete

---

## Phase 5: DRY fix — extract single-byte read loop in `frame.rs`

**Problem:** inside `read_exact_packet` (now in `frame.rs` after Phase 2), this exact loop shape appears twice — once reading the fixed-header byte, once inside the remaining-length varint loop:

```rust
let mut b = [0u8; 1];
let mut filled = 0;
while filled < b.len() {
    let n = read_chunk(stream, &mut b[filled..], timer, deadline_ms).await?;
    filled += n;
}
```

(the header-byte version uses a variable named `header` instead of `b`, otherwise identical).

**Fix:** extract a helper in `frame.rs`:

```rust
async fn read_one_byte<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    timer: &T,
    deadline_ms: Option<u64>,
) -> Result<u8, SocketError> {
    let mut b = [0u8; 1];
    let mut filled = 0;
    while filled < b.len() {
        let n = read_chunk(stream, &mut b[filled..], timer, deadline_ms).await?;
        filled += n;
    }
    Ok(b[0])
}
```

Replace both call sites in `read_exact_packet` with `let byte = read_one_byte(stream, timer, deadline_ms).await?;` and adjust the surrounding code to use `byte` instead of indexing into a local array.

**Important — this touches code with a documented correctness invariant.** Re-read `read_exact_packet`'s doc comment (moved in Phase 2) before making this change: the resumability guarantee (never lose bytes already read across a `SocketError::TimedOut`) must hold exactly as before. This particular change is safe because `read_one_byte` either fully succeeds (returns `Ok(byte)`, exactly one byte was consumed from the stream and is returned to the caller to store into `FrameReadState`) or fully fails before any byte is consumed (`read_chunk` erroring on the first partial-byte read) — there's no partial-byte state to lose, unlike the multi-byte payload read which deliberately stays a manual loop writing into `FrameReadState::ReadingPayload { buf, filled, .. }` so a timeout mid-payload preserves the bytes already landed. Do not attempt to extract a similar helper for the payload read loop — that one's manual structure is load-bearing, not incidental duplication.

**Steps:**
1. Add `read_one_byte` to `frame.rs`.
2. Replace both call sites inside `read_exact_packet`.
3. Update the doc comment's line references if it mentions specific line numbers (check — it may not).
4. Run the full verification gate, paying special attention to the two resumability regression tests (`test_read_exact_packet_stalled_connection_times_out`, `test_read_exact_packet_resumes_after_timeout_without_losing_bytes`) actually running and passing, not just compiling.

**Findings:** No line-number references in the doc comment needed updating. Both resumability regression tests (`test_read_exact_packet_stalled_connection_times_out`, `test_read_exact_packet_resumes_after_timeout_without_losing_bytes`) ran and passed, confirming the correctness invariant held. Test counts identical to Phase 4's gate.

**Status:** Complete

---

## Phase 6: Test-quality fix — `advance_packet_id` extraction

**Problem (real bug risk, not just style):** `publish_command()` in `mod.rs` contains:

```rust
let packet_id = self.next_packet_id;
self.next_packet_id = self.next_packet_id.wrapping_add(1);
if self.next_packet_id == 0 {
    self.next_packet_id = 1; // 0 is reserved in MQTT specifications
}
```

Three tests — `test_packet_id_skips_zero_on_wraparound`, `test_packet_id_normal_increment`, `test_packet_id_one_before_max` — verify this logic, but **each test re-implements the increment-and-skip-zero logic inline** rather than calling the production code path. Read them (they're plain `#[test]` functions near the top of the original `mod tests` block, not inside the `#[cfg(feature = "tokio")] mod async_tests` nested module). This means these tests currently prove nothing about `publish_command()` — they'd keep passing even if `publish_command()`'s logic were changed or broken, because they never call it.

**Fix:** extract a pure function and have both production code and tests call it:

```rust
/// Advances an MQTT packet identifier, skipping 0 (reserved) on wraparound.
fn advance_packet_id(current: u16) -> u16 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}
```

In `publish_command()`:
```rust
let packet_id = self.next_packet_id;
self.next_packet_id = advance_packet_id(self.next_packet_id);
```

Rewrite the three tests to call `advance_packet_id` directly instead of re-deriving its logic, e.g.:

```rust
#[test]
fn test_packet_id_skips_zero_on_wraparound() {
    assert_eq!(advance_packet_id(u16::MAX), 1, "Packet ID must skip 0 after wraparound");
}

#[test]
fn test_packet_id_normal_increment() {
    assert_eq!(advance_packet_id(100), 101);
}

#[test]
fn test_packet_id_one_before_max() {
    assert_eq!(advance_packet_id(u16::MAX - 1), u16::MAX, "ID before MAX should increment normally");
}
```

**Where this lives:** `advance_packet_id` belongs in `mod.rs` (it's `BambuMqttClient` session state logic, not codec/frame/pending concern) — place it near `publish_command()`. Its tests can be plain `#[test]` (no tokio needed) directly in `mod.rs`'s `#[cfg(test)] mod tests` block, not nested under `async_tests`.

**Steps:**
1. Add `advance_packet_id` to `mod.rs`.
2. Update `publish_command()` to call it.
3. Rewrite the 3 tests to call it instead of duplicating its logic.
4. Run the full verification gate.

**Findings:** These 3 tests now actually exercise `publish_command()`'s wraparound logic (via `advance_packet_id`) instead of a copy-pasted reimplementation — confirmed the fix is real by noting the old tests would have kept passing even if `publish_command()`'s inline logic had a bug, since they never called it. Test counts identical to Phase 5's gate.

**Status:** Complete

---

## Phase 7: Final pass — doc comments, module docs, CLAUDE.md

1. Re-read `src/mqtt/client/mod.rs` top-of-file module doc comment (originally lines 1–9 of `src/mqtt/client.rs`) and update it if it references things that moved (it currently describes the whole file's responsibilities in prose — fine to leave broad, but check it doesn't claim something now false, e.g. don't claim "handles ... QoS 1 publish queues" if that's misleading post-split — it isn't, this is a style check not a rewrite).
2. Grep the whole repo for `src/mqtt/client.rs` as a *path string* in comments (`CLAUDE.md`, `src/camera/binary.rs`, `src/client/connect.rs`, `src/io/mod.rs`, and this plan's own prior phases) and update any that now point to a stale path — decide case by case whether to say `src/mqtt/client/mod.rs`, `src/mqtt/client/frame.rs`, or just `src/mqtt/client/` depending on what the comment is actually pointing at.
3. Add a short bullet to `CLAUDE.md`'s "Non-Obvious Type Decisions" list (matching the style of the existing `PrinterClient` split bullets) recording: `src/mqtt/client.rs` was split into `src/mqtt/client/{mod,codec,frame,pending}.rs` for the same reason `client/mod.rs` was split (see the three refactor commits referenced at the top of this plan) — this is exactly the kind of fact CLAUDE.md's own header says must be recorded ("update this file" when "changing conventions"), and it's not derivable from reading the code alone (a fresh session seeing 4 small files wouldn't otherwise know *why* the boundaries are where they are, or that `pending.rs`'s split-`impl`-block pattern deliberately mirrors `PrinterClient`'s).
4. Run the full verification gate one final time.
5. Delete this file (`MQTT_CLIENT_REFACTOR.md`) once all phases are confirmed complete and the CLAUDE.md bullet is in place — per this repo's convention (see how the old root `PLAN.md` was retired in commit `e005926`), planning docs for completed work don't linger.

**Findings:** Step 1 required no change — `mod.rs`'s module doc comment still accurately describes the whole `mqtt::client` module's responsibilities post-split. Step 2: grepped for `src/mqtt/client.rs` as a path string and found 7 real code-comment references needing updates (beyond this plan's own self-references, which were left alone): `CLAUDE.md` (2 bullets), `src/camera/binary.rs` (5 doc-comment references), `src/client/connect.rs` (1), `src/client/dummy.rs` (1) — updated each to point at the specific file (`mod.rs` vs `frame.rs`) the comment was actually describing, per the plan's case-by-case guidance. Step 5 (delete this file) was overridden by explicit user instruction after Phase 6 — **this file is being kept**, not deleted, contrary to the original plan text below and the repo's `PLAN.md`-retirement convention referenced in the intro.

**Status:** Complete (deletion step skipped per user instruction — see above)

---

## Progress Tracker

| Phase | Description | Status |
|-------|-------------|--------|
| 1 | Extract `codec.rs` (packet encoding) | Complete |
| 2 | Extract `frame.rs` (resumable frame reader) | Complete |
| 3 | Extract `pending.rs` (pending-message buffer) | Complete |
| 4 | DRY fix: `write_frame` helper | Complete |
| 5 | DRY fix: `read_one_byte` helper in `frame.rs` | Complete |
| 6 | Test-quality fix: `advance_packet_id` extraction | Complete |
| 7 | Final pass: doc comments, `CLAUDE.md`, delete this file | Complete (file kept, not deleted — user override) |
