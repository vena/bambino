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
use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use bambino::Error;
use bambino::client::{
    AirPurificationMode, BuzzerMode, DoorOpenCheck, NozzleBlobDetectMode, XcamHaltSensitivity,
    XcamModule,
};
use bambino::io::tokio::TokioTimer;
use bambino::models::PrinterModel;
use bambino::mqtt::commands::{
    AirPrintDetectRequest, AirPurificationRequest, AmsControlOp, AutoRecoveryRequest, ChangeTemps,
    DoorOpenCheckRequest, FilamentBackupRequest, FilamentTangleDetectRequest,
    IdleHeatingProtectionRequest, NozzleBlobDetectRequest, SmartNozzleBlobDetectRequest,
    StoreSentFilesRequest, XcamControlRequest,
};
use bambino::mqtt::{
    AirductMode, AirductRequest, AmsChangeFilamentRequest, AmsControlRequest, AmsGetRfidRequest,
    BuzzerRequest, GetAccessCodeRequest, PrintJobConfig, ProjectFileRequest, PromptSoundRequest,
    SkipObjectsRequest, echo_key,
};
use serde::Serialize;

use crate::connection::{
    Printer, RESPONSE_TIMEOUT_SECS, Target, poll_raw_for, unix_now_secs, write_report,
};
use crate::error::CliError;
use crate::redact::redact_secrets;

/// Default per-command listening window. Comfortably longer than the sub-second ack latency
/// `probe.rs` runs have observed on a P1S, while keeping a full default sweep short enough to
/// watch interactively.
pub(crate) const DEFAULT_ACK_WINDOW_SECS: u64 = 5;
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
/// The first nine were confirmed ack-correlated on a P1S (the first eight under issue #26,
/// `GetAccessCode` under issue #140) and are on `ACK_CORRELATED_COMMANDS`. Of the last four
/// (#616-#619), only `set_against_continued_heating_mode` acked on a P1S; the other three drew
/// no reply on that model, which lacks all three features. Entries stay here rather than being
/// deleted: that evidence is model-specific, so the same sweep is what confirms (or refutes) the
/// allowlist on any other model, and re-running it is the cheap way to re-verify after a firmware update. Add a variant
/// for any future command before putting it on the allowlist, never after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AckTest {
    AmsControl,
    AmsGetRfid,
    AmsChangeFilament,
    SkipObjects,
    ProjectFile,
    SetAirduct,
    PrintOption,
    BuzzerCtrl,
    GetAccessCode,
    XcamControlSet,
    SetDoorStat,
    SetAgainstContinuedHeatingMode,
    PrintCacheSet,
    PrintOptionAutoRecovery,
    PrintOptionFilamentBackup,
    PrintOptionTangleDetect,
    PrintOptionNozzleBlobDetect,
    PrintOptionSmartNozzleBlobDetect,
    PrintOptionAirPrintDetect,
    PrintOptionAirPurification,
}

impl AckTest {
    /// The `command` string the printer sees, and the exact literal that would be added to
    /// `ACK_CORRELATED_COMMANDS` on a positive result. Also the `-t`/`--tests` selector for every
    /// test whose wire name is unique — see [`selector`](Self::selector).
    fn wire_command(&self) -> &'static str {
        match self {
            Self::AmsControl => AmsControlRequest::COMMAND,
            Self::AmsGetRfid => AmsGetRfidRequest::COMMAND,
            Self::AmsChangeFilament => AmsChangeFilamentRequest::COMMAND,
            Self::SkipObjects => SkipObjectsRequest::COMMAND,
            Self::ProjectFile => ProjectFileRequest::COMMAND,
            Self::SetAirduct => AirductRequest::COMMAND,
            Self::PrintOption => PromptSoundRequest::COMMAND,
            Self::BuzzerCtrl => BuzzerRequest::COMMAND,
            Self::GetAccessCode => GetAccessCodeRequest::COMMAND,
            Self::XcamControlSet => XcamControlRequest::COMMAND,
            Self::SetDoorStat => DoorOpenCheckRequest::COMMAND,
            Self::SetAgainstContinuedHeatingMode => IdleHeatingProtectionRequest::COMMAND,
            Self::PrintCacheSet => StoreSentFilesRequest::COMMAND,
            Self::PrintOptionAutoRecovery
            | Self::PrintOptionFilamentBackup
            | Self::PrintOptionTangleDetect
            | Self::PrintOptionNozzleBlobDetect
            | Self::PrintOptionSmartNozzleBlobDetect
            | Self::PrintOptionAirPrintDetect
            | Self::PrintOptionAirPurification => AutoRecoveryRequest::COMMAND,
        }
    }

    /// The `-t`/`--tests` name, unique per test.
    ///
    /// The wire command for most; the `print_option` tests after prompt sound share that wire
    /// name, so each is `print_option.<setting field>`.
    fn selector(&self) -> &'static str {
        match self {
            Self::PrintOptionAutoRecovery => "print_option.auto_recovery",
            Self::PrintOptionFilamentBackup => "print_option.auto_switch_filament",
            Self::PrintOptionTangleDetect => "print_option.filament_tangle_detect",
            Self::PrintOptionNozzleBlobDetect => "print_option.nozzle_blob_detect",
            Self::PrintOptionSmartNozzleBlobDetect => "print_option.nozzle_blob_detect_v2",
            Self::PrintOptionAirPrintDetect => "print_option.air_print_detect",
            Self::PrintOptionAirPurification => "print_option.air_purification",
            _ => self.wire_command(),
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
            Self::GetAccessCode => {
                "Ask the printer to report its own LAN access code (issue #140). Read-only: it \
                 queries a value the caller already had to know to connect, and changes nothing. \
                 A `system`-wrapped reply echoing our sequence_id is the evidence sought"
            }
            Self::XcamControlSet => {
                "Spaghetti detection on at medium (issue #616). SETTING: on a printer with the \
                 detector, this enables it at the firmware default; `xcam`-wrapped request, so \
                 the reply's wrapper is part of what is sought"
            }
            Self::SetDoorStat => {
                "Door-open check to pause the print (issue #617). SETTING: overwrites the \
                 door-open check on a printer with a door sensor"
            }
            Self::SetAgainstContinuedHeatingMode => {
                "Idle heating protection on (issue #618). SETTING: overwrites it on a printer \
                 that supports it"
            }
            Self::PrintCacheSet => {
                "Keep sent print files on external storage (issue #619). SETTING: overwrites it \
                 on a printer that supports it"
            }
            Self::PrintOptionAutoRecovery => {
                "Step-loss auto-recovery on, sending `option` and `auto_recovery` (#615). \
                 SETTING; supported on the P1S"
            }
            Self::PrintOptionFilamentBackup => {
                "AMS Filament Backup on (#615). SETTING; supported on the P1S"
            }
            Self::PrintOptionTangleDetect => "Filament tangle detection on (#615). SETTING",
            Self::PrintOptionNozzleBlobDetect => "Nozzle blob detection on (#615). SETTING",
            Self::PrintOptionSmartNozzleBlobDetect => {
                "Smart nozzle blob detection to on (#615). SETTING"
            }
            Self::PrintOptionAirPrintDetect => {
                "Non-visual air-printing detection on (#615). SETTING"
            }
            Self::PrintOptionAirPurification => {
                "End-of-print air purification to inside (#615). SETTING"
            }
        }
    }

    /// True for commands that overwrite a persistent user setting on a printer that has the feature.
    ///
    /// Excluded from the default sweep, like the physically actuating ones, but needs no
    /// confirmation: each sets the protective value (detector on, pause on door open, idle
    /// heating protection on, keep files), and on a printer without the feature it does nothing.
    fn changes_setting(&self) -> bool {
        matches!(
            self,
            Self::XcamControlSet
                | Self::SetDoorStat
                | Self::SetAgainstContinuedHeatingMode
                | Self::PrintCacheSet
                | Self::PrintOptionAutoRecovery
                | Self::PrintOptionFilamentBackup
                | Self::PrintOptionTangleDetect
                | Self::PrintOptionNozzleBlobDetect
                | Self::PrintOptionSmartNozzleBlobDetect
                | Self::PrintOptionAirPrintDetect
                | Self::PrintOptionAirPurification
        )
    }

    /// True for commands that can actuate hardware or start a job even with the inert-most
    /// arguments this harness can give them. Excluded from the default sweep and gated behind
    /// an interactive confirmation, so `ack-probe` with no `-t` is safe to run unattended on an
    /// idle machine.
    fn is_physically_actuating(&self) -> bool {
        matches!(
            self,
            Self::AmsGetRfid | Self::AmsChangeFilament | Self::ProjectFile
        )
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
            Self::GetAccessCode,
            Self::XcamControlSet,
            Self::SetDoorStat,
            Self::SetAgainstContinuedHeatingMode,
            Self::PrintCacheSet,
            Self::PrintOptionAutoRecovery,
            Self::PrintOptionFilamentBackup,
            Self::PrintOptionTangleDetect,
            Self::PrintOptionNozzleBlobDetect,
            Self::PrintOptionSmartNozzleBlobDetect,
            Self::PrintOptionAirPrintDetect,
            Self::PrintOptionAirPurification,
        ]
    }

    /// Tests run when `-t`/`--tests` is omitted — everything except the physically actuating
    /// and setting-changing commands, which must be named explicitly.
    fn default_set() -> Vec<AckTest> {
        Self::all_known()
            .iter()
            .copied()
            .filter(|t| !t.is_physically_actuating() && !t.changes_setting())
            .collect()
    }

    /// Builds this test's wire payload with `seq` as its `sequence_id`.
    ///
    /// Arguments are chosen to be the least consequential ones the command accepts — the point
    /// is to observe the *response envelope*, not to make the command succeed. A firmware
    /// rejection is just as good an ack as a success [REF-MQTT-ACK].
    fn build_payload(&self, model: PrinterModel, seq: u64) -> Result<serde_json::Value, CliError> {
        let value = match self {
            Self::AmsControl => {
                serde_json::to_value(AmsControlRequest::new(AmsControlOp::Resume, seq))
            }
            Self::AmsGetRfid => serde_json::to_value(AmsGetRfidRequest::new(0, 0, seq)),
            Self::AmsChangeFilament => serde_json::to_value(AmsChangeFilamentRequest::unload(
                0,
                ChangeTemps::FIRMWARE,
                None,
                seq,
            )),
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
            Self::BuzzerCtrl => serde_json::to_value(BuzzerRequest::new(BuzzerMode::Silent, seq)),
            Self::GetAccessCode => serde_json::to_value(GetAccessCodeRequest::new(seq)),
            Self::XcamControlSet => serde_json::to_value(XcamControlRequest::new(
                XcamModule::SpaghettiDetector,
                true,
                Some(XcamHaltSensitivity::Medium),
                seq,
            )),
            Self::SetDoorStat => {
                serde_json::to_value(DoorOpenCheckRequest::new(DoorOpenCheck::PausePrint, seq))
            }
            Self::SetAgainstContinuedHeatingMode => {
                serde_json::to_value(IdleHeatingProtectionRequest::new(true, seq))
            }
            Self::PrintCacheSet => serde_json::to_value(StoreSentFilesRequest::new(true, seq)),
            Self::PrintOptionAutoRecovery => {
                serde_json::to_value(AutoRecoveryRequest::new(true, seq))
            }
            Self::PrintOptionFilamentBackup => {
                serde_json::to_value(FilamentBackupRequest::new(true, seq))
            }
            Self::PrintOptionTangleDetect => {
                serde_json::to_value(FilamentTangleDetectRequest::new(true, seq))
            }
            Self::PrintOptionNozzleBlobDetect => {
                serde_json::to_value(NozzleBlobDetectRequest::new(true, seq))
            }
            Self::PrintOptionSmartNozzleBlobDetect => serde_json::to_value(
                SmartNozzleBlobDetectRequest::new(NozzleBlobDetectMode::On, seq),
            ),
            Self::PrintOptionAirPrintDetect => {
                serde_json::to_value(AirPrintDetectRequest::new(true, seq))
            }
            Self::PrintOptionAirPurification => serde_json::to_value(AirPurificationRequest::new(
                AirPurificationMode::Inside,
                seq,
            )),
        };

        value.map_err(|e| {
            CliError::Other(format!(
                "failed to serialize {} payload: {e}",
                self.wire_command()
            ))
        })
    }
}

/// Lets clap parse `-t` and list the tests, from the same name and description tables.
impl clap::ValueEnum for AckTest {
    fn value_variants<'a>() -> &'a [Self] {
        Self::all_known()
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        let help = if self.is_physically_actuating() {
            format!(
                "{} (actuates hardware — not run by default)",
                self.description()
            )
        } else if self.changes_setting() {
            format!(
                "{} (changes a persistent setting — not run by default)",
                self.description()
            )
        } else {
            self.description().to_owned()
        };
        Some(clap::builder::PossibleValue::new(self.selector()).help(help))
    }
}

#[derive(Serialize)]
struct ObservedMessage {
    elapsed_ms: u64,
    payload: serde_json::Value,
}

#[derive(Serialize)]
struct AckEntry {
    /// The `-t` name, which tells apart tests sharing a `wire_command`.
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

/// Returns the payload's top-level wrapper object (`print`/`system`/`pushing`/`info`), for the report's `ack_wrapper`/`result`/`reason` fields.
///
/// Correlation itself goes through the library's `echo_key`, the code whose behavior this
/// harness exists to justify, rather than a copy of its traversal.
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
async fn capture_ack(client: &mut Printer, expected_seq: u32, window: Duration) -> Capture {
    let start = Instant::now();
    let mut ack = None;
    let mut uncorrelated_count = 0usize;
    let mut uncorrelated_commands: Vec<String> = Vec::new();

    // poll_raw() rather than poll_telemetry(): a rejection ack for an unsupported command may
    // not deserialize into a typed telemetry event at all, and the raw envelope is exactly what
    // the correlation logic under test operates on.
    let polled = poll_raw_for(client, window, |message| {
        let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&message.payload) else {
            return ControlFlow::Continue(());
        };
        let Some((wrapper, inner)) = wrapper_object(&payload) else {
            return ControlFlow::Continue(());
        };
        let command = inner_str(inner, "command").map(str::to_string);

        if echo_key(&message.payload).is_some_and(|key| key.sequence_id == expected_seq) {
            if ack.is_none() {
                ack = Some(AckObservation {
                    message: ObservedMessage {
                        elapsed_ms: start.elapsed().as_millis() as u64,
                        payload: redact_secrets(payload.clone()),
                    },
                    wrapper: wrapper.to_string(),
                    command,
                    result: inner_str(inner, "result").map(str::to_string),
                    reason: inner_str(inner, "reason").map(str::to_string),
                });
            }
            return ControlFlow::Continue(());
        }

        uncorrelated_count += 1;
        if let Some(command) = command
            && !uncorrelated_commands.contains(&command)
        {
            uncorrelated_commands.push(command);
        }
        ControlFlow::Continue(())
    })
    .await;

    Capture {
        ack,
        uncorrelated_count,
        uncorrelated_commands,
        error: polled.err().map(|e| e.to_string()),
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
    let payload_bytes = serde_json::to_vec(&payload_value).map_err(|e| {
        CliError::Other(format!(
            "failed to encode {} payload: {e}",
            test.wire_command()
        ))
    })?;
    let sequence_id = echo_key(&payload_bytes)
        .ok_or_else(|| {
            CliError::Other(format!(
                "{} payload has no sequence_id to correlate against",
                test.wire_command()
            ))
        })?
        .sequence_id;

    eprint!(
        "[{}/{}] {} (seq {}, {}s window)... ",
        idx + 1,
        total,
        test.selector(),
        sequence_id,
        window.as_secs()
    );
    io::stderr().flush().unwrap_or(());

    let publish_result: Result<u16, Error> = match client.mqtt().await {
        Ok(mqtt) => {
            mqtt.publish_command(&payload_bytes, &TokioTimer::new())
                .await
        }
        Err(e) => Err(e),
    };

    let mut entry = AckEntry {
        test: test.selector().to_string(),
        wire_command: test.wire_command().to_string(),
        description: test.description().to_string(),
        sequence_id: sequence_id.to_string(),
        // Redacted like every other JSON value reaching the report, even though no current
        // `build_payload` arm emits a credential. The module invites new `AckTest` variants
        // ("add a variant for any future command before putting it on the allowlist"), and
        // these reports are written expressly to be attached to bug reports — so the guarantee
        // has to hold structurally rather than by each future author remembering it.
        sent_payload: redact_secrets(payload_value),
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

    let capture = capture_ack(client, sequence_id, window).await;
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
    // The outer bound makes the wait wall-clock: refresh_state checks its own timeout only
    // between messages.
    let timeout = Duration::from_secs(RESPONSE_TIMEOUT_SECS);
    if let Ok(refreshed) = tokio::time::timeout(timeout, client.refresh_state(timeout)).await {
        refreshed?;
    }

    match client.print_status() {
        Some(status) if status.is_busy() => Err(CliError::Other(format!(
            "printer is busy (gcode_state={status:?}) — ack-probe refuses to run during a \
                 print; skip_objects and project_file are destructive in that state"
        ))),
        Some(_) => Ok(()),
        None => Err(CliError::Other(format!(
            "no gcode_state received within {RESPONSE_TIMEOUT_SECS}s of a pushall — cannot confirm the \
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
        .map(|t| t.selector())
        .collect();
    if actuating.is_empty() {
        return Ok(true);
    }

    crate::prompt::confirm(&format!(
        "\
WARNING: the following selected tests actuate hardware or dispatch a print job:

  {}

`ams_change_filament` moves the feeder and may heat the nozzle.

`project_file` is the print-start command. It targets a deliberately nonexistent file, so
nothing prints — but the ack is receipt-only, and the printer then fails to read that file and
latches a `0500_C010` MicroSD read/write exception on its panel. This harness sends
`clean_print_error` afterwards to clear it, which is confirmed to work on a P1S. If that clear
does not take, run `bambino-cli control <IP> <SERIAL> clear-error`, or reinsert the card.

Clear the build plate, make sure no print is queued, and type 'yes' to continue.
",
        actuating.join("\n  ")
    ))
}

/// Prints the verdict table plus the copy-paste line for `ACK_CORRELATED_COMMANDS`.
fn print_summary(report: &AckReport) {
    eprintln!("\nack-probe summary ({}):", report.model);
    for entry in &report.tests {
        eprintln!(
            "  {:<36} {:<32} {}",
            entry.test,
            entry.verdict,
            entry
                .ack_result
                .as_deref()
                .map(|r| format!("result={r}"))
                .unwrap_or_else(|| format!("{} uncorrelated", entry.uncorrelated_message_count))
        );
    }

    let mut confirmed: Vec<&str> = Vec::new();
    for entry in report.tests.iter().filter(|e| e.verdict == verdict::ACK) {
        // The print_option tests share a wire name; list it once.
        if !confirmed.contains(&entry.wire_command.as_str()) {
            confirmed.push(&entry.wire_command);
        }
    }
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
    target: &Target,
    output: &str,
    tests: Option<Vec<AckTest>>,
    window_secs: u64,
) -> Result<(), CliError> {
    let tests = tests.unwrap_or_else(AckTest::default_set);
    // `main` bounds --window to 1..=3600, so `capture_ack`'s `Instant + window` can't overflow.
    let window = Duration::from_secs(window_secs);

    if !confirm_actuating_tests(&tests)? {
        return Ok(());
    }

    let mut client = target.connect_mqtt().await?;

    refuse_if_busy(&mut client).await?;

    eprintln!(
        "Probing {} command(s) for sequence_id-correlated acks...\n",
        tests.len()
    );

    let model = client.model();
    let serial_owned = client.serial().to_string();
    let timestamp = unix_now_secs();

    let mut entries = Vec::new();
    for (idx, test) in tests.iter().enumerate() {
        entries.push(run_one(&mut client, idx, tests.len(), *test, window).await?);
    }

    if tests.contains(&AckTest::ProjectFile) {
        clear_project_file_error(&mut client).await;
    }

    eprintln!("Probing {model} (serial {serial_owned})");

    let report = AckReport {
        model: format!("{:?}", model),
        timestamp,
        window_secs: window.as_secs(),
        tests: entries,
    };

    print_summary(&report);
    write_report(output, &report)
}
