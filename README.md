# myrtio-mqtt

An async, `no_std`, `no_alloc` MQTT client for embedded systems, built on the [Embassy](https://embassy.dev/) ecosystem.

## Features

- **Async & `no_std`**: Designed for bare-metal microcontrollers (ESP32, etc.) and asynchronous execution.
- **No Allocator Required**: Uses `heapless` for fixed-size buffers and internal state management.
- **Transport Agnostic**: Works over TCP via `embassy-net`, UART, or any reliable stream-based channel via the `MqttTransport` trait.
- **MQTT v3.1.1 & v5**: Core support for v3.1.1 with optional v5 support via feature flags.
- **Modular Runtime**: High-level `MqttRuntime` for building applications using object-safe `MqttModule`s.

## Crate Requirements

- **Rust Edition 2024**: Uses native `async fn` in traits.
- **Rust Version**: 1.90 or newer.

## Usage Styles

### 1. Direct Client Usage
Use `MqttClient` directly for simple, low-level control.

```rust
use myrtio_mqtt::{MqttClient, MqttOptions, MqttEvent, QoS, TcpTransport};
use embassy_time::Duration;

let transport = TcpTransport::new(socket, Duration::from_secs(5));
let options = MqttOptions::new("my-device-id");
// Client with space for 8 subscriptions and 1024-byte buffers
let mut client = MqttClient::<_, 8, 1024>::new(transport, options);

client.connect().await?;
client.subscribe("sensors/data", QoS::AtMostOnce).await?;

loop {
    // Poll handles keep-alives and incoming packets
    if let Ok(Some(MqttEvent::Publish(msg))) = client.poll().await {
        // Handle incoming message
        // msg.payload borrows from client.rx_buffer
    }
}
```

### 2. Runtime & Modules
Use `MqttRuntime` to compose multiple independent features (Home Assistant, Telemetry, etc.) into one event loop.

```rust
use myrtio_mqtt::runtime::{MqttModule, TopicCollector, PublishOutbox};
use myrtio_mqtt::packet::Publish;
use embassy_time::Duration;

struct TelemetryModule;

impl MqttModule for TelemetryModule {
    fn register(&self, collector: &mut dyn TopicCollector) {
        // Register topics for subscription
        collector.add("device/commands");
    }

    fn on_message(&mut self, msg: &Publish<'_>) {
        // Process incoming messages synchronously
    }

    fn on_tick(&mut self, outbox: &mut dyn PublishOutbox) -> Duration {
        // Periodic tasks: publish state
        outbox.publish("device/telemetry", b"{\"temp\": 22.5}", QoS::AtMostOnce);
        Duration::from_secs(30)
    }
}
```

## Key Concepts

- **Borrowed Payloads**: For efficiency, incoming `Publish` messages borrow their topic and payload directly from the client's internal receive buffer. They are only valid until the next call to `poll()` or until the module's `on_message` returns.
- **Object-Safe Design**: The `MqttModule` trait is object-safe (`dyn MqttModule`), allowing you to store modules in `StaticCell`s or compose them using `ModulePair` without complex generic parameters.
- **Outbox Pattern**: To keep modules object-safe and synchronous, they do not perform async I/O. Instead, they queue publish requests into a `PublishOutbox`. The `MqttRuntime` performs the actual async publishing after the module callback completes.

### Quick API Reference

| Module | Key Types |
|--------|-----------|
| **Root** | `MqttClient`, `MqttOptions`, `MqttEvent`, `QoS` |
| `transport` | `MqttTransport`, `TcpTransport` |
| `runtime` | `MqttRuntime`, `MqttModule`, `TopicCollector`, `PublishOutbox`, `PublisherHandle` |
