#![cfg(feature = "std")]

mod dashboard;

use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal;
use tokio::sync::mpsc;
use tokio::time::interval;

use bambino::error::BambuError;
use bambino::models::resolve_model;
use bambino::mqtt::PushAllRequest;

use crate::connection::connect_mqtt;

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
                        if let Err(e) = dashboard::render_dashboard(&msg.payload, &mut state, quirks) {
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
