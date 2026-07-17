# Fold `PrinterModel` into `PrinterIdentity`

## Problem

`PrinterIdentity` (`src/identity.rs`, shipped by the now-deleted
`PRINTER-IDENTITY-PLAN.md`) bundles `ip`/`serial`/`access_code`. Every constructor
that takes it — `PrinterClient::new()`, `BambuFtpsClient::connect()`/
`connect_control_stream()`/`from_control_stream()` — *also* takes a separate
`model: PrinterModel` argument alongside it. That's the same shape of redundancy
`PrinterIdentity` was built to remove for the other three fields: two related pieces
of data about the same printer, passed as two independent arguments with nothing
tying them together.

The asymmetry that makes this worth doing (and is why `model` wasn't included the
first time — it wasn't considered until raised separately after that plan shipped):
`model` is **derivable from `serial`** via the crate's own `resolve_model(serial: &str, dev_model: Option<&str>) -> PrinterModel`
(`src/models.rs:46`). `ip` has no equivalent — there's no `resolve_ip(serial)`, it's
irreducibly external data. That derivability is what lets `PrinterIdentity` absorb
`model` "for free," the same way `MqttClient::connect()` and
`BambuBinaryCameraStream::authenticate()` absorbed the struct despite not reading
`ip`: their callers necessarily already have the value, or (for `model`) can trivially
produce it from data they already have.

## Design decisions

1. **`model: PrinterModel` becomes a fourth field on `PrinterIdentity`.** Mandatory,
   `pub`, same as the other three — no `Default`. `PrinterModel::Unknown` exists as
   an honest "I don't know" sentinel already handled by `quirks()` (conservative
   X1C fallback, logged), so a case could be made for defaulting to it — rejected
   anyway, for consistency: every other field on this struct is deliberately
   supplied, not silently defaulted, and mixing one defaulted field into an
   otherwise-fully-explicit struct is a worse inconsistency than the minor
   convenience it buys.

2. **New convenience constructor, existing literal construction untouched.**
   `PrinterIdentity::new(ip: &str, serial: &str, access_code: &str) -> Self`,
   auto-resolving `model` via `resolve_model(serial, None)`. This is the *first*
   constructor this type has ever had — every existing call site
   (~25, enumerated below) builds it via a plain struct literal
   (`PrinterIdentity { ip, serial, access_code }`), and that stays fully supported;
   all fields remain `pub`. `::new()` is for callers who don't want to think about
   `model` at all. A caller who needs a specific `model` regardless of what the
   serial prefix implies just writes the literal with an explicit `model: ...` —
   no separate override method needed, the struct's existing openness already
   covers that case.

3. **`PrinterClient` drops its own `model: PrinterModel` field — full dedup, not
   just the constructor arguments.** `PrinterClient` currently stores *both*
   `identity: PrinterIdentity` and a separate `model: PrinterModel`
   (`src/client/mod.rs:121-122`), kept in sync manually at construction time. This
   was raised as a real fork: fix only the constructor-argument redundancy (touch
   ~14+9 call sites, leave `self.model` as a second, independently-settable field
   that something could someday desync from `self.identity.model`), or fully
   collapse it (touch the 26 `self.model` read sites too, but leave exactly one
   source of truth). **Decided: full collapse.** Anything less reintroduces the
   drift risk this entire effort exists to close, just moved from "constructor
   argument" to "long-lived struct field." `PrinterClient` stores only `identity`
   after this; `self.model` becomes `self.identity.model` everywhere.

4. **`MqttClient::connect()` and `BambuBinaryCameraStream::authenticate()` need no
   changes.** Neither reads `model` today, and adding it to `PrinterIdentity`
   doesn't change that — they already take `&PrinterIdentity` and simply don't read
   the new field, same as they already don't read `.ip`. Nothing to do here.

5. **`PrinterClient::from_mqtt(mqtt_client, model)` keeps its explicit `model`
   parameter, unrelated to this plan.** Its exclusion from taking a full
   `PrinterIdentity` as a *parameter* was already settled (it can't supply
   `ip`/`access_code` — its caller receives an already-connected `MqttClient` and
   may never have had them). That reasoning has nothing to do with `model` and
   isn't reopened here. What *does* need to change: `from_mqtt()` builds a
   `PrinterIdentity` internally for storage (`identity: PrinterIdentity { serial,
   ip: String::new(), access_code: String::new() }`, `src/client/mod.rs:243-247`)
   — that literal needs `model` added as its fourth field, sourced from
   `from_mqtt()`'s own `model` parameter (unchanged otherwise; the separate
   `model,` struct-literal line for `PrinterClient` itself just moves into the
   nested `PrinterIdentity` literal since `PrinterClient` no longer has its own
   field).

## Scope (exact call sites, grep-verified at time of writing)

**`PrinterClient::new(...)`** (drops its `model` argument): 14 sites —
`tests/client_test.rs` (7), `tests/camera_test.rs` (4), `README.md` (1),
`src/lib.rs` (1 crate-level doc example), `src/bin/bambino-cli/connection.rs` (1).

**`BambuFtpsClient::connect(...)`** (drops its `model` argument): 9 direct calls in
`tests/ftps_test.rs`, plus its own internal call to `connect_control_stream()`
(`src/ftps/client.rs:156`) and `PrinterClient::ensure_ftps()`'s two calls
(`connect_control_stream`, `from_control_stream`, `src/client/connect.rs:336,349`).

**`self.model` reads** (become `self.identity.model`): 26 sites across
`src/client/hardware.rs`, `thermal.rs`, `motion.rs`, `telemetry.rs`, `ams.rs`,
`connect.rs` (3 struct-rebuild sites in `with_timer`/`with_ftps`/`with_camera`,
plus `ensure_ftps`/`ensure_camera`), `print.rs`, and `mod.rs`'s own `.model()`
accessor (`src/client/mod.rs:423`) — re-grep `self\.model\b` scoped to `src/client/`
at implementation time rather than trusting this count if any of these files have
changed since.

**`PrinterIdentity { ... }` literal constructions needing a `model` field added**
(mandatory field, regardless of whether the consuming function reads it): ~25 total.
Most overlap with the `PrinterClient::new`/`BambuFtpsClient::connect` sites above
(already had a `model` value in scope, previously passed as a separate argument —
mechanical to move into the literal). The remainder are MQTT-only/camera-only test
sites that never had a `model` in scope at all, since neither `MqttClient::connect()`
nor `BambuBinaryCameraStream::authenticate()` ever took one:
- `tests/camera_test.rs`: 8 (`PrinterIdentity {` at lines 49, 127, 160, 192, 229,
  267, 295, 302 at time of writing)
- `src/mqtt/client/mod.rs`'s own test module: 4 (lines 655, 694, 738, 800)
- `README.md`'s camera example: 1 (line 197, separate from its `PrinterClient::new`
  example at line 65)

For these MQTT-only/camera-only sites, pick `model` per each test's actual intent —
e.g. `test_ensure_camera_rejects_rtsps_model_without_dialing` clearly needs an
RTSPS-protocol model to exercise its own name; a generic handshake test that never
branches on model can reasonably use whatever placeholder is already idiomatic
elsewhere in that file. Don't default-guess a single value for all of them.

## Phases

Each phase should compile and pass `make check-fast` on its own.

### Phase 1 — Add `model` to `PrinterIdentity`

Add the field and doc comment (rule 1), add `PrinterIdentity::new()` (rule 2). No
consumers changed yet — every existing call site still uses the 3-field literal,
which now fails to compile (missing field) until later phases update it. This
phase is *not* independently mergeable the way the original plan's Phase 1 was,
because this type has no `Default` and adding a mandatory field breaks every
existing literal immediately — expect this phase to land together with at least
enough of Phase 2/4 to make the crate compile again, or do it all in one sitting.

### Phase 2 — `PrinterClient` full dedup

Depends on Phase 1.

1. Remove `PrinterClient`'s `model: PrinterModel` field
   (`src/client/mod.rs:121-122`).
2. Update all 26 `self.model` reads to `self.identity.model` (re-grep per the
   Scope section — do not trust this count blindly).
3. `PrinterClient::new(tls, factory, identity: PrinterIdentity, model: PrinterModel)`
   drops its `model` parameter — becomes `new(tls, factory, identity: PrinterIdentity)`.
4. `PrinterClient::from_mqtt(mqtt_client, model)` keeps its `model` parameter (rule
   5) but adds `model` to its internal `PrinterIdentity { .. }` literal
   (`src/client/mod.rs:243-247`) and drops the separate `model,` line that used to
   populate `PrinterClient`'s own now-removed field.
5. Update `connect.rs`'s three struct-rebuild sites (`with_timer`, `with_ftps`,
   `with_camera`) — each currently has an `identity: self.identity, model:
   self.model,` pair; delete the now-dangling `model: self.model,` line (`identity:
   self.identity,` already carries it).
6. Update the 14 `PrinterClient::new(...)` call sites to drop their `model`
   argument, moving that value into the `PrinterIdentity` literal (or switching to
   `PrinterIdentity::new(ip, serial, access_code)` where the call site's existing
   `model` value is exactly what `resolve_model` would derive anyway — check per
   site, don't assume).

### Phase 3 — FTPS's three constructors drop `model`

Depends on Phase 1. Independent of Phase 2, but touches `src/client/connect.rs`'s
`ensure_ftps()`, which Phase 2 also touches (same file) — do Phase 2 first to avoid
a same-file merge headache, not a hard technical dependency.

1. `BambuFtpsClient::connect(raw_control, tls_connector, data_factory, identity: PrinterIdentity, timer, allow_unverified_tls_1_2)`
   — drops the separate `model: PrinterModel` param; every internal use of `model`
   becomes `identity.model`.
2. `connect_control_stream()` and `from_control_stream()` likewise drop their
   `model: PrinterModel` params, reading `identity.model` from the `&PrinterIdentity`
   they already take.
3. Update `ensure_ftps()`'s two call sites and `connect()`'s own internal call to
   `connect_control_stream()`.
4. Update the 9 direct `BambuFtpsClient::connect(...)` call sites in
   `tests/ftps_test.rs`.

### Phase 4 — Sweep remaining `PrinterIdentity` literals

Depends on Phase 1. Update every `PrinterIdentity { ... }` literal not already
touched by Phase 2/3's call-site updates — the MQTT-only/camera-only test sites and
the second README example listed in Scope above. Pick `model` per test intent, not
a blanket default (see Scope section's guidance).

### Phase 5 — Docs regen

Run `make docs`, commit the regenerated `docs/` separately from the fix commits
(per `CLAUDE.md`'s Docs regen convention). No `BACKLOG.md` entry needed — this
didn't originate from a tracked finding, it's direct follow-on design work from the
`PrinterIdentity` rollout; delete this plan file in its own commit once every phase
has landed, same convention as the previous plan's deletion.

## What this plan deliberately does not do

- Does not add a `PrinterIdentity::with_model()` builder/override method — plain
  struct-literal construction (already the crate's own established pattern for
  this type) already covers the "I know the exact model, don't guess" case.
- Does not change `MqttClient::connect()` or `BambuBinaryCameraStream::authenticate()`
  — they already take `&PrinterIdentity` and simply don't read `.model`, same as
  they don't read `.ip`. Nothing about this plan touches their signatures.
- Does not reopen `PrinterClient::from_mqtt()`'s exclusion from taking
  `PrinterIdentity` as a parameter — unrelated to `model`, already settled by the
  previous plan.
