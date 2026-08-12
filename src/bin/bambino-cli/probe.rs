#![cfg(feature = "cli")]

use std::io::{self, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bambino::client::{FanTarget, PrintStatus};
use bambino::Error;
use serde::Serialize;

use crate::connection::{Printer, create_printer};
use crate::error::CliError;

const DEFAULT_CAPTURE_WINDOW_SECS: u64 = 3;
const LONG_CAPTURE_WINDOW_SECS: u64 = 60;
const PUSHALL_TIMEOUT_SECS: u64 = 10;
// Mirrors PrinterClient::wait_for_homing()'s internal timeout override (src/client/motion.rs) —
// display-only, since that method manages its own deadline rather than taking one.
const HOMING_WAIT_DISPLAY_SECS: u64 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeTest {
    MoveZUnhomed,
    MoveXUnhomed,
    PauseWhenIdle,
    ResumeWhenIdle,
    StopWhenIdle,
    ClearError,
    LedOn,
    LedOff,
    FanPartZero,
    TempNozzleZero,
    TempBedZero,
    HomeAxes,
    HomeAxesRepeat,
    HomeAxesWait,
    HomeAxesRepeatWait,
    HomeAxesWithBusyCheck,
}

impl ProbeTest {
    fn name(&self) -> &'static str {
        match self {
            Self::MoveZUnhomed => "move_z_unhomed",
            Self::MoveXUnhomed => "move_x_unhomed",
            Self::PauseWhenIdle => "pause_when_idle",
            Self::ResumeWhenIdle => "resume_when_idle",
            Self::StopWhenIdle => "stop_when_idle",
            Self::ClearError => "clear_error",
            Self::LedOn => "led_on",
            Self::LedOff => "led_off",
            Self::FanPartZero => "fan_part_zero",
            Self::TempNozzleZero => "temp_nozzle_zero",
            Self::TempBedZero => "temp_bed_zero",
            Self::HomeAxes => "home_axes",
            Self::HomeAxesRepeat => "home_axes_repeat",
            Self::HomeAxesWait => "home_axes_wait",
            Self::HomeAxesRepeatWait => "home_axes_repeat_wait",
            Self::HomeAxesWithBusyCheck => "home_axes_with_busy_check",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::MoveZUnhomed => "Z+1mm move while unhomed (expect rejection)",
            Self::MoveXUnhomed => "X+5mm move while unhomed (expect rejection)",
            Self::PauseWhenIdle => "Pause print when no print active",
            Self::ResumeWhenIdle => "Resume print when no print active",
            Self::StopWhenIdle => "Stop print when no print active",
            Self::ClearError => "Clear print error when none active",
            Self::LedOn => "Turn chamber LED on",
            Self::LedOff => "Turn chamber LED off",
            Self::FanPartZero => "Set part cooling fan to 0%",
            Self::TempNozzleZero => "Set nozzle temperature to 0",
            Self::TempBedZero => "Set bed temperature to 0",
            Self::HomeAxes => "Home all axes (changes printer state)",
            Self::HomeAxesRepeat => {
                "Home all axes again immediately after home_axes (redundant re-home — printer is already homed going in)"
            }
            Self::HomeAxesWait => {
                "Home all axes, then block on wait_for_homing() until firmware confirms completion"
            }
            Self::HomeAxesRepeatWait => {
                "Home all axes again via wait_for_homing() (redundant re-home — validates wait_for_homing() does not false-resolve instantly on an already-homed printer)"
            }
            Self::HomeAxesWithBusyCheck => {
                "Holistic homing example: refuse if the printer is actively printing/paused (gcode_state), otherwise always try wait_for_homing() first to join any already-in-progress home, falling back to self-triggered home_axes() only on timeout. MANUAL: trigger homing from the printer's touchscreen/slicer during the confirmation pause to exercise the join path; not run by default."
            }
        }
    }

    /// Full registry of every test, in stable order.
    /// Used for `-t` lookup and the unknown-test help listing — includes manual-intervention tests,
    /// which are otherwise excluded from the no-`-t` default run; see
    /// [`default_set()`](Self::default_set).
    fn all_known() -> &'static [ProbeTest] {
        &[
            Self::MoveZUnhomed,
            Self::MoveXUnhomed,
            Self::PauseWhenIdle,
            Self::ResumeWhenIdle,
            Self::StopWhenIdle,
            Self::ClearError,
            Self::LedOn,
            Self::LedOff,
            Self::FanPartZero,
            Self::TempNozzleZero,
            Self::TempBedZero,
            Self::HomeAxes,
            Self::HomeAxesRepeat,
            Self::HomeAxesWait,
            Self::HomeAxesRepeatWait,
            Self::HomeAxesWithBusyCheck,
        ]
    }

    /// Tests run when `-t`/`--tests` is omitted — every known test except those requiring manual intervention.
    fn default_set() -> Vec<ProbeTest> {
        Self::all_known()
            .iter()
            .copied()
            .filter(|t| !t.requires_manual_intervention())
            .collect()
    }

    /// True for tests that need the operator to do something outside this process (e.g. trigger homing from the touchscreen) — excluded from the default set, selectable only explicitly via `-t`.
    fn requires_manual_intervention(&self) -> bool {
        matches!(self, Self::HomeAxesWithBusyCheck)
    }

    fn capture_window_secs(&self) -> u64 {
        match self {
            Self::HomeAxes | Self::HomeAxesRepeat => LONG_CAPTURE_WINDOW_SECS,
            Self::HomeAxesWait | Self::HomeAxesRepeatWait => HOMING_WAIT_DISPLAY_SECS,
            _ => DEFAULT_CAPTURE_WINDOW_SECS,
        }
    }

    /// True for tests that block on [`PrinterClient::wait_for_homing()`] instead of capturing raw telemetry for a fixed window — `wait_for_homing()` consumes every message it polls internally, so there's nothing left to capture alongside it.
    fn uses_wait_for_homing(&self) -> bool {
        matches!(self, Self::HomeAxesWait | Self::HomeAxesRepeatWait)
    }

    fn from_name(name: &str) -> Option<ProbeTest> {
        Self::all_known().iter().find(|t| t.name() == name).copied()
    }
}

#[derive(Serialize)]
struct CapturedMessage {
    elapsed_ms: u64,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct ProbeEntry {
    test: String,
    description: String,
    capture_window_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    publish_error: Option<String>,
    // capture_responses() failures are recorded here instead of aborting run() via
    // `?`, which previously discarded every already-captured entry and wrote nothing at all.
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_error: Option<String>,
    responses: Vec<CapturedMessage>,
    elapsed_ms: u64,
    response_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    wait_outcome: Option<String>,
}

#[derive(Serialize)]
struct ProbeReport {
    model: String,
    serial: String,
    timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pushall: Option<serde_json::Value>,
    tests: Vec<ProbeEntry>,
}

async fn capture_pushall(
    client: &mut Printer,
    timeout: Duration,
) -> Result<Option<serde_json::Value>, Error> {
    let deadline = Instant::now() + timeout;

    // Goes through poll_telemetry() (not poll_raw()) so this also warms
    // PrinterClient's home_flag/gcode_state cache from the very first response.
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = match tokio::time::timeout(remaining, client.poll_telemetry()).await {
            Ok(Ok(event)) => event,
            Ok(Err(e)) => return Err(e),
            Err(_) => break,
        };
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&event.raw().payload)
            && v.get("print").and_then(|p| p.get("gcode_state")).is_some()
        {
            return Ok(Some(v));
        }
    }

    Ok(None)
}

async fn send_command(client: &mut Printer, test: ProbeTest) -> Result<(), Error> {
    match test {
        ProbeTest::MoveZUnhomed => {
            client.move_relative('Z', 1.0, 500).await?;
        }
        ProbeTest::MoveXUnhomed => {
            client.move_relative('X', 5.0, 1000).await?;
        }
        ProbeTest::PauseWhenIdle => {
            client.pause_print().await?;
        }
        ProbeTest::ResumeWhenIdle => {
            client.resume_print().await?;
        }
        ProbeTest::StopWhenIdle => {
            client.stop_print().await?;
        }
        ProbeTest::ClearError => {
            client.clear_print_error().await?;
        }
        ProbeTest::LedOn => {
            client.set_led("chamber_light", true).await?;
        }
        ProbeTest::LedOff => {
            client.set_led("chamber_light", false).await?;
        }
        ProbeTest::FanPartZero => {
            client.set_fan_speed(FanTarget::PartCooling, 0).await?;
        }
        ProbeTest::TempNozzleZero => {
            client.set_nozzle_temperature(0, 0).await?;
        }
        ProbeTest::TempBedZero => {
            client.set_bed_temperature(0).await?;
        }
        ProbeTest::HomeAxes
        | ProbeTest::HomeAxesRepeat
        | ProbeTest::HomeAxesWait
        | ProbeTest::HomeAxesRepeatWait => {
            client.home_axes(false).await?;
        }
        ProbeTest::HomeAxesWithBusyCheck => {
            unreachable!("dispatched via run_holistic_homing(), not send_command()")
        }
    }
    Ok(())
}

/// Returns whether the printer is actively printing, preparing to print, or paused —
/// the one verified busy signal ([REF-MQTT-IDLEBUG]: `gcode_state` is the only field
/// this codebase treats as authoritative for busy/idle classification). `Preparing`
/// (wire `"PREPARE"`) covers homing/bed-leveling/priming motion before `RUNNING`.
fn printer_is_busy(client: &Printer) -> bool {
    matches!(
        client.print_status(),
        Some(PrintStatus::Preparing) | Some(PrintStatus::Running) | Some(PrintStatus::Paused)
    )
}

/// Demonstrates a consumer-style holistic homing routine for [`ProbeTest::HomeAxesWithBusyCheck`]:
///
/// 1. Refuse outright if the printer is actively printing/paused (hard safety gate).
/// 2. Skip if already homed.
/// 3. Otherwise always try `wait_for_homing()` first. It tracks `home_flag`, which
///    stays in the "not all set" state for an active cycle's entire duration — unlike
///    `mc_print_sub_stage`, which real-hardware testing showed pulses briefly near the
///    *start* of `G28` and reverts well before the cycle finishes [REF-MOTO-HOME]. A
///    probe run against a printer already several seconds into a UI-triggered home
///    observed `mc_print_sub_stage` back at its rest value despite `home_flag` still
///    showing unhomed axes — gating on the pulse missed it and self-triggered a
///    redundant `home_axes()` on top of the still-active external cycle. Trying
///    `wait_for_homing()` unconditionally has no such timing window: it joins an
///    in-progress home no matter how long it's been running before we connected.
/// 4. If the join times out (nothing ever resolved within ~90s), re-check the safety
///    gate once more, then self-trigger `home_axes()` and wait.
async fn run_holistic_homing(client: &mut Printer) -> Result<String, Error> {
    // Warm up the home_flag/gcode_state cache (a single poll may land on a partial
    // telemetry delta carrying neither). mc_print_sub_stage is recorded opportunistically
    // for context only — it does not gate any branch below, see the doc comment.
    let warmup_deadline = Instant::now() + Duration::from_secs(DEFAULT_CAPTURE_WINDOW_SECS);
    let mut sub_stage_at_start = None;
    while Instant::now() < warmup_deadline
        && (client.print_status().is_none() || client.is_all_axes_homed().is_none())
    {
        let remaining = warmup_deadline.saturating_duration_since(Instant::now());
        let event = match tokio::time::timeout(remaining, client.poll_telemetry()).await {
            Ok(Ok(event)) => event,
            Ok(Err(e)) => return Err(e),
            Err(_) => break,
        };
        if sub_stage_at_start.is_none() {
            sub_stage_at_start = event
                .report()
                .and_then(|r| r.print.as_ref())
                .and_then(|p| p.mc_print_sub_stage);
        }
    }

    if printer_is_busy(client) {
        return Ok(format!(
            "refused: printer busy (gcode_state={:?})",
            client.print_status()
        ));
    }

    if client.is_all_axes_homed() == Some(true) {
        return Ok("already homed, no action".to_string());
    }

    match client.wait_for_homing().await {
        Ok(()) => Ok(format!(
            "joined in-progress home, resolved (mc_print_sub_stage was {:?} at start)",
            sub_stage_at_start
        )),
        Err(Error::Timeout) => {
            // Nothing resolved during the wait. Re-check the safety gate before
            // self-triggering — printing state may have changed during the ~90s wait.
            tokio::time::timeout(
                Duration::from_secs(DEFAULT_CAPTURE_WINDOW_SECS),
                client.poll_telemetry(),
            )
            .await
            .map_err(|_| Error::Timeout)??;
            if printer_is_busy(client) {
                return Ok(format!(
                    "refused after wait: printer busy (gcode_state={:?})",
                    client.print_status()
                ));
            }
            client.home_axes(false).await?;
            match client.wait_for_homing().await {
                Ok(()) => Ok("self-triggered home, resolved".to_string()),
                Err(e) => Ok(format!("self-triggered home, error: {e}")),
            }
        }
        Err(e) => Ok(format!("error: {e}")),
    }
}

async fn capture_responses(
    client: &mut Printer,
    window: Duration,
) -> Result<Vec<CapturedMessage>, Error> {
    let mut responses = Vec::new();
    let start = Instant::now();
    let deadline = start + window;

    // Goes through poll_telemetry() (not poll_raw()) so every test's capture window
    // also warms PrinterClient's home_flag/gcode_state cache as a side effect.
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let event = match tokio::time::timeout(remaining, client.poll_telemetry()).await {
            Ok(Ok(event)) => event,
            Ok(Err(e)) => return Err(e),
            Err(_) => break,
        };
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&event.raw().payload) {
            responses.push(CapturedMessage {
                elapsed_ms: start.elapsed().as_millis() as u64,
                payload: v,
            });
        }
    }

    Ok(responses)
}

fn confirm_or_abort() -> Result<bool, CliError> {
    eprintln!(
        "\
╔══════════════════════════════════════════════════════════════╗
║  PROBE: Command Response Capture                           ║
║                                                            ║
║  This will send commands to your printer to capture        ║
║  firmware response patterns.                               ║
║                                                            ║
║  Ensure the bed is at least 50mm from the nozzle.          ║
║                                                            ║
║  Type 'yes' to continue.                                   ║
╚══════════════════════════════════════════════════════════════╝"
    );

    let mut confirmation = String::new();
    io::stdin().read_line(&mut confirmation)?;
    if confirmation.trim().to_lowercase() != "yes" {
        eprintln!("Aborted (expected 'yes').");
        return Ok(false);
    }
    Ok(true)
}

async fn run_pushall_capture(client: &mut Printer) -> Option<serde_json::Value> {
    eprint!("Requesting pushall state dump... ");
    io::stderr().flush().unwrap_or(());
    match client.request_pushall().await {
        Ok(_) => match capture_pushall(client, Duration::from_secs(PUSHALL_TIMEOUT_SECS)).await {
            Ok(Some(p)) => {
                eprintln!("captured.");
                Some(p)
            }
            Ok(None) => {
                eprintln!(
                    "timed out ({}s). Continuing without pushall.",
                    PUSHALL_TIMEOUT_SECS
                );
                None
            }
            Err(e) => {
                eprintln!("capture failed: {e}. Continuing without pushall.");
                None
            }
        },
        Err(e) => {
            eprintln!("request failed: {e}. Continuing without pushall.");
            None
        }
    }
}

/// Refuses to run the default test sweep against a printer that is printing, preparing, or
/// paused. Reuses the pushall already captured by [`run_pushall_capture`] rather than issuing a
/// second request. Bare `G28` mid-print can drive the toolhead into an in-progress part
/// (see `client/motion.rs`), and `MoveZUnhomed`/`MoveXUnhomed` move fixed distances regardless
/// of what's under the nozzle — mirrors `ack_probe.rs::refuse_if_busy`.
fn refuse_if_busy(client: &Printer) -> Result<(), CliError> {
    match client.print_status() {
        Some(status @ (PrintStatus::Preparing | PrintStatus::Running | PrintStatus::Paused)) => {
            Err(CliError::Other(format!(
                "printer is busy (gcode_state={status:?}) — probe refuses to run its default \
                 test sweep during a print; HomeAxes/MoveZUnhomed/MoveXUnhomed and friends can \
                 drive motion into an in-progress part"
            )))
        }
        Some(_) => Ok(()),
        None => Err(CliError::Other(
            "no gcode_state received from the pushall capture — cannot confirm the printer is \
             idle, refusing to run the default test sweep"
                .to_string(),
        )),
    }
}

async fn run_holistic_test(
    client: &mut Printer,
    idx: usize,
    total: usize,
    test: ProbeTest,
) -> ProbeEntry {
    eprint!(
        "[{}/{}] {} — {} (holistic check, up to ~{}s)... ",
        idx + 1,
        total,
        test.name(),
        test.description(),
        HOMING_WAIT_DISPLAY_SECS
    );
    io::stderr().flush().unwrap_or(());

    let start = Instant::now();
    let outcome = match run_holistic_homing(client).await {
        Ok(outcome) => outcome,
        Err(e) => format!("error: {e}"),
    };
    let elapsed_ms = start.elapsed().as_millis() as u64;

    eprintln!("{} in {}ms", outcome, elapsed_ms);

    ProbeEntry {
        test: test.name().to_string(),
        description: test.description().to_string(),
        capture_window_secs: 0,
        publish_error: None,
        capture_error: None,
        responses: Vec::new(),
        elapsed_ms,
        response_count: 0,
        wait_outcome: Some(outcome),
    }
}

async fn run_capture_test(
    client: &mut Printer,
    idx: usize,
    total: usize,
    test: ProbeTest,
) -> ProbeEntry {
    let window_secs = test.capture_window_secs();
    let capture_window = Duration::from_secs(window_secs);
    let window_label = if test.uses_wait_for_homing() {
        format!("up to {}s via wait_for_homing", window_secs)
    } else {
        format!("{}s window", window_secs)
    };

    eprint!(
        "[{}/{}] {} — {} ({})... ",
        idx + 1,
        total,
        test.name(),
        test.description(),
        window_label
    );
    io::stderr().flush().unwrap_or(());

    let start = Instant::now();

    let publish_error = match send_command(client, test).await {
        Ok(()) => None,
        Err(e) => Some(e.to_string()),
    };

    // capture_responses() failures used to propagate via `?`, discarding every
    // already-captured entry and aborting before the report was ever written. Recorded
    // as a per-entry capture_error instead, mirroring how publish_error already handles
    // send_command() failures — the run continues to the next test either way.
    let (responses, wait_outcome, capture_error) = if publish_error.is_none() {
        if test.uses_wait_for_homing() {
            let outcome = match client.wait_for_homing().await {
                Ok(()) => "resolved".to_string(),
                Err(e) => format!("error: {e}"),
            };
            (Vec::new(), Some(outcome), None)
        } else {
            match capture_responses(client, capture_window).await {
                Ok(r) => (r, None, None),
                Err(e) => (Vec::new(), None, Some(e.to_string())),
            }
        }
    } else {
        (Vec::new(), None, None)
    };

    let elapsed_ms = start.elapsed().as_millis() as u64;
    let response_count = responses.len();

    eprintln!(
        "{} response{} in {}ms{}{}{}",
        response_count,
        if response_count == 1 { "" } else { "s" },
        elapsed_ms,
        if publish_error.is_some() {
            " (publish failed)"
        } else {
            ""
        },
        if capture_error.is_some() {
            " (capture failed)"
        } else {
            ""
        },
        wait_outcome
            .as_deref()
            .map(|o| format!(" [wait_for_homing: {o}]"))
            .unwrap_or_default()
    );

    ProbeEntry {
        test: test.name().to_string(),
        description: test.description().to_string(),
        capture_window_secs: window_secs,
        publish_error,
        capture_error,
        responses,
        elapsed_ms,
        response_count,
        wait_outcome,
    }
}

async fn run_test_loop(client: &mut Printer, tests: &[ProbeTest]) -> Vec<ProbeEntry> {
    let mut entries = Vec::new();
    for (idx, test) in tests.iter().enumerate() {
        let entry = if matches!(test, ProbeTest::HomeAxesWithBusyCheck) {
            run_holistic_test(client, idx, tests.len(), *test).await
        } else {
            run_capture_test(client, idx, tests.len(), *test).await
        };
        entries.push(entry);
    }
    entries
}

pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    output: &str,
    tests_arg: Option<&str>,
) -> Result<(), CliError> {
    let output_path = output;
    let test_filter: Option<Vec<String>> =
        tests_arg.map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

    let tests: Vec<ProbeTest> = if let Some(ref filter) = test_filter {
        let mut selected = Vec::new();
        for name in filter {
            match ProbeTest::from_name(name) {
                Some(t) => selected.push(t),
                None => {
                    eprintln!("Unknown test: '{}'. Available tests:", name);
                    for t in ProbeTest::all_known() {
                        let manual = if t.requires_manual_intervention() {
                            " (manual — not run by default)"
                        } else {
                            ""
                        };
                        eprintln!("  {} — {}{}", t.name(), t.description(), manual);
                    }
                    return Err(CliError::InvalidArgs(format!("Unknown test name: '{}'", name)));
                }
            }
        }
        selected
    } else {
        ProbeTest::default_set()
    };

    if !confirm_or_abort()? {
        return Ok(());
    }

    eprintln!("Connecting to {}:8883...", ip);
    let mut client = create_printer(ip, serial, access_code)?;
    client.connect_mqtt().await?;
    eprintln!("Connected.");

    let pushall = run_pushall_capture(&mut client).await;

    refuse_if_busy(&client)?;

    eprintln!("Running {} tests...\n", tests.len());

    let model = client.model();
    let serial_owned = client.serial().to_string();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let entries = run_test_loop(&mut client, &tests).await;

    let report = ProbeReport {
        model: format!("{:?}", model),
        serial: serial_owned,
        timestamp,
        pushall,
        tests: entries,
    };

    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| CliError::Other(format!("failed to serialize probe report: {e}")))?;

    std::fs::write(output_path, json.as_bytes())?;

    eprintln!("\nReport written to {}", output_path);
    Ok(())
}
