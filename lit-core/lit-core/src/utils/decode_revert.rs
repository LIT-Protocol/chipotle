//! Decode standard Solidity revert reasons from raw EVM revert data.
//!
//! Solidity encodes revert reasons in two forms:
//! - `Error(string)` with selector `0x08c379a0`
//! - `Panic(uint256)` with selector `0x4e487b71`
//!
//! This module handles both without depending on ethers or any ABI library.

/// The 4-byte selector for `Error(string)` — `0x08c379a0`.
const ERROR_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0];

/// The 4-byte selector for `Panic(uint256)` — `0x4e487b71`.
const PANIC_SELECTOR: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71];

/// Attempt to decode a human-readable revert reason from raw EVM revert data.
///
/// Returns `Some(message)` if the data contains a standard `Error(string)` or
/// `Panic(uint256)`, otherwise `None` (which means it may be a custom error
/// that requires ABI-specific decoding).
pub fn decode_revert(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }

    let selector: [u8; 4] = data[0..4].try_into().ok()?;
    let payload = &data[4..];

    match selector {
        ERROR_SELECTOR => decode_error_string(payload),
        PANIC_SELECTOR => decode_panic_code(payload),
        _ => None,
    }
}

/// Decode the `Error(string)` ABI encoding:
///   - bytes 0..32:  offset to string data (must be 0x20)
///   - bytes 32..64: string length
///   - bytes 64..:   string content (padded to 32-byte boundary)
fn decode_error_string(payload: &[u8]) -> Option<String> {
    if payload.len() < 64 {
        return None;
    }

    // Validate the ABI offset word (bytes 0..32). For standard Error(string),
    // the high 31 bytes must be zero and the low byte must be 0x20.
    if payload[..31].iter().any(|&b| b != 0) || payload[31] != 0x20 {
        return None;
    }

    // Read string length from bytes 32..64 (big-endian u256).
    // Reject values that don't fit in the low 8 bytes.
    let len_bytes = &payload[32..64];
    if len_bytes[..24].iter().any(|&b| b != 0) {
        return None;
    }
    let len = usize::try_from(u64::from_be_bytes(len_bytes[24..32].try_into().ok()?)).ok()?;

    // Use checked_add to prevent overflow on 64 + len.
    let end = 64usize.checked_add(len)?;
    if payload.len() < end {
        return None;
    }

    let s = std::str::from_utf8(&payload[64..end]).ok()?;
    Some(s.to_string())
}

/// Decode the `Panic(uint256)` code into a human-readable message.
fn decode_panic_code(payload: &[u8]) -> Option<String> {
    if payload.len() < 32 {
        return None;
    }

    // The panic code is a uint256. All known Solidity panic codes fit in a
    // single byte, so reject values with non-zero high bytes as malformed.
    let code_bytes = &payload[0..32];
    if code_bytes[..24].iter().any(|&b| b != 0) {
        return None;
    }
    let code = u64::from_be_bytes(code_bytes[24..32].try_into().ok()?);

    let description = match code {
        0x00 => "generic compiler panic",
        0x01 => "assertion failed",
        0x11 => "arithmetic overflow/underflow",
        0x12 => "division or modulo by zero",
        0x21 => "conversion to invalid enum value",
        0x22 => "access to incorrectly encoded storage byte array",
        0x31 => "pop on empty array",
        0x32 => "array index out of bounds",
        0x41 => "allocation of too much memory",
        0x51 => "called an uninitialized function pointer",
        _ => return Some(format!("Panic(0x{code:02x})")),
    };

    Some(format!("Panic(0x{code:02x}): {description}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_error_string_basic() {
        // Error("Insufficient funds") encoded
        let mut data = Vec::new();
        data.extend_from_slice(&ERROR_SELECTOR);
        // offset = 32
        data.extend_from_slice(&[0u8; 31]);
        data.push(0x20);
        // length = 18 ("Insufficient funds")
        data.extend_from_slice(&[0u8; 31]);
        data.push(18);
        // string content padded to 32 bytes
        const MSG: &[u8] = b"Insufficient funds";
        data.extend_from_slice(MSG);
        data.extend_from_slice(&[0u8; 32 - MSG.len()]);

        assert_eq!(decode_revert(&data), Some("Insufficient funds".to_string()));
    }

    #[test]
    fn decode_panic_assertion() {
        let mut data = Vec::new();
        data.extend_from_slice(&PANIC_SELECTOR);
        data.extend_from_slice(&[0u8; 31]);
        data.push(0x01);

        assert_eq!(decode_revert(&data), Some("Panic(0x01): assertion failed".to_string()));
    }

    #[test]
    fn decode_panic_overflow() {
        let mut data = Vec::new();
        data.extend_from_slice(&PANIC_SELECTOR);
        data.extend_from_slice(&[0u8; 31]);
        data.push(0x11);

        assert_eq!(
            decode_revert(&data),
            Some("Panic(0x11): arithmetic overflow/underflow".to_string())
        );
    }

    #[test]
    fn decode_unknown_selector_returns_none() {
        let data = [0xde, 0xad, 0xbe, 0xef, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(decode_revert(&data), None);
    }

    #[test]
    fn decode_too_short_returns_none() {
        assert_eq!(decode_revert(&[0x08, 0xc3]), None);
    }

    #[test]
    fn decode_empty_returns_none() {
        assert_eq!(decode_revert(&[]), None);
    }

    #[test]
    fn decode_error_string_payload_too_short() {
        // Error selector with payload < 64 bytes
        let mut data = Vec::new();
        data.extend_from_slice(&ERROR_SELECTOR);
        data.extend_from_slice(&[0u8; 32]); // only 32 bytes of payload, need 64
        assert_eq!(decode_revert(&data), None);
    }

    #[test]
    fn decode_error_string_length_exceeds_payload() {
        // Error selector with length claiming more data than available
        let mut data = Vec::new();
        data.extend_from_slice(&ERROR_SELECTOR);
        // offset = 32
        data.extend_from_slice(&[0u8; 31]);
        data.push(0x20);
        // length = 255 (way more than we provide)
        data.extend_from_slice(&[0u8; 31]);
        data.push(0xff);
        // only 4 bytes of content
        data.extend_from_slice(b"abcd");
        assert_eq!(decode_revert(&data), None);
    }

    #[test]
    fn decode_panic_unknown_code() {
        let mut data = Vec::new();
        data.extend_from_slice(&PANIC_SELECTOR);
        data.extend_from_slice(&[0u8; 31]);
        data.push(0xff);
        assert_eq!(decode_revert(&data), Some("Panic(0xff)".to_string()));
    }

    #[test]
    fn decode_panic_payload_too_short() {
        // Panic selector with payload < 32 bytes
        let mut data = Vec::new();
        data.extend_from_slice(&PANIC_SELECTOR);
        data.extend_from_slice(&[0u8; 16]); // only 16 bytes
        assert_eq!(decode_revert(&data), None);
    }

    #[test]
    fn decode_error_string_bad_offset_returns_none() {
        // Error selector with offset != 0x20
        let mut data = Vec::new();
        data.extend_from_slice(&ERROR_SELECTOR);
        // offset = 0x40 (wrong)
        data.extend_from_slice(&[0u8; 31]);
        data.push(0x40);
        // length = 5
        data.extend_from_slice(&[0u8; 31]);
        data.push(5);
        data.extend_from_slice(b"hello");
        data.extend_from_slice(&[0u8; 27]);
        assert_eq!(decode_revert(&data), None);
    }

    #[test]
    fn decode_error_string_nonzero_high_length_bytes_returns_none() {
        // Error selector with non-zero high bytes in the length word
        let mut data = Vec::new();
        data.extend_from_slice(&ERROR_SELECTOR);
        // offset = 0x20
        data.extend_from_slice(&[0u8; 31]);
        data.push(0x20);
        // length word with non-zero high byte
        let mut len_word = [0u8; 32];
        len_word[0] = 1; // non-zero high byte
        len_word[31] = 5;
        data.extend_from_slice(&len_word);
        data.extend_from_slice(b"hello");
        data.extend_from_slice(&[0u8; 27]);
        assert_eq!(decode_revert(&data), None);
    }

    #[test]
    fn decode_panic_nonzero_high_bytes_returns_none() {
        // Panic with non-zero high bytes in the code word
        let mut data = Vec::new();
        data.extend_from_slice(&PANIC_SELECTOR);
        let mut code_word = [0u8; 32];
        code_word[0] = 1; // non-zero high byte
        code_word[31] = 0x01;
        data.extend_from_slice(&code_word);
        assert_eq!(decode_revert(&data), None);
    }
}
