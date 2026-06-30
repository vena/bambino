#![cfg(feature = "cli")]

use std::time::Duration;

use bambino::client::PrinterClient;
use bambino::client::dummy::{DummyFactory, DummyRawIo, DummyTls};
use bambino::error::BambuError;
use bambino::io::tokio::{
    TokioSecureConnector, TokioTimer, TokioTlsConnector, build_unsafe_client_config,
};
use bambino::models::resolve_model;

const CONNECT_TIMEOUT_SECS: u64 = 5;

pub type Printer =
    PrinterClient<TokioSecureConnector, TokioTimer, DummyRawIo, DummyTls, DummyFactory>;

pub fn create_printer(ip: &str, serial: &str, access_code: &str) -> Result<Printer, BambuError> {
    validate_params(ip, serial, access_code)?;

    let config = build_unsafe_client_config();
    let tls_connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));
    let connector =
        TokioSecureConnector::new(tls_connector, Duration::from_secs(CONNECT_TIMEOUT_SECS));

    let model = resolve_model(serial, None);

    Ok(PrinterClient::new(connector, ip, serial, access_code, model).with_timer(TokioTimer::new()))
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

    if access_code.is_empty() || access_code.len() > 16 {
        return Err(BambuError::ProtocolViolation(
            format!(
                "Invalid access code: expected 1-16 characters, got {}",
                access_code.len()
            )
            .into(),
        ));
    }

    Ok(())
}
