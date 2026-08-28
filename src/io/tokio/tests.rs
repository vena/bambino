use super::*;
use rustls::client::danger::ServerCertVerifier;
use rustls_pki_types::UnixTime;
use x509_parser::prelude::FromDer;

#[test]
fn test_build_unsafe_client_config() {
    let config = build_unsafe_client_config();
    assert!(config.alpn_protocols.is_empty());
}

#[test]
fn test_build_verified_client_config_empty_roots_fails_fast() {
    // An empty root store can never validate a chain — CnFallbackServerVerifier::new fails
    // immediately (NoRootAnchors) rather than deferring to a confusing handshake-time error.
    let config = build_verified_client_config(std::iter::empty(), None);
    assert!(
        config.is_err(),
        "empty root store should fail fast at config-build time"
    );
}

#[test]
fn test_build_verified_client_config_with_options_tls12() {
    let (ca_der, ..) = test_support::generate_test_ca();
    let config = build_verified_client_config_with_options([ca_der], None, true);
    assert!(config.is_ok());
}

/// Shared fixtures for `CnFallbackServerVerifier` tests: real DER-encoded certs rather than
/// hand-crafted byte arrays, so the parser is exercised against genuine X.509 encoding.
mod test_support {
    use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, SigningKey};

    use super::*;

    /// Returns the CA cert, an `Issuer` for signing v3 test leaves, and the CA key's own
    /// DER + algorithm — the latter two let `strip_to_v1` reconstruct an independent
    /// `KeyPair` instance for the same key (needed since `Issuer::new` consumes its key).
    pub(super) fn generate_test_ca() -> (
        CertificateDer<'static>,
        Issuer<'static, KeyPair>,
        Vec<u8>,
        &'static rcgen::SignatureAlgorithm,
    ) {
        let mut params = CertificateParams::new(Vec::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(DnType::CommonName, "bambino test CA");
        let key = KeyPair::generate().unwrap();
        let key_der = key.serialize_der();
        let algo = key.algorithm();
        let cert = params.clone().self_signed(&key).unwrap();
        let der = cert.der().clone();
        (der, Issuer::new(params, key), key_der, algo)
    }

    /// Builds an intermediate CA cert signed by `parent_issuer`, plus an `Issuer` for
    /// signing leaves under it — used by the two-level chain regression test.
    pub(super) fn generate_test_intermediate_ca(
        parent_issuer: &Issuer<'_, KeyPair>,
        common_name: &str,
    ) -> (CertificateDer<'static>, Issuer<'static, KeyPair>) {
        let mut params = CertificateParams::new(Vec::new()).unwrap();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        let key = KeyPair::generate().unwrap();
        let cert = params.clone().signed_by(&key, parent_issuer).unwrap();
        let der = cert.der().clone();
        (der, Issuer::new(params, key))
    }

    /// Builds a v3 leaf cert signed by `issuer`. `common_name` is always set; `san_dns_names`
    /// controls whether a SAN extension is present at all.
    pub(super) fn generate_test_leaf(
        issuer: &Issuer<'_, KeyPair>,
        common_name: &str,
        san_dns_names: Vec<String>,
    ) -> CertificateDer<'static> {
        let mut params = CertificateParams::new(san_dns_names).unwrap();
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        let key = KeyPair::generate().unwrap();
        params.signed_by(&key, issuer).unwrap().der().clone()
    }

    /// Builds a v3 leaf cert signed by `issuer` with an explicit (possibly expired)
    /// validity window, for the expiry-rejection test.
    pub(super) fn generate_test_leaf_with_validity(
        issuer: &Issuer<'_, KeyPair>,
        common_name: &str,
        not_before: time::OffsetDateTime,
        not_after: time::OffsetDateTime,
    ) -> CertificateDer<'static> {
        let mut params = CertificateParams::new(Vec::new()).unwrap();
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        params.not_before = not_before;
        params.not_after = not_after;
        let key = KeyPair::generate().unwrap();
        params.signed_by(&key, issuer).unwrap().der().clone()
    }

    /// Minimal DER (tag, content, rest) reader — test-fixture scaffolding only, deliberately
    /// kept separate from any production parsing code.
    fn read_tlv(input: &[u8]) -> (u8, &[u8], &[u8]) {
        let tag = input[0];
        let len_byte = input[1] as usize;
        let (len, header_len) = if len_byte & 0x80 == 0 {
            (len_byte, 2)
        } else {
            let n = len_byte & 0x7F;
            let mut len = 0usize;
            for b in &input[2..2 + n] {
                len = (len << 8) | *b as usize;
            }
            (len, 2 + n)
        };
        (
            tag,
            &input[header_len..header_len + len],
            &input[header_len + len..],
        )
    }

    fn write_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        let len = content.len();
        if len < 0x80 {
            out.push(len as u8);
        } else {
            let bytes = len.to_be_bytes();
            let first_nonzero = bytes
                .iter()
                .position(|&b| b != 0)
                .unwrap_or(bytes.len() - 1);
            let significant = &bytes[first_nonzero..];
            out.push(0x80 | significant.len() as u8);
            out.extend_from_slice(significant);
        }
        out.extend_from_slice(content);
        out
    }

    /// Strips an rcgen-generated v3 leaf's `[0]` version tag and `[3]` extensions block,
    /// then re-signs the resulting v1-shaped TBSCertificate under `issuer_key` and
    /// re-wraps it — producing a cert with the exact shape real Bambu printer certs have
    /// (confirmed against a live P1S): no version tag (implicit v1), no extensions at all.
    /// This is the fixture that actually proves the verifier handles real printer certs —
    /// the rcgen-default v3 fixtures above do not, and did not catch this session's first,
    /// broken attempt at this verifier.
    pub(super) fn strip_to_v1(
        v3_der: &CertificateDer<'static>,
        issuer_key: &KeyPair,
    ) -> CertificateDer<'static> {
        let (cert_tag, cert_content, _) = read_tlv(v3_der.as_ref());
        assert_eq!(cert_tag, 0x30, "expected outer Certificate SEQUENCE");
        let (tbs_tag, tbs_content, after_tbs) = read_tlv(cert_content);
        assert_eq!(tbs_tag, 0x30, "expected TBSCertificate SEQUENCE");
        let (sig_alg_tag, sig_alg_content, _) = read_tlv(after_tbs);
        assert_eq!(sig_alg_tag, 0x30, "expected signatureAlgorithm SEQUENCE");

        let mut remaining = tbs_content;
        let mut kept = Vec::new();
        while !remaining.is_empty() {
            let (tag, content, rest) = read_tlv(remaining);
            // Drop [0] EXPLICIT version and [3] EXPLICIT extensions — everything else
            // (serialNumber, signature AlgorithmIdentifier, issuer, validity, subject,
            // subjectPublicKeyInfo) is kept verbatim from the real, correctly-encoded v3
            // cert.
            if tag != 0xA0 && tag != 0xA3 {
                kept.extend_from_slice(&write_tlv(tag, content));
            }
            remaining = rest;
        }

        let new_tbs = write_tlv(0x30, &kept);
        let signature = issuer_key
            .sign(&new_tbs)
            .expect("test fixture signing must succeed");
        let mut bit_string_content = vec![0x00]; // BIT STRING "unused bits" prefix
        bit_string_content.extend_from_slice(&signature);

        let mut cert_body = Vec::new();
        cert_body.extend_from_slice(&new_tbs);
        cert_body.extend_from_slice(&write_tlv(0x30, sig_alg_content));
        cert_body.extend_from_slice(&write_tlv(0x03, &bit_string_content));

        CertificateDer::from(write_tlv(0x30, &cert_body))
    }
}

#[test]
fn test_cn_fallback_verifier_accepts_cn_match_when_no_san() {
    let (ca_der, issuer, ..) = test_support::generate_test_ca();
    let leaf = test_support::generate_test_leaf(&issuer, "TESTSERIAL0001", Vec::new());
    let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
    let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

    let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
    assert!(result.is_ok(), "expected CN fallback match, got {result:?}");
}

#[test]
fn test_cn_fallback_verifier_rejects_cn_mismatch_when_no_san() {
    let (ca_der, issuer, ..) = test_support::generate_test_ca();
    let leaf = test_support::generate_test_leaf(&issuer, "TESTSERIAL0001", Vec::new());
    let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
    let server_name = ServerName::try_from("SOMEOTHERSERIAL").unwrap();

    let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
    assert!(result.is_err(), "CN mismatch must be rejected");
}

#[test]
fn test_cn_fallback_verifier_prefers_san_over_mismatched_cn() {
    let (ca_der, issuer, ..) = test_support::generate_test_ca();
    // CN deliberately wrong; SAN carries the real expected identity. A cert with any SAN
    // extension present must be matched via SAN only, per mbedtls's own algorithm.
    let leaf =
        test_support::generate_test_leaf(&issuer, "WRONG-CN", vec!["TESTSERIAL0001".to_string()]);
    let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
    let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

    let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
    assert!(result.is_ok(), "expected SAN match, got {result:?}");
}

#[test]
fn test_cn_fallback_verifier_rejects_untrusted_chain() {
    let (ca_der, ..) = test_support::generate_test_ca();
    // A second, unrelated CA signs this leaf — it must never validate against the first
    // CA's trusted roots, regardless of whether the name matches.
    let (_other_ca_der, other_issuer, ..) = test_support::generate_test_ca();
    let leaf = test_support::generate_test_leaf(&other_issuer, "TESTSERIAL0001", Vec::new());
    let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
    let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

    let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
    assert!(
        result.is_err(),
        "cert signed by an untrusted CA must be rejected"
    );
}

#[test]
fn test_cn_fallback_verifier_accepts_leaf_via_intermediate() {
    // A leaf signed by an intermediate CA (itself signed by the trusted root, not
    // trusted directly) must validate when the intermediate is presented in `intermediates`
    // — the verifier previously only ever checked the leaf's issuer directly against the
    // trusted roots, so this always failed with UnknownIssuer regardless.
    let (root_der, root_issuer, ..) = test_support::generate_test_ca();
    let (intermediate_der, intermediate_issuer) =
        test_support::generate_test_intermediate_ca(&root_issuer, "bambino test intermediate CA");
    let leaf = test_support::generate_test_leaf(&intermediate_issuer, "TESTSERIAL0001", Vec::new());

    let verifier = CnFallbackServerVerifier::new([root_der]).unwrap();
    let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

    let result = verifier.verify_server_cert(
        &leaf,
        &[intermediate_der],
        &server_name,
        &[],
        UnixTime::now(),
    );
    assert!(
        result.is_ok(),
        "expected a leaf signed by a trusted root's intermediate to validate, got {result:?}"
    );
}

#[test]
fn test_cn_fallback_verifier_rejects_leaf_via_untrusted_intermediate() {
    // Sibling of the acceptance case: an intermediate signed by an *unrelated* CA must still
    // be rejected even when presented as an intermediate — presence in `intermediates` alone
    // must never grant trust, only an unbroken signature chain up to a trusted root does.
    let (root_der, ..) = test_support::generate_test_ca();
    let (_other_root_der, other_root_issuer, ..) = test_support::generate_test_ca();
    let (intermediate_der, intermediate_issuer) = test_support::generate_test_intermediate_ca(
        &other_root_issuer,
        "bambino test rogue intermediate CA",
    );
    let leaf = test_support::generate_test_leaf(&intermediate_issuer, "TESTSERIAL0001", Vec::new());

    let verifier = CnFallbackServerVerifier::new([root_der]).unwrap();
    let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

    let result = verifier.verify_server_cert(
        &leaf,
        &[intermediate_der],
        &server_name,
        &[],
        UnixTime::now(),
    );
    assert!(
        result.is_err(),
        "a leaf chained through an untrusted intermediate must still be rejected"
    );
}

#[test]
fn test_cn_fallback_verifier_skips_wrong_signature_for_duplicate_subject_intermediate() {
    // Two intermediates share the same subject CN, but only the second (by
    // position in `intermediates`) is the one that actually signed the leaf. The old
    // chain-walk picked the first subject-name match, failed its signature check, and
    // `break`s the whole walk — this proves it now tries the next same-subject candidate
    // instead of giving up.
    let (root_der, root_issuer, ..) = test_support::generate_test_ca();
    let (_decoy_root_der, decoy_root_issuer, ..) = test_support::generate_test_ca();
    let (decoy_intermediate_der, _decoy_intermediate_issuer) =
        test_support::generate_test_intermediate_ca(&decoy_root_issuer, "shared-subject-name");
    let (genuine_intermediate_der, genuine_intermediate_issuer) =
        test_support::generate_test_intermediate_ca(&root_issuer, "shared-subject-name");
    let leaf = test_support::generate_test_leaf(
        &genuine_intermediate_issuer,
        "TESTSERIAL0001",
        Vec::new(),
    );

    let verifier = CnFallbackServerVerifier::new([root_der]).unwrap();
    let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

    let result = verifier.verify_server_cert(
        &leaf,
        // Decoy (wrong signer, same subject name) ordered first on purpose.
        &[decoy_intermediate_der, genuine_intermediate_der],
        &server_name,
        &[],
        UnixTime::now(),
    );
    assert!(
        result.is_ok(),
        "expected chain-walk to skip the signature-mismatched same-subject decoy and find \
         the genuine intermediate, got {result:?}"
    );
}

#[test]
fn test_cn_fallback_verifier_rejects_expired_cert() {
    let (ca_der, issuer, ..) = test_support::generate_test_ca();
    let not_before = time::OffsetDateTime::UNIX_EPOCH;
    let not_after = not_before + time::Duration::days(1); // expired long ago
    let leaf = test_support::generate_test_leaf_with_validity(
        &issuer,
        "TESTSERIAL0001",
        not_before,
        not_after,
    );
    let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
    let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

    let result = verifier.verify_server_cert(&leaf, &[], &server_name, &[], UnixTime::now());
    assert!(result.is_err(), "an expired cert must be rejected");
}

/// The test that would have caught this session's first, broken attempt at this verifier:
/// a genuinely v1-shaped cert (no version tag, no extensions — the real Bambu printer cert
/// shape, confirmed against a live P1S), not just rcgen's default v3 output.
#[test]
fn test_cn_fallback_verifier_accepts_real_v1_shaped_cert_with_cn_match() {
    use rcgen::KeyPair;

    let (ca_der, issuer, ca_key_der, ca_algo) = test_support::generate_test_ca();
    let v3_leaf = test_support::generate_test_leaf(&issuer, "TESTSERIAL0001", Vec::new());
    let ca_key_der = PrivateKeyDer::try_from(ca_key_der).unwrap();
    let ca_key_copy = KeyPair::from_der_and_sign_algo(&ca_key_der, ca_algo).unwrap();
    let v1_leaf = test_support::strip_to_v1(&v3_leaf, &ca_key_copy);

    // Confirm the fixture really is v1-shaped before trusting the test result below: a v3
    // cert always has an extensions block (even if empty of entries); a true v1 cert has
    // none of the extensions TLV at all.
    let (_, parsed) = x509_parser::certificate::X509Certificate::from_der(v1_leaf.as_ref())
        .expect("v1 fixture must still be valid DER");
    assert!(
        parsed.extensions().is_empty(),
        "v1 fixture must have no extensions block at all"
    );

    let verifier = CnFallbackServerVerifier::new([ca_der]).unwrap();
    let server_name = ServerName::try_from("TESTSERIAL0001").unwrap();

    let result = verifier.verify_server_cert(&v1_leaf, &[], &server_name, &[], UnixTime::now());
    assert!(
        result.is_ok(),
        "expected a real v1-shaped, SAN-less cert to validate via CN fallback, got {result:?}"
    );
}

#[test]
fn test_build_verified_client_config_bad_key_returns_error() {
    // Exercise this crate's build_verified_client_config, not just
    // rustls_pki_types::PrivateKeyDer::try_from's own parsing (a fact about that crate, not
    // this one). A well-typed-but-garbage PKCS#8 key bypasses try_from's format sniffing so
    // the bogus bytes reach rustls' own key validation inside with_client_auth_cert.
    let (ca_der, ..) = test_support::generate_test_ca();
    let bogus_key = PrivateKeyDer::Pkcs8(rustls_pki_types::PrivatePkcs8KeyDer::from(vec![0u8; 10]));
    let bogus_cert = CertificateDer::from(vec![0u8; 10]);

    let result = build_verified_client_config([ca_der], Some((vec![bogus_cert], bogus_key)));

    assert!(
        result.is_err(),
        "garbage PKCS#8 key bytes must fail client config construction"
    );
}

#[test]
fn test_handshake_error_preserves_rustls_certificate_verdict() {
    // tokio-rustls reports a handshake failure as `io::Error::new(InvalidData, rustls::Error)`
    // (`tokio_rustls::common::Stream::read_io`), so the typed verdict survives only if the
    // mapping downcasts for it — `InvalidData` itself maps to `Other` (GitHub issue #157).
    let unknown_issuer = std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        rustls::Error::InvalidCertificate(rustls::CertificateError::UnknownIssuer),
    );
    assert_eq!(
        map_tls_handshake_error(unknown_issuer),
        SocketError::CertificateInvalid(CertificateFailure::UntrustedAnchor)
    );

    let bad_name = std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        rustls::Error::InvalidCertificate(rustls::CertificateError::NotValidForName),
    );
    assert_eq!(
        map_tls_handshake_error(bad_name),
        SocketError::CertificateInvalid(CertificateFailure::NameMismatch)
    );
}

#[test]
fn test_handshake_error_non_certificate_paths_unchanged() {
    // A rustls error that isn't a certificate rejection, and a plain socket error, both keep
    // the pre-existing mapping rather than being forced into a certificate verdict.
    let protocol_err =
        std::io::Error::new(std::io::ErrorKind::InvalidData, rustls::Error::DecryptError);
    assert!(matches!(
        map_tls_handshake_error(protocol_err),
        SocketError::Other(_)
    ));

    let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
    assert_eq!(
        map_tls_handshake_error(refused),
        SocketError::ConnectionRefused
    );
}

#[test]
#[allow(deprecated)] // Same reason as `map_rustls_certificate_error`'s own allow.
fn test_rustls_certificate_error_mapping_covers_context_twins() {
    // Each `*Context` variant carries extra detail but means the same thing to a caller, so it
    // must not fall through to `Unspecified` alongside its plain twin.
    assert_eq!(
        map_rustls_certificate_error(&rustls::CertificateError::Expired),
        CertificateFailure::Expired
    );
    assert_eq!(
        map_rustls_certificate_error(&rustls::CertificateError::NotValidYet),
        CertificateFailure::NotYetValid
    );
    assert_eq!(
        map_rustls_certificate_error(&rustls::CertificateError::Revoked),
        CertificateFailure::Revoked
    );
    assert_eq!(
        map_rustls_certificate_error(&rustls::CertificateError::BadEncoding),
        CertificateFailure::Malformed
    );
    assert_eq!(
        map_rustls_certificate_error(&rustls::CertificateError::UnsupportedSignatureAlgorithm),
        CertificateFailure::UnsupportedAlgorithm
    );
    assert_eq!(
        map_rustls_certificate_error(&rustls::CertificateError::InvalidPurpose),
        CertificateFailure::InvalidPurpose
    );
    // No portable counterpart — still a rejection, so `Unspecified`, never a softer verdict.
    assert_eq!(
        map_rustls_certificate_error(&rustls::CertificateError::UnhandledCriticalExtension),
        CertificateFailure::Unspecified
    );
}

#[test]
fn test_peer_sent_no_certificates_is_reported_as_missing() {
    // rustls puts `NoCertificatesPresented` at the top level of `Error`, not under
    // `CertificateError`, so it is only reached by the second match arm — without it this
    // lands in `Other` and a caller cannot tell "nothing to capture" from an unrelated
    // transport failure.
    let none_presented = std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        rustls::Error::NoCertificatesPresented,
    );
    assert_eq!(
        map_tls_handshake_error(none_presented),
        SocketError::CertificateInvalid(CertificateFailure::Missing)
    );
}
