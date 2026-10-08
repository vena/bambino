#![cfg(feature = "cli")]

//! # Interactive Developer CLI Testing Utility
//!
//! Provides an on-machine terminal application to test, monitor, and debug the
//! `bambino` protocol engine against physical hardware targets on the local network.

use std::process;
use std::sync::atomic::{AtomicBool, Ordering};

use clap::{Parser, Subcommand};

mod ack_probe;
mod camera;
mod connection;
mod control;
mod discover;
mod error;
mod inspect_cert;
mod monitor;
mod probe;
mod prompt;
mod redact;
mod storage;
mod table;
mod trust;
mod verify_tls;

use connection::Target;

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
                  passed, and bypasses all model safety checks; see its --help.
                  ams (dry | dry-stop)
Files actions:    list  upload  download  delete  space  clock-check
Camera actions:   snapshot
Probe options:    -o/--output  -t/--tests
Ack-probe:        -o/--output  -t/--tests  --window"
)]
struct Cli {
    /// Enable verbose connection and packet debugging output
    #[arg(short = 'v', long, global = true)]
    verbose: bool,

    /// Verify the printer's TLS certificate against these CA certs instead of skipping
    /// verification entirely. Accepts a single PEM/DER file or a directory of them (e.g.
    /// --with-certs certs/). Applies to every printer-facing subcommand (MQTT, FTPS, camera)
    /// except inspect-cert, which never verifies because its job is to capture whatever
    /// certificate the printer presents. Without it the CLI performs no certificate
    /// verification at all.
    #[arg(long, value_name = "PATH", global = true)]
    with_certs: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan the local subnet for nearby active printers
    Discover,

    /// Query expansion bus module and firmware versions
    Info {
        #[command(flatten)]
        target: Target,
        /// Print expansion bus module serials in the table (they identify physical hardware,
        /// so they're hidden by default; a stdout redirect captures whatever this prints)
        #[arg(long)]
        show_serials: bool,
    },

    /// Stream real-time status telemetry and HMS warnings
    Monitor {
        #[command(flatten)]
        target: Target,
    },

    /// Dump the raw pushall JSON response and exit (or every subsequent push, with --follow)
    Dump {
        #[command(flatten)]
        target: Target,
        /// Keep printing every subsequent `print`-bearing push as one compact NDJSON line
        /// until interrupted (Ctrl+C), instead of exiting after the first pushall response —
        /// for capturing a sequence of incremental pushes (e.g. across a tray-load event).
        #[arg(short = 'f', long)]
        follow: bool,
        /// Print serials and access codes as the printer sent them instead of `<redacted>`
        /// (a stdout redirect captures whatever this prints)
        #[arg(long)]
        show_serials: bool,
    },

    /// Run command response capture suite and write report
    Probe {
        #[command(flatten)]
        target: Target,
        /// Output file path
        #[arg(short = 'o', long, default_value = "probe_report.json")]
        output: String,
        /// Comma-separated test names to run (default: all non-manual tests)
        #[arg(short = 't', long, value_delimiter = ',')]
        tests: Option<Vec<probe::ProbeTest>>,
    },

    // Evidence harness for `ACK_CORRELATED_COMMANDS` (issue #26). Doc comments on this enum are
    // the CLI's `--help` text, so contributor references stay in `//` comments like this one.
    /// Check which MQTT commands echo a correlatable `sequence_id` ack
    AckProbe {
        #[command(flatten)]
        target: Target,
        /// Output file path
        #[arg(short = 'o', long, default_value = "ack_probe_report.json")]
        output: String,
        /// Comma-separated wire command names to test (default: all that neither actuate hardware nor change a setting)
        #[arg(short = 't', long, value_delimiter = ',')]
        tests: Option<Vec<ack_probe::AckTest>>,
        /// Seconds to listen for a correlated ack after each command (1-3600)
        #[arg(
            long,
            default_value_t = ack_probe::DEFAULT_ACK_WINDOW_SECS,
            value_parser = clap::value_parser!(u64).range(1..=3600)
        )]
        window: u64,
    },

    /// Dispatch a movement or hardware control command
    #[command(
        flatten_help = true,
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] home\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] move <AXIS> <DISTANCE> [FEEDRATE]\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] extrude <LENGTH> [FEEDRATE]\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] fan <TARGET> <SPEED_PERCENT>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] temp <TARGET> <VALUE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] led <NODE> <STATE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] pause\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] resume\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] stop\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] gcode <GCODE_LINE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] gcode-raw [OPTIONS] <GCODE_LINE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] speed <LEVEL>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] clear-error\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] airduct <MODE>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] calibrate <ROUTINES>... [--watch [--show-serials]]\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] ams dry <ID> --material <NAME> | --temp <C> --duration-hours <H>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] ams dry-stop <ID>\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] ams help [COMMAND]\n       bambino-cli control <IP> <SERIAL> [ACCESS_CODE] help [COMMAND]..."
    )]
    Control {
        #[command(flatten)]
        target: Target,
        #[command(subcommand)]
        action: control::ControlAction,
    },

    /// Traverse and transfer files on the printer's MicroSD card
    #[command(
        flatten_help = true,
        override_usage = "bambino-cli files <IP> <SERIAL> [ACCESS_CODE] list [REMOTE_PATH]\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] upload <LOCAL_PATH> <REMOTE_PATH>\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] download <REMOTE_PATH> <LOCAL_PATH>\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] delete <REMOTE_PATH>\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] clock-check\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] space\n       bambino-cli files <IP> <SERIAL> [ACCESS_CODE] help [COMMAND]..."
    )]
    Files {
        #[command(flatten)]
        target: Target,
        #[command(subcommand)]
        action: storage::FilesAction,
        // The embassy escape hatch, ported to the CLI for testing; see src/ftps/CLAUDE.md and
        // src/io/CLAUDE.md.
        /// Skip the check that P2S/X2D FTPS negotiated TLS 1.2. The CLI already requests
        /// TLS 1.2 for those models; this proceeds even if the connection reports another version.
        #[arg(long)]
        allow_unverified_tls_1_2: bool,
    },

    /// Camera streaming operations
    #[command(
        flatten_help = true,
        override_usage = "bambino-cli camera <IP> <SERIAL> [ACCESS_CODE] snapshot [OUTPUT]\n       bambino-cli camera <IP> <SERIAL> [ACCESS_CODE] help [COMMAND]..."
    )]
    Camera {
        #[command(flatten)]
        target: Target,
        #[command(subcommand)]
        action: camera::CameraAction,
    },

    // Background on the SAN/CN identity check: .claude/rules/tls-identity-sni.md.
    /// Capture a printer's raw TLS certificate chain to disk for SAN/CN inspection
    ///
    /// Also reports whether the printer sends its issuing CA alongside the leaf, which decides
    /// what certificate pinning an application can build. No FTPS/MQTT traffic is exchanged.
    InspectCert {
        ip: String,
        serial: String,
        /// TLS port to connect to (990=FTPS, 8883=MQTT, 322=RTSPS, 6000=camera)
        #[arg(long, default_value_t = bambino::ftps::FTPS_PORT)]
        port: u16,
        /// Where to write the leaf certificate's raw DER bytes. Any further chain members are
        /// written beside it with `.chain<N>` before the extension (cert.der → cert.chain1.der)
        #[arg(short = 'o', long, default_value = "printer_leaf_cert.der")]
        output: String,
    },

    // Validates `build_verified_client_config`/`CnFallbackServerVerifier` end-to-end; background
    // in .claude/rules/tls-identity-sni.md.
    /// Attempt a CA-verified TLS handshake against a printer
    ///
    /// Checks the chain of trust, the handshake signature, and that the certificate names the
    /// printer's serial. Requires --with-certs. No FTPS/MQTT traffic is exchanged.
    VerifyTls {
        ip: String,
        serial: String,
        /// TLS port to connect to (990=FTPS, 8883=MQTT, 322=RTSPS, 6000=camera)
        #[arg(long, default_value_t = bambino::ftps::FTPS_PORT)]
        port: u16,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    VERBOSE.store(cli.verbose, Ordering::SeqCst);
    let log_level = if cli.verbose { "debug" } else { "warn" };
    let mut log_builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level));
    log_builder.format_target(true);
    // The monitor dashboard owns the terminal: raw mode plus the alternate screen are
    // tty-level states shared by stdout and stderr, so any log record written while it runs
    // lands mid-screen at the dashboard's cursor position with stair-stepped line breaks.
    // Discarding is safe here because the dashboard surfaces its own diagnostics through
    // `RawWriter` (see monitor::dashboard), and a fatal error still returns as a `CliError`
    // printed after the terminal guard restores the screen.
    if matches!(cli.command, Commands::Monitor { .. }) {
        log_builder.target(env_logger::Target::Pipe(Box::new(std::io::sink())));
    }
    log_builder.init();

    if let Some(path) = cli.with_certs.as_deref() {
        match trust::load_trust_anchors(path) {
            Ok(anchors) => trust::set_trusted_roots(anchors),
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        }
    }

    let result = match cli.command {
        Commands::Discover => discover::run().await,
        Commands::Info {
            target,
            show_serials,
        } => control::run_info(&target, show_serials).await,
        Commands::Monitor { target } => monitor::run(&target).await,
        Commands::Dump {
            target,
            follow,
            show_serials,
        } => monitor::dump(&target, follow, show_serials).await,
        Commands::Probe {
            target,
            output,
            tests,
        } => probe::run(&target, &output, tests).await,
        Commands::AckProbe {
            target,
            output,
            tests,
            window,
        } => ack_probe::run(&target, &output, tests, window).await,
        Commands::Control { target, action } => control::run(&target, action).await,
        Commands::Files {
            target,
            action,
            allow_unverified_tls_1_2,
        } => storage::run(&target, action, allow_unverified_tls_1_2).await,
        Commands::Camera { target, action } => camera::run(&target, action).await,
        Commands::InspectCert {
            ip,
            serial,
            port,
            output,
        } => inspect_cert::run(&ip, &serial, port, &output).await,
        Commands::VerifyTls { ip, serial, port } => verify_tls::run(&ip, &serial, port).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
