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

use bambino::diagnostics::{decode_hms_alert, decode_print_error};
use bambino::discovery::resolve_model;
use bambino::error::BambuError;
use bambino::io::tokio::{
    build_unsafe_client_config, to_socket_error, TokioTimer, TokioTlsConnector,
};
use bambino::io::{TlsConnector, TokioIo};
use bambino::mqtt::{BambuMqttClient, PushAllRequest};
use bambino::quirks::ModelQuirks;
use bambino::types::PrintTelemetry;

/// Connects, sends `pushall`, and dumps the first response containing a `print` object as pretty JSON.
pub async fn dump(ip: &str, serial: &str, access_code: &str) -> Result<(), BambuError> {
    eprintln!("Connecting to {}:8883 for raw telemetry dump...", ip);

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

    let mut mqtt =
        BambuMqttClient::connect::<TokioTimer>(secure_stream, serial, access_code).await?;

    let push_req = PushAllRequest::new(10001);
    let push_payload = serde_json::to_vec(&push_req).map_err(|_| BambuError::SerializationError)?;
    mqtt.publish_command(&push_payload).await?;

    // Collect messages until we get one with a "print" object containing "gcode_state"
    let timeout = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            res = mqtt.poll_telemetry() => {
                let msg = res?;
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&msg.payload) {
                    if v.get("print").and_then(|p| p.get("gcode_state")).is_some() {
                        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                        return Ok(());
                    }
                }
            }
            _ = &mut timeout => {
                eprintln!("Timed out waiting for pushall response.");
                return Ok(());
            }
        }
    }
}

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

    let model = resolve_model(serial, None);
    let quirks = model.quirks();
    println!(
        "Monitoring active ({}). Press Ctrl+C to terminate.\n",
        serial
    );

    let mut state = serde_json::Map::new();

    loop {
        tokio::select! {
            // A: Listen for incoming telemetry payloads
            telemetry_res = mqtt.poll_telemetry() => {
                match telemetry_res {
                    Ok(msg) => {
                        if let Err(e) = render_dashboard(&msg.payload, &mut state, quirks) {
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

/// Merges a partial telemetry update into accumulated state and redraws the dashboard.
fn render_dashboard(
    payload: &[u8],
    state: &mut serde_json::Map<String, serde_json::Value>,
    quirks: &dyn ModelQuirks,
) -> Result<(), serde_json::Error> {
    let v: serde_json::Value = serde_json::from_slice(payload)?;

    let mut had_update = false;

    if let Some(serde_json::Value::Object(print_obj)) = v.get("print") {
        for (key, value) in print_obj {
            state.insert(key.clone(), value.clone());
        }
        had_update = true;
    }

    if let Some(device_obj) = v.get("device") {
        state.insert("_device".to_string(), device_obj.clone());
        had_update = true;
    }

    if !had_update {
        return Ok(());
    }

    print!("\x1B[2J\x1B[1;1H");

    // -- Print Status --
    let gcode_state = state
        .get("gcode_state")
        .and_then(|s| s.as_str())
        .unwrap_or("UNKNOWN");
    let subtask_name = state
        .get("subtask_name")
        .and_then(|s| s.as_str())
        .unwrap_or("None");
    let progress = state
        .get("progress")
        .and_then(|p| p.as_f64())
        .unwrap_or(0.0);
    let layer_num = state.get("layer_num").and_then(|l| l.as_i64()).unwrap_or(0);
    let total_layers = state
        .get("total_layers")
        .and_then(|l| l.as_i64())
        .unwrap_or(0);
    let remaining_sec = state
        .get("mc_remaining_time")
        .and_then(|t| t.as_i64())
        .unwrap_or(0);

    let remaining_formatted = if remaining_sec > 0 {
        format!("{}m {}s", remaining_sec / 60, remaining_sec % 60)
    } else {
        String::from("--")
    };

    println!("================== Bambu Lab Printer Live Dashboard ===================");
    println!("{:<20} : {}", "Operational State", gcode_state);
    println!("{:<20} : {}", "Active Job Name", subtask_name);
    println!(
        "{:<20} : {:.1}%  ({}/{})",
        "Print Progress", progress, layer_num, total_layers
    );
    println!("{:<20} : {}", "Time Remaining", remaining_formatted);

    // -- Nozzles --
    // Build a unified nozzle list: prefer device.nozzle.info[], backfill from print-level fields.
    struct NozzleEntry {
        id: u64,
        diameter: String,
        ntype: String,
        temp: String,
    }

    let mut nozzles: Vec<NozzleEntry> = Vec::new();

    if let Some(device_nozzles) = state
        .get("_device")
        .and_then(|d| d.get("nozzle"))
        .and_then(|n| n.get("info"))
        .and_then(|i| i.as_array())
    {
        for n in device_nozzles {
            let id = n.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
            if id >= 16 {
                continue;
            }
            let diameter = n
                .get("diameter")
                .and_then(|d| d.as_f64())
                .map(|d| format!("{:.1}mm", d))
                .unwrap_or_else(|| "--".to_string());
            let ntype = n
                .get("type")
                .or_else(|| n.get("nozzle_type"))
                .and_then(|t| t.as_str())
                .unwrap_or("--")
                .to_string();
            nozzles.push(NozzleEntry {
                id,
                diameter,
                ntype,
                temp: String::new(),
            });
        }
    }

    // Backfill from print-level fields if device data hasn't arrived
    if nozzles.is_empty() {
        let diameter = state
            .get("nozzle_diameter")
            .and_then(|s| s.as_str())
            .unwrap_or("--")
            .to_string();
        let ntype = state
            .get("nozzle_type")
            .and_then(|s| s.as_str())
            .unwrap_or("--")
            .to_string();
        nozzles.push(NozzleEntry {
            id: 0,
            diameter: format!("{}mm", diameter),
            ntype,
            temp: String::new(),
        });
    }

    // Fill in temperatures from print-level fields
    let nozzle_temper = state
        .get("nozzle_temper")
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    let nozzle_target = state
        .get("nozzle_target_temper")
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    let (nozzle_act, _) = PrintTelemetry::unpack_temperature(nozzle_temper);
    let (_, nozzle_tgt) = PrintTelemetry::unpack_temperature(nozzle_target);

    if nozzles.len() == 1 {
        nozzles[0].temp = format!("{}°C / T: {}°C", nozzle_act, nozzle_tgt);
    } else if nozzles.len() >= 2 {
        // IDEX: nozzle_temper = left(#1) actual, nozzle_target_temper = right(#0) target
        if let Some(right) = nozzles.iter_mut().find(|n| n.id == 0) {
            right.temp = format!("target: {}°C", nozzle_tgt);
        }
        if let Some(left) = nozzles.iter_mut().find(|n| n.id == 1) {
            left.temp = format!("{}°C", nozzle_act);
        }
    }

    println!("\n--- Nozzles -----------------------------------------------------------");
    for row in nozzles.chunks(2) {
        let mut cols: Vec<String> = Vec::new();
        for n in row {
            if n.temp.is_empty() {
                cols.push(format!("#{}: {} {}", n.id, n.diameter, n.ntype));
            } else {
                cols.push(format!(
                    "#{}: {} {} ({})",
                    n.id, n.diameter, n.ntype, n.temp
                ));
            }
        }
        if cols.len() == 2 {
            println!("{:<34} │ {}", cols[0], cols[1]);
        } else {
            println!("{}", cols[0]);
        }
    }

    // -- Thermal (bed + chamber) --
    let bed_temper = state
        .get("bed_temper")
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    let (bed_act, bed_tgt) = PrintTelemetry::unpack_temperature(bed_temper);

    println!("\n--- Thermal -----------------------------------------------------------");
    println!(
        "{:<10} : {}°C / T: {}°C",
        "Heated Bed", bed_act, bed_tgt
    );

    if !quirks.ignores_chamber_temperature() {
        let chamber_temper = state
            .get("chamber_temper")
            .and_then(|t| t.as_u64())
            .unwrap_or(0) as u32;
        let (chamber_act, chamber_tgt) = PrintTelemetry::unpack_temperature(chamber_temper);
        println!(
            "{:<20} : {:>3}°C / {:>3}°C",
            "Chamber", chamber_act, chamber_tgt
        );
    }

    // -- Fans & System (two-column layout) --
    let fan_values = [
        ("Part Cooling", get_fan_pct(state, "cooling_fan_speed")),
        ("Aux Fan", get_fan_pct(state, "big_fan1_speed")),
        ("Chamber Fan", get_fan_pct(state, "big_fan2_speed")),
        ("Heatbreak Fan", get_fan_pct(state, "heatbreak_fan_speed")),
    ];

    let wifi = state
        .get("wifi_signal")
        .and_then(|s| s.as_str())
        .unwrap_or("--");
    let sdcard = match state.get("sdcard") {
        Some(serde_json::Value::Bool(true)) => "Inserted",
        Some(serde_json::Value::String(s)) if s.to_uppercase() == "HAS_SDCARD_NORMAL" => "Inserted",
        Some(serde_json::Value::Number(n)) if n.as_i64().unwrap_or(0) != 0 => "Inserted",
        Some(serde_json::Value::Bool(false)) | Some(serde_json::Value::Number(_)) => "Not Detected",
        _ => "--",
    };
    let recording = state
        .get("ipcam_record")
        .and_then(|s| s.as_str())
        .unwrap_or("--");
    let timelapse = state
        .get("timelapse")
        .and_then(|s| s.as_str())
        .unwrap_or("--");

    let sys_values = [
        ("WiFi", wifi),
        ("SD Card", sdcard),
        ("Recording", recording),
        ("Timelapse", timelapse),
    ];

    println!("\n--- Fans & System -----------------------------------------------------");
    for i in 0..4 {
        println!(
            "{:<14} : {:<6} {:>3} {:<14} : {}",
            fan_values[i].0, fan_values[i].1, "│", sys_values[i].0, sys_values[i].1
        );
    }

    // -- AMS --
    if let Some(ams_array) = state
        .get("ams")
        .and_then(|a| a.get("ams"))
        .and_then(|a| a.as_array())
    {
        for unit in ams_array {
            let unit_id = json_as_str_or_num(unit.get("id"));
            let temp = unit.get("temp").and_then(|t| t.as_str()).unwrap_or("--");
            let humidity = json_as_parsed_u64(unit.get("humidity_raw"))
                .map(|h| format!("{}%", h))
                .unwrap_or_else(|| {
                    unit.get("humidity")
                        .and_then(|h| h.as_str())
                        .map(|s| format!("idx:{}", s))
                        .unwrap_or_else(|| "--".to_string())
                });

            let dry_suffix = match json_as_parsed_u64(unit.get("dry_time")) {
                Some(mins) if mins > 0 => {
                    let dry_temp = unit
                        .get("dry_setting")
                        .and_then(|ds| ds.get("dry_temperature"))
                        .and_then(|t| t.as_i64())
                        .filter(|t| *t > 0);
                    match dry_temp {
                        Some(t) => format!(" Drying: {}:{:02}@{}°C", mins / 60, mins % 60, t),
                        None => format!(" Drying: {}:{:02} left", mins / 60, mins % 60),
                    }
                }
                _ => String::new(),
            };

            let header = format!(
                "\n--- AMS #{} ({}°C, RH:{}){}",
                unit_id, temp, humidity, dry_suffix
            );
            let pad = 71usize.saturating_sub(header.len() - 1);
            println!("{} {}", header, "-".repeat(pad));

            if let Some(trays) = unit.get("tray").and_then(|t| t.as_array()) {
                let mut table =
                    crate::table::Table::new(vec!["Slot", "Status", "Material", "Remaining"]);

                for tray in trays {
                    let tray_id = json_as_str_or_num(tray.get("id"));

                    let tray_state = tray.get("state").and_then(|s| s.as_u64()).map(|s| s as u8);
                    let status = match tray_state {
                        Some(11) => "Loaded",
                        Some(10) => "Present",
                        Some(9) | Some(0) | None => "Empty",
                        _ => "Unknown",
                    };

                    let material = tray.get("tray_type").and_then(|t| t.as_str()).unwrap_or("");

                    let remain = tray
                        .get("remain")
                        .and_then(|r| r.as_i64())
                        .filter(|r| *r >= 0)
                        .map(|r| format!("{}%", r))
                        .unwrap_or_default();

                    table.add_row(vec![&tray_id, status, material, &remain]);
                }

                table.print();
            }
        }
    }

    // -- External Spool --
    if let Some(vt) = state.get("vt_tray") {
        let tray_type = vt.get("tray_type").and_then(|t| t.as_str()).unwrap_or("");
        if !tray_type.is_empty() {
            let tray_color = vt.get("tray_color").and_then(|c| c.as_str()).unwrap_or("");
            let nozzle_temp = vt
                .get("nozzle_temp_max")
                .and_then(|t| t.as_str())
                .unwrap_or("--");
            let color_swatch = format_color_swatch(tray_color);
            println!("\n--- External Spool ----------------------------------------------------");
            println!(
                "{:<20} : {} {} (max {}°C)",
                "Material", tray_type, color_swatch, nozzle_temp
            );
        }
    }

    println!("=======================================================================");

    // -- Diagnostics --
    if let Some(err_val) = state.get("print_error").and_then(|e| e.as_u64()) {
        if let Some(decoded_err) = decode_print_error(err_val as u32) {
            if decoded_err.is_genuine_fault {
                println!(
                    "\x1B[1;31m[ACTIVE ERROR] Code: {}\x1B[0m",
                    decoded_err.short_code
                );
            }
        }
    }

    if let Some(hms_array) = state.get("hms").and_then(|h| h.as_array()) {
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
            println!("Active Hardware Alerts:");
            for decoded in &active_hms {
                println!(
                    "  \x1B[1;33m[{}] Severity: {:?} (Module: {})\x1B[0m",
                    decoded.short_code, decoded.severity, decoded.module_id
                );
            }
        }
    }

    Ok(())
}

/// Extracts a fan speed field as a percentage string (0-15 step scale).
fn get_fan_pct(state: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
    state
        .get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u32>().ok())
        .map(|step| format!("{}%", (step as f32 / 15.0 * 100.0).round() as u32))
        .unwrap_or_else(|| "--".to_string())
}

/// Extracts a JSON value as a display string, handling both string and numeric types.
fn json_as_str_or_num(val: Option<&serde_json::Value>) -> String {
    match val {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => "?".to_string(),
    }
}

/// Parses a JSON value as u64, accepting both numeric and string-encoded integers.
fn json_as_parsed_u64(val: Option<&serde_json::Value>) -> Option<u64> {
    match val {
        Some(serde_json::Value::Number(n)) => n.as_u64(),
        Some(serde_json::Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// Converts an RRGGBBAA hex color string into an ANSI true-color swatch.
fn format_color_swatch(hex_color: &str) -> String {
    if hex_color.len() < 6 {
        return String::new();
    }
    let r = u8::from_str_radix(&hex_color[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex_color[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex_color[4..6], 16).unwrap_or(0);
    format!("\x1B[48;2;{};{};{}m  \x1B[0m", r, g, b)
}
