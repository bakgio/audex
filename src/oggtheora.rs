//! Support for Ogg Theora video files.
//!
//! This module provides support for Ogg Theora files, a free and open video codec
//! developed by the Xiph.Org Foundation. While primarily a video format, Theora files
//! are included in this audio metadata library because they use the same Ogg container
//! and Vorbis Comment tagging system as Ogg audio formats.
//!
//! **Note**: Theora is a **video codec**, not an audio codec. This module provides
//! metadata and container support but does not decode video frames.
//!
//! # File Format
//!
//! Ogg Theora files consist of:
//! - **Ogg container**: Flexible bitstream container supporting multiplexing
//! - **Theora codec**: Lossy video compression based on VP3
//! - **Vorbis Comments**: Standard Ogg metadata tagging format
//!
//! ## Structure
//!
//! An Ogg Theora file contains:
//!
//! 1. **Identification Header**: Contains video parameters (resolution, framerate, colorspace)
//! 2. **Comment Header**: Vorbis Comment metadata (TITLE, ARTIST, etc.)
//! 3. **Setup Header**: Codec configuration data
//! 4. **Video Data**: Compressed Theora video frames
//!
//! # Video Characteristics
//!
//! - **Codec Type**: Lossy video compression (based on VP3)
//! - **Resolution**: Arbitrary video dimensions
//! - **Frame Rates**: Variable, typically 24-60 fps
//! - **Colorspace**: YUV 4:2:0, 4:2:2, or 4:4:4
//! - **Quality**: Variable bitrate and quality settings
//! - **File Extension**: `.ogv` or `.ogg`
//! - **MIME Type**: `video/ogg`, `video/x-theora`
//!
//! ## Use Cases
//!
//! - **Web video**: HTML5 `<video>` element support
//! - **Open source video**: Patent-free video codec
//! - **Archival**: Long-term preservation with open standards
//! - **Multimedia containers**: Often multiplexed with Vorbis or Opus audio
//!
//! # Tagging
//!
//! Ogg Theora uses Vorbis Comments for metadata:
//!
//! - **Multi-value fields**: Multiple values per tag
//! - **UTF-8 encoding**: Full Unicode support
//! - **Standard fields**: TITLE, ARTIST, ALBUM, DATE, DESCRIPTION, etc.
//! - **Case-insensitive keys**: Normalized to lowercase
//! - **Video-specific tags**: ENCODER, COPYRIGHT, LICENSE, etc.
//!
//! # Basic Usage
//!
//! ## Loading and Reading Information
//!
//! ```no_run
//! use audex::oggtheora::OggTheora;
//! use audex::FileType;
//!
//! # fn main() -> audex::Result<()> {
//! let theora = OggTheora::load("video.ogv")?;
//!
//! // Access video information
//! println!("Resolution: {}x{}", theora.info.width, theora.info.height);
//! println!("Frame Rate: {} fps", theora.info.fps);
//! println!("Bitrate: {} bps", theora.info.bitrate);
//!
//! if let Some(duration) = theora.info.length {
//!     println!("Duration: {:.2} seconds", duration.as_secs_f64());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Working with Tags
//!
//! ```no_run
//! use audex::oggtheora::OggTheora;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut theora = OggTheora::load("video.ogv")?;
//!
//! if let Some(tags) = theora.tags_mut() {
//!     tags.set_single("TITLE", "Video Title".to_string());
//!     tags.set_single("ARTIST", "Creator Name".to_string());
//!     tags.set_single("DATE", "2024".to_string());
//!     tags.set_single("DESCRIPTION", "Video description".to_string());
//! }
//!
//! theora.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Why Include Video in an Audio Library?
//!
//! Ogg Theora is included because:
//! 1. **Shared container**: Uses the same Ogg container as audio formats
//! 2. **Compatible metadata**: Uses Vorbis Comments like Ogg Vorbis/Opus
//! 3. **Multimedia files**: Theora files often contain audio streams
//! 4. **Unified interface**: Consistent API across Ogg-based formats
//!
//! # References
//!
//! - [Theora Specification](https://www.theora.org/doc/Theora.pdf)
//! - [Xiph.Org Foundation](https://xiph.org/)
//! - [Vorbis Comment Specification](https://www.xiph.org/vorbis/doc/v-comment.html)

use crate::VERSION_STRING;
use crate::ogg::OggPage;
use crate::vorbis::VCommentDict;
use crate::{AudexError, FileType, Result, StreamInfo};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::{Cursor, Seek};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use std::io::SeekFrom;
#[cfg(feature = "async")]
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};
#[cfg(feature = "async")]
use tokio::io::{AsyncSeekExt, BufReader as TokioBufReader};

/// Vorbis Comment metadata container for Ogg Theora files.
///
/// Wraps [`VCommentDict`] to provide metadata tagging for Theora video files using
/// the standard Vorbis Comment format shared across all Ogg-based formats.
///
/// Common video-specific tags include TITLE, ARTIST (creator), DATE, DESCRIPTION,
/// COPYRIGHT, LICENSE, and ENCODER.
#[derive(Debug, Default)]
pub struct TheoraTags {
    /// The underlying Vorbis Comment dictionary
    pub inner: VCommentDict,
    /// Ogg stream serial number
    pub serial: u32,
}

impl std::ops::Deref for TheoraTags {
    type Target = VCommentDict;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for TheoraTags {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl crate::Tags for TheoraTags {
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
        format!("TheoraTags({})", self.inner.keys().len())
    }

    fn module_name(&self) -> &'static str {
        "oggtheora"
    }
}

/// Represents an Ogg Theora video file with metadata and stream information.
///
/// Provides access to video stream parameters (resolution, framerate, bitrate) and
/// Vorbis Comment metadata. Note that this handles video files within an audio
/// metadata library due to shared Ogg container and tagging formats.
///
/// File extension: `.ogv` or `.ogg`
#[derive(Debug)]
pub struct OggTheora {
    /// Video stream information
    pub info: TheoraInfo,
    /// Optional Vorbis Comment metadata tags
    pub tags: Option<TheoraTags>,
    /// Path to the file (used for saving)
    pub path: Option<std::path::PathBuf>,
}

/// Video stream information for Ogg Theora files.
///
/// Contains technical details about the Theora video stream extracted from the
/// identification header, including resolution, framerate, bitrate, and codec parameters.
///
/// # Key Fields
///
/// - **`width`**, **`height`**: Display resolution (visible area)
/// - **`frame_width`**, **`frame_height`**: Encoded frame dimensions (may be larger)
/// - **`fps`**: Frames per second (calculated from framerate numerator/denominator)
/// - **`bitrate`**: Average bitrate in bits per second
/// - **`length`**: Video duration
/// - **`quality`**: Quality level (0-63, higher is better)
/// - **`colorspace`**: YUV colorspace (0=undefined, 1=Rec. 470M, 2=Rec. 470BG, 3=Rec. 709)
/// - **`aspect_numerator`**, **`aspect_denominator`**: Pixel aspect ratio
#[derive(Debug, Clone)]
pub struct TheoraInfo {
    /// Video duration
    pub length: Option<Duration>,
    /// Frames per second
    pub fps: f64,
    /// Average bitrate in bps
    pub bitrate: u32,
    /// Display width in pixels
    pub width: u32,
    /// Display height in pixels
    pub height: u32,
    /// Ogg stream serial number
    pub serial: u32,
    /// Granule position shift
    pub granule_shift: u8,
    /// Theora version major number
    pub version_major: u8,
    /// Theora version minor number
    pub version_minor: u8,
    /// Encoded frame width
    pub frame_width: u32,
    /// Encoded frame height
    pub frame_height: u32,
    /// Horizontal offset for cropping
    pub offset_x: u32,
    /// Vertical offset for cropping
    pub offset_y: u32,
    /// Pixel aspect ratio numerator
    pub aspect_numerator: u32,
    /// Pixel aspect ratio denominator
    pub aspect_denominator: u32,
    /// Colorspace identifier
    pub colorspace: u8,
    /// Pixel format
    pub pixel_fmt: u8,
    /// Target bitrate in bps
    pub target_bitrate: u32,
    /// Quality level (0-63)
    pub quality: u8,
    /// Keyframe granule shift
    pub keyframe_granule_shift: u8,
}

impl Default for TheoraInfo {
    fn default() -> Self {
        Self {
            length: None,
            fps: 0.0,
            bitrate: 0,
            width: 0,
            height: 0,
            serial: 0,
            granule_shift: 0,
            version_major: 0,
            version_minor: 0,
            frame_width: 0,
            frame_height: 0,
            offset_x: 0,
            offset_y: 0,
            aspect_numerator: 0,
            aspect_denominator: 0,
            colorspace: 0,
            pixel_fmt: 0,
            target_bitrate: 0,
            quality: 0,
            keyframe_granule_shift: 0,
        }
    }
}

impl StreamInfo for TheoraInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        if self.bitrate > 0 {
            Some(self.bitrate)
        } else {
            None
        }
    }

    fn sample_rate(&self) -> Option<u32> {
        None // Video format - no sample rate
    }

    fn channels(&self) -> Option<u16> {
        None // Video format - no channels
    }

    fn bits_per_sample(&self) -> Option<u16> {
        None // Video format - varies
    }
}

impl TheoraInfo {
    /// Parse Theora identification header
    pub fn from_identification_header(packet: &[u8]) -> Result<Self> {
        if packet.len() < 42 {
            return Err(AudexError::InvalidData(
                "Theora identification header too short".to_string(),
            ));
        }

        // Check packet type and signature
        if packet[0] != 0x80 || &packet[1..7] != b"theora" {
            return Err(AudexError::InvalidData(
                "Invalid Theora identification header".to_string(),
            ));
        }

        let mut cursor = Cursor::new(&packet[7..]);

        // Parse header fields
        let version_major = cursor.read_u8()?;
        let version_minor = cursor.read_u8()?;
        let _version_subminor = cursor.read_u8()?;

        // Per the Theora spec, any file with major version 3 and minor
        // version >= 2 should be decodable. Reject only truly incompatible
        // major versions or older minor versions with different semantics.
        if version_major != 3 || version_minor < 2 {
            return Err(AudexError::UnsupportedFormat(format!(
                "Found Theora version {}.{}, expected major 3 with minor >= 2",
                version_major, version_minor
            )));
        }

        // Frame width and height (macroblock aligned)
        let frame_width = (cursor.read_u16::<BigEndian>()? as u32) << 4;
        let frame_height = (cursor.read_u16::<BigEndian>()? as u32) << 4;

        // Picture width and height (actual video dimensions)
        let width = read_u24_be(&mut cursor)?;
        let height = read_u24_be(&mut cursor)?;

        // Picture offset
        let offset_x = cursor.read_u8()? as u32;
        let offset_y = cursor.read_u8()? as u32;

        // Frame rate
        let fps_numerator = cursor.read_u32::<BigEndian>()?;
        let fps_denominator = cursor.read_u32::<BigEndian>()?;

        if fps_denominator == 0 || fps_numerator == 0 {
            return Err(AudexError::InvalidData(
                "Frame rate numerator or denominator is zero".to_string(),
            ));
        }

        let fps = fps_numerator as f64 / fps_denominator as f64;

        // Aspect ratio
        let aspect_numerator = read_u24_be(&mut cursor)?;
        let aspect_denominator = read_u24_be(&mut cursor)?;

        // Colorspace and pixel format
        let colorspace = cursor.read_u8()?;
        let bitrate_bytes = read_u24_be(&mut cursor)?; // Nominal bitrate

        // Quality and keyframe frequency
        let quality_keyframe = cursor.read_u16::<BigEndian>()?;
        let quality = (quality_keyframe >> 10) as u8; // Upper 6 bits
        let keyframe_granule_shift = (quality_keyframe >> 5) as u8 & 0x1F; // Next 5 bits
        let pixel_fmt = ((quality_keyframe >> 3) & 0x03) as u8; // Bits 4-3: pixel format

        // The Theora spec defines valid granule shift values as 1-31.
        // A shift of 0 would produce incorrect bitmask calculations and
        // misleading duration values.
        if keyframe_granule_shift == 0 {
            return Err(crate::AudexError::InvalidData(
                "Theora keyframe granule shift must be 1-31, got 0".to_string(),
            ));
        }

        Ok(Self {
            length: None, // Will be calculated later
            fps,
            bitrate: bitrate_bytes,
            width,
            height,
            serial: 0, // Will be set by caller
            granule_shift: keyframe_granule_shift,
            version_major,
            version_minor,
            frame_width,
            frame_height,
            offset_x,
            offset_y,
            aspect_numerator,
            aspect_denominator,
            colorspace,
            pixel_fmt,
            target_bitrate: bitrate_bytes,
            quality,
            keyframe_granule_shift,
        })
    }

    /// Calculate duration from position using Theora's granule interpretation
    pub fn set_length(&mut self, position: i64) {
        if self.fps > 0.0 && position > 0 && position != -1 {
            let granule_position = position as u64;
            // Theora granule position encoding:
            // Upper bits contain the frame count of the last keyframe
            // Lower bits contain frames since that keyframe
            let mask = (1u64 << self.granule_shift) - 1;
            let keyframe_count = granule_position >> self.granule_shift;
            let frames_since_keyframe = granule_position & mask;
            let total_frames = keyframe_count + frames_since_keyframe;

            let duration_secs = total_frames as f64 / self.fps;
            if duration_secs.is_finite() && duration_secs >= 0.0 && duration_secs <= u64::MAX as f64
            {
                self.length = Some(Duration::from_secs_f64(duration_secs));
            }
        }
    }

    /// Pretty print format
    pub fn pretty_print(&self) -> String {
        let duration = self
            .length
            .map(|d| format!("{:.2}", d.as_secs_f64()))
            .unwrap_or_else(|| "unknown".to_string());

        format!("Ogg Theora, {} seconds, {} bps", duration, self.bitrate)
    }
}

// Helper function for reading 24-bit big endian values
pub fn read_u24_be<R: ReadBytesExt>(reader: &mut R) -> std::io::Result<u32> {
    let mut buf = [0u8; 3];
    reader.read_exact(&mut buf)?;
    Ok(((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | (buf[2] as u32))
}

impl FileType for OggTheora {
    type Tags = TheoraTags;
    type Info = TheoraInfo;

    fn format_id() -> &'static str {
        "OggTheora"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        use std::fs::File;
        use std::io::BufReader;

        debug_event!("parsing OGG Theora file");
        let path_buf = path.as_ref().to_path_buf();
        let file = File::open(&path_buf)?;
        let mut reader = BufReader::new(file);

        // Parse Ogg file
        reader.seek(std::io::SeekFrom::Start(0))?;

        // Find first Theora stream
        let mut theora_info = None;
        let mut theora_serial = None;
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;

        // Parse identification header
        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                break;
            }
            let page = match OggPage::from_reader(&mut reader) {
                Ok(page) => page,
                Err(e) => {
                    if let AudexError::Io(io_err) = &e {
                        if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                            break;
                        }
                    }
                    return Err(e);
                }
            };

            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 7 && first_packet.starts_with(b"\x80theora") {
                    // Found Theora identification header
                    if !page.is_first() {
                        return Err(AudexError::InvalidData(
                            "Theora identification header not on first page".to_string(),
                        ));
                    }

                    let mut info = TheoraInfo::from_identification_header(first_packet)?;
                    info.serial = page.serial;
                    theora_info = Some(info);
                    theora_serial = Some(page.serial);
                    break;
                }
            }
        }

        let mut info = theora_info
            .ok_or_else(|| AudexError::InvalidData("No Theora stream found".to_string()))?;

        let serial = theora_serial
            .ok_or_else(|| AudexError::InvalidData("No Theora serial number found".to_string()))?;

        // Parse comment header
        let mut tags = None;
        let mut found_comment = false;
        let mut comment_pages = Vec::new();
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();
        pages_read = 0;

        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                break;
            }

            let page = match OggPage::from_reader(&mut reader) {
                Ok(page) => page,
                Err(_) => break,
            };

            if page.serial == serial {
                if let Some(first_packet) = page.packets.first() {
                    if first_packet.len() >= 7 && first_packet.starts_with(b"\x81theora") {
                        // Found Theora comment header
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg Theora comment packet",
                        )?;
                        comment_pages.push(page);
                        found_comment = true;
                    } else if found_comment {
                        // Check if this continues the comment header
                        if !comment_pages.last().is_none_or(|p| p.is_complete()) {
                            OggPage::accumulate_page_bytes_with_limit(
                                limits,
                                &mut cumulative_bytes,
                                &page,
                                "Ogg Theora comment packet",
                            )?;
                            comment_pages.push(page);
                        } else {
                            break; // End of comment header
                        }
                    }
                }
            }
        }

        // Process comment header if found
        if !comment_pages.is_empty() {
            let packets = OggPage::to_packets(&comment_pages, false)?;
            if let Some(comment_packet) = packets.first() {
                if comment_packet.len() > 7 {
                    // Skip "\x81theora" prefix
                    let comment_data = &comment_packet[7..];
                    // Ogg Theora comment packets don't have framing bits
                    let vcomment = VCommentDict::from_bytes_with_options(
                        comment_data,
                        crate::vorbis::ErrorMode::Strict,
                        false,
                    )?;
                    tags = Some(TheoraTags {
                        inner: vcomment,
                        serial,
                    });
                }
            }
        }

        // Find last page to calculate duration
        reader.seek(std::io::SeekFrom::Start(0))?;
        let last_page = OggPage::find_last(&mut reader, serial, true)?
            .ok_or_else(|| AudexError::InvalidData("could not find last page".to_string()))?;
        if last_page.position > 0 && last_page.position != -1 {
            info.set_length(last_page.position);
        }

        if let Some(ref _t) = tags {
            debug_event!(tag_count = _t.keys().len(), "OGG Theora tags loaded");
        }

        Ok(Self {
            info,
            tags,
            path: Some(path_buf),
        })
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing OGG Theora file from reader");
        let mut reader = reader;
        reader.seek(std::io::SeekFrom::Start(0))?;

        // Find first Theora stream
        let mut theora_info = None;
        let mut theora_serial = None;
        const MAX_PAGE_SEARCH: usize = 1024;
        let mut pages_read: usize = 0;

        // Parse identification header
        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                break;
            }
            let page = match OggPage::from_reader(&mut reader) {
                Ok(page) => page,
                Err(e) => {
                    if let AudexError::Io(io_err) = &e {
                        if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                            break;
                        }
                    }
                    return Err(e);
                }
            };

            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 7 && first_packet.starts_with(b"\x80theora") {
                    // Found Theora identification header
                    if !page.is_first() {
                        return Err(AudexError::InvalidData(
                            "Theora identification header not on first page".to_string(),
                        ));
                    }

                    let mut info = TheoraInfo::from_identification_header(first_packet)?;
                    info.serial = page.serial;
                    theora_info = Some(info);
                    theora_serial = Some(page.serial);
                    break;
                }
            }
        }

        let mut info = theora_info
            .ok_or_else(|| AudexError::InvalidData("No Theora stream found".to_string()))?;

        let serial = theora_serial
            .ok_or_else(|| AudexError::InvalidData("No Theora serial number found".to_string()))?;

        // Parse comment header
        let mut tags = None;
        let mut found_comment = false;
        let mut comment_pages = Vec::new();
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();
        pages_read = 0;

        loop {
            pages_read += 1;
            if pages_read > MAX_PAGE_SEARCH {
                break;
            }

            let page = match OggPage::from_reader(&mut reader) {
                Ok(page) => page,
                Err(_) => break,
            };

            if page.serial == serial {
                if let Some(first_packet) = page.packets.first() {
                    if first_packet.len() >= 7 && first_packet.starts_with(b"\x81theora") {
                        // Found Theora comment header; enforce cumulative byte budget
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg Theora comment packet",
                        )?;
                        comment_pages.push(page);
                        found_comment = true;
                    } else if found_comment {
                        // Check if this continues the comment header.
                        // Use is_none_or to safely handle an empty vec (treat as complete).
                        if !comment_pages.last().is_none_or(|p| p.is_complete()) {
                            OggPage::accumulate_page_bytes_with_limit(
                                limits,
                                &mut cumulative_bytes,
                                &page,
                                "Ogg Theora comment packet",
                            )?;
                            comment_pages.push(page);
                        } else {
                            break; // End of comment header
                        }
                    }
                }
            }
        }

        // Process comment header if found
        if !comment_pages.is_empty() {
            let packets = OggPage::to_packets(&comment_pages, false)?;
            if let Some(comment_packet) = packets.first() {
                if comment_packet.len() > 7 {
                    // Skip "\x81theora" prefix
                    let comment_data = &comment_packet[7..];
                    // Ogg Theora comment packets don't have framing bits
                    let vcomment = VCommentDict::from_bytes_with_options(
                        comment_data,
                        crate::vorbis::ErrorMode::Strict,
                        false,
                    )?;
                    tags = Some(TheoraTags {
                        inner: vcomment,
                        serial,
                    });
                }
            }
        }

        // Find last page to calculate duration
        reader.seek(std::io::SeekFrom::Start(0))?;
        let last_page = OggPage::find_last(&mut reader, serial, true)?
            .ok_or_else(|| AudexError::InvalidData("could not find last page".to_string()))?;
        if last_page.position > 0 && last_page.position != -1 {
            info.set_length(last_page.position);
        }

        Ok(Self {
            info,
            tags,
            path: None,
        })
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving OGG Theora metadata");
        let path = self.path.as_ref().ok_or_else(|| {
            warn_event!("no file path available for OGG Theora save");
            AudexError::InvalidOperation("No file path available for saving".to_string())
        })?;

        if let Some(ref tags) = self.tags {
            self.inject_tags(path, tags)?;
        }

        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        // Create empty tags with empty vendor string and inject
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        let empty_tags = TheoraTags {
            inner,
            serial: self.info.serial,
        };
        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::InvalidOperation("No file path available for deletion".to_string())
        })?;
        self.inject_tags(path, &empty_tags)?;
        self.tags = Some(empty_tags);

        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        if let Some(ref tags) = self.tags {
            // Read all data into a Cursor which satisfies the Sized + 'static
            // bounds required by inject_theora_tags (and the internal OggPage helpers).
            let data =
                crate::util::read_all_from_writer_limited(writer, "in-memory Ogg Theora save")?;
            let mut cursor = std::io::Cursor::new(data);
            self.inject_theora_tags(&mut cursor, tags)?;
            // Write modified data back to the original writer
            let result = cursor.into_inner();
            writer.seek(std::io::SeekFrom::Start(0))?;
            std::io::Write::write_all(writer, &result)?;
            crate::util::truncate_writer_dyn(writer, result.len() as u64)?;
        }
        Ok(())
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        let empty_tags = TheoraTags {
            inner,
            serial: self.info.serial,
        };
        let data = crate::util::read_all_from_writer_limited(writer, "in-memory Ogg Theora clear")?;
        let mut cursor = std::io::Cursor::new(data);
        self.inject_theora_tags(&mut cursor, &empty_tags)?;
        let result = cursor.into_inner();
        writer.seek(std::io::SeekFrom::Start(0))?;
        std::io::Write::write_all(writer, &result)?;
        crate::util::truncate_writer_dyn(writer, result.len() as u64)?;
        self.tags = Some(empty_tags);
        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        if let Some(ref tags) = self.tags {
            self.inject_tags(path, tags)?;
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
    /// use audex::oggtheora::OggTheora;
    /// use audex::FileType;
    ///
    /// let mut theora = OggTheora::load("video.ogv")?;
    /// if theora.tags.is_none() {
    ///     theora.add_tags()?;
    /// }
    /// theora.set("title", vec!["My Video".to_string()])?;
    /// theora.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(TheoraTags {
            inner: VCommentDict::new(),
            serial: self.info.serial,
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

    fn score(filename: &str, header: &[u8]) -> i32 {
        let mut score = 0;

        // Check for Ogg signature first
        if header.len() >= 4 && &header[0..4] == b"OggS" {
            score += 1;
        } else {
            // For non-OGG headers, check if it's a .ogv file with reasonable content
            let lower_filename = filename.to_lowercase();
            if lower_filename.ends_with(".ogv")
                && !header.is_empty()
                && header.len() >= 4
                && !header.starts_with(b"MP3")
                && !header.starts_with(b"ID3")
            {
                return 1; // OGV files with reasonable content get score 1
            }
            return 0; // Not an Ogg file and not a reasonable OGV file
        }

        // Check for Theora identification or comment headers in the data
        if header.len() >= 11 {
            // Minimum: 4 bytes OggS + 7 bytes for theora header signature
            if header.windows(7).any(|window| window == b"\x80theora") {
                score += 2; // Found identification header
            }
            if header.windows(7).any(|window| window == b"\x81theora") {
                score += 2; // Found comment header
            }
        }

        // Check file extension bonus
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".ogv") {
            score += 2; // OGV extension is strong indicator of Theora video
        } else if lower_filename.ends_with(".ogg") && score > 1 {
            score += 1; // .ogg extension bonus only if we found headers
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["video/x-theora", "video/ogg"]
    }
}

impl OggTheora {
    /// Inject tags into both Theora (video) and Vorbis (audio) comment packets
    fn inject_tags<P: AsRef<Path>>(&self, path: P, tags: &TheoraTags) -> Result<()> {
        use std::fs::OpenOptions;

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.as_ref())?;
        self.inject_theora_tags(&mut file, tags)
    }

    /// Inject tags into Theora video stream comment packet
    fn inject_theora_tags<F: std::io::Read + std::io::Write + std::io::Seek + 'static>(
        &self,
        file: &mut F,
        tags: &TheoraTags,
    ) -> Result<()> {
        use std::io::SeekFrom;

        let serial = self.info.serial;

        // Find Theora comment header pages
        let mut comment_pages = Vec::new();
        let mut found_comment = false;
        let mut cumulative_bytes = 0u64;
        let limits = crate::limits::ParseLimits::default();
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

            if page.serial == serial {
                if let Some(first_packet) = page.packets.first() {
                    if first_packet.len() >= 7 && first_packet.starts_with(b"\x81theora") {
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg Theora comment packet",
                        )?;
                        comment_pages.push(page);
                        found_comment = true;
                    } else if found_comment {
                        if !comment_pages.last().is_none_or(|p| p.is_complete())
                            && comment_pages.last().is_some_and(|p| p.packets.len() <= 1)
                        {
                            OggPage::accumulate_page_bytes_with_limit(
                                limits,
                                &mut cumulative_bytes,
                                &page,
                                "Ogg Theora comment packet",
                            )?;
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
                "No Theora comment header found".to_string(),
            ));
        }

        // Reconstruct comment packets (strict=False )
        let packets = OggPage::to_packets(&comment_pages, false)?;
        if packets.is_empty() {
            return Err(AudexError::InvalidData(
                "Failed to reconstruct comment packet".to_string(),
            ));
        }

        // Calculate content size (file size minus first packet)
        let content_size = {
            let old_pos = file.stream_position()?;
            let file_size = file.seek(SeekFrom::End(0))?;
            file.seek(SeekFrom::Start(old_pos))?; // Restore position
            // Use saturating subtraction to prevent overflow on large or crafted values
            i64::try_from(file_size)
                .unwrap_or(i64::MAX)
                .saturating_sub(i64::try_from(packets[0].len()).unwrap_or(0))
        };

        // Create new comment data
        let vcomment_data = {
            let mut data = b"\x81theora".to_vec();
            let mut vcomment_bytes = Vec::new();

            let mut comment_to_write = tags.inner.clone();
            // Only set Audex vendor string when there are actual tags to write
            if !comment_to_write.keys().is_empty() {
                comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
            }

            // Use framing=false for Theora
            comment_to_write.write(&mut vcomment_bytes, Some(false))?;
            data.extend_from_slice(&vcomment_bytes);
            data
        };

        let padding_left = packets[0].len() as i64 - vcomment_data.len() as i64;

        // Calculate padding using PaddingInfo
        let info = crate::tags::PaddingInfo::new(padding_left, content_size);
        let new_padding = info.get_padding_with(None::<fn(&crate::tags::PaddingInfo) -> i64>); // No padding function for now

        // Set the new comment packet with proper padding
        let mut new_packets = packets;
        new_packets[0] = vcomment_data;
        if new_padding > 0 {
            new_packets[0].extend_from_slice(&vec![0u8; usize::try_from(new_padding).unwrap_or(0)]);
        }

        // Create new pages, preserving original structure where possible
        let new_pages = OggPage::from_packets_try_preserve(new_packets.clone(), &comment_pages);

        // Fall back to regular from_packets if try_preserve failed
        let final_pages = if new_pages.is_empty() {
            let first_sequence = comment_pages[0].sequence;
            // Use 0 as fallback if position is negative (sentinel value) or pages are empty
            let original_granule = comment_pages
                .last()
                .map(|p| {
                    if p.position < 0 {
                        0u64
                    } else {
                        p.position as u64
                    }
                })
                .unwrap_or(0);
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

        // Replace the comment pages in the file
        OggPage::replace(file, &comment_pages, final_pages)?;
        Ok(())
    }

    /// Add tags if none exist
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(TheoraTags {
            inner: VCommentDict::new(),
            serial: self.info.serial,
        });
        Ok(())
    }
}

/// Standalone function for clearing tags from a file
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut theora = OggTheora::load(path)?;
    theora.clear()
}

/// Async implementation block for OggTheora
///
/// Provides asynchronous versions of all file I/O operations for non-blocking
/// usage in async runtimes like Tokio. These methods mirror the synchronous
/// API but use async/await for better concurrency support.
#[cfg(feature = "async")]
impl OggTheora {
    /// Loads an Ogg Theora file asynchronously from the specified path.
    ///
    /// This method performs non-blocking file I/O to read and parse the Theora
    /// stream, extracting both stream information (video dimensions, frame rate,
    /// bitrate) and metadata tags.
    ///
    /// # Arguments
    /// * `path` - The file path to load the Ogg Theora file from
    ///
    /// # Returns
    /// * `Result<Self>` - The parsed OggTheora instance or an error
    ///
    /// # Example
    /// ```rust,no_run
    /// use audex::oggtheora::OggTheora;
    ///
    /// #[tokio::main]
    /// async fn main() -> audex::Result<()> {
    ///     let theora = OggTheora::load_async("video.ogv").await?;
    ///     println!("Duration: {:?}", theora.info.length);
    ///     Ok(())
    /// }
    /// ```
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

    /// Parses Theora stream information asynchronously from the reader.
    ///
    /// This internal method scans Ogg pages to find the Theora identification
    /// header and extracts video stream metadata including dimensions, frame
    /// rate, and codec parameters.
    ///
    /// # Arguments
    /// * `reader` - An async reader positioned at the start of the Ogg stream
    ///
    /// # Returns
    /// * `Result<TheoraInfo>` - The parsed stream information or an error
    async fn parse_info_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
    ) -> Result<TheoraInfo> {
        // Find the Theora stream by looking for the identification packet
        loop {
            let page = match OggPage::from_reader_async(reader).await {
                Ok(page) => page,
                Err(_) => {
                    return Err(AudexError::InvalidData(
                        "No Theora stream found".to_string(),
                    ));
                }
            };

            // Look for Theora identification packet starting with "\x80theora"
            if let Some(first_packet) = page.packets.first() {
                if first_packet.len() >= 7
                    && first_packet[0] == 0x80
                    && &first_packet[1..7] == b"theora"
                {
                    let mut info = TheoraInfo::from_identification_header(first_packet)?;
                    info.serial = page.serial;

                    // Calculate length from last page
                    Self::post_tags_info_async(reader, &mut info).await?;

                    return Ok(info);
                }
            }
        }
    }

    /// Calculates the stream duration asynchronously from the last Ogg page.
    ///
    /// This method seeks to find the last page of the Theora stream and uses
    /// its granule position to calculate the total duration based on the
    /// frame rate and keyframe granule shift.
    ///
    /// # Arguments
    /// * `reader` - An async reader for the Ogg stream
    /// * `info` - Mutable reference to the TheoraInfo to update with duration
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if the operation fails
    async fn post_tags_info_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        info: &mut TheoraInfo,
    ) -> Result<()> {
        let last_page = OggPage::find_last_async(reader, info.serial, true)
            .await?
            .ok_or_else(|| AudexError::InvalidData("could not find last page".to_string()))?;
        if last_page.position != -1 && info.fps > 0.0 {
            // Decode granule position to get frame count
            let granule_shift = info.keyframe_granule_shift;
            let granule = last_page.position as u64;
            let keyframe = granule >> granule_shift;
            let offset = granule & ((1u64 << granule_shift) - 1);
            let total_frames = keyframe + offset;

            let duration_secs = total_frames as f64 / info.fps;
            if duration_secs.is_finite() && duration_secs >= 0.0 && duration_secs <= u64::MAX as f64
            {
                info.length = Some(Duration::from_secs_f64(duration_secs));
            }
        }
        Ok(())
    }

    /// Parses Theora comment tags asynchronously from the stream.
    ///
    /// This method reads the Vorbis comment packet from the Theora stream,
    /// which contains metadata such as title, artist, and other user-defined
    /// tags in key-value format.
    ///
    /// # Arguments
    /// * `reader` - An async reader positioned for reading
    /// * `serial` - The serial number of the Theora stream to match
    ///
    /// # Returns
    /// * `Result<TheoraTags>` - The parsed tags or an error
    async fn parse_tags_async<R: tokio::io::AsyncRead + tokio::io::AsyncSeek + Unpin>(
        reader: &mut R,
        serial: u32,
    ) -> Result<TheoraTags> {
        let mut tags = TheoraTags {
            inner: VCommentDict::new(),
            serial,
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
                if let Some(first_packet) = page.packets.first() {
                    // Check for comment packet (type 0x81)
                    if first_packet.len() >= 7
                        && first_packet[0] == 0x81
                        && &first_packet[1..7] == b"theora"
                    {
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg Theora comment packet",
                        )?;
                        pages.push(page);
                        found_tags = true;
                    } else if !found_header
                        && first_packet.len() >= 7
                        && first_packet[0] == 0x80
                        && &first_packet[1..7] == b"theora"
                    {
                        found_header = true;
                    } else if found_tags && !pages.last().is_none_or(|p| p.is_complete()) {
                        OggPage::accumulate_page_bytes_with_limit(
                            limits,
                            &mut cumulative_bytes,
                            &page,
                            "Ogg Theora comment packet",
                        )?;
                        pages.push(page);
                    } else if found_tags {
                        break;
                    }
                }
            }
        }

        if pages.is_empty() {
            return Ok(tags);
        }

        // Reconstruct packets from pages
        let packets = OggPage::to_packets(&pages, false)?;
        if packets.is_empty() || packets[0].len() < 7 {
            return Ok(tags);
        }

        // Parse Vorbis comment data (skip "\x81theora" header)
        let comment_data = &packets[0][7..];
        let mut cursor = Cursor::new(comment_data);

        let _ = tags
            .inner
            .load(&mut cursor, crate::vorbis::ErrorMode::Replace, false);

        Ok(tags)
    }

    /// Saves the Ogg Theora file asynchronously with updated tags.
    ///
    /// This method writes any modified metadata tags back to the file using
    /// non-blocking I/O operations. The file must have been loaded with a
    /// valid path for this operation to succeed.
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if saving fails
    ///
    /// # Errors
    /// Returns an error if no file path is available or if writing fails.
    ///
    /// # Example
    /// ```rust,no_run
    /// use audex::oggtheora::OggTheora;
    /// use audex::{FileType, Tags};
    ///
    /// #[tokio::main]
    /// async fn main() -> audex::Result<()> {
    ///     let mut theora = OggTheora::load_async("video.ogv").await?;
    ///     if let Some(tags) = theora.tags_mut() {
    ///         tags.set("TITLE", vec!["My Video".to_string()]);
    ///     }
    ///     theora.save_async().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn save_async(&mut self) -> Result<()> {
        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::InvalidOperation("No file path available for saving".to_string())
        })?;

        if let Some(ref tags) = self.tags {
            Self::inject_tags_async(path, tags).await?;
        }

        Ok(())
    }

    /// Injects tags into the Ogg Theora file asynchronously.
    ///
    /// This internal method handles the low-level operation of replacing the
    /// Vorbis comment packet in the Theora stream with new tag data. It
    /// preserves the stream structure while updating the metadata.
    ///
    /// # Arguments
    /// * `path` - The file path to write tags to
    /// * `tags` - The tags to inject into the file
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if injection fails
    async fn inject_tags_async<P: AsRef<Path>>(path: P, tags: &TheoraTags) -> Result<()> {
        let file_path = path.as_ref();

        let file = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(file_path)
            .await?;

        let mut reader = TokioBufReader::new(file);

        // Find existing comment pages
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
                    if first_packet.len() >= 7
                        && first_packet[0] == 0x81
                        && &first_packet[1..7] == b"theora"
                    {
                        comment_pages.push(page);
                        found_tags = true;
                    } else if found_tags && !comment_pages.last().is_none_or(|p| p.is_complete()) {
                        comment_pages.push(page);
                    } else if found_tags {
                        break;
                    }
                }
            }
        }

        if comment_pages.is_empty() {
            return Err(AudexError::InvalidData(
                "No Theora comment packet found".to_string(),
            ));
        }

        // Reconstruct packets
        let old_packets = OggPage::to_packets(&comment_pages, false)?;
        if old_packets.is_empty() {
            return Err(AudexError::InvalidData(
                "Failed to reconstruct comment packet".to_string(),
            ));
        }

        // Calculate content size (file size minus first packet)
        let content_size = {
            let old_pos = reader.stream_position().await?;
            let file_size = reader.seek(SeekFrom::End(0)).await?;
            reader.seek(SeekFrom::Start(old_pos)).await?;
            // Use saturating subtraction to prevent overflow on large or crafted values
            i64::try_from(file_size)
                .unwrap_or(i64::MAX)
                .saturating_sub(i64::try_from(old_packets[0].len()).unwrap_or(0))
        };

        // Create new comment data: b"\x81theora" + vcomment(framing=false)
        let vcomment_data = {
            let mut data = b"\x81theora".to_vec();
            let mut vcomment_bytes = Vec::new();

            let mut comment_to_write = tags.inner.clone();
            // Only set Audex vendor string when there are actual tags to write
            if !comment_to_write.keys().is_empty() {
                comment_to_write.set_vendor(format!("Audex {}", VERSION_STRING));
            }

            // Use framing=false for Theora
            comment_to_write.write(&mut vcomment_bytes, Some(false))?;
            data.extend_from_slice(&vcomment_bytes);
            data
        };

        let padding_left = old_packets[0].len() as i64 - vcomment_data.len() as i64;

        // Calculate padding using PaddingInfo
        let info = crate::tags::PaddingInfo::new(padding_left, content_size);
        let new_padding = info.get_padding_with(None::<fn(&crate::tags::PaddingInfo) -> i64>);

        // Set the new comment packet with proper padding
        let mut new_packets = old_packets;
        new_packets[0] = vcomment_data;
        if new_padding > 0 {
            new_packets[0].extend_from_slice(&vec![0u8; usize::try_from(new_padding).unwrap_or(0)]);
        }

        // Create new pages, preserving original structure where possible
        let new_pages = OggPage::from_packets_try_preserve(new_packets.clone(), &comment_pages);

        // Fall back to regular from_packets if try_preserve failed
        let new_pages = if new_pages.is_empty() {
            let first_sequence = comment_pages[0].sequence;
            // Use 0 as fallback if position is negative (sentinel value) or pages are empty
            let original_granule = comment_pages
                .last()
                .map(|p| {
                    if p.position < 0 {
                        0u64
                    } else {
                        p.position as u64
                    }
                })
                .unwrap_or(0);
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

        OggPage::replace_async(&mut writer, &comment_pages, new_pages).await?;

        Ok(())
    }

    /// Clears all metadata tags from the Ogg Theora file asynchronously.
    ///
    /// This method removes all existing tags by replacing them with an empty
    /// tag set. The vendor string is preserved. This operation is useful for
    /// stripping all metadata from a video file.
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if clearing fails
    ///
    /// # Errors
    /// Returns an error if no file path is available or if writing fails.
    ///
    /// # Example
    /// ```rust,no_run
    /// use audex::oggtheora::OggTheora;
    ///
    /// #[tokio::main]
    /// async fn main() -> audex::Result<()> {
    ///     let mut theora = OggTheora::load_async("video.ogv").await?;
    ///     theora.clear_async().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn clear_async(&mut self) -> Result<()> {
        let mut inner = VCommentDict::new();
        inner.set_vendor(String::new());
        let empty_tags = TheoraTags {
            inner,
            serial: self.info.serial,
        };

        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::InvalidOperation("No file path available for deletion".to_string())
        })?;

        Self::inject_tags_async(path, &empty_tags).await?;
        self.tags = Some(empty_tags);

        Ok(())
    }

    /// Deletes all metadata tags from an Ogg Theora file at the specified path.
    ///
    /// This is a convenience method that loads the file, clears all tags, and
    /// saves the changes in one operation. Useful for batch processing files
    /// without maintaining an OggTheora instance.
    ///
    /// # Arguments
    /// * `path` - The file path to clear tags from
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if the operation fails
    ///
    /// # Example
    /// ```rust,no_run
    /// use audex::oggtheora::OggTheora;
    ///
    /// #[tokio::main]
    /// async fn main() -> audex::Result<()> {
    ///     OggTheora::delete_async("video.ogv").await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        let mut theora = Self::load_async(path).await?;
        theora.clear_async().await
    }
}

/// Standalone async function for clearing tags from an Ogg Theora file.
///
/// This function provides a convenient way to remove all metadata tags from
/// a file without creating an OggTheora instance manually. It loads the file,
/// clears all tags, and saves the changes asynchronously.
///
/// # Arguments
/// * `path` - The file path to clear tags from
///
/// # Returns
/// * `Result<()>` - Success or an error if the operation fails
///
/// # Example
/// ```rust,no_run
/// use audex::oggtheora;
///
/// #[tokio::main]
/// async fn main() -> audex::Result<()> {
///     oggtheora::clear_async("video.ogv").await?;
///     Ok(())
/// }
/// ```
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<Path>>(path: P) -> Result<()> {
    OggTheora::delete_async(path).await
}
