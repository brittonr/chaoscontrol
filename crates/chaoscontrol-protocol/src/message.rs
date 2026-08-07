/// Encode one message and JSON detail bytes into a payload buffer.
pub fn encode_payload(buffer: &mut [u8], message: &str, json_details: &[u8]) -> Option<usize> {
    const LENGTH_FIELD_BYTES: usize = core::mem::size_of::<u16>();
    let mut offset = 0;
    let message_bytes = message.as_bytes();
    let message_length = message_bytes.len();
    if message_length > u16::MAX as usize {
        return None;
    }
    if offset + LENGTH_FIELD_BYTES + message_length > buffer.len() {
        return None;
    }
    buffer[offset..offset + LENGTH_FIELD_BYTES]
        .copy_from_slice(&(message_length as u16).to_le_bytes());
    offset += LENGTH_FIELD_BYTES;
    buffer[offset..offset + message_length].copy_from_slice(message_bytes);
    offset += message_length;

    let json_length = json_details.len();
    if json_length > u16::MAX as usize {
        return None;
    }
    if offset + LENGTH_FIELD_BYTES + json_length > buffer.len() {
        return None;
    }
    buffer[offset..offset + LENGTH_FIELD_BYTES]
        .copy_from_slice(&(json_length as u16).to_le_bytes());
    offset += LENGTH_FIELD_BYTES;
    buffer[offset..offset + json_length].copy_from_slice(json_details);
    offset += json_length;
    Some(offset)
}

#[cfg(feature = "std")]
extern crate alloc;

/// Decoded message and raw JSON detail bytes.
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPayload {
    pub message: alloc::string::String,
    pub json_details: alloc::vec::Vec<u8>,
}

/// Decode one payload buffer.
#[cfg(feature = "std")]
pub fn decode_payload(buffer: &[u8]) -> Option<DecodedPayload> {
    const LENGTH_FIELD_BYTES: usize = core::mem::size_of::<u16>();
    let mut offset = 0;
    if offset + LENGTH_FIELD_BYTES > buffer.len() {
        return None;
    }
    let message_length = u16::from_le_bytes([buffer[offset], buffer[offset + 1]]) as usize;
    offset += LENGTH_FIELD_BYTES;
    if offset + message_length > buffer.len() {
        return None;
    }
    let message = alloc::string::String::from_utf8_lossy(&buffer[offset..offset + message_length])
        .into_owned();
    offset += message_length;

    if offset + LENGTH_FIELD_BYTES > buffer.len() {
        return None;
    }
    let json_length = u16::from_le_bytes([buffer[offset], buffer[offset + 1]]) as usize;
    offset += LENGTH_FIELD_BYTES;
    if offset + json_length > buffer.len() {
        return None;
    }
    let json_details = buffer[offset..offset + json_length].to_vec();
    Some(DecodedPayload {
        message,
        json_details,
    })
}
