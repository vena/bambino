//! # Lightweight, Transport-Agnostic MQTT v3.1.1 Client Session
//!
//! Implements a dedicated async MQTT client designed to execute over our abstract
//! `AsyncIo` trait bounds. This custom client facilitates secure MQTTS connection
//! negotiations, subscription registrations, QoS 1 publish queues, keep-alive frames,
//! and write-channel zombie detection [REF-MQTT-CONN] [REF-MQTT-ZOMBIE].
//!
//! Designed for absolute execution safety across standard hosts, ESP-IDF microcontrollers,
//! and bare-metal Embassy targets.

#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap;
#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::collections::BTreeMap;
#[cfg(feature = "std")]
use std::collections::VecDeque;

use core::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

use crate::client::dummy::DummyTimer;
use crate::error::Error;
use crate::identity::PrinterIdentity;
use crate::io::{AsyncIo, Raced, SocketError, TimerProvider, race};

mod codec;
use codec::{
    PACKET_TYPE_CONNACK, PACKET_TYPE_PINGRESP, PACKET_TYPE_PUBACK, PACKET_TYPE_PUBLISH,
    PACKET_TYPE_SUBACK, encode_connect, encode_pingreq, encode_puback, encode_publish_qos1,
    encode_subscribe,
};

mod frame;
use frame::{
    FrameReadState, MQTT_MAX_PAYLOAD_BYTES, MQTT_READ_TIMEOUT_SECS, MQTT_WRITE_TIMEOUT_SECS,
    read_exact_packet,
};

mod pending;

/// Monotonic counter for generating unique MQTT client IDs across connections.
/// Each `connect()` call increments this to avoid stale QoS 1 queue conflicts
/// when the broker hasn't fully torn down a prior session's TCP socket.
static CONNECTION_COUNTER: AtomicU32 = AtomicU32::new(0);

pub(crate) const MQTT_IN_FLIGHT_LIMIT: usize = 200;
/// How long an unacknowledged QoS 1 packet id is kept in `in_flight` before `tick_zombie_check`
/// drops it.
///
/// Entries used to be removed *only* by a matching PUBACK, so a broker that dropped PUBACKs
/// leaked them permanently: once 200 accumulated, every later `publish_command` failed forever
/// against a condition nothing could clear. Set well above `MQTT_ZOMBIE_TIMEOUT_SECS` so a
/// genuinely slow ack is never aged out before the zombie check has had its say — this is a
/// leak backstop, not a retransmission policy (QoS 1 redelivery is the broker's job).
pub(crate) const MQTT_IN_FLIGHT_TTL_SECS: u32 = 120;
pub(crate) const MQTT_ZOMBIE_TIMEOUT_SECS: u32 = 10;
pub(crate) const MQTT_STALE_CONNECTION_SECS: u32 = 60;

/// How long the connection may go with no client-to-broker traffic before
/// `poll_telemetry_with_timer` sends a keepalive PINGREQ.
///
/// `MQTT_KEEP_ALIVE_SECS` (30) obliges this client to send *something* within 1.5× that
/// window — 45s — or the broker drops the connection (MQTT 3.1.1 §3.1.2.10). Nothing in the
/// library honored that: only the CLI's monitor sent pings, on its own timer, so a
/// library-only consumer following README's "connect, then `poll_telemetry()` in a loop"
/// sent zero bytes and was dropped after ~45s — surfacing as a bare I/O error, since
/// `MQTT_STALE_CONNECTION_SECS` (60) could not even report staleness before the disconnect.
///
/// 20s sits below both bounds that matter: the 45s broker deadline, and
/// `MQTT_READ_TIMEOUT_SECS` (30), so a ping still falls due between two consecutive
/// blocking reads on a link with no inbound telemetry at all.
pub(crate) const MQTT_PING_INTERVAL_SECS: u64 = 20;

// ============================================================================
// MQTT Client Session Management
// ============================================================================

/// Incoming MQTT message details parsed from the wire.
#[derive(Debug, Clone)]
pub struct MqttMessage {
    /// Full MQTT topic string the message arrived on (e.g. "device/{serial}/report").
    pub topic: String,
    /// Raw JSON payload bytes as received off the wire.
    pub payload: Vec<u8>,
}

/// Lightweight MQTT client session running over an established `AsyncIo` stream.
pub struct MqttClient<IO: AsyncIo> {
    stream: IO,
    request_topic: String,
    serial: String,
    next_packet_id: u16,
    /// Outgoing QoS 1 packet tracking registry, packet id → seconds elapsed since it was
    /// published. Handles up to `MQTT_IN_FLIGHT_LIMIT` concurrent unacknowledged entries; a
    /// PUBACK removes an entry, and `tick_zombie_check` ages out anything past
    /// `MQTT_IN_FLIGHT_TTL_SECS` so a broker dropping acks cannot wedge the queue permanently.
    in_flight: BTreeMap<u16, u32>,
    /// Messages buffered by request-response round-trips (e.g. `poll_until`), drained first by `poll_telemetry()` before reading from the wire.
    pending_messages: VecDeque<MqttMessage>,
    /// Combined topic+payload byte size of every message currently in `pending_messages`.
    /// Kept in sync by `push_pending()` (adds/evicts) and `poll_telemetry()` (drains) so
    /// eviction in `push_pending()` never has to walk the whole deque to size it.
    pending_bytes: usize,
    /// Accumulated elapsed seconds since the last command publish while waiting for a response update.
    write_pending_secs: Option<u32>,
    /// `sequence_id` [REF-MQTT-ACK] of the command that armed `write_pending_secs` — poll_wire's
    /// PUBLISH arm only clears the zombie timer on a reply echoing this exact value, not on any
    /// incoming PUBLISH (background telemetry arrives far more often than
    /// MQTT_ZOMBIE_TIMEOUT_SECS and would otherwise mask a real zombie episode forever).
    write_pending_sequence_id: Option<String>,
    /// Whether a PINGREQ has been sent with no PINGRESP yet received.
    ///
    /// Set by `send_ping_with_timer`, cleared by `poll_wire`'s PINGRESP arm, and checked on the
    /// *next* `send_ping_with_timer`: a second ping falling due while one is still outstanding
    /// means the broker has stopped answering keepalives. `MQTT_STALE_CONNECTION_SECS` alone
    /// cannot catch that — any inbound traffic resets the staleness counter, so a broker still
    /// streaming telemetry while ignoring PINGREQ would never be flagged.
    ping_outstanding: bool,
    /// Accumulated elapsed seconds since the last received message of any kind.
    /// Used to detect silent connection loss independent of publish activity.
    secs_since_last_message: u32,
    /// Byte-level progress of an in-flight frame read, preserved across a timed-out `read_exact_packet` call so `poll_wire()` resumes correctly instead of desyncing the stream — see `FrameReadState`'s doc comment.
    read_state: FrameReadState,
    /// Monotonic timestamp of the last frame this client wrote, driving the keepalive PINGREQ
    /// in `poll_telemetry_with_timer` (see [`MQTT_PING_INTERVAL_SECS`]).
    ///
    /// Deliberately *not* derived from `secs_since_last_message`: that counter only advances
    /// when the caller calls `tick_zombie_check`, so gating keepalives on it would reproduce the
    /// very "works only if the consumer knows to do extra work" flaw this exists to remove.
    ///
    /// `None` means unstamped — no write has happened yet, or the timer has no real clock. It is
    /// an `Option` rather than a `0` sentinel because a monotonic clock legitimately reads 0:
    /// `TokioTimer` measures from its own construction, so the first `now_millis()` of a fresh
    /// client really is 0, and a sentinel would read that as "never stamped" forever.
    last_outbound_ms: Option<u64>,
    /// Set once a `write_frame_with_timer` call fails — a write timeout or I/O error may
    /// already have put a partial frame on the wire, and unlike a read timeout (safe to
    /// retry via `FrameReadState`), a write has no resumable partial-progress state.
    /// Every subsequent write fails fast instead of writing again into a desynced stream.
    write_poisoned: bool,
}

/// Advances an MQTT packet identifier, skipping 0 (reserved) on wraparound.
fn advance_packet_id(current: u16) -> u16 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
}

/// Extracts the `sequence_id` echoed one level inside a Bambu MQTT JSON payload's top-level
/// wrapper object (`print`/`system`/`pushing`/`info` — see [REF-MQTT-ACK]). Used to correlate
/// a command ack with the command that armed the write-zombie timer, rather than treating any
/// incoming PUBLISH (including background `push_status` telemetry, which carries its own
/// independent sequence_id counter under the same shape) as proof the write channel is alive.
fn extract_sequence_id(payload: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    value
        .as_object()?
        .values()
        .find_map(|inner| inner.get("sequence_id")?.as_str())
        .map(|s| s.to_string())
}

/// Wire command names confirmed to produce an echoed ack [REF-MQTT-ACK] that write-zombie
/// detection can correlate against by `sequence_id`. Deliberately an allowlist, not "every
/// command except pushall": most command families here have never been checked against real
/// hardware, and defaulting an unverified command to "assumed correlatable" is exactly the bug
/// that shipped and broke bambino-cli's monitor against a real P1S (pushall has no ack at all,
/// see `extract_command_and_sequence_id`'s doc comment) — an allowlist instead degrades an
/// unverified command to the old permissive "any PUBLISH clears it" behavior, never to a hang.
///
/// Evidence per entry:
/// - `pause`/`resume`/`stop`/`gcode_line`/`clean_print_error`/`calibration`/`print_speed`/
///   `ledctrl`: documented directly in reference/03_mqtt_telemetry.md:543-572 (REF-MQTT-ACK).
/// - `ams_filament_setting`/`ams_filament_drying`/`extrusion_cali_get`/`extrusion_cali_set`/
///   `extrusion_cali_sel`/`extrusion_cali_del`: confirmed against real hardware by bambuddy's
///   independently reverse-engineered MQTT client (`backend/app/services/bambu_mqtt.py`),
///   which runs its own 10s write-zombie watchdog off `ams_filament_setting`'s echoed response
///   (their issue #887) and separately handles echoed responses for the `extrusion_cali_*`
///   family and `ams_filament_drying`.
/// - `get_version`: echoed-response shape confirmed by `src/types/version.rs`'s deserialization
///   test fixture.
///
/// - `skip_objects`/`project_file`/`ams_control`/`ams_get_rfid`/`ams_change_filament`/
///   `set_airduct`/`print_option`/`buzzer_ctrl`: confirmed on a real P1S (firmware 2025) by a
///   `bambino-cli ack-probe` run, issue #26. Each was published with a known `sequence_id` and
///   each echoed it back within 13-57ms inside a `print` wrapper carrying the same `command`
///   name, with background `push_status` traffic flowing alongside in six of the eight windows
///   — so the correlation is genuinely by ID, not "a message happened to arrive".
///
/// Every entry above returned `result: "success"`, including `set_airduct` and `buzzer_ctrl`,
/// which address hardware a P1S does not have (no chamber damper, no fire-alarm buzzer), and
/// `project_file` aimed at a file that does not exist. That is the documented P1S behavior
/// [REF-MQTT-ACK]: the ack confirms *receipt*, not execution or even feature support. Presence
/// on this list therefore says nothing about whether a command does anything on a given model
/// — it says only that the printer answers, which is all write-zombie detection needs.
///
/// Not on this list: `pushall`, confirmed to produce *no* ack at all (see
/// `extract_command_and_sequence_id`'s doc comment).
///
/// To add a further command, run `bambino-cli ack-probe` against real hardware and cite its
/// report: it publishes the command with a known `sequence_id` and records whether a response
/// echoing that exact ID arrives, which is the only evidence that distinguishes a real ack from
/// the background `push_status` stream. Do not add an entry on the strength of a payload's
/// *shape* alone. The P1S run above does not generalize to other models either — re-run it on
/// the model in question before assuming a command behaves the same there.
const ACK_CORRELATED_COMMANDS: &[&str] = &[
    "pause",
    "resume",
    "stop",
    "gcode_line",
    "clean_print_error",
    "calibration",
    "print_speed",
    "ledctrl",
    "ams_filament_setting",
    "ams_filament_drying",
    "extrusion_cali_get",
    "extrusion_cali_set",
    "extrusion_cali_sel",
    "extrusion_cali_del",
    "get_version",
    "skip_objects",
    "project_file",
    "ams_control",
    "ams_get_rfid",
    "ams_change_filament",
    "set_airduct",
    "print_option",
    "buzzer_ctrl",
    "get_access_code",
];

/// Extracts the `command` name and `sequence_id` from an outgoing command payload's single
/// top-level wrapper object (`print`/`system`/`pushing`/`info` — the Payload+Request pattern
/// always nests exactly one). `pushall` (`pushing` wrapper) triggers an unlabeled state-dump
/// stream rather than an echoed ack [REF-MQTT-LIFECYCLE], so it's excluded from
/// `ACK_CORRELATED_COMMANDS` above like every other command without confirmed ack evidence.
fn extract_command_and_sequence_id(payload: &[u8]) -> Option<(String, String)> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let inner = value.as_object()?.values().next()?;
    let command = inner.get("command")?.as_str()?.to_string();
    let sequence_id = inner.get("sequence_id")?.as_str()?.to_string();
    Some((command, sequence_id))
}

/// Maps an `embedded_io_async::ErrorKind` (the only information a generic `AsyncIo` error
/// exposes, regardless of platform) to the closest `SocketError` variant — the
/// `embedded_io_async::ErrorKind` counterpart to `map_io_error_kind`
/// (`std::io::ErrorKind -> embedded_io_async::ErrorKind`, `src/io/mod.rs`), used here since
/// `write_frame` operates over the generic `AsyncIo` trait rather than a concrete
/// `std::io::Error`, so `map_io_error_kind` itself doesn't apply. Falls back to `Other` for
/// kinds with no direct `SocketError` equivalent.
fn map_embedded_io_error_kind(kind: embedded_io_async::ErrorKind) -> SocketError {
    use embedded_io_async::ErrorKind;
    match kind {
        ErrorKind::ConnectionRefused => SocketError::ConnectionRefused,
        ErrorKind::ConnectionAborted => SocketError::ConnectionAborted,
        ErrorKind::ConnectionReset => SocketError::ConnectionReset,
        ErrorKind::NotConnected => SocketError::NotConnected,
        ErrorKind::TimedOut => SocketError::TimedOut,
        ErrorKind::AddrInUse => SocketError::AddressInUse,
        ErrorKind::AddrNotAvailable => SocketError::AddressNotAvailable,
        ErrorKind::InvalidInput => SocketError::InvalidInput,
        _ => SocketError::Other("MQTT write_frame I/O error".into()),
    }
}

/// Which of a printer's two MQTT topics a client subscribes to on connect.
///
/// The printer exposes `device/<serial>/report` (telemetry it publishes) and
/// `device/<serial>/request` (commands clients publish to it). A normal session wants the
/// former; the latter only makes sense for observing another client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionTopic {
    /// `device/<serial>/report` — the printer's telemetry stream. The normal choice.
    Report,
    /// `device/<serial>/request` — the topic *clients publish commands to*, subscribed in order
    /// to observe what another client (typically BambuStudio) sends.
    ///
    /// Diagnostic use only. Two things to know before relying on it:
    ///
    /// 1. Whether the broker permits a second client to subscribe here is a firmware ACL
    ///    question, not a protocol guarantee. A refusal surfaces as a rejected SUBACK
    ///    (`Error::ProtocolViolation`), which is an answer, not a bug.
    /// 2. A client subscribed to `Request` receives no telemetry, so none of
    ///    `PrinterClient`'s state caches will ever populate. Use it for capture, not control.
    Request,
}

impl SubscriptionTopic {
    /// Returns the topic suffix that follows `device/<serial>/`.
    pub fn suffix(&self) -> &'static str {
        match self {
            Self::Report => "report",
            Self::Request => "request",
        }
    }
}

/// Writes and flushes a complete packet to `stream`, mapping I/O failures via `map_embedded_io_error_kind` instead of collapsing everything to a fixed `ConnectionAborted`.
/// A free function (not a method) so `connect()` can call it before `Self` exists.
async fn write_frame<IO: AsyncIo>(stream: &mut IO, packet: &[u8]) -> Result<(), Error> {
    use embedded_io_async::Error as _;
    stream
        .write_all(packet)
        .await
        .map_err(|e| Error::Network(map_embedded_io_error_kind(e.kind())))?;
    stream
        .flush()
        .await
        .map_err(|e| Error::Network(map_embedded_io_error_kind(e.kind())))
}

/// Same as [`write_frame`], but races the write against `MQTT_WRITE_TIMEOUT_SECS` when `timer`
/// has a real wall-clock — without this, a stalled peer that stops draining its
/// socket buffer (or a dead connection with no RST yet) blocks `write_all()`/`flush()`
/// forever, unlike the read path's existing `MQTT_READ_TIMEOUT_SECS` protection. Unlike
/// `read_chunk`'s single-step racing (needed for resumability across partial reads), a timed-
/// out write has no partial-progress state worth preserving — the caller treats the whole
/// frame as unsent and fails the operation.
async fn write_frame_with_timer<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    packet: &[u8],
    timer: &T,
) -> Result<(), Error> {
    if !timer.has_real_clock() {
        return write_frame(stream, packet).await;
    }
    let write_fut = write_frame(stream, packet);
    let sleep_fut = timer.sleep(core::time::Duration::from_secs(MQTT_WRITE_TIMEOUT_SECS));
    match race(write_fut, sleep_fut).await {
        Raced::Left(result) => result,
        Raced::Right(_) => Err(Error::Network(SocketError::TimedOut)),
    }
}

impl<IO: AsyncIo> MqttClient<IO> {
    /// Writes a frame via [`write_frame_with_timer`], poisoning the connection on failure.
    ///
    /// A write timeout or I/O error may already have put a partial frame on the wire, and
    /// unlike a read timeout (safe to retry via `FrameReadState`), a write has no resumable
    /// partial-progress state — once poisoned, every subsequent call fails immediately
    /// without touching the stream again.
    async fn write_frame_guarded<T: TimerProvider>(
        &mut self,
        packet: &[u8],
        timer: &T,
    ) -> Result<(), Error> {
        if self.write_poisoned {
            return Err(Error::Network(SocketError::ConnectionAborted));
        }
        write_frame_with_timer(&mut self.stream, packet, timer)
            .await
            .inspect_err(|_| self.write_poisoned = true)
            .inspect(|_| {
                // Every outbound frame resets the keepalive clock, not just PINGREQ: the broker
                // deadline is about client-to-broker traffic of any kind, so a client publishing
                // commands steadily never needs to ping at all.
                if timer.has_real_clock() {
                    self.last_outbound_ms = Some(timer.now_millis());
                }
            })
    }
    /// Executes a secure local network connection handshake and subscription loop with the printer.
    ///
    /// **Authentication Note:** If the printer's physical broker rejects credentials due to
    /// an invalid access code, this function returns `Error::AccessDenied`.
    ///
    /// **Unbounded by design — callers must supply their own deadline.** The CONNECT/CONNACK
    /// and SUBSCRIBE/SUBACK writes and reads inside this function have no internal timeout
    /// (`DummyTimer` is used throughout, so a stalled peer hangs this call forever). This is
    /// safe for `PrinterClient::ensure_mqtt()`, the sole production call site, because it
    /// wraps the *entire* dial+connect sequence in `race_against_connect_timeout`. A caller
    /// invoking `MqttClient::connect()` directly (bypassing `PrinterClient`) gets no such
    /// bound and must wrap this call in its own timeout (e.g. `tokio::time::timeout`) against
    /// a peer that stalls before CONNACK/SUBACK.
    pub async fn connect(stream: IO, identity: &PrinterIdentity) -> Result<Self, Error> {
        Self::connect_subscribed(stream, identity, SubscriptionTopic::Report).await
    }

    /// Same handshake as [`MqttClient::connect`], but subscribes to a caller-chosen topic.
    ///
    /// Exists for [`SubscriptionTopic::Request`], which is a diagnostic capture mode rather than
    /// a normal session — see that variant's doc comment. Every caveat on `connect` applies here
    /// unchanged, including that this call is unbounded and needs the caller's own deadline.
    pub async fn connect_subscribed(
        mut stream: IO,
        identity: &PrinterIdentity,
        topic: SubscriptionTopic,
    ) -> Result<Self, Error> {
        let serial = identity.serial.as_str();
        let access_code = identity.access_code.as_str();
        let conn_id = CONNECTION_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        let client_id = format!("bambino_{}_{}", serial, conn_id);
        let connect_pkt = encode_connect(&client_id, "bblp", access_code);

        log::debug!(
            "Transmitting CONNECT payload (client_id: '{}', user: 'bblp')",
            client_id
        );

        write_frame(&mut stream, &connect_pkt).await?;

        // Read CONNACK packet. `DummyTimer` (`has_real_clock() == false`) makes
        // `read_exact_packet` fall back to a plain unbounded read here. Deliberately not wired
        // to `PrinterClient`'s configurable `Timer`: `connect()` runs before a `MqttClient`
        // (and thus a persistent `FrameReadState`) exists, and the connect-phase handshake
        // (TCP+TLS dial timeout, `PrinterClient::connect_timeout_secs`) is a separate concern
        // from a stall on an already-established connection, mid-`poll_wire()`.
        let mut read_state = FrameReadState::default();

        log::debug!("Awaiting broker CONNACK response packet");

        let (header, payload_buf) = read_exact_packet(
            &mut stream,
            &mut read_state,
            &DummyTimer,
            MQTT_READ_TIMEOUT_SECS * 1000,
        )
        .await?;

        let packet_type = header >> 4;

        log::debug!(
            "Received raw packet header type: {}, remaining size: {} bytes",
            packet_type,
            payload_buf.len()
        );

        if packet_type != PACKET_TYPE_CONNACK {
            return Err(Error::ProtocolViolation(
                "Expected CONNACK frame".into(),
            ));
        }
        if payload_buf.len() < 2 {
            return Err(Error::ProtocolViolation(
                "Short CONNACK payload".into(),
            ));
        }
        let connack_code = payload_buf[1];

        log::debug!("Connection accepted response byte: {}", connack_code);

        // MQTT v3.1.1 CONNACK codes 1-3 (unacceptable protocol version, identifier
        // rejected, server unavailable) are distinct from 4-5 (bad credentials/not authorized)
        // — only the latter pair actually means the access code was rejected, matching
        // AccessDenied's own doc comment. Collapsing 1-3 into AccessDenied too would misdiagnose
        // e.g. a transient "server unavailable" as "check your access code."
        match connack_code {
            0 => {}
            4 | 5 => {
                log::warn!(
                    "Broker rejected connection with CONNACK return code: {} (access denied)",
                    connack_code
                );
                return Err(Error::AccessDenied);
            }
            other => {
                log::warn!(
                    "Broker rejected connection with CONNACK return code: {} (not an access-code rejection)",
                    other
                );
                return Err(Error::ProtocolViolation(
                    format!("Broker rejected connection with CONNACK return code {other}").into(),
                ));
            }
        }

        let subscribe_topic = format!("device/{}/{}", serial, topic.suffix());

        log::debug!(
            "Sending SUBSCRIBE frame targeting topic: '{}' (granted QoS 1)",
            subscribe_topic
        );

        let subscribe_pkt = encode_subscribe(1, &subscribe_topic, 1);

        write_frame(&mut stream, &subscribe_pkt).await?;

        // Read SUBACK packet. `read_state` was reset to `Idle` on the successful CONNACK
        // read above, so reusing it here starts a fresh frame read.
        log::debug!("Awaiting broker SUBACK verification packet");

        let (sub_header, payload_buf) = read_exact_packet(
            &mut stream,
            &mut read_state,
            &DummyTimer,
            MQTT_READ_TIMEOUT_SECS * 1000,
        )
        .await?;
        let sub_type = sub_header >> 4;

        log::debug!("Received raw packet header type: {}", sub_type);

        if sub_type != PACKET_TYPE_SUBACK {
            return Err(Error::ProtocolViolation(
                "Expected SUBACK frame".into(),
            ));
        }
        if payload_buf.len() < 3 {
            return Err(Error::ProtocolViolation("Short SUBACK payload".into()));
        }
        // The SUBACK's variable header echoes the SUBSCRIBE packet id, which `encode_subscribe`
        // always sets to 1. A mismatch means the bytes at [2] are not the return code for the
        // subscription we sent, so validating the code alone would be meaningless.
        let echoed_packet_id = u16::from_be_bytes([payload_buf[0], payload_buf[1]]);
        if echoed_packet_id != 1 {
            return Err(Error::ProtocolViolation(
                "SUBACK echoed an unexpected packet id".into(),
            ));
        }

        let return_code = payload_buf[2];

        log::debug!("SUBACK response status granted: 0x{:02X}", return_code);

        // MQTT 3.1.1 §3.9.3 defines exactly four return codes: 0x00/0x01/0x02 granted (max QoS)
        // and 0x80 failure. Rejecting only 0x80 accepted every other byte — a firmware bug, or
        // a byte read at the wrong offset because the SUBACK carried multiple topic results —
        // and returned a client subscribed to nothing, whose failure only surfaced much later
        // as "no telemetry ever arrives".
        if !(0x00..=0x02).contains(&return_code) {
            return Err(Error::ProtocolViolation(
                "Subscription rejected by physical broker".into(),
            ));
        }

        Ok(Self {
            stream,
            request_topic: format!("device/{}/request", serial),
            serial: serial.to_string(),
            next_packet_id: 2, // 1 is consumed by SUBSCRIBE handshake
            in_flight: BTreeMap::new(),
            pending_messages: VecDeque::new(),
            pending_bytes: 0,
            write_pending_secs: None,
            write_pending_sequence_id: None,
            ping_outstanding: false,
            last_outbound_ms: None,
            secs_since_last_message: 0,
            read_state: FrameReadState::default(),
            write_poisoned: false,
        })
    }

    /// Returns the serial number this client authenticated with (`connect()`'s `serial` argument).
    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// Submits a serialized JSON command payload to the printer's request channel.
    ///
    /// **In-flight Bounds Verification:**
    /// If the unacknowledged queue size equals or exceeds `MQTT_IN_FLIGHT_LIMIT`, this function
    /// returns [`Error::Backpressure`] without sending, to protect memory space and prevent
    /// packet drift [REF-MQTT-CONN]. A saturated queue is not a timeout — retrying immediately
    /// will not clear it; drain it by servicing PUBACKs (`poll_wire`) or let
    /// `tick_zombie_check` age the entries out.
    ///
    /// Payloads larger than `MQTT_MAX_PAYLOAD_BYTES` are rejected with
    /// [`Error::ProtocolViolation`] rather than encoded, mirroring the read path's own cap.
    ///
    /// `DummyTimer` (`has_real_clock() == false`) makes the underlying write unbounded here.
    /// `PrinterClient` callers get the new stalled-write protection via
    /// `publish_command_with_timer()` instead, since they have a real `Timer` available.
    pub async fn publish_command(&mut self, payload: &[u8]) -> Result<u16, Error> {
        self.publish_command_with_timer(payload, &DummyTimer).await
    }

    /// Same as [`publish_command()`](Self::publish_command), but honors `timer` for the
    /// underlying write's per-call deadline (see `write_frame_with_timer`).
    pub(crate) async fn publish_command_with_timer<T: TimerProvider>(
        &mut self,
        payload: &[u8],
        timer: &T,
    ) -> Result<u16, Error> {
        if self.in_flight.len() >= MQTT_IN_FLIGHT_LIMIT {
            log::warn!(
                "In-flight command backlog saturated ({} items)",
                self.in_flight.len()
            );
            // Not `SocketError::TimedOut`: saturation isn't a stall, and reporting it as one
            // invited callers into an infinite retry loop against a queue that only inbound
            // PUBACKs or `tick_zombie_check`'s TTL sweep can drain.
            return Err(Error::Backpressure);
        }

        // Symmetric with the read path's own `MQTT_MAX_PAYLOAD_BYTES` check
        // (`frame.rs::read_exact_packet`). `publish_command` is public and README advertises
        // sending raw payloads through it, so without this an oversized payload reached
        // `encode_remaining_length`, which has no 4-byte varint ceiling of its own, and wrote a
        // malformed frame that desyncs the broker instead of returning an error.
        if payload.len() > MQTT_MAX_PAYLOAD_BYTES {
            log::warn!(
                "Refusing to publish a {}-byte payload; the cap is {} bytes",
                payload.len(),
                MQTT_MAX_PAYLOAD_BYTES
            );
            return Err(Error::ProtocolViolation(
                "MQTT payload exceeds MQTT_MAX_PAYLOAD_BYTES".into(),
            ));
        }

        let packet_id = self.next_packet_id;
        self.next_packet_id = advance_packet_id(self.next_packet_id);

        log::debug!(
            "Publishing QoS 1 command (packet_id: {}) to topic: '{}' (payload length: {} bytes)",
            packet_id,
            self.request_topic,
            payload.len()
        );

        let packet = encode_publish_qos1(packet_id, &self.request_topic, payload);

        self.write_frame_guarded(&packet, timer).await?;

        self.in_flight.insert(packet_id, 0);

        // Arm write-channel zombie detection tracking [REF-MQTT-ZOMBIE] — only on
        // the *first* unanswered command, not unconditionally on every call. Resetting to 0
        // on each publish_command() while an earlier command is still awaiting a response
        // would let a steady stream of new commands mask that earlier one's zombie state
        // indefinitely, since the counter never reaches MQTT_ZOMBIE_TIMEOUT_SECS.
        if self.write_pending_secs.is_none() {
            self.write_pending_secs = Some(0);
            // Only correlate commands with confirmed ack evidence (ACK_CORRELATED_COMMANDS);
            // everything else (including pushall) falls back to clearing on any PUBLISH, same
            // as before this correlation fix existed.
            self.write_pending_sequence_id = match extract_command_and_sequence_id(payload) {
                Some((command, seq)) if ACK_CORRELATED_COMMANDS.contains(&command.as_str()) => {
                    Some(seq)
                }
                _ => None,
            };
        }

        Ok(packet_id)
    }

    /// Returns the next MQTT message, draining any buffered messages first.
    ///
    /// Messages are buffered when request-response methods (e.g. `get_version()`) read
    /// non-matching messages off the wire while waiting for a specific response. This
    /// method drains those buffered messages in FIFO order before reading new packets
    /// from the wire.
    ///
    /// Handles MQTT protocol frames transparently: sends `PUBACK` for incoming QoS 1
    /// publishes, clears matching packet IDs from the in-flight tracker on `PUBACK`,
    /// and acknowledges `PINGRESP` — only application-level `PUBLISH` payloads are
    /// returned.
    pub async fn poll_telemetry(&mut self) -> Result<MqttMessage, Error> {
        // `DummyTimer` has no real wall-clock (`has_real_clock() == false`), so
        // `poll_wire()` falls back to an unbounded read here. `PrinterClient` callers get the
        // new bounded-read protection via `poll_telemetry_with_timer()` instead, since they
        // have a real `Timer` available.
        self.poll_telemetry_with_timer(&DummyTimer).await
    }

    /// Same as [`poll_telemetry()`](Self::poll_telemetry), but honors `timer` for the underlying wire read's per-read deadline (see [`poll_wire`](Self::poll_wire)).
    ///
    /// Used by `PrinterClient`, which owns its own configurable `Timer` and wants the
    /// stalled-read protection that requires a genuine wall-clock to be meaningful.
    pub(crate) async fn poll_telemetry_with_timer<T: TimerProvider>(
        &mut self,
        timer: &T,
    ) -> Result<MqttMessage, Error> {
        if let Some(buffered) = self.pending_messages.pop_front() {
            self.pending_bytes = self
                .pending_bytes
                .saturating_sub(Self::message_size(&buffered));
            return Ok(buffered);
        }
        self.send_keepalive_if_due(timer).await?;
        self.poll_wire(timer).await
    }

    /// Sends a keepalive PINGREQ if the connection has been silent outbound for
    /// [`MQTT_PING_INTERVAL_SECS`], honoring the keepalive this client advertises in CONNECT.
    ///
    /// Sent *before* the blocking read rather than after: `poll_wire` may block for up to
    /// `MQTT_READ_TIMEOUT_SECS` on a link with no inbound telemetry, and a ping issued after
    /// that would already be too late on the second such read.
    ///
    /// No-op without a real wall-clock (`DummyTimer`), which is what `MqttClient::poll_telemetry`
    /// and test clients use — those keep the pre-existing behavior of never pinging.
    async fn send_keepalive_if_due<T: TimerProvider>(&mut self, timer: &T) -> Result<(), Error> {
        if !timer.has_real_clock() {
            return Ok(());
        }

        // First call after connect: stamp the clock rather than ping, so the interval is
        // measured from a real outbound event instead of from the timer's origin.
        let Some(last_outbound_ms) = self.last_outbound_ms else {
            self.last_outbound_ms = Some(timer.now_millis());
            return Ok(());
        };

        let idle_ms = timer.now_millis().saturating_sub(last_outbound_ms);
        if idle_ms < MQTT_PING_INTERVAL_SECS * 1000 {
            return Ok(());
        }

        log::trace!("Keepalive due after {}ms of outbound silence", idle_ms);
        self.send_ping_with_timer(timer).await
    }

    /// Reads the next message directly from the wire, bypassing the pending buffer.
    ///
    /// Used by `PrinterClient::poll_until()` which manages its own buffer stashing
    /// and must not re-read messages it just pushed.
    ///
    /// Bounds each individual low-level read step to
    /// [`MQTT_READ_TIMEOUT_SECS`] when `timer` has a real wall-clock (see
    /// [`TimerProvider::has_real_clock`]) — closes the "connection stalls with zero
    /// incoming bytes" hang that neither `PrinterClient::poll_until`'s wall-clock
    /// timeout nor its message-count valve can catch, since both only run *after* a full
    /// frame has already been received (see `read_exact_packet`'s doc comment for the
    /// mechanism, and the resumability invariant that makes retrying after a timeout
    /// safe rather than stream-corrupting).
    pub(crate) async fn poll_wire<T: TimerProvider>(
        &mut self,
        timer: &T,
    ) -> Result<MqttMessage, Error> {
        loop {
            let (header, payload_buf) = read_exact_packet(
                &mut self.stream,
                &mut self.read_state,
                timer,
                MQTT_READ_TIMEOUT_SECS * 1000,
            )
            .await?;

            self.secs_since_last_message = 0;

            let packet_type = header >> 4;

            log::trace!(
                "Parsed wire packet type: {}, size: {} bytes",
                packet_type,
                payload_buf.len()
            );

            match packet_type {
                PACKET_TYPE_PUBLISH => {
                    let qos = (header & 0x06) >> 1;

                    if payload_buf.len() < 2 {
                        return Err(Error::ProtocolViolation(
                            "Short publish payload".into(),
                        ));
                    }
                    let topic_len = u16::from_be_bytes([payload_buf[0], payload_buf[1]]) as usize;
                    if payload_buf.len() < 2 + topic_len {
                        return Err(Error::ProtocolViolation(
                            "Invalid topic length bounds".into(),
                        ));
                    }

                    let topic = core::str::from_utf8(&payload_buf[2..2 + topic_len])
                        .map_err(|_| Error::ProtocolViolation("Non-UTF8 topic name".into()))?
                        .to_string();

                    let mut payload_start = 2 + topic_len;
                    let mut packet_id = None;
                    if qos >= 1 {
                        if payload_buf.len() < payload_start + 2 {
                            return Err(Error::ProtocolViolation(
                                "Missing packet ID in QoS 1+".into(),
                            ));
                        }
                        let id = u16::from_be_bytes([
                            payload_buf[payload_start],
                            payload_buf[payload_start + 1],
                        ]);
                        packet_id = Some(id);
                        payload_start += 2;
                    }

                    let payload = payload_buf[payload_start..].to_vec();

                    log::debug!(
                        "Received PUBLISH frame on topic: '{}', QoS: {}, packet_id: {:?}, payload size: {} bytes",
                        topic,
                        qos,
                        packet_id,
                        payload.len()
                    );

                    // QoS 1 requires PUBACK; QoS 2 requires a PUBREC/PUBREL/PUBCOMP handshake,
                    // which this client doesn't implement — Bambu printers never
                    // publish above QoS 1 in practice, so this stays a logged, non-fatal gap
                    // rather than a full protocol extension for a case never observed against
                    // real hardware. A broker that did send genuine QoS 2 would see no PUBREC
                    // and may retransmit with DUP set.
                    if qos == 1 {
                        let id = packet_id.expect("QoS 1 always has packet_id");
                        log::trace!("Sending automatic PUBACK for packet_id: {}", id);

                        let ack = encode_puback(id);
                        self.write_frame_guarded(&ack, timer).await?;
                    } else if qos >= 2 {
                        log::warn!(
                            "Received QoS {} PUBLISH (packet_id: {:?}) — QoS 2 handshake \
                             (PUBREC/PUBREL/PUBCOMP) is not implemented; broker may retransmit",
                            qos,
                            packet_id
                        );
                    }

                    // Reset write channel zombie tracking only when this PUBLISH's echoed
                    // sequence_id [REF-MQTT-ACK] matches the outstanding command's — not on any
                    // incoming PUBLISH. Background telemetry (push_status) carries its own
                    // independent, low-value sequence_id counter and arrives far more often than
                    // MQTT_ZOMBIE_TIMEOUT_SECS, so an unconditional reset here would mask a real
                    // zombie episode (broker discarding commands) forever [REF-MQTT-ZOMBIE].
                    // A pending command with no known sequence_id (pushall's `pushing` wrapper
                    // has no echoed ack, see `wrapper_key`) falls back to clearing on any
                    // PUBLISH, matching pre-correlation behavior for that case only.
                    let should_clear = match &self.write_pending_sequence_id {
                        Some(expected) => extract_sequence_id(&payload).as_deref() == Some(expected.as_str()),
                        None => self.write_pending_secs.is_some(),
                    };
                    if should_clear {
                        self.write_pending_secs = None;
                        self.write_pending_sequence_id = None;
                    }

                    return Ok(MqttMessage { topic, payload });
                }
                PACKET_TYPE_PUBACK => {
                    if payload_buf.len() < 2 {
                        return Err(Error::ProtocolViolation(
                            "Invalid PUBACK length".into(),
                        ));
                    }
                    let ack_id = u16::from_be_bytes([payload_buf[0], payload_buf[1]]);

                    log::trace!(
                        "Received PUBACK from broker for outbound packet_id: {}",
                        ack_id
                    );

                    self.in_flight.remove(&ack_id);
                }
                PACKET_TYPE_PINGRESP => {
                    log::trace!("Received keep-alive PINGRESP from broker");
                    self.ping_outstanding = false;
                }
                _ => {
                    log::debug!("Ignoring un-handled control frame code: {}", packet_type);
                }
            }
        }
    }

    /// Dispatches an asynchronous `PINGREQ` keep-alive frame to maintain socket validity.
    ///
    /// `DummyTimer` makes the underlying write unbounded here, mirroring `publish_command()`.
    /// `PrinterClient` callers get stalled-write protection via `send_ping_with_timer()`
    /// instead.
    pub async fn send_ping(&mut self) -> Result<(), Error> {
        self.send_ping_with_timer(&DummyTimer).await
    }

    /// Same as [`send_ping()`](Self::send_ping), but honors `timer` for the underlying write's
    /// per-call deadline (see `write_frame_with_timer`).
    pub(crate) async fn send_ping_with_timer<T: TimerProvider>(
        &mut self,
        timer: &T,
    ) -> Result<(), Error> {
        // A ping already outstanding when the next one falls due means the broker acknowledged
        // neither. Callers ping on their own schedule, so "a second ping is due" is the only
        // point at which enough time has demonstrably passed to call it a failure — and it is a
        // failure the staleness counter cannot see, since inbound telemetry keeps resetting it.
        if self.ping_outstanding {
            log::warn!("Broker did not answer the previous PINGREQ; treating the link as dead");
            return Err(Error::Timeout);
        }

        log::trace!("Transmitting PINGREQ keep-alive packet");

        let ping = encode_pingreq();
        self.write_frame_guarded(&ping, timer).await?;
        self.ping_outstanding = true;
        Ok(())
    }

    /// Returns true once a write has failed and left the stream possibly desynced.
    ///
    /// A poisoned client is permanently unusable: every later `publish_command`, `send_ping`,
    /// and automatic PUBACK returns `ConnectionAborted` forever, because a failed write may have
    /// put a partial frame on the wire and, unlike a read, has no resumable progress state.
    /// Without this accessor a retry loop could not tell that error apart from a transient one
    /// and would spin against a client that can never recover; the correct response is to drop
    /// the connection and reconnect (`PrinterClient::disconnect_mqtt()`).
    pub fn is_poisoned(&self) -> bool {
        self.write_poisoned
    }

    /// Platform-agnostic timer tick update.
    ///
    /// Evaluates two independent liveness conditions:
    /// 1. **Write zombie**: A published command has gone unanswered for 10+ seconds
    ///    [REF-MQTT-ZOMBIE].
    /// 2. **Connection staleness**: No packets of any kind received for 60+ seconds,
    ///    indicating a silently dropped connection — independent of (1) [REF-MQTT-CONN].
    pub fn tick_zombie_check(&mut self, elapsed_secs: u32) -> Result<(), Error> {
        // Age out unacknowledged QoS 1 entries. Without this, a broker that drops PUBACKs leaks
        // `in_flight` entries with no expiry and no drain, and `publish_command` returns
        // saturation forever — see `MQTT_IN_FLIGHT_TTL_SECS`.
        self.in_flight.retain(|packet_id, age_secs| {
            *age_secs += elapsed_secs;
            if *age_secs >= MQTT_IN_FLIGHT_TTL_SECS {
                log::warn!(
                    "Dropping unacknowledged QoS 1 packet_id {} after {}s with no PUBACK",
                    packet_id,
                    age_secs
                );
                return false;
            }
            true
        });

        if let Some(ref mut secs) = self.write_pending_secs {
            *secs += elapsed_secs;
            if *secs >= MQTT_ZOMBIE_TIMEOUT_SECS {
                log::warn!(
                    "Zombie state detected: command issued but zero telemetry updates received for >= {}s",
                    MQTT_ZOMBIE_TIMEOUT_SECS
                );
                return Err(Error::Timeout);
            }
        }

        self.secs_since_last_message += elapsed_secs;
        if self.secs_since_last_message >= MQTT_STALE_CONNECTION_SECS {
            log::warn!(
                "Connection stale: no packets received for >= {}s",
                MQTT_STALE_CONNECTION_SECS
            );
            return Err(Error::Timeout);
        }

        Ok(())
    }

    /// Returns the number of current un-acknowledged QoS 1 packets.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_id_skips_zero_on_wraparound() {
        assert_eq!(
            advance_packet_id(u16::MAX),
            1,
            "Packet ID must skip 0 after wraparound"
        );
    }

    #[test]
    fn test_packet_id_normal_increment() {
        assert_eq!(advance_packet_id(100), 101);
    }

    #[test]
    fn test_packet_id_one_before_max() {
        assert_eq!(
            advance_packet_id(u16::MAX - 1),
            u16::MAX,
            "ID before MAX should increment normally"
        );
    }

    #[cfg(feature = "tokio")]
    mod async_tests {
        use super::super::*;
        use crate::io::TokioIo;
        use crate::models::PrinterModel;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        #[tokio::test]
        async fn test_connack_rejection_returns_access_denied() {
            let (client_stream, mut server_stream) = tokio::io::duplex(8192);

            let server_task = tokio::spawn(async move {
                // Read and discard the CONNECT packet
                let mut discard = vec![0u8; 256];
                let _ = server_stream.read(&mut discard).await;

                // Reply with CONNACK: return code 5 (not authorized)
                server_stream
                    .write_all(&[0x20, 0x02, 0x00, 0x05])
                    .await
                    .unwrap();
                server_stream.flush().await.unwrap();
            });

            let result =
                MqttClient::connect(
                    TokioIo(client_stream),
                    &PrinterIdentity {
                        ip: String::new(),
                        serial: "01P000000000000".into(),
                        access_code: "12345678".into(),
                        model: PrinterModel::P1S,
                    },
                )
                    .await;
            let err = result.err().expect("Expected error, got Ok");
            assert!(
                matches!(err, crate::error::Error::AccessDenied),
                "Expected AccessDenied, got {:?}",
                err
            );
            server_task.await.unwrap();
        }

        #[tokio::test]
        async fn test_connack_server_unavailable_returns_protocol_violation() {
            // CONNACK codes 1-3 (unacceptable protocol version, identifier rejected,
            // server unavailable) are distinct from 4-5 (bad credentials/not authorized) — only
            // the latter pair means the access code was actually rejected. Code 3 here
            // (server unavailable) must not misdiagnose as AccessDenied.
            let (client_stream, mut server_stream) = tokio::io::duplex(8192);

            let server_task = tokio::spawn(async move {
                let mut discard = vec![0u8; 256];
                let _ = server_stream.read(&mut discard).await;

                // Reply with CONNACK: return code 3 (server unavailable)
                server_stream
                    .write_all(&[0x20, 0x02, 0x00, 0x03])
                    .await
                    .unwrap();
                server_stream.flush().await.unwrap();
            });

            let result =
                MqttClient::connect(
                    TokioIo(client_stream),
                    &PrinterIdentity {
                        ip: String::new(),
                        serial: "01P000000000000".into(),
                        access_code: "12345678".into(),
                        model: PrinterModel::P1S,
                    },
                )
                    .await;
            let err = result.err().expect("Expected error, got Ok");
            assert!(
                matches!(err, crate::error::Error::ProtocolViolation(_)),
                "Expected ProtocolViolation, got {:?}",
                err
            );
            server_task.await.unwrap();
        }

        #[tokio::test]
        async fn test_suback_rejection_returns_protocol_violation() {
            let (client_stream, mut server_stream) = tokio::io::duplex(8192);

            let server_task = tokio::spawn(async move {
                let mut discard = vec![0u8; 256];
                // Read CONNECT
                let _ = server_stream.read(&mut discard).await;
                // Reply CONNACK accepted
                server_stream
                    .write_all(&[0x20, 0x02, 0x00, 0x00])
                    .await
                    .unwrap();
                server_stream.flush().await.unwrap();

                // Read SUBSCRIBE
                let _ = server_stream.read(&mut discard).await;
                // Reply SUBACK with return code 0x80 (rejected)
                server_stream
                    .write_all(&[0x90, 0x03, 0x00, 0x01, 0x80])
                    .await
                    .unwrap();
                server_stream.flush().await.unwrap();
            });

            let result =
                MqttClient::connect(
                    TokioIo(client_stream),
                    &PrinterIdentity {
                        ip: String::new(),
                        serial: "01P000000000000".into(),
                        access_code: "12345678".into(),
                        model: PrinterModel::P1S,
                    },
                )
                    .await;
            let err = result.err().expect("Expected error, got Ok");
            assert!(
                matches!(err, crate::error::Error::ProtocolViolation(_)),
                "Expected ProtocolViolation for SUBACK rejection, got {:?}",
                err
            );
            server_task.await.unwrap();
        }

        #[tokio::test]
        async fn test_poll_telemetry_with_timer_resumes_split_frame_through_persistent_client() {
            // The resumable-frame-read invariant (.claude/rules/wire-read-deadline.md)
            // was previously only unit-tested against a bare `FrameReadState`/`read_exact_packet`
            // call (frame.rs) — never through a live, persistent `MqttClient::read_state`
            // field with a real (non-Dummy) timer, the exact combination `PrinterClient` uses via
            // `poll_telemetry_with_timer`. A regression reconstructing a fresh `FrameReadState`
            // per `poll_wire()` call (instead of reusing `self.read_state`) would go uncaught
            // without this.
            let (client_stream, mut server_stream) = tokio::io::duplex(8192);

            let server_task = tokio::spawn(async move {
                let mut discard = vec![0u8; 256];
                // Read CONNECT, reply CONNACK accepted.
                let _ = server_stream.read(&mut discard).await;
                server_stream
                    .write_all(&[0x20, 0x02, 0x00, 0x00])
                    .await
                    .unwrap();
                server_stream.flush().await.unwrap();
                // Read SUBSCRIBE, reply SUBACK accepted (QoS 1 granted).
                let _ = server_stream.read(&mut discard).await;
                server_stream
                    .write_all(&[0x90, 0x03, 0x00, 0x01, 0x01])
                    .await
                    .unwrap();
                server_stream.flush().await.unwrap();

                // Split a real PUBLISH QoS 1 frame across two write_all calls with a real sleep
                // between them, so the client's first poll attempt reads a partial frame,
                // stashes it in self.read_state, and the second attempt must resume from there.
                let frame = encode_publish_qos1(1, "device/01P000000000000/report", b"{\"print\":{}}");
                let split = frame.len() / 2;
                server_stream.write_all(&frame[..split]).await.unwrap();
                server_stream.flush().await.unwrap();
                tokio::time::sleep(core::time::Duration::from_millis(200)).await;
                server_stream.write_all(&frame[split..]).await.unwrap();
                server_stream.flush().await.unwrap();

                // Drain the automatic PUBACK the client sends back for the QoS 1 PUBLISH.
                let _ = server_stream.read(&mut discard).await;
            });

            let mut client =
                MqttClient::connect(
                    TokioIo(client_stream),
                    &PrinterIdentity {
                        ip: String::new(),
                        serial: "01P000000000000".into(),
                        access_code: "12345678".into(),
                        model: PrinterModel::P1S,
                    },
                )
                    .await
                    .expect("connect should succeed");

            let timer = crate::io::tokio::TokioTimer::new();
            let msg = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                client.poll_telemetry_with_timer(&timer),
            )
            .await
            .expect("poll_telemetry_with_timer hung past the meta-safety timeout")
            .expect("split PUBLISH frame should reassemble successfully");

            assert_eq!(msg.topic, "device/01P000000000000/report");
            assert_eq!(msg.payload, b"{\"print\":{}}");

            server_task.await.unwrap();
        }

        #[tokio::test]
        async fn test_publish_command_does_not_reset_zombie_timer_while_pending() {
            // publish_command() used to unconditionally set write_pending_secs to
            // Some(0) on every call, even while an earlier command's response was still
            // outstanding. A steady stream of new commands would then mask that earlier
            // command's zombie state forever, since the counter never reached
            // MQTT_ZOMBIE_TIMEOUT_SECS. It must only arm on the *first* unanswered command.
            //
            // Constructs the client directly (bypassing the CONNECT/SUBSCRIBE handshake
            // entirely, permitted since this test lives in the same module as the private
            // fields) rather than draining publish_command's writes with hand-rolled
            // one-shot `.read()` calls on a mock server: two publish_command calls in a row
            // have no read round-trip between them (unlike CONNECT->SUBSCRIBE, which the
            // client's own handshake await naturally serializes), so both writes can land in
            // the duplex buffer before a mock reader ever polls it — coalescing into a single
            // `.read()` and leaving a second one-shot `.read()` blocked forever. Just holding
            // `_server_stream` open (never reading it) sidesteps that hazard entirely.
            let (client_stream, _server_stream) = tokio::io::duplex(8192);
            let mut client = MqttClient {
                stream: TokioIo(client_stream),
                request_topic: "device/01P000000000000/request".to_string(),
                serial: "01P000000000000".to_string(),
                next_packet_id: 2,
                in_flight: BTreeMap::new(),
                pending_messages: VecDeque::new(),
                pending_bytes: 0,
                write_pending_secs: None,
                write_pending_sequence_id: None,
                ping_outstanding: false,
                last_outbound_ms: None,
                secs_since_last_message: 0,
                read_state: FrameReadState::default(),
                write_poisoned: false,
            };

            client
                .publish_command(b"{}")
                .await
                .expect("first publish failed");
            assert_eq!(client.write_pending_secs, Some(0));

            client.tick_zombie_check(5).expect("tick should not error");
            assert_eq!(client.write_pending_secs, Some(5));

            client
                .publish_command(b"{}")
                .await
                .expect("second publish failed");
            assert_eq!(
                client.write_pending_secs,
                Some(5),
                "a second publish while the first is still unanswered must not reset the zombie timer"
            );
        }

        #[tokio::test]
        async fn test_keepalive_ping_fires_only_once_the_interval_has_elapsed() {
            // Regression (issue #113): CONNECT advertises MQTT_KEEP_ALIVE_SECS, obliging this
            // client to send traffic within 45s, but nothing in the library did — a consumer
            // polling telemetry in a loop sent zero bytes and was dropped by the broker.
            //
            // Drives a controllable clock rather than TokioTimer: the interval under test is
            // 20s, and TokioTimer measures from its own construction, so a real-clock test would
            // have to either sleep 20s or backdate into a saturating_sub floor of 0.
            struct FakeClock {
                now_ms: core::cell::Cell<u64>,
            }
            impl crate::io::TimerProvider for FakeClock {
                async fn sleep(
                    &self,
                    _duration: core::time::Duration,
                ) -> Result<(), crate::io::TimerError> {
                    // Never completes, so write_frame_with_timer's write-vs-deadline race is
                    // always won by the write. A sleep that returned instantly would make every
                    // write look like a timeout.
                    core::future::pending().await
                }
                fn now_millis(&self) -> u64 {
                    self.now_ms.get()
                }
            }

            let timer = FakeClock {
                now_ms: core::cell::Cell::new(1_000),
            };
            let (client_stream, _server_stream) = tokio::io::duplex(8192);
            let mut client = MqttClient {
                stream: TokioIo(client_stream),
                request_topic: "device/01P000000000000/request".to_string(),
                serial: "01P000000000000".to_string(),
                next_packet_id: 2,
                in_flight: BTreeMap::new(),
                pending_messages: VecDeque::new(),
                pending_bytes: 0,
                write_pending_secs: None,
                write_pending_sequence_id: None,
                ping_outstanding: false,
                last_outbound_ms: None,
                secs_since_last_message: 0,
                read_state: FrameReadState::default(),
                write_poisoned: false,
            };

            // First call stamps the clock instead of pinging, so the interval is measured from
            // a real outbound event rather than from the epoch.
            client
                .send_keepalive_if_due(&timer)
                .await
                .expect("stamping call should not error");
            assert!(!client.ping_outstanding, "first call must not ping");
            assert!(
                client.last_outbound_ms.is_some(),
                "first call must stamp the clock"
            );

            // Still inside the interval: no ping.
            client
                .send_keepalive_if_due(&timer)
                .await
                .expect("in-interval call should not error");
            assert!(
                !client.ping_outstanding,
                "no ping is due while the connection has recent outbound traffic"
            );

            // Advance past the interval; the ping must now fire without any caller action — in
            // particular without the caller having driven tick_zombie_check, which is what the
            // pre-existing secs_since_last_message counter would have required.
            timer
                .now_ms
                .set(timer.now_ms.get() + (MQTT_PING_INTERVAL_SECS + 1) * 1000);
            client
                .send_keepalive_if_due(&timer)
                .await
                .expect("keepalive ping should send");
            assert!(
                client.ping_outstanding,
                "a ping must be sent once the outbound-silence interval has elapsed"
            );
        }

        #[tokio::test]
        async fn test_saturated_in_flight_queue_reports_backpressure_and_drains_on_tick() {
            // Regression: in_flight entries were removed only by a matching PUBACK, so a broker
            // that dropped acks leaked them permanently and every later publish_command returned
            // SocketError::TimedOut forever — inviting a caller's retry-on-timeout policy into an
            // infinite loop against a condition that was neither a timeout nor self-clearing.
            let (client_stream, _server_stream) = tokio::io::duplex(64 * 1024);
            let mut client = MqttClient {
                stream: TokioIo(client_stream),
                request_topic: "device/01P000000000000/request".to_string(),
                serial: "01P000000000000".to_string(),
                next_packet_id: 2,
                in_flight: (0..MQTT_IN_FLIGHT_LIMIT as u16).map(|id| (id + 1, 0)).collect(),
                pending_messages: VecDeque::new(),
                pending_bytes: 0,
                write_pending_secs: None,
                write_pending_sequence_id: None,
                ping_outstanding: false,
                last_outbound_ms: None,
                secs_since_last_message: 0,
                read_state: FrameReadState::default(),
                write_poisoned: false,
            };

            let result = client.publish_command(b"{}").await;
            assert!(
                matches!(result, Err(Error::Backpressure)),
                "saturation must report Backpressure, not a timeout, got {:?}",
                result
            );

            // A tick below the TTL leaves the queue saturated; crossing it drains it. Ages are
            // set directly and `secs_since_last_message` reset before each tick, so the
            // independent 60s stale-connection valve doesn't fire first and mask what's being
            // tested (it would, on any single tick large enough to reach the TTL).
            client.secs_since_last_message = 0;
            client.tick_zombie_check(1).expect("tick should not error");
            assert_eq!(client.in_flight_count(), MQTT_IN_FLIGHT_LIMIT);

            for age in client.in_flight.values_mut() {
                *age = MQTT_IN_FLIGHT_TTL_SECS - 1;
            }
            client.secs_since_last_message = 0;
            client.tick_zombie_check(1).expect("tick should not error");
            assert_eq!(client.in_flight_count(), 0);
            client
                .publish_command(b"{}")
                .await
                .expect("publish must succeed once the leaked entries aged out");
        }

        #[tokio::test]
        async fn test_poll_wire_only_clears_zombie_timer_on_matching_sequence_id() {
            // Regression for the bug this issue tracks: poll_wire's PUBLISH arm used to reset
            // write_pending_secs unconditionally on any incoming PUBLISH. Background telemetry
            // (push_status) arrives far more often than MQTT_ZOMBIE_TIMEOUT_SECS and carries its
            // own independent sequence_id, so that reset masked a real zombie episode (broker
            // silently discarding commands) forever. It must only clear on a PUBLISH whose
            // sequence_id matches the outstanding command's.
            let (client_stream, mut server_stream) = tokio::io::duplex(8192);
            let mut client = MqttClient {
                stream: TokioIo(client_stream),
                request_topic: "device/01P000000000000/request".to_string(),
                serial: "01P000000000000".to_string(),
                next_packet_id: 2,
                in_flight: BTreeMap::new(),
                pending_messages: VecDeque::new(),
                pending_bytes: 0,
                write_pending_secs: Some(0),
                write_pending_sequence_id: Some("100002".to_string()),
                ping_outstanding: false,
                last_outbound_ms: None,
                secs_since_last_message: 0,
                read_state: FrameReadState::default(),
                write_poisoned: false,
            };

            // Unrelated telemetry with its own low-value sequence_id must not clear the timer.
            let telemetry = encode_publish_qos1(
                1,
                "device/01P000000000000/report",
                b"{\"print\":{\"sequence_id\":\"1\"}}",
            );
            server_stream.write_all(&telemetry).await.unwrap();
            server_stream.flush().await.unwrap();

            let timer = crate::client::dummy::DummyTimer;
            client
                .poll_wire(&timer)
                .await
                .expect("telemetry PUBLISH should parse");
            assert_eq!(
                client.write_pending_secs,
                Some(0),
                "unrelated telemetry must not clear the write-zombie timer"
            );

            // The matching ack must clear it.
            let ack = encode_publish_qos1(
                2,
                "device/01P000000000000/report",
                b"{\"print\":{\"sequence_id\":\"100002\",\"result\":\"success\"}}",
            );
            server_stream.write_all(&ack).await.unwrap();
            server_stream.flush().await.unwrap();

            client
                .poll_wire(&timer)
                .await
                .expect("matching ack PUBLISH should parse");
            assert_eq!(
                client.write_pending_secs, None,
                "a PUBLISH echoing the outstanding command's sequence_id must clear the timer"
            );
            assert_eq!(client.write_pending_sequence_id, None);
        }

        #[tokio::test]
        async fn test_pushall_zombie_timer_clears_on_any_publish_since_it_has_no_echoed_ack() {
            // Regression: bambino-cli's monitor sends exactly one command at startup —
            // request_pushall() — then only pings. pushall (`pushing` wrapper) triggers an
            // unlabeled push_status stream [REF-MQTT-LIFECYCLE], not an echoed ack, so its
            // sequence_id can never match an incoming PUBLISH. Requiring strict correlation for
            // it (like print/system commands get) left write_pending_secs armed forever and
            // fired a false zombie timeout against real hardware within MQTT_ZOMBIE_TIMEOUT_SECS.
            let (client_stream, mut server_stream) = tokio::io::duplex(8192);
            let mut client = MqttClient {
                stream: TokioIo(client_stream),
                request_topic: "device/01P000000000000/request".to_string(),
                serial: "01P000000000000".to_string(),
                next_packet_id: 2,
                in_flight: BTreeMap::new(),
                pending_messages: VecDeque::new(),
                pending_bytes: 0,
                write_pending_secs: None,
                write_pending_sequence_id: None,
                ping_outstanding: false,
                last_outbound_ms: None,
                secs_since_last_message: 0,
                read_state: FrameReadState::default(),
                write_poisoned: false,
            };

            client
                .publish_command(b"{\"pushing\":{\"command\":\"pushall\",\"sequence_id\":\"20001\"}}")
                .await
                .expect("pushall publish failed");
            assert_eq!(client.write_pending_secs, Some(0));
            assert_eq!(
                client.write_pending_sequence_id, None,
                "pushall has no echoed ack to correlate against"
            );

            // An ordinary push_status update carrying the printer's own unrelated sequence_id
            // must still clear the timer, since pushall has nothing to correlate against.
            let telemetry = encode_publish_qos1(
                1,
                "device/01P000000000000/report",
                b"{\"print\":{\"sequence_id\":\"1\"}}",
            );
            server_stream.write_all(&telemetry).await.unwrap();
            server_stream.flush().await.unwrap();

            let timer = crate::client::dummy::DummyTimer;
            client
                .poll_wire(&timer)
                .await
                .expect("telemetry PUBLISH should parse");
            assert_eq!(client.write_pending_secs, None);
            assert_eq!(client.write_pending_sequence_id, None);
        }
    }
}
