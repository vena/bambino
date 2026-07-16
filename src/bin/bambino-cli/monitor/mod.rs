#![cfg(feature = "cli")]

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

use crate::connection::create_printer;

/// Connects, sends `pushall`, and either dumps the first response containing a `print` object
/// as pretty JSON (default) or, with `follow`, keeps printing every subsequent `print`-bearing
/// push as one compact NDJSON line until interrupted (Ctrl+C) — for capturing a sequence of
/// incremental pushes (e.g. across a tray-load event) rather than a single snapshot.
pub async fn dump(
    ip: &str,
    serial: &str,
    access_code: &str,
    follow: bool,
) -> Result<(), BambuError> {
    eprintln!("Connecting to {}:8883 for raw telemetry dump...", ip);

    let mut printer = create_printer(ip, serial, access_code)?;
    printer.request_pushall().await?;

    if follow {
        eprintln!("Following telemetry pushes as NDJSON — Ctrl+C to stop.");

        // BUG-092: MQTT_KEEP_ALIVE_SECS (client/codec.rs) is 30 — without a periodic ping,
        // the broker resets the connection once that elapses with no packet from the client.
        // Mirrors run()'s PING_TICK_SECS/ping_timer below.
        const PING_TICK_SECS: u64 = 15;
        let mut ping_timer = interval(Duration::from_secs(PING_TICK_SECS));
        ping_timer.tick().await;

        loop {
            tokio::select! {
                res = printer.poll_raw() => {
                    let msg = res?;
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&msg.payload)
                        && v.get("print").is_some()
                    {
                        println!("{}", serde_json::to_string(&v).unwrap_or_default());
                    }
                }
                _ = ping_timer.tick() => {
                    printer.send_ping().await?;
                    // BUG-129: matches run()'s dashboard loop below — a silently-dead
                    // connection during `monitor dump --follow` previously had no zombie
                    // detection at all, hanging indefinitely.
                    printer.mqtt().await?.tick_zombie_check(PING_TICK_SECS as u32)?;
                }
            }
        }
    }

    let timeout = tokio::time::sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            res = printer.poll_raw() => {
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
        // BUG-043: construct the guard immediately after raw mode is enabled, before the
        // fallible alt-screen/cursor-hide write below — if that write or flush fails and `?`
        // returns early, `guard` (already bound) still gets dropped as this function returns,
        // so `Drop` still restores the terminal. Returning `Ok(Self)` only at the end (the
        // previous shape) meant a write failure left raw mode enabled with no guard ever
        // constructed to undo it.
        let guard = Self;
        let mut stdout = io::stdout();
        write!(stdout, "\x1B[?1049h\x1B[?25l")?;
        stdout.flush()?;
        Ok(guard)
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

    let mut printer = create_printer(ip, serial, access_code)?;
    let quirks = printer.model().quirks();

    printer.request_pushall().await?;

    const PING_TICK_SECS: u64 = 15;
    let mut ping_timer = interval(Duration::from_secs(PING_TICK_SECS));
    ping_timer.tick().await;

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

    // NOTE: racing `poll_telemetry()` against `ping_timer.tick()` here means a silently
    // dropped connection is caught by `tick_zombie_check`'s 60s `secs_since_last_message`
    // counter below, not by `poll_wire`'s 30s per-read deadline (`mqtt/client/frame.rs`) —
    // every time this select drops the in-flight telemetry future (every PING_TICK_SECS),
    // that deadline resets before it can fire. Confirmed on real hardware 2026-07-06; see
    // CLAUDE.md's "select!-multiplexed consumers" entry for why this is expected, not a bug.
    let result = loop {
        tokio::select! {
            telemetry_res = printer.poll_telemetry() => {
                match telemetry_res {
                    Ok(event) => {
                        let payload = &event.raw().payload;
                        if let Err(e) = dashboard::render_dashboard(payload, &mut state, quirks) {
                            log::warn!("Failed to render telemetry updates: {:?}", e);
                        }
                    }
                    Err(e) => break Err(e),
                }
            }

            _ = ping_timer.tick() => {
                if let Err(e) = printer.send_ping().await {
                    log::warn!("Failed to dispatch keep-alive ping: {:?}", e);
                }
                // `tick_zombie_check` already logs its own `log::warn!` describing which
                // liveness condition tripped before returning `Err`, so a detected zombie is
                // treated as fatal here (mirroring the `poll_telemetry` error branch above)
                // rather than logged-and-ignored like a single failed ping write above —
                // continuing to loop against a connection this check has already confirmed
                // dead would defeat the point of running it.
                match printer.mqtt().await {
                    Ok(mqtt) => {
                        if let Err(e) = mqtt.tick_zombie_check(PING_TICK_SECS as u32) {
                            break Err(e);
                        }
                    }
                    Err(e) => break Err(e),
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
