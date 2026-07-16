#![cfg(feature = "cli")]

use std::fs;
use std::path::Path;

use bambino::camera::CameraProtocol;
use bambino::error::Error;
use bambino::io::tokio::{TokioRawStreamFactory, TokioTlsConnector, build_unsafe_client_config};
use clap::Subcommand;

use crate::connection::create_printer;

#[derive(Subcommand, Debug)]
pub enum CameraAction {
    /// Capture a single JPEG frame (A1/P1 binary protocol only)
    #[command(override_usage = "bambino-cli camera <IP> <SERIAL> [ACCESS_CODE] snapshot [OUTPUT]")]
    Snapshot { output: Option<String> },
}

/// Dispatches a typed camera action.
pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    action: CameraAction,
) -> Result<(), Error> {
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
) -> Result<(), Error> {
    let printer = create_printer(ip, serial, access_code)?;

    let protocol = printer.model().quirks().camera_protocol();
    if protocol != CameraProtocol::BinaryJpeg {
        eprintln!(
            "Warning: {} uses RTSPS (port {}), not the binary JPEG protocol.",
            serial,
            protocol.default_port()
        );
        eprintln!("The snapshot command only supports binary camera streaming (A1/P1 series).");
        return Err(Error::ProtocolViolation(
            "Model does not support binary JPEG camera protocol".into(),
        ));
    }

    let config = build_unsafe_client_config();
    let tls_connector = TokioTlsConnector::new(tokio_rustls::TlsConnector::from(config));

    let mut printer = printer.with_camera(tls_connector, TokioRawStreamFactory);

    println!("Connecting to {}:{} ...", ip, protocol.default_port());

    println!("Capturing frame ...");
    let mut frame = Vec::new();
    printer.read_camera_frame(&mut frame).await?;

    let path = Path::new(output_path);
    fs::write(path, &frame).map_err(|e| {
        Error::ProtocolViolation(format!("Failed to write {}: {}", output_path, e).into())
    })?;

    println!("Saved {} bytes to {}", frame.len(), output_path);
    Ok(())
}
