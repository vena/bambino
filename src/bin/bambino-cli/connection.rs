#![cfg(feature = "cli")]

use bambino::client::PrinterClient;
use bambino::client::dummy::{DummyFactory, DummyRawIo, DummyTls};
use bambino::identity::PrinterIdentity;
use std::sync::Arc;

use bambino::io::tokio::{TokioRawStreamFactory, TokioTimer, TokioTlsConnector};
use bambino::io::{RawStreamFactory, TlsConnector, TokioIo};
use tokio_rustls::rustls::ClientConfig;

use crate::error::CliError;
use crate::trust::build_cli_tls_config;

const CONNECT_TIMEOUT_SECS: u64 = 5;

/// How long a subcommand waits for the printer to answer a request it expects a reply to (a
/// `pushall` snapshot, a `get_version`) before reporting a timeout.
pub(crate) const RESPONSE_TIMEOUT_SECS: u64 = 10;

/// Bounds one connect-phase await (TCP dial or TLS handshake) by `CONNECT_TIMEOUT_SECS`.
///
/// For the diagnostics that dial outside `PrinterClient` and so don't get its connect timeout:
/// a port that accepts TCP but never answers TLS would otherwise hang them indefinitely.
pub(crate) async fn with_connect_timeout<T>(
    what: &str,
    fut: impl Future<Output = Result<T, CliError>>,
) -> Result<T, CliError> {
    ::tokio::time::timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS), fut)
        .await
        .map_err(|_| CliError::Network(format!("{what} timed out after {CONNECT_TIMEOUT_SECS}s")))?
}

/// TLS stream `dial_and_handshake` returns.
pub(crate) type TlsStream =
    <TokioTlsConnector as TlsConnector<TokioIo<::tokio::net::TcpStream>>>::Stream;

/// Dials `ip:port` and completes a TLS handshake under `config`, sending `serial` as the SNI.
///
/// Goes through the library's own `TokioRawStreamFactory` and `TokioTlsConnector`, so a
/// diagnostic exercises the same dial path `PrinterClient` uses. The connector is returned
/// alongside the stream for `peer_chain_der`.
pub(crate) async fn dial_and_handshake(
    ip: &str,
    serial: &str,
    port: u16,
    config: Arc<ClientConfig>,
) -> Result<(TokioTlsConnector, TlsStream), CliError> {
    let raw_stream = with_connect_timeout(&format!("TCP connect to {ip} port {port}"), async {
        TokioRawStreamFactory.dial(ip, port).await.map_err(|e| {
            CliError::Network(format!(
                "TCP connect to {ip} port {port} failed: {}",
                bambino::Error::from(e)
            ))
        })
    })
    .await?;

    let connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));
    let stream = with_connect_timeout(&format!("TLS handshake with {ip} port {port}"), async {
        connector.connect(serial, raw_stream).await.map_err(|e| {
            CliError::Network(format!(
                "TLS handshake with {ip} port {port} (SNI={serial}) failed: {}",
                bambino::Error::from(e)
            ))
        })
    })
    .await?;

    Ok((connector, stream))
}

/// Environment variable consulted as a fallback source for the access code when the positional `access_code` CLI argument is omitted or empty.
/// Lets scripted/CI usage avoid putting the access code in shell history; the positional arg still
/// takes precedence when non-empty.
const ACCESS_CODE_ENV_VAR: &str = "BAMBINO_ACCESS_CODE";

/// Resolves the access code to actually use: the positional CLI argument if non-empty, otherwise the `BAMBINO_ACCESS_CODE` environment variable (empty string if unset), letting `validate_params`'s existing empty-check produce a consistent error either way.
fn resolve_access_code(access_code: &str) -> String {
    if access_code.is_empty() {
        std::env::var(ACCESS_CODE_ENV_VAR).unwrap_or_default()
    } else {
        access_code.to_owned()
    }
}

/// Printer address and credentials, flattened into every printer-facing subcommand.
///
/// No `Debug` derive: it would print the access code.
#[derive(clap::Args)]
pub struct Target {
    pub ip: String,
    pub serial: String,
    /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
    #[arg(default_value = "")]
    access_code: String,
}

impl Target {
    /// Validates the target and builds a not-yet-connected printer client for it.
    pub(crate) fn printer(&self) -> Result<Printer, CliError> {
        create_printer(
            &self.ip,
            &self.serial,
            &resolve_access_code(&self.access_code),
        )
    }

    /// Builds a printer client and connects its MQTT channel, reporting progress on stderr.
    pub(crate) async fn connect_mqtt(&self) -> Result<Printer, CliError> {
        eprintln!(
            "Connecting to {} port {}...",
            self.ip,
            bambino::mqtt::MQTTS_PORT
        );
        let mut client = self.printer()?;
        client.connect_mqtt().await?;
        eprintln!("Connected.");
        Ok(client)
    }
}

pub type Printer = PrinterClient<
    TokioIo<::tokio::net::TcpStream>,
    TokioTlsConnector,
    TokioRawStreamFactory,
    TokioTimer,
    DummyRawIo,
    DummyTls,
    DummyFactory,
>;

/// Seconds since the Unix epoch, for report timestamps.
pub(crate) fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Writes `report` to `path` as pretty-printed JSON and says where on stderr.
pub(crate) fn write_report(path: &str, report: &impl serde::Serialize) -> Result<(), CliError> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|e| CliError::Other(format!("failed to serialize report: {e}")))?;
    std::fs::write(path, json.as_bytes())?;
    eprintln!("\nReport written to {path}");
    Ok(())
}

pub fn create_printer(ip: &str, serial: &str, access_code: &str) -> Result<Printer, CliError> {
    validate_params(ip, serial, access_code)?;

    let config = build_cli_tls_config(false)?;
    let tls_connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));

    Ok(PrinterClient::new(
        tls_connector,
        TokioRawStreamFactory,
        PrinterIdentity::new(ip, serial, access_code),
    )
    .with_timer(TokioTimer::new())
    .with_connect_timeout(CONNECT_TIMEOUT_SECS))
}

pub(crate) fn validate_params(ip: &str, serial: &str, access_code: &str) -> Result<(), CliError> {
    validate_ip_serial(ip, serial)?;
    bambino::identity::validate_access_code(access_code)
        .map_err(|e| CliError::InvalidArgs(e.to_string()))
}

/// Validates just the `ip`/`serial` pair, for subcommands that take no access code at all
/// (e.g. `verify-tls`, `inspect-cert`) — they must still reject malformed IPs/serials with
/// `InvalidArgs` before anything reaches the network/TLS layer.
///
/// The IP-literal requirement is the CLI's own: the library also accepts a hostname.
pub(crate) fn validate_ip_serial(ip: &str, serial: &str) -> Result<(), CliError> {
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(CliError::InvalidArgs(format!("Invalid IP address: '{ip}'")));
    }
    bambino::identity::validate_serial(serial).map_err(|e| CliError::InvalidArgs(e.to_string()))
}
