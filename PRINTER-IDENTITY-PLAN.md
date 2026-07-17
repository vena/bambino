# `PrinterIdentity`: fixing the ip/serial/access_code transposition risk

## Problem

`BUG-196` started narrow — `BambuFtpsClient::connect()`/`connect_control_stream()`/
`from_control_stream()` suppress `clippy::too_many_arguments` (8-9 positional args,
several adjacent same-typed `&str`) — but triage widened it: `PrinterClient::new()`
has the identical `ip: &str, serial: &str, access_code: &str` adjacency risk, just
under clippy's arg-count threshold, so it never got flagged. Three adjacent
same-typed string params with no compiler-level protection against transposition is
a real risk regardless of whether clippy's blunt heuristic happens to notice it.

Separately, `PrinterClient::from_mqtt(mqtt_client, serial, model)` takes a `serial`
that duplicates data already implicitly present in the `mqtt_client` it's handed
(baked into `MqttClient`'s internal `request_topic` field as
`format!("device/{}/request", serial)`, `src/mqtt/client/mod.rs:291`) — nothing
today verifies the two match, so a caller could pass the wrong printer's serial to
`from_mqtt()` and nothing would catch it.

This plan fixes both, and establishes one shared calling convention across every
"connect to protocol X" entry point in the crate — not just the ones where
transposition risk alone would force it — with exactly one structural exception
(rule 3).

## Design decisions (already made — do not re-litigate without new evidence)

1. **New type**: `PrinterIdentity { ip: String, serial: String, access_code: String }`
   (or `&str` fields if a borrowed-view variant proves cleaner during implementation —
   decide at implementation time, not a blocking question). No `Option` fields, ever
   — see rationale below.

2. **Module placement: top-level, not nested under `client/`.** `MqttClient`
   (`mqtt::client`) and `BambuFtpsClient` (`ftps::client`) are meant to be directly
   usable without going through `PrinterClient` (README's "Direct protocol access"
   section explicitly documents this for FTPS; `MqttClient::connect()` is `pub` and
   used standalone in `tests/telemetry_replay_test.rs` and README's own `from_mqtt`
   example, so it's de facto in the same category even though README doesn't name it
   explicitly). If `PrinterIdentity` lived under `client/`, a direct FTPS-only or
   MQTT-only consumer would have to import from the higher-level `client` module just
   to build one — a backwards dependency (the foundational protocol layer depending
   on the orchestrator built on top of it). Put it next to `PrinterModel`
   (`src/models.rs`) — same category of crate-wide, dependency-neutral type. A new
   `src/identity.rs`, re-exported from `lib.rs` the same way `PrinterModel` is.

3. **Scope: which constructors take `&PrinterIdentity`, and which don't.**

   | Constructor | Takes `PrinterIdentity`? | Why |
   |---|---|---|
   | `PrinterClient::new()` | Yes | Owns all three fields long-term for its own multi-protocol orchestration; always has all three by construction. |
   | `BambuFtpsClient::connect()` / `connect_control_stream()` / `from_control_stream()` | Yes | FTPS structurally needs all three (see `ip` for later passive-data-channel dials) — every realistic caller, direct or via `PrinterClient`, already has all three in hand regardless of whether they're bundled. Bundling costs nothing. |
   | `MqttClient::connect()` | **Yes** | Reads `.serial`/`.access_code`, ignores `.ip`. `connect()`'s own direct caller *just finished dialing the stream themselves* — they always know the address they used, in some form (even an embedded caller with a static IP config just needs a `.to_string()`). That's mild friction, not fabricating data from nothing, so the free-reuse argument applies here too. |
   | `BambuBinaryCameraStream::authenticate()` | **Yes** | Reads `.access_code` only. To have a stream to authenticate at all, the caller already dialed (`ip`) and TLS-wrapped with SNI (`serial`) — the full identity was always in their hands, just not threaded through as an argument today. No fabrication cost. |
   | `PrinterClient::from_mqtt()` | **No** — this is the one real exception | Its caller receives an **already-connected** `MqttClient`, possibly handed to them by something else entirely — no dial happened in their own code, so `ip`/`access_code` may never have existed for them in any form. This isn't "wrong representation of data they have," it's "the data doesn't exist for them." See rule 6. |

   **The test that actually decides this** (apply it to any future constructor, not
   just eyeballed arg count, and not just "does the callee use the field"): *does the
   direct caller of this specific constructor necessarily just performed the action
   that produces every field the struct would require — even if some of those
   fields aren't literally passed as arguments today?* `MqttClient::connect()` and
   `BambuBinaryCameraStream::authenticate()` both pass this test once you look past
   "the function doesn't use `ip`/`serial` today" to "the caller necessarily has it
   anyway, because they just dialed/handshook." `from_mqtt()` fails it — its caller
   didn't perform any of those actions; someone else did, elsewhere, and only handed
   over the finished result.

4. **Why not `ip: Option<String>` to let `from_mqtt()` take `PrinterIdentity` too?**
   Rejected. An `Option` field doesn't remove the risk it's meant to fix, it changes
   its shape: today `from_mqtt()` has no `ip`/`access_code` parameters at all, so
   supplying wrong ones is impossible. With `Option` fields on a shared struct, a
   caller *can* set `Some(fake_value)` and it compiles fine — nothing stops a
   fabricated placeholder from looking exactly as valid as real data. A
   rustc-caught omission traded for a silently-wrong value is a regression, not a
   fix.

5. **`MqttClient` gains a stored `serial` field + `pub fn serial(&self) -> &str`.**
   Currently `connect()` takes `serial: &str` only to build `client_id` and
   `request_topic`, then discards the borrow — the value survives *inside*
   `request_topic` (`format!("device/{}/request", serial)`) but isn't retrievable
   without parsing it back out. Store it directly instead: `serial: String` on the
   struct, set in `connect()` from `identity.serial` (rule 3's `PrinterIdentity`
   param) rather than a standalone `&str` param.

6. **`PrinterClient::from_mqtt()` signature shrinks to `from_mqtt(mqtt_client, model)`.**
   Reads `serial` via `mqtt_client.serial()` (compute `let serial =
   mqtt_client.serial().to_string();` before moving `mqtt_client` into
   `Self { mqtt: Some(mqtt_client), .. }` — no borrow-check conflict). This doesn't
   just deduplicate a redundant argument, it removes a mismatch class entirely: there
   is no longer any way to hand `from_mqtt()` a `serial` that disagrees with what the
   `MqttClient` actually connected as. There's no legitimate
   reason for `PrinterClient` to report a different serial than the one its own wire
   session used — if you need to control a second printer, you construct a second
   `PrinterClient`.

   **Out of scope, deliberately**: the existing `BUG-072` runtime-panic guard (two
   near-identical `assert!` blocks in `with_ftps()`/`with_camera()`,
   `src/client/connect.rs:256-264` and `:468-...` at time of writing) that catches
   `from_mqtt()`-constructed clients trying to use FTPS/camera with empty
   `ip`/`access_code`. This plan does not change that guard from a runtime panic to a
   compile-time error — doing so would mean giving `PrinterClient` two genuinely
   different shapes (one with real `ip`/`access_code`, one without) at the type
   level, which is a bigger redesign than "stop transposing adjacent strings." Note
   it here so it isn't rediscovered as a surprise; a future session can decide
   whether it's worth doing, starting from this note.

## Scope (exact call sites, grep-verified at time of writing)

`PrinterClient::new(...)`: 14 call sites — `tests/client_test.rs` (7),
`tests/camera_test.rs` (4), `README.md` (1 prose example), `src/lib.rs` (1 crate-level
doc example), `src/bin/bambino-cli/connection.rs::create_printer` (1). `docs/` hits
are generated, not edited directly — regenerate via `make docs` as the last step.

`BambuFtpsClient::connect(...)` direct calls: `tests/ftps_test.rs` (9).

`connect_control_stream()`/`from_control_stream()`: `src/client/connect.rs::ensure_ftps`
(1 call to each) + `src/ftps/client.rs::connect`'s own internal call to
`connect_control_stream` (this is `connect()`'s own implementation, not a separate
external caller — update together with the constructor signature itself in the same
edit).

`PrinterClient::from_mqtt(...)`: 3 call sites — `tests/common/client.rs::connect_test_client`
(covers the ~80 test-suite uses that go through this one helper, courtesy of the
`BUG-170` dedup work), `tests/telemetry_replay_test.rs` (1 direct), `README.md` (1
prose example).

`MqttClient::connect(...)`: re-count at implementation time — this plan's earlier
draft assumed the signature wouldn't change and didn't enumerate these; it now does
(rule 3), so every call site needs the same `PrinterIdentity`-construction update as
`PrinterClient::new(...)`'s. Expect this to mostly overlap with `PrinterClient::new`'s
call sites plus `PrinterClient::ensure_mqtt()`'s internal call
(`src/client/connect.rs`) plus any direct-use test/doc sites — grep
`MqttClient::connect(` fresh rather than trusting a stale count here.

`BambuBinaryCameraStream::authenticate(...)`: re-count at implementation time,
same reasoning — grep `\.authenticate(` scoped to camera call sites fresh.

## Phases

Each phase should compile and pass `make check-fast` on its own — don't batch
phases into one giant diff. Order matters where noted; phases not depending on each
other can be done in either order or split across sessions.

### Phase 1 — `MqttClient` stores `serial`; `from_mqtt()` drops its redundant param

No dependency on `PrinterIdentity` — safe to do first, standalone, even before
Phase 2 exists.

1. Add `serial: String` field to `MqttClient` (`src/mqtt/client/mod.rs`), set from
   the existing `serial: &str` parameter `connect()` already receives (signature
   unchanged in this phase — the `&PrinterIdentity` switch is Phase 4).
2. Add `pub fn serial(&self) -> &str`.
3. Change `PrinterClient::from_mqtt(mqtt_client: MqttClient<IO>, serial: &str, model: PrinterModel)`
   to `from_mqtt(mqtt_client: MqttClient<IO>, model: PrinterModel)` — capture
   `mqtt_client.serial().to_string()` before moving `mqtt_client` into the returned
   `Self`.
4. Update the 3 `PrinterClient::from_mqtt(...)` call sites (`tests/common/client.rs`,
   `tests/telemetry_replay_test.rs`, `README.md`).

### Phase 2 — Add `PrinterIdentity`, no consumers yet

Create `src/identity.rs` with the struct (rule 1), doc comments explaining the
no-`Option` decision inline (future readers shouldn't have to find this plan doc to
understand why `ip` isn't optional). Re-export from `lib.rs` next to `PrinterModel`.
No existing code changes yet — this phase is additive only, trivially safe to land
alone. Phases 3-6 depend on this one; Phase 1 does not.

### Phase 3 — `PrinterClient::new()` and internal storage switch to `PrinterIdentity`

Depends on Phase 2. Do before Phase 4's `ensure_mqtt()` step and Phase 5 if doing
all in one session, to avoid the throwaway-construction workaround noted below.

1. Replace `PrinterClient`'s three loose `ip: String, serial: String, access_code: String`
   fields with one `identity: PrinterIdentity` field.
2. Change `PrinterClient::new()`'s signature from `(tls, factory, ip: &str, serial: &str, access_code: &str, model)`
   to `(tls, factory, identity: PrinterIdentity, model)` (or `&PrinterIdentity` +
   clone internally — pick whichever reads better once the surrounding builder-chain
   code is in front of you; both are fine, this is not a decide-first question).
3. Update every internal read of `self.ip`/`self.serial`/`self.access_code`
   throughout `src/client/*.rs` to `self.identity.ip`/`.serial`/`.access_code` (grep
   for `self.ip`, `self.serial`, `self.access_code` after this phase starts — do not
   guess the count, the field rename means every existing reference needs the
   `.identity.` segment inserted).
4. Update the 14 `PrinterClient::new(...)` call sites listed above to construct a
   `PrinterIdentity` instead of passing three loose strings.

### Phase 4 — `MqttClient::connect()` switches to `PrinterIdentity`

Depends on Phase 2; ideally also Phase 3 (so `ensure_mqtt()` can pass `&self.identity`
directly — otherwise construct a throwaway `PrinterIdentity` from loose fields here
instead of blocking on Phase 3, and reconcile when Phase 3 lands). Independent of
Phase 1, which already landed the `serial`-storage/`from_mqtt()` half of this work.

1. Change `MqttClient::connect(stream, serial: &str, access_code: &str)` to
   `connect(stream, identity: &PrinterIdentity)` — reads `identity.serial`,
   `identity.access_code` where the old params were used (`identity.serial` also
   feeds the `serial` field Phase 1 added); `identity.ip` is unread (rule 3).
2. Update `PrinterClient::ensure_mqtt()` (`src/client/connect.rs`) to pass
   `&self.identity`.
3. Update every `MqttClient::connect(...)` call site (re-counted per the Scope
   section above).

### Phase 5 — FTPS's three constructors switch to `PrinterIdentity`

Depends on Phase 2. Independent of Phases 1/3/4, but touches `src/client/connect.rs`'s
`ensure_ftps()` which Phase 3 also touches (same file, different function) — do
Phase 3 first to avoid a merge headache within one file, not a hard technical
dependency.

1. `BambuFtpsClient::connect(raw_control, tls_connector, data_factory, model, identity: PrinterIdentity, timer, allow_unverified_tls_1_2)`
   — `ip`/`serial`/`access_code` collapse into one param; `allow_unverified_tls_1_2`
   stays a separate `bool` (it's not identity data, don't fold it in just because
   it's also currently a trailing param).
2. `connect_control_stream()` takes `&PrinterIdentity` (borrowed — it doesn't own the
   data past this call) even though it only reads `.serial`/`.access_code` internally
   (`ip` isn't needed until the data channel dials later) — this is FTPS, rule 3 says
   bundle regardless of which subset a given internal helper reads, because the
   *public* `connect()` entry point needs all three and every caller has them.
3. `from_control_stream()` takes `&PrinterIdentity` similarly (reads `.ip`/`.serial`,
   not `.access_code` — already consumed by the handshake that produced its
   `control_stream`).
4. Update `src/client/connect.rs::ensure_ftps()`'s two call sites
   (`connect_control_stream`, `from_control_stream`) to pass `&self.identity`.
5. Update the 9 direct `BambuFtpsClient::connect(...)` call sites in
   `tests/ftps_test.rs`.

### Phase 6 — Camera's `authenticate()` switches to `PrinterIdentity`

Depends on Phase 2. Small, independent of Phases 1/3/4/5 — safe to do any time
after Phase 2, including standalone in its own session.

1. `BambuBinaryCameraStream::authenticate(&mut self, identity: &PrinterIdentity)` —
   reads `identity.access_code`, `.ip`/`.serial` unread (rule 3). `new(stream)`
   itself is unaffected (still takes just the stream — identity data isn't needed
   until `authenticate()`).
2. Update `PrinterClient::ensure_camera()` (`src/client/connect.rs`) to pass
   `&self.identity` (same throwaway-construction note as Phase 4 if Phase 3 hasn't
   landed yet in this session).
3. Update every direct `.authenticate(...)` call site (re-counted per the Scope
   section above).

### Phase 7 — Docs regen and BACKLOG close-out

Per this repo's `backlog` skill: run `make docs`, commit the regenerated `docs/`
separately from the fix commits (per `CLAUDE.md`'s Docs regen convention — batch it,
don't pay the cost per-fix). Move `BUG-196` from `Open` to `Fixed`, `Detail` column
citing this plan file **and** the fix commit's short hash (per the backlog skill's
review-file-lifecycle convention) — then delete this plan file in its own commit
once every phase has landed, same convention as `RENAME-BAMBUERROR-PLAN.md`'s
deletion.

## What this plan deliberately does not do

- Does not change `BUG-072`'s runtime-panic guard to a compile-time one (rule 6,
  "out of scope, deliberately").
- Does not give `PrinterClient::from_mqtt()` a `PrinterIdentity` — the one
  structural exception in rule 3, not an oversight. Do not "complete the set" by
  adding it later without re-deriving why it was excluded.
- Does not add `Option` fields to `PrinterIdentity` under any circumstance (rule 4).
