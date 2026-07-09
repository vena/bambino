#![cfg(feature = "cli")]

//! Diagnostic-only command supporting `TLS_SNI_HOSTNAME_MISMATCH_PLAN.md`'s final validation
//! step: attempts a real CA-verified TLS handshake against a printer using
//! `bambino::io::tokio::build_verified_client_config` (and therefore the new
//! `CnFallbackServerVerifier`), sending `serial` as the SNI value. Reports success or the exact
//! `rustls`/`BambuError` failure — this is the only way to confirm the verifier's SAN-then-CN
//! logic behaves correctly against a real printer's handshake, not just against rcgen fixtures.

use bambino::error::BambuError;
use bambino::io::tokio::build_verified_client_config;
use rustls_pki_types::{CertificateDer, ServerName, pem::PemObject};

/// Connects to `ip:port`, loads `ca_cert_path` as the sole trust anchor, and attempts a
/// verified TLS handshake sending `serial` as the SNI value. No FTPS/MQTT protocol traffic is
/// exchanged beyond the handshake itself.
pub async fn run(ip: &str, serial: &str, port: u16, ca_cert_path: &str) -> Result<(), BambuError> {
    let ca_cert = CertificateDer::from_pem_file(ca_cert_path).map_err(|e| {
        BambuError::ProtocolViolation(format!("failed to load {ca_cert_path}: {e}").into())
    })?;

    let config = build_verified_client_config(vec![ca_cert], None).map_err(|e| {
        BambuError::ProtocolViolation(format!("failed to build verified TLS config: {e}").into())
    })?;
    let connector = tokio_rustls::TlsConnector::from(config);

    let addr = format!("{ip}:{port}");
    let stream = ::tokio::net::TcpStream::connect(&addr).await.map_err(|e| {
        BambuError::ProtocolViolation(format!("TCP connect to {addr} failed: {e}").into())
    })?;

    let server_name = ServerName::try_from(serial.to_string()).map_err(|_| {
        BambuError::ProtocolViolation(format!("invalid serial for SNI: '{serial}'").into())
    })?;

    match connector.connect(server_name, stream).await {
        Ok(_) => {
            println!(
                "Verified TLS handshake with {addr} (SNI={serial}) succeeded — \
                 CnFallbackServerVerifier accepted the printer's cert."
            );
            Ok(())
        }
        Err(e) => Err(BambuError::ProtocolViolation(
            format!("Verified TLS handshake with {addr} (SNI={serial}) FAILED: {e}").into(),
        )),
    }
}
