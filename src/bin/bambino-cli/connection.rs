#![cfg(feature = "cli")]

use bambino::client::PrinterClient;
use bambino::client::dummy::{DummyFactory, DummyRawIo, DummyTls};
use bambino::error::BambuError;
use bambino::io::TokioIo;
use bambino::io::tokio::{
    TokioRawStreamFactory, TokioTimer, TokioTlsConnector, build_unsafe_client_config,
};
use bambino::models::resolve_model;

const CONNECT_TIMEOUT_SECS: u64 = 5;

/// Environment variable consulted as a fallback source for the access code when the positional `access_code` CLI argument is omitted or empty.
/// Lets scripted/CI usage avoid putting the access code in shell history; the positional arg still
/// takes precedence when non-empty.
const ACCESS_CODE_ENV_VAR: &str = "BAMBINO_ACCESS_CODE";

/// Resolves the access code to actually use: the positional CLI argument if non-empty, otherwise the `BAMBINO_ACCESS_CODE` environment variable (empty string if unset), letting `validate_params`'s existing empty-check produce a consistent error either way.
pub(crate) fn resolve_access_code(access_code: String) -> String {
    if access_code.is_empty() {
        std::env::var(ACCESS_CODE_ENV_VAR).unwrap_or_default()
    } else {
        access_code
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

pub fn create_printer(ip: &str, serial: &str, access_code: &str) -> Result<Printer, BambuError> {
    validate_params(ip, serial, access_code)?;

    let config = build_unsafe_client_config();
    let tls_connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));

    let model = resolve_model(serial, None);

    Ok(PrinterClient::new(
        tls_connector,
        TokioRawStreamFactory,
        ip,
        serial,
        access_code,
        model,
    )
    .with_timer(TokioTimer::new())
    .with_connect_timeout(CONNECT_TIMEOUT_SECS))
}

pub(crate) fn validate_params(ip: &str, serial: &str, access_code: &str) -> Result<(), BambuError> {
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err(BambuError::ProtocolViolation(
            format!("Invalid IP address: '{}'", ip).into(),
        ));
    }

    if serial.is_empty() || serial.len() > 20 || !serial.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(BambuError::ProtocolViolation(
            format!(
                "Invalid serial number: '{}' (expected 1-20 alphanumeric characters)",
                serial
            )
            .into(),
        ));
    }

    // BUG-130: aligned with CAMERA_PASSWORD_MAX_LEN (camera/binary.rs) and the alphanumeric
    // requirement camera/rtsps.rs and camera/binary.rs both enforce downstream, instead of a
    // narrower CLI-only ceiling that could reject a legitimate code or admit one that fails later.
    if access_code.is_empty()
        || access_code.len() > bambino::camera::binary::CAMERA_PASSWORD_MAX_LEN
        || !access_code.bytes().all(|b| b.is_ascii_alphanumeric())
    {
        return Err(BambuError::ProtocolViolation(
            format!(
                "Invalid access code: expected 1-{} alphanumeric characters, got {}",
                bambino::camera::binary::CAMERA_PASSWORD_MAX_LEN,
                access_code.len()
            )
            .into(),
        ));
    }

    Ok(())
}
