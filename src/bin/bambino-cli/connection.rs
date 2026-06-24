#![cfg(feature = "std")]

use std::time::Duration;
use tokio::net::TcpStream;

use bambino::error::BambuError;
use bambino::io::tokio::{
    TokioTimer, TokioTlsConnector, build_unsafe_client_config, to_socket_error,
};
use bambino::io::{SocketError, TlsConnector, TokioIo};
use bambino::mqtt::BambuMqttClient;

const MQTTS_PORT: u16 = 8883;
const CONNECT_TIMEOUT_SECS: u64 = 5;

pub type MqttClient =
    BambuMqttClient<<TokioTlsConnector as TlsConnector<TokioIo<TcpStream>>>::Stream>;

pub async fn connect_mqtt(
    ip: &str,
    serial: &str,
    access_code: &str,
) -> Result<MqttClient, BambuError> {
    validate_params(ip, serial, access_code)?;

    log::debug!("Configuring TLS client context utilizing self-signed certificate verifier");
    let config = build_unsafe_client_config();
    let connector = tokio_rustls::TlsConnector::from(config);
    let tls_connector = TokioTlsConnector::new(connector);

    let addr = format!("{}:{}", ip, MQTTS_PORT);
    log::debug!("Dialing TCP socket to {}", addr);
    let tcp_stream = tokio::time::timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        TcpStream::connect(&addr),
    )
    .await
    .map_err(|_| BambuError::NetworkError(SocketError::TimedOut))?
    .map_err(to_socket_error)?;
    let raw_io = TokioIo(tcp_stream);

    log::debug!("Wrapping socket in secure TLS session");
    let secure_stream = tls_connector.connect(ip, MQTTS_PORT, raw_io).await?;

    log::debug!("Initiating secure MQTT v3.1.1 protocol handshake");
    let client = BambuMqttClient::connect::<TokioTimer>(secure_stream, serial, access_code).await?;

    log::debug!("MQTT protocol session established successfully");
    Ok(client)
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
