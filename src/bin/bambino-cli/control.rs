#![cfg(feature = "std")]

//! # Motion, Thermal, and Peripheral Control Subcommand
//!
//! Handles dispatching manual commands to the printer motion controller
//! and querying hardware modules from the expansion bus [REF-MOTO-GCODE].
//!
//! Incorporates detailed diagnostic telemetry printing if `--verbose` is enabled
//! to isolate connection, handshake, and packet serialization issues.

use std::io::{self, Write};
use std::time::Duration;

use bambino::client::{FanTarget, PrinterClient};
use bambino::error::BambuError;
use bambino::models::resolve_model;

use crate::connection::connect_mqtt;

/// Connects to the printer, sends a `get_version` command, and displays expansion bus modules.
pub async fn run_info(ip: &str, serial: &str, access_code: &str) -> Result<(), BambuError> {
    let is_verbose = crate::is_verbose();
    let mqtt = connect_mqtt(ip, serial, access_code).await?;
    let model = resolve_model(serial, None);
    let mut printer = PrinterClient::new(mqtt, serial, model);

    println!("Querying expansion bus version database...");

    match tokio::time::timeout(Duration::from_secs(10), printer.get_version()).await {
        Ok(Ok(info)) => {
            let mut table = crate::table::Table::new(vec![
                "Product", "Module", "Hardware", "Firmware", "Serial",
            ]);

            for m in &info.module {
                if !m.visible && !is_verbose {
                    continue;
                }
                table.add_row(vec![&m.product_name, &m.name, &m.hw_ver, &m.sw_ver, &m.sn]);
            }

            println!();
            table.print();
            if !is_verbose {
                println!("\n  Use -v to show all internal modules.");
            }
            println!();
        }
        Ok(Err(e)) => {
            log::debug!("Version query generated an error: {:?}", e);
            return Err(e);
        }
        Err(_) => {
            println!("\n\x1B[1;33mNotice: Version query timed out after 10 seconds.\x1B[0m");
            println!(
                "Note: If this model does not reply, it confirms the physical firmware on this"
            );
            println!(
                "specific hardware track discards or ignores 'get_version' payloads over MQTTS.\n"
            );
        }
    }

    Ok(())
}

/// Parses and routes control commands using the unified `PrinterClient` coordinator.
pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    action_args: &[String],
) -> Result<(), BambuError> {
    if action_args.is_empty() {
        return Err(BambuError::ProtocolViolation(
            "Missing control action identifier".into(),
        ));
    }

    let action = action_args[0].to_lowercase();

    log::debug!("Running control subcommand action: '{}'", action);

    let mqtt = connect_mqtt(ip, serial, access_code).await?;
    let model = resolve_model(serial, None);
    let mut client = PrinterClient::new(mqtt, serial, model);

    match action.as_str() {
        "home" => {
            println!("Dispatching safe homing command macro...");
            client.home_axes(false).await?;
            println!("Homing command published successfully.");
        }
        "move" => {
            if action_args.len() < 4 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> move <axis> <distance> [feedrate]"
                        .into(),
                ));
            }
            let axis_char = action_args[1].chars().next().ok_or_else(|| {
                BambuError::ProtocolViolation(
                    format!("Invalid axis: '{}' (expected X, Y, or Z)", action_args[1]).into(),
                )
            })?;
            let distance = action_args[2].parse::<f32>().map_err(|_| {
                BambuError::ProtocolViolation(
                    format!("Invalid distance: '{}' (expected a number)", action_args[2]).into(),
                )
            })?;
            let feedrate = action_args
                .get(3)
                .and_then(|f| f.parse::<u32>().ok())
                .unwrap_or(3000);

            println!("Dispatching motion G-code G0 relative move...");
            client.move_relative(axis_char, distance, feedrate).await?;
            println!("Motion command published successfully.");
        }
        "extrude" => {
            if action_args.len() < 3 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> extrude <length> [feedrate]".into(),
                ));
            }
            let length = action_args[1].parse::<f32>().map_err(|_| {
                BambuError::ProtocolViolation(
                    format!("Invalid length: '{}' (expected a number)", action_args[1]).into(),
                )
            })?;
            let feedrate = action_args
                .get(2)
                .and_then(|f| f.parse::<u32>().ok())
                .unwrap_or(900);

            println!("Dispatching relative extrusion manual feed sequence...");
            client.extrude(length, feedrate).await?;
            println!("Extrusion command published successfully.");
        }
        "fan" => {
            if action_args.len() < 3 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> fan <target> <speed_percent>"
                        .into(),
                ));
            }
            let target = action_args[1].to_lowercase();
            let speed = action_args[2].parse::<u8>().map_err(|_| {
                BambuError::ProtocolViolation(
                    format!("Invalid speed: '{}' (expected 0-100)", action_args[2]).into(),
                )
            })?;

            let fan_target = match target.as_str() {
                "part" => FanTarget::PartCooling,
                "aux" => FanTarget::AuxiliaryLeft,
                "exhaust" => FanTarget::ChamberExhaust,
                "right" => FanTarget::AuxiliaryRight,
                _ => {
                    return Err(BambuError::ProtocolViolation(
                        "Invalid fan target. Choose 'part', 'aux', 'exhaust', or 'right'".into(),
                    ));
                }
            };

            println!("Configuring cooling fan PWM scale...");
            client.set_fan_speed(fan_target, speed).await?;
            println!("Fan control command published successfully.");
        }
        "temp" => {
            if action_args.len() < 3 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> temp <target> <value>".into(),
                ));
            }
            let target = action_args[1].to_lowercase();
            let val = action_args[2].parse::<u16>().map_err(|_| {
                BambuError::ProtocolViolation(
                    format!(
                        "Invalid temperature: '{}' (expected 0-65535)",
                        action_args[2]
                    )
                    .into(),
                )
            })?;

            match target.as_str() {
                "nozzle" => {
                    println!("Dispatching T0 hotend heater target...");
                    client.set_nozzle_temperature(0, val).await?;
                }
                "bed" => {
                    println!("Dispatching build-plate heater target...");
                    client.set_bed_temperature(val).await?;
                }
                "chamber" => {
                    println!("Dispatching chamber heating target...");
                    client.set_chamber_temperature(val).await?;
                }
                _ => {
                    return Err(BambuError::ProtocolViolation(
                        "Invalid thermal target. Choose 'nozzle', 'bed', or 'chamber'".into(),
                    ));
                }
            }
            println!("Thermal command published successfully.");
        }
        "led" => {
            if action_args.len() < 3 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> led <node> <on|off>".into(),
                ));
            }
            let node = action_args[1].to_lowercase();
            let state = action_args[2].to_lowercase();

            let led_node = match node.as_str() {
                "chamber" => "chamber_light",
                "work" => "work_light",
                other => other,
            };

            let turn_on = match state.as_str() {
                "on" => true,
                "off" => false,
                _ => {
                    return Err(BambuError::ProtocolViolation(
                        "Invalid LED switch. Choose 'on' or 'off'".into(),
                    ));
                }
            };

            println!("Dispatching ledctrl command register block...");
            client.toggle_led(led_node, turn_on).await?;
            println!("LED command published successfully.");
        }
        "pause" => {
            println!("Suspending print queue execution...");
            client.pause_print().await?;
        }
        "resume" => {
            println!("Resuming print queue execution...");
            client.resume_print().await?;
        }
        "stop" => {
            println!("Aborting active print job pipeline...");
            client.stop_print().await?;
        }
        "gcode" => {
            if action_args.len() < 2 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> gcode \"<gcode_line>\"".into(),
                ));
            }
            let gcode_line = &action_args[1];
            println!("Dispatching G-code (with safety checks)...");
            client.send_gcode(gcode_line).await?;
            println!("G-code command published successfully.");
        }
        "gcode-raw" => {
            if action_args.len() < 2 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> gcode-raw [--unsafe] \"<gcode_line>\""
                        .into(),
                ));
            }

            let (unsafe_flag, gcode_line) = if action_args[1] == "--unsafe" {
                if action_args.len() < 3 {
                    return Err(BambuError::ProtocolViolation(
                        "Usage: control <ip> <serial> <access_code> gcode-raw --unsafe \"<gcode_line>\""
                            .into(),
                    ));
                }
                (true, &action_args[2])
            } else {
                (false, &action_args[1])
            };

            if !unsafe_flag {
                eprint!(
                    "WARNING: gcode-raw bypasses all safety checks. \
                     Sending unsafe commands can damage your printer.\n\
                     Type 'yes' to confirm: "
                );
                io::stderr().flush().unwrap_or(());
                let mut confirmation = String::new();
                io::stdin().read_line(&mut confirmation).map_err(|_| {
                    BambuError::ProtocolViolation("Failed to read confirmation".into())
                })?;
                if confirmation.trim().to_lowercase() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            println!("Dispatching raw G-code (no safety checks)...");
            client.send_gcode_raw(gcode_line).await?;
            println!("Raw G-code command published successfully.");
        }
        other => {
            return Err(BambuError::ProtocolViolation(
                format!("Unrecognized control action identifier '{}'", other).into(),
            ));
        }
    }

    Ok(())
}
