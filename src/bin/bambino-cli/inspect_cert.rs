#![cfg(feature = "cli")]

//! Diagnostic-only command that captures the raw TLS certificate chain a
//! printer presents so it can be inspected for a Subject Alternative Name
//! (SAN) — every cert sample checked into `certs/` is a CA/intermediate
//! cert, never an actual per-printer leaf cert, so this is the only way to
//! confirm what a real printer sends. See `.claude/rules/tls-identity-sni.md`
//! for the resulting identity-verification invariant. Not part of the
//! library's connection path; does not touch `NoCertificateVerification` or
//! any `PrinterClient` code.
//!
//! The **whole** chain is captured, not just the leaf, because whether a printer sends its
//! issuing CA in the handshake decides what certificate pinning a consumer can build on
//! [`TlsConnector::peer_chain_der`](bambino::io::TlsConnector::peer_chain_der): if the issuer
//! is present it can be captured once and passed back through `with_certs(..)` for genuine
//! stack-enforced verification on every later connection; if only the leaf is sent, a consumer
//! is stuck comparing a leaf fingerprint with verification disabled. That question is
//! unanswerable without a printer on the LAN, which is exactly what this command has.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::error::CliError;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls;

/// Certificate verifier used only by `inspect-cert`: accepts any certificate (identical
/// unconditional-trust behavior to `bambino::io::tokio::NoCertificateVerification`) but also
/// stashes the peer's raw DER certificate chain so `run` can write it to disk after the
/// handshake completes.
///
/// Stores the chain leaf-first, matching the order `TlsConnector::peer_chain_der` returns, so
/// what this command writes out is what a consumer of that accessor would see.
#[derive(Debug, Default)]
struct CapturingVerifier {
    captured: Mutex<Option<Vec<Vec<u8>>>>,
}

impl ServerCertVerifier for CapturingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        let chain = core::iter::once(end_entity)
            .chain(intermediates)
            .map(|cert| cert.as_ref().to_vec())
            .collect();
        *self.captured.lock().expect("verifier mutex poisoned") = Some(chain);
        Ok(ServerCertVerified::assertion())
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

/// Builds the on-disk path for chain position `index`, counting the leaf as 0.
///
/// The leaf keeps `output` verbatim so an existing invocation writes exactly the file it always
/// did; each additional certificate gets `.chain<N>` spliced in before the extension
/// (`printer_leaf_cert.der` → `printer_leaf_cert.chain1.der`), which keeps the `.der` suffix
/// intact so `openssl -inform DER` and shell globs still work on the whole set.
fn chain_member_path(output: &str, index: usize) -> PathBuf {
    if index == 0 {
        return PathBuf::from(output);
    }

    let path = Path::new(output);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let name = match path.extension() {
        Some(ext) => format!("{stem}.chain{index}.{}", ext.to_string_lossy()),
        None => format!("{stem}.chain{index}"),
    };

    path.with_file_name(name)
}

/// Connects to `ip:port`, completes a TLS handshake sending `serial` as the SNI value
/// (matching what a real fix would send), and writes every certificate the printer presented
/// as raw DER — the leaf to `output`, any further chain members to the `.chain<N>` paths
/// `chain_member_path` derives. No FTPS/MQTT protocol traffic is exchanged beyond the
/// handshake itself — the connection is dropped as soon as it completes.
pub async fn run(ip: &str, serial: &str, port: u16, output: &str) -> Result<(), CliError> {
    let addr = format!("{ip}:{port}");
    let stream = ::tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|e| CliError::Network(format!("TCP connect to {addr} failed: {e}")))?;

    let verifier = Arc::new(CapturingVerifier::default());
    let provider = rustls::crypto::ring::default_provider();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(rustls::DEFAULT_VERSIONS)
        .expect("protocol versions must initialize")
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));

    let server_name = ServerName::try_from(serial.to_string())
        .map_err(|_| CliError::InvalidArgs(format!("invalid serial for SNI: '{serial}'")))?;

    let tls_stream = connector
        .connect(server_name, stream)
        .await
        .map_err(|e| CliError::Network(format!("TLS handshake with {addr} failed: {e}")))?;
    drop(tls_stream);

    let chain = verifier
        .captured
        .lock()
        .expect("verifier mutex poisoned")
        .take()
        .expect("handshake succeeded but no certificate was captured");

    println!(
        "Printer presented {} certificate(s) in its handshake chain.",
        chain.len()
    );

    for (index, der) in chain.iter().enumerate() {
        let path = chain_member_path(output, index);
        std::fs::write(&path, der)?;

        let role = if index == 0 { "leaf" } else { "issuer/CA" };
        let display = path.display();
        println!("  [{index}] {role}, {} bytes → {display}", der.len());
        println!(
            "      openssl x509 -in {display} -inform DER -noout -text -ext subjectAltName"
        );
    }

    if chain.len() == 1 {
        println!(
            "Only the leaf was sent — a consumer pinning this printer cannot capture an anchor \
             from the handshake and must compare the leaf itself (see issue #142)."
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::chain_member_path;

    #[test]
    fn leaf_keeps_the_output_path_verbatim() {
        assert_eq!(
            chain_member_path("printer_leaf_cert.der", 0).to_string_lossy(),
            "printer_leaf_cert.der"
        );
    }

    #[test]
    fn chain_members_splice_the_index_before_the_extension() {
        assert_eq!(
            chain_member_path("printer_leaf_cert.der", 1).to_string_lossy(),
            "printer_leaf_cert.chain1.der"
        );
        assert_eq!(
            chain_member_path("/tmp/out/cert.der", 2).to_string_lossy(),
            "/tmp/out/cert.chain2.der"
        );
    }

    #[test]
    fn extensionless_output_appends_the_index() {
        assert_eq!(
            chain_member_path("cert", 1).to_string_lossy(),
            "cert.chain1"
        );
    }

    /// A dotfile is all stem and no extension to `Path`, so the suffix must not be mistaken for
    /// one and duplicated — `.cert` has to stay a single leading dot.
    #[test]
    fn dotfile_output_is_not_treated_as_an_extension() {
        assert_eq!(
            chain_member_path(".cert", 1).to_string_lossy(),
            ".cert.chain1"
        );
    }
}
