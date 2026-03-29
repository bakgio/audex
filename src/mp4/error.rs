//! MP4-specific error types for file parsing and metadata handling.
//!
//! This module defines error types specific to MP4/M4A file operations, including
//! atom parsing errors, metadata validation errors, and stream information errors.

use crate::AudexError;
use std::fmt;

/// General error type for MP4 file operations.
///
/// This is the base error type for MP4-related operations. More specific error
/// types like [`MP4MetadataError`] and [`MP4StreamInfoError`] automatically
/// convert to this type for unified error handling.
///
/// # Examples
///
/// ```
/// use audex::mp4::MP4Error;
///
/// let error = MP4Error::new("Invalid atom structure");
/// println!("Error: {}", error);
/// ```
#[derive(Debug, Clone)]
pub struct MP4Error {
    message: String,
}

impl MP4Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MP4Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for MP4Error {}

impl From<MP4Error> for AudexError {
    fn from(err: MP4Error) -> Self {
        AudexError::ParseError(err.message)
    }
}

/// Error specific to MP4 metadata operations.
///
/// This error occurs when there are issues with iTunes-style metadata atoms
/// in the `ilst` (item list) structure, such as invalid atom types, malformed
/// data values, or missing required metadata fields.
///
/// # Common Causes
///
/// - Invalid or corrupted metadata atom structure
/// - Unsupported metadata value types
/// - Missing required `meta` or `ilst` atoms
/// - Malformed iTunes-style tag atoms
///
/// # Examples
///
/// ```
/// use audex::mp4::MP4MetadataError;
///
/// let error = MP4MetadataError::new("Missing ilst atom");
/// ```
#[derive(Debug, Clone)]
pub struct MP4MetadataError {
    message: String,
}

impl MP4MetadataError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MP4MetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MP4 Metadata Error: {}", self.message)
    }
}

impl std::error::Error for MP4MetadataError {}

impl From<MP4MetadataError> for AudexError {
    fn from(err: MP4MetadataError) -> Self {
        AudexError::ParseError(err.message)
    }
}

impl From<MP4MetadataError> for MP4Error {
    fn from(err: MP4MetadataError) -> Self {
        MP4Error::new(err.message)
    }
}

/// Error specific to MP4 audio stream information parsing.
///
/// This error occurs when parsing the audio track information from `trak`
/// (track) and `mdia` (media) atoms. It indicates problems with extracting
/// codec information, sample rates, bitrates, or other stream properties.
///
/// # Common Causes
///
/// - Missing or corrupted `trak` atom
/// - Invalid `stsd` (sample description) atom
/// - Unsupported audio codec
/// - Missing required media information atoms
/// - Malformed track header data
///
/// # Examples
///
/// ```
/// use audex::mp4::MP4StreamInfoError;
///
/// let error = MP4StreamInfoError::new("No audio track found");
/// ```
#[derive(Debug, Clone)]
pub struct MP4StreamInfoError {
    message: String,
}

impl MP4StreamInfoError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MP4StreamInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MP4 Stream Info Error: {}", self.message)
    }
}

impl std::error::Error for MP4StreamInfoError {}

impl From<MP4StreamInfoError> for AudexError {
    fn from(err: MP4StreamInfoError) -> Self {
        AudexError::ParseError(err.message)
    }
}

impl From<MP4StreamInfoError> for MP4Error {
    fn from(err: MP4StreamInfoError) -> Self {
        MP4Error::new(err.message)
    }
}

/// Error indicating no audio track was found in the MP4 file.
///
/// This error occurs when an MP4 file is parsed but contains no audio tracks,
/// or when the expected audio track cannot be located. MP4 files can contain
/// video-only content, or the audio track may be malformed.
///
/// # Common Causes
///
/// - File contains only video tracks (no audio)
/// - All tracks are disabled or hidden
/// - Corrupted `trak` atoms
/// - File is not a valid MP4 container
///
/// # Examples
///
/// ```
/// use audex::mp4::MP4NoTrackError;
///
/// let error = MP4NoTrackError::new("No audio tracks in file");
/// ```
#[derive(Debug, Clone)]
pub struct MP4NoTrackError {
    message: String,
}

impl MP4NoTrackError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MP4NoTrackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MP4 No Track Error: {}", self.message)
    }
}

impl std::error::Error for MP4NoTrackError {}

impl From<MP4NoTrackError> for AudexError {
    fn from(err: MP4NoTrackError) -> Self {
        AudexError::ParseError(err.message)
    }
}

impl From<MP4NoTrackError> for MP4StreamInfoError {
    fn from(err: MP4NoTrackError) -> Self {
        MP4StreamInfoError::new(err.message)
    }
}

impl From<MP4NoTrackError> for MP4Error {
    fn from(err: MP4NoTrackError) -> Self {
        MP4Error::new(err.message)
    }
}

/// Error for invalid or unsupported metadata values in iTunes tags.
///
/// This error occurs when attempting to set metadata values that are invalid
/// for the specific iTunes tag atom type. Different atoms support different
/// data types (text, integers, binary data, etc.).
///
/// # Common Causes
///
/// - Wrong data type for atom (e.g., text for a numeric field)
/// - Value exceeds size limits
/// - Invalid data format for specific atom type
/// - Unsupported data type identifier
///
/// # Examples
///
/// ```
/// use audex::mp4::MP4MetadataValueError;
///
/// let error = MP4MetadataValueError::new("Invalid track number format");
/// ```
#[derive(Debug, Clone)]
pub struct MP4MetadataValueError {
    message: String,
}

impl MP4MetadataValueError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for MP4MetadataValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MP4 Metadata Value Error: {}", self.message)
    }
}

impl std::error::Error for MP4MetadataValueError {}

impl From<MP4MetadataValueError> for AudexError {
    fn from(err: MP4MetadataValueError) -> Self {
        AudexError::ParseError(err.message)
    }
}

impl From<MP4MetadataValueError> for MP4MetadataError {
    fn from(err: MP4MetadataValueError) -> Self {
        MP4MetadataError::new(err.message)
    }
}

impl From<MP4MetadataValueError> for MP4Error {
    fn from(err: MP4MetadataValueError) -> Self {
        MP4Error::new(err.message)
    }
}

/// Error for invalid or unsupported keys in the EasyMP4 interface.
///
/// This error occurs when attempting to access or modify metadata using
/// human-readable key names in the EasyMP4 interface with keys that are
/// not recognized or supported.
///
/// The EasyMP4 interface provides simplified access to common iTunes metadata
/// fields using friendly names like "title", "artist", "album". This error
/// indicates the requested key doesn't map to a valid iTunes atom.
///
/// # Common Causes
///
/// - Typo in key name (e.g., "titel" instead of "title")
/// - Attempting to use raw atom names instead of friendly names
/// - Requesting access to unsupported or custom metadata fields
///
/// # Examples
///
/// ```
/// use audex::mp4::EasyMP4KeyError;
///
/// let error = EasyMP4KeyError::new("unknown_field");
/// println!("Invalid key: {}", error.key());
/// ```
///
/// # See Also
///
/// - [`key2name`](crate::mp4::util::key2name) - Convert string key to atom name bytes (Latin-1 encoding)
/// - [`name2key`](crate::mp4::util::name2key) - Convert atom name bytes to string key (Latin-1 decoding)
#[derive(Debug, Clone)]
pub struct EasyMP4KeyError {
    key: String,
    message: String,
}

impl EasyMP4KeyError {
    pub fn new(key: impl Into<String>) -> Self {
        let key = key.into();
        let message = format!("'{}'", key);
        Self { key, message }
    }

    pub fn with_message(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

impl fmt::Display for EasyMP4KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for EasyMP4KeyError {}

impl From<EasyMP4KeyError> for AudexError {
    fn from(err: EasyMP4KeyError) -> Self {
        AudexError::ParseError(err.message)
    }
}

impl From<EasyMP4KeyError> for MP4Error {
    fn from(err: EasyMP4KeyError) -> Self {
        MP4Error::new(err.message)
    }
}
