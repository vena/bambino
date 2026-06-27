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

use bambino::client::{CalibrationOption, FanTarget, PrintSpeed, PrinterClient};
use bambino::error::BambuError;
use bambino::models::resolve_model;
use bambino::mqtt::AirductMode;

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
        "speed" => {
            if action_args.len() < 2 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> speed <silent|standard|sport|ludicrous>"
                        .into(),
                ));
            }
            let level = match action_args[1].to_lowercase().as_str() {
                "silent" => PrintSpeed::Silent,
                "standard" => PrintSpeed::Standard,
                "sport" => PrintSpeed::Sport,
                "ludicrous" => PrintSpeed::Ludicrous,
                _ => {
                    return Err(BambuError::ProtocolViolation(
                        "Invalid speed level. Choose 'silent', 'standard', 'sport', or 'ludicrous'"
                            .into(),
                    ));
                }
            };

            println!(
                "Setting print speed to {}...",
                action_args[1].to_lowercase()
            );
            client.set_print_speed(level).await?;
            println!("Print speed command published successfully.");
        }
        "clear-error" => {
            println!("Clearing active print error codes...");
            client.clear_print_error().await?;
            println!("Clear error command published successfully.");
        }
        "airduct" => {
            if action_args.len() < 2 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> airduct <cooling|heating|laser>"
                        .into(),
                ));
            }
            let mode = match action_args[1].to_lowercase().as_str() {
                "cooling" => AirductMode::Cooling,
                "heating" => AirductMode::Heating,
                "laser" => AirductMode::Laser,
                _ => {
                    return Err(BambuError::ProtocolViolation(
                        "Invalid airduct mode. Choose 'cooling', 'heating', or 'laser'".into(),
                    ));
                }
            };

            println!(
                "Switching airduct damper to {} mode...",
                action_args[1].to_lowercase()
            );
            client.set_airduct_mode(mode).await?;
            println!("Airduct command published successfully.");
        }
        "calibrate" => {
            if action_args.len() < 2 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> calibrate <routine> [routine...]\n  \
                     Routines: bed-leveling, vibration, motor-noise, nozzle-height, heatbed-thermal"
                        .into(),
                ));
            }

            let mut options = CalibrationOption(0);
            for arg in &action_args[1..] {
                let flag = match arg.to_lowercase().as_str() {
                    "bed-leveling" => CalibrationOption::BED_LEVELING,
                    "vibration" => CalibrationOption::VIBRATION_COMPENSATION,
                    "motor-noise" => CalibrationOption::MOTOR_NOISE_CANCELLATION,
                    "nozzle-height" => CalibrationOption::NOZZLE_HEIGHT,
                    "heatbed-thermal" => CalibrationOption::HEATBED_THERMAL,
                    other => {
                        return Err(BambuError::ProtocolViolation(
                            format!(
                                "Unknown calibration routine '{}'. Choose from: \
                                 bed-leveling, vibration, motor-noise, nozzle-height, heatbed-thermal",
                                other
                            )
                            .into(),
                        ));
                    }
                };
                options = options | flag;
            }

            println!("Triggering calibration routines...");
            client.start_calibration(options).await?;
            println!("Calibration command published successfully.");
        }
        "ams" => {
            if action_args.len() < 2 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> ams <dry|dry-stop> [ARGS]".into(),
                ));
            }
            let ams_action = action_args[1].to_lowercase();
            match ams_action.as_str() {
                "dry" => {
                    if action_args.len() < 7 {
                        return Err(BambuError::ProtocolViolation(
                            "Usage: control <ip> <serial> <access_code> ams dry <ams_id> <temp> <time_min> <rotate_tray> <filament>"
                                .into(),
                        ));
                    }
                    let ams_id = action_args[2].parse::<i32>().map_err(|_| {
                        BambuError::ProtocolViolation(
                            format!("Invalid AMS ID: '{}' (expected integer)", action_args[2])
                                .into(),
                        )
                    })?;
                    let temp = action_args[3].parse::<u32>().map_err(|_| {
                        BambuError::ProtocolViolation(
                            format!(
                                "Invalid temperature: '{}' (expected integer)",
                                action_args[3]
                            )
                            .into(),
                        )
                    })?;
                    let time = action_args[4].parse::<u32>().map_err(|_| {
                        BambuError::ProtocolViolation(
                            format!("Invalid time: '{}' (expected minutes)", action_args[4]).into(),
                        )
                    })?;
                    let rotate = match action_args[5].to_lowercase().as_str() {
                        "true" | "yes" | "1" => true,
                        "false" | "no" | "0" => false,
                        _ => {
                            return Err(BambuError::ProtocolViolation(
                                "Invalid rotate_tray value. Choose 'true' or 'false'".into(),
                            ));
                        }
                    };
                    let filament = &action_args[6];

                    println!(
                        "Starting AMS {} drying cycle at {}°C for {} minutes...",
                        ams_id, temp, time
                    );
                    client
                        .start_drying(ams_id, temp, time, rotate, filament)
                        .await?;
                    println!("AMS drying command published successfully.");
                }
                "dry-stop" => {
                    if action_args.len() < 3 {
                        return Err(BambuError::ProtocolViolation(
                            "Usage: control <ip> <serial> <access_code> ams dry-stop <ams_id>"
                                .into(),
                        ));
                    }
                    let ams_id = action_args[2].parse::<i32>().map_err(|_| {
                        BambuError::ProtocolViolation(
                            format!("Invalid AMS ID: '{}' (expected integer)", action_args[2])
                                .into(),
                        )
                    })?;

                    println!("Stopping AMS {} drying cycle...", ams_id);
                    client.stop_drying(ams_id).await?;
                    println!("AMS drying stop command published successfully.");
                }
                other => {
                    return Err(BambuError::ProtocolViolation(
                        format!("Unknown AMS action '{}'. Choose 'dry' or 'dry-stop'", other)
                            .into(),
                    ));
                }
            }
        }
        other => {
            return Err(BambuError::ProtocolViolation(
                format!("Unrecognized control action identifier '{}'", other).into(),
            ));
        }
    }

    Ok(())
}
