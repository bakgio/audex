//! Support for Ogg Speex audio files.
//!
//! This module provides comprehensive support for Ogg Speex files, a legacy codec
//! primarily designed for speech compression. Speex excels at low bitrate voice
//! encoding and was widely used for VoIP applications before being superseded by
//! Opus (which combines Speex's SILK technology with CELT).
//!
//! # File Format
//!
//! Ogg Speex files consist of:
//! - **Ogg container**: Flexible container format supporting multiplexing
//! - **Speex codec**: Lossy speech compression optimized for human voice
//! - **Vorbis Comments**: Standard Ogg metadata tagging format
//!
//! ## Structure
//!
//! An Ogg Speex file contains these packet types:
//!
//! 1. **Identification Header** (`Speex   `): 80-byte header with codec parameters
//! 2. **Comment Header**: Vorbis Comment metadata (TITLE, ARTIST, etc.)
//! 3. **Audio Data**: Compressed Speex audio frames
//!
//! # Audio Characteristics
//!
//! - **Codec Type**: Lossy speech compression (not suitable for music)
//! - **Sample Rates**: 8 kHz (narrowband), 16 kHz (wideband), 32 kHz (ultra-wideband)
//! - **Bitrate**: 2.15 - 44 kbps, variable bitrate (VBR) supported
//! - **Channels**: 1 (mono) or 2 (stereo), optimized for mono speech
//! - **Frame Size**: Typically 160 samples (20 ms at 8 kHz)
//! - **File Extension**: `.spx`
//! - **MIME Type**: `audio/x-speex`
//!
//! ## Use Cases
//!
//! Speex was designed for:
//! - **VoIP applications**: Low latency voice communication
//! - **Podcasts and audiobooks**: Speech-focused content
//! - **Voice recording**: Interviews, dictation, voice notes
//! - **Low bandwidth scenarios**: Dial-up or limited network connections
//!
//! **Note**: For new applications, consider using Opus instead, which provides
//! better quality and efficiency while maintaining Speex compatibility.
//!
//! # Tagging
//!
//! Ogg Speex uses Vorbis Comments for metadata, supporting:
//!
//! - **Multi-value fields**: Multiple values per tag (e.g., multiple artists)
//! - **UTF-8 encoding**: Full Unicode support for international text
//! - **Standard fields**: TITLE, ARTIST, ALBUM, DATE, GENRE, DESCRIPTION, etc.
//! - **Case-insensitive keys**: Field names normalized to uppercase
//! - **Embedded pictures**: Via METADATA_BLOCK_PICTURE field
//!
//! # Basic Usage
//!
//! ## Loading and Reading Information
//!
//! ```no_run
//! use audex::oggspeex::OggSpeex;
//! use audex::{FileType, StreamInfo};
//!
//! # fn main() -> audex::Result<()> {
//! let speex = OggSpeex::load("speech.spx")?;
//!
//! // Access stream information
//! println!("Sample Rate: {} Hz", speex.info.sample_rate);
//! println!("Channels: {}", speex.info.channels);
//! println!("Version: {}", speex.info.version);
//! println!("VBR: {}", if speex.info.vbr != 0 { "Yes" } else { "No" });
//!
//! if let Some(duration) = speex.info.length {
//!     println!("Duration: {:.2} seconds", duration.as_secs_f64());
//! }
//!
//! if let Some(bitrate) = speex.info.bitrate {
//!     println!("Bitrate: {} bps", bitrate);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Reading and Modifying Tags
//!
//! ```no_run
//! use audex::oggspeex::OggSpeex;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut speex = OggSpeex::load("speech.spx")?;
//!
//! // Read existing tags
//! if let Some(tags) = speex.tags() {
//!     if let Some(title) = tags.get_first("TITLE") {
//!         println!("Title: {}", title);
//!     }
//!     if let Some(artist) = tags.get_first("ARTIST") {
//!         println!("Artist: {}", artist);
//!     }
//! }
//!
//! // Modify tags
//! if let Some(tags) = speex.tags_mut() {
//!     tags.set_single("TITLE", "Podcast Episode 1".to_string());
//!     tags.set_single("ARTIST", "Podcast Host".to_string());
//!     tags.set_single("ALBUM", "Podcast Series".to_string());
//!     tags.set_single("DATE", "2024".to_string());
//!     tags.set_single("DESCRIPTION", "Episode description".to_string());
//! }
//!
//! // Save changes
//! speex.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Creating Tags
//!
//! ```no_run
//! use audex::oggspeex::OggSpeex;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut speex = OggSpeex::load("speech.spx")?;
//!
//! // Create tags if they don't exist
//! if speex.tags.is_none() {
//!     speex.add_tags()?;
//! }
//!
//! if let Some(tags) = speex.tags_mut() {
//!     tags.set_single("TITLE", "New Recording".to_string());
//!     tags.set_single("GENRE", "Podcast".to_string());
//! }
//!
//! speex.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Removing All Metadata
//!
//! ```no_run
//! use audex::oggspeex::OggSpeex;
//! use audex::FileType;
//!
//! # fn main() -> audex::Result<()> {
//! let mut speex = OggSpeex::load("speech.spx")?;
//! speex.clear()?;  // Removes all tags
//! # Ok(())
//! # }
//! ```
//!
//! # Asynchronous Operations
//!
//! When the `async` feature is enabled:
//!
//! ```ignore
//! // Note: This example requires the `async` feature and a tokio runtime.
//! // Enable with: audex = { version = "*", features = ["async"] }
//! use audex::oggspeex::OggSpeex;
//! use audex::{FileType, Tags};
//!
//! # async fn example() -> audex::Result<()> {
//! let mut speex = OggSpeex::load_async("speech.spx").await?;
//!
//! if let Some(tags) = speex.tags_mut() {
//!     tags.set_single("TITLE", "Async Recording".to_string());
//! }
//!
//! speex.save_async().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Technical Details
//!
//! ## Speex Modes
//!
//! Speex supports three modes based on sample rate:
//!
//! - **Narrowband (Mode 0)**: 8 kHz - Telephone quality, minimal bandwidth
//! - **Wideband (Mode 1)**: 16 kHz - Improved clarity for VoIP
//! - **Ultra-wideband (Mode 2)**: 32 kHz - Near-CD quality for speech
//!
//! ## Bitrate and Quality
//!
//! Speex supports both constant bitrate (CBR) and variable bitrate (VBR):
//!
//! - **CBR**: Fixed bitrate throughout the stream
//! - **VBR**: Adapts bitrate based on audio complexity
//! - **Quality Range**: 0-10, where higher values mean better quality
//!
//! ## Identification Header
//!
//! The 80-byte identification header contains:
//!
//! ```text
//! Bytes  0-7  : "Speex   " (5-byte signature + 3 trailing spaces, 8 bytes total)
//! Bytes  8-27 : Speex version string (e.g., "speex-1.2")
//! Bytes 28-31 : Numeric version ID
//! Bytes 32-35 : Header size (always 80)
//! Bytes 36-39 : Sample rate in Hz
//! Bytes 40-43 : Mode (0=narrowband, 1=wideband, 2=ultra-wideband)
//! Bytes 44-47 : Mode bitstream version
//! Bytes 48-51 : Number of channels
//! Bytes 52-55 : Bitrate (-1 for VBR, otherwise bitrate in bps)
//! Bytes 56-59 : Frame size in samples
//! Bytes 60-63 : VBR flag (0=CBR, 1=VBR)
//! Bytes 64-67 : Frames per packet
//! Bytes 68-71 : Extra headers count
//! Bytes 72-75 : Reserved
//! Bytes 76-79 : Reserved
//! ```
//!
//! # Error Handling
//!
//! Common errors when working with Ogg Speex files:
//!
//! - **`SpeexHeaderError`**: Invalid or missing Speex identification header
//! - **`SpeexError`**: General Speex parsing or validation errors
//! - **`VorbisError`**: Invalid Vorbis Comment metadata
//!
//! # Migration to Opus
//!
//! If you're working with Speex files, consider migrating to Opus:
//!
//! - **Better quality**: Opus provides superior quality at the same bitrate
//! - **Lower latency**: Optimized for real-time communication
//! - **Wider range**: Supports both speech and music
//! - **Standardized**: IETF standard (RFC 6716) with broad support
//!
//! # References
//!
//! - [Speex Codec Specification](https://www.speex.org/docs/manual/speex-manual/)
//! - [Ogg Speex Mapping](https://www.speex.org/docs/manual/speex-manual/node8.html)
//! - [Vorbis Comment Specification](https://www.xiph.org/vorbis/doc/v-comment.html)

use crate::VERSION_STRING;
use crate::ogg::OggPage;
use crate::vorbis::VCommentDict;
use crate::{AudexError, FileType, Result, StreamInfo};
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};
#[cfg(feature = "async")]
use tokio::io::{AsyncSeekExt, BufReader as TokioBufReader};

/// General error type for Speex codec operations.
///
/// Raised when there are problems reading or parsing Speex data, such as
/// invalid headers, malformed packets, or unsupported codec parameters.
///
/// # Examples
///
/// ```
/// use audex::oggspeex::SpeexError;
///
/// let error = SpeexError("Invalid Speex version".to_string());
/// println!("Error: {}", error);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SpeexError(pub String);

impl std::fmt::Display for SpeexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for SpeexError {}

/// Error type for Speex header parsing failures.
///
/// This error occurs when the Speex identification header is invalid, missing,
/// or contains unexpected values. Common causes include:
///
/// - Missing or invalid `Speex   ` signature (8 bytes with 3 spaces)
/// - Truncated identification packet (less than 80 bytes)
/// - Zero sample rate or channel count
/// - No Speex stream found in Ogg container
/// - Unsupported Speex version
///
/// # Examples
///
/// ```
/// use audex::oggspeex::{SpeexHeaderError, SpeexError};
///
/// let error = SpeexHeaderError(SpeexError("Sample rate cannot be zero".to_string()));
/// println!("Header error: {}", error);
/// // Output: "Speex header error: Sample rate cannot be zero"
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct SpeexHeaderError(pub SpeexError);

impl std::fmt::Display for SpeexHeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Speex header error: {}", self.0)
    }
}

impl std::error::Error for SpeexHeaderError {}

impl From<SpeexError> for SpeexHeaderError {
    fn from(err: SpeexError) -> Self {
        SpeexHeaderError(err)
    }
}

impl From<SpeexHeaderError> for AudexError {
    fn from(err: SpeexHeaderError) -> Self {
        AudexError::InvalidData(err.to_string())
    }
}

impl From<SpeexError> for AudexError {
    fn from(err: SpeexError) -> Self {
        AudexError::InvalidData(err.0)
    }
}

/// Audio stream information for Ogg Speex files.
///
/// Contains detailed technical information about a Speex audio stream, including
/// codec parameters, encoding settings, and stream properties extracted from the
/// 80-byte Speex identification header.
///
/// # Common Fields
///
/// - **`length`**: Duration of the audio calculated from final page granule position
/// - **`channels`**: Number of audio channels (1 for mono, 2 for stereo)
/// - **`sample_rate`**: Sample rate in Hz (8000, 16000, or 32000)
/// - **`bitrate`**: Target bitrate in bps (-1 for VBR, 0+ for CBR)
/// - **`serial`**: Ogg logical stream serial number
///
/// # Speex-Specific Fields
///
/// - **`version`**: Speex version string (e.g., "speex-1.2")
/// - **`version_id`**: Numeric version identifier
/// - **`mode`**: Encoding mode (0=narrowband, 1=wideband, 2=ultra-wideband)
/// - **`mode_bitstream_version`**: Bitstream version for the mode
/// - **`nb_channels`**: Number of channels (duplicates `channels`)
/// - **`nb_frames`**: Number of frames per packet
/// - **`frame_size`**: Frame size in samples
/// - **`vbr`**: Variable bitrate flag (0=CBR, 1=VBR)
/// - **`frames_per_packet`**: Frames bundled per Ogg packet
/// - **`extra_headers`**: Count of extra header packets
/// - **`reserved1`**, **`reserved2`**: Reserved for future use
///
/// # Encoding Modes
///
/// Speex defines three encoding modes based on sample rate:
///
/// - **Mode 0 (Narrowband)**: 8 kHz - Optimized for telephone-quality speech
/// - **Mode 1 (Wideband)**: 16 kHz - Enhanced clarity for VoIP
/// - **Mode 2 (Ultra-wideband)**: 32 kHz - High-quality speech reproduction
///
/// # Examples
///
/// ```no_run
/// use audex::oggspeex::OggSpeex;
/// use audex::FileType;
///
/// # fn main() -> audex::Result<()> {
/// let speex = OggSpeex::load("speech.spx")?;
/// let info = &speex.info;
///
/// // Display basic information
/// println!("Sample Rate: {} Hz", info.sample_rate);
/// println!("Channels: {}", info.channels);
/// println!("Speex Version: {}", info.version);
///
/// // Check encoding mode
/// let mode_name = match info.mode {
///     0 => "Narrowband (8 kHz)",
///     1 => "Wideband (16 kHz)",
///     2 => "Ultra-wideband (32 kHz)",
///     _ => "Unknown",
/// };
/// println!("Mode: {}", mode_name);
///
/// // Check bitrate mode
/// if info.vbr != 0 {
///     println!("Encoding: Variable Bitrate (VBR)");
/// } else if let Some(bitrate) = info.bitrate {
///     println!("Encoding: Constant Bitrate ({} bps)", bitrate);
/// }
///
/// // Display duration
/// if let Some(duration) = info.length {
///     println!("Duration: {:.2} seconds", duration.as_secs_f64());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default)]
pub struct SpeexInfo {
    /// Duration of the audio stream
    pub length: Option<Duration>,
    /// Number of audio channels (1 or 2)
    pub channels: u16,
    /// Sample rate in Hz (8000, 16000, or 32000)
    pub sample_rate: u32,
    /// Bitrate in bps (-1 for VBR, 0+ for CBR)
    pub bitrate: Option<i32>,
    /// Ogg logical stream serial number
    pub serial: u32,

    /// Speex version string (e.g., "speex-1.2")
    pub version: String,
    /// Numeric version identifier
    pub version_id: u32,
    /// Encoding mode (0=narrowband, 1=wideband, 2=ultra-wideband)
    pub mode: i32,
    /// Bitstream version for the encoding mode
    pub mode_bitstream_version: i32,
    /// Number of channels (same as `channels`)
    pub nb_channels: u32,
    /// Number of frames (reserved, typically 0)
    pub nb_frames: i32,
    /// Frame size in samples
    pub frame_size: u32,
    /// Variable bitrate flag (0=CBR, 1=VBR)
    pub vbr: i32,
    /// Number of frames per Ogg packet
    pub frames_per_packet: u32,
    /// Number of extra header packets
    pub extra_headers: u32,
    /// Reserved field (unused)
    pub reserved1: u32,
    /// Reserved field (unused)
    pub reserved2: u32,
}

impl SpeexInfo {
    /// Create SpeexInfo by parsing from a readable source
    ///
    /// Parse OGG pages to find the Speex stream
    /// the Speex stream starting with b"Speex   ".
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut speex_info = Self::default();
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;

        // Find the Speex stream by looking for the Speex identification packet
        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(SpeexHeaderError(SpeexError(
                    "No Speex stream found within page limit".to_string(),
                ))
                .into());
            }

            let page = match OggPage::from_reader(reader) {
                Ok(page) => page,
                Err(_) => {
                    return Err(
                        SpeexHeaderError(SpeexError("No Speex stream found".to_string())).into(),
                    );
                }
            };

            // Look for Speex identification packet starting with "Speex   "
            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 8 && &first_packet[0..8] == b"Speex   " {
                    speex_info.serial = page.serial;
                    speex_info.parse_identification_packet(first_packet)?;

                    // Calculate length from final page
                    speex_info.post_tags(reader)?;

                    return Ok(speex_info);
                }
            }
        }
    }

    /// Parse the Speex identification header
    ///
    /// Format follows specification: 80-byte header with specific layout
    fn parse_identification_packet(&mut self, packet: &[u8]) -> Result<()> {
        if packet.len() < 80 {
            return Err(SpeexHeaderError(SpeexError(
                "Invalid Speex identification packet length".to_string(),
            ))
            .into());
        }

        // Skip "Speex   " (8 bytes) and parse header
        let mut cursor = Cursor::new(&packet[8..]);

        // Parse version string (20 bytes)
        let mut version_bytes = [0u8; 20];
        cursor.read_exact(&mut version_bytes)?;
        // Find null terminator and create string
        let version_len = version_bytes.iter().position(|&x| x == 0).unwrap_or(20);
        self.version = String::from_utf8_lossy(&version_bytes[..version_len]).into_owned();

        // Parse numeric version (4 bytes, little-endian)
        self.version_id = cursor.read_u32::<LittleEndian>()?;

        // Parse header size (should be 80 - 4 bytes)
        let _header_size = cursor.read_u32::<LittleEndian>()?;

        // Parse sample rate (4 bytes, little-endian) - bytes 36-39 from start
        self.sample_rate = cursor.read_u32::<LittleEndian>()?;

        // Parse mode (4 bytes)
        self.mode = cursor.read_i32::<LittleEndian>()?;

        // Parse mode bitstream version (4 bytes)
        self.mode_bitstream_version = cursor.read_i32::<LittleEndian>()?;

        // Parse nb_channels (4 bytes, little-endian) - bytes 48-51 from start
        self.nb_channels = cursor.read_u32::<LittleEndian>()?;

        // Reject channel counts that exceed u16 range to prevent silent truncation
        if self.nb_channels > u16::MAX as u32 {
            return Err(SpeexHeaderError(SpeexError(format!(
                "Channel count {} exceeds maximum supported value of {}",
                self.nb_channels,
                u16::MAX
            )))
            .into());
        }
        self.channels = self.nb_channels as u16;

        // Parse bitrate (4 bytes, signed little-endian) - bytes 52-55 from start.
        // Speex uses -1 to signal "VBR with unknown bitrate." Store negative values
        // as None to preserve the semantic distinction between "unknown" and "zero."
        let raw_bitrate = cursor.read_i32::<LittleEndian>()?;
        self.bitrate = if raw_bitrate >= 0 {
            Some(raw_bitrate)
        } else {
            None
        };

        // Parse frame_size (4 bytes)
        self.frame_size = cursor.read_u32::<LittleEndian>()?;

        // Parse vbr flag (4 bytes)
        self.vbr = cursor.read_i32::<LittleEndian>()?;

        // Parse frames_per_packet (4 bytes)
        self.frames_per_packet = cursor.read_u32::<LittleEndian>()?;

        // Parse extra_headers (4 bytes)
        self.extra_headers = cursor.read_u32::<LittleEndian>()?;

        // Parse reserved fields (4 bytes each)
        self.reserved1 = cursor.read_u32::<LittleEndian>()?;
        self.reserved2 = cursor.read_u32::<LittleEndian>()?;

        // Validate fields
        if self.sample_rate == 0 {
            return Err(
                SpeexHeaderError(SpeexError("Sample rate cannot be zero".to_string())).into(),
            );
        }

        if self.nb_channels == 0 {
            return Err(
                SpeexHeaderError(SpeexError("Channel count cannot be zero".to_string())).into(),
            );
        }

        Ok(())
    }

    /// Calculate stream length from audio data
    ///
    /// Finds the final page with this serial number to calculate accurate length.
    fn post_tags<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Gracefully handle truncated files by leaving length as None
        // rather than failing outright, matching OggFLAC behavior.
        let last_page = match OggPage::find_last_with_finishing(reader, self.serial, true)? {
            Some(page) => page,
            None => return Ok(()),
        };
        if last_page.position >= 0 && self.sample_rate > 0 {
            // Calculate duration from position
            let duration_secs = last_page.position as f64 / self.sample_rate as f64;
            if duration_secs.is_finite() && duration_secs >= 0.0 && duration_secs <= u64::MAX as f64
            {
                self.length = Some(Duration::from_secs_f64(duration_secs));
            }
        }
        Ok(())
    }

    /// Format stream information for display
    ///
    /// Returns a formatted string describing the Speex stream.
    pub fn pprint(&self) -> String {
        let duration = self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0);

        format!("Ogg Speex, {:.2} seconds", duration)
    }
}

impl StreamInfo for SpeexInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        // Convert signed bitrate to unsigned if positive
        self.bitrate
            .and_then(|b| if b > 0 { Some(b as u32) } else { None })
    }

    fn sample_rate(&self) -> Option<u32> {
        if self.sample_rate > 0 {
            Some(self.sample_rate)
        } else {
            None
        }
    }

    fn channels(&self) -> Option<u16> {
        if self.channels > 0 {
            Some(self.channels)
        } else {
            None
        }
    }

    fn bits_per_sample(&self) -> Option<u16> {
        None // Speex is lossy, no fixed bits per sample
    }
}

/// Vorbis Comment metadata container for Ogg Speex files.
///
/// Wraps a [`VCommentDict`] to provide metadata tagging for Speex audio files.
/// Speex uses the same Vorbis Comment format as Ogg Vorbis and Ogg Opus.
///
/// # Fields
///
/// - **`inner`**: The underlying [`VCommentDict`] containing tag key-value pairs
/// - **`serial`**: Ogg stream serial number identifying this Speex stream
/// - **`padding`**: Extra padding bytes after the comment data
///
/// # Tag Format
///
/// Vorbis Comments are UTF-8 key-value pairs:
/// - Keys are case-insensitive (normalized to uppercase)
/// - Values are UTF-8 strings
/// - Multiple values per key are supported
/// - Common fields: TITLE, ARTIST, ALBUM, DATE, GENRE, DESCRIPTION, etc.
///
/// # Deref Behavior
///
/// This struct implements `Deref` and `DerefMut` to [`VCommentDict`], providing
/// direct access to all tagging methods.
///
/// # Examples
///
/// ```no_run
/// use audex::oggspeex::OggSpeex;
/// use audex::{FileType, Tags};
///
/// # fn main() -> audex::Result<()> {
/// let mut speex = OggSpeex::load("speech.spx")?;
///
/// if let Some(tags) = speex.tags_mut() {
///     // Set single-value tags
///     tags.set_single("TITLE", "Podcast Episode 42".to_string());
///     tags.set_single("ARTIST", "Host Name".to_string());
///     tags.set_single("DATE", "2024".to_string());
///     tags.set_single("DESCRIPTION", "Episode summary".to_string());
///
///     // Set multi-value tag (multiple artists)
///     tags.set("ARTIST", vec![
///         "Host 1".to_string(),
///         "Host 2".to_string(),
///     ]);
///
///     // Read tags
///     if let Some(title) = tags.get_first("TITLE") {
///         println!("Title: {}", title);
///     }
///
///     // Remove tags
///     tags.remove("COMMENT");
/// }
///
/// speex.save()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default)]
pub struct SpeexTags {
    /// The underlying Vorbis Comment dictionary
    pub inner: VCommentDict,
    /// Ogg stream serial number
    pub serial: u32,
    /// Padding bytes after comment data
    pub padding: Vec<u8>,
}

impl std::ops::Deref for SpeexTags {
    type Target = VCommentDict;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for SpeexTags {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl crate::Tags for SpeexTags {
    fn get(&self, key: &str) -> Option<&[String]> {
        self.inner.get(key)
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        self.inner.set(key, values)
    }

    fn remove(&mut self, key: &str) {
        self.inner.remove(key);
    }

    fn keys(&self) -> Vec<String> {
        self.inner.keys()
    }

    fn pprint(&self) -> String {
        format!("SpeexTags({})", self.inner.keys().len())
    }

    fn module_name(&self) -> &'static str {
        "oggspeex"
    }
}

impl SpeexTags {
    /// Create SpeexTags by reading from subsequent OGG pages
    ///
    /// Read OGG pages for comment data.
    pub fn from_reader<R: Read + Seek>(reader: &mut R, serial: u32) -> Result<Self> {
        let mut tags = SpeexTags {
            inner: VCommentDict::new(),
            serial,
            padding: Vec::new(),
        };

        // Seek to start to find comment packets
        reader.seek(SeekFrom::Start(0))?;

        // Skip the identification packet (first page with matching serial)
        let mut found_id = false;
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;
        while let Ok(page) = OggPage::from_reader(reader) {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                break;
            }
            if page.serial == serial {
                found_id = true;
                break;
            }
        }

        if !found_id {
            return Ok(tags);
        }

        // Collect pages for the comment packet (may span multiple pages)
        let mut comment_pages = Vec::new();
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();
        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                break;
            }

            let page = match OggPage::from_reader(reader) {
                Ok(page) => page,
                Err(_) => break,
            };

            if page.serial == serial {
                OggPage::accumulate_page_bytes_with_limit(
                    limits,
                    &mut cumulative_bytes,
                    &page,
                    "Ogg Speex comment packet",
                )?;
                comment_pages.push(page);
                if comment_pages.last().is_some_and(|p| p.is_complete()) {
                    break;
                }
            }
        }

        if comment_pages.is_empty() {
            return Ok(tags);
        }

        // Reconstruct the full comment packet from pages
        let packets = OggPage::to_packets(&comment_pages, false)?;
        if packets.is_empty() {
            return Ok(tags);
        }

        // Try to parse the first packet as VorbisComment
        let mut cursor = Cursor::new(&packets[0]);
        match tags
            .inner
            .load(&mut cursor, crate::vorbis::ErrorMode::Strict, false)
        {
            Ok(_) => {
                // Check for additional padding after comments
                let pos = cursor.position() as usize;
                if pos < packets[0].len() {
                    tags.padding = packets[0][pos..].to_vec();
                }
            }
            Err(_) => {
                // If parsing fails, return empty tags
            }
        }

        Ok(tags)
    }

    /// Write tags back to OGG Speex file
    ///
    /// This method writes the modified Vorbis comments back to the file.
    pub fn inject<P: AsRef<Path>>(
        &self,
        path: P,
        padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()> {
        use std::fs::OpenOptions;

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.as_ref())?;

        self.inject_writer(&mut file, padding_func)
    }

    /// Inject tags into a writer that implements Read + Write + Seek.
    ///
    /// This is the core implementation used by both `inject` (file-based) and
    /// `save_to_writer` (writer-based) saving.
    pub fn inject_writer<F: Read + Write + Seek + 'static>(
        &self,
        file: &mut F,
        padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()> {
        // Step 1: Find the first header page with stream info
        file.seek(SeekFrom::Start(0))?;
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;

        // Find identification page first
        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(SpeexHeaderError(SpeexError(
                    "No Speex stream found within page limit".to_string(),
                ))
                .into());
            }

            let page = match OggPage::from_reader(file) {
                Ok(page) => page,
                Err(_) => {
                    return Err(
                        SpeexHeaderError(SpeexError("No Speex stream found".to_string())).into(),
                    );
                }
            };

            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 8 && first_packet.starts_with(b"Speex   ") {
                    if page.serial != self.serial {
                        return Err(
                            SpeexHeaderError(SpeexError("Serial mismatch".to_string())).into()
                        );
                    }
                    break;
                }
            }
        }

        // Step 2: Find comment packet (next page with matching serial)
        let page = loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(SpeexHeaderError(SpeexError(
                    "No comment packet found within page limit".to_string(),
                ))
                .into());
            }

            let page = match OggPage::from_reader(file) {
                Ok(page) => page,
                Err(_) => {
                    return Err(SpeexHeaderError(SpeexError(
                        "No comment packet found".to_string(),
                    ))
                    .into());
                }
            };

            if page.serial == self.serial {
                break page;
            }
        };

        // Step 3: Collect all continuation pages for the comment packet.
        // Cap the number of pages to prevent excessive CPU usage from
        // crafted files with many incomplete single-packet pages.
        let mut old_pages = vec![page];
        while !old_pages.last().is_none_or(|p| p.is_complete())
            && old_pages.last().is_some_and(|p| p.packets.len() <= 1)
            && old_pages.len() < MAX_PAGE_SEARCH
        {
            let page = match OggPage::from_reader(file) {
                Ok(page) => page,
                Err(_) => break,
            };
            if page.serial == old_pages[0].serial {
                old_pages.push(page);
            }
        }

        // Step 4: Extract original packets
        let packets = OggPage::to_packets(&old_pages, false)?;
        if packets.is_empty() {
            return Err(SpeexHeaderError(SpeexError(
                "No packets found in comment pages".to_string(),
            ))
            .into());
        }

        // Step 5: Calculate content size and padding
        // Use saturating subtraction to prevent overflow on large or crafted values
        let content_size = i64::try_from(file.seek(SeekFrom::End(0))?)
            .unwrap_or(i64::MAX)
            .saturating_sub(i64::try_from(packets[0].len()).unwrap_or(0));

        // Create new comment data
        let mut comment_to_write = self.inner.clone();
        // Only set Audex vendor string when there are actual tags to write
        if !comment_to_write.keys().is_empty() {
            comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
        }

        let mut vcomment_data = Vec::new();
        comment_to_write.write(&mut vcomment_data, Some(false))?;

        let padding_left = packets[0].len() as i64 - vcomment_data.len() as i64;

        let info = crate::tags::PaddingInfo::new(padding_left, content_size);
        let new_padding = info.get_padding_with(padding_func);

        // Step 6: Set new comment packet
        let mut new_packets = packets;
        new_packets[0] = vcomment_data;
        if new_padding > 0 {
            new_packets[0].extend_from_slice(&vec![0u8; usize::try_from(new_padding).unwrap_or(0)]);
        }

        // Step 7: Create new pages preserving structure, with fallback if
        // the preserved layout cannot accommodate the new packet sizes
        let new_pages = OggPage::from_packets_try_preserve(new_packets.clone(), &old_pages);

        let final_pages = if new_pages.is_empty() {
            // Fallback to regular page creation — preserve original granule position
            let first_sequence = old_pages[0].sequence;

            let last_position = old_pages
                .last()
                .ok_or_else(|| AudexError::InvalidData("no comment pages found".to_string()))?
                .position;
            // Negative granule positions (e.g. -1) are Ogg "no position" sentinels;
            // clamp to zero to avoid u64 wrapping
            let original_granule = if last_position < 0 {
                0u64
            } else {
                last_position as u64
            };
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

        // Step 8: Replace pages
        OggPage::replace(file, &old_pages, final_pages)?;

        Ok(())
    }
}

/// Represents an Ogg Speex audio file with metadata and stream information.
///
/// This is the primary interface for working with Ogg Speex files, a legacy speech
/// codec designed for low-bitrate voice compression. Provides access to both stream
/// parameters and Vorbis Comment metadata.
///
/// # Fields
///
/// - **`info`**: Audio stream information ([`SpeexInfo`])
/// - **`tags`**: Optional Vorbis Comment metadata ([`SpeexTags`])
///
/// # File Format
///
/// Ogg Speex encapsulates lossy speech-optimized audio within an Ogg container:
/// - **Speech compression**: Optimized for human voice, not music
/// - **Low bitrate**: 2.15 - 44 kbps, ideal for VoIP and podcasts
/// - **Flexible modes**: Narrowband (8 kHz), wideband (16 kHz), ultra-wideband (32 kHz)
///
/// Common file extension: `.spx`
///
/// # Examples
///
/// ## Loading and Reading Information
///
/// ```no_run
/// use audex::oggspeex::OggSpeex;
/// use audex::FileType;
///
/// # fn main() -> audex::Result<()> {
/// let speex = OggSpeex::load("speech.spx")?;
///
/// // Access stream information
/// println!("Sample Rate: {} Hz", speex.info.sample_rate);
/// println!("Channels: {}", speex.info.channels);
/// println!("Speex Version: {}", speex.info.version);
///
/// // Check encoding mode
/// match speex.info.mode {
///     0 => println!("Mode: Narrowband (8 kHz)"),
///     1 => println!("Mode: Wideband (16 kHz)"),
///     2 => println!("Mode: Ultra-wideband (32 kHz)"),
///     _ => println!("Mode: Unknown"),
/// }
///
/// if let Some(duration) = speex.info.length {
///     println!("Duration: {:.2} seconds", duration.as_secs_f64());
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Working with Tags
///
/// ```no_run
/// use audex::oggspeex::OggSpeex;
/// use audex::{FileType, Tags};
///
/// # fn main() -> audex::Result<()> {
/// let mut speex = OggSpeex::load("speech.spx")?;
///
/// if let Some(tags) = speex.tags_mut() {
///     // Read existing tags
///     if let Some(title) = tags.get_first("TITLE") {
///         println!("Current title: {}", title);
///     }
///
///     // Modify tags
///     tags.set_single("TITLE", "Podcast Episode 1".to_string());
///     tags.set_single("ARTIST", "Podcast Host".to_string());
///     tags.set_single("ALBUM", "Podcast Series".to_string());
///     tags.set_single("DATE", "2024".to_string());
/// }
///
/// // Save changes
/// speex.save()?;
/// # Ok(())
/// # }
/// ```
///
/// ## Removing All Metadata
///
/// ```no_run
/// use audex::oggspeex::OggSpeex;
/// use audex::FileType;
///
/// # fn main() -> audex::Result<()> {
/// let mut speex = OggSpeex::load("speech.spx")?;
/// speex.clear()?;  // Removes all tags
/// # Ok(())
/// # }
/// ```
///
/// # Asynchronous Operations
///
/// When the `async` feature is enabled:
///
/// ```ignore
/// // Note: This example requires the `async` feature and a tokio runtime.
/// // Enable with: audex = { version = "*", features = ["async"] }
/// use audex::oggspeex::OggSpeex;
/// use audex::{FileType, Tags};
///
/// # async fn example() -> audex::Result<()> {
/// let mut speex = OggSpeex::load_async("speech.spx").await?;
///
/// if let Some(tags) = speex.tags_mut() {
///     tags.set_single("TITLE", "Async Recording".to_string());
/// }
///
/// speex.save_async().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct OggSpeex {
    /// Audio stream information extracted from the Speex header
    pub info: SpeexInfo,
    /// Optional Vorbis Comment metadata tags
    pub tags: Option<SpeexTags>,
    /// Path to the file (used for saving)
    path: Option<std::path::PathBuf>,
}

impl OggSpeex {
    /// Create a new empty OggSpeex instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: SpeexInfo::default(),
            tags: None,
            path: None,
        }
    }

    /// Add tags if none exist
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(SpeexTags {
            inner: VCommentDict::new(),
            serial: self.info.serial,
            padding: Vec::new(),
        });
        Ok(())
    }
}

impl Default for OggSpeex {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for OggSpeex {
    type Tags = SpeexTags;
    type Info = SpeexInfo;

    fn format_id() -> &'static str {
        "OggSpeex"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing OGG Speex file");
        let path_buf = path.as_ref().to_path_buf();
        let file = File::open(&path_buf)?;
        let mut reader = BufReader::new(file);

        // Parse stream info
        reader.seek(SeekFrom::Start(0))?;
        let info = SpeexInfo::from_reader(&mut reader)?;

        // Parse tags
        reader.seek(SeekFrom::Start(0))?;
        let tags = SpeexTags::from_reader(&mut reader, info.serial)?;
        debug_event!(tag_count = tags.keys().len(), "OGG Speex tags loaded");

        Ok(Self {
            info,
            tags: Some(tags),
            path: Some(path_buf),
        })
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing OGG Speex file from reader");
        let mut reader = reader;
        reader.seek(SeekFrom::Start(0))?;
        let info = SpeexInfo::from_reader(&mut reader)?;
        reader.seek(SeekFrom::Start(0))?;
        let tags = SpeexTags::from_reader(&mut reader, info.serial)?;
        debug_event!(tag_count = tags.keys().len(), "OGG Speex tags loaded");
        Ok(Self {
            info,
            tags: Some(tags),
            path: None,
        })
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving OGG Speex metadata");
        let path = self.path.as_ref().ok_or_else(|| {
            warn_event!("no file path available for OGG Speex save");
            AudexError::InvalidOperation("No file path available for saving".to_string())
        })?;

        if let Some(ref mut tags) = self.tags {
            tags.inject(path, None)?;
        }

        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        // Create empty tags with empty vendor string and inject
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        let empty_tags = SpeexTags {
            inner,
            serial: self.info.serial,
            padding: Vec::new(),
        };

        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::InvalidOperation("No file path available for deletion".to_string())
        })?;

        empty_tags.inject(path, None)?;
        self.tags = Some(empty_tags);

        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        if let Some(ref tags) = self.tags {
            // Read all data into a Cursor which satisfies the Sized + 'static
            // bounds required by inject_writer (and the internal OggPage helpers).
            let data =
                crate::util::read_all_from_writer_limited(writer, "in-memory Ogg Speex save")?;
            let mut cursor = Cursor::new(data);
            tags.inject_writer(&mut cursor, None)?;
            // Write modified data back to the original writer
            let result = cursor.into_inner();
            writer.seek(SeekFrom::Start(0))?;
            writer.write_all(&result)?;
            crate::util::truncate_writer_dyn(writer, result.len() as u64)?;
        }
        Ok(())
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        let empty_tags = SpeexTags {
            inner,
            serial: self.info.serial,
            padding: Vec::new(),
        };
        let data = crate::util::read_all_from_writer_limited(writer, "in-memory Ogg Speex clear")?;
        let mut cursor = Cursor::new(data);
        empty_tags.inject_writer(&mut cursor, None)?;
        let result = cursor.into_inner();
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&result)?;
        crate::util::truncate_writer_dyn(writer, result.len() as u64)?;
        self.tags = Some(empty_tags);
        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        if let Some(ref tags) = self.tags {
            tags.inject(path, None)?;
        }
        Ok(())
    }

    /// Adds empty Vorbis comment block to the file.
    ///
    /// Creates a new empty tag structure if none exists. If tags already exist,
    /// returns an error.
    ///
    /// # Errors
    ///
    /// Returns `AudexError::InvalidOperation` if tags already exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::oggspeex::OggSpeex;
    /// use audex::FileType;
    ///
    /// let mut speex = OggSpeex::load("song.spx")?;
    /// if speex.tags.is_none() {
    ///     speex.add_tags()?;
    /// }
    /// speex.set("title", vec!["My Song".to_string()])?;
    /// speex.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(SpeexTags {
            inner: VCommentDict::new(),
            serial: self.info.serial,
            padding: Vec::new(),
        });
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

    /// Score function for format detection.
    fn score(filename: &str, header: &[u8]) -> i32 {
        let ogg_score = if header.starts_with(b"OggS") { 1 } else { 0 };
        let speex_score = if header.windows(8).any(|window| window == b"Speex   ") {
            1
        } else {
            0
        };

        // Boost score for .spx extension to beat OggOpus detection
        let ext_score = if filename.to_lowercase().ends_with(".spx") {
            2
        } else {
            0
        };

        ogg_score * speex_score + ext_score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/x-speex"]
    }
}

/// Remove all metadata tags from an Ogg Speex file.
///
/// This convenience function loads the specified Ogg Speex file, removes all
/// Vorbis Comment tags, and saves the changes back to the file.
///
/// # Arguments
///
/// * `path` - Path to the Ogg Speex file
///
/// # Returns
///
/// Returns `Ok(())` if tags were successfully removed, or an error if the file
/// could not be loaded or saved.
///
/// # Examples
///
/// ```no_run
/// use audex::oggspeex;
///
/// # fn main() -> audex::Result<()> {
/// // Remove all tags from a file
/// oggspeex::clear("speech.spx")?;
/// # Ok(())
/// # }
/// ```
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut speex = OggSpeex::load(path)?;
    speex.clear()
}

/// Async implementation for OggSpeex
///
/// This block provides asynchronous versions of all major OggSpeex operations,
/// enabling non-blocking I/O for improved performance in async applications.
#[cfg(feature = "async")]
impl OggSpeex {
    /// Load an OGG Speex file asynchronously.
    ///
    /// This method reads the file using async I/O, parsing both the stream
    /// information (sample rate, channels, duration) and Vorbis comment tags.
    ///
    /// # Arguments
    /// * `path` - Path to the OGG Speex file to load
    ///
    /// # Returns
    /// * `Result<Self>` - The loaded OggSpeex instance or an error
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = TokioFile::open(&path_buf).await?;
        let mut reader = TokioBufReader::new(file);

        // Parse stream info asynchronously
        reader.seek(SeekFrom::Start(0)).await?;
        let info = Self::parse_info_async(&mut reader).await?;

        // Parse tags asynchronously
        let tags = Self::parse_tags_async(&mut reader, info.serial).await?;

        Ok(Self {
            info,
            tags: Some(tags),
            path: Some(path_buf),
        })
    }

    /// Parse Speex stream information asynchronously.
    ///
    /// Async I/O for Ogg page reading, delegates packet parsing to the sync
    /// `parse_identification_packet` (pure in-memory computation on `&[u8]`).
    async fn parse_info_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
    ) -> Result<SpeexInfo> {
        let mut speex_info = SpeexInfo::default();

        // Find the Speex stream by looking for the Speex identification packet
        loop {
            let page = match OggPage::from_reader_async(reader).await {
                Ok(page) => page,
                Err(_) => {
                    return Err(AudexError::InvalidData("No Speex stream found".to_string()));
                }
            };

            // Look for Speex identification packet starting with "Speex   "
            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 8 && &first_packet[0..8] == b"Speex   " {
                    speex_info.serial = page.serial;
                    speex_info.parse_identification_packet(first_packet)?;

                    // Calculate length from last page
                    Self::post_tags_info_async(reader, &mut speex_info).await?;

                    return Ok(speex_info);
                }
            }
        }
    }

    /// Calculate stream duration from the last OGG page asynchronously.
    ///
    /// Seeks to find the final page of the Speex stream to determine
    /// the total duration based on the granule position.
    async fn post_tags_info_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        info: &mut SpeexInfo,
    ) -> Result<()> {
        // Gracefully handle truncated files by leaving length as None
        // rather than failing outright, matching OggFLAC behavior.
        let last_page = match OggPage::find_last_async(reader, info.serial, true).await? {
            Some(page) => page,
            None => return Ok(()),
        };
        if info.sample_rate > 0 && last_page.position > 0 {
            let duration_secs = last_page.position as f64 / info.sample_rate as f64;
            if duration_secs.is_finite() && duration_secs >= 0.0 && duration_secs <= u64::MAX as f64
            {
                info.length = Some(std::time::Duration::from_secs_f64(duration_secs));
            }
        }
        Ok(())
    }

    /// Parse Vorbis comment tags asynchronously.
    ///
    /// Reads the comment packet from the OGG stream and parses it as
    /// Vorbis comments, extracting metadata like title, artist, album, etc.
    async fn parse_tags_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        serial: u32,
    ) -> Result<SpeexTags> {
        let mut tags = SpeexTags {
            inner: VCommentDict::new(),
            serial,
            padding: Vec::new(),
        };

        reader.seek(SeekFrom::Start(0)).await?;

        let mut pages = Vec::new();
        let mut found_header = false;
        let mut found_tags = false;
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();

        loop {
            let page = match OggPage::from_reader_async(reader).await {
                Ok(page) => page,
                Err(_) => break,
            };

            if page.serial == serial {
                if !found_header {
                    // Skip the first packet (identification header)
                    found_header = true;
                    continue;
                }

                if !found_tags {
                    OggPage::accumulate_page_bytes_with_limit(
                        limits,
                        &mut cumulative_bytes,
                        &page,
                        "Ogg Speex comment packet",
                    )?;
                    pages.push(page);
                    found_tags = true;
                } else if !pages.last().is_none_or(|p| p.is_complete()) {
                    OggPage::accumulate_page_bytes_with_limit(
                        limits,
                        &mut cumulative_bytes,
                        &page,
                        "Ogg Speex comment packet",
                    )?;
                    pages.push(page);
                } else {
                    break;
                }
            }
        }

        if pages.is_empty() {
            return Ok(tags);
        }

        // Reconstruct packets from pages
        let packets = OggPage::to_packets(&pages, false)?;
        if packets.is_empty() {
            return Ok(tags);
        }

        // Parse Vorbis comment data
        let comment_data = &packets[0];
        let mut cursor = Cursor::new(comment_data);

        match tags
            .inner
            .load(&mut cursor, crate::vorbis::ErrorMode::Replace, false)
        {
            Ok(_) => {
                let pos = cursor.position() as usize;
                if pos < comment_data.len() {
                    tags.padding = comment_data[pos..].to_vec();
                }
            }
            Err(_) => {
                tags.inner = VCommentDict::new();
                tags.padding = comment_data.to_vec();
            }
        }

        Ok(tags)
    }

    /// Save modified tags to the OGG Speex file asynchronously.
    ///
    /// Writes the current tags back to the file, preserving the audio data
    /// and other stream information. Uses async I/O for non-blocking operation.
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if no path is available
    pub async fn save_async(&mut self) -> Result<()> {
        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::InvalidOperation("No file path available for saving".to_string())
        })?;

        if let Some(ref tags) = self.tags {
            Self::inject_tags_async(path, tags, None).await?;
        }

        Ok(())
    }

    /// Inject Vorbis comment tags into the file asynchronously.
    ///
    /// Replaces the existing comment packet in the OGG stream with the new tags,
    /// updating the vendor string and preserving the file structure.
    async fn inject_tags_async<P: AsRef<Path>>(
        path: P,
        tags: &SpeexTags,
        padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()> {
        let file_path = path.as_ref();

        let file = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(file_path)
            .await?;

        let mut reader = TokioBufReader::new(file);

        // Find existing comment pages
        let mut comment_pages = Vec::new();
        let mut found_header = false;
        let mut found_tags = false;

        reader.seek(SeekFrom::Start(0)).await?;

        loop {
            let page = match OggPage::from_reader_async(&mut reader).await {
                Ok(page) => page,
                Err(_) => break,
            };

            if page.serial == tags.serial {
                if !found_header {
                    found_header = true;
                    continue;
                }

                if !found_tags {
                    comment_pages.push(page);
                    found_tags = true;
                } else if !comment_pages.last().is_none_or(|p| p.is_complete()) {
                    comment_pages.push(page);
                } else {
                    break;
                }
            }
        }

        if comment_pages.is_empty() {
            return Err(AudexError::InvalidData(
                "No comment packet found".to_string(),
            ));
        }

        // Extract original packets
        let old_packets = OggPage::to_packets(&comment_pages, false)?;
        if old_packets.is_empty() {
            return Err(AudexError::InvalidData(
                "No packets found in comment pages".to_string(),
            ));
        }

        // Calculate content size for padding calculation
        let content_size = {
            let old_pos = reader.stream_position().await?;
            let file_size = reader.seek(SeekFrom::End(0)).await?;
            reader.seek(SeekFrom::Start(old_pos)).await?;
            // Use saturating subtraction to prevent overflow on large or crafted values
            i64::try_from(file_size)
                .unwrap_or(i64::MAX)
                .saturating_sub(i64::try_from(old_packets[0].len()).unwrap_or(0))
        };

        // Create new comment data
        let mut comment_to_write = tags.inner.clone();
        // Only set Audex vendor string when there are actual tags to write
        if !comment_to_write.keys().is_empty() {
            comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
        }

        let mut vcomment_data = Vec::new();
        comment_to_write.write(&mut vcomment_data, Some(false))?;

        let padding_left = old_packets[0].len() as i64 - vcomment_data.len() as i64;

        // Calculate optimal padding using PaddingInfo
        let info = crate::tags::PaddingInfo::new(padding_left, content_size);
        let new_padding = info.get_padding_with(padding_func);

        // Reconstruct packets
        let mut new_packets = old_packets;
        new_packets[0] = vcomment_data;
        if new_padding > 0 {
            new_packets[0].extend_from_slice(&vec![0u8; usize::try_from(new_padding).unwrap_or(0)]);
        }

        // Create new pages preserving structure
        let new_pages = OggPage::from_packets_try_preserve(new_packets, &comment_pages);

        // Replace pages in file
        drop(reader);
        let mut writer = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(file_path)
            .await?;

        OggPage::replace_async(&mut writer, &comment_pages, new_pages).await?;

        Ok(())
    }

    /// Clear all tags from the OGG Speex file asynchronously.
    ///
    /// Removes all Vorbis comment metadata from the file, leaving only
    /// empty tags. Useful for stripping all metadata from an audio file.
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if no path is available
    pub async fn clear_async(&mut self) -> Result<()> {
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        let empty_tags = SpeexTags {
            inner,
            serial: self.info.serial,
            padding: Vec::new(),
        };

        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::InvalidOperation("No file path available for deletion".to_string())
        })?;

        Self::inject_tags_async(path, &empty_tags, None).await?;
        self.tags = Some(empty_tags);

        Ok(())
    }

    /// Delete all tags from an OGG Speex file at the given path asynchronously.
    ///
    /// This is a static method that loads the file, clears its tags, and saves it.
    /// Provides a convenient way to clear tags without maintaining an OggSpeex instance.
    ///
    /// # Arguments
    /// * `path` - Path to the OGG Speex file
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        let mut speex = Self::load_async(path).await?;
        speex.clear_async().await
    }
}

/// Clear all tags from an OGG Speex file asynchronously.
///
/// Standalone async function for removing all Vorbis comment metadata
/// from an OGG Speex file. This is a convenience wrapper around
/// `OggSpeex::delete_async()`.
///
/// # Arguments
/// * `path` - Path to the OGG Speex file
///
/// # Returns
/// * `Result<()>` - Success or an error
///
/// # Example
/// ```no_run
/// use audex::oggspeex::clear_async;
///
/// #[tokio::main]
/// async fn main() -> audex::Result<()> {
///     clear_async("audio.spx").await?;
///     Ok(())
/// }
/// ```
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<Path>>(path: P) -> Result<()> {
    OggSpeex::delete_async(path).await
}
