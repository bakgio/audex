//! Support for Ogg FLAC audio files.
//!
//! This module provides reading and writing capabilities for Ogg FLAC files, which
//! encapsulate lossless FLAC audio streams within an Ogg container. Ogg FLAC is an
//! alternative container format to native FLAC, offering better streaming support
//! and the ability to multiplex with other Ogg-based codecs.
//!
//! # File Format
//!
//! Ogg FLAC files consist of:
//! - **Ogg container**: A flexible container format supporting multiple logical streams
//! - **FLAC codec**: Free Lossless Audio Codec providing perfect audio reproduction
//! - **Vorbis Comments**: Metadata tags compatible with other Ogg formats
//!
//! ## Structure
//!
//! An Ogg FLAC file contains a series of Ogg pages that carry FLAC packets:
//!
//! 1. **Identification Header** (`\x7FFLAC`): Contains FLAC mapping version and STREAMINFO block
//! 2. **Vorbis Comment Block**: Metadata tags (TITLE, ARTIST, ALBUM, etc.)
//! 3. **Additional Metadata**: Optional FLAC metadata blocks (seektable, cuesheet, etc.)
//! 4. **Audio Data**: Compressed FLAC audio frames
//!
//! ## Differences from Native FLAC
//!
//! - **Container**: Ogg pages instead of native FLAC framing
//! - **Seeking**: Uses Ogg page granule positions instead of FLAC seektables
//! - **Metadata**: Stored as FLAC metadata blocks within Ogg packets
//! - **Streaming**: Better suited for network streaming due to Ogg's design
//!
//! # Audio Characteristics
//!
//! - **Compression**: Lossless (bit-perfect reproduction)
//! - **Sample Rates**: Up to 655,350 Hz
//! - **Channels**: 1-8 channels
//! - **Bit Depth**: 1-32 bits per sample
//! - **File Extension**: `.oga` or `.ogg`
//! - **MIME Type**: `audio/x-oggflac`
//!
//! # Tagging
//!
//! Ogg FLAC uses Vorbis Comments for metadata, the same tagging format used by
//! Ogg Vorbis and Ogg Opus. Tags support:
//!
//! - **Multi-value fields**: Multiple artists, genres, etc.
//! - **UTF-8 encoding**: Full Unicode support
//! - **Standard fields**: TITLE, ARTIST, ALBUM, DATE, TRACKNUMBER, etc.
//! - **Case-insensitive keys**: Field names normalized to lowercase
//! - **Embedded pictures**: Via METADATA_BLOCK_PICTURE field
//!
//! # Basic Usage
//!
//! ## Loading and Reading Tags
//!
//! ```no_run
//! use audex::oggflac::OggFlac;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! // Load an Ogg FLAC file
//! let oggflac = OggFlac::load("song.oga")?;
//!
//! // Access stream information
//! println!("Sample Rate: {} Hz", oggflac.info.sample_rate);
//! println!("Channels: {}", oggflac.info.channels);
//! println!("Bits per Sample: {}", oggflac.info.bits_per_sample);
//!
//! // Read metadata tags
//! if let Some(tags) = oggflac.tags() {
//!     if let Some(title) = tags.get_first("TITLE") {
//!         println!("Title: {}", title);
//!     }
//!     if let Some(artists) = tags.get("ARTIST") {
//!         for artist in artists {
//!             println!("Artist: {}", artist);
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Modifying and Saving Tags
//!
//! ```no_run
//! use audex::oggflac::OggFlac;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut oggflac = OggFlac::load("song.oga")?;
//!
//! // Modify tags
//! if let Some(tags) = oggflac.tags_mut() {
//!     tags.set_single("TITLE", "New Song Title".to_string());
//!     tags.set_single("ARTIST", "Artist Name".to_string());
//!     tags.set_single("ALBUM", "Album Title".to_string());
//!     tags.set_single("DATE", "2024".to_string());
//! }
//!
//! // Save changes back to file
//! oggflac.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Creating Tags
//!
//! ```no_run
//! use audex::oggflac::OggFlac;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut oggflac = OggFlac::load("song.oga")?;
//!
//! // Create tags if they don't exist
//! if oggflac.tags.is_none() {
//!     use audex::oggflac::OggFLACVComment;
//!     use audex::vorbis::VCommentDict;
//!
//!     oggflac.tags = Some(OggFLACVComment {
//!         inner: VCommentDict::new(),
//!         serial_number: oggflac.info.serial_number,
//!     });
//! }
//!
//! if let Some(tags) = oggflac.tags_mut() {
//!     tags.set_single("TITLE", "New Song".to_string());
//! }
//!
//! oggflac.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Clearing All Metadata
//!
//! ```no_run
//! use audex::oggflac::OggFlac;
//! use audex::FileType;
//!
//! # fn main() -> audex::Result<()> {
//! let mut oggflac = OggFlac::load("song.oga")?;
//! oggflac.clear()?;  // Removes all tags
//! # Ok(())
//! # }
//! ```
//!
//! # Asynchronous Operations
//!
//! When the `async` feature is enabled, asynchronous methods are available:
//!
//! ```ignore
//! // Note: This example requires the `async` feature and a tokio runtime.
//! // Enable with: audex = { version = "*", features = ["async"] }
//! use audex::oggflac::OggFlac;
//! use audex::{FileType, Tags};
//!
//! # async fn example() -> audex::Result<()> {
//! // Load file asynchronously
//! let mut oggflac = OggFlac::load_async("song.oga").await?;
//!
//! // Modify tags
//! if let Some(tags) = oggflac.tags_mut() {
//!     tags.set_single("TITLE", "Async Title".to_string());
//! }
//!
//! // Save asynchronously
//! oggflac.save_async().await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Technical Details
//!
//! ## FLAC Mapping in Ogg
//!
//! The Ogg FLAC mapping specification defines how FLAC packets are encapsulated:
//!
//! - **Mapping Version**: Currently 1.0
//! - **Identification Packet**: First packet starts with `\x7FFLAC` signature
//! - **STREAMINFO Block**: Embedded in identification packet (34 bytes)
//! - **Metadata Blocks**: Subsequent FLAC metadata blocks in separate packets
//! - **Audio Frames**: FLAC audio frames in remaining packets
//!
//! ## Granule Position
//!
//! Ogg pages use granule positions to track playback position:
//! - Measured in PCM samples from stream start
//! - Used for seeking and duration calculation
//! - Final page's granule position = total samples
//!
//! # Error Handling
//!
//! Common errors when working with Ogg FLAC files:
//!
//! - **`OggFLACHeaderError`**: Invalid FLAC identification header
//! - **`OggError`**: General Ogg container errors
//! - **`VorbisError`**: Invalid Vorbis Comment data
//!
//! # References
//!
//! - [Ogg FLAC Mapping Specification](https://xiph.org/flac/ogg_mapping.html)
//! - [FLAC Format Specification](https://xiph.org/flac/format.html)
//! - [Vorbis Comment Specification](https://www.xiph.org/vorbis/doc/v-comment.html)

use crate::VERSION_STRING;
use crate::ogg::OggPage;
use crate::vorbis::VCommentDict;
use crate::{AudexError, FileType, Result, StreamInfo};
use byteorder::{BigEndian, ReadBytesExt};
use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};
#[cfg(feature = "async")]
use tokio::io::{AsyncSeekExt, BufReader as TokioBufReader};

/// Legacy type alias for backward compatibility
pub use OggFlac as OGGFLAC;

/// General error type for Ogg container operations.
///
/// This error is raised when there are problems reading or writing Ogg container
/// pages, such as invalid page headers, corrupt data, or I/O failures.
///
/// # Examples
///
/// ```
/// use audex::oggflac::OggError;
///
/// let error = OggError("Invalid Ogg page header".to_string());
/// println!("Error: {}", error);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct OggError(pub String);

impl std::fmt::Display for OggError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for OggError {}

/// Error type for Ogg FLAC header parsing failures.
///
/// This error occurs when the FLAC identification header within an Ogg stream is
/// invalid, missing, or malformed. Common causes include:
///
/// - Missing or invalid `\x7FFLAC` signature
/// - Unsupported FLAC mapping version
/// - Invalid `fLaC` marker
/// - Truncated STREAMINFO block
/// - No FLAC stream found in Ogg container
///
/// # Examples
///
/// ```
/// use audex::oggflac::{OggFLACHeaderError, OggError};
///
/// let error = OggFLACHeaderError(OggError("Invalid FLAC marker".to_string()));
/// println!("Header error: {}", error);
/// // Output: "OggFLAC header error: Invalid FLAC marker"
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct OggFLACHeaderError(pub OggError);

impl std::fmt::Display for OggFLACHeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OggFLAC header error: {}", self.0)
    }
}

impl std::error::Error for OggFLACHeaderError {}

impl From<OggError> for OggFLACHeaderError {
    fn from(err: OggError) -> Self {
        OggFLACHeaderError(err)
    }
}

impl From<OggFLACHeaderError> for AudexError {
    fn from(err: OggFLACHeaderError) -> Self {
        AudexError::InvalidData(err.to_string())
    }
}

impl From<OggError> for AudexError {
    fn from(err: OggError) -> Self {
        AudexError::InvalidData(err.0)
    }
}

/// Audio stream information for Ogg FLAC files.
///
/// Contains the technical details of the FLAC audio stream encapsulated within
/// an Ogg container. This information is extracted from the STREAMINFO metadata
/// block in the FLAC identification header.
///
/// # Fields
///
/// - **`min_blocksize`**: Minimum block size in samples (typically 16-65535)
/// - **`max_blocksize`**: Maximum block size in samples (typically 16-65535)
/// - **`sample_rate`**: Sample rate in Hz (1-655350 Hz)
/// - **`channels`**: Number of audio channels (1-8)
/// - **`bits_per_sample`**: Bits per sample (1-32 bits)
/// - **`total_samples`**: Total number of PCM samples in the stream
/// - **`length`**: Duration of the audio calculated from total samples and sample rate
/// - **`serial_number`**: Ogg logical stream serial number for this FLAC stream
///
/// # Block Size
///
/// The block size determines how many samples are processed together:
/// - Fixed block size files have `min_blocksize == max_blocksize`
/// - Variable block size files may have different min/max values
/// - Larger blocks generally provide better compression but slower seeking
///
/// # Examples
///
/// ```no_run
/// use audex::oggflac::OggFlac;
/// use audex::FileType;
///
/// # fn main() -> audex::Result<()> {
/// let oggflac = OggFlac::load("song.oga")?;
/// let info = &oggflac.info;
///
/// println!("Sample Rate: {} Hz", info.sample_rate);
/// println!("Channels: {}", info.channels);
/// println!("Bit Depth: {} bits", info.bits_per_sample);
/// println!("Block Size: {} - {}", info.min_blocksize, info.max_blocksize);
///
/// if let Some(duration) = info.length {
///     println!("Duration: {:.2} seconds", duration.as_secs_f64());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default)]
pub struct OggFLACStreamInfo {
    /// Minimum block size in samples used in the stream
    pub min_blocksize: u16,
    /// Maximum block size in samples used in the stream
    pub max_blocksize: u16,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of audio channels (1-8)
    pub channels: u16,
    /// Bits per sample (1-32)
    pub bits_per_sample: u16,
    /// Total number of PCM samples in the stream
    pub total_samples: u64,
    /// Duration of the audio stream
    pub length: Option<Duration>,
    /// Ogg logical stream serial number
    pub serial_number: u32,
}

impl OggFLACStreamInfo {
    /// Create OggFLACStreamInfo by parsing from a readable source
    ///
    /// Parse OGG pages to find the FLAC stream
    /// the FLAC stream starting with b"\x7FFLAC".
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut stream_info = Self::default();
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;

        // Find the FLAC stream by looking for the identification header
        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(OggFLACHeaderError(OggError(
                    "No FLAC stream found within page limit".to_string(),
                ))
                .into());
            }

            let page = match OggPage::from_reader(reader) {
                Ok(page) => page,
                Err(_) => {
                    return Err(
                        OggFLACHeaderError(OggError("No FLAC stream found".to_string())).into(),
                    );
                }
            };

            // Look for FLAC identification packet starting with "\x7FFLAC"
            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 5 && &first_packet[0..5] == b"\x7FFLAC" {
                    stream_info.serial_number = page.serial;
                    stream_info.parse_header(first_packet)?;

                    // Calculate length from final page
                    stream_info.post_tags(reader)?;

                    return Ok(stream_info);
                }
            }
        }
    }

    /// Parse the FLAC identification header
    ///
    /// Format is: "\x7FFLAC" + >BBH4s (major, minor, packets, flac_marker) + FLAC stream info
    fn parse_header(&mut self, packet: &[u8]) -> Result<()> {
        if packet.len() < 13 {
            return Err(
                OggFLACHeaderError(OggError("Invalid FLAC header length".to_string())).into(),
            );
        }

        // Skip "\x7FFLAC" (5 bytes)
        let mut cursor = Cursor::new(&packet[5..]);

        // Parse header: >BBH4s
        let major = cursor.read_u8()?;
        let minor = cursor.read_u8()?;
        let _packets = cursor.read_u16::<BigEndian>()?;

        // Read FLAC marker (4 bytes)
        let mut flac_marker = [0u8; 4];
        cursor.read_exact(&mut flac_marker)?;

        // Validate version and marker
        if (major, minor) != (1, 0) {
            return Err(OggFLACHeaderError(OggError(format!(
                "unknown mapping version: {}.{}",
                major, minor
            )))
            .into());
        }

        if &flac_marker != b"fLaC" {
            return Err(OggFLACHeaderError(OggError("Invalid FLAC marker".to_string())).into());
        }

        // Parse FLAC stream info from remaining bytes (starting at byte 17)
        if packet.len() < 17 {
            return Err(OggFLACHeaderError(OggError(
                "Invalid FLAC stream info length".to_string(),
            ))
            .into());
        }

        let stream_data = &packet[17..];
        self.parse_flac_stream_info(stream_data)?;

        Ok(())
    }

    /// Parse FLAC stream info block from raw bytes
    fn parse_flac_stream_info(&mut self, data: &[u8]) -> Result<()> {
        // The stream data from Ogg FLAC is the raw STREAMINFO block (34 bytes)
        // without the metadata block header, per the Ogg FLAC mapping specification
        if data.len() < 34 {
            return Err(OggFLACHeaderError(OggError(format!(
                "STREAMINFO block too short: got {} bytes, need 34",
                data.len()
            )))
            .into());
        }

        // Parse STREAMINFO directly (no block header to skip in Ogg FLAC)
        let streaminfo_data = &data[0..34];
        let mut cursor = Cursor::new(streaminfo_data);

        // Parse STREAMINFO fields exactly like FLACStreamInfo
        self.min_blocksize = cursor.read_u16::<BigEndian>()?;
        self.max_blocksize = cursor.read_u16::<BigEndian>()?;

        // Skip min/max framesize (6 bytes)
        std::io::Seek::seek(&mut cursor, SeekFrom::Current(6))?;

        // Parse sample rate, channels, bits per sample, total samples (8 bytes combined)
        let combined = cursor.read_u64::<BigEndian>()?;

        self.sample_rate = ((combined >> 44) & 0xFFFFF) as u32; // 20 bits
        self.channels = (((combined >> 41) & 0x07) as u16) + 1; // 3 bits, +1 because encoded as channels-1
        self.bits_per_sample = (((combined >> 36) & 0x1F) as u16) + 1; // 5 bits, +1 because encoded as bps-1
        self.total_samples = combined & 0xFFFFFFFFF; // 36 bits

        // Reject a zero sample rate. The FLAC spec encodes 0 to mean "get from
        // STREAMINFO elsewhere," but a standalone Ogg FLAC stream must carry a
        // valid playable rate. Allowing 0 through would break duration calculations
        // and could cause division-by-zero in downstream consumers.
        if self.sample_rate == 0 {
            return Err(OggFLACHeaderError(OggError(
                "Invalid sample rate: 0 is not a valid playable rate".to_string(),
            ))
            .into());
        }

        Ok(())
    }

    /// Calculate stream length from audio data
    ///
    /// Finds the final page with this serial number to calculate accurate length.
    fn post_tags<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        if self.length.is_some() {
            return Ok(());
        }
        if let Some(last_page) =
            OggPage::find_last_with_finishing(reader, self.serial_number, true)?
        {
            if self.sample_rate > 0 && last_page.position >= 0 {
                let duration_secs = last_page.position as f64 / self.sample_rate as f64;
                if duration_secs.is_finite()
                    && duration_secs >= 0.0
                    && duration_secs <= u64::MAX as f64
                {
                    self.length = Some(Duration::from_secs_f64(duration_secs));
                }
            }
        }
        Ok(())
    }

    /// Format stream information for display
    ///
    /// Returns a formatted string describing the Ogg FLAC stream.
    pub fn pprint(&self) -> String {
        let duration = self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0);

        format!("Ogg FLAC, {:.2} seconds, {} Hz", duration, self.sample_rate)
    }
}

impl StreamInfo for OggFLACStreamInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        None
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
        if self.bits_per_sample > 0 {
            Some(self.bits_per_sample)
        } else {
            None
        }
    }
}

/// Vorbis Comment metadata container for Ogg FLAC files.
///
/// This struct wraps a [`VCommentDict`] and provides the tagging interface for
/// Ogg FLAC files. Vorbis Comments are the standard metadata format used across
/// all Ogg-based audio formats (Vorbis, Opus, FLAC).
///
/// # Fields
///
/// - **`inner`**: The underlying [`VCommentDict`] containing tag data
/// - **`serial_number`**: Ogg stream serial number for this FLAC stream
///
/// # Tag Format
///
/// Vorbis Comments store metadata as UTF-8 key-value pairs:
/// - Keys are case-insensitive (normalized to lowercase)
/// - Values are UTF-8 strings
/// - Multiple values per key are supported
/// - Common fields: TITLE, ARTIST, ALBUM, DATE, TRACKNUMBER, GENRE, etc.
///
/// # Deref Behavior
///
/// This struct implements `Deref` and `DerefMut` to [`VCommentDict`], allowing
/// direct access to all [`VCommentDict`] methods.
///
/// # Examples
///
/// ```no_run
/// use audex::oggflac::OggFlac;
/// use audex::{FileType, Tags};
///
/// # fn main() -> audex::Result<()> {
/// let mut oggflac = OggFlac::load("song.oga")?;
///
/// if let Some(tags) = oggflac.tags_mut() {
///     // Single-value tags
///     tags.set_single("TITLE", "Song Title".to_string());
///     tags.set_single("ALBUM", "Album Name".to_string());
///
///     // Multi-value tags (multiple artists)
///     tags.set("ARTIST", vec![
///         "Artist 1".to_string(),
///         "Artist 2".to_string(),
///     ]);
///
///     // Reading tags
///     if let Some(title) = tags.get_first("TITLE") {
///         println!("Title: {}", title);
///     }
///
///     // Removing tags
///     tags.remove("COMMENT");
/// }
///
/// oggflac.save()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default)]
pub struct OggFLACVComment {
    /// The underlying Vorbis Comment dictionary
    pub inner: VCommentDict,
    /// Ogg stream serial number identifying this FLAC stream
    pub serial_number: u32,
}

impl std::ops::Deref for OggFLACVComment {
    type Target = VCommentDict;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for OggFLACVComment {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl crate::Tags for OggFLACVComment {
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
        format!("OggFLACVComment({})", self.inner.keys().len())
    }

    fn module_name(&self) -> &'static str {
        "oggflac"
    }
}

impl OggFLACVComment {
    /// Create OggFLACVComment by reading from subsequent OGG pages
    ///
    /// Read OGG pages for comment data.
    pub fn from_reader<R: Read + Seek>(reader: &mut R, serial_number: u32) -> Result<Self> {
        let mut tags = OggFLACVComment {
            inner: VCommentDict::new(),
            serial_number,
        };

        // Collect all pages belonging to the comment block.
        // The comment may span multiple pages, so we must reassemble
        // the full packet using to_packets() before parsing.
        let mut pages = Vec::new();
        let mut found_comment = false;
        let mut pages_read: usize = 0;
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();

        while let Ok(page) = OggPage::from_reader(reader) {
            pages_read += 1;
            if pages_read > Self::MAX_PAGE_SEARCH {
                break;
            }
            if page.serial == serial_number {
                if let Some(first_packet) = page.packets.first() {
                    if first_packet.len() >= 4 {
                        let block_type = first_packet[0] & 0x7F;
                        if block_type == 4 {
                            OggPage::accumulate_page_bytes_with_limit(
                                limits,
                                &mut cumulative_bytes,
                                &page,
                                "Ogg FLAC comment packet",
                            )?;
                            pages.push(page);
                            found_comment = true;
                        } else if found_comment {
                            let is_complete = pages
                                .last()
                                .ok_or_else(|| {
                                    AudexError::InvalidData(
                                        "expected non-empty page list after comment block".into(),
                                    )
                                })?
                                .is_complete();
                            if !is_complete {
                                OggPage::accumulate_page_bytes_with_limit(
                                    limits,
                                    &mut cumulative_bytes,
                                    &page,
                                    "Ogg FLAC comment packet",
                                )?;
                                pages.push(page);
                            } else {
                                break;
                            }
                        }
                    }
                } else if found_comment {
                    // Continuation page with no new packet start
                    let is_complete = pages
                        .last()
                        .ok_or_else(|| {
                            AudexError::InvalidData(
                                "expected non-empty page list after comment block".into(),
                            )
                        })?
                        .is_complete();
                    if !is_complete {
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg FLAC comment packet",
                        )?;
                        pages.push(page);
                    }
                }
            }
        }

        if pages.is_empty() {
            return Ok(tags);
        }

        // Reconstruct full packets from collected pages
        let packets = OggPage::to_packets(&pages, false)?;
        if packets.is_empty() || packets[0].len() < 4 {
            return Ok(tags);
        }

        // Skip the metadata block header (4 bytes) and parse Vorbis comments
        let comment_data = &packets[0][4..];
        let mut cursor = Cursor::new(comment_data);

        let _ = tags
            .inner
            .load(&mut cursor, crate::vorbis::ErrorMode::Replace, false);

        Ok(tags)
    }

    /// Maximum number of OGG pages to search before giving up
    const MAX_PAGE_SEARCH: usize = 1024;

    /// Write tags back to OGG FLAC file using in-place page replacement.
    ///
    /// This method finds only the Vorbis comment pages, replaces them using
    /// `OggPage::replace()`
    ///
    /// Note: Unlike OggVorbis, OggFLAC does not use padding because the FLAC
    /// metadata block header encodes an explicit data size - padding null bytes
    /// would corrupt the block for parsers.
    pub fn inject<R: Read + Write + Seek + 'static>(
        &self,
        fileobj: &mut R,
        _padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()> {
        use crate::ogg::OggPage;

        // Ogg FLAC: the page immediately after the identification header
        // contains the Vorbis Comment metadata block.
        // First, find the page containing the FLAC identification header (\x7FFLAC).
        fileobj.seek(SeekFrom::Start(0))?;
        let mut page = OggPage::from_reader(fileobj)?;
        let mut pages_read = 1usize;
        while page.packets.is_empty() || !page.packets[0].starts_with(b"\x7FFLAC") {
            page = OggPage::from_reader(fileobj)?;
            pages_read += 1;
            if pages_read > Self::MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for FLAC header".to_string(),
                ));
            }
        }

        let first_page = page;

        // Find the comment block page (next sequence after the header, same serial).
        // We derive the expected sequence from the header page rather than
        // hardcoding 1, since the header page may not always be sequence 0
        // (e.g. in multiplexed or chained Ogg files).
        let expected_seq = first_page.sequence + 1;
        let mut page = OggPage::from_reader(fileobj)?;
        pages_read += 1;
        while !(page.sequence == expected_seq && page.serial == first_page.serial) {
            page = OggPage::from_reader(fileobj)?;
            pages_read += 1;
            if pages_read > Self::MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for comment block".to_string(),
                ));
            }
        }

        let mut old_pages = vec![page];

        // Collect all pages belonging to the comment packet
        loop {
            let last_page = old_pages.last().ok_or_else(|| {
                AudexError::InvalidData(
                    "expected non-empty page list while reading comment pages".into(),
                )
            })?;
            if last_page.is_complete() || last_page.packets.len() > 1 {
                break;
            }
            let page = OggPage::from_reader(fileobj)?;
            pages_read += 1;
            if pages_read > Self::MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while reading comment pages".to_string(),
                ));
            }
            if page.serial == first_page.serial {
                old_pages.push(page);
            }
        }

        let packets = OggPage::to_packets(&old_pages, false)?;
        if packets.is_empty() {
            return Err(AudexError::InvalidData("No packets found".to_string()));
        }

        // Create new Vorbis comment data (no framing bit for OGG FLAC)
        let mut comment_to_write = self.inner.clone();
        // Only set Audex vendor string when there are actual tags to write
        if !comment_to_write.keys().is_empty() {
            comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
        }
        let mut comment_data = Vec::new();
        comment_to_write.write(&mut comment_data, Some(false))?;

        // The block size field is only 24 bits wide (3 bytes). Reject data
        // that would overflow the field to prevent silent truncation.
        if comment_data.len() > 0xFF_FFFF {
            return Err(AudexError::InvalidData(format!(
                "OGG FLAC comment data size ({}) exceeds 24-bit maximum ({})",
                comment_data.len(),
                0xFF_FFFF
            )));
        }

        let header_byte = packets[0][0];
        let block_size = comment_data.len() as u32;
        let mut new_comment_packet = Vec::with_capacity(4 + comment_data.len());
        new_comment_packet.push(header_byte);
        new_comment_packet.extend_from_slice(&[
            ((block_size >> 16) & 0xFF) as u8,
            ((block_size >> 8) & 0xFF) as u8,
            (block_size & 0xFF) as u8,
        ]);
        new_comment_packet.extend_from_slice(&comment_data);

        // Replace packets[0] with new comment data
        let mut new_packets = packets;
        new_packets[0] = new_comment_packet;

        // Create new pages using from_packets
        let new_pages = OggPage::from_packets(new_packets, old_pages[0].sequence, 4096, 2048);

        if new_pages.is_empty() {
            return Err(AudexError::InvalidData(
                "Failed to create new OGG pages".to_string(),
            ));
        }

        OggPage::replace(fileobj, &old_pages, new_pages)?;

        Ok(())
    }
}

/// Represents an Ogg FLAC audio file with metadata and stream information.
///
/// This is the primary interface for working with Ogg FLAC files. It provides
/// access to both the audio stream information and Vorbis Comment metadata tags.
///
/// # Fields
///
/// - **`info`**: Audio stream information ([`OggFLACStreamInfo`])
/// - **`tags`**: Optional Vorbis Comment metadata ([`OggFLACVComment`])
///
/// # File Format
///
/// Ogg FLAC encapsulates lossless FLAC audio within an Ogg container, combining:
/// - **Lossless compression**: Bit-perfect audio reproduction
/// - **Flexible container**: Ogg's streaming-friendly design
/// - **Standard metadata**: Vorbis Comments compatible with other Ogg formats
///
/// Common file extensions: `.oga`, `.ogg`
///
/// # Examples
///
/// ## Loading and Reading Information
///
/// ```no_run
/// use audex::oggflac::OggFlac;
/// use audex::FileType;
///
/// # fn main() -> audex::Result<()> {
/// let oggflac = OggFlac::load("song.oga")?;
///
/// // Access stream information
/// println!("Sample Rate: {} Hz", oggflac.info.sample_rate);
/// println!("Channels: {}", oggflac.info.channels);
/// println!("Bits per Sample: {}", oggflac.info.bits_per_sample);
///
/// if let Some(duration) = oggflac.info.length {
///     println!("Duration: {:.2} seconds", duration.as_secs_f64());
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Working with Tags
///
/// ```no_run
/// use audex::oggflac::OggFlac;
/// use audex::{FileType, Tags};
///
/// # fn main() -> audex::Result<()> {
/// let mut oggflac = OggFlac::load("song.oga")?;
///
/// if let Some(tags) = oggflac.tags_mut() {
///     // Read existing tags
///     if let Some(title) = tags.get_first("TITLE") {
///         println!("Current title: {}", title);
///     }
///
///     // Modify tags
///     tags.set_single("TITLE", "New Title".to_string());
///     tags.set_single("ARTIST", "Artist Name".to_string());
///     tags.set_single("ALBUM", "Album Name".to_string());
///     tags.set_single("DATE", "2024".to_string());
/// }
///
/// // Save changes
/// oggflac.save()?;
/// # Ok(())
/// # }
/// ```
///
/// ## Removing All Metadata
///
/// ```no_run
/// use audex::oggflac::OggFlac;
/// use audex::FileType;
///
/// # fn main() -> audex::Result<()> {
/// let mut oggflac = OggFlac::load("song.oga")?;
/// oggflac.clear()?;  // Removes all tags
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
/// use audex::oggflac::OggFlac;
/// use audex::{FileType, Tags};
///
/// # async fn example() -> audex::Result<()> {
/// let mut oggflac = OggFlac::load_async("song.oga").await?;
///
/// if let Some(tags) = oggflac.tags_mut() {
///     tags.set_single("TITLE", "Async Title".to_string());
/// }
///
/// oggflac.save_async().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct OggFlac {
    /// Audio stream information extracted from the STREAMINFO block
    pub info: OggFLACStreamInfo,
    /// Optional Vorbis Comment metadata tags
    pub tags: Option<OggFLACVComment>,
    /// Path to the file (used for saving)
    filename: String,
}

impl OggFlac {
    /// Create a new empty OggFlac instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: OggFLACStreamInfo::default(),
            tags: None,
            filename: String::new(),
        }
    }

    /// Load Ogg FLAC file from path
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing OGG FLAC file");
        let file_path = path.as_ref();
        let mut file = BufReader::new(File::open(file_path)?);

        let mut oggflac = Self::new();
        oggflac.filename = file_path.to_string_lossy().to_string();

        // Parse stream info
        oggflac.info = OggFLACStreamInfo::from_reader(&mut file)?;

        // Parse comments
        file.seek(SeekFrom::Start(0))?;
        oggflac.tags = Some(OggFLACVComment::from_reader(
            &mut file,
            oggflac.info.serial_number,
        )?);
        if let Some(ref _tags) = oggflac.tags {
            debug_event!(tag_count = _tags.keys().len(), "OGG FLAC tags loaded");
        }

        Ok(oggflac)
    }

    /// Save tags with optional custom padding function and optional new path.
    ///
    /// This is the primary save method that supports padding calculation for
    /// optimal file size management, matching the approach used by OggVorbis.
    pub fn save_with_options<P>(
        &mut self,
        path: Option<P>,
        padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        use std::fs::OpenOptions;

        let (file_path, is_new_path) = match &path {
            Some(p) => (p.as_ref().to_string_lossy().to_string(), true),
            None => (self.filename.clone(), false),
        };

        if file_path.is_empty() {
            return Err(AudexError::InvalidData(
                "No filename available for saving".to_string(),
            ));
        }

        let tags = self
            .tags
            .as_ref()
            .ok_or_else(|| AudexError::InvalidData("No tags available for saving".to_string()))?;

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(Path::new(&file_path))?;
        tags.inject(&mut file, padding_func)?;

        if is_new_path {
            self.filename = file_path;
        }

        Ok(())
    }
}

impl Default for OggFlac {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for OggFlac {
    type Tags = OggFLACVComment;
    type Info = OggFLACStreamInfo;

    fn format_id() -> &'static str {
        "OggFlac"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file(path)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        let mut reader = reader;
        let mut oggflac = Self::new();
        oggflac.info = OggFLACStreamInfo::from_reader(&mut reader)?;
        reader.seek(std::io::SeekFrom::Start(0))?;
        oggflac.tags = Some(OggFLACVComment::from_reader(
            &mut reader,
            oggflac.info.serial_number,
        )?);
        Ok(oggflac)
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving OGG FLAC metadata");
        self.save_with_options(None::<&str>, None)
    }

    fn clear(&mut self) -> Result<()> {
        // Preserve the previous tags so we can restore them if the save fails.
        // Without this, a failed save would leave in-memory tags wiped while the
        // file remains unchanged, putting the object in an inconsistent state.
        let prev_tags = self.tags.take();

        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        self.tags = Some(OggFLACVComment {
            inner,
            serial_number: self.info.serial_number,
        });

        if let Err(e) = self.save() {
            self.tags = prev_tags;
            return Err(e);
        }
        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        let tags = self
            .tags
            .as_ref()
            .ok_or_else(|| AudexError::InvalidData("No tags available for saving".to_string()))?;

        // Read all data into memory so we can work with a Cursor that is
        // Sized + 'static, which the inject method requires.
        let buf = crate::util::read_all_from_writer_limited(writer, "in-memory Ogg FLAC save")?;
        let mut cursor = Cursor::new(buf);
        tags.inject(&mut cursor, None)?;

        // Write the modified data back to the original writer
        let result = cursor.into_inner();
        writer.seek(std::io::SeekFrom::Start(0))?;
        writer.write_all(&result)?;
        crate::util::truncate_writer_dyn(writer, result.len() as u64)?;

        Ok(())
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        self.tags = Some(OggFLACVComment {
            inner,
            serial_number: self.info.serial_number,
        });
        self.save_to_writer(writer)
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        self.save_with_options(Some(path), None)
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
    /// use audex::oggflac::OggFlac;
    /// use audex::FileType;
    ///
    /// let mut flac = OggFlac::load("song.oga")?;
    /// if flac.tags.is_none() {
    ///     flac.add_tags()?;
    /// }
    /// flac.set("title", vec!["My Song".to_string()])?;
    /// flac.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(OggFLACVComment {
            inner: VCommentDict::new(),
            serial_number: self.info.serial_number,
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
    fn score(_filename: &str, header: &[u8]) -> i32 {
        let oggs_score = if header.len() >= 4 && &header[0..4] == b"OggS" {
            1
        } else {
            0
        };

        let flac_score = {
            let has_flac = header.windows(4).any(|window| window == b"FLAC");
            let has_flac_lower = header.windows(4).any(|window| window == b"fLaC");

            (if has_flac { 1 } else { 0 }) + (if has_flac_lower { 1 } else { 0 })
        };

        oggs_score * flac_score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/x-oggflac"]
    }
}

/// Remove all metadata tags from an Ogg FLAC file.
///
/// This convenience function loads the specified Ogg FLAC file, removes all
/// Vorbis Comment tags, and saves the changes back to the file.
///
/// # Arguments
///
/// * `path` - Path to the Ogg FLAC file
///
/// # Returns
///
/// Returns `Ok(())` if tags were successfully removed, or an error if the file
/// could not be loaded or saved.
///
/// # Examples
///
/// ```no_run
/// use audex::oggflac;
///
/// # fn main() -> audex::Result<()> {
/// // Remove all tags from a file
/// oggflac::clear("song.oga")?;
/// # Ok(())
/// # }
/// ```
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut oggflac = OggFlac::load(path)?;
    oggflac.clear()
}

/// Async methods for OggFlac
///
/// These methods provide asynchronous I/O operations for loading, saving, and
/// manipulating Ogg FLAC files without blocking the executor.
#[cfg(feature = "async")]
impl OggFlac {
    /// Load an Ogg FLAC file asynchronously from the specified path.
    ///
    /// This method opens the file using tokio's async file I/O, parses the
    /// Ogg FLAC stream information and Vorbis comments without blocking.
    ///
    /// # Arguments
    /// * `path` - The path to the Ogg FLAC file to load
    ///
    /// # Returns
    /// A Result containing the loaded OggFlac instance or an error
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = TokioFile::open(&path_buf).await?;
        let mut reader = TokioBufReader::new(file);

        // Parse stream info
        reader.seek(SeekFrom::Start(0)).await?;
        let info = Self::parse_info_async(&mut reader).await?;

        // Parse tags
        let tags = Self::parse_tags_async(&mut reader, info.serial_number).await?;

        Ok(Self {
            info,
            tags: Some(tags),
            filename: path_buf.to_string_lossy().to_string(),
        })
    }

    /// Parse Ogg FLAC stream information asynchronously.
    ///
    /// Async I/O for Ogg page reading, delegates packet parsing to the sync
    /// `parse_header` (pure in-memory computation on `&[u8]`).
    async fn parse_info_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
    ) -> Result<OggFLACStreamInfo> {
        let mut stream_info = OggFLACStreamInfo::default();

        // Find the FLAC stream by looking for the identification header
        loop {
            let page = match OggPage::from_reader_async(reader).await {
                Ok(page) => page,
                Err(_) => {
                    return Err(AudexError::InvalidData("No FLAC stream found".to_string()));
                }
            };

            // Look for FLAC identification packet starting with "\x7FFLAC"
            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 5 && &first_packet[0..5] == b"\x7FFLAC" {
                    stream_info.serial_number = page.serial;
                    stream_info.parse_header(first_packet)?;

                    // Calculate length from last page
                    Self::post_tags_info_async(reader, &mut stream_info).await?;

                    return Ok(stream_info);
                }
            }
        }
    }

    /// Calculate stream duration from the last Ogg page asynchronously.
    ///
    /// Seeks to find the final page of the stream to determine the total
    /// number of samples, then calculates the duration based on sample rate.
    ///
    /// # Arguments
    /// * `reader` - An async reader for the Ogg stream
    /// * `info` - Mutable reference to the stream info to update with duration
    ///
    /// # Returns
    /// A Result indicating success or an error
    async fn post_tags_info_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        info: &mut OggFLACStreamInfo,
    ) -> Result<()> {
        if info.length.is_some() {
            return Ok(());
        }
        if let Some(last_page) = OggPage::find_last_async(reader, info.serial_number, true).await? {
            if info.sample_rate > 0 && last_page.position > 0 {
                let duration_secs = last_page.position as f64 / info.sample_rate as f64;
                if duration_secs.is_finite()
                    && duration_secs >= 0.0
                    && duration_secs <= u64::MAX as f64
                {
                    info.length = Some(std::time::Duration::from_secs_f64(duration_secs));
                }
            }
        }
        Ok(())
    }

    /// Parse Vorbis comment tags asynchronously.
    ///
    /// Searches for the Vorbis comment metadata block in the Ogg stream and
    /// parses the key-value pairs into an OggFLACVComment structure.
    ///
    /// # Arguments
    /// * `reader` - An async reader for the Ogg stream
    /// * `serial_number` - The serial number of the FLAC stream
    ///
    /// # Returns
    /// A Result containing the parsed OggFLACVComment or an error
    async fn parse_tags_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        serial_number: u32,
    ) -> Result<OggFLACVComment> {
        reader.seek(SeekFrom::Start(0)).await?;

        let mut pages = Vec::new();
        let mut found_tags = false;
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();

        loop {
            let page = match OggPage::from_reader_async(reader).await {
                Ok(page) => page,
                Err(_) => break,
            };

            if page.serial == serial_number {
                if let Some(first_packet) = page.packets.first() {
                    // Look for Vorbis comment metadata block (type 4)
                    if first_packet.len() >= 4 {
                        let block_type = first_packet[0] & 0x7F;
                        if block_type == 4 {
                            OggPage::accumulate_page_bytes_with_limit(
                                limits,
                                &mut cumulative_bytes,
                                &page,
                                "Ogg FLAC comment packet",
                            )?;
                            pages.push(page);
                            found_tags = true;
                        } else if found_tags {
                            let is_complete = pages
                                .last()
                                .ok_or_else(|| {
                                    AudexError::InvalidData(
                                        "expected non-empty page list after comment block".into(),
                                    )
                                })?
                                .is_complete();
                            if !is_complete {
                                OggPage::accumulate_page_bytes_with_limit(
                                    limits,
                                    &mut cumulative_bytes,
                                    &page,
                                    "Ogg FLAC comment packet",
                                )?;
                                pages.push(page);
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
        }

        let mut tags = OggFLACVComment {
            inner: VCommentDict::new(),
            serial_number,
        };

        if pages.is_empty() {
            return Ok(tags);
        }

        // Reconstruct packets from pages
        let packets = OggPage::to_packets(&pages, false)?;
        if packets.is_empty() || packets[0].len() < 4 {
            return Ok(tags);
        }

        // Skip the metadata block header (4 bytes) and parse Vorbis comments
        let comment_data = &packets[0][4..];
        let mut cursor = Cursor::new(comment_data);

        let _ = tags
            .inner
            .load(&mut cursor, crate::vorbis::ErrorMode::Replace, false);

        Ok(tags)
    }

    /// Save the Ogg FLAC file asynchronously.
    ///
    /// Writes the current tags back to the file using async I/O operations.
    /// The file is updated in place with the modified Vorbis comments.
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn save_async(&mut self) -> Result<()> {
        self.save_with_options_async(None::<&str>, None).await
    }

    /// Save tags asynchronously with optional custom padding function and optional new path.
    pub async fn save_with_options_async<P>(
        &mut self,
        path: Option<P>,
        padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let (file_path, is_new_path) = match &path {
            Some(p) => (p.as_ref().to_string_lossy().to_string(), true),
            None => (self.filename.clone(), false),
        };

        if file_path.is_empty() {
            return Err(AudexError::InvalidData(
                "No filename available for saving".to_string(),
            ));
        }

        let tags = self
            .tags
            .as_ref()
            .ok_or_else(|| AudexError::InvalidData("No tags available for saving".to_string()))?;

        let mut file = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)
            .await?;

        Self::inject_tags_async(&mut file, tags, padding_func).await?;

        if is_new_path {
            self.filename = file_path;
        }

        Ok(())
    }

    /// Inject tags into an Ogg FLAC file asynchronously.
    ///
    /// No padding is used because the FLAC metadata block header encodes an
    /// explicit data size - padding null bytes would corrupt the block for parsers.
    async fn inject_tags_async(
        fileobj: &mut TokioFile,
        tags: &OggFLACVComment,
        _padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()> {
        const MAX_PAGE_SEARCH: usize = 1024;

        // Find the page containing the FLAC identification header (\x7FFLAC)
        fileobj.seek(SeekFrom::Start(0)).await?;
        let mut page = OggPage::from_reader_async(fileobj).await?;
        let mut pages_read = 1usize;
        while page.packets.is_empty() || !page.packets[0].starts_with(b"\x7FFLAC") {
            page = OggPage::from_reader_async(fileobj).await?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for FLAC header".to_string(),
                ));
            }
        }

        let first_page = page;

        // Find the comment block page (next sequence after the header, same serial).
        // We derive the expected sequence from the header page rather than
        // hardcoding 1, since the header page may not always be sequence 0
        // (e.g. in multiplexed or chained Ogg files).
        let expected_seq = first_page.sequence + 1;
        let mut page = OggPage::from_reader_async(fileobj).await?;
        pages_read += 1;
        while !(page.sequence == expected_seq && page.serial == first_page.serial) {
            page = OggPage::from_reader_async(fileobj).await?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for comment block".to_string(),
                ));
            }
        }

        let mut old_pages = vec![page];

        // Collect all pages belonging to the comment packet
        loop {
            let last_page = old_pages.last().ok_or_else(|| {
                AudexError::InvalidData(
                    "expected non-empty page list while reading comment pages".into(),
                )
            })?;
            if last_page.is_complete() || last_page.packets.len() > 1 {
                break;
            }
            let page = OggPage::from_reader_async(fileobj).await?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while reading comment pages".to_string(),
                ));
            }
            if page.serial == first_page.serial {
                old_pages.push(page);
            }
        }

        let packets = OggPage::to_packets(&old_pages, false)?;
        if packets.is_empty() {
            return Err(AudexError::InvalidData("No packets found".to_string()));
        }

        // Create new Vorbis comment data (no framing bit for OGG FLAC)
        let mut comment_to_write = tags.inner.clone();
        // Only set Audex vendor string when there are actual tags to write
        if !comment_to_write.keys().is_empty() {
            comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
        }
        let mut comment_data = Vec::new();
        comment_to_write.write(&mut comment_data, Some(false))?;

        // The block size field is only 24 bits wide (3 bytes). Reject data
        // that would overflow the field to prevent silent truncation.
        if comment_data.len() > 0xFF_FFFF {
            return Err(AudexError::InvalidData(format!(
                "OGG FLAC comment data size ({}) exceeds 24-bit maximum ({})",
                comment_data.len(),
                0xFF_FFFF
            )));
        }

        let header_byte = packets[0][0];
        let block_size = comment_data.len() as u32;
        let mut new_comment_packet = Vec::with_capacity(4 + comment_data.len());
        new_comment_packet.push(header_byte);
        new_comment_packet.extend_from_slice(&[
            ((block_size >> 16) & 0xFF) as u8,
            ((block_size >> 8) & 0xFF) as u8,
            (block_size & 0xFF) as u8,
        ]);
        new_comment_packet.extend_from_slice(&comment_data);

        // Replace packets[0] with new comment data
        let mut new_packets = packets;
        new_packets[0] = new_comment_packet;

        // Create new pages using from_packets
        let new_pages = OggPage::from_packets(new_packets, old_pages[0].sequence, 4096, 2048);

        if new_pages.is_empty() {
            return Err(AudexError::InvalidData(
                "Failed to create new OGG pages".to_string(),
            ));
        }

        OggPage::replace_async(fileobj, &old_pages, new_pages).await?;

        Ok(())
    }

    /// Clear all tags from the Ogg FLAC file asynchronously.
    ///
    /// Replaces all existing Vorbis comments with an empty comment block,
    /// effectively removing all metadata from the file.
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn clear_async(&mut self) -> Result<()> {
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        let empty_tags = OggFLACVComment {
            inner,
            serial_number: self.info.serial_number,
        };

        self.tags = Some(empty_tags);
        self.save_async().await?;

        Ok(())
    }

    /// Delete all tags from an Ogg FLAC file at the specified path asynchronously.
    ///
    /// This is a convenience method that loads the file, clears its tags, and
    /// saves it back in a single operation.
    ///
    /// # Arguments
    /// * `path` - The path to the Ogg FLAC file
    ///
    /// # Returns
    /// A Result indicating success or an error
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        let mut oggflac = Self::load_async(path).await?;
        oggflac.clear_async().await
    }
}

/// Standalone async function for clearing tags from an Ogg FLAC file.
///
/// This function provides a convenient way to remove all Vorbis comments
/// from an Ogg FLAC file without manually loading and managing the file object.
///
/// # Arguments
/// * `path` - The path to the Ogg FLAC file
///
/// # Returns
/// A Result indicating success or an error
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<Path>>(path: P) -> Result<()> {
    OggFlac::delete_async(path).await
}
