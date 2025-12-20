//! Publish request handling and outbox implementations.
//!
//! This module provides the channel-based publish request system and
//! `PublishOutbox` implementations for use with `MqttModule`.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender};
use heapless::Vec;

use super::traits::PublishOutbox;
use crate::QoS;

/// A request to publish a message, sent via channel from controllers to the runtime.
///
/// This struct holds references to the topic and payload, which must outlive
/// the request. For static topics/payloads (common in embedded), use `'static`.
#[derive(Debug, Clone)]
pub struct PublishRequest<'a> {
    /// The topic to publish to
    pub topic: &'a str,
    /// The payload bytes
    pub payload: &'a [u8],
    /// Quality of Service level
    pub qos: QoS,
}

pub type PublishRequestChannel<'a, const OUTBOX_DEPTH: usize> =
    Channel<CriticalSectionRawMutex, PublishRequest<'a>, OUTBOX_DEPTH>;

pub type PublishRequestSender<'a, const OUTBOX_DEPTH: usize> =
    Sender<'a, CriticalSectionRawMutex, PublishRequest<'a>, OUTBOX_DEPTH>;

pub type PublishRequestReceiver<'a, const OUTBOX_DEPTH: usize> =
    Receiver<'a, CriticalSectionRawMutex, PublishRequest<'a>, OUTBOX_DEPTH>;

/// A handle that allows controllers to publish MQTT messages without direct
/// access to the `MqttClient`.
///
/// This handle wraps a channel sender and can be cloned and passed to multiple
/// tasks. The runtime receives these requests and performs the actual publish.
#[derive(Clone, Copy)]
pub struct PublisherHandle<'a, const OUTBOX_DEPTH: usize> {
    tx: Sender<'a, CriticalSectionRawMutex, PublishRequest<'a>, OUTBOX_DEPTH>,
}

impl<'a, const OUTBOX_DEPTH: usize> PublisherHandle<'a, OUTBOX_DEPTH> {
    /// Create a new `PublisherHandle` from a channel sender.
    pub fn new(tx: Sender<'a, CriticalSectionRawMutex, PublishRequest<'a>, OUTBOX_DEPTH>) -> Self {
        Self { tx }
    }

    /// Publish a message asynchronously.
    ///
    /// This method sends the publish request to the runtime via the channel.
    /// It will wait if the channel is full.
    pub async fn publish(&self, topic: &'a str, payload: &'a [u8], qos: QoS) {
        let req = PublishRequest {
            topic,
            payload,
            qos,
        };
        self.tx.send(req).await;
    }

    /// Try to publish a message without waiting.
    ///
    /// Returns `false` if the channel is full.
    pub fn try_publish(&self, topic: &'a str, payload: &'a [u8], qos: QoS) -> bool {
        let req = PublishRequest {
            topic,
            payload,
            qos,
        };
        self.tx.try_send(req).is_ok()
    }
}

/// A buffered outbox that collects publish requests during module callbacks.
///
/// This is used internally by the runtime to collect publishes from `on_tick`
/// and `on_start` calls, then drain them asynchronously afterwards.
///
/// # Type Parameters
///
/// - `CAPACITY`: Maximum number of publish requests that can be buffered
/// - `TOPIC_SIZE`: Maximum topic string length
/// - `PAYLOAD_SIZE`: Maximum payload size
pub struct BufferedOutbox<const CAPACITY: usize, const TOPIC_SIZE: usize, const PAYLOAD_SIZE: usize>
{
    requests: Vec<OwnedPublishRequest<TOPIC_SIZE, PAYLOAD_SIZE>, CAPACITY>,
}

/// An owned publish request with inline storage for topic and payload.
///
/// This allows the outbox to store requests without requiring the original
/// data to remain borrowed.
#[derive(Clone)]
pub struct OwnedPublishRequest<const TOPIC_SIZE: usize, const PAYLOAD_SIZE: usize> {
    /// The topic (stored inline)
    pub topic: heapless::String<TOPIC_SIZE>,
    /// The payload (stored inline)
    pub payload: heapless::Vec<u8, PAYLOAD_SIZE>,
    /// Quality of Service level
    pub qos: QoS,
}

impl<const CAPACITY: usize, const TOPIC_SIZE: usize, const PAYLOAD_SIZE: usize>
    BufferedOutbox<CAPACITY, TOPIC_SIZE, PAYLOAD_SIZE>
{
    /// Create a new empty buffered outbox.
    pub fn new() -> Self {
        Self {
            requests: Vec::new(),
        }
    }

    /// Drain all buffered requests, returning an iterator.
    pub fn drain(
        &mut self,
    ) -> impl Iterator<Item = OwnedPublishRequest<TOPIC_SIZE, PAYLOAD_SIZE>> + '_ {
        self.requests.iter().cloned()
    }

    /// Clear all buffered requests.
    pub fn clear(&mut self) {
        self.requests.clear();
    }

    /// Check if the outbox is empty.
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    /// Get the number of buffered requests.
    pub fn len(&self) -> usize {
        self.requests.len()
    }
}

impl<const CAPACITY: usize, const TOPIC_SIZE: usize, const PAYLOAD_SIZE: usize> Default
    for BufferedOutbox<CAPACITY, TOPIC_SIZE, PAYLOAD_SIZE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const CAPACITY: usize, const TOPIC_SIZE: usize, const PAYLOAD_SIZE: usize> PublishOutbox
    for BufferedOutbox<CAPACITY, TOPIC_SIZE, PAYLOAD_SIZE>
{
    fn publish(&mut self, topic: &str, payload: &[u8], qos: QoS) {
        // Try to store the request; silently drop if full or data too large
        let mut topic_str = heapless::String::new();
        if topic_str.push_str(topic).is_err() {
            return; // Topic too long
        }

        let mut payload_vec = heapless::Vec::new();
        if payload_vec.extend_from_slice(payload).is_err() {
            return; // Payload too large
        }

        let req = OwnedPublishRequest {
            topic: topic_str,
            payload: payload_vec,
            qos,
        };

        let _ = self.requests.push(req); // Silently drop if full
    }
}
