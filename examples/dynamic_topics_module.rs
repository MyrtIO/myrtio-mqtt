//! Example: Building an MQTT module with dynamic topics
//!
//! This example demonstrates how to implement the `MqttModule` trait
//! using topics stored as fields in the module struct. This is useful
//! when topics are constructed at runtime based on device configuration.
//!
//! # Key Concepts
//!
//! - Store topics in `heapless::String` fields
//! - The `register<'reg>(&'reg self, ...)` lifetime allows borrowing from `self`
//! - Topics are valid as long as the module lives
//!
//! # When to Use Dynamic Topics
//!
//! - Device ID is read from flash/EEPROM at startup
//! - Topics include configurable prefixes
//! - Multiple instances of the same module with different topics
//!
//! # Note
//!
//! This example is for illustration purposes and won't compile as a
//! standalone binary without a proper transport implementation.

#![no_std]
#![no_main]

use embassy_time::Duration;
use heapless::String;
use myrtio_mqtt::{
    packet::Publish,
    runtime::{Context, MqttModule, TopicRegistry},
    transport::{MqttTransport, TransportError},
};

/// Maximum length for topic strings
const MAX_TOPIC_LEN: usize = 64;

/// A sensor module with configurable device ID in topics.
///
/// Topics are constructed at creation time and stored in the struct.
/// The new `register<'reg>` lifetime model allows these to be registered
/// because they live as long as the module itself.
pub struct SensorModule {
    /// Command topic: `{device_id}/sensor/cmd`
    cmd_topic: String<MAX_TOPIC_LEN>,
    /// State topic: `{device_id}/sensor/state`
    state_topic: String<MAX_TOPIC_LEN>,
    /// Current sensor value
    value: i32,
    /// Flag for state change
    needs_publish: bool,
}

impl SensorModule {
    /// Create a new sensor module with the given device ID.
    ///
    /// Topics are constructed as:
    /// - Command: `{device_id}/sensor/cmd`
    /// - State: `{device_id}/sensor/state`
    pub fn new(device_id: &str) -> Self {
        let mut cmd_topic = String::new();
        let _ = cmd_topic.push_str(device_id);
        let _ = cmd_topic.push_str("/sensor/cmd");

        let mut state_topic = String::new();
        let _ = state_topic.push_str(device_id);
        let _ = state_topic.push_str("/sensor/state");

        Self {
            cmd_topic,
            state_topic,
            value: 0,
            needs_publish: false,
        }
    }

    /// Update the sensor value (called from hardware driver)
    pub fn set_value(&mut self, value: i32) {
        if self.value != value {
            self.value = value;
            self.needs_publish = true;
        }
    }
}

impl<'a, T, const MAX_TOPICS: usize, const BUF_SIZE: usize> MqttModule<'a, T, MAX_TOPICS, BUF_SIZE>
    for SensorModule
where
    T: MqttTransport,
{
    /// Register topics for subscription.
    ///
    /// The `'reg` lifetime is tied to `&'reg self`, which means we can
    /// borrow from our fields. The topics in `self.cmd_topic` and
    /// `self.state_topic` live as long as `self`, so they can be
    /// registered in the registry.
    fn register<'reg>(&'reg self, registry: &mut TopicRegistry<'reg, MAX_TOPICS>) {
        // Register our command topic - borrowed from self
        let _ = registry.add(self.cmd_topic.as_str());
    }

    /// Handle incoming messages.
    fn on_message<'m>(&mut self, msg: &Publish<'m>) {
        // Check if this message is for our command topic
        if msg.topic == self.cmd_topic.as_str() {
            // Parse command (e.g., "SET:123" to set calibration offset)
            if let Ok(s) = core::str::from_utf8(msg.payload) {
                if let Some(val_str) = s.strip_prefix("SET:") {
                    if let Ok(val) = val_str.trim().parse::<i32>() {
                        self.value = val;
                        self.needs_publish = true;
                    }
                }
            }
        }
    }

    /// Periodic task: publish current state
    async fn on_tick(&mut self, ctx: &mut Context<'a, '_, T, MAX_TOPICS, BUF_SIZE>) -> Duration
    where
        T::Error: TransportError,
    {
        self.needs_publish = false;

        // Format value as string
        let mut buf = [0u8; 16];
        let len = format_i32(self.value, &mut buf);

        // Publish to our state topic
        let _ = ctx
            .publish(
                self.state_topic.as_str(),
                &buf[..len],
                myrtio_mqtt::QoS::AtMostOnce,
            )
            .await;

        // Re-publish every 60 seconds
        Duration::from_secs(60)
    }

    fn needs_immediate_publish(&self) -> bool {
        self.needs_publish
    }
}

/// Format i32 to bytes
fn format_i32(value: i32, buf: &mut [u8]) -> usize {
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
    let _ = write!(w, "{}", value);
    w.pos
}

// Placeholder main - actual implementation would use embassy executor
#[cfg(not(any(target_arch = "xtensa", target_arch = "riscv32")))]
fn main() {
    // This example is for documentation purposes.
    // See the firmware crates for real usage examples.
    //
    // Usage would look like:
    //
    // ```
    // // Read device ID from flash
    // let device_id = read_device_id_from_flash();
    //
    // // Create module with dynamic topics
    // let sensor = SensorModule::new(&device_id);
    //
    // // Create runtime and run
    // let mut runtime = MqttRuntime::new(client, sensor, publisher_rx);
    // runtime.run().await;
    // ```
}

#[cfg(any(target_arch = "xtensa", target_arch = "riscv32"))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}
