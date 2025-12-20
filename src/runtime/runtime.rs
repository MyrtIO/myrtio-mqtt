//! MQTT Runtime - drives modules and handles the event loop.

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Receiver;
use embassy_time::{Duration, Instant, Timer};
use heapless::Vec;

use super::traits::MqttModule;
use super::publisher::{BufferedOutbox, PublishRequest};
use super::registry::TopicRegistry;
use crate::client::MqttClient;
use crate::error::MqttError;
use crate::transport::{MqttTransport, TransportError};
use crate::{MqttEvent, QoS};

/// The MQTT runtime that drives modules and handles the event loop.
///
/// The runtime owns the `MqttClient` and multiplexes between:
/// - Incoming MQTT messages (dispatched to modules)
/// - Outgoing publish requests from controllers (via channel)
/// - Periodic ticks for module housekeeping
///
/// # Object-Safe Module Support
///
/// The runtime is generic over the module type `M`, which must implement
/// `MqttModule<MAX_TOPICS>`. Because `MqttModule` is object-safe, you can use:
///
/// - Concrete module types for maximum performance
/// - `&mut dyn MqttModule<MAX_TOPICS>` for trait objects (no `Box` needed)
///
/// # Topic Registration
///
/// During startup, the runtime calls `module.register()` to collect all topics
/// the module wants to subscribe to. Topics should be `'static` strings for
/// embedded use cases.
///
/// # Publishing Pattern
///
/// Modules use a `BufferedOutbox` to queue publish requests during `on_tick`
/// and `on_start`. The runtime then drains the outbox and performs the actual
/// async publishing.
pub struct MqttRuntime<
    'a,
    T,
    M,
    const MAX_TOPICS: usize,
    const BUF_SIZE: usize,
    const OUTBOX_DEPTH: usize,
> where
    T: MqttTransport,
    M: MqttModule<MAX_TOPICS>,
{
    client: MqttClient<'a, T, MAX_TOPICS, BUF_SIZE>,
    module: M,
    publisher_rx: Receiver<'a, CriticalSectionRawMutex, PublishRequest<'a>, OUTBOX_DEPTH>,
}

/// Constants for the internal publish outbox used during module callbacks.
const OUTBOX_CAPACITY: usize = 8;
const OUTBOX_TOPIC_SIZE: usize = 128;
const OUTBOX_PAYLOAD_SIZE: usize = 512;

impl<'a, T, M, const MAX_TOPICS: usize, const BUF_SIZE: usize, const OUTBOX_DEPTH: usize>
    MqttRuntime<'a, T, M, MAX_TOPICS, BUF_SIZE, OUTBOX_DEPTH>
where
    T: MqttTransport,
    T::Error: TransportError,
    M: MqttModule<MAX_TOPICS>,
{
    /// Create a new MQTT runtime.
    ///
    /// # Arguments
    ///
    /// - `client`: The MQTT client (not yet connected)
    /// - `module`: The module (or composed modules) to drive
    /// - `publisher_rx`: Receiver end of the publish request channel
    pub fn new(
        client: MqttClient<'a, T, MAX_TOPICS, BUF_SIZE>,
        module: M,
        publisher_rx: Receiver<'a, CriticalSectionRawMutex, PublishRequest<'a>, OUTBOX_DEPTH>,
    ) -> Self {
        Self {
            client,
            module,
            publisher_rx,
        }
    }

    /// Run the MQTT runtime event loop.
    ///
    /// This method:
    /// 1. Connects to the MQTT broker
    /// 2. Subscribes to all topics registered by the module
    /// 3. Calls `on_start` for initial setup
    /// 4. Enters the main loop handling messages, publishes, and ticks
    ///
    /// This method runs forever unless an error occurs.
    pub async fn run(&mut self) -> Result<(), MqttError<T::Error>> {
        // Connect to the broker
        self.client.connect().await?;

        // Collect and subscribe to topics in a scoped block.
        {
            let mut registry = TopicRegistry::<MAX_TOPICS>::new();
            self.module.register(&mut registry);

            // Copy topics to a temporary Vec so we can subscribe after dropping registry
            let mut topics_to_subscribe: Vec<&str, MAX_TOPICS> = Vec::new();
            for topic in registry.iter() {
                let _ = topics_to_subscribe.push(*topic);
            }

            // Subscribe to all registered topics
            for topic in topics_to_subscribe.iter() {
                self.client.subscribe(topic, QoS::AtMostOnce).await?;
            }
        }

        // Create a reusable outbox for module callbacks
        let mut outbox: BufferedOutbox<OUTBOX_CAPACITY, OUTBOX_TOPIC_SIZE, OUTBOX_PAYLOAD_SIZE> =
            BufferedOutbox::new();

        // Call on_start for initial setup
        self.module.on_start(&mut outbox);
        self.drain_outbox(&mut outbox).await?;

        // Initial tick
        let mut next_tick = self.module.on_tick(&mut outbox);
        self.drain_outbox(&mut outbox).await?;

        // Main event loop
        loop {
            // Track when tick is due
            let tick_deadline = Instant::now() + next_tick;

            // First, check for incoming publish requests (non-blocking)
            if let Ok(req) = self.publisher_rx.try_receive() {
                self.client.publish(req.topic, req.payload, req.qos).await?;
                continue;
            }

            // Calculate remaining time until tick
            let now = Instant::now();
            let remaining = if now >= tick_deadline {
                Duration::from_millis(0)
            } else {
                tick_deadline - now
            };

            // Select between poll and tick timer
            let timer_fut = Timer::after(remaining);
            let poll_fut = self.client.poll();

            // Track if we need immediate publish after message handling
            let mut needs_publish = false;

            match select(poll_fut, timer_fut).await {
                Either::First(result) => {
                    // Incoming MQTT message or keep-alive handled
                    match result {
                        Ok(Some(MqttEvent::Publish(msg))) => {
                            self.module.on_message(&msg);
                            needs_publish = self.module.needs_immediate_publish();
                        }
                        Ok(None) => {
                            // No message, keep-alive was sent, continue
                        }
                        Err(e) => return Err(e),
                    }
                }
                Either::Second(()) => {
                    // Tick timer expired - trigger publish
                    needs_publish = true;
                }
            }

            // If module needs to publish (either from tick or message response)
            if needs_publish {
                next_tick = self.module.on_tick(&mut outbox);
                self.drain_outbox(&mut outbox).await?;
            }
        }
    }

    /// Drain the outbox and publish all buffered messages.
    async fn drain_outbox(
        &mut self,
        outbox: &mut BufferedOutbox<OUTBOX_CAPACITY, OUTBOX_TOPIC_SIZE, OUTBOX_PAYLOAD_SIZE>,
    ) -> Result<(), MqttError<T::Error>> {
        for req in outbox.drain() {
            self.client
                .publish(req.topic.as_str(), req.payload.as_slice(), req.qos)
                .await?;
        }
        outbox.clear();
        Ok(())
    }

    /// Get a reference to the underlying module.
    pub fn module(&self) -> &M {
        &self.module
    }

    /// Get a mutable reference to the underlying module.
    pub fn module_mut(&mut self) -> &mut M {
        &mut self.module
    }
}

