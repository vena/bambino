#![cfg(feature = "std")]

//! # Real-Time Telemetry & Diagnostics Monitoring Subcommand
//!
//! Establishes a secure connection to MQTTS Port 8883 [REF-MQTT-CONN],
//! initiates a state dump, and continuously processes state updates [REF-MQTT-ENV].
//!
//! Employs `tokio::select!` to multiplex between incoming telemetry updates
//! and outbound keep-alive PING frames, rendering a live terminal dashboard.

use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::interval;

use bambu_lan::diagnostics::{decode_hms_alert, decode_print_error};
use bambu_lan::error::BambuError;
use bambu_lan::io::tokio::{
    build_unsafe_client_config, to_socket_error, TokioTimer, TokioTlsConnector,
};
use bambu_lan::io::{TlsConnector, TokioIo};
use bambu_lan::mqtt::{BambuMqttClient, PushAllRequest};
use bambu_lan::types::PrintTelemetry;

/// Establishes the secure MQTTS session, sends `pushall`, and runs the dashboard loop.
pub async fn run(ip: &str, serial: &str, access_code: &str) -> Result<(), BambuError> {
    println!("Connecting to secure MQTT broker at {}:8883...", ip);

    // 1. Establish the underlying TCP stream and wrap in secure TLS context [REF-NET-SECURE]
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

    // 2. Perform the MQTT v3.1.1 protocol handshake
    let mut mqtt =
        BambuMqttClient::connect::<TokioTimer>(secure_stream, serial, access_code).await?;
    println!("MQTT Connection successfully established. Querying status database...");

    // 3. Command the printer to dump its initial state machine values
    let seq_id = 10001;
    let push_req = PushAllRequest::new(seq_id);
    let push_payload = serde_json::to_vec(&push_req).map_err(|_| BambuError::SerializationError)?;
    mqtt.publish_command(&push_payload).await?;

    // 4. Configure our keep-alive timer loop (Ping interval set to 15 seconds)
    let mut ping_timer = interval(Duration::from_secs(15));
    // Skip the first immediate tick
    ping_timer.tick().await;

    println!("Monitoring active. Press Ctrl+C to terminate.\n");

    loop {
        tokio::select! {
            // A: Listen for incoming telemetry payloads
            telemetry_res = mqtt.poll_telemetry() => {
                match telemetry_res {
                    Ok(msg) => {
                        if let Err(e) = render_dashboard(&msg.payload) {
                            eprintln!("Warning: Failed to render telemetry updates: {:?}", e);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }

            // B: Periodically send keep-alives to prevent zombie TCP dropouts [REF-MQTT-ZOMBIE]
            _ = ping_timer.tick() => {
                if let Err(e) = mqtt.send_ping().await {
                    eprintln!("Warning: Failed to dispatch keep-alive ping: {:?}", e);
                }
            }
        }
    }
}

/// Parses the raw JSON report payload and draws a clean status dashboard in the console.
fn render_dashboard(payload: &[u8]) -> Result<(), serde_json::Error> {
    let v: serde_json::Value = serde_json::from_slice(payload)?;

    // Telemetry updates often contain partial segments. Only refresh terminal output
    // when we receive structural print state parameters.
    let print_obj = match v.get("print") {
        Some(p) if p.is_object() => p,
        _ => return Ok(()),
    };

    let gcode_state = print_obj
        .get("gcode_state")
        .and_then(|s| s.as_str())
        .unwrap_or("UNKNOWN");
    let subtask_name = print_obj
        .get("subtask_name")
        .and_then(|s| s.as_str())
        .unwrap_or("None");
    let progress = print_obj
        .get("progress")
        .and_then(|p| p.as_f64())
        .unwrap_or(0.0);
    let layer_num = print_obj
        .get("layer_num")
        .and_then(|l| l.as_i64())
        .unwrap_or(0);
    let total_layers = print_obj
        .get("total_layers")
        .and_then(|l| l.as_i64())
        .unwrap_or(0);
    let remaining_sec = print_obj
        .get("mc_remaining_time")
        .and_then(|t| t.as_i64())
        .unwrap_or(0);

    // Unpack temperatures safely using our composite helpers [REF-THER-DECODE]
    let nozzle_temper = print_obj
        .get("nozzle_temper")
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    let (nozzle_act, nozzle_tgt) = PrintTelemetry::unpack_temperature(nozzle_temper);

    let bed_temper = print_obj
        .get("bed_temper")
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    let (bed_act, bed_tgt) = PrintTelemetry::unpack_temperature(bed_temper);

    let chamber_temper = print_obj
        .get("chamber_temper")
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    let (chamber_act, chamber_tgt) = PrintTelemetry::unpack_temperature(chamber_temper);

    // Format remaining time nicely
    let remaining_formatted = if remaining_sec > 0 {
        format!("{}m {}s", remaining_sec / 60, remaining_sec % 60)
    } else {
        String::from("Unknown")
    };

    // Cleanly clear terminal cursor lines to simulate live in-place redraws
    print!("\x1B[2J\x1B[1;1H");

    println!("=================== Bambu Lab Printer Live Dashboard ===================");
    println!("{:<20} : {}", "Operational State", gcode_state);
    println!("{:<20} : {}", "Active Job Name", subtask_name);
    println!("{:<20} : {:.1}%", "Print Progress", progress);
    println!("{:<20} : {} / {}", "Layer Range", layer_num, total_layers);
    println!("{:<20} : {}", "Time Remaining", remaining_formatted);
    println!("------------------------------------------------------------------------");
    println!(
        "{:<20} : Actual: {:>3}°C  |  Target: {:>3}°C",
        "Hotend Temp", nozzle_act, nozzle_tgt
    );
    println!(
        "{:<20} : Actual: {:>3}°C  |  Target: {:>3}°C",
        "Heated Bed Temp", bed_act, bed_tgt
    );
    println!(
        "{:<20} : Actual: {:>3}°C  |  Target: {:>3}°C",
        "Chamber Temp", chamber_act, chamber_tgt
    );
    println!("========================================================================");

    // 5. Unpack and display system hardware errors dynamically [REF-DIAG-HMS]
    if let Some(err_val) = print_obj.get("print_error").and_then(|e| e.as_u64()) {
        if let Some(decoded_err) = decode_print_error(err_val as u32) {
            if decoded_err.is_genuine_fault {
                println!(
                    "\x1B[1;31m[ACTIVE ERROR] State Code: {}\x1B[0m",
                    decoded_err.short_code
                );
            }
        }
    }

    if let Some(hms_array) = print_obj.get("hms").and_then(|h| h.as_array()) {
        let mut active_hms = Vec::new();
        for alert in hms_array {
            if let (Some(attr), Some(code)) = (
                alert.get("attr").and_then(|a| a.as_u64()),
                alert.get("code").and_then(|c| c.as_u64()),
            ) {
                let decoded = decode_hms_alert(attr as u32, code as u32);
                if decoded.is_genuine_fault {
                    active_hms.push(decoded);
                }
            }
        }

        if !active_hms.is_empty() {
            println!("\nActive Hardware Alerts:");
            for decoded in active_hms {
                println!(
                    "  \x1B[1;33m- [{}] Severity: {:?} (Module: {})\x1B[0m",
                    decoded.short_code, decoded.severity, decoded.module_id
                );
            }
        }
    }

    Ok(())
}
