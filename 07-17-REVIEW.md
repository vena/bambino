**Status:** COMPLETE

# Quality Review — 2026-07-17

Scope note: this is a **quality-focused** deep review, narrower than the standard `deep-review` skill's correctness sweep. It was requested after several rounds of `*-PLAN.md` implementation work surfaced two recurring smells: narrative code comments (comments that describe a diff — "used to X, now Y", "previously Z" — rather than a durable invariant) and `#[allow(clippy::too_many_arguments)]` directives suppressing a warning instead of addressing the underlying argument count. The crate was partitioned into 17 review units (same boundaries the standard sweep would use), each reviewed in parallel by a subagent for exactly these two smells only — this was not a hunt for functional/correctness bugs.

**Comment-quality bar (root `CLAUDE.md`):** default to no comments; a comment only earns its place when the WHY is non-obvious — a hidden constraint, a subtle invariant, a workaround for a specific bug, behavior that would surprise a reader. Comments must not explain WHAT the code does, and must not reference the current task, a fix, or callers ("used by X", "added for the Y flow", "handles the case from issue #123"). A comment that says "previously did X, now does Y" is a finding only when the historical framing is the *entire* content and no durable invariant survives its removal — a comment that also explains why current behavior must stay a certain way is legitimate and NOT a finding, even if it mentions history in passing.

**`too_many_arguments` bar:** every `#[allow(clippy::too_many_arguments)]` site was a candidate finding. Each was judged on whether the argument list is naturally reducible to a params struct/builder without over-engineering a one-off, versus inherently wide (e.g. 1:1 wire-protocol field mapping) where a struct would just relocate the problem.

**Confidence tagging:** findings were tagged `CONFIRMED` (clear violation) or `PLAUSIBLE` (judgment-dependent). All `PLAUSIBLE` findings were re-verified by direct file read and triaged below — each is either promoted to `BACKLOG.md`'s `Open` table (Sev3) or marked `Wontfix` with a reason. `BACKLOG.md` is the status source of truth from here on; the tables in this file are a point-in-time snapshot.

**Standalone reading note:** this file is meant to be read on its own by a fresh session with no memory of the conversation that produced it. File:line references may have drifted if other changes landed on `main` since this sweep ran.

**Addendum (same day):** after the initial 17-unit sweep (§1–17 below, BUG-207–247), a follow-up question surfaced a broader gap those 17 units didn't fully close: many comments cite an inline `BUG-NNN` tracker ID even where the surrounding text carries no diff-narration at all (e.g. `ams/mapping.rs:50-53`'s BUG-069 citation, explicitly marked Wontfix in §13 below for exactly this reason — no narration, just a bare tag). Those were correctly out of the original two-smell scope, but `BUG-NNN` is this repo's own internal numbering — see `BACKLOG.md`'s own preamble noting it "stands in for a real issue tracker until this repo has a GitHub remote, migrate to Issues then." GitHub Issues assigns its own sequential numbers with no 1:1 mapping to `BUG-NNN`, so every inline tag becomes a dangling reference on that migration day — same rot-class as the "issue #123" pattern CLAUDE.md's comment rule already names. §18 below is a crate-wide inventory of every remaining inline `BUG-NNN`, tracked as one consolidated entry (**BUG-248**) rather than one row per occurrence, since it's a single uniform pattern repeated ~250 times, not 250 distinct bugs.

---

## 1. types/telemetry (core)

NO ISSUES FOUND. All BUG-NNN-tagged `merge_from` doc comments (BUG-034, 091, 093–098, 105–112, 120, 121, 123, 126, 158) state a durable invariant with a cited verification source (BambuStudio/pybambu/bambuddy or a wire capture); none are pure diff-narration. No `allow(clippy::too_many_arguments)` in this unit.

## 2. types/telemetry (tests)

- `src/types/telemetry/tests/bed.rs:80-82` — **CONFIRMED**, promoted as **BUG-207**. Leftover in-progress arithmetic scratch work ("Let me calculate...", "No..."), contradicted by the correct annotation at line 85.
- `src/types/telemetry/tests/misc.rs:606-609` — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-231**. First two lines are a legitimate BUG-067 regression header; the trailing clause ("...instead of the previous verbatim duplicate of test_mc_percent_deserialization, which asserted nothing about progress at all") narrates the test's own history, not production behavior.

No `allow(clippy::too_many_arguments)` in this unit. All other BUG-NNN comments across ams.rs/device.rs/misc.rs/nozzle.rs/bed.rs tests carry a durable wire-format/precedence invariant and are not findings.

## 3. ftps (core)

- `src/ftps/client.rs:175, 217, 321` — **CONFIRMED** (all three), promoted as **BUG-208/209/210**. Empirically verified by temporarily stripping the three `#[allow(clippy::too_many_arguments)]` lines and running `cargo clippy --lib -- -D clippy::too_many_arguments`: zero errors. Actual arg counts are 6, 5, and 7 — clippy's default threshold only fires above 7. All three annotations are dead weight, likely stale from before `PrinterIdentity` folded several params together. None of the three would benefit from a params struct even if the lint did fire — the args are orthogonal generic-typed I/O primitives already coming from the impl's own generics.

No narrative-comment findings — this module is comment-dense but every "previously"/"used to" phrasing carries a durable forward-looking invariant. One incidental, out-of-scope observation (not a finding under this sweep's two smells, noted for awareness): `src/ftps/protocol.rs`'s `validate_ftp_path` doc comment has a duplicated line (copy-paste artifact) in its CR/LF/NUL paragraph.

## 4. ftps (tests)

- `src/ftps/protocol/tests.rs:358-361` — **CONFIRMED**, promoted as **BUG-211**. `test_write_command_sends_single_write_call`'s comment is legitimate for its first two-and-a-half lines (explains a real P1S firmware quirk), but closes with "a bug introduced in commit 6385019 and fixed by combining back into a single write" — a commit-hash citation that rots as history is rewritten/squashed.

No `allow(clippy::too_many_arguments)` in this unit. All other comments in `tests/ftps_test.rs` and `tests/common/mock_ftps.rs` are legitimate WHY-focused rationale.

## 5. mqtt/client

- `src/mqtt/client/mod.rs:121-122` — **CONFIRMED**, promoted as **BUG-212**. "(the previous behavior)" parenthetical on `write_frame`'s doc is pure narration.
- `src/mqtt/client/mod.rs:180-187` — **CONFIRMED**, promoted as **BUG-213**. `connect()`'s CONNACK-read comment references "this fix's target" and "pre-existing connect-time behavior."
- `src/mqtt/client/mod.rs:381-388` — **CONFIRMED**, promoted as **BUG-214**. `poll_telemetry()` names `tests/mqtt_test.rs` as a caller and says it "keeps its exact prior behavior."
- `src/mqtt/client/mod.rs:318-321` (`publish_command()`) — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-232**. "identical to this crate's pre-existing behavior" is diff-narration; the routing guidance to `publish_command_with_timer()` is legitimate and kept.
- `src/mqtt/client/mod.rs:542-546` (`send_ping()`) — **PLAUSIBLE**, re-verified (quote matches). **Wontfix**: "mirroring `publish_command()`" is a legitimate cross-reference between sibling wrapper pairs, not a "previously X" narration — no removal warranted.

No `allow(clippy::too_many_arguments)` in this unit.

## 6. mqtt/commands

- `src/mqtt/commands/ams.rs:68, 266` (`AmsFilamentSettingRequest::new`, `AmsFilamentDryingRequest::new`) — reviewed, **NOT findings**. Both constructors map 1:1 onto their payload's wire fields per this crate's established Payload+Request pattern; a params struct would just relocate the problem.
- `src/mqtt/commands/ams.rs:228-229` (`AmsFilamentDryingPayload` doc) — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-233**. Closing sentence ("The previous schema...shared no field name with either source") narrates a schema a reader never saw; the preceding BambuStudio/bambuddy citation is legitimate and kept.
- `tests/mqtt_test.rs` — **CONFIRMED**, promoted as **BUG-215**. Six numbered step-header comments (`// 1. Spawn...` through `// 6. Test Write-Channel Zombie Detection`) plus two minor restatement comments narrate well-named calls with no attached invariant.

`tests/common/mock_mqtt.rs` and the rest of `src/mqtt/commands/*` — no issues; BUG-079/001/022/033/119-tagged comments all state durable invariants.

## 7. client (core)

NO ISSUES FOUND. `src/client/ams.rs:137`'s `allow(clippy::too_many_arguments)` was reviewed and judged **not a finding** — `start_drying`'s 8 params each independently map to the AMS drying wire payload; a params struct would add indirection without reducing complexity for the single call site. All narrative-shaped comments across the 12 files carry a durable invariant (sequence-ID reseed determinism, BUG-072 panic rationale, fan-port address-space gotcha, etc.).

## 8. client (tests)

- `tests/client_test.rs:217, 221` — **CONFIRMED**, promoted as **BUG-216**. "Bed temperature verification"/"Nozzle temperature verification" restate the M140/M104 string asserted on the next line.
- `tests/client_test.rs:353, 357` — **CONFIRMED**, promoted as **BUG-217**. Same pattern for fan G-code; the trailing `// 50% PWM`/`// 100% PWM` comments on the same lines are legitimate and kept.
- `tests/client_test.rs` systemic "Verify X"/"X verification" header pattern (dozens of instances, e.g. lines 43, 49, 52, 56, 60...) — **PLAUSIBLE**. **Wontfix**: in this 2966-line file's many near-identical multi-assert test blocks, these act as skim-navigation aids for a human scanning broker-task closures — a legitimate readability tradeoff the strict rule doesn't cleanly resolve either way, distinct from the two CONFIRMED instances above (BUG-216/217) which add literally zero information beyond the adjacent line.
- `tests/common/io.rs:126` (`MockDataStreamFactory::active_stream` field doc) — **PLAUSIBLE**. **Wontfix**: mildly redundant with the type signature but consistent with this codebase's convention of documenting struct fields; not a clear narrative-comment violation.
- `tests/common/io.rs:137` (`dial()` comment) — **PLAUSIBLE**, re-verified (quote matches). **Wontfix**: "simulate a standard TCP connection refusal" explains that `ConnectionRefused` here is deliberate mock behavior, not accidental — genuine WHY value.

No `allow(clippy::too_many_arguments)` in this unit. `tests/common/client.rs` and `tests/common/mod.rs` are clean.

## 9. camera

- `tests/camera_test.rs:36-100` — **CONFIRMED**, promoted as **BUG-218**. Numbered step comments (`// 1. Spawn...` through `// 5. Verify Stream Exhaustion`) plus `// Frame 1`/`// Frame 2`/`// Frame 3` restate well-named calls (`authenticate`, `read_next_frame`).
- `tests/common/mock_camera.rs:26-77` — **CONFIRMED**, promoted as **BUG-219**. Same pattern, duplicating the struct doc's own numbered protocol-step list.
- `src/camera/rtsps.rs:296` — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-234**. "Regression for Phase 4.2:" prefix ties the (otherwise legitimate) defect description to a planning-phase checkpoint rather than a stable BUG-ID, unlike this file's other BUG-005-tagged regressions.

No `allow(clippy::too_many_arguments)` in this unit. `src/camera/mod.rs` and `src/camera/binary.rs` are clean.

## 10. discovery

- `src/discovery/mod.rs:151` — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-235**. "Catch-all transient socket timeout. Returns None to allow retry loop cycles." restates the adjacent `Err(TimedOut) => Ok(None)` arm with no non-obvious invariant, unlike the adjacent BUG-046 comment on the `Err(e)` arm.
- `src/discovery/mod.rs:526-530` — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-236**. Test comment narrates a removed "old poll-counting approach" with no BUG-ID anchor and nothing a future reader can verify about it.

No `allow(clippy::too_many_arguments)` in this unit. `src/discovery/parser.rs` is clean — its BUG-NNN comments all explain a live parsing/protocol invariant.

## 11. quirks

NO ISSUES FOUND. No `allow(clippy::too_many_arguments)` in any of the 9 files. Every comment (trait docs, model-specific firmware-bug explanations in p2.rs/x2.rs, the x1.rs bed-temp/voltage rationale) carries a durable, non-obvious invariant with citations.

## 12. diagnostics

- `src/diagnostics/hms.rs:182` — **CONFIRMED**, promoted as **BUG-220**. "is_status_step now compares the full code..." narrates the fix rather than stating the rule the test enforces.
- `src/diagnostics/hms.rs:283` — **CONFIRMED**, promoted as **BUG-221**. Pure before/after narration ("was Unknown from a wrong attr-byte read", "was code_low-only").
- `src/diagnostics/hms.rs:297` — **CONFIRMED**, promoted as **BUG-222**. Same pattern.
- `src/diagnostics/hms.rs:43-46` (`from_code` doc) — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-237**. First sentence + BambuStudio/pybambu citation is legitimate; "BUG-108: previously derived from attr (...)" clause is narration.
- `src/diagnostics/hms.rs:97-100` (`is_status_step` doc) — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-238**. The data-driven justification (4591/4592 real faults) is legitimate and load-bearing; only the "BUG-109:" prefix is a task-reference to drop.
- `src/diagnostics/kprofile.rs:568-569` — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-239**. "previously serialized...skipping clamp_task_id()" narrates the fix; the `.claude/rules/task-id-clamping.md` pointer is legitimate and kept.

No `allow(clippy::too_many_arguments)` in this unit. `src/diagnostics/mod.rs` is clean.

## 13. ams

- `src/ams/mapping.rs:249-254` (`is_external_spool_safety_valid`) — **CONFIRMED**, promoted as **BUG-223**.
- `src/ams/mapping.rs:258-262` (`is_valid_physical`) — **CONFIRMED**, promoted as **BUG-224**.
- `src/ams/mapping.rs:120-125` (`flat_channel_id_for_entry` doc) — **CONFIRMED**, promoted as **BUG-225**. Names a specific caller/file/method chain (`ProjectFileRequest::from_config`, `mqtt/commands/print_job.rs`) — a near-verbatim match of CLAUDE.md's own prohibited example.
- `src/ams/mapping.rs:184-187` (`build_ams_mapping` out-of-range warning) — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-240**. First sentence (why this is a caller bug, not a skippable entry) is a legitimate non-obvious design rationale; "Silently dropping it previously left..." is narration.
- `src/ams/mapping.rs:50-53` (`flat_channel_id` doc, BUG-069) — **PLAUSIBLE**, re-verified (quote matches). **Wontfix**: no "previously"/"used to" language at all — states a real hidden constraint (public fields bypass wire-parsing bounds checks) consistent with this file's pervasive BUG-ID-tagged citation convention.
- `src/ams/parser.rs` (`test_clean_stale_tray_data_clears_drying_fields`, "Phase 4.12 regression:") — **PLAUSIBLE**, promoted as **BUG-241**, consistent with the same phase-label treatment as BUG-234.
- `src/ams/parser.rs:12-21` (`AMS_MAX_STANDARD_ID` doc) — reviewed, **NOT a finding**. Despite "reverted from 7 back to 3" framing, the substance is a durable, load-bearing sourcing note citing three independent sources — exactly CLAUDE.md's own "note the verification source" convention.

No `allow(clippy::too_many_arguments)` in this unit. `src/ams/mod.rs` is a clean re-export module.

## 14. io

- `src/io/esp_idf.rs:~247` (`map_esp_tls_connect_error` doc) — **CONFIRMED**, promoted as **BUG-226**. Ends with "Used by `EspIdfTlsConnector::connect`" — the exact "used by X" pattern the rule names.
- `src/io/esp_idf.rs:~256` (`EspIdfTlsCerts` doc) — **CONFIRMED**, promoted as **BUG-227**. Narrates a deleted type (`EspIdfSecureConnector`/`SecureConnect`) a reader never saw; only the trailing "so a future cert-related option...only needs to be added in one place" is a live rationale.
- `src/io/esp_idf.rs:~590` (`TlsConnector::connect` doc) — **CONFIRMED**, promoted as **BUG-228**. "; previously this loop had no upper bound at all" adds nothing beyond the already-complete bounding-mechanism sentence.
- `src/io/esp_idf.rs:168-171` (`TLS_POLL_INTERVAL` doc) — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-242**. The `non_block=true`→poll→preemptable mechanism explanation is legitimate; "already fixes the actual problem this phase targets" and "which it could not do before" are narration.
- `src/io/esp_idf.rs:~281, ~370` (`build_tls_config`, `query_negotiated_tls_version` docs) — **PLAUSIBLE**, promoted as **BUG-243** (bundled). Both end with "used by X (below)" caller references.
- `src/io/embassy.rs:117, 122, 126, 180, 235` (`EmbassyTlsConnector` docs) — **PLAUSIBLE**, promoted as **BUG-244** (bundled). Repo-wide grep confirmed no `embedded-tls`-backed connector exists anywhere in `src/` anymore; framing five separate facts as "unlike the old embedded-tls connector" gives a reader who never saw that code nothing. Line 126 is the weakest candidate (it uses the comparison to explain *why* `None` is honest, not just guessed) but was bundled into the same entry for a single edit pass.
- `src/io/mod.rs:294-297` (`race()` `pub(crate)` rationale) — **PLAUSIBLE**, re-verified (quote matches). **Wontfix**: names two call sites, but is explaining a real visibility-scope design decision (why `pub(crate)` and not private), not narrating a diff.

No `allow(clippy::too_many_arguments)` in this unit. `src/io/tokio.rs` and `src/io/tokio/cert_verify.rs` are clean.

## 15. bin/bambino-cli (core)

- `src/bin/bambino-cli/connection.rs:67-69` — **CONFIRMED**, promoted as **BUG-229**. Leads with "BUG-130:" and narrates "instead of a narrower CLI-only ceiling that could reject..." rather than stating the constraint (must match `CAMERA_PASSWORD_MAX_LEN`) directly.
- `src/bin/bambino-cli/error.rs:3-9` — **CONFIRMED**, promoted as **BUG-230**. Module doc's rationale paragraph is legitimate; the trailing `(BUG-181)` parenthetical is a stale tracker pointer.

No `allow(clippy::too_many_arguments)` in this unit. `table.rs`'s regression-test comment, `main.rs`'s `AtomicBool` rationale, and the TLS-rule cross-references in `verify_tls.rs`/`inspect_cert.rs` are all legitimate.

## 16. bin/bambino-cli (commands)

- `src/bin/bambino-cli/probe.rs:259-260` — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-245**. The current rule (why `Preparing` counts as busy) is fully stated in the preceding four lines; the "BUG-099: previously fell through to Unknown..." clause is narration.
- `src/bin/bambino-cli/monitor/mod.rs:55-57` — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-246**. "matches run()'s dashboard loop below" is a legitimate cross-reference and is kept; "previously had no zombie detection at all, hanging indefinitely" is narration.

No `allow(clippy::too_many_arguments)` in this unit. `probe.rs:161-162/487-490`, `storage.rs`'s clock/disconnect-ordering comments, and `dashboard.rs`'s MQTT-merge/RAII-ordering comments are all legitimate. `dashboard.rs:12-13`'s embedded `(BUG-194: was write!(...).unwrap_or(()) duplicated at ~24 call sites)` parenthetical is the same class of issue but was judged too minor/embedded to warrant its own entry — left as a documented non-blocking observation, not filed.

## 17. core loose files (error.rs, identity.rs, models.rs, lib.rs)

- `src/error.rs:11` — **PLAUSIBLE**, re-verified (quote matches), promoted as **BUG-247**. The manual/std-only sync-gap description is a legitimate durable invariant; the trailing `(BUG-013)` parenthetical is a stale tracker pointer.

No `allow(clippy::too_many_arguments)` in this unit. `identity.rs`'s struct-bundling rationale, `models.rs`'s serial-prefix/H2C notes, and `error.rs`'s `assert_all_variants_covered` comment are all legitimate.

## 18. Crate-wide: inline `BUG-NNN` tracker IDs (BUG-248, addendum)

Added same day as §1–17, in response to a follow-up question about tracker-migration risk (see the Addendum note at the top of this file). Scope: every comment (`//` or `///`) anywhere in `src/`/`tests/` that contains a literal `BUG-NNN` token, regardless of whether it's paired with narrative framing — a strictly broader net than §1–17's narrative-only check. Found via `ctx_search(action="regex", pattern="BUG-[0-9]+")` scoped separately to `src/` and `tests/`.

**48 files affected, ~250 occurrences total** (~210 in `src/`, ~40 in `tests/`):

`src/`: `ams/mapping.rs`, `ams/parser.rs`, `camera/binary.rs`, `camera/rtsps.rs`, `client/ams.rs`, `client/connect.rs`, `client/motion.rs`, `client/telemetry.rs`, `diagnostics/hms.rs`, `diagnostics/kprofile.rs`, `discovery/mod.rs`, `discovery/parser.rs`, `error.rs`, `ftps/client.rs`, `ftps/parser.rs`, `ftps/protocol.rs`, `ftps/protocol/tests.rs`, `io/embassy.rs`, `io/esp_idf.rs`, `io/tokio/cert_verify.rs`, `io/tokio/tests.rs`, `mqtt/client/codec.rs`, `mqtt/client/frame.rs`, `mqtt/client/mod.rs`, `mqtt/client/pending.rs`, `mqtt/commands/ams.rs`, `mqtt/commands/mod.rs`, `mqtt/commands/print_job.rs`, `quirks/mod.rs`, `quirks/models/a2.rs`, `quirks/models/h2.rs`, `quirks/models/x2.rs`, `types/telemetry/ams.rs`, `types/telemetry/device.rs`, `types/telemetry/diagnostics.rs`, `types/telemetry/mod.rs`, `types/telemetry/report.rs`, `types/telemetry/tests/ams.rs`, `types/telemetry/tests/bed.rs`, `types/telemetry/tests/device.rs`, `types/telemetry/tests/misc.rs`, `types/telemetry/tests/nozzle.rs`.

`tests/`: `client_test.rs`, `common/mock_ftps.rs`, `ftps_test.rs`, `camera_test.rs`, `common/mock_camera.rs`, `common/mock_mqtt.rs`, `common/client.rs`, `telemetry_replay_test.rs`.

**Fix direction (uniform across all instances, per-comment, not per-file):**
1. Strip the `BUG-NNN` token (and any `BUG-NNN:` sentence-lead colon) from the comment text.
2. Keep the rest verbatim, in particular any external citation (`BambuStudio's DevXxx.cpp:NN-MM`, `pybambu`'s/`bambuddy`'s named function, a `[REF-*]` protocol-spec tag, a wire-capture/fixture reference) — those are third-party provenance, not this repo's tracker, and don't rot on a GitHub Issues migration.
3. Where `BUG-NNN` is used purely as a same-file cross-reference between two related comments (e.g. `types/telemetry/device.rs:57`'s "same shape as `AmsStatusReport::merge_from` (BUG-091) one struct up") with no external citation attached, replace the ID with the symbol/function name it's pointing at instead (`AmsStatusReport::merge_from`) — the cross-reference value survives; only the numeric-ID coupling to this repo's tracker goes.
4. Test names containing `BUG-NNN` as part of the Rust identifier itself (e.g. any `test_bug_099_...`-style name, if present) are NOT in scope here — renaming a test function is a separate, higher-blast-radius change than trimming a comment, and identifier-embedded IDs don't feed doc generation the way comments do; call it out separately if it comes up during the actual fix pass.

This is intentionally tracked as one consolidated `BUG-248` row rather than ~250 individual rows: it's a single uniform convention question ("should this repo's internal IDs live inline in comments"), already answered (no), not 250 distinct defects each needing independent triage. A future fix pass can work through the file list above in any order; each file's edits are independent of every other file's.

---

## Summary table

`BACKLOG.md` is the status source of truth from here on — this table is a point-in-time snapshot at sweep completion and will not be updated as bugs get fixed.

| BUG-ID | Sev | Module | File(s) | One-line |
|---|---|---|---|---|
| BUG-207 | Sev3 | telemetry-tests | types/telemetry/tests/bed.rs | Dead scratch-arithmetic comment contradicts the real one three lines down |
| BUG-208 | Sev3 | ftps-core | ftps/client.rs | Dead `too_many_arguments` allow on `connect()` (6 args, threshold 8) |
| BUG-209 | Sev3 | ftps-core | ftps/client.rs | Dead `too_many_arguments` allow on `connect_control_stream()` (5 args) |
| BUG-210 | Sev3 | ftps-core | ftps/client.rs | Dead `too_many_arguments` allow on `from_control_stream()` (7 args) |
| BUG-211 | Sev3 | ftps-tests | ftps/protocol/tests.rs | Regression comment cites a rotting commit hash |
| BUG-212 | Sev3 | mqtt/client | mqtt/client/mod.rs | "(the previous behavior)" parenthetical |
| BUG-213 | Sev3 | mqtt/client | mqtt/client/mod.rs | CONNACK comment references "this fix's target" |
| BUG-214 | Sev3 | mqtt/client | mqtt/client/mod.rs | `poll_telemetry()` names a test-file caller |
| BUG-215 | Sev3 | mqtt/commands | tests/mqtt_test.rs | Six numbered step-narration comments |
| BUG-216 | Sev3 | client-tests | tests/client_test.rs | Temp-verification comments restate the asserted G-code |
| BUG-217 | Sev3 | client-tests | tests/client_test.rs | Fan-verification comments restate the asserted G-code |
| BUG-218 | Sev3 | camera | tests/camera_test.rs | Numbered step-narration comments in handshake test |
| BUG-219 | Sev3 | camera | tests/common/mock_camera.rs | Numbered step comments duplicate the struct doc |
| BUG-220 | Sev3 | diagnostics | diagnostics/hms.rs | Test comment narrates fix, not rule (line 182) |
| BUG-221 | Sev3 | diagnostics | diagnostics/hms.rs | Test comment narrates fix, not rule (line 283) |
| BUG-222 | Sev3 | diagnostics | diagnostics/hms.rs | Test comment narrates fix, not rule (line 297) |
| BUG-223 | Sev3 | ams | ams/mapping.rs | `is_external_spool_safety_valid` narrates old bug |
| BUG-224 | Sev3 | ams | ams/mapping.rs | `is_valid_physical` narrates old bug |
| BUG-225 | Sev3 | ams | ams/mapping.rs | `flat_channel_id_for_entry` doc names a specific caller |
| BUG-226 | Sev3 | io | io/esp_idf.rs | "Used by X" caller reference |
| BUG-227 | Sev3 | io | io/esp_idf.rs | Narrates a deleted type reader never saw |
| BUG-228 | Sev3 | io | io/esp_idf.rs | "previously this loop had no upper bound" clause |
| BUG-229 | Sev3 | cli-core | bin/bambino-cli/connection.rs | BUG-130 citation + diff-narration |
| BUG-230 | Sev3 | cli-core | bin/bambino-cli/error.rs | Stale `(BUG-181)` parenthetical |
| BUG-231 | Sev3 | telemetry-tests | types/telemetry/tests/misc.rs | Trailing clause narrates test's own prior form |
| BUG-232 | Sev3 | mqtt/client | mqtt/client/mod.rs | `publish_command()` "pre-existing behavior" clause |
| BUG-233 | Sev3 | mqtt/commands | mqtt/commands/ams.rs | Doc narrates old field schema reader never saw |
| BUG-234 | Sev3 | camera | camera/rtsps.rs | "Regression for Phase 4.2:" planning-checkpoint label |
| BUG-235 | Sev3 | discovery | discovery/mod.rs | Comment restates adjacent match arm |
| BUG-236 | Sev3 | discovery | discovery/mod.rs | Test comment narrates removed implementation |
| BUG-237 | Sev3 | diagnostics | diagnostics/hms.rs | "BUG-108: previously derived from attr" clause |
| BUG-238 | Sev3 | diagnostics | diagnostics/hms.rs | "BUG-109:" prefix on otherwise-legitimate doc |
| BUG-239 | Sev3 | diagnostics | diagnostics/kprofile.rs | "previously serialized...skipping" narrates fix |
| BUG-240 | Sev3 | ams | ams/mapping.rs | "Silently dropping it previously left..." clause |
| BUG-241 | Sev3 | ams | ams/parser.rs | "Phase 4.12 regression:" planning-checkpoint label |
| BUG-242 | Sev3 | io | io/esp_idf.rs | "this phase targets...which it could not do before" |
| BUG-243 | Sev3 | io | io/esp_idf.rs | Two "used by X (below)" caller references |
| BUG-244 | Sev3 | io | io/embassy.rs | Five comparisons to a fully-removed connector type |
| BUG-245 | Sev3 | cli-commands | bin/bambino-cli/probe.rs | "BUG-099: previously fell through" on complete rule |
| BUG-246 | Sev3 | cli-commands | bin/bambino-cli/monitor/mod.rs | "previously had no zombie detection" clause |
| BUG-247 | Sev3 | core | error.rs | Stale `(BUG-013)` parenthetical |
| BUG-248 | Sev3 | crate-wide | 48 files, ~250 comments | Inline `BUG-NNN` tracker IDs will dangle on a future issue-tracker migration (§18 addendum) |

**Wontfix (PLAUSIBLE findings triaged as not real violations):**
- mqtt/client/mod.rs:542-546 — legitimate sibling cross-reference, no "previously" language.
- tests/client_test.rs systemic "Verify X" headers — legitimate skim-navigation aid in a large multi-assert test file.
- tests/common/io.rs:126 — mildly redundant field doc, not a clear violation.
- tests/common/io.rs:137 — explains deliberate (not accidental) mock behavior.
- ams/mapping.rs:50-53 — states a real hidden constraint, no diff-narration language.
- io/mod.rs:294-297 — legitimate `pub(crate)`-visibility design rationale.

**Totals:** 24 CONFIRMED + 17 promoted PLAUSIBLE = 41 new `BUG-ID`s from the §1–17 sweep (BUG-207–BUG-247), all Sev3 (comment/doc-hygiene and dead-lint-suppression debt — non-blocking, tracked). 6 PLAUSIBLE findings closed as Wontfix after re-verification. 4 of 17 units (telemetry-core, client-core, quirks) came back completely clean on both original smells. Plus **BUG-248** (§18 addendum): one consolidated crate-wide entry covering ~250 inline `BUG-NNN` tracker-ID references across 48 files, not counted in the 41 above since it's tracked as a single row, not per-occurrence.
