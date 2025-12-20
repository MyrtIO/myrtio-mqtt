//! MQTT Runtime Module
//!
//! Provides a runtime abstraction for building modular MQTT applications.
//!
//! # Overview
//!
//! The runtime system allows you to create reusable MQTT modules that handle:
//! - Topic registration and subscription
//! - Incoming message handling
//! - Periodic tasks (state publishing, discovery, etc.)
//!
//! # Object-Safe Design
//!
//! The `MqttModule` trait is designed to be dyn-compatible, allowing you to use
//! `&mut dyn MqttModule` as a trait object. This is essential for `no_std`
//! embedded environments where you want to:
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
//! # Example
//!
//! See `examples/const_topics_module.rs` for a complete example of building
//! a module with constant topics.

pub(crate) mod event_loop;
pub(crate) mod publisher;
pub(crate) mod registry;
pub(crate) mod traits;

pub use event_loop::MqttRuntime;
pub use publisher::{
    BufferedOutbox, OwnedPublishRequest, PublishRequest, PublishRequestChannel,
    PublishRequestReceiver, PublishRequestSender, PublisherHandle,
};
pub use registry::TopicRegistry;
pub use traits::{ModulePair, MqttModule, NoopModule, PublishOutbox, TopicCollector};

// Re-export Publish for convenient use in modules
pub use crate::packet::Publish;
