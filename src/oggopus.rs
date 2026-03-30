//! # Ogg Opus Format Support
//!
//! This module provides comprehensive support for reading and writing Ogg Opus audio files.
//! Opus is a modern, versatile audio codec designed for high quality and low latency,
//! particularly suited for interactive real-time applications like VoIP and streaming.
//!
//! ## Overview
//!
//! Ogg Opus combines the Opus audio codec with the Ogg container format:
//! - **Codec**: Opus (combines SILK and CELT technologies)
//! - **Container**: Ogg bitstream
//! - **Quality**: Excellent quality at low bitrates
//! - **Latency**: Very low latency suitable for real-time use
//! - **Flexibility**: Works well for speech and music
//!
//! ## File Format
//!
//! Ogg Opus files use the `.opus` extension and consist of:
//! 1. **OpusHead packet**: Identification header with codec configuration
//! 2. **OpusTags packet**: Vorbis Comments for metadata
//! 3. **Audio packets**: Opus-encoded audio data
//!
//! ## Audio Characteristics
//!
//! - **Sample rate**: Always operates at 48 kHz internally (original rate preserved in metadata)
//! - **Bitrate range**: 6 kbps to 510 kbps total
//! - **Channels**: Mono, stereo, or multichannel (up to 255 channels)
//! - **Frame sizes**: Flexible from 2.5ms to 60ms
//! - **Hybrid codec**: Combines SILK (speech) and CELT (music) codecs
//!
//! ## Technical Features
//!
//! - **Pre-skip**: Sample count to skip at decoder startup
//! - **Output gain**: Playback volume adjustment in Q7.8 dB
//! - **Channel mapping**: Supports various channel layouts
//! - **Version**: Currently version 1 of the specification
//!
//! ## Tagging
//!
//! Ogg Opus uses Vorbis Comments (same as Ogg Vorbis) for metadata:
//! - Human-readable field names (TITLE, ARTIST, ALBUM, etc.)
//! - Multiple values per field supported
//! - Case-insensitive field names
//! - UTF-8 encoded values
//!
//! ## Examples
//!
//! ### Loading and reading file information
//!
//! ```no_run
//! use audex::oggopus::OggOpus;
//! use audex::FileType;
//!
//! let opus = OggOpus::load("song.opus").unwrap();
//!
//! println!("Audio Format Information:");
//! println!("  Channels: {}", opus.info.channels);
//! println!("  Sample rate: {} Hz", opus.info.sample_rate);
//! println!("  Opus version: {}", opus.info.version);
//! println!("  Pre-skip: {} samples", opus.info.pre_skip);
//!
//! if let Some(length) = opus.info.length {
//!     let secs = length.as_secs();
//!     println!("  Duration: {}:{:02}", secs / 60, secs % 60);
//! }
//!
//! // Output gain in dB (Q7.8 format)
//! let gain_db = opus.info.gain as f64 / 256.0;
//! println!("  Output gain: {:.2} dB", gain_db);
//! ```
//!
//! ### Reading and modifying tags
//!
//! ```no_run
//! use audex::oggopus::OggOpus;
//! use audex::FileType;
//! use audex::Tags;
//!
//! let mut opus = OggOpus::load("song.opus").unwrap();
//!
//! if let Some(ref mut tags) = opus.tags {
//!     // Read existing tags
//!     if let Some(title) = tags.get("TITLE") {
//!         println!("Title: {}", title[0]);
//!     }
//!
//!     // Modify tags using set for Opus Comments
//!     tags.set("TITLE", vec!["New Title".to_string()]);
//!     tags.set("ARTIST", vec!["Artist Name".to_string()]);
//!     tags.set("ALBUM", vec!["Album Name".to_string()]);
//!     tags.set("DATE", vec!["2024".to_string()]);
//! }
//!
//! opus.save().unwrap();
//! ```
//!
//! ### Creating tags if they don't exist
//!
//! ```no_run
//! use audex::oggopus::OggOpus;
//! use audex::FileType;
//!
//! let mut opus = OggOpus::load("song.opus").unwrap();
//!
//! if opus.tags.is_none() {
//!     opus.add_tags().unwrap();
//! }
//!
//! if let Some(ref mut tags) = opus.tags {
//!     tags.set("TITLE", vec!["Title".to_string()]);
//! }
//!
//! opus.save().unwrap();
//! ```
//!
//! ### Working with channel mapping
//!
//! ```no_run
//! use audex::oggopus::OggOpus;
//! use audex::FileType;
//!
//! let opus = OggOpus::load("song.opus").unwrap();
//!
//! println!("Channel configuration:");
//! println!("  Total channels: {}", opus.info.channels);
//! println!("  Mapping family: {}", opus.info.channel_mapping_family);
//!
//! if let Some(stream_count) = opus.info.stream_count {
//!     println!("  Stream count: {}", stream_count);
//! }
//!
//! if let Some(coupled_count) = opus.info.coupled_stream_count {
//!     println!("  Coupled streams: {}", coupled_count);
//! }
//! ```
//!
//! ## Specification
//!
//! This module follows the official Ogg Opus specification:
//! - Opus codec: <https://opus-codec.org/>
//! - Ogg Opus encapsulation: <https://tools.ietf.org/html/rfc7845>

use crate::VERSION_STRING;
use crate::ogg::OggPage;
use crate::vorbis::VCommentDict;
use crate::{AudexError, FileType, Result, StreamInfo};
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};
#[cfg(feature = "async")]
use tokio::io::{AsyncSeekExt, BufReader as TokioBufReader};

/// General error type for Ogg Opus operations.
///
/// This is the base error type for Ogg Opus file parsing and processing.
/// It wraps error messages describing what went wrong during file operations.
///
/// # Common Causes
///
/// - Invalid or corrupted Ogg Opus file structure
/// - Missing OpusHead or OpusTags packets
/// - Unsupported Opus version
/// - File is not an Ogg Opus file
/// - I/O errors during file access
///
/// # Examples
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// match OggOpus::load("file.opus") {
///     Ok(opus) => println!("Loaded successfully"),
///     Err(e) => eprintln!("Failed to load: {}", e),
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct OpusError(pub String);

impl std::fmt::Display for OpusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for OpusError {}

/// Error type for Opus header packet parsing failures.
///
/// This error specifically occurs when parsing the OpusHead identification header
/// that contains codec configuration and stream parameters.
///
/// # Common Causes
///
/// - Invalid OpusHead packet signature (not starting with "OpusHead")
/// - OpusHead packet too short (less than 19 bytes)
/// - Unsupported Opus version (major version not 0)
/// - Invalid channel count or channel mapping configuration
/// - Truncated or corrupted header data
/// - Missing OpusHead packet in Ogg stream
///
/// # Examples
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// match OggOpus::load("invalid.opus") {
///     Ok(opus) => println!("Valid Opus file"),
///     Err(e) => {
///         eprintln!("Header parsing failed: {}", e);
///         eprintln!("File may be corrupted or not a valid Opus file");
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct OpusHeaderError(pub OpusError);

impl std::fmt::Display for OpusHeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Opus header error: {}", self.0)
    }
}

impl std::error::Error for OpusHeaderError {}

impl From<OpusError> for OpusHeaderError {
    fn from(err: OpusError) -> Self {
        OpusHeaderError(err)
    }
}

impl From<OpusHeaderError> for AudexError {
    fn from(err: OpusHeaderError) -> Self {
        AudexError::InvalidData(err.to_string())
    }
}

impl From<OpusError> for AudexError {
    fn from(err: OpusError) -> Self {
        AudexError::InvalidData(err.0)
    }
}

/// Audio stream information for Ogg Opus files.
///
/// Contains technical details about the Opus audio stream extracted from the
/// OpusHead identification header and calculated from the Ogg bitstream structure.
///
/// # Fields
///
/// - **`length`**: Total duration of the audio file
/// - **`channels`**: Number of output channels (1=mono, 2=stereo, etc.)
/// - **`sample_rate`**: Always 48000 Hz (Opus internally operates at 48 kHz)
/// - **`pre_skip`**: Number of samples to discard from decoder output at start
/// - **`version`**: Opus header version byte (e.g. 1 means major=0, minor=1)
/// - **`gain`**: Output gain in Q7.8 dB format (divide by 256 to get dB)
/// - **`channel_mapping_family`**: Channel mapping family (0=mono/stereo, 1=surround, 255=undefined)
/// - **`stream_count`**: Number of encoded streams (for multichannel)
/// - **`coupled_stream_count`**: Number of coupled stereo streams (for multichannel)
/// - **`channel_mapping`**: Channel mapping table (for multichannel)
/// - **`serial`**: Ogg logical bitstream serial number
///
/// # Opus Specifics
///
/// ## Sample Rate
/// Opus always operates at 48 kHz internally, regardless of the original recording sample rate.
/// The original sample rate is preserved in the OpusHead header but not exposed here.
///
/// ## Pre-skip
/// Due to Opus's algorithmic delay, the first `pre_skip` samples from the decoder should be
/// discarded. This is typically 312 samples (6.5 ms at 48 kHz) for Opus v1.
///
/// ## Output Gain
/// The gain field provides a playback volume adjustment in Q7.8 dB format.
/// To convert to decibels: `gain_db = gain as f64 / 256.0`
///
/// ## Channel Mapping
/// - **Family 0**: Mono (channels=1) or stereo (channels=2)
/// - **Family 1**: Vorbis channel order for surround sound (up to 8 channels)
/// - **Family 255**: No defined channel mapping
///
/// # Examples
///
/// ## Reading stream information
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// let opus = OggOpus::load("song.opus").unwrap();
/// let info = &opus.info;
///
/// println!("Opus Stream Information:");
/// println!("  Channels: {}", info.channels);
/// println!("  Sample rate: {} Hz", info.sample_rate);
/// println!("  Pre-skip: {} samples ({:.2} ms)",
///     info.pre_skip,
///     info.pre_skip as f64 * 1000.0 / info.sample_rate as f64);
///
/// // Convert gain to dB
/// let gain_db = info.gain as f64 / 256.0;
/// println!("  Output gain: {:.2} dB", gain_db);
///
/// if let Some(length) = info.length {
///     println!("  Duration: {:.2} seconds", length.as_secs_f64());
/// }
/// ```
///
/// ## Analyzing channel configuration
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// let opus = OggOpus::load("multichannel.opus").unwrap();
/// let info = &opus.info;
///
/// match info.channel_mapping_family {
///     0 => {
///         if info.channels == 1 {
///             println!("Mono audio");
///         } else {
///             println!("Stereo audio");
///         }
///     }
///     1 => {
///         println!("Surround sound: {} channels", info.channels);
///         if let Some(streams) = info.stream_count {
///             println!("  Encoded streams: {}", streams);
///         }
///         if let Some(coupled) = info.coupled_stream_count {
///             println!("  Coupled streams: {}", coupled);
///         }
///     }
///     255 => println!("Undefined channel mapping"),
///     family => println!("Unknown channel mapping family: {}", family),
/// }
/// ```
///
/// ## Checking decoder requirements
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// let opus = OggOpus::load("song.opus").unwrap();
/// let info = &opus.info;
///
/// // Check version compatibility
/// if info.version >> 4 != 0 {
///     eprintln!("Warning: Unsupported major version");
/// }
///
/// // Calculate pre-skip duration
/// let pre_skip_ms = info.pre_skip as f64 * 1000.0 / info.sample_rate as f64;
/// println!("Decoder should skip first {:.2} ms", pre_skip_ms);
/// ```
#[derive(Debug, Clone, Default)]
pub struct OpusInfo {
    pub length: Option<Duration>,
    pub channels: u16,
    pub sample_rate: u32, // Always 48000 for Opus
    pub pre_skip: u16,
    pub version: u8,
    pub gain: i16,
    pub channel_mapping_family: u8,
    pub stream_count: Option<u8>,
    pub coupled_stream_count: Option<u8>,
    pub channel_mapping: Option<Vec<u8>>,
    pub serial: u32,
}

impl OpusInfo {
    /// Create OpusInfo by parsing from a readable source
    ///
    /// Parses OGG pages to find the Opus stream starting with b"OpusHead".
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut opus_info = Self {
            sample_rate: 48000, // Opus always uses 48kHz internally
            ..Default::default()
        };
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;

        // Find the Opus stream by looking for the OpusHead packet
        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(OpusHeaderError(OpusError(
                    "No Opus stream found within page limit".to_string(),
                ))
                .into());
            }

            let page = match OggPage::from_reader(reader) {
                Ok(page) => page,
                Err(_) => {
                    return Err(
                        OpusHeaderError(OpusError("No Opus stream found".to_string())).into(),
                    );
                }
            };

            // Look for Opus identification packet starting with "OpusHead"
            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 8 && &first_packet[0..8] == b"OpusHead" {
                    opus_info.serial = page.serial;
                    opus_info.parse_head_packet(first_packet)?;

                    // Calculate length from final page
                    opus_info.post_tags(reader)?;

                    return Ok(opus_info);
                }
            }
        }
    }

    /// Parse the Opus identification header (OpusHead packet)
    ///
    /// Format follows specification: "<BBHIHBB" + optional channel mapping
    fn parse_head_packet(&mut self, packet: &[u8]) -> Result<()> {
        if packet.len() < 19 {
            return Err(
                OpusHeaderError(OpusError("Invalid OpusHead packet length".to_string())).into(),
            );
        }

        // Skip "OpusHead" (8 bytes) and parse header: <BBHIHBB
        let mut cursor = Cursor::new(&packet[8..]);

        // Parse header fields
        self.version = cursor.read_u8()?;
        self.channels = cursor.read_u8()? as u16;
        self.pre_skip = cursor.read_u16::<LittleEndian>()?;
        let _sample_rate = cursor.read_u32::<LittleEndian>()?; // Original sample rate, but we use 48000
        self.gain = cursor.read_i16::<LittleEndian>()?;
        self.channel_mapping_family = cursor.read_u8()?;

        // Validate version (only major version 0 supported)
        if self.version >> 4 != 0 {
            return Err(OpusHeaderError(OpusError(format!(
                "Unsupported Opus version: {}",
                self.version >> 4
            )))
            .into());
        }

        // Validate channel count
        if self.channels == 0 || self.channels > 255 {
            return Err(OpusHeaderError(OpusError(format!(
                "Invalid channel count: {}",
                self.channels
            )))
            .into());
        }

        // Parse channel mapping if family > 0
        if self.channel_mapping_family > 0 {
            if packet.len() < 21 {
                return Err(OpusHeaderError(OpusError(
                    "Insufficient data for channel mapping".to_string(),
                ))
                .into());
            }

            self.stream_count = Some(cursor.read_u8()?);
            self.coupled_stream_count = Some(cursor.read_u8()?);

            let expected_mapping_len = self.channels as usize;
            if packet.len() < 21 + expected_mapping_len {
                return Err(OpusHeaderError(OpusError(
                    "Insufficient channel mapping data".to_string(),
                ))
                .into());
            }

            let mut channel_mapping = vec![0u8; expected_mapping_len];
            cursor.read_exact(&mut channel_mapping)?;
            self.channel_mapping = Some(channel_mapping);
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
        if last_page.position > 0 {
            // Calculate duration: (position - pre_skip) / 48000
            let effective_samples =
                (last_page.position as u64).saturating_sub(self.pre_skip as u64);
            let duration_secs = effective_samples as f64 / 48000.0;
            if duration_secs.is_finite() && duration_secs >= 0.0 && duration_secs <= u64::MAX as f64
            {
                self.length = Some(Duration::from_secs_f64(duration_secs));
            }
        }
        Ok(())
    }

    /// Format stream information for display
    ///
    /// Returns a formatted string describing the Opus stream.
    pub fn pprint(&self) -> String {
        let duration = self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0);

        format!(
            "Opus, {:.2} seconds, {} channel(s), {} Hz",
            duration, self.channels, self.sample_rate
        )
    }
}

impl StreamInfo for OpusInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        // Opus is variable bitrate - we can't determine this from headers alone
        None
    }

    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate) // Always 48000 for Opus
    }

    fn channels(&self) -> Option<u16> {
        if self.channels > 0 {
            Some(self.channels)
        } else {
            None
        }
    }

    fn bits_per_sample(&self) -> Option<u16> {
        None // Opus is lossy, no fixed bits per sample
    }
}

/// Vorbis Comment metadata tags for Ogg Opus files.
///
/// OpusTags wraps Vorbis Comments (same format as Ogg Vorbis) providing
/// metadata storage for Ogg Opus files. The tags are stored in the OpusTags
/// packet which follows the OpusHead packet in the Ogg bitstream.
///
/// # Structure
///
/// - **`inner`**: The underlying Vorbis Comment dictionary
/// - **`serial`**: Ogg stream serial number (links tags to audio stream)
/// - **`padding`**: Reserved padding data after comments
///
/// # Tag Format
///
/// Uses Vorbis Comment format with:
/// - **Field names**: Human-readable (TITLE, ARTIST, ALBUM, DATE, etc.)
/// - **Multiple values**: Single field can have multiple values
/// - **Case-insensitive**: Field names treated case-insensitively
/// - **UTF-8**: All values encoded as UTF-8
///
/// # Common Fields
///
/// - **TITLE**: Track title
/// - **ARTIST**: Artist/performer name
/// - **ALBUM**: Album name
/// - **DATE**: Release date (typically YYYY format)
/// - **TRACKNUMBER**: Track number on album
/// - **GENRE**: Music genre
/// - **COMMENT**: Free-form comment
/// - **ALBUMARTIST**: Album artist (if different from track artist)
///
/// # Examples
///
/// ## Reading tags
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::{FileType, Tags};
///
/// let opus = OggOpus::load("song.opus").unwrap();
///
/// if let Some(ref tags) = opus.tags {
///     // Read single-value fields using the Tags trait
///     if let Some(title) = tags.get("TITLE") {
///         println!("Title: {}", title[0]);
///     }
///
///     if let Some(artist) = tags.get("ARTIST") {
///         println!("Artist: {}", artist[0]);
///     }
///
///     // List all tags
///     println!("\nAll tags:");
///     for key in tags.keys() {
///         if let Some(values) = tags.get(&key) {
///             for value in values {
///                 println!("  {}: {}", key, value);
///             }
///         }
///     }
/// }
/// ```
///
/// ## Modifying tags
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// let mut opus = OggOpus::load("song.opus").unwrap();
///
/// if let Some(ref mut tags) = opus.tags {
///     // Set single values
///     tags.set("TITLE", vec!["New Title".to_string()]);
///     tags.set("ARTIST", vec!["Artist Name".to_string()]);
///     tags.set("DATE", vec!["2024".to_string()]);
///
///     // Set multiple values (e.g., multiple genres)
///     tags.set("GENRE", vec![
///         "Electronic".to_string(),
///         "Ambient".to_string(),
///     ]);
/// }
///
/// opus.save().unwrap();
/// ```
///
/// ## Deref access to VCommentDict
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::{FileType, Tags};
///
/// let mut opus = OggOpus::load("song.opus").unwrap();
///
/// if let Some(ref mut tags) = opus.tags {
///     // OpusTags derefs to VCommentDict, so you can use its methods directly
///     tags.set("TITLE", vec!["Title".to_string()]);
///     // Use the Tags trait remove method to delete a tag
///     tags.remove("COMMENT");
///
///     // Access underlying VCommentDict explicitly if needed
///     println!("Vendor: {}", tags.inner.vendor());
/// }
/// ```
#[derive(Debug, Default)]
pub struct OpusTags {
    pub inner: VCommentDict,
    pub serial: u32,
    pub padding: i32,      // Padding size (number of zero bytes after comments)
    pub pad_data: Vec<u8>, // Opaque data after comments (preserved as-is when LSB of first byte is 1)
}

impl std::ops::Deref for OpusTags {
    type Target = VCommentDict;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for OpusTags {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl crate::Tags for OpusTags {
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
        format!("OpusTags({})", self.inner.keys().len())
    }

    fn module_name(&self) -> &'static str {
        "oggopus"
    }
}

impl OpusTags {
    /// Create OpusTags by reading from subsequent OGG pages
    ///
    /// Read OGG pages for comment data.
    pub fn from_reader<R: Read + Seek>(reader: &mut R, serial: u32) -> Result<Self> {
        let mut tags = OpusTags {
            inner: VCommentDict::new(),
            serial,
            padding: 0,
            pad_data: Vec::new(),
        };

        // Seek to start to find OpusTags pages
        reader.seek(SeekFrom::Start(0))?;

        // Collect all pages for the OpusTags packet
        let mut pages = Vec::new();
        let mut found_tags = false;
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;
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
                if let Some(first_packet) = page.packets.first() {
                    if first_packet.len() >= 8 && first_packet.starts_with(b"OpusTags") {
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg Opus comment packet",
                        )?;
                        pages.push(page);
                        found_tags = true;
                    } else if found_tags {
                        let last_complete = pages
                            .last()
                            .ok_or_else(|| {
                                AudexError::InvalidData(
                                    "expected non-empty page list after tag header".into(),
                                )
                            })?
                            .is_complete();
                        if !last_complete {
                            OggPage::accumulate_page_bytes_with_limit(
                                limits,
                                &mut cumulative_bytes,
                                &page,
                                "Ogg Opus comment packet",
                            )?;
                            pages.push(page);
                        } else {
                            break;
                        }
                    }
                } else if found_tags {
                    // Continuation page with an empty packets vec -- still
                    // part of the multi-page OpusTags packet. Append it so
                    // that to_packets can reassemble the full comment data.
                    let last_complete = pages
                        .last()
                        .ok_or_else(|| {
                            AudexError::InvalidData(
                                "expected non-empty page list after tag header".into(),
                            )
                        })?
                        .is_complete();
                    if !last_complete {
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg Opus comment packet",
                        )?;
                        pages.push(page);
                    }
                }
            }
        }

        if pages.is_empty() {
            return Ok(tags);
        }

        // Reconstruct packets from pages (handles multipage comments)
        let packets = OggPage::to_packets(&pages, false)?;
        if packets.is_empty() || packets[0].len() < 8 {
            return Ok(tags);
        }

        // Look for OpusTags packet
        if &packets[0][0..8] == b"OpusTags" {
            // Parse comment data starting after "OpusTags"
            let comment_data = &packets[0][8..];

            // Find where comments end and padding begins
            let mut cursor = Cursor::new(comment_data);

            // Try to parse the VorbisComment data - use Replace mode like OggVorbis
            match tags
                .inner
                .load(&mut cursor, crate::vorbis::ErrorMode::Replace, false)
            {
                Ok(_) => {
                    // Check for additional data after comments
                    let pos = cursor.position() as usize;
                    if pos < comment_data.len() {
                        let remaining = &comment_data[pos..];
                        // Cap to i32::MAX to prevent overflow on pathologically large padding
                        tags.padding = remaining.len().min(i32::MAX as usize) as i32;

                        if !remaining.is_empty() && (remaining[0] & 0x1) == 1 {
                            tags.pad_data = remaining.to_vec();
                            tags.padding = 0; // we have to preserve, so no padding
                        }
                    }
                }
                Err(_) => {
                    // If parsing fails, treat entire packet as empty comments
                    tags.inner = VCommentDict::new();
                    tags.padding = comment_data.len().min(i32::MAX as usize) as i32;
                }
            }
        }

        Ok(tags)
    }

    /// Write tags back to OGG Opus file
    ///
    /// This method writes the modified Vorbis comments back to the file.
    pub fn inject<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.as_ref())?;
        self.inject_writer(&mut file)
    }

    /// Inject tags into a writer that implements Read + Write + Seek.
    ///
    /// This is the core implementation used by both `inject` (file-based) and
    /// `save_to_writer` (writer-based) saving.
    pub fn inject_writer<F: Read + Write + Seek + 'static>(&self, file: &mut F) -> Result<()> {
        // Find OpusTags header pages
        let mut comment_pages = Vec::new();
        let mut found_tags = false;
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;

        file.seek(SeekFrom::Start(0))?;

        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                break;
            }

            let page = match OggPage::from_reader(file) {
                Ok(page) => page,
                Err(_) => break,
            };

            if page.serial == self.serial {
                if let Some(first_packet) = page.packets.first() {
                    if first_packet.len() >= 8 && first_packet.starts_with(b"OpusTags") {
                        comment_pages.push(page);
                        found_tags = true;
                    } else if found_tags {
                        let last_complete = comment_pages
                            .last()
                            .ok_or_else(|| {
                                AudexError::InvalidData(
                                    "expected non-empty page list after tag header".into(),
                                )
                            })?
                            .is_complete();
                        if !last_complete {
                            comment_pages.push(page);
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        if comment_pages.is_empty() {
            return Err(AudexError::InvalidData(
                "No OpusTags header found".to_string(),
            ));
        }

        // Reconstruct packets from old pages
        let old_packets = OggPage::to_packets(&comment_pages, false)?;
        if old_packets.is_empty() {
            return Err(AudexError::InvalidData("No packets found".to_string()));
        }

        // Create new OpusTags data: b"OpusTags" + vorbis comment bytes
        let mut comment_to_write = self.inner.clone();
        // Only set Audex vendor string when there are actual tags to write
        if !comment_to_write.keys().is_empty() {
            comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
        }

        let mut vcomment_data = b"OpusTags".to_vec();
        // Opus uses framing=false
        let mut vcomment_bytes = Vec::new();
        comment_to_write.write(&mut vcomment_bytes, Some(false))?;
        vcomment_data.extend_from_slice(&vcomment_bytes);

        let mut new_packets = old_packets;

        if !self.pad_data.is_empty() {
            // If we have opaque padding data to preserve, we can't add more padding
            // as long as we don't know the structure of what follows
            new_packets[0] = vcomment_data;
            new_packets[0].extend_from_slice(&self.pad_data);
        } else {
            // Calculate content_size (approx file size minus old comment packet)
            let content_size = {
                let old_pos = file.stream_position()?;
                let file_size = file.seek(SeekFrom::End(0))?;
                file.seek(SeekFrom::Start(old_pos))?;
                // Use saturating subtraction to prevent overflow on large or crafted values
                i64::try_from(file_size)
                    .unwrap_or(i64::MAX)
                    .saturating_sub(i64::try_from(new_packets[0].len()).unwrap_or(0))
            };

            let padding_left = new_packets[0].len() as i64 - vcomment_data.len() as i64;

            // Use PaddingInfo to calculate optimal padding (same as OggVorbis)
            let info = crate::tags::PaddingInfo::new(padding_left, content_size);
            let new_padding = info.get_padding_with(None::<fn(&crate::tags::PaddingInfo) -> i64>);

            new_packets[0] = vcomment_data;
            if new_padding > 0 {
                new_packets[0]
                    .extend_from_slice(&vec![0u8; usize::try_from(new_padding).unwrap_or(0)]);
            }
        }

        // Create new pages - try to preserve page layout
        let new_pages = OggPage::from_packets_try_preserve(new_packets.clone(), &comment_pages);

        // Fallback if from_packets_try_preserve fails (returns empty)
        let final_pages = if new_pages.is_empty() {
            let first_sequence = comment_pages[0].sequence;
            let last_position = comment_pages
                .last()
                .ok_or_else(|| AudexError::InvalidData("no comment pages collected".to_string()))?
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

        // Replace the old pages with new pages
        OggPage::replace(file, &comment_pages, final_pages)?;

        Ok(())
    }
}

/// Main structure representing an Ogg Opus audio file.
///
/// This is the primary interface for reading and writing Ogg Opus files,
/// providing access to both audio stream information and Vorbis Comment metadata.
///
/// # Structure
///
/// - **`info`**: Opus stream information (channels, sample rate, pre-skip, gain, etc.)
/// - **`tags`**: Optional Vorbis Comments for metadata (title, artist, album, etc.)
/// - **`path`**: Internal file path used for save operations
///
/// # File Format
///
/// Ogg Opus uses the Ogg container format with Opus-encoded audio:
/// - **Extension**: `.opus` (standard)
/// - **MIME type**: `audio/ogg` (or `audio/ogg; codecs=opus`)
/// - **Codec**: Opus (hybrid SILK + CELT codec)
/// - **Sample rate**: Always 48 kHz internally
///
/// # Examples
///
/// ## Loading and reading file information
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// let opus = OggOpus::load("song.opus").unwrap();
///
/// // Access stream information
/// let info = &opus.info;
/// println!("Channels: {}", info.channels);
/// println!("Sample rate: {} Hz", info.sample_rate);
/// println!("Pre-skip: {} samples", info.pre_skip);
///
/// // Convert gain to dB
/// let gain_db = info.gain as f64 / 256.0;
/// println!("Output gain: {:.2} dB", gain_db);
///
/// if let Some(length) = info.length {
///     println!("Duration: {:.2} seconds", length.as_secs_f64());
/// }
/// ```
///
/// ## Reading and modifying tags
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::{FileType, Tags};
///
/// let mut opus = OggOpus::load("song.opus").unwrap();
///
/// // Read existing tags using the Tags trait
/// if let Some(ref tags) = opus.tags {
///     if let Some(title) = tags.get("TITLE") {
///         println!("Current title: {}", title[0]);
///     }
/// }
///
/// // Modify tags
/// if let Some(ref mut tags) = opus.tags {
///     tags.set("TITLE", vec!["New Title".to_string()]);
///     tags.set("ARTIST", vec!["Artist Name".to_string()]);
///     tags.set("ALBUM", vec!["Album Name".to_string()]);
///     tags.set("DATE", vec!["2024".to_string()]);
/// }
///
/// // Save changes back to file
/// opus.save().unwrap();
/// ```
///
/// ## Creating tags if they don't exist
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// let mut opus = OggOpus::load("song.opus").unwrap();
///
/// // Add tags if file has none
/// if opus.tags.is_none() {
///     opus.add_tags().unwrap();
/// }
///
/// if let Some(ref mut tags) = opus.tags {
///     tags.set("TITLE", vec!["New Song".to_string()]);
///     tags.set("ARTIST", vec!["Artist".to_string()]);
/// }
///
/// opus.save().unwrap();
/// ```
///
/// ## Analyzing Opus-specific features
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// let opus = OggOpus::load("song.opus").unwrap();
/// let info = &opus.info;
///
/// // Calculate pre-skip duration
/// let pre_skip_ms = info.pre_skip as f64 * 1000.0 / info.sample_rate as f64;
/// println!("Decoder pre-skip: {:.2} ms", pre_skip_ms);
///
/// // Check channel mapping
/// match info.channel_mapping_family {
///     0 => println!("Mono/Stereo (family 0)"),
///     1 => {
///         println!("Surround sound (family 1)");
///         if let Some(streams) = info.stream_count {
///             println!("  Streams: {}", streams);
///         }
///     }
///     f => println!("Unknown mapping family: {}", f),
/// }
/// ```
///
/// ## Removing all metadata
///
/// ```no_run
/// use audex::oggopus::OggOpus;
/// use audex::FileType;
///
/// let mut opus = OggOpus::load("song.opus").unwrap();
///
/// // Clear all tags
/// opus.clear().unwrap();
///
/// println!("All metadata removed");
/// ```
#[derive(Debug)]
pub struct OggOpus {
    pub info: OpusInfo,
    pub tags: Option<OpusTags>,
    path: Option<std::path::PathBuf>,
}

impl OggOpus {
    /// Create a new empty OggOpus instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: OpusInfo::default(),
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
        self.tags = Some(OpusTags {
            inner: VCommentDict::new(),
            serial: self.info.serial,
            padding: 0,
            pad_data: Vec::new(),
        });
        Ok(())
    }
}

impl Default for OggOpus {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for OggOpus {
    type Tags = OpusTags;
    type Info = OpusInfo;

    fn format_id() -> &'static str {
        "OggOpus"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing OGG Opus file");
        let path_buf = path.as_ref().to_path_buf();
        let file = File::open(&path_buf)?;
        let mut reader = BufReader::new(file);

        // Parse stream info
        reader.seek(SeekFrom::Start(0))?;
        let info = OpusInfo::from_reader(&mut reader)?;

        // Parse tags - continue from current position after info header
        let tags = OpusTags::from_reader(&mut reader, info.serial)?;
        debug_event!(tag_count = tags.keys().len(), "OGG Opus tags loaded");

        Ok(Self {
            info,
            tags: Some(tags),
            path: Some(path_buf),
        })
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing OGG Opus file from reader");
        let mut reader = reader;
        reader.seek(SeekFrom::Start(0))?;
        let info = OpusInfo::from_reader(&mut reader)?;
        let tags = OpusTags::from_reader(&mut reader, info.serial)?;
        debug_event!(tag_count = tags.keys().len(), "OGG Opus tags loaded");
        Ok(Self {
            info,
            tags: Some(tags),
            path: None,
        })
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving OGG Opus metadata");
        let path = self.path.as_ref().ok_or_else(|| {
            warn_event!("no file path available for OGG Opus save");
            AudexError::InvalidOperation("No file path available for saving".to_string())
        })?;

        if let Some(ref tags) = self.tags {
            tags.inject(path)?;
        }

        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        // Create empty tags with empty vendor string and inject
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        let empty_tags = OpusTags {
            inner,
            serial: self.info.serial,
            padding: 0,
            pad_data: Vec::new(),
        };

        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::InvalidOperation("No file path available for deletion".to_string())
        })?;

        empty_tags.inject(path)?;
        self.tags = Some(empty_tags);

        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        if let Some(ref tags) = self.tags {
            // Read all data into a Cursor which satisfies the Sized + 'static
            // bounds required by inject_writer (and the internal OggPage helpers).
            let data =
                crate::util::read_all_from_writer_limited(writer, "in-memory Ogg Opus save")?;
            let mut cursor = Cursor::new(data);
            tags.inject_writer(&mut cursor)?;
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
        let empty_tags = OpusTags {
            inner,
            serial: self.info.serial,
            padding: 0,
            pad_data: Vec::new(),
        };
        let data = crate::util::read_all_from_writer_limited(writer, "in-memory Ogg Opus clear")?;
        let mut cursor = Cursor::new(data);
        empty_tags.inject_writer(&mut cursor)?;
        let result = cursor.into_inner();
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&result)?;
        crate::util::truncate_writer_dyn(writer, result.len() as u64)?;
        self.tags = Some(empty_tags);
        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        if let Some(ref tags) = self.tags {
            tags.inject(path)?;
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
    /// use audex::oggopus::OggOpus;
    /// use audex::FileType;
    ///
    /// let mut opus = OggOpus::load("song.opus")?;
    /// if opus.tags.is_none() {
    ///     opus.add_tags()?;
    /// }
    /// opus.set("title", vec!["My Song".to_string()])?;
    /// opus.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(OpusTags {
            inner: VCommentDict::new(),
            serial: self.info.serial,
            padding: 0,
            pad_data: Vec::new(),
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

    /// Score function for format detection:
    /// return (b"OpusHead" in data) + (b"OggS" in data)
    fn score(_filename: &str, header: &[u8]) -> i32 {
        let opus_head_score = if header.windows(8).any(|window| window == b"OpusHead") {
            1
        } else {
            0
        };
        let ogg_score = if header.windows(4).any(|window| window == b"OggS") {
            1
        } else {
            0
        };

        opus_head_score + ogg_score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/ogg", "audio/ogg; codecs=opus"]
    }
}

/// Module-level clear function for removing tags from OGG Opus files
///
/// Clear tags from OGG Opus file.
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut opus = OggOpus::load(path)?;
    opus.clear()
}

#[cfg(feature = "async")]
impl OggOpus {
    /// Load Ogg Opus file asynchronously
    ///
    /// Reads and parses an Ogg Opus file from disk asynchronously.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the Ogg Opus file
    ///
    /// # Returns
    ///
    /// A new `OggOpus` instance with parsed info and tags
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = TokioFile::open(&path_buf).await?;
        let mut reader = TokioBufReader::new(file);

        // Parse stream info
        reader.seek(SeekFrom::Start(0)).await?;
        let info = Self::parse_info_async(&mut reader).await?;

        // Parse tags
        let tags = Self::parse_tags_async(&mut reader, info.serial).await?;

        Ok(Self {
            info,
            tags: Some(tags),
            path: Some(path_buf),
        })
    }

    /// Parse Opus info asynchronously
    async fn parse_info_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
    ) -> Result<OpusInfo> {
        let mut opus_info = OpusInfo {
            sample_rate: 48000, // Opus always uses 48kHz internally
            ..Default::default()
        };

        // Find the Opus stream by looking for the OpusHead packet
        loop {
            let page = match OggPage::from_reader_async(reader).await {
                Ok(page) => page,
                Err(_) => {
                    return Err(AudexError::InvalidData("No Opus stream found".to_string()));
                }
            };

            // Look for Opus identification packet starting with "OpusHead"
            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 8 && &first_packet[0..8] == b"OpusHead" {
                    opus_info.serial = page.serial;

                    // Parse the header packet using existing sync method
                    opus_info.parse_head_packet(first_packet)?;

                    // Calculate length from last page
                    Self::post_tags_info_async(reader, &mut opus_info).await?;

                    return Ok(opus_info);
                }
            }
        }
    }

    /// Calculate length from last page asynchronously
    async fn post_tags_info_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        info: &mut OpusInfo,
    ) -> Result<()> {
        // Gracefully handle truncated files by leaving length as None
        // rather than failing outright, matching OggFLAC behavior.
        let last_page = match OggPage::find_last_async(reader, info.serial, true).await? {
            Some(page) => page,
            None => return Ok(()),
        };
        if last_page.position > 0 {
            // Calculate duration: (position - pre_skip) / 48000
            let effective_samples =
                (last_page.position as u64).saturating_sub(info.pre_skip as u64);
            let duration_secs = effective_samples as f64 / 48000.0;
            if duration_secs.is_finite() && duration_secs >= 0.0 && duration_secs <= u64::MAX as f64
            {
                info.length = Some(Duration::from_secs_f64(duration_secs));
            }
        }
        Ok(())
    }

    /// Parse tags asynchronously
    async fn parse_tags_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        serial: u32,
    ) -> Result<OpusTags> {
        let mut tags = OpusTags {
            inner: VCommentDict::new(),
            serial,
            padding: 0,
            pad_data: Vec::new(),
        };

        // Seek to start to find OpusTags pages
        reader.seek(SeekFrom::Start(0)).await?;

        // Collect all pages for the OpusTags packet
        let mut pages = Vec::new();
        let mut found_tags = false;
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();

        loop {
            let page = match OggPage::from_reader_async(reader).await {
                Ok(page) => page,
                Err(_) => break,
            };

            if page.serial == serial {
                if let Some(first_packet) = page.packets.first() {
                    if first_packet.len() >= 8 && first_packet.starts_with(b"OpusTags") {
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg Opus comment packet",
                        )?;
                        pages.push(page);
                        found_tags = true;
                    } else if found_tags {
                        let last_complete = pages
                            .last()
                            .ok_or_else(|| {
                                AudexError::InvalidData(
                                    "expected non-empty page list after tag header".into(),
                                )
                            })?
                            .is_complete();
                        if !last_complete {
                            OggPage::accumulate_page_bytes_with_limit(
                                limits,
                                &mut cumulative_bytes,
                                &page,
                                "Ogg Opus comment packet",
                            )?;
                            pages.push(page);
                        } else {
                            break;
                        }
                    }
                } else if found_tags {
                    // Continuation page with an empty packets vec -- still
                    // part of the multi-page OpusTags packet. Append it so
                    // that to_packets can reassemble the full comment data.
                    let last_complete = pages
                        .last()
                        .ok_or_else(|| {
                            AudexError::InvalidData(
                                "expected non-empty page list after tag header".into(),
                            )
                        })?
                        .is_complete();
                    if !last_complete {
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg Opus comment packet",
                        )?;
                        pages.push(page);
                    }
                }
            }
        }

        if pages.is_empty() {
            return Ok(tags);
        }

        // Reconstruct packets from pages
        let packets = OggPage::to_packets(&pages, false)?;
        if packets.is_empty() || packets[0].len() < 8 {
            return Ok(tags);
        }

        // Look for OpusTags packet
        if &packets[0][0..8] == b"OpusTags" {
            let comment_data = &packets[0][8..];
            let mut cursor = std::io::Cursor::new(comment_data);

            match tags
                .inner
                .load(&mut cursor, crate::vorbis::ErrorMode::Replace, false)
            {
                Ok(_) => {
                    let pos = cursor.position() as usize;
                    if pos < comment_data.len() {
                        let remaining = &comment_data[pos..];
                        // Cap to i32::MAX to prevent overflow on pathologically large padding
                        tags.padding = remaining.len().min(i32::MAX as usize) as i32;

                        if !remaining.is_empty() && (remaining[0] & 0x1) == 1 {
                            tags.pad_data = remaining.to_vec();
                            tags.padding = 0;
                        }
                    }
                }
                Err(_) => {
                    tags.inner = VCommentDict::new();
                    tags.padding = comment_data.len().min(i32::MAX as usize) as i32;
                }
            }
        }

        Ok(tags)
    }

    /// Save Ogg Opus file asynchronously
    ///
    /// Saves the current tags back to the file.
    pub async fn save_async(&mut self) -> Result<()> {
        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::InvalidOperation("No file path available for saving".to_string())
        })?;

        if let Some(ref tags) = self.tags {
            Self::inject_tags_async(path, tags).await?;
        }

        Ok(())
    }

    /// Inject tags into file asynchronously
    async fn inject_tags_async<P: AsRef<Path>>(path: P, tags: &OpusTags) -> Result<()> {
        let file_path = path.as_ref();

        // Open file for reading and writing
        let file = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(file_path)
            .await?;

        let mut reader = TokioBufReader::new(file);

        // Find OpusTags header pages
        let mut comment_pages = Vec::new();
        let mut found_tags = false;

        reader.seek(SeekFrom::Start(0)).await?;

        loop {
            let page = match OggPage::from_reader_async(&mut reader).await {
                Ok(page) => page,
                Err(_) => break,
            };

            if page.serial == tags.serial {
                if let Some(first_packet) = page.packets.first() {
                    if first_packet.len() >= 8 && first_packet.starts_with(b"OpusTags") {
                        comment_pages.push(page);
                        found_tags = true;
                    } else if found_tags {
                        let last_complete = comment_pages
                            .last()
                            .ok_or_else(|| {
                                AudexError::InvalidData(
                                    "expected non-empty page list after tag header".into(),
                                )
                            })?
                            .is_complete();
                        if !last_complete {
                            comment_pages.push(page);
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        if comment_pages.is_empty() {
            return Err(AudexError::InvalidData(
                "No OpusTags header found".to_string(),
            ));
        }

        // Reconstruct packets
        let old_packets = OggPage::to_packets(&comment_pages, false)?;
        if old_packets.is_empty() {
            return Err(AudexError::InvalidData("No packets found".to_string()));
        }

        // Create new OpusTags data: b"OpusTags" + vorbis comment bytes
        let mut comment_to_write = tags.inner.clone();
        // Only set Audex vendor string when there are actual tags to write
        if !comment_to_write.keys().is_empty() {
            comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
        }

        let mut vcomment_data = b"OpusTags".to_vec();
        let mut vcomment_bytes = Vec::new();
        comment_to_write.write(&mut vcomment_bytes, Some(false))?;
        vcomment_data.extend_from_slice(&vcomment_bytes);

        let mut new_packets = old_packets;

        if !tags.pad_data.is_empty() {
            // Preserve opaque data as-is
            new_packets[0] = vcomment_data;
            new_packets[0].extend_from_slice(&tags.pad_data);
        } else {
            // Calculate content_size and use PaddingInfo
            let content_size = {
                let file_meta = tokio::fs::metadata(file_path).await?;
                // Use saturating subtraction to prevent overflow on large or crafted values
                i64::try_from(file_meta.len())
                    .unwrap_or(i64::MAX)
                    .saturating_sub(i64::try_from(new_packets[0].len()).unwrap_or(0))
            };

            let padding_left = new_packets[0].len() as i64 - vcomment_data.len() as i64;

            let info = crate::tags::PaddingInfo::new(padding_left, content_size);
            let new_padding = info.get_padding_with(None::<fn(&crate::tags::PaddingInfo) -> i64>);

            new_packets[0] = vcomment_data;
            if new_padding > 0 {
                new_packets[0]
                    .extend_from_slice(&vec![0u8; usize::try_from(new_padding).unwrap_or(0)]);
            }
        }

        // Create new pages - try to preserve page layout
        let new_pages = OggPage::from_packets_try_preserve(new_packets.clone(), &comment_pages);

        let final_pages = if new_pages.is_empty() {
            let first_sequence = comment_pages[0].sequence;
            let last_position = comment_pages
                .last()
                .ok_or_else(|| AudexError::InvalidData("no comment pages collected".to_string()))?
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

        // Replace pages in file
        drop(reader);
        let mut writer = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(file_path)
            .await?;

        OggPage::replace_async(&mut writer, &comment_pages, final_pages).await?;

        Ok(())
    }

    /// Clear tags asynchronously
    ///
    /// Removes all tags from the file and saves the changes.
    pub async fn clear_async(&mut self) -> Result<()> {
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        let empty_tags = OpusTags {
            inner,
            serial: self.info.serial,
            padding: 0,
            pad_data: Vec::new(),
        };

        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::InvalidOperation("No file path available for deletion".to_string())
        })?;

        Self::inject_tags_async(path, &empty_tags).await?;
        self.tags = Some(empty_tags);

        Ok(())
    }

    /// Delete file tags asynchronously (standalone operation)
    ///
    /// Loads, clears, and saves tags in one operation.
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        let mut opus = Self::load_async(path).await?;
        opus.clear_async().await
    }
}

/// Standalone async function for clearing tags from a file
///
/// # Arguments
/// * `path` - Path to the file
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<Path>>(path: P) -> Result<()> {
    OggOpus::delete_async(path).await
}
