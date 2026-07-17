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
                  gcode-raw prompts for interactive confirmation unless --unsafe is
                  passed, and bypasses all model safety checks — see its --help.
                  ams (dry | dry-stop)
Files actions:    list  upload  delete  space  clock-check
Camera actions:   snapshot
Probe options:    -o/--output  -t/--tests"
)]
struct Cli {
    /// Enable verbose connection and packet debugging output
    #[arg(short = 'v', long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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

    /// Dump the raw pushall JSON response and exit (or every subsequent push, with --follow)
    Dump {
        ip: String,
        serial: String,
        /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
        #[arg(default_value = "")]
        access_code: String,
        /// Keep printing every subsequent `print`-bearing push as one compact NDJSON line
        /// until interrupted (Ctrl+C), instead of exiting after the first pushall response —
        /// for capturing a sequence of incremental pushes (e.g. across a tray-load event).
        #[arg(short = 'f', long)]
        follow: bool,
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
    #[command(
        flatten_help = true,
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] home\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] move <AXIS> <DISTANCE> [FEEDRATE]\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] extrude <LENGTH> [FEEDRATE]\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] fan <TARGET> <SPEED_PERCENT>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] temp <TARGET> <VALUE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] led <NODE> <STATE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] pause\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] resume\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] stop\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] gcode <GCODE_LINE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] gcode-raw [OPTIONS] <GCODE_LINE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] speed <LEVEL>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] clear-error\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] airduct <MODE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] calibrate <ROUTINES>...\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] ams dry <ID> <TEMP> <HOURS> <ROTATE> <FILAMENT>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] ams dry-stop <ID>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] ams help [COMMAND]\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] help [COMMAND]..."
    )]
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
    #[command(
        flatten_help = true,
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] list [REMOTE_PATH]\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] upload <LOCAL_PATH> <REMOTE_PATH>\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] delete <REMOTE_PATH>\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] clock-check\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] space\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] help [COMMAND]..."
    )]
    Files {
        ip: String,
        serial: String,
        /// Falls back to the BAMBINO_ACCESS_CODE env var if omitted or empty
        #[arg(default_value = "")]
        access_code: String,
        #[command(subcommand)]
        action: storage::FilesAction,
        /// Bypass BambuFtpsClient's TLS-1.2-enforcement check for P2S/X2D (the embassy
        /// escape hatch, ported to the CLI for testing; see src/ftps/CLAUDE.md and
        /// src/io/CLAUDE.md). On tokio, force_tls_1_2 is already applied automatically
        /// per-model — this flag exists to let a caller override enforcement even when
        /// negotiated_version reports non-1.2.
        #[arg(long)]
        allow_unverified_tls_1_2: bool,
    },

    /// Camera streaming operations
    #[command(
        flatten_help = true,
        override_usage = "bambino-cli camera <IP> <SERIAL> [ACCESS_CODE] snapshot [OUTPUT]\n       bambino-cli camera <IP> <SERIAL> [ACCESS_CODE] help [COMMAND]..."
    )]
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
    /// (see .claude/rules/tls-identity-sni.md). No FTPS/MQTT traffic is exchanged.
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
    /// (see .claude/rules/tls-identity-sni.md). No FTPS/MQTT traffic is exchanged.
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
        Commands::Discover => discover::run().await,
        Commands::Info {
            ip,
            serial,
            access_code,
        } => control::run_info(&ip, &serial, &resolve_access_code(access_code)).await,
        Commands::Monitor {
            ip,
            serial,
            access_code,
        } => monitor::run(&ip, &serial, &resolve_access_code(access_code)).await,
        Commands::Dump {
            ip,
            serial,
            access_code,
            follow,
        } => monitor::dump(&ip, &serial, &resolve_access_code(access_code), follow).await,
        Commands::Probe {
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
        Commands::Control {
            ip,
            serial,
            access_code,
            action,
        } => control::run(&ip, &serial, &resolve_access_code(access_code), action).await,
        Commands::Files {
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
        Commands::Camera {
            ip,
            serial,
            access_code,
            action,
        } => camera::run(&ip, &serial, &resolve_access_code(access_code), action).await,
        Commands::InspectCert {
            ip,
            serial,
            port,
            output,
        } => inspect_cert::run(&ip, &serial, port, &output).await,
        Commands::VerifyTls {
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
