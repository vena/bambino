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
use alloc::collections::BTreeSet;
#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::collections::BTreeSet;
#[cfg(feature = "std")]
use std::collections::VecDeque;

use core::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

use crate::client::dummy::DummyTimer;
use crate::error::BambuError;
use crate::io::{AsyncIo, SocketError, TimerProvider, read_chunk};

mod codec;
use codec::{
    PACKET_TYPE_CONNACK, PACKET_TYPE_PINGRESP, PACKET_TYPE_PUBACK, PACKET_TYPE_PUBLISH,
    PACKET_TYPE_SUBACK, encode_connect, encode_pingreq, encode_puback, encode_publish_qos1,
    encode_subscribe,
};

/// Monotonic counter for generating unique MQTT client IDs across connections.
/// Each `connect()` call increments this to avoid stale QoS 1 queue conflicts
/// when the broker hasn't fully torn down a prior session's TCP socket.
static CONNECTION_COUNTER: AtomicU32 = AtomicU32::new(0);

pub(crate) const MQTT_MAX_PAYLOAD_BYTES: usize = 1_048_576; // 1 MiB
pub(crate) const MQTT_IN_FLIGHT_LIMIT: usize = 200;
pub(crate) const MQTT_ZOMBIE_TIMEOUT_SECS: u32 = 10;
pub(crate) const MQTT_STALE_CONNECTION_SECS: u32 = 60;
/// Upper bound on the combined topic+payload size of all buffered `pending_messages`.
/// Generous for a handful of telemetry updates, small enough to stay safe on ESP32.
/// Once exceeded, `push_pending()` evicts from the front (oldest first) until the new
/// message fits, logging a `log::warn!` for each eviction.
pub(crate) const MQTT_PENDING_BUFFER_MAX_BYTES: usize = 2_097_152; // 2 MiB

/// Per-call deadline for `read_exact_packet` when a genuine wall-clock
/// [`TimerProvider`] is available (see [`TimerProvider::has_real_clock`]).
///
/// Bounds a single `poll_wire()` invocation's total wait for *new* bytes to arrive —
/// independent of, and strictly lower-level than, `PrinterClient::poll_until`'s
/// `command_timeout_secs`/`POLL_UNTIL_MAX_MESSAGES` valves (`src/client/mod.rs`), which
/// only ever run *after* a full frame has already been received and therefore cannot
/// catch a stall that happens mid-read [REF-MQTT-STALL]. A connection that stalls with
/// zero incoming bytes may take up to this long to surface as
/// `BambuError::NetworkError(SocketError::TimedOut)`, even if the caller configured a
/// shorter `command_timeout_secs` — the two timeouts are independent layers, not summed
/// or coordinated.
pub(crate) const MQTT_READ_TIMEOUT_SECS: u64 = 30;

/// Byte-level progress of an in-flight MQTT frame read, preserved across a timed-out
/// `read_exact_packet` call so a subsequent call resumes exactly where the previous one
/// left off instead of misinterpreting still-arriving bytes of the *same* frame as a new
/// frame's header — see `read_exact_packet`'s doc comment for why losing this state
/// would permanently desync the stream parser.
#[derive(Default)]
enum FrameReadState {
    /// No partial frame in progress — the next read starts a fresh header byte.
    #[default]
    Idle,
    /// Header byte read; the MQTT variable-length "remaining length" field is not yet
    /// fully decoded.
    ReadingRemainingLength {
        header: u8,
        value: usize,
        multiplier: usize,
    },
    /// Remaining length fully decoded; `buf` is pre-sized to the full payload length and
    /// accumulates bytes as they arrive, `filled` tracks how many are valid so far.
    ReadingPayload {
        header: u8,
        buf: Vec<u8>,
        filled: usize,
    },
}

/// Reads exactly one standard MQTT frame asynchronously from our abstract socket,
/// resuming from `state` if a prior call on this same stream timed out partway through.
///
/// **Correctness invariant — never violate this:** on a `SocketError::TimedOut` return,
/// `state` must retain every byte already read for the in-progress frame. The MQTT wire
/// format has no resynchronization marker — if bytes already consumed from `stream` were
/// ever discarded here, the *next* call would start reading from the middle of whatever
/// the peer sends next, permanently desyncing the frame parser until the connection is
/// dropped and re-established (the same failure class as the `write_command` regression
/// documented in `CLAUDE.md`). This is why the payload is read via a loop of small
/// `read_chunk()` steps (each individually resumable) instead of one atomic multi-byte
/// read — see `read_chunk`'s doc comment for the cancellation-safety reasoning. A
/// `SocketError::ConnectionReset`/`InvalidInput` return means the connection itself is no
/// longer usable regardless of `state` — the caller must reconnect (constructing a new
/// `BambuMqttClient`, and thus a fresh `FrameReadState`) rather than keep polling the
/// same stream.
///
/// Computes a fresh deadline every call from `budget_ms` (not once per logical frame) —
/// each call to this function gets its own bounded window to make progress, regardless
/// of how many prior calls already timed out waiting on this same in-progress frame.
/// Callers outside tests should pass `MQTT_READ_TIMEOUT_SECS * 1000`; tests use a small
/// `budget_ms` directly so stalled-read regression tests don't need to wait out the real
/// production timeout.
async fn read_exact_packet<IO: AsyncIo, T: TimerProvider>(
    stream: &mut IO,
    state: &mut FrameReadState,
    timer: &T,
    budget_ms: u64,
) -> Result<(u8, Vec<u8>), SocketError> {
    let deadline_ms = if timer.has_real_clock() {
        Some(timer.now_millis().saturating_add(budget_ms))
    } else {
        None
    };

    // Fixed header packet type byte (only if not already read by a prior, timed-out call).
    if matches!(state, FrameReadState::Idle) {
        let mut header = [0u8; 1];
        let mut filled = 0;
        while filled < header.len() {
            let n = read_chunk(stream, &mut header[filled..], timer, deadline_ms).await?;
            filled += n;
        }
        *state = FrameReadState::ReadingRemainingLength {
            header: header[0],
            value: 0,
            multiplier: 1,
        };
    }

    // Variable-length remaining length (resumes mid-varint if a prior call stalled here).
    if let FrameReadState::ReadingRemainingLength {
        header,
        value,
        multiplier,
    } = state
    {
        loop {
            let mut b = [0u8; 1];
            let mut filled = 0;
            while filled < b.len() {
                let n = read_chunk(stream, &mut b[filled..], timer, deadline_ms).await?;
                filled += n;
            }
            *value += ((b[0] & 127) as usize) * *multiplier;
            if (b[0] & 128) == 0 {
                break;
            }
            *multiplier *= 128;
            if *multiplier > 128 * 128 * 128 {
                *state = FrameReadState::Idle;
                return Err(SocketError::InvalidInput); // Protocol violation
            }
        }

        let rem_len = *value;
        let hdr = *header;

        if rem_len > MQTT_MAX_PAYLOAD_BYTES {
            *state = FrameReadState::Idle;
            log::warn!("MQTT payload length {} exceeds maximum", rem_len);
            return Err(SocketError::InvalidInput);
        }

        *state = FrameReadState::ReadingPayload {
            header: hdr,
            buf: vec![0u8; rem_len],
            filled: 0,
        };
    }

    // Payload bytes (resumes from `filled` if a prior call stalled mid-payload).
    if let FrameReadState::ReadingPayload {
        header,
        buf,
        filled,
    } = state
    {
        while *filled < buf.len() {
            let n = read_chunk(stream, &mut buf[*filled..], timer, deadline_ms).await?;
            *filled += n;
        }
        let hdr = *header;
        let payload = core::mem::take(buf);
        *state = FrameReadState::Idle;
        return Ok((hdr, payload));
    }

    unreachable!("FrameReadState must be ReadingPayload after remaining-length decode")
}

// ============================================================================
// MQTT Client Session Management
// ============================================================================

/// Incoming MQTT message details parsed from the wire.
#[derive(Debug, Clone)]
pub struct MqttMessage {
    pub topic: String,
    pub payload: Vec<u8>,
}

/// Lightweight MQTT client session running over an established `AsyncIo` stream.
pub struct BambuMqttClient<IO: AsyncIo> {
    stream: IO,
    request_topic: String,
    next_packet_id: u16,
    /// Outgoing QoS 1 packet tracking registry. Handles up to 200 concurrent unacknowledged entries.
    in_flight: BTreeSet<u16>,
    /// Messages buffered by request-response round-trips (e.g. `poll_until`),
    /// drained first by `poll_telemetry()` before reading from the wire.
    pending_messages: VecDeque<MqttMessage>,
    /// Combined topic+payload byte size of every message currently in `pending_messages`.
    /// Kept in sync by `push_pending()` (adds/evicts) and `poll_telemetry()` (drains) so
    /// eviction in `push_pending()` never has to walk the whole deque to size it.
    pending_bytes: usize,
    /// Accumulated elapsed seconds since the last command publish while waiting for a response update.
    write_pending_secs: Option<u32>,
    /// Incremental scale of unacknowledged ping requests.
    ping_outstanding: bool,
    /// Accumulated elapsed seconds since the last received message of any kind.
    /// Used to detect silent connection loss independent of publish activity.
    secs_since_last_message: u32,
    /// Byte-level progress of an in-flight frame read, preserved across a timed-out
    /// `read_exact_packet` call so `poll_wire()` resumes correctly instead of desyncing
    /// the stream — see `FrameReadState`'s doc comment.
    read_state: FrameReadState,
}

impl<IO: AsyncIo> BambuMqttClient<IO> {
    /// Executes a secure local network connection handshake and subscription loop with the printer.
    ///
    /// **Authentication Note:** If the printer's physical broker rejects credentials due to
    /// an invalid access code, this function returns `BambuError::AccessDenied`.
    pub async fn connect(
        mut stream: IO,
        serial: &str,
        access_code: &str,
    ) -> Result<Self, BambuError> {
        let conn_id = CONNECTION_COUNTER.fetch_add(1, AtomicOrdering::Relaxed);
        let client_id = format!("bambino_{}_{}", serial, conn_id);
        let connect_pkt = encode_connect(&client_id, "bblp", access_code);

        log::debug!(
            "Transmitting CONNECT payload (client_id: '{}', user: 'bblp')",
            client_id
        );

        stream
            .write_all(&connect_pkt)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        stream
            .flush()
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

        // Read CONNACK packet. `DummyTimer` (`has_real_clock() == false`) makes
        // `read_exact_packet` fall back to a plain unbounded read here — identical to
        // this crate's pre-existing connect-time behavior. Deliberately not wired to
        // `PrinterClient`'s configurable `Timer`: `connect()` runs before a `BambuMqttClient`
        // (and thus a persistent `FrameReadState`) exists, and the connect-phase handshake
        // (TCP+TLS dial timeout, `PrinterClient::connect_timeout_secs`) is a separate concern
        // from this fix's target (a stall on an already-established connection,
        // mid-`poll_wire()`).
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
            return Err(BambuError::ProtocolViolation(
                "Expected CONNACK frame".into(),
            ));
        }
        if payload_buf.len() < 2 {
            return Err(BambuError::ProtocolViolation(
                "Short CONNACK payload".into(),
            ));
        }
        let connack_code = payload_buf[1];

        log::debug!("Connection accepted response byte: {}", connack_code);

        if connack_code != 0 {
            log::warn!(
                "Broker rejected connection with CONNACK return code: {}",
                connack_code
            );
            return Err(BambuError::AccessDenied);
        }

        // Subscribe to report topic
        let report_topic = format!("device/{}/report", serial);

        log::debug!(
            "Sending SUBSCRIBE frame targeting topic: '{}' (granted QoS 1)",
            report_topic
        );

        let subscribe_pkt = encode_subscribe(1, &report_topic, 1);

        stream
            .write_all(&subscribe_pkt)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        stream
            .flush()
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

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
            return Err(BambuError::ProtocolViolation(
                "Expected SUBACK frame".into(),
            ));
        }
        if payload_buf.len() < 3 {
            return Err(BambuError::ProtocolViolation("Short SUBACK payload".into()));
        }
        let return_code = payload_buf[2];

        log::debug!("SUBACK response status granted: 0x{:02X}", return_code);

        if return_code == 0x80 {
            return Err(BambuError::ProtocolViolation(
                "Subscription rejected by physical broker".into(),
            ));
        }

        Ok(Self {
            stream,
            request_topic: format!("device/{}/request", serial),
            next_packet_id: 2, // 1 is consumed by SUBSCRIBE handshake
            in_flight: BTreeSet::new(),
            pending_messages: VecDeque::new(),
            pending_bytes: 0,
            write_pending_secs: None,
            ping_outstanding: false,
            secs_since_last_message: 0,
            read_state: FrameReadState::default(),
        })
    }

    /// Submits a serialized JSON command payload to the printer's request channel.
    ///
    /// **In-flight Bounds Verification:**
    /// If the unacknowledged queue size equals or exceeds 200, this function returns a
    /// network timeout error to protect memory space and prevent packet drift [REF-MQTT-CONN].
    pub async fn publish_command(&mut self, payload: &[u8]) -> Result<u16, BambuError> {
        if self.in_flight.len() >= MQTT_IN_FLIGHT_LIMIT {
            log::warn!(
                "In-flight command backlog saturated ({} items)",
                self.in_flight.len()
            );
            return Err(BambuError::NetworkError(SocketError::TimedOut));
        }

        let packet_id = self.next_packet_id;
        self.next_packet_id = self.next_packet_id.wrapping_add(1);
        if self.next_packet_id == 0 {
            self.next_packet_id = 1; // 0 is reserved in MQTT specifications
        }

        log::debug!(
            "Publishing QoS 1 command (packet_id: {}) to topic: '{}' (payload length: {} bytes)",
            packet_id,
            self.request_topic,
            payload.len()
        );

        let packet = encode_publish_qos1(packet_id, &self.request_topic, payload);

        self.stream
            .write_all(&packet)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        self.stream
            .flush()
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;

        self.in_flight.insert(packet_id);

        // Arm/reset write-channel zombie detection tracking [REF-MQTT-ZOMBIE]
        self.write_pending_secs = Some(0);

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
    pub async fn poll_telemetry(&mut self) -> Result<MqttMessage, BambuError> {
        // `DummyTimer` has no real wall-clock (`has_real_clock() == false`), so
        // `poll_wire()` falls back to its pre-existing unbounded read here — this public,
        // timer-less entry point (used directly by e.g. `tests/mqtt_test.rs` and any
        // caller holding a raw `BambuMqttClient` without a `PrinterClient`) keeps its
        // exact prior behavior. `PrinterClient` callers get the new bounded-read
        // protection via `poll_telemetry_with_timer()` instead, since they have a real
        // `Timer` available.
        self.poll_telemetry_with_timer(&DummyTimer).await
    }

    /// Same as [`poll_telemetry()`](Self::poll_telemetry), but honors `timer` for the
    /// underlying wire read's per-read deadline (see [`poll_wire`](Self::poll_wire)).
    ///
    /// Used by `PrinterClient`, which owns its own configurable `Timer` and wants the
    /// stalled-read protection that requires a genuine wall-clock to be meaningful.
    pub(crate) async fn poll_telemetry_with_timer<T: TimerProvider>(
        &mut self,
        timer: &T,
    ) -> Result<MqttMessage, BambuError> {
        if let Some(buffered) = self.pending_messages.pop_front() {
            self.pending_bytes = self
                .pending_bytes
                .saturating_sub(Self::message_size(&buffered));
            return Ok(buffered);
        }
        self.poll_wire(timer).await
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
    ) -> Result<MqttMessage, BambuError> {
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
                        return Err(BambuError::ProtocolViolation(
                            "Short publish payload".into(),
                        ));
                    }
                    let topic_len = u16::from_be_bytes([payload_buf[0], payload_buf[1]]) as usize;
                    if payload_buf.len() < 2 + topic_len {
                        return Err(BambuError::ProtocolViolation(
                            "Invalid topic length bounds".into(),
                        ));
                    }

                    let topic = core::str::from_utf8(&payload_buf[2..2 + topic_len])
                        .map_err(|_| BambuError::ProtocolViolation("Non-UTF8 topic name".into()))?
                        .to_string();

                    let mut payload_start = 2 + topic_len;
                    let mut packet_id = None;
                    if qos >= 1 {
                        if payload_buf.len() < payload_start + 2 {
                            return Err(BambuError::ProtocolViolation(
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

                    // QoS 1 requires PUBACK; QoS 2 would require PUBREC (unsupported)
                    if qos == 1 {
                        let id = packet_id.expect("QoS 1 always has packet_id");
                        log::trace!("Sending automatic PUBACK for packet_id: {}", id);

                        let ack = encode_puback(id);
                        self.stream.write_all(&ack).await.map_err(|_| {
                            BambuError::NetworkError(SocketError::ConnectionAborted)
                        })?;
                        self.stream.flush().await.map_err(|_| {
                            BambuError::NetworkError(SocketError::ConnectionAborted)
                        })?;
                    }

                    // Reset write channel zombie tracking since a telemetry update was received
                    self.write_pending_secs = None;

                    return Ok(MqttMessage { topic, payload });
                }
                PACKET_TYPE_PUBACK => {
                    if payload_buf.len() < 2 {
                        return Err(BambuError::ProtocolViolation(
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
    pub async fn send_ping(&mut self) -> Result<(), BambuError> {
        log::trace!("Transmitting PINGREQ keep-alive packet");

        let ping = encode_pingreq();
        self.stream
            .write_all(&ping)
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        self.stream
            .flush()
            .await
            .map_err(|_| BambuError::NetworkError(SocketError::ConnectionAborted))?;
        self.ping_outstanding = true;
        Ok(())
    }

    /// Platform-agnostic timer tick update.
    ///
    /// Evaluates two independent liveness conditions:
    /// 1. **Write zombie**: A published command has gone unanswered for 10+ seconds.
    /// 2. **Connection staleness**: No packets of any kind received for 60+ seconds,
    ///    indicating a silently dropped connection [REF-MQTT-ZOMBIE].
    pub fn tick_zombie_check(&mut self, elapsed_secs: u32) -> Result<(), BambuError> {
        if let Some(ref mut secs) = self.write_pending_secs {
            *secs += elapsed_secs;
            if *secs >= MQTT_ZOMBIE_TIMEOUT_SECS {
                log::warn!(
                    "Zombie state detected: command issued but zero telemetry updates received for >= {}s",
                    MQTT_ZOMBIE_TIMEOUT_SECS
                );
                return Err(BambuError::Timeout);
            }
        }

        self.secs_since_last_message += elapsed_secs;
        if self.secs_since_last_message >= MQTT_STALE_CONNECTION_SECS {
            log::warn!(
                "Connection stale: no packets received for >= {}s",
                MQTT_STALE_CONNECTION_SECS
            );
            return Err(BambuError::Timeout);
        }

        Ok(())
    }

    /// Returns a slice containing current un-acknowledged QoS 1 packet identifiers.
    pub fn get_in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    /// Combined topic+payload byte size accounted for a single buffered message.
    /// Shared by `push_pending()` (accounting on insert/evict) and `poll_telemetry()`
    /// (accounting on drain) so both stay in sync with `pending_bytes`.
    fn message_size(msg: &MqttMessage) -> usize {
        msg.topic.len() + msg.payload.len()
    }

    /// Stashes a message back into the pending buffer for later retrieval.
    ///
    /// Used by `PrinterClient::poll_until()` to buffer non-matching messages
    /// during request-response round-trips.
    ///
    /// **Bounded growth:** if adding `msg` would push the buffer's total tracked size
    /// (`pending_bytes`) past `MQTT_PENDING_BUFFER_MAX_BYTES`, the oldest buffered
    /// messages are evicted from the front (FIFO) until it fits, each eviction logged
    /// via `log::warn!`. Without this, a caller that keeps issuing request-response
    /// calls whose responses never arrive (firmware bug, wrong echoed sequence_id, or a
    /// malicious/compromised device on the LAN) could grow this buffer unboundedly —
    /// unacceptable on the ESP-IDF/Embassy targets this crate supports, where RAM is
    /// measured in KB.
    pub(crate) fn push_pending(&mut self, msg: MqttMessage) {
        let incoming_size = Self::message_size(&msg);

        while !self.pending_messages.is_empty()
            && self.pending_bytes + incoming_size > MQTT_PENDING_BUFFER_MAX_BYTES
        {
            if let Some(evicted) = self.pending_messages.pop_front() {
                let evicted_size = Self::message_size(&evicted);
                self.pending_bytes = self.pending_bytes.saturating_sub(evicted_size);
                log::warn!(
                    "Pending MQTT message buffer exceeded {} bytes; evicting oldest buffered message (topic: '{}', {} bytes)",
                    MQTT_PENDING_BUFFER_MAX_BYTES,
                    evicted.topic,
                    evicted_size
                );
            }
        }

        self.pending_bytes += incoming_size;
        self.pending_messages.push_back(msg);
    }

    /// Scans the pending buffer (FIFO order) for the first message `matcher` accepts,
    /// removing and returning it. Non-matching messages are left in the buffer in their
    /// original relative order.
    ///
    /// Used by `PrinterClient::poll_until()` to check previously-buffered messages
    /// (stashed by an earlier, unrelated `poll_until()` call) for a match before falling
    /// through to reading new packets off the wire.
    pub(crate) fn take_pending_matching<F, T>(&mut self, mut matcher: F) -> Option<T>
    where
        F: FnMut(&MqttMessage) -> Option<T>,
    {
        let mut survivors = VecDeque::with_capacity(self.pending_messages.len());
        let mut result = None;

        while let Some(msg) = self.pending_messages.pop_front() {
            let matched = if result.is_none() {
                matcher(&msg)
            } else {
                None
            };
            match matched {
                Some(r) => {
                    self.pending_bytes =
                        self.pending_bytes.saturating_sub(Self::message_size(&msg));
                    result = Some(r);
                }
                None => survivors.push_back(msg),
            }
        }

        self.pending_messages = survivors;
        result
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_packet_id_skips_zero_on_wraparound() {
        let mut next_packet_id: u16 = u16::MAX;

        let issued_id = next_packet_id;
        next_packet_id = next_packet_id.wrapping_add(1);
        if next_packet_id == 0 {
            next_packet_id = 1;
        }

        assert_eq!(issued_id, u16::MAX);
        assert_eq!(next_packet_id, 1, "Packet ID must skip 0 after wraparound");
    }

    #[test]
    fn test_packet_id_normal_increment() {
        let mut next_packet_id: u16 = 100;

        next_packet_id = next_packet_id.wrapping_add(1);
        if next_packet_id == 0 {
            next_packet_id = 1;
        }

        assert_eq!(next_packet_id, 101);
    }

    #[test]
    fn test_packet_id_one_before_max() {
        let mut next_packet_id: u16 = u16::MAX - 1;

        next_packet_id = next_packet_id.wrapping_add(1);
        if next_packet_id == 0 {
            next_packet_id = 1;
        }

        assert_eq!(
            next_packet_id,
            u16::MAX,
            "ID before MAX should increment normally"
        );
    }

    #[cfg(feature = "tokio")]
    mod async_tests {
        use super::super::*;
        use crate::io::TokioIo;
        use crate::mqtt::client::codec::encode_remaining_length;
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
                BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
                    .await;
            let err = result.err().expect("Expected error, got Ok");
            assert!(
                matches!(err, crate::error::BambuError::AccessDenied),
                "Expected AccessDenied, got {:?}",
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
                BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
                    .await;
            let err = result.err().expect("Expected error, got Ok");
            assert!(
                matches!(err, crate::error::BambuError::ProtocolViolation(_)),
                "Expected ProtocolViolation for SUBACK rejection, got {:?}",
                err
            );
            server_task.await.unwrap();
        }

        #[tokio::test]
        async fn test_read_exact_packet_oom_guard() {
            // Craft a packet with remaining length exceeding MQTT_MAX_PAYLOAD_BYTES (1 MiB)
            let oversized_len: usize = MQTT_MAX_PAYLOAD_BYTES + 1;
            let mut data = vec![0x30u8]; // PUBLISH header
            data.extend_from_slice(&encode_remaining_length(oversized_len));

            let cursor = std::io::Cursor::new(data);
            let mut stream = TokioIo(cursor);
            let mut state = FrameReadState::default();
            let result = read_exact_packet(
                &mut stream,
                &mut state,
                &DummyTimer,
                MQTT_READ_TIMEOUT_SECS * 1000,
            )
            .await;
            assert!(
                matches!(result, Err(crate::io::SocketError::InvalidInput)),
                "Expected InvalidInput for oversized payload, got {:?}",
                result
            );
        }

        #[tokio::test]
        async fn test_read_exact_packet_malformed_remaining_length() {
            // 5 continuation bytes → multiplier exceeds 128^3, protocol violation
            let data = vec![0x30, 0x80, 0x80, 0x80, 0x80, 0x01];
            let cursor = std::io::Cursor::new(data);
            let mut stream = TokioIo(cursor);
            let mut state = FrameReadState::default();
            let result = read_exact_packet(
                &mut stream,
                &mut state,
                &DummyTimer,
                MQTT_READ_TIMEOUT_SECS * 1000,
            )
            .await;
            assert!(
                matches!(result, Err(crate::io::SocketError::InvalidInput)),
                "Expected InvalidInput for malformed remaining length, got {:?}",
                result
            );
        }

        /// Builds a `BambuMqttClient` without going through `connect()`'s handshake — the
        /// stream is never touched by the pending-buffer tests below, so an unread/unwritten
        /// in-memory cursor is sufficient.
        fn test_client() -> BambuMqttClient<TokioIo<std::io::Cursor<Vec<u8>>>> {
            BambuMqttClient {
                stream: TokioIo(std::io::Cursor::new(Vec::new())),
                request_topic: "device/test/request".to_string(),
                next_packet_id: 2,
                in_flight: BTreeSet::new(),
                pending_messages: VecDeque::new(),
                pending_bytes: 0,
                write_pending_secs: None,
                ping_outstanding: false,
                secs_since_last_message: 0,
                read_state: FrameReadState::default(),
            }
        }

        /// Regression test: a caller that keeps issuing request-response calls whose responses
        /// never arrive (firmware bug, wrong echoed sequence_id, or a malicious/compromised
        /// device on the LAN) must not be able to grow `pending_messages` without bound —
        /// unacceptable on ESP-IDF/Embassy targets where RAM is measured in KB. Pushes 320
        /// never-matching messages (well past a generous margin) and asserts the buffer stays
        /// within `MQTT_PENDING_BUFFER_MAX_BYTES` with the oldest entries evicted first (FIFO).
        #[test]
        fn test_push_pending_evicts_oldest_beyond_max_bytes() {
            let mut client = test_client();

            // ~8 KiB payload per message; 320 messages ≈ 2.5 MiB, comfortably past the 2 MiB cap.
            let payload_size = 8 * 1024;
            let total_messages = 320;
            for i in 0..total_messages {
                client.push_pending(MqttMessage {
                    topic: format!("device/test/report/{}", i),
                    payload: vec![0u8; payload_size],
                });
            }

            assert!(
                client.pending_bytes <= MQTT_PENDING_BUFFER_MAX_BYTES,
                "pending_bytes ({}) exceeded cap ({})",
                client.pending_bytes,
                MQTT_PENDING_BUFFER_MAX_BYTES
            );
            assert!(
                client.pending_messages.len() < total_messages,
                "expected eviction to have dropped some of the {} pushed messages, {} remain",
                total_messages,
                client.pending_messages.len()
            );

            // FIFO eviction: the newest message must have survived...
            let newest = client
                .pending_messages
                .back()
                .expect("buffer should not be empty");
            assert_eq!(
                newest.topic,
                format!("device/test/report/{}", total_messages - 1)
            );

            // ...and the very first pushed message must have been evicted.
            assert!(
                !client
                    .pending_messages
                    .iter()
                    .any(|m| m.topic == "device/test/report/0"),
                "oldest message should have been evicted first"
            );
        }

        /// Regression test for the `poll_until` integration: `take_pending_matching` must
        /// find and remove exactly the matching message, leaving the rest in their original
        /// relative order, and must keep `pending_bytes` accounting in sync with the removal.
        #[test]
        fn test_take_pending_matching_removes_only_the_match() {
            let mut client = test_client();

            client.push_pending(MqttMessage {
                topic: "a".to_string(),
                payload: vec![1],
            });
            client.push_pending(MqttMessage {
                topic: "b".to_string(),
                payload: vec![2, 2],
            });
            client.push_pending(MqttMessage {
                topic: "c".to_string(),
                payload: vec![3],
            });
            let bytes_before = client.pending_bytes;

            let found = client.take_pending_matching(|m| {
                if m.topic == "b" {
                    Some(m.payload.clone())
                } else {
                    None
                }
            });

            assert_eq!(found, Some(vec![2, 2]));
            let topics: Vec<&str> = client
                .pending_messages
                .iter()
                .map(|m| m.topic.as_str())
                .collect();
            assert_eq!(
                topics,
                vec!["a", "c"],
                "non-matching messages must survive in order"
            );
            // Removed message was topic "b" (1 byte) + payload [2, 2] (2 bytes) = 3 bytes.
            assert_eq!(
                client.pending_bytes,
                bytes_before - 3,
                "pending_bytes must shrink by exactly the removed message's size"
            );
        }

        /// `take_pending_matching` must return `None` and leave the buffer untouched when
        /// nothing matches.
        #[test]
        fn test_take_pending_matching_returns_none_when_no_match() {
            let mut client = test_client();
            client.push_pending(MqttMessage {
                topic: "a".to_string(),
                payload: vec![1],
            });
            let bytes_before = client.pending_bytes;

            let found: Option<()> = client.take_pending_matching(|_| None);

            assert_eq!(found, None);
            assert_eq!(client.pending_messages.len(), 1);
            assert_eq!(client.pending_bytes, bytes_before);
        }

        /// Regression test: a connection that stalls with zero
        /// incoming bytes must not hang `read_exact_packet`/`poll_wire` forever. Uses a
        /// `tokio::io::duplex` whose server side never writes anything, so the client's
        /// low-level `read()` call is genuinely pending (not merely slow) — exactly the
        /// "dead TCP, printer powered off mid-session" scenario the fix targets. Passes
        /// a small `budget_ms` directly (bypassing the real `MQTT_READ_TIMEOUT_SECS`
        /// constant) so this test doesn't need to wait out the production timeout. The
        /// outer `tokio::time::timeout` is a meta-safety net: if the implementation
        /// regresses to hanging forever, this test fails promptly instead of wedging the
        /// whole suite.
        #[tokio::test]
        async fn test_read_exact_packet_stalled_connection_times_out() {
            let (client_stream, _server_stream) = tokio::io::duplex(64);
            // Server side is kept alive (bound to `_server_stream`) but never writes —
            // dropping it would deliver `Ok(0)`/EOF instead of a genuine stall.

            let mut stream = TokioIo(client_stream);
            let mut state = FrameReadState::default();
            let timer = crate::io::tokio::TokioTimer::new();
            let budget_ms = 50;

            let started = std::time::Instant::now();
            let result = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                read_exact_packet(&mut stream, &mut state, &timer, budget_ms),
            )
            .await
            .expect(
                "read_exact_packet hung past the 5s meta-safety timeout instead of \
                 honoring its own budget — this is the exact regression this test guards \
                 against",
            );
            let elapsed = started.elapsed();

            assert!(
                matches!(result, Err(crate::io::SocketError::TimedOut)),
                "Expected TimedOut for a stalled connection, got {:?}",
                result
            );
            assert!(
                elapsed < core::time::Duration::from_secs(2),
                "read_exact_packet took {:?} to time out against a {}ms budget — too slow",
                elapsed,
                budget_ms
            );
        }

        /// Regression test for the correctness hinge above: bytes already read
        /// into a partial-packet buffer before a timeout must never be lost. Simulates a
        /// connection that delivers *part* of a frame, stalls long enough to time out,
        /// then delivers the rest — and asserts the second `read_exact_packet` call
        /// reconstructs the exact original frame (not corrupted, not desynced, not
        /// duplicated), proving `FrameReadState` correctly carried the partial payload
        /// across the timed-out attempt.
        #[tokio::test]
        async fn test_read_exact_packet_resumes_after_timeout_without_losing_bytes() {
            let (client_stream, mut server_stream) = tokio::io::duplex(64);
            let mut stream = TokioIo(client_stream);
            let mut state = FrameReadState::default();
            let timer = crate::io::tokio::TokioTimer::new();

            // Full intended frame: header 0x99, remaining-length 4, payload [AA BB CC DD].
            // Server sends the header, remaining-length, and only the first 2 payload
            // bytes, then stops — the client will read header+remlen+2 payload bytes
            // successfully, then stall waiting for the last 2 payload bytes.
            server_stream
                .write_all(&[0x99, 0x04, 0xAA, 0xBB])
                .await
                .unwrap();
            server_stream.flush().await.unwrap();

            let first_attempt = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                read_exact_packet(&mut stream, &mut state, &timer, 50),
            )
            .await
            .expect("first read_exact_packet attempt hung past the meta-safety timeout");

            assert!(
                matches!(first_attempt, Err(crate::io::SocketError::TimedOut)),
                "Expected the first attempt to time out waiting on the missing payload \
                 bytes, got {:?}",
                first_attempt
            );

            // The partial frame must be preserved exactly: header captured, 2 of 4
            // payload bytes already landed correctly, nothing corrupted or lost.
            match &state {
                FrameReadState::ReadingPayload {
                    header,
                    buf,
                    filled,
                } => {
                    assert_eq!(*header, 0x99, "header byte must survive the timeout");
                    assert_eq!(
                        *filled, 2,
                        "exactly the 2 bytes that arrived must be recorded"
                    );
                    assert_eq!(
                        &buf[..2],
                        &[0xAA, 0xBB],
                        "already-read payload bytes must not be corrupted"
                    );
                }
                other => panic!(
                    "expected FrameReadState::ReadingPayload with 2 bytes filled after a \
                     mid-payload timeout, got a different state variant (state debug \
                     unavailable, matched arm: {})",
                    match other {
                        FrameReadState::Idle => "Idle",
                        FrameReadState::ReadingRemainingLength { .. } => "ReadingRemainingLength",
                        FrameReadState::ReadingPayload { .. } => unreachable!(),
                    }
                ),
            }

            // Now the rest of the frame arrives.
            server_stream.write_all(&[0xCC, 0xDD]).await.unwrap();
            server_stream.flush().await.unwrap();

            let second_attempt = tokio::time::timeout(
                core::time::Duration::from_secs(5),
                read_exact_packet(&mut stream, &mut state, &timer, 2000),
            )
            .await
            .expect("second read_exact_packet attempt hung past the meta-safety timeout")
            .expect("second attempt should succeed now that the rest of the frame arrived");

            assert_eq!(
                second_attempt,
                (0x99u8, vec![0xAA, 0xBB, 0xCC, 0xDD]),
                "resumed read must reconstruct the exact original frame with no lost, \
                 duplicated, or reordered bytes"
            );
            assert!(
                matches!(state, FrameReadState::Idle),
                "state must reset to Idle after a fully-assembled frame is returned"
            );
        }
    }
}
