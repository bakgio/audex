//! ID3v2 data type specifications comprehensive implementation id3
//!
//! This module provides a complete specification system for ID3v2 frame parsing and serialization,
//! supporting all standard ID3v2 data types with proper encoding, validation, and version compatibility.

use super::util::{
    BitPaddedInt, Unsynch, decode_synchsafe_int_checked, encode_synchsafe_int, is_valid_frame_id,
};
use crate::{AudexError, Result};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use std::io::{Read, Write};

/// Parsed ID3v2 frame header (10 bytes for v2.3/v2.4, 6 bytes for v2.2)
///
/// Contains the frame identifier, payload size, flags (compression,
/// encryption, grouping, unsynchronization), and the ID3 version used
/// for parsing.
#[derive(Debug, Clone)]
pub struct FrameHeader {
    /// Four-character frame identifier (e.g. `"TIT2"`, `"APIC"`)
    pub frame_id: String,
    /// Size of the frame payload in bytes (excluding this header)
    pub size: u32,
    /// Frame flags parsed from the 2-byte flags field
    pub flags: FrameFlags,
    /// ID3v2 version as (major, minor) — e.g. (4, 0) for v2.4
    pub version: (u8, u8),
    /// Whether the global unsynchronization flag is set on the tag header
    pub global_unsync: bool,
}

impl FrameHeader {
    pub fn new(frame_id: String, size: u32, flags: u16, version: (u8, u8)) -> Self {
        Self {
            frame_id,
            size,
            flags: FrameFlags::from_raw(flags, version),
            version,
            global_unsync: false,
        }
    }

    /// Parse frame header from bytes
    pub fn from_bytes(data: &[u8], version: (u8, u8)) -> Result<Self> {
        if data.len() < 10 {
            return Err(AudexError::InvalidData(
                "Frame header too short".to_string(),
            ));
        }

        let frame_id = String::from_utf8(data[0..4].to_vec())
            .map_err(|_| AudexError::InvalidData("Invalid frame ID".to_string()))?;

        if !is_valid_frame_id(&frame_id) {
            return Err(AudexError::InvalidData(format!(
                "Invalid frame ID: {}",
                frame_id
            )));
        }

        let size = match version {
            (2, 4) => decode_synchsafe_int_checked(&data[4..8])?,
            _ => u32::from_be_bytes([data[4], data[5], data[6], data[7]]),
        };

        // Validate the declared frame size against the remaining tag data
        // (excluding the 10-byte header itself) to prevent oversized allocations.
        let remaining_after_header = u32::try_from(data.len() - 10)
            .map_err(|_| AudexError::InvalidData("Frame data exceeds u32 capacity".to_string()))?;
        if size > remaining_after_header {
            return Err(AudexError::InvalidData(format!(
                "Frame '{}' size {} exceeds remaining tag data ({})",
                frame_id, size, remaining_after_header
            )));
        }

        let flags = u16::from_be_bytes([data[8], data[9]]);

        Ok(Self::new(frame_id, size, flags, version))
    }

    /// Convert header to bytes for writing.
    /// Returns an error if the frame size exceeds the synchsafe encoding limit
    /// for ID3v2.4 tags.
    pub fn to_bytes(&self) -> crate::Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(10);
        bytes.extend_from_slice(self.frame_id.as_bytes());

        let size_bytes = match self.version {
            (2, 4) => {
                let encoded = super::util::encode_synchsafe_int(self.size)?;
                encoded.to_vec()
            }
            _ => self.size.to_be_bytes().to_vec(),
        };
        bytes.extend_from_slice(&size_bytes);

        let flags_bytes = self.flags.to_raw(self.version).to_be_bytes();
        bytes.extend_from_slice(&flags_bytes);

        Ok(bytes)
    }

    /// Parse frame header for ID3v2.2 (3-character frame ID).
    ///
    /// The `data` slice should contain the remaining tag data starting at
    /// the frame header position. The frame size is validated against the
    /// available data to prevent oversized allocations from malformed tags.
    pub fn from_bytes_v22(data: &[u8]) -> Result<Self> {
        if data.len() < 6 {
            return Err(AudexError::InvalidData(
                "ID3v2.2 frame header too short".to_string(),
            ));
        }

        let frame_id = String::from_utf8(data[0..3].to_vec())
            .map_err(|_| AudexError::InvalidData("Invalid frame ID".to_string()))?;

        if !crate::id3::util::is_valid_frame_id(&frame_id) {
            return Err(AudexError::InvalidData(format!(
                "Invalid frame ID: {}",
                frame_id
            )));
        }

        let size = (data[3] as u32) << 16 | (data[4] as u32) << 8 | (data[5] as u32);

        // Validate the declared frame size against the remaining tag data
        // (excluding the 6-byte header itself). A 3-byte size field can hold
        // up to ~16 MB, which could trigger a large allocation if unchecked.
        let remaining_after_header = u32::try_from(data.len() - 6).map_err(|_| {
            AudexError::InvalidData("ID3v2.2 frame data exceeds u32 capacity".to_string())
        })?;
        if size > remaining_after_header {
            return Err(AudexError::InvalidData(format!(
                "ID3v2.2 frame '{}' size {} exceeds remaining tag data ({})",
                frame_id, size, remaining_after_header
            )));
        }

        Ok(Self {
            frame_id,
            size,
            flags: FrameFlags::new(), // No flags in ID3v2.2
            version: (2, 2),
            global_unsync: false,
        })
    }

    /// Parse frame header for ID3v2.3
    pub fn from_bytes_v23(data: &[u8]) -> Result<Self> {
        Self::from_bytes(data, (2, 3))
    }

    /// Parse frame header for ID3v2.4
    pub fn from_bytes_v24(data: &[u8]) -> Result<Self> {
        Self::from_bytes(data, (2, 4))
    }

    /// Parse frame header for ID3v2.4 with a custom size parser.
    /// Used by determine_bpi to handle iTunes files that use regular ints
    /// instead of synchsafe ints for frame sizes.
    pub fn from_bytes_v24_with_size_parser(
        data: &[u8],
        size_parser: fn(&[u8]) -> Result<u32>,
    ) -> Result<Self> {
        if data.len() < 10 {
            return Err(AudexError::InvalidData(
                "Frame header too short".to_string(),
            ));
        }

        let frame_id = String::from_utf8(data[0..4].to_vec())
            .map_err(|_| AudexError::InvalidData("Invalid frame ID".to_string()))?;

        if !is_valid_frame_id(&frame_id) {
            return Err(AudexError::InvalidData(format!(
                "Invalid frame ID: {}",
                frame_id
            )));
        }

        let size = size_parser(&data[4..8])?;

        // Validate the declared frame size against the remaining tag data
        // (excluding the 10-byte header) to prevent oversized allocations
        let remaining_after_header = u32::try_from(data.len() - 10)
            .map_err(|_| AudexError::InvalidData("Frame data exceeds u32 capacity".to_string()))?;
        if size > remaining_after_header {
            return Err(AudexError::InvalidData(format!(
                "Frame '{}' size {} exceeds remaining tag data ({})",
                frame_id, size, remaining_after_header
            )));
        }

        let flags = u16::from_be_bytes([data[8], data[9]]);

        Ok(Self::new(frame_id, size, flags, (2, 4)))
    }
}

/// Frame flags structure handling both ID3v2.3 and ID3v2.4
#[derive(Debug, Clone)]
/// Parsed frame flags from the 2-byte flags field in ID3v2.3/v2.4 headers
///
/// Flags control how the frame data should be interpreted (compression,
/// unsynchronization) and what should happen when the tag or file is modified
/// (discard on alter). v2.2 frames have no flags.
pub struct FrameFlags {
    /// Frame should be discarded if the tag is altered
    pub alter_tag: bool,
    /// Frame should be discarded if the file (audio) is altered
    pub alter_file: bool,
    /// Frame is read-only and should not be modified
    pub read_only: bool,
    /// Frame belongs to a group (has a group ID byte prefix)
    pub group_id: bool,
    /// Frame data is compressed with zlib
    pub compression: bool,
    /// Frame data is encrypted (not supported by this library)
    pub encryption: bool,
    /// Frame has per-frame unsynchronization applied (v2.4 only)
    pub unsync: bool,
    /// Frame has a 4-byte data length indicator prefix (v2.4 only)
    pub data_length: bool,
}

impl FrameFlags {
    /// Create a new `FrameFlags` with all flags cleared (no special processing).
    pub fn new() -> Self {
        Self {
            alter_tag: false,
            alter_file: false,
            read_only: false,
            group_id: false,
            compression: false,
            encryption: false,
            unsync: false,
            data_length: false,
        }
    }

    pub fn from_raw(flags: u16, version: (u8, u8)) -> Self {
        match version {
            (2, 3) => Self::from_v23(flags),
            (2, 4) => Self::from_v24(flags),
            _ => Self::new(),
        }
    }

    fn from_v23(flags: u16) -> Self {
        Self {
            alter_tag: (flags & 0x8000) != 0,
            alter_file: (flags & 0x4000) != 0,
            read_only: (flags & 0x2000) != 0,
            group_id: (flags & 0x0020) != 0,
            compression: (flags & 0x0080) != 0,
            encryption: (flags & 0x0040) != 0,
            unsync: false,      // Not supported in ID3v2.3
            data_length: false, // Not supported in ID3v2.3
        }
    }

    fn from_v24(flags: u16) -> Self {
        Self {
            alter_tag: (flags & 0x4000) != 0,
            alter_file: (flags & 0x2000) != 0,
            read_only: (flags & 0x1000) != 0,
            group_id: (flags & 0x0040) != 0,
            compression: (flags & 0x0008) != 0,
            encryption: (flags & 0x0004) != 0,
            unsync: (flags & 0x0002) != 0,
            data_length: (flags & 0x0001) != 0,
        }
    }

    pub fn to_raw(&self, version: (u8, u8)) -> u16 {
        match version {
            (2, 3) => self.to_v23(),
            (2, 4) => self.to_v24(),
            _ => 0,
        }
    }

    fn to_v23(&self) -> u16 {
        let mut flags = 0u16;
        if self.alter_tag {
            flags |= 0x8000;
        }
        if self.alter_file {
            flags |= 0x4000;
        }
        if self.read_only {
            flags |= 0x2000;
        }
        if self.group_id {
            flags |= 0x0020;
        }
        if self.compression {
            flags |= 0x0080;
        }
        if self.encryption {
            flags |= 0x0040;
        }
        flags
    }

    fn to_v24(&self) -> u16 {
        let mut flags = 0u16;
        if self.alter_tag {
            flags |= 0x4000;
        }
        if self.alter_file {
            flags |= 0x2000;
        }
        if self.read_only {
            flags |= 0x1000;
        }
        if self.group_id {
            flags |= 0x0040;
        }
        if self.compression {
            flags |= 0x0008;
        }
        if self.encryption {
            flags |= 0x0004;
        }
        if self.unsync {
            flags |= 0x0002;
        }
        if self.data_length {
            flags |= 0x0001;
        }
        flags
    }

    /// Validate flags for specific ID3 version
    pub fn validate(&self, version: (u8, u8)) -> Result<()> {
        if self.encryption {
            return Err(AudexError::UnsupportedFormat(
                "Frame encryption is not supported".to_string(),
            ));
        }

        match version {
            (2, 3) => {
                if self.unsync {
                    return Err(AudexError::InvalidData(
                        "Unsynchronization flag not valid in ID3v2.3".to_string(),
                    ));
                }
                if self.data_length {
                    return Err(AudexError::InvalidData(
                        "Data length flag not valid in ID3v2.3".to_string(),
                    ));
                }
            }
            (2, 4) => {
                // All flags are valid in ID3v2.4
            }
            (2, 2) => {
                // ID3v2.2 has no per-frame flags; silently ignore any that
                // were carried over from a higher-version source frame.
            }
            _ => {
                return Err(AudexError::UnsupportedFormat(format!(
                    "Unsupported ID3 version: {}.{}",
                    version.0, version.1
                )));
            }
        }

        Ok(())
    }
}

impl Default for FrameFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// Frame data processing utilities
/// Applies frame-level transformations during read and write operations.
///
/// On read: strips group ID bytes, removes unsynchronization, decompresses
/// zlib data, and validates data length indicators.
/// On write: applies compression, unsynchronization, and adds data length
/// indicators as needed based on the frame flags.
pub struct FrameProcessor;

impl FrameProcessor {
    /// Process frame data based on flags (decompress, remove unsync, etc.)
    pub fn process_read(header: &FrameHeader, mut data: Vec<u8>) -> Result<Vec<u8>> {
        let flags = &header.flags;

        // Validate flags
        flags.validate(header.version)?;

        // Handle group ID - strip the group identifier byte first.
        // Per the ID3v2.4 spec, the group identifier precedes the data
        // length indicator in the frame's additional info area.
        if flags.group_id {
            if data.is_empty() {
                return Err(AudexError::InvalidData("Missing group ID byte".to_string()));
            }
            data = data[1..].to_vec(); // Skip group ID byte
        }

        // Handle data length indicator (ID3v2.4) - follows group ID in the stream.
        // The spec encodes this as a syncsafe integer (7 bits per byte).
        // Save the raw bytes for the compression fallback path.
        let mut datalen_saved: Option<Vec<u8>> = None;
        let mut indicated_length: Option<usize> = None;
        if flags.data_length && header.version == (2, 4) {
            if data.len() < 4 {
                return Err(AudexError::InvalidData(
                    "Missing data length indicator".to_string(),
                ));
            }
            indicated_length =
                Some(super::util::decode_synchsafe_int_checked(&data[0..4])? as usize);
            datalen_saved = Some(data[..4].to_vec());
            data = data[4..].to_vec();
        }

        // Handle unsynchronization - must happen before decompression.
        // Per-frame unsync (flags.unsync) is an ID3v2.4-only feature.
        // Global unsync (header.global_unsync) applies ONLY to v2.3;
        // in v2.4, unsynchronization is handled per-frame via flags.unsync.
        let needs_unsync = if header.global_unsync && header.version.1 == 3 {
            // v2.3: global unsync is the only mechanism (no per-frame flags)
            true
        } else {
            // v2.4: only per-frame unsync flags apply; global flag is ignored
            flags.unsync && header.version == (2, 4)
        };
        if needs_unsync {
            data = Unsynch::decode(&data).map_err(|_| {
                AudexError::InvalidData(format!(
                    "Frame '{}': unsynchronization decode failed on corrupt data",
                    header.frame_id,
                ))
            })?;
        }

        // For v2.3, compressed frames have a 4-byte uncompressed size prepended.
        // Save the declared size so we can validate it after decompression.
        // Also save the raw prefix bytes for use in the decompression fallback,
        // since v2.3 frames lack a data length indicator (that's a v2.4 feature).
        let mut declared_uncompressed_size: Option<u32> = None;
        let mut v23_size_prefix: Option<Vec<u8>> = None;
        if flags.compression && header.version == (2, 3) {
            if data.len() < 4 {
                return Err(AudexError::InvalidData(
                    "Compressed frame too short for size prefix".to_string(),
                ));
            }
            declared_uncompressed_size =
                Some(u32::from_be_bytes([data[0], data[1], data[2], data[3]]));
            v23_size_prefix = Some(data[..4].to_vec());
            data = data[4..].to_vec();
        }

        // Handle compression - after unsync removal (with fallback for BUG H8)
        if flags.compression {
            match Self::decompress_zlib(&data) {
                Ok(decompressed) => data = decompressed,
                Err(_) => {
                    // Fallback: some old taggers didn't write the uncompressed size
                    // correctly, so try prepending the stripped prefix bytes back.
                    // For v2.4, use the data length indicator; for v2.3, use the
                    // 4-byte uncompressed size prefix that was stripped earlier.
                    let fallback_bytes = datalen_saved.as_ref().or(v23_size_prefix.as_ref());
                    if let Some(prefix) = fallback_bytes {
                        let mut retry_data = prefix.clone();
                        retry_data.extend_from_slice(&data);
                        data = Self::decompress_zlib(&retry_data)?;
                    } else {
                        return Err(AudexError::InvalidData(
                            "Failed to decompress frame".to_string(),
                        ));
                    }
                }
            }

            // Validate the decompressed output against the declared size.
            // A mismatch may indicate a zip bomb or corrupt frame data.
            if let Some(declared) = declared_uncompressed_size {
                if data.len() != declared as usize {
                    return Err(AudexError::InvalidData(format!(
                        "Decompressed size ({}) does not match declared uncompressed size ({})",
                        data.len(),
                        declared
                    )));
                }
            }

            // ID3v2.4 spec requires compressed frames to carry a data length
            // indicator so the decompressed size can be validated. When both
            // declared sizes are absent we reject the frame rather than
            // silently accepting unvalidated data.
            if declared_uncompressed_size.is_none() && indicated_length.is_none() {
                return Err(AudexError::InvalidData(format!(
                    "Frame '{}': compressed without a declared size or data length indicator; \
                     decompressed output ({} bytes) cannot be validated against a known size",
                    header.frame_id,
                    data.len()
                )));
            }
        }

        if let Some(indicated_length) = indicated_length {
            if data.len() != indicated_length {
                return Err(AudexError::InvalidData(format!(
                    "Frame '{}' data length ({}) does not match indicated length ({})",
                    header.frame_id,
                    data.len(),
                    indicated_length
                )));
            }
        }

        Ok(data)
    }

    /// Process frame data for writing (compress, add unsync, etc.)
    pub fn process_write(header: &FrameHeader, mut data: Vec<u8>) -> Result<Vec<u8>> {
        let flags = &header.flags;

        // Validate flags
        flags.validate(header.version)?;

        let original_length = data.len();

        // Handle compression - apply first
        if flags.compression {
            data = Self::compress_zlib(&data)?;
        }

        // Handle unsynchronization (ID3v2.4 only) - apply after compression
        if flags.unsync && header.version == (2, 4) {
            data = Unsynch::encode(&data);
        }

        // Handle data length indicator (ID3v2.4) - prepend before group ID.
        // The spec requires this to be a syncsafe integer (7 bits per byte).
        // In the final byte stream, group_id comes first, then data_length,
        // so we prepend data_length first, then group_id in front of it.
        if flags.data_length && header.version == (2, 4) {
            let original_length_u32 = u32::try_from(original_length).map_err(|_| {
                AudexError::InvalidData(format!(
                    "Frame data length {} exceeds u32 maximum",
                    original_length
                ))
            })?;
            let syncsafe = super::util::encode_synchsafe_int(original_length_u32)?;
            let mut new_data = syncsafe.to_vec();
            new_data.extend(data);
            data = new_data;
        }

        // Handle group ID - prepend group ID byte in front of everything.
        // Per the ID3v2.4 spec, the group identifier byte precedes the
        // data length indicator in the frame's additional info area.
        //
        // NOTE: The actual group ID value is stripped during process_read and
        // not preserved in FrameFlags (which only stores a bool indicating
        // presence). Until FrameFlags or FrameHeader is extended to store the
        // original byte, round-tripping will reset the group ID to 0x00.
        // This is a known limitation -- see FrameFlags.group_id.
        if flags.group_id {
            let mut new_data = vec![0x00];
            new_data.extend(data);
            data = new_data;
        }

        Ok(data)
    }

    /// Compress data using zlib
    fn compress_zlib(data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(data)
            .map_err(|e| AudexError::InvalidData(format!("Compression failed: {}", e)))?;
        encoder
            .finish()
            .map_err(|e| AudexError::InvalidData(format!("Compression failed: {}", e)))
    }

    /// Decompress data using zlib.
    ///
    /// Caps the decompressed output at 32 MB to prevent zip-bomb payloads from
    /// causing an out-of-memory condition. 32 MB is far beyond any realistic
    /// single-frame payload (cover art, lyrics, etc.) while still blocking
    /// adversarial inputs that expand a few hundred bytes into gigabytes.
    fn decompress_zlib(data: &[u8]) -> Result<Vec<u8>> {
        const MAX_DECOMPRESSED_SIZE: u64 = 32 * 1024 * 1024;

        let decoder = ZlibDecoder::new(data);
        let mut limited = decoder.take(MAX_DECOMPRESSED_SIZE + 1);
        let mut decompressed = Vec::new();
        limited
            .read_to_end(&mut decompressed)
            .map_err(|e| AudexError::InvalidData(format!("Decompression failed: {}", e)))?;

        if decompressed.len() as u64 > MAX_DECOMPRESSED_SIZE {
            return Err(AudexError::InvalidData(format!(
                "Decompressed frame data exceeds {} MB limit",
                MAX_DECOMPRESSED_SIZE / (1024 * 1024)
            )));
        }

        Ok(decompressed)
    }
}

/// Configuration for ID3 frame specification writing
#[derive(Debug, Clone)]
pub struct FrameWriteConfig {
    pub version: (u8, u8),
    pub use_synchsafe_ints: bool,
    pub default_encoding: TextEncoding,
    pub v23_separator: u8,
}

impl Default for FrameWriteConfig {
    fn default() -> Self {
        Self {
            version: (2, 4),
            use_synchsafe_ints: true,
            default_encoding: TextEncoding::Utf8,
            v23_separator: b'/', // ID3v2.3 uses "/" for multi-value text fields
        }
    }
}

/// Frame data container for specification processing
#[derive(Debug, Clone)]
pub struct FrameData {
    pub frame_id: String,
    pub size: u32,
    pub flags: u16,
    pub version: (u8, u8),
}

impl FrameData {
    pub fn new(frame_id: String, size: u32, flags: u16, version: (u8, u8)) -> Self {
        Self {
            frame_id,
            size,
            flags,
            version,
        }
    }

    pub fn is_v23(&self) -> bool {
        self.version.0 == 2 && self.version.1 == 3
    }

    pub fn is_v24(&self) -> bool {
        self.version.0 == 2 && self.version.1 == 4
    }
}

/// Text encoding types supported by ID3v2 tags
///
/// # Version Compatibility
///
/// | Encoding | ID3v2.3 | ID3v2.4 |
/// |----------|---------|---------|
/// | LATIN1   | ✅      | ✅      |
/// | UTF-16   | ✅      | ✅      |
/// | UTF-16BE | ❌      | ✅      |
/// | UTF-8    | ❌      | ✅      |
///
/// When saving as ID3v2.3, incompatible encodings (UTF-8 and UTF-16BE) are
/// automatically converted to UTF-16 to maintain specification compliance.
///
/// # Default Encoding
///
/// All text frames default to UTF-16 with BOM, which provides maximum
/// compatibility across all ID3v2 versions while supporting full Unicode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextEncoding {
    /// ISO-8859-1 (LATIN1) - Single-byte encoding for Western European characters
    ///
    /// Compatible with all ID3v2 versions. Most compact for ASCII text but
    /// limited character set.
    Latin1 = 0,

    /// UTF-16 with BOM (default) - Variable-width Unicode encoding
    ///
    /// The default encoding for all text frames. Compatible with all ID3v2 versions.
    /// Automatically includes byte-order mark (BOM) to indicate endianness.
    #[default]
    Utf16 = 1,

    /// UTF-16 big-endian without BOM - ID3v2.4 only
    ///
    /// Only valid in ID3v2.4 and later. Automatically converted to UTF-16 with BOM
    /// when saving as ID3v2.3.
    Utf16Be = 2,

    /// UTF-8 - ID3v2.4 only
    ///
    /// Variable-width Unicode encoding. Only valid in ID3v2.4 and later.
    /// Automatically converted to UTF-16 with BOM when saving as ID3v2.3.
    /// More space-efficient than UTF-16 for mostly-ASCII text.
    Utf8 = 3,
}

impl TextEncoding {
    pub fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            0 => Ok(TextEncoding::Latin1),
            1 => Ok(TextEncoding::Utf16),
            2 => Ok(TextEncoding::Utf16Be),
            3 => Ok(TextEncoding::Utf8),
            _ => Err(AudexError::InvalidData(format!(
                "Invalid text encoding: {}",
                byte
            ))),
        }
    }

    pub fn to_byte(self) -> u8 {
        self as u8
    }

    /// Get the null terminator for this encoding
    pub fn null_terminator(&self) -> &'static [u8] {
        match self {
            TextEncoding::Latin1 | TextEncoding::Utf8 => b"\x00",
            TextEncoding::Utf16 | TextEncoding::Utf16Be => b"\x00\x00",
        }
    }

    /// Check if this encoding is valid for the given ID3 version
    pub fn is_valid_for_version(&self, version: (u8, u8)) -> bool {
        match self {
            // UTF-8 and UTF-16BE are only valid in ID3v2.4 and later
            TextEncoding::Utf8 | TextEncoding::Utf16Be => version >= (2, 4),
            // LATIN1 and UTF-16 (with BOM) are valid in all versions
            _ => true,
        }
    }

    /// Convert text to bytes using this encoding
    pub fn encode_text(&self, text: &str) -> Result<Vec<u8>> {
        match self {
            TextEncoding::Latin1 => {
                // Check if all characters can be represented in Latin1
                for ch in text.chars() {
                    if ch as u32 > 255 {
                        return Err(AudexError::InvalidData(format!(
                            "Character '{}' cannot be encoded in Latin1",
                            ch
                        )));
                    }
                }
                // Convert each character to its Latin-1 byte value (0-255)
                // NOT the UTF-8 byte representation
                Ok(text.chars().map(|c| c as u8).collect())
            }
            TextEncoding::Utf8 => Ok(text.as_bytes().to_vec()),
            TextEncoding::Utf16 => {
                let mut bytes = vec![0xFF, 0xFE]; // Little-endian BOM
                for ch in text.encode_utf16() {
                    bytes.extend_from_slice(&ch.to_le_bytes());
                }
                Ok(bytes)
            }
            TextEncoding::Utf16Be => {
                let mut bytes = Vec::new();
                for ch in text.encode_utf16() {
                    bytes.extend_from_slice(&ch.to_be_bytes());
                }
                Ok(bytes)
            }
        }
    }

    /// Decode bytes to text using this encoding
    pub fn decode_text(&self, data: &[u8]) -> Result<String> {
        match self {
            TextEncoding::Latin1 => Ok(data.iter().map(|&b| b as char).collect()),
            TextEncoding::Utf8 => String::from_utf8(data.to_vec())
                .map_err(|e| AudexError::InvalidData(format!("Invalid UTF-8: {}", e))),
            TextEncoding::Utf16 => Self::decode_utf16(data, true),
            TextEncoding::Utf16Be => Self::decode_utf16(data, false),
        }
    }

    fn decode_utf16(data: &[u8], detect_bom: bool) -> Result<String> {
        if data.len() < 2 {
            return Ok(String::new());
        }

        let (data, little_endian) = if detect_bom {
            if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xFE {
                (&data[2..], true)
            } else if data.len() >= 2 && data[0] == 0xFE && data[1] == 0xFF {
                (&data[2..], false)
            } else {
                (data, true) // Default to little-endian if no BOM (most real-world tags are LE)
            }
        } else {
            // Even without BOM detection, a UTF-16BE stream may carry a
            // big-endian BOM (0xFE 0xFF). Strip it so the decoded text
            // does not start with U+FEFF (zero-width no-break space).
            if data.len() >= 2 && data[0] == 0xFE && data[1] == 0xFF {
                (&data[2..], false)
            } else {
                (data, false) // Big-endian without BOM
            }
        };

        // UTF-16 requires an even number of bytes. If the data has an odd
        // length, the trailing orphan byte cannot form a valid code unit.
        // Truncate it rather than padding with 0x00 (which would create a
        // phantom character that isn't in the original data).
        let data = if data.len() % 2 != 0 {
            data[..data.len() - 1].to_vec()
        } else {
            data.to_vec()
        };

        let mut utf16_chars = Vec::new();
        for chunk in data.chunks(2) {
            let ch = if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            };
            utf16_chars.push(ch);
        }

        String::from_utf16(&utf16_chars)
            .map_err(|e| AudexError::InvalidData(format!("Invalid UTF-16: {}", e)))
    }
}

impl TryFrom<u8> for TextEncoding {
    type Error = AudexError;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        Self::from_byte(value)
    }
}

/// ID3v2.4 timestamp format
#[derive(Debug, Clone, PartialEq)]
pub struct ID3TimeStamp {
    pub text: String,
    pub year: Option<u16>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub second: Option<u8>,
}

impl ID3TimeStamp {
    pub fn new(text: String) -> Self {
        Self {
            text,
            year: None,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        }
    }

    pub fn parse(text: &str) -> Self {
        // ID3v2.4 timestamp format: YYYY[-MM[-DD[THH[:MM[:SS]]]]]
        let mut timestamp = Self::new(text.to_string());

        // Timestamp fields use fixed byte offsets; reject non-ASCII input
        // to avoid panicking on multi-byte character boundaries.
        if !text.is_ascii() {
            return timestamp;
        }

        if text.len() >= 4 {
            if let Ok(year) = text[0..4].parse() {
                timestamp.year = Some(year);
            }
        }

        if text.len() >= 7 && text.as_bytes()[4] == b'-' {
            if let Ok(month) = text[5..7].parse::<u8>() {
                // Month must be in the range 1-12
                if (1..=12).contains(&month) {
                    timestamp.month = Some(month);
                }
            }
        }

        if text.len() >= 10 && text.as_bytes()[7] == b'-' {
            if let Ok(day) = text[8..10].parse::<u8>() {
                // Day must be in the range 1-31
                if (1..=31).contains(&day) {
                    timestamp.day = Some(day);
                }
            }
        }

        if text.len() >= 13 && text.as_bytes()[10] == b'T' {
            if let Ok(hour) = text[11..13].parse::<u8>() {
                // Hour must be in the range 0-23
                if hour <= 23 {
                    timestamp.hour = Some(hour);
                }
            }
        }

        if text.len() >= 16 && text.as_bytes()[13] == b':' {
            if let Ok(minute) = text[14..16].parse::<u8>() {
                // Minute must be in the range 0-59
                if minute <= 59 {
                    timestamp.minute = Some(minute);
                }
            }
        }

        if text.len() >= 19 && text.as_bytes()[16] == b':' {
            if let Ok(second) = text[17..19].parse::<u8>() {
                // Second must be in the range 0-59
                if second <= 59 {
                    timestamp.second = Some(second);
                }
            }
        }

        timestamp
    }
}

/// Base trait for all ID3 specifications
///
/// This trait defines the core interface that all specification types must implement
/// for reading, writing, and validating ID3 frame data.
pub trait Spec {
    type Value: Clone;

    /// Read data from bytes using this specification
    fn read(
        &self,
        header: &FrameHeader,
        frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)>;

    /// Write value to bytes using this specification
    fn write(
        &self,
        config: &FrameWriteConfig,
        frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>>;

    /// Validate value for this specification
    fn validate(&self, frame: &FrameData, value: Self::Value) -> Result<Self::Value>;

    /// Validate value for ID3v2.3 compatibility
    fn validate23(&self, frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        // Default implementation just calls validate
        self.validate(frame, value)
    }

    /// Get the name of this specification (for debugging)
    fn name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Error type for specification-related errors
#[derive(Debug, Clone)]
pub struct SpecError {
    pub message: String,
    pub spec_name: String,
}

impl SpecError {
    pub fn new(spec_name: &str, message: String) -> Self {
        Self {
            message,
            spec_name: spec_name.to_string(),
        }
    }
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Spec error in {}: {}", self.spec_name, self.message)
    }
}

impl std::error::Error for SpecError {}

/// Convert SpecError to AudexError
impl From<SpecError> for AudexError {
    fn from(err: SpecError) -> Self {
        AudexError::InvalidData(err.to_string())
    }
}

/// Single byte specification
#[derive(Debug, Clone)]
pub struct ByteSpec {
    pub name: String,
    pub default: u8,
}

impl ByteSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            default: 0,
        }
    }

    pub fn with_default(name: &str, default: u8) -> Self {
        Self {
            name: name.to_string(),
            default,
        }
    }
}

impl Spec for ByteSpec {
    type Value = u8;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.is_empty() {
            return Ok((self.default, 0));
        }
        Ok((data[0], 1))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        Ok(vec![*value])
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// Text encoding specification
#[derive(Debug, Clone)]
pub struct EncodingSpec {
    pub name: String,
}

impl EncodingSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for EncodingSpec {
    type Value = TextEncoding;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.is_empty() {
            return Ok((TextEncoding::default(), 0));
        }
        let encoding = TextEncoding::from_byte(data[0])?;
        Ok((encoding, 1))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        Ok(vec![value.to_byte()])
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }

    fn validate23(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        // ID3v2.3 only supports LATIN1 (0) and UTF-16 with BOM (1)
        // UTF-8 (3) and UTF-16BE (2) were added in ID3v2.4
        match value {
            TextEncoding::Utf8 | TextEncoding::Utf16Be => Ok(TextEncoding::Utf16),
            _ => Ok(value),
        }
    }
}

/// Fixed-size ASCII string specification
#[derive(Debug, Clone)]
pub struct StringSpec {
    pub name: String,
    pub size: usize,
}

impl StringSpec {
    pub fn new(name: &str, size: usize) -> Self {
        Self {
            name: name.to_string(),
            size,
        }
    }
}

impl Spec for StringSpec {
    type Value = String;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        let bytes_to_read = std::cmp::min(self.size, data.len());
        let text_data = &data[..bytes_to_read];

        // Find null terminator or use all bytes
        let end = text_data
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(text_data.len());

        // Validate UTF-8 properly - should fail on invalid UTF-8
        let text = String::from_utf8(text_data[..end].to_vec())
            .map_err(|_| SpecError::new("StringSpec", "Invalid UTF-8 data".to_string()))?;
        Ok((text, self.size))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let bytes = value.as_bytes();
        let copy_len = std::cmp::min(bytes.len(), self.size);

        if bytes.len() >= self.size {
            // If string is longer than or equal to size, truncate to exact size
            Ok(bytes[..copy_len].to_vec())
        } else {
            // If string is shorter, pad with null bytes to exact size
            let mut data = vec![0u8; self.size];
            data[..copy_len].copy_from_slice(&bytes[..copy_len]);
            Ok(data)
        }
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        // Validate exact length requirement
        if value.len() != self.size {
            return Err(SpecError::new(
                "StringSpec",
                format!(
                    "String length {} does not match required size {}",
                    value.len(),
                    self.size
                ),
            )
            .into());
        }

        // Validate ASCII-only characters
        if !value.is_ascii() {
            return Err(SpecError::new(
                "StringSpec",
                format!("Non-ASCII characters in string: {}", value),
            )
            .into());
        }

        Ok(value)
    }
}

/// Frame ID specification extending StringSpec with validation
#[derive(Debug, Clone)]
pub struct FrameIDSpec {
    pub name: String,
    pub length: usize,
}

impl FrameIDSpec {
    pub fn new(name: &str, length: usize) -> Self {
        Self {
            name: name.to_string(),
            length,
        }
    }
}

impl Spec for FrameIDSpec {
    type Value = String;

    fn read(
        &self,
        header: &FrameHeader,
        frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        // Use StringSpec's read behavior
        let string_spec = StringSpec::new(&self.name, self.length);
        string_spec.read(header, frame, data)
    }

    fn write(
        &self,
        config: &FrameWriteConfig,
        frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        // Use StringSpec's write behavior
        let string_spec = StringSpec::new(&self.name, self.length);
        string_spec.write(config, frame, value)
    }

    fn validate(&self, frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        // First validate using StringSpec
        let string_spec = StringSpec::new(&self.name, self.length);
        let validated_value = string_spec.validate(frame, value)?;

        // Then validate frame ID specifically
        if !is_valid_frame_id(&validated_value) {
            return Err(SpecError::new("FrameIDSpec", "Invalid frame ID".to_string()).into());
        }

        Ok(validated_value)
    }
}

/// Raw binary data specification
#[derive(Debug, Clone)]
pub struct BinaryDataSpec {
    pub name: String,
}

impl BinaryDataSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for BinaryDataSpec {
    type Value = Vec<u8>;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        Ok((data.to_vec(), data.len()))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        Ok(value.clone())
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// Encoding-aware null terminator specification
#[derive(Debug, Clone)]
pub struct TerminatorSpec {
    pub name: String,
    pub encoding: TextEncoding,
}

impl TerminatorSpec {
    pub fn new(name: &str, encoding: TextEncoding) -> Self {
        Self {
            name: name.to_string(),
            encoding,
        }
    }
}

impl Spec for TerminatorSpec {
    type Value = Vec<u8>;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        let terminator = self.encoding.null_terminator();
        if data.len() >= terminator.len() && &data[..terminator.len()] == terminator {
            Ok((terminator.to_vec(), terminator.len()))
        } else {
            Ok((vec![], 0))
        }
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        _value: &Self::Value,
    ) -> Result<Vec<u8>> {
        Ok(self.encoding.null_terminator().to_vec())
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// Encoded text specification with encoding-dependent serialization
#[derive(Debug, Clone)]
pub struct EncodedTextSpec {
    pub name: String,
}

impl EncodedTextSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for EncodedTextSpec {
    type Value = String;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.is_empty() {
            return Ok((String::new(), 0));
        }

        // First byte is encoding
        let encoding = TextEncoding::from_byte(data[0])?;
        let text_data = &data[1..];

        // Find null terminator based on encoding.
        // For UTF-16 encodings, the search must step by 2 bytes to stay
        // aligned with character boundaries. A byte-level search can
        // produce false matches when adjacent characters happen to have
        // a 0x00 byte at their boundary.
        let terminator = encoding.null_terminator();
        let end = if terminator.len() == 2 {
            let mut pos = 0;
            let mut found = None;
            while pos + 1 < text_data.len() {
                if text_data[pos] == 0 && text_data[pos + 1] == 0 {
                    found = Some(pos);
                    break;
                }
                pos += 2;
            }
            found.unwrap_or(text_data.len())
        } else {
            text_data
                .windows(terminator.len())
                .position(|window| window == terminator)
                .unwrap_or(text_data.len())
        };

        let text = encoding.decode_text(&text_data[..end])?;

        // Calculate consumed bytes: encoding byte + text bytes + null terminator (if present)
        let consumed = if end < text_data.len() {
            // Null terminator was found
            1 + end + encoding.null_terminator().len()
        } else {
            // No null terminator, consume all data
            data.len()
        };

        Ok((text, consumed))
    }

    fn write(
        &self,
        config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let encoding = config.default_encoding;
        let mut result = vec![encoding.to_byte()];
        result.extend_from_slice(&encoding.encode_text(value)?);
        result.extend_from_slice(encoding.null_terminator());
        Ok(result)
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// Encoded numeric text specification for numeric text fields
#[derive(Debug, Clone)]
pub struct EncodedNumericTextSpec {
    pub name: String,
}

impl EncodedNumericTextSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for EncodedNumericTextSpec {
    type Value = String;

    fn read(
        &self,
        header: &FrameHeader,
        frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        let encoded_text_spec = EncodedTextSpec::new(&self.name);
        encoded_text_spec.read(header, frame, data)
    }

    fn write(
        &self,
        config: &FrameWriteConfig,
        frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let encoded_text_spec = EncodedTextSpec::new(&self.name);
        encoded_text_spec.write(config, frame, value)
    }

    fn validate(&self, frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        let encoded_text_spec = EncodedTextSpec::new(&self.name);
        encoded_text_spec.validate(frame, value)
    }
}

/// Encoded numeric part text specification for track numbers and similar fields
#[derive(Debug, Clone)]
pub struct EncodedNumericPartTextSpec {
    pub name: String,
}

impl EncodedNumericPartTextSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for EncodedNumericPartTextSpec {
    type Value = String;

    fn read(
        &self,
        header: &FrameHeader,
        frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        let encoded_text_spec = EncodedTextSpec::new(&self.name);
        encoded_text_spec.read(header, frame, data)
    }

    fn write(
        &self,
        config: &FrameWriteConfig,
        frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let encoded_text_spec = EncodedTextSpec::new(&self.name);
        encoded_text_spec.write(config, frame, value)
    }

    fn validate(&self, frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        let encoded_text_spec = EncodedTextSpec::new(&self.name);
        encoded_text_spec.validate(frame, value)
    }
}

/// Fixed Latin-1 text specification
#[derive(Debug, Clone)]
pub struct Latin1TextSpec {
    pub name: String,
}

impl Latin1TextSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for Latin1TextSpec {
    type Value = String;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        // Find null terminator
        let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        let text = TextEncoding::Latin1.decode_text(&data[..end])?;
        let consumed = if end < data.len() { end + 1 } else { end };
        Ok((text, consumed))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let mut result = TextEncoding::Latin1.encode_text(value)?;
        result.push(0); // Null terminator
        Ok(result)
    }

    fn validate(&self, frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        // Validate that all characters can be encoded in Latin-1
        for ch in value.chars() {
            if ch as u32 > 255 {
                return Err(SpecError::new(
                    "Latin1TextSpec",
                    format!("Character '{}' cannot be encoded in Latin-1", ch),
                )
                .into());
            }
        }

        // Special validation for MIME types in APIC frames
        if frame.frame_id == "APIC"
            && self.name == "mime_type"
            && (!value.contains('/') || value.is_empty())
        {
            return Err(SpecError::new(
                "Latin1TextSpec",
                "MIME type must be in format 'type/subtype'".to_string(),
            )
            .into());
        }

        Ok(value)
    }
}

/// List of Latin-1 text strings with count prefix
#[derive(Debug, Clone)]
pub struct Latin1TextListSpec {
    pub name: String,
}

impl Latin1TextListSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for Latin1TextListSpec {
    type Value = Vec<String>;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.is_empty() {
            return Ok((vec![], 0));
        }

        let count = data[0] as usize;
        let mut result = Vec::with_capacity(count);
        let mut pos = 1;

        for _ in 0..count {
            if pos >= data.len() {
                break;
            }

            let remaining = &data[pos..];
            let end = remaining
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(remaining.len());

            // Always add the text, even if empty
            let text = TextEncoding::Latin1.decode_text(&remaining[..end])?;
            result.push(text);

            pos += end + 1; // +1 for null terminator
        }

        Ok((result, pos))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        // The item count is stored as a single byte, so reject lists that
        // exceed the maximum representable value rather than silently wrapping.
        let count = u8::try_from(value.len()).map_err(|_| {
            SpecError::new(
                "Latin1TextListSpec",
                "Too many text entries (max 255)".to_string(),
            )
        })?;
        let mut result = vec![count];

        for text in value {
            result.extend_from_slice(&TextEncoding::Latin1.encode_text(text)?);
            result.push(0); // Null terminator
        }

        Ok(result)
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        // Validate count fits in u8
        if value.len() > 255 {
            return Err(SpecError::new(
                "Latin1TextListSpec",
                "Too many text entries (max 255)".to_string(),
            )
            .into());
        }

        // Validate each string can be encoded in Latin-1
        for text in &value {
            for ch in text.chars() {
                if ch as u32 > 255 {
                    return Err(SpecError::new(
                        "Latin1TextListSpec",
                        format!("Character '{}' cannot be encoded in Latin-1", ch),
                    )
                    .into());
                }
            }
        }

        Ok(value)
    }
}

/// Multiple specification (for arrays/lists)
#[derive(Debug, Clone)]
pub struct MultiSpec<T: Spec + Clone> {
    pub name: String,
    pub spec: T,
    pub separator: Vec<u8>,
    pub optional: bool,
}

impl<T: Spec + Clone> MultiSpec<T> {
    pub fn new(name: &str, spec: T) -> Self {
        Self {
            name: name.to_string(),
            spec,
            separator: vec![0],
            optional: false,
        }
    }

    pub fn with_separator(name: &str, spec: T, separator: Vec<u8>) -> Self {
        Self {
            name: name.to_string(),
            spec,
            separator,
            optional: false,
        }
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

impl<T: Spec + Clone> Spec for MultiSpec<T> {
    type Value = Vec<T::Value>;

    fn read(
        &self,
        header: &FrameHeader,
        frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        let mut result = Vec::new();
        let mut pos = 0;

        while pos < data.len() {
            // Try to read one item
            let remaining = &data[pos..];
            match self.spec.read(header, frame, remaining) {
                Ok((value, consumed)) => {
                    if consumed == 0 {
                        break; // Avoid infinite loop
                    }
                    result.push(value);
                    pos += consumed;

                    // Skip separator if present
                    if pos < data.len()
                        && data.len() - pos >= self.separator.len()
                        && data[pos..pos + self.separator.len()] == *self.separator
                    {
                        pos += self.separator.len();
                    }
                }
                Err(_) => {
                    if result.is_empty() && !self.optional {
                        return Err(SpecError::new(
                            "MultiSpec",
                            "Failed to read required items".to_string(),
                        )
                        .into());
                    }
                    break;
                }
            }
        }

        Ok((result, pos))
    }

    fn write(
        &self,
        config: &FrameWriteConfig,
        frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let mut result = Vec::new();

        for (i, item) in value.iter().enumerate() {
            if i > 0 {
                result.extend_from_slice(&self.separator);
            }
            result.extend_from_slice(&self.spec.write(config, frame, item)?);
        }

        Ok(result)
    }

    fn validate(&self, frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        let mut validated = Vec::new();

        for item in value {
            validated.push(self.spec.validate(frame, item)?);
        }

        Ok(validated)
    }

    fn validate23(&self, frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        let mut validated = Vec::new();

        for item in value {
            validated.push(self.spec.validate23(frame, item)?);
        }

        Ok(validated)
    }
}

/// Sized integer specification (fixed number of bytes)
#[derive(Debug, Clone)]
pub struct SizedIntegerSpec {
    pub name: String,
    pub size: usize,
    pub signed: bool,
}

impl SizedIntegerSpec {
    pub fn new(name: &str, size: usize) -> Self {
        Self {
            name: name.to_string(),
            size,
            signed: false,
        }
    }

    pub fn signed(mut self) -> Self {
        self.signed = true;
        self
    }
}

impl Spec for SizedIntegerSpec {
    type Value = i64;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.len() < self.size {
            return Err(SpecError::new(
                "SizedIntegerSpec",
                format!("Not enough bytes for {}-byte integer", self.size),
            )
            .into());
        }

        let bytes = &data[..self.size];
        let value = match self.size {
            1 => {
                if self.signed {
                    bytes[0] as i8 as i64
                } else {
                    bytes[0] as i64
                }
            }
            2 => {
                let val = u16::from_be_bytes([bytes[0], bytes[1]]);
                if self.signed {
                    val as i16 as i64
                } else {
                    val as i64
                }
            }
            3 => {
                let val = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
                if self.signed && bytes[0] & 0x80 != 0 {
                    // Sign extend
                    (val | 0xFF000000) as i32 as i64
                } else {
                    val as i64
                }
            }
            4 => {
                let val = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                if self.signed {
                    val as i32 as i64
                } else {
                    val as i64
                }
            }
            8 => {
                let val = u64::from_be_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]);
                // Both signed and unsigned 8-byte values are cast to i64 the same way
                val as i64
            }
            _ => {
                return Err(SpecError::new(
                    "SizedIntegerSpec",
                    format!("Unsupported integer size: {}", self.size),
                )
                .into());
            }
        };

        Ok((value, self.size))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let bytes = match self.size {
            1 => {
                vec![*value as u8]
            }
            2 => (*value as u16).to_be_bytes().to_vec(),
            3 => {
                let val = *value as u32;
                vec![(val >> 16) as u8, (val >> 8) as u8, val as u8]
            }
            4 => (*value as u32).to_be_bytes().to_vec(),
            8 => (*value as u64).to_be_bytes().to_vec(),
            _ => {
                return Err(SpecError::new(
                    "SizedIntegerSpec",
                    format!("Unsupported integer size: {}", self.size),
                )
                .into());
            }
        };

        Ok(bytes)
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        // Validate value fits in the specified size
        let (min_val, max_val) = match (self.size, self.signed) {
            (1, false) => (0, 255),
            (1, true) => (-128, 127),
            (2, false) => (0, 65535),
            (2, true) => (-32768, 32767),
            (3, false) => (0, 16777215),
            (3, true) => (-8388608, 8388607),
            (4, false) => (0, 4294967295),
            (4, true) => (-2147483648, 2147483647),
            (8, _) => (i64::MIN, i64::MAX),
            _ => return Ok(value),
        };

        if value < min_val || value > max_val {
            return Err(SpecError::new(
                "SizedIntegerSpec",
                format!(
                    "Value {} out of range for {}-byte {} integer",
                    value,
                    self.size,
                    if self.signed { "signed" } else { "unsigned" }
                ),
            )
            .into());
        }

        Ok(value)
    }
}

/// Integer specification for variable-width bit-padded integers
#[derive(Debug, Clone)]
pub struct IntegerSpec {
    pub name: String,
}

impl IntegerSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for IntegerSpec {
    type Value = u32;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.is_empty() {
            return Err(SpecError::new("IntegerSpec", "No data to read".to_string()).into());
        }

        // Use all data with variable width (width=-1 in specification)
        let bit_padded_int = BitPaddedInt::from_bytes(data, 7, true)?;
        let value: u32 = bit_padded_int.value();

        Ok((value, data.len()))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        // Use variable width (width=-1 in standard equivalent)
        BitPaddedInt::to_str(*value, None, Some(true), Some(-1), None)
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// Variable-length integer specification (supports synchsafe integers)
#[derive(Debug, Clone)]
pub struct VarLengthSpec {
    pub name: String,
    pub synchsafe: bool,
}

impl VarLengthSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            synchsafe: false,
        }
    }

    pub fn synchsafe(mut self) -> Self {
        self.synchsafe = true;
        self
    }
}

impl Spec for VarLengthSpec {
    type Value = u32;

    fn read(
        &self,
        _header: &FrameHeader,
        frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.len() < 4 {
            return Err(SpecError::new(
                "VarLengthSpec",
                "Not enough bytes for variable-length integer".to_string(),
            )
            .into());
        }

        let bytes = &data[..4];
        let value = if self.synchsafe || frame.is_v24() {
            decode_synchsafe_int_checked(bytes)?
        } else {
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        };

        Ok((value, 4))
    }

    fn write(
        &self,
        config: &FrameWriteConfig,
        frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let bytes = if self.synchsafe || (config.use_synchsafe_ints && frame.is_v24()) {
            encode_synchsafe_int(*value)?
        } else {
            value.to_be_bytes()
        };

        Ok(bytes.to_vec())
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        if self.synchsafe && value > 0x0FFFFFFF {
            return Err(SpecError::new(
                "VarLengthSpec",
                "Value too large for synchsafe integer".to_string(),
            )
            .into());
        }
        Ok(value)
    }
}

/// ID3v2 header structure (10 bytes)
#[derive(Debug, Clone)]
pub struct ID3Header {
    pub major_version: u8,
    pub revision: u8,
    pub flags: u8,
    pub size: u32,
}

impl ID3Header {
    /// Create new ID3Header for testing
    pub fn new(major: u8, minor: u8, flags: u8, size: u32) -> Self {
        Self {
            major_version: major,
            revision: minor,
            flags,
            size,
        }
    }

    /// Parse ID3v2 header from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 10 {
            return Err(AudexError::InvalidData("ID3 header too short".to_string()));
        }

        if &data[0..3] != b"ID3" {
            return Err(AudexError::InvalidData(
                "Invalid ID3 header signature".to_string(),
            ));
        }

        let major_version = data[3];
        let revision = data[4];
        let flags = data[5];

        // Only ID3v2.2, v2.3, and v2.4 are defined in the specification.
        // Reject all other major versions to avoid misinterpreting unknown formats.
        if ![2, 3, 4].contains(&major_version) {
            return Err(AudexError::InvalidData(format!(
                "Unsupported ID3v2 major version: {}",
                major_version
            )));
        }

        // Validate ID3v2 revision - for major version 2, only revisions 2, 3, and 4 are supported
        if major_version == 2 && revision > 4 {
            return Err(AudexError::InvalidData(format!(
                "Unsupported ID3v2 revision: v{}.{}",
                major_version, revision
            )));
        }

        // Validate that header size bytes are valid synchsafe (bit 7 must be 0 in each byte)
        if data[6..10].iter().any(|&b| b & 0x80 != 0) {
            return Err(AudexError::InvalidData(
                "Header size not synchsafe".to_string(),
            ));
        }

        // Size is stored as synchsafe integer
        let size = decode_synchsafe_int_checked(&data[6..10])?;

        Ok(Self {
            major_version,
            revision,
            flags,
            size,
        })
    }

    /// Convert header to bytes.
    /// Returns an error if the tag size exceeds the synchsafe encoding limit.
    pub fn to_bytes(&self) -> crate::Result<[u8; 10]> {
        let mut header = [0u8; 10];
        header[0..3].copy_from_slice(b"ID3");
        header[3] = self.major_version;
        header[4] = self.revision;
        header[5] = self.flags;

        let size_bytes = encode_synchsafe_int(self.size)?;
        header[6..10].copy_from_slice(&size_bytes);

        Ok(header)
    }

    /// Check if unsynchronization flag is set
    pub fn has_unsynchronization(&self) -> bool {
        self.flags & 0x80 != 0
    }

    /// Check if extended header flag is set
    pub fn has_extended_header(&self) -> bool {
        self.flags & 0x40 != 0
    }

    /// Check if experimental flag is set
    pub fn is_experimental(&self) -> bool {
        self.flags & 0x20 != 0
    }

    /// Check if footer is present
    pub fn has_footer(&self) -> bool {
        self.major_version >= 4 && (self.flags & 0x10 != 0)
    }

    /// Get version as tuple
    pub fn version(&self) -> (u8, u8) {
        (self.major_version, self.revision)
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn flags(&self) -> u8 {
        self.flags
    }
}

/// Picture type enumeration for APIC frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
#[derive(Default)]
pub enum PictureType {
    Other = 0,
    FileIcon = 1, // 32x32 pixels 'file icon' (PNG only)
    OtherFileIcon = 2,
    #[default]
    CoverFront = 3,
    CoverBack = 4,
    LeafletPage = 5,
    Media = 6, // e.g. label side of CD
    LeadArtist = 7,
    Artist = 8,
    Conductor = 9,
    Band = 10,
    Composer = 11,
    Lyricist = 12,
    RecordingLocation = 13,
    DuringRecording = 14,
    DuringPerformance = 15,
    ScreenCapture = 16,
    Fish = 17, // A bright coloured fish
    Illustration = 18,
    BandLogotype = 19,
    PublisherLogotype = 20,
}

impl PictureType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Other),
            1 => Some(Self::FileIcon),
            2 => Some(Self::OtherFileIcon),
            3 => Some(Self::CoverFront),
            4 => Some(Self::CoverBack),
            5 => Some(Self::LeafletPage),
            6 => Some(Self::Media),
            7 => Some(Self::LeadArtist),
            8 => Some(Self::Artist),
            9 => Some(Self::Conductor),
            10 => Some(Self::Band),
            11 => Some(Self::Composer),
            12 => Some(Self::Lyricist),
            13 => Some(Self::RecordingLocation),
            14 => Some(Self::DuringRecording),
            15 => Some(Self::DuringPerformance),
            16 => Some(Self::ScreenCapture),
            17 => Some(Self::Fish),
            18 => Some(Self::Illustration),
            19 => Some(Self::BandLogotype),
            20 => Some(Self::PublisherLogotype),
            _ => None,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Other => "Other",
            Self::FileIcon => "32x32 pixels file icon",
            Self::OtherFileIcon => "Other file icon",
            Self::CoverFront => "Cover (front)",
            Self::CoverBack => "Cover (back)",
            Self::LeafletPage => "Leaflet page",
            Self::Media => "Media",
            Self::LeadArtist => "Lead artist/lead performer/soloist",
            Self::Artist => "Artist/performer",
            Self::Conductor => "Conductor",
            Self::Band => "Band/Orchestra",
            Self::Composer => "Composer",
            Self::Lyricist => "Lyricist/text writer",
            Self::RecordingLocation => "Recording Location",
            Self::DuringRecording => "During recording",
            Self::DuringPerformance => "During performance",
            Self::ScreenCapture => "Movie/video screen capture",
            Self::Fish => "A bright coloured fish",
            Self::Illustration => "Illustration",
            Self::BandLogotype => "Band/artist logotype",
            Self::PublisherLogotype => "Publisher/Studio logotype",
        }
    }
}

/// CTOC flags for table of contents frames
#[derive(Debug, Clone, Copy, Default)]
pub struct CTOCFlags(u8);

impl CTOCFlags {
    pub const TOP_LEVEL: u8 = 0x02; // Identifies the CTOC root frame
    pub const ORDERED: u8 = 0x01; // Child elements are ordered

    pub fn new(flags: u8) -> Self {
        Self(flags)
    }

    pub fn is_top_level(&self) -> bool {
        self.0 & Self::TOP_LEVEL != 0
    }

    pub fn is_ordered(&self) -> bool {
        self.0 & Self::ORDERED != 0
    }

    pub fn value(&self) -> u8 {
        self.0
    }
}

/// Picture type specification for APIC frames
#[derive(Debug, Clone)]
pub struct PictureTypeSpec {
    pub name: String,
}

impl PictureTypeSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for PictureTypeSpec {
    type Value = PictureType;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.is_empty() {
            return Err(
                SpecError::new("PictureTypeSpec", "No data for picture type".to_string()).into(),
            );
        }

        let picture_type = PictureType::from_u8(data[0]).unwrap_or(PictureType::Other);

        Ok((picture_type, 1))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        Ok(vec![*value as u8])
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// CTOC flags specification
#[derive(Debug, Clone)]
pub struct CTOCFlagsSpec {
    pub name: String,
}

impl CTOCFlagsSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for CTOCFlagsSpec {
    type Value = CTOCFlags;

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.is_empty() {
            return Err(
                SpecError::new("CTOCFlagsSpec", "No data for CTOC flags".to_string()).into(),
            );
        }

        Ok((CTOCFlags::new(data[0]), 1))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        Ok(vec![value.value()])
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// TimeStamp specification for TDRC and similar frames
#[derive(Debug, Clone)]
pub struct TimeStampSpec {
    pub name: String,
}

impl TimeStampSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for TimeStampSpec {
    type Value = ID3TimeStamp;

    fn read(
        &self,
        header: &FrameHeader,
        frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        let text_spec = EncodedTextSpec::new("timestamp_text");
        let (text, consumed) = text_spec.read(header, frame, data)?;

        let timestamp = ID3TimeStamp::parse(&text);
        Ok((timestamp, consumed))
    }

    fn write(
        &self,
        config: &FrameWriteConfig,
        frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let text_spec = EncodedTextSpec::new("timestamp_text");
        // Convert space to T for ISO format when writing
        let iso_text = value.text.replace(' ', "T");
        text_spec.write(config, frame, &iso_text)
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// Volume adjustment specification (RVA2 frame)
#[derive(Debug, Clone)]
pub struct VolumeAdjustmentSpec {
    pub name: String,
}

impl VolumeAdjustmentSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for VolumeAdjustmentSpec {
    type Value = f32; // Volume adjustment in dB

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.len() < 2 {
            return Err(SpecError::new(
                "VolumeAdjustmentSpec",
                "Not enough data for volume adjustment".to_string(),
            )
            .into());
        }

        let raw_value = i16::from_be_bytes([data[0], data[1]]);
        let volume_db = raw_value as f32 / 512.0;

        Ok((volume_db, 2))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        // Enforce the same range used by validate() so that extreme values
        // are rejected rather than silently saturated during the f32→i16 cast.
        if !(-64.0..=64.0).contains(value) {
            return Err(SpecError::new(
                "VolumeAdjustmentSpec",
                "Volume adjustment out of valid range (-64 to +64 dB)".to_string(),
            )
            .into());
        }

        let raw_value = (value * 512.0).round() as i16;

        Ok(raw_value.to_be_bytes().to_vec())
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        if !(-64.0..=64.0).contains(&value) {
            return Err(SpecError::new(
                "VolumeAdjustmentSpec",
                "Volume adjustment out of valid range (-64 to +64 dB)".to_string(),
            )
            .into());
        }
        Ok(value)
    }
}

/// Volume peak specification (RVA2 frame)
#[derive(Debug, Clone)]
pub struct VolumePeakSpec {
    pub name: String,
}

impl VolumePeakSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for VolumePeakSpec {
    type Value = f32; // Peak level (0.0 to 1.0)

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.is_empty() {
            return Err(
                SpecError::new("VolumePeakSpec", "No data for volume peak".to_string()).into(),
            );
        }

        let bits = data[0];

        // A peak with 0 bits has no meaningful value
        if bits == 0 {
            return Ok((0.0, 1));
        }

        let vol_bytes = bits.div_ceil(8).min(4) as usize;

        if data.len() < 1 + vol_bytes {
            return Err(
                SpecError::new("VolumePeakSpec", "Not enough frame data".to_string()).into(),
            );
        }

        let shift = ((8 - (bits & 7)) & 7) + ((4 - vol_bytes) * 8) as u8;

        // Validate shift is within the valid range for a u32 left-shift.
        // The arithmetic above should always produce 0..=31, but we guard
        // against unexpected inputs to avoid undefined-behaviour-class panics.
        debug_assert!(shift <= 31, "shift out of range for u32: {shift}");

        let mut peak = 0u32;

        for &byte in data.iter().take(vol_bytes + 1).skip(1) {
            peak = (peak << 8) | byte as u32;
        }

        // Use checked_shl so an out-of-range shift saturates to zero
        // instead of panicking in debug or wrapping in release builds.
        peak = peak.checked_shl(shift.min(31) as u32).unwrap_or(0);
        let peak_float = peak as f32 / (2_u32.pow(31) - 1) as f32;

        Ok((peak_float, 1 + vol_bytes))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        // Validate range before casting to avoid silent truncation.
        if *value < 0.0 || *value > 1.0 {
            return Err(
                SpecError::new("VolumePeakSpec", "Peak volume out of range".to_string()).into(),
            );
        }

        let raw_value = (value * 32768.0).round() as u16;

        // Always write as 16 bits for consistency
        let mut result = vec![0x10]; // 16 bits indicator
        result.extend_from_slice(&raw_value.to_be_bytes());

        Ok(result)
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        if !(0.0..=1.0).contains(&value) {
            return Err(SpecError::new(
                "VolumePeakSpec",
                "Peak volume must be between 0.0 and 1.0".to_string(),
            )
            .into());
        }
        Ok(value)
    }
}

/// RVA specification for legacy RVAD frames
#[derive(Debug, Clone)]
pub struct RVASpec {
    pub name: String,
    pub stereo_only: bool,
}

impl RVASpec {
    pub fn new(name: &str, stereo_only: bool) -> Self {
        Self {
            name: name.to_string(),
            stereo_only,
        }
    }
}

impl Spec for RVASpec {
    type Value = Vec<i32>; // Volume adjustments

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        if data.len() < 2 {
            return Err(SpecError::new("RVASpec", "Not enough data for RVA".to_string()).into());
        }

        let flags = data[0];
        let bits = data[1];

        if bits == 0 {
            return Err(SpecError::new("RVASpec", "Bits used must be > 0".to_string()).into());
        }

        let bytes_per_value = bits.div_ceil(8) as usize;
        let max_values = if self.stereo_only { 4 } else { 12 };

        let mut values = Vec::new();
        let mut offset = 2;

        while offset + bytes_per_value <= data.len() && values.len() < max_values {
            let bytes = &data[offset..offset + bytes_per_value];
            let mut value = 0i32;

            for &byte in bytes {
                value = (value << 8) | byte as i32;
            }

            values.push(value);
            offset += bytes_per_value;
        }

        if values.len() < 2 {
            return Err(
                SpecError::new("RVASpec", "First two values not optional".to_string()).into(),
            );
        }

        // Apply increment/decrement flags
        let flag_indices = [0, 1, 4, 5, 8, 10];
        for (bit, &index) in flag_indices.iter().enumerate() {
            if index < values.len() && (flags & (1 << bit)) == 0 {
                values[index] = -values[index];
            }
        }

        Ok((values, offset))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let max_values = if self.stereo_only { 4 } else { 12 };

        if value.len() < 2 || value.len() > max_values {
            return Err(SpecError::new(
                "RVASpec",
                format!(
                    "At least two volume change values required, max {}",
                    max_values
                ),
            )
            .into());
        }

        let mut result = Vec::new();
        let mut flags = 0u8;
        let mut abs_values = value.clone();

        // Calculate flags and absolute values
        let flag_indices = [0, 1, 4, 5, 8, 10];
        for (bit, &index) in flag_indices.iter().enumerate() {
            if index < abs_values.len() {
                if abs_values[index] < 0 {
                    abs_values[index] = -abs_values[index];
                } else {
                    flags |= 1 << bit;
                }
            }
        }

        result.push(flags);

        // Serialize values and find max byte length
        let mut byte_values = Vec::new();
        for &val in &abs_values {
            let bytes = BitPaddedInt::to_str(val as u32, Some(8), Some(true), Some(-1), Some(2))?;
            byte_values.push(bytes);
        }

        let max_bytes = byte_values.iter().map(|v| v.len()).max().unwrap_or(2);

        // Pad all values to same length
        for bytes in &mut byte_values {
            while bytes.len() < max_bytes {
                bytes.push(0);
            }
        }

        let bits = max_bytes * 8;
        result.push(bits as u8);

        for bytes in byte_values {
            result.extend_from_slice(&bytes);
        }

        Ok(result)
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        let max_values = if self.stereo_only { 4 } else { 12 };

        if value.len() < 2 || value.len() > max_values {
            return Err(SpecError::new(
                "RVASpec",
                format!("Needs list of length 2..{}", max_values),
            )
            .into());
        }

        Ok(value)
    }
}

/// ASPI index specification for seek point indices
#[derive(Debug, Clone)]
pub struct ASPIIndexSpec {
    pub name: String,
}

impl ASPIIndexSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for ASPIIndexSpec {
    type Value = Vec<u16>; // Seek point indices

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        let entry_size = 2; // 16-bit entries
        let num_entries = data.len() / entry_size;

        let mut indices = Vec::new();
        for i in 0..num_entries {
            let offset = i * entry_size;
            if offset + entry_size <= data.len() {
                let value = u16::from_be_bytes([data[offset], data[offset + 1]]);
                indices.push(value);
            }
        }

        Ok((indices, data.len()))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let mut result = Vec::new();

        // Assume 16-bit entries
        for &index in value {
            result.extend_from_slice(&index.to_be_bytes());
        }

        Ok(result)
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// Synchronized text specification (SYLT frame)
#[derive(Debug, Clone)]
pub struct SynchronizedTextSpec {
    pub name: String,
}

impl SynchronizedTextSpec {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Spec for SynchronizedTextSpec {
    type Value = Vec<(String, u32)>; // (text, timestamp) pairs

    fn read(
        &self,
        _header: &FrameHeader,
        _frame: &FrameData,
        data: &[u8],
    ) -> Result<(Self::Value, usize)> {
        let mut texts = Vec::new();
        let mut offset = 0;

        // Default to UTF-8 encoding
        let encoding = TextEncoding::Utf8;

        let terminator = match encoding {
            TextEncoding::Latin1 => vec![0x00],
            TextEncoding::Utf16 => vec![0x00, 0x00],
            TextEncoding::Utf16Be => vec![0x00, 0x00],
            TextEncoding::Utf8 => vec![0x00],
        };

        while offset < data.len() {
            // Find text terminator
            let text_end =
                find_terminator(&data[offset..], &terminator).unwrap_or(data.len() - offset);

            let text_bytes = &data[offset..offset + text_end];
            let text = match encoding {
                TextEncoding::Latin1 => text_bytes.iter().map(|&b| b as char).collect(),
                TextEncoding::Utf8 => String::from_utf8_lossy(text_bytes).into_owned(),
                TextEncoding::Utf16 | TextEncoding::Utf16Be => {
                    // Simplified UTF-16 handling
                    String::from_utf8_lossy(text_bytes).into_owned()
                }
            };

            offset += text_end + terminator.len();

            // Read timestamp (4 bytes)
            if offset + 4 > data.len() {
                return Err(SpecError::new(
                    "SynchronizedTextSpec",
                    "Not enough data for timestamp".to_string(),
                )
                .into());
            }

            let timestamp = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);

            texts.push((text, timestamp));
            offset += 4;
        }

        Ok((texts, data.len()))
    }

    fn write(
        &self,
        _config: &FrameWriteConfig,
        _frame: &FrameData,
        value: &Self::Value,
    ) -> Result<Vec<u8>> {
        let mut result = Vec::new();

        // Default to UTF-8 encoding
        let encoding = TextEncoding::Utf8;

        let terminator = match encoding {
            TextEncoding::Latin1 => vec![0x00],
            TextEncoding::Utf16 => vec![0x00, 0x00],
            TextEncoding::Utf16Be => vec![0x00, 0x00],
            TextEncoding::Utf8 => vec![0x00],
        };

        for (text, timestamp) in value {
            // Encode text based on encoding
            let text_bytes = match encoding {
                TextEncoding::Latin1 => text.chars().map(|c| c as u8).collect::<Vec<u8>>(),
                TextEncoding::Utf8 => text.as_bytes().to_vec(),
                TextEncoding::Utf16 | TextEncoding::Utf16Be => {
                    // Simplified UTF-16 encoding
                    text.as_bytes().to_vec()
                }
            };

            result.extend_from_slice(&text_bytes);
            result.extend_from_slice(&terminator);
            result.extend_from_slice(&timestamp.to_be_bytes());
        }

        Ok(result)
    }

    fn validate(&self, _frame: &FrameData, value: Self::Value) -> Result<Self::Value> {
        Ok(value)
    }
}

/// Helper function to find string terminator in data.
/// For 2-byte terminators (UTF-16), searches at 2-byte-aligned offsets only.
fn find_terminator(data: &[u8], terminator: &[u8]) -> Option<usize> {
    if terminator.is_empty() {
        return Some(data.len());
    }

    if terminator.len() == 2 {
        let mut pos = 0;
        while pos + 1 < data.len() {
            if data[pos] == terminator[0] && data[pos + 1] == terminator[1] {
                return Some(pos);
            }
            pos += 2;
        }
        None
    } else {
        data.windows(terminator.len())
            .position(|window| window == terminator)
    }
}
