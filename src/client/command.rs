//! # Command Handles and Outcomes
//!
//! Every fire-and-forget [`PrinterClient`](super::PrinterClient) command returns a
//! [`CommandHandle`] naming the `sequence_id` it was published under. The printer answers every
//! command except `pushall` with an echo carrying that id [REF-MQTT-ACK].
//! [`poll_telemetry()`](super::PrinterClient::poll_telemetry) decodes that echo into a
//! [`CommandOutcome`] and delivers it as
//! [`TelemetryEvent::Command`](super::TelemetryEvent::Command); when no echo arrives it
//! reports the command as timed out or lost to a disconnect instead, so every echoing command
//! ends in exactly one outcome.
//! [`await_ack()`](super::PrinterClient::await_ack) waits for one command's outcome inline.
//!
//! **An accepted command is a received command, not an executed one.** On a P1S,
//! `set_airduct` and `buzzer_ctrl` ack `result: "success"` on hardware the printer does not
//! have, and `project_file` acks success for a file that does not exist
//! (`reference/03_mqtt_telemetry.md` §REF-MQTT-ACK). Whether a command took effect shows up in
//! later telemetry, and which field depends on the command — see that section's effect-signal
//! table.

#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(feature = "std")]
use std::collections::VecDeque;

use serde::Deserialize;

use crate::diagnostics::{DecodedPrintError, decode_print_error};
use crate::mqtt::client::MQTT_IN_FLIGHT_LIMIT;

/// Upper bound on commands awaiting an echo, matching the MQTT layer's own in-flight bound.
///
/// Reaching it resolves the oldest entry as [`CommandOutcome::TimedOut`] rather than growing
/// without limit on a link where echoes never arrive and the caller never polls.
pub(crate) const PENDING_COMMAND_LIMIT: usize = MQTT_IN_FLIGHT_LIMIT;

/// How many already-delivered outcomes [`await_ack()`](super::PrinterClient::await_ack) can still answer from.
pub(crate) const RESOLVED_OUTCOME_LIMIT: usize = 32;

/// `command` values carried by genuine telemetry pushes rather than by a command echo.
const TELEMETRY_COMMANDS: &[&str] = &["push_status", "pushall"];

/// Whether the printer answers a command with an echo of its `sequence_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AckExpectation {
    /// The printer echoes the command.
    ///
    /// Confirmed on a P1S for every command bambino sends except `pushall`
    /// (`reference/03_mqtt_telemetry.md` §REF-MQTT-ACK); other models are unmeasured.
    Echoes,
    /// The printer sends no echo, so publishing is the whole outcome.
    ///
    /// `pushall` is the one such command: it triggers a state dump instead
    /// [REF-MQTT-LIFECYCLE].
    SettlesOnPublish,
}

/// Names a command this client published, for matching the printer's answer to it.
///
/// Only a [`PrinterClient`](super::PrinterClient) mints one, so a handle always refers to a
/// `sequence_id` this client actually sent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommandHandle {
    command: String,
    sequence_id: u32,
    ack: AckExpectation,
}

impl CommandHandle {
    pub(crate) fn new(command: String, sequence_id: u32, ack: AckExpectation) -> Self {
        Self {
            command,
            sequence_id,
            ack,
        }
    }

    /// Returns the wire command name, e.g. `"gcode_line"` or `"ams_filament_drying"`.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Returns the `sequence_id` the command was published under, which the printer echoes back.
    pub fn sequence_id(&self) -> u32 {
        self.sequence_id
    }

    /// Returns whether an echo is coming for this command.
    pub fn ack(&self) -> AckExpectation {
        self.ack
    }

    pub(crate) fn is_answered_by(&self, echo: &CommandEcho) -> bool {
        echo.command == self.command && echo.sequence_id == Some(self.sequence_id)
    }
}

/// The terminal outcome of one published command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    /// The printer echoed the command with `result: "success"` and no error code.
    ///
    /// Confirms receipt only, not that the command had any effect — see the module docs.
    Accepted,
    /// The printer echoed the command with a failure verdict.
    Refused(CommandRefusal),
    /// The printer echoed the command without any verdict.
    ///
    /// P1S firmware 01.10.00.00 answers some commands with a bare `{command, sequence_id}` and
    /// no `result`, while refusing them through HMS instead (bambuddy #2732). Receipt is all
    /// this proves; it is not success.
    NoVerdict,
    /// No echo arrived before the command's deadline.
    ///
    /// Not evidence of rejection: the command may still have been executed. Deadlines are
    /// [`set_command_timeout()`](super::PrinterClient::set_command_timeout) from publish, and are
    /// only measured with a real clock ([`with_timer()`](super::PrinterClient::with_timer)).
    TimedOut,
    /// The MQTT session ended between publish and echo, so no echo can arrive.
    ///
    /// Sessions use Clean Session and subscribe afresh, and an echo arrives within milliseconds
    /// while a reconnect takes seconds, so an answer addressed to the old session is never
    /// delivered on the new one. The command may still have been executed.
    ConnectionLost,
    /// The command never echoes ([`AckExpectation::SettlesOnPublish`]), so publishing was the
    /// whole outcome.
    SettledOnPublish,
}

/// The printer's stated reasons for refusing a command.
///
/// Every field is as the printer sent it, and any of them may be absent: `result`/`reason` are
/// the generic pair, `err_code` a device error code, and `errno` a per-command code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRefusal {
    /// The echoed `result` string (`"fail"`, `"failed"`, …), when present.
    pub result: Option<String>,
    /// The echoed free-text `reason`, e.g. `"mqtt message verify failed"` when LAN developer
    /// mode is off. `None` when absent or empty.
    pub reason: Option<String>,
    /// Non-zero device error code.
    ///
    /// BambuStudio shows it through the same dialog as the `print_error` register
    /// (`DeviceManager.cpp:3044`), so it decodes the same way — see
    /// [`decoded_error()`](Self::decoded_error).
    pub err_code: Option<u32>,
    /// Non-zero per-command code.
    ///
    /// For `ams_change_filament`, `-2` means the chamber and `-4` the AMS is too hot to load the
    /// filament without softening it; the echo's `soft_temp` field, when present, is the limit
    /// in °C (BambuStudio `DeviceManager.cpp:2993-3016`).
    pub errno: Option<i32>,
}

impl CommandRefusal {
    /// Decodes [`err_code`](Self::err_code) into its `MMMM_CCCC` short code.
    pub fn decoded_error(&self) -> Option<DecodedPrintError> {
        self.err_code.and_then(decode_print_error)
    }
}

/// A command paired with its terminal outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResolution {
    /// The command, as returned when it was published.
    pub handle: CommandHandle,
    /// What became of it.
    pub outcome: CommandOutcome,
}

/// The fields a command echo carries inside its `print`/`system`/`info` wrapper.
///
/// Everything but `command` is a raw [`serde_json::Value`] so an unexpected type in one field
/// degrades that field rather than failing the whole echo.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct EchoFields {
    command: Option<String>,
    sequence_id: Option<serde_json::Value>,
    result: Option<serde_json::Value>,
    reason: Option<serde_json::Value>,
    err_code: Option<serde_json::Value>,
    errno: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct EchoEnvelope {
    print: Option<EchoFields>,
    system: Option<EchoFields>,
    info: Option<EchoFields>,
}

/// A command echo read off the report topic — a frame whose wrapper names a command other than a telemetry push.
#[derive(Debug)]
pub(crate) struct CommandEcho {
    command: String,
    /// `None` when absent or not a decimal number; such an echo can never match a handle.
    sequence_id: Option<u32>,
    fields: EchoFields,
}

/// Reads `payload` as a command echo, or returns `None` for anything else (telemetry, non-JSON).
///
/// Checks every wrapper a command echo arrives under (`print`, `system`, `info`), not only
/// `print`: a `system`-wrapped `ledctrl` echo otherwise deserializes as an empty telemetry
/// report.
pub(crate) fn parse_command_echo(payload: &[u8]) -> Option<CommandEcho> {
    let envelope: EchoEnvelope = serde_json::from_slice(payload).ok()?;
    [envelope.print, envelope.system, envelope.info]
        .into_iter()
        .flatten()
        .find_map(|mut fields| {
            let command = fields.command.take()?;
            if TELEMETRY_COMMANDS.contains(&command.as_str()) {
                return None;
            }
            let sequence_id = match &fields.sequence_id {
                Some(serde_json::Value::String(s)) => s.parse().ok(),
                Some(serde_json::Value::Number(n)) => {
                    n.as_u64().and_then(|n| u32::try_from(n).ok())
                }
                _ => None,
            };
            Some(CommandEcho {
                command,
                sequence_id,
                fields,
            })
        })
}

/// Decodes the verdict an echo carries.
///
/// A refusal is any of: a `result` beginning with `fail` in any case (BambuStudio's
/// `DevCalib.cpp` checks `"fail"`; the signature-verify refusal sends `"failed"`), a non-zero
/// numeric `err_code` (BambuStudio raises its error dialog on one regardless of `result`), or a
/// non-zero numeric `errno`. `result` compares case-insensitively because the inbound
/// `project_file` push spells it `"SUCCESS"`. Anything else without `result: "success"` is
/// [`CommandOutcome::NoVerdict`].
pub(crate) fn decode_verdict(echo: &CommandEcho) -> CommandOutcome {
    let fields = &echo.fields;
    let result = fields.result.as_ref().and_then(|v| v.as_str());
    let err_code = fields
        .err_code
        .as_ref()
        .and_then(|v| v.as_u64())
        .filter(|&code| code != 0)
        .and_then(|code| u32::try_from(code).ok());
    let errno = fields
        .errno
        .as_ref()
        .and_then(|v| v.as_i64())
        .filter(|&code| code != 0)
        .and_then(|code| i32::try_from(code).ok());
    let failed = result.is_some_and(|r| {
        r.get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("fail"))
    });

    if failed || err_code.is_some() || errno.is_some() {
        return CommandOutcome::Refused(CommandRefusal {
            result: result.map(ToString::to_string),
            reason: fields
                .reason
                .as_ref()
                .and_then(|v| v.as_str())
                .filter(|r| !r.is_empty())
                .map(ToString::to_string),
            err_code,
            errno,
        });
    }
    match result {
        Some(r) if r.eq_ignore_ascii_case("success") => CommandOutcome::Accepted,
        _ => CommandOutcome::NoVerdict,
    }
}

struct PendingCommand {
    handle: CommandHandle,
    /// Monotonic `now_millis()` after which the command resolves as timed out; `None` without a
    /// real clock or with the command timeout disabled.
    deadline_ms: Option<u64>,
}

/// Bookkeeping that gives every echoing command exactly one [`CommandOutcome`].
///
/// Commands wait in `pending` (publish order) until an echo matches, their deadline passes, or
/// the connection ends. Outcomes produced without a message — timeouts, disconnects, evictions
/// — queue in `ready` for `poll_telemetry()` to hand out. Every delivered outcome is also kept
/// briefly in `resolved`, so `await_ack()` can answer for a command whose outcome the caller's
/// event loop already consumed.
#[derive(Default)]
pub(crate) struct CommandTracker {
    pending: VecDeque<PendingCommand>,
    ready: VecDeque<CommandResolution>,
    resolved: VecDeque<CommandResolution>,
}

impl CommandTracker {
    /// Starts tracking `handle` if its command echoes.
    pub(crate) fn track(&mut self, handle: &CommandHandle, deadline_ms: Option<u64>) {
        if handle.ack != AckExpectation::Echoes {
            return;
        }
        if self.pending.len() >= PENDING_COMMAND_LIMIT
            && let Some(oldest) = self.pending.pop_front()
        {
            log::warn!(
                "{} commands awaiting an echo; resolving the oldest ({} seq {}) as timed out",
                PENDING_COMMAND_LIMIT,
                oldest.handle.command,
                oldest.handle.sequence_id
            );
            self.ready.push_back(CommandResolution {
                handle: oldest.handle,
                outcome: CommandOutcome::TimedOut,
            });
        }
        self.pending.push_back(PendingCommand {
            handle: handle.clone(),
            deadline_ms,
        });
    }

    /// Removes and returns the pending command `echo` answers, if any.
    pub(crate) fn take_answered(&mut self, echo: &CommandEcho) -> Option<CommandHandle> {
        let index = self
            .pending
            .iter()
            .position(|p| p.handle.is_answered_by(echo))?;
        self.pending.remove(index).map(|p| p.handle)
    }

    /// Returns the next outcome that needs no message: a queued one first, else the oldest pending command past its deadline.
    pub(crate) fn next_unanswered(&mut self, now_ms: Option<u64>) -> Option<CommandResolution> {
        if let Some(resolution) = self.ready.pop_front() {
            return Some(resolution);
        }
        let now_ms = now_ms?;
        let index = self
            .pending
            .iter()
            .position(|p| p.deadline_ms.is_some_and(|deadline| now_ms >= deadline))?;
        self.pending.remove(index).map(|p| CommandResolution {
            handle: p.handle,
            outcome: CommandOutcome::TimedOut,
        })
    }

    /// Resolves every pending command as [`CommandOutcome::ConnectionLost`].
    pub(crate) fn connection_ended(&mut self) {
        self.ready
            .extend(self.pending.drain(..).map(|p| CommandResolution {
                handle: p.handle,
                outcome: CommandOutcome::ConnectionLost,
            }));
    }

    /// Records an outcome that has been handed to the caller.
    pub(crate) fn remember(&mut self, resolution: &CommandResolution) {
        if self.resolved.len() >= RESOLVED_OUTCOME_LIMIT {
            self.resolved.pop_front();
        }
        self.resolved.push_back(resolution.clone());
    }

    /// Returns `handle`'s outcome if it is already known — queued but undelivered (removed, so it is not delivered twice) or delivered recently.
    pub(crate) fn take_known(&mut self, handle: &CommandHandle) -> Option<CommandOutcome> {
        if let Some(index) = self.ready.iter().position(|r| r.handle == *handle) {
            return self.ready.remove(index).map(|r| r.outcome);
        }
        self.resolved
            .iter()
            .find(|r| r.handle == *handle)
            .map(|r| r.outcome.clone())
    }

    /// Whether `handle` is still awaiting an echo.
    pub(crate) fn is_pending(&self, handle: &CommandHandle) -> bool {
        self.pending.iter().any(|p| p.handle == *handle)
    }

    /// Stops tracking `handle` without producing an outcome event.
    pub(crate) fn forget(&mut self, handle: &CommandHandle) {
        self.pending.retain(|p| p.handle != *handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo(payload: &str) -> CommandEcho {
        parse_command_echo(payload.as_bytes()).expect("payload should read as a command echo")
    }

    fn handle(command: &str, sequence_id: u32) -> CommandHandle {
        CommandHandle::new(command.to_string(), sequence_id, AckExpectation::Echoes)
    }

    #[test]
    fn test_telemetry_pushes_are_not_echoes() {
        assert!(
            parse_command_echo(br#"{"print":{"command":"push_status","sequence_id":"58"}}"#)
                .is_none()
        );
        assert!(parse_command_echo(br#"{"print":{"command":"pushall"}}"#).is_none());
        assert!(parse_command_echo(br#"{"device":{}}"#).is_none());
        assert!(parse_command_echo(b"not json").is_none());
    }

    #[test]
    fn test_echo_found_under_every_command_wrapper() {
        for wrapper in ["print", "system", "info"] {
            let payload =
                format!(r#"{{"{wrapper}":{{"command":"ledctrl","sequence_id":"30001"}}}}"#);
            let parsed = echo(&payload);
            assert!(
                handle("ledctrl", 30001).is_answered_by(&parsed),
                "wrapper {wrapper}"
            );
        }
    }

    #[test]
    fn test_verdict_success_in_any_case_is_accepted() {
        for result in ["success", "SUCCESS"] {
            let payload = format!(
                r#"{{"print":{{"command":"pause","sequence_id":"1","result":"{result}","reason":"success"}}}}"#
            );
            assert_eq!(decode_verdict(&echo(&payload)), CommandOutcome::Accepted);
        }
    }

    #[test]
    fn test_verdict_fail_spellings_are_refusals_with_reason() {
        for result in ["fail", "failed", "FAILED"] {
            let payload = format!(
                r#"{{"print":{{"command":"ams_filament_setting","sequence_id":"1","result":"{result}","reason":"mqtt message verify failed"}}}}"#
            );
            let CommandOutcome::Refused(refusal) = decode_verdict(&echo(&payload)) else {
                panic!("result {result} must be a refusal");
            };
            assert_eq!(refusal.result.as_deref(), Some(result));
            assert_eq!(
                refusal.reason.as_deref(),
                Some("mqtt message verify failed")
            );
        }
    }

    #[test]
    fn test_verdict_err_code_refuses_even_with_success_result() {
        let parsed = echo(
            r#"{"print":{"command":"project_file","sequence_id":"1","result":"success","err_code":83935249}}"#,
        );
        let CommandOutcome::Refused(refusal) = decode_verdict(&parsed) else {
            panic!("a non-zero err_code must be a refusal");
        };
        assert_eq!(refusal.err_code, Some(83935249));
        assert_eq!(
            refusal.decoded_error().map(|e| e.short_code),
            Some("0500_C011".to_string())
        );
    }

    #[test]
    fn test_verdict_errno_refuses_and_zero_codes_do_not() {
        let parsed = echo(
            r#"{"print":{"command":"ams_change_filament","sequence_id":"1","result":"success","errno":-2,"soft_temp":45}}"#,
        );
        assert!(matches!(
            decode_verdict(&parsed),
            CommandOutcome::Refused(CommandRefusal {
                errno: Some(-2),
                ..
            })
        ));
        let zero = echo(
            r#"{"print":{"command":"ams_change_filament","sequence_id":"1","result":"success","errno":0,"err_code":0}}"#,
        );
        assert_eq!(decode_verdict(&zero), CommandOutcome::Accepted);
    }

    #[test]
    fn test_verdict_bare_echo_is_no_verdict() {
        let parsed = echo(r#"{"print":{"command":"ams_filament_setting","sequence_id":"3"}}"#);
        assert_eq!(decode_verdict(&parsed), CommandOutcome::NoVerdict);
    }

    #[test]
    fn test_echo_must_match_command_and_sequence_id() {
        let parsed = echo(r#"{"print":{"command":"gcode_line","sequence_id":"30001"}}"#);
        assert!(handle("gcode_line", 30001).is_answered_by(&parsed));
        assert!(!handle("gcode_line", 30002).is_answered_by(&parsed));
        assert!(!handle("pause", 30001).is_answered_by(&parsed));
    }

    #[test]
    fn test_tracker_resolves_each_command_once() {
        let mut tracker = CommandTracker::default();
        let answered = handle("gcode_line", 30001);
        let expiring = handle("pause", 30002);
        let lost = handle("resume", 30003);
        tracker.track(&answered, Some(1_000));
        tracker.track(&expiring, Some(1_000));
        tracker.track(&lost, None);

        let parsed = echo(r#"{"print":{"command":"gcode_line","sequence_id":"30001"}}"#);
        assert_eq!(tracker.take_answered(&parsed), Some(answered.clone()));
        assert_eq!(
            tracker.take_answered(&parsed),
            None,
            "an echo resolves its command once"
        );

        assert!(
            tracker.next_unanswered(Some(999)).is_none(),
            "deadline not reached"
        );
        let timed_out = tracker
            .next_unanswered(Some(1_000))
            .expect("deadline reached");
        assert_eq!(timed_out.handle, expiring);
        assert_eq!(timed_out.outcome, CommandOutcome::TimedOut);

        tracker.connection_ended();
        let resolution = tracker
            .next_unanswered(None)
            .expect("disconnect queues an outcome");
        assert_eq!(resolution.handle, lost);
        assert_eq!(resolution.outcome, CommandOutcome::ConnectionLost);
        assert!(tracker.next_unanswered(Some(u64::MAX)).is_none());
    }

    #[test]
    fn test_tracker_ignores_commands_that_settle_on_publish() {
        let mut tracker = CommandTracker::default();
        let pushall = CommandHandle::new(
            "pushall".to_string(),
            30001,
            AckExpectation::SettlesOnPublish,
        );
        tracker.track(&pushall, Some(0));
        assert!(!tracker.is_pending(&pushall));
        assert!(tracker.next_unanswered(Some(u64::MAX)).is_none());
    }

    #[test]
    fn test_tracker_evicts_oldest_as_timed_out_at_the_limit() {
        let mut tracker = CommandTracker::default();
        for seq in 0..=PENDING_COMMAND_LIMIT as u32 {
            tracker.track(&handle("gcode_line", 30_000 + seq), None);
        }
        let evicted = tracker
            .next_unanswered(None)
            .expect("eviction queues an outcome");
        assert_eq!(evicted.handle, handle("gcode_line", 30_000));
        assert_eq!(evicted.outcome, CommandOutcome::TimedOut);
        assert!(tracker.is_pending(&handle("gcode_line", 30_000 + PENDING_COMMAND_LIMIT as u32)));
    }

    #[test]
    fn test_take_known_consumes_queued_but_keeps_delivered() {
        let mut tracker = CommandTracker::default();
        let queued = handle("pause", 30001);
        tracker.track(&queued, None);
        tracker.connection_ended();
        assert_eq!(
            tracker.take_known(&queued),
            Some(CommandOutcome::ConnectionLost)
        );
        assert!(
            tracker.next_unanswered(None).is_none(),
            "taken outcome is not delivered again"
        );

        let delivered = CommandResolution {
            handle: handle("resume", 30002),
            outcome: CommandOutcome::Accepted,
        };
        tracker.remember(&delivered);
        assert_eq!(
            tracker.take_known(&delivered.handle),
            Some(CommandOutcome::Accepted)
        );
        assert_eq!(
            tracker.take_known(&delivered.handle),
            Some(CommandOutcome::Accepted)
        );
    }
}
