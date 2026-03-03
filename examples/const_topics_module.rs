//! Example: Building an MQTT module with constant topics
//!
//! This example demonstrates how to implement the `MqttModule` trait
//! using static `&'static str` topics. This is the recommended approach
//! for embedded systems where topics are known at compile time.
//!
//! # Key Concepts
//!
//! - Define topics as `const` or `static` strings
//! - Implement `register()` to add topics to the registry
//! - Implement `on_message()` to handle incoming messages
//! - Use `on_tick()` for periodic state publishing
//!
//! # Note
//!
//! This example is for illustration purposes and won't compile as a
//! standalone binary without a proper transport implementation.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use embassy_time::Duration;
use myrtio_mqtt::{
    packet::Publish,
    runtime::{Context, MqttModule, TopicRegistry},
    transport::{MqttTransport, TransportError},
};

// Define topics as constants - these have 'static lifetime
const CMD_TOPIC: &str = "device/light/cmd";
const STATE_TOPIC: &str = "device/light/state";
const BRIGHTNESS_CMD_TOPIC: &str = "device/light/brightness/cmd";

/// Static state storage (common pattern in embedded)
static LIGHT_ON: AtomicBool = AtomicBool::new(false);
static BRIGHTNESS: AtomicU8 = AtomicU8::new(255);

/// A simple light controller module using constant topics.
///
/// This module handles:
/// - On/Off commands via `device/light/cmd`
/// - Brightness commands via `device/light/brightness/cmd`
/// - State publishing via `device/light/state`
pub struct LightModule {
    /// Flag indicating state changed and needs publishing
    needs_publish: bool,
}

impl LightModule {
    /// Create a new light module
    pub const fn new() -> Self {
        Self {
            needs_publish: false,
        }
    }

    /// Get current light state as JSON bytes
    fn get_state_payload(&self, buf: &mut [u8]) -> usize {
        let on = LIGHT_ON.load(Ordering::Relaxed);
        let brightness = BRIGHTNESS.load(Ordering::Relaxed);

        // Simple JSON formatting without serde
        let state = if on { "ON" } else { "OFF" };
        let len = format_state(state, brightness, buf);
        len
    }
}

impl<'a, T, const MAX_TOPICS: usize, const BUF_SIZE: usize> MqttModule<'a, T, MAX_TOPICS, BUF_SIZE>
    for LightModule
where
    T: MqttTransport,
{
    /// Register topics for subscription.
    ///
    /// The `'reg` lifetime is tied to `&self`, but since our topics are
    /// `&'static str` constants, they satisfy any lifetime requirement.
    fn register<'reg>(&'reg self, registry: &mut TopicRegistry<'reg, MAX_TOPICS>) {
        // Register command topics - these are 'static so always valid
        let _ = registry.add(CMD_TOPIC);
        let _ = registry.add(BRIGHTNESS_CMD_TOPIC);
    }

    /// Handle incoming messages.
    fn on_message<'m>(&mut self, msg: &Publish<'m>) {
        match msg.topic {
            CMD_TOPIC => {
                // Handle on/off command
                if msg.payload == b"ON" {
                    LIGHT_ON.store(true, Ordering::Relaxed);
                    self.needs_publish = true;
                } else if msg.payload == b"OFF" {
                    LIGHT_ON.store(false, Ordering::Relaxed);
                    self.needs_publish = true;
                }
            }
            BRIGHTNESS_CMD_TOPIC => {
                // Handle brightness command (expects decimal string)
                if let Ok(s) = core::str::from_utf8(msg.payload) {
                    if let Ok(val) = s.trim().parse::<u8>() {
                        BRIGHTNESS.store(val, Ordering::Relaxed);
                        self.needs_publish = true;
                    }
                }
            }
            _ => {}
        }
    }

    /// Periodic task: publish current state
    async fn on_tick(&mut self, ctx: &mut Context<'a, '_, T, MAX_TOPICS, BUF_SIZE>) -> Duration
    where
        T::Error: TransportError,
    {
        self.needs_publish = false;

        // Publish state
        let mut buf = [0u8; 64];
        let len = self.get_state_payload(&mut buf);
        let _ = ctx
            .publish(STATE_TOPIC, &buf[..len], myrtio_mqtt::QoS::AtMostOnce)
            .await;

        // Re-publish state every 30 seconds
        Duration::from_secs(30)
    }

    fn needs_immediate_publish(&self) -> bool {
        self.needs_publish
    }
}

/// Format state as simple JSON: {"state":"ON","brightness":255}
fn format_state(state: &str, brightness: u8, buf: &mut [u8]) -> usize {
    use core::fmt::Write;

    struct BufWriter<'a> {
        buf: &'a mut [u8],
        pos: usize,
    }

    impl Write for BufWriter<'_> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let bytes = s.as_bytes();
            if self.pos + bytes.len() > self.buf.len() {
                return Err(core::fmt::Error);
            }
            self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
            self.pos += bytes.len();
            Ok(())
        }
    }

    let mut w = BufWriter { buf, pos: 0 };
    let _ = write!(
        w,
        "{{\"state\":\"{}\",\"brightness\":{}}}",
        state, brightness
    );
    w.pos
}

// Placeholder main - actual implementation would use embassy executor
#[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
fn main() {
    // This example is for documentation purposes.
    // See the firmware crates for real usage examples.
}

#[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
