#![cfg(feature = "std")]

use std::io::{self, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bambino::client::FanTarget;
use bambino::error::BambuError;
use serde::Serialize;

use crate::connection::{Printer, create_printer};

const DEFAULT_CAPTURE_WINDOW_SECS: u64 = 3;
const LONG_CAPTURE_WINDOW_SECS: u64 = 60;
const PUSHALL_TIMEOUT_SECS: u64 = 10;

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
        }
    }

    fn all_ordered() -> &'static [ProbeTest] {
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
        ]
    }

    fn capture_window_secs(&self) -> u64 {
        match self {
            Self::HomeAxes | Self::HomeAxesRepeat => LONG_CAPTURE_WINDOW_SECS,
            _ => DEFAULT_CAPTURE_WINDOW_SECS,
        }
    }

    fn from_name(name: &str) -> Option<ProbeTest> {
        Self::all_ordered()
            .iter()
            .find(|t| t.name() == name)
            .copied()
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
    responses: Vec<CapturedMessage>,
    elapsed_ms: u64,
    response_count: usize,
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
) -> Result<Option<serde_json::Value>, BambuError> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        let msg = client.poll_raw().await?;
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&msg.payload)
            && v.get("print").and_then(|p| p.get("gcode_state")).is_some()
        {
            return Ok(Some(v));
        }
    }

    Ok(None)
}

async fn send_command(client: &mut Printer, test: ProbeTest) -> Result<(), BambuError> {
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
            client.toggle_led("chamber_light", true).await?;
        }
        ProbeTest::LedOff => {
            client.toggle_led("chamber_light", false).await?;
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
        ProbeTest::HomeAxes | ProbeTest::HomeAxesRepeat => {
            client.home_axes(false).await?;
        }
    }
    Ok(())
}

async fn capture_responses(
    client: &mut Printer,
    window: Duration,
) -> Result<Vec<CapturedMessage>, BambuError> {
    let mut responses = Vec::new();
    let start = Instant::now();
    let deadline = start + window;

    while Instant::now() < deadline {
        let msg = client.poll_raw().await?;
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&msg.payload) {
            responses.push(CapturedMessage {
                elapsed_ms: start.elapsed().as_millis() as u64,
                payload: v,
            });
        }
    }

    Ok(responses)
}

pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    args: &[String],
) -> Result<(), BambuError> {
    let mut output_path = String::from("probe_report.json");
    let mut test_filter: Option<Vec<String>> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--output" | "-o" => {
                i += 1;
                if i < args.len() {
                    output_path.clone_from(&args[i]);
                }
            }
            "--tests" | "-t" => {
                i += 1;
                if i < args.len() {
                    test_filter = Some(args[i].split(',').map(|s| s.trim().to_string()).collect());
                }
            }
            _ => {}
        }
        i += 1;
    }

    let tests: Vec<ProbeTest> = if let Some(ref filter) = test_filter {
        let mut selected = Vec::new();
        for name in filter {
            match ProbeTest::from_name(name) {
                Some(t) => selected.push(t),
                None => {
                    eprintln!("Unknown test: '{}'. Available tests:", name);
                    for t in ProbeTest::all_ordered() {
                        eprintln!("  {} — {}", t.name(), t.description());
                    }
                    return Err(BambuError::ProtocolViolation(
                        format!("Unknown test name: '{}'", name).into(),
                    ));
                }
            }
        }
        selected
    } else {
        ProbeTest::all_ordered().to_vec()
    };

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
║  Press Enter to continue or Ctrl+C to abort.               ║
╚══════════════════════════════════════════════════════════════╝"
    );

    let mut confirmation = String::new();
    io::stdin()
        .read_line(&mut confirmation)
        .map_err(|_| BambuError::ProtocolViolation("Failed to read user confirmation".into()))?;

    eprintln!("Connecting to {}:8883...", ip);
    let mut client = create_printer(ip, serial, access_code)?;
    client.connect_mqtt().await?;
    eprintln!("Connected.");

    eprint!("Requesting pushall state dump... ");
    io::stderr().flush().unwrap_or(());
    client.request_pushall().await?;
    let pushall = capture_pushall(&mut client, Duration::from_secs(PUSHALL_TIMEOUT_SECS)).await?;
    if pushall.is_some() {
        eprintln!("captured.");
    } else {
        eprintln!(
            "timed out ({}s). Continuing without pushall.",
            PUSHALL_TIMEOUT_SECS
        );
    }

    eprintln!("Running {} tests...\n", tests.len());

    let model = client.model();
    let serial_owned = client.serial().to_string();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut entries = Vec::new();

    for (idx, test) in tests.iter().enumerate() {
        let window_secs = test.capture_window_secs();
        let capture_window = Duration::from_secs(window_secs);

        eprint!(
            "[{}/{}] {} — {} ({}s window)... ",
            idx + 1,
            tests.len(),
            test.name(),
            test.description(),
            window_secs
        );
        io::stderr().flush().unwrap_or(());

        let start = Instant::now();

        let publish_error = match send_command(&mut client, *test).await {
            Ok(()) => None,
            Err(e) => Some(e.to_string()),
        };

        let responses = if publish_error.is_none() {
            capture_responses(&mut client, capture_window).await?
        } else {
            Vec::new()
        };

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let response_count = responses.len();

        eprintln!(
            "{} response{} in {}ms{}",
            response_count,
            if response_count == 1 { "" } else { "s" },
            elapsed_ms,
            if publish_error.is_some() {
                " (publish failed)"
            } else {
                ""
            }
        );

        entries.push(ProbeEntry {
            test: test.name().to_string(),
            description: test.description().to_string(),
            capture_window_secs: window_secs,
            publish_error,
            responses,
            elapsed_ms,
            response_count,
        });
    }

    let report = ProbeReport {
        model: format!("{:?}", model),
        serial: serial_owned,
        timestamp,
        pushall,
        tests: entries,
    };

    let json = serde_json::to_string_pretty(&report).map_err(|_| BambuError::SerializationError)?;

    std::fs::write(&output_path, json.as_bytes()).map_err(|e| {
        BambuError::ProtocolViolation(
            format!("Failed to write report to '{}': {}", output_path, e).into(),
        )
    })?;

    eprintln!("\nReport written to {}", output_path);
    Ok(())
}
