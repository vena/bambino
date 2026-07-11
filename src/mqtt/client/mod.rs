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
use alloc::vec::Vec;

#[cfg(feature = "std")]
use std::collections::BTreeSet;
#[cfg(feature = "std")]
use std::collections::VecDeque;

use core::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

use crate::client::dummy::DummyTimer;
use crate::error::BambuError;
use crate::io::{AsyncIo, SocketError, TimerProvider};

mod codec;
use codec::{
    PACKET_TYPE_CONNACK, PACKET_TYPE_PINGRESP, PACKET_TYPE_PUBACK, PACKET_TYPE_PUBLISH,
    PACKET_TYPE_SUBACK, encode_connect, encode_pingreq, encode_puback, encode_publish_qos1,
    encode_subscribe,
};

mod frame;
use frame::{FrameReadState, MQTT_READ_TIMEOUT_SECS, read_exact_packet};

mod pending;

/// Monotonic counter for generating unique MQTT client IDs across connections.
/// Each `connect()` call increments this to avoid stale QoS 1 queue conflicts
/// when the broker hasn't fully torn down a prior session's TCP socket.
static CONNECTION_COUNTER: AtomicU32 = AtomicU32::new(0);

pub(crate) const MQTT_IN_FLIGHT_LIMIT: usize = 200;
pub(crate) const MQTT_ZOMBIE_TIMEOUT_SECS: u32 = 10;
pub(crate) const MQTT_STALE_CONNECTION_SECS: u32 = 60;

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
pub struct BambuMqttClient<IO: AsyncIo> {
    stream: IO,
    request_topic: String,
    next_packet_id: u16,
    /// Outgoing QoS 1 packet tracking registry. Handles up to 200 concurrent unacknowledged entries.
    in_flight: BTreeSet<u16>,
    /// Messages buffered by request-response round-trips (e.g. `poll_until`), drained first by `poll_telemetry()` before reading from the wire.
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
    /// Byte-level progress of an in-flight frame read, preserved across a timed-out `read_exact_packet` call so `poll_wire()` resumes correctly instead of desyncing the stream — see `FrameReadState`'s doc comment.
    read_state: FrameReadState,
}

/// Advances an MQTT packet identifier, skipping 0 (reserved) on wraparound.
fn advance_packet_id(current: u16) -> u16 {
    let next = current.wrapping_add(1);
    if next == 0 { 1 } else { next }
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

/// Writes and flushes a complete packet to `stream`, mapping I/O failures via `map_embedded_io_error_kind` instead of collapsing everything to a fixed `ConnectionAborted` (the previous behavior).
/// A free function (not a method) so `connect()` can call it before `Self` exists.
async fn write_frame<IO: AsyncIo>(stream: &mut IO, packet: &[u8]) -> Result<(), BambuError> {
    use embedded_io_async::Error as _;
    stream
        .write_all(packet)
        .await
        .map_err(|e| BambuError::NetworkError(map_embedded_io_error_kind(e.kind())))?;
    stream
        .flush()
        .await
        .map_err(|e| BambuError::NetworkError(map_embedded_io_error_kind(e.kind())))
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

        write_frame(&mut stream, &connect_pkt).await?;

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

        // BUG-032: MQTT v3.1.1 CONNACK codes 1-3 (unacceptable protocol version, identifier
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
                return Err(BambuError::AccessDenied);
            }
            other => {
                log::warn!(
                    "Broker rejected connection with CONNACK return code: {} (not an access-code rejection)",
                    other
                );
                return Err(BambuError::ProtocolViolation(
                    format!("Broker rejected connection with CONNACK return code {other}").into(),
                ));
            }
        }

        // Subscribe to report topic
        let report_topic = format!("device/{}/report", serial);

        log::debug!(
            "Sending SUBSCRIBE frame targeting topic: '{}' (granted QoS 1)",
            report_topic
        );

        let subscribe_pkt = encode_subscribe(1, &report_topic, 1);

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
        self.next_packet_id = advance_packet_id(self.next_packet_id);

        log::debug!(
            "Publishing QoS 1 command (packet_id: {}) to topic: '{}' (payload length: {} bytes)",
            packet_id,
            self.request_topic,
            payload.len()
        );

        let packet = encode_publish_qos1(packet_id, &self.request_topic, payload);

        write_frame(&mut self.stream, &packet).await?;

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

    /// Same as [`poll_telemetry()`](Self::poll_telemetry), but honors `timer` for the underlying wire read's per-read deadline (see [`poll_wire`](Self::poll_wire)).
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

                    // QoS 1 requires PUBACK; QoS 2 requires a PUBREC/PUBREL/PUBCOMP handshake,
                    // which this client doesn't implement (BUG-052) — Bambu printers never
                    // publish above QoS 1 in practice, so this stays a logged, non-fatal gap
                    // rather than a full protocol extension for a case never observed against
                    // real hardware. A broker that did send genuine QoS 2 would see no PUBREC
                    // and may retransmit with DUP set.
                    if qos == 1 {
                        let id = packet_id.expect("QoS 1 always has packet_id");
                        log::trace!("Sending automatic PUBACK for packet_id: {}", id);

                        let ack = encode_puback(id);
                        write_frame(&mut self.stream, &ack).await?;
                    } else if qos >= 2 {
                        log::warn!(
                            "Received QoS {} PUBLISH (packet_id: {:?}) — QoS 2 handshake \
                             (PUBREC/PUBREL/PUBCOMP) is not implemented; broker may retransmit",
                            qos,
                            packet_id
                        );
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
        write_frame(&mut self.stream, &ping).await?;
        self.ping_outstanding = true;
        Ok(())
    }

    /// Platform-agnostic timer tick update.
    ///
    /// Evaluates two independent liveness conditions:
    /// 1. **Write zombie**: A published command has gone unanswered for 10+ seconds
    ///    [REF-MQTT-ZOMBIE].
    /// 2. **Connection staleness**: No packets of any kind received for 60+ seconds,
    ///    indicating a silently dropped connection — independent of (1) [REF-MQTT-CONN].
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

    /// Returns the number of current un-acknowledged QoS 1 packets.
    pub fn get_in_flight_count(&self) -> usize {
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
        async fn test_connack_server_unavailable_returns_protocol_violation() {
            // BUG-032: CONNACK codes 1-3 (unacceptable protocol version, identifier rejected,
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
                BambuMqttClient::connect(TokioIo(client_stream), "01P000000000000", "12345678")
                    .await;
            let err = result.err().expect("Expected error, got Ok");
            assert!(
                matches!(err, crate::error::BambuError::ProtocolViolation(_)),
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
    }
}
