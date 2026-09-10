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

use crate::connection::{Printer, create_printer};
use crate::error::CliError;

/// Prints incoming pushes as compact NDJSON, one line each, until interrupted or the
/// connection fails.
///
/// `print_only` gates *which messages* are emitted, never which keys survive: every emitted
/// line is the complete payload, re-serialized from a generic `Value` rather than from a typed
/// struct, so fields bambino does not model at all still reach the capture.
///
/// - `true` — only `print`-bearing pushes. `dump --follow`'s long-standing contract.
/// - `false` — every message on the report topic. The topic also carries `info` (get_version
///   responses), `system`, and `mc_print` roots, which `TelemetryReport` does not model at all
///   (`types/telemetry/mod.rs` declares `print` only). A diagnostic capture chasing an unknown
///   progress indicator must not pre-filter those away — the field being looked for may well be
///   under a root nobody has declared yet (see issue #227).
///
/// Never returns `Ok` — the caller ends the capture with Ctrl+C, so the only exit is the `?`
/// on a poll/ping error.
pub(crate) async fn follow_pushes(printer: &mut Printer, print_only: bool) -> Result<(), CliError> {
    // MQTT_KEEP_ALIVE_SECS (client/codec.rs) is 30 — without a periodic ping,
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
                    && (!print_only || v.get("print").is_some())
                {
                    println!("{}", serde_json::to_string(&v).unwrap_or_default());
                }
            }
            _ = ping_timer.tick() => {
                printer.send_ping().await?;
                // Matches run()'s dashboard loop below.
                printer.mqtt().await?.tick_zombie_check(PING_TICK_SECS as u32)?;
            }
        }
    }
}

/// Connects, sends `pushall`, and either dumps the first response containing a `print` object
/// as pretty JSON (default) or, with `follow`, keeps printing every subsequent `print`-bearing
/// push as one compact NDJSON line until interrupted (Ctrl+C) — for capturing a sequence of
/// incremental pushes (e.g. across a tray-load event) rather than a single snapshot.
pub async fn dump(ip: &str, serial: &str, access_code: &str, follow: bool) -> Result<(), CliError> {
    eprintln!("Connecting to {}:8883 for raw telemetry dump...", ip);

    let mut printer = create_printer(ip, serial, access_code)?;
    printer.request_pushall().await?;

    if follow {
        eprintln!("Following telemetry pushes as NDJSON — Ctrl+C to stop.");
        return follow_pushes(&mut printer, true).await;
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
        // Construct the guard immediately after raw mode is enabled, before the
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
pub async fn run(ip: &str, serial: &str, access_code: &str) -> Result<(), CliError> {
    eprintln!("Connecting to secure MQTT broker at {}:8883...", ip);

    let mut printer = create_printer(ip, serial, access_code)?;
    let quirks = printer.model().quirks();

    printer.request_pushall().await?;

    const PING_TICK_SECS: u64 = 15;
    let mut ping_timer = interval(Duration::from_secs(PING_TICK_SECS));
    ping_timer.tick().await;

    let _guard = TerminalGuard::enter()?;

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

    // Most recent non-fatal diagnostic, shown in the dashboard footer. These used to be
    // `log::warn!` calls, which reach stderr on the same raw-mode tty the dashboard is
    // drawing to and corrupt it; the CLI's logger is silenced for this subcommand
    // (see main.rs), so the footer is now the only place they surface.
    let mut warning: Option<String> = None;

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
                        // Read the progress cache after `poll_telemetry()` has folded this
                        // frame in, so the dashboard shows the same values a library consumer
                        // would see rather than re-deriving them from the raw map.
                        let progress = printer.print_progress();
                        let payload = &event.raw().payload;
                        match dashboard::render_dashboard(
                            payload,
                            &mut state,
                            quirks,
                            progress,
                            warning.as_deref(),
                        ) {
                            Ok(()) => warning = None,
                            Err(e) => {
                                warning =
                                    Some(format!("Failed to render telemetry updates: {:?}", e));
                            }
                        }
                    }
                    Err(e) => break Err(e),
                }
            }

            _ = ping_timer.tick() => {
                if let Err(e) = printer.send_ping().await {
                    warning = Some(format!("Failed to dispatch keep-alive ping: {:?}", e));
                }
                // `tick_zombie_check` logs its own `log::warn!` describing which liveness
                // condition tripped before returning `Err` (discarded under this subcommand's
                // silenced logger, but the `Err` itself surfaces as a `CliError` once the
                // terminal guard has restored the screen), so a detected zombie is
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
    result.map_err(CliError::from)
}

fn should_quit(key: &KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q' | 'Q' | 'x' | 'X') if key.modifiers == KeyModifiers::NONE => true,
        KeyCode::Char('c') if key.modifiers == KeyModifiers::CONTROL => true,
        KeyCode::Esc => true,
        _ => false,
    }
}
