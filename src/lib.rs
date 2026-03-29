//! Audex strives to deliver a comprehensive audio metadata tagging solution.
//!
//! # Basic Usage
//!
//! Load metadata from a file:
//!
//! ```no_run
//! use audex::File;
//!
//! let metadata = File::load("audio.mp3")?;
//! # Ok::<(), audex::AudexError>(())
//! ```
//!
//! `metadata` acts like a dictionary of tags in the file. Tags are generally a
//! list of string-like values, but may have additional methods available
//! depending on tag or format. They may also be entirely different objects
//! for certain keys, again depending on format.
//!
//! # Async Operations
//!
//! All format types support async operations when the `async` feature is enabled.
//! Async methods use the `_async` suffix:
//!
//! ```ignore
//! // Note: This example requires the `async` feature and a tokio runtime.
//! // Enable with: audex = { version = "*", features = ["async"] }
//! use audex::flac::FLAC;
//! use audex::FileType;
//!
//! # async fn example() -> Result<(), audex::AudexError> {
//! // Load file asynchronously
//! let mut flac = FLAC::load_async("audio.flac").await?;
//!
//! // Modify tags
//! flac.set("artist", vec!["Artist Name".to_string()])?;
//!
//! // Save asynchronously
//! flac.save_async().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Loading from In-Memory Buffers
//!
//! You can load files from in-memory buffers or any `Read + Seek` source:
//!
//! ```no_run
//! use audex::File;
//! use std::io::Cursor;
//!
//! let audio_data = vec![/* audio file bytes */];
//! let cursor = Cursor::new(audio_data);
//! let metadata = File::load_from_reader(cursor, None)?;
//! # Ok::<(), audex::AudexError>(())
//! ```
//!
//! # Easy Wrappers
//!
//! Use EasyID3 for simplified MP3 tag access. Note that EasyID3's `set()` method
//! takes `&[String]` (a slice), while the [`FileType`] trait's `set()` takes
//! `Vec<String>`. The easy wrappers have their own dedicated API.
//!
//! ```no_run
//! use audex::easyid3::EasyID3;
//! use audex::FileType;
//!
//! let mut tags = EasyID3::load("song.mp3")?;
//! // EasyID3::set() takes &[String], not Vec<String>
//! tags.set("artist", &["Artist Name".to_string()])?;
//! tags.set("album", &["Album Title".to_string()])?;
//! tags.save()?;
//! # Ok::<(), audex::AudexError>(())
//! ```
//!
//! Use EasyMP4 for simplified MP4/M4A tag access. Note that EasyMP4's `set()` method
//! takes `Vec<String>`, while EasyID3's `set()` takes `&[String]`.
//!
//! ```no_run
//! use audex::easymp4::EasyMP4;
//! use audex::FileType;
//!
//! let mut tags = EasyMP4::load("song.m4a")?;
//! tags.set("title", vec!["Track Title".to_string()])?;
//! tags.set("artist", vec!["Artist Name".to_string()])?;
//! tags.save()?;
//! # Ok::<(), audex::AudexError>(())
//! ```
//!
//! EasyMP4 also supports custom key registration:
//!
//! ```no_run
//! use audex::easymp4::EasyMP4;
//! use audex::FileType;
//!
//! let mut tags = EasyMP4::load("song.m4a")?;
//! tags.register_text_key("my_tag", "----:TXXX:My Tag")?;
//! tags.set("my_tag", vec!["custom_value".to_string()])?;
//! tags.save()?;
//! # Ok::<(), audex::AudexError>(())
//! ```
//!
//! # Dynamic Key Registration
//!
//! Register custom keys for EasyID3. Two registration methods are available:
//!
//! - `register_txxx_key(key, description)` — maps a human-readable key to a TXXX
//!   (user-defined text) frame with the given description
//! - `register_text_key(key, frame_id)` — maps a human-readable key to a standard
//!   ID3v2 text frame (e.g., `"TDRC"`, `"TCOM"`)
//!
//! ```no_run
//! use audex::easyid3::EasyID3;
//! use audex::FileType;
//!
//! let mut tags = EasyID3::load("song.mp3")?;
//!
//! // Map "barcode" to a TXXX frame with description "BARCODE"
//! tags.register_txxx_key("barcode", "BARCODE")?;
//! tags.set("barcode", &["1234567890123".to_string()])?;
//!
//! // Map "composer" to the standard TCOM frame
//! tags.register_text_key("composer", "TCOM")?;
//! tags.set("composer", &["J.S. Bach".to_string()])?;
//!
//! tags.save()?;
//! # Ok::<(), audex::AudexError>(())
//! ```
//!
//! # M4A/MP4 Compatibility
//!
//! The `m4a` module provides compatibility aliases:
//!
//! ```no_run
//! use audex::m4a::M4A;
//! use audex::FileType;
//!
//! let audio = M4A::load("song.m4a")?;
//! # Ok::<(), audex::AudexError>(())
//! ```
//!
//! # Tag Deletion
//!
//! Delete ID3 tags from MP3 files:
//!
//! ```no_run
//! use audex::mp3;
//!
//! // Delete all ID3 tags
//! mp3::clear("song.mp3")?;
//!
//! // Delete only ID3v2 tags
//! mp3::clear_with_options("song.mp3", false, true)?;
//! # Ok::<(), audex::AudexError>(())
//! ```
//!
//! # Advanced Examples
//!
//! ## Custom I/O with In-Memory Buffers
//!
//! Load and manipulate audio metadata entirely in memory:
//!
//! ```no_run
//! use audex::File;
//! use std::io::Cursor;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! // Read file into memory
//! let audio_data = std::fs::read("music.flac")?;
//!
//! // Create a cursor for seeking within the buffer
//! let cursor = Cursor::new(audio_data.clone());
//!
//! // Load metadata from the in-memory buffer
//! let mut metadata = File::load_from_reader(cursor, Some("music.flac".into()))?;
//!
//! // Modify tags
//! metadata.set("artist", vec!["New Artist".to_string()])?;
//! metadata.set("album", vec!["New Album".to_string()])?;
//!
//! // Save back to an in-memory buffer via save_to_writer
//! let mut out_cursor = std::io::Cursor::new(audio_data);
//! metadata.save_to_writer(&mut out_cursor)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Async File Processing with Tokio
//!
//! Process multiple files concurrently using async operations:
//!
//! ```ignore
//! // Note: This example requires the `async` feature and tokio runtime.
//! // Enable with: audex = { version = "*", features = ["async"] }
//! use audex::File;
//! use audex::FileType;
//! use tokio::task;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Process multiple files concurrently
//! let files = vec!["song1.mp3", "song2.flac", "song3.m4a"];
//!
//! let mut tasks = Vec::new();
//! for file_path in files {
//!     tasks.push(task::spawn(async move {
//!         // Load file asynchronously
//!         let mut file = File::load_async(file_path).await?;
//!
//!         // Update metadata
//!         file.set("album", vec!["Compilation".to_string()])?;
//!
//!         // Save changes asynchronously
//!         file.save_async().await?;
//!
//!         Ok::<_, audex::AudexError>(())
//!     }));
//! }
//!
//! // Wait for all tasks to complete
//! for task in tasks {
//!     task.await??;
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Format-Specific Operations
//!
//! Work directly with specific format types for advanced features:
//!
//! ```no_run
//! use audex::flac::FLAC;
//! use audex::{FileType, StreamInfo};
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! // Load as a specific format for format-specific features
//! let mut flac = FLAC::load("audio.flac")?;
//!
//! // Access stream information via the StreamInfo trait
//! let info = flac.info();
//! println!("Sample rate: {} Hz", info.sample_rate().unwrap_or(0));
//! println!("Channels: {}", info.channels().unwrap_or(0));
//! println!("Bits per sample: {}", info.bits_per_sample().unwrap_or(0));
//!
//! // Modify tags using the dictionary interface
//! flac.set("comment", vec!["Remastered edition".to_string()])?;
//!
//! // Save changes back to file
//! flac.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Batch Tag Updates
//!
//! Update multiple tags at once using the update method:
//!
//! ```no_run
//! use audex::File;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let mut file = File::load("track.mp3")?;
//!
//! // Prepare batch updates
//! let updates = vec![
//!     ("artist".to_string(), vec!["Artist Name".to_string()]),
//!     ("album".to_string(), vec!["Album Title".to_string()]),
//!     ("title".to_string(), vec!["Track Title".to_string()]),
//!     ("date".to_string(), vec!["2024".to_string()]),
//!     ("genre".to_string(), vec!["Electronic".to_string()]),
//! ];
//!
//! // Apply all updates at once
//! file.update(updates)?;
//! file.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Read-Only Formats
//!
//! Some formats only provide stream information and do not support tag reading
//! or writing:
//!
//! - **AAC** ([`aac`]): ADTS/ADIF stream info only — read-only, no tagging (use M4A container for tags)
//! - **AC-3/E-AC-3** ([`ac3`]): Stream info only — read-only, no tagging (use a container format for tags)
//! - **SMF/MIDI** ([`smf`]): Duration only (MIDI has no tag concept)
//!
//! Calling `set()` or `remove()` on these formats is a no-op or returns an error.
//!
//! # APEv2 and ASF Tag Access
//!
//! Formats using APEv2 tags (Monkey's Audio, Musepack, WavPack, TAK,
//! OptimFROG) and ASF/WMA files store tag values internally as `APEValue` and
//! `ASFAttribute` types respectively, not as plain strings. Because of this, the
//! unified `Tags::get()` trait method currently returns `None` for these formats.
//! TrueAudio files may use either ID3v1/v2 or APEv2 tags; when APEv2 tags are
//! present, the same limitation applies.
//!
//! **To read tags from these formats**, use the native format-specific API:
//!
//! ```no_run
//! use audex::wavpack::WavPack;
//! use audex::FileType;
//!
//! let wv = WavPack::load("song.wv")?;
//! if let Some(ref tags) = wv.tags {
//!     // Use the native APEv2 get() which returns Option<&APEValue>
//!     if let Some(value) = tags.get("Title") {
//!         println!("Title: {}", value.as_string().unwrap_or_default());
//!     }
//! }
//! # Ok::<(), audex::AudexError>(())
//! ```
//!
//! Writing tags through `set()` works correctly for all formats, as the trait
//! implementation converts `Vec<String>` to the appropriate internal type.
//! The `keys()` method also works correctly for all formats.
//!
//! # Troubleshooting
//!
//! ## Unsupported Format Errors
//!
//! If you encounter `UnsupportedFormat` errors, ensure:
//! - The file extension matches the actual file format
//! - The file is not corrupted or truncated
//! - The format is supported by this library (check the module list)
//!
//! ## Tag Encoding Issues
//!
//! Text tags are expected to be UTF-8 encoded. If you encounter encoding errors:
//! - Verify the source text is valid UTF-8
//! - For ID3v2 tags, encoding is handled automatically
//! - For legacy formats (ID3v1), non-ASCII characters may be limited
//!
//! ## File Corruption Warnings
//!
//! Always work on copies of important files:
//! ```no_run
//! # use std::io;
//! // Create a backup before modifying
//! std::fs::copy("original.mp3", "original.mp3.backup")?;
//!
//! // Now safe to modify
//! // ... your code here ...
//! # Ok::<(), io::Error>(())
//! ```
//!
//! ## Async Feature Not Available
//!
//! If async methods are not available, enable the `async` feature:
//! ```toml
//! [dependencies]
//! audex = { version = "*", features = ["async"] }
//! ```

use std::error::Error as StdError;
use std::io::{Read, Seek, Write};
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

#[cfg(feature = "serde")]
use serde::{Serialize, Serializer};

// Internal tracing macros — must be declared before all other modules
// so that trace_event!, debug_event!, info_event!, warn_event!, and
// error_event! are available everywhere.
#[macro_use]
mod tracing_macros;

// Serde helpers — custom serialization for Duration, binary data, etc.
#[cfg(feature = "serde")]
pub mod serde_helpers;

// Snapshot — unified serializable view of audio metadata
#[cfg(feature = "serde")]
pub mod snapshot;

pub use file::{DynamicFileType, DynamicStreamInfo, File};

/// Version string - automatically derived from Cargo.toml
pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");

// Core modules
/// Genre constants, ID3v1 genre table, and genre string parsing utilities
pub mod constants;
/// Tag diffing — structured comparison of metadata between two files or states
pub mod diff;
mod file;
/// IFF (Interchange File Format) container primitives for AIFF and similar formats
pub mod iff;
/// Global parse limits — configurable ceilings on tag and image allocations
pub mod limits;
/// ReplayGain metadata reading, writing, and conversion utilities
pub mod replaygain;
/// RIFF (Resource Interchange File Format) container primitives for WAV and similar formats
pub mod riff;
/// Tag conversion and cross-format metadata transfer
pub mod tagmap;
/// Core tagging traits: [`Tags`], [`Metadata`], [`MetadataFields`], and [`BasicTags`]
pub mod tags;
/// Low-level utilities: binary readers, byte manipulation, file I/O helpers
pub mod util;
/// Vorbis Comment tagging support shared by FLAC, Ogg Vorbis, Ogg Opus, and related formats
pub mod vorbis;

// Format modules
/// ASF/WMA (Advanced Systems Format) metadata support
pub mod asf;
/// ID3v1/ID3v2 tag reading and writing (used by MP3, AIFF, WAV, and others)
pub mod id3;
/// Compatibility shim: re-exports [`mp4::MP4`] as `M4A`, [`easymp4::EasyMP4`] as `EasyM4A`, and [`easymp4::EasyMP4Tags`] as `EasyM4ATags`
pub mod m4a;
/// MP3 (MPEG Layer III) format support with ID3v1, ID3v2, and APEv2 tagging.
/// Includes [`mp3::EasyMP3`] for simplified tag access via the [`easyid3::EasyID3`] interface.
pub mod mp3;
/// MP4/M4A (MPEG-4 Part 14) format support with iTunes-style atom tags
pub mod mp4;

// Single-file formats
/// AAC (Advanced Audio Coding) stream info — ADTS/ADIF parsing (read-only, no tagging)
pub mod aac;
/// AC-3/E-AC-3 (Dolby Digital) stream info (read-only, no tagging)
pub mod ac3;
/// AIFF (Audio Interchange File Format) support with ID3v2 tagging
pub mod aiff;
/// APEv2 tag format used by Monkey's Audio, WavPack, Musepack, and others
pub mod apev2;
/// DSDIFF (DSD Interchange File Format) support with ID3v2 tagging
pub mod dsdiff;
/// DSF (DSD Stream File) support with ID3v2 tagging
pub mod dsf;
/// Simplified ID3v2 interface with human-readable key names for MP3 files
pub mod easyid3;
/// Simplified MP4/M4A tag interface with human-readable key names
pub mod easymp4;
/// FLAC (Free Lossless Audio Codec) support with Vorbis Comment tagging
pub mod flac;
/// Monkey's Audio (APE) lossless format with APEv2 tagging
pub mod monkeysaudio;
/// Musepack (MPC) format with APEv2 tagging
pub mod musepack;
/// Ogg container format primitives (pages, packets, streams)
pub mod ogg;
/// Ogg FLAC (FLAC in Ogg container) with Vorbis Comment tagging
pub mod oggflac;
/// Ogg Opus audio with Vorbis Comment tagging
pub mod oggopus;
/// Ogg Speex audio with Vorbis Comment tagging
pub mod oggspeex;
/// Ogg Theora video with Vorbis Comment tagging
pub mod oggtheora;
/// Ogg Vorbis audio with Vorbis Comment tagging
pub mod oggvorbis;
/// OptimFROG lossless format with APEv2 tagging
pub mod optimfrog;
/// SMF (Standard MIDI File) — duration-only parsing (read-only, no tagging)
pub mod smf;
/// TAK (Tom's Lossless Audio Kompressor) with APEv2 tagging
pub mod tak;
/// TrueAudio (TTA) lossless format with ID3v1/v2 and APEv2 tagging
pub mod trueaudio;
/// WAVE (WAV) format support with ID3v2 tagging
pub mod wave;
/// WavPack lossless/lossy format with APEv2 tagging
pub mod wavpack;

/// Primary error type for audio metadata operations
///
/// This enum covers all error conditions that can occur when loading, parsing,
/// or saving audio files and their metadata.
#[derive(Error, Debug)]
pub enum AudexError {
    /// Standard I/O error (file not found, permission denied, etc.)
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The file format is not recognized or supported
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    /// Data in the file is invalid or corrupt
    #[error("Invalid data: {0}")]
    InvalidData(String),

    /// Error parsing file structure or metadata
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Expected file/format header was not found
    #[error("Header not found")]
    HeaderNotFound,

    /// Format-specific error from an underlying parser
    #[error("Format-specific error: {0}")]
    FormatError(Box<dyn StdError + Send + Sync>),

    /// Requested operation is not supported for this format
    #[error("Operation not supported: {0}")]
    Unsupported(String),

    /// Feature exists but is not yet implemented
    #[error("Feature not implemented: {0}")]
    NotImplemented(String),

    /// Operation is invalid in the current state
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Internal error indicating a bug or inconsistency
    #[error("Internal error: {0}")]
    InternalError(String),

    /// ASF/WMA format-specific error
    #[error("ASF error: {0}")]
    ASF(#[from] asf::util::ASFError),

    /// FLAC file missing the fLaC magic header
    #[error("FLAC: No header found")]
    FLACNoHeader,

    /// Error in FLAC Vorbis Comment metadata block
    #[error("FLAC: Vorbis comment error")]
    FLACVorbis,

    /// APEv2 tag header not found in file
    #[error("APE: No header found")]
    APENoHeader,

    /// Invalid APEv2 tag item
    #[error("APE: Bad item error: {0}")]
    APEBadItem(String),

    /// APEv2 tag version not supported
    #[error("APE: Unsupported version")]
    APEUnsupportedVersion,

    /// WAV format error
    #[error("WAV error: {0}")]
    WAVError(String),

    /// Invalid RIFF/WAV chunk structure
    #[error("WAV: Invalid chunk: {0}")]
    WAVInvalidChunk(String),

    /// AAC format error
    #[error("AAC error: {0}")]
    AACError(String),

    /// AC3/E-AC3 format error
    #[error("AC3 error: {0}")]
    AC3Error(String),

    /// AIFF format error
    #[error("AIFF error: {0}")]
    AIFFError(String),

    /// IFF container format error
    #[error("IFF error: {0}")]
    IFFError(String),

    /// Musepack header parsing error
    #[error("Musepack header error: {0}")]
    MusepackHeaderError(String),

    /// TrueAudio (TTA) header parsing error
    #[error("TrueAudio header error: {0}")]
    TrueAudioHeaderError(String),

    /// TAK header parsing error
    #[error("TAK header error: {0}")]
    TAKHeaderError(String),

    /// WavPack header parsing error
    #[error("WAVPACK header error: {0}")]
    WavPackHeaderError(String),

    /// OptimFROG header parsing error
    #[error("OptimFROG header error: {0}")]
    OptimFROGHeaderError(String),

    /// Monkey's Audio (APE) header parsing error
    #[error("Monkey's Audio header error: {0}")]
    MonkeysAudioHeaderError(String),

    /// DSF (DSD) format error
    #[error("DSF error: {0}")]
    DSFError(String),

    /// Called method is not implemented for this type
    #[error("Method not implemented: {0}")]
    NotImplementedMethod(String),

    /// Tag operation not supported for the file's format
    #[error("Tag operation not supported for this format: {0}")]
    TagOperationUnsupported(String),

    /// ID3 tag header not found at expected position
    #[error("ID3 header not found - file doesn't start with an ID3 tag")]
    ID3NoHeaderError,

    /// ID3 unsynchronization data is malformed
    #[error("ID3 bad unsynchronization data")]
    ID3BadUnsynchData,

    /// ID3 frame data is shorter than expected
    #[error("ID3 frame too short: expected at least {expected} bytes, got {actual}")]
    ID3FrameTooShort { expected: usize, actual: usize },

    /// Container nesting depth exceeds the allowed maximum
    #[error("Nesting depth exceeds maximum allowed depth of {max_depth}")]
    DepthLimitExceeded { max_depth: u32 },
}

// ---------------------------------------------------------------------------
// Serde-friendly error representation
// ---------------------------------------------------------------------------

/// Serializable representation of an [`AudexError`].
///
/// `AudexError` contains non-serializable types (`Box<dyn Error>`,
/// `std::io::Error`), so it cannot derive `Serialize` directly. This
/// struct captures the error kind and human-readable message, which is
/// sufficient for JSON/TOML transport.
///
/// # Serialization
///
/// When the `serde` feature is enabled, this type implements
/// `Serialize` and `Deserialize`, allowing conversion to/from
/// JSON, TOML, and other serde-supported formats.
#[cfg(feature = "serde")]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SerializableError {
    /// Error variant name (e.g. "Io", "ParseError", "InvalidData")
    pub kind: String,
    /// Full human-readable error message
    pub message: String,
}

#[cfg(feature = "serde")]
impl From<&AudexError> for SerializableError {
    fn from(err: &AudexError) -> Self {
        // Derive the variant name from the Display prefix before the colon,
        // falling back to a generic label when the format doesn't include one.
        let kind = match err {
            AudexError::Io(_) => "Io",
            AudexError::UnsupportedFormat(_) => "UnsupportedFormat",
            AudexError::InvalidData(_) => "InvalidData",
            AudexError::ParseError(_) => "ParseError",
            AudexError::HeaderNotFound => "HeaderNotFound",
            AudexError::FormatError(_) => "FormatError",
            AudexError::Unsupported(_) => "Unsupported",
            AudexError::NotImplemented(_) => "NotImplemented",
            AudexError::InvalidOperation(_) => "InvalidOperation",
            AudexError::ASF(_) => "ASF",
            AudexError::FLACNoHeader => "FLACNoHeader",
            AudexError::FLACVorbis => "FLACVorbis",
            AudexError::APENoHeader => "APENoHeader",
            AudexError::APEBadItem(_) => "APEBadItem",
            AudexError::APEUnsupportedVersion => "APEUnsupportedVersion",
            AudexError::WAVError(_) => "WAVError",
            AudexError::WAVInvalidChunk(_) => "WAVInvalidChunk",
            AudexError::AACError(_) => "AACError",
            AudexError::AC3Error(_) => "AC3Error",
            AudexError::AIFFError(_) => "AIFFError",
            AudexError::IFFError(_) => "IFFError",
            AudexError::MusepackHeaderError(_) => "MusepackHeaderError",
            AudexError::TrueAudioHeaderError(_) => "TrueAudioHeaderError",
            AudexError::TAKHeaderError(_) => "TAKHeaderError",
            AudexError::WavPackHeaderError(_) => "WavPackHeaderError",
            AudexError::OptimFROGHeaderError(_) => "OptimFROGHeaderError",
            AudexError::MonkeysAudioHeaderError(_) => "MonkeysAudioHeaderError",
            AudexError::DSFError(_) => "DSFError",
            AudexError::NotImplementedMethod(_) => "NotImplementedMethod",
            AudexError::TagOperationUnsupported(_) => "TagOperationUnsupported",
            AudexError::ID3NoHeaderError => "ID3NoHeaderError",
            AudexError::ID3BadUnsynchData => "ID3BadUnsynchData",
            AudexError::ID3FrameTooShort { .. } => "ID3FrameTooShort",
            AudexError::DepthLimitExceeded { .. } => "DepthLimitExceeded",
            AudexError::InternalError(_) => "InternalError",
        };

        SerializableError {
            kind: kind.to_string(),
            message: err.to_string(),
        }
    }
}

#[cfg(feature = "serde")]
impl AudexError {
    /// Convert this error into a serializable representation.
    pub fn to_serializable(&self) -> SerializableError {
        SerializableError::from(self)
    }
}

/// Manual `Serialize` for `AudexError` — delegates to [`SerializableError`].
#[cfg(feature = "serde")]
impl Serialize for AudexError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.to_serializable().serialize(serializer)
    }
}

/// Convenience result type used throughout the library
pub type Result<T> = std::result::Result<T, AudexError>;

// Tags and metadata exports
pub use tags::{BasicTags, Metadata, MetadataFields, PaddingInfo, Tags};

// Tag conversion re-exports
pub use tagmap::{
    ConversionOptions, ConversionReport, SkipReason, StandardField, TagMap, convert_tags,
    convert_tags_with_options,
};

// Diff re-exports
pub use diff::{
    DiffOptions, FieldChange, FieldEntry, StreamInfoDiff, TagDiff, diff_normalized,
    diff_normalized_with_options,
};

// Snapshot re-exports (enabled with "serde" feature)
#[cfg(feature = "serde")]
pub use snapshot::{StreamInfoSnapshot, TagSnapshot};

// Async exports (enabled with "async" feature)
// Format detection
#[cfg(feature = "async")]
pub use file::detect_format_async;
pub use file::detect_ogg_format;

// MP4 clear function
#[cfg(feature = "async")]
pub use mp4::clear_async as mp4_clear_async;

// OGG utility exports
#[cfg(feature = "async")]
pub use ogg::seek_end_async;

// IFF async types and functions
#[cfg(feature = "async")]
pub use iff::{
    IffChunkAsync, IffFileAsync, RiffFileAsync, delete_chunk_async, insert_iff_chunk_async,
    insert_riff_chunk_async, resize_iff_chunk_async, resize_riff_chunk_async,
    update_iff_file_size_async, update_riff_file_size_async,
};

// AIFF async functions
#[cfg(feature = "async")]
pub use aiff::{clear_async as aiff_clear_async, open_async as aiff_open_async};

// WAVE async functions
#[cfg(feature = "async")]
pub use wave::{clear_async as wave_clear_async, open_async as wave_open_async};

// Async utility functions
#[cfg(feature = "async")]
pub use util::{
    delete_bytes_async, insert_bytes_async, loadfile_read_async, loadfile_write_async,
    read_full_async, resize_bytes_async,
};

// Buffer size constant (shared between sync and async)
pub use util::DEFAULT_BUFFER_SIZE;

// Global parse limits
pub use limits::ParseLimits;

/// Detect the audio format of a file without fully loading it.
///
/// Uses a multi-stage scoring system (magic bytes, extension, content analysis)
/// to identify the format. Returns the format name as a string.
///
/// # Examples
///
/// ```no_run
/// let format = audex::detect_format("music.mp3")?;
/// println!("Detected: {}", format);
/// # Ok::<(), audex::AudexError>(())
/// ```
pub use file::detect_format;
pub use file::detect_format_from_bytes;

/// Combined trait for types that implement both [`Read`] and [`Seek`].
///
/// This trait is automatically implemented for all types that implement both
/// `Read` and `Seek`, enabling their use as trait objects (`&mut dyn ReadSeek`)
/// for reader-based file loading without requiring a filesystem path.
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// Combined trait for types that implement [`Read`], [`Write`], and [`Seek`].
///
/// This trait is automatically implemented for all types that implement all three
/// traits, enabling their use as trait objects (`&mut dyn ReadWriteSeek`) for
/// writer-based saving without requiring a filesystem path.
///
/// Common implementors include `std::io::Cursor<Vec<u8>>` and `std::fs::File`.
pub trait ReadWriteSeek: Read + Write + Seek {}
impl<T: Read + Write + Seek> ReadWriteSeek for T {}

/// Base trait for all file format implementations
///
/// This trait defines the interface that each audio format (MP3, FLAC, MP4, etc.)
/// must implement. It provides:
///
/// - **Associated types**: `Tags` (the format's tag type) and `Info` (stream info type)
/// - **File I/O**: `load`, `save`, `clear`
/// - **Tag access**: Key-value `get`/`set`/`remove` methods that delegate to the
///   associated `Tags` implementation
/// - **Format detection**: `score` and `mime_types` for automatic format identification
///
/// For format-agnostic runtime usage, see `file::DynamicFileType` which wraps
/// any `FileType` implementor and is returned by [`File::load`].
///
/// The key-value methods (`get`, `set`, `remove`, etc.) on this trait return
/// `Result<()>` (for mutators) and `Option<Vec<String>>` (for accessors), wrapping
/// the underlying [`Tags`] trait methods.
pub trait FileType {
    /// The tag type used by this format (e.g., `ID3Tags`, `VCommentDict`, `MP4Tags`)
    type Tags: Tags;
    /// The stream info type for this format (e.g., `MPEGInfo`, `FLACStreamInfo`)
    type Info: StreamInfo;

    /// Load a file from the given path
    fn load<P: AsRef<Path>>(path: P) -> Result<Self>
    where
        Self: Sized;

    /// Save metadata back to the file
    fn save(&mut self) -> Result<()>;

    /// Clear all metadata from the file
    fn clear(&mut self) -> Result<()>;

    /// Save metadata to a writer that implements Read + Write + Seek.
    ///
    /// The writer must contain the complete original file data (audio + any
    /// existing tags). The method modifies the writer in-place, just as
    /// [`save`](Self::save) modifies a file on disk.
    ///
    /// Not all formats support writer-based saving. The default returns an error.
    fn save_to_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        let _ = writer;
        Err(AudexError::Unsupported(
            "This format does not support saving to a writer".to_string(),
        ))
    }

    /// Clear all metadata from a writer that implements Read + Write + Seek.
    ///
    /// The writer must contain the complete original file data.
    ///
    /// Not all formats support writer-based clearing. The default returns an error.
    fn clear_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        let _ = writer;
        Err(AudexError::Unsupported(
            "This format does not support clearing via a writer".to_string(),
        ))
    }

    /// Save metadata to a file at the given path.
    ///
    /// The target file must already exist and contain valid audio data for
    /// this format. The method opens it in read-write mode and modifies the
    /// metadata in-place, just as [`save`](Self::save) modifies the stored
    /// file.
    ///
    /// This enables two workflows that [`save`](Self::save) alone cannot:
    /// - Load from one path, save to a different path (after copying the file)
    /// - Load from a reader (where `filename` is `None`), save to a file path
    ///
    /// Not all formats support path-based saving. The default returns an error.
    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        let _ = path;
        Err(AudexError::Unsupported(
            "This format does not support saving to a path".to_string(),
        ))
    }

    /// Get the file's tags, if any
    fn tags(&self) -> Option<&Self::Tags>;

    /// Get mutable access to tags
    fn tags_mut(&mut self) -> Option<&mut Self::Tags>;

    /// Get stream information
    fn info(&self) -> &Self::Info;

    /// Score this format's confidence in handling the given file
    ///
    /// Returns 0 if format cannot handle the file. Higher positive values
    /// indicate more confidence. The return type is `i32` (unbounded).
    fn score(filename: &str, header: &[u8]) -> i32;

    /// MIME types supported by this format
    fn mime_types() -> &'static [&'static str];

    /// Stable, human-readable identifier for this format (e.g. "MP3", "FLAC").
    ///
    /// Used by the dynamic file wrapper to dispatch async operations to the
    /// correct concrete type. Unlike `std::any::type_name`, this value is
    /// guaranteed to remain the same across compiler versions.
    ///
    /// The default implementation falls back to extracting the short name
    /// from `std::any::type_name`, but concrete types should override this
    /// to return a hardcoded string literal.
    fn format_id() -> &'static str
    where
        Self: Sized,
    {
        let full = std::any::type_name::<Self>();
        match full.rsplit("::").next() {
            Some(short) => short,
            None => full,
        }
    }

    /// Load from a reader that implements [`Read`] + [`Seek`].
    ///
    /// This allows loading metadata from in-memory buffers, network streams,
    /// or any other source without requiring a file path on disk. The loaded
    /// instance will have no associated filename and [`save`](Self::save) may
    /// not work unless the format supports it.
    ///
    /// Not all formats support reader-based loading. The default implementation
    /// returns an [`AudexError::Unsupported`] error.
    fn load_from_reader(reader: &mut dyn ReadSeek) -> Result<Self>
    where
        Self: Sized,
    {
        let _ = reader;
        Err(AudexError::Unsupported(
            "This format does not support loading from a reader".to_string(),
        ))
    }

    /// Add tags to the file if not already present
    ///
    /// Creates an appropriate tag format for this file type.
    /// Behavior when tags already exist is format-dependent.
    fn add_tags(&mut self) -> Result<()> {
        Err(AudexError::NotImplemented(
            "Adding tags not supported for this format".to_string(),
        ))
    }

    /// Get the filename associated with this file, if any
    fn filename(&self) -> Option<&str> {
        None
    }

    /// Returns a list of MIME types for this file instance
    fn mime(&self) -> Vec<&'static str> {
        Self::mime_types().to_vec()
    }

    /// Pretty print the file's stream information and tags
    ///
    /// Returns a formatted string containing stream information and tag key=value pairs
    fn pprint(&self) -> String {
        let stream_info = self.info().pprint();
        let mime_types = self.mime();
        let mime_type = mime_types.first().unwrap_or(&"application/octet-stream");
        let stream_line = format!("{} ({})", stream_info, mime_type);

        if let Some(tags) = self.tags() {
            let tags_str = tags.pprint();
            if !tags_str.is_empty() {
                format!("{}\n{}", stream_line, tags_str)
            } else {
                stream_line
            }
        } else {
            stream_line
        }
    }

    /// Get values for a tag key
    fn get(&self, key: &str) -> Option<Vec<String>> {
        self.tags()?.get(key).map(|slice| slice.to_vec())
    }

    /// Set values for a tag key
    fn set(&mut self, key: &str, values: Vec<String>) -> Result<()> {
        if let Some(tags) = self.tags_mut() {
            tags.set(key, values);
            Ok(())
        } else {
            Err(AudexError::Unsupported(
                "This format does not support tags".to_string(),
            ))
        }
    }

    /// Remove a tag key
    fn remove(&mut self, key: &str) -> Result<()> {
        if let Some(tags) = self.tags_mut() {
            tags.remove(key);
            Ok(())
        } else {
            Err(AudexError::Unsupported(
                "This format does not support tags".to_string(),
            ))
        }
    }

    /// Get all tag keys
    fn keys(&self) -> Vec<String> {
        self.tags().map(|t| t.keys()).unwrap_or_default()
    }

    /// Check if tag key exists
    fn contains_key(&self, key: &str) -> bool {
        self.tags().map(|t| t.contains_key(key)).unwrap_or(false)
    }

    /// Get the first value for a key, if any
    fn get_first(&self, key: &str) -> Option<String> {
        self.get(key)?.into_iter().next()
    }

    /// Set a single value for a key
    fn set_single(&mut self, key: &str, value: String) -> Result<()> {
        self.set(key, vec![value])
    }

    /// Get number of tag pairs
    fn len(&self) -> usize {
        self.keys().len()
    }

    /// Check if no tags are present
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get all values as (key, values) pairs
    fn items(&self) -> Vec<(String, Vec<String>)> {
        let mut items = Vec::new();
        for key in self.keys() {
            if let Some(values) = self.get(&key) {
                items.push((key, values));
            }
        }
        items
    }

    /// Get all values as a flat list
    fn values(&self) -> Vec<Vec<String>> {
        self.items().into_iter().map(|(_, v)| v).collect()
    }

    /// Update tags from another set of key-value pairs
    fn update(&mut self, other: Vec<(String, Vec<String>)>) -> Result<()> {
        for (key, values) in other {
            self.set(&key, values)?;
        }
        Ok(())
    }

    /// Get a value with a default if not present
    fn get_or(&self, key: &str, default: Vec<String>) -> Vec<String> {
        self.get(key).unwrap_or(default)
    }

    /// Pop a key and return its values
    fn pop(&mut self, key: &str) -> Result<Option<Vec<String>>> {
        let values = self.get(key);
        if values.is_some() {
            self.remove(key)?;
        }
        Ok(values)
    }

    /// Pop a key with a default value
    fn pop_or(&mut self, key: &str, default: Vec<String>) -> Result<Vec<String>> {
        Ok(self.pop(key)?.unwrap_or(default))
    }
}

/// Stream information trait for audio properties
///
/// Implemented by each format's info type (e.g., `MPEGInfo`, `FLACStreamInfo`).
/// Methods return `Option` because not all formats can determine all properties
/// (e.g., bitrate may be unknown for some VBR streams, bits_per_sample is not
/// applicable to lossy formats).
pub trait StreamInfo {
    /// Length of the audio stream
    fn length(&self) -> Option<Duration>;

    /// Bitrate in bits per second
    fn bitrate(&self) -> Option<u32>;

    /// Sample rate in Hz
    fn sample_rate(&self) -> Option<u32>;

    /// Number of channels
    fn channels(&self) -> Option<u16>;

    /// Bits per sample
    fn bits_per_sample(&self) -> Option<u16>;

    /// Pretty print stream information
    fn pprint(&self) -> String {
        let mut output = String::new();

        if let Some(length) = self.length() {
            output.push_str(&format!("Length: {:.2}s\n", length.as_secs_f64()));
        }

        if let Some(bitrate) = self.bitrate() {
            output.push_str(&format!("Bitrate: {} bps\n", bitrate));
        }

        if let Some(sample_rate) = self.sample_rate() {
            output.push_str(&format!("Sample Rate: {} Hz\n", sample_rate));
        }

        if let Some(channels) = self.channels() {
            output.push_str(&format!("Channels: {}\n", channels));
        }

        if let Some(bits_per_sample) = self.bits_per_sample() {
            output.push_str(&format!("Bits per Sample: {}\n", bits_per_sample));
        }

        if output.is_empty() {
            "<No stream information available>".to_string()
        } else {
            output.trim_end().to_string()
        }
    }
}
