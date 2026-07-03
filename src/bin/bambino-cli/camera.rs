#![cfg(feature = "cli")]

use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::net::TcpStream;

use bambino::camera::CameraProtocol;
use bambino::camera::binary::BambuBinaryCameraStream;
use bambino::error::BambuError;
use bambino::io::tokio::{TokioTlsConnector, build_unsafe_client_config, to_socket_error};
use bambino::io::{SocketError, TlsConnector, TokioIo};
use bambino::models::resolve_model;
use clap::Subcommand;

use crate::connection::validate_params;

const CONNECT_TIMEOUT_SECS: u64 = 5;

#[derive(Subcommand, Debug)]
pub enum CameraAction {
    /// Capture a single JPEG frame (A1/P1 binary protocol only)
    Snapshot { output: Option<String> },
}

/// Dispatches a typed camera action.
pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    action: CameraAction,
) -> Result<(), BambuError> {
    match action {
        CameraAction::Snapshot { output } => {
            let output_path = output.as_deref().unwrap_or("snapshot.jpg");
            run_snapshot(ip, serial, access_code, output_path).await
        }
    }
}

async fn run_snapshot(
    ip: &str,
    serial: &str,
    access_code: &str,
    output_path: &str,
) -> Result<(), BambuError> {
    validate_params(ip, serial, access_code)?;

    let model = resolve_model(serial, None);
    let protocol = model.quirks().camera_protocol();
    if protocol != CameraProtocol::BinaryJpeg {
        eprintln!(
            "Warning: {} uses RTSPS (port {}), not the binary JPEG protocol.",
            serial,
            protocol.default_port()
        );
        eprintln!("The snapshot command only supports binary camera streaming (A1/P1 series).");
        return Err(BambuError::ProtocolViolation(
            "Model does not support binary JPEG camera protocol".into(),
        ));
    }

    let port = protocol.default_port();
    println!("Connecting to {}:{} ...", ip, port);

    let config = build_unsafe_client_config();
    let connector = tokio_rustls::TlsConnector::from(config);
    let tls_connector = TokioTlsConnector::new(connector);

    let addr = format!("{}:{}", ip, port);
    let tcp = tokio::time::timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        TcpStream::connect(&addr),
    )
    .await
    .map_err(|_| BambuError::NetworkError(SocketError::TimedOut))?
    .map_err(to_socket_error)?;

    let tls = tls_connector.connect(ip, TokioIo(tcp)).await?;

    let mut camera = BambuBinaryCameraStream::new(tls);

    println!("Authenticating ...");
    camera.authenticate(access_code).await?;

    println!("Capturing frame ...");
    let mut frame = Vec::new();
    camera.read_next_frame(&mut frame).await?;

    let path = Path::new(output_path);
    fs::write(path, &frame).map_err(|e| {
        BambuError::ProtocolViolation(format!("Failed to write {}: {}", output_path, e).into())
    })?;

    println!("Saved {} bytes to {}", frame.len(), output_path);
    Ok(())
}
