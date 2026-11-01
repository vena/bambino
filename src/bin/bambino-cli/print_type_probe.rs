#![cfg(feature = "cli")]

//! # Screen-Initiated Job Recorder
//!
//! Records what `print.print_type`, `subtask_name`, `gcode_state`, `gcode_file` and
//! `mc_print_stage` read while jobs started from the printer's own screen run (issue #654).
//! The operator taps each job on the screen; this tool only listens.
//!
//! The only command ever published is `pushall`, a read request. Nothing here moves or heats
//! the machine. `reference/03_mqtt_telemetry.md` ("P1/A1 Series Rate Limit") records an
//! unverified caution against frequent `pushall` on P1/A1, so besides one at the start for the
//! idle baseline, each job gets at most two: one [`MID_JOB_PUSHALL_DELAY`] after `gcode_state`
//! first leaves idle, so a full report lands mid-job and shows a `print_type` that deltas omit
//! (none for a job whose state never leaves idle), and one when the operator ends the job.
//!
//! No `serial` in the report, for the reason on `ProbeReport` in `probe.rs`: every stored frame
//! goes through [`redact_secrets`].

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::time::Duration;

use bambino::Error;
use bambino::client::PrintStatus;
use bambino::io::SocketError;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::time::{Instant, Interval, sleep_until, timeout};

use crate::connection::{
    Printer, RESPONSE_TIMEOUT_SECS, Target, poll_events_for, unix_now_secs, write_report,
};
use crate::error::CliError;
use crate::redact::redact_secrets;

/// `print` keys this recorder watches.
const TRACKED_KEYS: [&str; 5] = [
    "print_type",
    "subtask_name",
    "gcode_state",
    "gcode_file",
    "mc_print_stage",
];

/// A `print` object with more keys than this is a full status report.
///
/// Mirrors the library's `FULL_REPORT_MIN_KEYS` (`src/types/telemetry/mod.rs`), which is
/// `pub(crate)` and so unreachable from this binary. Keep the two equal.
const FULL_REPORT_MIN_KEYS: usize = 30;

/// Delay between `gcode_state` first leaving idle and the mid-job `pushall`.
const MID_JOB_PUSHALL_DELAY: Duration = Duration::from_secs(10);

/// How long to wait for the end-of-job `pushall` response before moving on.
const END_PUSHALL_WAIT: Duration = Duration::from_secs(RESPONSE_TIMEOUT_SECS);

/// Jobs the operator is walked through, in order: label and screen instruction.
const JOBS: [(&str, &str); 4] = [
    ("load", "Load filament from the printer's screen"),
    ("unload", "Unload filament from the printer's screen"),
    (
        "calibration",
        "Run calibration from the printer's screen (a P1S has one option that runs the whole \
         suite; on a printer with separate routines, add each as an extra job afterwards)",
    ),
    (
        "file_print",
        "Start a print from the printer's own file list on its screen; cancel it from the \
         screen once it is printing",
    ),
];

/// One change of a tracked field.
#[derive(Debug, Clone, PartialEq, Serialize)]
struct Transition {
    elapsed_ms: u64,
    field: &'static str,
    value: Value,
    /// Whether the carrying frame had a full-report-sized `print` object.
    full_report: bool,
}

/// A redacted raw frame that carried at least one tracked key.
#[derive(Serialize)]
struct FrameRecord {
    elapsed_ms: u64,
    frame: Value,
}

/// Tracks the last value of each tracked field and what changed.
#[derive(Default)]
struct JobRecorder {
    last: BTreeMap<&'static str, Value>,
    transitions: Vec<Transition>,
    frames: Vec<FrameRecord>,
}

impl JobRecorder {
    /// Feeds one decoded frame and returns the transitions it produced.
    ///
    /// A field is recorded on first sighting and whenever it differs from the last value seen.
    /// A field the frame omits is not a change. A frame carrying any tracked key is kept,
    /// redacted.
    fn observe(&mut self, elapsed_ms: u64, frame: &Value) -> Vec<Transition> {
        let Some(print) = frame.get("print").and_then(Value::as_object) else {
            return Vec::new();
        };
        let full_report = print.len() > FULL_REPORT_MIN_KEYS;
        let mut carries_tracked = false;
        let mut new = Vec::new();
        for field in TRACKED_KEYS {
            let Some(value) = print.get(field) else {
                continue;
            };
            carries_tracked = true;
            if self.last.get(field) == Some(value) {
                continue;
            }
            self.last.insert(field, value.clone());
            new.push(Transition {
                elapsed_ms,
                field,
                value: value.clone(),
                full_report,
            });
        }
        if carries_tracked {
            self.frames.push(FrameRecord {
                elapsed_ms,
                frame: redact_secrets(frame.clone()),
            });
        }
        self.transitions.extend(new.iter().cloned());
        new
    }

    /// Last seen value of every tracked field, `null` for one never seen.
    fn snapshot(&self) -> serde_json::Map<String, Value> {
        TRACKED_KEYS
            .iter()
            .map(|&k| {
                (
                    k.to_string(),
                    self.last.get(k).cloned().unwrap_or(Value::Null),
                )
            })
            .collect()
    }
}

/// Distinct values `field` took across `transitions`, in first-seen order.
fn distinct_values(transitions: &[Transition], field: &str) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    for t in transitions.iter().filter(|t| t.field == field) {
        let rendered = t.value.to_string();
        if !seen.contains(&rendered) {
            seen.push(rendered);
        }
    }
    seen
}

/// Whether `gcode_state` marks a job in flight (anything but idle, finished or failed).
fn leaves_idle(gcode_state: &str) -> bool {
    !matches!(
        PrintStatus::from_gcode_state(gcode_state),
        PrintStatus::Idle | PrintStatus::Finished | PrintStatus::Failed
    )
}

#[derive(Serialize)]
struct JobEntry {
    label: String,
    description: String,
    skipped: bool,
    transitions: Vec<Transition>,
    frames: Vec<FrameRecord>,
}

#[derive(Serialize)]
struct Report {
    model: String,
    timestamp: u64,
    baseline: serde_json::Map<String, Value>,
    jobs: Vec<JobEntry>,
}

/// One thing the session loop produced.
enum Step {
    /// A decoded telemetry frame.
    Frame(Value),
    /// A line the operator typed.
    Line(String),
    /// The requested `pushall` deadline passed.
    PushallDue,
    /// Stdin closed.
    Eof,
}

/// A connected printer plus the operator's stdin, driven together.
struct Session {
    client: Printer,
    keepalive: Interval,
    lines: UnboundedReceiver<String>,
}

impl Session {
    /// Waits for the next frame, operator line or `pushall_at`, running keepalive meanwhile.
    ///
    /// Same shape as the monitor's loop (`monitor::run`): the `select!` cancels the poll on
    /// every keepalive tick, so the zombie check in `keepalive_tick` must stay in the loop.
    async fn step(&mut self, pushall_at: Option<Instant>) -> Result<Step, CliError> {
        let deadline = pushall_at.unwrap_or_else(|| Instant::now() + Duration::from_secs(86_400));
        loop {
            tokio::select! {
                res = self.client.poll_telemetry() => match res {
                    Ok(event) => {
                        if let Some(raw) = event.raw()
                            && let Ok(v) = serde_json::from_slice::<Value>(&raw.payload)
                        {
                            return Ok(Step::Frame(v));
                        }
                    }
                    // The wire went quiet, not dead; a dead link is caught by the keepalive.
                    Err(Error::Network(SocketError::TimedOut)) => {}
                    Err(e) => return Err(e.into()),
                },
                _ = self.keepalive.tick() => self.client.keepalive_tick().await?,
                line = self.lines.recv() => {
                    return Ok(match line {
                        Some(l) => Step::Line(l),
                        None => Step::Eof,
                    });
                }
                _ = sleep_until(deadline) => {
                    if pushall_at.is_some() {
                        return Ok(Step::PushallDue);
                    }
                }
            }
        }
    }

    /// Prompts on stderr and returns the operator's trimmed answer, polling meanwhile.
    async fn ask(&mut self, prompt: &str) -> Result<String, CliError> {
        while self.lines.try_recv().is_ok() {}
        eprint!("{prompt}");
        io::stderr().flush().unwrap_or(());
        loop {
            match self.step(None).await? {
                Step::Line(l) => return Ok(l.trim().to_string()),
                Step::Eof => return Err(CliError::Io(io::ErrorKind::UnexpectedEof.into())),
                Step::Frame(_) | Step::PushallDue => {}
            }
        }
    }

    /// Records one job from the operator's go-ahead until they press Enter again.
    async fn record(&mut self, label: &str, description: &str) -> Result<JobEntry, CliError> {
        let mut rec = JobRecorder::default();
        let start = Instant::now();
        eprintln!(
            "Recording. Start it on the screen now. Press Enter once it has finished and the \
             screen is idle again."
        );
        let mut pushall_at: Option<Instant> = None;
        let mut left_idle = false;
        loop {
            match self.step(pushall_at).await? {
                Step::Frame(v) => {
                    let elapsed_ms = elapsed_ms(start);
                    for t in rec.observe(elapsed_ms, &v) {
                        print_transition(&t);
                    }
                    if !left_idle
                        && v.get("print")
                            .and_then(|p| p.get("gcode_state"))
                            .and_then(Value::as_str)
                            .is_some_and(leaves_idle)
                    {
                        left_idle = true;
                        pushall_at = Some(Instant::now() + MID_JOB_PUSHALL_DELAY);
                    }
                }
                Step::PushallDue => {
                    pushall_at = None;
                    self.client.request_pushall().await?;
                }
                Step::Line(_) => break,
                Step::Eof => return Err(CliError::Io(io::ErrorKind::UnexpectedEof.into())),
            }
        }

        // End-of-job pushall; keep listening until its full report arrives or the wait lapses.
        self.client.request_pushall().await?;
        let end_deadline = Instant::now() + END_PUSHALL_WAIT;
        'drain: loop {
            let Ok(step) = timeout(
                end_deadline.saturating_duration_since(Instant::now()),
                self.step(None),
            )
            .await
            else {
                break;
            };
            match step? {
                Step::Frame(v) => {
                    let elapsed_ms = elapsed_ms(start);
                    let new = rec.observe(elapsed_ms, &v);
                    for t in &new {
                        print_transition(t);
                    }
                    let full = v
                        .get("print")
                        .and_then(Value::as_object)
                        .is_some_and(|p| p.len() > FULL_REPORT_MIN_KEYS);
                    if full {
                        break 'drain;
                    }
                }
                Step::Eof => return Err(CliError::Io(io::ErrorKind::UnexpectedEof.into())),
                Step::Line(_) | Step::PushallDue => {}
            }
        }

        Ok(JobEntry {
            label: label.to_string(),
            description: description.to_string(),
            skipped: false,
            transitions: rec.transitions,
            frames: rec.frames,
        })
    }
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn print_transition(t: &Transition) {
    let marker = if t.full_report { "  [full report]" } else { "" };
    eprintln!(
        "  +{:.1}s {} = {}{}",
        t.elapsed_ms as f64 / 1000.0,
        t.field,
        t.value,
        marker
    );
}

/// Reads stdin lines on a blocking std thread. Tokio's stdin needs the `io-std` feature,
/// which `Cargo.toml` doesn't enable.
fn spawn_stdin_reader() -> UnboundedReceiver<String> {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        loop {
            let mut line = String::new();
            match stdin.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

/// Walks the operator through each job and writes the report after every one.
pub async fn run(target: &Target, output: &str) -> Result<(), CliError> {
    let mut client = target.connect_mqtt().await?;
    let keepalive = crate::monitor::keepalive_interval().await;

    // Idle baseline from one pushall.
    client.request_pushall().await?;
    let mut baseline_rec = JobRecorder::default();
    poll_events_for(
        &mut client,
        Duration::from_secs(RESPONSE_TIMEOUT_SECS),
        |_, event| {
            if let Some(raw) = event.raw()
                && let Ok(v) = serde_json::from_slice::<Value>(&raw.payload)
            {
                baseline_rec.observe(0, &v);
                if v.get("print").and_then(|p| p.get("gcode_state")).is_some() {
                    return std::ops::ControlFlow::Break(());
                }
            }
            std::ops::ControlFlow::Continue(())
        },
    )
    .await?;

    match client.print_status() {
        Some(PrintStatus::Idle | PrintStatus::Finished | PrintStatus::Failed) => {}
        other => {
            let state = baseline_rec
                .last
                .get("gcode_state")
                .map_or_else(|| "not reported".to_string(), Value::to_string);
            return Err(CliError::InvalidArgs(format!(
                "The printer reports gcode_state {state} ({other:?}), not IDLE, FINISH or FAILED, \
                 so it cannot give a clean baseline. Finish or cancel the current job from the \
                 screen, wait for it to go idle, and run this again."
            )));
        }
    }

    let mut report = Report {
        model: format!("{:?}", client.model()),
        timestamp: unix_now_secs(),
        baseline: baseline_rec.snapshot(),
        jobs: Vec::new(),
    };
    eprintln!(
        "Baseline recorded. Only `pushall` is sent to the printer; nothing is moved or heated."
    );

    let mut session = Session {
        client,
        keepalive,
        lines: spawn_stdin_reader(),
    };

    let mut queue: Vec<(String, String)> = JOBS
        .iter()
        .map(|(l, d)| (l.to_string(), d.to_string()))
        .collect();
    queue.reverse();
    loop {
        let (label, description) = match queue.pop() {
            Some(job) => job,
            None => {
                let label = session
                    .ask(
                        "Any other job started from the screen? Type a short label and press \
                         Enter, or just Enter to finish.\n> ",
                    )
                    .await?;
                if label.is_empty() {
                    break;
                }
                (label.clone(), label)
            }
        };
        let answer = session
            .ask(&format!(
                "\nNext: {description}. Make sure the printer is idle. Press Enter to start \
                 recording, or type s and Enter to skip.\n> "
            ))
            .await?;
        let entry = if answer.eq_ignore_ascii_case("s") {
            JobEntry {
                label,
                description,
                skipped: true,
                transitions: Vec::new(),
                frames: Vec::new(),
            }
        } else {
            session.record(&label, &description).await?
        };
        report.jobs.push(entry);
        write_report(output, &report)?;
    }

    eprintln!("\nSummary:");
    for job in &report.jobs {
        if job.skipped {
            eprintln!("  {}: skipped", job.label);
            continue;
        }
        let join = |field| {
            let v = distinct_values(&job.transitions, field);
            if v.is_empty() {
                "(none seen)".to_string()
            } else {
                v.join(", ")
            }
        };
        eprintln!(
            "  {}: print_type {}; subtask_name {}",
            job.label,
            join("print_type"),
            join("subtask_name")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fields(ts: &[Transition]) -> Vec<(&'static str, String)> {
        ts.iter().map(|t| (t.field, t.value.to_string())).collect()
    }

    #[test]
    fn first_sighting_is_recorded() {
        let mut rec = JobRecorder::default();
        let new = rec.observe(
            5,
            &json!({"print": {"print_type": "system", "gcode_state": "IDLE"}}),
        );
        assert_eq!(
            fields(&new),
            vec![
                ("print_type", "\"system\"".to_string()),
                ("gcode_state", "\"IDLE\"".to_string())
            ]
        );
        assert_eq!(new[0].elapsed_ms, 5);
        assert_eq!(rec.transitions.len(), 2);
    }

    #[test]
    fn unchanged_value_is_not_recorded_again() {
        let mut rec = JobRecorder::default();
        rec.observe(0, &json!({"print": {"print_type": "system"}}));
        let again = rec.observe(1, &json!({"print": {"print_type": "system"}}));
        assert!(again.is_empty());
        let changed = rec.observe(2, &json!({"print": {"print_type": "idle"}}));
        assert_eq!(
            fields(&changed),
            vec![("print_type", "\"idle\"".to_string())]
        );
        assert_eq!(rec.transitions.len(), 2);
    }

    #[test]
    fn field_absent_from_a_delta_is_not_a_change() {
        let mut rec = JobRecorder::default();
        rec.observe(
            0,
            &json!({"print": {"print_type": "system", "gcode_state": "RUNNING"}}),
        );
        let delta = rec.observe(
            1,
            &json!({"print": {"gcode_state": "RUNNING", "mc_percent": 3}}),
        );
        assert!(delta.is_empty());
        assert_eq!(rec.last["print_type"], json!("system"));
        assert_eq!(rec.snapshot()["print_type"], json!("system"));
    }

    #[test]
    fn frames_without_tracked_keys_or_print_are_ignored_and_secrets_redacted() {
        let mut rec = JobRecorder::default();
        rec.observe(0, &json!({"info": {"sn": "x"}}));
        rec.observe(0, &json!({"print": {"mc_percent": 3}}));
        assert!(rec.frames.is_empty());
        rec.observe(1, &json!({"print": {"print_type": "a", "sn": "SECRET"}}));
        assert_eq!(rec.frames.len(), 1);
        assert_eq!(rec.frames[0].frame["print"]["sn"], "<redacted>");
    }

    #[test]
    fn full_report_flag_follows_print_key_count() {
        let mut rec = JobRecorder::default();
        let mut big = serde_json::Map::new();
        for i in 0..=FULL_REPORT_MIN_KEYS {
            big.insert(format!("k{i}"), json!(i));
        }
        big.insert("print_type".into(), json!("system"));
        let new = rec.observe(0, &json!({ "print": big }));
        assert!(new[0].full_report);
        let small = rec.observe(1, &json!({"print": {"print_type": "idle"}}));
        assert!(!small[0].full_report);
    }

    #[test]
    fn idle_detection_and_distinct_values() {
        assert!(!leaves_idle("IDLE"));
        assert!(!leaves_idle("FINISH"));
        assert!(leaves_idle("PREPARE"));
        let mut rec = JobRecorder::default();
        rec.observe(0, &json!({"print": {"print_type": "system"}}));
        rec.observe(1, &json!({"print": {"print_type": "idle"}}));
        rec.observe(2, &json!({"print": {"print_type": "system"}}));
        assert_eq!(
            distinct_values(&rec.transitions, "print_type"),
            vec!["\"system\"".to_string(), "\"idle\"".to_string()]
        );
    }
}
