use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{CertificateError, DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, SignatureVerificationAlgorithm, UnixTime};
use tokio_rustls::rustls;
use x509_parser::prelude::FromDer;

/// Custom certificate verifier that disables **all** peer certificate verification.
///
/// This bypasses far more than the CA chain walk: `verify_tls12_signature` and
/// `verify_tls13_signature` both return `HandshakeSignatureValid::assertion()`
/// unconditionally, so the peer never proves possession of the private key matching the
/// certificate it presented. That handshake-signature check — not the chain check alone — is
/// what actually prevents a MITM (see [`CnFallbackServerVerifier`]'s own doc). Identity is not
/// checked either: any certificate from any host is accepted for any name.
///
/// **Why this is the default:**
/// Physical Bambu Lab printers host an onboard local MQTTS/FTPS broker whose leaf cert carries
/// the printer's serial number in the CN field and chains to BBL's own private CA, which is in
/// no OS certificate store — so standard verifiers reject the connection for lack of a trust
/// anchor, not because the cert is bad. Callers holding the BBL CA certs can verify properly via
/// [`super::build_verified_client_config`]; this type is the fallback for callers who don't.
///
/// Earlier revisions of this comment described the leaf as *self-signed*. That is wrong: a live
/// P1S (firmware 01.10.00.00) completed a full chain-verified handshake against the BBL CA
/// anchors, so the leaf is CA-issued and a genuine chain of trust is available.
#[derive(Debug)]
pub struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, RustlsError> {
        // Asserting validation is OK. The printer certificate uses the serial number as its CN.
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        // Expose support for all typical legacy and modern signing configurations to avoid handshake failure.
        vec![
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// Certificate verifier for the "verified" (CA-checked) connection path that validates real
/// chain-of-trust against caller-supplied trusted roots, but — unlike rustls's default
/// `WebPkiServerVerifier` — works against real Bambu printer certs at all.
///
/// **Why this can't use `rustls-webpki`:** real Bambu printer certs are X.509 **v1** (confirmed
/// against a live P1S — no version tag, implicit v1 encoding per RFC 5280 §4.1.2.1).
/// `rustls-webpki` (confirmed against the pinned `0.103.13`, `src/cert.rs::version3`) rejects
/// *any* cert that isn't v3, unconditionally — this is deliberate mozilla::pkix policy
/// ("We allow only v3"), not a bug, and it applies to `EndEntityCert`/`ParsedCertificate`
/// parsing used by chain validation *and* to the free functions `verify_tls12_signature`/
/// `verify_tls13_signature` (which independently re-parse the leaf via
/// `EndEntityCert::try_from` during the handshake's signature check). So neither chain
/// validation nor the handshake-signature check can be delegated to anything in
/// `rustls-webpki` for a real Bambu cert — confirmed as a known limitation other real-world
/// self-signed device certs have hit too: rustls/rustls#1298 (the identical
/// "UnsupportedCertVersion" error, hit by an unrelated user), rustls/webpki#205 ("Support
/// self-signed certificate"), and rustls/rustls#772, where the LND project hit the exact same
/// wall with its own self-signed device cert — their `SingleCertVerifier` pattern (comparing
/// the peer cert against a pinned expected cert, bypassing webpki's chain logic entirely) is
/// the community-blessed approach this verifier adapts, using signed-by-root trust instead of
/// exact-leaf pinning so it survives individual device cert rotation.
///
/// This verifier uses `x509-parser` instead (a general ASN.1/X.509 parser, not a
/// policy-enforcing validator — confirmed via its own test suite that it treats the version
/// field as optional, defaulting to v1 when absent, exactly per the DER grammar) for all
/// parsing, and does two independent things no other code in this crate does:
/// - **Chain-of-trust**: walks from the leaf through the presented intermediates (this
///   used to check the leaf directly against the trusted roots only, silently ignoring
///   `intermediates` — a legitimate two-level custom CA (offline root + issuing intermediate)
///   failed with `UnknownIssuer` even though the chain was valid) until it either lands on a
///   caller-supplied trusted root's public key or runs out of intermediates, verifying each
///   issuer/subject match and signature link along the way, with an unexpired validity period
///   on the leaf. (`verify_server_cert`, via `X509Certificate::verify_signature` — real
///   `ring`-backed verification, not hand-rolled crypto.)
/// - **Handshake-signature check**: does the live TLS handshake signature verify under the
///   leaf's own public key? (`verify_tls12_signature`/`verify_tls13_signature`, via
///   `rustls_pki_types::SignatureVerificationAlgorithm::verify_signature` directly — this is
///   the check that actually proves the peer holds the private key matching the presented
///   cert; per the LND issue's own reasoning, this is what prevents MITM here, not the chain
///   check alone.)
///
/// Identity (SAN-then-CN, mirroring mbedtls's `x509_crt_verify_name` algorithm) is still
/// checked last, same logic as before — only its data source changed, from a hand-rolled DER
/// walker to `x509-parser`'s parsed fields.
pub struct CnFallbackServerVerifier {
    trusted_roots: Vec<CertificateDer<'static>>,
    algs_mapping: &'static [(
        SignatureScheme,
        &'static [&'static dyn SignatureVerificationAlgorithm],
    )],
}

impl CnFallbackServerVerifier {
    /// Builds the verifier from a set of trusted root certs. Fails if `ca_certs` is empty or
    /// any supplied cert fails to parse — there is nothing to validate a chain against
    /// otherwise, so failing fast at config-build time (rather than silently succeeding and
    /// only failing later at handshake time) is deliberate.
    pub fn new(
        ca_certs: impl IntoIterator<Item = CertificateDer<'static>>,
    ) -> Result<Self, RustlsError> {
        let trusted_roots: Vec<_> = ca_certs.into_iter().collect();
        if trusted_roots.is_empty() {
            return Err(RustlsError::General(
                "CnFallbackServerVerifier requires at least one trusted root cert".into(),
            ));
        }
        for root in &trusted_roots {
            x509_parser::certificate::X509Certificate::from_der(root.as_ref()).map_err(|e| {
                RustlsError::General(format!("failed to parse trusted root cert: {e}"))
            })?;
        }

        let provider = rustls::crypto::ring::default_provider();
        Ok(Self {
            trusted_roots,
            algs_mapping: provider.signature_verification_algorithms.mapping,
        })
    }
}

impl core::fmt::Debug for CnFallbackServerVerifier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CnFallbackServerVerifier").finish()
    }
}

impl ServerCertVerifier for CnFallbackServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        use x509_parser::certificate::X509Certificate;
        use x509_parser::time::ASN1Time;

        let (_, leaf) = X509Certificate::from_der(end_entity.as_ref())
            .map_err(|_| RustlsError::InvalidCertificate(CertificateError::BadEncoding))?;

        let now_asn1 = ASN1Time::from_timestamp(now.as_secs() as i64)
            .map_err(|_| RustlsError::InvalidCertificate(CertificateError::BadEncoding))?;
        if !leaf.validity().is_valid_at(now_asn1) {
            let err = if now_asn1.timestamp() < leaf.validity().not_before.timestamp() {
                CertificateError::NotValidYet
            } else {
                CertificateError::Expired
            };
            return Err(RustlsError::InvalidCertificate(err));
        }

        // Walk from the leaf through `intermediates` (parsed once up front, then
        // consumed as they're matched — each intermediate is usable at most once, so a cyclic
        // issuer/subject arrangement can't stall the loop) until landing on a trusted root or
        // running out of intermediates. Every hop must both match issuer/subject *and* verify
        // the signature — a name match alone proves nothing. Previously this only checked the
        // leaf directly against the trusted roots, so a legitimate multi-level custom CA
        // (offline root + issuing intermediate) always failed with `UnknownIssuer`.
        let parsed_intermediates: Vec<X509Certificate> = intermediates
            .iter()
            .filter_map(|der| X509Certificate::from_der(der.as_ref()).ok().map(|(_, c)| c))
            .collect();
        let mut used = vec![false; parsed_intermediates.len()];

        log::debug!(
            "Verifying TLS chain: leaf subject='{}' issuer='{}', {} intermediate(s) presented, \
             {} trust anchor(s) configured",
            leaf.subject(),
            leaf.issuer(),
            parsed_intermediates.len(),
            self.trusted_roots.len()
        );

        let mut current = &leaf;
        let mut chain_trusted = false;
        // The loop index doubles as the number of intermediates already traversed below the cert
        // about to be adopted — one is adopted per iteration that doesn't break — which is what
        // the `pathLenConstraint` check needs.
        for intermediates_below in 0..=parsed_intermediates.len() {
            // `find_map` rather than `any` so the matching anchor's subject can be logged:
            // with several anchors supplied at once (a root plus its device-CA intermediates,
            // say) "the handshake succeeded" alone doesn't say *which* one the chain landed on,
            // which is the whole question when auditing a printer's trust path.
            let matched_root = self.trusted_roots.iter().find_map(|root_der| {
                let (_, root) = X509Certificate::from_der(root_der.as_ref()).ok()?;
                (current.issuer().as_raw() == root.subject().as_raw()
                    && current.verify_signature(Some(root.public_key())).is_ok()
                    && root.validity().is_valid_at(now_asn1))
                .then(|| root.subject().to_string())
            });
            if let Some(root_subject) = matched_root {
                log::debug!(
                    "TLS chain anchored: '{}' verified against trusted anchor '{root_subject}' \
                     after {intermediates_below} intermediate(s)",
                    current.subject()
                );
                chain_trusted = true;
                break;
            }

            // Try every unused intermediate matching the issuer subject, not just the
            // first by position — a duplicate-subject-name chain (e.g. a rotated intermediate
            // reusing its predecessor's subject) could have the wrong one land first, fail
            // signature verification, and abort the whole walk even though a later same-subject
            // candidate would verify. Fail-closed only: this could spuriously reject a legitimate
            // chain, never accept a bad one, since every candidate is still signature-checked.
            // Validity is part of the *selection* predicate, not a post-selection check, for the
            // same reason signature verification is: a rotated intermediate pair (old expired +
            // new valid, same subject) presented old-first would otherwise select the expired
            // one and abort the walk, never trying its valid sibling. Fail-closed either way,
            // but the post-selection-only form rejected chains that are genuinely good.
            let next_idx = parsed_intermediates
                .iter()
                .enumerate()
                .find(|(i, c)| {
                    !used[*i]
                        && current.issuer().as_raw() == c.subject().as_raw()
                        && current.verify_signature(Some(c.public_key())).is_ok()
                        && c.validity().is_valid_at(now_asn1)
                })
                .map(|(i, _)| i);
            let Some(idx) = next_idx else { break };
            used[idx] = true;
            current = &parsed_intermediates[idx];
            log::debug!(
                "TLS chain hop: adopted presented intermediate '{}' as issuer",
                current.subject()
            );
            // A cert only gets to *act* as a CA if it says it is one. Without this, a chain hop
            // was accepted on subject/issuer name equality plus signature alone, so anyone
            // holding an ordinary leaf issued by the trusted CA could mint a sub-cert with
            // `CN=<other printer serial>`, present their own leaf as an intermediate, and have
            // the walk mark the chain trusted (CVE-2002-0862 class). Only certs used as issuers
            // are checked — the leaf itself is exempt, since real Bambu v1 leaf certs carry no
            // extensions at all.
            check_ca_capable(current, intermediates_below as u32)?;
            if !current.validity().is_valid_at(now_asn1) {
                let err = if now_asn1.timestamp() < current.validity().not_before.timestamp() {
                    CertificateError::NotValidYet
                } else {
                    CertificateError::Expired
                };
                return Err(RustlsError::InvalidCertificate(err));
            }
        }
        if !chain_trusted {
            return Err(RustlsError::InvalidCertificate(
                CertificateError::UnknownIssuer,
            ));
        }

        verify_name_matches_leaf_cert(&leaf, server_name)?;

        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_handshake_signature(cert, message, dss, self.algs_mapping, true)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        if !scheme_supported_in_tls13(dss.scheme) {
            return Err(RustlsError::PeerMisbehaved(
                rustls::PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme,
            ));
        }
        verify_handshake_signature(cert, message, dss, self.algs_mapping, false)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs_mapping
            .iter()
            .map(|(scheme, _)| *scheme)
            .collect()
    }
}

/// Rejects a peer-supplied cert that is being used as an issuer but isn't allowed to be one.
///
/// Requires `basicConstraints` with `ca == true`, `keyUsage.keyCertSign` when a `keyUsage`
/// extension is present at all (absent `keyUsage` means unrestricted, per RFC 5280 §4.2.1.3),
/// and a `pathLenConstraint` at least as large as the number of intermediates already traversed
/// beneath this one. `intermediates_below` counts intermediates only, not the leaf, matching RFC
/// 5280 §4.2.1.9's definition.
///
/// Applies to peer-supplied intermediates only. Trusted roots are anchors the caller chose, and
/// the leaf is never used as an issuer.
fn check_ca_capable(
    cert: &x509_parser::certificate::X509Certificate<'_>,
    intermediates_below: u32,
) -> Result<(), RustlsError> {
    // `UnknownIssuer` rather than a bespoke error: from the caller's point of view a cert that
    // may not sign certs is not a usable issuer, which is exactly what that variant means.
    let reject = |reason: &str| {
        log::warn!("Rejecting TLS chain: intermediate is not a usable CA ({reason})");
        RustlsError::InvalidCertificate(CertificateError::UnknownIssuer)
    };

    let Ok(Some(bc)) = cert.basic_constraints() else {
        return Err(reject("no basicConstraints extension"));
    };
    if !bc.value.ca {
        return Err(reject("basicConstraints CA is false"));
    }
    if let Some(path_len) = bc.value.path_len_constraint
        && intermediates_below > path_len
    {
        return Err(reject("pathLenConstraint exceeded"));
    }
    if let Ok(Some(ku)) = cert.key_usage()
        && !ku.value.key_cert_sign()
    {
        return Err(reject("keyUsage lacks keyCertSign"));
    }
    Ok(())
}

/// Returns true if `scheme` is legal for a TLS 1.3 CertificateVerify, per RFC 8446 §4.2.3.
///
/// Reimplements rustls's own `SignatureScheme::supported_in_tls13()`, which is crate-private and
/// therefore unreachable from here. The `rustls-webpki` free functions this verifier replaces
/// open with that gate (`rustls/src/webpki/verify.rs:194-196`); omitting it let a peer sign the
/// CertificateVerify with `RSA_PKCS1_SHA1` (or any other PKCS#1/SHA-1 scheme) and be accepted in
/// a TLS 1.3 handshake. `supported_verify_schemes` advertises the full ring mapping including
/// PKCS#1 — legal for TLS 1.2, which is what makes it reachable — so the gate has to live on the
/// 1.3 side rather than in the advertised list.
///
/// Denylist, not an allowlist, matching rustls: a scheme allocated after this was written is
/// permitted in TLS 1.3 by default rather than silently breaking a future handshake.
fn scheme_supported_in_tls13(scheme: SignatureScheme) -> bool {
    !matches!(
        scheme,
        // SHA-1 hashes ("Legacy algorithms" in §4.2.3).
        SignatureScheme::RSA_PKCS1_SHA1
            | SignatureScheme::ECDSA_SHA1_Legacy
            // RSASSA-PKCS1-v1_5 in any hash — TLS 1.3 requires RSA-PSS.
            | SignatureScheme::RSA_PKCS1_SHA256
            | SignatureScheme::RSA_PKCS1_SHA384
            | SignatureScheme::RSA_PKCS1_SHA512
    )
}

/// Verifies a live TLS handshake signature against the leaf cert's own public key, extracted
/// via `x509-parser` (bypassing `rustls-webpki`'s `EndEntityCert`, which would reject a v1
/// cert again here just as it does during chain validation). `try_all`: TLS 1.2 tries every
/// candidate algorithm mapped to `dss.scheme` in turn (multiple `SignatureVerificationAlgorithm`s
/// can share one `SignatureScheme`); TLS 1.3 only tries the first, matching the documented
/// behavior of the `rustls-webpki` free functions of the same name this replaces.
fn verify_handshake_signature(
    cert: &CertificateDer<'_>,
    message: &[u8],
    dss: &DigitallySignedStruct,
    algs_mapping: &'static [(
        SignatureScheme,
        &'static [&'static dyn SignatureVerificationAlgorithm],
    )],
    try_all: bool,
) -> Result<HandshakeSignatureValid, RustlsError> {
    let (_, leaf) = x509_parser::certificate::X509Certificate::from_der(cert.as_ref())
        .map_err(|_| RustlsError::InvalidCertificate(CertificateError::BadEncoding))?;
    let public_key = &leaf.public_key().subject_public_key.data;

    let candidates = algs_mapping
        .iter()
        .find(|(scheme, _)| *scheme == dss.scheme)
        .map(|(_, algs)| *algs)
        .ok_or(RustlsError::PeerIncompatible(
            rustls::PeerIncompatible::SignatureAlgorithmsExtensionRequired,
        ))?;

    let candidates = if try_all {
        candidates
    } else {
        candidates.get(..1).unwrap_or(candidates)
    };

    for alg in candidates {
        if alg
            .verify_signature(public_key, message, dss.signature())
            .is_ok()
        {
            return Ok(HandshakeSignatureValid::assertion());
        }
    }
    Err(RustlsError::InvalidCertificate(
        CertificateError::BadSignature,
    ))
}

/// Verifies a parsed leaf cert's identity against `server_name`: SAN first if present, else
/// Subject CN — mirroring mbedtls's `x509_crt_verify_name` algorithm. Only
/// `ServerName::DnsName` is supported (Bambu serials are never IP addresses).
fn verify_name_matches_leaf_cert(
    leaf: &x509_parser::certificate::X509Certificate<'_>,
    server_name: &ServerName<'_>,
) -> Result<(), RustlsError> {
    let expected = match server_name {
        ServerName::DnsName(dns) => dns.as_ref(),
        _ => return Err(RustlsError::UnsupportedNameType),
    };

    // `subject_alternative_name()` returns `Result<Option<_>>`, and x509-parser reports `Err`
    // for a duplicate or malformed SAN extension. Collapsing that to `None` with `.ok()` made a
    // broken SAN indistinguishable from "no SAN present" and fell through to CN matching — the
    // inverse of the documented SAN-then-CN precedence, so a cert carrying a real SAN for
    // another printer plus a malformed second SAN and `CN=<target serial>` would match on CN.
    // A SAN that is present but unparseable is a bad cert, not an absent extension.
    let san = match leaf.subject_alternative_name() {
        Ok(ext) => ext.map(|ext| &ext.value.general_names),
        Err(e) => {
            log::debug!("leaf certificate has an unparseable SAN extension: {:?}", e);
            return Err(RustlsError::InvalidCertificate(
                CertificateError::BadEncoding,
            ));
        }
    };

    if let Some(general_names) = san {
        // RFC 6125 §6.4.4 and mbedtls's `x509_crt_verify_name`: any *present* SAN is a hard
        // match-or-fail against its dNSName entries, whether it has none (only iPAddress/
        // rfc822Name/URI) or several that don't match — CN fallback applies only to an absent
        // SAN, so both cases return the same error rather than falling through to CN.
        let mut dns_names = general_names.iter().filter_map(|gn| match gn {
            x509_parser::extensions::GeneralName::DNSName(name) => Some(*name),
            _ => None,
        });
        return if dns_names.any(|n| n.eq_ignore_ascii_case(expected)) {
            Ok(())
        } else {
            Err(RustlsError::InvalidCertificate(
                CertificateError::NotValidForName,
            ))
        };
    }

    match_subject_cn(leaf, expected)
}

fn match_subject_cn(
    leaf: &x509_parser::certificate::X509Certificate<'_>,
    expected: &str,
) -> Result<(), RustlsError> {
    let matches = leaf
        .subject()
        .iter_common_name()
        .filter_map(|cn| cn.as_str().ok())
        .any(|cn| cn.eq_ignore_ascii_case(expected));

    if matches {
        Ok(())
    } else {
        Err(RustlsError::InvalidCertificate(
            CertificateError::NotValidForName,
        ))
    }
}
