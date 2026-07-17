#![cfg(feature = "cli")]

//! Diagnostic-only command that attempts a real CA-verified TLS handshake
//! against a printer using `bambino::io::tokio::build_verified_client_config`
//! (and therefore `CnFallbackServerVerifier`), sending `serial` as the SNI
//! value. Reports success or the exact `rustls`/`Error` failure — this
//! is the only way to confirm the verifier's SAN-then-CN logic (see
//! `.claude/rules/tls-identity-sni.md`) behaves correctly against a real
//! printer's handshake, not just against rcgen fixtures.

use bambino::io::tokio::build_verified_client_config;
use rustls_pki_types::{CertificateDer, ServerName, pem::PemObject};

use crate::error::CliError;

/// Connects to `ip:port`, loads `ca_cert_path` as the sole trust anchor, and attempts a
/// verified TLS handshake sending `serial` as the SNI value. No FTPS/MQTT protocol traffic is
/// exchanged beyond the handshake itself.
pub async fn run(ip: &str, serial: &str, port: u16, ca_cert_path: &str) -> Result<(), CliError> {
    let ca_cert = CertificateDer::from_pem_file(ca_cert_path)
        .map_err(|e| CliError::Other(format!("failed to load {ca_cert_path}: {e}")))?;

    let config = build_verified_client_config(vec![ca_cert], None)
        .map_err(|e| CliError::Other(format!("failed to build verified TLS config: {e}")))?;
    let connector = tokio_rustls::TlsConnector::from(config);

    let addr = format!("{ip}:{port}");
    let stream = ::tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|e| CliError::Network(format!("TCP connect to {addr} failed: {e}")))?;

    let server_name = ServerName::try_from(serial.to_string())
        .map_err(|_| CliError::InvalidArgs(format!("invalid serial for SNI: '{serial}'")))?;

    match connector.connect(server_name, stream).await {
        Ok(_) => {
            println!(
                "Verified TLS handshake with {addr} (SNI={serial}) succeeded — \
                 CnFallbackServerVerifier accepted the printer's cert."
            );
            Ok(())
        }
        Err(e) => Err(CliError::Network(format!(
            "Verified TLS handshake with {addr} (SNI={serial}) FAILED: {e}"
        ))),
    }
}
