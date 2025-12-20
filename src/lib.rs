//! # Async MQTT Client for Embedded Systems
//!
//! `myrtio-mqtt` is a `no_std` compatible, asynchronous MQTT client designed for embedded
//! systems, built upon the [Embassy](https://embassy.dev/) async ecosystem.
//!
//! ## Core Features
//!
//! - **`no_std` & `no_alloc`:** Designed to run on bare-metal microcontrollers without requiring a
//!   standard library or dynamic memory allocation. Buffers are managed using `heapless`.
//! - **Fully Async:** Built with `async/await` and leverages the Embassy ecosystem for timers
//!   and networking, ensuring non-blocking operations.
//! - **Rust 2024 Edition:** Uses native `async fn` in traits, removing the need for `async-trait`.
//! - **MQTT v3.1.1 and v5 Support:** Supports both major versions of the MQTT protocol, selectable
//!   via feature flags.
//! - **Transport Agnostic:** A flexible `MqttTransport` trait allows the client to run over any
//!   reliable, ordered, stream-based communication channel, including TCP, UART, or SPI.
//! - **QoS 0 & 1:** Implements "at most once" and "at least once" delivery guarantees.
//!
//! ## Architecture
//!
//! The crate provides two ways to use MQTT:
//!
//! ### 1. Direct Client Usage
//!
//! Use `MqttClient` directly for simple applications:
//!
//! ```ignore
//! let mut client = MqttClient::<_, 5, 256>::new(transport, options);
//! client.connect().await?;
//! client.subscribe("topic", QoS::AtMostOnce).await?;
//! client.publish("topic", b"payload", QoS::AtMostOnce).await?;
//! ```
//!
//! ### 2. Runtime with Modules
//!
//! Use `MqttRuntime` with `MqttModule` trait for complex applications with
//! multiple concerns (Home Assistant, telemetry, OTA, etc.):
//!
//! ```ignore
//! use myrtio_mqtt::runtime::{MqttModule, MqttRuntime, TopicRegistry, Context};
//!
//! struct MyModule;
//!
//! impl<'a, T, const MAX_TOPICS: usize, const BUF_SIZE: usize>
//!     MqttModule<'a, T, MAX_TOPICS, BUF_SIZE> for MyModule
//! where
//!     T: MqttTransport,
//! {
//!     fn register<'reg>(&'reg self, registry: &mut TopicRegistry<'reg, MAX_TOPICS>) {
//!         let _ = registry.add("device/cmd");
//!     }
//!
//!     fn on_message<'m>(&mut self, msg: &Publish<'m>) {
//!         // Handle incoming messages
//!     }
//! }
//! ```
//!
//! ## Topic Registration Lifetime Model
//!
//! The `MqttModule::register` method uses a generic lifetime `'reg` tied to the
//! borrow of `self`. This allows modules to register:
//!
//! - **Static topics**: `const CMD_TOPIC: &str = "device/cmd";` (recommended)
//! - **Dynamic topics**: Topics stored in `heapless::String` fields
//!
//! The registry is only used during initial subscription and is dropped before
//! the main event loop, so topics only need to live as long as the module.
//!
//! See `examples/const_topics_module.rs` and `examples/dynamic_topics_module.rs`
//! for complete examples.

#![no_std]
pub mod client;
pub mod error;
pub mod packet;
pub mod runtime;
pub mod transport;
pub mod util;

// Re-export key types for easier access at the crate root.
pub use client::{MqttClient, MqttEvent, MqttOptions};
pub use packet::QoS;
pub use transport::TcpTransport;
