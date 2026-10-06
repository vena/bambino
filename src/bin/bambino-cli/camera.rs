#![cfg(feature = "cli")]

use std::fs;
use std::path::Path;

use bambino::Error;
use bambino::camera::CameraProtocol;
use bambino::io::TlsVersions;
use bambino::io::tokio::TokioRawStreamFactory;

use crate::trust::build_cli_tls_connector;
use clap::Subcommand;

use crate::connection::Target;
use crate::error::CliError;

#[derive(Subcommand, Debug)]
pub enum CameraAction {
    /// Capture a single JPEG frame (A1/P1 binary protocol only)
    #[command(override_usage = "bambino-cli camera <IP> <SERIAL> [ACCESS_CODE] snapshot [OUTPUT]")]
    Snapshot {
        #[arg(default_value = "snapshot.jpg")]
        output: String,
    },
}

/// Dispatches a typed camera action.
pub async fn run(target: &Target, action: CameraAction) -> Result<(), CliError> {
    match action {
        CameraAction::Snapshot { output } => run_snapshot(target, &output).await,
    }
}

async fn run_snapshot(target: &Target, output_path: &str) -> Result<(), CliError> {
    let tls_connector = build_cli_tls_connector(TlsVersions::Default)?;
    let mut printer = target
        .printer()?
        .with_camera(tls_connector, TokioRawStreamFactory);

    println!(
        "Connecting to {} port {} ...",
        target.ip,
        CameraProtocol::BinaryJpeg.default_port()
    );
    println!("Capturing frame ...");
    // The library refuses an RTSPS model before dialing; that refusal is the only check.
    let frame = printer.read_camera_frame().await.map_err(|e| match e {
        Error::ModelMismatch(_) => CliError::InvalidArgs(format!(
            "{} streams its camera over RTSPS; snapshot supports only the binary JPEG \
             protocol (A1/P1 series)",
            target.serial
        )),
        other => other.into(),
    })?;

    let path = Path::new(output_path);
    fs::write(path, &frame)?;

    println!("Saved {} bytes to {}", frame.len(), output_path);
    Ok(())
}
