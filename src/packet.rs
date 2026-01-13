//! # MQTT Packet Structures and Serialization
//!
//! This module defines the core MQTT packet types and the traits for encoding and
//! decoding them to and from a byte buffer. It supports both MQTT v3.1.1 and v5
//! through conditional compilation.

use crate::client::MqttVersion;
use crate::error::{MqttError, ProtocolError};
use crate::transport;
use crate::util::{self, read_utf8_string, write_utf8_string};
use core::marker::PhantomData;
use heapless::Vec;

// Conditionally import v5-specific utilities only when the feature is enabled.
#[cfg(feature = "v5")]
use crate::util::{read_properties, write_properties};

/// Represents the Quality of Service (QoS) levels for MQTT messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u8)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

/// A trait for packets that can be encoded into a byte buffer.
pub trait EncodePacket {
    fn encode(
        &self,
        buf: &mut [u8],
        version: MqttVersion,
    ) -> Result<usize, MqttError<transport::ErrorPlaceHolder>>;
}

/// A trait for packets that can be decoded from a byte buffer.
pub trait DecodePacket<'a>: Sized {
    fn decode(
        buf: &'a [u8],
        version: MqttVersion,
    ) -> Result<Self, MqttError<transport::ErrorPlaceHolder>>;
}

/// An enumeration of all possible MQTT control packets.
#[derive(Debug)]
pub enum MqttPacket<'a> {
    Connect(Connect<'a>),
    ConnAck(ConnAck<'a>),
    Publish(Publish<'a>),
    PubAck(PubAck<'a>),
    Subscribe(Subscribe<'a>),
    SubAck(SubAck<'a>),
    PingReq,
    PingResp,
    Disconnect(Disconnect<'a>),
}

/// Decodes a raw byte buffer into a specific `MqttPacket`.
pub fn decode<'a, T>(
    buf: &'a [u8],
    version: MqttVersion,
) -> Result<Option<MqttPacket<'a>>, MqttError<T>>
where
    T: transport::TransportError,
{
    if buf.is_empty() {
        return Ok(None);
    }

    let packet_type = buf[0] >> 4;
    let packet = match packet_type {
        1 => MqttPacket::Connect(
            Connect::decode(buf, version).map_err(MqttError::cast_transport_error)?,
        ),
        2 => MqttPacket::ConnAck(
            ConnAck::decode(buf, version).map_err(MqttError::cast_transport_error)?,
        ),
        3 => MqttPacket::Publish(
            Publish::decode(buf, version).map_err(MqttError::cast_transport_error)?,
        ),
        4 => MqttPacket::PubAck(
            PubAck::decode(buf, version).map_err(MqttError::cast_transport_error)?,
        ),
        8 => MqttPacket::Subscribe(
            Subscribe::decode(buf, version).map_err(MqttError::cast_transport_error)?,
        ),
        9 => MqttPacket::SubAck(
            SubAck::decode(buf, version).map_err(MqttError::cast_transport_error)?,
        ),
        12 => MqttPacket::PingReq,
        13 => MqttPacket::PingResp,
        14 => MqttPacket::Disconnect(
            Disconnect::decode(buf, version).map_err(MqttError::cast_transport_error)?,
        ),
        _ => {
            return Err(MqttError::Protocol(ProtocolError::InvalidPacketType(
                packet_type,
            )));
        }
    };

    Ok(Some(packet))
}

#[cfg(feature = "v5")]
#[derive(Debug)]
pub struct Property<'a> {
    pub id: u8,
    pub data: &'a [u8],
}

// --- CONNECT Packet ---
#[derive(Debug)]
pub struct Connect<'a> {
    pub clean_session: bool,
    pub keep_alive: u16,
    pub client_id: &'a str,
    pub username: Option<&'a str>,
    pub password: Option<&'a [u8]>,
    #[cfg(feature = "v5")]
    pub properties: Vec<Property<'a>, 8>,
}

impl<'a> Connect<'a> {
    pub fn new(client_id: &'a str, keep_alive: u16, clean_session: bool) -> Self {
        Self {
            client_id,
            keep_alive,
            clean_session,
            username: None,
            password: None,
            #[cfg(feature = "v5")]
            properties: Vec::new(),
        }
    }

    /// Creates a new Connect packet with authentication credentials.
    pub fn with_credentials(
        client_id: &'a str,
        keep_alive: u16,
        clean_session: bool,
        username: Option<&'a str>,
        password: Option<&'a [u8]>,
    ) -> Self {
        Self {
            client_id,
            keep_alive,
            clean_session,
            username,
            password,
            #[cfg(feature = "v5")]
            properties: Vec::new(),
        }
    }
}

impl<'a> EncodePacket for Connect<'a> {
    fn encode(
        &self,
        buf: &mut [u8],
        version: MqttVersion,
    ) -> Result<usize, MqttError<transport::ErrorPlaceHolder>> {
        let mut cursor = 0;
        buf[cursor] = 0x10;
        cursor += 1;
        let remaining_len_pos = cursor;
        cursor += 4;
        let content_start = cursor;
        // Protocol name is "MQTT" for both v3.1.1 and v5
        cursor += write_utf8_string(&mut buf[cursor..], "MQTT")?;
        // Protocol level: 4 for MQTT 3.1.1, 5 for MQTT 5.0
        buf[cursor] = if version == MqttVersion::V5 { 5 } else { 4 };
        cursor += 1;

        // Build connect flags
        let mut flags = 0u8;
        if self.clean_session {
            flags |= 0x02; // Clean Session flag (bit 1)
        }
        if self.username.is_some() {
            flags |= 0x80; // Username flag (bit 7)
        }
        if self.password.is_some() {
            flags |= 0x40; // Password flag (bit 6)
        }
        buf[cursor] = flags;
        let flags_pos = cursor; // Save position to update flags later if needed
        let _ = flags_pos; // Suppress unused warning
        cursor += 1;

        buf[cursor..cursor + 2].copy_from_slice(&self.keep_alive.to_be_bytes());
        cursor += 2;
        #[cfg(feature = "v5")]
        if version == MqttVersion::V5 {
            write_properties(&mut cursor, buf, &self.properties)?;
        }

        // Payload: Client ID
        cursor += write_utf8_string(&mut buf[cursor..], self.client_id)?;

        // Payload: Username (if present)
        if let Some(username) = self.username {
            cursor += write_utf8_string(&mut buf[cursor..], username)?;
        }

        // Payload: Password (if present) - written as binary data (2-byte length + data)
        if let Some(password) = self.password {
            let len = password.len();
            if len > u16::MAX as usize {
                return Err(MqttError::Protocol(ProtocolError::PayloadTooLarge));
            }
            if cursor + 2 + len > buf.len() {
                return Err(MqttError::BufferTooSmall);
            }
            buf[cursor..cursor + 2].copy_from_slice(&(len as u16).to_be_bytes());
            cursor += 2;
            buf[cursor..cursor + len].copy_from_slice(password);
            cursor += len;
        }

        let remaining_len = cursor - content_start;
        let len_bytes =
            util::write_variable_byte_integer_len(&mut buf[remaining_len_pos..], remaining_len)?;
        let header_len = 1 + len_bytes;
        buf.copy_within(content_start..cursor, header_len);
        Ok(header_len + remaining_len)
    }
}
impl<'a> DecodePacket<'a> for Connect<'a> {
    fn decode(
        buf: &'a [u8],
        _version: MqttVersion,
    ) -> Result<Self, MqttError<transport::ErrorPlaceHolder>> {
        let mut cursor = 2;
        cursor += 6;
        let connect_flags = buf[cursor];
        let clean_session = (connect_flags & 0x02) != 0;
        let has_username = (connect_flags & 0x80) != 0;
        let has_password = (connect_flags & 0x40) != 0;
        cursor += 1;
        let keep_alive = u16::from_be_bytes([buf[cursor], buf[cursor + 1]]);
        cursor += 2;
        #[cfg(feature = "v5")]
        let properties = if _version == MqttVersion::V5 {
            read_properties(&mut cursor, buf)?
        } else {
            Vec::new()
        };
        let client_id = read_utf8_string(&mut cursor, buf)?;
        // Note: Will topic/message parsing would go here if supported
        let username = if has_username {
            Some(read_utf8_string(&mut cursor, buf)?)
        } else {
            None
        };
        let password = if has_password {
            let len = u16::from_be_bytes([buf[cursor], buf[cursor + 1]]) as usize;
            cursor += 2;
            let pwd = &buf[cursor..cursor + len];
            cursor += len;
            Some(pwd)
        } else {
            None
        };
        Ok(Self {
            clean_session,
            keep_alive,
            client_id,
            username,
            password,
            #[cfg(feature = "v5")]
            properties,
        })
    }
}

// --- CONNACK Packet ---
#[derive(Debug)]
pub struct ConnAck<'a> {
    pub session_present: bool,
    pub reason_code: u8,
    #[cfg(feature = "v5")]
    pub properties: Vec<Property<'a>, 8>,
    #[cfg(not(feature = "v5"))]
    _phantom: PhantomData<&'a ()>,
}
impl<'a> DecodePacket<'a> for ConnAck<'a> {
    fn decode(
        buf: &'a [u8],
        _version: MqttVersion,
    ) -> Result<Self, MqttError<transport::ErrorPlaceHolder>> {
        let mut cursor = 2;
        let session_present = (buf[cursor] & 0x01) != 0;
        cursor += 1;
        let reason_code = buf[cursor];
        #[cfg(feature = "v5")]
        let properties = if version == MqttVersion::V5 {
            cursor += 1;
            read_properties(&mut cursor, buf)?
        } else {
            Vec::new()
        };
        Ok(Self {
            session_present,
            reason_code,
            #[cfg(feature = "v5")]
            properties,
            #[cfg(not(feature = "v5"))]
            _phantom: PhantomData,
        })
    }
}

// --- PUBLISH Packet ---
#[derive(Debug)]
pub struct Publish<'a> {
    pub topic: &'a str,
    pub qos: QoS,
    /// MQTT retain flag. When set, the broker stores the last message on this topic.
    ///
    /// Home Assistant MQTT discovery expects config publishes to be retained.
    pub retain: bool,
    pub payload: &'a [u8],
    pub packet_id: Option<u16>,
    #[cfg(feature = "v5")]
    pub properties: Vec<Property<'a>, 8>,
}
impl<'a> DecodePacket<'a> for Publish<'a> {
    fn decode(
        buf: &'a [u8],
        _version: MqttVersion,
    ) -> Result<Self, MqttError<transport::ErrorPlaceHolder>> {
        let flags = buf[0] & 0x0F;
        let retain = (flags & 0x01) != 0;
        let qos = match (flags >> 1) & 0x03 {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => return Err(MqttError::Protocol(ProtocolError::MalformedPacket)),
        };

        let mut cursor = 1;
        let _remaining_len = util::read_variable_byte_integer(&mut cursor, buf)?;

        let topic = read_utf8_string(&mut cursor, buf)?;

        let packet_id = if qos != QoS::AtMostOnce {
            let id = u16::from_be_bytes([buf[cursor], buf[cursor + 1]]);
            cursor += 2;
            Some(id)
        } else {
            None
        };

        #[cfg(feature = "v5")]
        let properties = if _version == MqttVersion::V5 {
            crate::util::read_properties(&mut cursor, buf)?
        } else {
            Vec::new()
        };

        let payload = &buf[cursor..];

        Ok(Publish {
            topic,
            qos,
            retain,
            payload,
            packet_id,
            #[cfg(feature = "v5")]
            properties,
        })
    }
}
impl<'a> EncodePacket for Publish<'a> {
    fn encode(
        &self,
        buf: &mut [u8],
        _version: MqttVersion,
    ) -> Result<usize, MqttError<transport::ErrorPlaceHolder>> {
        let mut cursor = 0;

        // Fixed header: PUBLISH packet type (3) with QoS + retain flags
        let retain_flag = u8::from(self.retain);
        let flags = ((self.qos as u8) << 1) | retain_flag;
        buf[cursor] = 0x30 | flags;
        cursor += 1;

        // Reserve space for remaining length (max 4 bytes)
        let remaining_len_pos = cursor;
        cursor += 4;
        let content_start = cursor;

        // Topic name
        cursor += write_utf8_string(&mut buf[cursor..], self.topic)?;

        // Packet ID (only for QoS > 0)
        if self.qos != QoS::AtMostOnce
            && let Some(id) = self.packet_id
        {
            buf[cursor..cursor + 2].copy_from_slice(&id.to_be_bytes());
            cursor += 2;
        }

        // Payload
        if cursor + self.payload.len() > buf.len() {
            return Err(MqttError::BufferTooSmall);
        }
        buf[cursor..cursor + self.payload.len()].copy_from_slice(self.payload);
        cursor += self.payload.len();

        // Write remaining length and compact
        let remaining_len = cursor - content_start;
        let len_bytes =
            util::write_variable_byte_integer_len(&mut buf[remaining_len_pos..], remaining_len)?;
        let header_len = 1 + len_bytes;
        buf.copy_within(content_start..cursor, header_len);

        Ok(header_len + remaining_len)
    }
}

// --- PUBACK Packet ---
#[derive(Debug)]
pub struct PubAck<'a> {
    pub packet_id: u16,
    #[cfg(feature = "v5")]
    pub properties: Vec<Property<'a>, 8>,
    #[cfg(not(feature = "v5"))]
    _phantom: PhantomData<&'a ()>,
}
impl<'a> DecodePacket<'a> for PubAck<'a> {
    fn decode(
        _buf: &'a [u8],
        _version: MqttVersion,
    ) -> Result<Self, MqttError<transport::ErrorPlaceHolder>> {
        Ok(PubAck {
            packet_id: 0,
            #[cfg(feature = "v5")]
            properties: Vec::new(),
            #[cfg(not(feature = "v5"))]
            _phantom: PhantomData,
        })
    }
}

// --- SUBSCRIBE Packet ---
#[derive(Debug)]
pub struct Subscribe<'a> {
    pub packet_id: u16,
    pub topics: Vec<(&'a str, QoS), 8>,
    #[cfg(feature = "v5")]
    pub properties: Vec<Property<'a>, 8>,
}

impl<'a> Subscribe<'a> {
    /// Creates a new Subscribe packet with a single topic.
    pub fn new(packet_id: u16, topic: &'a str, qos: QoS) -> Self {
        let mut topics = Vec::new();
        let _ = topics.push((topic, qos));
        Self {
            packet_id,
            topics,
            #[cfg(feature = "v5")]
            properties: Vec::new(),
        }
    }
}

impl<'a> DecodePacket<'a> for Subscribe<'a> {
    fn decode(
        _buf: &'a [u8],
        _version: MqttVersion,
    ) -> Result<Self, MqttError<transport::ErrorPlaceHolder>> {
        Ok(Subscribe {
            packet_id: 0,
            topics: Vec::new(),
            #[cfg(feature = "v5")]
            properties: Vec::new(),
        })
    }
}
impl<'a> EncodePacket for Subscribe<'a> {
    fn encode(
        &self,
        buf: &mut [u8],
        _version: MqttVersion,
    ) -> Result<usize, MqttError<transport::ErrorPlaceHolder>> {
        let mut cursor = 0;

        // Fixed header: SUBSCRIBE packet type (8) with reserved bits (0x02)
        buf[cursor] = 0x82;
        cursor += 1;

        // Reserve space for remaining length
        let remaining_len_pos = cursor;
        cursor += 4;
        let content_start = cursor;

        // Packet ID
        buf[cursor..cursor + 2].copy_from_slice(&self.packet_id.to_be_bytes());
        cursor += 2;

        // Topic filters with QoS
        for (topic, qos) in &self.topics {
            cursor += write_utf8_string(&mut buf[cursor..], topic)?;
            buf[cursor] = *qos as u8;
            cursor += 1;
        }

        // Write remaining length and compact
        let remaining_len = cursor - content_start;
        let len_bytes =
            util::write_variable_byte_integer_len(&mut buf[remaining_len_pos..], remaining_len)?;
        let header_len = 1 + len_bytes;
        buf.copy_within(content_start..cursor, header_len);

        Ok(header_len + remaining_len)
    }
}

// --- SUBACK Packet ---
#[derive(Debug)]
pub struct SubAck<'a> {
    pub packet_id: u16,
    pub reason_codes: Vec<u8, 8>,
    #[cfg(feature = "v5")]
    pub properties: Vec<Property<'a>, 8>,
    #[cfg(not(feature = "v5"))]
    _phantom: PhantomData<&'a ()>,
}
impl<'a> DecodePacket<'a> for SubAck<'a> {
    fn decode(
        buf: &'a [u8],
        _version: MqttVersion,
    ) -> Result<Self, MqttError<transport::ErrorPlaceHolder>> {
        let mut cursor = 1;
        let remaining_len = util::read_variable_byte_integer(&mut cursor, buf)?;
        let packet_end = cursor + remaining_len;

        // Packet ID
        let packet_id = u16::from_be_bytes([buf[cursor], buf[cursor + 1]]);
        cursor += 2;

        #[cfg(feature = "v5")]
        let properties = if _version == MqttVersion::V5 {
            crate::util::read_properties(&mut cursor, buf)?
        } else {
            Vec::new()
        };

        // Reason codes
        let mut reason_codes = Vec::new();
        while cursor < packet_end {
            let _ = reason_codes.push(buf[cursor]);
            cursor += 1;
        }

        Ok(SubAck {
            packet_id,
            reason_codes,
            #[cfg(feature = "v5")]
            properties,
            #[cfg(not(feature = "v5"))]
            _phantom: PhantomData,
        })
    }
}

// --- PINGREQ Packet ---
#[derive(Debug)]
pub struct PingReq;
impl EncodePacket for PingReq {
    fn encode(
        &self,
        buf: &mut [u8],
        _version: MqttVersion,
    ) -> Result<usize, MqttError<transport::ErrorPlaceHolder>> {
        if buf.len() < 2 {
            return Err(MqttError::BufferTooSmall);
        }
        buf[0] = 0xC0;
        buf[1] = 0x00;
        Ok(2)
    }
}

// --- PINGRESP Packet ---
#[derive(Debug)]
pub struct PingResp;

// --- DISCONNECT Packet ---
#[derive(Debug)]
pub struct Disconnect<'a> {
    #[cfg(feature = "v5")]
    pub reason_code: u8,
    #[cfg(feature = "v5")]
    pub properties: Vec<Property<'a>, 8>,
    #[cfg(not(feature = "v5"))]
    pub _phantom: PhantomData<&'a ()>,
}
impl<'a> DecodePacket<'a> for Disconnect<'a> {
    fn decode(
        _buf: &'a [u8],
        _version: MqttVersion,
    ) -> Result<Self, MqttError<transport::ErrorPlaceHolder>> {
        Ok(Disconnect {
            #[cfg(feature = "v5")]
            reason_code: 0,
            #[cfg(feature = "v5")]
            properties: Vec::new(),
            #[cfg(not(feature = "v5"))]
            _phantom: PhantomData,
        })
    }
}
impl<'a> EncodePacket for Disconnect<'a> {
    fn encode(
        &self,
        buf: &mut [u8],
        _version: MqttVersion,
    ) -> Result<usize, MqttError<transport::ErrorPlaceHolder>> {
        if buf.len() < 2 {
            return Err(MqttError::BufferTooSmall);
        }
        buf[0] = 0xE0;
        buf[1] = 0x00;
        Ok(2)
    }
}
