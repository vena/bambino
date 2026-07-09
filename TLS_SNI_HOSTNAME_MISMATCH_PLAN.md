# TLS SNI/Hostname Mismatch + v1-Cert Chain Validation — Fix Plan (v2)

## Status as of this writing

This is a full rewrite of the original plan after implementation revealed a second, deeper
problem beyond the original SNI/CN scope. Read this whole doc before touching anything —
Part 1 is done and must not be reworked; Part 2 is the actual remaining task, and its
approach has already been decided (not left as an open fork) because the user directing this
work has no TLS background and asked for a firm recommendation rather than more options.

## Part 1 — DONE, verified, do not rework

### 1a. Serial-vs-IP parameter fix (the original bug)

Every `TlsConnector::connect()` call site now passes the printer's serial, not its IP, as
`host`/SNI:
- `src/client/connect.rs` `ensure_mqtt()` and `ensure_camera()`
- `src/ftps/client.rs` `connect()` (control channel) and `open_data_channel()` (data channel)
  — `BambuFtpsClient` gained a new `serial: String` field and `connect()` a new `serial: &str`
  parameter (breaking, pre-1.0), inserted between `ip` and `access_code`.

This applies uniformly across **all** platforms (tokio/esp-idf/embassy) since
`client/connect.rs`/`ftps/client.rs` are shared, non-platform-specific code — there is nothing
platform-specific left to do for this part.

Regression tests, passing: `tests/client_test.rs::test_ensure_mqtt_connects_tls_with_serial_not_ip`,
`tests/ftps_test.rs::test_ftps_control_channel_connects_with_serial_not_ip`, both using the new
`HostCapturingTlsConnector` mock (`tests/common/io.rs`) to assert the actual string sent over
the wire, not just that the code compiles.

**This part is correct and complete. Do not touch it.**

### 1b. esp-idf and embassy verified-mode paths are confirmed unaffected by anything in Part 2

- `EspIdfTlsConnector::with_certs()` (`src/io/esp_idf.rs:588+`) uses ESP-IDF's vendored mbedTLS
  via `esp_idf_svc::tls`.
- `EmbassyTlsConnector::with_ca_chain()` (`src/io/embassy.rs:123+`) uses `mbedtls-rs`.

Both are real mbedTLS. Confirmed via its actual C source
(`espressif/mbedtls` @ `ffb280b`, `library/x509_crt.c::x509_crt_verify_name`) that mbedTLS (a)
falls back to matching Subject CN when no SAN extension is present, and (b) — newly confirmed
this session against real hardware — has no equivalent of rustls-webpki's "v3 only" policy
(see Part 2), so it accepts the real v1 Bambu certs without complaint. Once 1a's fix lands
(already done), both platforms' verified-mode paths work correctly against real printers with
**zero additional code**.

**No further esp-idf/embassy work needed for this bug, ever, unless a future session proves
otherwise against real hardware.**

### 1c. Diagnostic tooling (keep, useful going forward)

- `bambino-cli inspect-cert <ip> <serial> [--port] [--output]`
  (`src/bin/bambino-cli/inspect_cert.rs`) — captures a printer's raw leaf cert DER to a local
  file for offline inspection (e.g. `openssl x509 -inform DER -noout -text`). **Never commit
  the output file** — it contains the target printer's real serial number in its Subject CN;
  `.gitignore` now has a blanket `*.der` rule specifically because of this (a captured file was
  briefly left untracked in the repo root this session and had to be cleaned up — don't
  reintroduce that).
- `bambino-cli verify-tls <ip> <serial> --port <port> --ca-cert <path>`
  (`src/bin/bambino-cli/verify_tls.rs`) — attempts a real verified handshake using
  `build_verified_client_config`, reports success or the exact failure. **This is what
  surfaced the Part 2 blocker and is the acceptance test for the Part 2 rework** — don't
  consider Part 2 done without running this against real hardware again.
- `certs/bbl-ca-root.pem` (gitignored, never shipped/committed) — the real Bambu root CA cert
  (`Subject: C=CN, O=BBL Technologies Co., Ltd, CN=BBL CA`, self-signed) that directly signs a
  real captured P1S leaf cert, confirmed via `openssl x509 -noout -subject -issuer` matching
  byte-for-byte. Extracted from the `pybambu` Home Assistant integration's bundled cert file
  (`ha-bambulab/custom_components/bambu_lab/pybambu/certs/bambu.cert` — 5 concatenated PEM
  blocks; this is block 5, the root). Use this to test the Part 2 implementation against real
  hardware. Do not regenerate or fabricate a substitute — this is Bambu's actual production
  root cert, already public via the pybambu project, and the file's provenance matters if
  anyone ever audits why a client trusts it.

## Part 2 — CONFIRMED BROKEN, needs a full rework (this is the actual remaining work)

### What's broken, and how it was found

`CnFallbackServerVerifier` (`src/io/tokio.rs`, added earlier this session) was built on the
assumption that `rustls-webpki`'s chain-validation machinery
(`verify_server_cert_signed_by_trust_anchor`, `ParsedCertificate`/`EndEntityCert`) could be
reused wholesale, with only the SAN-then-CN name-matching step replaced. **This assumption is
wrong for real Bambu printer certs**, and the rcgen-based unit tests that shipped alongside it
did not catch this, because `rcgen` defaults to emitting v3 certificates — masking the exact
real-world failure mode. Do not trust rcgen-only coverage as proof this works; it wasn't.

Confirmed live against a real P1S via `bambino-cli verify-tls`:
```
Error: Protocol violation: Verified TLS handshake with 192.168.1.158:990 (SNI=01P00A4C2009981)
FAILED: invalid peer certificate: Other(OtherError(UnsupportedCertVersion))
```

Root cause, confirmed from `rustls/webpki` source (pinned tag `v/0.103.13`,
`src/cert.rs:81,258-269`):
```rust
// mozilla::pkix supports v1, v2, v3, and v4, including both the implicit
// (correct) and explicit (incorrect) encoding of v1. We allow only v3.
fn version3(input: &mut untrusted::Reader<'_>) -> Result<(), Error> {
    der::nested(input, der::Tag::ContextSpecificConstructed0, Error::UnsupportedCertVersion, |input| {
        let version = u8::from_der(input)?;
        if version != 2 { return Err(Error::UnsupportedCertVersion); }
        Ok(())
    })
}
```
`Cert::from_input` — used by both `EndEntityCert::try_from`/`ParsedCertificate::try_from` **and**,
critically, by the free functions `verify_tls12_signature`/`verify_tls13_signature` themselves
(they independently call `webpki::EndEntityCert::try_from(cert)` again on the same leaf DER
during the handshake's signature-verification step — confirmed from `rustls/rustls` v0.23.41
`src/webpki/verify.rs:161,196`) — calls `version3()` unconditionally. Real Bambu leaf certs are
X.509 **v1** (confirmed: the captured P1S cert has no version tag at all — implicit v1
encoding, per RFC 5280 §4.1.2.1), so this fails immediately, before any name-check logic ever
runs.

**This means no part of rustls-webpki's cert handling can be reused for a real Bambu leaf cert
— not chain validation, not the handshake-signature check.** This is a known, previously
reported limitation with other real-world self-signed device certs, confirmed via web search,
not something specific to Bambu or a misreading:
- https://github.com/rustls/rustls/issues/1298 — "invalid peer certificate:
  UnsupportedCertVersion", the identical error string, hit by an unrelated user.
- https://github.com/rustls/webpki/issues/205 — "Support self-signed certificate".
- https://github.com/rustls/rustls/issues/772 — "Working around invalid self-signed
  certificates". LND hit the exact same wall with its own self-signed device cert; their
  resolution (a `SingleCertVerifier` comparing the peer cert byte-for-byte against a pinned
  expected cert, bypassing webpki's chain logic entirely) is the community-blessed pattern this
  plan adapts below, with one difference explained in the next section.

### Why this doesn't touch esp-idf/embassy, and why the fix is host-only

mbedTLS (both `EspIdfTlsConnector` and `EmbassyTlsConnector`) has no version-3-only policy —
this is a webpki/mozilla::pkix-specific design choice (explicit in the source comment above),
not a general TLS-library requirement. Both platforms already validate the real v1 certs
correctly today (Part 1b). **The fix below touches `src/io/tokio.rs` only** — this also
answers an explicit embedded-performance concern raised while deciding this: adding a new
dependency (`x509-parser`, see below) gated behind the `tokio` feature only means esp-idf and
embassy never compile it, never link it, and see zero binary/RAM/flash impact — they keep
using their existing hardware-accelerated mbedTLS path, untouched.

### Decided approach: verify the leaf is signed by a caller-supplied trusted root, bypassing `rustls-webpki`'s `Cert` type entirely

This is a firm decision, not a fork for the next session to pick between. Two shapes were
weighed:

- **Exact leaf pinning** (LND's approach): `verify_server_cert` just compares the peer's cert
  byte-for-byte against one expected cert. Simplest possible `verify_server_cert`, but every
  user would need to pre-capture their own printer's exact cert bytes (via `inspect-cert`) and
  re-pin whenever it regenerates (firmware update, factory reset).
- **Signed-by-root** (chosen): `verify_server_cert` checks the leaf's signature validates under
  a caller-supplied trusted root's public key, plus expiry and an issuer-matches-root-subject
  check. Works for any printer signed by that root, survives individual device cert rotation.

The two shapes need almost the same amount of new code, because **both** require the harder,
unavoidable half regardless: a custom `verify_tls12_signature`/`verify_tls13_signature` that
extracts the leaf's own public key (SPKI) from the raw DER (bypassing `EndEntityCert::try_from`,
which would fail again) and calls `SignatureVerificationAlgorithm::verify_signature` directly —
this is the check that actually proves the peer holds the private key matching the presented
cert during the live handshake (per the LND issue's own reasoning: chain trust alone doesn't
stop MITM here, this check does). Given that machinery is required either way, "signed-by-root"
only adds: one more `verify_signature` call (leaf's signature over its own TBS bytes, checked
against the root's SPKI), a byte-equality check (leaf's issuer field == root's subject field,
both raw DER — no DN parsing needed), and two date comparisons (leaf's NotBefore/NotAfter
against current time). That marginal cost is small enough that the meaningfully more robust
option — survives cert rotation, works for any printer under the pinned root, doesn't need
every user to re-run `inspect-cert` after a firmware update — is worth it.

### Implementation

**New dependency**: `x509-parser` (pure Rust, actively maintained, does not enforce webpki's
v3-only policy — it's a general ASN.1/X.509 *parser*, not a policy-enforcing validator). Verify
its current API via `find-docs`/context7 before writing code against it — do not assume method
names from training data. Add as `optional = true` in `[dependencies]`, listed **only** under
the `tokio` feature array in `Cargo.toml` (mirroring exactly how `tokio-rustls`/
`rustls-pki-types` are already gated). After adding it, confirm
`cargo check --no-default-features --features embassy --lib` and the plain `alloc`-feature
build still pass with `x509-parser` never appearing in their dependency graphs — they should,
since nothing in `src/io/esp_idf.rs`/`src/io/embassy.rs` would ever reference it.

Replace `CnFallbackServerVerifier`'s current internals (delete the hand-rolled `read_tlv`/
`parse_subject_and_san`/`extract_common_name`/`extract_san_dns_names`/`read_extension_octet_string`/
`dns_names_from_san_value` DER walker added this session in full — `x509-parser` supersedes it
entirely; do not keep two parsers side by side) with:

1. **Construction**: `CnFallbackServerVerifier::new(ca_certs: impl IntoIterator<Item = CertificateDer<'static>>) -> Result<Self, ...>`
   — keep the existing plural `ca_certs` shape (matches `build_verified_client_config`'s
   existing public signature) rather than narrowing to one root, even though only P1S's flat
   chain (leaf signed directly by `BBL CA`, no intermediate) is confirmed so far. Bambu ships
   multiple root/intermediate CAs across its product line (`BBL CA`, `BBL CA2 RSA`,
   `BBL CA2 ECC`, confirmed from the `certs/`/pybambu bundle) — a different model's leaf might
   chain through a `BBL Device CA <model>` intermediate instead of signing directly under the
   root. Parse each supplied cert once at construction via
   `x509_parser::certificate::X509Certificate::from_der`, storing each one's raw `subject` DER
   bytes and SPKI. Still **no multi-level path-building** — try each supplied cert as a
   **direct** signer of the leaf only. If a specific model turns out to need a 2-hop chain,
   that's new information requiring a fresh investigation (repeat the `inspect-cert`/
   `verify-tls` cycle against that model), not something to guess at now.

2. **`verify_server_cert`**:
   - Parse `end_entity` via `x509_parser::certificate::X509Certificate::from_der` (works for
     v1 — no version check enforced by this crate).
   - For each candidate trusted cert from construction: check raw-byte equality of
     `end_entity`'s issuer field against the candidate's subject field; if it matches, verify
     `end_entity`'s signature (TBS bytes + declared signature-algorithm OID + signature value,
     all read via `x509-parser`'s parsed structure) validates under the candidate's SPKI using
     `rustls_pki_types::SignatureVerificationAlgorithm` (map the cert's signature-algorithm OID
     to the concrete algorithm constant reachable via
     `rustls::crypto::ring::default_provider().signature_verification_algorithms` — resolve the
     exact mapping mechanism during implementation, don't hardcode an assumption; the captured
     P1S leaf is `sha256WithRSAEncryption` per its `openssl x509 -text` dump, so RSA PKCS#1
     SHA-256 must be supported at minimum, but do not assume every Bambu model uses the same
     algorithm — X2D and H2 series ship an ECC-signed CA too, per `certs/bambu_x2c_260425.cert`).
   - Check expiry: `end_entity`'s NotBefore/NotAfter (via `x509-parser`) against `now: UnixTime`.
   - If no candidate signed it, or expiry fails: reject with the specific
     `rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer)` or
     `CertificateError::Expired`/`CertificateError::NotValidYet` as appropriate — not a generic
     error, so callers get an accurate failure reason.
   - Then run the existing SAN-then-CN identity check (this logic is already correct from this
     session — just re-point its data source from the hand-rolled parser to `x509-parser`'s
     parsed SAN/subject-CN fields instead of re-deriving the extraction).

3. **`verify_tls12_signature`/`verify_tls13_signature`**: cannot delegate to
   `WebPkiServerVerifier` or the free functions — both re-parse via `EndEntityCert::try_from`,
   which fails again on a v1 cert. Implement directly: parse `cert` via `x509-parser` to get
   SPKI, map `dss.scheme` to a candidate algorithm list (mirror
   `WebPkiSupportedAlgorithms::mapping`'s shape — reachable as
   `rustls::crypto::ring::default_provider().signature_verification_algorithms.mapping`, a
   public field even though its containing type isn't independently nameable; iterate its
   entries directly rather than trying to name the type), and call
   `alg.verify_signature(spki, message, dss.signature())`, trying each candidate in turn for
   TLS 1.2 (matching `verify_tls12_signature`'s documented try-all-candidates behavior) but
   only the first match for TLS 1.3 (matching `verify_tls13_signature`'s documented
   first-match-only behavior) — mirror the existing free functions' documented semantics
   exactly, don't invent different behavior.

4. **`supported_verify_schemes`**: doesn't touch cert parsing at all (just advertises supported
   `SignatureScheme`s in the ClientHello) — keep the existing static list already used in
   `NoCertificateVerification`/the current `CnFallbackServerVerifier`, or derive it from
   `provider.signature_verification_algorithms.supported_schemes()` if reachable the same way
   `.all`/`.mapping` are (check during implementation).

### Tests

The existing rcgen-based unit tests (`test_cn_fallback_verifier_*`) validated the SAN/CN logic
but **did not catch this bug**, because `rcgen` defaults to v3 certs. Do not trust rcgen-only
coverage for this rework — it already fooled one attempt.

1. **A genuine v1 test fixture — the one test that would have caught this session's mistake.**
   `rcgen`'s public API was not found to support emitting v1 certs directly when checked this
   session — re-verify that's still true before assuming it. If genuinely absent, construct a
   v1 cert by hand: build a minimal TBSCertificate DER (no version tag, no extensions block —
   actually *simpler* to construct than a v3 cert, since there's no extensions block to encode
   at all), containing a known test CN, sign it with a real key (`rcgen::KeyPair` or a direct
   `ring`/`rsa` crate signing call — resolve which during implementation) under a known test
   root, and feed the raw signed bytes into the verifier as the "peer cert" in a test.
   **Do not consider this rework done without this test passing.**
2. Existing rcgen-based SAN/CN fallback tests (SAN-present-with-mismatched-CN,
   SAN-absent+CN-match, SAN-absent+CN-mismatch) — keep, still structurally valid. The
   SAN-present case can stay rcgen/v3-based (that path was never in question — v1 certs are
   always SAN-absent by construction, since v1 can't carry extensions at all, so a "v1 cert
   with a SAN" is a contradiction and doesn't need its own test). The two SAN-absent cases
   should be re-run against the new v1 fixture from point 1, not just the old v3-but-no-SAN
   rcgen shape, since that shape is not what a real printer's cert looks like.
3. Untrusted-chain rejection test (already exists) — keep, adapt to whatever the new
   signature-based rejection path's exact error variant turns out to be.
4. Expiry rejection test — new, not previously covered, and now our own responsibility to get
   right since this isn't delegated to webpki anymore.
5. **Final acceptance test, not automated, and the one that actually matters**: re-run
   `bambino-cli verify-tls <p1s-ip> <serial> --ca-cert certs/bbl-ca-root.pem` against the real
   P1S and confirm it now reports success. The unit tests above are necessary but this is the
   actual bar for "done" — it's the only thing that caught the first attempt's bug when
   comprehensive rcgen-based unit tests didn't.

### Docs

- `README.md`'s TLS configuration section: update the `CnFallbackServerVerifier` description to
  reflect the `x509-parser`-based implementation; remove the (no-longer-true) description of
  delegating to `rustls`'s own `verify_server_cert_signed_by_trust_anchor`/`WebPkiServerVerifier`.
- `CLAUDE.md`'s "Non-Obvious Type Decisions" bullet: rewrite to describe the actual final
  implementation (x509-parser, signed-by-root, own signature verification for both the
  chain-check and the handshake-check) and the `UnsupportedCertVersion` finding with its
  GitHub issue citations, so a future reader understands *why* this couldn't just delegate to
  rustls-webpki, instead of re-discovering it the hard way.
- Note in both: verified against a live P1S via `bambino-cli verify-tls` (flat chain, leaf
  signed directly by `CN=BBL CA`, no intermediate) — but other models' chains (P2S/X2D/H2
  series, which use per-model `BBL Device CA <name>` intermediates per the `certs/bambu_*.cert`
  samples) have **not** been confirmed end-to-end and should be treated as unverified until
  someone runs `verify-tls` against one of those models directly.

### Verification gates

Same five as the original plan, re-run after this rework:
```sh
cargo build
cargo test
cargo build --no-default-features --features alloc --lib
cargo check --no-default-features --features embassy --lib
cargo clippy
```
Plus the real-hardware acceptance test above (Tests, point 5) — not optional for this change,
given it's the only thing that caught the bug in the first attempt.

## Non-goals

- Do not touch `NoCertificateVerification`/`build_unsafe_client_config` — confirmed correct and
  unaffected; this is what essentially all current usage relies on, and nothing in Part 2
  changes that.
- Do not fold this into the `mbedtls-rs`/Embassy escape-hatch work — orthogonal, per the
  original plan's own reasoning; still true.
- Do not attempt multi-level path-building (intermediate CA chains) speculatively — only
  P1S's flat chain is hardware-confirmed; extend only when a specific model is confirmed via
  `inspect-cert`/`verify-tls` to actually need it.
- Do not add `x509-parser` (or any new dependency introduced by this work) anywhere outside the
  `tokio` feature gate — this is the guarantee that esp-idf/embassy stay untouched.

## Definition of done

1. ✅ `x509-parser`-based `CnFallbackServerVerifier` implemented per the Implementation section,
   entirely replacing the hand-rolled DER walker from the first attempt.
2. ✅ All tests in the Tests section passing, including the hand-built v1 fixture test
   (`test_cn_fallback_verifier_accepts_real_v1_shaped_cert_with_cn_match`) — not just rcgen-based
   v3 coverage, which already proved insufficient once. Full suite green: `cargo build`,
   `cargo test` (all binaries), `cargo build --no-default-features --features alloc --lib`,
   `cargo check --no-default-features --features embassy --lib`, `cargo clippy --all-targets`
   (zero warnings). Confirmed `x509-parser` never appears in the `embassy`-feature dependency
   tree (`cargo tree --no-default-features --features embassy -e normal`).
3. ✅ **Done, confirmed against real hardware**: `bambino-cli verify-tls` against the real P1S
   (`192.168.1.158`, serial `01P00A4C2009981`, `--ca-cert certs/bbl-ca-root.pem`) reported
   `Verified TLS handshake ... succeeded — CnFallbackServerVerifier accepted the printer's
   cert.` This is the item that actually matters — the first implementation attempt passed
   every unit test and still failed against real hardware, so this confirmation is the real
   bar for done, not the unit tests alone.
4. ✅ Docs updated (`README.md`'s TLS configuration section, `CLAUDE.md`'s "Non-Obvious Type
   Decisions" bullet) — both now describe the actual `x509-parser`-based implementation and the
   `UnsupportedCertVersion` finding with GitHub issue citations.
5. ✅ All five verification-gate commands passing (see point 2).
6. Final report note, unchanged from before: only P1S's chain shape is hardware-confirmed
   (flat, leaf signed directly by `CN=BBL CA`, no intermediate). Other models' cert chains
   (P2S/X2D/H2 series use per-model `BBL Device CA <name>` intermediates per the
   `certs/bambu_*.cert` samples) remain unverified until someone runs `verify-tls` against one
   of those models directly — `CnFallbackServerVerifier::new` already accepts multiple trusted
   roots and tries each as a direct signer, so a single-hop intermediate chain would need no
   code change, only a caller supplying the right intermediate cert; a genuine 2-hop chain
   would need new work (see Non-goals).
