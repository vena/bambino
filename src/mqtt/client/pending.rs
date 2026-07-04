//! Pending-message buffer management for `BambuMqttClient`.

#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(feature = "std")]
use std::collections::VecDeque;

use super::{BambuMqttClient, MqttMessage};
use crate::io::AsyncIo;

/// Upper bound on the combined topic+payload size of all buffered `pending_messages`.
/// Generous for a handful of telemetry updates, small enough to stay safe on ESP32.
/// Once exceeded, `push_pending()` evicts from the front (oldest first) until the new
/// message fits, logging a `log::warn!` for each eviction.
pub(crate) const MQTT_PENDING_BUFFER_MAX_BYTES: usize = 2_097_152; // 2 MiB

impl<IO: AsyncIo> BambuMqttClient<IO> {
    /// Combined topic+payload byte size accounted for a single buffered message.
    /// Shared by `push_pending()` (accounting on insert/evict) and `poll_telemetry()`
    /// (accounting on drain) so both stay in sync with `pending_bytes`.
    pub(crate) fn message_size(msg: &MqttMessage) -> usize {
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
    #[cfg(feature = "tokio")]
    mod async_tests {
        use super::super::*;
        use crate::io::TokioIo;
        use crate::mqtt::client::frame::FrameReadState;
        use std::collections::BTreeSet;

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
    }
}
