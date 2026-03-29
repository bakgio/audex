//! # Ogg Vorbis Format Support
//!
//! This module provides comprehensive support for reading and writing Ogg Vorbis audio files.
//! Ogg Vorbis is a free, open-source lossy audio codec offering high quality compression,
//! widely used for music streaming and distribution.
//!
//! ## Overview
//!
//! Vorbis audio is wrapped in an Ogg container format, which provides:
//! - **Streaming support**: Efficient sequential reading
//! - **Multiple logical streams**: Can contain multiple audio tracks
//! - **Metadata support**: Vorbis Comments for tagging
//! - **Error resilience**: Graceful degradation with corrupted data
//!
//! This module uses the first Vorbis stream found in the Ogg bitstream.
//!
//! ## File Format
//!
//! Ogg Vorbis files typically use the `.ogg` extension and consist of:
//! 1. **Identification header**: Codec version, channels, sample rate
//! 2. **Comment header**: Vorbis Comments (tags/metadata)
//! 3. **Setup header**: Codec configuration
//! 4. **Audio packets**: Compressed audio data
//!
//! ## Audio Characteristics
//!
//! - **Lossy compression**: Smaller file sizes than lossless formats
//! - **Variable bitrate (VBR)**: Optimizes quality per audio complexity
//! - **Sample rates**: Typically 8kHz to 192kHz
//! - **Channels**: Mono, stereo, or multichannel (up to 255 channels)
//!
//! ## Tagging
//!
//! Ogg Vorbis uses Vorbis Comments for metadata, which support:
//! - Human-readable field names (TITLE, ARTIST, ALBUM, etc.)
//! - Multiple values per field
//! - Case-insensitive field names
//! - UTF-8 encoded values
//!
//! ## Examples
//!
//! ### Loading and reading file information
//!
//! ```no_run
//! use audex::oggvorbis::OggVorbis;
//! use audex::FileType;
//!
//! let vorbis = OggVorbis::load("song.ogg").unwrap();
//!
//! if let Some(ref info) = vorbis.info {
//!     println!("Sample rate: {} Hz", info.sample_rate);
//!     println!("Channels: {}", info.channels);
//!     println!("Duration: {:?}", info.length);
//!
//!     // Bitrate values are in bits per second, divide by 1000 for kbps
//!     if let Some(bitrate) = info.bitrate {
//!         println!("Bitrate: {} kbps", bitrate / 1000);
//!     }
//!
//!     if let Some(nominal) = info.nominal_bitrate {
//!         println!("Nominal bitrate: {} kbps", nominal / 1000);
//!     }
//! }
//! ```
//!
//! ### Reading and modifying tags
//!
//! ```no_run
//! use audex::oggvorbis::OggVorbis;
//! use audex::FileType;
//! use audex::Tags;
//!
//! let mut vorbis = OggVorbis::load("song.ogg").unwrap();
//!
//! if let Some(ref mut tags) = vorbis.tags {
//!     // Read existing tags
//!     if let Some(title) = tags.get("TITLE") {
//!         println!("Title: {}", title[0]);
//!     }
//!
//!     // Modify tags using set for Vorbis Comments
//!     tags.set("TITLE", vec!["New Title".to_string()]);
//!     tags.set("ARTIST", vec!["Artist Name".to_string()]);
//!     tags.set("ALBUM", vec!["Album Name".to_string()]);
//!     tags.set("DATE", vec!["2024".to_string()]);
//! }
//!
//! vorbis.save().unwrap();
//! ```
//!
//! ### Creating tags if they don't exist
//!
//! ```no_run
//! use audex::oggvorbis::OggVorbis;
//! use audex::FileType;
//!
//! let mut vorbis = OggVorbis::load("song.ogg").unwrap();
//!
//! if vorbis.tags.is_none() {
//!     vorbis.add_tags().unwrap();
//! }
//!
//! if let Some(ref mut tags) = vorbis.tags {
//!     tags.set("TITLE", vec!["Title".to_string()]);
//! }
//!
//! vorbis.save().unwrap();
//! ```
//!
//! ### Working with multiple values
//!
//! ```no_run
//! use audex::oggvorbis::OggVorbis;
//! use audex::FileType;
//! use audex::Tags;
//!
//! let mut vorbis = OggVorbis::load("song.ogg").unwrap();
//!
//! if let Some(ref mut tags) = vorbis.tags {
//!     // Add multiple artists using set
//!     tags.set("ARTIST", vec![
//!         "Artist One".to_string(),
//!         "Artist Two".to_string(),
//!     ]);
//!
//!     // Read all values
//!     if let Some(artists) = tags.get("ARTIST") {
//!         for artist in artists {
//!             println!("Artist: {}", artist);
//!         }
//!     }
//! }
//!
//! vorbis.save().unwrap();
//! ```
//!
//! ## References
//!
//! - Ogg Vorbis specification: <http://www.xiph.org/vorbis/doc/Vorbis_I_spec.html>
//! - Vorbis website: <http://vorbis.com/>
//! - Ogg container format: <http://www.xiph.org/ogg/>

use crate::VERSION_STRING;
use crate::ogg::OggPage;
use crate::vorbis::VCommentDict;
use crate::{AudexError, FileType, Result, StreamInfo};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Maximum number of OGG pages to read when searching for a specific packet.
/// Prevents OOM from malicious files with many small pages.
const MAX_PAGE_SEARCH: usize = 1024;

#[cfg(feature = "async")]
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};
#[cfg(feature = "async")]
use tokio::io::{AsyncSeekExt, BufReader as TokioBufReader};

/// Error type for Ogg Vorbis file operations.
///
/// This error occurs during parsing, reading, or writing Ogg Vorbis files.
/// It covers general Vorbis errors, I/O failures, and invalid file data.
///
/// # Variants
///
/// - **General**: General Ogg Vorbis processing errors
/// - **Io**: File system I/O errors during read/write operations
/// - **InvalidData**: Malformed or corrupted Vorbis data
///
/// # Common Causes
///
/// - Corrupted or incomplete Ogg Vorbis files
/// - Invalid header packets
/// - Malformed Vorbis Comment data
/// - I/O errors during file access
/// - Unsupported Vorbis encoder settings
///
/// # Examples
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::FileType;
///
/// match OggVorbis::load("corrupted.ogg") {
///     Ok(vorbis) => println!("Loaded successfully"),
///     Err(e) => eprintln!("Failed to load: {}", e),
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum OggVorbisError {
    #[error("Ogg Vorbis error: {0}")]
    General(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

/// Error type for Vorbis header parsing failures.
///
/// This error specifically occurs when parsing Vorbis stream headers
/// (identification, comment, or setup headers) that are malformed or invalid.
///
/// # Variants
///
/// - **InvalidHeader**: Header packet is malformed or doesn't meet specification
/// - **Io**: I/O error while reading header data
///
/// # Common Causes
///
/// - Invalid Vorbis identification header signature
/// - Sample rate set to zero (invalid per specification)
/// - Channel count set to zero
/// - Truncated header packets
/// - Incorrect packet type indicators
/// - File is not a valid Ogg Vorbis file
///
/// # Examples
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::FileType;
///
/// match OggVorbis::load("invalid_header.ogg") {
///     Ok(vorbis) => println!("Valid Vorbis file"),
///     Err(e) => eprintln!("Header parsing failed: {}", e),
/// }
/// ```
#[derive(Debug, thiserror::Error)]
pub enum OggVorbisHeaderError {
    #[error("Header error: {0}")]
    InvalidHeader(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<OggVorbisError> for AudexError {
    fn from(err: OggVorbisError) -> Self {
        match err {
            OggVorbisError::General(msg) => AudexError::InvalidData(msg),
            OggVorbisError::Io(e) => AudexError::Io(e),
            OggVorbisError::InvalidData(msg) => AudexError::InvalidData(msg),
        }
    }
}

impl From<OggVorbisHeaderError> for AudexError {
    fn from(err: OggVorbisHeaderError) -> Self {
        match err {
            OggVorbisHeaderError::InvalidHeader(msg) => AudexError::InvalidData(msg),
            OggVorbisHeaderError::Io(e) => AudexError::Io(e),
        }
    }
}

/// Vorbis comments embedded in an Ogg bitstream
#[derive(Debug, Clone)]
pub struct OggVCommentDict {
    inner: VCommentDict,
}

impl Default for OggVCommentDict {
    fn default() -> Self {
        Self::new()
    }
}

impl OggVCommentDict {
    /// Create new empty Vorbis comments
    pub fn new() -> Self {
        Self {
            inner: VCommentDict::new(),
        }
    }

    /// Create from file object and info
    pub fn from_fileobj<R: Read + Seek>(fileobj: &mut R, info: &OggVorbisInfo) -> Result<Self> {
        let mut pages = Vec::new();
        let mut complete = false;
        let mut pages_read = 0usize;
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();

        while !complete {
            let page = OggPage::from_reader(fileobj)?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for comment packet".to_string(),
                ));
            }
            if page.serial == info.serial {
                OggPage::accumulate_page_bytes_with_limit(
                    limits,
                    &mut cumulative_bytes,
                    &page,
                    "OGG Vorbis comment packet",
                )?;
                pages.push(page.clone());
                complete = page.is_complete() || page.packets.len() > 1;
            }
        }

        let packets = OggPage::to_packets(&pages, false)?;
        if packets.is_empty() || packets[0].len() < 7 {
            return Err(AudexError::InvalidData(
                "Invalid Vorbis comment packet".to_string(),
            ));
        }

        // Verify the packet starts with the Vorbis comment header
        // (type byte 0x03 followed by "vorbis") before stripping it.
        if &packets[0][..7] != b"\x03vorbis" {
            return Err(AudexError::InvalidData(format!(
                "Expected Vorbis comment header (\\x03vorbis), got {:?}",
                &packets[0][..7]
            )));
        }
        let data = &packets[0][7..];

        let inner =
            VCommentDict::from_bytes_with_options(data, crate::vorbis::ErrorMode::Replace, true)?;

        // Store original data and calculate padding
        let _original_data = Some(data.to_vec());
        // Calculate size with framing bit to match actual written size
        let mut size_buffer = Vec::new();
        inner.write(&mut size_buffer, Some(true))?;
        let _vcomment_size = size_buffer.len();
        let _original_padding = data.len().saturating_sub(_vcomment_size);

        Ok(Self { inner })
    }

    /// Inject tags into the file
    pub fn inject<R: Read + Write + Seek + 'static>(
        &self,
        fileobj: &mut R,
        padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()> {
        // Find the old pages in the file; we'll need to remove them,
        // plus grab any stray setup packet data out of them.
        fileobj.seek(SeekFrom::Start(0))?;

        // Read the first page to obtain the stream serial number.
        // In a multiplexed Ogg file multiple logical streams share the
        // container, so we must filter by serial when searching for the
        // Vorbis comment header to avoid matching a different stream.
        let first_page = OggPage::from_reader(fileobj)?;
        let stream_serial = first_page.serial;
        let mut page = first_page;
        let mut pages_read = 1usize;
        while page.packets.is_empty()
            || !page.packets[0].starts_with(b"\x03vorbis")
            || page.serial != stream_serial
        {
            page = OggPage::from_reader(fileobj)?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for comment header".to_string(),
                ));
            }
        }

        let mut old_pages = vec![page];
        // Collect all pages belonging to the comment packet
        loop {
            let last_page = old_pages.last().ok_or_else(|| {
                AudexError::InvalidData(
                    "expected non-empty page list while reading comments".into(),
                )
            })?;
            if last_page.is_complete() || last_page.packets.len() > 1 {
                break;
            }
            let page = OggPage::from_reader(fileobj)?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while reading comment pages".to_string(),
                ));
            }
            if page.serial == old_pages[0].serial {
                old_pages.push(page);
            }
        }

        let packets = OggPage::to_packets(&old_pages, false)?;
        if packets.is_empty() {
            return Err(AudexError::InvalidData("No packets found".to_string()));
        }

        // Calculate content size (approximate) - file size minus first packet
        // get_size preserves file position, so we must do the same
        let content_size = {
            let old_pos = fileobj.stream_position()?;
            let file_size = fileobj.seek(SeekFrom::End(0))?;
            fileobj.seek(SeekFrom::Start(old_pos))?; // Restore position
            // Use saturating subtraction to prevent overflow on large or crafted values
            i64::try_from(file_size)
                .unwrap_or(i64::MAX)
                .saturating_sub(i64::try_from(packets[0].len()).unwrap_or(0))
        };

        // Create Vorbis comment data
        let vcomment_data = {
            let mut data = b"\x03vorbis".to_vec();
            let mut vcomment_bytes = Vec::new();

            // Create a copy of our inner VCommentDict
            let mut comment_to_write = self.inner.clone();
            // Set vendor string based on whether there are actual tags to write
            if !comment_to_write.keys().is_empty() {
                comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
            }

            // Use framing=true by default
            comment_to_write.write(&mut vcomment_bytes, Some(true))?;
            data.extend_from_slice(&vcomment_bytes);
            data
        };
        let padding_left = packets[0].len() as i64 - vcomment_data.len() as i64;

        // Use PaddingInfo: info = PaddingInfo(padding_left, content_size)
        let info = crate::tags::PaddingInfo::new(padding_left, content_size);
        // Calculate new_padding = info.get_padding_with(padding_func)
        let new_padding = info.get_padding_with(padding_func);

        // Set the new comment packet with proper padding - matches packets[0] = vcomment_data + b"\x00" * new_padding
        let mut new_packets = packets;
        new_packets[0] = vcomment_data;
        // Negative padding indicates the content exceeds the available space;
        // clamp to zero rather than silently discarding data.
        let padding_bytes = if new_padding < 0 {
            0usize
        } else {
            usize::try_from(new_padding).unwrap_or(0)
        };
        if padding_bytes > 0 {
            new_packets[0].extend_from_slice(&vec![0u8; padding_bytes]);
        }

        // Create new pages using _from_packets_try_preserve
        let new_pages = OggPage::from_packets_try_preserve(new_packets.clone(), &old_pages);

        let final_pages = if new_pages.is_empty() {
            // Fallback to regular from_packets - preserve original granule position
            let first_sequence = old_pages[0].sequence;

            let raw_position = old_pages
                .last()
                .ok_or_else(|| AudexError::InvalidData("no comment pages found".to_string()))?
                .position;
            // Per the Ogg spec, granule position -1 means "no granule position set"
            // for this page. Use 0 for comment header pages in this case.
            let original_granule = if raw_position < 0 {
                0u64
            } else {
                raw_position as u64
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

        OggPage::replace(fileobj, &old_pages, final_pages)?;

        Ok(())
    }
}

// Delegate VCommentDict methods to inner
impl std::ops::Deref for OggVCommentDict {
    type Target = VCommentDict;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for OggVCommentDict {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

// Implement Tags trait by delegating to inner VCommentDict
use crate::Tags;
impl Tags for OggVCommentDict {
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

/// Main structure representing an Ogg Vorbis audio file.
///
/// This is the primary interface for reading and writing Ogg Vorbis files,
/// providing access to both audio stream information and Vorbis Comment metadata.
///
/// # Structure
///
/// - **`info`**: Optional audio stream information (sample rate, channels, bitrate, duration)
/// - **`tags`**: Optional Vorbis Comments for metadata (title, artist, album, etc.)
/// - **`filename`**: Internal file path used for save operations
///
/// # File Format
///
/// Ogg Vorbis uses the Ogg container format with Vorbis-encoded audio:
/// - **Extension**: `.ogg` (standard), `.oga` (audio-specific)
/// - **MIME type**: `audio/ogg`, `audio/vorbis`
/// - **Codec**: Vorbis (lossy compression with VBR support)
///
/// # Examples
///
/// ## Loading and reading file information
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::FileType;
///
/// let vorbis = OggVorbis::load("song.ogg").unwrap();
///
/// // Access stream information
/// if let Some(ref info) = vorbis.info {
///     println!("Sample rate: {} Hz", info.sample_rate);
///     println!("Channels: {}", info.channels);
///
///     if let Some(bitrate) = info.bitrate {
///         println!("Bitrate: {} kbps", bitrate / 1000);
///     }
/// }
/// ```
///
/// ## Reading and modifying tags
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::{FileType, Tags};
///
/// let mut vorbis = OggVorbis::load("song.ogg").unwrap();
///
/// // Read existing tags using the Tags trait
/// if let Some(ref tags) = vorbis.tags {
///     if let Some(title) = tags.get("TITLE") {
///         println!("Current title: {}", title[0]);
///     }
/// }
///
/// // Modify tags using set for Vorbis Comments
/// if let Some(ref mut tags) = vorbis.tags {
///     tags.set("TITLE", vec!["New Title".to_string()]);
///     tags.set("ARTIST", vec!["Artist Name".to_string()]);
///     tags.set("ALBUM", vec!["Album Name".to_string()]);
///     tags.set("DATE", vec!["2024".to_string()]);
/// }
///
/// // Save changes back to file
/// vorbis.save().unwrap();
/// ```
///
/// ## Creating tags if they don't exist
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::FileType;
///
/// let mut vorbis = OggVorbis::load("song.ogg").unwrap();
///
/// // Add tags if file has none
/// if vorbis.tags.is_none() {
///     vorbis.add_tags().unwrap();
///  }
///
/// if let Some(ref mut tags) = vorbis.tags {
///     tags.set("TITLE", vec!["New Song".to_string()]);
/// }
///
/// vorbis.save().unwrap();
/// ```
///
/// ## Working with multiple tag values
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::FileType;
///
/// let mut vorbis = OggVorbis::load("song.ogg").unwrap();
///
/// if let Some(ref mut tags) = vorbis.tags {
///     // Add multiple artists (featured artists) using set
///     tags.set("ARTIST", vec![
///         "Primary Artist".to_string(),
///         "Featured Artist".to_string(),
///     ]);
///
///     // Add multiple genres
///     tags.set("GENRE", vec![
///         "Rock".to_string(),
///         "Alternative".to_string(),
///     ]);
/// }
///
/// vorbis.save().unwrap();
/// ```
///
/// ## Removing all metadata
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::FileType;
///
/// let mut vorbis = OggVorbis::load("song.ogg").unwrap();
///
/// // Clear all tags
/// vorbis.clear().unwrap();
///
/// println!("All metadata removed");
/// ```
#[derive(Debug, Default)]
pub struct OggVorbis {
    /// Stream information
    pub info: Option<OggVorbisInfo>,
    /// Vorbis comments embedded in Ogg bitstream
    pub tags: Option<OggVCommentDict>,
    /// File path for saving operations
    filename: Option<PathBuf>,
}

/// Audio stream information for Ogg Vorbis files.
///
/// Contains technical details about the Vorbis audio stream extracted from
/// the identification header and calculated from the Ogg bitstream structure.
///
/// # Fields
///
/// - **`length`**: Total duration of the audio file
/// - **`bitrate`**: Average bitrate in bits per second (calculated from file size and duration)
/// - **`sample_rate`**: Audio sample rate in Hz (typically 44100 or 48000)
/// - **`channels`**: Number of audio channels (1=mono, 2=stereo, etc.)
/// - **`serial`**: Ogg logical bitstream serial number (unique stream identifier)
/// - **`version`**: Vorbis encoder version (should be 0 for standard Vorbis)
/// - **`max_bitrate`**: Maximum instantaneous bitrate hint (optional, encoder-specific)
/// - **`nominal_bitrate`**: Target/nominal bitrate hint (optional, encoder-specific)
/// - **`min_bitrate`**: Minimum instantaneous bitrate hint (optional, encoder-specific)
///
/// # Bitrate Information
///
/// Vorbis uses variable bitrate (VBR) encoding by default. The bitrate fields provide hints:
/// - **bitrate**: Actual average calculated from file
/// - **nominal_bitrate**: Encoder's target quality setting
/// - **max_bitrate/min_bitrate**: Bitrate constraints (often unused)
///
/// # Examples
///
/// ## Reading stream information
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::FileType;
///
/// let vorbis = OggVorbis::load("song.ogg").unwrap();
///
/// if let Some(ref info) = vorbis.info {
///     println!("Audio Format Information:");
///     println!("  Sample rate: {} Hz", info.sample_rate);
///     println!("  Channels: {}", info.channels);
///     println!("  Vorbis version: {}", info.version);
///
///     if let Some(length) = info.length {
///         let secs = length.as_secs();
///         println!("  Duration: {}:{:02}", secs / 60, secs % 60);
///     }
///
///     if let Some(bitrate) = info.bitrate {
///         println!("  Average bitrate: {} kbps", bitrate / 1000);
///     }
///
///     if let Some(nominal) = info.nominal_bitrate {
///         println!("  Nominal bitrate: {} kbps", nominal / 1000);
///     }
/// }
/// ```
///
/// ## Checking audio quality settings
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::FileType;
///
/// let vorbis = OggVorbis::load("song.ogg").unwrap();
///
/// if let Some(ref info) = vorbis.info {
///     // Determine quality level based on bitrate
///     if let Some(nominal) = info.nominal_bitrate {
///         let quality = match nominal / 1000 {
///             0..=96 => "Low quality",
///             97..=128 => "Standard quality",
///             129..=192 => "High quality",
///             _ => "Very high quality",
///         };
///         println!("Encoding quality: {}", quality);
///     }
///
///     // Check if VBR constraints are set
///     if info.max_bitrate.is_some() || info.min_bitrate.is_some() {
///         println!("Bitrate constraints enabled");
///     }
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct OggVorbisInfo {
    pub length: Option<Duration>,
    pub bitrate: Option<u32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub serial: u32,
    pub version: u32,
    pub max_bitrate: Option<u32>,
    pub nominal_bitrate: Option<u32>,
    pub min_bitrate: Option<u32>,
}

impl StreamInfo for OggVorbisInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        self.bitrate
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
        None // Vorbis is lossy
    }
}

impl OggVorbisInfo {
    /// Parse Vorbis identification header
    pub fn from_identification_header(packet: &[u8]) -> Result<Self> {
        if packet.len() < 30 {
            return Err(AudexError::InvalidData(
                "Vorbis identification header too short".to_string(),
            ));
        }

        // Check packet type and signature
        if packet[0] != 1 || &packet[1..7] != b"vorbis" {
            return Err(AudexError::InvalidData(
                "Invalid Vorbis identification header".to_string(),
            ));
        }

        let mut cursor = Cursor::new(&packet[7..]);

        let version = cursor.read_u32::<LittleEndian>()?;
        let channels = cursor.read_u8()? as u16;
        let sample_rate = cursor.read_u32::<LittleEndian>()?;
        let max_bitrate = cursor.read_u32::<LittleEndian>()?;
        let nominal_bitrate = cursor.read_u32::<LittleEndian>()?;
        let min_bitrate = cursor.read_u32::<LittleEndian>()?;

        if sample_rate == 0 {
            return Err(OggVorbisHeaderError::InvalidHeader(
                "sample rate can't be zero".to_string(),
            )
            .into());
        }

        if channels == 0 {
            return Err(AudexError::InvalidData(
                "Channel count cannot be zero".to_string(),
            ));
        }

        // Convert negative bitrates (0xFFFFFFFF in u32) to 0 per specification
        let max_bitrate = if max_bitrate == 0xFFFFFFFF {
            0
        } else {
            max_bitrate
        };
        let nominal_bitrate = if nominal_bitrate == 0xFFFFFFFF {
            0
        } else {
            nominal_bitrate
        };
        let min_bitrate = if min_bitrate == 0xFFFFFFFF {
            0
        } else {
            min_bitrate
        };

        // Calculate effective bitrate following standard logic.
        // Return None when no meaningful bitrate information is available
        // (all three fields are zero).
        let bitrate = if nominal_bitrate == 0 {
            // If nominal is 0, use average of max and min
            let avg = ((max_bitrate as u64 + min_bitrate as u64) / 2) as u32;
            if avg > 0 { Some(avg) } else { None }
        } else if max_bitrate > 0 && max_bitrate < nominal_bitrate {
            // If max bitrate exists and is less than nominal, use max
            Some(max_bitrate)
        } else if min_bitrate > nominal_bitrate {
            // If min bitrate is greater than nominal, use min
            Some(min_bitrate)
        } else {
            // Use nominal bitrate
            Some(nominal_bitrate)
        };

        // Convert for storage (back to Option<u32>)
        let max_bitrate_opt = if max_bitrate > 0 {
            Some(max_bitrate)
        } else {
            None
        };
        let nominal_bitrate_opt = if nominal_bitrate > 0 {
            Some(nominal_bitrate)
        } else {
            None
        };
        let min_bitrate_opt = if min_bitrate > 0 {
            Some(min_bitrate)
        } else {
            None
        };

        Ok(Self {
            length: None, // Will be calculated later
            bitrate,
            sample_rate,
            channels,
            serial: 0, // Will be set by caller
            version,
            max_bitrate: max_bitrate_opt,
            nominal_bitrate: nominal_bitrate_opt,
            min_bitrate: min_bitrate_opt,
        })
    }

    /// Calculate duration from position
    pub fn set_length(&mut self, position: i64) {
        if self.sample_rate > 0 && position > 0 {
            let duration_secs = position as f64 / self.sample_rate as f64;
            if duration_secs.is_finite() && duration_secs <= u64::MAX as f64 {
                self.length = Some(Duration::from_secs_f64(duration_secs));
            }
        } else {
            self.length = None;
        }
    }

    /// Pretty print format
    pub fn pprint(&self) -> String {
        let duration = self
            .length
            .map(|d| format!("{:.2}", d.as_secs_f64()))
            .unwrap_or_else(|| "0.00".to_string());
        let bitrate = self.bitrate.unwrap_or(0);

        format!("Ogg Vorbis, {} seconds, {} bps", duration, bitrate)
    }

    /// Calculate length from last page
    pub fn post_tags<R: Read + Seek>(&mut self, fileobj: &mut R) -> Result<()> {
        let last_page = OggPage::find_last(fileobj, self.serial, true)?
            .ok_or_else(|| AudexError::InvalidData("could not find last page".to_string()))?;
        if last_page.position > 0 {
            let length_secs = last_page.position as f64 / self.sample_rate as f64;
            if length_secs.is_finite() && length_secs >= 0.0 && length_secs <= u64::MAX as f64 {
                self.length = Some(Duration::from_secs_f64(length_secs));
            }
        }
        Ok(())
    }
}

impl FileType for OggVorbis {
    type Tags = OggVCommentDict;
    type Info = OggVorbisInfo;

    fn format_id() -> &'static str {
        "OggVorbis"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        use std::fs::File;
        use std::io::BufReader;

        debug_event!("parsing OGG Vorbis file");
        let path_buf = path.as_ref().to_path_buf();
        let file = File::open(&path_buf)?;
        let mut reader = BufReader::new(file);

        // Parse Ogg file following standard initialization process
        reader.seek(std::io::SeekFrom::Start(0))?;

        // Find first page with packets
        let mut page = OggPage::from_reader(&mut reader)?;
        if page.packets.is_empty() {
            return Err(
                OggVorbisHeaderError::InvalidHeader("page has not packets".to_string()).into(),
            );
        }

        // Find Vorbis identification header
        let mut pages_read = 1usize;
        while page.packets.is_empty() || !page.packets[0].starts_with(b"\x01vorbis") {
            page = OggPage::from_reader(&mut reader)?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for identification header".to_string(),
                ));
            }
        }

        if !page.is_first() {
            return Err(OggVorbisHeaderError::InvalidHeader(
                "page has ID header, but doesn't start a stream".to_string(),
            )
            .into());
        }

        if page.packets[0].len() < 28 {
            return Err(OggVorbisHeaderError::InvalidHeader(
                "page contains a packet too short to be valid".to_string(),
            )
            .into());
        }

        let mut info = OggVorbisInfo::from_identification_header(&page.packets[0])?;
        info.serial = page.serial;

        // Parse comment header using OggVCommentDict
        // Note: Don't seek back to start - continue from current position after identification header
        let tags = OggVCommentDict::from_fileobj(&mut reader, &info)?;
        debug_event!(tag_count = tags.keys().len(), "OGG Vorbis tags loaded");

        // Calculate length using post_tags method
        info.post_tags(&mut reader)?;

        Ok(Self {
            info: Some(info),
            tags: Some(tags),
            filename: Some(path_buf),
        })
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing OGG Vorbis file from reader");
        let mut reader = reader;
        reader.seek(std::io::SeekFrom::Start(0))?;

        // Find first page with packets
        let mut page = OggPage::from_reader(&mut reader)?;
        if page.packets.is_empty() {
            return Err(
                OggVorbisHeaderError::InvalidHeader("page has not packets".to_string()).into(),
            );
        }

        // Find Vorbis identification header
        let mut pages_read = 1usize;
        while page.packets.is_empty() || !page.packets[0].starts_with(b"\x01vorbis") {
            page = OggPage::from_reader(&mut reader)?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for identification header".to_string(),
                ));
            }
        }

        if !page.is_first() {
            return Err(OggVorbisHeaderError::InvalidHeader(
                "page has ID header, but doesn't start a stream".to_string(),
            )
            .into());
        }

        if page.packets[0].len() < 28 {
            return Err(OggVorbisHeaderError::InvalidHeader(
                "page contains a packet too short to be valid".to_string(),
            )
            .into());
        }

        let mut info = OggVorbisInfo::from_identification_header(&page.packets[0])?;
        info.serial = page.serial;

        // Parse comment header using OggVCommentDict
        let tags = OggVCommentDict::from_fileobj(&mut reader, &info)?;

        // Calculate length using post_tags method
        info.post_tags(&mut reader)?;

        Ok(Self {
            info: Some(info),
            tags: Some(tags),
            filename: None,
        })
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving OGG Vorbis metadata");
        if let Some(path) = self.filename.clone() {
            self.save_with_options(Some(path), None)
        } else {
            warn_event!("no filename available for OGG Vorbis save");
            Err(AudexError::InvalidData(
                "No filename available for saving".to_string(),
            ))
        }
    }

    fn clear(&mut self) -> Result<()> {
        // Preserve the previous tags so we can restore them if the save fails.
        // Without this, a failed save would leave in-memory tags wiped while the
        // file remains unchanged, putting the object in an inconsistent state.
        let prev_tags = self.tags.take();

        let mut empty = OggVCommentDict::new();
        empty.set_vendor(String::new());
        self.tags = Some(empty);

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
        let buf = crate::util::read_all_from_writer_limited(writer, "in-memory Ogg Vorbis save")?;
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
        let mut empty = OggVCommentDict::new();
        empty.set_vendor(String::new());
        self.tags = Some(empty);
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
    /// use audex::oggvorbis::OggVorbis;
    /// use audex::FileType;
    ///
    /// let mut vorbis = OggVorbis::load("song.ogg")?;
    /// if vorbis.tags.is_none() {
    ///     vorbis.add_tags()?;
    /// }
    /// vorbis.set("title", vec!["My Song".to_string()])?;
    /// vorbis.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(OggVCommentDict::new());
        Ok(())
    }

    fn tags(&self) -> Option<&Self::Tags> {
        self.tags.as_ref()
    }

    fn tags_mut(&mut self) -> Option<&mut Self::Tags> {
        self.tags.as_mut()
    }

    fn info(&self) -> &Self::Info {
        // Return a static default when info is absent, rather than panicking.
        // This can happen if the struct was default-constructed or after a
        // partial parse that did not populate the info field.
        static DEFAULT_INFO: OggVorbisInfo = OggVorbisInfo {
            length: None,
            bitrate: None,
            sample_rate: 0,
            channels: 0,
            serial: 0,
            version: 0,
            max_bitrate: None,
            nominal_bitrate: None,
            min_bitrate: None,
        };
        self.info.as_ref().unwrap_or(&DEFAULT_INFO)
    }

    fn score(_filename: &str, header: &[u8]) -> i32 {
        // Return 1 if both conditions are met, 0 otherwise
        let has_ogg_signature = header.len() >= 4 && &header[0..4] == b"OggS";
        let has_vorbis_marker =
            header.len() >= 7 && header.windows(7).any(|window| window == b"\x01vorbis");

        if has_ogg_signature && has_vorbis_marker {
            1
        } else {
            0
        }
    }

    fn mime_types() -> &'static [&'static str] {
        &[
            "audio/ogg",
            "audio/vorbis",
            "audio/x-vorbis",
            "application/ogg",
        ]
    }
}

impl OggVorbis {
    /// Create new OggVorbis instance from a file
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load(path)
    }

    /// Save with advanced options
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
            Some(p) => (p.as_ref().to_path_buf(), true),
            None => (
                self.filename.clone().ok_or_else(|| {
                    AudexError::InvalidData("No filename available for saving".to_string())
                })?,
                false,
            ),
        };

        // Get tags or return error if no tags to save
        let tags = self
            .tags
            .as_ref()
            .ok_or_else(|| AudexError::InvalidData("No tags available for saving".to_string()))?;

        let mut file = OpenOptions::new().read(true).write(true).open(&file_path)?;
        tags.inject(&mut file, padding_func)?;

        // Update stored filename if a new path was provided
        if is_new_path {
            self.filename = Some(file_path);
        }

        Ok(())
    }

    /// Error type for OggVorbis
    pub const ERROR: &'static str = "OggVorbisHeaderError";

    /// MIME types supported
    pub const MIMES: &'static [&'static str] = &["audio/vorbis", "audio/x-vorbis"];

    /// Static method to score file format match
    pub fn score_static(_filename: &str, _fileobj: &mut dyn Read, header: &[u8]) -> i32 {
        // Return 1 if both conditions are met, 0 otherwise
        let has_ogg_signature = header.len() >= 4 && &header[0..4] == b"OggS";
        let has_vorbis_marker =
            header.len() >= 7 && header.windows(7).any(|window| window == b"\x01vorbis");

        if has_ogg_signature && has_vorbis_marker {
            1
        } else {
            0
        }
    }

    /// Add tags if none exist
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(OggVCommentDict::new());
        Ok(())
    }
}

/// Standalone function for deleting tags from a file
///
/// # Arguments
/// * `path` - Path to the file
///
/// # Errors
/// Returns `Err` if the file cannot be read, parsed, or written to.
///
/// # Example
/// ```no_run
/// use audex::oggvorbis;
/// oggvorbis::clear("/path/to/file.ogg").expect("Failed to clear tags");
/// ```
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut vorbis = OggVorbis::load(path)?;
    vorbis.clear()
}

/// Helper method for creating OggVCommentDict from VCommentDict
impl OggVCommentDict {
    /// Create from inner VCommentDict
    ///
    /// This method allows creating an OggVCommentDict from a raw VCommentDict,
    /// which is useful for async operations that parse tags separately.
    pub fn from_inner(inner: VCommentDict) -> Self {
        let mut result = Self::new();
        // Copy all tags from inner
        for key in inner.keys() {
            if let Some(values) = inner.get(&key) {
                result.set(&key, values.to_vec());
            }
        }
        result.set_vendor(inner.vendor().to_string());
        result
    }
}

#[cfg(feature = "async")]
impl OggVorbis {
    /// Load Ogg Vorbis file asynchronously
    ///
    /// Reads and parses an Ogg Vorbis file from disk asynchronously.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the Ogg Vorbis file
    ///
    /// # Returns
    ///
    /// A new `OggVorbis` instance with parsed info and tags
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = TokioFile::open(&path_buf).await?;
        let mut reader = TokioBufReader::new(file);

        // Parse Ogg file following standard initialization process
        reader.seek(SeekFrom::Start(0)).await?;

        // Find first page with packets
        let mut page = OggPage::from_reader_async(&mut reader).await?;
        if page.packets.is_empty() {
            return Err(
                OggVorbisHeaderError::InvalidHeader("page has no packets".to_string()).into(),
            );
        }

        // Find Vorbis identification header
        let mut pages_read = 1usize;
        while page.packets.is_empty() || !page.packets[0].starts_with(b"\x01vorbis") {
            page = OggPage::from_reader_async(&mut reader).await?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for identification header".to_string(),
                ));
            }
        }

        if !page.is_first() {
            return Err(OggVorbisHeaderError::InvalidHeader(
                "page has ID header, but doesn't start a stream".to_string(),
            )
            .into());
        }

        if page.packets[0].len() < 28 {
            return Err(OggVorbisHeaderError::InvalidHeader(
                "page contains a packet too short to be valid".to_string(),
            )
            .into());
        }

        let mut info = OggVorbisInfo::from_identification_header(&page.packets[0])?;
        info.serial = page.serial;

        // Parse comment header using async method
        let tags = Self::parse_tags_async(&mut reader, &info).await?;

        // Calculate length using async post_tags method
        Self::post_tags_async(&mut reader, &mut info).await?;

        Ok(Self {
            info: Some(info),
            tags: Some(tags),
            filename: Some(path_buf),
        })
    }

    /// Parse tags from async reader
    async fn parse_tags_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        info: &OggVorbisInfo,
    ) -> Result<OggVCommentDict> {
        let mut pages = Vec::new();
        let mut complete = false;
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();

        // Read pages until we have complete Vorbis comment packet
        let mut pages_read = 0usize;
        while !complete {
            let page = OggPage::from_reader_async(reader).await?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for comment packet".to_string(),
                ));
            }
            if page.serial == info.serial {
                OggPage::accumulate_page_bytes_with_limit(
                    limits,
                    &mut cumulative_bytes,
                    &page,
                    "OGG Vorbis comment packet",
                )?;
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

        // Parse with Replace mode to handle encoding issues gracefully
        let inner =
            VCommentDict::from_bytes_with_options(data, crate::vorbis::ErrorMode::Replace, true)?;

        Ok(OggVCommentDict::from_inner(inner))
    }

    /// Calculate length from last page (async version)
    async fn post_tags_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        info: &mut OggVorbisInfo,
    ) -> Result<()> {
        let last_page = OggPage::find_last_async(reader, info.serial, true)
            .await?
            .ok_or_else(|| AudexError::InvalidData("could not find last page".to_string()))?;
        if last_page.position > 0 {
            let length_secs = last_page.position as f64 / info.sample_rate as f64;
            if length_secs.is_finite() && length_secs >= 0.0 && length_secs <= u64::MAX as f64 {
                info.length = Some(Duration::from_secs_f64(length_secs));
            }
        }
        Ok(())
    }

    /// Save Ogg Vorbis file asynchronously
    ///
    /// Saves the current tags back to the file.
    pub async fn save_async(&mut self) -> Result<()> {
        if let Some(path) = self.filename.clone() {
            self.save_with_options_async(Some(path), None).await
        } else {
            Err(AudexError::InvalidData(
                "No filename available for saving".to_string(),
            ))
        }
    }

    /// Save with advanced options asynchronously
    ///
    /// # Arguments
    ///
    /// * `path` - Optional path to save to (uses stored filename if None)
    /// * `padding_func` - Optional function to calculate padding size
    pub async fn save_with_options_async<P>(
        &mut self,
        path: Option<P>,
        padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let (file_path, is_new_path) = match &path {
            Some(p) => (p.as_ref().to_path_buf(), true),
            None => (
                self.filename.clone().ok_or_else(|| {
                    AudexError::InvalidData("No filename available for saving".to_string())
                })?,
                false,
            ),
        };

        // Get tags or return error if no tags to save
        let tags = self
            .tags
            .as_ref()
            .ok_or_else(|| AudexError::InvalidData("No tags available for saving".to_string()))?;

        // Open the file for reading and writing
        let mut file = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)
            .await?;

        // Use inject method to write tags
        Self::inject_tags_async(&mut file, tags, padding_func).await?;

        // Update stored filename if a new path was provided
        if is_new_path {
            self.filename = Some(file_path);
        }

        Ok(())
    }

    /// Inject tags into the file asynchronously
    async fn inject_tags_async(
        fileobj: &mut TokioFile,
        tags: &OggVCommentDict,
        padding_func: Option<fn(&crate::tags::PaddingInfo) -> i64>,
    ) -> Result<()> {
        // Find the old Vorbis comment pages in the file
        fileobj.seek(SeekFrom::Start(0)).await?;

        // Find the page containing the Vorbis comment header
        let mut page = OggPage::from_reader_async(fileobj).await?;
        let mut pages_read = 1usize;
        while page.packets.is_empty() || !page.packets[0].starts_with(b"\x03vorbis") {
            page = OggPage::from_reader_async(fileobj).await?;
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                return Err(AudexError::InvalidData(
                    "Too many OGG pages while searching for comment header".to_string(),
                ));
            }
        }

        let mut old_pages = vec![page];

        // Collect all pages belonging to the comment packet
        loop {
            let last_page = old_pages.last().ok_or_else(|| {
                AudexError::InvalidData(
                    "expected non-empty page list while reading comments".into(),
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
            let file_size = fileobj.seek(SeekFrom::End(0)).await?;
            fileobj.seek(SeekFrom::Start(old_pos)).await?;
            // Use saturating subtraction to prevent overflow on large or crafted values
            i64::try_from(file_size)
                .unwrap_or(i64::MAX)
                .saturating_sub(i64::try_from(packets[0].len()).unwrap_or(0))
        };

        // Create new Vorbis comment data
        let vcomment_data = {
            let mut data = b"\x03vorbis".to_vec();
            let mut vcomment_bytes = Vec::new();

            let mut comment_to_write = VCommentDict::clone(&**tags);
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
        // Negative padding indicates the content exceeds the available space;
        // clamp to zero rather than silently discarding data.
        let padding_bytes = if new_padding < 0 {
            0usize
        } else {
            usize::try_from(new_padding).unwrap_or(0)
        };
        if padding_bytes > 0 {
            new_packets[0].extend_from_slice(&vec![0u8; padding_bytes]);
        }

        // Create new Ogg pages, preserving layout if possible
        let new_pages = OggPage::from_packets_try_preserve(new_packets.clone(), &old_pages);

        let final_pages = if new_pages.is_empty() {
            let first_sequence = old_pages[0].sequence;
            let raw_position = old_pages
                .last()
                .ok_or_else(|| AudexError::InvalidData("no comment pages found".to_string()))?
                .position;
            // Per the Ogg spec, granule position -1 means "no granule position set"
            // for this page. Use 0 for comment header pages in this case.
            let original_granule = if raw_position < 0 {
                0u64
            } else {
                raw_position as u64
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

        // Replace old pages with new ones
        OggPage::replace_async(fileobj, &old_pages, final_pages).await?;

        Ok(())
    }

    /// Clear tags asynchronously
    ///
    /// Removes all tags from the file and saves the changes.
    pub async fn clear_async(&mut self) -> Result<()> {
        let mut empty = OggVCommentDict::new();
        empty.set_vendor(String::new());
        self.tags = Some(empty);
        self.save_async().await
    }

    /// Delete file tags asynchronously (standalone operation)
    ///
    /// Loads, clears, and saves tags in one operation.
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        let mut vorbis = Self::load_async(path).await?;
        vorbis.clear_async().await
    }
}

/// Standalone async function for clearing tags from a file
///
/// # Arguments
/// * `path` - Path to the file
///
/// # Example
/// ```no_run
/// use audex::oggvorbis;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     oggvorbis::clear_async("/path/to/file.ogg").await?;
///     Ok(())
/// }
/// ```
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<Path>>(path: P) -> Result<()> {
    OggVorbis::delete_async(path).await
}
