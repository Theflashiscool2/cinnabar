//! Uncompressed Java framing, signed VarInts and length-prefixed UTF-8 strings.

use crate::ProtocolError;

/// Java's frame length is a VarInt restricted to three bytes.
pub const MAX_FRAME_BYTES: usize = (1 << 21) - 1;
/// Maximum encoded width of a frame length, unlike a general five-byte VarInt.
pub const FRAME_LENGTH_BYTES: usize = 3;

/// Appends a signed VarInt using Java's two's-complement representation, not ZigZag.
pub fn write_varint(value: i32, out: &mut Vec<u8>) {
    let mut value = value as u32;
    loop {
        let next = (value & 0x7f) as u8;
        value >>= 7;
        out.push(if value == 0 { next } else { next | 0x80 });
        if value == 0 {
            return;
        }
    }
}

/// Reads one VarInt, rejecting truncation and overflow, including in the fifth byte.
pub fn read_varint(input: &mut &[u8]) -> Result<i32, ProtocolError> {
    let mut value = 0u32;
    for index in 0..5 {
        let (&byte, rest) = input.split_first().ok_or(ProtocolError::Truncated)?;
        *input = rest;
        if index == 4 && byte & 0xf0 != 0 {
            return Err(ProtocolError::InvalidVarInt);
        }
        value |= u32::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            return Ok(value as i32);
        }
    }
    Err(ProtocolError::InvalidVarInt)
}

/// Encodes a UTF-8 string constrained by Java UTF-16 code units.
pub fn write_string(value: &str, max_units: usize, out: &mut Vec<u8>) -> Result<(), ProtocolError> {
    validate_string(value, max_units)?;
    let length = i32::try_from(value.len()).map_err(|_| ProtocolError::StringTooLong)?;
    write_varint(length, out);
    out.extend_from_slice(value.as_bytes());
    Ok(())
}

/// Borrows a UTF-8 string after checking byte length and UTF-16 length.
pub fn read_string<'a>(input: &mut &'a [u8], max_units: usize) -> Result<&'a str, ProtocolError> {
    let length = usize::try_from(read_varint(input)?).map_err(|_| ProtocolError::StringTooLong)?;
    if length > max_units.saturating_mul(3) {
        return Err(ProtocolError::StringTooLong);
    }
    let (bytes, rest) = input
        .split_at_checked(length)
        .ok_or(ProtocolError::Truncated)?;
    let value = std::str::from_utf8(bytes)?;
    validate_string(value, max_units)?;
    *input = rest;
    Ok(value)
}

/// Checks strings before allocating a wire representation.
fn validate_string(value: &str, max_units: usize) -> Result<(), ProtocolError> {
    if value.len() > max_units.saturating_mul(3) || value.encode_utf16().count() > max_units {
        return Err(ProtocolError::StringTooLong);
    }
    Ok(())
}

/// Prefixes a complete uncompressed packet body, including its packet ID.
pub fn frame(body: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if body.is_empty() || body.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::InvalidFrameLength);
    }
    let mut out = Vec::with_capacity(FRAME_LENGTH_BYTES + body.len());
    write_varint(body.len() as i32, &mut out);
    out.extend_from_slice(body);
    Ok(out)
}

#[cfg(test)]
#[path = "codec_tests.rs"]
mod tests;
