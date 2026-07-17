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

This plan fixes both, and documents why a single shared identity type does **not**
extend to every constructor that happens to take some of these fields — worked out
via discussion, not assumed; see "Why not one struct everywhere" below.

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
   | `MqttClient::connect()` | **No** — keeps plain `serial: &str, access_code: &str` | `ip` is not something `connect()` uses, and a direct caller may not have one in this shape at all (dialed via hostname, embedded/static config, some other addressing scheme). Forcing a mandatory `PrinterIdentity.ip` field on them demands data the function doesn't need and they might not possess. `PrinterClient::ensure_mqtt()` (the internal caller) still benefits from reuse at the *storage* layer — it holds `self.identity: PrinterIdentity` and passes `&self.identity.serial, &self.identity.access_code` — but `MqttClient::connect()`'s own public signature stays untouched. |
   | `BambuBinaryCameraStream::authenticate()` | **No** — keeps its current single `access_code: &str` param | Only ever reads one field. A single-argument function was never a transposition risk in the first place; wrapping it in a 3-field struct would be strictly more complex for zero safety gain. |

   **The test that actually decides this** (apply it to any future constructor,
   don't just eyeball arg count): *does every realistic direct caller of this
   specific public constructor already possess every field the bundled struct would
   require, independent of whether the fields are bundled?* If yes (FTPS,
   `PrinterClient::new`), bundling is free reuse. If no (MQTT's `ip`), bundling forces
   fabricated/unavailable data on some callers — worse than the problem being fixed.

4. **Why not `ip: Option<String>` to let one struct cover every case anyway?**
   Rejected. An `Option` field doesn't remove the risk it's meant to fix, it changes
   its shape: today `MqttClient::connect()` has no `ip` parameter, so passing one is
   a compile error. With `ip: Option<...>` on a shared struct, a caller *can* set
   `Some(ip)` (e.g. copy-pasted from a `PrinterClient::new()` call site) and it
   compiles fine — `MqttClient::connect()` either silently ignores it (no signal at
   all that the field was pointless there) or, worse, someone wires it in later
   because "the field's right there." A rustc-caught mismatch traded for a
   silently-wrong one is a regression, not a fix.

5. **`MqttClient` gains a stored `serial` field + `pub fn serial(&self) -> &str`.**
   Currently `connect()` takes `serial: &str` only to build `client_id` and
   `request_topic`, then discards the borrow — the value survives *inside*
   `request_topic` (`format!("device/{}/request", serial)`) but isn't retrievable
   without parsing it back out. Store it directly instead: `serial: String` on the
   struct, set in `connect()` from the same parameter it already receives.

6. **`PrinterClient::from_mqtt()` signature shrinks to `from_mqtt(mqtt_client, model)`.**
   Reads `serial` via `mqtt_client.serial()` (compute `let serial =
   mqtt_client.serial().to_string();` before moving `mqtt_client` into
   `Self { mqtt: Some(mqtt_client), .. }` — no borrow-check conflict). This doesn't
   just deduplicate a redundant argument, it removes a mismatch class entirely: there
   is no longer any way to hand `from_mqtt()` a `serial` that disagrees with what the
   `MqttClient` actually connected as. Confirmed with the user: there's no legitimate
   reason for `PrinterClient` to report a different serial than the one its own wire
   session used — if you need to control a second printer, you construct a second
   `PrinterClient`.

   `from_mqtt()` does **not** take a `PrinterIdentity` — `ip`/`access_code` are
   structurally unavailable on this path (a caller with only an already-connected
   `MqttClient` never had them, or consumed them elsewhere before this call). This is
   the concrete case that proves rule 3/4's "mandatory-all-three is sometimes
   harmful" — forcing this constructor to accept a full identity would break it (it
   cannot supply `ip`/`access_code`) or perpetuate today's fabricated-empty-string
   pattern under a type that *looks* like it guarantees real data, which is worse
   than plain strings that obviously might be empty.

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

`MqttClient::connect(...)`: signature does not change (rule 3) — no call-site edits
needed beyond whatever already exists.

## Phases

Each phase should compile and pass `make check-fast` on its own — don't batch
phases into one giant diff. Order matters where noted; phases not depending on each
other can be done in either order or split across sessions.

### Phase 1 — Add `PrinterIdentity`, no consumers yet

Create `src/identity.rs` with the struct (rule 1), doc comments explaining the
no-`Option` decision inline (future readers shouldn't have to find this plan doc to
understand why `ip` isn't optional). Re-export from `lib.rs` next to `PrinterModel`.
No existing code changes yet — this phase is additive only, trivially safe to land
alone.

### Phase 2 — `MqttClient` stores `serial`; `from_mqtt()` drops its redundant param

Independent of Phase 1 (`from_mqtt()` doesn't take `PrinterIdentity` — rule 6). Do
this phase in either order relative to Phase 1/3/4.

1. Add `serial: String` field to `MqttClient` (`src/mqtt/client/mod.rs`), set it in
   `connect()` from the existing `serial: &str` parameter (unchanged signature).
2. Add `pub fn serial(&self) -> &str`.
3. Change `PrinterClient::from_mqtt(mqtt_client: MqttClient<IO>, serial: &str, model: PrinterModel)`
   to `from_mqtt(mqtt_client: MqttClient<IO>, model: PrinterModel)` — capture
   `mqtt_client.serial().to_string()` before moving `mqtt_client` into the returned
   `Self`.
4. Update the 3 call sites listed above (`tests/common/client.rs`,
   `tests/telemetry_replay_test.rs`, `README.md`).

### Phase 3 — `PrinterClient::new()` and internal storage switch to `PrinterIdentity`

Depends on Phase 1.

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

### Phase 4 — FTPS's three constructors switch to `PrinterIdentity`

Depends on Phase 1. Independent of Phase 2/3, but touches `src/client/connect.rs`'s
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

### Phase 5 — Docs regen and BACKLOG close-out

Per this repo's `backlog` skill: run `make docs`, commit the regenerated `docs/`
separately from the fix commits (per `CLAUDE.md`'s Docs regen convention — batch it,
don't pay the cost per-fix). Move `BUG-196` from `Open` to `Fixed`, `Detail` column
citing this plan file **and** the fix commit's short hash (per the backlog skill's
review-file-lifecycle convention) — then delete this plan file in its own commit
once every phase has landed, same convention as `RENAME-BAMBUERROR-PLAN.md`'s
deletion.

## What this plan deliberately does not do

- Does not touch `BambuBinaryCameraStream::new()`/`authenticate()` — no transposition
  risk existed there (rule 3, camera row).
- Does not change `BUG-072`'s runtime-panic guard to a compile-time one (rule 6,
  "out of scope, deliberately").
- Does not attempt to unify `MqttClient::connect()`'s signature with FTPS's — rule 3
  explains why that would actively hurt direct MQTT consumers.
