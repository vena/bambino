#![cfg(feature = "cli")]

//! # Ack-Correlation Test Harness
//!
//! Answers one question per command, against real hardware: does the printer echo a response
//! carrying *the same `sequence_id` we sent*, distinct from the background `push_status`
//! telemetry stream (which runs its own independent, low-valued `sequence_id` counter
//! [REF-MQTT-ACK])?
//!
//! This is the evidence gate for `ACK_CORRELATED_COMMANDS` (`src/mqtt/client/mod.rs`): a command
//! on that allowlist gets strict `sequence_id`-correlated write-zombie detection, and a command
//! off it degrades to the permissive "any PUBLISH clears the timer" behavior. Adding a command
//! there on assumption rather than evidence is exactly the bug that shipped once already
//! (`pushall`, which has no ack at all, hung `bambino-cli monitor` against a real P1S), so
//! entries move onto the list only after a run of this harness reports `ACK` for them.
//!
//! A positive result is narrow on purpose: it means the printer *answers*, not that the command
//! does anything. The P1S sweep behind issue #26 acked `set_airduct` and `buzzer_ctrl` with
//! `result: "success"` on a machine that has neither an airduct damper nor a buzzer
//! [REF-MQTT-ACK]. Write-zombie detection needs exactly that narrow fact and nothing more.
//!
//! Unlike `probe.rs` — which captures whole response *windows* to characterize firmware
//! behavior — this harness cares about exactly one bit per command, and so builds each request
//! struct directly (rather than going through `PrinterClient`'s high-level wrappers) to pin the
//! `sequence_id` it must correlate against.

use std::io::{self, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bambino::Error;
use bambino::client::{BuzzerMode, PrintStatus};
use bambino::models::PrinterModel;
use bambino::mqtt::{
    AirductMode, AirductRequest, AmsChangeFilamentRequest, AmsControlRequest, AmsGetRfidRequest,
    BuzzerRequest, PrintJobConfig, ProjectFileRequest, PromptSoundRequest, SkipObjectsRequest,
};
use serde::Serialize;

use crate::connection::{Printer, create_printer};
use crate::error::CliError;

/// Default per-command listening window. Comfortably longer than the sub-second ack latency
/// `probe.rs` runs have observed on a P1S, while keeping a full default sweep short enough to
/// watch interactively.
const DEFAULT_ACK_WINDOW_SECS: u64 = 5;
/// Time allowed for `gcode_state` to arrive before the busy gate gives up and refuses to run.
/// Covers a full `pushall` round trip, not just an incremental delta — matches `probe.rs`'s own
/// `PUSHALL_TIMEOUT_SECS`.
const BUSY_WARMUP_SECS: u64 = 10;
/// Filename used by the `project_file` test. `project_file` *starts a print job*, so the test
/// names a file that cannot exist on the SD card rather than a real one — nothing prints.
///
/// This does **not** make the test consequence-free, and the original claim here that "the
/// firmware rejects it" was wrong. The ack is receipt-only [REF-MQTT-ACK]: the printer returns
/// `result: "success"`, then asynchronously tries to fetch the file, fails to read it, and
/// raises a panel-latched `0500_C010` MicroSD read/write exception well after the capture window
/// has closed — the same failure mode [REF-FTPS-FLUSH] documents for a file dispatched before
/// its write buffers flushed. Observed on a real P1S. [`clear_project_file_error`] sends
/// `clean_print_error` afterwards to clear it.
const NONEXISTENT_PROJECT_FILE: &str = "__bambino_ack_probe_absent__.3mf";
/// How long to let `0500_C010` latch before clearing it. The error surfaces asynchronously,
/// after the ack; clearing too eagerly leaves it to appear once the harness has already exited.
const PROJECT_FILE_ERROR_SETTLE_SECS: u64 = 10;

/// Verdict strings recorded per entry and printed in the summary table.
mod verdict {
    /// A response echoing our exact `sequence_id` arrived — eligible for `ACK_CORRELATED_COMMANDS`.
    pub const ACK: &str = "ack_correlated";
    /// No response echoed our `sequence_id`, but other traffic did arrive — the connection was
    /// alive and the printer still said nothing, which is real evidence of "no ack".
    pub const NO_ACK: &str = "no_ack";
    /// Nothing at all arrived during the window. Says nothing about the command — the session
    /// may simply have been quiet (or dead). Re-run rather than recording this as evidence.
    pub const NO_TRAFFIC: &str = "inconclusive_no_traffic";
    /// The PUBLISH itself failed; the command never reached the printer.
    pub const PUBLISH_FAILED: &str = "publish_failed";
    /// The listening loop errored out mid-window.
    pub const CAPTURE_FAILED: &str = "capture_failed";
    /// A message echoed our `sequence_id` but identified itself as `push_status`. Should be
    /// impossible (the two counters are disjoint [REF-MQTT-ACK]); treated as unusable rather
    /// than silently counted as an ack.
    pub const AMBIGUOUS: &str = "ambiguous_push_status_collision";
}

/// One command under test.
///
/// All eight were confirmed ack-correlated on a P1S (issue #26) and are now on
/// `ACK_CORRELATED_COMMANDS`. They stay here rather than being deleted: that evidence is
/// model-specific, so the same sweep is what confirms (or refutes) the allowlist on any other
/// model, and re-running it is the cheap way to re-verify after a firmware update. Add a variant
/// for any future command before putting it on the allowlist, never after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AckTest {
    AmsControl,
    AmsGetRfid,
    AmsChangeFilament,
    SkipObjects,
    ProjectFile,
    SetAirduct,
    PrintOption,
    BuzzerCtrl,
}

impl AckTest {
    /// Test selector accepted by `-t`/`--tests`. Kept identical to the wire command name so a
    /// summary line can be pasted straight into `ACK_CORRELATED_COMMANDS`.
    fn name(&self) -> &'static str {
        self.wire_command()
    }

    /// The `command` string the printer sees, and the exact literal that would be added to
    /// `ACK_CORRELATED_COMMANDS` on a positive result.
    fn wire_command(&self) -> &'static str {
        match self {
            Self::AmsControl => "ams_control",
            Self::AmsGetRfid => "ams_get_rfid",
            Self::AmsChangeFilament => "ams_change_filament",
            Self::SkipObjects => "skip_objects",
            Self::ProjectFile => "project_file",
            Self::SetAirduct => "set_airduct",
            Self::PrintOption => "print_option",
            Self::BuzzerCtrl => "buzzer_ctrl",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::AmsControl => "AMS feed resume (inert while idle)",
            Self::AmsGetRfid => {
                "RFID scan of AMS 0 slot 0 — advances filament to the reader node; a \"no tag\" \
                 response still answers the ack question, no genuine Bambu spool required"
            }
            Self::AmsChangeFilament => {
                "Unload from AMS 0 (slot/target 255, firmware-chosen temps). PHYSICAL: actuates \
                 the feeder and may heat the nozzle"
            }
            Self::SkipObjects => "Skip object index 1 (inert while no print is active)",
            Self::ProjectFile => {
                "Start a print of a deliberately nonexistent file. PHYSICAL: this is the \
                 print-start command, and it latches a 0500_C010 SD read/write error on the \
                 panel, which this harness clears afterwards"
            }
            Self::SetAirduct => {
                "Airduct damper to cooling — may be unsupported on P1/A1 (no chamber damper); a \
                 rejection response is a valid ack, silence means not-applicable"
            }
            Self::PrintOption => "Enable notification sounds (A1/A1 Mini/A2L feature)",
            Self::BuzzerCtrl => "Buzzer to silent/disarmed (H2-series feature)",
        }
    }

    /// True for commands that can actuate hardware or start a job even with the inert-most
    /// arguments this harness can give them. Excluded from the default sweep and gated behind
    /// an interactive confirmation, so `ack-probe` with no `-t` is safe to run unattended on an
    /// idle machine.
    fn is_physically_actuating(&self) -> bool {
        matches!(self, Self::AmsChangeFilament | Self::ProjectFile)
    }

    fn all_known() -> &'static [AckTest] {
        &[
            Self::AmsControl,
            Self::AmsGetRfid,
            Self::AmsChangeFilament,
            Self::SkipObjects,
            Self::ProjectFile,
            Self::SetAirduct,
            Self::PrintOption,
            Self::BuzzerCtrl,
        ]
    }

    /// Tests run when `-t`/`--tests` is omitted — everything except the physically actuating
    /// commands, which must be named explicitly.
    fn default_set() -> Vec<AckTest> {
        Self::all_known()
            .iter()
            .copied()
            .filter(|t| !t.is_physically_actuating())
            .collect()
    }

    fn from_name(name: &str) -> Option<AckTest> {
        Self::all_known().iter().find(|t| t.name() == name).copied()
    }

    /// Builds this test's wire payload with `seq` as its `sequence_id`.
    ///
    /// Arguments are chosen to be the least consequential ones the command accepts — the point
    /// is to observe the *response envelope*, not to make the command succeed. A firmware
    /// rejection is just as good an ack as a success [REF-MQTT-ACK].
    fn build_payload(&self, model: PrinterModel, seq: u64) -> Result<serde_json::Value, CliError> {
        let value = match self {
            Self::AmsControl => serde_json::to_value(AmsControlRequest::new("resume", seq)),
            Self::AmsGetRfid => serde_json::to_value(AmsGetRfidRequest::new(0, 0, seq)),
            Self::AmsChangeFilament => {
                serde_json::to_value(AmsChangeFilamentRequest::new(0, 255, 255, -1, -1, seq))
            }
            Self::SkipObjects => serde_json::to_value(SkipObjectsRequest::new(vec![1], seq)),
            Self::ProjectFile => {
                let config = PrintJobConfig::new(
                    NONEXISTENT_PROJECT_FILE,
                    "Metadata/plate_1.gcode",
                    "bambino ack probe",
                    seq,
                    "textured",
                );
                serde_json::to_value(ProjectFileRequest::from_config(&config, seq, model))
            }
            Self::SetAirduct => {
                serde_json::to_value(AirductRequest::new(AirductMode::Cooling, seq))
            }
            Self::PrintOption => serde_json::to_value(PromptSoundRequest::new(true, seq)),
            Self::BuzzerCtrl => {
                serde_json::to_value(BuzzerRequest::new(BuzzerMode::Silent as i32, seq))
            }
        };

        value.map_err(|e| {
            CliError::Other(format!(
                "failed to serialize {} payload: {e}",
                self.wire_command()
            ))
        })
    }
}

#[derive(Serialize)]
struct ObservedMessage {
    elapsed_ms: u64,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct AckEntry {
    test: String,
    wire_command: String,
    description: String,
    /// The `sequence_id` actually put on the wire, as a string (matching the wire encoding).
    sequence_id: String,
    sent_payload: serde_json::Value,
    window_secs: u64,
    verdict: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    publish_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_error: Option<String>,
    /// The full correlated response, verbatim, when one arrived.
    #[serde(skip_serializing_if = "Option::is_none")]
    ack: Option<ObservedMessage>,
    /// Top-level wrapper key of the correlated response (`print`/`system`/`info`/…).
    #[serde(skip_serializing_if = "Option::is_none")]
    ack_wrapper: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ack_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ack_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ack_reason: Option<String>,
    /// Every message seen during the window that did *not* echo our `sequence_id`. A non-zero
    /// count is what makes a `no_ack` verdict meaningful rather than inconclusive.
    uncorrelated_message_count: usize,
    /// Distinct `command` names among those uncorrelated messages, for the report reader.
    uncorrelated_commands: Vec<String>,
}

/// No `serial` field, deliberately — same reasoning as `ProbeReport` in `probe.rs`: the serial
/// is a credential, `-o/--output` takes an arbitrary path outside `.gitignore`'s
/// `ack_probe_report*.json` glob, and stderr conveys it to the operator without writing it down.
#[derive(Serialize)]
struct AckReport {
    model: String,
    timestamp: u64,
    window_secs: u64,
    tests: Vec<AckEntry>,
}

/// Returns the payload's single top-level wrapper object (`print`/`system`/`pushing`/`info`) —
/// mirrors `extract_sequence_id`'s traversal in `src/mqtt/client/mod.rs`, which is the code
/// whose behavior this harness exists to justify.
fn wrapper_object(
    payload: &serde_json::Value,
) -> Option<(&str, &serde_json::Map<String, serde_json::Value>)> {
    payload
        .as_object()?
        .iter()
        .find_map(|(key, inner)| Some((key.as_str(), inner.as_object()?)))
}

fn inner_str<'a>(
    inner: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    inner.get(key)?.as_str()
}

/// The one response that echoed our `sequence_id`, decomposed into the fields the report
/// records — the ack envelope's shape varies by command family [REF-MQTT-ACK], so `result`
/// and `reason` are optional even on a genuine ack.
struct AckObservation {
    message: ObservedMessage,
    wrapper: String,
    command: Option<String>,
    result: Option<String>,
    reason: Option<String>,
}

/// Outcome of one listening window.
struct Capture {
    ack: Option<AckObservation>,
    uncorrelated_count: usize,
    uncorrelated_commands: Vec<String>,
    error: Option<String>,
}

/// Listens for `window` after a command was published, returning the first response whose
/// wrapper object echoes `expected_seq` plus a tally of everything else that arrived.
///
/// Keeps listening for the full window even after a match so `uncorrelated_commands` reflects
/// the whole window — the report reader needs to see that background telemetry was flowing
/// alongside the ack, which is what distinguishes a real correlated ack from a lucky read.
async fn capture_ack(client: &mut Printer, expected_seq: &str, window: Duration) -> Capture {
    let start = Instant::now();
    let deadline = start + window;
    let mut ack = None;
    let mut uncorrelated_count = 0usize;
    let mut uncorrelated_commands: Vec<String> = Vec::new();
    let mut error = None;

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        // poll_raw() rather than poll_telemetry(): a rejection ack for an unsupported command
        // may not deserialize into a typed telemetry event at all, and the raw envelope is
        // exactly what the correlation logic under test operates on.
        let message = match tokio::time::timeout(remaining, client.poll_raw()).await {
            Ok(Ok(message)) => message,
            Ok(Err(e)) => {
                error = Some(e.to_string());
                break;
            }
            Err(_) => break,
        };

        let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&message.payload) else {
            continue;
        };
        let Some((wrapper, inner)) = wrapper_object(&payload) else {
            continue;
        };
        let command = inner_str(inner, "command").map(str::to_string);

        if inner_str(inner, "sequence_id") == Some(expected_seq) {
            if ack.is_none() {
                ack = Some(AckObservation {
                    message: ObservedMessage {
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        payload: payload.clone(),
                    },
                    wrapper: wrapper.to_string(),
                    command,
                    result: inner_str(inner, "result").map(str::to_string),
                    reason: inner_str(inner, "reason").map(str::to_string),
                });
            }
            continue;
        }

        uncorrelated_count += 1;
        if let Some(command) = command
            && !uncorrelated_commands.contains(&command)
        {
            uncorrelated_commands.push(command);
        }
    }

    Capture {
        ack,
        uncorrelated_count,
        uncorrelated_commands,
        error,
    }
}

async fn run_one(
    client: &mut Printer,
    idx: usize,
    total: usize,
    test: AckTest,
    window: Duration,
) -> Result<AckEntry, CliError> {
    let model = client.model();
    let seq = client.next_sequence_id();
    let payload_value = test.build_payload(model, seq)?;
    // The clamped sequence_id the constructor actually wrote, not the raw counter — these
    // differ once the counter wraps TASK_ID_MAX, and it is the wire value we must match.
    let sequence_id = wrapper_object(&payload_value)
        .and_then(|(_, inner)| inner_str(inner, "sequence_id"))
        .ok_or_else(|| {
            CliError::Other(format!(
                "{} payload has no sequence_id to correlate against",
                test.wire_command()
            ))
        })?
        .to_string();
    let payload_bytes = serde_json::to_vec(&payload_value)
        .map_err(|e| CliError::Other(format!("failed to encode {} payload: {e}", test.name())))?;

    eprint!(
        "[{}/{}] {} (seq {}, {}s window)... ",
        idx + 1,
        total,
        test.name(),
        sequence_id,
        window.as_secs()
    );
    io::stderr().flush().unwrap_or(());

    let publish_result: Result<u16, Error> = match client.mqtt().await {
        Ok(mqtt) => mqtt.publish_command(&payload_bytes).await,
        Err(e) => Err(e),
    };

    let mut entry = AckEntry {
        test: test.name().to_string(),
        wire_command: test.wire_command().to_string(),
        description: test.description().to_string(),
        sequence_id: sequence_id.clone(),
        sent_payload: payload_value,
        window_secs: window.as_secs(),
        verdict: verdict::PUBLISH_FAILED,
        publish_error: None,
        capture_error: None,
        ack: None,
        ack_wrapper: None,
        ack_command: None,
        ack_result: None,
        ack_reason: None,
        uncorrelated_message_count: 0,
        uncorrelated_commands: Vec::new(),
    };

    if let Err(e) = publish_result {
        eprintln!("publish failed: {e}");
        entry.publish_error = Some(e.to_string());
        return Ok(entry);
    }

    let capture = capture_ack(client, &sequence_id, window).await;
    entry.uncorrelated_message_count = capture.uncorrelated_count;
    entry.uncorrelated_commands = capture.uncorrelated_commands;
    entry.capture_error = capture.error;

    entry.verdict = match capture.ack {
        Some(observation) => {
            let ambiguous = observation.command.as_deref() == Some("push_status");
            entry.ack_wrapper = Some(observation.wrapper);
            entry.ack_command = observation.command;
            entry.ack_result = observation.result;
            entry.ack_reason = observation.reason;
            entry.ack = Some(observation.message);
            if ambiguous {
                verdict::AMBIGUOUS
            } else {
                verdict::ACK
            }
        }
        None if entry.capture_error.is_some() => verdict::CAPTURE_FAILED,
        None if entry.uncorrelated_message_count > 0 => verdict::NO_ACK,
        None => verdict::NO_TRAFFIC,
    };

    eprintln!(
        "{}{} ({} uncorrelated message{})",
        entry.verdict,
        entry
            .ack
            .as_ref()
            .map(|a| format!(
                " [{}.{} result={} in {}ms]",
                entry.ack_wrapper.as_deref().unwrap_or("?"),
                entry.ack_command.as_deref().unwrap_or("?"),
                entry.ack_result.as_deref().unwrap_or("?"),
                a.elapsed_ms
            ))
            .unwrap_or_default(),
        entry.uncorrelated_message_count,
        if entry.uncorrelated_message_count == 1 {
            ""
        } else {
            "s"
        }
    );

    Ok(entry)
}

/// Refuses to run against a printer that is printing, preparing, or paused.
///
/// Not politeness: `skip_objects` and `project_file` are only *inert* while idle — skipping an
/// object mid-print destroys the job, and [REF-MQTT-REPLAY] documents a `project_file` dispatch
/// during an active print halting the motion controller with `0500_4003`. Bails rather than
/// silently skipping those two tests, since a run started mid-print says nothing trustworthy
/// about the other commands' ack behavior either.
///
/// Requests a `pushall` first rather than just waiting on the incremental stream: an idle
/// printer's deltas frequently carry no `gcode_state` field at all, so polling alone leaves the
/// cache empty and the gate refuses a perfectly idle machine. `monitor` and `probe` both open
/// the same way.
async fn refuse_if_busy(client: &mut Printer) -> Result<(), CliError> {
    client.request_pushall().await?;

    let deadline = Instant::now() + Duration::from_secs(BUSY_WARMUP_SECS);
    while Instant::now() < deadline && client.print_status().is_none() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, client.poll_telemetry()).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(CliError::Library(e)),
            Err(_) => break,
        }
    }

    match client.print_status() {
        Some(status @ (PrintStatus::Preparing | PrintStatus::Running | PrintStatus::Paused)) => {
            Err(CliError::Other(format!(
                "printer is busy (gcode_state={status:?}) — ack-probe refuses to run during a \
                 print; skip_objects and project_file are destructive in that state"
            )))
        }
        Some(_) => Ok(()),
        None => Err(CliError::Other(format!(
            "no gcode_state received within {BUSY_WARMUP_SECS}s of a pushall — cannot confirm the \
             printer is idle, refusing to run"
        ))),
    }
}

/// Clears the `0500_C010` the `project_file` test induces (see [`NONEXISTENT_PROJECT_FILE`]).
///
/// The error latches on the printer's panel asynchronously, after the ack the test correlates
/// against — leaving it set would strand the operator with a hardware fault raised by a
/// diagnostic tool. Waits before clearing so the clear cannot race ahead of the error appearing.
///
/// Best-effort and deliberately non-fatal: this runs after every test has already been recorded,
/// so a failure here must not cost the caller the report. Says so on stderr instead.
async fn clear_project_file_error(client: &mut Printer) {
    eprint!(
        "\nClearing the 0500_C010 induced by project_file (waiting {}s for it to latch)... ",
        PROJECT_FILE_ERROR_SETTLE_SECS
    );
    io::stderr().flush().unwrap_or(());

    tokio::time::sleep(Duration::from_secs(PROJECT_FILE_ERROR_SETTLE_SECS)).await;

    match client.clear_print_error().await {
        Ok(_) => eprintln!("sent."),
        Err(e) => eprintln!(
            "failed: {e}\n  Clear it manually: bambino-cli control <IP> <SERIAL> clear-error"
        ),
    }

    eprintln!(
        "  clean_print_error is confirmed to clear this on a P1S; if the panel still shows \
         0500_C010, reinsert the MicroSD card."
    );
}

fn confirm_actuating_tests(tests: &[AckTest]) -> Result<bool, CliError> {
    let actuating: Vec<&str> = tests
        .iter()
        .filter(|t| t.is_physically_actuating())
        .map(|t| t.name())
        .collect();
    if actuating.is_empty() {
        return Ok(true);
    }

    eprintln!(
        "\
WARNING: the following selected tests actuate hardware or dispatch a print job:

  {}

`ams_change_filament` moves the feeder and may heat the nozzle.

`project_file` is the print-start command. It targets a deliberately nonexistent file, so
nothing prints — but the ack is receipt-only, and the printer then fails to read that file and
latches a `0500_C010` MicroSD read/write exception on its panel. This harness sends
`clean_print_error` afterwards to clear it, which is confirmed to work on a P1S. If that clear
does not take, run `bambino-cli control <IP> <SERIAL> clear-error`, or reinsert the card.

Clear the build plate, make sure no print is queued, and type 'yes' to continue.",
        actuating.join("\n  ")
    );

    let mut confirmation = String::new();
    io::stdin().read_line(&mut confirmation)?;
    if confirmation.trim().to_lowercase() != "yes" {
        eprintln!("Aborted (expected 'yes').");
        return Ok(false);
    }
    Ok(true)
}

fn select_tests(tests_arg: Option<&str>) -> Result<Vec<AckTest>, CliError> {
    let Some(arg) = tests_arg else {
        return Ok(AckTest::default_set());
    };

    let mut selected = Vec::new();
    for name in arg.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match AckTest::from_name(name) {
            Some(t) => selected.push(t),
            None => {
                eprintln!("Unknown test: '{}'. Available tests:", name);
                for t in AckTest::all_known() {
                    let gated = if t.is_physically_actuating() {
                        " (actuates hardware — not run by default)"
                    } else {
                        ""
                    };
                    eprintln!("  {} — {}{}", t.name(), t.description(), gated);
                }
                return Err(CliError::InvalidArgs(format!(
                    "Unknown test name: '{}'",
                    name
                )));
            }
        }
    }
    if selected.is_empty() {
        return Err(CliError::InvalidArgs(
            "--tests matched no test names".to_string(),
        ));
    }
    Ok(selected)
}

/// Prints the verdict table plus the copy-paste line for `ACK_CORRELATED_COMMANDS`.
fn print_summary(report: &AckReport) {
    eprintln!("\nack-probe summary ({}):", report.model);
    for entry in &report.tests {
        eprintln!(
            "  {:<20} {:<32} {}",
            entry.wire_command,
            entry.verdict,
            entry
                .ack_result
                .as_deref()
                .map(|r| format!("result={r}"))
                .unwrap_or_else(|| format!("{} uncorrelated", entry.uncorrelated_message_count))
        );
    }

    let confirmed: Vec<&str> = report
        .tests
        .iter()
        .filter(|e| e.verdict == verdict::ACK)
        .map(|e| e.wire_command.as_str())
        .collect();
    if confirmed.is_empty() {
        eprintln!("\nNothing confirmed this run — ACK_CORRELATED_COMMANDS unchanged.");
    } else {
        eprintln!(
            "\nConfirmed ack-correlated — add to ACK_CORRELATED_COMMANDS (src/mqtt/client/mod.rs),\n\
             citing this report as the evidence source:\n    {}",
            confirmed
                .iter()
                .map(|c| format!("\"{c}\","))
                .collect::<Vec<_>>()
                .join("\n    ")
        );
    }

    if report
        .tests
        .iter()
        .any(|e| e.verdict == verdict::NO_TRAFFIC)
    {
        eprintln!(
            "\nSome tests saw no traffic at all — that is inconclusive, not evidence of \
             \"no ack\". Re-run those with a longer --window."
        );
    }
}

pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    output: &str,
    tests_arg: Option<&str>,
    window_secs: Option<u64>,
) -> Result<(), CliError> {
    let tests = select_tests(tests_arg)?;
    let window = Duration::from_secs(window_secs.unwrap_or(DEFAULT_ACK_WINDOW_SECS));
    if window.is_zero() {
        return Err(CliError::InvalidArgs(
            "--window must be at least 1 second".to_string(),
        ));
    }

    if !confirm_actuating_tests(&tests)? {
        return Ok(());
    }

    eprintln!("Connecting to {}:8883...", ip);
    let mut client = create_printer(ip, serial, access_code)?;
    client.connect_mqtt().await?;
    eprintln!("Connected.");

    refuse_if_busy(&mut client).await?;

    eprintln!(
        "Probing {} command(s) for sequence_id-correlated acks...\n",
        tests.len()
    );

    let model = client.model();
    let serial_owned = client.serial().to_string();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut entries = Vec::new();
    for (idx, test) in tests.iter().enumerate() {
        entries.push(run_one(&mut client, idx, tests.len(), *test, window).await?);
    }

    if tests.contains(&AckTest::ProjectFile) {
        clear_project_file_error(&mut client).await;
    }

    eprintln!("Probing {} (serial {})", format_args!("{:?}", model), serial_owned);

    let report = AckReport {
        model: format!("{:?}", model),
        timestamp,
        window_secs: window.as_secs(),
        tests: entries,
    };

    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| CliError::Other(format!("failed to serialize ack report: {e}")))?;
    std::fs::write(output, json.as_bytes())?;

    print_summary(&report);
    eprintln!("\nReport written to {}", output);
    Ok(())
}
