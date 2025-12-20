//! MQTT Module trait and utilities.
//!
//! This module defines the object-safe `MqttModule` trait that allows building
//! reusable MQTT integrations (e.g. Home Assistant, telemetry, OTA updates).
//!
//! # Object Safety
//!
//! The `MqttModule` trait is designed to be dyn-compatible, meaning you can use
//! `&mut dyn MqttModule<MAX_TOPICS>` as a trait object. This is essential for
//! `no_std` embedded environments where you want to:
//!
//! - Store modules in `StaticCell` and pass them to Embassy tasks
//! - Avoid generic type parameters on task functions
//! - Decouple runtime infrastructure from module-specific logic
//!
//! # Publishing Pattern
//!
//! Modules never perform async I/O directly. Instead, they use the `PublishOutbox`
//! trait to queue publish requests. The runtime then performs the actual async
//! publishing after the module method returns.
//!
//! This separation keeps the trait object-safe while maintaining good performance.

use embassy_time::Duration;

use crate::packet::Publish;
use crate::packet::QoS;

/// Object-safe trait for queuing MQTT publish requests.
///
/// Modules use this to schedule publishes during `on_tick` and `on_start`.
/// The actual async publishing is done by the runtime after the module returns.
///
/// # Example
///
/// ```ignore
/// fn on_tick(&mut self, outbox: &mut dyn PublishOutbox) -> Duration {
///     outbox.publish("device/state", b"online", QoS::AtMostOnce);
///     Duration::from_secs(30)
/// }
/// ```
pub trait PublishOutbox {
    /// Queue a message for publishing.
    ///
    /// This is synchronous and returns immediately. The runtime will
    /// actually publish the message asynchronously after the module
    /// method returns.
    ///
    /// # Arguments
    ///
    /// - `topic`: The MQTT topic to publish to
    /// - `payload`: The message payload bytes
    /// - `qos`: Quality of Service level
    fn publish(&mut self, topic: &str, payload: &[u8], qos: QoS);
}

/// Object-safe trait for collecting topics during registration.
///
/// The runtime provides this to modules during `register()`. Modules
/// call `add()` for each topic they want to subscribe to.
pub trait TopicCollector {
    /// Add a topic to subscribe to.
    ///
    /// The topic string is copied internally, so it only needs to live
    /// for the duration of this call.
    ///
    /// Returns `true` if the topic was added, `false` if the collector is full.
    fn add(&mut self, topic: &str) -> bool;
}

/// Object-safe trait for MQTT modules that handle incoming messages and periodic tasks.
///
/// Implement this trait to create reusable MQTT integrations (e.g. Home Assistant,
/// telemetry, OTA updates). Modules are composed together and driven by the
/// `MqttRuntime`.
///
/// # Object Safety
///
/// This trait is designed to be dyn-compatible (`dyn MqttModule`),
/// allowing modules to be stored as trait objects in `no_std` environments
/// without heap allocation.
///
/// Key design choices for object safety:
/// - No `async fn` methods (all I/O is done via `PublishOutbox`)
/// - No generic type parameters or lifetimes on methods
/// - Transport-agnostic (modules don't know about TCP, UART, etc.)
/// - Callback-based topic registration (avoids lifetime issues)
///
/// # Example
///
/// ```ignore
/// const CMD_TOPIC: &str = "device/cmd";
/// const STATE_TOPIC: &str = "device/state";
///
/// struct MyModule {
///     pending_state_update: bool,
/// }
///
/// impl MqttModule for MyModule {
///     fn register(&self, collector: &mut dyn TopicCollector) {
///         collector.add(CMD_TOPIC);
///     }
///
///     fn on_message(&mut self, msg: &Publish<'_>) {
///         if msg.topic == CMD_TOPIC {
///             // Process command, set flag for response
///             self.pending_state_update = true;
///         }
///     }
///
///     fn on_tick(&mut self, outbox: &mut dyn PublishOutbox) -> Duration {
///         outbox.publish(STATE_TOPIC, b"online", QoS::AtMostOnce);
///         Duration::from_secs(30)
///     }
///
///     fn needs_immediate_publish(&self) -> bool {
///         self.pending_state_update
///     }
/// }
/// ```
pub trait MqttModule {
    /// Register topics that this module wants to subscribe to.
    ///
    /// Called once during runtime initialization. Add all command/input topics
    /// that this module needs to receive messages on.
    ///
    /// Topics are copied by the collector, so they only need to live for
    /// the duration of this call.
    fn register(&self, collector: &mut dyn TopicCollector);

    /// Handle an incoming MQTT message (synchronous processing only).
    ///
    /// Called for every incoming publish. The module should check if `msg.topic`
    /// matches one of its registered topics and handle accordingly.
    ///
    /// **Important**: This method cannot publish responses directly because the
    /// message data borrows from the client's buffer. Instead:
    /// - Process the message and update internal state
    /// - Set a flag indicating a response is needed
    /// - Publish the response in `on_tick` (triggered by `needs_immediate_publish`)
    fn on_message(&mut self, msg: &Publish<'_>);

    /// Perform periodic tasks and return the desired interval until the next tick.
    ///
    /// Called periodically by the runtime. Use this for:
    /// - Publishing state updates via `outbox`
    /// - Re-announcing discovery configs
    /// - Heartbeats or keep-alive logic
    /// - Sending responses to commands received in `on_message`
    ///
    /// The default implementation does nothing and returns a 60-second interval.
    fn on_tick(&mut self, _outbox: &mut dyn PublishOutbox) -> Duration {
        Duration::from_secs(60)
    }

    /// Called once after connection is established and subscriptions are done.
    ///
    /// Use this for initial announces, state publishing, etc.
    /// The default implementation does nothing.
    fn on_start(&mut self, _outbox: &mut dyn PublishOutbox) {}

    /// Check if the module needs to publish immediately after processing a message.
    ///
    /// If this returns `true`, `on_tick` will be called immediately after `on_message`.
    /// The default implementation returns `false`.
    fn needs_immediate_publish(&self) -> bool {
        false
    }
}

/// A no-op module that does nothing.
///
/// Useful as a placeholder or for testing.
pub struct NoopModule;

impl MqttModule for NoopModule {
    fn register(&self, _collector: &mut dyn TopicCollector) {}

    fn on_message(&mut self, _msg: &Publish<'_>) {}
}

/// A composite module that combines two modules into one.
///
/// Both modules receive all messages and ticks. Use this to compose
/// multiple independent modules into a single runtime.
///
/// # Example
///
/// ```ignore
/// let ha_module = HomeAssistantModule::new();
/// let telemetry_module = TelemetryModule::new();
/// let combined = ModulePair::new(ha_module, telemetry_module);
/// ```
pub struct ModulePair<M1, M2> {
    /// First module
    pub first: M1,
    /// Second module
    pub second: M2,
}

impl<M1, M2> ModulePair<M1, M2> {
    /// Create a new combined module from two modules.
    pub fn new(first: M1, second: M2) -> Self {
        Self { first, second }
    }
}

impl<M1, M2> MqttModule for ModulePair<M1, M2>
where
    M1: MqttModule,
    M2: MqttModule,
{
    fn register(&self, collector: &mut dyn TopicCollector) {
        self.first.register(collector);
        self.second.register(collector);
    }

    fn on_message(&mut self, msg: &Publish<'_>) {
        self.first.on_message(msg);
        self.second.on_message(msg);
    }

    fn on_tick(&mut self, outbox: &mut dyn PublishOutbox) -> Duration {
        let d1 = self.first.on_tick(outbox);
        let d2 = self.second.on_tick(outbox);
        // Return the smaller interval so both modules get ticked appropriately
        if d1 < d2 { d1 } else { d2 }
    }

    fn on_start(&mut self, outbox: &mut dyn PublishOutbox) {
        self.first.on_start(outbox);
        self.second.on_start(outbox);
    }

    fn needs_immediate_publish(&self) -> bool {
        self.first.needs_immediate_publish() || self.second.needs_immediate_publish()
    }
}

/// Blanket implementation for mutable references to trait objects.
///
/// This allows using `&mut dyn MqttModule` wherever `MqttModule` is expected.
impl<M: MqttModule + ?Sized> MqttModule for &mut M {
    fn register(&self, collector: &mut dyn TopicCollector) {
        (**self).register(collector)
    }

    fn on_message(&mut self, msg: &Publish<'_>) {
        (**self).on_message(msg)
    }

    fn on_tick(&mut self, outbox: &mut dyn PublishOutbox) -> Duration {
        (**self).on_tick(outbox)
    }

    fn on_start(&mut self, outbox: &mut dyn PublishOutbox) {
        (**self).on_start(outbox)
    }

    fn needs_immediate_publish(&self) -> bool {
        (**self).needs_immediate_publish()
    }
}
