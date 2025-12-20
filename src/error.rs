//! # Error Types
//!
//! This module defines the error types used throughout the MQTT client library,
//! providing detailed information about potential failures, from transport issues
//! to protocol violations.

use crate::transport;

/// A placeholder error type used in generic contexts where the specific transport
/// error is not yet known. This is a common pattern for implementing `encode` methods
/// that need to return a `Result` compatible with the client's error type.
#[derive(Debug)]
pub struct ErrorPlaceHolder;

impl transport::TransportError for ErrorPlaceHolder {
    // This is a marker implementation and doesn't need a body.
}

/// The primary error enum for the MQTT client.
///
/// It is generic over the transport error type `T`, allowing it to wrap
/// specific errors from the underlying network transport (e.g., TCP, UART).
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MqttError<T> {
    /// An error occurred in the underlying transport layer.
    Transport(T),
    /// A protocol-level error occurred, indicating a violation of the MQTT specification.
    Protocol(ProtocolError),
    /// The connection was refused by the broker. The enclosed code provides the reason.
    ConnectionRefused(ConnectReasonCode),
    /// The client is not currently connected to the broker.
    NotConnected,
    /// The buffer provided for an operation was too small.
    BufferTooSmall,
    /// An operation timed out.
    Timeout,
}

/// Implements the `From` trait to allow for automatic conversion of any transport
/// error into an `MqttError`. This is what allows the `?` operator to work
/// seamlessly on `Result`s from the transport layer.
impl<T: transport::TransportError> From<T> for MqttError<T> {
    fn from(err: T) -> Self {
        MqttError::Transport(err)
    }
}

impl<T: transport::TransportError> MqttError<T> {
    /// A helper method to convert an `MqttError` with a placeholder transport error
    /// into an `MqttError` with a specific transport error type `T`.
    ///
    /// This is used to bridge the gap between generic packet encoding functions
    /// and the specific error type required by the client's `Result`.
    pub fn cast_transport_error<E: transport::TransportError>(other: MqttError<E>) -> MqttError<T> {
        match other {
            MqttError::Protocol(p) => MqttError::Protocol(p),
            MqttError::ConnectionRefused(c) => MqttError::ConnectionRefused(c),
            MqttError::NotConnected => MqttError::NotConnected,
            MqttError::BufferTooSmall => MqttError::BufferTooSmall,
            MqttError::Timeout => MqttError::Timeout,
            // The transport variant can't be cast, as we don't know the concrete type `E`.
            // This method is designed for errors originating from packet logic, which
            // should not produce transport errors directly.
            MqttError::Transport(_) => panic!("Cannot cast a transport error"),
        }
    }
}

/// Represents the reason codes for a connection refusal (`CONNACK`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum ConnectReasonCode {
    /// The connection was accepted.
    Success = 0,
    /// The broker does not support the requested MQTT protocol version.
    UnacceptableProtocolVersion = 1,
    /// The client identifier is not valid.
    IdentifierRejected = 2,
    /// The broker is unavailable.
    ServerUnavailable = 3,
    /// The username or password is not valid.
    BadUserNameOrPassword = 4,
    /// The client is not authorized to connect.
    NotAuthorized = 5,
    /// An unknown or unspecified error occurred.
    Other(u8),
}

impl From<u8> for ConnectReasonCode {
    fn from(val: u8) -> Self {
        match val {
            0 => Self::Success,
            1 => Self::UnacceptableProtocolVersion,
            2 => Self::IdentifierRejected,
            3 => Self::ServerUnavailable,
            4 => Self::BadUserNameOrPassword,
            5 => Self::NotAuthorized,
            _ => Self::Other(val),
        }
    }
}

/// Enumerates specific MQTT protocol errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ProtocolError {
    /// An invalid packet type was received.
    InvalidPacketType(u8),
    /// The server sent an invalid or unexpected response.
    InvalidResponse,
    /// The connection was closed by the broker.
    ConnectionClosed,
    /// A packet was received that was not correctly formed.
    MalformedPacket,
    /// The payload of a message exceeds the maximum allowable size.
    PayloadTooLarge,
    /// A string was not valid UTF-8.
    InvalidUtf8String,
    /// An MQTT v5 packet contained too many properties.
    #[cfg(feature = "v5")]
    TooManyProperties,
}
