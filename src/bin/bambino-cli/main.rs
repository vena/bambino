#![cfg(feature = "cli")]

//! # Interactive Developer CLI Testing Utility
//!
//! Provides an on-machine terminal application to test, monitor, and debug the
//! `bambino` protocol engine against physical hardware targets on the local network.
//!
//! Handles command-line argument routing to specialized submodules without pulling
//! in heavy external parsing frameworks.

use std::env;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};

mod camera;
mod connection;
mod control;
mod discover;
mod monitor;
mod probe;
mod storage;
mod table;

/// Global static indicating whether verbose debug logging is requested.
///
/// **Why this is an AtomicBool:**
/// Allows lightweight, thread-safe access from deep within async tasks and submodules
/// (e.g., `control.rs`, `monitor.rs`, and protocol libraries) without requiring complex
/// parameter passing or pull-in of large configuration containers.
pub static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Checks if the application-wide verbose flag has been armed.
pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed)
}

/// Prints standardized interactive help instructions to the standard output.
fn print_usage() {
    println!(
        r#"Bambu Lab Local LAN Protocol Developer CLI Tool

Usage:
  bambino-cli [FLAGS] <COMMAND> [ARGS...]

Flags:
  -v, --verbose                                    Enable verbose connection and packet debugging output

Commands:
  discover                                         Scan the local subnet for nearby active printers
  info    <ip> <serial> <access_code>              Query expansion bus module and firmware versions
  monitor <ip> <serial> <access_code>              Stream real-time status telemetry and HMS warnings
  dump    <ip> <serial> <access_code>              Dump the raw pushall JSON response and exit
  probe   <ip> <serial> <access_code> [OPTIONS]    Run command response capture suite and write report
  control <ip> <serial> <access_code> <ACTION>     Dispatch a movement or hardware control command
  files   <ip> <serial> <access_code> <ACTION>     Traverse and transfer files on the printer's MicroSD card
  camera  <ip> <serial> <access_code> <ACTION>     Camera streaming operations

Control Actions:
  home                                             Home all structural motion axes safely
  move <axis> <distance> <feedrate>                Execute relative motion (e.g., move z -10 3000)
  extrude <length> <feedrate>                      Extrude relative filament length (e.g., extrude 10 900)
  fan <target> <speed_percent>                     Configure PWM fan speed (targets: part, aux, exhaust, right)
  temp <target> <value>                            Set hotend or build-plate temperatures (targets: nozzle, bed)
  led <node> <on|off>                              Toggle chamber or auxiliary LEDs (nodes: chamber, work)
  speed <level>                                    Set print speed (silent, standard, sport, ludicrous)
  clear-error                                      Clear active print error codes
  airduct <mode>                                   Switch airduct damper (cooling, heating, laser)
  calibrate <routine> [routine...]                 Trigger calibration (bed-leveling, vibration, motor-noise,
                                                     nozzle-height, heatbed-thermal)
  ams dry <id> <temp> <time> <rotate> <filament>   Start AMS drying cycle (time in minutes)
  ams dry-stop <id>                                Stop AMS drying cycle
  gcode <gcode_line>                               Send G-code with model safety checks
  gcode-raw [--unsafe] <gcode_line>                Send raw G-code bypassing safety checks
  pause | resume | stop                            Manage active print queue execution states

Files Actions:
  list <remote_path>                               Perform a UNIX directory listing traversal
  upload <local_path> <remote_path>                Upload a local file to the remote card path
  delete <remote_path>                             Remove a file from the remote filesystem path
  space                                            Query available MicroSD card capacity

Camera Actions:
  snapshot [output.jpg]                            Capture a single JPEG frame (A1/P1 binary protocol only)

Probe Options:
  -o, --output <path>                              Output file (default: probe_report.json)
  -t, --tests <list>                               Comma-separated test names to run (default: all)
                                                   Tests: move_z_unhomed, pause_when_idle, resume_when_idle,
                                                     stop_when_idle, clear_error, led_on, led_off,
                                                     fan_part_zero, temp_nozzle_zero, temp_bed_zero, home_axes
"#
    );
}

#[tokio::main]
async fn main() {
    let mut args: Vec<String> = env::args().collect();

    // Detect and strip out all verbose flags to simplify positional command matching.
    let mut verbose = false;
    args.retain(|arg| {
        if arg == "--verbose" || arg == "-v" {
            verbose = true;
            false
        } else {
            true
        }
    });
    VERBOSE.store(verbose, Ordering::SeqCst);

    let log_level = if verbose { "debug" } else { "warn" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_target(true)
        .init();

    // Positional matching is relative to the binary path at args[0]
    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let command = args[1].to_lowercase();

    let result = match command.as_str() {
        "discover" => discover::run().await,
        "info" => {
            if args.len() < 5 {
                eprintln!(
                    "Error: Missing required parameters.\nUsage: bambino-cli info <ip> <serial> <access_code>"
                );
                process::exit(1);
            }
            control::run_info(&args[2], &args[3], &args[4]).await
        }
        "monitor" => {
            if args.len() < 5 {
                eprintln!(
                    "Error: Missing required parameters.\nUsage: bambino-cli monitor <ip> <serial> <access_code>"
                );
                process::exit(1);
            }
            monitor::run(&args[2], &args[3], &args[4]).await
        }
        "dump" => {
            if args.len() < 5 {
                eprintln!(
                    "Error: Missing required parameters.\nUsage: bambino-cli dump <ip> <serial> <access_code>"
                );
                process::exit(1);
            }
            monitor::dump(&args[2], &args[3], &args[4]).await
        }
        "probe" => {
            if args.len() < 5 {
                eprintln!(
                    "Error: Missing required parameters.\nUsage: bambino-cli probe <ip> <serial> <access_code> [OPTIONS]"
                );
                process::exit(1);
            }
            probe::run(&args[2], &args[3], &args[4], &args[5..]).await
        }
        "control" => {
            if args.len() < 6 {
                eprintln!(
                    "Error: Missing action parameter.\nUsage: bambino-cli control <ip> <serial> <access_code> <ACTION> [ARGS]"
                );
                process::exit(1);
            }
            control::run(&args[2], &args[3], &args[4], &args[5..]).await
        }
        "files" => {
            if args.len() < 6 {
                eprintln!(
                    "Error: Missing action parameter.\nUsage: bambino-cli files <ip> <serial> <access_code> <ACTION> [ARGS]"
                );
                process::exit(1);
            }
            storage::run(&args[2], &args[3], &args[4], &args[5..]).await
        }
        "camera" => {
            if args.len() < 6 {
                eprintln!(
                    "Error: Missing action parameter.\nUsage: bambino-cli camera <ip> <serial> <access_code> <ACTION> [ARGS]"
                );
                process::exit(1);
            }
            camera::run(&args[2], &args[3], &args[4], &args[5..]).await
        }
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        other => {
            eprintln!("Error: Unrecognized command '{}'.", other);
            print_usage();
            process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
