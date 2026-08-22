#![cfg(feature = "cli")]

//! Diagnostic-only command that attempts a real CA-verified TLS handshake against a printer
//! using `bambino::io::tokio::build_verified_client_config`
//! (and therefore `CnFallbackServerVerifier`), sending `serial` as the SNI
//! value. Reports success or the exact `rustls`/`Error` failure — this
//! is the only way to confirm the verifier's SAN-then-CN logic (see
//! `.claude/rules/tls-identity-sni.md`) behaves correctly against a real
//! printer's handshake, not just against rcgen fixtures.

use bambino::io::tokio::build_verified_client_config;
use rustls_pki_types::ServerName;

use crate::error::CliError;
use crate::trust::trusted_roots;

/// Connects to `ip:port` using the global `--with-certs` trust anchors and attempts a verified
/// TLS handshake sending `serial` as the SNI value. No FTPS/MQTT protocol traffic is exchanged
/// beyond the handshake itself.
///
/// Anchors come from the same loader every other subcommand uses, so a directory of certs and a
/// multi-cert PEM bundle both work here — the previous `--ca-cert <FILE>` form took one path and
/// silently kept only its *first* certificate, which quietly discarded 4 of the 5 anchors in a
/// bundle and made a failure look like the printer's fault rather than the loader's.
pub async fn run(ip: &str, serial: &str, port: u16) -> Result<(), CliError> {
    let Some(anchors) = trusted_roots() else {
        return Err(CliError::InvalidArgs(
            "verify-tls needs trust anchors: pass --with-certs <PATH> (a cert file or a directory \
             of them)"
                .into(),
        ));
    };

    let config = build_verified_client_config(anchors.to_vec(), None)
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
