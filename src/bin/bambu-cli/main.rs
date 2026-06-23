#![cfg(feature = "std")]

//! # Interactive Developer CLI Testing Utility
//!
//! Provides an on-machine terminal application to test, monitor, and debug the
//! `bambu-lan` protocol engine against physical hardware targets on the local network.
//!
//! Handles command-line argument routing to specialized submodules without pulling
//! in heavy external parsing frameworks.

use std::env;
use std::process;

mod control;
mod discover;
mod monitor;
mod storage;

/// Prints standardized interactive help instructions to the standard output.
fn print_usage() {
    println!(
        r#"Bambu Lab Local LAN Protocol Developer CLI Tool

Usage:
  bambu-cli <COMMAND> [ARGS...]

Commands:
  discover                                         Scan the local subnet for nearby active printers
  info    <ip> <serial> <access_code>              Query expansion bus module and firmware versions
  monitor <ip> <serial> <access_code>              Stream real-time status telemetry and HMS warnings
  control <ip> <serial> <access_code> <ACTION>     Dispatch a movement or hardware control command
  files   <ip> <serial> <access_code> <ACTION>     Traverse and transfer files on the printer's MicroSD card

Control Actions:
  home                                             Home all structural motion axes safely
  move <axis> <distance> <feedrate>                Execute relative motion (e.g., move z -10 3000)
  extrude <length> <feedrate>                      Extrude relative filament length (e.g., extrude 10 900)
  fan <target> <speed_percent>                     Configure PWM fan speed (targets: part, aux, exhaust)
  temp <target> <value>                            Set hotend or build-plate temperatures (targets: nozzle, bed)
  led <node> <on|off>                              Toggle chamber or auxiliary LEDs (nodes: chamber, work)
  pause | resume | stop                            Manage active print queue execution states

Files Actions:
  list <remote_path>                               Perform a UNIX directory listing traversal
  upload <local_path> <remote_path>                Upload a local file to the remote card path
  delete <remote_path>                             Remove a file from the remote filesystem path
  space                                            Query available MicroSD card capacity
"#
    );
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let command = args[1].to_lowercase();

    let result = match command.as_str() {
        "discover" => discover::run().await,
        "info" => {
            if args.len() < 5 {
                eprintln!("Error: Missing required parameters.\nUsage: bambu-cli info <ip> <serial> <access_code>");
                process::exit(1);
            }
            control::run_info(&args[2], &args[3], &args[4]).await
        }
        "monitor" => {
            if args.len() < 5 {
                eprintln!("Error: Missing required parameters.\nUsage: bambu-cli monitor <ip> <serial> <access_code>");
                process::exit(1);
            }
            monitor::run(&args[2], &args[3], &args[4]).await
        }
        "control" => {
            if args.len() < 6 {
                eprintln!("Error: Missing action parameter.\nUsage: bambu-cli control <ip> <serial> <access_code> <ACTION> [ARGS]");
                process::exit(1);
            }
            control::run(&args[2], &args[3], &args[4], &args[5..]).await
        }
        "files" => {
            if args.len() < 6 {
                eprintln!("Error: Missing action parameter.\nUsage: bambu-cli files <ip> <serial> <access_code> <ACTION> [ARGS]");
                process::exit(1);
            }
            storage::run(&args[2], &args[3], &args[4], &args[5..]).await
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
        eprintln!("Command Execution Failure: {:?}", e);
        process::exit(1);
    }
}
