#![cfg(feature = "cli")]

//! # Interactive Developer CLI Testing Utility
//!
//! Provides an on-machine terminal application to test, monitor, and debug the
//! `bambino` protocol engine against physical hardware targets on the local network.

use std::process;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{Parser, Subcommand};

mod camera;
mod connection;
mod control;
mod discover;
mod inspect_cert;
mod monitor;
mod probe;
mod storage;
mod table;
mod verify_tls;

use connection::resolve_access_code;

/// Global static indicating whether verbose debug logging is requested.
///
/// **Why this is an AtomicBool:**
/// Allows lightweight, thread-safe access from deep within async tasks and submodules
/// without requiring complex parameter passing or large configuration containers.
pub static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Checks if the application-wide verbose flag has been armed.
pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

#[derive(Parser)]
#[command(
    name = "bambino-cli",
    about = "Bambu Lab Local LAN Protocol Developer CLI Tool",
    after_help = "\
Most commands require positional args: <IP> <SERIAL> <ACCESS_CODE>
ACCESS_CODE may be omitted (or passed as \"\") to fall back to the
BAMBINO_ACCESS_CODE environment variable.
Run 'bambino-cli <COMMAND> --help' for full argument details.

Control actions:  home  move  extrude  fan  temp  led  speed  clear-error
                  airduct  calibrate  gcode  gcode-raw  pause  resume  stop
                  ams (dry | dry-stop)
Files actions:    list  upload  delete  space
Camera actions:   snapshot
Probe options:    -o/--output  -t/--tests"
)]
struct Cli {
    /// Enable verbose connection and packet debugging output
    #[arg(short = 'v', long)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan the local subnet for nearby active printers
    Discover,

    /// Query expansion bus module and firmware versions
    Info {
        ip: String,
        serial: String,
        /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
        #[arg(default_value = "")]
        access_code: String,
    },

    /// Stream real-time status telemetry and HMS warnings
    Monitor {
        ip: String,
        serial: String,
        /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
        #[arg(default_value = "")]
        access_code: String,
    },

    /// Dump the raw pushall JSON response and exit
    Dump {
        ip: String,
        serial: String,
        /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
        #[arg(default_value = "")]
        access_code: String,
    },

    /// Run command response capture suite and write report
    Probe {
        ip: String,
        serial: String,
        /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
        #[arg(default_value = "")]
        access_code: String,
        /// Output file path
        #[arg(short = 'o', long, default_value = "probe_report.json")]
        output: String,
        /// Comma-separated test names to run (default: all non-manual tests)
        #[arg(short = 't', long)]
        tests: Option<String>,
    },

    /// Dispatch a movement or hardware control command
    #[command(flatten_help = true)]
    Control {
        ip: String,
        serial: String,
        /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
        #[arg(default_value = "")]
        access_code: String,
        #[command(subcommand)]
        action: control::ControlAction,
    },

    /// Traverse and transfer files on the printer's MicroSD card
    #[command(flatten_help = true)]
    Files {
        ip: String,
        serial: String,
        /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
        #[arg(default_value = "")]
        access_code: String,
        #[command(subcommand)]
        action: storage::FilesAction,
        /// Bypass BambuFtpsClient's TLS-1.2-enforcement check for P2S/X2D (embassy escape
        /// hatch semantics ported to the CLI for testing; see EMBASSY_TLS_ESCAPE_HATCH_PLAN.md).
        /// On tokio, force_tls_1_2 is already applied automatically per-model — this flag exists
        /// to let a caller override enforcement even when negotiated_version reports non-1.2.
        #[arg(long)]
        allow_unverified_tls_1_2: bool,
    },

    /// Camera streaming operations
    #[command(flatten_help = true)]
    Camera {
        ip: String,
        serial: String,
        /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
        #[arg(default_value = "")]
        access_code: String,
        #[command(subcommand)]
        action: camera::CameraAction,
    },

    /// Diagnostic: capture a printer's raw leaf TLS cert to disk for SAN/CN inspection
    /// (see TLS_SNI_HOSTNAME_MISMATCH_PLAN.md). No FTPS/MQTT traffic is exchanged.
    InspectCert {
        ip: String,
        serial: String,
        /// TLS port to connect to (990=FTPS, 8883=MQTT, 322=RTSPS, 6000=camera)
        #[arg(long, default_value_t = 990)]
        port: u16,
        /// Where to write the captured leaf certificate's raw DER bytes
        #[arg(short = 'o', long, default_value = "printer_leaf_cert.der")]
        output: String,
    },

    /// Diagnostic: attempt a real CA-verified TLS handshake (SNI=serial) against a printer
    /// using build_verified_client_config, to validate CnFallbackServerVerifier end-to-end
    /// (see TLS_SNI_HOSTNAME_MISMATCH_PLAN.md). No FTPS/MQTT traffic is exchanged.
    VerifyTls {
        ip: String,
        serial: String,
        /// TLS port to connect to (990=FTPS, 8883=MQTT, 322=RTSPS, 6000=camera)
        #[arg(long, default_value_t = 990)]
        port: u16,
        /// Path to a PEM-encoded CA cert to trust (e.g. certs/bbl-ca-root.pem)
        #[arg(long)]
        ca_cert: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    VERBOSE.store(cli.verbose, Ordering::SeqCst);
    let log_level = if cli.verbose { "debug" } else { "warn" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_target(true)
        .init();

    let result = match cli.command {
        Command::Discover => discover::run().await,
        Command::Info {
            ip,
            serial,
            access_code,
        } => control::run_info(&ip, &serial, &resolve_access_code(access_code)).await,
        Command::Monitor {
            ip,
            serial,
            access_code,
        } => monitor::run(&ip, &serial, &resolve_access_code(access_code)).await,
        Command::Dump {
            ip,
            serial,
            access_code,
        } => monitor::dump(&ip, &serial, &resolve_access_code(access_code)).await,
        Command::Probe {
            ip,
            serial,
            access_code,
            output,
            tests,
        } => {
            probe::run(
                &ip,
                &serial,
                &resolve_access_code(access_code),
                &output,
                tests.as_deref(),
            )
            .await
        }
        Command::Control {
            ip,
            serial,
            access_code,
            action,
        } => control::run(&ip, &serial, &resolve_access_code(access_code), action).await,
        Command::Files {
            ip,
            serial,
            access_code,
            action,
            allow_unverified_tls_1_2,
        } => {
            storage::run(
                &ip,
                &serial,
                &resolve_access_code(access_code),
                action,
                allow_unverified_tls_1_2,
            )
            .await
        }
        Command::Camera {
            ip,
            serial,
            access_code,
            action,
        } => camera::run(&ip, &serial, &resolve_access_code(access_code), action).await,
        Command::InspectCert {
            ip,
            serial,
            port,
            output,
        } => inspect_cert::run(&ip, &serial, port, &output).await,
        Command::VerifyTls {
            ip,
            serial,
            port,
            ca_cert,
        } => verify_tls::run(&ip, &serial, port, &ca_cert).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
