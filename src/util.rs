//! # MQTT Serialization Utilities
//!
//! This module provides helper functions for reading and writing MQTT-specific data types
//! from and to byte buffers, such as variable-byte integers and length-prefixed strings.

use crate::error::{MqttError, ProtocolError};
use crate::transport;

/// Reads a variable-byte integer from the buffer, advancing the cursor.
///
/// This is a common encoding scheme in MQTT for packet lengths.
pub fn read_variable_byte_integer(
    cursor: &mut usize,
    buf: &[u8],
) -> Result<usize, MqttError<transport::ErrorPlaceHolder>> {
    let mut multiplier = 1;
    let mut value = 0;
    let mut i = 0;
    loop {
        let encoded_byte = buf
            .get(*cursor + i)
            .ok_or(MqttError::Protocol(ProtocolError::MalformedPacket))?;
        value += (encoded_byte & 127) as usize * multiplier;
        if (encoded_byte & 128) == 0 {
            break;
        }
        multiplier *= 128;
        i += 1;
        if i >= 4 {
            return Err(MqttError::Protocol(ProtocolError::MalformedPacket));
        }
    }
    *cursor += i + 1;
    Ok(value)
}

/// Writes a variable-byte integer to the buffer, advancing the cursor.
pub fn write_variable_byte_integer(
    cursor: &mut usize,
    buf: &mut [u8],
    mut val: usize,
) -> Result<(), MqttError<transport::ErrorPlaceHolder>> {
    loop {
        let mut encoded_byte = (val % 128) as u8;
        val /= 128;
        if val > 0 {
            encoded_byte |= 128;
        }
        // CORRECTED: Dereference the `&mut u8` to assign the value directly.
        *buf.get_mut(*cursor).ok_or(MqttError::BufferTooSmall)? = encoded_byte;
        *cursor += 1;
        if val == 0 {
            break;
        }
    }
    Ok(())
}

/// A simplified version of `write_variable_byte_integer` for external use that returns the byte count.
pub fn write_variable_byte_integer_len(
    buf: &mut [u8],
    mut val: usize,
) -> Result<usize, MqttError<transport::ErrorPlaceHolder>> {
    let mut i = 0;
    loop {
        let mut encoded_byte = (val % 128) as u8;
        val /= 128;
        if val > 0 {
            encoded_byte |= 128;
        }
        // CORRECTED: Dereference the `&mut u8` to assign the value directly.
        *buf.get_mut(i).ok_or(MqttError::BufferTooSmall)? = encoded_byte;
        i += 1;
        if val == 0 {
            break;
        }
    }
    Ok(i)
}

/// Reads a UTF-8 encoded string (prefixed with a 2-byte length) from the buffer.
pub fn read_utf8_string<'a>(
    cursor: &mut usize,
    buf: &'a [u8],
) -> Result<&'a str, MqttError<transport::ErrorPlaceHolder>> {
    let len = u16::from_be_bytes(
        buf.get(*cursor..*cursor + 2)
            .ok_or(MqttError::Protocol(ProtocolError::MalformedPacket))?
            .try_into()
            .unwrap(),
    ) as usize;
    *cursor += 2;
    let s = core::str::from_utf8(
        buf.get(*cursor..*cursor + len)
            .ok_or(MqttError::Protocol(ProtocolError::MalformedPacket))?,
    )
    .map_err(|_| MqttError::Protocol(ProtocolError::InvalidUtf8String))?;
    *cursor += len;
    Ok(s)
}

/// Writes a UTF-8 encoded string (prefixed with a 2-byte length) to the buffer.
pub fn write_utf8_string(
    buf: &mut [u8],
    s: &str,
) -> Result<usize, MqttError<transport::ErrorPlaceHolder>> {
    let len = s.len();
    if len > u16::MAX as usize {
        return Err(MqttError::Protocol(ProtocolError::PayloadTooLarge));
    }
    let len_bytes = (len as u16).to_be_bytes();

    let required_space = 2 + len;
    let slice = buf
        .get_mut(0..required_space)
        .ok_or(MqttError::BufferTooSmall)?;

    slice[0..2].copy_from_slice(&len_bytes);
    slice[2..].copy_from_slice(s.as_bytes());
    Ok(required_space)
}

/// Reads MQTT v5 properties from the buffer.
#[cfg(feature = "v5")]
pub fn read_properties<'a>(
    cursor: &mut usize,
    buf: &'a [u8],
) -> Result<Vec<packet::Property<'a>, 8>, MqttError<transport::ErrorPlaceHolder>> {
    let mut properties = Vec::new();
    let prop_len = read_variable_byte_integer(cursor, buf)?;
    let prop_end = *cursor + prop_len;

    while *cursor < prop_end {
        let id = buf[*cursor];
        *cursor += 1;
        let data_start = *cursor;
        // This is a simplified implementation. A real one would decode property data
        // based on the specific property ID.
        let data_len = 1; // Placeholder
        *cursor += data_len;
        properties
            .push(packet::Property {
                id,
                data: &buf[data_start..data_start + data_len],
            })
            .map_err(|_| MqttError::Protocol(ProtocolError::TooManyProperties))?;
    }
    Ok(properties)
}

/// Writes MQTT v5 properties to the buffer.
#[cfg(feature = "v5")]
pub fn write_properties(
    cursor: &mut usize,
    buf: &mut [u8],
    properties: &[packet::Property],
) -> Result<(), MqttError<transport::ErrorPlaceHolder>> {
    // This is a simplified implementation. A real one would calculate total length first.
    let prop_len_cursor_start = *cursor;
    *cursor += 1; // Reserve space for length

    let props_start = *cursor;
    for prop in properties {
        buf[*cursor] = prop.id;
        *cursor += 1;
        buf[*cursor..*cursor + prop.data.len()].copy_from_slice(prop.data);
        *cursor += prop.data.len();
    }
    let total_prop_len = *cursor - props_start;

    // Write the actual property length
    let mut temp_cursor = prop_len_cursor_start;
    let _ = crate::util::write_variable_byte_integer(&mut temp_cursor, buf, total_prop_len)?;

    Ok(())
}
