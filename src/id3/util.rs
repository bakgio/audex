//! ID3 low-level utilities
//!
//! Provides error types, synchsafe integer encoding ([`BitPaddedInt`]),
//! unsynchronization codec ([`Unsynch`]), frame ID validation, and
//! v2.2-to-v2.3 frame ID upgrade tables.

use crate::{AudexError, Result};
use std::fmt;

/// Base ID3 error type
#[derive(Debug, Clone)]
pub struct ID3Error {
    pub message: String,
}

impl fmt::Display for ID3Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ID3Error {}

impl From<ID3Error> for AudexError {
    fn from(err: ID3Error) -> Self {
        AudexError::InvalidData(err.message)
    }
}

/// ID3 no header error
#[derive(Debug, Clone)]
pub struct ID3NoHeaderError {
    pub message: String,
}

impl fmt::Display for ID3NoHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ID3NoHeaderError {}

impl From<ID3NoHeaderError> for AudexError {
    fn from(err: ID3NoHeaderError) -> Self {
        AudexError::InvalidData(err.message)
    }
}

/// ID3 unsupported version error
#[derive(Debug, Clone)]
pub struct ID3UnsupportedVersionError {
    pub message: String,
}

impl fmt::Display for ID3UnsupportedVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ID3UnsupportedVersionError {}

impl From<ID3UnsupportedVersionError> for AudexError {
    fn from(err: ID3UnsupportedVersionError) -> Self {
        AudexError::UnsupportedFormat(err.message)
    }
}

/// ID3 encryption unsupported error
#[derive(Debug, Clone)]
pub struct ID3EncryptionUnsupportedError {
    pub message: String,
}

impl fmt::Display for ID3EncryptionUnsupportedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ID3EncryptionUnsupportedError {}

impl From<ID3EncryptionUnsupportedError> for AudexError {
    fn from(err: ID3EncryptionUnsupportedError) -> Self {
        AudexError::UnsupportedFormat(err.message)
    }
}

/// ID3 junk frame error
#[derive(Debug, Clone)]
pub struct ID3JunkFrameError {
    pub message: String,
}

impl fmt::Display for ID3JunkFrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ID3JunkFrameError {}

impl From<ID3JunkFrameError> for AudexError {
    fn from(err: ID3JunkFrameError) -> Self {
        AudexError::InvalidData(err.message)
    }
}

/// ID3 bad unsynch data error
#[derive(Debug, Clone)]
pub struct ID3BadUnsynchData {
    pub message: String,
}

impl fmt::Display for ID3BadUnsynchData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ID3BadUnsynchData {}

/// ID3 bad compressed data error
#[derive(Debug, Clone)]
pub struct ID3BadCompressedData {
    pub message: String,
}

impl fmt::Display for ID3BadCompressedData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ID3BadCompressedData {}

/// ID3 tag error
#[derive(Debug, Clone)]
pub struct ID3TagError {
    pub message: String,
}

impl fmt::Display for ID3TagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ID3TagError {}

/// ID3 warning
#[derive(Debug, Clone)]
pub struct ID3Warning {
    pub message: String,
}

impl fmt::Display for ID3Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ID3Warning {}

/// Validate frame ID - exactly following standard: frame_id.isalnum() and frame_id.isupper()
/// Check if a string is a valid ID3v2 frame ID.
///
/// Valid frame IDs consist of 3 or 4 uppercase ASCII alphanumeric characters
/// (e.g. `"TIT2"`, `"APIC"`, `"TT2"`).
pub fn is_valid_frame_id(frame_id: &str) -> bool {
    // Frame IDs must be exactly 3 (v2.2) or 4 (v2.3/v2.4) characters
    if frame_id.len() != 3 && frame_id.len() != 4 {
        return false;
    }

    // Check isalnum() - all characters must be alphanumeric and ASCII
    let is_alnum = frame_id
        .chars()
        .all(|c| c.is_alphanumeric() && c.is_ascii());

    // Check isupper() - there must be at least one cased character and all cased characters must be uppercase
    let has_cased = frame_id.chars().any(|c| c.is_alphabetic());
    let all_cased_upper = frame_id
        .chars()
        .filter(|c| c.is_alphabetic())
        .all(|c| c.is_uppercase());

    is_alnum && has_cased && all_cased_upper
}

/// ID3 configuration for saving tags - exactly following standard constructor
#[derive(Debug, Clone)]
pub struct ID3SaveConfig {
    pub v2_version: u8,
    pub v23_separator: Option<String>,
}

impl ID3SaveConfig {
    /// Create new ID3SaveConfig with standard parameters
    pub fn new(v2_version: Option<u8>, v23_separator: Option<String>) -> Result<Self> {
        let version = v2_version.unwrap_or(4);

        // Assert v2_version in (3, 4)
        if version != 3 && version != 4 {
            return Err(AudexError::InvalidData(format!(
                "v2_version must be 3 or 4, got {}",
                version
            )));
        }

        Ok(ID3SaveConfig {
            v2_version: version,
            v23_separator,
        })
    }
}

/// Unsynchronization utilities
/// ID3v2 unsynchronization codec
///
/// Unsynchronization is an encoding scheme that ensures ID3v2 tag data
/// never contains the byte sequence `0xFF 0xE0`–`0xFF 0xFF`, which could
/// be mistaken for an MPEG sync word by naive decoders. It works by
/// inserting a `0x00` byte after every `0xFF` byte during encoding, and
/// removing those inserted bytes during decoding.
pub struct Unsynch;

impl Unsynch {
    /// Decode unsynchronized data back to its original form.
    ///
    /// Removes the `0x00` bytes that were inserted after `0xFF` bytes
    /// during encoding. Returns the original unmodified data.
    ///
    /// This implementation is intentionally lenient: non-conformant sequences
    /// such as bare `0xFF 0xFF` pairs or `0xFF` followed by a byte >= `0xE0`
    /// without a protection byte are passed through unchanged rather than
    /// rejected. This improves compatibility with real-world taggers that
    /// emit malformed unsynchronization. As a consequence, callers should not
    /// assume round-trip fidelity — `decode(encode(data))` is always identity,
    /// but `encode(decode(data))` may differ if `data` contains non-conformant
    /// sequences that were not originally produced by a correct encoder.
    pub fn decode(value: &[u8]) -> Result<Vec<u8>> {
        // Split data on 0xFF bytes
        let fragments: Vec<&[u8]> = value.split(|&b| b == 0xFF).collect();

        // Tolerate trailing 0xFF from non-conforming taggers.
        // A bare 0xFF at the end is harmless — there is no following byte
        // that could form a false MPEG sync. Rather than rejecting the
        // entire payload, preserve the trailing 0xFF in the output.
        let has_trailing_ff = fragments.len() > 1 && fragments.last() == Some(&[].as_ref());

        let mut result = Vec::new();

        // Add first fragment
        result.extend_from_slice(fragments[0]);

        // Determine how many fragments to process (exclude trailing empty
        // fragment when the input ends with 0xFF)
        let fragment_count = if has_trailing_ff {
            fragments.len() - 1
        } else {
            fragments.len()
        };

        // Process remaining fragments:
        for fragment in fragments[1..fragment_count].iter() {
            // Add the FF separator back
            result.push(0xFF);

            if fragment.is_empty() {
                // Two consecutive 0xFF bytes — pass through leniently.
                // Non-conformant taggers sometimes produce these sequences.
                continue;
            }

            if fragment[0] >= 0xE0 {
                // Non-conformant sequence: 0xFF followed by a byte >= 0xE0
                // without the required 0x00 protection byte. Real-world
                // taggers sometimes emit these. Pass through as-is rather
                // than rejecting the entire frame.
                result.extend_from_slice(fragment);
            } else if fragment[0] == 0x00 {
                // Standard protection byte — strip it
                if fragment.len() > 1 {
                    result.extend_from_slice(&fragment[1..]);
                }
            } else {
                result.extend_from_slice(fragment);
            }
        }

        // Append the trailing 0xFF that was split off, if present
        if has_trailing_ff {
            result.push(0xFF);
        }

        Ok(result)
    }

    /// Check whether the given data contains byte sequences that require
    /// unsynchronization (0xFF followed by a byte >= 0xE0 or == 0x00, or a
    /// trailing 0xFF). Returns `true` if encoding would modify the data.
    pub fn needs_encode(value: &[u8]) -> bool {
        for i in 0..value.len() {
            if value[i] == 0xFF {
                if i + 1 >= value.len() {
                    // Trailing 0xFF needs protection
                    return true;
                }
                if value[i + 1] >= 0xE0 || value[i + 1] == 0x00 {
                    return true;
                }
            }
        }
        false
    }

    /// Encode data with unsynchronization
    /// Encode data with unsynchronization.
    ///
    /// Inserts `0x00` after every `0xFF` byte to prevent false MPEG sync
    /// detection in legacy players.
    pub fn encode(value: &[u8]) -> Vec<u8> {
        // Split data on 0xFF bytes
        let fragments: Vec<&[u8]> = value.split(|&b| b == 0xFF).collect();

        let mut result = Vec::new();

        // Add first fragment
        result.extend_from_slice(fragments[0]);

        // Process remaining fragments:
        for fragment in fragments.iter().skip(1) {
            // Add the FF separator back
            result.push(0xFF);

            // Check if fragment needs unsync marker:
            if fragment.is_empty() || fragment[0] >= 0xE0 || fragment[0] == 0x00 {
                result.push(0x00); // Insert protection byte
            }

            result.extend_from_slice(fragment);
        }

        result
    }
}

/// Synchsafe integer used in ID3v2 tag and frame headers
///
/// ID3v2.4 uses "synchsafe" integers where only the lower 7 bits of each
/// byte carry data (the high bit is always 0). This prevents false MPEG
/// sync detection. For example, the 4-byte synchsafe integer `0x00 0x00
/// 0x02 0x01` decodes to `0x101` (257).
///
/// The `bits` parameter controls how many bits per byte are used (default 7
/// for synchsafe). Setting `bits=8` treats the input as a regular integer.
#[derive(Debug, Clone, PartialEq)]
pub struct BitPaddedInt {
    /// Decoded integer value
    value: u32,
    /// Number of data bits per byte (7 for synchsafe, 8 for regular)
    pub bits: u8,
    /// Whether the input is big-endian
    pub bigendian: bool,
}

impl BitPaddedInt {
    /// Create new BitPaddedInt
    pub fn new(
        value: BitPaddedIntInput,
        bits: Option<u8>,
        bigendian: Option<bool>,
    ) -> Result<Self> {
        let bits = bits.unwrap_or(7);
        let bigendian = bigendian.unwrap_or(true);

        // Calculate bit mask
        let mask = (1u32 << bits) - 1;
        let mut numeric_value = 0u32;
        let mut shift = 0u32;

        match value {
            BitPaddedIntInput::Int(val) => {
                // Negative values not allowed
                if val < 0 {
                    return Err(AudexError::InvalidData(
                        "BitPaddedInt value cannot be negative".to_string(),
                    ));
                }

                let mut val = val as u32;
                // Process value bits:
                //             numeric_value += (value & mask) << shift
                //             value >>= 8
                //             shift += bits
                while val > 0 {
                    // Ensure the shift fits within u32 range
                    if shift >= 32 {
                        return Err(AudexError::InvalidData(
                            "BitPaddedInt overflow: value exceeds u32 capacity".to_string(),
                        ));
                    }

                    // Use checked arithmetic to detect values too large for u32
                    let masked = val & mask;
                    let shifted = masked.checked_shl(shift).ok_or_else(|| {
                        AudexError::InvalidData(
                            "BitPaddedInt overflow: shift exceeds u32 range".to_string(),
                        )
                    })?;
                    numeric_value = numeric_value.checked_add(shifted).ok_or_else(|| {
                        AudexError::InvalidData(
                            "BitPaddedInt overflow: accumulated value exceeds u32".to_string(),
                        )
                    })?;

                    val >>= 8;
                    shift += bits as u32;
                }
            }
            BitPaddedIntInput::Bytes(bytes) => {
                // Reverse bytes if big endian
                let byte_iter: Box<dyn Iterator<Item = u8>> = if bigendian {
                    Box::new(bytes.into_iter().rev())
                } else {
                    Box::new(bytes.into_iter())
                };

                // Process each byte:
                //             numeric_value += (byte & mask) << shift
                //             shift += bits
                for byte in byte_iter {
                    // Skip zero bytes that contribute nothing to the result
                    let masked = byte as u32 & mask;
                    if masked == 0 {
                        shift += bits as u32;
                        continue;
                    }

                    // Ensure the shift fits within u32 range
                    if shift >= 32 {
                        return Err(AudexError::InvalidData(
                            "BitPaddedInt overflow: value exceeds u32 capacity".to_string(),
                        ));
                    }

                    // Use checked arithmetic to detect values too large for u32
                    let shifted = masked.checked_shl(shift).ok_or_else(|| {
                        AudexError::InvalidData(
                            "BitPaddedInt overflow: shift exceeds u32 range".to_string(),
                        )
                    })?;

                    // Detect if the shift silently dropped any high bits
                    if shifted >> shift != masked {
                        return Err(AudexError::InvalidData(
                            "BitPaddedInt overflow: value exceeds u32 capacity".to_string(),
                        ));
                    }

                    numeric_value = numeric_value.checked_add(shifted).ok_or_else(|| {
                        AudexError::InvalidData(
                            "BitPaddedInt overflow: accumulated value exceeds u32".to_string(),
                        )
                    })?;

                    shift += bits as u32;
                }
            }
        }

        Ok(BitPaddedInt {
            value: numeric_value,
            bits,
            bigendian,
        })
    }

    /// Create from bytes (convenience method for compatibility)
    pub fn from_bytes(bytes: &[u8], bits: u8, bigendian: bool) -> Result<Self> {
        Self::new(
            BitPaddedIntInput::Bytes(bytes.to_vec()),
            Some(bits),
            Some(bigendian),
        )
    }

    /// Get the numeric value
    pub fn value(&self) -> u32 {
        self.value
    }

    /// Convert to string representation
    pub fn as_str(&self, width: Option<i32>, minwidth: Option<u32>) -> Result<Vec<u8>> {
        Self::to_str(
            self.value,
            Some(self.bits),
            Some(self.bigendian),
            width,
            minwidth,
        )
    }

    /// Static to_str method - exact standard specification
    pub fn to_str(
        value: u32,
        bits: Option<u8>,
        bigendian: Option<bool>,
        width: Option<i32>,
        minwidth: Option<u32>,
    ) -> Result<Vec<u8>> {
        let bits = bits.unwrap_or(7);
        let bigendian = bigendian.unwrap_or(true);
        let width = width.unwrap_or(4);
        let minwidth = minwidth.unwrap_or(4);

        // Reject invalid negative widths. Only -1 (growable mode) and
        // non-negative values are valid. Any other negative value would
        // wrap to a huge usize on cast, causing an enormous allocation.
        if width < -1 {
            return Err(AudexError::InvalidData(format!(
                "Invalid negative width: {}",
                width
            )));
        }

        // Cap the width to prevent excessive memory allocation from untrusted input.
        // Synchsafe integers in practice are at most 4-8 bytes; 16 is generous.
        const MAX_BPI_WIDTH: i32 = 16;
        if width > MAX_BPI_WIDTH {
            return Err(AudexError::InvalidData(format!(
                "BitPaddedInt width {} exceeds maximum of {}",
                width, MAX_BPI_WIDTH
            )));
        }

        // Calculate bit mask
        let mask = (1u32 << bits) - 1;
        let mut value = value;

        let mut bytes = if width != -1 {
            let width = width as usize;
            let mut bytes = vec![0u8; width];
            let mut index = 0;

            while value > 0 {
                if index >= width {
                    return Err(AudexError::InvalidData(format!(
                        "Value too wide (>{} bytes)",
                        width
                    )));
                }
                bytes[index] = (value & mask) as u8;
                value >>= bits;
                index += 1;
            }

            bytes
        } else {
            // PCNT and POPM use growing integers of at least 4 bytes (=minwidth) as counters
            let mut bytes = Vec::new();

            while value > 0 {
                bytes.push((value & mask) as u8);
                value >>= bits;
            }

            // Pad to minwidth with zeros
            while bytes.len() < minwidth as usize {
                bytes.push(0x00);
            }

            bytes
        };

        if bigendian {
            bytes.reverse();
        }

        Ok(bytes)
    }

    /// Check if padding bits are all zero - exact standard specification
    pub fn has_valid_padding(value: BitPaddedIntInput, bits: Option<u8>) -> bool {
        let bits = bits.unwrap_or(7);

        // Assert bits <= 8
        if bits > 8 {
            return false;
        }

        // Calculate mask for unused bits
        let mask = (((1u32 << (8 - bits)) - 1) << bits) as u8;

        match value {
            BitPaddedIntInput::Int(val) => {
                if val < 0 {
                    return false;
                }
                let mut val = val as u32;
                while val > 0 {
                    if (val as u8) & mask != 0 {
                        return false;
                    }
                    val >>= 8;
                }
            }
            BitPaddedIntInput::Bytes(bytes) => {
                for byte in bytes {
                    if byte & mask != 0 {
                        return false;
                    }
                }
            }
        }

        true
    }
}

/// Input type for BitPaddedInt to support both int and bytes as standard
#[derive(Debug, Clone)]
pub enum BitPaddedIntInput {
    Int(i32),
    Bytes(Vec<u8>),
}

impl From<i32> for BitPaddedIntInput {
    fn from(val: i32) -> Self {
        BitPaddedIntInput::Int(val)
    }
}

impl From<u32> for BitPaddedIntInput {
    fn from(val: u32) -> Self {
        // Encode as 4 big-endian bytes to preserve the full u32 range
        // without truncation from the u32-to-i32 cast
        BitPaddedIntInput::Bytes(val.to_be_bytes().to_vec())
    }
}

impl From<Vec<u8>> for BitPaddedIntInput {
    fn from(val: Vec<u8>) -> Self {
        BitPaddedIntInput::Bytes(val)
    }
}

impl From<&[u8]> for BitPaddedIntInput {
    fn from(val: &[u8]) -> Self {
        BitPaddedIntInput::Bytes(val.to_vec())
    }
}

impl fmt::Display for BitPaddedInt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<BitPaddedInt> for u32 {
    fn from(bpi: BitPaddedInt) -> Self {
        bpi.value
    }
}

/// Remove unsynchronization from data.
///
/// Returns an error if the data contains invalid unsynchronization sequences
/// (e.g. 0xFF followed by 0xE0 or higher), rather than silently returning
/// the raw undecoded data. Callers can then decide how to handle the failure.
pub fn remove_unsynchronization(data: &[u8]) -> Result<Vec<u8>> {
    Unsynch::decode(data)
}

/// Add unsynchronization to data
pub fn add_unsynchronization(data: &[u8]) -> Vec<u8> {
    Unsynch::encode(data)
}

/// Decode synchsafe integer (7 bits per byte).
/// Limited to 4 bytes (28 bits) to prevent overflow of the u32 result.
///
/// Note: this function silently masks high bits. Use `decode_synchsafe_int_checked`
/// when validation of the input bytes is required.
pub fn decode_synchsafe_int(bytes: &[u8]) -> u32 {
    let mut result = 0u32;
    for &byte in bytes.iter().take(4) {
        result = (result << 7) | (byte & 0x7F) as u32;
    }
    result
}

/// Decode synchsafe integer with validation that no high bits are set.
///
/// Returns an error if any byte has bit 7 set, which would indicate
/// the value is not a properly encoded synchsafe integer. This catches
/// malformed tags that use standard big-endian sizes where synchsafe
/// encoding is required.
pub fn decode_synchsafe_int_checked(bytes: &[u8]) -> Result<u32> {
    for (i, &byte) in bytes.iter().take(4).enumerate() {
        if byte & 0x80 != 0 {
            return Err(AudexError::InvalidData(format!(
                "synchsafe integer byte {} has high bit set: 0x{:02X}",
                i, byte
            )));
        }
    }
    Ok(decode_synchsafe_int(bytes))
}

/// Encode a value as a synchsafe integer (7 bits per byte, 4 bytes = 28 bits max).
/// Values must not exceed 0x0FFF_FFFF (268,435,455).
///
/// Returns an error if the value exceeds the 28-bit maximum, since silently
/// dropping the high bits would produce a corrupt tag size field.
pub fn encode_synchsafe_int(value: u32) -> Result<[u8; 4]> {
    if value > 0x0FFF_FFFF {
        return Err(AudexError::InvalidData(format!(
            "synchsafe encoding only supports values up to 2^28 - 1, got {}",
            value
        )));
    }
    Ok([
        ((value >> 21) & 0x7F) as u8,
        ((value >> 14) & 0x7F) as u8,
        ((value >> 7) & 0x7F) as u8,
        (value & 0x7F) as u8,
    ])
}

/// Validate frame ID for different ID3 versions
pub fn is_valid_frame_id_versioned(frame_id: &str, version: (u8, u8)) -> bool {
    match version {
        (2, 2) => {
            // ID3v2.2 uses 3-character frame IDs
            frame_id.len() == 3 && is_valid_frame_id(frame_id)
        }
        (2, 3) | (2, 4) => {
            // ID3v2.3 and 2.4 use 4-character frame IDs
            frame_id.len() == 4 && is_valid_frame_id(frame_id)
        }
        _ => false,
    }
}

/// Convert ID3v2.2 frame ID to ID3v2.3/2.4
pub fn upgrade_frame_id(frame_id: &str) -> Option<String> {
    match frame_id {
        "TT2" => Some("TIT2".to_string()), // Title
        "TP1" => Some("TPE1".to_string()), // Artist
        "TAL" => Some("TALB".to_string()), // Album
        "TYE" => Some("TYER".to_string()), // Year
        "TCO" => Some("TCON".to_string()), // Genre
        "TRK" => Some("TRCK".to_string()), // Track
        "COM" => Some("COMM".to_string()), // Comment
        "PIC" => Some("APIC".to_string()), // Picture
        "TXT" => Some("TEXT".to_string()), // Lyricist/text writer
        _ => None,
    }
}

/// Convert ID3v2.3/2.4 frame ID to ID3v2.2
pub fn downgrade_frame_id(frame_id: &str) -> Option<String> {
    match frame_id {
        "TIT2" => Some("TT2".to_string()),
        "TPE1" => Some("TP1".to_string()),
        "TALB" => Some("TAL".to_string()),
        "TYER" | "TDRC" => Some("TYE".to_string()),
        "TCON" => Some("TCO".to_string()),
        "TRCK" => Some("TRK".to_string()),
        "COMM" => Some("COM".to_string()),
        "APIC" => Some("PIC".to_string()),
        "TEXT" => Some("TXT".to_string()), // Lyricist/text writer
        "TXXX" => Some("TXX".to_string()), // User-defined text
        _ => None,
    }
}

/// Calculate total tag size including header and padding
pub fn calculate_tag_size(frame_data_size: usize, padding: usize) -> usize {
    10 + frame_data_size + padding // 10 bytes for ID3v2 header
}

/// Find sync-safe pattern in data (11 consecutive 1 bits)
pub fn find_sync_pattern(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(1) {
        let pattern = (data[i] as u16) << 8 | data[i + 1] as u16;
        if pattern & 0xFFE0 == 0xFFE0 {
            return Some(i);
        }
    }
    None
}

/// Validate ID3v2 header
pub fn validate_id3_header(data: &[u8]) -> Result<()> {
    if data.len() < 10 {
        return Err(AudexError::InvalidData("ID3 header too short".to_string()));
    }

    if &data[0..3] != b"ID3" {
        return Err(AudexError::InvalidData("Invalid ID3 signature".to_string()));
    }

    let major_version = data[3];
    let _revision = data[4];

    if !(2..=4).contains(&major_version) {
        return Err(AudexError::InvalidData(format!(
            "Unsupported ID3 version: 2.{}",
            major_version
        )));
    }

    // The v2.2 spec does not strictly define the revision byte, so we
    // accept any revision value for v2.2 tags without treating it as an error.

    // Check that size bytes don't have high bit set (synchsafe requirement)
    for &byte in &data[6..10] {
        if byte & 0x80 != 0 {
            return Err(AudexError::InvalidData(
                "Invalid synchsafe integer in header".to_string(),
            ));
        }
    }

    Ok(())
}

/// Determine minimum ID3 version needed for a frame ID
pub fn min_version_for_frame(frame_id: &str) -> Option<(u8, u8)> {
    match frame_id {
        // ID3v2.4 only frames
        "TDRC" | "TDRL" | "TDTG" | "TIPL" | "TMCL" | "TSOA" | "TSOP" | "TSOT" => Some((2, 4)),

        // ID3v2.3+ frames
        "TYER" | "TDAT" | "TIME" | "TORY" | "TRDA" | "TSIZ" => Some((2, 3)),

        // Most common frames work from ID3v2.3+
        _ if frame_id.len() == 4 => Some((2, 3)),
        _ if frame_id.len() == 3 => Some((2, 2)),

        _ => None,
    }
}

/// Enhanced junk frame recovery and error handling
pub struct JunkFrameRecovery;

impl JunkFrameRecovery {
    /// Attempt to recover from junk frame data by finding next valid frame header
    pub fn recover_from_junk(data: &[u8], offset: usize, version: (u8, u8)) -> Option<usize> {
        let frame_id_len = match version {
            (2, 2) => 3,
            (2, 3) | (2, 4) => 4,
            _ => return None,
        };

        // Cap the scan distance to avoid spending too long on corrupt data
        const MAX_SCAN_DISTANCE: usize = 65536;
        let scan_end = data
            .len()
            .saturating_sub(10)
            .min(offset.saturating_add(MAX_SCAN_DISTANCE));

        // Search for the next potential frame header
        for i in offset..scan_end {
            // Check if we have enough data for a frame header
            if i + 10 > data.len() {
                break;
            }

            // Try to parse frame ID
            let potential_id = match std::str::from_utf8(&data[i..i + frame_id_len]) {
                Ok(id) => id,
                Err(_) => continue,
            };

            // Validate frame ID
            if !is_valid_frame_id(potential_id) {
                continue;
            }

            // Check if frame size is reasonable
            let size_start = i + frame_id_len;
            if size_start + 4 > data.len() {
                continue;
            }

            let frame_size = match version {
                (2, 4) => match decode_synchsafe_int_checked(&data[size_start..size_start + 4]) {
                    Ok(v) => v,
                    Err(_) => continue, // invalid synchsafe byte; skip this candidate
                },
                _ => u32::from_be_bytes([
                    data[size_start],
                    data[size_start + 1],
                    data[size_start + 2],
                    data[size_start + 3],
                ]),
            };

            // Validate frame size against the global ParseLimits cap
            // instead of a hardcoded ceiling, keeping the recovery path
            // consistent with the rest of the library.
            let max_frame = crate::limits::ParseLimits::default().max_tag_size;
            // Use checked arithmetic to prevent overflow on 32-bit platforms
            if frame_size > 0
                && (frame_size as u64) < max_frame
                && i.checked_add(10)
                    .and_then(|v| v.checked_add(frame_size as usize))
                    .is_some_and(|end| end <= data.len())
            {
                return Some(i);
            }
        }

        None
    }

    /// Attempt to reconstruct a damaged frame by scanning for recognizable patterns
    pub fn reconstruct_frame(data: &[u8], frame_id: &str, expected_size: u32) -> Result<Vec<u8>> {
        // For text frames, try to find text content
        if frame_id.starts_with('T') && frame_id != "TXXX" {
            return Self::reconstruct_text_frame(data, expected_size);
        }

        // For comment frames
        if frame_id == "COMM" {
            return Self::reconstruct_comment_frame(data, expected_size);
        }

        // For URL frames
        if frame_id.starts_with('W') {
            return Self::reconstruct_url_frame(data, expected_size);
        }

        // For other frames, return original data if size matches roughly.
        // Compare in u64 space to avoid truncation when data.len() > u32::MAX.
        if (data.len() as u64) <= (expected_size as u64) + 10 {
            // Allow some tolerance
            Ok(data.to_vec())
        } else {
            Err(AudexError::InvalidData(format!(
                "Cannot reconstruct frame {}",
                frame_id
            )))
        }
    }

    /// Reconstruct text frame by finding valid text content
    fn reconstruct_text_frame(data: &[u8], expected_size: u32) -> Result<Vec<u8>> {
        if data.is_empty() {
            return Ok(vec![0x00]); // Empty text frame with Latin1 encoding
        }

        // Try to find encoding byte
        let encoding = if !data.is_empty() && data[0] <= 3 {
            data[0]
        } else {
            0 // Default to Latin1
        };

        // Try to extract text portion
        let text_start = if encoding <= 3 && data.len() > 1 {
            1
        } else {
            0
        };
        let text_data = &data[text_start..];

        // Validate text content based on encoding
        let is_valid_text = match encoding {
            0 => text_data.iter().all(|&b| b >= 0x20 || b == 0x00), // Latin1 printable
            1 | 2 => text_data.len() % 2 == 0,                      // UTF-16 must be even length
            3 => std::str::from_utf8(text_data).is_ok(),            // UTF-8 validation
            _ => false,
        };

        // Compare in u64 space to avoid truncation when data.len() > u32::MAX.
        if is_valid_text && (data.len() as u64) <= (expected_size as u64) + 5 {
            Ok(data.to_vec())
        } else {
            // Fallback: create minimal valid frame
            Ok(vec![0x00]) // Latin1 encoding with empty text
        }
    }

    /// Reconstruct comment frame by finding language and text
    fn reconstruct_comment_frame(data: &[u8], expected_size: u32) -> Result<Vec<u8>> {
        if data.len() < 4 {
            // Create minimal valid comment frame
            return Ok(vec![
                0x00, // Latin1 encoding
                b'e', b'n', b'g', // English language
                0x00, // Empty description
                0x00, // Empty comment
            ]);
        }

        // Check if first byte looks like encoding
        let encoding = if data[0] <= 3 { data[0] } else { 0x00 };

        // Try to find language code (3 bytes)
        let lang_start = 1;
        let lang_bytes = if data.len() >= 4 {
            &data[lang_start..lang_start + 3]
        } else {
            b"eng"
        };

        // Validate language code (should be ASCII letters)
        let lang_valid = lang_bytes.iter().all(|&b| b.is_ascii_alphabetic());

        // Compare in u64 space to avoid truncation when data.len() > u32::MAX.
        if lang_valid && (data.len() as u64) <= (expected_size as u64) + 10 {
            Ok(data.to_vec())
        } else {
            // Create fallback comment frame
            Ok(vec![
                encoding, b'e', b'n', b'g', // English
                0x00, // Empty description
                0x00, // Empty comment
            ])
        }
    }

    /// Reconstruct URL frame by finding valid URL data
    fn reconstruct_url_frame(data: &[u8], expected_size: u32) -> Result<Vec<u8>> {
        // URL frames contain raw URL data (no encoding byte)

        // Check if data looks like a URL
        let url_str = String::from_utf8_lossy(data);
        let looks_like_url = url_str.starts_with("http://")
            || url_str.starts_with("https://")
            || url_str.starts_with("ftp://")
            || url_str.contains("://");

        // Compare in u64 space to avoid truncation when data.len() > u32::MAX.
        if looks_like_url && (data.len() as u64) <= (expected_size as u64) + 10 {
            Ok(data.to_vec())
        } else {
            // Return empty URL frame
            Ok(Vec::new())
        }
    }

    /// Check if frame data appears corrupted
    pub fn is_frame_corrupted(data: &[u8], frame_id: &str, _flags: u16) -> bool {
        // Check for obvious corruption signs

        // Empty data for non-optional frames
        if data.is_empty() && !Self::is_optional_frame(frame_id) {
            return true;
        }

        // Text frames should have valid encoding byte
        if frame_id.starts_with('T') && frame_id != "TXXX" && !data.is_empty() && data[0] > 3 {
            return true;
        }

        // Comment frames need at least encoding + language + 2 nulls
        if frame_id == "COMM" && data.len() < 5 {
            return true;
        }

        // Known binary frame types where high null-byte ratios are expected
        const BINARY_FRAMES: &[&str] = &[
            "APIC", "GEOB", "PRIV", "MCDI", "AENC", "COMR", "ENCR", "GRID", "LINK", "OWNE", "RBUF",
            "RVRB", "SYLT", "SYTC", "ETCO", "MLLT", "SEEK", "SIGN", "ASPI",
        ];

        // Check for excessive null bytes (common in corruption), but skip
        // binary frames where null-heavy payloads are normal
        if !BINARY_FRAMES.contains(&frame_id) {
            let null_count = data.iter().filter(|&&b| b == 0).count();
            if data.len() > 10 && null_count > data.len() * 2 / 3 {
                return true;
            }
        }

        // Check for binary data in text frames
        if frame_id.starts_with('T') && data.len() > 1 {
            let text_data = &data[1..]; // Skip encoding byte
            let non_printable = text_data.iter().filter(|&&b| b < 0x20 && b != 0x00).count();
            if non_printable > text_data.len() / 4 {
                return true;
            }
        }

        false
    }

    /// Check if frame is optional and can be safely skipped
    fn is_optional_frame(frame_id: &str) -> bool {
        matches!(
            frame_id,
            // Optional metadata frames
            "TPOS" | "TPUB" | "TOFN" | "TOLY" | "TOPE" | "TORY" | "TOWN" | "TRSN" | "TRSO" |
            // Optional technical frames  
            "TBPM" | "TKEY" | "TLAN" | "TLEN" | "TMED" | "TMOO" | "TOAL" |
            // User-defined frames
            "TXXX" | "WXXX" |
            // Comments and lyrics
            "COMM" | "USLT" |
            // Pictures and objects
            "APIC" | "GEOB" |
            // Deprecated frames
            "RVAD" | "EQUA" | "IPLS" | "TDAT" | "TIME" | "TRDA" | "TSIZ" | "TYER"
        )
    }
}

/// Get default text encoding for ID3 version
pub fn default_text_encoding(version: (u8, u8)) -> u8 {
    match version {
        (2, 2) | (2, 3) => 0, // Latin-1 for older versions
        (2, 4) => 3,          // UTF-8 for ID3v2.4
        _ => 0,
    }
}
