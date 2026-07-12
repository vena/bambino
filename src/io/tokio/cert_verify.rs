use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{CertificateError, DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, SignatureVerificationAlgorithm, UnixTime};
use tokio_rustls::rustls;
use x509_parser::prelude::FromDer;

/// Custom certificate verifier that bypasses standard CA chain authority validation.
///
/// **Why this is required:**
/// Physical Bambu Lab printers (all models) host an onboard local MQTTS/FTPS broker
/// utilizing self-signed certificates with the printer's serial number in the CN field.
/// Because these do not trace back to any root authority in OS certificate stores,
/// standard verifiers reject the connections immediately.
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
/// - **Chain-of-trust**: walks from the leaf through the presented intermediates (BUG-008: this
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

        // BUG-008: walk from the leaf through `intermediates` (parsed once up front, then
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

        let mut current = &leaf;
        let mut chain_trusted = false;
        for _ in 0..=parsed_intermediates.len() {
            if self.trusted_roots.iter().any(|root_der| {
                let Ok((_, root)) = X509Certificate::from_der(root_der.as_ref()) else {
                    return false;
                };
                current.issuer().as_raw() == root.subject().as_raw()
                    && current.verify_signature(Some(root.public_key())).is_ok()
            }) {
                chain_trusted = true;
                break;
            }

            // BUG-048: try every unused intermediate matching the issuer subject, not just the
            // first by position — a duplicate-subject-name chain (e.g. a rotated intermediate
            // reusing its predecessor's subject) could have the wrong one land first, fail
            // signature verification, and abort the whole walk even though a later same-subject
            // candidate would verify. Fail-closed only: this could spuriously reject a legitimate
            // chain, never accept a bad one, since every candidate is still signature-checked.
            let next_idx = parsed_intermediates
                .iter()
                .enumerate()
                .find(|(i, c)| {
                    !used[*i]
                        && current.issuer().as_raw() == c.subject().as_raw()
                        && current.verify_signature(Some(c.public_key())).is_ok()
                })
                .map(|(i, _)| i);
            let Some(idx) = next_idx else { break };
            used[idx] = true;
            current = &parsed_intermediates[idx];
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
        verify_handshake_signature(cert, message, dss, self.algs_mapping, false)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs_mapping
            .iter()
            .map(|(scheme, _)| *scheme)
            .collect()
    }
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

    let san = leaf
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|ext| &ext.value.general_names);

    if let Some(general_names) = san {
        let dns_names = general_names.iter().filter_map(|gn| match gn {
            x509_parser::extensions::GeneralName::DNSName(name) => Some(*name),
            _ => None,
        });
        return if dns_names.clone().next().is_some() {
            if dns_names.clone().any(|n| n.eq_ignore_ascii_case(expected)) {
                Ok(())
            } else {
                Err(RustlsError::InvalidCertificate(
                    CertificateError::NotValidForName,
                ))
            }
        } else {
            match_subject_cn(leaf, expected)
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
