#![cfg(feature = "std")]

use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;
use tokio::sync::mpsc;
use tokio::time::interval;

use bambino::diagnostics::{decode_hms_alert, decode_print_error};
use bambino::error::BambuError;
use bambino::models::resolve_model;
use bambino::mqtt::PushAllRequest;
use bambino::quirks::ModelQuirks;
use bambino::types::PrinterTelemetry;

use crate::connection::connect_mqtt;

/// Write adapter that translates `\n` to `\r\n` for raw-mode terminal output.
struct RawWriter<W: Write>(W);

impl<W: Write> Write for RawWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut last = 0;
        for (i, &byte) in buf.iter().enumerate() {
            if byte == b'\n' {
                if i > last {
                    self.0.write_all(&buf[last..i])?;
                }
                self.0.write_all(b"\r\n")?;
                last = i + 1;
            }
        }
        if last < buf.len() {
            self.0.write_all(&buf[last..])?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Connects, sends `pushall`, and dumps the first response containing a `print` object as pretty JSON.
pub async fn dump(ip: &str, serial: &str, access_code: &str) -> Result<(), BambuError> {
    eprintln!("Connecting to {}:8883 for raw telemetry dump...", ip);

    let mut mqtt = connect_mqtt(ip, serial, access_code).await?;

    let push_req = PushAllRequest::new(10001);
    let push_payload = serde_json::to_vec(&push_req).map_err(|_| BambuError::SerializationError)?;
    mqtt.publish_command(&push_payload).await?;

    let timeout = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            res = mqtt.poll_telemetry() => {
                let msg = res?;
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&msg.payload)
                    && v.get("print").and_then(|p| p.get("gcode_state")).is_some()
                {
                    println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                    return Ok(());
                }
            }
            _ = &mut timeout => {
                eprintln!("Timed out waiting for pushall response.");
                return Ok(());
            }
        }
    }
}

/// RAII guard that restores terminal state on drop.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        write!(stdout, "\x1B[?1049h\x1B[?25l")?;
        stdout.flush()?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = write!(stdout, "\x1B[?25h\x1B[?1049l");
        let _ = stdout.flush();
        let _ = terminal::disable_raw_mode();
    }
}

/// Establishes the secure MQTTS session, sends `pushall`, and runs the dashboard loop.
pub async fn run(ip: &str, serial: &str, access_code: &str) -> Result<(), BambuError> {
    eprintln!("Connecting to secure MQTT broker at {}:8883...", ip);

    let mut mqtt = connect_mqtt(ip, serial, access_code).await?;
    eprintln!("MQTT Connection successfully established. Querying status database...");

    let seq_id = 10001;
    let push_req = PushAllRequest::new(seq_id);
    let push_payload = serde_json::to_vec(&push_req).map_err(|_| BambuError::SerializationError)?;
    mqtt.publish_command(&push_payload).await?;

    let mut ping_timer = interval(Duration::from_secs(15));
    ping_timer.tick().await;

    let model = resolve_model(serial, None);
    let quirks = model.quirks();

    let _guard = TerminalGuard::enter().map_err(|_| {
        BambuError::ProtocolViolation("failed to initialize terminal raw mode".into())
    })?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_flag = shutdown.clone();
    let (key_tx, mut key_rx) = mpsc::channel::<KeyEvent>(4);
    tokio::task::spawn_blocking(move || {
        while !shutdown_flag.load(Ordering::Relaxed) {
            if event::poll(Duration::from_millis(50)).unwrap_or(false)
                && let Ok(Event::Key(key)) = event::read()
                && key_tx.blocking_send(key).is_err()
            {
                break;
            }
        }
    });

    let mut state = serde_json::Map::new();

    let result = loop {
        tokio::select! {
            telemetry_res = mqtt.poll_telemetry() => {
                match telemetry_res {
                    Ok(msg) => {
                        if let Err(e) = render_dashboard(&msg.payload, &mut state, quirks) {
                            log::warn!("Failed to render telemetry updates: {:?}", e);
                        }
                    }
                    Err(e) => break Err(e),
                }
            }

            _ = ping_timer.tick() => {
                if let Err(e) = mqtt.send_ping().await {
                    log::warn!("Failed to dispatch keep-alive ping: {:?}", e);
                }
            }

            Some(key) = key_rx.recv() => {
                if should_quit(&key) {
                    break Ok(());
                }
            }
        }
    };

    shutdown.store(true, Ordering::Relaxed);
    result
}

fn should_quit(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q' | 'Q' | 'x' | 'X') if key.modifiers == KeyModifiers::NONE => true,
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => true,
        KeyCode::Esc => true,
        _ => false,
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

    let mut w = RawWriter(io::stdout());
    write!(w, "\x1B[1;1H\x1B[2J").unwrap_or(());

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

    writeln!(
        w,
        "================== Bambu Lab Printer Live Dashboard ==================="
    )
    .unwrap_or(());
    writeln!(w, "{:<20} : {}", "Operational State", gcode_state).unwrap_or(());
    writeln!(w, "{:<20} : {}", "Active Job Name", subtask_name).unwrap_or(());
    writeln!(
        w,
        "{:<20} : {:.1}%  ({}/{})",
        "Print Progress", progress, layer_num, total_layers
    )
    .unwrap_or(());
    writeln!(w, "{:<20} : {}", "Time Remaining", remaining_formatted).unwrap_or(());

    let spd_label = match state.get("spd_lvl").and_then(|v| v.as_u64()) {
        Some(1) => "Silent",
        Some(2) => "Standard",
        Some(3) => "Sport",
        Some(4) => "Ludicrous",
        _ => "--",
    };
    let spd_mag = state
        .get("spd_mag")
        .and_then(|v| v.as_u64())
        .map(|m| format!("{}%", m))
        .unwrap_or_else(|| "--".to_string());
    writeln!(w, "{:<20} : {} ({})", "Print Speed", spd_label, spd_mag).unwrap_or(());

    // -- Nozzles --
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

    let nozzle_temper = state
        .get("nozzle_temper")
        .and_then(|t| t.as_f64())
        .unwrap_or(0.0);
    let nozzle_target = state
        .get("nozzle_target_temper")
        .and_then(|t| t.as_f64())
        .unwrap_or(0.0);
    let (nozzle_act, _) = PrinterTelemetry::unpack_temperature(nozzle_temper);
    let nozzle_tgt = nozzle_target as u16;

    if nozzles.len() == 1 {
        nozzles[0].temp = format!("{}°C / T: {}°C", nozzle_act, nozzle_tgt);
    } else if nozzles.len() >= 2 {
        if let Some(right) = nozzles.iter_mut().find(|n| n.id == 0) {
            right.temp = format!("target: {}°C", nozzle_tgt);
        }
        if let Some(left) = nozzles.iter_mut().find(|n| n.id == 1) {
            left.temp = format!("{}°C", nozzle_act);
        }
    }

    writeln!(
        w,
        "\n--- Nozzles -----------------------------------------------------------"
    )
    .unwrap_or(());
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
            writeln!(w, "{:<34} │ {}", cols[0], cols[1]).unwrap_or(());
        } else {
            writeln!(w, "{}", cols[0]).unwrap_or(());
        }
    }

    // -- Thermal (bed + chamber) --
    let bed_temper = state
        .get("bed_temper")
        .and_then(|t| t.as_f64())
        .unwrap_or(0.0);
    let bed_target = state
        .get("bed_target_temper")
        .and_then(|t| t.as_f64())
        .unwrap_or(0.0);
    let (bed_act, _) = PrinterTelemetry::unpack_temperature(bed_temper);
    let bed_tgt = bed_target as u16;

    writeln!(
        w,
        "\n--- Thermal -----------------------------------------------------------"
    )
    .unwrap_or(());
    writeln!(w, "{:<10} : {}°C / T: {}°C", "Heated Bed", bed_act, bed_tgt).unwrap_or(());

    if !quirks.ignores_chamber_temperature() {
        let chamber_temper = state
            .get("chamber_temper")
            .and_then(|t| t.as_f64())
            .unwrap_or(0.0);
        let (chamber_act, chamber_tgt) = PrinterTelemetry::unpack_temperature(chamber_temper);
        writeln!(
            w,
            "{:<20} : {:>3}°C / {:>3}°C",
            "Chamber", chamber_act, chamber_tgt
        )
        .unwrap_or(());
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

    writeln!(
        w,
        "\n--- Fans & System -----------------------------------------------------"
    )
    .unwrap_or(());
    for i in 0..4 {
        writeln!(
            w,
            "{:<14} : {:<6} {:>3} {:<14} : {}",
            fan_values[i].0, fan_values[i].1, "│", sys_values[i].0, sys_values[i].1
        )
        .unwrap_or(());
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
            writeln!(w, "{} {}", header, "-".repeat(pad)).unwrap_or(());

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

                table.write_to(&mut w);
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
            writeln!(
                w,
                "\n--- External Spool ----------------------------------------------------"
            )
            .unwrap_or(());
            writeln!(
                w,
                "{:<20} : {} {} (max {}°C)",
                "Material", tray_type, color_swatch, nozzle_temp
            )
            .unwrap_or(());
        }
    }

    writeln!(
        w,
        "======================================================================="
    )
    .unwrap_or(());

    // -- Diagnostics --
    if let Some(err_val) = state.get("print_error").and_then(|e| e.as_u64())
        && let Some(decoded_err) = decode_print_error(err_val as u32)
        && decoded_err.is_genuine_fault
    {
        writeln!(
            w,
            "\x1B[1;31m[ACTIVE ERROR] Code: {}\x1B[0m",
            decoded_err.short_code
        )
        .unwrap_or(());
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
            writeln!(w, "Active Hardware Alerts:").unwrap_or(());
            for decoded in &active_hms {
                writeln!(
                    w,
                    "  \x1B[1;33m[{}] Severity: {:?} (Module: {})\x1B[0m",
                    decoded.short_code, decoded.severity, decoded.module_id
                )
                .unwrap_or(());
            }
        }
    }

    writeln!(w, "\n\x1B[2m[q/x/Esc to quit]\x1B[0m").unwrap_or(());
    w.flush().unwrap_or(());

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
