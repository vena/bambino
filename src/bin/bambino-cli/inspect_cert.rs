#![cfg(feature = "cli")]

//! Diagnostic-only command that captures the raw leaf TLS certificate a
//! printer presents so it can be inspected for a Subject Alternative Name
//! (SAN) — every cert sample checked into `certs/` is a CA/intermediate
//! cert, never an actual per-printer leaf cert, so this is the only way to
//! confirm what a real printer sends. See `.claude/rules/tls-identity-sni.md`
//! for the resulting identity-verification invariant. Not part of the
//! library's connection path; does not touch `NoCertificateVerification` or
//! any `PrinterClient` code.

use std::sync::{Arc, Mutex};

use bambino::error::Error;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{DigitallySignedStruct, Error as RustlsError, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use tokio_rustls::rustls;

/// Certificate verifier used only by `inspect-cert`: accepts any certificate (identical
/// unconditional-trust behavior to `bambino::io::tokio::NoCertificateVerification`) but also
/// stashes the leaf certificate's raw DER bytes so `run` can write them to disk after the
/// handshake completes.
#[derive(Debug, Default)]
struct CapturingVerifier {
    captured: Mutex<Option<Vec<u8>>>,
}

impl ServerCertVerifier for CapturingVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        *self.captured.lock().expect("verifier mutex poisoned") =
            Some(end_entity.as_ref().to_vec());
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

/// Connects to `ip:port`, completes a TLS handshake sending `serial` as the SNI value
/// (matching what a real fix would send), and writes the leaf certificate's raw DER bytes to
/// `output`. No FTPS/MQTT protocol traffic is exchanged beyond the handshake itself — the
/// connection is dropped as soon as it completes.
pub async fn run(ip: &str, serial: &str, port: u16, output: &str) -> Result<(), Error> {
    let addr = format!("{ip}:{port}");
    let stream = ::tokio::net::TcpStream::connect(&addr).await.map_err(|e| {
        Error::ProtocolViolation(format!("TCP connect to {addr} failed: {e}").into())
    })?;

    let verifier = Arc::new(CapturingVerifier::default());
    let provider = rustls::crypto::ring::default_provider();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_protocol_versions(rustls::DEFAULT_VERSIONS)
        .expect("protocol versions must initialize")
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));

    let server_name = ServerName::try_from(serial.to_string()).map_err(|_| {
        Error::ProtocolViolation(format!("invalid serial for SNI: '{serial}'").into())
    })?;

    let tls_stream = connector.connect(server_name, stream).await.map_err(|e| {
        Error::ProtocolViolation(format!("TLS handshake with {addr} failed: {e}").into())
    })?;
    drop(tls_stream);

    let der = verifier
        .captured
        .lock()
        .expect("verifier mutex poisoned")
        .take()
        .expect("handshake succeeded but no certificate was captured");

    std::fs::write(output, &der).map_err(|e| {
        Error::ProtocolViolation(format!("failed to write {output}: {e}").into())
    })?;

    println!(
        "Captured leaf certificate ({} bytes) written to {output}",
        der.len()
    );
    println!(
        "Inspect with: openssl x509 -in {output} -inform DER -noout -text -ext subjectAltName"
    );

    Ok(())
}
