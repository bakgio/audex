//! ID3v2 tag container and frame management
//!
//! This module provides the core ID3v2 tag container ([`ID3Tags`]) with frame
//! management, version conversion, and advanced container operations.
//!
//! # Key Types
//!
//! - [`ID3Header`] — Parsed ID3v2 header with version, flags, and size
//! - [`ID3Tags`] — Primary tag container holding frames in a sorted dictionary
//! - [`ID3SaveConfig`] — Configuration for tag serialization (version, padding, separators)
//! - [`ExtendedID3Header`] — Extended header with CRC, restrictions, and extra flags
//!
//! # Internal Details
//!
//! ID3v2 tags support three major versions (v2.2, v2.3, v2.4) with different
//! frame header sizes (6 bytes for v2.2, 10 bytes for v2.3/v2.4) and size
//! encoding (big-endian integers for v2.2/v2.3, synchsafe integers for v2.4).
//! The [`determine_bpi`] function handles iTunes compatibility where v2.4 tags
//! were incorrectly written with regular integers instead of synchsafe encoding.

use crate::id3::frames::{Frame, FrameRegistry, TextFrame};
use crate::id3::specs::{FrameHeader, FrameProcessor, ID3TimeStamp, TextEncoding};
use crate::id3::util::{BitPaddedInt, Unsynch};
use crate::tags::{Metadata, Tags};
use crate::util::{delete_bytes, insert_bytes};
use crate::{AudexError, Result};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;

#[cfg(feature = "serde")]
use crate::id3::frames::{APIC, COMM, TXXX, WXXX};

/// Version constants for ID3v2 specifications
const _V24: (u8, u8, u8) = (2, 4, 0);
const _V23: (u8, u8, u8) = (2, 3, 0);
const _V22: (u8, u8, u8) = (2, 2, 0);
const _V11: (u8, u8) = (1, 1);

/// Parsed ID3v2 tag header
///
/// Represents the 10-byte header at the start of every ID3v2 tag. Contains
/// the version number, flags (unsynchronization, extended header, experimental,
/// footer), and total tag size. Used during both reading and writing.
#[derive(Debug, Clone)]
pub struct ID3Header {
    /// Version tuple (major, minor, revision)
    pub version: (u8, u8, u8),
    /// Header flags
    pub flags: u8,
    /// Tag body size (excludes the 10-byte header)
    pub size: u32,
    /// Extended header data if present
    pub _extdata: Option<Vec<u8>>,
    /// Known frames for this version
    pub _known_frames: Option<HashMap<String, String>>,
}

impl Default for ID3Header {
    fn default() -> Self {
        Self::new()
    }
}

impl ID3Header {
    /// Create new ID3Header with testing defaults
    pub fn new() -> Self {
        Self {
            version: _V24,
            flags: 0,
            size: 0,
            _extdata: None,
            _known_frames: None,
        }
    }

    /// Convert from specs::ID3Header to tags::ID3Header
    pub fn from_specs_header(specs_header: &crate::id3::specs::ID3Header) -> Self {
        Self {
            version: (2, specs_header.major_version, specs_header.revision),
            flags: specs_header.flags,
            size: specs_header.size,
            _extdata: None,
            _known_frames: None,
        }
    }

    /// Parse ID3Header from file object
    pub fn from_reader<R: Read>(mut reader: R) -> Result<Self> {
        let mut header_data = [0u8; 10];
        reader.read_exact(&mut header_data)?;

        if &header_data[0..3] != b"ID3" {
            return Err(AudexError::ID3NoHeaderError);
        }

        let vmaj = header_data[3];
        let vrev = header_data[4];
        let flags = header_data[5];
        let size_bytes = &header_data[6..10];

        if ![2, 3, 4].contains(&vmaj) {
            return Err(AudexError::InvalidData(format!(
                "ID3v2.{} not supported",
                vmaj
            )));
        }

        if !BitPaddedInt::has_valid_padding(size_bytes.into(), Some(7)) {
            return Err(AudexError::InvalidData(
                "Header size not synchsafe".to_string(),
            ));
        }
        // Store body-only size (excludes the 10-byte header) to match
        // the convention used by specs::ID3Header and from_specs_header.
        let size = BitPaddedInt::new(size_bytes.into(), Some(7), Some(true))?.value();

        // Validate flags based on version
        // v2.4: only bits 4-7 defined, v2.3: only bits 5-7 defined, v2.2: only bits 6-7 defined
        if (vmaj >= 4 && (flags & 0x0f) != 0)
            || (vmaj == 3 && (flags & 0x1f) != 0)
            || (vmaj == 2 && (flags & 0x3f) != 0)
        {
            return Err(AudexError::InvalidData(format!(
                "Invalid flags {:02x}",
                flags
            )));
        }

        let mut header = Self {
            version: (2, vmaj, vrev),
            flags,
            size,
            _extdata: None,
            _known_frames: None,
        };

        // Handle extended header
        if header.f_extended() {
            header._extdata = Some(header.read_extended_header(
                &mut reader,
                vmaj,
                crate::limits::ParseLimits::default(),
            )?);
        }

        Ok(header)
    }

    /// Read extended header data
    fn read_extended_header<R: Read>(
        &self,
        mut reader: R,
        version: u8,
        limits: crate::limits::ParseLimits,
    ) -> Result<Vec<u8>> {
        let mut extsize_data = [0u8; 4];
        reader.read_exact(&mut extsize_data)?;

        // Check if extsize_data looks like a frame ID (common bug in some taggers)
        if let Ok(frame_id) = std::str::from_utf8(&extsize_data) {
            // Check if it matches a known frame ID pattern (uppercase letters/digits)
            if frame_id
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            {
                // This looks like a frame ID, not an extended header size
                // The extended header flag was probably set incorrectly
                // Return the 4 bytes as data so the caller can prepend them back
                return Ok(extsize_data.to_vec());
            }
        }

        let extsize = if version >= 4 {
            // In ID3v2.4 the extended header size is synchsafe and includes
            // its own 4 bytes.  The decoded value must be at least 4;
            // anything smaller would underflow the subtraction.
            let raw = BitPaddedInt::new(extsize_data.as_slice().into(), Some(7), Some(true))
                .map_err(|_| AudexError::InvalidData("Invalid extended header size".to_string()))?
                .value() as usize;
            if raw < 4 {
                return Err(AudexError::InvalidData(format!(
                    "ID3v2.4 extended header size too small: {} (minimum is 4)",
                    raw
                )));
            }
            raw - 4
        } else {
            u32::from_be_bytes(extsize_data) as usize
        };

        // Enforce both the hardcoded safety cap and the user-configured
        // ParseLimits, whichever is tighter.
        const MAX_EXT_HEADER: usize = 64 * 1024 * 1024;
        if extsize > MAX_EXT_HEADER {
            return Err(AudexError::ParseError(format!(
                "ID3 extended header too large: {} bytes",
                extsize
            )));
        }
        limits.check_tag_size(extsize as u64, "ID3 extended header")?;

        if extsize > 0 {
            let mut ext_data = vec![0u8; extsize];
            reader.read_exact(&mut ext_data)?;
            Ok(ext_data)
        } else {
            Ok(Vec::new())
        }
    }

    /// Check if unsynchronization flag is set
    pub fn f_unsynch(&self) -> bool {
        (self.flags & 0x80) != 0
    }

    /// Check if extended header flag is set
    pub fn f_extended(&self) -> bool {
        (self.flags & 0x40) != 0
    }

    /// Check if experimental flag is set
    pub fn f_experimental(&self) -> bool {
        (self.flags & 0x20) != 0
    }

    /// Check if footer flag is set (v2.4 only)
    pub fn f_footer(&self) -> bool {
        (self.flags & 0x10) != 0
    }

    /// Get known frames for this version
    pub fn known_frames(&self) -> HashMap<String, String> {
        if let Some(ref frames) = self._known_frames {
            frames.clone()
        } else if self.version >= _V23 {
            self.get_frames_v23_v24()
        } else if self.version >= _V22 {
            self.get_frames_v22()
        } else {
            HashMap::new()
        }
    }

    /// Get ID3v2.3/2.4 frame list
    fn get_frames_v23_v24(&self) -> HashMap<String, String> {
        let frames = [
            "TIT2", "TPE1", "TALB", "TDRC", "TCON", "TRCK", "TPOS", "TYER", "TDAT", "TIME", "COMM",
            "APIC", "TXXX", "WXXX", "TIPL", "TMCL", "IPLS", "TORY", "TDOR", "CHAP", "CTOC",
        ];

        frames
            .iter()
            .map(|&f| (f.to_string(), f.to_string()))
            .collect()
    }

    /// Get ID3v2.2 frame list
    fn get_frames_v22(&self) -> HashMap<String, String> {
        let frames = [
            "TT2", "TP1", "TAL", "TYE", "TCO", "TRK", "COM", "PIC", "TXX", "WXX",
        ];

        frames
            .iter()
            .map(|&f| (f.to_string(), f.to_string()))
            .collect()
    }

    /// Get major version - helper method
    pub fn major_version(&self) -> u8 {
        self.version.1
    }

    /// Get revision - helper method
    pub fn revision(&self) -> u8 {
        self.version.2
    }

    /// Check if has extended header - helper method
    pub fn has_extended_header(&self) -> bool {
        self.f_extended()
    }
}

/// Determine BitPaddedInt usage for iTunes compatibility
///
/// Takes id3v2.4 frame data and determines if ints or bitpaddedints
/// should be used for parsing. Needed because iTunes used to write
/// normal ints for frame sizes.
pub fn determine_bpi(data: &[u8], frames: &HashMap<String, String>) -> fn(&[u8]) -> Result<u32> {
    const EMPTY: &[u8] = &[0u8; 10];

    // Guard against unrealistically large data that would cause silent
    // truncation when cast to i64 for the offset arithmetic below.
    // ID3v2 tags are limited to ~256 MB by the synchsafe size encoding,
    // so anything beyond i64::MAX is clearly invalid.
    let data_len_i64 = i64::try_from(data.len()).unwrap_or(i64::MAX);

    // Maximum number of frames to examine in each scan pass. Real-world
    // ID3v2 tags rarely exceed a few hundred frames, so 1000 is more than
    // sufficient for an accurate heuristic while preventing linear-time
    // scanning of adversarially large buffers.
    const MAX_SCAN_ITERATIONS: usize = 1000;

    // Count number of tags found as BitPaddedInt and how far past
    let mut offset = 0;
    let mut asbpi = 0;
    let mut bpioff: i64 = 0;
    let mut iterations = 0;

    while offset < data.len().saturating_sub(10) {
        // Cap the number of frames examined to prevent CPU exhaustion on
        // crafted tags with many small frames spanning hundreds of megabytes
        if iterations >= MAX_SCAN_ITERATIONS {
            break;
        }
        iterations += 1;
        let part = &data[offset..offset + 10];
        if part == EMPTY {
            let remainder = i64::try_from(data.len() - offset).unwrap_or(0);
            bpioff = -(remainder % 10);
            break;
        }

        if part.len() < 10 {
            break;
        }

        let name = &part[0..4];
        let size_bytes = &part[4..8];
        let _flags = &part[8..10];

        // Parse size as BitPaddedInt
        if let Ok(size_bpi) = BitPaddedInt::new(size_bytes.into(), Some(7), Some(true)) {
            let size = size_bpi.value() as usize;
            // Guard against overflow on 32-bit platforms where 10 + size
            // could wrap around to a small value
            offset = match offset.checked_add(10 + size) {
                Some(new_offset) => new_offset,
                None => data.len(), // treat as end of data
            };

            if let Ok(name_str) = std::str::from_utf8(name) {
                if frames.contains_key(name_str) {
                    asbpi += 1;
                }
            }
        } else {
            break;
        }
    }

    if offset >= data.len() {
        let offset_i64 = i64::try_from(offset).unwrap_or(data_len_i64);
        bpioff = offset_i64.saturating_sub(data_len_i64);
    }

    // Count number of tags found as int and how far past
    offset = 0;
    let mut asint = 0;
    let mut intoff: i64 = 0;
    iterations = 0;

    while offset < data.len().saturating_sub(10) {
        // Apply the same iteration cap to the integer-size scan pass
        if iterations >= MAX_SCAN_ITERATIONS {
            break;
        }
        iterations += 1;
        let part = &data[offset..offset + 10];
        if part == EMPTY {
            let remainder = i64::try_from(data.len() - offset).unwrap_or(0);
            intoff = -(remainder % 10);
            break;
        }

        if part.len() < 10 {
            break;
        }

        let name = &part[0..4];
        let size_bytes = &part[4..8];
        let _flags = &part[8..10];

        // Parse size as regular int
        let size = u32::from_be_bytes([size_bytes[0], size_bytes[1], size_bytes[2], size_bytes[3]])
            as usize;
        // Guard against overflow on 32-bit platforms where 10 + size
        // could wrap around to a small value
        offset = match offset.checked_add(10 + size) {
            Some(new_offset) => new_offset,
            None => data.len(), // treat as end of data
        };

        if let Ok(name_str) = std::str::from_utf8(name) {
            if frames.contains_key(name_str) {
                asint += 1;
            }
        }
    }

    if offset >= data.len() {
        let offset_i64 = i64::try_from(offset).unwrap_or(data_len_i64);
        intoff = offset_i64.saturating_sub(data_len_i64);
    }

    // If more tags as int, or equal and bpi is past and int is not
    let chose_int = asint > asbpi || (asint == asbpi && (bpioff >= 1 && intoff <= 1));
    let initial_choice = if chose_int {
        parse_size_as_int
    } else {
        parse_size_as_bpi
    };

    // Secondary validation: verify the chosen interpretation produces frames
    // with valid IDs at each boundary. This prevents crafted data from
    // fooling the heuristic by embedding fake headers that only appear valid
    // under the wrong size interpretation.
    let chosen_valid = validate_frame_boundaries(data, initial_choice);

    if !chosen_valid {
        // The chosen interpretation failed validation; try the alternative.
        let alternative: fn(&[u8]) -> Result<u32> = if chose_int {
            parse_size_as_bpi
        } else {
            parse_size_as_int
        };
        if validate_frame_boundaries(data, alternative) {
            return alternative;
        }
    }

    initial_choice
}

/// Walk frame boundaries using the given size parser and check that every
/// frame header contains a valid 4-character ID. Returns false if any
/// non-padding frame has an invalid ID, indicating the size interpretation
/// is wrong.
fn validate_frame_boundaries(data: &[u8], size_parser: fn(&[u8]) -> Result<u32>) -> bool {
    const EMPTY: &[u8] = &[0u8; 10];
    const MAX_VALIDATION_FRAMES: usize = 50;

    let mut offset = 0;
    let mut frames_checked = 0;

    while offset + 10 <= data.len() {
        if frames_checked >= MAX_VALIDATION_FRAMES {
            break;
        }

        let part = &data[offset..offset + 10];
        if part == EMPTY {
            break;
        }

        let name = &part[0..4];
        let size_bytes = &part[4..8];

        // Every non-padding frame must have a valid frame ID
        if let Ok(name_str) = std::str::from_utf8(name) {
            if !crate::id3::util::is_valid_frame_id(name_str) {
                return false;
            }
        } else {
            return false;
        }

        let size = match size_parser(size_bytes) {
            Ok(s) => s as usize,
            Err(_) => return false,
        };

        offset = match offset.checked_add(10 + size) {
            Some(new_offset) => new_offset,
            None => return false,
        };

        frames_checked += 1;
    }

    // Must have found at least one valid frame to consider this valid
    frames_checked > 0
}

/// Parse size as regular big-endian integer
fn parse_size_as_int(bytes: &[u8]) -> Result<u32> {
    if bytes.len() < 4 {
        return Err(AudexError::InvalidData(
            "Not enough bytes for size".to_string(),
        ));
    }
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

/// Parse size as BitPaddedInt (synchsafe)
fn parse_size_as_bpi(bytes: &[u8]) -> Result<u32> {
    BitPaddedInt::new(bytes.into(), Some(7), Some(true))
        .map(|bpi| bpi.value())
        .map_err(|e| AudexError::InvalidData(format!("Invalid BitPaddedInt: {}", e)))
}

/// Configuration for saving ID3 tags with advanced options
///
/// Controls how ID3v2 tags are serialized to disk, including the output
/// version, multi-value separator for v2.3, padding strategy, and whether
/// to preserve unknown frames or write an ID3v1 tag alongside ID3v2.
#[derive(Debug, Clone)]
pub struct ID3SaveConfig {
    /// Target ID3v2 major version (2, 3, or 4)
    pub v2_version: u8,
    /// Target ID3v2 minor version (usually 0)
    pub v2_minor: u8,
    /// Multi-value separator string for ID3v2.3 text frames (default: "/")
    pub v23_sep: String,
    /// Multi-value separator byte for ID3v2.3 text frames
    pub v23_separator: u8,
    /// Padding bytes to add after tag data (`None` = no padding)
    pub padding: Option<usize>,
    /// Whether to merge compatible duplicate frames during save
    pub merge_frames: bool,
    /// Whether to preserve frames not recognized by the library
    pub preserve_unknown: bool,
    /// Whether to apply zlib compression to frame data
    pub compress_frames: bool,
    /// How to handle the ID3v1 tag at the end of the file
    pub write_v1: crate::id3::file::ID3v1SaveOptions,
    /// Whether to apply unsynchronization encoding to the tag
    pub unsync: bool,
    /// Whether to write an extended header
    pub extended_header: bool,
    /// Whether to convert v2.4-only frames to v2.3 equivalents (e.g., TDRC → TYER+TDAT)
    /// Only used for MP3/EasyMP3 path; container formats (AIFF, WAV, etc.) keep TDRC as-is
    pub convert_v24_frames: bool,
}

impl Default for ID3SaveConfig {
    fn default() -> Self {
        Self {
            v2_version: 4,
            v2_minor: 0,
            v23_sep: "/".to_string(),
            v23_separator: b'/',
            padding: Some(1024),
            merge_frames: true,
            preserve_unknown: true,
            compress_frames: false,
            write_v1: crate::id3::file::ID3v1SaveOptions::REMOVE,
            unsync: false,
            extended_header: false,
            convert_v24_frames: false,
        }
    }
}

impl ID3SaveConfig {
    /// Create simple config for backward compatibility with file.rs.
    /// Validates that v2_version is 3 or 4 (the only versions supported for writing).
    pub fn simple(v2_version: u8, v23_sep: Option<String>) -> Result<Self> {
        if v2_version != 3 && v2_version != 4 {
            return Err(AudexError::InvalidData(format!(
                "v2_version must be 3 or 4, got {}",
                v2_version
            )));
        }

        let mut config = Self {
            v2_version,
            ..Default::default()
        };
        if let Some(sep) = v23_sep {
            config.v23_sep = sep;
        }
        Ok(config)
    }
}

/// Extended ID3v2 header with CRC, restrictions, and extra flags
///
/// Present only when the extended header flag is set in [`ID3Header`].
/// Contains optional CRC-32 for data integrity, padding size information,
/// and tag restriction flags (v2.4 only).
#[derive(Debug, Clone)]
pub struct ExtendedID3Header {
    /// Base header with version and primary flags
    pub base: ID3Header,
    /// Size of the extended header in bytes
    pub extended_header_size: Option<u32>,
    /// Extended header flags (format depends on version)
    pub extended_flags: Option<u16>,
    /// Amount of padding after the tag (v2.3)
    pub padding_size: Option<u32>,
    /// CRC-32 of the tag data (if CRC flag is set)
    pub crc32: Option<u32>,
    /// Tag restriction flags (v2.4 only)
    pub restrictions: Option<u8>,
}

impl ExtendedID3Header {
    /// Check if unsynchronization flag is set
    pub fn f_unsynch(&self) -> bool {
        (self.base.flags & 0x80) != 0
    }

    /// Check if extended header flag is set
    pub fn f_extended(&self) -> bool {
        (self.base.flags & 0x40) != 0
    }

    /// Check if experimental flag is set
    pub fn f_experimental(&self) -> bool {
        (self.base.flags & 0x20) != 0
    }

    /// Check if footer flag is set (v2.4 only)
    pub fn f_footer(&self) -> bool {
        if self.base.major_version() >= 4 {
            (self.base.flags & 0x10) != 0
        } else {
            false
        }
    }
}

/// ID3v2 tag container with frame management
///
/// Stores ID3v2 frames in a sorted dictionary (`BTreeMap`) keyed by a
/// hash-key string derived from the frame ID and disambiguating data
/// (e.g. `"TIT2"`, `"TXXX:BARCODE"`, `"APIC:#0"`). Implements the
/// [`Tags`] trait for unified tag access and the [`Metadata`] trait for
/// file I/O.
///
/// Frames are stored as trait objects (`Box<dyn Frame>`) to support the
/// many different frame types (text, URL, picture, comment, etc.).
#[derive(Debug)]
pub struct ID3Tags {
    /// Frame dictionary for direct access (matches _DictProxy__dict)
    pub dict: BTreeMap<String, Box<dyn Frame>>,
    /// Quick lookup by frame ID for compatibility
    pub frames_by_id: HashMap<String, Vec<String>>, // ID -> hash keys
    /// ID3 version (major, minor)
    version: (u8, u8),
    /// Extended header information
    header: Option<ExtendedID3Header>,
    /// Configuration for operations
    config: ID3SaveConfig,
    /// Unknown frames data
    pub unknown_frames: Vec<Vec<u8>>,
    /// Version for unknown frames
    pub _unknown_v2_version: u8,
    /// Text cache for get() method to return references (with interior mutability)
    text_cache: RefCell<HashMap<String, Vec<String>>>,
    /// Tag flags from header (unsynchronization, extended header, experimental, footer)
    pub f_flags: u8,
    /// Total tag size (including header)
    pub size: u32,
    /// Filename the tag was loaded from or will be saved to
    pub filename: Option<std::path::PathBuf>,
}

impl ID3Tags {
    /// Create new empty ID3Tags with default ID3v2.4 configuration
    pub fn new() -> Self {
        Self {
            dict: BTreeMap::new(),
            frames_by_id: HashMap::new(),
            version: (2, 4), // Default to ID3v2.4
            header: None,

            config: ID3SaveConfig::default(),
            unknown_frames: Vec::new(),
            _unknown_v2_version: 4,
            text_cache: RefCell::new(HashMap::new()),
            f_flags: 0,
            size: 0,
            filename: None,
        }
    }

    /// Create ID3Tags with specific version
    pub fn with_version(major: u8, minor: u8) -> Self {
        let config = ID3SaveConfig {
            v2_version: major,
            v2_minor: minor,
            ..Default::default()
        };

        Self {
            dict: BTreeMap::new(),
            frames_by_id: HashMap::new(),
            version: (major, minor),
            header: None,

            config,
            unknown_frames: Vec::new(),
            _unknown_v2_version: major,
            text_cache: RefCell::new(HashMap::new()),
            f_flags: 0,
            size: 0,
            filename: None,
        }
    }

    /// Create ID3Tags with custom configuration
    pub fn with_config(config: ID3SaveConfig) -> Self {
        let version = (config.v2_version, config.v2_minor);
        Self {
            dict: BTreeMap::new(),
            frames_by_id: HashMap::new(),
            version,
            header: None,

            config,
            unknown_frames: Vec::new(),
            _unknown_v2_version: version.0,
            text_cache: RefCell::new(HashMap::new()),
            f_flags: 0,
            size: 0,
            filename: None,
        }
    }

    /// Get the number of frames in the tag
    pub fn len(&self) -> usize {
        self.dict.len()
    }

    /// Check if the tag is empty
    pub fn is_empty(&self) -> bool {
        self.dict.is_empty()
    }

    /// Parse ID3Tags from raw tag data with advanced error recovery
    pub fn from_data(data: &[u8], header: &ID3Header) -> Result<Self> {
        // ID3 header stores version as (major, revision) where major is 2, 3, or 4
        // But ID3Tags uses (2, major) format, so convert: v2.3.0 -> (2, 3)
        let mut tags = Self::with_version(2, header.major_version());

        // Set header flags and size from the parsed header
        tags.f_flags = header.flags;
        tags.size = header.size;

        // Skip the ID3v2 header (10 bytes: "ID3" + version + revision + flags + size)
        // The data passed to this function includes the full ID3v2 tag including the header
        let header_size = 10;

        // Parse extended header if present (starts after the 10-byte ID3 header)
        let extended_header = if header.has_extended_header() {
            Some(tags._parse_extended_header(&data[header_size..], header)?)
        } else {
            None
        };
        tags.header = extended_header;

        // Calculate final position for frame data (ID3 header + extended header if present)
        let extended_header_offset = tags._get_frame_data_start(&data[header_size..], header)?;
        let final_frame_pos = header_size
            .checked_add(extended_header_offset)
            .ok_or_else(|| {
                AudexError::InvalidData(
                    "ID3 header size + extended header offset overflows".to_string(),
                )
            })?;

        // Parse frames starting from the correct position
        if final_frame_pos >= data.len() {
            return Ok(tags);
        }
        tags._read_frames(&data[final_frame_pos..], header)?;

        // Convert v2.3 (and older) frames to v2.4.
        // This merges TYER+TDAT+TIME → TDRC, TORY → TDOR, IPLS → TIPL,
        // and removes obsolete frames (RVAD, EQUA, TRDA, TSIZ, etc.).
        if tags.version.1 < 4 {
            tags.update_to_v24();
        }

        Ok(tags)
    }

    /// Serialize all frames to bytes for writing to disk.
    ///
    /// Frames are sorted by priority (core metadata first, large binary
    /// frames last) and serialized according to `config`. Unknown frames
    /// are appended if the target version matches the source version.
    pub(crate) fn write_tags(&self, config: &ID3SaveConfig) -> Result<Vec<u8>> {
        // Sort frames by importance
        let order = ["TIT2", "TPE1", "TRCK", "TALB", "TPOS", "TDRC", "TCON"];

        let mut frame_data_pairs: Vec<(String, Vec<u8>)> = Vec::new();

        // Collect frame data
        for frame in self.dict.values() {
            let data = self.save_frame_boxed(frame.as_ref(), None, config)?;
            if data.is_empty() {
                continue; // Skip empty frame data
            }
            frame_data_pairs.push((frame.frame_id().to_string(), data));
        }

        // Sort frames by priority
        frame_data_pairs.sort_by(|a, b| {
            let frame_id_a = &a.0;
            let frame_id_b = &b.0;

            let prio_a = order
                .iter()
                .position(|&id| id == frame_id_a)
                .unwrap_or_else(|| {
                    if frame_id_a == "APIC" {
                        order.len() + 1 // APIC frames placed last due to size
                    } else {
                        order.len()
                    }
                });

            let prio_b = order
                .iter()
                .position(|&id| id == frame_id_b)
                .unwrap_or_else(|| {
                    if frame_id_b == "APIC" {
                        order.len() + 1 // APIC frames placed last due to size
                    } else {
                        order.len()
                    }
                });

            prio_a
                .cmp(&prio_b)
                .then_with(|| {
                    // For APIC frames, preserve order (secondary key)
                    if frame_id_a == "APIC" && frame_id_b == "APIC" {
                        std::cmp::Ordering::Equal // Preserve original order
                    } else {
                        b.1.len().cmp(&a.1.len()) // Larger frames first
                    }
                })
                .then_with(|| {
                    // Use frame hash key for stable sorting
                    frame_id_a.cmp(frame_id_b)
                })
        });

        let mut result = Vec::new();

        // Add sorted frame data
        for (_, data) in frame_data_pairs {
            result.extend_from_slice(&data);
        }

        // Add unknown frames if version matches
        if self._unknown_v2_version == config.v2_version {
            for unknown_data in &self.unknown_frames {
                if unknown_data.len() > 10 {
                    result.extend_from_slice(unknown_data);
                }
            }
        }

        Ok(result)
    }

    /// Save boxed frame to bytes
    pub fn save_frame_boxed(
        &self,
        frame: &dyn Frame,
        _name: Option<&str>,
        _config: &ID3SaveConfig,
    ) -> Result<Vec<u8>> {
        // Check for empty TextFrame
        if frame.frame_id().starts_with('T') {
            if let Some(text) = self._extract_text_from_frame(frame) {
                if text.is_empty() {
                    return Ok(Vec::new()); // Skip empty text frames
                }
            }
        }

        // Generate frame bytes with header
        let frame_data = match frame.to_data() {
            Ok(data) => data,
            Err(_) => return Ok(Vec::new()), // Skip frames that fail to serialize
        };

        if frame_data.is_empty() {
            return Ok(Vec::new()); // Skip empty frames
        }

        // Validate frame data length fits in u32 before building the header
        let frame_data_size = u32::try_from(frame_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "Frame '{}' data too large for u32 size field: {} bytes",
                frame.frame_id(),
                frame_data.len()
            ))
        })?;

        // Build complete frame with header
        let mut result = Vec::new();

        // Frame ID (4 bytes)
        let frame_id = frame.frame_id();
        result.extend_from_slice(frame_id.as_bytes());

        // Preserve original frame flags and apply write-side transformations
        // (compression, unsync, etc.) so round-tripped frames retain their
        // original encoding properties.
        let flags = frame.frame_flags();
        let version = (2u8, _config.v2_version);
        let header = FrameHeader::new(
            frame.frame_id().to_string(),
            frame_data_size,
            flags.to_raw(version),
            version,
        );
        // Apply write-side transformations (compression, unsync, etc.)
        // Errors must propagate so the caller knows the frame could not be encoded.
        let frame_data = FrameProcessor::process_write(&header, frame_data)?;

        // Validate processed frame data length also fits in u32
        let size = u32::try_from(frame_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "Frame '{}' processed data too large for u32 size field: {} bytes",
                frame.frame_id(),
                frame_data.len()
            ))
        })?;

        // Frame size (4 bytes) - synchsafe for ID3v2.4, regular for v2.3
        if _config.v2_version == 4 {
            let synchsafe = BitPaddedInt::to_str(size, Some(7), Some(true), Some(4), Some(4))
                .map_err(|_| {
                    AudexError::InvalidData(format!(
                        "Frame '{}' size {} exceeds synchsafe encoding capacity",
                        frame.frame_id(),
                        size,
                    ))
                })?;
            result.extend_from_slice(&synchsafe);
        } else {
            result.extend_from_slice(&size.to_be_bytes());
        }

        // Frame flags (2 bytes) — use the frame's original flags
        let raw_flags = flags.to_raw(version);
        result.extend_from_slice(&raw_flags.to_be_bytes());

        // Frame data (with any flag-driven transformations applied)
        result.extend_from_slice(&frame_data);

        Ok(result)
    }

    /// Save frame to bytes
    pub fn save_frame(
        &self,
        frame: &dyn Frame,
        _name: Option<&str>,
        config: &ID3SaveConfig,
    ) -> Result<Vec<u8>> {
        // Check for empty TextFrame
        if frame.frame_id().starts_with('T') {
            if let Some(text) = self._extract_text_from_frame_ref(frame) {
                if text.is_empty() {
                    return Ok(Vec::new()); // Skip empty text frames
                }
            }
        }

        let frame_data = match frame.to_data() {
            Ok(data) => data,
            Err(_) => return Ok(Vec::new()), // Skip frames that can't serialize
        };

        let frame_id = frame.frame_id();

        // Validate frame data length fits in u32 before building the header
        let frame_data_size = u32::try_from(frame_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "Frame '{}' data too large for u32 size field: {} bytes",
                frame_id,
                frame_data.len()
            ))
        })?;

        // Preserve original frame flags and apply write-side transformations
        let flags = frame.frame_flags();
        let version = (2u8, config.v2_version);
        let frame_header = FrameHeader::new(
            frame_id.to_string(),
            frame_data_size,
            flags.to_raw(version),
            version,
        );
        // Apply write-side transformations (compression, unsync, etc.)
        // Errors must propagate so the caller knows the frame could not be encoded.
        let frame_data = FrameProcessor::process_write(&frame_header, frame_data)?;
        let raw_flags = flags.to_raw(version);

        // Validate processed frame data length fits in u32
        let size = u32::try_from(frame_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "Frame '{}' processed data too large for u32 size field: {} bytes",
                frame_id,
                frame_data.len()
            ))
        })?;

        // Determine frame header format based on version
        let mut header = Vec::new();

        match config.v2_version {
            4 => {
                // ID3v2.4: use synchsafe integers (7-bit)
                header.extend_from_slice(frame_id.as_bytes());
                header.extend_from_slice(
                    &BitPaddedInt::to_str(size, Some(7), Some(true), Some(4), Some(4)).map_err(
                        |_| {
                            AudexError::InvalidData(format!(
                                "Frame '{}' size {} exceeds synchsafe encoding capacity",
                                frame_id, size,
                            ))
                        },
                    )?,
                );
                header.extend_from_slice(&raw_flags.to_be_bytes());
            }
            3 => {
                // ID3v2.3: use regular integers (8-bit)
                header.extend_from_slice(frame_id.as_bytes());
                header.extend_from_slice(&size.to_be_bytes());
                header.extend_from_slice(&raw_flags.to_be_bytes());
            }
            2 => {
                // ID3v2.2: 3-byte frame ID, 3-byte size (no flags in v2.2).
                // Auto-downgrade v2.3/v2.4 4-char IDs to their 3-char equivalents.
                let v22_id = if frame_id.len() == 3 {
                    Some(frame_id.to_string())
                } else if frame_id.len() == 4 {
                    crate::id3::util::downgrade_frame_id(frame_id)
                } else {
                    None
                };
                if let Some(id) = v22_id {
                    if size > 0x00FF_FFFF {
                        return Err(AudexError::InvalidData(format!(
                            "Frame '{}' size {} exceeds the ID3v2.2 24-bit size limit",
                            frame_id, size
                        )));
                    }
                    header.extend_from_slice(id.as_bytes());
                    let size_bytes = size.to_be_bytes();
                    header.extend_from_slice(&size_bytes[1..4]);
                } else {
                    return Err(AudexError::InvalidData(format!(
                        "Frame '{}' cannot be represented in ID3v2.2",
                        frame_id
                    )));
                }
            }
            _ => return Ok(Vec::new()), // Unsupported version
        }

        let mut result = header;
        result.extend_from_slice(&frame_data);
        Ok(result)
    }

    /// Parse extended header with version-specific logic - simplified
    fn _parse_extended_header(
        &self,
        _data: &[u8],
        header: &ID3Header,
    ) -> Result<ExtendedID3Header> {
        Ok(ExtendedID3Header {
            base: header.clone(),
            extended_header_size: None,
            extended_flags: None,
            padding_size: None,
            crc32: None,
            restrictions: None,
        })
    }

    /// Parse ID3v1 tag with enhanced field extraction
    pub fn from_id3v1(data: &[u8]) -> Result<Self> {
        if data.len() != 128 || &data[0..3] != b"TAG" {
            return Err(AudexError::InvalidData("Invalid ID3v1 tag".to_string()));
        }

        let mut tags = Self::with_version(1, 0);

        // Extract fields with proper null termination handling
        let title = Self::extract_id3v1_string(&data[3..33]);
        let artist = Self::extract_id3v1_string(&data[33..63]);
        let album = Self::extract_id3v1_string(&data[63..93]);
        let year = Self::extract_id3v1_string(&data[93..97]);

        // Handle comment field (may include track number in ID3v1.1)
        let (comment, track_number) = if data[125] == 0 && data[126] != 0 {
            // ID3v1.1 format with track number
            (Self::extract_id3v1_string(&data[97..125]), Some(data[126]))
        } else {
            // Standard ID3v1 comment
            (Self::extract_id3v1_string(&data[97..127]), None)
        };

        let genre = data[127];

        // Add frames without strict validation — ID3v1 uses version 1.0 internally
        // but stores data as ID3v2-style frames, so version-based checks don't apply.
        if !title.is_empty() {
            let frame = TextFrame::new("TIT2".to_string(), vec![title]);
            tags.add_frame(Box::new(frame), false)?;
        }
        if !artist.is_empty() {
            let frame = TextFrame::new("TPE1".to_string(), vec![artist]);
            tags.add_frame(Box::new(frame), false)?;
        }
        if !album.is_empty() {
            let frame = TextFrame::new("TALB".to_string(), vec![album]);
            tags.add_frame(Box::new(frame), false)?;
        }
        if !year.is_empty() {
            let frame = TextFrame::new("TDRC".to_string(), vec![year]);
            tags.add_frame(Box::new(frame), false)?;
        }
        if !comment.is_empty() {
            let frame = TextFrame::new("COMM".to_string(), vec![comment]);
            tags.add_frame(Box::new(frame), false)?;
        }
        if let Some(track) = track_number {
            let frame = TextFrame::new("TRCK".to_string(), vec![track.to_string()]);
            tags.add_frame(Box::new(frame), false)?;
        }

        // Add genre
        if let Some(genre_name) = crate::constants::get_genre(genre) {
            let frame = TextFrame::new("TCON".to_string(), vec![genre_name.to_string()]);
            tags.add_frame(Box::new(frame), false)?;
        }

        Ok(tags)
    }

    /// Extract null-terminated string from ID3v1 field.
    /// Decodes as Latin-1 (ISO 8859-1): each byte maps directly to its Unicode code point.
    fn extract_id3v1_string(data: &[u8]) -> String {
        let null_pos = data.iter().position(|&b| b == 0).unwrap_or(data.len());
        data[..null_pos]
            .iter()
            .map(|&b| b as char)
            .collect::<String>()
            .trim()
            .to_string()
    }

    /// Add frame with validation
    pub fn add(&mut self, frame: Box<dyn Frame>) -> Result<()> {
        self.add_frame(frame, true)
    }

    /// Internal add method with sophisticated frame merging
    pub(crate) fn add_frame(&mut self, frame: Box<dyn Frame>, strict: bool) -> Result<()> {
        // Validate frame before adding
        if strict {
            self._validate_frame(frame.as_ref())?;
        }

        // Generate hash key for this frame (may include more than just frame ID)
        let hash_key = self._generate_frame_hash_key(frame.as_ref());

        // Check if frame already exists at this hash key
        if let Some(existing_frame) = self.dict.remove(&hash_key) {
            // Frame conflict detected - attempt to merge
            let merged_frame = if self.config.merge_frames && !strict {
                self._try_merge_frames(existing_frame, frame)?
            } else {
                // Replace existing with new frame
                frame
            };

            // Insert the merged/replacement frame
            let frame_id = merged_frame.frame_id().to_string();
            self.dict.insert(hash_key.clone(), merged_frame);

            // Update frame ID lookup — remove any stale duplicate before inserting
            let vec = self.frames_by_id.entry(frame_id).or_default();
            vec.retain(|k| k != &hash_key);
            vec.push(hash_key);
        } else {
            // No conflict - add new frame
            let frame_id = frame.frame_id().to_string();
            self.dict.insert(hash_key.clone(), frame);

            // Update frame ID lookup
            self.frames_by_id
                .entry(frame_id)
                .or_default()
                .push(hash_key);
        }

        Ok(())
    }

    /// Generate sophisticated hash key for frame identification
    /// This enables multiple frames of same type with different attributes
    fn _generate_frame_hash_key(&self, frame: &dyn Frame) -> String {
        // Use the frame's own hash_key method, which knows how to generate the correct key
        frame.hash_key()
    }

    /// Attempt to merge two frames of the same type
    fn _try_merge_frames(
        &self,
        mut existing: Box<dyn Frame>,
        new: Box<dyn Frame>,
    ) -> Result<Box<dyn Frame>> {
        // Extract information before attempting merge
        let frame_id = existing.frame_id().to_string();
        let _is_text_frame = frame_id.starts_with('T') && frame_id != "TXXX";

        // Try to merge using the frame's built-in merge capability
        // Since merge_frame consumes the new frame, we need to handle this carefully
        let result = existing.merge_frame(new);

        match result {
            Ok(()) => Ok(existing),
            Err(e) => Err(AudexError::InvalidData(format!(
                "Failed to merge frame '{}': {}. The new frame would be lost.",
                frame_id, e,
            ))),
        }
    }

    /// Merge text frames by concatenating values
    fn _merge_text_frames(
        &self,
        existing: Box<dyn Frame>,
        new: Box<dyn Frame>,
    ) -> Result<Box<dyn Frame>> {
        // Extract text from both frames
        let existing_text = self
            ._extract_text_from_frame(existing.as_ref())
            .unwrap_or_default();
        let new_text = self
            ._extract_text_from_frame(new.as_ref())
            .unwrap_or_default();

        // Combine texts with appropriate separator
        let separator = &self.config.v23_sep;
        let combined_text = if existing_text.is_empty() {
            new_text
        } else if new_text.is_empty() {
            existing_text
        } else {
            format!("{}{}{}", existing_text, separator, new_text)
        };

        // Create new merged text frame
        let merged_frame = TextFrame::new(existing.frame_id().to_string(), vec![combined_text]);
        Ok(Box::new(merged_frame))
    }

    /// Extract description from TXXX frame
    fn _extract_txxx_description(&self, frame: &dyn Frame) -> Option<String> {
        // For TXXX frames, extract description from the frame's text data
        // TXXX format: [encoding][description][null][text]
        if frame.frame_id() == "TXXX" {
            // Try to get raw data and parse it
            if let Ok(data) = frame.to_data() {
                if data.len() > 1 {
                    let Ok(encoding) = TextEncoding::from_byte(data[0]) else {
                        return Some("".to_string());
                    };
                    let text_data = &data[1..];
                    let null_term = encoding.null_terminator();

                    // Find first null terminator to separate description from text
                    let desc_end = if null_term.len() == 1 {
                        text_data
                            .iter()
                            .position(|&b| b == null_term[0])
                            .unwrap_or(text_data.len())
                    } else {
                        // Odd-length UTF-16 data is malformed; treat as unsplittable
                        if text_data.len() % 2 != 0 {
                            text_data.len()
                        } else {
                            (0..text_data.len().saturating_sub(1))
                                .step_by(2)
                                .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                                .unwrap_or(text_data.len())
                        }
                    };

                    if let Ok(description) = encoding.decode_text(&text_data[..desc_end]) {
                        return Some(description);
                    }
                }
            }
        }
        Some("".to_string())
    }

    /// Extract description from WXXX frame
    fn _extract_wxxx_description(&self, frame: &dyn Frame) -> Option<String> {
        // For WXXX frames, extract description
        // WXXX format: [encoding][description][null][url]
        if frame.frame_id() == "WXXX" {
            if let Ok(data) = frame.to_data() {
                if data.len() > 1 {
                    let Ok(encoding) = TextEncoding::from_byte(data[0]) else {
                        return Some("".to_string());
                    };
                    let text_data = &data[1..];
                    let null_term = encoding.null_terminator();

                    // Find description end
                    let desc_end = if null_term.len() == 1 {
                        text_data
                            .iter()
                            .position(|&b| b == null_term[0])
                            .unwrap_or(text_data.len())
                    } else {
                        // Odd-length UTF-16 data is malformed; treat as unsplittable
                        if text_data.len() % 2 != 0 {
                            text_data.len()
                        } else {
                            (0..text_data.len().saturating_sub(1))
                                .step_by(2)
                                .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                                .unwrap_or(text_data.len())
                        }
                    };

                    if let Ok(description) = encoding.decode_text(&text_data[..desc_end]) {
                        return Some(description);
                    }
                }
            }
        }
        Some("".to_string())
    }

    /// Extract language and description from COMM frame
    fn _extract_comm_lang_description(&self, frame: &dyn Frame) -> (String, String) {
        if frame.frame_id() == "COMM" {
            if let Ok(data) = frame.to_data() {
                if data.len() >= 5 {
                    // COMM format: [encoding][language][description][null][text]
                    let Ok(encoding) = TextEncoding::from_byte(data[0]) else {
                        return ("eng".to_string(), "".to_string());
                    };
                    let language = String::from_utf8_lossy(&data[1..4]).into_owned();
                    let text_data = &data[4..];
                    let null_term = encoding.null_terminator();

                    // Find description end
                    let desc_end = if null_term.len() == 1 {
                        text_data
                            .iter()
                            .position(|&b| b == null_term[0])
                            .unwrap_or(text_data.len())
                    } else {
                        // Odd-length UTF-16 data is malformed; treat as unsplittable
                        if text_data.len() % 2 != 0 {
                            text_data.len()
                        } else {
                            (0..text_data.len().saturating_sub(1))
                                .step_by(2)
                                .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                                .unwrap_or(text_data.len())
                        }
                    };

                    if let Ok(description) = encoding.decode_text(&text_data[..desc_end]) {
                        return (language, description);
                    }
                }
            }
        }
        ("eng".to_string(), "".to_string())
    }

    /// Extract language and description from USLT frame
    fn _extract_uslt_lang_description(&self, frame: &dyn Frame) -> (String, String) {
        if frame.frame_id() == "USLT" {
            if let Ok(data) = frame.to_data() {
                if data.len() >= 5 {
                    // USLT format: [encoding][language][description][null][lyrics]
                    let Ok(encoding) = TextEncoding::from_byte(data[0]) else {
                        return ("eng".to_string(), "".to_string());
                    };
                    let language = String::from_utf8_lossy(&data[1..4]).into_owned();
                    let text_data = &data[4..];
                    let null_term = encoding.null_terminator();

                    // Find description end
                    let desc_end = if null_term.len() == 1 {
                        text_data
                            .iter()
                            .position(|&b| b == null_term[0])
                            .unwrap_or(text_data.len())
                    } else {
                        // Odd-length UTF-16 data is malformed; treat as unsplittable
                        if text_data.len() % 2 != 0 {
                            text_data.len()
                        } else {
                            (0..text_data.len().saturating_sub(1))
                                .step_by(2)
                                .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                                .unwrap_or(text_data.len())
                        }
                    };

                    if let Ok(description) = encoding.decode_text(&text_data[..desc_end]) {
                        return (language, description);
                    }
                }
            }
        }
        ("eng".to_string(), "".to_string())
    }

    /// Extract picture type from APIC frame
    fn _extract_apic_picture_type(&self, frame: &dyn Frame) -> Option<String> {
        if frame.frame_id() == "APIC" {
            if let Ok(data) = frame.to_data() {
                if data.len() > 1 {
                    // APIC format: [encoding][mime_type][null][picture_type][description][null][picture_data]
                    let mut offset = 1; // Skip encoding byte

                    // Find MIME type end (null-terminated)
                    if let Some(mime_end) = data[offset..].iter().position(|&b| b == 0) {
                        offset += mime_end + 1; // Skip MIME type and null

                        // Picture type is next byte
                        if offset < data.len() {
                            let picture_type = data[offset];
                            return Some(picture_type.to_string());
                        }
                    }
                }
            }
        }
        Some("0".to_string()) // Default to "Other"
    }

    /// Extract filename and description from GEOB frame
    fn _extract_geob_filename_description(&self, frame: &dyn Frame) -> (String, String) {
        if frame.frame_id() == "GEOB" {
            if let Ok(data) = frame.to_data() {
                if data.len() > 1 {
                    // GEOB format: [encoding][mime_type][null][filename][null][description][null][data]
                    let Ok(encoding) = TextEncoding::from_byte(data[0]) else {
                        return ("".to_string(), "".to_string());
                    };
                    let mut offset = 1;

                    // Skip MIME type
                    if let Some(mime_end) = data[offset..].iter().position(|&b| b == 0) {
                        offset += mime_end + 1;

                        let remaining = &data[offset..];
                        let null_term = encoding.null_terminator();

                        // Find filename end
                        let filename_end = if null_term.len() == 1 {
                            remaining
                                .iter()
                                .position(|&b| b == null_term[0])
                                .unwrap_or(remaining.len())
                        } else {
                            // Odd-length UTF-16 data is malformed; treat as unsplittable
                            if remaining.len() % 2 != 0 {
                                remaining.len()
                            } else {
                                (0..remaining.len().saturating_sub(1))
                                    .step_by(2)
                                    .find(|&i| remaining[i] == 0 && remaining[i + 1] == 0)
                                    .unwrap_or(remaining.len())
                            }
                        };

                        let filename = encoding
                            .decode_text(&remaining[..filename_end])
                            .unwrap_or_default();
                        let desc_start = filename_end + null_term.len();

                        if desc_start < remaining.len() {
                            let desc_data = &remaining[desc_start..];
                            let desc_end = if null_term.len() == 1 {
                                desc_data
                                    .iter()
                                    .position(|&b| b == null_term[0])
                                    .unwrap_or(desc_data.len())
                            } else {
                                // Odd-length UTF-16 data is malformed; treat as unsplittable
                                if desc_data.len() % 2 != 0 {
                                    desc_data.len()
                                } else {
                                    (0..desc_data.len().saturating_sub(1))
                                        .step_by(2)
                                        .find(|&i| desc_data[i] == 0 && desc_data[i + 1] == 0)
                                        .unwrap_or(desc_data.len())
                                }
                            };

                            let description = encoding
                                .decode_text(&desc_data[..desc_end])
                                .unwrap_or_default();
                            return (filename, description);
                        }
                    }
                }
            }
        }
        ("".to_string(), "".to_string())
    }

    /// Extract email from POPM frame
    fn _extract_popm_email(&self, frame: &dyn Frame) -> Option<String> {
        if frame.frame_id() == "POPM" {
            if let Ok(data) = frame.to_data() {
                // POPM format: [email][null][rating][count]
                if let Some(email_end) = data.iter().position(|&b| b == 0) {
                    if let Ok(email) = String::from_utf8(data[..email_end].to_vec()) {
                        return Some(email);
                    }
                }
            }
        }
        Some("".to_string())
    }

    /// Extract identification from RVA2 frame
    fn _extract_rva2_identification(&self, frame: &dyn Frame) -> Option<String> {
        if frame.frame_id() == "RVA2" {
            if let Ok(data) = frame.to_data() {
                // RVA2 format: [identification][null][channel_data...]
                if let Some(id_end) = data.iter().position(|&b| b == 0) {
                    if let Ok(identification) = String::from_utf8(data[..id_end].to_vec()) {
                        return Some(identification);
                    }
                }
            }
        }
        Some("".to_string())
    }

    /// Extract owner identifier from PRIV frame
    fn _extract_priv_owner(&self, frame: &dyn Frame) -> Option<String> {
        if frame.frame_id() == "PRIV" {
            if let Ok(data) = frame.to_data() {
                // PRIV format: [owner][null][private_data]
                if let Some(owner_end) = data.iter().position(|&b| b == 0) {
                    if let Ok(owner) = String::from_utf8(data[..owner_end].to_vec()) {
                        return Some(owner);
                    }
                }
            }
        }
        Some("".to_string())
    }

    /// Extract element ID from CHAP frame
    fn _extract_chap_element_id(&self, frame: &dyn Frame) -> Option<String> {
        if frame.frame_id() == "CHAP" {
            if let Ok(data) = frame.to_data() {
                // CHAP format: [element_id][null][start_time][end_time][start_offset][end_offset][sub_frames...]
                if let Some(id_end) = data.iter().position(|&b| b == 0) {
                    if let Ok(element_id) = String::from_utf8(data[..id_end].to_vec()) {
                        return Some(element_id);
                    }
                }
            }
        }
        Some("".to_string())
    }

    /// Extract element ID from CTOC frame
    fn _extract_ctoc_element_id(&self, frame: &dyn Frame) -> Option<String> {
        if frame.frame_id() == "CTOC" {
            if let Ok(data) = frame.to_data() {
                // CTOC format: [element_id][null][flags][entry_count][child_ids...][sub_frames...]
                if let Some(id_end) = data.iter().position(|&b| b == 0) {
                    if let Ok(element_id) = String::from_utf8(data[..id_end].to_vec()) {
                        return Some(element_id);
                    }
                }
            }
        }
        Some("".to_string())
    }

    /// Get next APIC index for uniqueness
    fn _get_next_apic_index(&self) -> usize {
        self.dict.keys().filter(|k| k.starts_with("APIC:")).count()
    }

    /// Get next CHAP index for uniqueness
    fn _get_next_chap_index(&self) -> usize {
        self.dict.keys().filter(|k| k.starts_with("CHAP:")).count()
    }

    /// Get next CTOC index for uniqueness
    fn _get_next_ctoc_index(&self) -> usize {
        self.dict.keys().filter(|k| k.starts_with("CTOC:")).count()
    }

    /// Validate frame for current version and configuration
    fn _validate_frame(&self, frame: &dyn Frame) -> Result<()> {
        let frame_id = frame.frame_id();

        // Check version compatibility
        match (self.version.0, self.version.1) {
            (2, 2) => {
                // ID3v2.2 validation
                if frame_id.len() != 3 {
                    return Err(AudexError::InvalidData(format!(
                        "Invalid ID3v2.2 frame ID: {}",
                        frame_id
                    )));
                }
            }
            (2, 3) | (2, 4) => {
                // ID3v2.3/2.4 validation
                if frame_id.len() != 4 {
                    return Err(AudexError::InvalidData(format!(
                        "Invalid ID3v2.{} frame ID: {}",
                        self.version.1, frame_id
                    )));
                }

                // Check v2.4-only frames in v2.3
                if self.version.1 == 3 {
                    let v24_only = [
                        "ASPI", "EQU2", "RVA2", "SEEK", "SIGN", "TDEN", "TDOR", "TDRC", "TDRL",
                        "TDTG", "TIPL", "TMCL", "TMOO", "TPRO", "TSOA", "TSOP", "TSOT", "TSST",
                    ];
                    if v24_only.contains(&frame_id) {
                        return Err(AudexError::InvalidData(format!(
                            "Frame {} not supported in ID3v2.3",
                            frame_id
                        )));
                    }
                }
            }
            _ => {
                return Err(AudexError::InvalidData(format!(
                    "Unsupported ID3 version: {}.{}",
                    self.version.0, self.version.1
                )));
            }
        }

        Ok(())
    }

    /// Get all frames matching a pattern.
    /// Supports pattern matching like "TXXX:CustomField:" for specific user frames.
    ///
    /// Supports exact key matching (e.g. `"TIT2"`) and pattern matching
    /// with `:` separators (e.g. `"TXXX:BARCODE"` for TXXX frames with
    /// a specific description).
    pub fn getall(&self, key: &str) -> Vec<&dyn Frame> {
        let mut results = Vec::new();

        // Check if pattern matching is needed (contains ':')
        if key.contains(':') {
            // Pattern matching for complex keys like "TXXX:description"
            for (hash_key, frame) in &self.dict {
                if self._key_matches_pattern(hash_key, key) {
                    results.push(frame.as_ref());
                }
            }
        } else {
            // Simple frame ID lookup - collect frames that match the frame ID
            for (hash_key, frame) in &self.dict {
                if hash_key == key || hash_key.starts_with(&(key.to_owned() + ":")) {
                    results.push(frame.as_ref());
                }
            }
        }

        results
    }

    /// Check if hash key matches pattern
    fn _key_matches_pattern(&self, hash_key: &str, pattern: &str) -> bool {
        if pattern.ends_with(':') {
            // Prefix matching like "TXXX:CustomField:"
            hash_key.starts_with(pattern)
        } else {
            // Exact matching
            hash_key == pattern
        }
    }

    /// Set all frames of a given type.
    /// Replace all frames matching a key with the given frames.
    ///
    /// First removes all existing frames for `key`, then adds the
    /// provided frames. Use this for bulk frame replacement.
    pub fn setall(&mut self, key: &str, frames: Vec<Box<dyn Frame>>) -> Result<()> {
        for frame in &frames {
            self._validate_frame(frame.as_ref())?;
        }

        // Remove existing frames first
        self.delall(key);

        // Add new frames
        for frame in frames {
            self.add(frame)?;
        }

        Ok(())
    }

    /// Delete all frames matching a pattern.
    pub fn delall(&mut self, key: &str) {
        let mut keys_to_remove = Vec::new();

        // Find matching hash keys
        if key.contains(':') {
            // Pattern matching
            for hash_key in self.dict.keys() {
                if self._key_matches_pattern(hash_key, key) {
                    keys_to_remove.push(hash_key.clone());
                }
            }
        } else {
            // Simple frame ID lookup
            if let Some(hash_keys) = self.frames_by_id.get(key) {
                keys_to_remove.extend(hash_keys.clone());
            }
        }

        // Remove frames
        for hash_key in &keys_to_remove {
            self.dict.remove(hash_key);
        }

        // Update frame ID lookup
        if !key.contains(':') {
            self.frames_by_id.remove(key);
        } else {
            // Remove hash keys from frame ID lookup for pattern matches
            for hash_key in &keys_to_remove {
                for (_, hash_key_list) in self.frames_by_id.iter_mut() {
                    hash_key_list.retain(|hk| hk != hash_key);
                }
                // Remove empty entries
                self.frames_by_id.retain(|_, v| !v.is_empty());
            }
        }
    }

    /// Get frames by frame ID (compatibility method)
    /// Returns Option for backward compatibility with existing code
    /// Get all frames with the given frame ID, or `None` if no matches.
    ///
    /// Convenience wrapper around [`getall`](Self::getall) that returns
    /// `None` instead of an empty vector when no frames match.
    pub fn get_frames(&self, frame_id: &str) -> Option<Vec<&dyn Frame>> {
        let frames = self.getall(frame_id);
        if frames.is_empty() {
            None
        } else {
            Some(frames)
        }
    }

    /// Remove all frames with given ID
    pub fn remove(&mut self, frame_id: &str) {
        self.delall(frame_id);
    }

    /// Add a simple text frame (compatibility method)
    pub fn add_text_frame(&mut self, frame_id: &str, text: Vec<String>) -> Result<()> {
        let frame = TextFrame::new(frame_id.to_string(), text);
        self.add(Box::new(frame))
    }

    /// Add a text frame with a specific encoding
    pub fn add_text_frame_with_encoding(
        &mut self,
        frame_id: &str,
        text: Vec<String>,
        encoding: TextEncoding,
    ) -> Result<()> {
        let frame = TextFrame::with_encoding(frame_id.to_string(), encoding, text);
        self.add(Box::new(frame))
    }

    /// Set a tag with a specific text encoding.
    ///
    /// Like `Tags::set()` but allows specifying the encoding for the frame.
    /// Applies to all frame types: text frames, TXXX, COMM, USLT.
    pub fn set_with_encoding(
        &mut self,
        key: &str,
        values: Vec<String>,
        encoding: TextEncoding,
    ) -> Result<()> {
        // Handle TXXX:Description keys
        if let Some(description) = key.strip_prefix("TXXX:") {
            if !values.is_empty() {
                let frame = crate::id3::TXXX::new(encoding, description.to_string(), values);
                self._validate_frame(&frame)?;
                self.delall(key);
                self.add(Box::new(frame))?;
            } else {
                self.delall(key);
            }
            return Ok(());
        }

        // Handle COMM keys
        if key == "COMM" || key.starts_with("COMM:") {
            if let Some(text) = values.into_iter().next() {
                let (desc, lang) = if let Some(rest) = key.strip_prefix("COMM:") {
                    let parts: Vec<&str> = rest.splitn(2, ':').collect();
                    let desc = parts.first().copied().unwrap_or("").to_string();
                    let lang_str = parts.get(1).copied().unwrap_or("eng");
                    let mut lang = [b' '; 3];
                    for (i, b) in lang_str.as_bytes().iter().take(3).enumerate() {
                        lang[i] = *b;
                    }
                    (desc, lang)
                } else {
                    (String::new(), *b"eng")
                };
                let frame = crate::id3::COMM::new(encoding, lang, desc, text);
                self._validate_frame(&frame)?;
                self.delall(key);
                self.add(Box::new(frame))?;
            } else {
                self.delall(key);
            }
            return Ok(());
        }

        // Handle USLT keys
        if key == "USLT" || key.starts_with("USLT:") {
            if let Some(text) = values.into_iter().next() {
                let (desc, lang) = if let Some(rest) = key.strip_prefix("USLT:") {
                    let parts: Vec<&str> = rest.splitn(2, ':').collect();
                    let desc = parts.first().copied().unwrap_or("").to_string();
                    let lang_str = parts.get(1).copied().unwrap_or("eng");
                    let mut lang = [b' '; 3];
                    for (i, b) in lang_str.as_bytes().iter().take(3).enumerate() {
                        lang[i] = *b;
                    }
                    (desc, lang)
                } else {
                    (String::new(), *b"eng")
                };
                let frame = crate::id3::USLT::new(encoding, lang, desc, text);
                self._validate_frame(&frame)?;
                self.delall(key);
                self.add(Box::new(frame))?;
            } else {
                self.delall(key);
            }
            return Ok(());
        }

        // Map common tag names to ID3 frame IDs
        let frame_id = match key.to_uppercase().as_str() {
            "TITLE" => "TIT2",
            "ARTIST" => "TPE1",
            "ALBUM" => "TALB",
            "DATE" => "TDRC",
            "GENRE" => "TCON",
            "TRACKNUMBER" => "TRCK",
            _ => key,
        };

        if !values.is_empty() {
            let frame = TextFrame::with_encoding(frame_id.to_string(), encoding, values);
            self._validate_frame(&frame)?;
            self.remove(frame_id);
            self.add(Box::new(frame))?;
        } else {
            self.remove(frame_id);
        }
        Ok(())
    }

    /// Get text from first frame of given ID
    pub fn get_text(&self, frame_id: &str) -> Option<String> {
        if let Some(frames) = self.get_frames(frame_id) {
            if let Some(frame) = frames.first() {
                // Try to extract text based on frame type
                // Use helper method to extract text
                if let Some(text) = self._extract_text_from_frame(*frame) {
                    Some(text)
                } else {
                    // Fallback to description parsing
                    let desc = frame.description();
                    desc.split(": ").nth(1).map(|text| text.to_string())
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Set text for a frame (replaces existing)
    pub fn set_text(&mut self, frame_id: &str, text: String) -> Result<()> {
        self.remove(frame_id);
        // Split null-separated values into separate text entries
        // so multi-value frames get proper per-value BOM encoding
        let values: Vec<String> = text.split('\0').map(|s| s.to_string()).collect();
        self.add_text_frame(frame_id, values)
    }

    /// Get ID3 version
    pub fn version(&self) -> (u8, u8) {
        self.version
    }

    /// Set ID3 version
    ///
    /// Upgrading from v2.2 to v2.3/v2.4 is supported and automatically converts
    /// three-character frame IDs to their four-character equivalents.
    ///
    /// Downgrading from v2.3/v2.4 to v2.2 is NOT supported. The v2.2 format uses
    /// shorter frame IDs and a different header layout; blindly rewriting v2.3/v2.4
    /// frames into a v2.2 container would produce an invalid tag. Attempting this
    /// will log a warning and leave the frames unconverted.
    pub fn set_version(&mut self, major: u8, minor: u8) {
        let old_version = self.version;

        // Downgrading to v2.2 is not supported and would produce invalid tags.
        if minor == 2 && old_version.1 != 2 {
            warn_event!(
                old_major = old_version.0,
                old_minor = old_version.1,
                "downgrading ID3v2.{}.{} to v2.2 is not supported; \
                 frames will not be converted and the resulting tag may be invalid",
                old_version.0,
                old_version.1
            );
        }

        self.version = (major, minor);

        // If upgrading from v2.2 to v2.3/v2.4, upgrade frame IDs
        if old_version.1 == 2 && (minor == 3 || minor == 4) {
            self._upgrade_all_frames_v22_to_v23();
        }
    }

    /// Get text values for a frame ID
    pub fn get(&self, frame_id: &str) -> Option<Vec<String>> {
        if let Some(hash_keys) = self.frames_by_id.get(frame_id) {
            for hash_key in hash_keys {
                if let Some(frame) = self.dict.get(hash_key) {
                    if let Some(text_values) = frame.text_values() {
                        return Some(text_values);
                    }
                    // Fallback to parsing description
                    let desc = frame.description();
                    if let Some(colon_pos) = desc.find(": ") {
                        let text = desc[colon_pos + 2..].to_string();
                        return Some(vec![text]);
                    }
                }
            }
        }
        None
    }

    /// Get major version number
    pub fn major_version(&self) -> u8 {
        self.version.0
    }

    /// Get minor version number
    pub fn minor_version(&self) -> u8 {
        self.version.1
    }

    /// Check if unsynchronization flag is set
    pub fn unsynchronisation(&self) -> bool {
        if let Some(ref header) = self.header {
            header.f_unsynch()
        } else {
            false
        }
    }

    /// Parse and add a frame from raw data
    pub fn parse_and_add_frame(
        &mut self,
        frame_id: &str,
        data: &[u8],
        _header: &FrameHeader,
    ) -> Result<()> {
        // Try to parse as text frame first (most common)
        if frame_id.starts_with('T') && frame_id != "TXXX" {
            if let Ok(text) = self.parse_text_frame_data(data) {
                // During parsing, use non-strict mode to allow v2.4 frames in v2.3 files for compatibility
                let frame = TextFrame::new(frame_id.to_string(), text);
                self.add_frame(Box::new(frame), false)?; // strict=false for parsing compatibility
                return Ok(());
            }
        }

        // Parse specific frame types - simplified implementation
        match frame_id {
            "TXXX" => {
                // User-defined text frame format: [encoding][description][null][text]
                if data.is_empty() {
                    return Ok(());
                }

                let encoding = TextEncoding::from_byte(data[0])?;
                let null_term = encoding.null_terminator();
                let pos = 1;

                // Find the description field by looking for null terminator
                let desc_end = if null_term.len() == 1 {
                    // Single-byte null terminator (Latin1, UTF-8)
                    data[pos..]
                        .iter()
                        .position(|&b| b == null_term[0])
                        .map(|p| pos + p)
                } else {
                    // Two-byte null terminator (UTF-16)
                    // Odd-length UTF-16 data is malformed; skip null search
                    if (data.len() - pos) % 2 != 0 {
                        None
                    } else {
                        // Step by 2 to maintain UTF-16 alignment
                        (pos..data.len().saturating_sub(1))
                            .step_by(2)
                            .find(|&i| i + 1 < data.len() && &data[i..i + 2] == null_term)
                    }
                };

                let (description, value_start) = if let Some(end_pos) = desc_end {
                    let desc = encoding.decode_text(&data[pos..end_pos])?;
                    (desc, end_pos + null_term.len())
                } else {
                    // No null terminator found - treat entire remaining data as description
                    let desc = encoding.decode_text(&data[pos..])?;
                    (desc, data.len())
                };

                // Parse the value field
                let text_values = if value_start < data.len() {
                    let text_str = encoding.decode_text(&data[value_start..])?;
                    text_str
                        .split('\0')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                } else {
                    vec![]
                };

                let frame =
                    crate::id3::frames::TXXX::new(encoding, description.clone(), text_values);
                let key = format!("TXXX:{}", description);
                self.dict.insert(key.clone(), Box::new(frame));
                self.frames_by_id.entry(key.clone()).or_default().push(key);
                Ok(())
            }
            "COMM" => {
                // Comment frame format: [encoding][language(3)][description][null][text]
                if data.len() < 5 {
                    return Ok(());
                }

                let encoding = TextEncoding::from_byte(data[0])?;
                let language = [data[1], data[2], data[3]];
                let null_term = encoding.null_terminator();
                let pos = 4;

                // Find the description field by looking for null terminator
                let desc_end = if null_term.len() == 1 {
                    // Single-byte null terminator (Latin1, UTF-8)
                    data[pos..]
                        .iter()
                        .position(|&b| b == null_term[0])
                        .map(|p| pos + p)
                } else {
                    // Two-byte null terminator (UTF-16)
                    // Odd-length UTF-16 data is malformed; skip null search
                    if (data.len() - pos) % 2 != 0 {
                        None
                    } else {
                        // Step by 2 to maintain UTF-16 alignment
                        (pos..data.len().saturating_sub(1))
                            .step_by(2)
                            .find(|&i| i + 1 < data.len() && &data[i..i + 2] == null_term)
                    }
                };

                let (description, text_start) = if let Some(end_pos) = desc_end {
                    let desc = encoding.decode_text(&data[pos..end_pos])?;
                    (desc, end_pos + null_term.len())
                } else {
                    // No null terminator found - treat entire remaining data as description
                    let desc = encoding.decode_text(&data[pos..])?;
                    (desc, data.len())
                };

                // Parse the comment text field (strip trailing null terminator)
                let comment_text = if text_start < data.len() {
                    let raw = &data[text_start..];
                    let raw = crate::id3::frames::strip_trailing_null(raw, &encoding);
                    encoding.decode_text(raw)?
                } else {
                    String::new()
                };

                let comm_frame = crate::id3::frames::COMM::new(
                    encoding,
                    language,
                    description.clone(),
                    comment_text,
                );
                // Format key : "COMM::lang" or "COMM:desc:lang"
                let lang_str = String::from_utf8_lossy(&language);
                let key = if description.is_empty() {
                    format!("COMM::{}", lang_str)
                } else {
                    format!("COMM:{}:{}", description, lang_str)
                };
                self.dict.insert(key.clone(), Box::new(comm_frame));
                self.frames_by_id
                    .entry("COMM".to_string())
                    .or_default()
                    .push(key);
                Ok(())
            }
            "USLT" => {
                // Unsynchronized lyrics frame format: [encoding][language(3)][description][null][text]
                if data.len() < 5 {
                    return Ok(());
                }

                let encoding = TextEncoding::from_byte(data[0])?;
                let language = [data[1], data[2], data[3]];
                let null_term = encoding.null_terminator();
                let pos = 4;

                // Find the description field by looking for null terminator
                let desc_end = if null_term.len() == 1 {
                    // Single-byte null terminator (Latin1, UTF-8)
                    data[pos..]
                        .iter()
                        .position(|&b| b == null_term[0])
                        .map(|p| pos + p)
                } else {
                    // Two-byte null terminator (UTF-16)
                    // Odd-length UTF-16 data is malformed; skip null search
                    if (data.len() - pos) % 2 != 0 {
                        None
                    } else {
                        // Step by 2 to maintain UTF-16 alignment
                        (pos..data.len().saturating_sub(1))
                            .step_by(2)
                            .find(|&i| i + 1 < data.len() && &data[i..i + 2] == null_term)
                    }
                };

                let (description, text_start) = if let Some(end_pos) = desc_end {
                    let desc = encoding.decode_text(&data[pos..end_pos])?;
                    (desc, end_pos + null_term.len())
                } else {
                    // No null terminator found - treat entire remaining data as description
                    let desc = encoding.decode_text(&data[pos..])?;
                    (desc, data.len())
                };

                // Parse the lyrics text field (strip trailing null terminator)
                let lyrics_text = if text_start < data.len() {
                    let raw = &data[text_start..];
                    let raw = crate::id3::frames::strip_trailing_null(raw, &encoding);
                    encoding.decode_text(raw)?
                } else {
                    String::new()
                };

                let uslt_frame = crate::id3::frames::USLT::new(
                    encoding,
                    language,
                    description.clone(),
                    lyrics_text,
                );
                // Format key : "USLT::lang" or "USLT:desc:lang"
                let lang_str = String::from_utf8_lossy(&language);
                let key = if description.is_empty() {
                    format!("USLT::{}", lang_str)
                } else {
                    format!("USLT:{}:{}", description, lang_str)
                };
                self.dict.insert(key.clone(), Box::new(uslt_frame));
                self.frames_by_id
                    .entry("USLT".to_string())
                    .or_default()
                    .push(key);
                Ok(())
            }
            "APIC" => {
                // Attached picture frame format: [encoding][mime_type][null][picture_type][description][null][picture_data]
                if data.len() < 5 {
                    return Ok(());
                }

                let encoding = TextEncoding::from_byte(data[0])?;
                let mut pos = 1;

                // Find MIME type (null-terminated Latin1 string)
                let mime_end = data[pos..]
                    .iter()
                    .position(|&b| b == 0)
                    .map(|p| pos + p)
                    .unwrap_or(data.len());
                let mime_type = String::from_utf8_lossy(&data[pos..mime_end]).into_owned();
                pos = mime_end + 1; // Skip null terminator

                if pos >= data.len() {
                    return Ok(()); // Insufficient data
                }

                // Read picture type byte
                let picture_type = crate::id3::frames::PictureType::from(data[pos]);
                pos += 1;

                if pos >= data.len() {
                    return Ok(()); // Insufficient data
                }

                // Find description (null-terminated with encoding-specific terminator)
                let null_term = encoding.null_terminator();

                let desc_end = if null_term.len() == 1 {
                    // Single-byte null terminator (Latin1, UTF-8)
                    data[pos..]
                        .iter()
                        .position(|&b| b == null_term[0])
                        .map(|p| pos + p)
                } else {
                    // Two-byte null terminator (UTF-16)
                    // Odd-length UTF-16 data is malformed; skip null search
                    if (data.len() - pos) % 2 != 0 {
                        None
                    } else {
                        // Step by 2 to maintain UTF-16 alignment
                        (pos..data.len().saturating_sub(1))
                            .step_by(2)
                            .find(|&i| i + 1 < data.len() && &data[i..i + 2] == null_term)
                    }
                };

                let (description, pic_data_start) = if let Some(end_pos) = desc_end {
                    let desc = encoding.decode_text(&data[pos..end_pos])?;
                    (desc, end_pos + null_term.len())
                } else {
                    // No null terminator found - use empty description
                    (String::new(), pos)
                };

                // Remaining data is the picture data
                let picture_data = if pic_data_start < data.len() {
                    data[pic_data_start..].to_vec()
                } else {
                    Vec::new()
                };

                let apic_frame = crate::id3::frames::APIC::new(
                    encoding,
                    mime_type,
                    picture_type,
                    description.clone(),
                    picture_data,
                );
                // Format key : "APIC:desc" or just "APIC:" if no description
                let key = format!("APIC:{}", description);
                self.dict.insert(key.clone(), Box::new(apic_frame));
                self.frames_by_id
                    .entry("APIC".to_string())
                    .or_default()
                    .push(key);
                Ok(())
            }
            _ => {
                // Store unknown frames for preservation
                self.unknown_frames.push(data.to_vec());
                Ok(())
            }
        }
    }

    /// Parse text frame data
    fn parse_text_frame_data(&self, data: &[u8]) -> Result<Vec<String>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let text_data = &data[1..];
        if text_data.is_empty() {
            return Ok(Vec::new());
        }

        let decoded = encoding.decode_text(text_data)?;
        let values = decoded
            .split('\u{0}')
            .filter(|part| !part.is_empty())
            .map(|part| part.trim_start_matches('\u{feff}').to_string())
            .collect();
        Ok(values)
    }

    /// Convert to bytes for writing with configuration
    pub fn write_with_config(&self, config: &ID3SaveConfig) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Collect and sort frames by priority for optimal layout
        let mut sorted_frames: Vec<(&String, &dyn Frame)> = Vec::new();

        // Add all frames from the dict EXCEPT TSSE (we'll add our own TSSE)
        for (hash_key, frame) in &self.dict {
            // Skip TSSE frames - we'll replace them with our own
            if frame.frame_id() != "TSSE" {
                sorted_frames.push((hash_key, frame.as_ref()));
            }
        }

        // Save whether we have any frames (before sorted_frames is consumed)
        let has_user_frames = !sorted_frames.is_empty();

        // Always add our Audex TSSE frame (replaces any existing TSSE)
        // Use UTF-16 for v2.3, UTF-8 for v2.4
        let tsse_encoding = if config.v2_version == 3 {
            crate::id3::TextEncoding::Utf16
        } else {
            crate::id3::TextEncoding::Utf8
        };

        let tsse_frame_data = Some(crate::id3::TSSE {
            frame_id: "TSSE".to_string(),
            encoding: tsse_encoding,
            text: vec![format!("Audex {}", crate::VERSION_STRING)],
            flags: Default::default(),
        });

        sorted_frames.sort_by(|a, b| {
            let a_id = a.1.frame_id();
            let b_id = b.1.frame_id();

            // Define frame priority following the order (lower number = higher priority)
            let get_priority = |frame_id: &str| -> u8 {
                match frame_id {
                    // Core frames in the exact order
                    "TIT2" => 0,          // title
                    "TPE1" => 1,          // artist
                    "TRCK" => 2,          // track
                    "TALB" => 3,          // album
                    "TPOS" => 4,          // disc
                    "TDRC" | "TYER" => 5, // date
                    "TCON" => 6,          // genre

                    // APIC frames placed last due to their size
                    "APIC" => 100,

                    // Everything else gets medium priority
                    _ => 50,
                }
            };

            let a_priority = get_priority(a_id);
            let b_priority = get_priority(b_id);

            // Sort by priority first, then by frame ID for stability
            a_priority
                .cmp(&b_priority)
                .then_with(|| a_id.cmp(b_id))
                .then_with(|| a.0.cmp(b.0)) // Hash key for stability
        });

        // Collect extra frames generated from conversion (e.g. TDRC → TYER)
        let mut extra_frames: Vec<(String, Vec<u8>)> = Vec::new();

        // Write frames
        for (_, frame) in sorted_frames {
            let frame_id = frame.frame_id();

            // For v2.3 with convert_v24_frames: convert TDRC → TYER
            // Skip TDRC from output and emit TYER instead
            if config.v2_version == 3 && config.convert_v24_frames && frame_id == "TDRC" {
                if let Some(f) = frame.as_any().downcast_ref::<crate::id3::TextFrame>() {
                    // Extract year from TDRC text (e.g. "2013-09-27" → "2013", or "2013" → "2013")
                    if let Some(first_text) = f.text.first() {
                        let year = if first_text.len() >= 4 {
                            &first_text[..4]
                        } else {
                            first_text.as_str()
                        };
                        // Create TYER frame with same encoding as TDRC:
                        // preserve original encoding, don't convert to UTF-16)
                        let tyer = crate::id3::TextFrame {
                            frame_id: "TYER".to_string(),
                            encoding: f.encoding,
                            text: vec![year.to_string()],
                            flags: Default::default(),
                        };
                        let tyer_data = tyer.to_data()?;
                        if !tyer_data.is_empty() {
                            extra_frames.push(("TYER".to_string(), tyer_data));
                        }
                    }
                }
                continue; // Skip writing TDRC itself
            }

            let effective_frame_id = frame_id.to_string();

            // For ID3v2.3, convert incompatible encodings to UTF-16 using trait-based approach
            // This handles all 13 frame types with encoding fields automatically
            let frame_data = if config.v2_version == 3 {
                // Clone and convert frame types with encoding for v2.3 compatibility
                // Uses the Frame trait's convert_encoding_for_version method

                // Macro to reduce code duplication for frame type handling
                macro_rules! convert_and_serialize {
                    ($frame_type:ty) => {
                        if let Some(f) = frame.as_any().downcast_ref::<$frame_type>() {
                            let mut modified = f.clone();
                            modified.convert_encoding_for_version((2, 3));
                            modified.to_data()?
                        } else {
                            frame.to_data()?
                        }
                    };
                }

                // Try each frame type that implements HasEncoding
                if frame
                    .as_any()
                    .downcast_ref::<crate::id3::TextFrame>()
                    .is_some()
                {
                    convert_and_serialize!(crate::id3::TextFrame)
                } else if frame.as_any().downcast_ref::<crate::id3::TXXX>().is_some() {
                    convert_and_serialize!(crate::id3::TXXX)
                } else if frame.as_any().downcast_ref::<crate::id3::COMM>().is_some() {
                    convert_and_serialize!(crate::id3::COMM)
                } else if frame.as_any().downcast_ref::<crate::id3::USER>().is_some() {
                    convert_and_serialize!(crate::id3::USER)
                } else if frame.as_any().downcast_ref::<crate::id3::TIPL>().is_some() {
                    convert_and_serialize!(crate::id3::TIPL)
                } else if frame.as_any().downcast_ref::<crate::id3::TMCL>().is_some() {
                    convert_and_serialize!(crate::id3::TMCL)
                } else if frame.as_any().downcast_ref::<crate::id3::APIC>().is_some() {
                    convert_and_serialize!(crate::id3::APIC)
                } else if frame.as_any().downcast_ref::<crate::id3::USLT>().is_some() {
                    convert_and_serialize!(crate::id3::USLT)
                } else if frame.as_any().downcast_ref::<crate::id3::GEOB>().is_some() {
                    convert_and_serialize!(crate::id3::GEOB)
                } else if frame.as_any().downcast_ref::<crate::id3::WXXX>().is_some() {
                    convert_and_serialize!(crate::id3::WXXX)
                } else if frame.as_any().downcast_ref::<crate::id3::SYLT>().is_some() {
                    convert_and_serialize!(crate::id3::SYLT)
                } else if frame.as_any().downcast_ref::<crate::id3::OWNE>().is_some() {
                    convert_and_serialize!(crate::id3::OWNE)
                } else if frame.as_any().downcast_ref::<crate::id3::COMR>().is_some() {
                    convert_and_serialize!(crate::id3::COMR)
                } else {
                    // For frame types without encoding, serialize directly
                    crate::id3::frames::serialize_frame_for_version(frame, (2, config.v2_version))?
                }
            } else {
                crate::id3::frames::serialize_frame_for_version(frame, (2, config.v2_version))?
            };

            if frame_data.is_empty() {
                continue;
            }

            // Create frame header (use effective_frame_id for TDRC->TYER etc.)
            let frame_size = u32::try_from(frame_data.len()).map_err(|_| {
                AudexError::InvalidData(format!(
                    "Frame '{}' data too large for u32 size field: {} bytes",
                    effective_frame_id,
                    frame_data.len()
                ))
            })?;
            let header = FrameHeader {
                frame_id: effective_frame_id.clone(),
                size: frame_size,
                flags: crate::id3::specs::FrameFlags::new(), // Default flags
                version: (2, config.v2_version),
                global_unsync: false,
            };

            // Write frame header based on version
            match config.v2_version {
                2 => {
                    // For v2.2, skip frames with 4-character IDs
                    if effective_frame_id.len() != 3 {
                        continue;
                    }
                    data.extend_from_slice(&header.to_bytes()?);
                }
                3 => data.extend_from_slice(&header.to_bytes()?),
                4 => data.extend_from_slice(&header.to_bytes()?),
                _ => {
                    return Err(AudexError::InvalidData(format!(
                        "Unsupported ID3 version: {}",
                        config.v2_version
                    )));
                }
            }

            // Write frame data
            data.extend_from_slice(&frame_data);
        }

        // Write extra frames generated from conversion (e.g. TYER from TDRC)
        for (extra_id, extra_data) in &extra_frames {
            let extra_size = u32::try_from(extra_data.len()).map_err(|_| {
                AudexError::InvalidData(format!(
                    "Frame '{}' data too large for u32 size field: {} bytes",
                    extra_id,
                    extra_data.len()
                ))
            })?;
            let header = FrameHeader {
                frame_id: extra_id.clone(),
                size: extra_size,
                flags: crate::id3::specs::FrameFlags::new(),
                version: (2, config.v2_version),
                global_unsync: false,
            };
            data.extend_from_slice(&header.to_bytes()?);
            data.extend_from_slice(extra_data);
        }

        // Add TSSE frame for Audex vendor if we created one
        // Only add TSSE if there are other frames - don't add TSSE to an otherwise empty tag
        if let Some(ref tsse) = tsse_frame_data {
            if has_user_frames {
                let tsse_data =
                    crate::id3::frames::serialize_frame_for_version(tsse, (2, config.v2_version))?;
                if !tsse_data.is_empty() {
                    let tsse_size = u32::try_from(tsse_data.len()).map_err(|_| {
                        AudexError::InvalidData(format!(
                            "Frame 'TSSE' data too large for u32 size field: {} bytes",
                            tsse_data.len()
                        ))
                    })?;
                    let header = FrameHeader {
                        frame_id: "TSSE".to_string(),
                        size: tsse_size,
                        flags: crate::id3::specs::FrameFlags::new(),
                        version: (2, config.v2_version),
                        global_unsync: false,
                    };

                    data.extend_from_slice(&header.to_bytes()?);
                    data.extend_from_slice(&tsse_data);
                }
            }
        }

        // Add padding if configured
        if let Some(padding_size) = config.padding {
            data.resize(data.len() + padding_size, 0);
        }

        Ok(data)
    }

    /// Convert to bytes for writing
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // Use the save config's v2_version directly — it is always set to the
        // correct sub-version (3 or 4) regardless of how the tag was created.
        let config = ID3SaveConfig::simple(self.config.v2_version, Some("/".to_string()))?;
        self.write_with_config(&config)
    }

    /// Generate complete ID3v2 data with header for embedding in containers like WAVE
    pub fn to_id3v2_bytes(&self) -> Result<Vec<u8>> {
        let config = ID3SaveConfig::simple(self.config.v2_version, Some("/".to_string()))?;
        let tag_data = self.write_with_config(&config)?;

        let mut id3v2_data = Vec::new();

        if !tag_data.is_empty() {
            let final_tag_data = tag_data;

            // Write ID3v2 header
            id3v2_data.extend_from_slice(b"ID3"); // File identifier
            id3v2_data.push(config.v2_version); // Major version
            id3v2_data.push(0); // Revision
            id3v2_data.push(0); // Flags

            // Write synchsafe size (tag data length).
            // A synchsafe 4-byte integer can represent at most 2^28 - 1.
            if final_tag_data.len() > 0x0FFF_FFFF {
                return Err(crate::AudexError::InvalidData(format!(
                    "Tag data size {} exceeds the ID3v2 synchsafe maximum (268_435_455 bytes)",
                    final_tag_data.len(),
                )));
            }
            let size = u32::try_from(final_tag_data.len()).map_err(|_| {
                crate::AudexError::InvalidData(format!(
                    "Tag data length {} exceeds u32::MAX",
                    final_tag_data.len(),
                ))
            })?;
            let synchsafe = [
                ((size >> 21) & 0x7F) as u8,
                ((size >> 14) & 0x7F) as u8,
                ((size >> 7) & 0x7F) as u8,
                (size & 0x7F) as u8,
            ];
            id3v2_data.extend_from_slice(&synchsafe);

            // Write tag data
            id3v2_data.extend_from_slice(&final_tag_data);
        }

        Ok(id3v2_data)
    }

    /// Clear all frames from memory
    pub fn clear(&mut self) {
        self.dict.clear();
        self.frames_by_id.clear();
        self.unknown_frames.clear();
    }

    /// Get the number of frame IDs for debugging
    pub fn frame_id_count(&self) -> usize {
        self.frames_by_id.len()
    }

    /// Update from another ID3Tags instance.
    ///
    /// This method is designed for merging text metadata only. It extracts
    /// displayable text from each frame and re-creates it as a text frame.
    /// Binary frames such as APIC (pictures) are intentionally skipped and
    /// should be copied separately using dedicated methods like `add_picture`.
    pub fn update(&mut self, other: &ID3Tags) {
        for frame_id in other.frames_by_id.keys() {
            if let Some(frames) = other.get_frames(frame_id) {
                // Clone the frames and add them
                self.remove(frame_id);
                for frame in frames {
                    if let Ok(text) = self.extract_text_for_update(frame) {
                        let _ = self.add_text_frame(frame_id, vec![text]);
                    }
                }
            }
        }
    }

    /// Extract text from a frame for update operation
    fn extract_text_for_update(&self, frame: &dyn crate::id3::frames::Frame) -> Result<String> {
        let desc = frame.description();
        if let Some(colon_pos) = desc.find(": ") {
            Ok(desc[colon_pos + 2..].to_string())
        } else {
            Ok(String::new())
        }
    }
}

impl Default for ID3Tags {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ID3Tags {
    fn clone(&self) -> Self {
        let mut new_tags = ID3Tags::with_version(self.version.0, self.version.1);
        new_tags.config = self.config.clone();
        new_tags.unknown_frames = self.unknown_frames.clone();
        new_tags._unknown_v2_version = self._unknown_v2_version;
        new_tags.text_cache = RefCell::new(self.text_cache.borrow().clone());

        // Copy all frames from dict (which includes all unique hash keys)
        // This preserves frames with unique descriptions like TXXX:Foo, TXXX:Bar
        for frame in self.dict.values() {
            // Serialize and deserialize to create a deep copy
            match frame.to_data() {
                Ok(data) => match FrameRegistry::create_frame(frame.frame_id(), &data) {
                    Ok(cloned_frame) => {
                        let _ = new_tags.add(cloned_frame);
                    }
                    Err(_e) => {
                        warn_event!(
                            %_e,
                            frame_id = frame.frame_id(),
                            "frame deserialization failed during clone, frame dropped"
                        );
                    }
                },
                Err(_e) => {
                    warn_event!(
                        %_e,
                        frame_id = frame.frame_id(),
                        "frame serialization failed during clone, frame dropped"
                    );
                }
            }
        }

        new_tags
    }
}

impl Tags for ID3Tags {
    fn get(&self, _key: &str) -> Option<&[String]> {
        // Due to lifetime restrictions, we cannot safely return &[String]
        None
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        use crate::id3::specs::TextEncoding;

        // Handle TXXX:Description keys → create a proper TXXX frame
        if let Some(description) = key.strip_prefix("TXXX:") {
            self.delall(key);
            if !values.is_empty() {
                let frame =
                    crate::id3::TXXX::new(TextEncoding::Utf8, description.to_string(), values);
                let _ = self.add(Box::new(frame));
            }
            return;
        }

        // Handle COMM keys → create a proper COMM frame
        // Accepts "COMM", "COMM::eng", "COMM:desc:lang"
        if key == "COMM" || key.starts_with("COMM:") {
            self.delall(key);
            if let Some(text) = values.into_iter().next() {
                let (desc, lang) = if let Some(rest) = key.strip_prefix("COMM:") {
                    let parts: Vec<&str> = rest.splitn(2, ':').collect();
                    let desc = parts.first().copied().unwrap_or("").to_string();
                    let lang_str = parts.get(1).copied().unwrap_or("eng");
                    let mut lang = [b' '; 3];
                    for (i, b) in lang_str.as_bytes().iter().take(3).enumerate() {
                        lang[i] = *b;
                    }
                    (desc, lang)
                } else {
                    (String::new(), *b"eng")
                };
                let frame = crate::id3::COMM::new(TextEncoding::Utf8, lang, desc, text);
                let _ = self.add(Box::new(frame));
            }
            return;
        }

        // Handle USLT keys → create a proper USLT frame
        // Accepts "USLT", "USLT::eng", "USLT:desc:lang"
        if key == "USLT" || key.starts_with("USLT:") {
            self.delall(key);
            if let Some(text) = values.into_iter().next() {
                let (desc, lang) = if let Some(rest) = key.strip_prefix("USLT:") {
                    let parts: Vec<&str> = rest.splitn(2, ':').collect();
                    let desc = parts.first().copied().unwrap_or("").to_string();
                    let lang_str = parts.get(1).copied().unwrap_or("eng");
                    let mut lang = [b' '; 3];
                    for (i, b) in lang_str.as_bytes().iter().take(3).enumerate() {
                        lang[i] = *b;
                    }
                    (desc, lang)
                } else {
                    (String::new(), *b"eng")
                };
                let frame = crate::id3::USLT::new(TextEncoding::Utf8, lang, desc, text);
                let _ = self.add(Box::new(frame));
            }
            return;
        }

        // Map common tag names to ID3 frame IDs
        let frame_id = match key.to_uppercase().as_str() {
            "TITLE" => "TIT2",
            "ARTIST" => "TPE1",
            "ALBUM" => "TALB",
            "DATE" => "TDRC",
            "GENRE" => "TCON",
            "TRACKNUMBER" => "TRCK",
            _ => key,
        };

        self.remove(frame_id);
        if !values.is_empty() {
            let _ = self.add_text_frame(frame_id, values);
        }
    }

    fn remove(&mut self, key: &str) {
        let frame_id = match key.to_uppercase().as_str() {
            "TITLE" => "TIT2",
            "ARTIST" => "TPE1",
            "ALBUM" => "TALB",
            "DATE" => "TDRC",
            "GENRE" => "TCON",
            "TRACKNUMBER" => "TRCK",
            _ => key, // Use the key directly if it doesn't match common names
        };

        self.remove(frame_id);
    }

    fn keys(&self) -> Vec<String> {
        // Return dict hash keys which include descriptors for multi-instance frames
        // (e.g. "TXXX:Catalog Number", "COMM::eng", "USLT::eng") rather than just
        // frame IDs ("TXXX", "COMM") — this lets the mapping layer see each
        // individual TXXX/COMM/USLT frame separately.
        self.dict.keys().cloned().collect()
    }

    fn pprint(&self) -> String {
        let mut result = String::new();
        let mut keys: Vec<_> = self.keys();
        keys.sort();

        for key in keys {
            if let Some(values) = self.get(&key) {
                for value in values {
                    result.push_str(&format!("{}={}\n", key, value));
                }
            }
        }

        result
    }
}

impl Metadata for ID3Tags {
    type Error = crate::AudexError;

    fn new() -> Self {
        ID3Tags::new()
    }

    fn load_from_fileobj(filething: &mut crate::util::AnyFileThing) -> crate::Result<Self> {
        if let Some(path) = filething.filename() {
            let id3 = crate::id3::ID3::load_from_file(path)?;
            Ok(id3.tags)
        } else {
            Err(crate::AudexError::InvalidOperation(
                "ID3Tags.load_from_fileobj requires a real file path".to_string(),
            ))
        }
    }

    fn save_to_fileobj(&self, filething: &mut crate::util::AnyFileThing) -> crate::Result<()> {
        let path = filething.filename().ok_or_else(|| {
            crate::AudexError::InvalidOperation(
                "ID3Tags.save_to_fileobj requires a real file path".to_string(),
            )
        })?;

        let mut id3 = crate::id3::ID3::load_from_file(path).unwrap_or_default();
        id3.tags = self.clone();
        id3.filename = Some(path.to_string_lossy().to_string());
        id3.save()
    }

    fn delete_from_fileobj(filething: &mut crate::util::AnyFileThing) -> crate::Result<()> {
        let path = filething.filename().ok_or_else(|| {
            crate::AudexError::InvalidOperation(
                "ID3Tags.clear_from_fileobj requires a real file path".to_string(),
            )
        })?;
        crate::id3::file::clear(path, true, true)
    }
}

impl crate::tags::MetadataFields for ID3Tags {
    fn artist(&self) -> Option<&String> {
        // Due to Rust lifetime constraints, we cannot safely return &String from RefCell
        // The test should use get_artist_owned() instead
        None
    }

    fn set_artist(&mut self, artist: String) {
        let _ = self.add_text_frame("TPE1", vec![artist]);
    }

    fn album(&self) -> Option<&String> {
        // Due to Rust lifetime constraints, we cannot safely return &String from RefCell
        // The test should use get_text() instead
        None
    }

    fn set_album(&mut self, album: String) {
        let _ = self.add_text_frame("TALB", vec![album]);
    }

    fn title(&self) -> Option<&String> {
        // Due to Rust lifetime constraints, we cannot safely return &String from RefCell
        // The test should use get_text() instead
        None
    }

    fn set_title(&mut self, title: String) {
        let _ = self.add_text_frame("TIT2", vec![title]);
    }

    fn track_number(&self) -> Option<u32> {
        self.get_text("TRCK").and_then(|s| {
            // Parse track number - handle formats like "5" or "5/10"
            s.split('/').next().and_then(|n| n.trim().parse().ok())
        })
    }

    fn set_track_number(&mut self, track: u32) {
        let _ = self.add_text_frame("TRCK", vec![track.to_string()]);
    }

    fn date(&self) -> Option<&String> {
        // Due to Rust lifetime constraints, we cannot safely return &String from RefCell
        // The test should use get_text() instead
        None
    }

    fn set_date(&mut self, date: String) {
        // Use TDRC for v2.4, TYER for v2.3/v2.2
        if self.version.1 == 4 {
            let _ = self.add_text_frame("TDRC", vec![date]);
        } else {
            let _ = self.add_text_frame("TYER", vec![date]);
        }
    }

    fn genre(&self) -> Option<&String> {
        // Due to Rust lifetime constraints, we cannot safely return &String from RefCell
        // The test should use get_text() instead
        None
    }

    fn set_genre(&mut self, genre: String) {
        let _ = self.add_text_frame("TCON", vec![genre]);
    }
}

/// Additional methods for MetadataFields that return owned strings
impl ID3Tags {
    /// Get artist as owned String
    pub fn get_artist_owned(&self) -> Option<String> {
        self.get_text_values("ARTIST")?.into_iter().next()
    }

    /// Get album as owned String
    pub fn get_album_owned(&self) -> Option<String> {
        self.get_text_values("ALBUM")?.into_iter().next()
    }

    /// Get title as owned String
    pub fn get_title_owned(&self) -> Option<String> {
        self.get_text_values("TITLE")?.into_iter().next()
    }

    /// Get date as owned String
    pub fn get_date_owned(&self) -> Option<String> {
        self.get_text_values("DATE")?.into_iter().next()
    }

    /// Get genre as owned String
    pub fn get_genre_owned(&self) -> Option<String> {
        self.get_text_values("GENRE")?.into_iter().next()
    }
}

/// Additional key-value methods for ID3Tags
impl ID3Tags {
    /// Get frame by key, returns `None` if not found.
    ///
    /// This is the non-panicking alternative to the `tags["key"]` indexing
    /// syntax and should be preferred when the key may not exist (e.g. when
    /// reading metadata from untrusted files).
    #[doc(alias = "index")]
    pub fn get_frame(&self, key: &str) -> Option<&dyn Frame> {
        // Try exact hash key first
        if let Some(frame) = self.dict.get(key) {
            return Some(frame.as_ref());
        }

        // Look for frames by ID prefix
        for (hash_key, frame) in &self.dict {
            if hash_key == key || hash_key.starts_with(&(key.to_owned() + ":")) {
                return Some(frame.as_ref());
            }
        }

        None
    }

    /// Get a mutable reference to a frame by key, returns `None` if not found.
    ///
    /// This is the non-panicking alternative to `&mut tags["key"]` and should
    /// be preferred when the key may not exist (e.g. when processing metadata
    /// from untrusted files or user-supplied keys).
    pub fn get_frame_mut(&mut self, key: &str) -> Option<&mut Box<dyn Frame>> {
        // Try exact hash key first
        if self.dict.contains_key(key) {
            return self.dict.get_mut(key);
        }

        // Look for frames by ID prefix
        let mut found_key: Option<String> = None;
        for hash_key in self.dict.keys() {
            if hash_key == key || hash_key.starts_with(&(key.to_owned() + ":")) {
                found_key = Some(hash_key.clone());
                break;
            }
        }

        if let Some(found_key) = found_key {
            return self.dict.get_mut(&found_key);
        }

        None
    }

    /// Set frame by key
    pub fn set_frame(&mut self, key: &str, frame: Box<dyn Frame>) -> Result<()> {
        let hash_key = key.to_string();

        // Remove existing frame with this key
        if self.dict.contains_key(&hash_key) {
            self.dict.remove(&hash_key);
        }

        // Add new frame
        let frame_id = frame.frame_id().to_string();
        self.dict.insert(hash_key.clone(), frame);

        // Update frame ID lookup
        self.frames_by_id
            .entry(frame_id)
            .or_default()
            .push(hash_key);

        Ok(())
    }

    /// Pop frame by key
    pub fn pop(&mut self, key: &str) -> Option<Box<dyn Frame>> {
        // Try exact hash key first
        if let Some(frame) = self.dict.remove(key) {
            // Clean up frame ID lookup
            let frame_id = frame.frame_id().to_string();
            if let Some(hash_keys) = self.frames_by_id.get_mut(&frame_id) {
                hash_keys.retain(|k| k != key);
                if hash_keys.is_empty() {
                    self.frames_by_id.remove(&frame_id);
                }
            }
            return Some(frame);
        }

        // Look for frames by ID prefix
        let mut found_key: Option<String> = None;
        for hash_key in self.dict.keys() {
            if hash_key == key || hash_key.starts_with(&(key.to_owned() + ":")) {
                found_key = Some(hash_key.clone());
                break;
            }
        }

        if let Some(found_key) = found_key {
            if let Some(frame) = self.dict.remove(&found_key) {
                // Clean up frame ID lookup
                let frame_id = frame.frame_id().to_string();
                if let Some(hash_keys) = self.frames_by_id.get_mut(&frame_id) {
                    hash_keys.retain(|k| k != &found_key);
                    if hash_keys.is_empty() {
                        self.frames_by_id.remove(&frame_id);
                    }
                }
                return Some(frame);
            }
        }

        None
    }

    /// Pop a key-value pair
    pub fn popitem(&mut self) -> Option<(String, Box<dyn Frame>)> {
        if let Some((key, frame)) = self.dict.pop_first() {
            // Clean up frame ID lookup
            let frame_id = frame.frame_id().to_string();
            if let Some(hash_keys) = self.frames_by_id.get_mut(&frame_id) {
                hash_keys.retain(|k| k != &key);
                if hash_keys.is_empty() {
                    self.frames_by_id.remove(&frame_id);
                }
            }
            Some((key, frame))
        } else {
            None
        }
    }

    /// Set default value if key doesn't exist.
    ///
    /// If `key` is already present the existing frame is returned unchanged.
    /// Otherwise `default_frame` is inserted and a reference to it is returned.
    pub fn setdefault(&mut self, key: &str, default_frame: Box<dyn Frame>) -> Result<&dyn Frame> {
        if self.get_frame(key).is_none() {
            self.set_frame(key, default_frame)?;
        }
        self.get_frame(key).ok_or_else(|| {
            crate::AudexError::InvalidData(format!(
                "setdefault: key '{}' not found after insertion",
                key
            ))
        })
    }

    /// Check if key exists in the tags
    pub fn contains_frame(&self, key: &str) -> bool {
        self.get_frame(key).is_some()
    }

    /// Get all hash keys
    pub fn frame_keys(&self) -> Vec<String> {
        self.dict.keys().cloned().collect()
    }

    /// Get all frame values
    pub fn frame_values(&self) -> Vec<&dyn Frame> {
        self.dict.values().map(|b| b.as_ref()).collect()
    }

    /// Get all key-value pairs
    pub fn frame_items(&self) -> Vec<(&String, &dyn Frame)> {
        self.dict.iter().map(|(k, v)| (k, v.as_ref())).collect()
    }

    /// Get text values for a key (Returns owned strings to avoid lifetime issues)
    pub fn get_text_values(&self, key: &str) -> Option<Vec<String>> {
        // Map common tag names to ID3 frame IDs
        let frame_id = match key.to_uppercase().as_str() {
            "TITLE" => "TIT2",
            "ARTIST" => "TPE1",
            "ALBUM" => "TALB",
            "DATE" => "TDRC",
            "GENRE" => "TCON",
            "TRACKNUMBER" => "TRCK",
            _ => key, // Use key as-is if no mapping
        };

        // Get all frames with this ID and extract text
        let frames = self.getall(frame_id);
        if frames.is_empty() {
            return None;
        }

        // For most cases, we want the text array from the first frame
        if let Some(frame) = frames.first() {
            if let Some(text_array) = self._extract_text_array_from_frame(*frame) {
                Some(text_array)
            } else if let Some(text) = self._extract_text_from_frame(*frame) {
                // Fallback to single text extraction
                Some(vec![text])
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Update text cache when frames are modified
    #[allow(dead_code)]
    pub(crate) fn update_text_cache(&self, key: &str) {
        // Map common names to frame IDs
        let frame_id = match key {
            "TITLE" => "TIT2",
            "ARTIST" => "TPE1",
            "ALBUM" => "TALB",
            "DATE" => "TDRC",
            "GENRE" => "TCON",
            "TRACKNUMBER" => "TRCK",
            _ => key, // Use key directly if no mapping
        };

        if let Some(text_values) = self.get_text_values(frame_id) {
            self.text_cache
                .borrow_mut()
                .insert(key.to_string(), text_values);
        } else {
            self.text_cache.borrow_mut().remove(key);
        }
    }

    /// Clear the text cache
    pub(crate) fn clear_text_cache(&self) {
        self.text_cache.borrow_mut().clear();
    }

    /// Delete frames of a given type and add new text frames - convenience method for setall() with strings
    pub fn setall_text(&mut self, key: &str, values: Vec<String>) -> Result<()> {
        self.delall(key);
        if !values.is_empty() {
            self.add_text_frame(key, values)?;
        }
        Ok(())
    }

    /// Return tags in a human-readable format
    pub fn pprint(&self) -> String {
        let mut output = Vec::new();

        // Sort frames by ID for consistent output
        let mut frame_ids: Vec<_> = self.frames_by_id.keys().collect();
        frame_ids.sort();

        for frame_id in frame_ids {
            let frames = self.getall(frame_id);
            for frame in frames {
                output.push(frame.description());
            }
        }

        if output.is_empty() {
            "No frames found".to_string()
        } else {
            output.join("\n")
        }
    }

    /// Save changes to a file
    pub fn save<P: AsRef<std::path::Path>>(
        &self,
        _filething: P,
        v1: u8,
        v2_version: u8,
        v23_sep: Option<String>,
        padding: Option<usize>,
    ) -> Result<()> {
        debug_event!("saving ID3v2 tags");
        trace_event!(
            v2_version = v2_version,
            write_v1 = v1,
            "configuring ID3 save"
        );
        let v1_option = match v1 {
            0 => crate::id3::file::ID3v1SaveOptions::REMOVE,
            1 => crate::id3::file::ID3v1SaveOptions::UPDATE,
            _ => crate::id3::file::ID3v1SaveOptions::CREATE,
        };
        let config = ID3SaveConfig {
            v2_version,
            v2_minor: 0,
            v23_sep: v23_sep.unwrap_or_else(|| "/".to_string()),
            v23_separator: b'/',
            padding,
            merge_frames: true,
            preserve_unknown: true,
            compress_frames: false,
            write_v1: v1_option,
            unsync: false,
            extended_header: false,
            convert_v24_frames: true,
        };

        // Write ID3v2 tag to file
        self.save_to_file(_filething, &config)
    }

    /// Load tags from a filename
    pub fn load<P: AsRef<std::path::Path>>(
        filething: P,
        known_frames: Option<HashMap<String, String>>,
        translate: bool,
        v2_version: u8,
        load_v1: bool,
    ) -> Result<Self> {
        debug_event!("parsing ID3v2 tags");
        let _ = (filething, known_frames, translate, v2_version, load_v1);
        Err(crate::AudexError::NotImplementedMethod(
            "File I/O load not fully implemented yet".to_string(),
        ))
    }

    /// Clear tags from a file
    pub fn clear_file<P: AsRef<std::path::Path>>(
        filething: P,
        clear_v1: bool,
        clear_v2: bool,
    ) -> Result<()> {
        // Use the file module's clear function which has the full implementation
        crate::id3::file::clear(filething.as_ref(), clear_v1, clear_v2)
    }

    /// Get the total size of the ID3 tag, including header
    pub fn size(&self) -> usize {
        // Estimate size based on current frame data
        let config = ID3SaveConfig::default();
        self.write_with_config(&config)
            .map(|data| data.len())
            .unwrap_or(0)
    }

    /// Get unknown frames as raw data
    pub fn unknown_frames(&self) -> Vec<Vec<u8>> {
        Vec::new()
    }
}

/// Version conversion and frame upgrade methods
impl ID3Tags {
    /// Update tag to ID3v2.4 format
    pub fn update_to_v24(&mut self) {
        if self.version.0 != 2 {
            return; // Only convert ID3v2.x tags
        }

        self.version = (2, 4);
        self.config.v2_version = 4;
        self.config.v2_minor = 0;

        // Convert timestamp frames: TYER + TDAT + TIME -> TDRC
        self._convert_timestamps_to_v24();

        // Convert TORY -> TDOR (original release year -> original release date)
        self._convert_tory_to_tdor();

        // Convert IPLS -> TIPL (involved people list -> involved people list v2.4)
        self._convert_ipls_to_tipl();

        // Remove obsolete frames
        let obsolete_frames = ["RVAD", "EQUA", "TRDA", "TSIZ", "TDAT", "TIME", "TYER"];
        for frame_id in &obsolete_frames {
            self.delall(frame_id);
        }

        // Update frame versions for CHAP/CTOC recursively
        self._update_chapter_frames_to_v24();

        // Update common frame properties
        self._update_common_v24();

        // Clear text cache after version conversion
        self.clear_text_cache();

        // Validate all frames for v2.4 compatibility
        self._validate_all_frames_for_version(4);
    }

    /// Check if a string is exactly N ASCII digits
    fn _is_digits(s: &str, n: usize) -> bool {
        s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
    }

    /// Convert timestamp frames to TDRC
    fn _convert_timestamps_to_v24(&mut self) {
        let tyer_frames = self.getall("TYER");
        let tdat_frames = self.getall("TDAT");
        let time_frames = self.getall("TIME");

        if tyer_frames.is_empty() {
            return; // No year information to convert
        }

        let mut timestamps = Vec::new();

        // Process each year frame
        for (i, tyer_frame) in tyer_frames.iter().enumerate() {
            if let Some(year_text) = self._extract_text_from_frame(*tyer_frame) {
                let mut timestamp = String::new();

                let year_trimmed = year_text.trim();
                let year_bytes = year_trimmed.as_bytes();
                let (year_part, month_day_part) = if year_bytes.len() >= 4
                    && year_bytes[..4].iter().all(|b| b.is_ascii_digit())
                {
                    if year_bytes.len() == 4 {
                        (&year_trimmed[..4], None)
                    } else if year_bytes.len() == 10
                        && year_bytes[4] == b'-'
                        && year_bytes[5..7].iter().all(|b| b.is_ascii_digit())
                        && year_bytes[7] == b'-'
                        && year_bytes[8..10].iter().all(|b| b.is_ascii_digit())
                    {
                        (&year_trimmed[..4], Some(&year_trimmed[4..10]))
                    } else {
                        // Doesn't match expected pattern — skip
                        continue;
                    }
                } else {
                    continue; // Invalid year
                };

                timestamp.push_str(year_part);

                if let Some(tdat_frame) = tdat_frames.get(i) {
                    if let Some(date_text) = self._extract_text_from_frame(*tdat_frame) {
                        let dt = date_text.trim();
                        if Self::_is_digits(dt, 4) {
                            let day = &dt[..2];
                            let month = &dt[2..4];
                            let month_day = format!("-{}-{}", month, day);
                            timestamp.push_str(&month_day);

                            // Add time if available (HHMM format, 4 digits exactly)
                            if let Some(time_frame) = time_frames.get(i) {
                                if let Some(time_text) = self._extract_text_from_frame(*time_frame)
                                {
                                    let tt = time_text.trim();
                                    if Self::_is_digits(tt, 4) {
                                        let hour = &tt[..2];
                                        let minute = &tt[2..4];
                                        timestamp.push_str(&format!("T{}:{}:00", hour, minute));
                                    }
                                }
                            }
                        }
                    }
                } else if let Some(md) = month_day_part {
                    // Use month-day from TYER itself if present
                    timestamp.push_str(md);
                }

                if !timestamp.is_empty() {
                    timestamps.push(timestamp);
                }
            }
        }

        // Add TDRC frames if we have timestamps and no existing TDRC
        if !timestamps.is_empty() && self.getall("TDRC").is_empty() {
            if let Ok(()) = self.add_text_frame("TDRC", timestamps) {
                // Successfully added TDRC, remove old frames
                self.delall("TYER");
                self.delall("TDAT");
                self.delall("TIME");
            }
        }
    }

    /// Convert TORY to TDOR
    fn _convert_tory_to_tdor(&mut self) {
        let tory_frames = self.getall("TORY");
        if tory_frames.is_empty() || !self.getall("TDOR").is_empty() {
            return; // No TORY or TDOR already exists
        }

        let mut years = Vec::new();
        for frame in &tory_frames {
            years.extend(self._extract_text_values_from_frame(*frame));
        }

        if !years.is_empty() {
            if let Ok(()) = self.add_text_frame("TDOR", years) {
                self.delall("TORY");
            }
        }
    }

    /// Convert IPLS to TIPL
    fn _convert_ipls_to_tipl(&mut self) {
        let ipls_frames = self.getall("IPLS");
        if ipls_frames.is_empty() || !self.getall("TIPL").is_empty() {
            return; // No IPLS or TIPL already exists
        }

        let mut people_lists = Vec::new();
        for frame in &ipls_frames {
            people_lists.extend(self._extract_text_values_from_frame(*frame));
        }

        if !people_lists.is_empty() {
            if let Ok(()) = self.add_text_frame("TIPL", people_lists) {
                self.delall("IPLS");
            }
        }
    }

    /// Update chapter frames recursively
    fn _update_chapter_frames_to_v24(&mut self) {
        // Find CHAP and CTOC frames and recursively update their sub-frames
        let chap_frames = self.getall("CHAP");
        let ctoc_frames = self.getall("CTOC");

        // Update CHAP and CTOC frames recursively (frames are implemented as FrameData)
        if !chap_frames.is_empty() || !ctoc_frames.is_empty() {
            // CHAP/CTOC frames are implemented in FrameData enum with sub_frames support
            // For full recursive update, we would need to:
            // 1. Cast frames to their specific types to access sub_frames
            // 2. Recursively update each sub-frame to v2.4 format
            // 3. Update any frame IDs, encodings, etc.
            warn_event!(
                chap_count = chap_frames.len(),
                ctoc_count = ctoc_frames.len(),
                "CHAP/CTOC recursive update not fully implemented"
            );
        }
    }

    /// Update common frame properties for v2.4
    fn _update_common_v24(&mut self) {
        // Update genre frames to remove (xx)Genre format
        self._update_genre_frames();

        // Update APIC MIME types
        self._update_apic_mime_types();
    }

    /// Update tag to ID3v2.3 format
    pub fn update_to_v23(&mut self) {
        if self.version.0 != 2 {
            return; // Only convert ID3v2.x tags
        }

        self.version = (2, 3);
        self.config.v2_version = 3;
        self.config.v2_minor = 0;

        // Convert people list frames: TIPL + TMCL -> IPLS
        self._convert_people_lists_to_v23();

        // Convert TDOR -> TORY (original release date -> original release year)
        self._convert_tdor_to_tory();

        // Convert TDRC -> TYER, TDAT, TIME (recording date -> separate fields)
        self._convert_tdrc_to_separate_fields();

        // Remove ID3v2.4-only frames
        let v24_only_frames = [
            "ASPI", "EQU2", "RVA2", "SEEK", "SIGN", "TDEN", "TDOR", "TDRC", "TDRL", "TDTG", "TIPL",
            "TMCL", "TMOO", "TPRO", "TSOA", "TSOP", "TSOT", "TSST",
        ];

        for frame_id in &v24_only_frames {
            self.delall(frame_id);
        }

        // Update chapter frames recursively
        self._update_chapter_frames_to_v23();

        // Update common frame properties
        self._update_common_v23();

        // Clear text cache after version conversion
        self.clear_text_cache();

        // Validate all frames for v2.3 compatibility
        self._validate_all_frames_for_version(3);
    }

    /// Convert TIPL and TMCL to IPLS for v2.3
    fn _convert_people_lists_to_v23(&mut self) {
        if !self.getall("IPLS").is_empty() {
            return; // IPLS already exists
        }

        let mut people_list = Vec::new();

        // Collect from TIPL (involved people list)
        for frame in self.getall("TIPL") {
            people_list.extend(self._extract_text_values_from_frame(frame));
        }

        // Collect from TMCL (musician credits list)
        for frame in self.getall("TMCL") {
            people_list.extend(self._extract_text_values_from_frame(frame));
        }

        // Create IPLS frame if we have people list data
        if !people_list.is_empty() {
            if let Ok(()) = self.add_text_frame("IPLS", people_list) {
                self.delall("TIPL");
                self.delall("TMCL");
            }
        }
    }

    /// Convert TDOR to TORY (extract year only)
    fn _convert_tdor_to_tory(&mut self) {
        let tdor_frames = self.getall("TDOR");
        if tdor_frames.is_empty() || !self.getall("TORY").is_empty() {
            return; // No TDOR or TORY already exists
        }

        let mut years = Vec::new();
        for frame in &tdor_frames {
            for date_text in self._extract_text_values_from_frame(*frame) {
                // Try to parse as ISO timestamp and extract year
                let timestamp = ID3TimeStamp::new(date_text.clone());
                if let Some(year) = timestamp.year {
                    years.push(format!("{:04}", year));
                } else if date_text.len() >= 4 {
                    // Fallback: try to extract first 4 digits as year
                    let year_part = &date_text[..4];
                    if year_part.chars().all(|c| c.is_ascii_digit()) {
                        years.push(year_part.to_string());
                    }
                }
            }
        }

        if !years.is_empty() {
            if let Ok(()) = self.add_text_frame("TORY", years) {
                self.delall("TDOR");
            }
        }
    }

    /// Convert TDRC to separate TYER, TDAT, TIME fields
    fn _convert_tdrc_to_separate_fields(&mut self) {
        let tdrc_frames = self.getall("TDRC");
        if tdrc_frames.is_empty() {
            return; // No TDRC to convert
        }

        let mut years = Vec::new();
        let mut dates = Vec::new();
        let mut times = Vec::new();

        for frame in &tdrc_frames {
            for date_text in self._extract_text_values_from_frame(*frame) {
                let timestamp = ID3TimeStamp::new(date_text.clone());
                // Extract year (YYYY)
                if let Some(year) = timestamp.year {
                    years.push(format!("{:04}", year));
                }

                // Extract date (DDMM)
                if let (Some(month), Some(day)) = (timestamp.month, timestamp.day) {
                    dates.push(format!("{:02}{:02}", day, month));
                }

                // Extract time (HHMM)
                if let (Some(hour), Some(minute)) = (timestamp.hour, timestamp.minute) {
                    times.push(format!("{:02}{:02}", hour, minute));
                }
            }
        }

        // Add frames if they don't exist and we have data
        let mut conversion_success = false;

        if !years.is_empty() && self.getall("TYER").is_empty() {
            if let Ok(()) = self.add_text_frame("TYER", years) {
                conversion_success = true;
            }
        }

        if !dates.is_empty() && self.getall("TDAT").is_empty() {
            if let Ok(()) = self.add_text_frame("TDAT", dates) {
                conversion_success = true;
            }
        }

        if !times.is_empty() && self.getall("TIME").is_empty() {
            if let Ok(()) = self.add_text_frame("TIME", times) {
                conversion_success = true;
            }
        }

        // Remove TDRC only if we successfully converted at least one component
        if conversion_success {
            self.delall("TDRC");
        }
    }

    /// Update chapter frames for v2.3 compatibility
    fn _update_chapter_frames_to_v23(&mut self) {
        // Find CHAP and CTOC frames
        let chap_frames = self.getall("CHAP");
        let ctoc_frames = self.getall("CTOC");

        // Update CHAP and CTOC frames recursively for v2.3 (frames are implemented as FrameData)
        if !chap_frames.is_empty() || !ctoc_frames.is_empty() {
            // CHAP/CTOC frames are implemented in FrameData enum with sub_frames support
            // For full recursive update, we would need to:
            // 1. Cast frames to their specific types to access sub_frames
            // 2. Recursively update each sub-frame to v2.3 format
            // 3. Update any frame IDs, encodings, etc.
            warn_event!(
                chap_count = chap_frames.len(),
                ctoc_count = ctoc_frames.len(),
                "CHAP/CTOC v2.3 recursive update not fully implemented"
            );
        }
    }

    /// Update common frame properties for v2.3
    fn _update_common_v23(&mut self) {
        // Update genre frames
        self._update_genre_frames();

        // Update APIC MIME types for v2.3 compatibility
        self._update_apic_mime_types();
    }

    /// Update genre frames to remove (xx)Genre format
    fn _update_genre_frames(&mut self) {
        let tcon_frames = self.getall("TCON");
        if tcon_frames.is_empty() {
            return;
        }

        let mut updated_genres = Vec::new();
        let mut needs_update = false;

        for frame in &tcon_frames {
            for genre_text in self._extract_text_values_from_frame(*frame) {
                let cleaned_genre = self._clean_genre_text(&genre_text);
                if cleaned_genre != genre_text {
                    needs_update = true;
                }
                updated_genres.push(cleaned_genre);
            }
        }

        if needs_update {
            self.delall("TCON");
            if let Err(_e) = self.add_text_frame("TCON", updated_genres) {
                warn_event!(%_e, "failed to update genre frames");
            }
        }
    }

    /// Clean genre text to remove (xx)Genre format.
    /// Handles multiple parenthesized IDs like "(21)(51)Rock" by stripping
    /// all leading numeric parenthesized groups and resolving each to a genre
    /// name. If free text follows the parenthesized IDs, it is used as the
    /// genre name instead of the numeric lookups.
    fn _clean_genre_text(&self, genre: &str) -> String {
        let mut remaining = genre;
        let mut resolved_names: Vec<String> = Vec::new();

        // Strip all leading parenthesized numeric genre IDs, resolving each
        while remaining.starts_with('(') {
            if let Some(close_paren) = remaining.find(')') {
                let number_part = &remaining[1..close_paren];
                if number_part.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(id) = number_part.parse::<u8>() {
                        if let Some(name) = crate::constants::get_genre(id) {
                            resolved_names.push(name.to_string());
                        }
                    }
                    remaining = &remaining[close_paren + 1..];
                    continue;
                }
            }
            // Not a numeric parenthesized group; stop stripping
            break;
        }

        // If there is trailing free text after all parenthesized IDs, prefer it
        if !remaining.is_empty() {
            return remaining.to_string();
        }

        // Otherwise return the last resolved genre name, or the original string
        if let Some(last) = resolved_names.pop() {
            return last;
        }

        genre.to_string()
    }

    /// Validate all frames for version compatibility
    fn _validate_all_frames_for_version(&mut self, target_version: u8) {
        let mut invalid_frames = Vec::new();

        // Collect frames that are not valid for the target version
        for (hash_key, frame) in &self.dict {
            let frame_id = frame.frame_id();

            match target_version {
                3 => {
                    // Check for ID3v2.4-only frames
                    let v24_only = [
                        "ASPI", "EQU2", "RVA2", "SEEK", "SIGN", "TDEN", "TDOR", "TDRC", "TDRL",
                        "TDTG", "TIPL", "TMCL", "TMOO", "TPRO", "TSOA", "TSOP", "TSOT", "TSST",
                    ];
                    if v24_only.contains(&frame_id) {
                        invalid_frames.push(hash_key.clone());
                    }
                }
                4 => {
                    // ID3v2.4 is more permissive - mainly check for deprecated frames
                    let deprecated = ["RVAD", "EQUA", "TRDA", "TSIZ"];
                    if deprecated.contains(&frame_id) {
                        invalid_frames.push(hash_key.clone());
                    }
                }
                _ => {} // No validation for other versions
            }
        }

        // Remove invalid frames
        for hash_key in invalid_frames {
            if let Some(frame) = self.dict.remove(&hash_key) {
                let frame_id = frame.frame_id().to_string();
                // Clean up frame ID lookup
                if let Some(hash_keys) = self.frames_by_id.get_mut(&frame_id) {
                    hash_keys.retain(|k| k != &hash_key);
                    if hash_keys.is_empty() {
                        self.frames_by_id.remove(&frame_id);
                    }
                }
            }
        }
    }

    /// Update APIC MIME types for version compatibility
    fn _update_apic_mime_types(&mut self) {
        // Get all APIC frames
        let apic_frames = self.getall("APIC");
        if apic_frames.is_empty() {
            return;
        }

        // Update APIC MIME types from old format to proper MIME types
        let mime_mappings = [("PNG", "image/png"), ("JPG", "image/jpeg")];

        // Update APIC MIME types to use proper format (APIC frames are now implemented)
        if !apic_frames.is_empty() {
            for apic_frame in &apic_frames {
                // Check if this frame needs MIME type update
                if let Some(description) = apic_frame.text_values() {
                    let empty_string = String::new();
                    let desc_text = description.first().unwrap_or(&empty_string);
                    for (old_mime, _new_mime) in &mime_mappings {
                        if desc_text.contains(old_mime) {
                            // APIC frames are implemented but updating them requires
                            // casting to APIC type and modifying mime_type field
                            // Future enhancement: update MIME type in APIC frame
                        }
                    }
                }
            }
        }
    }

    /// Upgrade frame from an older ID3 version
    pub(crate) fn upgrade_frame(&mut self, frame: Box<dyn Frame>) -> Result<Box<dyn Frame>> {
        let frame_id = frame.frame_id();

        // ID3v2.2 to v2.3/v2.4 frame ID upgrades
        let upgraded_id = self._upgrade_frame_id_v22_to_v23(frame_id);

        if upgraded_id != frame_id {
            // Create new frame with upgraded ID
            match self._reconstruct_frame_with_new_id(frame, &upgraded_id) {
                Ok(upgraded_frame) => return Ok(upgraded_frame),
                Err(_) => {
                    // If reconstruction fails, create a basic placeholder
                    let placeholder_frame =
                        TextFrame::new(upgraded_id, vec![format!("Upgraded frame")]);
                    return Ok(Box::new(placeholder_frame));
                }
            }
        }

        Ok(frame)
    }

    /// Upgrade v2.2 frame ID to v2.3/v2.4 equivalent
    fn _upgrade_frame_id_v22_to_v23(&self, frame_id: &str) -> String {
        let v22_to_v23_map = [
            ("TAL", "TALB"),
            ("TBP", "TBPM"),
            ("TCM", "TCOM"),
            ("TCO", "TCON"),
            ("TCR", "TCOP"),
            ("TEN", "TENC"),
            ("TXT", "TEXT"),
            ("TFT", "TFLT"),
            ("TT1", "TIT1"),
            ("TT2", "TIT2"),
            ("TT3", "TIT3"),
            ("TKE", "TKEY"),
            ("TLA", "TLAN"),
            ("TLE", "TLEN"),
            ("TMT", "TMED"),
            ("TOT", "TOAL"),
            ("TOF", "TOFN"),
            ("TOL", "TOLY"),
            ("TOA", "TOPE"),
            ("TOR", "TORY"),
            ("TOW", "TOWN"),
            ("TP1", "TPE1"),
            ("TP2", "TPE2"),
            ("TP3", "TPE3"),
            ("TP4", "TPE4"),
            ("TPA", "TPOS"),
            ("TPB", "TPUB"),
            ("TRK", "TRCK"),
            ("TRD", "TRDA"),
            ("TRN", "TRSN"),
            ("TRO", "TRSO"),
            ("TSI", "TSIZ"),
            ("TRC", "TSRC"),
            ("TSS", "TSSE"),
            ("WXX", "WXXX"),
            ("COM", "COMM"),
            ("PIC", "APIC"),
        ];

        // Handle version-specific upgrades for date/time frames
        // TYE (year) upgrades to TYER in v2.3, TDRC in v2.4
        if frame_id == "TYE" {
            return if self.version.1 == 4 {
                "TDRC".to_string()
            } else {
                "TYER".to_string()
            };
        }

        for (v22_id, v23_id) in &v22_to_v23_map {
            if *v22_id == frame_id {
                return v23_id.to_string();
            }
        }

        frame_id.to_string() // Return as-is if no mapping found
    }

    /// Upgrade all v2.2 frames to v2.3/v2.4 format
    fn _upgrade_all_frames_v22_to_v23(&mut self) {
        let mut frames_to_upgrade = Vec::new();

        // Collect frames that need upgrading
        for (hash_key, frame) in &self.dict {
            let frame_id = frame.frame_id();
            let upgraded_id = self._upgrade_frame_id_v22_to_v23(frame_id);
            if upgraded_id != frame_id {
                frames_to_upgrade.push(hash_key.clone());
            }
        }

        // Upgrade each frame
        for hash_key in frames_to_upgrade {
            if let Some(frame) = self.dict.remove(&hash_key) {
                // Remove from old frame ID mapping
                let old_frame_id = frame.frame_id().to_string();
                if let Some(hash_keys) = self.frames_by_id.get_mut(&old_frame_id) {
                    hash_keys.retain(|k| k != &hash_key);
                    if hash_keys.is_empty() {
                        self.frames_by_id.remove(&old_frame_id);
                    }
                }

                // Upgrade the frame
                if let Ok(upgraded_frame) = self.upgrade_frame(frame) {
                    let new_frame_id = upgraded_frame.frame_id().to_string();

                    // Add to dictionary with new hash key
                    let new_hash_key = self._generate_frame_hash_key(upgraded_frame.as_ref());
                    self.dict.insert(new_hash_key.clone(), upgraded_frame);

                    // Add to new frame ID mapping
                    self.frames_by_id
                        .entry(new_frame_id)
                        .or_default()
                        .push(new_hash_key);
                }
            }
        }
    }

    /// Get the start position of frame data after skipping headers
    fn _get_frame_data_start(&self, data: &[u8], header: &ID3Header) -> Result<usize> {
        let mut pos = 0;

        // Skip extended header if present
        if header.has_extended_header() {
            if data.len() < pos + 4 {
                return Err(AudexError::InvalidData(
                    "Extended header truncated: not enough data".to_string(),
                ));
            }

            // Check if extsize_data looks like a frame ID (common bug in some taggers)
            if let Ok(frame_id) = std::str::from_utf8(&data[pos..pos + 4]) {
                if frame_id
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                {
                    // This looks like a frame ID, not an extended header size
                    // The extended header flag was probably set incorrectly
                    return Ok(0);
                }
            }

            let ext_size = if header.major_version() >= 4 {
                // ID3v2.4: synchsafe size includes the 4 size bytes themselves
                crate::id3::util::decode_synchsafe_int_checked(&data[pos..pos + 4])? as usize
            } else {
                // ID3v2.3: extended header size does not include the 4 size bytes,
                // so add them back to get the total number of bytes to skip.
                // Guard against overflow on 32-bit targets where usize is 32 bits.
                let raw =
                    u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
                let raw_usize = usize::try_from(raw).map_err(|_| {
                    AudexError::InvalidData(
                        "Extended header size exceeds addressable range".to_string(),
                    )
                })?;
                raw_usize.checked_add(4).ok_or_else(|| {
                    AudexError::InvalidData("Extended header size overflow".to_string())
                })?
            };

            // Validate that the extended header size does not exceed the
            // available data. A crafted file could set a huge ext_size to
            // skip past all valid frame data, producing an empty tag.
            if pos + ext_size > data.len() {
                return Err(AudexError::InvalidData(
                    "Extended header size exceeds available tag data".to_string(),
                ));
            }

            pos += ext_size;
        }

        Ok(pos)
    }

    /// Advanced frame reading with error recovery
    fn _read_frames(&mut self, data: &[u8], header: &ID3Header) -> Result<()> {
        let owned_data;
        let data = if header.major_version() < 4 && header.f_unsynch() {
            owned_data = Unsynch::decode(data).map_err(|_| {
                AudexError::InvalidData(
                    "Unsynchronization decode failed: tag data contains invalid byte sequences"
                        .to_string(),
                )
            })?;
            &owned_data
        } else {
            data
        };

        let mut pos = 0;

        let v24_size_parser = if header.major_version() == 4 {
            let known_frames = header.known_frames();
            Some(determine_bpi(data, &known_frames))
        } else {
            None
        };

        // Parse frames with error recovery and a total frame count cap.
        // Legitimate tags rarely exceed a few hundred frames; the cap prevents
        // excessive HashMap insertions from crafted tags with millions of tiny frames.
        let mut error_count = 0;
        let mut frame_count: usize = 0;
        const MAX_ERRORS: usize = 10;
        const MAX_FRAMES_PER_TAG: usize = 50_000;

        while pos < data.len() && error_count < MAX_ERRORS && frame_count < MAX_FRAMES_PER_TAG {
            // Check for padding (all zeros)
            if data[pos] == 0 {
                break;
            }

            // Ensure we have enough data for frame header
            let header_size = if header.major_version() == 2 { 6 } else { 10 };
            if pos + header_size > data.len() {
                break;
            }

            // Parse frame header with error handling
            let frame_header = match self._parse_frame_header_with_bpi(
                &data[pos..],
                header.major_version(),
                v24_size_parser,
            ) {
                Ok(fh) => fh,
                Err(_) => {
                    // Try to recover by skipping one byte
                    pos += 1;
                    error_count += 1;
                    continue;
                }
            };

            pos += header_size;

            // A zero-size frame indicates the start of padding; stop parsing.
            // This is consistent with the file-level parser behavior.
            if frame_header.size == 0 {
                break;
            }

            // Validate frame size against remaining data.
            // Use checked arithmetic to prevent overflow on 32-bit platforms.
            let frame_end = match pos.checked_add(frame_header.size as usize) {
                Some(end) => end,
                None => {
                    // Overflow means the frame size is nonsensical; skip remaining data
                    pos = data.len();
                    error_count += 1;
                    continue;
                }
            };
            if frame_end > data.len() {
                // Skip to the end of available data rather than attempting to scan
                // forward for the next valid frame header. Scanning could misinterpret
                // arbitrary bytes (e.g. audio data or padding) as frame IDs, leading
                // to phantom frames with corrupt content. Stopping here is the safe
                // conservative choice.
                pos += (frame_header.size as usize).min(data.len() - pos);
                error_count += 1;
                continue;
            }

            let frame_data = &data[pos..frame_end];
            pos = frame_end;

            // Process frame with error handling
            match self.parse_and_add_frame(&frame_header.frame_id, frame_data, &frame_header) {
                Ok(_) => {
                    frame_count += 1;
                }
                Err(_) => {
                    error_count += 1;
                }
            }
        }

        Ok(())
    }

    /// Parse frame header with version-specific logic
    fn _parse_frame_header(&self, data: &[u8], version: u8) -> Result<FrameHeader> {
        self._parse_frame_header_with_bpi(data, version, None)
    }

    /// Parse frame header with optional custom size parser for v2.4 iTunes compat
    #[allow(clippy::type_complexity)]
    fn _parse_frame_header_with_bpi(
        &self,
        data: &[u8],
        version: u8,
        v24_size_parser: Option<fn(&[u8]) -> Result<u32>>,
    ) -> Result<FrameHeader> {
        match version {
            2 => {
                if data.len() < 6 {
                    return Err(AudexError::InvalidData(
                        "Frame header too short".to_string(),
                    ));
                }
                FrameHeader::from_bytes_v22(data)
            }
            3 => {
                if data.len() < 10 {
                    return Err(AudexError::InvalidData(
                        "Frame header too short".to_string(),
                    ));
                }
                FrameHeader::from_bytes_v23(data)
            }
            4 => {
                if data.len() < 10 {
                    return Err(AudexError::InvalidData(
                        "Frame header too short".to_string(),
                    ));
                }
                if let Some(parser) = v24_size_parser {
                    FrameHeader::from_bytes_v24_with_size_parser(data, parser)
                } else {
                    FrameHeader::from_bytes_v24(data)
                }
            }
            _ => Err(AudexError::InvalidData(format!(
                "Unsupported ID3 version: {}",
                version
            ))),
        }
    }

    /// Helper to safely extract text from text frames
    fn _extract_text_from_frame(&self, frame: &dyn Frame) -> Option<String> {
        self._extract_text_from_frame_ref(frame)
    }

    fn _extract_text_from_frame_ref(&self, frame: &dyn Frame) -> Option<String> {
        let description = frame.description();

        // RVA2 frames encode channel info in the prefix (e.g. "Master volume: +0.17 dB/0.99")
        // which must be preserved for correct round-tripping.
        if frame.frame_id() == "RVA2" {
            return Some(description);
        }

        // For other frames, extract text after the colon in descriptions like "TIT2: My Title"
        description
            .find(": ")
            .map(|colon_pos| description[colon_pos + 2..].to_string())
    }

    /// Extract text array from a frame (for proper multiple value support)
    fn _extract_text_array_from_frame(&self, frame: &dyn Frame) -> Option<Vec<String>> {
        // Use the new text_values method to get raw text values directly
        frame.text_values()
    }

    /// Helper to extract multiple text values from text frames
    fn _extract_text_values_from_frame(&self, frame: &dyn Frame) -> Vec<String> {
        if let Some(text) = self._extract_text_from_frame(frame) {
            // Frame IDs where "/" is part of the value format (e.g. "5/12" for
            // track numbers, or date strings) and must not be treated as a
            // multi-value separator.
            const SLASH_EXEMPT_FRAMES: &[&str] = &["TRCK", "TPOS", "TDRC", "TDAT", "TYER", "TIME"];
            let frame_id = frame.frame_id();

            // Split on common separators for multi-value fields
            let separators: &[&str] = if SLASH_EXEMPT_FRAMES.contains(&frame_id) {
                &[";", "\\", "||"]
            } else {
                &["/", ";", "\\", "||"]
            };
            for sep in separators {
                if text.contains(sep) {
                    return text.split(sep).map(|s| s.trim().to_string()).collect();
                }
            }
            vec![text]
        } else {
            Vec::new()
        }
    }

    /// Reconstruct frame with new frame ID
    fn _reconstruct_frame_with_new_id(
        &self,
        frame: Box<dyn Frame>,
        new_id: &str,
    ) -> Result<Box<dyn Frame>> {
        if let Some(text) = self._extract_text_from_frame(frame.as_ref()) {
            let text_frame = TextFrame::new(new_id.to_string(), vec![text]);
            Ok(Box::new(text_frame))
        } else {
            // If we can't extract text, return a placeholder
            let placeholder_frame = TextFrame::new(
                new_id.to_string(),
                vec![format!("Upgraded from {}", frame.frame_id())],
            );
            Ok(Box::new(placeholder_frame))
        }
    }

    /// Parse existing ID3v2 header from file to determine tag size
    /// Returns old_size (including 10-byte header) or 0 if no ID3v2 tag exists
    fn parse_existing_id3v2_header<P: AsRef<std::path::Path>>(file_path: P) -> Result<usize> {
        use std::fs::File;
        use std::io::Read;

        let mut file = File::open(file_path)?;
        let mut header_buffer = [0u8; 10];

        // Read first 10 bytes for ID3v2 header
        if file.read_exact(&mut header_buffer).is_err() {
            return Ok(0); // File too short or read error - no existing tag
        }

        // Check for ID3v2 header signature
        if &header_buffer[0..3] != b"ID3" {
            return Ok(0); // No ID3v2 header found
        }

        let vmaj = header_buffer[3];
        let _vrev = header_buffer[4];
        let flags = header_buffer[5];
        let size_bytes = &header_buffer[6..10];

        // Validate version
        if ![2, 3, 4].contains(&vmaj) {
            return Err(AudexError::InvalidData(format!(
                "Unsupported ID3v2.{} version",
                vmaj
            )));
        }

        // Parse synchsafe size (tag data size, excluding header)
        let tag_data_size = ((size_bytes[0] & 0x7F) as u32) << 21
            | ((size_bytes[1] & 0x7F) as u32) << 14
            | ((size_bytes[2] & 0x7F) as u32) << 7
            | ((size_bytes[3] & 0x7F) as u32);

        // Total tag size including the 10-byte header.
        // Use checked arithmetic to prevent overflow on 32-bit targets.
        let total_tag_size = usize::try_from(tag_data_size)
            .ok()
            .and_then(|s| s.checked_add(10))
            .ok_or_else(|| AudexError::InvalidData("Tag data size overflow".to_string()))?;

        // Handle footer flag (ID3v2.4 only)
        let has_footer = vmaj >= 4 && (flags & 0x10) != 0;
        let final_size = if has_footer {
            total_tag_size + 10 // Add footer size
        } else {
            total_tag_size
        };

        Ok(final_size)
    }

    /// Save ID3Tags to a file - internal implementation
    /// Uses in-place modification:
    /// 1. Parse existing ID3v2 header to get old_size
    /// 2. Generate new tag data and get new_size
    /// 3. If new_size > old_size: use insert_bytes()
    /// 4. If new_size < old_size: use delete_bytes()
    /// 5. Write new tag data at file start
    /// 6. Preserve all audio data untouched
    fn save_to_file<P: AsRef<std::path::Path>>(
        &self,
        file_path: P,
        config: &ID3SaveConfig,
    ) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::{Read, Seek, SeekFrom, Write};

        let path = file_path.as_ref();

        // Step 1: Parse existing ID3v2 header to get old_size
        let old_size = Self::parse_existing_id3v2_header(path)?;

        // Step 2: Generate new tag data and get new_size
        let tag_data = self.write_with_config(config)?;
        let new_tag_data = if !tag_data.is_empty() {
            // Calculate padding using PaddingInfo when config.padding is None
            let padded_tag_data = if config.padding.is_none() {
                let file_size = std::fs::metadata(path).map(|m| m.len()).map_err(|e| {
                    AudexError::Io(std::io::Error::new(
                        e.kind(),
                        format!(
                            "Failed to read file metadata for padding calculation: {}",
                            e
                        ),
                    ))
                })?;
                let needed = tag_data.len() + 10; // frame data + 10 byte ID3 header
                let available = old_size.saturating_sub(10); // old frame data space
                // trailing_size = data from tag position (0 for MP3) to end of file
                let trailing_size = i64::try_from(file_size).unwrap_or(i64::MAX);
                let available_i64 = i64::try_from(available).unwrap_or(i64::MAX);
                let needed_i64 = i64::try_from(needed).unwrap_or(i64::MAX);
                let padding_size = available_i64.saturating_sub(needed_i64);
                let info = crate::tags::PaddingInfo::new(padding_size, trailing_size);
                let new_padding = info.get_default_padding().max(0) as usize;

                let mut padded = tag_data;
                padded.resize(padded.len() + new_padding, 0);
                padded
            } else {
                tag_data
            };

            // Global unsynchronization is disabled because the reader does
            // not yet decode it, causing round-trip failures. The tag data is
            // written as-is; per-frame unsync can be added later when the
            // reader supports it.
            let final_tag_data = padded_tag_data;

            // Build complete ID3v2 tag with header
            let mut complete_tag = Vec::new();

            // Write ID3v2 header
            complete_tag.extend_from_slice(b"ID3"); // File identifier
            complete_tag.push(config.v2_version); // Major version
            complete_tag.push(0); // Minor version (revision)
            complete_tag.push(0); // Flags

            // Write synchsafe size (tag data length only, not including header).
            let size = u32::try_from(final_tag_data.len()).map_err(|_| {
                AudexError::InvalidData(format!(
                    "ID3 tag data too large for u32 size field: {} bytes",
                    final_tag_data.len()
                ))
            })?;
            let synchsafe = [
                ((size >> 21) & 0x7F) as u8,
                ((size >> 14) & 0x7F) as u8,
                ((size >> 7) & 0x7F) as u8,
                (size & 0x7F) as u8,
            ];
            complete_tag.extend_from_slice(&synchsafe);

            // Write tag data (with padding and unsync encoding applied)
            complete_tag.extend_from_slice(&final_tag_data);
            complete_tag
        } else {
            Vec::new()
        };

        let new_size = new_tag_data.len();

        // Step 3-4: Adjust file size based on size difference
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;

        if new_size > old_size {
            // Step 3: New tag is larger - insert bytes at end of old tag (offset old_size)
            let bytes_to_insert = new_size - old_size;
            insert_bytes(&mut file, bytes_to_insert as u64, old_size as u64, None)?;
        } else if new_size < old_size {
            // Step 4: New tag is smaller - delete bytes after new tag (offset new_size)
            let bytes_to_delete = old_size - new_size;
            delete_bytes(&mut file, bytes_to_delete as u64, new_size as u64, None)?;
        }
        // If sizes are equal, no file size adjustment needed

        // Step 5: Write new tag data at file start
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&new_tag_data)?;
        file.flush()?;

        // Step 6: Audio data is automatically preserved by the byte manipulation functions
        // The insert_bytes/delete_bytes functions handle moving the audio data correctly

        // Step 7: Handle ID3v1 tag at end of file
        {
            use crate::id3::file::ID3v1SaveOptions;
            let file_len = file.seek(SeekFrom::End(0))?;
            let has_existing_v1 = if file_len >= 128 {
                let mut tag_header = [0u8; 3];
                file.seek(SeekFrom::End(-128))?;
                file.read_exact(&mut tag_header).ok() == Some(()) && &tag_header == b"TAG"
            } else {
                false
            };

            match config.write_v1 {
                ID3v1SaveOptions::CREATE => {
                    let v1_data = crate::id3::id3v1::make_id3v1_from_dict(&self.dict);
                    if has_existing_v1 {
                        file.seek(SeekFrom::End(-128))?;
                    } else {
                        file.seek(SeekFrom::End(0))?;
                    }
                    file.write_all(&v1_data)?;
                    file.flush()?;
                }
                ID3v1SaveOptions::UPDATE if has_existing_v1 => {
                    let v1_data = crate::id3::id3v1::make_id3v1_from_dict(&self.dict);
                    file.seek(SeekFrom::End(-128))?;
                    file.write_all(&v1_data)?;
                    file.flush()?;
                }
                ID3v1SaveOptions::REMOVE if has_existing_v1 => {
                    // Remove existing ID3v1 tag by truncating
                    let new_len = file_len - 128;
                    file.set_len(new_len)?;
                }
                _ => {
                    // UPDATE with no existing tag, or REMOVE with no tag — nothing to do
                }
            }
        }

        Ok(())
    }

    /// Save ID3Tags to a writer that implements `Read + Write + Seek`.
    ///
    /// Writer-based equivalent of `save_to_file`. The writer must contain the
    /// complete original data (audio + any existing tags). The method modifies
    /// the writer in-place, identical to how `save_to_file` modifies a file on
    /// disk.
    #[allow(dead_code)]
    pub fn save_to_writer<W: std::io::Read + std::io::Write + std::io::Seek + 'static>(
        &self,
        writer: &mut W,
        config: &ID3SaveConfig,
    ) -> Result<()> {
        use std::io::SeekFrom;

        // Step 1: Parse existing ID3v2 header from the writer to get old_size
        let old_size = Self::parse_id3v2_header_from_reader(writer)?;

        // Step 2: Generate new tag data and get new_size
        let tag_data = self.write_with_config(config)?;
        let new_tag_data = if !tag_data.is_empty() {
            let padded_tag_data = if config.padding.is_none() {
                let file_size = writer.seek(SeekFrom::End(0))?;
                writer.seek(SeekFrom::Start(0))?;
                let needed = tag_data.len() + 10;
                let available = old_size.saturating_sub(10);
                let trailing_size = i64::try_from(file_size).unwrap_or(i64::MAX);
                let available_i64 = i64::try_from(available).unwrap_or(i64::MAX);
                let needed_i64 = i64::try_from(needed).unwrap_or(i64::MAX);
                let padding_size = available_i64.saturating_sub(needed_i64);
                let info = crate::tags::PaddingInfo::new(padding_size, trailing_size);
                let new_padding = info.get_default_padding().max(0) as usize;

                let mut padded = tag_data;
                padded.resize(padded.len() + new_padding, 0);
                padded
            } else {
                tag_data
            };

            let final_tag_data = padded_tag_data;

            // Build complete ID3v2 tag with header
            let mut complete_tag = Vec::new();
            complete_tag.extend_from_slice(b"ID3");
            complete_tag.push(config.v2_version);
            complete_tag.push(0); // revision
            complete_tag.push(0); // flags

            let size = u32::try_from(final_tag_data.len()).map_err(|_| {
                AudexError::InvalidData(format!(
                    "ID3 tag data too large for u32 size field: {} bytes",
                    final_tag_data.len()
                ))
            })?;
            let synchsafe = [
                ((size >> 21) & 0x7F) as u8,
                ((size >> 14) & 0x7F) as u8,
                ((size >> 7) & 0x7F) as u8,
                (size & 0x7F) as u8,
            ];
            complete_tag.extend_from_slice(&synchsafe);
            complete_tag.extend_from_slice(&final_tag_data);
            complete_tag
        } else {
            Vec::new()
        };

        let new_size = new_tag_data.len();

        // Step 3-4: Adjust data size based on size difference
        if new_size > old_size {
            let bytes_to_insert = new_size - old_size;
            insert_bytes(writer, bytes_to_insert as u64, old_size as u64, None)?;
        } else if new_size < old_size {
            let bytes_to_delete = old_size - new_size;
            delete_bytes(writer, bytes_to_delete as u64, new_size as u64, None)?;
        }

        // Step 5: Write new tag data at start
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&new_tag_data)?;
        writer.flush()?;

        // Step 7: Handle ID3v1 tag at end of writer
        {
            use crate::id3::file::ID3v1SaveOptions;
            let file_len = writer.seek(SeekFrom::End(0))?;
            let has_existing_v1 = if file_len >= 128 {
                let mut tag_header = [0u8; 3];
                writer.seek(SeekFrom::End(-128))?;
                writer.read_exact(&mut tag_header).ok() == Some(()) && &tag_header == b"TAG"
            } else {
                false
            };

            match config.write_v1 {
                ID3v1SaveOptions::CREATE => {
                    let v1_data = crate::id3::id3v1::make_id3v1_from_dict(&self.dict);
                    if has_existing_v1 {
                        writer.seek(SeekFrom::End(-128))?;
                    } else {
                        writer.seek(SeekFrom::End(0))?;
                    }
                    writer.write_all(&v1_data)?;
                    writer.flush()?;
                }
                ID3v1SaveOptions::UPDATE if has_existing_v1 => {
                    let v1_data = crate::id3::id3v1::make_id3v1_from_dict(&self.dict);
                    writer.seek(SeekFrom::End(-128))?;
                    writer.write_all(&v1_data)?;
                    writer.flush()?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Parse existing ID3v2 header from a reader to determine the tag size.
    ///
    /// Returns the total tag size in bytes (including the 10-byte header and
    /// optional footer), or 0 if no ID3v2 tag is present.
    #[allow(dead_code)]
    fn parse_id3v2_header_from_reader<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
    ) -> Result<usize> {
        use std::io::SeekFrom;

        reader.seek(SeekFrom::Start(0))?;
        let mut header_buffer = [0u8; 10];

        if reader.read_exact(&mut header_buffer).is_err() {
            return Ok(0);
        }

        if &header_buffer[0..3] != b"ID3" {
            return Ok(0);
        }

        let vmaj = header_buffer[3];
        let _vrev = header_buffer[4];
        let flags = header_buffer[5];
        let size_bytes = &header_buffer[6..10];

        if ![2, 3, 4].contains(&vmaj) {
            return Err(AudexError::InvalidData(format!(
                "Unsupported ID3v2.{} version",
                vmaj
            )));
        }

        let tag_data_size = ((size_bytes[0] & 0x7F) as u32) << 21
            | ((size_bytes[1] & 0x7F) as u32) << 14
            | ((size_bytes[2] & 0x7F) as u32) << 7
            | ((size_bytes[3] & 0x7F) as u32);

        // Use checked arithmetic to prevent overflow on 32-bit targets
        let total_tag_size = usize::try_from(tag_data_size)
            .ok()
            .and_then(|s| s.checked_add(10))
            .ok_or_else(|| AudexError::InvalidData("Tag data size overflow".to_string()))?;

        let has_footer = vmaj >= 4 && (flags & 0x10) != 0;
        let final_size = if has_footer {
            total_tag_size + 10
        } else {
            total_tag_size
        };

        Ok(final_size)
    }

    /// Async version of `parse_existing_id3v2_header`.
    /// Reads the first 10 bytes of the file using tokio I/O to determine the
    /// existing ID3v2 tag size.
    #[cfg(feature = "async")]
    async fn parse_existing_id3v2_header_async<P: AsRef<std::path::Path>>(
        file_path: P,
    ) -> Result<usize> {
        use tokio::io::AsyncReadExt;

        let mut file = tokio::fs::File::open(file_path).await?;
        let mut header_buffer = [0u8; 10];

        if file.read_exact(&mut header_buffer).await.is_err() {
            return Ok(0);
        }

        if &header_buffer[0..3] != b"ID3" {
            return Ok(0);
        }

        let vmaj = header_buffer[3];
        let flags = header_buffer[5];
        let size_bytes = &header_buffer[6..10];

        if ![2, 3, 4].contains(&vmaj) {
            return Err(AudexError::InvalidData(format!(
                "Unsupported ID3v2.{} version",
                vmaj
            )));
        }

        let tag_data_size = ((size_bytes[0] & 0x7F) as u32) << 21
            | ((size_bytes[1] & 0x7F) as u32) << 14
            | ((size_bytes[2] & 0x7F) as u32) << 7
            | ((size_bytes[3] & 0x7F) as u32);

        // Use checked arithmetic to prevent overflow on 32-bit targets
        let total_tag_size = usize::try_from(tag_data_size)
            .ok()
            .and_then(|s| s.checked_add(10))
            .ok_or_else(|| AudexError::InvalidData("Tag data size overflow".to_string()))?;

        let has_footer = vmaj >= 4 && (flags & 0x10) != 0;
        let final_size = if has_footer {
            total_tag_size + 10
        } else {
            total_tag_size
        };

        Ok(final_size)
    }

    /// Native async version of `save_to_file`.
    /// Uses tokio I/O for all file operations instead of blocking std I/O.
    #[cfg(feature = "async")]
    pub(crate) async fn save_to_file_async<P: AsRef<std::path::Path>>(
        &self,
        file_path: P,
        config: &ID3SaveConfig,
    ) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        let path = file_path.as_ref();

        // Step 1: Parse existing ID3v2 header to get old_size
        let old_size = Self::parse_existing_id3v2_header_async(path).await?;

        // Step 2: Generate new tag data (CPU-only, sync is fine)
        let tag_data = self.write_with_config(config)?;
        let new_tag_data = if !tag_data.is_empty() {
            let padded_tag_data = if config.padding.is_none() {
                let file_size = tokio::fs::metadata(path)
                    .await
                    .map(|m| m.len())
                    .map_err(|e| {
                        AudexError::Io(std::io::Error::new(
                            e.kind(),
                            format!(
                                "Failed to read file metadata for padding calculation: {}",
                                e
                            ),
                        ))
                    })?;
                let needed = tag_data.len() + 10;
                let available = old_size.saturating_sub(10);
                let trailing_size = i64::try_from(file_size).unwrap_or(i64::MAX);
                let available_i64 = i64::try_from(available).unwrap_or(i64::MAX);
                let needed_i64 = i64::try_from(needed).unwrap_or(i64::MAX);
                let padding_size = available_i64.saturating_sub(needed_i64);
                let info = crate::tags::PaddingInfo::new(padding_size, trailing_size);
                let new_padding = info.get_default_padding().max(0) as usize;

                let mut padded = tag_data;
                padded.resize(padded.len() + new_padding, 0);
                padded
            } else {
                tag_data
            };

            let final_tag_data = padded_tag_data;

            let mut complete_tag = Vec::new();
            complete_tag.extend_from_slice(b"ID3");
            complete_tag.push(config.v2_version);
            complete_tag.push(0); // revision
            complete_tag.push(0); // flags

            let size = u32::try_from(final_tag_data.len()).map_err(|_| {
                AudexError::InvalidData(format!(
                    "ID3 tag data too large for u32 size field: {} bytes",
                    final_tag_data.len()
                ))
            })?;
            let synchsafe = [
                ((size >> 21) & 0x7F) as u8,
                ((size >> 14) & 0x7F) as u8,
                ((size >> 7) & 0x7F) as u8,
                (size & 0x7F) as u8,
            ];
            complete_tag.extend_from_slice(&synchsafe);
            complete_tag.extend_from_slice(&final_tag_data);
            complete_tag
        } else {
            Vec::new()
        };

        let new_size = new_tag_data.len();

        // Step 3-4: Adjust file size
        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .await?;

        if new_size > old_size {
            let bytes_to_insert = new_size - old_size;
            crate::util::insert_bytes_async(
                &mut file,
                bytes_to_insert as u64,
                old_size as u64,
                None,
            )
            .await?;
        } else if new_size < old_size {
            let bytes_to_delete = old_size - new_size;
            crate::util::delete_bytes_async(
                &mut file,
                bytes_to_delete as u64,
                new_size as u64,
                None,
            )
            .await?;
        }

        // Step 5: Write new tag data at file start
        file.seek(std::io::SeekFrom::Start(0)).await?;
        file.write_all(&new_tag_data).await?;
        file.flush().await?;

        // Step 6: Handle ID3v1 tag at end of file
        {
            use crate::id3::file::ID3v1SaveOptions;
            let file_len = file.seek(std::io::SeekFrom::End(0)).await?;
            let has_existing_v1 = if file_len >= 128 {
                let mut tag_header = [0u8; 3];
                file.seek(std::io::SeekFrom::End(-128)).await?;
                file.read_exact(&mut tag_header).await.is_ok() && &tag_header == b"TAG"
            } else {
                false
            };

            match config.write_v1 {
                ID3v1SaveOptions::CREATE => {
                    let v1_data = crate::id3::id3v1::make_id3v1_from_dict(&self.dict);
                    if has_existing_v1 {
                        file.seek(std::io::SeekFrom::End(-128)).await?;
                    } else {
                        file.seek(std::io::SeekFrom::End(0)).await?;
                    }
                    file.write_all(&v1_data).await?;
                    file.flush().await?;
                }
                ID3v1SaveOptions::UPDATE if has_existing_v1 => {
                    let v1_data = crate::id3::id3v1::make_id3v1_from_dict(&self.dict);
                    file.seek(std::io::SeekFrom::End(-128)).await?;
                    file.write_all(&v1_data).await?;
                    file.flush().await?;
                }
                _ => {}
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Serde support — manual Serialize/Deserialize for ID3Tags
// ---------------------------------------------------------------------------
//
// ID3Tags holds `Box<dyn Frame>` trait objects which cannot derive
// Serialize.  Instead we convert to/from an intermediate struct that
// captures the most useful information in a format-agnostic way.

/// Serde-friendly representation of an ID3v2 tag container.
///
/// Created via [`ID3Tags::to_serializable`] and converted back
/// with [`ID3Tags::from_serializable`].
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SerializableID3Tags {
    /// ID3 version as (major, revision) — e.g. (4, 0) for ID3v2.4
    pub version: (u8, u8),
    /// Standard text frames keyed by frame ID (e.g. "TIT2" → ["Title"])
    pub text_frames: HashMap<String, Vec<String>>,
    /// User-defined text frames (TXXX) keyed by description
    pub user_text_frames: HashMap<String, Vec<String>>,
    /// Comment frames (COMM)
    pub comment_frames: Vec<SerializableComment>,
    /// Embedded picture frames (APIC)
    pub picture_frames: Vec<SerializablePicture>,
    /// User-defined URL frames (WXXX) keyed by description
    pub url_frames: HashMap<String, String>,
    /// Frame IDs present in the tag that could not be fully serialized
    pub unknown_frame_ids: Vec<String>,
}

/// A single COMM (comment) frame.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SerializableComment {
    /// ISO-639-2 language code (e.g. "eng")
    pub language: String,
    /// Short content description
    pub description: String,
    /// The comment text
    pub text: String,
}

/// A single APIC (attached picture) frame.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SerializablePicture {
    /// Human-readable picture type (e.g. "CoverFront")
    pub picture_type: String,
    /// MIME type of the image (e.g. "image/jpeg")
    pub mime_type: String,
    /// Optional description
    pub description: String,
    /// Base64-encoded image data
    #[serde(with = "crate::serde_helpers::bytes_as_base64")]
    pub data: Vec<u8>,
}

#[cfg(feature = "serde")]
impl ID3Tags {
    /// Build a serializable snapshot of this ID3 tag container.
    ///
    /// Iterates over every frame in the tag and classifies it into one
    /// of the known categories (text, user-text, comment, picture, URL).
    /// Frames that cannot be classified are recorded in `unknown_frame_ids`.
    pub fn to_serializable(&self) -> SerializableID3Tags {
        let mut text_frames: HashMap<String, Vec<String>> = HashMap::new();
        let mut user_text_frames: HashMap<String, Vec<String>> = HashMap::new();
        let mut comment_frames: Vec<SerializableComment> = Vec::new();
        let mut picture_frames: Vec<SerializablePicture> = Vec::new();
        let mut url_frames: HashMap<String, String> = HashMap::new();
        let mut unknown_frame_ids: Vec<String> = Vec::new();

        for frame in self.dict.values() {
            let any = frame.as_any();

            // Try downcasting to each known concrete frame type
            if let Some(tf) = any.downcast_ref::<TextFrame>() {
                text_frames.insert(tf.frame_id.clone(), tf.text.clone());
            } else if let Some(txxx) = any.downcast_ref::<TXXX>() {
                user_text_frames.insert(txxx.description.clone(), txxx.text.clone());
            } else if let Some(comm) = any.downcast_ref::<COMM>() {
                let lang = String::from_utf8_lossy(&comm.language).into_owned();
                comment_frames.push(SerializableComment {
                    language: lang,
                    description: comm.description.clone(),
                    text: comm.text.clone(),
                });
            } else if let Some(apic) = any.downcast_ref::<APIC>() {
                picture_frames.push(SerializablePicture {
                    picture_type: format!("{:?}", apic.type_),
                    mime_type: apic.mime.clone(),
                    description: apic.desc.clone(),
                    data: apic.data.clone(),
                });
            } else if let Some(wxxx) = any.downcast_ref::<WXXX>() {
                url_frames.insert(wxxx.description.clone(), wxxx.url.clone());
            } else {
                // Record unrecognised frame IDs so consumers know data was skipped
                let fid = frame.frame_id().to_string();
                if !unknown_frame_ids.contains(&fid) {
                    unknown_frame_ids.push(fid);
                }
            }
        }

        SerializableID3Tags {
            version: self.version(),
            text_frames,
            user_text_frames,
            comment_frames,
            picture_frames,
            url_frames,
            unknown_frame_ids,
        }
    }

    /// Reconstruct an `ID3Tags` from a serialized representation.
    ///
    /// Only text frames, TXXX, COMM, APIC, and WXXX are restored — any
    /// frame IDs listed in `unknown_frame_ids` are silently dropped
    /// because there is not enough information to recreate them.
    pub fn from_serializable(s: SerializableID3Tags) -> Self {
        use crate::id3::frames::PictureType;

        let mut tags = ID3Tags::new();
        tags.set_version(s.version.0, s.version.1);

        // Restore standard text frames
        for (frame_id, text) in s.text_frames {
            let _ = tags.add_text_frame(&frame_id, text);
        }

        // Restore user-defined text frames (TXXX)
        for (description, text) in s.user_text_frames {
            let frame = TXXX::new(TextEncoding::Utf8, description, text);
            let _ = tags.add(Box::new(frame));
        }

        // Restore comment frames
        for comment in s.comment_frames {
            let lang_bytes = comment.language.as_bytes();
            let mut lang = [b' '; 3];
            for (i, &b) in lang_bytes.iter().take(3).enumerate() {
                lang[i] = b;
            }
            let frame = COMM::new(TextEncoding::Utf8, lang, comment.description, comment.text);
            let _ = tags.add(Box::new(frame));
        }

        // Restore picture frames
        for pic in s.picture_frames {
            let ptype = match pic.picture_type.as_str() {
                "CoverFront" => PictureType::CoverFront,
                "CoverBack" => PictureType::CoverBack,
                "FileIcon" => PictureType::FileIcon,
                "LeadArtist" => PictureType::LeadArtist,
                "Artist" => PictureType::Artist,
                "Band" => PictureType::Band,
                "Composer" => PictureType::Composer,
                "Conductor" => PictureType::Conductor,
                "Lyricist" => PictureType::Lyricist,
                "Media" => PictureType::Media,
                "LeafletPage" => PictureType::LeafletPage,
                "Illustration" => PictureType::Illustration,
                _ => PictureType::Other,
            };
            let frame = APIC {
                encoding: TextEncoding::Utf8,
                mime: pic.mime_type,
                type_: ptype,
                desc: pic.description,
                data: pic.data,
            };
            let _ = tags.add(Box::new(frame));
        }

        // Restore WXXX frames
        for (description, url) in s.url_frames {
            let frame = WXXX::new(description, url);
            let _ = tags.add(Box::new(frame));
        }

        tags
    }
}

/// Serialize ID3Tags by converting to [`SerializableID3Tags`] first.
#[cfg(feature = "serde")]
impl serde::Serialize for ID3Tags {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        self.to_serializable().serialize(serializer)
    }
}

/// Deserialize ID3Tags from a [`SerializableID3Tags`] representation.
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ID3Tags {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let s = SerializableID3Tags::deserialize(deserializer)?;
        Ok(ID3Tags::from_serializable(s))
    }
}
