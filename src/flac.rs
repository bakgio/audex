//! # FLAC Format Support
//!
//! This module provides comprehensive support for reading and writing Free Lossless Audio Codec (FLAC) files.
//!
//! ## Overview
//!
//! FLAC is a lossless audio compression format that provides:
//! - **Lossless compression**: Perfect bit-for-bit reconstruction of original audio
//! - **Fast decoding**: Simple algorithm allows efficient decompression
//! - **Seeking support**: Seek tables enable quick navigation
//! - **Metadata support**: Vorbis Comments, embedded pictures, cue sheets, and more
//! - **Error detection**: MD5 checksums and CRC checks
//!
//! ## Supported Features
//!
//! - Read/write audio properties (sample rate, bit depth, channels, duration)
//! - Read/write Vorbis Comment tags (artist, title, album, etc.)
//! - Embedded artwork via PICTURE blocks
//! - Seek tables for fast seeking
//! - Cue sheets for CD track information
//! - Application-specific blocks
//! - Multiple metadata block types
//! - Robust parsing with error recovery
//!
//! ## Basic Usage
//!
//! ```no_run
//! use audex::flac::FLAC;
//! use audex::{FileType, Tags};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Load a FLAC file
//!     let mut flac = FLAC::load("song.flac")?;
//!
//!     // Access audio information
//!     println!("Sample rate: {} Hz", flac.info.sample_rate);
//!     println!("Bit depth: {} bits", flac.info.bits_per_sample);
//!     println!("Channels: {}", flac.info.channels);
//!     println!("Total samples: {}", flac.info.total_samples);
//!
//!     // Read Vorbis Comment tags using the Tags trait
//!     if let Some(ref tags) = flac.tags {
//!         if let Some(title) = tags.get("TITLE") {
//!             println!("Title: {:?}", title);
//!         }
//!     }
//!
//!     // Modify tags and save using set for Vorbis Comments
//!     if let Some(ref mut tags) = flac.tags {
//!         tags.set("TITLE", vec!["New Title".to_string()]);
//!     }
//!     flac.save()?;
//!     Ok(())
//! }
//! ```
//!
//! ## Metadata Blocks
//!
//! FLAC files contain metadata blocks before the audio data:
//! - **STREAMINFO**: Required block with audio properties (always present)
//! - **VORBIS_COMMENT**: Tags and metadata (artist, title, etc.)
//! - **PICTURE**: Embedded artwork (album art, etc.)
//! - **SEEKTABLE**: Seek points for fast navigation
//! - **CUESHEET**: CD track information
//! - **APPLICATION**: Application-specific data
//! - **PADDING**: Reserved space for future metadata additions
//!
//! ## Error Handling
//!
//! This implementation includes robust error handling:
//! - Validates block sizes against configurable limits
//! - Detects and handles corrupted blocks
//! - Supports partial parsing (continues after recoverable errors)
//! - Provides detailed error context with file positions
//!
//! ## See Also
//!
//! - `FLAC` - Main struct for FLAC file handling
//! - `FLACStreamInfo` - Audio stream information
//! - `VCommentDict` - Vorbis Comment tags
//! - `Picture` - Embedded artwork

/// Convert an arbitrary-length byte slice to an integer using big-endian byte order.
///
/// Reads up to 8 bytes and interprets them as a big-endian unsigned integer.
/// For slices shorter than 8 bytes, the result is zero-extended on the high bits.
/// Inputs longer than 8 bytes are clamped to the last 8 bytes to prevent
/// silent loss of high bits from overflowing the u64 accumulator.
pub fn to_int_be(data: &[u8]) -> u64 {
    // Only the last 8 bytes can fit in a u64; earlier bytes would be
    // shifted out. Clamp to avoid silent truncation.
    let start = data.len().saturating_sub(8);
    data[start..]
        .iter()
        .fold(0u64, |acc, &byte| (acc << 8) | byte as u64)
}

use crate::tags::PaddingInfo;
use crate::util::resize_bytes;
use crate::vorbis::VCommentDict;
use crate::{AudexError, FileType, Result, StreamInfo, VERSION_STRING};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Cursor, ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::Path;

#[cfg(feature = "async")]
use crate::util::resize_bytes_async;
#[cfg(feature = "async")]
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};
#[cfg(feature = "async")]
use tokio::io::BufReader as TokioBufReader;

/// Type alias for a user-supplied function that determines padding size after a save.
///
/// Receives the current [`PaddingInfo`] and returns the desired padding in bytes.
/// A negative return value means "use the library default".
type PaddingFunction = Box<dyn FnOnce(&PaddingInfo) -> i64>;
use std::cmp::min;
use std::time::Duration;

/// Configuration options for FLAC file parsing.
///
/// These options control how the parser handles various edge cases and errors
/// that may occur in real-world FLAC files. The defaults are tuned for maximum
/// compatibility with potentially malformed files.
///
/// # Examples
///
/// ```no_run
/// use audex::flac::{FLAC, FLACParseOptions};
///
/// // Use strict parsing (less forgiving)
/// let options = FLACParseOptions {
///     distrust_size: false,        // Trust block size headers
///     max_block_size: 1024 * 1024, // 1MB limit
///     ignore_errors: false,         // Fail on any error
///     streaming_io: true,
///     ..Default::default()
/// };
///
/// // Load a file with custom options
/// let flac = FLAC::from_file_with_options("audio.flac", options)?;
/// # Ok::<(), audex::AudexError>(())
/// ```
///
/// # Default Behavior
///
/// The default configuration is designed for maximum robustness:
/// - Validates block sizes rather than trusting headers
/// - Allows up to 16MB blocks (handles most real-world files)
/// - Continues parsing after recoverable errors
/// - Uses streaming I/O for memory efficiency
#[derive(Debug, Clone)]
pub struct FLACParseOptions {
    /// Validate block sizes against actual data instead of trusting headers.
    ///
    /// When `true`, the parser reads and validates block data before accepting
    /// the size from the header. This protects against corrupted or malicious
    /// files with incorrect size fields.
    ///
    /// **Default**: `true` (recommended for untrusted files)
    pub distrust_size: bool,

    /// Maximum allowed metadata block size in bytes.
    ///
    /// Blocks exceeding this size are treated as invalid. This prevents
    /// memory exhaustion from corrupted or malicious files claiming extremely
    /// large block sizes.
    ///
    /// **Default**: 16MB (16 * 1024 * 1024 bytes)
    pub max_block_size: u32,

    /// Continue parsing after encountering non-fatal errors.
    ///
    /// When `true`, the parser attempts to recover from errors and continue
    /// loading other metadata blocks. Errors are recorded in `parse_errors`
    /// but don't abort the entire load operation.
    ///
    /// **Default**: `true` (allows partial file access)
    pub ignore_errors: bool,

    /// Use streaming I/O for reading blocks.
    ///
    /// When `true`, uses buffered reading to reduce memory usage and improve
    /// performance for large files. Disabling may be useful for debugging.
    ///
    /// **Default**: `true` (recommended)
    pub streaming_io: bool,

    /// Error handling mode for Vorbis comment UTF-8 decoding.
    ///
    /// Controls how invalid UTF-8 sequences in Vorbis comments are handled:
    /// - `Strict`: Fail with an error on any invalid UTF-8
    /// - `Replace`: Replace invalid sequences with U+FFFD (default)
    /// - `Ignore`: Silently drop invalid sequences
    ///
    /// **Default**: `Replace` (backward-compatible behavior)
    pub vorbis_error_mode: crate::vorbis::ErrorMode,
}

impl Default for FLACParseOptions {
    fn default() -> Self {
        Self {
            distrust_size: true,
            max_block_size: 16 * 1024 * 1024, // 16MB
            ignore_errors: true,
            streaming_io: true,
            vorbis_error_mode: crate::vorbis::ErrorMode::Replace,
        }
    }
}

/// FLAC-specific error with contextual information.
///
/// This error type provides detailed information about what went wrong during
/// FLAC file parsing, including the error type, file position where it occurred,
/// and a human-readable context message.
///
/// # Examples
///
/// ```
/// use audex::flac::FLAC;
/// use audex::FileType;
///
/// match FLAC::load("corrupted.flac") {
///     Ok(flac) => {
///         // Check for recoverable errors during parsing
///         if !flac.parse_errors.is_empty() {
///             println!("Parsed with {} errors", flac.parse_errors.len());
///             for error in &flac.parse_errors {
///                 println!("  {:?} at position {:?}: {}",
///                     error.kind, error.position, error.context);
///             }
///         }
///     }
///     Err(e) => eprintln!("Failed to load: {}", e),
/// }
/// ```
#[derive(Debug, Clone)]
pub struct FLACError {
    /// The specific type of error that occurred
    pub kind: FLACErrorKind,

    /// File byte offset where the error was detected, if known
    pub position: Option<u64>,

    /// Human-readable description providing additional context
    pub context: String,
}

/// Categories of errors that can occur during FLAC parsing.
///
/// These error kinds represent different failure modes, from structural
/// issues (invalid headers) to data corruption (oversized blocks) to
/// partial successes where some blocks failed but others loaded.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FLACErrorKind {
    /// The FLAC file header is invalid or missing.
    ///
    /// This typically means the file is not a FLAC file, or the initial
    /// "fLaC" signature is missing or corrupted.
    InvalidHeader,

    /// A metadata block's size exceeds limits or is invalid.
    ///
    /// This occurs when:
    /// - Block size exceeds `max_block_size` setting
    /// - Block size would cause integer overflow
    /// - Block size doesn't match actual data length
    BlockSizeError,

    /// Multiple VORBIS_COMMENT blocks were found.
    ///
    /// The FLAC specification allows only one VORBIS_COMMENT block per file.
    /// Finding multiple blocks indicates a malformed or non-compliant file.
    MultipleVorbisBlocks,

    /// Multiple SEEKTABLE blocks were found.
    ///
    /// The FLAC specification allows only one SEEKTABLE block per file.
    MultipleSeekTableBlocks,

    /// Multiple CUESHEET blocks were found.
    ///
    /// The FLAC specification allows only one CUESHEET block per file.
    MultipleCueSheetBlocks,

    /// The STREAMINFO block contains an invalid sample rate of 0.
    InvalidSampleRate,

    /// A metadata block's data is corrupted or incomplete.
    ///
    /// The block header parsed successfully, but the block data itself
    /// is invalid, truncated, or fails validation checks.
    CorruptedBlock,

    /// A size calculation resulted in overflow.
    ///
    /// This typically indicates a corrupted file with invalid size fields
    /// that would cause integer overflow when processed.
    SizeOverflow,

    /// Parsing partially succeeded with some recoverable errors.
    ///
    /// Some metadata blocks loaded successfully, but others failed.
    /// The contained vector lists the specific errors encountered.
    /// This only occurs when `ignore_errors` is enabled in parse options.
    PartialSuccess(Vec<String>),
}

impl std::fmt::Display for FLACError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            FLACErrorKind::InvalidHeader => write!(f, "Invalid FLAC header"),
            FLACErrorKind::BlockSizeError => write!(f, "Block size error"),
            FLACErrorKind::MultipleVorbisBlocks => {
                write!(f, "Multiple Vorbis comment blocks found")
            }
            FLACErrorKind::MultipleSeekTableBlocks => {
                write!(f, "More than one SeekTable block found")
            }
            FLACErrorKind::MultipleCueSheetBlocks => {
                write!(f, "More than one CueSheet block found")
            }
            FLACErrorKind::InvalidSampleRate => {
                write!(f, "A sample rate value of 0 is invalid")
            }
            FLACErrorKind::CorruptedBlock => write!(f, "Corrupted block data"),
            FLACErrorKind::SizeOverflow => write!(f, "Size overflow"),
            FLACErrorKind::PartialSuccess(errors) => {
                write!(f, "Partial success with errors: {}", errors.join("; "))
            }
        }
    }
}

/// Represents a FLAC audio file with metadata and audio stream information.
///
/// This struct provides access to all FLAC metadata blocks and audio properties.
/// FLAC files contain a STREAMINFO block (always present) followed by optional
/// metadata blocks for tags, pictures, seeking, cue sheets, and more.
///
/// # Structure
///
/// A FLAC file consists of:
/// 1. **File header**: "fLaC" signature
/// 2. **Metadata blocks**: One or more blocks containing file information
/// 3. **Audio frames**: Compressed audio data (not loaded into this struct)
///
/// # Examples
///
/// ## Loading and Reading Metadata
///
/// ```no_run
/// use audex::flac::FLAC;
/// use audex::{FileType, Tags};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let flac = FLAC::load("song.flac")?;
///
///     // Access audio properties
///     println!("Sample rate: {} Hz", flac.info.sample_rate);
///     println!("Bit depth: {} bits", flac.info.bits_per_sample);
///     println!("Channels: {}", flac.info.channels);
///
///     // Read Vorbis Comment tags using the Tags trait
///     if let Some(ref tags) = flac.tags {
///         if let Some(artist) = tags.get("ARTIST") {
///             println!("Artist: {:?}", artist);
///         }
///     }
///
///     // Check for embedded artwork
///     if !flac.pictures.is_empty() {
///         println!("Found {} embedded pictures", flac.pictures.len());
///     }
///     Ok(())
/// }
/// ```
///
/// ## Modifying and Saving
///
/// ```no_run
/// use audex::flac::FLAC;
/// use audex::{FileType, Tags};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut flac = FLAC::load("song.flac")?;
///
///     // Modify tags using set for Vorbis Comments
///     if let Some(ref mut tags) = flac.tags {
///         tags.set("TITLE", vec!["New Title".to_string()]);
///         tags.set("ALBUM", vec!["New Album".to_string()]);
///     }
///
///     // Save changes
///     flac.save()?;
///     Ok(())
/// }
/// ```
///
/// ## Custom Parsing Options
///
/// ```no_run
/// use audex::flac::{FLAC, FLACParseOptions};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let options = FLACParseOptions {
///         distrust_size: true,
///         max_block_size: 16 * 1024 * 1024,
///         ignore_errors: true,
///         streaming_io: true,
///         ..Default::default()
///     };
///
///     // Load file with custom options using static method
///     let flac = FLAC::from_file_with_options("song.flac", options)?;
///     Ok(())
/// }
/// ```
///
/// # See Also
///
/// - [`FLACStreamInfo`] - Audio stream properties
/// - [`VCommentDict`] - Vorbis Comment tags
/// - [`Picture`] - Embedded artwork
#[derive(Debug)]
pub struct FLAC {
    /// Audio stream information from STREAMINFO block (always present)
    pub info: FLACStreamInfo,

    /// Vorbis Comment tags (TITLE, ARTIST, ALBUM, etc.), if present
    pub tags: Option<VCommentDict>,

    /// Embedded pictures (album art, artist photos, etc.)
    pub pictures: Vec<Picture>,

    /// Seek table for fast seeking to specific timestamps
    pub seektable: Option<SeekTable>,

    /// Cue sheet with CD track information
    pub cuesheet: Option<CueSheet>,

    /// Application-specific metadata blocks
    pub application_blocks: Vec<ApplicationBlock>,

    /// Generic metadata blocks not fitting other categories
    pub metadata_blocks: Vec<MetadataBlock>,

    /// Padding blocks (reserved space for future metadata)
    pub padding_blocks: Vec<Padding>,

    /// Path to the file (used for save operations)
    filename: String,

    /// Parsing configuration options
    parse_options: FLACParseOptions,

    /// Errors encountered during parsing (when ignore_errors is enabled)
    ///
    /// This field is publicly accessible to check for recoverable errors
    /// that occurred during file loading.
    pub parse_errors: Vec<FLACError>,

    /// Blocks with size values exceeding the 24-bit FLAC spec limit
    ///
    /// Stores (block_type, actual_size) for blocks that claim sizes > 16MB.
    /// Used for validation and diagnostics.
    invalid_overflow_size: Vec<(u8, usize)>,

    /// Original header sizes for oversized blocks loaded from file.
    ///
    /// Maps block_type -> original 3-byte header size for VorbisComment (4)
    /// and Picture (6) blocks that were already oversized on disk. When saving,
    /// if the serialized block still exceeds 0xFFFFFF and we have the original
    /// size recorded here, we write back the original (wrong) size instead of
    /// erroring -- preserving the file's existing brokenness rather than
    /// refusing to save.
    /// pattern.
    original_overflow_sizes: std::collections::HashMap<u8, u32>,

    /// Whether the file has been modified since loading
    ///
    /// Used to optimize saves (skip writing if nothing changed).
    dirty: bool,

    /// Original metadata bytes for comparison
    ///
    /// Stored to detect changes and enable intelligent save optimizations.
    original_metadata: Vec<u8>,

    /// Original block type ordering
    ///
    /// Preserves the original order of metadata blocks for byte-identical
    /// writes when nothing has changed.
    original_block_order: Vec<u8>,
}

impl FLAC {
    /// Create a new empty FLAC instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: FLACStreamInfo::default(),
            tags: None,
            pictures: Vec::new(),
            seektable: None,
            cuesheet: None,
            application_blocks: Vec::new(),
            metadata_blocks: Vec::new(),
            padding_blocks: Vec::new(),
            filename: String::new(),
            parse_options: FLACParseOptions::default(),
            parse_errors: Vec::new(),
            invalid_overflow_size: Vec::new(),
            original_overflow_sizes: std::collections::HashMap::new(),
            dirty: false,
            original_metadata: Vec::new(),
            original_block_order: Vec::new(),
        }
    }

    /// Create new FLAC with custom parsing options
    pub fn with_options(options: FLACParseOptions) -> Self {
        Self {
            info: FLACStreamInfo::default(),
            tags: None,
            pictures: Vec::new(),
            seektable: None,
            cuesheet: None,
            application_blocks: Vec::new(),
            metadata_blocks: Vec::new(),
            padding_blocks: Vec::new(),
            filename: String::new(),
            parse_options: options,
            parse_errors: Vec::new(),
            invalid_overflow_size: Vec::new(),
            original_overflow_sizes: std::collections::HashMap::new(),
            dirty: false,
            original_metadata: Vec::new(),
            original_block_order: Vec::new(),
        }
    }

    /// Get parsing errors encountered during file processing
    pub fn parse_errors(&self) -> &[FLACError] {
        &self.parse_errors
    }

    /// Check if parsing was successful (no critical errors)
    pub fn is_valid(&self) -> bool {
        !self
            .parse_errors
            .iter()
            .any(|e| matches!(e.kind, FLACErrorKind::InvalidHeader))
    }

    /// Loads a FLAC file from the specified path using default parsing options.
    ///
    /// This method opens and parses a FLAC file, loading all metadata blocks
    /// including STREAMINFO, Vorbis Comments, pictures, seek tables, and more.
    /// Audio frame data is not loaded into memory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the FLAC file to load
    ///
    /// # Returns
    ///
    /// * `Ok(FLAC)` - Successfully loaded FLAC file with all metadata
    /// * `Err(AudexError)` - Failed to open file or parse FLAC structure
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file cannot be opened (doesn't exist, permission denied, etc.)
    /// - The file is not a valid FLAC file (missing "fLaC" signature)
    /// - Required STREAMINFO block is missing or corrupted
    /// - Metadata blocks exceed size limits (see [`FLACParseOptions`])
    ///
    /// With default options, recoverable errors are logged in `parse_errors`
    /// but don't prevent loading. Check `flac.parse_errors` after loading
    /// to see if any issues were encountered.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::flac::FLAC;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let flac = FLAC::from_file("song.flac")?;
    ///
    ///     // Check for any parsing errors
    ///     if !flac.parse_errors.is_empty() {
    ///         println!("Loaded with {} warnings", flac.parse_errors.len());
    ///     }
    ///
    ///     println!("Sample rate: {} Hz", flac.info.sample_rate);
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # See Also
    ///
    /// - [`FLAC::from_file_with_options`] - Load with custom parsing options
    /// - [`FLAC::load`](FileType::load) - Trait method (same as `from_file`)
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file_with_options(path, FLACParseOptions::default())
    }

    /// Loads a FLAC file with custom parsing options.
    ///
    /// This method provides fine-grained control over how the FLAC file is parsed,
    /// including error handling behavior, size limits, and I/O strategy.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the FLAC file
    /// * `options` - Custom parsing configuration
    ///
    /// # Returns
    ///
    /// * `Ok(FLAC)` - Successfully loaded FLAC file
    /// * `Err(AudexError)` - Failed to load (behavior depends on options)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::flac::{FLAC, FLACParseOptions};
    ///
    /// // Strict parsing: fail on any error
    /// let options = FLACParseOptions {
    ///     distrust_size: false,
    ///     max_block_size: 1024 * 1024,  // 1MB limit
    ///     ignore_errors: false,           // Fail on errors
    ///     streaming_io: true,
    ///     ..Default::default()
    /// };
    ///
    /// match FLAC::from_file_with_options("song.flac", options) {
    ///     Ok(flac) => println!("Loaded successfully"),
    ///     Err(e) => eprintln!("Failed: {}", e),
    /// }
    /// ```
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all, fields(path = %path.as_ref().display())))]
    pub fn from_file_with_options<P: AsRef<Path>>(
        path: P,
        options: FLACParseOptions,
    ) -> Result<Self> {
        let path = path.as_ref();
        debug_event!("parsing FLAC file");
        let file = File::open(path)?;
        let mut flac = Self::with_options(options);
        flac.filename = path.to_string_lossy().to_string();

        if flac.parse_options.streaming_io {
            let mut reader = BufReader::new(file);
            flac.parse_flac_streaming(&mut reader)?;
        } else {
            // Legacy memory-based parsing
            let mut file = file;
            flac.parse_flac(&mut file)?;
        }

        Ok(flac)
    }

    /// Parse FLAC data from a reader with custom options.
    ///
    /// This allows callers to control parsing behavior (including Vorbis
    /// comment error handling) when loading from an in-memory buffer or
    /// any other reader.
    pub fn from_reader_with_options<R: Read + Seek>(
        reader: &mut R,
        options: FLACParseOptions,
    ) -> Result<Self> {
        let mut flac = Self::with_options(options);
        if flac.parse_options.streaming_io {
            flac.parse_flac_streaming(reader)?;
        } else {
            flac.parse_flac(reader)?;
        }
        Ok(flac)
    }

    /// Parse FLAC file structure
    fn parse_flac<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Check for ID3 tags first
        let mut signature = [0u8; 4];
        reader.read_exact(&mut signature)?;

        if &signature[..3] == b"ID3" {
            // Skip ID3v2 tag
            let mut id3_size_bytes = [0u8; 6];
            reader.read_exact(&mut id3_size_bytes)?;
            let id3_size = self.decode_id3_size(&id3_size_bytes[2..])?;
            reader.seek(SeekFrom::Current(id3_size as i64))?;

            // Read FLAC signature after ID3
            reader.read_exact(&mut signature)?;
        }

        if &signature != b"fLaC" {
            return Err(AudexError::FLACNoHeader);
        }

        // Store position after fLaC header
        let metadata_start = reader.stream_position()?;

        // Parse metadata blocks
        self.parse_metadata_blocks(reader)?;

        // Calculate audio offset by scanning block headers (reliable regardless
        // of reader state after complex read patterns)
        let metadata_end = self.find_audio_offset_from_file(reader)?;

        // Capture original metadata for change detection
        // Cap to actual file size to prevent OOM from crafted size fields
        let file_end = reader.seek(SeekFrom::End(0))?;
        let capped_end = metadata_end.min(file_end);
        // Guard against underflow when metadata_start exceeds the computed end
        // position (can happen with truncated or malformed files)
        let metadata_size_u64 = capped_end.checked_sub(metadata_start).ok_or_else(|| {
            AudexError::InvalidData("metadata region extends beyond file boundaries".to_string())
        })?;
        let metadata_size = usize::try_from(metadata_size_u64).map_err(|_| {
            AudexError::InvalidData("metadata region too large for this platform".to_string())
        })?;
        reader.seek(SeekFrom::Start(metadata_start))?;
        self.original_metadata = vec![0u8; metadata_size];
        // Use read_exact for byte-identical capture, tolerate EOF for truncated test files
        match reader.read_exact(&mut self.original_metadata) {
            Ok(()) => {} // Successfully read all metadata
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Truncated file - clear original_metadata so we don't try to compare
                self.original_metadata.clear();
            }
            Err(e) => return Err(e.into()),
        }

        // Calculate accurate bitrate from audio stream size
        // Use metadata_end (already computed) as the audio start position
        // instead of stream_position() which can be unreliable with BufReader
        if self.info.total_samples > 0 {
            if let Ok(end_pos) = reader.seek(SeekFrom::End(0)) {
                if let Some(duration) = self.info.length {
                    if end_pos >= metadata_end {
                        let audio_size = end_pos - metadata_end;
                        let duration_secs = duration.as_secs_f64();
                        if duration_secs > 0.0 {
                            let bitrate = (audio_size * 8) as f64 / duration_secs;
                            self.info.bitrate = Some(bitrate as u32);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Decode ID3v2 size (synchsafe integer)
    fn decode_id3_size(&self, size_bytes: &[u8]) -> Result<u32> {
        if size_bytes.len() != 4 {
            return Err(AudexError::InvalidData("Invalid ID3 size".to_string()));
        }

        let size = ((size_bytes[0] & 0x7F) as u32) << 21
            | ((size_bytes[1] & 0x7F) as u32) << 14
            | ((size_bytes[2] & 0x7F) as u32) << 7
            | (size_bytes[3] & 0x7F) as u32;

        Ok(size)
    }

    /// Parse FLAC file structure with streaming I/O
    fn parse_flac_streaming<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Check for ID3 tags first
        let mut signature = [0u8; 4];
        reader.read_exact(&mut signature)?;

        if &signature[..3] == b"ID3" {
            // Skip ID3v2 tag
            let mut id3_size_bytes = [0u8; 6];
            reader.read_exact(&mut id3_size_bytes)?;
            let id3_size = self.decode_id3_size(&id3_size_bytes[2..])?;
            reader.seek(SeekFrom::Current(id3_size as i64))?;

            // Read FLAC signature after ID3
            reader.read_exact(&mut signature)?;
        }

        if &signature != b"fLaC" {
            let error = FLACError {
                kind: FLACErrorKind::InvalidHeader,
                position: reader.stream_position().ok(),
                context: "Missing or invalid FLAC signature".to_string(),
            };
            self.parse_errors.push(error);
            return Err(AudexError::FLACNoHeader);
        }

        // Store position after fLaC header
        let metadata_start = reader.stream_position()?;

        // Parse metadata blocks with streaming
        self.parse_metadata_blocks_streaming(reader)?;

        // Calculate audio offset by scanning block headers (reliable regardless
        // of BufReader state, unlike stream_position() after complex read patterns)
        let metadata_end = self.find_audio_offset_from_file(reader)?;

        // Capture original metadata for change detection
        // Cap to actual file size to prevent OOM from crafted size fields
        let file_end = reader.seek(SeekFrom::End(0))?;
        let capped_end = metadata_end.min(file_end);
        // Guard against underflow when metadata_start exceeds the computed end
        // position (can happen with truncated or malformed files)
        let metadata_size_u64 = capped_end.checked_sub(metadata_start).ok_or_else(|| {
            AudexError::InvalidData("metadata region extends beyond file boundaries".to_string())
        })?;
        let metadata_size = usize::try_from(metadata_size_u64).map_err(|_| {
            AudexError::InvalidData("metadata region too large for this platform".to_string())
        })?;
        reader.seek(SeekFrom::Start(metadata_start))?;
        self.original_metadata = vec![0u8; metadata_size];
        // Use read_exact for byte-identical capture, tolerate EOF for truncated test files
        match reader.read_exact(&mut self.original_metadata) {
            Ok(()) => {} // Successfully read all metadata
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Truncated file - clear original_metadata so we don't try to compare
                self.original_metadata.clear();
            }
            Err(e) => return Err(e.into()),
        }

        // Calculate accurate bitrate from audio stream size
        if self.info.total_samples > 0 {
            if let Ok(end_pos) = reader.seek(SeekFrom::End(0)) {
                if let Some(duration) = self.info.length {
                    if end_pos >= metadata_end {
                        let audio_size = end_pos - metadata_end;
                        let duration_secs = duration.as_secs_f64();
                        if duration_secs > 0.0 {
                            let bitrate = (audio_size * 8) as f64 / duration_secs;
                            self.info.bitrate = Some(bitrate as u32);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse FLAC metadata blocks with streaming I/O and robustness
    fn parse_metadata_blocks_streaming<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        let mut is_last = false;
        let mut vorbis_comment_count = 0;
        // Cap the number of metadata blocks to prevent excessive iteration
        // on crafted files where the is_last flag is never set.
        const MAX_METADATA_BLOCKS: usize = 1024;
        let mut block_count: usize = 0;

        while !is_last {
            block_count += 1;
            if block_count > MAX_METADATA_BLOCKS {
                return Err(AudexError::InvalidData(format!(
                    "Exceeded maximum metadata block count ({})",
                    MAX_METADATA_BLOCKS
                )));
            }
            // Propagate stream_position errors instead of defaulting to 0,
            // which would cause subtraction underflow in later offset math
            let block_start_pos = reader.stream_position()?;

            // Read block header (4 bytes)
            let mut header = [0u8; 4];
            if let Err(e) = reader.read_exact(&mut header) {
                if self.parse_options.ignore_errors {
                    self.parse_errors.push(FLACError {
                        kind: FLACErrorKind::CorruptedBlock,
                        position: Some(block_start_pos),
                        context: format!("Failed to read block header: {}", e),
                    });
                    break;
                }
                return Err(e.into());
            }

            let block_type = header[0] & 0x7F; // Remove last block flag
            is_last = (header[0] & 0x80) != 0; // Check last block flag

            let block_size = u32::from_be_bytes([0, header[1], header[2], header[3]]);

            // Track block order for byte-identical writes
            self.original_block_order.push(block_type);

            // Read the block data to store in metadata_blocks
            let block_data_start = reader.stream_position()?;

            // Validate block size — always enforce the limit regardless of
            // distrust_size to prevent uncapped allocations from crafted files
            if block_size > self.parse_options.max_block_size {
                let error = FLACError {
                    kind: FLACErrorKind::BlockSizeError,
                    position: Some(block_start_pos),
                    context: format!(
                        "Block size {} exceeds maximum {}",
                        block_size, self.parse_options.max_block_size
                    ),
                };
                self.parse_errors.push(error);

                // VORBIS_COMMENT (block_type 4) block size errors are always fatal
                // to prevent OOM attacks, even with ignore_errors enabled
                if block_type == 4 || !self.parse_options.ignore_errors {
                    return Err(AudexError::InvalidData(format!(
                        "Block size {} exceeds maximum {}",
                        block_size, self.parse_options.max_block_size
                    )));
                }

                // For non-Vorbis blocks with ignore_errors, skip the full
                // declared block_size to position at the next block header.
                // Using min(block_size, max_block_size) here would leave the
                // reader mid-block, causing the next iteration to misparse.
                reader.seek(SeekFrom::Current(block_size as i64))?;
                continue;
            }

            // Emit per-block trace for observability
            trace_event!(
                block_type = block_type,
                block_size = block_size,
                is_last = is_last,
                "parsing FLAC metadata block"
            );

            // Handle different block types with error recovery
            let parse_result = match block_type {
                0 => {
                    // STREAMINFO block (mandatory, always first)
                    self.parse_streaminfo_block_safe(reader, block_size, block_start_pos)
                }
                1 => {
                    // PADDING block - parse and store
                    self.parse_padding_block_safe(reader, block_size, block_start_pos)
                }
                2 => {
                    // APPLICATION block - preserve all data
                    self.parse_application_block_safe(reader, block_size, block_start_pos)
                }
                3 => {
                    // SEEKTABLE block - check for duplicates
                    if self.seektable.is_some() {
                        self.parse_errors.push(FLACError {
                            kind: FLACErrorKind::MultipleSeekTableBlocks,
                            position: Some(block_start_pos),
                            context: "> 1 SeekTable block found".to_string(),
                        });

                        if !self.parse_options.ignore_errors {
                            return Err(AudexError::InvalidData(
                                "> 1 SeekTable block found".to_string(),
                            ));
                        }

                        // Skip duplicate SeekTable block
                        reader.seek(SeekFrom::Current(block_size as i64))?;
                        Ok(())
                    } else {
                        self.parse_seektable_block_safe(reader, block_size, block_start_pos)
                    }
                }
                4 => {
                    // VORBIS_COMMENT block - handle multiples gracefully
                    vorbis_comment_count += 1;
                    if vorbis_comment_count > 1 {
                        self.parse_errors.push(FLACError {
                            kind: FLACErrorKind::MultipleVorbisBlocks,
                            position: Some(block_start_pos),
                            context: format!(
                                "Found {} VORBIS_COMMENT blocks, using first",
                                vorbis_comment_count
                            ),
                        });

                        if !self.parse_options.ignore_errors {
                            return Err(AudexError::FLACVorbis);
                        }

                        // Skip duplicate Vorbis block
                        reader.seek(SeekFrom::Current(block_size as i64))?;
                        Ok(())
                    } else {
                        self.parse_vorbis_comment_block_safe(reader, block_size, block_start_pos)
                    }
                }
                5 => {
                    // CUESHEET block - check for duplicates
                    if self.cuesheet.is_some() {
                        self.parse_errors.push(FLACError {
                            kind: FLACErrorKind::MultipleCueSheetBlocks,
                            position: Some(block_start_pos),
                            context: "> 1 CueSheet block found".to_string(),
                        });

                        if !self.parse_options.ignore_errors {
                            return Err(AudexError::InvalidData(
                                "> 1 CueSheet block found".to_string(),
                            ));
                        }

                        // Skip duplicate CueSheet block
                        reader.seek(SeekFrom::Current(block_size as i64))?;
                        Ok(())
                    } else {
                        self.parse_cuesheet_block_safe(reader, block_size, block_start_pos)
                    }
                }
                6 => {
                    // PICTURE block
                    self.parse_picture_block_safe(reader, block_size, block_start_pos)
                }
                _ => {
                    // Unknown or reserved block type - skip
                    reader.seek(SeekFrom::Current(block_size as i64))?;
                    Ok(())
                }
            };

            // Handle parse errors
            if let Err(e) = parse_result {
                warn_event!(block_type = block_type, error = %e, "FLAC metadata block parse error");
                let error = FLACError {
                    kind: FLACErrorKind::CorruptedBlock,
                    position: Some(block_start_pos),
                    context: format!("Block type {} parse error: {}", block_type, e),
                };
                self.parse_errors.push(error);

                // Determine if error is fatal based on type and ignore_errors setting
                let is_fatal_error = if self.parse_options.ignore_errors {
                    // With ignore_errors, still fail on critical errors:
                    // - Fatal Vorbis errors (OOM attacks, structural corruption)
                    // but allow benign truncations to be skipped
                    block_type == 4 && self.is_fatal_vorbis_error(&e)
                } else {
                    // Without ignore_errors, STREAMINFO and certain Vorbis errors are fatal
                    block_type == 0 || (block_type == 4 && self.is_fatal_vorbis_error(&e))
                };

                if is_fatal_error {
                    return Err(e);
                }

                // Try to skip corrupted block and continue
                let current_pos = reader.stream_position()?;
                let expected_pos = block_start_pos + 4 + block_size as u64;
                if current_pos != expected_pos {
                    reader.seek(SeekFrom::Start(expected_pos))?;
                }
            } else {
                // Successfully parsed block - add it to metadata_blocks list
                // Read the block data to store
                let current_pos = reader.stream_position()?;
                let bytes_to_read = current_pos.checked_sub(block_data_start).ok_or_else(|| {
                    AudexError::InvalidData(format!(
                        "FLAC block position underflow: current {} < start {}",
                        current_pos, block_data_start
                    ))
                })? as usize;

                // For VorbisComment (4) and Picture (6) blocks: if the real
                // content size exceeds the 24-bit max, record the original
                // header size so we can write it back on save (distrust_size
                // round-trip).
                if (block_type == 4 || block_type == 6) && bytes_to_read > 0xFFFFFF {
                    self.original_overflow_sizes.insert(block_type, block_size);
                }

                // Seek back to read the block data
                reader.seek(SeekFrom::Start(block_data_start))?;
                let mut block_data = vec![0u8; bytes_to_read];
                reader.read_exact(&mut block_data)?;

                // Add to metadata_blocks
                self.metadata_blocks
                    .push(MetadataBlock::new(block_type, block_data));
            }
        }

        // Emit debug summaries after all metadata blocks are parsed
        debug_event!(
            sample_rate = self.info.sample_rate,
            channels = self.info.channels,
            bits_per_sample = self.info.bits_per_sample,
            total_samples = self.info.total_samples,
            "FLAC STREAMINFO parsed"
        );
        if let Some(ref _tags) = self.tags {
            debug_event!(
                tag_count = _tags.keys().len(),
                vendor = %_tags.vendor(),
                "FLAC Vorbis Comment parsed"
            );
        }
        let _picture_count = self.pictures.len();
        if _picture_count > 0 {
            debug_event!(picture_count = _picture_count, "FLAC pictures parsed");
        }

        Ok(())
    }

    /// Check if a VORBIS_COMMENT error is fatal (indicates corruption)
    fn is_fatal_vorbis_error(&self, error: &AudexError) -> bool {
        // Errors related to malicious data structures are fatal (e.g., OOM attacks)
        // Benign truncations (IO errors) are not fatal
        match error {
            AudexError::InvalidData(msg) => {
                // Fatal: Structural errors indicating malicious data
                // Non-fatal: "failed to fill whole buffer" (IO truncation)
                if msg.contains("failed to fill whole buffer") {
                    // This is a benign truncation, not fatal
                    false
                } else {
                    // Fatal OOM protection errors
                    msg.contains("exceeds maximum")
                        // Fatal: I/O read errors in Vorbis structure
                        || msg.contains("failed to read vendor")
                        || msg.contains("failed to read comment count")
                        || msg.contains("failed to read comment length")
                        || msg.contains("Truncated Vorbis")
                        // Fatal when strict mode: UTF-8 encoding errors wrapped
                        // by the safe parser as InvalidData
                        || (matches!(self.parse_options.vorbis_error_mode, crate::vorbis::ErrorMode::Strict)
                            && msg.contains("encoding error"))
                }
            }
            // UTF-8 encoding errors are fatal when strict error mode is selected
            AudexError::FormatError(_) => {
                matches!(
                    self.parse_options.vorbis_error_mode,
                    crate::vorbis::ErrorMode::Strict
                )
            }
            _ => false,
        }
    }

    /// Parse FLAC metadata blocks (legacy method)
    fn parse_metadata_blocks<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        let mut is_last = false;
        // Cap the number of metadata blocks to prevent excessive iteration
        // on crafted files where the is_last flag is never set. FLAC only
        // defines 7 block types (0-6) and real files rarely exceed a dozen
        // blocks, so 1024 is a generous upper bound.
        const MAX_METADATA_BLOCKS: usize = 1024;
        let mut block_count: usize = 0;

        while !is_last {
            block_count += 1;
            if block_count > MAX_METADATA_BLOCKS {
                return Err(AudexError::InvalidData(format!(
                    "Exceeded maximum metadata block count ({})",
                    MAX_METADATA_BLOCKS
                )));
            }
            // Read block header (4 bytes)
            let mut header = [0u8; 4];
            reader.read_exact(&mut header)?;

            let block_type = header[0] & 0x7F; // Remove last block flag
            is_last = (header[0] & 0x80) != 0; // Check last block flag

            let block_size = u32::from_be_bytes([0, header[1], header[2], header[3]]);

            // Enforce block size limit to prevent uncapped allocations
            // from crafted files (matching the streaming parser path)
            if block_size > self.parse_options.max_block_size {
                return Err(AudexError::InvalidData(format!(
                    "Block size {} exceeds maximum {}",
                    block_size, self.parse_options.max_block_size
                )));
            }

            // Track block order for byte-identical writes
            self.original_block_order.push(block_type);

            // Read the block data to store in metadata_blocks
            let block_data_start = reader.stream_position()?;

            match block_type {
                0 => {
                    // STREAMINFO block (mandatory, always first)
                    self.parse_streaminfo_block(reader, block_size)?;
                }
                1 => {
                    // PADDING block - parse and store
                    self.parse_padding_block(reader, block_size)?
                }
                2 => {
                    // APPLICATION block - preserve all data
                    self.parse_application_block(reader, block_size)?;
                }
                3 => {
                    // SEEKTABLE block
                    if self.seektable.is_some() {
                        return Err(AudexError::InvalidData(
                            "> 1 SeekTable block found".to_string(),
                        ));
                    }
                    self.parse_seektable_block(reader, block_size)?;
                }
                4 => {
                    // VORBIS_COMMENT block — reject duplicates since the
                    // FLAC spec mandates at most one per file
                    if self.tags.is_some() {
                        return Err(AudexError::InvalidData(
                            "> 1 VorbisComment block found".to_string(),
                        ));
                    }
                    self.parse_vorbis_comment_block(reader, block_size)?;
                }
                5 => {
                    // CUESHEET block
                    if self.cuesheet.is_some() {
                        return Err(AudexError::InvalidData(
                            "> 1 CueSheet block found".to_string(),
                        ));
                    }
                    self.parse_cuesheet_block(reader, block_size)?;
                }
                6 => {
                    // PICTURE block
                    self.parse_picture_block(reader, block_size)?;
                }
                _ => {
                    // Unknown or reserved block type - skip
                    reader.seek(SeekFrom::Current(block_size as i64))?;
                }
            }

            // Add block to metadata_blocks list
            let current_pos = reader.stream_position()?;
            let bytes_to_read = current_pos.checked_sub(block_data_start).ok_or_else(|| {
                AudexError::InvalidData(format!(
                    "FLAC block position underflow: current {} < start {}",
                    current_pos, block_data_start
                ))
            })? as usize;

            // For VorbisComment (4) and Picture (6) blocks: if the real
            // content size exceeds the 24-bit max, record the original
            // header size so we can write it back on save (distrust_size
            // round-trip).
            if (block_type == 4 || block_type == 6) && bytes_to_read > 0xFFFFFF {
                self.original_overflow_sizes.insert(block_type, block_size);
            }

            // Seek back to read the block data
            reader.seek(SeekFrom::Start(block_data_start))?;
            let mut block_data = vec![0u8; bytes_to_read];
            reader.read_exact(&mut block_data)?;

            // Add to metadata_blocks
            self.metadata_blocks
                .push(MetadataBlock::new(block_type, block_data));
        }

        Ok(())
    }

    /// Parse STREAMINFO metadata block (legacy)
    fn parse_streaminfo_block<R: Read>(&mut self, reader: &mut R, _block_size: u32) -> Result<()> {
        let mut data = [0u8; 34]; // STREAMINFO is always 34 bytes
        reader.read_exact(&mut data)?;

        let mut cursor = Cursor::new(&data);

        // Parse STREAMINFO fields
        self.info.min_blocksize = cursor.read_u16::<BigEndian>()?;
        self.info.max_blocksize = cursor.read_u16::<BigEndian>()?;

        let min_framesize_bytes = [cursor.read_u8()?, cursor.read_u8()?, cursor.read_u8()?];
        self.info.min_framesize = u32::from_be_bytes([
            0,
            min_framesize_bytes[0],
            min_framesize_bytes[1],
            min_framesize_bytes[2],
        ]);

        let max_framesize_bytes = [cursor.read_u8()?, cursor.read_u8()?, cursor.read_u8()?];
        self.info.max_framesize = u32::from_be_bytes([
            0,
            max_framesize_bytes[0],
            max_framesize_bytes[1],
            max_framesize_bytes[2],
        ]);

        // Parse sample rate, channels, bits per sample, total samples (20 + 3 + 5 + 36 = 64 bits)
        let combined = cursor.read_u64::<BigEndian>()?;

        self.info.sample_rate = ((combined >> 44) & 0xFFFFF) as u32; // 20 bits
        self.info.channels = (((combined >> 41) & 0x07) as u16) + 1; // 3 bits, +1 because encoded as channels-1
        self.info.bits_per_sample = (((combined >> 36) & 0x1F) as u16) + 1; // 5 bits, +1 because encoded as bps-1
        self.info.total_samples = combined & 0xFFFFFFFFF; // 36 bits

        // A sample rate of 0 is invalid per the FLAC specification
        if self.info.sample_rate == 0 {
            return Err(AudexError::InvalidData(
                "A sample rate value of 0 is invalid".to_string(),
            ));
        }

        // MD5 signature (16 bytes)
        cursor.read_exact(&mut self.info.md5_signature)?;

        // Calculate length and bitrate
        if self.info.sample_rate > 0 {
            if self.info.total_samples > 0 {
                let duration_secs = self.info.total_samples as f64 / self.info.sample_rate as f64;
                self.info.length = Some(Duration::from_secs_f64(duration_secs));

                // FLAC bitrate is variable, but we can estimate average
                // This would be more accurate with actual file size
                let bits_per_second = self.info.sample_rate as u64
                    * self.info.channels as u64
                    * self.info.bits_per_sample as u64;
                // Saturate to u32::MAX instead of silently truncating
                self.info.bitrate = Some(u32::try_from(bits_per_second).unwrap_or(u32::MAX));
            } else {
                // Zero samples means zero duration and zero bitrate
                self.info.length = Some(Duration::from_secs(0));
                self.info.bitrate = Some(0);
            }
        }

        Ok(())
    }

    /// Parse STREAMINFO metadata block with safety checks
    fn parse_streaminfo_block_safe<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        block_size: u32,
        _block_start_pos: u64,
    ) -> Result<()> {
        // STREAMINFO must be exactly 34 bytes
        if block_size != 34 {
            return Err(AudexError::InvalidData(format!(
                "Invalid STREAMINFO size: {} (expected 34)",
                block_size
            )));
        }

        self.parse_streaminfo_block(reader, block_size)
    }

    /// Parse VORBIS_COMMENT metadata block (legacy)
    fn parse_vorbis_comment_block<R: Read>(
        &mut self,
        reader: &mut R,
        block_size: u32,
    ) -> Result<()> {
        let mut data = vec![0u8; block_size as usize];
        reader.read_exact(&mut data)?;

        // Parse Vorbis comment data - FLAC files don't have framing bit
        let comment = VCommentDict::from_bytes_with_options(
            &data,
            self.parse_options.vorbis_error_mode,
            false,
        )?;
        self.tags = Some(comment);

        Ok(())
    }

    /// Parse VORBIS_COMMENT metadata block with safety checks
    fn parse_vorbis_comment_block_safe<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        block_size: u32,
        block_start_pos: u64,
    ) -> Result<()> {
        if self.parse_options.distrust_size {
            // When distrust_size is set, ignore header size and parse directly
            // from the stream. The VComment parser reads field-by-field using
            // internal length fields, leaving the stream at the correct position.
            let start_pos = reader.stream_position()?;
            let mut comment = VCommentDict::new();
            comment
                .load(reader, self.parse_options.vorbis_error_mode, false)
                .map_err(|e| {
                    AudexError::InvalidData(format!(
                        "Vorbis comment parse error at position {}: {}",
                        block_start_pos, e
                    ))
                })?;

            let real_size = reader.stream_position()? - start_pos;

            // Enforce max_block_size on the actual bytes consumed, not
            // just the header's declared size. A crafted file could
            // have a small header size but large internal VComment
            // length fields that bypass the caller's check.
            if real_size > self.parse_options.max_block_size as u64 {
                return Err(AudexError::InvalidData(format!(
                    "Vorbis comment actual size ({} bytes) exceeds max_block_size ({})",
                    real_size, self.parse_options.max_block_size
                )));
            }

            if real_size > 0xFFFFFF_u64 {
                self.original_overflow_sizes.insert(4, block_size);
            }

            self.tags = Some(comment);
        } else {
            // Enforce the same max_block_size limit used by the distrust_size
            // path and by all other block types. A hardcoded cap here would
            // create an inconsistency where disabling distrust_size silently
            // raises the effective limit beyond the configured ceiling.
            if block_size > self.parse_options.max_block_size {
                return Err(AudexError::InvalidData(format!(
                    "Vorbis comment block size ({} bytes) exceeds max_block_size ({})",
                    block_size, self.parse_options.max_block_size
                )));
            }

            let mut data = vec![0u8; block_size as usize];
            reader.read_exact(&mut data)?;

            // FLAC files don't have framing bit in Vorbis comments.
            // Use the configured error mode so that callers who set
            // e.g. ErrorMode::Strict get consistent validation
            // regardless of the distrust_size setting.
            let comment = VCommentDict::from_bytes_with_options(
                &data,
                self.parse_options.vorbis_error_mode,
                false,
            )?;
            self.tags = Some(comment);
        }

        Ok(())
    }

    /// Maximum metadata block size for legacy parsers (64 MB).
    const MAX_LEGACY_BLOCK: u32 = 64 * 1024 * 1024;

    /// Parse SEEKTABLE metadata block (legacy)
    fn parse_seektable_block<R: Read>(&mut self, reader: &mut R, block_size: u32) -> Result<()> {
        // Check against global ParseLimits before allocating, so that
        // restrictive limits configured for untrusted input are honoured
        // even through the legacy code path.
        crate::limits::ParseLimits::default()
            .check_tag_size(block_size as u64, "FLAC legacy seektable block")?;
        if block_size > Self::MAX_LEGACY_BLOCK {
            return Err(AudexError::ParseError(format!(
                "FLAC block too large: {} bytes",
                block_size
            )));
        }
        let mut data = vec![0u8; block_size as usize];
        reader.read_exact(&mut data)?;

        let seektable = SeekTable::from_bytes(&data)?;
        self.seektable = Some(seektable);

        Ok(())
    }

    /// Parse SEEKTABLE metadata block with safety checks
    fn parse_seektable_block_safe<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        block_size: u32,
        _block_start_pos: u64,
    ) -> Result<()> {
        let safe_size = if self.parse_options.distrust_size {
            min(block_size, self.parse_options.max_block_size)
        } else {
            block_size
        };

        let mut data = vec![0u8; safe_size as usize];
        let bytes_read = reader.read(&mut data)?;

        if bytes_read < safe_size as usize {
            return Err(AudexError::InvalidData(
                "Truncated SEEKTABLE block".to_string(),
            ));
        }

        data.truncate(bytes_read);

        // Use robust seek table parsing with limits
        let max_seekpoints = Some(10000); // Reasonable limit for seek points
        let seektable = SeekTable::from_bytes_with_options(&data, max_seekpoints)?;
        self.seektable = Some(seektable);

        // Skip any remaining bytes if we truncated
        if block_size > safe_size {
            reader.seek(SeekFrom::Current((block_size - safe_size) as i64))?;
        }

        Ok(())
    }

    /// Parse CUESHEET metadata block (legacy)
    fn parse_cuesheet_block<R: Read>(&mut self, reader: &mut R, block_size: u32) -> Result<()> {
        // Enforce global ParseLimits before allocating (see parse_seektable_block)
        crate::limits::ParseLimits::default()
            .check_tag_size(block_size as u64, "FLAC legacy cuesheet block")?;
        if block_size > Self::MAX_LEGACY_BLOCK {
            return Err(AudexError::ParseError(format!(
                "FLAC block too large: {} bytes",
                block_size
            )));
        }
        let mut data = vec![0u8; block_size as usize];
        reader.read_exact(&mut data)?;

        let cuesheet = CueSheet::from_bytes(&data)?;
        self.cuesheet = Some(cuesheet);

        Ok(())
    }

    /// Parse CUESHEET metadata block with safety checks
    fn parse_cuesheet_block_safe<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        block_size: u32,
        _block_start_pos: u64,
    ) -> Result<()> {
        let safe_size = if self.parse_options.distrust_size {
            min(block_size, self.parse_options.max_block_size)
        } else {
            block_size
        };

        let mut data = vec![0u8; safe_size as usize];
        let bytes_read = reader.read(&mut data)?;

        if bytes_read < safe_size as usize {
            return Err(AudexError::InvalidData(
                "Truncated CUESHEET block".to_string(),
            ));
        }

        data.truncate(bytes_read);
        let cuesheet = CueSheet::from_bytes(&data)?;
        self.cuesheet = Some(cuesheet);

        // Skip any remaining bytes if we truncated
        if block_size > safe_size {
            reader.seek(SeekFrom::Current((block_size - safe_size) as i64))?;
        }

        Ok(())
    }

    /// Parse PICTURE metadata block (legacy)
    fn parse_picture_block<R: Read>(&mut self, reader: &mut R, block_size: u32) -> Result<()> {
        // Enforce global ParseLimits before allocating (see parse_seektable_block)
        crate::limits::ParseLimits::default()
            .check_tag_size(block_size as u64, "FLAC legacy picture block")?;
        if block_size > Self::MAX_LEGACY_BLOCK {
            return Err(AudexError::ParseError(format!(
                "FLAC block too large: {} bytes",
                block_size
            )));
        }
        let mut data = vec![0u8; block_size as usize];
        reader.read_exact(&mut data)?;

        let picture = Picture::from_bytes(&data)?;
        self.pictures.push(picture);

        Ok(())
    }

    /// Parse PICTURE metadata block with safety checks
    fn parse_picture_block_safe<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        block_size: u32,
        _block_start_pos: u64,
    ) -> Result<()> {
        if self.parse_options.distrust_size {
            // When distrust_size is set, ignore header size and parse directly
            // from the stream using internal length fields.
            let start_pos = reader.stream_position()?;
            let max_picture_size = Some(self.parse_options.max_block_size as usize);

            match Picture::from_reader(reader, max_picture_size) {
                Ok(picture) => {
                    let real_size = reader.stream_position()? - start_pos;
                    if real_size > 0xFFFFFF_u64 {
                        self.original_overflow_sizes.insert(6, block_size);
                    }
                    self.pictures.push(picture);
                }
                Err(_) if self.parse_options.ignore_errors => {
                    // Skip corrupted picture, try to recover position
                    let _ = reader.seek(SeekFrom::Start(start_pos + block_size as u64));
                }
                Err(e) => return Err(e),
            }
        } else {
            let mut data = vec![0u8; block_size as usize];
            reader.read_exact(&mut data)?;

            let max_picture_size = Some(self.parse_options.max_block_size as usize);

            match Picture::from_bytes_with_options(&data, max_picture_size) {
                Ok(picture) => self.pictures.push(picture),
                Err(_) if self.parse_options.ignore_errors => {}
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    /// Parse PADDING metadata block (legacy)
    fn parse_padding_block<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        block_size: u32,
    ) -> Result<()> {
        // For padding blocks, we don't need to read the actual data
        // Just skip it and record the size
        let padding = Padding::new(block_size as usize);
        self.padding_blocks.push(padding);
        reader.seek(SeekFrom::Current(block_size as i64))?;
        Ok(())
    }

    /// Parse APPLICATION metadata block (legacy)
    fn parse_application_block<R: Read>(&mut self, reader: &mut R, block_size: u32) -> Result<()> {
        let mut data = vec![0u8; block_size as usize];
        reader.read_exact(&mut data)?;

        let application_block = ApplicationBlock::from_bytes(&data)?;
        self.application_blocks.push(application_block);

        Ok(())
    }

    /// Parse APPLICATION metadata block with safety checks
    fn parse_application_block_safe<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        block_size: u32,
        _block_start_pos: u64,
    ) -> Result<()> {
        let safe_size = if self.parse_options.distrust_size {
            min(block_size, self.parse_options.max_block_size)
        } else {
            block_size
        };

        let mut data = vec![0u8; safe_size as usize];
        let bytes_read = reader.read(&mut data)?;

        if bytes_read < safe_size as usize {
            return Err(AudexError::InvalidData(
                "Truncated APPLICATION block".to_string(),
            ));
        }

        data.truncate(bytes_read);

        let application_block = ApplicationBlock::from_bytes(&data)?;
        self.application_blocks.push(application_block);

        // Skip any remaining bytes if we truncated
        if block_size > safe_size {
            reader.seek(SeekFrom::Current((block_size - safe_size) as i64))?;
        }

        Ok(())
    }

    /// Parse PADDING metadata block with safety checks
    fn parse_padding_block_safe<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        block_size: u32,
        _block_start_pos: u64,
    ) -> Result<()> {
        // Padding blocks can be very large, so we handle them efficiently
        let safe_size = if self.parse_options.distrust_size {
            min(block_size, self.parse_options.max_block_size)
        } else {
            block_size
        };

        // Create padding block record
        let padding = Padding::new(safe_size as usize);
        self.padding_blocks.push(padding);

        // Skip the padding data without reading it into memory
        reader.seek(SeekFrom::Current(safe_size as i64))?;

        // Skip any remaining bytes if we truncated
        if block_size > safe_size {
            reader.seek(SeekFrom::Current((block_size - safe_size) as i64))?;
        }

        Ok(())
    }

    /// Add a new picture to the file
    pub fn add_picture(&mut self, picture: Picture) {
        // Pictures will be validated when serialized during save()
        self.pictures.push(picture);
        self.dirty = true;
    }

    /// Clear all pictures from the file
    pub fn clear_pictures(&mut self) {
        self.pictures.clear();
        // Also remove picture blocks from metadata_blocks (block type 6)
        self.metadata_blocks.retain(|block| block.block_type != 6);
        self.dirty = true;
    }

    /// Add a new Application block to the file
    pub fn add_application_block(&mut self, application_block: ApplicationBlock) {
        self.application_blocks.push(application_block);
        self.dirty = true;
    }

    /// Clear all Application blocks from the file
    pub fn clear_application_blocks(&mut self) {
        self.application_blocks.clear();
        self.dirty = true;
    }

    /// Get Application blocks with a specific application ID
    pub fn get_application_blocks_by_id(&self, application_id: [u8; 4]) -> Vec<&ApplicationBlock> {
        self.application_blocks
            .iter()
            .filter(|block| block.application_id == application_id)
            .collect()
    }

    /// Remove Application blocks with a specific application ID
    pub fn remove_application_blocks_by_id(&mut self, application_id: [u8; 4]) -> usize {
        let initial_len = self.application_blocks.len();
        self.application_blocks
            .retain(|block| block.application_id != application_id);
        let removed_count = initial_len - self.application_blocks.len();
        if removed_count > 0 {
            self.dirty = true;
        }
        removed_count
    }

    /// Add padding block to the file
    pub fn add_padding(&mut self, size: usize) {
        if size > 0 {
            self.padding_blocks.push(Padding::new(size));
            self.dirty = true;
        }
    }

    /// Clear all padding blocks from the file
    pub fn clear_padding(&mut self) {
        self.padding_blocks.clear();
        self.dirty = true;
    }

    /// Get total padding size across all padding blocks
    pub fn total_padding_size(&self) -> usize {
        self.padding_blocks.iter().map(|p| p.size).sum()
    }

    /// Get information about blocks that exceed size limits
    pub fn get_overflow_blocks(&self) -> &[(u8, usize)] {
        &self.invalid_overflow_size
    }

    /// Check if any blocks have overflow size issues
    pub fn has_overflow_blocks(&self) -> bool {
        !self.invalid_overflow_size.is_empty()
    }

    /// Optimize padding distribution for better performance
    /// Consolidates small padding blocks and removes empty ones
    pub fn optimize_padding(&mut self) {
        let initial_len = self.padding_blocks.len();

        // Remove zero-size padding blocks
        self.padding_blocks.retain(|p| p.size > 0);

        // If we have multiple small padding blocks, consider consolidating
        if self.padding_blocks.len() > 1 {
            let total_size: usize = self.padding_blocks.iter().map(|p| p.size).sum();
            let small_blocks = self.padding_blocks.iter().filter(|p| p.size < 1024).count();

            // If we have many small blocks, consolidate them
            if small_blocks > 3 {
                self.padding_blocks.clear();
                if total_size > 0 {
                    self.padding_blocks.push(Padding::new(total_size));
                }
            }
        }

        // Mark dirty if we actually changed the padding structure
        if initial_len != self.padding_blocks.len() {
            self.dirty = true;
        }
    }

    /// Calculate optimal padding size based on metadata growth patterns
    pub fn calculate_optimal_padding(&self, growth_factor: f64) -> usize {
        // Calculate current metadata size (excluding padding)
        let mut total_metadata_size = 34; // STREAMINFO is always 34 bytes

        if let Some(ref tags) = self.tags {
            if let Ok(data) = tags.to_bytes() {
                total_metadata_size += data.len();
            }
        }

        total_metadata_size += self
            .pictures
            .iter()
            .filter_map(|p| p.to_bytes().ok())
            .map(|data| data.len())
            .sum::<usize>();

        if let Some(ref seektable) = self.seektable {
            if let Ok(data) = seektable.to_bytes() {
                total_metadata_size += data.len();
            }
        }

        if let Some(ref cuesheet) = self.cuesheet {
            if let Ok(data) = cuesheet.to_bytes() {
                total_metadata_size += data.len();
            }
        }

        // Apply growth factor with reasonable bounds
        let growth_padding = (total_metadata_size as f64 * growth_factor) as usize;
        let min_padding = 1024; // 1KB minimum
        let max_padding = 64 * 1024; // 64KB maximum

        growth_padding.max(min_padding).min(max_padding)
    }

    /// Add empty Vorbis comment block to file
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() && !self.parse_options.ignore_errors {
            return Err(AudexError::FLACVorbis);
        }
        // With ignore_errors, replace existing tags

        self.tags = Some(VCommentDict::with_framing(false));
        self.dirty = true;
        Ok(())
    }

    /// Saves metadata changes to the FLAC file using in-place modification.
    ///
    /// This method writes modified metadata (tags, pictures, etc.) back to the file
    /// while preserving the audio data. It uses in-place modification when possible
    /// to avoid rewriting the entire file.
    ///
    /// # Strategy
    ///
    /// The save operation attempts to:
    /// 1. Reuse existing padding if metadata fits
    /// 2. Resize metadata blocks if needed
    /// 3. Only rewrite the metadata section, not audio data
    /// 4. Maintain optimal padding for future edits
    ///
    /// # Arguments
    ///
    /// * `path` - Optional alternative path to save to (uses original filename if `None`)
    /// * `_delete_id3` - (Reserved) Whether to remove ID3 tags
    /// * `padding_func` - Optional function to calculate custom padding size
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Metadata successfully saved
    /// * `Err(AudexError)` - Failed to write file
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file cannot be opened for writing
    /// - The file doesn't exist
    /// - Insufficient disk space
    /// - Metadata blocks exceed FLAC size limits
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::flac::FLAC;
    /// use audex::Tags;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut flac = FLAC::from_file("song.flac")?;
    ///
    ///     // Modify tags using the Tags trait
    ///     if let Some(ref mut tags) = flac.tags {
    ///         tags.set("ARTIST", vec!["New Artist".to_string()]);
    ///     }
    ///
    ///     // Save with default padding
    ///     flac.save_to_file(None::<&str>, false, None)?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// ```no_run
    /// use audex::flac::FLAC;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut flac = FLAC::from_file("song.flac")?;
    ///
    ///     // Save to different file
    ///     flac.save_to_file(Some("output.flac"), false, None)?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # See Also
    ///
    /// - [`FLAC::save`](FileType::save) - Trait method for saving
    /// - [`FLAC::add_padding`] - Add padding for future edits
    pub fn save_to_file<P: AsRef<Path>>(
        &mut self,
        path: Option<P>,
        _delete_id3: bool,
        padding_func: Option<PaddingFunction>,
    ) -> Result<()> {
        let file_path = match path {
            Some(p) => p.as_ref().to_path_buf(),
            None => std::path::PathBuf::from(&self.filename),
        };

        self.save_to_file_in_place(&file_path, padding_func)
    }

    fn save_to_file_in_place(
        &mut self,
        file_path: &Path,
        padding_func: Option<PaddingFunction>,
    ) -> Result<()> {
        if !file_path.exists() {
            return Err(AudexError::InvalidData("File does not exist".to_string()));
        }

        // Open file for reading and writing
        let mut file = OpenOptions::new().read(true).write(true).open(file_path)?;

        // Find where the audio data starts by reading minimal header data
        let audio_offset = self.find_audio_offset_from_file(&mut file)?;

        // Calculate available space (current metadata size minus "fLaC" header)
        let header_size = 4u64; // "fLaC" signature size
        let available = audio_offset.checked_sub(header_size).ok_or_else(|| {
            AudexError::InvalidData(
                "audio offset is smaller than FLAC header size, file may be corrupt".to_string(),
            )
        })?;

        // Calculate content (audio data) size for padding calculation
        let file_size = file.seek(SeekFrom::End(0))?;
        let content_size = file_size.checked_sub(audio_offset).ok_or_else(|| {
            AudexError::InvalidData(
                "file size is smaller than audio offset, file may be truncated".to_string(),
            )
        })? as usize;

        // Generate new metadata, or return early if nothing changed
        let new_metadata = match self.prepare_metadata(padding_func, available, content_size)? {
            Some(m) => m,
            None => return Ok(()), // no changes
        };

        let data_size = new_metadata.len() as u64;

        // Log metadata block size and padding details for diagnostics
        trace_event!(
            metadata_bytes = data_size,
            available_bytes = available,
            audio_offset = audio_offset,
            "writing FLAC metadata blocks"
        );

        // Use resize_bytes to modify file in-place (works with File which
        // supports set_len for truncation).
        resize_bytes(&mut file, available, data_size, header_size)?;

        // Write "fLaC" marker + new metadata
        file.seek(SeekFrom::Start(0))?;
        file.write_all(b"fLaC")?;
        file.write_all(&new_metadata)?;

        // Store new metadata as original and reset dirty flag
        self.original_metadata = new_metadata;
        self.dirty = false;

        Ok(())
    }

    /// Core save logic that operates on any readable/writable/seekable handle.
    ///
    /// This is used by the writer-based save path (`save_to_writer`,
    /// `clear_writer`). The handle must already contain valid FLAC data
    /// (starting with the `fLaC` marker followed by metadata blocks and
    /// audio frames).
    fn save_to_writer_impl(
        &mut self,
        file: &mut dyn crate::ReadWriteSeek,
        padding_func: Option<PaddingFunction>,
    ) -> Result<()> {
        // Find where the audio data starts by reading minimal header data
        let audio_offset = self.find_audio_offset_from_file(file)?;

        // Calculate available space (current metadata size minus "fLaC" header)
        let header_size = 4u64; // "fLaC" signature size
        let available = audio_offset.checked_sub(header_size).ok_or_else(|| {
            AudexError::InvalidData(
                "audio offset is smaller than FLAC header size, file may be corrupt".to_string(),
            )
        })?;

        // Calculate content (audio data) size for padding calculation
        let file_size = file.seek(SeekFrom::End(0))?;
        let content_size = file_size.checked_sub(audio_offset).ok_or_else(|| {
            AudexError::InvalidData(
                "file size is smaller than audio offset, file may be truncated".to_string(),
            )
        })? as usize;

        // Generate new metadata, or return early if nothing changed
        let new_metadata = match self.prepare_metadata(padding_func, available, content_size)? {
            Some(m) => m,
            None => return Ok(()), // no changes
        };

        let data_size = new_metadata.len() as u64;

        // Resize the metadata region in-place and keep track of the new logical
        // end so writer-based callers can observe the correct boundary.
        let logical_end = Self::resize_metadata_region(file, available, data_size, header_size)?;

        // Write "fLaC" marker + new metadata
        file.seek(SeekFrom::Start(0))?;
        file.write_all(b"fLaC")?;
        file.write_all(&new_metadata)?;
        file.seek(SeekFrom::Start(logical_end))?;

        // Store new metadata as original and reset dirty flag
        self.original_metadata = new_metadata;
        self.dirty = false;

        Ok(())
    }

    /// Prepare new metadata blocks for saving.
    ///
    /// Returns `Some(bytes)` with the serialised metadata if there are changes
    /// to write, or `None` if the metadata is unchanged and the save can be
    /// skipped.
    fn prepare_metadata(
        &mut self,
        padding_func: Option<PaddingFunction>,
        available: u64,
        content_size: usize,
    ) -> Result<Option<Vec<u8>>> {
        if padding_func.is_none()
            && !self.original_metadata.is_empty()
            && self.original_metadata.len() as u64 == available
        {
            // Try to regenerate and see if it matches original
            let regenerated =
                self.generate_metadata_blocks(None, available as usize, content_size)?;
            if regenerated == self.original_metadata {
                // No changes detected, skip write
                self.dirty = false;
                return Ok(None);
            }
            Ok(Some(regenerated))
        } else {
            // Need to regenerate with custom padding or size changed
            let regenerated =
                self.generate_metadata_blocks(padding_func, available as usize, content_size)?;
            Ok(Some(regenerated))
        }
    }

    /// Shift audio data and resize the metadata region between the `fLaC`
    /// header and the first audio frame.
    ///
    /// This operates on a `dyn ReadWriteSeek` trait object so it can be
    /// used by the writer-based save path without requiring a `'static`
    /// bound (unlike `resize_bytes` from `util`).
    ///
    /// Returns the new logical end of the file after resizing. When the
    /// metadata shrinks, stale trailing bytes are scrubbed or truncated via
    /// the shared writer helper before the value is returned.
    fn resize_metadata_region(
        file: &mut dyn crate::ReadWriteSeek,
        old_size: u64,
        new_size: u64,
        offset: u64,
    ) -> Result<u64> {
        if old_size == new_size {
            return file.seek(SeekFrom::End(0)).map_err(Into::into);
        }

        let file_size = file.seek(SeekFrom::End(0))?;
        let buffer_size: usize = 64 * 1024;

        if new_size > old_size {
            // Metadata grew -- shift audio data to the right to make room.
            let grow = new_size - old_size;
            let src_start = offset + old_size; // where audio currently starts
            let bytes_to_move = file_size - src_start;

            // Extend the stream by writing zeroes at the end.
            file.seek(SeekFrom::End(0))?;
            let mut remaining = grow;
            let zero_buf = vec![0u8; buffer_size];
            while remaining > 0 {
                let chunk = std::cmp::min(remaining, buffer_size as u64) as usize;
                file.write_all(&zero_buf[..chunk])?;
                remaining -= chunk as u64;
            }

            // Move data from right to left (reverse order to avoid overlap
            // corruption).
            if bytes_to_move > 0 {
                let mut pos = bytes_to_move;
                let mut buf = vec![0u8; buffer_size];
                while pos > 0 {
                    let chunk = std::cmp::min(pos, buffer_size as u64) as usize;
                    let read_offset = src_start + pos - chunk as u64;
                    let write_offset = read_offset + grow;

                    file.seek(SeekFrom::Start(read_offset))?;
                    file.read_exact(&mut buf[..chunk])?;
                    file.seek(SeekFrom::Start(write_offset))?;
                    file.write_all(&buf[..chunk])?;

                    pos -= chunk as u64;
                }
            }
            let new_total = file_size + (new_size - old_size);
            file.flush()?;
            Ok(new_total)
        } else {
            // Metadata shrank -- shift audio data to the left.
            let src_start = offset + old_size; // where audio currently starts
            let dst_start = offset + new_size; // where audio should go
            let bytes_to_move = file_size - src_start;

            // Move data left in forward order.
            let mut moved = 0u64;
            let mut buf = vec![0u8; buffer_size];
            while moved < bytes_to_move {
                let chunk = std::cmp::min(bytes_to_move - moved, buffer_size as u64) as usize;
                file.seek(SeekFrom::Start(src_start + moved))?;
                file.read_exact(&mut buf[..chunk])?;
                file.seek(SeekFrom::Start(dst_start + moved))?;
                file.write_all(&buf[..chunk])?;
                moved += chunk as u64;
            }

            // Scrub or truncate any stale trailing bytes left behind by the
            // left-shift. Trait-object writers cannot always be physically
            // truncated, so we use the shared fallback helper.
            let new_total = file_size - (old_size - new_size);
            crate::util::truncate_writer_dyn(file, new_total)?;
            file.flush()?;
            Ok(new_total)
        }
    }

    /// Find where audio data starts by reading from file handle
    fn find_audio_offset_from_file<F: Read + Seek + ?Sized>(&self, file: &mut F) -> Result<u64> {
        file.seek(SeekFrom::Start(0))?;

        // Check for ID3 tags first
        let mut signature = [0u8; 4];
        file.read_exact(&mut signature)?;

        if &signature[..3] == b"ID3" {
            // Skip ID3v2 tag
            let mut id3_size_bytes = [0u8; 6];
            file.read_exact(&mut id3_size_bytes)?;
            let id3_size = self.decode_id3_size(&id3_size_bytes[2..])?;
            file.seek(SeekFrom::Current(id3_size as i64))?;

            // Read FLAC signature after ID3
            file.read_exact(&mut signature)?;
        }

        if &signature != b"fLaC" {
            return Err(AudexError::FLACNoHeader);
        }

        // Cap the number of metadata blocks to prevent infinite loops on
        // crafted files where the is_last flag is never set and block
        // sizes are zero.
        const MAX_METADATA_BLOCKS: usize = 1024;
        let mut block_count: usize = 0;

        loop {
            block_count += 1;
            if block_count > MAX_METADATA_BLOCKS {
                return Err(AudexError::InvalidData(format!(
                    "Exceeded maximum metadata block count ({}) in audio offset search",
                    MAX_METADATA_BLOCKS
                )));
            }

            // Read block header (4 bytes)
            let mut header = [0u8; 4];
            match file.read_exact(&mut header) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => return Err(e.into()),
            }

            let is_last = (header[0] & 0x80) != 0;
            let block_type = header[0] & 0x7F;
            let block_size = u32::from_be_bytes([0, header[1], header[2], header[3]]) as u64;

            if block_type == 4 {
                // Vorbis comment: read actual content to find true size
                Self::skip_vorbis_comment_content(file)?;
            } else if block_type == 6 {
                // Picture: read actual content to find true size
                Self::skip_picture_content(file)?;
            } else {
                file.seek(SeekFrom::Current(block_size as i64))?;
            }

            if is_last {
                break;
            }
        }

        Ok(file.stream_position()?)
    }

    /// Skip past a Vorbis Comment block by reading its actual content
    /// (not trusting the header size field)
    fn skip_vorbis_comment_content<F: Read + Seek + ?Sized>(file: &mut F) -> Result<()> {
        let cur = file.stream_position()?;
        let file_end = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(cur))?;

        let mut len_buf = [0u8; 4];

        // vendor string length (4 bytes LE) + vendor string
        file.read_exact(&mut len_buf)?;
        let vendor_len = u32::from_le_bytes(len_buf) as u64;
        if file.stream_position()? + vendor_len > file_end {
            return Err(AudexError::InvalidData(
                "Vorbis vendor length exceeds file size".to_string(),
            ));
        }
        file.seek(SeekFrom::Current(vendor_len as i64))?;

        // comment count (4 bytes LE)
        file.read_exact(&mut len_buf)?;
        let count = u32::from_le_bytes(len_buf);
        if count > 100_000 {
            return Err(AudexError::InvalidData(format!(
                "Vorbis comment count {} too large",
                count
            )));
        }

        // each comment: length (4 bytes LE) + data
        for _ in 0..count {
            file.read_exact(&mut len_buf)?;
            let comment_len = u32::from_le_bytes(len_buf) as u64;
            if file.stream_position()? + comment_len > file_end {
                return Err(AudexError::InvalidData(
                    "Vorbis comment length exceeds file size".to_string(),
                ));
            }
            file.seek(SeekFrom::Current(comment_len as i64))?;
        }

        Ok(())
    }

    /// Skip past a Picture block by reading its actual content
    /// (not trusting the header size field)
    fn skip_picture_content<F: Read + Seek + ?Sized>(file: &mut F) -> Result<()> {
        let cur = file.stream_position()?;
        let file_end = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(cur))?;

        let mut buf4 = [0u8; 4];

        // picture type (4 bytes BE)
        file.read_exact(&mut buf4)?;

        // MIME type: length (4 bytes BE) + data
        file.read_exact(&mut buf4)?;
        let mime_len = u32::from_be_bytes(buf4) as u64;
        if file.stream_position()? + mime_len > file_end {
            return Err(AudexError::InvalidData(
                "Picture MIME length exceeds file size".to_string(),
            ));
        }
        file.seek(SeekFrom::Current(mime_len as i64))?;

        // description: length (4 bytes BE) + data
        file.read_exact(&mut buf4)?;
        let desc_len = u32::from_be_bytes(buf4) as u64;
        if file.stream_position()? + desc_len > file_end {
            return Err(AudexError::InvalidData(
                "Picture description length exceeds file size".to_string(),
            ));
        }
        file.seek(SeekFrom::Current(desc_len as i64))?;

        // width (4) + height (4) + depth (4) + colors (4)
        file.seek(SeekFrom::Current(16))?;

        // picture data: length (4 bytes BE) + data
        file.read_exact(&mut buf4)?;
        let data_len = u32::from_be_bytes(buf4) as u64;
        if file.stream_position()? + data_len > file_end {
            return Err(AudexError::InvalidData(
                "Picture data length exceeds file size".to_string(),
            ));
        }
        file.seek(SeekFrom::Current(data_len as i64))?;

        Ok(())
    }

    /// Generate new metadata blocks as a single byte vector
    fn generate_metadata_blocks(
        &mut self,
        padding_func: Option<PaddingFunction>,
        available: usize,
        content_size: usize,
    ) -> Result<Vec<u8>> {
        // Clear any previous overflow size tracking
        self.invalid_overflow_size.clear();

        // Collect metadata blocks to write
        let mut blocks = Vec::new();

        // Always include StreamInfo first
        let streaminfo_data = self.info.to_bytes()?;
        self.add_block_with_overflow_check(&mut blocks, 0, streaminfo_data)?;

        // Track which blocks we've written to avoid duplicates
        let mut written_blocks = std::collections::HashSet::new();
        written_blocks.insert(0u8); // STREAMINFO already written

        // Clone data before the loop to avoid borrow issues
        let application_blocks = self.application_blocks.clone();
        let pictures = self.pictures.clone();
        let metadata_blocks = self.metadata_blocks.clone();
        let original_block_order = self.original_block_order.clone();

        // Use original block order if available, otherwise use default order
        if !original_block_order.is_empty() {
            // Write blocks in original order
            for &block_type in &original_block_order {
                // Skip STREAMINFO (already written) and blocks we've already added
                if block_type == 0 || written_blocks.contains(&block_type) {
                    continue;
                }

                match block_type {
                    1 => {
                        // PADDING - skip for now, will be handled at the end
                        continue;
                    }
                    2 => {
                        // APPLICATION - add all application blocks
                        if !written_blocks.contains(&2) {
                            for app_block in &application_blocks {
                                let app_data = app_block.to_bytes()?;
                                self.add_block_with_overflow_check(&mut blocks, 2, app_data)?;
                            }
                            written_blocks.insert(2);
                        }
                    }
                    3 => {
                        // SEEKTABLE
                        if let Some(ref seektable) = self.seektable {
                            let seek_data = seektable.to_bytes()?;
                            self.add_block_with_overflow_check(&mut blocks, 3, seek_data)?;
                            written_blocks.insert(3);
                        }
                    }
                    4 => {
                        // VORBIS_COMMENT
                        if let Some(ref tags) = self.tags {
                            let mut comment_to_write = tags.clone();
                            if !comment_to_write.keys().is_empty() {
                                comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
                            }
                            let vorbis_data = comment_to_write.to_bytes()?;
                            self.add_block_with_overflow_check(&mut blocks, 4, vorbis_data)?;
                            written_blocks.insert(4);
                        }
                    }
                    5 => {
                        // CUESHEET
                        if let Some(ref cuesheet) = self.cuesheet {
                            let cue_data = cuesheet.to_bytes()?;
                            self.add_block_with_overflow_check(&mut blocks, 5, cue_data)?;
                            written_blocks.insert(5);
                        }
                    }
                    6 => {
                        // PICTURE - add all pictures
                        if !written_blocks.contains(&6) {
                            for picture in &pictures {
                                let pic_data = picture.to_bytes()?;
                                let override_size = self.validate_picture_size(&pic_data)?;
                                let mut block = MetadataBlock::new(6, pic_data);
                                block.override_header_size = override_size;
                                blocks.push(block);
                            }
                            written_blocks.insert(6);
                        }
                    }
                    _ => {
                        // Unknown block type - add from metadata_blocks
                        for metadata_block in &metadata_blocks {
                            if metadata_block.block_type == block_type {
                                let block_data = metadata_block.data.clone();
                                self.add_block_with_overflow_check(
                                    &mut blocks,
                                    block_type,
                                    block_data,
                                )?;
                            }
                        }
                        written_blocks.insert(block_type);
                    }
                }
            }
        }

        // Add any blocks not yet written (handles both default order and
        // blocks added after loading that weren't in the original order)
        if !written_blocks.contains(&4) {
            if let Some(ref tags) = self.tags {
                let mut comment_to_write = tags.clone();
                if !comment_to_write.keys().is_empty() {
                    comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
                }
                let vorbis_data = comment_to_write.to_bytes()?;
                self.add_block_with_overflow_check(&mut blocks, 4, vorbis_data)?;
                written_blocks.insert(4);
            }
        }

        if !written_blocks.contains(&3) {
            if let Some(ref seektable) = self.seektable {
                let seek_data = seektable.to_bytes()?;
                self.add_block_with_overflow_check(&mut blocks, 3, seek_data)?;
                written_blocks.insert(3);
            }
        }

        if !written_blocks.contains(&2) {
            for app_block in &application_blocks {
                let app_data = app_block.to_bytes()?;
                self.add_block_with_overflow_check(&mut blocks, 2, app_data)?;
            }
            written_blocks.insert(2);
        }

        if !written_blocks.contains(&5) {
            if let Some(ref cuesheet) = self.cuesheet {
                let cue_data = cuesheet.to_bytes()?;
                self.add_block_with_overflow_check(&mut blocks, 5, cue_data)?;
                written_blocks.insert(5);
            }
        }

        if !written_blocks.contains(&6) {
            for picture in &pictures {
                let pic_data = picture.to_bytes()?;
                let override_size = self.validate_picture_size(&pic_data)?;
                let mut block = MetadataBlock::new(6, pic_data);
                block.override_header_size = override_size;
                blocks.push(block);
            }
            written_blocks.insert(6);
        }

        // Add other metadata blocks not yet written
        for metadata_block in &metadata_blocks {
            if matches!(metadata_block.block_type, 0..=6) {
                continue;
            }
            if !written_blocks.contains(&metadata_block.block_type) {
                let block_data = metadata_block.data.clone();
                self.add_block_with_overflow_check(
                    &mut blocks,
                    metadata_block.block_type,
                    block_data,
                )?;
                written_blocks.insert(metadata_block.block_type);
            }
        }

        // Always recalculate padding:
        // strip all existing padding and add a single new padding block
        {
            let padding_size = self.calculate_padding_size_for_generation(
                available,
                content_size,
                &blocks,
                padding_func,
            )?;

            if padding_size > 0 {
                // Handle very large padding blocks by splitting if necessary
                self.add_padding_blocks(&mut blocks, padding_size)?;
            }
        }

        // Convert blocks to byte vector
        let mut metadata_bytes = Vec::new();
        for (i, block) in blocks.iter().enumerate() {
            let is_last = i == blocks.len() - 1;

            // Validate block type before writing (0-126 valid; 127 is
            // reserved, >= 128 collides with the is-last flag bit)
            if block.block_type >= 127 {
                return Err(AudexError::InvalidData(format!(
                    "FLAC block type {} is out of valid range (0-126)",
                    block.block_type
                )));
            }

            // Write block header
            let header_byte = block.block_type | if is_last { 0x80 } else { 0x00 };
            metadata_bytes.push(header_byte);

            // Write block size (24 bits), using override if present
            let size = block
                .override_header_size
                .unwrap_or(block.data.len() as u32);
            let size_bytes = size.to_be_bytes();
            metadata_bytes.extend_from_slice(&size_bytes[1..]);

            // Write block data
            metadata_bytes.extend_from_slice(&block.data);
        }

        Ok(metadata_bytes)
    }

    /// Calculate padding size for metadata generation
    fn calculate_padding_size_for_generation(
        &self,
        available: usize,
        content_size: usize,
        blocks: &[MetadataBlock],
        padding_func: Option<PaddingFunction>,
    ) -> Result<usize> {
        // Calculate size of all metadata blocks
        let mut blockssize: usize = blocks.iter().map(|b| 4 + b.data.len()).sum();

        // Take the padding overhead into account. We always add one to make things simple.
        // This adds the overhead of one padding block header (4 bytes)
        blockssize += 4;

        // Calculate padding space
        let padding_space = (available as i64) - (blockssize as i64);
        let cont_size = content_size as i64;

        // Create PaddingInfo
        let info = PaddingInfo::new(padding_space, cont_size);

        // Get padding using standard algorithm
        let padding_result = info.get_padding_with(padding_func);

        // Apply max size limit as per FLAC specification
        let padding_size = std::cmp::min(padding_result, Picture::MAX_SIZE as i64);

        Ok(padding_size.max(0) as usize)
    }

    /// Validate picture block size with tolerance.
    ///
    /// Returns `Some(original_size)` if the block was already oversized on
    /// load and should use the original header size; returns `None` if the
    /// block fits within spec limits.
    fn validate_picture_size(&mut self, pic_data: &[u8]) -> Result<Option<u32>> {
        const MAX_BLOCK_SIZE: usize = 0xFFFFFF; // 24-bit limit

        if pic_data.len() > MAX_BLOCK_SIZE {
            // If the picture block was already oversized when loaded, allow
            // the round-trip with the original header size.
            if let Some(&original_size) = self.original_overflow_sizes.get(&6u8) {
                self.invalid_overflow_size.push((6, pic_data.len()));
                return Ok(Some(original_size));
            }

            // Newly oversized -- error out.
            self.invalid_overflow_size.push((6, pic_data.len()));
            return Err(AudexError::InvalidData(format!(
                "Picture block too large: {} bytes (max: {} bytes)",
                pic_data.len(),
                MAX_BLOCK_SIZE
            )));
        }

        Ok(None)
    }

    /// Add a metadata block with overflow size checking.
    ///
    /// If the block data exceeds the 24-bit size limit (0xFFFFFF) but we have
    /// an original overflow size recorded from loading (i.e. the block was
    /// already oversized on disk), we allow the write and attach the original
    /// header size so the writer can emit it. This preserves the file's
    /// existing brokenness rather than refusing to save.
    fn add_block_with_overflow_check(
        &mut self,
        blocks: &mut Vec<MetadataBlock>,
        block_type: u8,
        data: Vec<u8>,
    ) -> Result<()> {
        const MAX_BLOCK_SIZE: usize = 0xFFFFFF; // 24-bit limit

        if data.len() > MAX_BLOCK_SIZE {
            // Check if this block type was already oversized when we loaded the
            // file. If so, we round-trip the original (wrong) header size
            // instead of erroring.
            if let Some(&original_size) = self.original_overflow_sizes.get(&block_type) {
                self.invalid_overflow_size.push((block_type, data.len()));
                let mut block = MetadataBlock::new(block_type, data);
                block.override_header_size = Some(original_size);
                blocks.push(block);
                return Ok(());
            }

            // Newly oversized block -- error out.
            self.invalid_overflow_size.push((block_type, data.len()));
            return Err(AudexError::InvalidData(format!(
                "Block type {} too large: {} bytes (max: {} bytes)",
                block_type,
                data.len(),
                MAX_BLOCK_SIZE
            )));
        }

        blocks.push(MetadataBlock::new(block_type, data));
        Ok(())
    }

    /// Add padding blocks, splitting large ones if necessary
    fn add_padding_blocks(
        &mut self,
        blocks: &mut Vec<MetadataBlock>,
        total_padding: usize,
    ) -> Result<()> {
        const MAX_PADDING_SIZE: usize = 0xFFFFFF; // 24-bit limit

        if total_padding == 0 {
            return Ok(());
        }

        if total_padding <= MAX_PADDING_SIZE {
            // Single padding block
            let padding_data = vec![0u8; total_padding];
            blocks.push(MetadataBlock::new(1, padding_data));
        } else {
            // Split into multiple padding blocks
            let mut remaining = total_padding;

            while remaining > 0 {
                let chunk_size = remaining.min(MAX_PADDING_SIZE);
                let padding_data = vec![0u8; chunk_size];
                blocks.push(MetadataBlock::new(1, padding_data));
                remaining -= chunk_size;
            }
        }

        Ok(())
    }

    /// Load FLAC file asynchronously with non-blocking I/O.
    ///
    /// This method provides the same functionality as `from_file()` but uses
    /// async I/O operations suitable for use in async runtimes like Tokio.
    ///
    /// # Arguments
    /// * `path` - Path to the FLAC file to load
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded FLAC file with metadata
    /// * `Err(AudexError)` - Error occurred during file loading or parsing
    #[cfg(feature = "async")]
    pub async fn from_file_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file_with_options_async(path, FLACParseOptions::default()).await
    }

    /// Load FLAC file asynchronously with custom parsing options.
    ///
    /// Allows fine-tuned control over parsing behavior such as error handling
    /// and block size validation when loading files asynchronously.
    ///
    /// # Arguments
    /// * `path` - Path to the FLAC file to load
    /// * `options` - Custom parsing options for robustness control
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded FLAC file
    /// * `Err(AudexError)` - Error during loading or parsing
    #[cfg(feature = "async")]
    pub async fn from_file_with_options_async<P: AsRef<Path>>(
        path: P,
        options: FLACParseOptions,
    ) -> Result<Self> {
        let path = path.as_ref();
        let file = TokioFile::open(path).await?;
        let mut flac = Self::with_options(options);
        flac.filename = path.to_string_lossy().to_string();

        // Use streaming parsing for async operations
        let mut reader = TokioBufReader::new(file);
        flac.parse_flac_streaming_async(&mut reader).await?;

        Ok(flac)
    }

    /// Convenience alias for from_file_async to match common API patterns.
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file_async(path).await
    }

    /// Parse FLAC file structure with streaming async I/O.
    ///
    /// Internal method that handles the actual parsing of FLAC metadata blocks
    /// using non-blocking I/O operations.
    #[cfg(feature = "async")]
    async fn parse_flac_streaming_async<R>(&mut self, reader: &mut TokioBufReader<R>) -> Result<()>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        // Check for ID3 tags that may precede FLAC data
        let mut signature = [0u8; 4];
        reader.read_exact(&mut signature).await?;

        if &signature[..3] == b"ID3" {
            // Skip ID3v2 tag to find FLAC signature
            let mut id3_size_bytes = [0u8; 6];
            reader.read_exact(&mut id3_size_bytes).await?;
            let id3_size = self.decode_id3_size(&id3_size_bytes[2..])?;
            reader.seek(SeekFrom::Current(id3_size as i64)).await?;

            // Read FLAC signature after ID3 tag
            reader.read_exact(&mut signature).await?;
        }

        if &signature != b"fLaC" {
            let error = FLACError {
                kind: FLACErrorKind::InvalidHeader,
                position: reader.stream_position().await.ok(),
                context: "Missing or invalid FLAC signature".to_string(),
            };
            self.parse_errors.push(error);
            return Err(AudexError::FLACNoHeader);
        }

        // Store position after fLaC header for metadata capture
        let metadata_start = reader.stream_position().await?;

        // Parse all metadata blocks
        self.parse_metadata_blocks_streaming_async(reader).await?;

        // Store position after metadata blocks (before audio data)
        let metadata_end = reader.stream_position().await?;

        // Capture original metadata for change detection during save
        // Cap to actual file size to prevent OOM from crafted size fields
        let file_end = reader.seek(SeekFrom::End(0)).await?;
        let capped_end = metadata_end.min(file_end);
        // Guard against underflow when metadata_start exceeds the computed end
        // position (can happen with truncated or malformed files)
        let metadata_size_u64 = capped_end.checked_sub(metadata_start).ok_or_else(|| {
            AudexError::InvalidData("metadata region extends beyond file boundaries".to_string())
        })?;
        let metadata_size = usize::try_from(metadata_size_u64).map_err(|_| {
            AudexError::InvalidData("metadata region too large for this platform".to_string())
        })?;
        reader.seek(SeekFrom::Start(metadata_start)).await?;
        self.original_metadata = vec![0u8; metadata_size];

        // Use read_exact for byte-identical capture, tolerate EOF for truncated files
        match reader.read_exact(&mut self.original_metadata).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Truncated file - clear original_metadata to skip comparison
                self.original_metadata.clear();
            }
            Err(e) => return Err(e.into()),
        }

        // Calculate accurate bitrate from audio stream size
        // Use metadata_end (already computed) as the audio start position
        // instead of stream_position() which can be unreliable with BufReader
        if self.info.total_samples > 0 {
            if let Ok(end_pos) = reader.seek(SeekFrom::End(0)).await {
                if let Some(duration) = self.info.length {
                    if end_pos >= metadata_end {
                        let audio_size = end_pos - metadata_end;
                        let duration_secs = duration.as_secs_f64();
                        if duration_secs > 0.0 {
                            let bitrate = (audio_size * 8) as f64 / duration_secs;
                            self.info.bitrate = Some(bitrate as u32);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse FLAC metadata blocks with streaming async I/O.
    ///
    /// Handles individual block parsing with error recovery based on parse options.
    #[cfg(feature = "async")]
    async fn parse_metadata_blocks_streaming_async<R>(
        &mut self,
        reader: &mut TokioBufReader<R>,
    ) -> Result<()>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let mut is_last = false;
        let mut vorbis_comment_count = 0;
        // Cap the number of metadata blocks to prevent excessive iteration
        // on crafted files where the is_last flag is never set.
        const MAX_METADATA_BLOCKS: usize = 1024;
        let mut block_count: usize = 0;

        while !is_last {
            block_count += 1;
            if block_count > MAX_METADATA_BLOCKS {
                return Err(AudexError::InvalidData(format!(
                    "Exceeded maximum metadata block count ({})",
                    MAX_METADATA_BLOCKS
                )));
            }
            let block_start_pos = reader.stream_position().await?;

            // Read 4-byte block header
            let mut header = [0u8; 4];
            if let Err(e) = reader.read_exact(&mut header).await {
                if self.parse_options.ignore_errors {
                    self.parse_errors.push(FLACError {
                        kind: FLACErrorKind::CorruptedBlock,
                        position: Some(block_start_pos),
                        context: format!("Failed to read block header: {}", e),
                    });
                    break;
                }
                return Err(e.into());
            }

            let block_type = header[0] & 0x7F;
            is_last = (header[0] & 0x80) != 0;
            let block_size = u32::from_be_bytes([0, header[1], header[2], header[3]]);

            // Track block order for byte-identical writes
            self.original_block_order.push(block_type);

            let block_data_start = reader.stream_position().await?;

            // Validate block size — always enforce the limit regardless of
            // distrust_size to prevent uncapped allocations from crafted files
            if block_size > self.parse_options.max_block_size {
                let error = FLACError {
                    kind: FLACErrorKind::BlockSizeError,
                    position: Some(block_start_pos),
                    context: format!(
                        "Block size {} exceeds maximum {}",
                        block_size, self.parse_options.max_block_size
                    ),
                };
                self.parse_errors.push(error);

                // VORBIS_COMMENT block size errors are always fatal to prevent OOM
                if block_type == 4 || !self.parse_options.ignore_errors {
                    return Err(AudexError::InvalidData(format!(
                        "Block size {} exceeds maximum {}",
                        block_size, self.parse_options.max_block_size
                    )));
                }

                // Skip oversized block for non-critical types
                let safe_skip_size = min(block_size, self.parse_options.max_block_size);
                reader
                    .seek(SeekFrom::Current(safe_skip_size as i64))
                    .await?;
                continue;
            }

            // Parse block based on type
            let parse_result = match block_type {
                0 => self.parse_streaminfo_block_async(reader, block_size).await,
                1 => self.parse_padding_block_async(reader, block_size).await,
                2 => self.parse_application_block_async(reader, block_size).await,
                3 => {
                    // SEEKTABLE block - check for duplicates
                    if self.seektable.is_some() {
                        self.parse_errors.push(FLACError {
                            kind: FLACErrorKind::MultipleSeekTableBlocks,
                            position: Some(block_start_pos),
                            context: "> 1 SeekTable block found".to_string(),
                        });

                        if !self.parse_options.ignore_errors {
                            return Err(AudexError::InvalidData(
                                "> 1 SeekTable block found".to_string(),
                            ));
                        }

                        // Skip duplicate SeekTable block
                        reader.seek(SeekFrom::Current(block_size as i64)).await?;
                        Ok(())
                    } else {
                        self.parse_seektable_block_async(reader, block_size).await
                    }
                }
                4 => {
                    // Handle VORBIS_COMMENT block with duplicate detection
                    vorbis_comment_count += 1;
                    if vorbis_comment_count > 1 {
                        self.parse_errors.push(FLACError {
                            kind: FLACErrorKind::MultipleVorbisBlocks,
                            position: Some(block_start_pos),
                            context: format!(
                                "Found {} VORBIS_COMMENT blocks, using first",
                                vorbis_comment_count
                            ),
                        });

                        if !self.parse_options.ignore_errors {
                            return Err(AudexError::FLACVorbis);
                        }

                        // Skip duplicate Vorbis block
                        reader.seek(SeekFrom::Current(block_size as i64)).await?;
                        Ok(())
                    } else {
                        self.parse_vorbis_comment_block_async(reader, block_size)
                            .await
                    }
                }
                5 => {
                    // CUESHEET block - check for duplicates
                    if self.cuesheet.is_some() {
                        self.parse_errors.push(FLACError {
                            kind: FLACErrorKind::MultipleCueSheetBlocks,
                            position: Some(block_start_pos),
                            context: "> 1 CueSheet block found".to_string(),
                        });

                        if !self.parse_options.ignore_errors {
                            return Err(AudexError::InvalidData(
                                "> 1 CueSheet block found".to_string(),
                            ));
                        }

                        // Skip duplicate CueSheet block
                        reader.seek(SeekFrom::Current(block_size as i64)).await?;
                        Ok(())
                    } else {
                        self.parse_cuesheet_block_async(reader, block_size).await
                    }
                }
                6 => self.parse_picture_block_async(reader, block_size).await,
                _ => {
                    // Unknown block type - skip it
                    reader.seek(SeekFrom::Current(block_size as i64)).await?;
                    Ok(())
                }
            };

            // Handle parse errors with recovery logic
            if let Err(e) = parse_result {
                let error = FLACError {
                    kind: FLACErrorKind::CorruptedBlock,
                    position: Some(block_start_pos),
                    context: format!("Block type {} parse error: {}", block_type, e),
                };
                self.parse_errors.push(error);

                // Determine if error is fatal
                let is_fatal_error = if self.parse_options.ignore_errors {
                    block_type == 4 && self.is_fatal_vorbis_error(&e)
                } else {
                    block_type == 0 || (block_type == 4 && self.is_fatal_vorbis_error(&e))
                };

                if is_fatal_error {
                    return Err(e);
                }

                // Try to skip corrupted block and continue
                let current_pos = reader.stream_position().await?;
                let expected_pos = block_start_pos + 4 + block_size as u64;
                if current_pos != expected_pos {
                    reader.seek(SeekFrom::Start(expected_pos)).await?;
                }
            } else {
                // Store successfully parsed block data
                let current_pos = reader.stream_position().await?;
                let bytes_to_read = current_pos.checked_sub(block_data_start).ok_or_else(|| {
                    AudexError::InvalidData(format!(
                        "FLAC block position underflow: current {} < start {}",
                        current_pos, block_data_start
                    ))
                })? as usize;

                // For VorbisComment (4) and Picture (6) blocks: if the real
                // content size exceeds the 24-bit max, record the original
                // header size so we can write it back on save (distrust_size
                // round-trip).
                if (block_type == 4 || block_type == 6) && bytes_to_read > 0xFFFFFF {
                    self.original_overflow_sizes.insert(block_type, block_size);
                }

                reader.seek(SeekFrom::Start(block_data_start)).await?;
                let mut block_data = vec![0u8; bytes_to_read];
                reader.read_exact(&mut block_data).await?;

                self.metadata_blocks
                    .push(MetadataBlock::new(block_type, block_data));
            }
        }

        Ok(())
    }

    /// Parse STREAMINFO metadata block asynchronously.
    #[cfg(feature = "async")]
    async fn parse_streaminfo_block_async<R>(
        &mut self,
        reader: &mut TokioBufReader<R>,
        block_size: u32,
    ) -> Result<()>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    {
        use tokio::io::AsyncReadExt;

        if block_size != 34 {
            return Err(AudexError::InvalidData(format!(
                "Invalid STREAMINFO size: {} (expected 34)",
                block_size
            )));
        }

        let mut data = [0u8; 34];
        reader.read_exact(&mut data).await?;
        self.info = FLACStreamInfo::from_bytes(&data)?;

        Ok(())
    }

    /// Parse PADDING metadata block asynchronously.
    #[cfg(feature = "async")]
    async fn parse_padding_block_async<R>(
        &mut self,
        reader: &mut TokioBufReader<R>,
        block_size: u32,
    ) -> Result<()>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    {
        use tokio::io::AsyncSeekExt;

        // Record padding size without reading actual null bytes
        let padding = Padding::new(block_size as usize);
        self.padding_blocks.push(padding);
        reader.seek(SeekFrom::Current(block_size as i64)).await?;
        Ok(())
    }

    /// Parse APPLICATION metadata block asynchronously.
    #[cfg(feature = "async")]
    async fn parse_application_block_async<R>(
        &mut self,
        reader: &mut TokioBufReader<R>,
        block_size: u32,
    ) -> Result<()>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let safe_size = if self.parse_options.distrust_size {
            min(block_size, self.parse_options.max_block_size)
        } else {
            block_size
        };

        let mut data = vec![0u8; safe_size as usize];
        reader.read_exact(&mut data).await?;

        let application_block = ApplicationBlock::from_bytes(&data)?;
        self.application_blocks.push(application_block);

        // Skip any remaining bytes if truncated
        if block_size > safe_size {
            reader
                .seek(SeekFrom::Current((block_size - safe_size) as i64))
                .await?;
        }

        Ok(())
    }

    /// Parse SEEKTABLE metadata block asynchronously.
    #[cfg(feature = "async")]
    async fn parse_seektable_block_async<R>(
        &mut self,
        reader: &mut TokioBufReader<R>,
        block_size: u32,
    ) -> Result<()>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let safe_size = if self.parse_options.distrust_size {
            min(block_size, self.parse_options.max_block_size)
        } else {
            block_size
        };

        let mut data = vec![0u8; safe_size as usize];
        let bytes_read = reader.read(&mut data).await?;

        if bytes_read < safe_size as usize {
            return Err(AudexError::InvalidData(
                "Truncated SEEKTABLE block".to_string(),
            ));
        }

        data.truncate(bytes_read);

        // Use robust seek table parsing with reasonable limits
        let max_seekpoints = Some(10000);
        let seektable = SeekTable::from_bytes_with_options(&data, max_seekpoints)?;
        self.seektable = Some(seektable);

        // Skip any remaining bytes if truncated
        if block_size > safe_size {
            reader
                .seek(SeekFrom::Current((block_size - safe_size) as i64))
                .await?;
        }

        Ok(())
    }

    /// Parse VORBIS_COMMENT metadata block asynchronously.
    #[cfg(feature = "async")]
    async fn parse_vorbis_comment_block_async<R>(
        &mut self,
        reader: &mut TokioBufReader<R>,
        block_size: u32,
    ) -> Result<()>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let safe_size = if self.parse_options.distrust_size {
            min(block_size, self.parse_options.max_block_size)
        } else {
            block_size
        };

        let mut data = vec![0u8; safe_size as usize];
        reader.read_exact(&mut data).await?;

        // FLAC files don't use framing bit in Vorbis comments
        let comment = VCommentDict::from_bytes_with_options(
            &data,
            self.parse_options.vorbis_error_mode,
            false,
        )?;
        self.tags = Some(comment);

        // Skip any remaining bytes if block was larger
        if block_size > safe_size {
            reader
                .seek(SeekFrom::Current((block_size - safe_size) as i64))
                .await?;
        }

        Ok(())
    }

    /// Parse CUESHEET metadata block asynchronously.
    #[cfg(feature = "async")]
    async fn parse_cuesheet_block_async<R>(
        &mut self,
        reader: &mut TokioBufReader<R>,
        block_size: u32,
    ) -> Result<()>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let safe_size = if self.parse_options.distrust_size {
            min(block_size, self.parse_options.max_block_size)
        } else {
            block_size
        };

        let mut data = vec![0u8; safe_size as usize];
        let bytes_read = reader.read(&mut data).await?;

        if bytes_read < safe_size as usize {
            return Err(AudexError::InvalidData(
                "Truncated CUESHEET block".to_string(),
            ));
        }

        data.truncate(bytes_read);
        let cuesheet = CueSheet::from_bytes(&data)?;
        self.cuesheet = Some(cuesheet);

        // Skip any remaining bytes if truncated
        if block_size > safe_size {
            reader
                .seek(SeekFrom::Current((block_size - safe_size) as i64))
                .await?;
        }

        Ok(())
    }

    /// Parse PICTURE metadata block asynchronously.
    #[cfg(feature = "async")]
    async fn parse_picture_block_async<R>(
        &mut self,
        reader: &mut TokioBufReader<R>,
        block_size: u32,
    ) -> Result<()>
    where
        R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin,
    {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let safe_size = if self.parse_options.distrust_size {
            min(block_size, self.parse_options.max_block_size)
        } else {
            block_size
        };

        let mut data = vec![0u8; safe_size as usize];
        let bytes_read = reader.read(&mut data).await?;

        // Handle truncated data based on error settings
        if bytes_read < safe_size as usize {
            if !self.parse_options.ignore_errors {
                return Err(AudexError::InvalidData(
                    "Truncated PICTURE block".to_string(),
                ));
            }
            if bytes_read == 0 {
                return Ok(());
            }
        }

        data.truncate(bytes_read);

        // Use robust picture parsing with size limits
        let max_picture_size = Some(self.parse_options.max_block_size as usize);

        match Picture::from_bytes_with_options(&data, max_picture_size) {
            Ok(picture) => self.pictures.push(picture),
            Err(_) if self.parse_options.ignore_errors => {
                // Skip corrupted picture when ignore_errors is enabled
            }
            Err(e) => return Err(e),
        }

        // Skip any remaining bytes if truncated
        if block_size > safe_size && bytes_read == safe_size as usize {
            reader
                .seek(SeekFrom::Current((block_size - safe_size) as i64))
                .await?;
        }

        Ok(())
    }

    /// Save FLAC file with metadata changes asynchronously.
    ///
    /// Writes modified metadata back to the file using non-blocking I/O.
    /// Supports in-place updates when possible, or resizes the file if needed.
    ///
    /// # Arguments
    /// * `path` - Optional path to save to (defaults to original file path)
    /// * `_delete_id3` - Whether to remove ID3 tags (reserved for future use)
    /// * `padding_func` - Optional custom padding calculation function
    #[cfg(feature = "async")]
    pub async fn save_to_file_async<P: AsRef<Path>>(
        &mut self,
        path: Option<P>,
        _delete_id3: bool,
        padding_func: Option<PaddingFunction>,
    ) -> Result<()> {
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};

        let file_path = match path {
            Some(p) => p.as_ref().to_path_buf(),
            None => std::path::PathBuf::from(&self.filename),
        };

        if !tokio::fs::try_exists(&file_path).await.unwrap_or(false) {
            return Err(AudexError::InvalidData("File does not exist".to_string()));
        }

        // Open file for read/write operations
        let mut file = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)
            .await?;

        // Find where audio data starts in the file
        let audio_offset = self.find_audio_offset_async(&mut file).await?;

        // Calculate available space for metadata
        let header_size = 4u64; // "fLaC" signature
        let available = audio_offset.checked_sub(header_size).ok_or_else(|| {
            AudexError::InvalidData(
                "audio offset is smaller than FLAC header size, file may be corrupt".to_string(),
            )
        })?;

        // Calculate content (audio data) size for padding calculation
        let file_size = file.seek(SeekFrom::End(0)).await?;
        let content_size = file_size.checked_sub(audio_offset).ok_or_else(|| {
            AudexError::InvalidData(
                "file size is smaller than audio offset, file may be truncated".to_string(),
            )
        })? as usize;

        // Determine if we need to regenerate metadata or can skip unchanged data
        let (new_metadata, data_size) = if padding_func.is_none()
            && !self.original_metadata.is_empty()
            && self.original_metadata.len() as u64 == available
        {
            // Try to regenerate and compare with original
            let regenerated =
                self.generate_metadata_blocks(None, available as usize, content_size)?;
            if regenerated == self.original_metadata {
                // No changes detected - skip write for efficiency
                self.dirty = false;
                return Ok(());
            }
            let size = regenerated.len() as u64;
            (regenerated, size)
        } else {
            let regenerated =
                self.generate_metadata_blocks(padding_func, available as usize, content_size)?;
            let size = regenerated.len() as u64;
            (regenerated, size)
        };

        // Resize file in-place if metadata size changed
        resize_bytes_async(&mut file, available, data_size, header_size).await?;

        // Write FLAC signature and new metadata
        file.seek(SeekFrom::Start(0)).await?;
        file.write_all(b"fLaC").await?;
        file.write_all(&new_metadata).await?;

        // Ensure data is written to disk
        file.flush().await?;

        // Update state after successful save
        self.original_metadata = new_metadata;
        self.dirty = false;

        Ok(())
    }

    /// Save changes to the FLAC file asynchronously using default settings.
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        self.save_to_file_async::<&str>(None, false, None).await
    }

    /// Clear all tags from the FLAC file asynchronously.
    ///
    /// Removes the Vorbis comment block and saves the file.
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        if self.tags.is_some() {
            self.tags = None;
            self.dirty = true;
            self.save_async().await?;
        }
        Ok(())
    }

    /// Delete the FLAC file from disk asynchronously.
    #[cfg(feature = "async")]
    pub async fn delete_async(&mut self) -> Result<()> {
        if !self.filename.is_empty() {
            tokio::fs::remove_file(&self.filename).await?;
        }
        Ok(())
    }

    /// Find where audio data starts in the file asynchronously.
    ///
    /// Internal method that scans through metadata blocks to find audio offset.
    #[cfg(feature = "async")]
    async fn find_audio_offset_async(&self, file: &mut TokioFile) -> Result<u64> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        file.seek(SeekFrom::Start(0)).await?;

        // Check for ID3 tags first
        let mut signature = [0u8; 4];
        file.read_exact(&mut signature).await?;

        if &signature[..3] == b"ID3" {
            // Skip ID3v2 tag
            let mut id3_size_bytes = [0u8; 6];
            file.read_exact(&mut id3_size_bytes).await?;
            let id3_size = self.decode_id3_size(&id3_size_bytes[2..])?;
            file.seek(SeekFrom::Current(id3_size as i64)).await?;

            // Read FLAC signature after ID3
            file.read_exact(&mut signature).await?;
        }

        if &signature != b"fLaC" {
            return Err(AudexError::FLACNoHeader);
        }

        // Cap the number of metadata blocks to prevent crafted files from
        // spinning forever when the last-block flag is never set.
        const MAX_METADATA_BLOCKS: usize = 1024;
        let mut block_count = 0usize;

        // Scan through all metadata blocks to find audio start
        loop {
            block_count += 1;
            if block_count > MAX_METADATA_BLOCKS {
                return Err(AudexError::InvalidData(format!(
                    "Exceeded maximum metadata block count ({}) in audio offset search",
                    MAX_METADATA_BLOCKS
                )));
            }

            let mut header = [0u8; 4];
            file.read_exact(&mut header).await?;

            let is_last = (header[0] & 0x80) != 0;
            let block_type = header[0] & 0x7F;
            let block_size = u32::from_be_bytes([0, header[1], header[2], header[3]]) as u64;

            if block_type == 4 {
                // Vorbis comment: read actual content and validate every seek
                // against the real file size instead of trusting embedded lengths.
                Self::skip_vorbis_comment_content_async(file).await?;
            } else if block_type == 6 {
                // Picture: validate all internal lengths against the real file size.
                Self::skip_picture_content_async(file).await?;
            } else {
                file.seek(SeekFrom::Current(block_size as i64)).await?;
            }

            if is_last {
                break;
            }
        }

        Ok(file.stream_position().await?)
    }

    #[cfg(feature = "async")]
    async fn skip_vorbis_comment_content_async(file: &mut TokioFile) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let cur = file.stream_position().await?;
        let file_end = file.seek(SeekFrom::End(0)).await?;
        file.seek(SeekFrom::Start(cur)).await?;

        let mut len_buf = [0u8; 4];

        file.read_exact(&mut len_buf).await?;
        let vendor_len = u32::from_le_bytes(len_buf) as u64;
        if file.stream_position().await? + vendor_len > file_end {
            return Err(AudexError::InvalidData(
                "Vorbis vendor length exceeds file size".to_string(),
            ));
        }
        file.seek(SeekFrom::Current(vendor_len as i64)).await?;

        file.read_exact(&mut len_buf).await?;
        let count = u32::from_le_bytes(len_buf);
        if count > 100_000 {
            return Err(AudexError::InvalidData(format!(
                "Vorbis comment count {} too large",
                count
            )));
        }

        for _ in 0..count {
            file.read_exact(&mut len_buf).await?;
            let comment_len = u32::from_le_bytes(len_buf) as u64;
            if file.stream_position().await? + comment_len > file_end {
                return Err(AudexError::InvalidData(
                    "Vorbis comment length exceeds file size".to_string(),
                ));
            }
            file.seek(SeekFrom::Current(comment_len as i64)).await?;
        }

        Ok(())
    }

    #[cfg(feature = "async")]
    async fn skip_picture_content_async(file: &mut TokioFile) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let cur = file.stream_position().await?;
        let file_end = file.seek(SeekFrom::End(0)).await?;
        file.seek(SeekFrom::Start(cur)).await?;

        let mut buf4 = [0u8; 4];

        file.read_exact(&mut buf4).await?; // picture type

        file.read_exact(&mut buf4).await?;
        let mime_len = u32::from_be_bytes(buf4) as u64;
        if file.stream_position().await? + mime_len > file_end {
            return Err(AudexError::InvalidData(
                "Picture MIME length exceeds file size".to_string(),
            ));
        }
        file.seek(SeekFrom::Current(mime_len as i64)).await?;

        file.read_exact(&mut buf4).await?;
        let desc_len = u32::from_be_bytes(buf4) as u64;
        if file.stream_position().await? + desc_len > file_end {
            return Err(AudexError::InvalidData(
                "Picture description length exceeds file size".to_string(),
            ));
        }
        file.seek(SeekFrom::Current(desc_len as i64)).await?;

        if file.stream_position().await? + 16 > file_end {
            return Err(AudexError::InvalidData(
                "Picture dimensions exceed file size".to_string(),
            ));
        }
        file.seek(SeekFrom::Current(16)).await?;

        file.read_exact(&mut buf4).await?;
        let data_len = u32::from_be_bytes(buf4) as u64;
        if file.stream_position().await? + data_len > file_end {
            return Err(AudexError::InvalidData(
                "Picture data length exceeds file size".to_string(),
            ));
        }
        file.seek(SeekFrom::Current(data_len as i64)).await?;

        Ok(())
    }
}

impl Default for FLAC {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Picture {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for SeekTable {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for CueSheet {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for FLAC {
    type Tags = VCommentDict;
    type Info = FLACStreamInfo;

    fn format_id() -> &'static str {
        "FLAC"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file(path)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing FLAC file from reader");
        let mut flac = Self::new();
        let mut reader = reader;
        flac.parse_flac_streaming(&mut reader)?;
        Ok(flac)
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving FLAC metadata");
        self.save_to_file::<&str>(None, false, None)
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        self.save_to_writer_impl(writer, None)
    }

    fn clear(&mut self) -> Result<()> {
        if self.tags.is_some() {
            self.tags = None;
            self.dirty = true;
            self.save()?;
        }
        Ok(())
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        if self.tags.is_some() {
            self.tags = None;
            self.dirty = true;
        }
        self.save_to_writer_impl(writer, None)
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        self.save_to_file(Some(path), false, None)
    }

    /// Adds empty Vorbis comment block to the file.
    ///
    /// Creates a new empty tag structure if none exists. If tags already exist,
    /// returns an error (unless parse_options.ignore_errors is set).
    ///
    /// # Errors
    ///
    /// Returns `AudexError::FLACVorbis` if tags already exist and ignore_errors is false.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::flac::FLAC;
    /// use audex::FileType;
    ///
    /// let mut flac = FLAC::load("song.flac")?;
    /// if flac.tags.is_none() {
    ///     flac.add_tags()?;
    /// }
    /// flac.set("title", vec!["My Song".to_string()])?;
    /// flac.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() && !self.parse_options.ignore_errors {
            return Err(AudexError::FLACVorbis);
        }
        // With ignore_errors, replace existing tags
        self.tags = Some(VCommentDict::with_framing(false));
        self.dirty = true;
        Ok(())
    }

    fn tags(&self) -> Option<&Self::Tags> {
        self.tags.as_ref()
    }

    fn tags_mut(&mut self) -> Option<&mut Self::Tags> {
        self.tags.as_mut()
    }

    fn info(&self) -> &Self::Info {
        &self.info
    }

    fn score(filename: &str, header: &[u8]) -> i32 {
        let mut score = 0;

        // Check for FLAC signature - can be directly at start or after ID3
        if header.len() >= 4 {
            if &header[0..4] == b"fLaC" {
                score += 10; // Direct FLAC signature
            } else if header.len() >= 10 && &header[0..3] == b"ID3" {
                // Check for FLAC signature after ID3 tag
                for i in 10..header.len().saturating_sub(4) {
                    if &header[i..i + 4] == b"fLaC" {
                        score += 10;
                        break;
                    }
                }
            }
        }

        // Check file extension
        if filename.to_lowercase().ends_with(".flac") {
            score += 3;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/flac", "audio/x-flac", "application/x-flac"]
    }
}

/// FLAC audio stream information from the STREAMINFO metadata block.
///
/// This struct contains essential audio properties extracted from the mandatory
/// STREAMINFO block that appears at the beginning of every FLAC file. It provides
/// both basic playback information (sample rate, channels, bit depth) and technical
/// details (block sizes, frame sizes, MD5 signature).
///
/// # STREAMINFO Block
///
/// The STREAMINFO block is always present as the first metadata block in a FLAC file.
/// It contains the minimum information needed to decode the audio stream.
///
/// # Examples
///
/// ```no_run
/// use audex::flac::FLAC;
/// use audex::FileType;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let flac = FLAC::load("song.flac")?;
///     let info = &flac.info;
///
///     // Basic audio properties
///     println!("Sample rate: {} Hz", info.sample_rate);
///     println!("Bit depth: {} bits", info.bits_per_sample);
///     println!("Channels: {}", info.channels);
///     println!("Total samples: {}", info.total_samples);
///
///     // Calculate duration
///     if let Some(duration) = info.length {
///         println!("Duration: {:.2} seconds", duration.as_secs_f64());
///     }
///
///     // Check if file has been decoded correctly (MD5 verification)
///     println!("MD5 signature: {}", hex::encode(&info.md5_signature));
///     Ok(())
/// }
/// ```
///
/// # Technical Details
///
/// FLAC uses variable block sizes for compression. The `min_blocksize` and `max_blocksize`
/// fields indicate the range of block sizes used in the file. Most FLAC files use a
/// constant block size of 4096 samples.
///
/// The MD5 signature is calculated over the unencoded audio data and can be used to
/// verify that the file has been decoded correctly.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FLACStreamInfo {
    /// Total audio duration, calculated from total_samples and sample_rate
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::duration_as_secs_f64")
    )]
    pub length: Option<Duration>,

    /// Average bitrate in bits per second, if calculable
    ///
    /// This is an estimate based on file size and duration. FLAC uses variable
    /// bitrate compression, so the actual bitrate varies throughout the file.
    pub bitrate: Option<u32>,

    /// Sample rate in Hz (e.g., 44100, 48000, 96000)
    ///
    /// Valid range: 1 Hz to 655,350 Hz (though practical range is much narrower).
    /// Common values: 44100 (CD quality), 48000 (DAT/DVD), 96000 (high-resolution).
    pub sample_rate: u32,

    /// Number of audio channels (1 = mono, 2 = stereo, etc.)
    ///
    /// Valid range: 1 to 8 channels. FLAC supports up to 8 independent channels.
    pub channels: u16,

    /// Bits per sample (bit depth)
    ///
    /// Valid range: 1 to 32 bits per sample.
    /// Common values: 16 (CD quality), 24 (high-resolution), 32 (float or int).
    pub bits_per_sample: u16,

    /// Total number of audio samples in the stream
    ///
    /// To calculate duration: `duration = total_samples / sample_rate`
    /// A value of 0 means the total is unknown (not recommended but allowed).
    pub total_samples: u64,

    /// MD5 signature of the unencoded audio data (16 bytes)
    ///
    /// This signature is computed over the raw PCM audio data and can be used
    /// to verify that decoding was performed correctly. All zeros means the
    /// signature was not calculated.
    pub md5_signature: [u8; 16],

    /// Minimum block size in samples used in the stream
    ///
    /// Must be at least 16 and at most 65535. Most files use 4096.
    pub min_blocksize: u16,

    /// Maximum block size in samples used in the stream
    ///
    /// Must be at least 16 and at most 65535. For constant block size encoding,
    /// this equals `min_blocksize`. Most files use 4096.
    pub max_blocksize: u16,

    /// Minimum frame size in bytes used in the stream
    ///
    /// A value of 0 means the value is unknown. Useful for buffer allocation.
    pub min_framesize: u32,

    /// Maximum frame size in bytes used in the stream
    ///
    /// A value of 0 means the value is unknown. Useful for buffer allocation.
    pub max_framesize: u32,
}

impl StreamInfo for FLACStreamInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        self.bitrate
    }

    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate)
    }

    fn channels(&self) -> Option<u16> {
        Some(self.channels)
    }

    fn bits_per_sample(&self) -> Option<u16> {
        Some(self.bits_per_sample)
    }
}

impl FLACStreamInfo {
    /// Parse StreamInfo from raw 34-byte STREAMINFO block data
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != 34 {
            return Err(AudexError::InvalidData(format!(
                "STREAMINFO must be exactly 34 bytes, got {}",
                data.len()
            )));
        }

        let mut cursor = Cursor::new(data);

        // Parse STREAMINFO fields
        let min_blocksize = cursor.read_u16::<BigEndian>()?;
        let max_blocksize = cursor.read_u16::<BigEndian>()?;

        let min_framesize_bytes = [cursor.read_u8()?, cursor.read_u8()?, cursor.read_u8()?];
        let min_framesize = u32::from_be_bytes([
            0,
            min_framesize_bytes[0],
            min_framesize_bytes[1],
            min_framesize_bytes[2],
        ]);

        let max_framesize_bytes = [cursor.read_u8()?, cursor.read_u8()?, cursor.read_u8()?];
        let max_framesize = u32::from_be_bytes([
            0,
            max_framesize_bytes[0],
            max_framesize_bytes[1],
            max_framesize_bytes[2],
        ]);

        // Parse sample rate, channels, bits per sample, total samples (20 + 3 + 5 + 36 = 64 bits)
        let combined = cursor.read_u64::<BigEndian>()?;

        let sample_rate = ((combined >> 44) & 0xFFFFF) as u32; // 20 bits
        let channels = (((combined >> 41) & 0x07) as u16) + 1; // 3 bits, +1 because encoded as channels-1
        let bits_per_sample = (((combined >> 36) & 0x1F) as u16) + 1; // 5 bits, +1 because encoded as bps-1
        let total_samples = combined & 0xFFFFFFFFF; // 36 bits

        // MD5 signature (16 bytes)
        let mut md5_signature = [0u8; 16];
        cursor.read_exact(&mut md5_signature)?;

        if sample_rate == 0 {
            return Err(AudexError::InvalidData(
                "A sample rate value of 0 is invalid".to_string(),
            ));
        }

        // Calculate length and initial bitrate estimate
        let (length, bitrate) = if sample_rate > 0 && total_samples > 0 {
            let duration_secs = total_samples as f64 / sample_rate as f64;
            let len = Some(Duration::from_secs_f64(duration_secs));

            // Initial estimate from uncompressed rate; overwritten with accurate
            // bitrate from file size after loading
            let bits_per_second = sample_rate as u64 * channels as u64 * bits_per_sample as u64;
            // Saturate to u32::MAX instead of silently truncating
            let br = Some(u32::try_from(bits_per_second).unwrap_or(u32::MAX));
            (len, br)
        } else {
            (Some(Duration::from_secs(0)), Some(0))
        };

        Ok(Self {
            length,
            bitrate,
            sample_rate,
            channels,
            bits_per_sample,
            total_samples,
            md5_signature,
            min_blocksize,
            max_blocksize,
            min_framesize,
            max_framesize,
        })
    }

    /// Convert StreamInfo to bytes for writing
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(34);
        let mut writer = Cursor::new(&mut buffer);

        // Min/max block size (2 bytes each)
        writer.write_u16::<BigEndian>(self.min_blocksize)?;
        writer.write_u16::<BigEndian>(self.max_blocksize)?;

        // Min/max frame size (3 bytes each)
        let min_frame_bytes = self.min_framesize.to_be_bytes();
        writer.write_all(&min_frame_bytes[1..])?;
        let max_frame_bytes = self.max_framesize.to_be_bytes();
        writer.write_all(&max_frame_bytes[1..])?;

        // Sample rate (20 bits), channels (3 bits), bits per sample (5 bits), total samples (36 bits)
        let combined = ((self.sample_rate as u64) << 44)
            | ((self.channels.saturating_sub(1) as u64) << 41)
            | ((self.bits_per_sample.saturating_sub(1) as u64) << 36)
            | (self.total_samples & 0xFFFFFFFFF);
        writer.write_u64::<BigEndian>(combined)?;

        // MD5 signature (16 bytes)
        writer.write_all(&self.md5_signature)?;

        Ok(buffer)
    }

    /// Write StreamInfo block to bytes (wrapper for compatibility with test API)
    pub fn write(&self) -> Result<Vec<u8>> {
        self.to_bytes()
    }
}

/// Robust file I/O wrapper for FLAC parsing.
///
/// Wraps a `Read + Seek` reader with position tracking and optional size
/// validation, providing better error messages when reads exceed expected
/// bounds. Used internally during FLAC metadata block parsing.
pub struct StrictReader<R: Read + Seek> {
    reader: R,
    position: u64,
    total_size: Option<u64>,
}

impl<R: Read + Seek> StrictReader<R> {
    pub fn new(mut reader: R) -> std::io::Result<Self> {
        // Get total size if possible
        let total_size = match reader.seek(SeekFrom::End(0)) {
            Ok(size) => {
                reader.seek(SeekFrom::Start(0))?;
                Some(size)
            }
            Err(_) => None,
        };

        Ok(Self {
            reader,
            position: 0,
            total_size,
        })
    }

    /// Read exact number of bytes with improved error messages
    pub fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let bytes_to_read = buf.len();

        // Check if we have enough data remaining
        if let Some(total_size) = self.total_size {
            if self.position + bytes_to_read as u64 > total_size {
                return Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    format!(
                        "Attempted to read {} bytes at position {}, but only {} bytes available",
                        bytes_to_read,
                        self.position,
                        total_size - self.position
                    ),
                ));
            }
        }

        match self.reader.read_exact(buf) {
            Ok(()) => {
                self.position += bytes_to_read as u64;
                Ok(())
            }
            Err(e) => {
                if e.kind() == ErrorKind::UnexpectedEof {
                    Err(std::io::Error::new(
                        ErrorKind::UnexpectedEof,
                        format!(
                            "Unexpected EOF at position {} while reading {} bytes: {}",
                            self.position, bytes_to_read, e
                        ),
                    ))
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Seek to position with validation
    pub fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self.reader.seek(pos) {
            Ok(new_pos) => {
                self.position = new_pos;

                // Validate position is within bounds
                if let Some(total_size) = self.total_size {
                    if new_pos > total_size {
                        return Err(std::io::Error::new(
                            ErrorKind::InvalidInput,
                            format!("Seek position {} exceeds file size {}", new_pos, total_size),
                        ));
                    }
                }

                Ok(new_pos)
            }
            Err(e) => Err(e),
        }
    }

    /// Get current stream position
    pub fn stream_position(&self) -> std::io::Result<u64> {
        Ok(self.position)
    }

    /// Read bytes with partial read support and validation
    pub fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.reader.read(buf) {
            Ok(bytes_read) => {
                self.position += bytes_read as u64;
                Ok(bytes_read)
            }
            Err(e) => Err(e),
        }
    }

    /// Check if more data is available
    pub fn has_data_remaining(&self) -> bool {
        if let Some(total_size) = self.total_size {
            self.position < total_size
        } else {
            true // Unknown size, assume data might be available
        }
    }
}

impl<R: Read + Seek> Read for StrictReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read(buf)
    }
}

impl<R: Read + Seek> Seek for StrictReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.seek(pos)
    }
}

/// A FLAC application block for preserving application-specific metadata.
///
/// Application blocks allow third-party tools to store proprietary data within
/// FLAC files without affecting playback. Each application block is identified
/// by a unique 4-byte ID registered with the FLAC project.
///
/// # Registered Application IDs
///
/// Some commonly used application IDs include:
///
/// | ID (ASCII) | Application                         |
/// |------------|-------------------------------------|
/// | `RIFF`     | RIFF-based audio information        |
/// | `aiff`     | AIFF audio metadata                 |
/// | `riff`     | RIFF INFO chunk                     |
/// | `seektable`| Seeking information                 |
///
/// # Size Limits
///
/// The data payload can be up to 16,777,211 bytes (2²⁴ - 1 minus the 4-byte ID).
/// The total block size including the ID must not exceed 16,777,215 bytes.
///
/// # Example
///
/// ```rust
/// use audex::flac::ApplicationBlock;
///
/// // Create an application block with a custom ID
/// let app_id = *b"TEST";
/// let custom_data = vec![0x01, 0x02, 0x03, 0x04];
/// let block = ApplicationBlock::new(app_id, custom_data);
///
/// assert_eq!(block.application_id, *b"TEST");
/// assert_eq!(block.data.len(), 4);
/// ```
///
/// # See Also
///
/// - [`MetadataBlock`] - Generic container for all metadata block types
/// - [FLAC Application ID Registry](https://xiph.org/flac/id.html) - Official list of registered IDs
#[derive(Debug, Clone)]
pub struct ApplicationBlock {
    /// The 4-byte application identifier.
    ///
    /// This should be a registered ID from the FLAC project or a unique identifier
    /// for custom applications. IDs are typically printable ASCII characters.
    pub application_id: [u8; 4],

    /// The raw binary data specific to the application.
    ///
    /// The format and interpretation of this data is entirely determined by
    /// the application identified by `application_id`. This library preserves
    /// the data verbatim during read/write operations.
    pub data: Vec<u8>,
}

impl ApplicationBlock {
    /// Create new Application block with ID and data
    pub fn new(application_id: [u8; 4], data: Vec<u8>) -> Self {
        Self {
            application_id,
            data,
        }
    }

    /// Parse Application block from raw block data
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(AudexError::InvalidData(
                "Application block too short".to_string(),
            ));
        }

        let mut application_id = [0u8; 4];
        application_id.copy_from_slice(&data[0..4]);

        let app_data = data[4..].to_vec();

        Ok(Self {
            application_id,
            data: app_data,
        })
    }

    /// Convert Application block back to raw bytes for writing
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(4 + self.data.len());
        buffer.extend_from_slice(&self.application_id);
        buffer.extend_from_slice(&self.data);
        Ok(buffer)
    }

    /// Get application ID as string (if valid ASCII)
    pub fn application_id_str(&self) -> Option<String> {
        std::str::from_utf8(&self.application_id)
            .ok()
            .map(|s| s.to_string())
    }

    /// Get the total size of the Application block
    pub fn total_size(&self) -> usize {
        4 + self.data.len()
    }
}

/// A generic FLAC metadata block for storing arbitrary metadata.
///
/// FLAC files consist of a series of metadata blocks followed by audio frames.
/// This struct represents a single metadata block with its type identifier and
/// raw byte data, allowing preservation of unknown or application-specific blocks.
///
/// # Block Types
///
/// The FLAC specification defines the following metadata block types:
///
/// | Type | Name          | Description                                    |
/// |------|---------------|------------------------------------------------|
/// | 0    | STREAMINFO    | Audio stream information (required, first)     |
/// | 1    | PADDING       | Reserved space for future metadata expansion   |
/// | 2    | APPLICATION   | Application-specific data                      |
/// | 3    | SEEKTABLE     | Seek points for random access                  |
/// | 4    | VORBIS_COMMENT| Vorbis-style metadata tags                     |
/// | 5    | CUESHEET      | CD table of contents                           |
/// | 6    | PICTURE       | Embedded picture (album art, etc.)             |
/// | 7-126| Reserved      | Reserved for future use                        |
/// | 127  | Invalid       | Invalid, used to mark invalid blocks           |
///
/// # Size Limits
///
/// Metadata block sizes are encoded as 24-bit values, allowing a maximum size
/// of 16,777,215 bytes (approximately 16 MB) per block.
///
/// # Example
///
/// ```
/// use audex::flac::MetadataBlock;
///
/// // Create a custom metadata block
/// let block = MetadataBlock::new(2, vec![0x41, 0x50, 0x50, 0x4C]); // APPLICATION block
///
/// assert_eq!(block.block_type, 2);
/// ```
#[derive(Debug, Clone)]
pub struct MetadataBlock {
    /// The metadata block type identifier (0-127).
    ///
    /// See the Block Types table above for standard type definitions.
    pub block_type: u8,

    /// The raw byte data contained within this metadata block.
    ///
    /// The interpretation of this data depends on the `block_type`.
    pub data: Vec<u8>,

    /// If set, the 3-byte size written into the block header will be this
    /// value instead of `data.len()`. Used for round-tripping oversized
    /// VorbisComment / Picture blocks whose real size exceeds the 24-bit
    /// limit -- we preserve the original (wrong) header size from the file
    /// rather than refusing to save.
    pub override_header_size: Option<u32>,
}

impl MetadataBlock {
    pub fn new(block_type: u8, data: Vec<u8>) -> Self {
        Self {
            block_type,
            data,
            override_header_size: None,
        }
    }

    /// Write metadata block to writer with header
    pub fn write_to<W: Write>(&self, writer: &mut W, is_last: bool) -> Result<()> {
        // Block type occupies bits 1-7 of the header byte (0-126 valid).
        // Type 127 is reserved as invalid by the FLAC spec, and values
        // >= 128 would collide with the is-last flag in bit 0.
        if self.block_type >= 127 {
            return Err(AudexError::InvalidData(format!(
                "FLAC block type {} is out of valid range (0-126)",
                self.block_type
            )));
        }

        // Header byte (1 bit last flag + 7 bits block type)
        let header_byte = self.block_type | if is_last { 0x80 } else { 0x00 };
        writer.write_u8(header_byte)?;

        // Block size (24 bits).
        // If override_header_size is set, use that value (round-tripping an
        // already-oversized block). Otherwise enforce the spec limit.
        let size: u32 = if let Some(overridden) = self.override_header_size {
            overridden
        } else {
            let sz = self.data.len() as u64;
            if sz > 0xFFFFFF {
                return Err(AudexError::InvalidData(format!(
                    "Block too large: {} bytes (max: {} bytes)",
                    sz, 0xFFFFFF
                )));
            }
            sz as u32
        };

        let size_bytes = size.to_be_bytes();
        writer.write_all(&size_bytes[1..])?;

        // Block data - write in chunks for very large blocks
        if self.data.len() > 1024 * 1024 {
            // 1MB threshold
            const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks
            for chunk in self.data.chunks(CHUNK_SIZE) {
                writer.write_all(chunk)?;
            }
        } else {
            writer.write_all(&self.data)?;
        }

        Ok(())
    }
}

/// A seek point entry within a FLAC seek table.
///
/// Seek points enable fast random access to specific positions within a FLAC stream.
/// Each seek point maps a sample number to a byte offset in the audio data, allowing
/// decoders to jump directly to a specific position without scanning from the beginning.
///
/// # Structure
///
/// Each seek point occupies exactly 18 bytes in the FLAC file:
/// - 8 bytes: First sample number in the target frame
/// - 8 bytes: Byte offset from the first frame header
/// - 2 bytes: Number of samples in the target frame
///
/// # Placeholder Points
///
/// A seek point with `first_sample` set to `0xFFFFFFFFFFFFFFFF` is a placeholder.
/// Placeholders reserve space in the seek table for future use and should be
/// ignored by decoders. Use [`SeekPoint::is_placeholder()`] to check.
///
/// # Example
///
/// ```
/// use audex::flac::SeekPoint;
///
/// // Create a seek point at sample 44100 (1 second at 44.1kHz)
/// let point = SeekPoint::new(44100, 8192, 4096);
/// assert!(!point.is_placeholder());
///
/// // Create a placeholder for future use
/// let placeholder = SeekPoint::placeholder();
/// assert!(placeholder.is_placeholder());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SeekPoint {
    /// The sample number of the first sample in the target frame.
    ///
    /// For placeholder points, this value is `0xFFFFFFFFFFFFFFFF`.
    pub first_sample: u64,

    /// The byte offset from the first byte of the first frame header to the
    /// first byte of the target frame's header.
    pub byte_offset: u64,

    /// The number of samples in the target frame.
    pub num_samples: u16,
}

impl SeekPoint {
    const PLACEHOLDER_SAMPLE: u64 = 0xFFFFFFFFFFFFFFFF;

    pub fn new(first_sample: u64, byte_offset: u64, num_samples: u16) -> Self {
        Self {
            first_sample,
            byte_offset,
            num_samples,
        }
    }

    pub fn placeholder() -> Self {
        Self::new(Self::PLACEHOLDER_SAMPLE, 0, 0)
    }

    pub fn is_placeholder(&self) -> bool {
        self.first_sample == Self::PLACEHOLDER_SAMPLE
    }
}

/// A FLAC seek table containing indexed seek points for random access.
///
/// The seek table is an optional metadata block that enables fast seeking within
/// a FLAC stream. It contains an ordered list of [`SeekPoint`] entries, each mapping
/// a sample number to a byte offset in the audio data.
///
/// # Purpose
///
/// Without a seek table, seeking to a specific position in a FLAC file requires
/// scanning from the beginning or using binary search on frame headers. The seek
/// table provides pre-computed offsets for quick random access, which is especially
/// valuable for:
///
/// - Large audio files where scanning would be slow
/// - Streaming applications requiring immediate seeks
/// - Audio players with seek bar interfaces
///
/// # Structure
///
/// The seek table metadata block contains:
/// - A sequence of 18-byte seek point entries
/// - Points must be sorted in ascending order by sample number
/// - Placeholder points (sample number = `0xFFFFFFFFFFFFFFFF`) may be included
///   for future expansion
///
/// # Example
///
/// ```
/// use audex::flac::{SeekTable, SeekPoint};
///
/// let mut table = SeekTable::new();
///
/// // Add seek points at regular intervals (every second at 44.1kHz)
/// table.seekpoints.push(SeekPoint::new(0, 0, 4096));
/// table.seekpoints.push(SeekPoint::new(44100, 8192, 4096));
/// table.seekpoints.push(SeekPoint::new(88200, 16384, 4096));
///
/// assert_eq!(table.seekpoints.len(), 3);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SeekTable {
    /// The ordered list of seek points in this table.
    ///
    /// Seek points should be sorted by `first_sample` in ascending order.
    /// Placeholder points should appear at the end of the list.
    pub seekpoints: Vec<SeekPoint>,
}

impl SeekTable {
    pub fn new() -> Self {
        Self {
            seekpoints: Vec::new(),
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Self::from_bytes_with_options(data, None)
    }

    /// Parse seek table with optional limits for robustness
    pub fn from_bytes_with_options(data: &[u8], max_seekpoints: Option<usize>) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        let mut seekpoints = Vec::new();

        // Each seekpoint is exactly 18 bytes
        let max_points = max_seekpoints.unwrap_or(100000); // Reasonable limit
        let expected_points = data.len() / 18;

        if expected_points > max_points {
            return Err(AudexError::InvalidData(format!(
                "Too many seek points: {} (max: {})",
                expected_points, max_points
            )));
        }

        while cursor.position() + 18 <= data.len() as u64 {
            let first_sample = cursor.read_u64::<BigEndian>()?;
            let byte_offset = cursor.read_u64::<BigEndian>()?;
            let num_samples = cursor.read_u16::<BigEndian>()?;

            seekpoints.push(SeekPoint::new(first_sample, byte_offset, num_samples));

            // Safety check
            if seekpoints.len() > max_points {
                break;
            }
        }

        Ok(Self { seekpoints })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut writer = Cursor::new(&mut buffer);

        for seekpoint in &self.seekpoints {
            writer.write_u64::<BigEndian>(seekpoint.first_sample)?;
            writer.write_u64::<BigEndian>(seekpoint.byte_offset)?;
            writer.write_u16::<BigEndian>(seekpoint.num_samples)?;
        }

        Ok(buffer)
    }

    /// Write seek table to bytes (wrapper for compatibility with test API)
    pub fn write(&self) -> Result<Vec<u8>> {
        self.to_bytes()
    }
}

/// An index point within a FLAC cue sheet track.
///
/// Index points mark significant positions within a track, following the CD-DA
/// standard. Each index provides a sample offset within the containing track.
///
/// # CD-DA Standard
///
/// In the CD-DA specification:
/// - **Index 00**: Pre-gap or hidden track area before the main audio
/// - **Index 01**: Start of the main audio content (always present)
/// - **Index 02+**: Additional markers for live recordings, medleys, etc.
///
/// # Example
///
/// ```
/// use audex::flac::CueSheetTrackIndex;
///
/// // Index 01 at the track start (offset 0)
/// let main_start = CueSheetTrackIndex {
///     index_number: 1,
///     index_offset: 0,
/// };
///
/// // Index 02 at a specific position within the track
/// let marker = CueSheetTrackIndex {
///     index_number: 2,
///     index_offset: 441000, // 10 seconds at 44.1kHz
/// };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct CueSheetTrackIndex {
    /// The index number (0-99 per CD-DA specification).
    ///
    /// - Index 0: Pre-gap area
    /// - Index 1: Start of main audio (required)
    /// - Index 2-99: Additional markers
    pub index_number: u8,

    /// Sample offset from the track start to this index point.
    ///
    /// For CD-DA compatibility, this should be a multiple of 588 samples
    /// (one CD frame = 588 samples at 44.1kHz).
    pub index_offset: u64,
}

/// A track entry within a FLAC cue sheet.
///
/// Each track represents an individual song or data segment on a CD, containing
/// the track's position, identification, audio characteristics, and index points.
///
/// # CD-DA Compliance
///
/// For CD-DA (Compact Disc Digital Audio) compatibility:
/// - Track numbers range from 1 to 99 (track 170 is reserved for lead-out)
/// - ISRC codes must be exactly 12 characters
/// - Track offsets should be multiples of 588 samples (one CD frame)
///
/// # Example
///
/// ```
/// use audex::flac::{CueSheetTrack, CueSheetTrackIndex};
///
/// let track = CueSheetTrack {
///     track_number: 1,
///     start_offset: 0,
///     isrc: String::from("USRC17607839"),
///     track_type: 0, // Audio track
///     pre_emphasis: false,
///     indexes: vec![
///         CueSheetTrackIndex { index_number: 1, index_offset: 0 },
///     ],
/// };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct CueSheetTrack {
    /// Track number (1-99 for audio, 170 for lead-out).
    pub track_number: u8,

    /// Sample offset from the beginning of the FLAC audio stream to the
    /// first sample of this track.
    pub start_offset: u64,

    /// International Standard Recording Code (12 ASCII characters).
    ///
    /// For CD-DA, this should be exactly 12 characters. An empty string
    /// indicates no ISRC is available for this track.
    pub isrc: String,

    /// Track type flag.
    ///
    /// - `0`: Audio track (CD-DA)
    /// - `1`: Digital data track (CD-ROM)
    pub track_type: u8,

    /// Whether pre-emphasis is applied to this track.
    ///
    /// Pre-emphasis is a form of audio equalization used on some older CDs
    /// to reduce high-frequency noise. Modern recordings typically do not
    /// use pre-emphasis.
    pub pre_emphasis: bool,

    /// Index points within this track.
    ///
    /// At minimum, index 01 (track start) should be present. Index 00
    /// represents the pre-gap area before the main audio.
    pub indexes: Vec<CueSheetTrackIndex>,
}

/// A FLAC cue sheet for storing CD table of contents and track information.
///
/// The cue sheet metadata block stores information about the CD structure from
/// which the FLAC file was ripped, including track boundaries, ISRCs (International
/// Standard Recording Codes), and index points.
///
/// # CD-DA Compliance
///
/// For files ripped from audio CDs, the cue sheet maintains CD-DA compliance:
/// - Sample offsets must be multiples of 588 (one CD frame at 44.1kHz)
/// - Lead-in must be at least 88200 samples (2 seconds) for CD-DA
/// - Track numbers range from 1-99, with track 170 reserved for lead-out
///
/// # Usage
///
/// Cue sheets are useful for:
/// - Preserving CD track information when ripping entire albums to single files
/// - Enabling accurate track splitting from a single FLAC file
/// - Storing ISRCs for proper track identification
/// - Maintaining CD-TEXT information compatibility
///
/// # Example
///
/// ```
/// use audex::flac::{CueSheet, CueSheetTrack, CueSheetTrackIndex};
///
/// let mut cuesheet = CueSheet::new();
/// cuesheet.media_catalog_number = "0012345678901".to_string();
/// cuesheet.is_compact_disc = true;
///
/// // Add a track
/// let track = CueSheetTrack {
///     track_number: 1,
///     start_offset: 0,
///     isrc: String::from("USRC17607839"),
///     track_type: 0, // Audio track
///     pre_emphasis: false,
///     indexes: vec![CueSheetTrackIndex { index_number: 1, index_offset: 0 }],
/// };
/// cuesheet.tracks.push(track);
/// ```
///
/// # See Also
///
/// - [`CueSheetTrack`] - Individual track entries
/// - [`CueSheetTrackIndex`] - Index points within tracks
#[derive(Debug, Clone, PartialEq)]
pub struct CueSheet {
    /// The media catalog number from the CD (UPC/EAN barcode).
    ///
    /// Up to 128 characters. Empty string if not available.
    /// For CD-DA, this is typically a 13-digit UPC/EAN code.
    pub media_catalog_number: String,

    /// Number of lead-in samples.
    ///
    /// For CD-DA, this must be at least 88200 samples (2 seconds at 44.1kHz).
    /// The lead-in is the silent area at the beginning of the disc.
    pub lead_in_samples: u64,

    /// Whether this cue sheet corresponds to a Compact Disc.
    ///
    /// When `true`, certain CD-DA constraints are expected to be followed
    /// (sample offsets as multiples of 588, etc.).
    pub is_compact_disc: bool,

    /// The list of tracks in the cue sheet.
    ///
    /// Should include all audio tracks plus a lead-out track (track 170).
    pub tracks: Vec<CueSheetTrack>,
}

impl CueSheet {
    pub fn new() -> Self {
        Self {
            media_catalog_number: String::new(),
            lead_in_samples: 88200, // Default for CD-DA
            is_compact_disc: true,
            tracks: Vec::new(),
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data);

        // Read media catalog number (128 bytes, null-terminated)
        let mut mcn_bytes = [0u8; 128];
        cursor.read_exact(&mut mcn_bytes)?;
        let mcn_end = mcn_bytes.iter().position(|&x| x == 0).unwrap_or(128);
        let media_catalog_number = String::from_utf8_lossy(&mcn_bytes[..mcn_end]).into_owned();

        // Lead-in samples (8 bytes)
        let lead_in_samples = cursor.read_u64::<BigEndian>()?;

        // Flags (1 byte) + reserved (258 bytes)
        let flags = cursor.read_u8()?;
        let is_compact_disc = (flags & 0x80) != 0;
        cursor.seek(SeekFrom::Current(258))?; // Skip reserved bytes

        // Number of tracks (1 byte). The FLAC spec allows at most 100
        // tracks in a CueSheet block (including the lead-out track).
        let num_tracks = cursor.read_u8()?;
        const MAX_CUESHEET_TRACKS: u8 = 100;
        if num_tracks > MAX_CUESHEET_TRACKS {
            return Err(AudexError::InvalidData(format!(
                "CueSheet track count {} exceeds FLAC spec limit of {}",
                num_tracks, MAX_CUESHEET_TRACKS
            )));
        }
        let mut tracks = Vec::new();

        for _ in 0..num_tracks {
            // Track offset (8 bytes)
            let start_offset = cursor.read_u64::<BigEndian>()?;

            // Track number (1 byte)
            let track_number = cursor.read_u8()?;

            // ISRC (12 bytes)
            let mut isrc_bytes = [0u8; 12];
            cursor.read_exact(&mut isrc_bytes)?;
            let isrc_end = isrc_bytes.iter().position(|&x| x == 0).unwrap_or(12);
            let isrc = String::from_utf8_lossy(&isrc_bytes[..isrc_end]).into_owned();

            // Flags (1 byte)
            let track_flags = cursor.read_u8()?;
            let track_type = (track_flags >> 7) & 1;
            let pre_emphasis = (track_flags & 0x40) != 0;

            // Reserved (13 bytes)
            cursor.seek(SeekFrom::Current(13))?;

            // Number of indexes (1 byte)
            let num_indexes = cursor.read_u8()?;
            let mut indexes = Vec::new();

            for _ in 0..num_indexes {
                let index_offset = cursor.read_u64::<BigEndian>()?;
                let index_number = cursor.read_u8()?;
                cursor.seek(SeekFrom::Current(3))?; // Reserved

                indexes.push(CueSheetTrackIndex {
                    index_number,
                    index_offset,
                });
            }

            tracks.push(CueSheetTrack {
                track_number,
                start_offset,
                isrc,
                track_type,
                pre_emphasis,
                indexes,
            });
        }

        Ok(Self {
            media_catalog_number,
            lead_in_samples,
            is_compact_disc,
            tracks,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut writer = Cursor::new(&mut buffer);

        // Media catalog number (128 bytes, null-padded)
        let mut mcn_bytes = [0u8; 128];
        let mcn_copy_len = self.media_catalog_number.len().min(128);
        mcn_bytes[..mcn_copy_len]
            .copy_from_slice(&self.media_catalog_number.as_bytes()[..mcn_copy_len]);
        writer.write_all(&mcn_bytes)?;

        // Lead-in samples
        writer.write_u64::<BigEndian>(self.lead_in_samples)?;

        // Flags + reserved
        let flags = if self.is_compact_disc { 0x80 } else { 0x00 };
        writer.write_u8(flags)?;
        writer.write_all(&[0u8; 258])?; // Reserved bytes

        // Number of tracks (must fit in a single byte per the FLAC CueSheet format)
        let track_count = u8::try_from(self.tracks.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "CueSheet track count {} exceeds maximum of 255",
                self.tracks.len()
            ))
        })?;
        writer.write_u8(track_count)?;

        // Write tracks
        for track in &self.tracks {
            writer.write_u64::<BigEndian>(track.start_offset)?;
            writer.write_u8(track.track_number)?;

            // ISRC (12 bytes, null-padded)
            let mut isrc_bytes = [0u8; 12];
            let isrc_copy_len = track.isrc.len().min(12);
            isrc_bytes[..isrc_copy_len].copy_from_slice(&track.isrc.as_bytes()[..isrc_copy_len]);
            writer.write_all(&isrc_bytes)?;

            // Track flags
            let track_flags =
                (track.track_type << 7) | if track.pre_emphasis { 0x40 } else { 0x00 };
            writer.write_u8(track_flags)?;

            // Reserved
            writer.write_all(&[0u8; 13])?;

            // Number of indexes (must fit in a single byte per the FLAC CueSheet format)
            let index_count = u8::try_from(track.indexes.len()).map_err(|_| {
                AudexError::InvalidData(format!(
                    "CueSheet track index count {} exceeds maximum of 255",
                    track.indexes.len()
                ))
            })?;
            writer.write_u8(index_count)?;

            // Write indexes
            for index in &track.indexes {
                writer.write_u64::<BigEndian>(index.index_offset)?;
                writer.write_u8(index.index_number)?;
                writer.write_all(&[0u8; 3])?; // Reserved
            }
        }

        Ok(buffer)
    }

    /// Write cue sheet to bytes (wrapper for compatibility with test API)
    pub fn write(&self) -> Result<Vec<u8>> {
        self.to_bytes()
    }
}

/// A FLAC padding block for metadata alignment and future expansion.
///
/// Padding blocks reserve empty space within the metadata section of a FLAC file,
/// allowing future edits (such as adding tags or pictures) without rewriting the
/// entire file. When metadata changes, the padding can absorb size differences.
///
/// # Purpose
///
/// Padding is useful for:
/// - **In-place editing**: Modify tags without rewriting audio data
/// - **Future expansion**: Reserve space for additional metadata
/// - **Alignment**: Align audio data to sector boundaries for optimal I/O
///
/// # Structure
///
/// A padding block consists entirely of zero bytes. The content is never read
/// during playback—only the size matters for reserving space.
///
/// # Size Limits
///
/// Padding block size is limited to 16,777,215 bytes (2²⁴ - 1) like all FLAC
/// metadata blocks. For very large padding requirements, multiple padding blocks
/// can be used.
///
/// # Example
///
/// ```
/// use audex::flac::Padding;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Create 4KB of padding for future tag expansion
///     let padding = Padding::new(4096);
///     assert_eq!(padding.size, 4096);
///
///     // Serialize to bytes (all zeros)
///     let bytes = padding.to_bytes()?;
///     assert_eq!(bytes.len(), 4096);
///     assert!(bytes.iter().all(|&b| b == 0));
///     Ok(())
/// }
/// ```
///
/// # See Also
///
/// - [`FLAC::add_padding`] - Add padding to a FLAC file
/// - [`FLAC::total_padding_size`] - Get total padding across all blocks
/// - [`FLAC::optimize_padding`] - Consolidate fragmented padding
#[derive(Debug, Clone, PartialEq)]
pub struct Padding {
    /// The size of the padding block in bytes.
    ///
    /// This represents the number of null bytes reserved for future use.
    /// A size of 0 creates an empty padding block (valid but not useful).
    pub size: usize,
}

impl Padding {
    pub fn new(size: usize) -> Self {
        Self { size }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        // Padding blocks should contain only null bytes
        // We don't need to validate the content, just store the size
        Ok(Self { size: data.len() })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        // Padding blocks are filled with null bytes
        Ok(vec![0u8; self.size])
    }

    /// Write padding to bytes (wrapper for compatibility with test API)
    pub fn write(&self) -> Result<Vec<u8>> {
        self.to_bytes()
    }

    /// Write padding block efficiently for large sizes
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<()> {
        // For very large padding blocks, write in chunks to avoid memory issues
        if self.size > 64 * 1024 {
            // 64KB threshold
            const CHUNK_SIZE: usize = 64 * 1024;
            let chunk = vec![0u8; CHUNK_SIZE];
            let full_chunks = self.size / CHUNK_SIZE;
            let remainder = self.size % CHUNK_SIZE;

            // Write full chunks
            for _ in 0..full_chunks {
                writer.write_all(&chunk)?;
            }

            // Write remainder
            if remainder > 0 {
                let remainder_chunk = vec![0u8; remainder];
                writer.write_all(&remainder_chunk)?;
            }
        } else {
            // Small padding blocks, write directly
            let padding_data = vec![0u8; self.size];
            writer.write_all(&padding_data)?;
        }

        Ok(())
    }
}

impl Default for Padding {
    fn default() -> Self {
        Self::new(0)
    }
}

/// FLAC picture metadata for embedded album art and other images.
///
/// The Picture struct represents embedded image data within a FLAC file,
/// following the FLAC METADATA_BLOCK_PICTURE format which is compatible with
/// ID3v2 APIC frame picture types.
///
/// # Picture Types
///
/// The `picture_type` field uses ID3v2 APIC picture type values:
///
/// | Type | Description                              |
/// |------|------------------------------------------|
/// | 0    | Other                                    |
/// | 1    | 32x32 pixel file icon (PNG only)         |
/// | 2    | Other file icon                          |
/// | 3    | Cover (front)                            |
/// | 4    | Cover (back)                             |
/// | 5    | Leaflet page                             |
/// | 6    | Media (e.g., CD label)                   |
/// | 7    | Lead artist/performer                    |
/// | 8    | Artist/performer                         |
/// | 9    | Conductor                                |
/// | 10   | Band/Orchestra                           |
/// | 11   | Composer                                 |
/// | 12   | Lyricist/text writer                     |
/// | 13   | Recording location                       |
/// | 14   | During recording                         |
/// | 15   | During performance                       |
/// | 16   | Movie/video screen capture               |
/// | 17   | A bright colored fish                    |
/// | 18   | Illustration                             |
/// | 19   | Band/artist logotype                     |
/// | 20   | Publisher/Studio logotype                |
///
/// # Size Limits
///
/// The total picture block size (metadata + image data) must not exceed
/// 16,777,215 bytes (2²⁴ - 1). For practical use, keep embedded images
/// under a few megabytes to avoid excessive file sizes.
///
/// # Supported Formats
///
/// While FLAC supports any image format, the following are most common:
/// - **JPEG** (`image/jpeg`): Best for photographs
/// - **PNG** (`image/png`): Best for graphics, required for 32x32 icons
/// - **GIF** (`image/gif`): For animated or indexed color images
///
/// # Example
///
/// ```no_run
/// use audex::flac::{FLAC, Picture};
/// use audex::FileType;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Create a front cover picture
///     let mut picture = Picture::new();
///     picture.picture_type = 3; // Front cover
///     picture.mime_type = "image/jpeg".to_string();
///     picture.description = "Album Cover".to_string();
///     picture.width = 500;
///     picture.height = 500;
///     picture.color_depth = 24;
///     picture.colors_used = 0; // Not indexed
///     picture.data = std::fs::read("cover.jpg")?;
///
///     // Add to FLAC file
///     let mut flac = FLAC::load("song.flac")?;
///     flac.add_picture(picture);
///     flac.save()?;
///     Ok(())
/// }
/// ```
///
/// # See Also
///
/// - [`FLAC::add_picture`] - Add a picture to a FLAC file
/// - [`FLAC::clear_pictures`] - Remove all pictures
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Picture {
    /// The picture type following ID3v2 APIC conventions.
    ///
    /// Common values: 0 (Other), 3 (Front cover), 4 (Back cover).
    /// See the Picture Types table above for all values.
    pub picture_type: u32,

    /// The MIME type of the image (e.g., "image/jpeg", "image/png").
    ///
    /// This should accurately reflect the image format in `data`.
    pub mime_type: String,

    /// A human-readable description of the picture.
    ///
    /// Typically used for accessibility or to distinguish between multiple
    /// pictures of the same type (e.g., "Front Cover", "CD Label").
    pub description: String,

    /// Image width in pixels.
    pub width: u32,

    /// Image height in pixels.
    pub height: u32,

    /// Color depth in bits per pixel (e.g., 24 for RGB, 32 for RGBA).
    pub color_depth: u32,

    /// Number of colors in the palette for indexed images.
    ///
    /// Set to 0 for non-indexed (true color) images like JPEG.
    /// For GIF or paletted PNG, this is the number of colors in the palette.
    pub colors_used: u32,

    /// The raw image data in the format specified by `mime_type`.
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::bytes_as_base64")
    )]
    pub data: Vec<u8>,
}

impl Picture {
    /// Maximum allowed size for picture data (16MB)
    pub const MAX_SIZE: usize = (1 << 24) - 1; // 2^24 - 1 = 16777215

    pub fn new() -> Self {
        Self {
            picture_type: 0,
            mime_type: String::new(),
            description: String::new(),
            width: 0,
            height: 0,
            color_depth: 0,
            colors_used: 0,
            data: Vec::new(),
        }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        Self::from_bytes_with_options(data, None)
    }

    /// Parse picture from bytes with optional size limits for robustness
    pub fn from_bytes_with_options(data: &[u8], max_picture_size: Option<usize>) -> Result<Self> {
        let mut cursor = Cursor::new(data);

        // Picture type (4 bytes)
        let picture_type = cursor.read_u32::<BigEndian>()?;

        // MIME type length and string
        let mime_len = cursor.read_u32::<BigEndian>()? as usize;

        // Validate MIME type length
        if mime_len > 256 {
            // Reasonable limit for MIME types
            return Err(AudexError::InvalidData(format!(
                "MIME type too long: {} bytes",
                mime_len
            )));
        }

        let mut mime_bytes = vec![0u8; mime_len];
        cursor.read_exact(&mut mime_bytes)?;
        let mime_type = String::from_utf8(mime_bytes)
            .map_err(|e| AudexError::InvalidData(format!("Invalid MIME type: {}", e)))?;

        // Description length and string
        let desc_len = cursor.read_u32::<BigEndian>()? as usize;

        // Validate description length
        if desc_len > 65536 {
            // 64KB limit for descriptions
            return Err(AudexError::InvalidData(format!(
                "Description too long: {} bytes",
                desc_len
            )));
        }

        let mut desc_bytes = vec![0u8; desc_len];
        cursor.read_exact(&mut desc_bytes)?;
        let description = String::from_utf8(desc_bytes)
            .map_err(|e| AudexError::InvalidData(format!("Invalid description: {}", e)))?;

        // Image properties
        let width = cursor.read_u32::<BigEndian>()?;
        let height = cursor.read_u32::<BigEndian>()?;
        let color_depth = cursor.read_u32::<BigEndian>()?;
        let colors_used = cursor.read_u32::<BigEndian>()?;

        // Picture data
        let data_len = cursor.read_u32::<BigEndian>()? as usize;

        // Apply the format-specific limit if provided, otherwise fall back
        // to the library-wide image allocation ceiling
        let global_limit = crate::limits::ParseLimits::default().max_image_size as usize;
        let max_size = max_picture_size.unwrap_or(global_limit);
        if data_len > max_size {
            return Err(AudexError::InvalidData(format!(
                "Picture data too large: {} bytes (max: {} bytes)",
                data_len, max_size
            )));
        }

        // Check if we have enough data left
        let remaining = data.len() - cursor.position() as usize;
        if data_len > remaining {
            return Err(AudexError::InvalidData(format!(
                "Picture data truncated: expected {} bytes, have {} bytes",
                data_len, remaining
            )));
        }

        let mut picture_data = vec![0u8; data_len];
        cursor.read_exact(&mut picture_data)?;

        // Validate image dimensions are reasonable
        if width > 100000 || height > 100000 {
            return Err(AudexError::InvalidData(format!(
                "Image dimensions too large: {}x{}",
                width, height
            )));
        }

        Ok(Self {
            picture_type,
            mime_type,
            description,
            width,
            height,
            color_depth,
            colors_used,
            data: picture_data,
        })
    }

    /// Parse picture directly from a reader (field-by-field), ignoring any
    /// external block size. Used when distrust_size is enabled.
    pub fn from_reader<R: Read>(reader: &mut R, max_picture_size: Option<usize>) -> Result<Self> {
        let picture_type = reader.read_u32::<BigEndian>()?;

        let mime_len = reader.read_u32::<BigEndian>()? as usize;
        if mime_len > 256 {
            return Err(AudexError::InvalidData(format!(
                "MIME type too long: {} bytes",
                mime_len
            )));
        }
        let mut mime_bytes = vec![0u8; mime_len];
        reader.read_exact(&mut mime_bytes)?;
        let mime_type = String::from_utf8(mime_bytes)
            .map_err(|e| AudexError::InvalidData(format!("Invalid MIME type: {}", e)))?;

        let desc_len = reader.read_u32::<BigEndian>()? as usize;
        if desc_len > 65536 {
            return Err(AudexError::InvalidData(format!(
                "Description too long: {} bytes",
                desc_len
            )));
        }
        let mut desc_bytes = vec![0u8; desc_len];
        reader.read_exact(&mut desc_bytes)?;
        let description = String::from_utf8(desc_bytes)
            .map_err(|e| AudexError::InvalidData(format!("Invalid description: {}", e)))?;

        let width = reader.read_u32::<BigEndian>()?;
        let height = reader.read_u32::<BigEndian>()?;
        let color_depth = reader.read_u32::<BigEndian>()?;
        let colors_used = reader.read_u32::<BigEndian>()?;

        let data_len = reader.read_u32::<BigEndian>()? as usize;
        let global_limit = crate::limits::ParseLimits::default().max_image_size as usize;
        let max_size = max_picture_size.unwrap_or(global_limit);
        if data_len > max_size {
            return Err(AudexError::InvalidData(format!(
                "Picture data too large: {} bytes (max: {} bytes)",
                data_len, max_size
            )));
        }

        let mut data = vec![0u8; data_len];
        reader.read_exact(&mut data)?;

        Ok(Self {
            picture_type,
            mime_type,
            description,
            width,
            height,
            color_depth,
            colors_used,
            data,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let mut writer = Cursor::new(&mut buffer);

        // Picture type
        writer.write_u32::<BigEndian>(self.picture_type)?;

        // MIME type
        let mime_bytes = self.mime_type.as_bytes();
        writer.write_u32::<BigEndian>(mime_bytes.len() as u32)?;
        writer.write_all(mime_bytes)?;

        // Description
        let desc_bytes = self.description.as_bytes();
        writer.write_u32::<BigEndian>(desc_bytes.len() as u32)?;
        writer.write_all(desc_bytes)?;

        // Image properties
        writer.write_u32::<BigEndian>(self.width)?;
        writer.write_u32::<BigEndian>(self.height)?;
        writer.write_u32::<BigEndian>(self.color_depth)?;
        writer.write_u32::<BigEndian>(self.colors_used)?;

        // Picture data
        writer.write_u32::<BigEndian>(self.data.len() as u32)?;
        writer.write_all(&self.data)?;

        Ok(buffer)
    }

    /// Serialize picture metadata to bytes
    ///
    /// This method converts the Picture struct to its binary representation.
    /// It serializes the picture metadata into the binary format used for METADATA_BLOCK_PICTURE
    /// in OGG/Opus files and FLAC picture metadata blocks.
    pub fn write(&self) -> Result<Vec<u8>> {
        self.to_bytes()
    }
}

/// Clear all tags from a FLAC file.
///
/// Args:
///     path: Path to the FLAC file
///
/// Raises:
///     AudexError on failure
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut flac = FLAC::load(path)?;
    flac.clear()
}
