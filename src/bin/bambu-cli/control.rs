#![cfg(feature = "std")]

//! # Motion, Thermal, and Peripheral Control Subcommand
//!
//! Handles dispatching manual commands to the printer motion controller
//! and querying hardware modules from the expansion bus [REF-MOTO-GCODE].

use std::time::Duration;
use tokio::net::TcpStream;

use bambu_lan::client::{FanTarget, PrinterClient};
use bambu_lan::discovery::resolve_model;
use bambu_lan::error::BambuError;
use bambu_lan::io::tokio::{
    build_unsafe_client_config, to_socket_error, TokioTimer, TokioTlsConnector,
};
use bambu_lan::io::{TlsConnector, TokioIo};
use bambu_lan::mqtt::{BambuMqttClient, GetVersionRequest};

/// Utility to connect and return a configured MQTT client wrapper.
async fn connect_mqtt(
    ip: &str,
    serial: &str,
    access_code: &str,
) -> Result<
    BambuMqttClient<<TokioTlsConnector as TlsConnector<TokioIo<TcpStream>>>::Stream>,
    BambuError,
> {
    let config = build_unsafe_client_config();
    let connector = tokio_rustls::TlsConnector::from(config);
    let tls_connector = TokioTlsConnector::new(connector);

    let tcp_stream = TcpStream::connect(format!("{}:8883", ip))
        .await
        .map_err(to_socket_error)
        .map_err(BambuError::NetworkError)?;
    let raw_io = TokioIo(tcp_stream);

    let secure_stream = tls_connector
        .connect(ip, 8883, raw_io)
        .await
        .map_err(BambuError::NetworkError)?;

    BambuMqttClient::connect::<TokioTimer>(secure_stream, serial, access_code).await
}

/// Connects to the printer, sends a `get_version` command, and displays expansion bus modules.
///
/// **Query Capability Check [REF-DIAG-HMS]:**
/// Verifies if the target printer model supports get_version commands using the model quirks engine.
/// If unsupported (such as on the P1 or A1 series), prints an informational notice and exits cleanly
/// without attempting to send the payload.
pub async fn run_info(ip: &str, serial: &str, access_code: &str) -> Result<(), BambuError> {
    let model = resolve_model(serial, None);

    if model.quirks().is_unsupported_command("get_version") {
        println!("\n\x1B[1;33mNotice: Version query is unsupported on this printer model.\x1B[0m");
        println!("Note: Lightweight printer models (such as the P1 and A1 series) do not");
        println!("support or reply to expansion bus 'get_version' queries over LAN mode.\n");
        return Ok(());
    }

    let mut mqtt = connect_mqtt(ip, serial, access_code).await?;

    println!("Querying expansion bus version database...");
    let req = GetVersionRequest::new(10002);
    let payload = serde_json::to_vec(&req).map_err(|_| BambuError::SerializationError)?;
    mqtt.publish_command(&payload).await?;

    let poll_future = async {
        loop {
            let msg = mqtt.poll_telemetry().await?;
            let v: serde_json::Value =
                serde_json::from_slice(&msg.payload).unwrap_or(serde_json::Value::Null);

            if v.get("command").and_then(|c| c.as_str()) == Some("get_version") {
                if let Some(modules) = v.get("module").and_then(|m| m.as_array()) {
                    return Ok::<_, BambuError>(modules.clone());
                }
            }
        }
    };

    match tokio::time::timeout(Duration::from_secs(5), poll_future).await {
        Ok(Ok(modules)) => {
            println!("\nDetected Expansion Bus Modules & Versions:");
            println!("{:=<75}", "");
            println!(
                "{:<15} | {:<18} | {:<30}",
                "Module Name", "Software Version", "Hardware Serial"
            );
            println!("{:=<75}", "");

            for m in modules {
                let name = m.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                let sw_ver = m
                    .get("sw_ver")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                let sn = m.get("sn").and_then(|s| s.as_str()).unwrap_or("N/A");
                println!("{:<15} | {:<18} | {:<30}", name, sw_ver, sn);
            }
            println!("{:=<75}\n", "");
        }
        Ok(Err(e)) => return Err(e),
        Err(_) => {
            println!("\n\x1B[1;33mNotice: Version query timed out after 5 seconds.\x1B[0m");
            println!("Note: Some lightweight printer models (such as the P1 and A1 series) do not");
            println!("support or reply to expansion bus 'get_version' queries over LAN mode.\n");
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
            "Missing control action identifier",
        ));
    }

    let action = action_args[0].to_lowercase();
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
                    "Usage: control <ip> <serial> <access_code> move <axis> <distance> [feedrate]",
                ));
            }
            let axis_char = action_args[1]
                .chars()
                .next()
                .ok_or(BambuError::ProtocolViolation("Invalid axis"))?;
            let distance = action_args[2]
                .parse::<f32>()
                .map_err(|_| BambuError::ProtocolViolation("Invalid distance format"))?;
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
                    "Usage: control <ip> <serial> <access_code> extrude <length> [feedrate]",
                ));
            }
            let length = action_args[1]
                .parse::<f32>()
                .map_err(|_| BambuError::ProtocolViolation("Invalid length format"))?;
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
                    "Usage: control <ip> <serial> <access_code> fan <target> <speed_percent>",
                ));
            }
            let target = action_args[1].to_lowercase();
            let speed = action_args[2]
                .parse::<u8>()
                .map_err(|_| BambuError::ProtocolViolation("Invalid speed percent"))?;

            let fan_target = match target.as_str() {
                "part" => FanTarget::PartCooling,
                "aux" => FanTarget::AuxiliaryLeft,
                "exhaust" => FanTarget::ChamberExhaust,
                _ => {
                    return Err(BambuError::ProtocolViolation(
                        "Invalid fan target. Choose 'part', 'aux', or 'exhaust'",
                    ))
                }
            };

            println!("Configuring cooling fan PWM scale...");
            client.set_fan_speed(fan_target, speed).await?;
            println!("Fan control command published successfully.");
        }
        "temp" => {
            if action_args.len() < 3 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> temp <target> <value>",
                ));
            }
            let target = action_args[1].to_lowercase();
            let val = action_args[2]
                .parse::<u16>()
                .map_err(|_| BambuError::ProtocolViolation("Invalid temperature target value"))?;

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
                        "Invalid thermal target. Choose 'nozzle', 'bed', or 'chamber'",
                    ))
                }
            }
            println!("Thermal command published successfully.");
        }
        "led" => {
            if action_args.len() < 3 {
                return Err(BambuError::ProtocolViolation(
                    "Usage: control <ip> <serial> <access_code> led <node> <on|off>",
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
                        "Invalid LED switch. Choose 'on' or 'off'",
                    ))
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
        other => {
            return Err(BambuError::ProtocolViolationDynamic(format!(
                "Unrecognized control action identifier '{}'",
                other
            )));
        }
    }

    Ok(())
}
