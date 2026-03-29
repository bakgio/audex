//! # Vorbis Comment Support
//!
//! This module provides a comprehensive implementation of Vorbis Comments, the tagging
//! format used by FLAC, Ogg Vorbis, Ogg Opus, and other Xiph.Org formats.
//!
//! ## Overview
//!
//! Vorbis Comments are a simple, flexible tagging system that stores metadata as
//! key-value pairs. Unlike ID3 tags, Vorbis Comments:
//! - Use human-readable field names (e.g., "TITLE", "ARTIST")
//! - Support multiple values for the same field
//! - Allow arbitrary custom fields
//! - Are case-insensitive for field names
//! - Store all data as UTF-8 text
//!
//! ## Supported Formats
//!
//! This implementation is shared across multiple formats:
//! - **FLAC**: Metadata block type 4 (VORBIS_COMMENT)
//! - **Ogg Vorbis**: Comment header packet
//! - **Ogg Opus**: Comment header packet
//! - **Ogg FLAC**: Vorbis comment in Ogg encapsulation
//!
//! ## Standard Fields
//!
//! Common Vorbis Comment fields include:
//! - **TITLE**: Track title
//! - **ARTIST**: Track artist(s)
//! - **ALBUM**: Album name
//! - **DATE**: Release date (often just year)
//! - **GENRE**: Musical genre
//! - **TRACKNUMBER**: Track number on album
//! - **ALBUMARTIST**: Album artist (for compilations)
//! - **COMPOSER**: Composer of the work
//! - **PERFORMER**: Performing artist
//! - **COPYRIGHT**: Copyright information
//! - **LICENSE**: License information
//! - **ORGANIZATION**: Record label or organization
//! - **DESCRIPTION**: Track description or comment
//! - **ISRC**: International Standard Recording Code
//!
//! ## Basic Usage
//!
//! ```no_run
//! use audex::vorbis::VCommentDict;
//! use audex::Tags;
//!
//! // Create new Vorbis Comments
//! let mut vc = VCommentDict::new();
//!
//! // Set single-value fields
//! vc.set("TITLE", vec!["My Song".to_string()]);
//! vc.set("ARTIST", vec!["Artist Name".to_string()]);
//!
//! // Set multi-value field (e.g., multiple artists)
//! vc.set("ARTIST", vec![
//!     "First Artist".to_string(),
//!     "Second Artist".to_string(),
//! ]);
//!
//! // Read fields
//! if let Some(title) = vc.get("title") {  // Case-insensitive
//!     println!("Title: {:?}", title);
//! }
//! ```
//!
//! ## Embedded Pictures
//!
//! While Vorbis Comments are text-only, FLAC-style embedded pictures can be stored
//! using base64-encoded METADATA_BLOCK_PICTURE fields. This module provides helper
//! methods for encoding and decoding these pictures.
//!
//! ## ReplayGain Support
//!
//! The `VCommentDict` type provides convenient methods for reading and writing
//! ReplayGain information stored in standard Vorbis Comment fields:
//!
//! - `VCommentDict::get_replaygain()` - Extract ReplayGain values into a structured format
//! - `VCommentDict::set_replaygain()` - Write ReplayGain values from a structured format
//! - `VCommentDict::clear_replaygain()` - Remove all ReplayGain fields
//!
//! ReplayGain data uses standardized field names:
//! - `REPLAYGAIN_TRACK_GAIN` - Track normalization adjustment in dB
//! - `REPLAYGAIN_TRACK_PEAK` - Track peak sample value (0.0 to 1.0)
//! - `REPLAYGAIN_ALBUM_GAIN` - Album normalization adjustment in dB
//! - `REPLAYGAIN_ALBUM_PEAK` - Album peak sample value (0.0 to 1.0)
//! - `REPLAYGAIN_REFERENCE_LOUDNESS` - Reference loudness level (typically 89.0 dB)
//!
//! See the [`crate::replaygain`] module for more details on ReplayGain functionality.
//!
//! ## See Also
//!
//! - `VCommentDict` - Main Vorbis Comment dictionary type
//! - `ErrorMode` - Error handling modes for invalid data
//! - [`crate::replaygain`] - ReplayGain information handling
//! - [Vorbis Comment Specification](https://xiph.org/vorbis/doc/v-comment.html)

use crate::limits::ParseLimits;
use crate::tags::{Metadata, Tags};
use crate::{AudexError, Result, VERSION_STRING};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::ops::Index;
use std::slice::SliceIndex;

#[cfg(feature = "async")]
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

use crate::flac::Picture;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

/// Errors specific to Vorbis Comment parsing and validation.
///
/// These errors occur when Vorbis Comment data is malformed, contains invalid
/// UTF-8 text, or violates the Vorbis Comment specification.
#[derive(Debug, Clone, PartialEq)]
pub enum VorbisError {
    /// The framing bit was not set or is invalid.
    ///
    /// In Ogg Vorbis and Opus streams, comment packets must end with a framing bit
    /// set to 1. This error indicates the framing bit was 0 or missing, suggesting
    /// corrupted or non-compliant data.
    UnsetFrameError,

    /// Comment data contains invalid UTF-8 sequences.
    ///
    /// All Vorbis Comment values must be valid UTF-8 text. This error occurs when
    /// the parser encounters byte sequences that cannot be decoded as UTF-8.
    /// The error message contains details about what failed.
    ///
    /// This can happen with:
    /// - Corrupted files
    /// - Files using non-UTF-8 encodings (e.g., Latin-1, Windows-1252)
    /// - Binary data incorrectly stored in text fields
    EncodingError(String),

    /// A comment field key is invalid.
    ///
    /// Vorbis Comment keys must contain only printable ASCII characters (0x20-0x7D)
    /// and cannot contain the equals sign ('='). This error occurs when a key
    /// violates these rules. The error message contains the invalid key.
    ///
    /// Valid examples: "TITLE", "ARTIST", "DATE", "CUSTOM_FIELD"
    /// Invalid examples: "" (empty), "tit=le" (contains '=')
    InvalidKey(String),
}

impl std::fmt::Display for VorbisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VorbisError::UnsetFrameError => write!(f, "framing bit was not set"),
            VorbisError::EncodingError(msg) => write!(f, "encoding error: {}", msg),
            VorbisError::InvalidKey(key) => write!(f, "invalid vorbis key: {}", key),
        }
    }
}

impl std::error::Error for VorbisError {}

/// Error handling strategy for Vorbis Comment parsing.
///
/// Determines how the parser handles invalid UTF-8 sequences and other
/// data errors encountered during parsing.
///
/// # Examples
///
/// ```
/// use audex::vorbis::ErrorMode;
///
/// // Strict mode: fail on any error
/// let mode = ErrorMode::Strict;
///
/// // Replace mode: use replacement character for invalid UTF-8
/// let mode = ErrorMode::Replace;
///
/// // Ignore mode: skip invalid data
/// let mode = ErrorMode::Ignore;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ErrorMode {
    /// Fail immediately on any invalid data.
    ///
    /// This is the most conservative mode. Any invalid UTF-8, malformed
    /// field, or specification violation will cause parsing to fail with
    /// an error. Use this mode when data integrity is critical.
    ///
    /// **Default mode**
    #[default]
    Strict,

    /// Replace invalid UTF-8 with Unicode replacement characters (U+FFFD).
    ///
    /// When invalid UTF-8 sequences are encountered, they are replaced with
    /// the standard Unicode replacement character (�). This allows parsing
    /// to continue while preserving some indication of data corruption.
    ///
    /// Useful for recovering readable text from partially corrupted files.
    Replace,

    /// Skip invalid data silently.
    ///
    /// Invalid UTF-8 sequences and malformed fields are ignored entirely.
    /// Parsing continues as if the invalid data didn't exist. This is the
    /// most permissive mode.
    ///
    /// Use with caution - this may hide serious data corruption issues.
    Ignore,
}

impl std::str::FromStr for ErrorMode {
    type Err = AudexError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "strict" => Ok(ErrorMode::Strict),
            "replace" => Ok(ErrorMode::Replace),
            "ignore" => Ok(ErrorMode::Ignore),
            _ => Err(AudexError::InvalidData(format!(
                "invalid error mode: {}",
                s
            ))),
        }
    }
}

/// Return true if a string is a valid Vorbis comment key.
///
/// Keys must be printable ASCII between 0x20 and 0x7D (inclusive)
/// and cannot contain '='.
pub fn is_valid_key(key: &str) -> bool {
    if key.is_empty() {
        return false;
    }

    for c in key.chars() {
        let code = c as u32;
        if !(0x20..=0x7D).contains(&code) || c == '=' {
            return false;
        }
    }

    true
}

/// Low-level Vorbis Comment parser and storage.
///
/// `VComment` provides direct access to Vorbis Comment data with full control over
/// the underlying comment structure. For most use cases, [`VCommentDict`] offers a
/// more convenient key-value interface.
///
/// # Overview
///
/// This struct stores Vorbis Comments as an ordered list of key-value pairs, preserving
/// the original order and supporting multiple values per key. It handles the binary
/// serialization format used in FLAC metadata blocks and Ogg stream comment headers.
///
/// # When to Use VComment vs VCommentDict
///
/// Use `VComment` when you need:
/// - Direct access to the ordered list of comments
/// - Control over comment ordering and insertion positions
/// - Index-based access to individual comments
/// - Implementation of custom tag processing logic
///
/// Use [`VCommentDict`] when you need:
/// - Simple key-value tag access
/// - Automatic key normalization
/// - Standard tag manipulation patterns
///
/// # Key Normalization
///
/// Per the Vorbis Comment specification, field names are case-insensitive. This
/// implementation normalizes all keys to lowercase internally:
/// - "TITLE", "Title", and "title" all map to the same field
/// - Original case is not preserved
///
/// # Framing Bit
///
/// Ogg Vorbis and Ogg Opus streams require a framing bit (a byte with value 1) at
/// the end of the comment block. FLAC metadata blocks do not use framing. The
/// `framing` parameter controls this behavior during load/write operations.
///
/// # Binary Format
///
/// The Vorbis Comment block format (all values little-endian):
/// 1. Vendor string length (4 bytes)
/// 2. Vendor string (UTF-8)
/// 3. Number of comments (4 bytes)
/// 4. For each comment:
///    - Comment length (4 bytes)
///    - Comment string as "KEY=value" (UTF-8)
/// 5. Framing bit (1 byte, only for Ogg streams)
///
/// # Examples
///
/// ## Basic Usage
///
/// ```
/// use audex::vorbis::VComment;
/// use audex::Tags;
///
/// let mut vc = VComment::new();
///
/// // Add comments using push (preserves order)
/// vc.push("TITLE".to_string(), "My Song".to_string()).unwrap();
/// vc.push("ARTIST".to_string(), "Artist Name".to_string()).unwrap();
///
/// // Access via Tags trait (case-insensitive)
/// let title = vc.get("title").unwrap();
/// assert_eq!(title[0], "My Song");
/// ```
///
/// ## Index-Based Access
///
/// ```
/// use audex::vorbis::VComment;
///
/// let mut vc = VComment::new();
/// vc.push("TITLE".to_string(), "First".to_string()).unwrap();
/// vc.push("TITLE".to_string(), "Second".to_string()).unwrap();
///
/// // Access by index
/// assert_eq!(vc[0].1, "First");
/// assert_eq!(vc[1].1, "Second");
///
/// // Insert at specific position
/// vc.insert(1, "ARTIST".to_string(), "Artist".to_string()).unwrap();
/// assert_eq!(vc[1].0, "ARTIST"); // Keys preserve original case
/// ```
///
/// ## Serialization
///
/// ```
/// use audex::vorbis::VComment;
///
/// let mut vc = VComment::new();
/// vc.push("TITLE".to_string(), "Test".to_string()).unwrap();
///
/// // Serialize without framing (for FLAC)
/// let bytes = vc.to_bytes_with_framing(false).unwrap();
///
/// // Deserialize
/// let loaded = VComment::from_bytes_with_options(&bytes, Default::default(), false).unwrap();
/// ```
///
/// # See Also
///
/// - [`VCommentDict`] - Higher-level dictionary interface
/// - [`ErrorMode`] - Error handling options for parsing
/// - [`is_valid_key`] - Key validation function
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VComment {
    /// The vendor string identifying the encoder that created this file.
    ///
    /// This string typically contains the name and version of the encoding
    /// software. For files created by this library, this defaults to
    /// "audex {version}".
    pub vendor: String,

    /// The comment data as (key, value) pairs, preserving original order.
    ///
    /// Keys preserve their original case. Multiple entries with the same key
    /// (case-insensitive) are allowed and represent multi-value fields.
    pub data: Vec<(String, String)>,

    /// Internal tag storage for efficient case-insensitive lookup.
    ///
    /// This HashMap mirrors the data in `data` but organized by key for O(1)
    /// access. It is automatically kept in sync with `data`.
    tags: HashMap<String, Vec<String>>,

    /// Whether to use a framing bit when writing.
    ///
    /// Set to `true` for Ogg Vorbis/Opus streams, `false` for FLAC metadata.
    framing: bool,
}

impl VComment {
    /// Create a new VComment with the default vendor string
    pub fn new() -> Self {
        Self {
            vendor: format!("Audex {}", VERSION_STRING),
            data: Vec::new(),
            tags: HashMap::new(),
            framing: true,
        }
    }

    /// Create a new VComment with a specific vendor string
    pub fn with_vendor(vendor: String) -> Self {
        Self {
            vendor,
            data: Vec::new(),
            tags: HashMap::new(),
            framing: true,
        }
    }

    /// Load vorbis comments from a reader
    pub fn load<R: Read>(
        &mut self,
        reader: &mut R,
        errors: ErrorMode,
        framing: bool,
    ) -> Result<()> {
        self.framing = framing;
        self.data.clear();
        self.tags.clear();

        trace_event!("loading Vorbis Comment data");

        let limits = ParseLimits::default();

        // Read vendor string length
        let vendor_length = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| AudexError::InvalidData(format!("failed to read vendor length: {}", e)))?;

        // Validate vendor length to prevent OOM attacks
        // Maximum 1 MB for vendor string (extremely generous for metadata)
        const MAX_VENDOR_LENGTH: u32 = 1_000_000;
        if vendor_length > MAX_VENDOR_LENGTH {
            return Err(AudexError::InvalidData(format!(
                "Vorbis vendor length {} exceeds maximum {}",
                vendor_length, MAX_VENDOR_LENGTH
            )));
        }

        let mut cumulative_bytes: u64 = 4;
        cumulative_bytes = cumulative_bytes.saturating_add(vendor_length as u64);
        limits.check_tag_size(cumulative_bytes, "Vorbis comment data")?;

        // Read vendor string
        let mut vendor_bytes = vec![0u8; vendor_length as usize];
        reader
            .read_exact(&mut vendor_bytes)
            .map_err(|e| AudexError::InvalidData(format!("failed to read vendor string: {}", e)))?;

        self.vendor = Self::decode_string(vendor_bytes, errors)?;
        trace_event!(vendor = %self.vendor, "Vorbis Comment vendor string");

        // Read number of comments
        let comment_count = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| AudexError::InvalidData(format!("failed to read comment count: {}", e)))?;
        trace_event!(comment_count = comment_count, "Vorbis Comment entry count");

        // Validate comment count to prevent excessive iteration.
        // 10,000 is well above any legitimate file (typical files have
        // 10-50 comments) but still caps the per-field parsing cost: each
        // comment requires a length read, allocation, UTF-8 decode, and
        // key normalization. The cumulative byte budget enforced after this
        // check provides the complementary memory guard.
        const MAX_COMMENT_COUNT: u32 = 10_000;
        if comment_count > MAX_COMMENT_COUNT {
            return Err(AudexError::InvalidData(format!(
                "Vorbis comment count {} exceeds maximum {}",
                comment_count, MAX_COMMENT_COUNT
            )));
        }

        // Read each comment, tracking cumulative bytes to prevent
        // memory exhaustion from many large-but-individually-valid comments
        let mut unknown_counter: u32 = 0;
        cumulative_bytes = cumulative_bytes.saturating_add(4);
        limits.check_tag_size(cumulative_bytes, "Vorbis comment data")?;

        for _ in 0..comment_count {
            let comment_length = reader.read_u32::<LittleEndian>().map_err(|e| {
                AudexError::InvalidData(format!("failed to read comment length: {}", e))
            })?;

            // Validate individual comment length to prevent OOM attacks
            // Maximum 10 MB per comment (generous for embedded album art in lyrics, etc.)
            const MAX_COMMENT_LENGTH: u32 = 10_000_000;
            if comment_length > MAX_COMMENT_LENGTH {
                return Err(AudexError::InvalidData(format!(
                    "Vorbis comment length {} exceeds maximum {}",
                    comment_length, MAX_COMMENT_LENGTH
                )));
            }

            // Enforce cumulative budget across all comments
            cumulative_bytes = cumulative_bytes
                .saturating_add(4)
                .saturating_add(comment_length as u64);
            limits.check_tag_size(cumulative_bytes, "Vorbis comment data")?;

            let mut comment_bytes = vec![0u8; comment_length as usize];
            reader.read_exact(&mut comment_bytes).map_err(|e| {
                AudexError::InvalidData(format!("failed to read comment data: {}", e))
            })?;

            let comment_string = Self::decode_string(comment_bytes, errors)?;

            if let Some((key, value)) = comment_string.split_once('=') {
                // Validate with lowercase key per spec, but preserve original case
                let normalized_key = key.to_lowercase();
                if is_valid_key(&normalized_key) {
                    trace_event!(key = %normalized_key, "Vorbis Comment entry parsed");
                    let value_string = value.to_string();
                    // Store original case key in data for round-trip preservation
                    self.data.push((key.to_string(), value_string.clone()));
                    // Use lowercase key for tags HashMap (case-insensitive lookups)
                    self.tags
                        .entry(normalized_key)
                        .or_default()
                        .push(value_string);
                } else if errors == ErrorMode::Strict {
                    return Err(AudexError::InvalidData(format!(
                        "invalid vorbis key: {}",
                        key
                    )));
                } else if errors == ErrorMode::Replace {
                    let sanitize = |s: &str| -> String {
                        s.chars()
                            .map(|c| {
                                let code = c as u32;
                                if (0x20..=0x7D).contains(&code) && c != '=' {
                                    c
                                } else {
                                    '?'
                                }
                            })
                            .collect()
                    };
                    let sanitized_key = sanitize(key);
                    let sanitized_normalized = sanitize(&normalized_key);
                    let value_string = value.to_string();
                    self.data.push((sanitized_key, value_string.clone()));
                    self.tags
                        .entry(sanitized_normalized)
                        .or_default()
                        .push(value_string);
                }
                // In ignore mode, we just skip invalid keys
            } else if errors == ErrorMode::Strict {
                return Err(AudexError::InvalidData(format!(
                    "comment missing '=': {}",
                    comment_string
                )));
            } else if errors == ErrorMode::Replace {
                // Preserve malformed comments with auto-generated keys
                let key = format!("unknown{}", unknown_counter);
                unknown_counter += 1;
                self.data.push((key.clone(), comment_string.to_string()));
                self.tags
                    .entry(key)
                    .or_default()
                    .push(comment_string.to_string());
            }
            // In ignore mode, drop the entry silently
        }

        // Check framing bit if required
        if framing {
            let mut framing_byte = [0u8];
            match reader.read_exact(&mut framing_byte) {
                Ok(_) => {
                    if framing_byte[0] & 1 == 0 {
                        return Err(AudexError::FormatError(Box::new(
                            VorbisError::UnsetFrameError,
                        )));
                    }
                }
                Err(_) if errors != ErrorMode::Strict => {
                    // In non-strict mode, missing framing bit is ok
                }
                Err(_) => {
                    // In strict mode, missing framing bit is an UnsetFrameError
                    return Err(AudexError::FormatError(Box::new(
                        VorbisError::UnsetFrameError,
                    )));
                }
            }
        }

        Ok(())
    }

    /// Write vorbis comments to a writer
    pub fn write<W: Write>(&self, writer: &mut W, framing: Option<bool>) -> Result<()> {
        debug_event!("saving Vorbis Comments");
        self.validate()?;

        let use_framing = framing.unwrap_or(self.framing);
        trace_event!(
            comment_count = self.data.len(),
            framing = use_framing,
            "writing Vorbis Comment fields"
        );

        // Write vendor string length as u32 (Vorbis format constraint)
        let vendor_bytes = self.vendor.as_bytes();
        let vendor_len = u32::try_from(vendor_bytes.len()).map_err(|_| {
            AudexError::InvalidData(
                "vendor string too large for Vorbis comment format (exceeds u32 max)".into(),
            )
        })?;
        writer.write_u32::<LittleEndian>(vendor_len).map_err(|e| {
            AudexError::InvalidData(format!("failed to write vendor length: {}", e))
        })?;
        writer.write_all(vendor_bytes).map_err(|e| {
            AudexError::InvalidData(format!("failed to write vendor string: {}", e))
        })?;

        // Write comment count as u32 (Vorbis format constraint)
        let comment_count = u32::try_from(self.data.len()).map_err(|_| {
            AudexError::InvalidData(
                "too many comments for Vorbis comment format (exceeds u32 max)".into(),
            )
        })?;
        writer
            .write_u32::<LittleEndian>(comment_count)
            .map_err(|e| {
                AudexError::InvalidData(format!("failed to write comment count: {}", e))
            })?;

        // Write each comment
        for (key, value) in &self.data {
            let comment = format!("{}={}", key, value);
            let comment_bytes = comment.as_bytes();
            let comment_len = u32::try_from(comment_bytes.len()).map_err(|_| {
                AudexError::InvalidData(
                    "comment too large for Vorbis comment format (exceeds u32 max)".into(),
                )
            })?;
            writer.write_u32::<LittleEndian>(comment_len).map_err(|e| {
                AudexError::InvalidData(format!("failed to write comment length: {}", e))
            })?;
            writer.write_all(comment_bytes).map_err(|e| {
                AudexError::InvalidData(format!("failed to write comment data: {}", e))
            })?;
        }

        // Write framing bit if required
        if use_framing {
            writer.write_u8(1).map_err(|e| {
                AudexError::InvalidData(format!("failed to write framing bit: {}", e))
            })?;
        }

        Ok(())
    }

    /// Validate all keys in the comment
    pub fn validate(&self) -> Result<()> {
        for (key, _) in &self.data {
            if !is_valid_key(key) {
                return Err(AudexError::FormatError(Box::new(VorbisError::InvalidKey(
                    key.clone(),
                ))));
            }
        }
        Ok(())
    }

    /// Clear all comments but preserve vendor string
    pub fn clear(&mut self) {
        self.data.clear();
        self.tags.clear();
    }

    /// Get the number of comments
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if there are no comments
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Add a new comment.
    ///
    /// Returns an error if the key is invalid (empty, contains `=`, or has
    /// characters outside the 0x20-0x7D range).
    pub fn push(&mut self, key: String, value: String) -> Result<()> {
        let normalized_key = key.to_lowercase();
        if !is_valid_key(&normalized_key) {
            return Err(crate::AudexError::InvalidData(format!(
                "Invalid Vorbis comment key: {:?}",
                key
            )));
        }
        // Store original case key in data for round-trip preservation
        self.data.push((key, value.clone()));
        // Use lowercase key for tags HashMap (case-insensitive lookups)
        self.tags.entry(normalized_key).or_default().push(value);
        Ok(())
    }

    /// Insert a comment at a specific position.
    ///
    /// Returns an error if the key is invalid (empty, contains `=`, or has
    /// characters outside the 0x20-0x7D range).
    pub fn insert(&mut self, index: usize, key: String, value: String) -> Result<()> {
        let normalized_key = key.to_lowercase();
        if !is_valid_key(&normalized_key) {
            return Err(crate::AudexError::InvalidData(format!(
                "Invalid Vorbis comment key: {:?}",
                key
            )));
        }
        // Store original case key in data for round-trip preservation
        self.data.insert(index, (key, value.clone()));
        // Rebuild tags map to maintain consistency
        self.rebuild_tags_map();
        Ok(())
    }

    /// Remove a comment at a specific position
    pub fn remove(&mut self, index: usize) -> Option<(String, String)> {
        if index < self.data.len() {
            let result = Some(self.data.remove(index));
            // Rebuild tags map to maintain consistency
            self.rebuild_tags_map();
            result
        } else {
            None
        }
    }

    /// Get an iterator over the comments
    pub fn iter(&self) -> std::slice::Iter<'_, (String, String)> {
        self.data.iter()
    }

    /// Mutate the comment data directly via a closure.
    ///
    /// The internal lookup cache is rebuilt after the closure returns to
    /// keep it in sync with the data Vec. Use this instead of direct
    /// data mutation to prevent stale lookups from get().
    pub fn modify<F>(&mut self, f: F)
    where
        F: FnOnce(&mut Vec<(String, String)>),
    {
        f(&mut self.data);
        self.rebuild_tags_map();
    }

    /// Helper function to rebuild the tags map from the data Vec
    fn rebuild_tags_map(&mut self) {
        self.tags.clear();
        for (key, value) in &self.data {
            self.tags
                .entry(key.to_lowercase())
                .or_default()
                .push(value.clone());
        }
    }

    /// Helper function to decode bytes to string with error handling
    fn decode_string(bytes: Vec<u8>, errors: ErrorMode) -> Result<String> {
        match errors {
            ErrorMode::Strict => String::from_utf8(bytes).map_err(|e| {
                AudexError::FormatError(Box::new(VorbisError::EncodingError(e.to_string())))
            }),
            ErrorMode::Replace => Ok(String::from_utf8_lossy(&bytes).into_owned()),
            ErrorMode::Ignore => {
                // Use lossy decoding then strip replacement characters so that
                // invalid sequences are silently dropped instead of replaced.
                let lossy = String::from_utf8_lossy(&bytes);
                Ok(lossy.replace('\u{FFFD}', ""))
            }
        }
    }
}

// Implement indexing for VComment
impl<I: SliceIndex<[(String, String)]>> Index<I> for VComment {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        &self.data[index]
    }
}

// IndexMut intentionally not implemented — direct mutation of the data Vec
// through indexing would desynchronize the internal tags HashMap used for
// case-insensitive key lookups. Use set() or modify() instead.

// Implement IntoIterator for VComment
impl IntoIterator for VComment {
    type Item = (String, String);
    type IntoIter = std::vec::IntoIter<(String, String)>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

impl<'a> IntoIterator for &'a VComment {
    type Item = &'a (String, String);
    type IntoIter = std::slice::Iter<'a, (String, String)>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

// IntoIterator for &mut VComment intentionally not implemented —
// mutable iteration would desynchronize the tags HashMap.
// Use modify() for direct data mutations instead.

impl Tags for VComment {
    fn get(&self, key: &str) -> Option<&[String]> {
        // Case-insensitive lookup - normalize to lowercase
        let normalized_key = key.to_lowercase();
        self.tags.get(&normalized_key).map(|v| v.as_slice())
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        let normalized_key = key.to_lowercase();

        if !is_valid_key(&normalized_key) {
            warn_event!(key = %key, "ignored invalid Vorbis comment key in Tags::set");
            return;
        }

        // Remove all existing entries with this key from data (case-insensitive)
        self.data
            .retain(|(k, _)| k.to_lowercase() != normalized_key);

        if values.is_empty() {
            // Remove from tags map as well
            self.tags.remove(&normalized_key);
        } else {
            // Add new entries to data preserving the caller's key case
            for value in &values {
                self.data.push((key.to_string(), value.clone()));
            }
            // Use lowercase key for tags HashMap (case-insensitive lookups)
            self.tags.insert(normalized_key, values);
        }
    }

    fn remove(&mut self, key: &str) {
        let normalized_key = key.to_lowercase();
        // Case-insensitive removal from data
        self.data
            .retain(|(k, _)| k.to_lowercase() != normalized_key);
        self.tags.remove(&normalized_key);
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.tags.keys().cloned().collect();
        keys.sort();
        keys
    }

    fn pprint(&self) -> String {
        let mut result = String::new();
        for (key, value) in &self.data {
            result.push_str(&format!("{}={}\n", key, value));
        }
        result
    }
}

impl Metadata for VComment {
    type Error = AudexError;

    fn new() -> Self {
        VComment::new()
    }

    fn load_from_fileobj(filething: &mut crate::util::AnyFileThing) -> Result<Self> {
        let mut comment = VComment::new();
        comment.load(filething, ErrorMode::Strict, true)?;
        Ok(comment)
    }

    fn save_to_fileobj(&self, filething: &mut crate::util::AnyFileThing) -> Result<()> {
        self.write(filething, None)
    }

    fn delete_from_fileobj(_filething: &mut crate::util::AnyFileThing) -> Result<()> {
        // For vorbis comments, deletion typically means clearing the comments
        // but keeping the structure. This is format-specific.
        Err(AudexError::NotImplementedMethod(
            "delete_from_fileobj not implemented for VComment".to_string(),
        ))
    }
}

/// Key-value interface for Vorbis Comments.
///
/// This struct provides a `HashMap`-like interface for working with Vorbis Comment
/// metadata. It handles the case-insensitive nature of Vorbis Comment keys and
/// supports multiple values per key as specified in the Vorbis Comment standard.
///
/// # Key Normalization
///
/// Vorbis Comment field names are case-insensitive. This implementation normalizes
/// all keys to lowercase for storage and lookup:
/// - "TITLE", "Title", and "title" all access the same field
/// - Keys are stored internally as lowercase
/// - Lookups are case-insensitive
///
/// # Multiple Values
///
/// Vorbis Comments allow multiple values for the same field. For example, a track
/// with multiple artists can have multiple "ARTIST" values. Methods return and
/// accept `Vec<String>` to support this:
///
/// ```
/// use audex::vorbis::VCommentDict;
/// use audex::Tags;
///
/// let mut vc = VCommentDict::new();
///
/// // Set multiple artists
/// vc.set("ARTIST", vec![
///     "First Artist".to_string(),
///     "Second Artist".to_string(),
/// ]);
///
/// // All values are preserved
/// assert_eq!(vc.get("artist").unwrap().len(), 2);
/// ```
///
/// # Examples
///
/// ## Basic Usage
///
/// ```
/// use audex::vorbis::VCommentDict;
/// use audex::Tags;
///
/// let mut vc = VCommentDict::new();
///
/// // Set fields
/// vc.set("TITLE", vec!["My Song".to_string()]);
/// vc.set("ARTIST", vec!["Artist Name".to_string()]);
/// vc.set("DATE", vec!["2024".to_string()]);
///
/// // Read fields (case-insensitive)
/// let title_values: &[String] = &["My Song".to_string()];
/// assert_eq!(vc.get("title"), Some(title_values));
/// let artist_values: &[String] = &["Artist Name".to_string()];
/// assert_eq!(vc.get("ARTIST"), Some(artist_values));
///
/// // Check existence
/// assert!(vc.contains_key("DATE"));
/// assert!(!vc.contains_key("GENRE"));
/// ```
///
/// ## Working with Multi-Value Fields
///
/// ```
/// use audex::vorbis::VCommentDict;
/// use audex::Tags;
///
/// let mut vc = VCommentDict::new();
///
/// // Multiple performers
/// vc.set("PERFORMER", vec![
///     "Vocalist".to_string(),
///     "Guitarist".to_string(),
///     "Drummer".to_string(),
/// ]);
///
/// // Iterate over values
/// if let Some(performers) = vc.get("PERFORMER") {
///     for performer in performers {
///         println!("Performer: {}", performer);
///     }
/// }
/// ```
///
/// ## Removing Fields
///
/// ```
/// use audex::vorbis::VCommentDict;
/// use audex::Tags;
///
/// let mut vc = VCommentDict::new();
/// vc.set("TITLE", vec!["Title".to_string()]);
/// vc.set("ARTIST", vec!["Artist".to_string()]);
///
/// // Remove a field
/// vc.remove("TITLE");
/// assert!(!vc.contains_key("TITLE"));
///
/// // Clear all fields
/// vc.clear();
/// assert_eq!(vc.keys().len(), 0);
/// ```
///
/// # See Also
///
/// - [`ErrorMode`] - Error handling for invalid data
/// - [`is_valid_key`] - Key validation function
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VCommentDict {
    /// Internal Vorbis Comment storage
    inner: VComment,
}

impl VCommentDict {
    /// Create a new VCommentDict
    pub fn new() -> Self {
        Self {
            inner: VComment::new(),
        }
    }

    /// Create a new VCommentDict with a specific framing setting
    pub fn with_framing(framing: bool) -> Self {
        let mut inner = VComment::new();
        inner.framing = framing;
        Self { inner }
    }

    /// Create a new VCommentDict with a specific vendor string
    pub fn with_vendor(vendor: String) -> Self {
        Self {
            inner: VComment::with_vendor(vendor),
        }
    }

    /// Load from a reader
    pub fn load<R: Read>(
        &mut self,
        reader: &mut R,
        errors: ErrorMode,
        framing: bool,
    ) -> Result<()> {
        self.inner.load(reader, errors, framing)
    }

    /// Write to a writer
    pub fn write<W: Write>(&self, writer: &mut W, framing: Option<bool>) -> Result<()> {
        self.inner.write(writer, framing)
    }

    /// Get the vendor string
    pub fn vendor(&self) -> &str {
        &self.inner.vendor
    }

    /// Set the vendor string
    pub fn set_vendor(&mut self, vendor: String) {
        self.inner.vendor = vendor;
    }

    /// Get values for a key (case-insensitive)
    pub fn get_values(&self, key: &str) -> Vec<String> {
        let normalized_key = key.to_lowercase();
        self.inner
            .data
            .iter()
            .filter_map(|(k, v)| {
                if k.to_lowercase() == normalized_key {
                    Some(v.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Set values for a key (case-insensitive)
    pub fn set(&mut self, key: &str, values: Vec<String>) {
        self.inner.set(key, values);
    }

    /// Get the first value for a key
    pub fn get_first(&self, key: &str) -> Option<String> {
        self.get_values(key).into_iter().next()
    }

    /// Set a single value for a key
    pub fn set_single(&mut self, key: &str, value: String) {
        self.inner.set(key, vec![value]);
    }

    /// Check if a key exists (case-insensitive)
    pub fn contains_key(&self, key: &str) -> bool {
        let normalized_key = key.to_lowercase();
        self.inner
            .data
            .iter()
            .any(|(k, _)| k.to_lowercase() == normalized_key)
    }

    /// Remove a key (case-insensitive)
    pub fn remove_key(&mut self, key: &str) {
        let normalized_key = key.to_lowercase();
        // Case-insensitive removal from data
        self.inner
            .data
            .retain(|(k, _)| k.to_lowercase() != normalized_key);
        self.inner.tags.remove(&normalized_key);
    }

    /// Get all keys
    pub fn keys(&self) -> Vec<String> {
        self.inner.keys()
    }

    /// Convert to a regular HashMap
    pub fn as_dict(&self) -> HashMap<String, Vec<String>> {
        let mut dict = HashMap::new();
        for key in self.keys() {
            dict.insert(key.clone(), self.get_values(&key));
        }
        dict
    }

    /// Clear all comments
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Get the underlying VComment
    pub fn inner(&self) -> &VComment {
        &self.inner
    }

    /// Get mutable access to the underlying VComment
    pub fn inner_mut(&mut self) -> &mut VComment {
        &mut self.inner
    }

    /// Add a picture to the Vorbis comments as a base64-encoded FLAC Picture block
    ///
    /// This stores the picture under the "metadata_block_picture" key,
    /// which is the standard way to embed cover art in Ogg formats (Vorbis, Opus, FLAC in Ogg).
    ///
    /// # Arguments
    /// * `picture` - The Picture object to add
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err` if serialization or encoding fails
    ///
    /// # Example
    /// ```rust
    /// use audex::vorbis::VCommentDict;
    /// use audex::flac::Picture;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut tags = VCommentDict::new();
    /// let mut picture = Picture::new();
    /// picture.data = vec![0xFF, 0xD8, 0xFF]; // Minimal JPEG header
    /// picture.mime_type = "image/jpeg".to_string();
    /// picture.picture_type = 3; // Front cover
    /// tags.add_picture(picture)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_picture(&mut self, picture: Picture) -> Result<()> {
        // Serialize the Picture to bytes
        let picture_data = picture.to_bytes()?;

        // Encode to base64
        let encoded = BASE64.encode(&picture_data);

        // Get existing pictures
        let mut pictures = self.get_values("metadata_block_picture");
        pictures.push(encoded);

        // Store back
        self.set("metadata_block_picture", pictures);

        Ok(())
    }

    /// Get all pictures from the Vorbis comments
    ///
    /// This retrieves and decodes all base64-encoded FLAC Picture blocks
    /// stored under the "metadata_block_picture" key.
    ///
    /// # Returns
    /// A vector of successfully decoded Pictures. Invalid entries are skipped.
    ///
    /// # Example
    /// ```rust
    /// use audex::vorbis::VCommentDict;
    /// use audex::flac::Picture;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut tags = VCommentDict::new();
    /// let mut picture = Picture::new();
    /// picture.data = vec![0xFF, 0xD8, 0xFF]; // Minimal JPEG header
    /// picture.mime_type = "image/jpeg".to_string();
    /// picture.description = "Front cover".to_string();
    /// tags.add_picture(picture)?;
    ///
    /// let pictures = tags.get_pictures();
    /// for picture in pictures {
    ///     println!("Found picture: {} ({})", picture.description, picture.mime_type);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_pictures(&self) -> Vec<Picture> {
        let mut result = Vec::new();

        for b64_data in self.get_values("metadata_block_picture") {
            // Decode from base64
            let data = match BASE64.decode(b64_data.as_bytes()) {
                Ok(d) => d,
                Err(_) => continue, // Skip invalid base64
            };

            // Parse as Picture
            match Picture::from_bytes(&data) {
                Ok(picture) => result.push(picture),
                Err(_) => continue, // Skip invalid Picture data
            }
        }

        result
    }

    /// Clear all pictures from the Vorbis comments
    ///
    /// This removes all "metadata_block_picture" entries from the tags.
    ///
    /// # Example
    /// ```rust
    /// use audex::vorbis::VCommentDict;
    /// use audex::flac::Picture;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut tags = VCommentDict::new();
    /// let mut picture = Picture::new();
    /// picture.data = vec![0xFF, 0xD8, 0xFF];
    /// picture.mime_type = "image/jpeg".to_string();
    /// tags.add_picture(picture)?;
    /// assert_eq!(tags.get_pictures().len(), 1);
    /// tags.clear_pictures();
    /// assert_eq!(tags.get_pictures().len(), 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn clear_pictures(&mut self) {
        self.remove_key("metadata_block_picture");
    }
}

impl Tags for VCommentDict {
    fn get(&self, key: &str) -> Option<&[String]> {
        self.inner.get(key)
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        self.inner.set(key, values);
    }

    fn remove(&mut self, key: &str) {
        self.remove_key(key);
    }

    fn keys(&self) -> Vec<String> {
        self.inner.keys()
    }

    fn pprint(&self) -> String {
        self.inner.pprint()
    }
}

impl Metadata for VCommentDict {
    type Error = AudexError;

    fn new() -> Self {
        VCommentDict::new()
    }

    fn load_from_fileobj(filething: &mut crate::util::AnyFileThing) -> Result<Self> {
        let mut dict = VCommentDict::new();
        dict.load(filething, ErrorMode::Strict, true)?;
        Ok(dict)
    }

    fn save_to_fileobj(&self, filething: &mut crate::util::AnyFileThing) -> Result<()> {
        self.write(filething, None)
    }

    fn delete_from_fileobj(_filething: &mut crate::util::AnyFileThing) -> Result<()> {
        Err(AudexError::NotImplementedMethod(
            "delete_from_fileobj not implemented for VCommentDict".to_string(),
        ))
    }
}

/// Convenience functions for loading and parsing
impl VComment {
    /// Load from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        let mut comment = VComment::new();
        comment.load(&mut cursor, ErrorMode::Strict, true)?;
        Ok(comment)
    }

    /// Load from bytes with custom settings
    pub fn from_bytes_with_options(data: &[u8], errors: ErrorMode, framing: bool) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        let mut comment = VComment::new();
        comment.load(&mut cursor, errors, framing)?;
        Ok(comment)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        self.write(&mut buffer, None)?;
        Ok(buffer)
    }

    /// Convert to bytes with custom framing
    pub fn to_bytes_with_framing(&self, framing: bool) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        self.write(&mut buffer, Some(framing))?;
        Ok(buffer)
    }
}

impl VCommentDict {
    /// Load from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        let mut dict = VCommentDict::new();
        dict.load(&mut cursor, ErrorMode::Strict, true)?;
        Ok(dict)
    }

    /// Load from bytes with custom settings
    pub fn from_bytes_with_options(data: &[u8], errors: ErrorMode, framing: bool) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        let mut dict = VCommentDict::new();
        dict.load(&mut cursor, errors, framing)?;
        Ok(dict)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        self.write(&mut buffer, None)?;
        Ok(buffer)
    }

    /// Get ReplayGain information from Vorbis Comments
    ///
    /// Extracts REPLAYGAIN_* tags and returns a ReplayGainInfo struct containing
    /// track and album normalization values. Returns an empty ReplayGainInfo if
    /// no ReplayGain tags are present.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::vorbis::VCommentDict;
    /// use audex::Tags;
    ///
    /// let mut vc = VCommentDict::new();
    /// vc.set("REPLAYGAIN_TRACK_GAIN", vec!["-6.20 dB".to_string()]);
    /// vc.set("REPLAYGAIN_TRACK_PEAK", vec!["0.950866".to_string()]);
    ///
    /// let rg = vc.get_replaygain();
    /// if let Some(track_gain) = rg.track_gain() {
    ///     println!("Track gain: {} dB", track_gain);
    /// }
    /// ```
    pub fn get_replaygain(&self) -> crate::replaygain::ReplayGainInfo {
        use crate::replaygain;

        // Convert to HashMap for the replaygain module
        let mut map = std::collections::HashMap::new();
        for key in self.keys() {
            if let Some(values) = self.get(&key) {
                map.insert(key, values.to_vec());
            }
        }

        replaygain::from_vorbis_comments(&map)
    }

    /// Set ReplayGain information in Vorbis Comments
    ///
    /// Updates REPLAYGAIN_* tags with values from a ReplayGainInfo struct.
    /// Only non-None values will be written. Existing ReplayGain tags will
    /// be updated or added as needed.
    ///
    /// Returns an error if any gain/peak value is not finite (NaN or Infinity).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::vorbis::VCommentDict;
    /// use audex::replaygain::ReplayGainInfo;
    ///
    /// let mut vc = VCommentDict::new();
    /// let rg = ReplayGainInfo::with_track(-6.20, 0.950866).unwrap();
    ///
    /// vc.set_replaygain(&rg).unwrap();
    /// // Now REPLAYGAIN_TRACK_GAIN and REPLAYGAIN_TRACK_PEAK are set
    /// ```
    pub fn set_replaygain(
        &mut self,
        info: &crate::replaygain::ReplayGainInfo,
    ) -> crate::Result<()> {
        use crate::replaygain;

        // Convert to HashMap
        let mut map = std::collections::HashMap::new();
        for key in self.keys() {
            if let Some(values) = self.get(&key) {
                map.insert(key, values.to_vec());
            }
        }

        // Update with ReplayGain values
        replaygain::to_vorbis_comments(info, &mut map)?;

        // Write back to VCommentDict
        for (key, values) in map {
            self.set(&key, values);
        }

        Ok(())
    }

    /// Remove all ReplayGain information from Vorbis Comments
    ///
    /// Removes all standard ReplayGain tags (track gain, track peak, album gain,
    /// album peak, and reference loudness) from the Vorbis Comments.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::vorbis::VCommentDict;
    ///
    /// let mut vc = VCommentDict::new();
    /// // ... set some ReplayGain values ...
    ///
    /// // Remove all ReplayGain data
    /// vc.clear_replaygain();
    /// ```
    pub fn clear_replaygain(&mut self) {
        use crate::replaygain::vorbis_keys;

        // Remove all ReplayGain keys (case-insensitive)
        self.remove(vorbis_keys::TRACK_GAIN);
        self.remove(vorbis_keys::TRACK_PEAK);
        self.remove(vorbis_keys::ALBUM_GAIN);
        self.remove(vorbis_keys::ALBUM_PEAK);
        self.remove(vorbis_keys::REFERENCE_LOUDNESS);
    }
}

#[cfg(feature = "async")]
impl VComment {
    /// Load vorbis comments from an async reader
    ///
    /// Reads and parses Vorbis comments from an async source. This is the async
    /// equivalent of `VComment::load`.
    ///
    /// # Arguments
    ///
    /// * `reader` - Async reader to read from
    /// * `errors` - Error handling mode for invalid data
    /// * `framing` - Whether to expect a framing bit at the end
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if parsing fails
    pub async fn load_async<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
        errors: ErrorMode,
        framing: bool,
    ) -> Result<()> {
        self.framing = framing;
        self.data.clear();
        self.tags.clear();

        let limits = ParseLimits::default();

        // Read vendor string length
        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| AudexError::InvalidData(format!("failed to read vendor length: {}", e)))?;
        let vendor_length = u32::from_le_bytes(len_buf);

        // Validate vendor length to prevent OOM attacks
        const MAX_VENDOR_LENGTH: u32 = 1_000_000;
        if vendor_length > MAX_VENDOR_LENGTH {
            return Err(AudexError::InvalidData(format!(
                "Vorbis vendor length {} exceeds maximum {}",
                vendor_length, MAX_VENDOR_LENGTH
            )));
        }

        let mut cumulative_bytes: u64 = 4;
        cumulative_bytes = cumulative_bytes.saturating_add(vendor_length as u64);
        limits.check_tag_size(cumulative_bytes, "Vorbis comment data")?;

        // Read vendor string
        let mut vendor_bytes = vec![0u8; vendor_length as usize];
        reader
            .read_exact(&mut vendor_bytes)
            .await
            .map_err(|e| AudexError::InvalidData(format!("failed to read vendor string: {}", e)))?;

        self.vendor = Self::decode_string(vendor_bytes, errors)?;

        // Read number of comments
        reader
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| AudexError::InvalidData(format!("failed to read comment count: {}", e)))?;
        let comment_count = u32::from_le_bytes(len_buf);

        // Cap iteration count. See the sync parser for rationale on the 10k limit.
        const MAX_COMMENT_COUNT: u32 = 10_000;
        if comment_count > MAX_COMMENT_COUNT {
            return Err(AudexError::InvalidData(format!(
                "Vorbis comment count {} exceeds maximum {}",
                comment_count, MAX_COMMENT_COUNT
            )));
        }

        // Read each comment
        let mut unknown_counter: u32 = 0;
        cumulative_bytes = cumulative_bytes.saturating_add(4);
        limits.check_tag_size(cumulative_bytes, "Vorbis comment data")?;
        for _ in 0..comment_count {
            reader.read_exact(&mut len_buf).await.map_err(|e| {
                AudexError::InvalidData(format!("failed to read comment length: {}", e))
            })?;
            let comment_length = u32::from_le_bytes(len_buf);

            // Validate individual comment length
            const MAX_COMMENT_LENGTH: u32 = 10_000_000;
            if comment_length > MAX_COMMENT_LENGTH {
                return Err(AudexError::InvalidData(format!(
                    "Vorbis comment length {} exceeds maximum {}",
                    comment_length, MAX_COMMENT_LENGTH
                )));
            }

            cumulative_bytes = cumulative_bytes
                .saturating_add(4)
                .saturating_add(comment_length as u64);
            limits.check_tag_size(cumulative_bytes, "Vorbis comment data")?;

            let mut comment_bytes = vec![0u8; comment_length as usize];
            reader.read_exact(&mut comment_bytes).await.map_err(|e| {
                AudexError::InvalidData(format!("failed to read comment data: {}", e))
            })?;

            let comment_string = Self::decode_string(comment_bytes, errors)?;

            if let Some((key, value)) = comment_string.split_once('=') {
                // Normalize key to lowercase per Vorbis specification
                let normalized_key = key.to_lowercase();
                if is_valid_key(&normalized_key) {
                    let value_string = value.to_string();
                    // Store normalized lowercase key for consistency
                    self.data
                        .push((normalized_key.clone(), value_string.clone()));
                    // Update tags HashMap for efficient lookups
                    self.tags
                        .entry(normalized_key)
                        .or_default()
                        .push(value_string);
                } else if errors == ErrorMode::Strict {
                    return Err(AudexError::InvalidData(format!(
                        "invalid vorbis key: {}",
                        key
                    )));
                } else if errors == ErrorMode::Replace {
                    let sanitize = |s: &str| -> String {
                        s.chars()
                            .map(|c| {
                                let code = c as u32;
                                if (0x20..=0x7D).contains(&code) && c != '=' {
                                    c
                                } else {
                                    '?'
                                }
                            })
                            .collect()
                    };
                    let sanitized_key = sanitize(&normalized_key);
                    let value_string = value.to_string();
                    self.data
                        .push((sanitized_key.clone(), value_string.clone()));
                    self.tags
                        .entry(sanitized_key)
                        .or_default()
                        .push(value_string);
                }
                // In ignore mode, we just skip invalid keys
            } else if errors == ErrorMode::Strict {
                return Err(AudexError::InvalidData(format!(
                    "comment missing '=': {}",
                    comment_string
                )));
            } else if errors == ErrorMode::Replace {
                // Preserve malformed comments with auto-generated keys
                let key = format!("unknown{}", unknown_counter);
                unknown_counter += 1;
                self.data.push((key.clone(), comment_string.to_string()));
                self.tags
                    .entry(key)
                    .or_default()
                    .push(comment_string.to_string());
            }
            // In ignore mode, drop the entry silently
        }

        // Check framing bit if required
        if framing {
            let mut framing_byte = [0u8; 1];
            match reader.read_exact(&mut framing_byte).await {
                Ok(_) => {
                    if framing_byte[0] & 1 == 0 {
                        return Err(AudexError::FormatError(Box::new(
                            VorbisError::UnsetFrameError,
                        )));
                    }
                }
                Err(_) if errors != ErrorMode::Strict => {
                    // In non-strict mode, missing framing bit is acceptable
                }
                Err(_) => {
                    return Err(AudexError::FormatError(Box::new(
                        VorbisError::UnsetFrameError,
                    )));
                }
            }
        }

        Ok(())
    }

    /// Write vorbis comments to an async writer
    ///
    /// Serializes and writes Vorbis comments to an async destination.
    /// This is the async equivalent of `VComment::write`.
    ///
    /// # Arguments
    ///
    /// * `writer` - Async writer to write to
    /// * `framing` - Whether to write a framing bit (uses stored value if None)
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if writing fails
    pub async fn write_async<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        framing: Option<bool>,
    ) -> Result<()> {
        self.validate()?;

        let use_framing = framing.unwrap_or(self.framing);

        // Write vendor string
        let vendor_bytes = self.vendor.as_bytes();
        writer
            .write_all(&(vendor_bytes.len() as u32).to_le_bytes())
            .await
            .map_err(|e| {
                AudexError::InvalidData(format!("failed to write vendor length: {}", e))
            })?;
        writer.write_all(vendor_bytes).await.map_err(|e| {
            AudexError::InvalidData(format!("failed to write vendor string: {}", e))
        })?;

        // Write comment count
        writer
            .write_all(&(self.data.len() as u32).to_le_bytes())
            .await
            .map_err(|e| {
                AudexError::InvalidData(format!("failed to write comment count: {}", e))
            })?;

        // Write each comment in key=value format
        for (key, value) in &self.data {
            let comment = format!("{}={}", key, value);
            let comment_bytes = comment.as_bytes();
            writer
                .write_all(&(comment_bytes.len() as u32).to_le_bytes())
                .await
                .map_err(|e| {
                    AudexError::InvalidData(format!("failed to write comment length: {}", e))
                })?;
            writer.write_all(comment_bytes).await.map_err(|e| {
                AudexError::InvalidData(format!("failed to write comment data: {}", e))
            })?;
        }

        // Write framing bit if required
        if use_framing {
            writer.write_all(&[1u8]).await.map_err(|e| {
                AudexError::InvalidData(format!("failed to write framing bit: {}", e))
            })?;
        }

        Ok(())
    }
}

#[cfg(feature = "async")]
impl VCommentDict {
    /// Load from an async reader
    ///
    /// This is the async equivalent of `VCommentDict::load`.
    pub async fn load_async<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
        errors: ErrorMode,
        framing: bool,
    ) -> Result<()> {
        self.inner.load_async(reader, errors, framing).await
    }

    /// Write to an async writer
    ///
    /// This is the async equivalent of `VCommentDict::write`.
    pub async fn write_async<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        framing: Option<bool>,
    ) -> Result<()> {
        self.inner.write_async(writer, framing).await
    }

    /// Load from bytes asynchronously
    ///
    /// Convenience method to create a VCommentDict from byte data.
    pub async fn from_bytes_async(data: &[u8]) -> Result<Self> {
        let mut reader = data;
        let mut dict = VCommentDict::new();
        dict.load_async(&mut reader, ErrorMode::Strict, true)
            .await?;
        Ok(dict)
    }

    /// Load from bytes with custom settings asynchronously
    ///
    /// Convenience method to create a VCommentDict from byte data with custom options.
    pub async fn from_bytes_with_options_async(
        data: &[u8],
        errors: ErrorMode,
        framing: bool,
    ) -> Result<Self> {
        let mut reader = data;
        let mut dict = VCommentDict::new();
        dict.load_async(&mut reader, errors, framing).await?;
        Ok(dict)
    }
}

/// Async OggVCommentDict for Ogg Vorbis files
///
/// This struct provides async operations for reading and writing Vorbis comments
/// in Ogg container files. It wraps a VCommentDict and adds Ogg-specific functionality.
#[cfg(feature = "async")]
#[derive(Debug, Clone)]
pub struct OggVCommentDictAsync {
    inner: VCommentDict,
}

#[cfg(feature = "async")]
impl Default for OggVCommentDictAsync {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "async")]
impl OggVCommentDictAsync {
    /// Create new empty Vorbis comments for Ogg files
    pub fn new() -> Self {
        Self {
            inner: VCommentDict::new(),
        }
    }

    /// Create from async file object and stream serial number
    ///
    /// Reads Vorbis comments from an Ogg stream identified by its serial number.
    ///
    /// # Arguments
    ///
    /// * `fileobj` - Async file reader
    /// * `serial` - Stream serial number to read comments from
    ///
    /// # Returns
    ///
    /// A new OggVCommentDictAsync with the parsed comments
    pub async fn from_fileobj_async<R: AsyncRead + tokio::io::AsyncSeek + Unpin>(
        fileobj: &mut R,
        serial: u32,
    ) -> Result<Self> {
        use crate::ogg::OggPage;

        let mut pages = Vec::new();
        let mut complete = false;

        // Read pages until we have complete Vorbis comment packet
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read = 0usize;
        while !complete {
            let page = OggPage::from_reader_async(fileobj).await?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for comment packet".to_string(),
                ));
            }
            if page.serial == serial {
                pages.push(page.clone());
                complete = page.is_complete() || page.packets.len() > 1;
            }
        }

        // Extract packets from pages
        let packets = OggPage::to_packets(&pages, false)?;
        if packets.is_empty() || packets[0].len() < 7 {
            return Err(AudexError::InvalidData(
                "Invalid Vorbis comment packet".to_string(),
            ));
        }

        // Strip off "\x03vorbis" header to get raw comment data
        let data = &packets[0][7..];

        // Parse with Replace mode to handle any encoding issues gracefully
        let inner = VCommentDict::from_bytes_with_options(data, ErrorMode::Replace, true)?;

        Ok(Self { inner })
    }

    /// Inject tags into the file asynchronously
    ///
    /// Writes the current Vorbis comments back to an Ogg file, replacing the
    /// existing comment pages.
    ///
    /// # Arguments
    ///
    /// * `fileobj` - Async file handle with read/write access
    /// * `padding_func` - Optional function to calculate padding size
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an error if injection fails
    pub async fn inject_async(
        &self,
        fileobj: &mut tokio::fs::File,
        padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()> {
        use crate::ogg::OggPage;

        // Find the old Vorbis comment pages in the file
        fileobj.seek(std::io::SeekFrom::Start(0)).await?;
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;

        // Find the page containing the Vorbis comment header
        let mut page = OggPage::from_reader_async(fileobj).await?;
        pages_read += 1;
        while !page
            .packets
            .first()
            .is_some_and(|p| p.starts_with(b"\x03vorbis"))
        {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Vorbis comment header not found within page limit".to_string(),
                ));
            }
            page = OggPage::from_reader_async(fileobj).await?;
        }

        let mut old_pages = vec![page];

        // Collect all pages belonging to the comment packet.
        // Defensive: verify non-empty before accessing .last()
        while {
            let last = old_pages.last().ok_or_else(|| {
                AudexError::InvalidData("No Ogg pages collected for comment packet".to_string())
            })?;
            !(last.is_complete() || last.packets.len() > 1)
        } {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many pages while collecting comment packet".to_string(),
                ));
            }
            let page = OggPage::from_reader_async(fileobj).await?;
            if page.serial == old_pages[0].serial {
                old_pages.push(page);
            }
        }

        let packets = OggPage::to_packets(&old_pages, false)?;
        if packets.is_empty() {
            return Err(AudexError::InvalidData("No packets found".to_string()));
        }

        // Calculate content size for padding calculation
        let content_size = {
            let old_pos = fileobj.stream_position().await?;
            let file_size = fileobj.seek(std::io::SeekFrom::End(0)).await?;
            fileobj.seek(std::io::SeekFrom::Start(old_pos)).await?;
            file_size as i64 - packets[0].len() as i64
        };

        // Create new Vorbis comment data
        let vcomment_data = {
            let mut data = b"\x03vorbis".to_vec();
            let mut vcomment_bytes = Vec::new();

            let mut comment_to_write = self.inner.clone();
            // Only set Audex vendor string when there are actual tags to write
            if !comment_to_write.keys().is_empty() {
                comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
            }
            comment_to_write.write(&mut vcomment_bytes, Some(true))?;
            data.extend_from_slice(&vcomment_bytes);
            data
        };
        let padding_left = packets[0].len() as i64 - vcomment_data.len() as i64;

        // Calculate optimal padding
        let info = crate::tags::PaddingInfo::new(padding_left, content_size);
        let new_padding = info.get_padding_with(padding_func);

        // Build new packet with padding
        let mut new_packets = packets;
        new_packets[0] = vcomment_data;
        if new_padding > 0 {
            new_packets[0].extend_from_slice(&vec![0u8; usize::try_from(new_padding).unwrap_or(0)]);
        }

        // Create new Ogg pages, preserving layout if possible
        let new_pages = OggPage::from_packets_try_preserve(new_packets.clone(), &old_pages);

        let final_pages = if new_pages.is_empty() {
            if old_pages.is_empty() {
                return Err(AudexError::InvalidData(
                    "No Ogg pages found for Vorbis comment stream".to_string(),
                ));
            }
            let first_sequence = old_pages[0].sequence;
            let original_granule = old_pages
                .last()
                .expect("old_pages confirmed non-empty")
                .position as u64;
            OggPage::from_packets_with_options(
                new_packets,
                first_sequence,
                4096,
                2048,
                original_granule,
            )?
        } else {
            new_pages
        };

        if final_pages.is_empty() {
            return Err(AudexError::InvalidData(
                "Failed to create new OGG pages".to_string(),
            ));
        }

        // Replace old pages with new ones
        OggPage::replace_async(fileobj, &old_pages, final_pages).await?;

        Ok(())
    }
}

// Delegate VCommentDict methods to inner through Deref
#[cfg(feature = "async")]
impl std::ops::Deref for OggVCommentDictAsync {
    type Target = VCommentDict;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(feature = "async")]
impl std::ops::DerefMut for OggVCommentDictAsync {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

// Implement Tags trait for OggVCommentDictAsync
#[cfg(feature = "async")]
impl Tags for OggVCommentDictAsync {
    fn get(&self, key: &str) -> Option<&[String]> {
        self.inner.get(key)
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        self.inner.set(key, values)
    }

    fn remove(&mut self, key: &str) {
        self.inner.remove(key)
    }

    fn keys(&self) -> Vec<String> {
        self.inner.keys()
    }

    fn pprint(&self) -> String {
        self.inner.pprint()
    }
}
