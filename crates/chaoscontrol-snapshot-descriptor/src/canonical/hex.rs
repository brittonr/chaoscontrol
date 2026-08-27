const HEX_ALPHABET_BYTES: usize = 16;
const HEX_CHARACTERS_PER_BYTE: usize = 2;
const HEX_HIGH_NIBBLE_SHIFT: u32 = 4;
const HEX_LOW_NIBBLE_MASK: u8 = 0x0f;
const HEX_DIGITS: &[u8; HEX_ALPHABET_BYTES] = b"0123456789abcdef";

pub(super) fn lower(bytes: &[u8]) -> String {
    let capacity = bytes.len().saturating_mul(HEX_CHARACTERS_PER_BYTE);
    let mut output = String::with_capacity(capacity);
    for byte in bytes {
        output.push(char::from(
            HEX_DIGITS[usize::from(byte >> HEX_HIGH_NIBBLE_SHIFT)],
        ));
        output.push(char::from(
            HEX_DIGITS[usize::from(byte & HEX_LOW_NIBBLE_MASK)],
        ));
    }
    output
}
