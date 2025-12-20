//! # MQTT Transport Abstraction
//!
//! This module defines the `MqttTransport` trait, which abstracts the underlying
//! communication channel (like TCP, UART, etc.), allowing the MQTT client to be
//! hardware and network-stack agnostic.
//!
//! With the Rust 2024 Edition, this trait uses native `async fn`, removing the
//! need for the `#[async_trait]` macro.

use crate::error::MqttError;
use embassy_net::tcp::{Error as TcpError, TcpSocket};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;

/// A placeholder error type used in contexts where the actual transport error is not known,
/// such as in the `EncodePacket` trait.
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ErrorPlaceHolder;

/// A trait representing a transport for MQTT packets.
#[allow(async_fn_in_trait)]
pub trait MqttTransport {
    /// The error type returned by the transport.
    type Error: core::fmt::Debug;

    /// Sends a buffer of data over the transport.
    async fn send(&mut self, buf: &[u8]) -> Result<(), Self::Error>;

    /// Receives data from the transport into a buffer.
    ///
    /// Returns the number of bytes read.
    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error>;
}

// Allow the placeholder to be treated as a transport error for generic contexts.
impl TransportError for ErrorPlaceHolder {}

/// A marker trait for transport-related errors.
pub trait TransportError: core::fmt::Debug {}

// Implement TransportError for MqttError so TcpTransport works with client methods
impl<T: core::fmt::Debug> TransportError for MqttError<T> {}

// Implement TransportError for embassy_net tcp error
impl TransportError for TcpError {}

/// TCP transport implementation using `embassy-net`.
pub struct TcpTransport<'a> {
    socket: TcpSocket<'a>,
    timeout: Duration,
}

impl<'a> TcpTransport<'a> {
    /// Creates a new `TcpTransport` with the given socket and timeout.
    pub fn new(socket: TcpSocket<'a>, timeout: Duration) -> Self {
        Self { socket, timeout }
    }

    /// A helper function to perform a read with a timeout.
    async fn read_with_timeout(
        &mut self,
        buf: &mut [u8],
    ) -> Result<usize, MqttError<embassy_net::tcp::Error>> {
        // Use `select` to race the read operation against a timer.
        let read_fut = self.socket.read(buf);
        let timer = Timer::after(self.timeout);

        match futures::future::select(core::pin::pin!(read_fut), core::pin::pin!(timer)).await {
            futures::future::Either::Left((Ok(n), _)) => {
                #[cfg(feature = "esp-println")]
                esp_println::println!("TCP read: {} bytes", n);

                if n == 0 {
                    // If the peer closes the connection, read returns 0.
                    #[cfg(feature = "esp-println")]
                    esp_println::println!("TCP connection closed by peer!");

                    Err(MqttError::Protocol(
                        super::error::ProtocolError::ConnectionClosed,
                    ))
                } else {
                    Ok(n)
                }
            }
            futures::future::Either::Left((Err(e), _)) => {
                #[cfg(feature = "esp-println")]
                esp_println::println!("TCP read error: {:?}", e);

                Err(MqttError::Transport(e))
            }
            futures::future::Either::Right(((), _)) => {
                #[cfg(feature = "esp-println")]
                esp_println::println!("TCP read timeout!");

                Err(MqttError::Timeout)
            }
        }
    }
}

impl<'a> MqttTransport for TcpTransport<'a> {
    type Error = MqttError<embassy_net::tcp::Error>;

    async fn send(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        #[cfg(feature = "esp-println")]
        esp_println::println!("TCP TX ({} bytes): {:02X?}", buf.len(), buf);

        self.socket.write_all(buf).await.map_err(|e| {
            #[cfg(feature = "esp-println")]
            esp_println::println!("TCP write error: {:?}", e);
            MqttError::Transport(e)
        })?;

        // Flush to ensure data is actually sent to the network
        self.socket.flush().await.map_err(MqttError::Transport)
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.read_with_timeout(buf).await
    }
}
