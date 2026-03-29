//! MP4 file handling/mp4

use crate::mp4::as_entry::AudioSampleEntry;
use crate::mp4::atom::{Atoms, MP4Atom};
use crate::mp4::util::{name2key, parse_full_atom};
use crate::tags::{Metadata, PaddingInfo, Tags};
use crate::util::{insert_bytes, resize_bytes};
use crate::{AudexError, FileType, Result, StreamInfo};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

const MAX_TOTAL_ILST_DATA_BYTES: u64 = 128 * 1024 * 1024;

/// Data type identifiers for MP4 metadata atoms.
///
/// These type codes indicate how data is stored within iTunes-style metadata atoms.
/// Each metadata value in the `ilst` (item list) atom includes a type identifier
/// that specifies how to interpret the raw bytes.
///
/// # Usage
///
/// The data type is stored in the metadata atom's `data` sub-atom and determines:
/// - How to decode the raw bytes (text encoding, integer format, etc.)
/// - What kind of data the field contains (text, image, number, etc.)
/// - Which operations are valid for the field
///
/// # Common Types
///
/// Most iTunes metadata uses:
/// - **Utf8** (1): Standard text fields (title, artist, album, etc.)
/// - **Integer** (21): Numeric fields (track number, disc number, etc.)
/// - **Jpeg** (13) / **Png** (14): Cover artwork
///
/// # Examples
///
/// ```
/// use audex::mp4::AtomDataType;
///
/// // Text metadata
/// let title_type = AtomDataType::Utf8;
///
/// // Cover artwork
/// let jpeg_cover = AtomDataType::Jpeg;
/// let png_cover = AtomDataType::Png;
///
/// // Numeric metadata
/// let track_num_type = AtomDataType::Integer;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u32)]
pub enum AtomDataType {
    /// Implicit type - no explicit type indicator needed.
    ///
    /// Used for atoms where only one data type is valid, so the type
    /// can be inferred from the atom name alone.
    Implicit = 0,

    /// UTF-8 encoded text without byte order mark or null terminator.
    ///
    /// The most common type for text metadata. Text is stored as raw UTF-8
    /// bytes without length prefix or terminator.
    Utf8 = 1,

    /// UTF-16 Big Endian encoded text (UTF-16BE).
    ///
    /// Less common than UTF-8. Used for text that requires UTF-16 encoding.
    Utf16 = 2,

    /// Shift-JIS encoded text.
    ///
    /// Legacy Japanese character encoding. Deprecated except for special
    /// Japanese characters not representable in UTF-8.
    Sjis = 3,

    /// HTML formatted text.
    ///
    /// The HTML header specifies which HTML version is used.
    Html = 6,

    /// XML formatted text.
    ///
    /// The XML header must identify the DTD or schema.
    Xml = 7,

    /// UUID (Universally Unique Identifier).
    ///
    /// Also known as GUID. Stored as 16 bytes in binary format.
    /// Valid for use as an identifier.
    Uuid = 8,

    /// International Standard Recording Code (ISRC).
    ///
    /// Stored as UTF-8 text. Used to uniquely identify sound recordings.
    Isrc = 9,

    /// MPEG-4 Intellectual Property Management and Protection.
    ///
    /// Stored as UTF-8 text. Valid as an identifier.
    Mi3p = 10,

    /// GIF image format (deprecated).
    ///
    /// Support for GIF artwork is deprecated. Use JPEG or PNG instead.
    Gif = 12,

    /// JPEG image format.
    ///
    /// Standard format for cover artwork. Widely supported.
    Jpeg = 13,

    /// PNG image format.
    ///
    /// Alternative format for cover artwork. Supports transparency.
    Png = 14,

    /// Absolute URL in UTF-8 characters.
    Url = 15,

    /// Duration in milliseconds.
    ///
    /// Stored as a 32-bit integer.
    Duration = 16,

    /// Date and time in UTC.
    ///
    /// Stored as seconds since midnight, January 1, 1904.
    /// Can be 32-bit or 64-bit integer.
    DateTime = 17,

    /// Enumerated genre values.
    ///
    /// A predefined list of genre codes.
    Genres = 18,

    /// Signed big-endian integer.
    ///
    /// Can be 1, 2, 3, 4, or 8 bytes in length. Used for numeric
    /// metadata like track numbers, disc numbers, tempo, etc.
    Integer = 21,

    /// RIAA Parental Advisory rating.
    ///
    /// 8-bit integer: -1 = no advisory, 1 = explicit, 0 = unspecified
    RiaaPA = 24,

    /// Universal Product Code (UPC).
    ///
    /// Stored as UTF-8 text. Valid as an identifier.
    Upc = 25,

    /// Windows Bitmap (BMP) image format.
    Bmp = 27,
}

impl AtomDataType {
    /// Convert from u32 to AtomDataType
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(AtomDataType::Implicit),
            1 => Some(AtomDataType::Utf8),
            2 => Some(AtomDataType::Utf16),
            3 => Some(AtomDataType::Sjis),
            6 => Some(AtomDataType::Html),
            7 => Some(AtomDataType::Xml),
            8 => Some(AtomDataType::Uuid),
            9 => Some(AtomDataType::Isrc),
            10 => Some(AtomDataType::Mi3p),
            12 => Some(AtomDataType::Gif),
            13 => Some(AtomDataType::Jpeg),
            14 => Some(AtomDataType::Png),
            15 => Some(AtomDataType::Url),
            16 => Some(AtomDataType::Duration),
            17 => Some(AtomDataType::DateTime),
            18 => Some(AtomDataType::Genres),
            21 => Some(AtomDataType::Integer),
            24 => Some(AtomDataType::RiaaPA),
            25 => Some(AtomDataType::Upc),
            27 => Some(AtomDataType::Bmp),
            _ => None,
        }
    }
}

/// Cover artwork embedded in MP4/M4A files.
///
/// This struct represents album artwork or other images embedded in the MP4
/// file's metadata. Cover art is stored in the `covr` atom within the `ilst`
/// (iTunes metadata) section.
///
/// # Supported Formats
///
/// Common image formats:
/// - **JPEG**: Most widely supported, smaller file sizes
/// - **PNG**: Supports transparency, larger file sizes
/// - **BMP**: Windows bitmap (rarely used)
/// - **GIF**: Deprecated, avoid using
///
/// # Examples
///
/// ## Creating Cover Art
///
/// ```
/// use audex::mp4::MP4Cover;
///
/// // Create JPEG cover
/// let jpeg_data = vec![0xFF, 0xD8, 0xFF, /* ... */];
/// let cover = MP4Cover::new_jpeg(jpeg_data);
///
/// // Create PNG cover
/// let png_data = vec![0x89, 0x50, 0x4E, 0x47, /* ... */];
/// let cover = MP4Cover::new_png(png_data);
/// ```
///
/// ## Auto-detecting Format
///
/// ```no_run
/// use audex::mp4::{MP4Cover, AtomDataType};
///
/// let image_data = std::fs::read("cover.jpg").unwrap();
///
/// // Detect format from magic bytes
/// if let Some(format) = MP4Cover::detect_format(&image_data) {
///     let cover = MP4Cover::new(image_data, format);
///     println!("Detected format: {:?}", format);
/// }
/// ```
///
/// ## Adding to MP4 File
///
/// ```no_run
/// use audex::mp4::{MP4, MP4Cover};
/// use audex::FileType;
///
/// let mut mp4 = MP4::load("song.m4a").unwrap();
///
/// // Load image
/// let image_data = std::fs::read("artwork.jpg").unwrap();
/// let cover = MP4Cover::new_jpeg(image_data);
///
/// // Add to file (implementation depends on MP4Tags)
/// // mp4.tags.insert("covr".to_string(), vec![cover]);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MP4Cover {
    /// Raw image data bytes
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::bytes_as_base64")
    )]
    pub data: Vec<u8>,

    /// Image format identifier (JPEG, PNG, etc.)
    pub imageformat: AtomDataType,
}

impl MP4Cover {
    /// JPEG image format constant
    pub const FORMAT_JPEG: AtomDataType = AtomDataType::Jpeg;
    /// PNG image format constant
    pub const FORMAT_PNG: AtomDataType = AtomDataType::Png;

    /// Create a new MP4Cover with the given data and format
    pub fn new(data: Vec<u8>, imageformat: AtomDataType) -> Self {
        Self { data, imageformat }
    }

    /// Create a new JPEG cover
    pub fn new_jpeg(data: Vec<u8>) -> Self {
        Self::new(data, Self::FORMAT_JPEG)
    }

    /// Create a new PNG cover
    pub fn new_png(data: Vec<u8>) -> Self {
        Self::new(data, Self::FORMAT_PNG)
    }

    /// Detect image format from data using magic bytes
    /// Returns the detected format or None if unrecognized
    pub fn detect_format(data: &[u8]) -> Option<AtomDataType> {
        if data.len() < 4 {
            return None;
        }

        // Check for JPEG magic bytes (FF D8 FF)
        if data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF {
            return Some(AtomDataType::Jpeg);
        }

        // Check for PNG magic bytes (89 50 4E 47)
        if data.len() >= 8
            && data[0] == 0x89
            && data[1] == 0x50
            && data[2] == 0x4E
            && data[3] == 0x47
            && data[4] == 0x0D
            && data[5] == 0x0A
            && data[6] == 0x1A
            && data[7] == 0x0A
        {
            return Some(AtomDataType::Png);
        }

        // Check for GIF magic bytes (47 49 46 38)
        if data[0] == 0x47 && data[1] == 0x49 && data[2] == 0x46 && data[3] == 0x38 {
            return Some(AtomDataType::Gif);
        }

        // Check for BMP magic bytes (42 4D)
        if data[0] == 0x42 && data[1] == 0x4D {
            return Some(AtomDataType::Bmp);
        }

        None
    }

    /// Create a new MP4Cover with automatic format detection
    /// Falls back to JPEG if format cannot be detected
    pub fn new_auto_detect(data: Vec<u8>) -> Self {
        let imageformat = Self::detect_format(&data).unwrap_or(Self::FORMAT_JPEG);
        Self::new(data, imageformat)
    }
}

impl AsRef<[u8]> for MP4Cover {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl std::ops::Deref for MP4Cover {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

/// Custom freeform metadata value for non-standard iTunes tags.
///
/// Freeform tags (identified by the `----` atom) allow storing custom metadata
/// that doesn't fit into the standard iTunes metadata schema. Each freeform value
/// consists of binary data, a format identifier, and a version number.
///
/// # Structure
///
/// - **`data`**: Raw binary data for the metadata value
/// - **`dataformat`**: Type identifier indicating how to interpret the data (text, binary, etc.)
/// - **`version`**: Version number for format compatibility (typically 0)
///
/// # Common Uses
///
/// - Custom application-specific metadata
/// - Extended tag fields not in standard iTunes schema
/// - Binary data that needs specific format handling
/// - Cross-platform metadata exchange
///
/// # Examples
///
/// ## Creating a text freeform tag
///
/// ```
/// use audex::mp4::MP4FreeForm;
///
/// let custom_tag = MP4FreeForm::new_text(b"Custom metadata value".to_vec());
/// ```
///
/// ## Creating a binary freeform tag
///
/// ```
/// use audex::mp4::{MP4FreeForm, AtomDataType};
///
/// let binary_data = vec![0x01, 0x02, 0x03, 0x04];
/// let custom_tag = MP4FreeForm::new(binary_data, AtomDataType::Implicit, 0);
/// ```
///
/// ## Adding freeform metadata to an MP4 file
///
/// ```no_run
/// use audex::mp4::{MP4, MP4FreeForm};
/// use audex::FileType;
///
/// let mut mp4 = MP4::load("song.m4a").unwrap();
///
/// if let Some(ref mut tags) = mp4.tags {
///     let custom_value = MP4FreeForm::new_text(b"MyApp Metadata".to_vec());
///     tags.freeforms.insert(
///         "----:com.myapp:custom_field".to_string(),
///         vec![custom_value]
///     );
/// }
///
/// mp4.save().unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MP4FreeForm {
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::bytes_as_base64")
    )]
    pub data: Vec<u8>,
    pub dataformat: AtomDataType,
    pub version: u8,
}

impl MP4FreeForm {
    /// Data format constant (deprecated, use AtomDataType::Implicit)
    pub const FORMAT_DATA: AtomDataType = AtomDataType::Implicit;
    /// Text format constant (deprecated, use AtomDataType::Utf8)
    pub const FORMAT_TEXT: AtomDataType = AtomDataType::Utf8;

    /// Create a new MP4FreeForm with the given data, format, and version
    pub fn new(data: Vec<u8>, dataformat: AtomDataType, version: u8) -> Self {
        Self {
            data,
            dataformat,
            version,
        }
    }

    /// Create a new freeform value with UTF-8 text format
    pub fn new_text(data: Vec<u8>) -> Self {
        Self::new(data, AtomDataType::Utf8, 0)
    }

    /// Create a new freeform value with implicit format
    pub fn new_data(data: Vec<u8>) -> Self {
        Self::new(data, AtomDataType::Implicit, 0)
    }
}

impl AsRef<[u8]> for MP4FreeForm {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl std::ops::Deref for MP4FreeForm {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

/// Individual chapter marker for audiobooks and podcasts.
///
/// Represents a single chapter within an M4A/M4B audiobook file. Each chapter
/// has a start position (in seconds) and a descriptive title.
///
/// # Fields
///
/// - **`start`**: Chapter start position in seconds from the beginning of the file
/// - **`title`**: Human-readable chapter name or description
///
/// # Examples
///
/// ## Creating a chapter
///
/// ```
/// use audex::mp4::Chapter;
///
/// let chapter = Chapter::new(0.0, "Introduction".to_string());
/// println!("Chapter '{}' starts at {} seconds", chapter.title, chapter.start);
/// ```
///
/// ## Reading chapters from an audiobook
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mp4 = MP4::load("audiobook.m4b").unwrap();
///
/// if let Some(ref chapters) = mp4.chapters {
///     for (i, chapter) in chapters.iter().enumerate() {
///         println!("Chapter {}: {} (starts at {:.2}s)",
///             i + 1, chapter.title, chapter.start);
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    /// Position from the start of the file in seconds
    pub start: f64,
    /// Title of the chapter
    pub title: String,
}

impl Chapter {
    /// Create a new chapter
    pub fn new(start: f64, title: String) -> Self {
        Self { start, title }
    }
}

/// Collection of chapter markers for M4A/M4B audiobook files.
///
/// This structure stores chapter information found in the `chpl` (chapter list) atom
/// within the user data section of MP4/M4A files. Chapters are commonly used in
/// audiobooks (M4B files) and long-form podcasts to allow navigation to specific sections.
///
/// # Structure
///
/// - **`chapters`**: Ordered list of chapter markers with start times and titles
/// - **`timescale`**: Optional timescale value from the movie header (used for time calculations)
/// - **`duration`**: Optional total duration from the movie header
///
/// # Atom Location
///
/// Chapters are stored in the `moov.udta.chpl` atom path within the MP4 container.
///
/// # Examples
///
/// ## Reading chapter information
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mp4 = MP4::load("audiobook.m4b").unwrap();
///
/// if let Some(ref chapters) = mp4.chapters {
///     println!("Found {} chapters", chapters.len());
///
///     for (i, chapter) in chapters.iter().enumerate() {
///         let minutes = (chapter.start / 60.0).floor();
///         let seconds = chapter.start % 60.0;
///         println!("  {}. {} - {}:{:02.0}",
///             i + 1, chapter.title, minutes, seconds);
///     }
///
///     if let Some(timescale) = chapters.timescale {
///         println!("Timescale: {} units/second", timescale);
///     }
/// }
/// ```
///
/// ## Indexing chapters
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mp4 = MP4::load("audiobook.m4b").unwrap();
///
/// if let Some(ref chapters) = mp4.chapters {
///     // Access first chapter
///     if let Some(first) = chapters.get(0) {
///         println!("First chapter: {}", first.title);
///     }
///
///     // Access by position
///     if let Some(third) = chapters.get(2) {
///         println!("Third chapter starts at: {}s", third.start);
///     }
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct MP4Chapters {
    pub chapters: Vec<Chapter>,
    pub timescale: Option<u32>,
    pub duration: Option<u64>,
}

impl MP4Chapters {
    /// Create a new empty MP4Chapters
    pub fn new() -> Self {
        Self::default()
    }

    /// Load chapters from atoms
    pub fn load<R: Read + Seek>(atoms: &Atoms, reader: &mut R) -> Result<Option<Self>> {
        if !Self::can_load(atoms) {
            return Ok(None);
        }

        let mut chapters = MP4Chapters::new();

        // Parse mvhd for timescale and duration
        if let Some(mvhd) = atoms.get("moov.mvhd") {
            chapters.parse_mvhd(mvhd, reader)?;
        }

        if chapters.timescale.is_none() {
            return Err(AudexError::ParseError(
                "Unable to get timescale".to_string(),
            ));
        }

        // Parse chpl for chapter data
        if let Some(chpl) = atoms.get("moov.udta.chpl") {
            chapters.parse_chpl(chpl, reader)?;
        }

        Ok(Some(chapters))
    }

    /// Check if chapters can be loaded from atoms
    pub fn can_load(atoms: &Atoms) -> bool {
        atoms.contains("moov.udta.chpl") && atoms.contains("moov.mvhd")
    }

    /// Parse mvhd atom for timescale and duration
    fn parse_mvhd<R: Read + Seek>(&mut self, mvhd: &MP4Atom, reader: &mut R) -> Result<()> {
        let data = mvhd.read_data(reader)?;
        self.parse_mvhd_data(&data)
    }

    /// Parse mvhd data that has already been read from the stream.
    fn parse_mvhd_data(&mut self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Err(AudexError::ParseError("Invalid mvhd".to_string()));
        }

        let version = data[0];
        let mut pos = 4; // Skip version + flags

        match version {
            0 => {
                if data.len() < pos + 16 {
                    return Err(AudexError::ParseError("mvhd too short".to_string()));
                }
                pos += 8; // Skip created, modified

                self.timescale = Some(u32::from_be_bytes([
                    data[pos],
                    data[pos + 1],
                    data[pos + 2],
                    data[pos + 3],
                ]));
                pos += 4;

                self.duration = Some(u32::from_be_bytes([
                    data[pos],
                    data[pos + 1],
                    data[pos + 2],
                    data[pos + 3],
                ]) as u64);
            }
            1 => {
                if data.len() < pos + 24 {
                    return Err(AudexError::ParseError("mvhd too short".to_string()));
                }
                pos += 16; // Skip created, modified

                self.timescale = Some(u32::from_be_bytes([
                    data[pos],
                    data[pos + 1],
                    data[pos + 2],
                    data[pos + 3],
                ]));
                pos += 4;

                self.duration = Some(u64::from_be_bytes([
                    data[pos],
                    data[pos + 1],
                    data[pos + 2],
                    data[pos + 3],
                    data[pos + 4],
                    data[pos + 5],
                    data[pos + 6],
                    data[pos + 7],
                ]));
            }
            _ => {
                return Err(AudexError::ParseError(format!(
                    "Unknown mvhd version {}",
                    version
                )));
            }
        }

        Ok(())
    }

    /// Parse chpl atom for chapter data
    fn parse_chpl<R: Read + Seek>(&mut self, chpl: &MP4Atom, reader: &mut R) -> Result<()> {
        let data = chpl.read_data(reader)?;
        self.parse_chpl_data(&data)
    }

    /// Parse chpl data that has already been read from the stream.
    pub fn parse_chpl_data(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 9 {
            return Err(AudexError::ParseError("Invalid chpl atom".to_string()));
        }

        // The first byte is the version field of the full box header.
        // Version 1 uses a 4-byte (u32) chapter count to support >255 chapters,
        // while version 0 uses a single byte.
        let version = data[0];
        let (chapters_count, mut pos) = if version >= 1 {
            if data.len() < 13 {
                return Err(AudexError::ParseError(
                    "chpl v1 atom too short for 4-byte chapter count".to_string(),
                ));
            }
            // Safe to cast: the subsequent bounds check against remaining data
            // ensures this value cannot exceed the slice length.
            let count = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
            // Each chapter requires at least 9 bytes (8-byte timestamp + 1-byte title length),
            // so reject counts that exceed the remaining data.
            let remaining = data.len().saturating_sub(12);
            if count > remaining / 9 {
                return Err(AudexError::ParseError(
                    "chpl chapter count exceeds available data".to_string(),
                ));
            }
            (count, 12)
        } else {
            let count = data[8] as usize;
            // Validate chapter count against remaining data even for version 0.
            let remaining = data.len().saturating_sub(9);
            if count > remaining / 9 {
                return Err(AudexError::ParseError(
                    "chpl chapter count exceeds available data".to_string(),
                ));
            }
            (count, 9)
        };

        for i in 0..chapters_count {
            if pos + 8 > data.len() {
                return Err(AudexError::ParseError("chpl atom truncated".to_string()));
            }

            // chpl timestamps are in 100-nanosecond units per the Nero chapter spec.
            // Convert directly to seconds by dividing by 10,000,000.
            let raw_time = u64::from_be_bytes([
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
                data[pos + 4],
                data[pos + 5],
                data[pos + 6],
                data[pos + 7],
            ]);
            pos += 8;

            if pos >= data.len() {
                return Err(AudexError::ParseError("chpl atom truncated".to_string()));
            }

            let title_len = data[pos] as usize;
            pos += 1;

            if pos + title_len > data.len() {
                return Err(AudexError::ParseError("chpl atom truncated".to_string()));
            }

            let title_bytes = &data[pos..pos + title_len];
            let title = String::from_utf8(title_bytes.to_vec())
                .map_err(|e| AudexError::ParseError(format!("chapter {} title: {}", i, e)))?;
            pos += title_len;

            let start_seconds = raw_time as f64 / 10_000_000.0;

            self.chapters.push(Chapter::new(start_seconds, title));
        }

        Ok(())
    }

    /// Get all chapters
    pub fn chapters(&self) -> &[Chapter] {
        &self.chapters
    }

    /// Get mutable reference to chapters
    pub fn chapters_mut(&mut self) -> &mut Vec<Chapter> {
        &mut self.chapters
    }

    /// Add a chapter
    pub fn add_chapter(&mut self, chapter: Chapter) {
        self.chapters.push(chapter);
    }

    /// Clear all chapters
    pub fn clear(&mut self) {
        self.chapters.clear();
    }

    /// Get number of chapters
    pub fn len(&self) -> usize {
        self.chapters.len()
    }

    /// Check if there are no chapters
    pub fn is_empty(&self) -> bool {
        self.chapters.is_empty()
    }

    /// Get an iterator over the chapters
    pub fn iter(&self) -> std::slice::Iter<'_, Chapter> {
        self.chapters.iter()
    }

    /// Get a chapter by index
    pub fn get(&self, index: usize) -> Option<&Chapter> {
        self.chapters.get(index)
    }

    /// Format chapters for display
    pub fn pprint(&self) -> String {
        let mut result = String::new();
        for chapter in &self.chapters {
            // Skip chapters with non-finite or out-of-range start times
            let Ok(duration) = Duration::try_from_secs_f64(chapter.start.max(0.0)) else {
                continue;
            };
            let hours = duration.as_secs() / 3600;
            let minutes = (duration.as_secs() % 3600) / 60;
            let seconds = duration.as_secs() % 60;
            let millis = duration.subsec_millis();

            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&format!(
                "{}:{:02}:{:02}.{:03} {}",
                hours, minutes, seconds, millis, chapter.title
            ));
        }

        if result.is_empty() {
            "chapters=".to_string()
        } else {
            format!("chapters=\n  {}", result.replace('\n', "\n  "))
        }
    }
}

/// iTunes-style metadata tags for MP4/M4A files.
///
/// This structure provides access to the iTunes metadata stored in the `ilst` (item list)
/// atom within MP4 container files. It supports standard iTunes tags, cover artwork,
/// and custom freeform metadata.
///
/// # Structure
///
/// - **`tags`**: Standard iTunes metadata fields (©nam, ©ART, ©alb, etc.)
/// - **`covers`**: Embedded cover artwork images (JPEG or PNG)
/// - **`freeforms`**: Custom freeform metadata for non-standard fields
/// - **`failed_atoms`**: Raw data from atoms that couldn't be parsed
///
/// # iTunes Tag Keys
///
/// Tags use four-character atom names, often prefixed with the copyright symbol (©, U+00A9):
///
/// - **©nam**: Title
/// - **©ART**: Artist
/// - **©alb**: Album
/// - **©day**: Release date/year
/// - **©gen**: Genre
/// - **©wrt**: Composer
/// - **©cmt**: Comment
/// - **trkn**: Track number (special format)
/// - **disk**: Disc number (special format)
/// - **covr**: Cover artwork
///
/// # Examples
///
/// ## Reading iTunes metadata
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mp4 = MP4::load("song.m4a").unwrap();
///
/// if let Some(ref tags) = mp4.tags {
///     // Read standard tags
///     if let Some(title) = tags.tags.get("©nam") {
///         println!("Title: {}", title.join(", "));
///     }
///
///     if let Some(artist) = tags.tags.get("©ART") {
///         println!("Artist: {}", artist.join(", "));
///     }
///
///     // Check for cover art
///     if !tags.covers.is_empty() {
///         println!("Has {} cover image(s)", tags.covers.len());
///         println!("First cover format: {:?}", tags.covers[0].imageformat);
///     }
/// }
/// ```
///
/// ## Modifying metadata
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mut mp4 = MP4::load("song.m4a").unwrap();
///
/// if let Some(ref mut tags) = mp4.tags {
///     // Set title and artist
///     tags.tags.insert("©nam".to_string(), vec!["New Title".to_string()]);
///     tags.tags.insert("©ART".to_string(), vec!["New Artist".to_string()]);
///
///     // Set release year
///     tags.tags.insert("©day".to_string(), vec!["2024".to_string()]);
/// }
///
/// mp4.save().unwrap();
/// ```
///
/// ## Working with cover artwork
///
/// ```no_run
/// use audex::mp4::{MP4, MP4Cover, AtomDataType};
/// use std::fs;
/// use audex::FileType;
///
/// let mut mp4 = MP4::load("song.m4a").unwrap();
///
/// if let Some(ref mut tags) = mp4.tags {
///     // Read cover art data from file
///     let cover_data = fs::read("cover.jpg").unwrap();
///     let cover = MP4Cover::new(cover_data, AtomDataType::Jpeg);
///
///     // Replace all covers with the new one
///     tags.covers = vec![cover];
/// }
///
/// mp4.save().unwrap();
/// ```
///
/// ## Using freeform tags
///
/// ```no_run
/// use audex::mp4::{MP4, MP4FreeForm};
/// use audex::FileType;
///
/// let mut mp4 = MP4::load("song.m4a").unwrap();
///
/// if let Some(ref mut tags) = mp4.tags {
///     // Add custom metadata
///     let custom_data = MP4FreeForm::new_text(b"Custom Value".to_vec());
///     tags.freeforms.insert(
///         "----:com.apple.iTunes:CUSTOM_FIELD".to_string(),
///         vec![custom_data]
///     );
/// }
///
/// mp4.save().unwrap();
/// ```
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MP4Tags {
    pub tags: HashMap<String, Vec<String>>,
    pub covers: Vec<MP4Cover>,
    pub freeforms: HashMap<String, Vec<MP4FreeForm>>,
    /// Failed atoms that couldn't be parsed (stored as raw data)
    pub failed_atoms: HashMap<String, Vec<Vec<u8>>>,
    padding: usize,
}

impl MP4Tags {
    /// Create a new empty MP4Tags with Audex vendor tag
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if MP4Tags can be loaded from atoms
    pub fn can_load(atoms: &Atoms) -> bool {
        atoms.contains("moov.udta.meta.ilst")
    }

    /// Load tags from atoms
    pub fn load<R: Read + Seek>(atoms: &Atoms, reader: &mut R) -> Result<Option<Self>> {
        if !Self::can_load(atoms) {
            return Ok(None);
        }

        let ilst_path = atoms
            .path("moov.udta.meta.ilst")
            .ok_or_else(|| AudexError::ParseError("Failed to get ilst path".to_string()))?;

        let ilst = ilst_path
            .last()
            .ok_or_else(|| AudexError::ParseError("Empty ilst path".to_string()))?;

        let mut tags = MP4Tags::new();

        // Check for adjacent free atom for padding calculation
        if let Some(meta_path) = atoms.path("moov.udta.meta") {
            if let Some(meta) = meta_path.last() {
                if let Some(children) = &meta.children {
                    for (i, child) in children.iter().enumerate() {
                        if child.name == *b"ilst" {
                            // Check previous and next atoms for free atoms
                            if i > 0 && children[i - 1].name == *b"free" {
                                tags.padding = usize::try_from(children[i - 1].data_length)
                                    .unwrap_or(usize::MAX);
                            } else if i + 1 < children.len() && children[i + 1].name == *b"free" {
                                tags.padding = usize::try_from(children[i + 1].data_length)
                                    .unwrap_or(usize::MAX);
                            }
                            break;
                        }
                    }
                }
            }
        }

        // Parse ilst children — read each atom's data once and reuse it
        // for both parsing and the failed_atoms fallback
        if let Some(children) = &ilst.children {
            let mut total_ilst_bytes = 0u64;
            for child in children {
                total_ilst_bytes =
                    total_ilst_bytes
                        .checked_add(child.data_length)
                        .ok_or_else(|| {
                            AudexError::InvalidData("MP4 metadata size overflow".to_string())
                        })?;
                if total_ilst_bytes > MAX_TOTAL_ILST_DATA_BYTES {
                    return Err(AudexError::InvalidData(format!(
                        "MP4 metadata exceeds cumulative {} byte limit",
                        MAX_TOTAL_ILST_DATA_BYTES
                    )));
                }
                let data = child.read_data(reader)?;

                if tags.parse_metadata_atom_data(child, &data).is_err() {
                    let key = crate::mp4::util::name2key(&child.name);
                    tags.failed_atoms.entry(key).or_default().push(data);
                }
            }
        }

        Ok(Some(tags))
    }

    /// Parse a single metadata atom from pre-read data.
    fn parse_metadata_atom_data(&mut self, atom: &MP4Atom, data: &[u8]) -> Result<()> {
        // Check if we have specific parser for this atom
        match &atom.name {
            b"covr" => self.parse_cover_atom(data)?,
            b"----" => self.parse_freeform_atom(data)?,
            b"trkn" | b"disk" => self.parse_pair_atom(&atom.name, data)?,
            b"cpil" | b"pgap" | b"pcst" => self.parse_bool_atom(&atom.name, data)?,
            b"gnre" => self.parse_genre_atom(data)?,
            // Integer atoms with specific byte requirements
            b"plID" => self.parse_integer_atom_bytes(&atom.name, data, 8)?,
            b"cnID" | b"geID" | b"atID" | b"sfID" | b"cmID" | b"tvsn" | b"tves" => {
                self.parse_integer_atom_bytes(&atom.name, data, 4)?
            }
            b"tmpo" | b"\xa9mvi" | b"\xa9mvc" => {
                self.parse_integer_atom_bytes(&atom.name, data, 2)?
            }
            b"akID" | b"shwm" | b"stik" | b"hdvd" | b"rtng" => {
                self.parse_integer_atom_bytes(&atom.name, data, 1)?
            }
            // Text atoms (including implicit text atoms)
            _ => {
                // Try to parse as text - for known text atoms allow implicit format
                let is_known_text = matches!(
                    &atom.name,
                    b"\xa9nam"
                        | b"\xa9alb"
                        | b"\xa9ART"
                        | b"aART"
                        | b"\xa9wrt"
                        | b"\xa9day"
                        | b"\xa9cmt"
                        | b"desc"
                        | b"purd"
                        | b"\xa9grp"
                        | b"\xa9gen"
                        | b"\xa9lyr"
                        | b"catg"
                        | b"keyw"
                        | b"\xa9too"
                        | b"\xa9pub"
                        | b"cprt"
                        | b"soal"
                        | b"soaa"
                        | b"soar"
                        | b"sonm"
                        | b"soco"
                        | b"sosn"
                        | b"tvsh"
                        | b"\xa9wrk"
                        | b"\xa9mvn"
                        | b"purl"
                        | b"egid"
                );

                if is_known_text {
                    self.parse_text_atom(&atom.name, data)?;
                } else {
                    // Unknown atom - reject it so it gets stored in failed_atoms
                    return Err(AudexError::ParseError(format!(
                        "Unknown atom type: {}",
                        crate::mp4::util::name2key(&atom.name)
                    )));
                }
            }
        }

        Ok(())
    }

    /// Parse cover artwork atom
    fn parse_cover_atom(&mut self, data: &[u8]) -> Result<()> {
        // Limit the number of embedded cover images to reject adversarial files
        // that pack thousands of tiny data atoms inside a single covr atom.
        const MAX_COVER_IMAGES: usize = 256;

        let mut pos = 0;

        while pos + 12 <= data.len() {
            if self.covers.len() >= MAX_COVER_IMAGES {
                break;
            }
            let length =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;

            if length < 16 || pos.checked_add(length).is_none_or(|end| end > data.len()) {
                return Err(AudexError::ParseError(
                    "invalid covr child atom length".to_string(),
                ));
            }

            let name = &data[pos + 4..pos + 8];
            if name != b"data" {
                // Skip name atoms
                if name == b"name" {
                    pos += length;
                    continue;
                }
                return Err(AudexError::ParseError(format!(
                    "unexpected atom {:?} inside covr",
                    name
                )));
            }

            let format_raw =
                u32::from_be_bytes([data[pos + 8], data[pos + 9], data[pos + 10], data[pos + 11]]);

            // Parse image format using from_u32 to support all image types (JPEG, PNG, GIF, BMP)
            // Default to JPEG if format is unrecognized or not a valid image type
            let imageformat = match AtomDataType::from_u32(format_raw) {
                Some(AtomDataType::Jpeg) => AtomDataType::Jpeg,
                Some(AtomDataType::Png) => AtomDataType::Png,
                Some(AtomDataType::Gif) => AtomDataType::Gif,
                Some(AtomDataType::Bmp) => AtomDataType::Bmp,
                // Default to JPEG for Implicit or any other non-image type
                _ => AtomDataType::Jpeg,
            };

            let cover_len = length - 16;
            crate::limits::ParseLimits::default()
                .check_image_size(cover_len as u64, "MP4 covr image")?;
            let cover_data = data[pos + 16..pos + length].to_vec();
            self.covers.push(MP4Cover::new(cover_data, imageformat));

            pos += length;
        }

        Ok(())
    }

    /// Parse freeform atom (----)
    fn parse_freeform_atom(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 8 {
            return Err(AudexError::ParseError(
                "Freeform atom too short".to_string(),
            ));
        }

        // Parse mean atom
        // Safe to cast: immediately validated against data.len() below.
        let mean_length = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        if mean_length < 12 || mean_length > data.len() {
            return Err(AudexError::ParseError("Invalid mean length".to_string()));
        }

        // Reject invalid UTF-8 instead of using lossy conversion — replacement
        // characters would make the key unmatchable on round-trip, causing
        // silent data loss. Invalid atoms are preserved in failed_atoms.
        let mean = String::from_utf8(data[12..mean_length].to_vec()).map_err(|_| {
            AudexError::ParseError("Freeform atom mean field contains invalid UTF-8".to_string())
        })?;

        // Parse name atom
        let name_start = mean_length;
        if name_start + 8 > data.len() {
            return Err(AudexError::ParseError(
                "Freeform atom truncated".to_string(),
            ));
        }

        let name_length = u32::from_be_bytes([
            data[name_start],
            data[name_start + 1],
            data[name_start + 2],
            data[name_start + 3],
        ]) as usize;

        // Use checked arithmetic to prevent overflow on 32-bit platforms
        let name_end = name_start.checked_add(name_length).ok_or_else(|| {
            AudexError::ParseError("freeform atom name length overflow".to_string())
        })?;
        if name_length < 12 || name_end > data.len() {
            return Err(AudexError::ParseError("Invalid name length".to_string()));
        }

        let name = String::from_utf8(data[name_start + 12..name_end].to_vec()).map_err(|_| {
            AudexError::ParseError("Freeform atom name field contains invalid UTF-8".to_string())
        })?;

        // Parse data atoms
        let mut pos = name_end;
        let key = format!("----:{}:{}", mean, name);
        let mut values = Vec::new();

        while pos + 16 <= data.len() {
            let length =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;

            if length < 16 || pos.checked_add(length).is_none_or(|end| end > data.len()) {
                return Err(AudexError::ParseError(
                    "invalid freeform data child atom length".to_string(),
                ));
            }

            let atom_name = &data[pos + 4..pos + 8];
            if atom_name != b"data" {
                return Err(AudexError::ParseError(
                    "unexpected freeform child atom name".to_string(),
                ));
            }

            let version = data[pos + 8];
            let flags = u32::from_be_bytes([0, data[pos + 9], data[pos + 10], data[pos + 11]]);
            // Parse data format using from_u32 to support all AtomDataType variants
            let dataformat = AtomDataType::from_u32(flags).unwrap_or(AtomDataType::Implicit);

            let value_data = data[pos + 16..pos + length].to_vec();
            values.push(MP4FreeForm::new(value_data, dataformat, version));

            pos += length;
        }

        if !values.is_empty() {
            // Append to existing values if key already exists (for multiple freeform atoms with same mean:name)
            self.freeforms.entry(key).or_default().extend(values);
        }

        Ok(())
    }

    /// Parse pair atom (track/disk numbers)
    fn parse_pair_atom(&mut self, name: &[u8; 4], data: &[u8]) -> Result<()> {
        let mut values = Vec::new();
        let mut pos = 0;

        while pos + 12 <= data.len() {
            let length =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;

            if length < 16 || pos.checked_add(length).is_none_or(|end| end > data.len()) {
                break;
            }

            if &data[pos + 4..pos + 8] == b"data" && length >= 22 {
                // Bytes 18-19: track/disc number, bytes 20-21: total count.
                // Both require length >= 22 to stay within the atom boundary.
                let track = u16::from_be_bytes([data[pos + 18], data[pos + 19]]);
                let total = u16::from_be_bytes([data[pos + 20], data[pos + 21]]);

                if total > 0 {
                    values.push(format!("{}/{}", track, total));
                } else {
                    values.push(track.to_string());
                }
            }

            pos += length;
        }

        if !values.is_empty() {
            let key = name2key(name);
            // Append to existing values if key already exists (for multiple atoms with same name)
            self.tags.entry(key).or_default().extend(values);
        }

        Ok(())
    }

    /// Parse boolean atom
    fn parse_bool_atom(&mut self, name: &[u8; 4], data: &[u8]) -> Result<()> {
        let mut pos = 0;

        while pos + 12 <= data.len() {
            let length =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;

            if length < 17 || pos.checked_add(length).is_none_or(|end| end > data.len()) {
                return Err(AudexError::ParseError(
                    "invalid boolean child atom length".to_string(),
                ));
            }

            if &data[pos + 4..pos + 8] == b"data" {
                let value = data[pos + 16] != 0;
                let key = name2key(name);
                self.tags.insert(key, vec![value.to_string()]);
                break;
            }

            pos += length;
        }

        Ok(())
    }

    /// Parse text atom
    fn parse_text_atom(&mut self, name: &[u8; 4], data: &[u8]) -> Result<()> {
        let mut values = Vec::new();
        let mut pos = 0;

        while pos + 12 <= data.len() {
            let length =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;

            if length < 16 || pos.checked_add(length).is_none_or(|end| end > data.len()) {
                return Err(AudexError::ParseError(
                    "invalid text child atom length".to_string(),
                ));
            }

            if &data[pos + 4..pos + 8] == b"data" {
                // Check data type (bytes 8-11): should be 0 (Implicit) or 1 (Utf8)
                let data_type = u32::from_be_bytes([
                    data[pos + 8],
                    data[pos + 9],
                    data[pos + 10],
                    data[pos + 11],
                ]);

                // Only accept valid text data types (0=Implicit, 1=Utf8)
                if data_type == 0 || data_type == 1 {
                    let text_data = &data[pos + 16..pos + length];
                    let text = String::from_utf8(text_data.to_vec())
                        .map_err(|e| {
                            AudexError::ParseError(format!("Non-UTF-8 text data in atom: {}", e))
                        })?
                        .trim_end_matches('\0')
                        .to_string();
                    if !text.is_empty() {
                        values.push(text);
                    }
                } else {
                    // Invalid data type - return error so it gets stored in failed_atoms
                    return Err(AudexError::ParseError(format!(
                        "Invalid data type {} for text atom",
                        data_type
                    )));
                }
            }

            pos += length;
        }

        if !values.is_empty() {
            let key = name2key(name);
            // Append to existing values if key already exists (for multiple atoms with same name)
            self.tags.entry(key).or_default().extend(values);
        }

        Ok(())
    }

    /// Parse genre atom (converts ID3v1 genre to text)
    fn parse_genre_atom(&mut self, data: &[u8]) -> Result<()> {
        let mut values = Vec::new();
        let mut pos = 0;

        while pos + 12 <= data.len() {
            let length =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;

            if length < 18 || pos.checked_add(length).is_none_or(|end| end > data.len()) {
                break;
            }

            if &data[pos + 4..pos + 8] == b"data" && length >= 18 {
                let genre_id = u16::from_be_bytes([data[pos + 16], data[pos + 17]]);

                // Convert to freeform genre following standard format
                if genre_id > 0 && genre_id <= 255 {
                    // Use full genre list from constants module (genre_id is 1-based)
                    let genre_name =
                        crate::constants::get_genre((genre_id - 1) as u8).ok_or_else(|| {
                            AudexError::ParseError(format!("unknown genre id: {}", genre_id))
                        })?;
                    values.push(genre_name.to_string());
                } else {
                    return Err(AudexError::ParseError("invalid genre".to_string()));
                }
            }

            pos += length;
        }

        if !values.is_empty() {
            let key = crate::mp4::util::name2key(b"\xa9gen"); // Convert to ©gen
            self.tags.insert(key, values);
        }

        Ok(())
    }

    /// Parse integer atom with specific minimum byte requirements.
    /// Rejects atoms whose data payload is smaller than `min_bytes`.
    fn parse_integer_atom_bytes(
        &mut self,
        name: &[u8; 4],
        data: &[u8],
        min_bytes: usize,
    ) -> Result<()> {
        let mut values = Vec::new();
        let mut pos = 0;

        while pos + 12 <= data.len() {
            let length =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
                    as usize;

            if length < 17 || pos.checked_add(length).is_none_or(|end| end > data.len()) {
                return Err(AudexError::ParseError(
                    "invalid integer child atom length".to_string(),
                ));
            }

            if &data[pos + 4..pos + 8] == b"data" {
                // Check version and flags
                let version = data[pos + 8];
                let flags = u32::from_be_bytes([0, data[pos + 9], data[pos + 10], data[pos + 11]]);

                if version != 0 {
                    return Err(AudexError::ParseError("unsupported version".to_string()));
                }

                if flags != AtomDataType::Implicit as u32 && flags != AtomDataType::Integer as u32 {
                    return Err(AudexError::ParseError("unsupported type".to_string()));
                }

                let data_len = length - 16;

                // Reject atoms whose payload is smaller than the expected size
                if data_len < min_bytes {
                    return Err(AudexError::ParseError(format!(
                        "integer atom too small: got {} bytes, expected at least {}",
                        data_len, min_bytes
                    )));
                }

                let value = match data_len {
                    1 => (data[pos + 16] as i8) as i64,
                    2 => i16::from_be_bytes([data[pos + 16], data[pos + 17]]) as i64,
                    3 => {
                        let b0 = data[pos + 16];
                        let b1 = data[pos + 17];
                        let b2 = data[pos + 18];
                        // Sign-extend: replicate the high bit into the leading byte
                        let sign = if b0 & 0x80 != 0 { 0xFF } else { 0x00 };
                        i32::from_be_bytes([sign, b0, b1, b2]) as i64
                    }
                    4 => i32::from_be_bytes([
                        data[pos + 16],
                        data[pos + 17],
                        data[pos + 18],
                        data[pos + 19],
                    ]) as i64,
                    8 => i64::from_be_bytes([
                        data[pos + 16],
                        data[pos + 17],
                        data[pos + 18],
                        data[pos + 19],
                        data[pos + 20],
                        data[pos + 21],
                        data[pos + 22],
                        data[pos + 23],
                    ]),
                    _ => {
                        return Err(AudexError::ParseError(format!(
                            "invalid value size {}",
                            data_len
                        )));
                    }
                };

                values.push(value.to_string());
            }

            pos += length;
        }

        if !values.is_empty() {
            let key = crate::mp4::util::name2key(name);
            self.tags.insert(key, values);
        }

        Ok(())
    }

    /// Get the first value for a key
    pub fn get_first(&self, key: &str) -> Option<&String> {
        self.tags.get(key)?.first()
    }

    /// Set a single value for a key
    pub fn set_single(&mut self, key: &str, value: String) {
        self.tags.insert(key.to_string(), vec![value]);
    }

    /// Add a value to a key (supporting multiple values)
    pub fn add_value(&mut self, key: &str, value: String) {
        self.tags.entry(key.to_string()).or_default().push(value);
    }

    /// Get padding size
    pub fn padding(&self) -> usize {
        self.padding
    }

    /// Set padding size
    pub fn set_padding(&mut self, padding: usize) {
        self.padding = padding;
    }

    /// Save tags to an MP4 file using in-place modification
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        debug_event!("saving MP4 tags");
        let path = path.as_ref();

        // Open file for reading and writing
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(AudexError::Io)?;

        // Load file structure
        let atoms = Atoms::parse(&mut file)
            .map_err(|e| AudexError::ParseError(format!("Failed to parse MP4 atoms: {}", e)))?;

        // Render new ilst data
        let new_ilst_data = self
            .render_ilst()
            .map_err(|e| AudexError::ParseError(format!("Failed to render metadata: {}", e)))?;

        trace_event!(ilst_bytes = new_ilst_data.len(), "rendered MP4 ilst atom");

        // Try to find existing ilst path
        if let Some(ilst_path) = atoms.path("moov.udta.meta.ilst") {
            self.save_existing(&mut file, &atoms, &ilst_path, &new_ilst_data)?;
        } else {
            self.save_new(&mut file, &atoms, &new_ilst_data)?;
        }

        Ok(())
    }

    /// Save tags to a writer that implements Read + Write + Seek.
    ///
    /// This performs the same operation as [`save`](Self::save) but operates on
    /// an in-memory writer (or any `Read + Write + Seek` implementor) instead of
    /// opening a file by path. The writer must contain the complete original
    /// MP4 file data.
    ///
    /// For trait-object writers (`dyn ReadWriteSeek`), stale trailing bytes
    /// are zeroed but the writer retains its original size. Use
    /// [`save_to`](Self::save_to) with a concrete type to get physical
    /// truncation when the writer supports it.
    pub fn save_to_writer(&self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        let result = self.render_to_vec(writer)?;
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&result)?;

        // Zero-fill stale trailing bytes (trait-object writers cannot be
        // physically truncated).
        let written_end = writer.stream_position()?;
        crate::util::truncate_writer_dyn(writer, written_end)?;

        Ok(())
    }

    /// Save tags to a concrete writer type, with physical truncation when
    /// the writer supports it (e.g. `File`, `Cursor<Vec<u8>>`).
    ///
    /// This is the preferred method when the concrete writer type is known
    /// at compile time. For trait-object writers, use
    /// [`save_to_writer`](Self::save_to_writer).
    pub fn save_to<W: Read + Write + Seek + std::any::Any>(&self, writer: &mut W) -> Result<()> {
        let result = self.render_to_vec(writer)?;
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&result)?;

        // Physically truncate the writer if the concrete type supports it;
        // fall back to zeroing for unknown types.
        let written_end = writer.stream_position()?;
        crate::util::truncate_writer(writer, written_end)?;

        Ok(())
    }

    /// Read the writer contents, apply metadata changes in a Cursor, and
    /// return the correctly-sized output bytes.
    ///
    /// # Memory usage
    ///
    /// This method reads the entire file into an in-memory `Vec`, then
    /// performs atom rewriting on a `Cursor` over that buffer. Peak memory
    /// consumption is approximately **2x the file size** (original buffer
    /// plus the modified copy). A size guard rejects files larger than
    /// `crate::limits::MAX_IN_MEMORY_WRITER_FILE` before allocating. For large MP4
    /// files, prefer the file-path-based [`save`](Self::save) method which
    /// operates directly on the file handle and avoids buffering the entire
    /// stream in memory.
    fn render_to_vec(&self, writer: &mut dyn crate::ReadWriteSeek) -> Result<Vec<u8>> {
        // Read the entire content into memory so we can operate on a Cursor,
        // which satisfies the Sized + 'static bounds required by the internal
        // save helpers (resize_bytes, insert_bytes, etc.).

        // Guard against OOM before allocating the full read buffer. Use a
        // dedicated whole-file ceiling so large audio payloads with small tags
        // are still accepted by writer-based operations.
        let file_size = writer.seek(SeekFrom::End(0))?;
        let max_read_size = crate::limits::MAX_IN_MEMORY_WRITER_FILE;
        if file_size > max_read_size {
            return Err(AudexError::InvalidData(format!(
                "File size ({} bytes) exceeds maximum for in-memory MP4 save ({} bytes)",
                file_size, max_read_size
            )));
        }

        writer.seek(SeekFrom::Start(0))?;
        let mut data = Vec::new();
        writer.read_to_end(&mut data)?;
        let mut cursor = Cursor::new(data);

        // Load file structure
        let atoms = Atoms::parse(&mut cursor)
            .map_err(|e| AudexError::ParseError(format!("Failed to parse MP4 atoms: {}", e)))?;

        // Render new ilst data
        let new_ilst_data = self
            .render_ilst()
            .map_err(|e| AudexError::ParseError(format!("Failed to render metadata: {}", e)))?;

        // Try to find existing ilst path
        if let Some(ilst_path) = atoms.path("moov.udta.meta.ilst") {
            self.save_existing(&mut cursor, &atoms, &ilst_path, &new_ilst_data)?;
        } else {
            self.save_new(&mut cursor, &atoms, &new_ilst_data)?;
        }

        Ok(cursor.into_inner())
    }

    /// Save existing ilst atom using save_existing pattern
    fn save_existing<F: Read + Write + Seek + 'static>(
        &self,
        file: &mut F,
        atoms: &Atoms,
        ilst_path: &[&MP4Atom],
        new_ilst_data: &[u8],
    ) -> Result<()> {
        let ilst = ilst_path
            .last()
            .ok_or_else(|| AudexError::ParseError("Empty atom path for ilst".to_string()))?;
        let mut offset = ilst.offset;
        let mut length = ilst.length;

        // Use adjacent free atom if there is one ()
        let free_atom = self.find_padding(ilst_path);
        if let Some(free) = free_atom {
            offset = std::cmp::min(offset, free.offset);
            length = length.checked_add(free.length).ok_or_else(|| {
                AudexError::InvalidData("ilst + free atom length overflows u64".to_string())
            })?;
        }

        // Always add a padding atom to make things easier
        let padding_overhead = 8; // len(Atom.render(b"free", b""))

        // Validate length against file size to prevent OOM from crafted atom sizes
        let file_size = file.seek(SeekFrom::End(0))?;
        file.seek(SeekFrom::Start(0))?;
        let safe_length = std::cmp::min(length, file_size);

        // Use PaddingInfo for smart padding calculation
        // Note: ilst_header_size accounts for the 8-byte ilst atom header that wraps new_ilst_data
        let ilst_header_size: i64 = 8;

        // Safe conversions — reject files whose sizes exceed i64::MAX to
        // prevent silent wrapping of the padding/offset arithmetic
        let file_size_i64 = i64::try_from(file_size).map_err(|_| {
            crate::AudexError::InvalidData("file size exceeds maximum supported value".into())
        })?;
        let offset_i64 = i64::try_from(offset).map_err(|_| {
            crate::AudexError::InvalidData("atom offset exceeds maximum supported value".into())
        })?;
        let safe_length_i64 = i64::try_from(safe_length).map_err(|_| {
            crate::AudexError::InvalidData("atom length exceeds maximum supported value".into())
        })?;

        // Clamp to zero: a malformed atom whose offset + length exceeds
        // the file size would produce a negative value.
        let content_size = (file_size_i64 - (offset_i64 + safe_length_i64)).max(0);
        let padding_size =
            safe_length_i64 - (new_ilst_data.len() as i64 + ilst_header_size + padding_overhead);
        let info = PaddingInfo::new(padding_size, content_size);
        // Safe conversion: clamp padding to a sane maximum (10 MB) and use
        // try_from to avoid wrapping on 32-bit targets.
        const MAX_PADDING: usize = 10 * 1024 * 1024;
        let new_padding = usize::try_from(info.get_default_padding().max(0))
            .unwrap_or(0)
            .min(MAX_PADDING);

        // Build the data to write: ilst data + free atom
        // new_ilst_data already contains the individual atoms, we need to wrap in ilst header
        let ilst_atom = MP4Atom::render(b"ilst", new_ilst_data)?;
        let mut final_data = ilst_atom;

        if new_padding > 0 {
            let free_data = vec![0u8; new_padding];
            let free_atom = MP4Atom::render(b"free", &free_data)?;
            final_data.extend_from_slice(&free_atom);
        }

        // Use the clamped length so the resize never exceeds the actual file size
        resize_bytes(file, safe_length, final_data.len() as u64, offset)?;
        let delta = i64::try_from(final_data.len()).map_err(|_| {
            crate::AudexError::InvalidData(
                "output data size exceeds maximum supported value".into(),
            )
        })? - safe_length_i64;

        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&final_data)?;

        // Update parent atoms - pass path[:-1] (exclude ilst itself)
        self.update_parents(file, &ilst_path[..ilst_path.len() - 1], delta, offset)?;

        // Update offset tables.
        // Note: `resize_bytes` invalidates the cached `Atoms` tree because
        // atom offsets and lengths have shifted. The `update_offsets` call
        // below compensates by applying `delta` to every stco/co64 entry
        // that sits beyond `offset`. This works correctly for the single-
        // resize case (which is the only case this code path exercises).
        // Multiple sequential resizes against the same `atoms` snapshot
        // would require re-parsing the atom tree between each resize.
        if delta == 0 && safe_length != final_data.len() as u64 {
            return Err(AudexError::InvalidOperation(
                "MP4 save encountered an unexpected resize state".to_string(),
            ));
        }
        self.update_offsets(file, atoms, delta, offset)?;

        Ok(())
    }

    /// Save new ilst atom when none exists
    fn save_new<F: Read + Write + Seek + 'static>(
        &self,
        file: &mut F,
        atoms: &Atoms,
        new_ilst_data: &[u8],
    ) -> Result<()> {
        // Create hdlr atom for meta
        let hdlr = MP4Atom::render(
            b"hdlr",
            &[
                0, 0, 0, 0, // version/flags
                0, 0, 0, 0, // pre-defined
                b'm', b'd', b'i', b'r', // handler_type
                b'a', b'p', b'p', b'l', // component manufacturer
                0, 0, 0, 0, 0, 0, 0, 0, 0, // reserved + name
            ],
        )?;

        // Reject empty ilst data early -- writing an empty ilst atom is invalid
        if new_ilst_data.is_empty() {
            return Err(AudexError::InvalidData(
                "Cannot save empty metadata to MP4".to_string(),
            ));
        }

        let ilst_atom = MP4Atom::render(b"ilst", new_ilst_data)?;
        let meta_data = [&[0u8; 4], &hdlr[..], &ilst_atom[..]].concat(); // version/flags + hdlr + ilst

        let path = if let Some(udta_path) = atoms.path("moov.udta") {
            udta_path
        } else {
            atoms
                .path("moov")
                .ok_or_else(|| AudexError::ParseError("No moov atom found".to_string()))?
        };

        let last_atom = path
            .last()
            .ok_or_else(|| AudexError::ParseError("Empty atom path for moov".to_string()))?;
        // Skip past the atom header to reach the payload. The header size is
        // derived from the atom's own fields (data_offset - offset), which
        // correctly handles both standard 8-byte and extended 16-byte headers.
        let header_size = last_atom
            .data_offset
            .checked_sub(last_atom.offset)
            .ok_or_else(|| {
                AudexError::ParseError("atom data_offset is before atom start".to_string())
            })?;
        let offset = last_atom.offset.checked_add(header_size).ok_or_else(|| {
            AudexError::ParseError("moov atom offset too large to add header size".to_string())
        })?;

        // Use PaddingInfo for smart padding calculation
        let file_size = file.seek(SeekFrom::End(0)).map_err(AudexError::Io)?;
        let content_size = file_size.checked_sub(offset).ok_or_else(|| {
            AudexError::ParseError(
                "moov offset exceeds file size, file may be truncated".to_string(),
            )
        })?;
        // Safe conversions: reject values exceeding i64::MAX to match the
        // validation used in save_existing and prevent silent wrapping
        let content_size_i64 = i64::try_from(content_size).map_err(|_| {
            AudexError::InvalidData(
                "content size exceeds maximum supported value for padding calculation".to_string(),
            )
        })?;
        let meta_data_len_i64 = i64::try_from(meta_data.len()).map_err(|_| {
            AudexError::InvalidData("metadata size exceeds maximum supported value".to_string())
        })?;
        let padding_size = -meta_data_len_i64;
        let info = PaddingInfo::new(padding_size, content_size_i64);
        // Safe conversion: clamp padding to a sane maximum (10 MB) and use
        // try_from to avoid wrapping on 32-bit targets.
        const MAX_PADDING: usize = 10 * 1024 * 1024;
        let new_padding = usize::try_from(info.get_default_padding().max(0))
            .unwrap_or(0)
            .min(MAX_PADDING);

        let free_atom = if new_padding > 0 {
            MP4Atom::render(b"free", &vec![0u8; new_padding])?
        } else {
            Vec::new()
        };

        let meta = MP4Atom::render(b"meta", &[&meta_data[..], &free_atom[..]].concat())?;

        let data = if last_atom.name != *b"udta" {
            // moov.udta not found -- create one
            MP4Atom::render(b"udta", &meta)?
        } else {
            meta
        };

        // Use insert_bytes for new metadata ()
        insert_bytes(file, data.len() as u64, offset, None)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&data)?;

        // Update parents and offsets with safe size conversion
        let data_len_i64 = i64::try_from(data.len()).map_err(|_| {
            AudexError::InvalidData(
                "inserted data size exceeds maximum supported value".to_string(),
            )
        })?;
        self.update_parents(file, &path, data_len_i64, offset)?;
        self.update_offsets(file, atoms, data_len_i64, offset)?;

        Ok(())
    }

    /// Find adjacent free atom for padding
    fn find_padding<'a>(&self, ilst_path: &[&'a MP4Atom]) -> Option<&'a MP4Atom> {
        if ilst_path.len() < 2 {
            return None;
        }

        let meta = ilst_path[ilst_path.len() - 2]; // parent of ilst should be meta
        if let Some(children) = &meta.children {
            for (i, child) in children.iter().enumerate() {
                if child.name == *b"ilst" {
                    // Check previous sibling for free atom
                    if i > 0 && children[i - 1].name == *b"free" {
                        return Some(&children[i - 1]);
                    }
                    // Check next sibling for free atom
                    if i + 1 < children.len() && children[i + 1].name == *b"free" {
                        return Some(&children[i + 1]);
                    }
                    break;
                }
            }
        }

        None
    }

    /// Update parent atoms with new size.
    ///
    /// When data is inserted or removed at `resize_offset`, any parent atom
    /// whose original offset lies beyond that point has already been shifted
    /// in the file. We must adjust the seek position by `delta` before
    /// reading/writing, mirroring the correction applied in
    /// `update_offset_table`.
    fn update_parents<F: Read + Write + Seek>(
        &self,
        file: &mut F,
        path: &[&MP4Atom],
        delta: i64,
        resize_offset: u64,
    ) -> Result<()> {
        if delta == 0 {
            return Ok(());
        }

        for atom in path {
            // Adjust for data that shifted after the resize point
            let mut actual_offset = atom.offset;
            if actual_offset > resize_offset {
                actual_offset = (actual_offset as i64)
                    .checked_add(delta)
                    .filter(|&v| v >= 0)
                    .ok_or_else(|| {
                        AudexError::ParseError(format!(
                            "Parent atom offset underflow: {} + {}",
                            atom.offset, delta
                        ))
                    })? as u64;
            }

            file.seek(SeekFrom::Start(actual_offset))?;
            let mut size_bytes = [0u8; 4];
            file.read_exact(&mut size_bytes)?;
            let size = u32::from_be_bytes(size_bytes);

            if size == 1 {
                // 64-bit extended size
                file.seek(SeekFrom::Start(actual_offset + 8))?;
                let mut size_bytes = [0u8; 8];
                file.read_exact(&mut size_bytes)?;
                let size = u64::from_be_bytes(size_bytes);
                // Safe conversion: reject sizes that exceed i64::MAX to
                // prevent silent wrapping to negative values
                let size_i64 = i64::try_from(size).map_err(|_| {
                    AudexError::ParseError(format!(
                        "64-bit atom size {} exceeds maximum representable value",
                        size
                    ))
                })?;
                // Use checked arithmetic to prevent overflow corruption
                let new_size =
                    size_i64
                        .checked_add(delta)
                        .filter(|&s| s >= 8)
                        .ok_or_else(|| {
                            AudexError::ParseError(format!(
                                "Atom size overflow: {} + {} produces invalid size",
                                size, delta
                            ))
                        })?;
                file.seek(SeekFrom::Start(actual_offset + 8))?;
                file.write_all(&new_size.to_be_bytes())?;
            } else {
                // 32-bit size — use checked arithmetic to prevent truncation
                let new_size = (size as i64)
                    .checked_add(delta)
                    .filter(|&s| s >= 8 && s <= u32::MAX as i64)
                    .ok_or_else(|| {
                        AudexError::ParseError(format!(
                            "Atom size overflow: {} + {} produces invalid 32-bit size",
                            size, delta
                        ))
                    })? as u32;
                file.seek(SeekFrom::Start(actual_offset))?;
                file.write_all(&new_size.to_be_bytes())?;
            }
        }

        Ok(())
    }

    /// Update offset tables after modifying metadata
    fn update_offsets<F: Read + Write + Seek>(
        &self,
        file: &mut F,
        atoms: &Atoms,
        delta: i64,
        offset: u64,
    ) -> Result<()> {
        if delta == 0 {
            return Ok(());
        }

        // Find moov atom and update stco/co64 offset tables within it
        if let Some(moov) = atoms.atoms.iter().find(|a| a.name == *b"moov") {
            // Update stco (32-bit chunk offset) tables
            for atom in moov.findall(b"stco", true) {
                self.update_offset_table(file, atom, delta, offset, false)?;
            }
            // Update co64 (64-bit chunk offset) tables
            for atom in moov.findall(b"co64", true) {
                self.update_offset_table(file, atom, delta, offset, true)?;
            }
        }

        // Update tfhd atoms in moof (fragmented MP4)
        if let Some(moof) = atoms.atoms.iter().find(|a| a.name == *b"moof") {
            for atom in moof.findall(b"tfhd", true) {
                self.update_tfhd(file, atom, delta, offset)?;
            }
        }

        Ok(())
    }

    /// Update a single stco or co64 offset table
    fn update_offset_table<F: Read + Write + Seek>(
        &self,
        file: &mut F,
        atom: &crate::mp4::atom::MP4Atom,
        delta: i64,
        offset: u64,
        is_64bit: bool,
    ) -> Result<()> {
        let mut atom_offset = atom.offset;
        if atom_offset > offset {
            // Use checked arithmetic to prevent underflow
            atom_offset = (atom_offset as i64)
                .checked_add(delta)
                .filter(|&v| v >= 0)
                .ok_or_else(|| {
                    AudexError::ParseError(format!(
                        "Offset table atom position underflow: {} + {}",
                        atom_offset, delta
                    ))
                })? as u64;
        }

        // Read offset count (skip 8-byte atom header + 4-byte version/flags)
        file.seek(std::io::SeekFrom::Start(atom_offset + 12))
            .map_err(|e| AudexError::ParseError(format!("Seek failed: {}", e)))?;

        let mut count_buf = [0u8; 4];
        file.read_exact(&mut count_buf)
            .map_err(|e| AudexError::ParseError(format!("Read failed: {}", e)))?;
        // Safe to cast: validated against atom capacity below, which bounds
        // it well within addressable memory on any platform.
        let count = u32::from_be_bytes(count_buf) as usize;

        // Validate entry count against the atom's declared size.
        // The atom payload after the 8-byte header, 4-byte version/flags,
        // and 4-byte count field must hold count * entry_size bytes.
        let entry_size = if is_64bit { 8 } else { 4 };
        let header_overhead: u64 = 16; // 8 (atom header) + 4 (version/flags) + 4 (count)
        let max_data_bytes = atom.length.saturating_sub(header_overhead);
        let max_entries = max_data_bytes / entry_size as u64;
        if (count as u64) > max_entries {
            return Err(AudexError::ParseError(format!(
                "Offset table entry count ({}) exceeds atom capacity ({} entries fit in {} bytes)",
                count, max_entries, max_data_bytes
            )));
        }

        let alloc_size = count.checked_mul(entry_size).ok_or_else(|| {
            AudexError::ParseError(format!(
                "Offset table allocation overflow: {} entries * {} bytes",
                count, entry_size
            ))
        })?;
        let mut data = vec![0u8; alloc_size];
        file.read_exact(&mut data)
            .map_err(|e| AudexError::ParseError(format!("Read offsets failed: {}", e)))?;

        // Update offsets that are after the modification point
        for i in 0..count {
            let pos = i * entry_size;
            if is_64bit {
                let o = u64::from_be_bytes([
                    data[pos],
                    data[pos + 1],
                    data[pos + 2],
                    data[pos + 3],
                    data[pos + 4],
                    data[pos + 5],
                    data[pos + 6],
                    data[pos + 7],
                ]);
                if o > offset {
                    // Safe conversion: reject offsets that exceed i64::MAX
                    // to prevent silent wrapping to negative values
                    let o_i64 = i64::try_from(o).map_err(|_| {
                        AudexError::ParseError(format!(
                            "64-bit chunk offset {} exceeds maximum representable value",
                            o
                        ))
                    })?;
                    // Use checked arithmetic to prevent underflow wrapping
                    let new_o = o_i64
                        .checked_add(delta)
                        .filter(|&v| v >= 0)
                        .ok_or_else(|| {
                            AudexError::ParseError(format!(
                                "Chunk offset underflow: {} + {} is negative",
                                o, delta
                            ))
                        })? as u64;
                    data[pos..pos + 8].copy_from_slice(&new_o.to_be_bytes());
                }
            } else {
                let o =
                    u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
                if (o as u64) > offset {
                    // Use checked arithmetic to prevent underflow wrapping
                    let new_o = (o as i64)
                        .checked_add(delta)
                        .filter(|&v| v >= 0 && v <= u32::MAX as i64)
                        .ok_or_else(|| {
                            AudexError::ParseError(format!(
                                "Chunk offset underflow: {} + {} is invalid for 32-bit",
                                o, delta
                            ))
                        })? as u32;
                    data[pos..pos + 4].copy_from_slice(&new_o.to_be_bytes());
                }
            }
        }

        // Write updated offsets back
        file.seek(std::io::SeekFrom::Start(atom_offset + 16))
            .map_err(|e| AudexError::ParseError(format!("Seek failed: {}", e)))?;
        file.write_all(&data)
            .map_err(|e| AudexError::ParseError(format!("Write offsets failed: {}", e)))?;

        Ok(())
    }

    /// Update tfhd atom's base_data_offset in fragmented MP4
    fn update_tfhd<F: Read + Write + Seek>(
        &self,
        file: &mut F,
        atom: &crate::mp4::atom::MP4Atom,
        delta: i64,
        offset: u64,
    ) -> Result<()> {
        let mut atom_offset = atom.offset;
        if atom_offset > offset {
            // Use checked arithmetic to prevent underflow
            atom_offset = (atom_offset as i64)
                .checked_add(delta)
                .filter(|&v| v >= 0)
                .ok_or_else(|| {
                    AudexError::ParseError(format!(
                        "tfhd atom offset underflow: {} + {}",
                        atom_offset, delta
                    ))
                })? as u64;
        }

        // Read version(1) + flags(3) starting at offset+8 (after header)
        file.seek(std::io::SeekFrom::Start(atom_offset + 9))
            .map_err(|e| AudexError::ParseError(format!("Seek failed: {}", e)))?;

        let atom_len = usize::try_from(atom.length).map_err(|_| {
            AudexError::ParseError(format!(
                "Atom length {} exceeds addressable range",
                atom.length
            ))
        })?;
        if atom_len < 12 {
            return Err(AudexError::ParseError(format!(
                "tfhd atom too short for version and flags: {} bytes",
                atom.length
            )));
        }
        let alloc_size = atom_len - 9;
        // Guard against oversized allocations from crafted atom lengths
        let limits = crate::limits::ParseLimits::default();
        if alloc_size as u64 > limits.max_tag_size {
            return Err(AudexError::ParseError(format!(
                "tfhd atom data size ({} bytes) exceeds maximum allowed ({})",
                alloc_size, limits.max_tag_size
            )));
        }
        let mut data = vec![0u8; alloc_size];
        file.read_exact(&mut data)
            .map_err(|e| AudexError::ParseError(format!("Read tfhd failed: {}", e)))?;

        // flags are first 3 bytes (big-endian 24-bit)
        let flags = u32::from_be_bytes([0, data[0], data[1], data[2]]);

        // base-data-offset-present flag (bit 0)
        if flags & 1 != 0 {
            // base_data_offset is at data[7..15] (after flags(3) + track_id(4))
            if data.len() >= 15 {
                let o = u64::from_be_bytes([
                    data[7], data[8], data[9], data[10], data[11], data[12], data[13], data[14],
                ]);
                if o > offset {
                    // Safe conversion: reject offsets that exceed i64::MAX
                    // to prevent silent wrapping to negative values
                    let o_i64 = i64::try_from(o).map_err(|_| {
                        AudexError::ParseError(format!(
                            "64-bit tfhd base_data_offset {} exceeds maximum representable value",
                            o
                        ))
                    })?;
                    // Use checked arithmetic to prevent underflow wrapping
                    let new_o = o_i64
                        .checked_add(delta)
                        .filter(|&v| v >= 0)
                        .ok_or_else(|| {
                            AudexError::ParseError(format!(
                                "tfhd base_data_offset underflow: {} + {}",
                                o, delta
                            ))
                        })? as u64;
                    file.seek(std::io::SeekFrom::Start(atom_offset + 16))
                        .map_err(|e| AudexError::ParseError(format!("Seek failed: {}", e)))?;
                    file.write_all(&new_o.to_be_bytes())
                        .map_err(|e| AudexError::ParseError(format!("Write tfhd failed: {}", e)))?;
                }
            }
        }

        Ok(())
    }

    /// Render ilst atom data containing all metadata with iTunes-compatible ordering.
    /// Only sets the vendor tag if not already present in the tags.
    ///
    /// This method creates the ilst atom data that contains all metadata tags
    /// in iTunes-compatible order for optimal compatibility with media players.
    pub fn render_ilst(&self) -> Result<Vec<u8>> {
        let mut ilst_data = Vec::new();

        // Set the vendor tag only if the user hasn't explicitly set a custom value.
        // This preserves user-provided encoding software metadata on round-trip.
        let mut tags_with_vendor = self.tags.clone();
        let has_content =
            !tags_with_vendor.is_empty() || !self.freeforms.is_empty() || !self.covers.is_empty();
        if has_content && !tags_with_vendor.contains_key("©too") {
            tags_with_vendor.insert(
                "©too".to_string(),
                vec![format!("Audex {}", crate::VERSION_STRING)],
            );
        }

        // Define iTunes-compatible atom ordering for optimal compatibility
        let itunes_order = [
            "©nam", "©ART", "aART", "©wrt", "©alb", "©gen", "gnre", "©day", "trkn", "disk", "©cmt",
            "©lyr", "©grp", "©too", "cpil", "pgap", "pcst", "tmpo", "stik", "hdvd", "rtng",
            "covr", // Cover art gets special position
            "catg", "keyw", "purd", "purl", "egid", "soal", "soaa", "soar", "sonm", "soco", "sosn",
            "tvsh", "©mvn", "©wrk", // Integer atoms with specific ordering
            "plID", "cnID", "geID", "atID", "sfID", "cmID", "akID", "tvsn", "tves", "©mvi", "©mvc",
            "shwm",
        ];

        // Track keys that fail to render so we can report them
        let mut skipped_keys: Vec<String> = Vec::new();

        // Add standard metadata atoms in iTunes-compatible order
        for ordered_key in &itunes_order {
            if *ordered_key == "covr" {
                // Handle cover artwork at its designated position
                if !self.covers.is_empty() {
                    let cover_data = self.render_cover_atom()?;
                    ilst_data.extend_from_slice(&cover_data);
                }
            } else if let Some(values) = tags_with_vendor.get(*ordered_key) {
                match self.render_metadata_atom(ordered_key, values) {
                    Ok(atom_data) => ilst_data.extend_from_slice(&atom_data),
                    Err(_e) => {
                        warn_event!("Failed to render metadata key '{}': {}", ordered_key, _e);
                        skipped_keys.push(ordered_key.to_string());
                    }
                }
            }
        }

        // Add any remaining standard tags not in the order list, sorted for deterministic output
        let mut remaining_keys: Vec<&String> = tags_with_vendor
            .keys()
            .filter(|k| !itunes_order.contains(&k.as_str()))
            .collect();
        remaining_keys.sort();
        for key in remaining_keys {
            let values = &tags_with_vendor[key];
            match self.render_metadata_atom(key, values) {
                Ok(atom_data) => ilst_data.extend_from_slice(&atom_data),
                Err(_e) => {
                    warn_event!("Failed to render metadata key '{}': {}", key, _e);
                    skipped_keys.push(key.clone());
                }
            }
        }

        // Add freeform metadata at the end (sorted by key for consistency)
        let mut freeform_keys: Vec<_> = self.freeforms.keys().collect();
        freeform_keys.sort();
        for key in freeform_keys {
            if let Some(values) = self.freeforms.get(key) {
                let freeform_data = self.render_freeform_atom(key, values)?;
                ilst_data.extend_from_slice(&freeform_data);
            }
        }

        // Write back failed (unknown) atoms that we couldn't parse,
        // unless we now have a recognized atom with the same key.
        // Sort keys for deterministic output, matching the freeform ordering above.
        let mut failed_keys: Vec<_> = self.failed_atoms.keys().collect();
        failed_keys.sort();
        for key in failed_keys {
            let data_list = &self.failed_atoms[key];
            // Skip if we already have a recognized tag with this key
            // (freeform atoms with key "----" can have duplicates, so always write those back)
            let name_bytes = crate::mp4::util::key2name(key)?;
            if name_bytes != b"----" && self.tags.contains_key(key) {
                continue;
            }
            for data in data_list {
                if name_bytes.len() == 4 {
                    let name = [name_bytes[0], name_bytes[1], name_bytes[2], name_bytes[3]];
                    let atom_data = MP4Atom::render(&name, data)?;
                    ilst_data.extend_from_slice(&atom_data);
                }
            }
        }

        // Refuse to silently lose metadata — report all keys that failed to render
        if !skipped_keys.is_empty() {
            return Err(AudexError::InvalidData(format!(
                "failed to render {} metadata tag(s): {}",
                skipped_keys.len(),
                skipped_keys.join(", ")
            )));
        }

        Ok(ilst_data)
    }

    /// Render a standard metadata atom
    fn render_metadata_atom(&self, key: &str, values: &[String]) -> Result<Vec<u8>> {
        let name_bytes = crate::mp4::util::key2name(key)?;
        if name_bytes.len() != 4 {
            return Err(AudexError::ParseError(format!(
                "Invalid key length: {}",
                key
            )));
        }
        let name = [name_bytes[0], name_bytes[1], name_bytes[2], name_bytes[3]];

        let mut atom_data = Vec::new();

        for value in values {
            let data_atom = match key {
                "trkn" | "disk" => self.render_pair_data(&name, value)?,
                "cpil" | "pgap" | "pcst" => self.render_bool_data(&name, value)?,
                "gnre" => self.render_genre_data(value)?,
                key if self.is_integer_atom(key) => self.render_integer_data(&name, value)?,
                _ => self.render_text_data(&name, value)?,
            };
            atom_data.extend_from_slice(&data_atom);
        }

        MP4Atom::render(&name, &atom_data)
    }

    /// Check if an atom should be rendered as integer
    fn is_integer_atom(&self, key: &str) -> bool {
        matches!(
            key,
            "plID"
                | "cnID"
                | "geID"
                | "atID"
                | "sfID"
                | "cmID"
                | "tvsn"
                | "tves"
                | "tmpo"
                | "©mvi"
                | "©mvc"
                | "akID"
                | "shwm"
                | "stik"
                | "hdvd"
                | "rtng"
        )
    }

    /// Render text data atom
    fn render_text_data(&self, _name: &[u8; 4], value: &str) -> Result<Vec<u8>> {
        let text_bytes = value.as_bytes();
        let mut data = Vec::new();

        // data atom header — validate total size fits in u32
        let atom_size = u32::try_from(text_bytes.len() + 16)
            .map_err(|_| AudexError::InvalidData("Text data too large for MP4 atom".to_string()))?;
        data.extend_from_slice(&atom_size.to_be_bytes());
        data.extend_from_slice(b"data");
        data.push(0); // version
        data.extend_from_slice(&(AtomDataType::Utf8 as u32).to_be_bytes()[1..4]); // flags
        data.extend_from_slice(&[0u8; 4]); // reserved
        data.extend_from_slice(text_bytes);

        Ok(data)
    }

    /// Render integer data atom
    fn render_integer_data(&self, name: &[u8; 4], value: &str) -> Result<Vec<u8>> {
        let int_value: i64 = value
            .parse()
            .map_err(|_| AudexError::ParseError(format!("Invalid integer: {}", value)))?;

        // Minimum byte size per atom type, matching iTunes convention
        let min_bytes: u8 = match name {
            b"plID" => 8,
            b"cnID" | b"geID" | b"atID" | b"sfID" | b"cmID" | b"tvsn" | b"tves" => 4,
            b"tmpo" | b"\xa9mvi" | b"\xa9mvc" => 2,
            _ => 1, // akID, shwm, stik, hdvd, rtng, etc.
        };

        // Adaptively select the smallest size that fits the value, respecting min_bytes
        let bytes = if (-128..=127).contains(&int_value) && min_bytes <= 1 {
            vec![int_value as u8]
        } else if (-32768..=32767).contains(&int_value) && min_bytes <= 2 {
            (int_value as i16).to_be_bytes().to_vec()
        } else if (-2147483648..=2147483647).contains(&int_value) && min_bytes <= 4 {
            (int_value as i32).to_be_bytes().to_vec()
        } else if min_bytes <= 8 {
            int_value.to_be_bytes().to_vec()
        } else {
            return Err(AudexError::ParseError(format!(
                "Integer value {} out of range for atom {:?}",
                int_value,
                std::str::from_utf8(name).unwrap_or("????")
            )));
        };

        let mut data = Vec::new();
        let atom_size = u32::try_from(bytes.len() + 16).map_err(|_| {
            AudexError::InvalidData("Integer data too large for MP4 atom".to_string())
        })?;
        data.extend_from_slice(&atom_size.to_be_bytes());
        data.extend_from_slice(b"data");
        data.push(0); // version
        data.extend_from_slice(&(AtomDataType::Integer as u32).to_be_bytes()[1..4]); // flags
        data.extend_from_slice(&[0u8; 4]); // reserved
        data.extend_from_slice(&bytes);

        Ok(data)
    }

    /// Render pair data atom (track/disk numbers)
    ///
    /// trkn uses 8-byte payload: [2 pad][2 track][2 total][2 trailing pad]
    /// disk uses 6-byte payload: [2 pad][2 disc][2 total]
    fn render_pair_data(&self, name: &[u8; 4], value: &str) -> Result<Vec<u8>> {
        let parts: Vec<&str> = value.split('/').collect();
        let track: u16 = parts[0]
            .parse()
            .map_err(|_| AudexError::ParseError(format!("Invalid track number: {}", parts[0])))?;
        let total: u16 = if parts.len() > 1 {
            parts[1]
                .parse()
                .map_err(|_| AudexError::ParseError(format!("Invalid total: {}", parts[1])))?
        } else {
            0
        };

        // trkn has 2-byte trailing padding, disk does not
        let has_trailing_pad = name == b"trkn";
        let payload_size: u32 = if has_trailing_pad { 8 } else { 6 };

        let mut data = Vec::new();
        data.extend_from_slice(&(16 + payload_size).to_be_bytes());
        data.extend_from_slice(b"data");
        data.push(0); // version
        data.extend_from_slice(&(AtomDataType::Implicit as u32).to_be_bytes()[1..4]); // flags
        data.extend_from_slice(&[0u8; 4]); // reserved
        data.extend_from_slice(&[0u8; 2]); // padding
        data.extend_from_slice(&track.to_be_bytes());
        data.extend_from_slice(&total.to_be_bytes());
        if has_trailing_pad {
            data.extend_from_slice(&[0u8; 2]);
        }

        Ok(data)
    }

    /// Render boolean data atom
    fn render_bool_data(&self, _name: &[u8; 4], value: &str) -> Result<Vec<u8>> {
        let bool_value = match value.to_lowercase().as_str() {
            "true" | "1" | "yes" => 1u8,
            "false" | "0" | "no" => 0u8,
            _ => {
                return Err(AudexError::ParseError(format!(
                    "Invalid boolean: {}",
                    value
                )));
            }
        };

        let mut data = Vec::new();
        data.extend_from_slice(&17u32.to_be_bytes()); // 16 + 1 byte of data
        data.extend_from_slice(b"data");
        data.push(0); // version
        data.extend_from_slice(&(AtomDataType::Integer as u32).to_be_bytes()[1..4]); // flags
        data.extend_from_slice(&[0u8; 4]); // reserved
        data.push(bool_value);

        Ok(data)
    }

    /// Render genre data atom (convert back to ID if possible)
    fn render_genre_data(&self, value: &str) -> Result<Vec<u8>> {
        self.render_text_data(b"\xa9gen", value)
    }

    /// Render cover artwork atom
    fn render_cover_atom(&self) -> Result<Vec<u8>> {
        let mut covr_data = Vec::new();

        for cover in &self.covers {
            let mut data = Vec::new();
            let atom_size = u32::try_from(cover.data.len() + 16).map_err(|_| {
                AudexError::InvalidData("Cover data too large for MP4 atom".to_string())
            })?;
            data.extend_from_slice(&atom_size.to_be_bytes());
            data.extend_from_slice(b"data");
            data.push(0); // version
            data.extend_from_slice(&(cover.imageformat as u32).to_be_bytes()[1..4]); // flags
            data.extend_from_slice(&[0u8; 4]); // reserved
            data.extend_from_slice(&cover.data);

            covr_data.extend_from_slice(&data);
        }

        MP4Atom::render(b"covr", &covr_data)
    }

    /// Render freeform metadata atom
    fn render_freeform_atom(&self, key: &str, values: &[MP4FreeForm]) -> Result<Vec<u8>> {
        // Parse freeform key (format: "----:mean:name")
        let parts: Vec<&str> = key.split(':').collect();
        if parts.len() != 3 || parts[0] != "----" {
            return Err(AudexError::ParseError(format!(
                "Invalid freeform key: {}",
                key
            )));
        }

        let mean = parts[1];
        let name = parts[2];

        let mut freeform_data = Vec::new();

        // mean atom — use checked conversion to prevent silent truncation
        let mean_bytes = mean.as_bytes();
        let mean_atom_size = u32::try_from(mean_bytes.len() + 12).map_err(|_| {
            AudexError::InvalidData("Freeform mean field too large for MP4 atom".to_string())
        })?;
        freeform_data.extend_from_slice(&mean_atom_size.to_be_bytes());
        freeform_data.extend_from_slice(b"mean");
        freeform_data.extend_from_slice(&[0u8; 4]); // version + flags
        freeform_data.extend_from_slice(mean_bytes);

        // name atom — use checked conversion to prevent silent truncation
        let name_bytes = name.as_bytes();
        let name_atom_size = u32::try_from(name_bytes.len() + 12).map_err(|_| {
            AudexError::InvalidData("Freeform name field too large for MP4 atom".to_string())
        })?;
        freeform_data.extend_from_slice(&name_atom_size.to_be_bytes());
        freeform_data.extend_from_slice(b"name");
        freeform_data.extend_from_slice(&[0u8; 4]); // version + flags
        freeform_data.extend_from_slice(name_bytes);

        // data atoms
        for value in values {
            let mut data = Vec::new();
            let atom_size = u32::try_from(value.data.len() + 16).map_err(|_| {
                AudexError::InvalidData("Freeform data too large for MP4 atom".to_string())
            })?;
            data.extend_from_slice(&atom_size.to_be_bytes());
            data.extend_from_slice(b"data");
            data.push(value.version); // version
            data.extend_from_slice(&(value.dataformat as u32).to_be_bytes()[1..4]); // flags
            data.extend_from_slice(&[0u8; 4]); // reserved
            data.extend_from_slice(&value.data);

            freeform_data.extend_from_slice(&data);
        }

        MP4Atom::render(b"----", &freeform_data)
    }
}

#[cfg(feature = "async")]
impl MP4Tags {
    /// Save tags to an MP4 file using native async I/O.
    pub async fn save_async<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();

        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .await
            .map_err(AudexError::Io)?;

        let atoms = Atoms::parse_async(&mut file)
            .await
            .map_err(|e| AudexError::ParseError(format!("Failed to parse MP4 atoms: {}", e)))?;

        let new_ilst_data = self
            .render_ilst()
            .map_err(|e| AudexError::ParseError(format!("Failed to render metadata: {}", e)))?;

        if let Some(ilst_path) = atoms.path("moov.udta.meta.ilst") {
            self.save_existing_async(&mut file, &atoms, &ilst_path, &new_ilst_data)
                .await?;
        } else {
            self.save_new_async(&mut file, &atoms, &new_ilst_data)
                .await?;
        }

        use tokio::io::AsyncWriteExt;
        file.flush().await.map_err(AudexError::Io)?;

        Ok(())
    }

    /// Async version of `save_existing`.
    async fn save_existing_async(
        &self,
        file: &mut tokio::fs::File,
        atoms: &Atoms,
        ilst_path: &[&MP4Atom],
        new_ilst_data: &[u8],
    ) -> Result<()> {
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};

        let ilst = ilst_path
            .last()
            .ok_or_else(|| AudexError::ParseError("Empty atom path for ilst".to_string()))?;
        let mut offset = ilst.offset;
        let mut length = ilst.length;

        let free_atom = self.find_padding(ilst_path);
        if let Some(free) = free_atom {
            offset = std::cmp::min(offset, free.offset);
            length = length.checked_add(free.length).ok_or_else(|| {
                AudexError::InvalidData("ilst + free atom length overflows u64".to_string())
            })?;
        }

        let padding_overhead = 8;

        let file_size = file.seek(SeekFrom::End(0)).await?;
        file.seek(SeekFrom::Start(0)).await?;
        let safe_length = std::cmp::min(length, file_size);

        let ilst_header_size: i64 = 8;

        // Safe conversions — reject files whose sizes exceed i64::MAX to
        // prevent silent wrapping of the padding/offset arithmetic
        let file_size_i64 = i64::try_from(file_size).map_err(|_| {
            crate::AudexError::InvalidData("file size exceeds maximum supported value".into())
        })?;
        let offset_i64 = i64::try_from(offset).map_err(|_| {
            crate::AudexError::InvalidData("atom offset exceeds maximum supported value".into())
        })?;
        let safe_length_i64 = i64::try_from(safe_length).map_err(|_| {
            crate::AudexError::InvalidData("atom length exceeds maximum supported value".into())
        })?;

        // Clamp to zero: a malformed atom whose offset + length exceeds
        // the file size would produce a negative value.
        let content_size = (file_size_i64 - (offset_i64 + safe_length_i64)).max(0);
        let padding_size =
            safe_length_i64 - (new_ilst_data.len() as i64 + ilst_header_size + padding_overhead);
        let info = PaddingInfo::new(padding_size, content_size);
        // Safe conversion: clamp padding to a sane maximum (10 MB) and use
        // try_from to avoid wrapping on 32-bit targets.
        const MAX_PADDING: usize = 10 * 1024 * 1024;
        let new_padding = usize::try_from(info.get_default_padding().max(0))
            .unwrap_or(0)
            .min(MAX_PADDING);

        let ilst_atom = MP4Atom::render(b"ilst", new_ilst_data)?;
        let mut final_data = ilst_atom;

        if new_padding > 0 {
            let free_data = vec![0u8; new_padding];
            let free_atom = MP4Atom::render(b"free", &free_data)?;
            final_data.extend_from_slice(&free_atom);
        }

        // Use the clamped length so the resize never exceeds the actual file size
        crate::util::resize_bytes_async(file, safe_length, final_data.len() as u64, offset).await?;
        let delta = i64::try_from(final_data.len()).map_err(|_| {
            crate::AudexError::InvalidData(
                "output data size exceeds maximum supported value".into(),
            )
        })? - safe_length_i64;

        file.seek(SeekFrom::Start(offset)).await?;
        file.write_all(&final_data).await?;

        self.update_parents_async(file, &ilst_path[..ilst_path.len() - 1], delta, offset)
            .await?;
        if delta == 0 && safe_length != final_data.len() as u64 {
            return Err(AudexError::InvalidOperation(
                "MP4 save encountered an unexpected resize state".to_string(),
            ));
        }
        self.update_offsets_async(file, atoms, delta, offset)
            .await?;

        Ok(())
    }

    /// Async version of `save_new`.
    async fn save_new_async(
        &self,
        file: &mut tokio::fs::File,
        atoms: &Atoms,
        new_ilst_data: &[u8],
    ) -> Result<()> {
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};

        if new_ilst_data.is_empty() {
            return Err(AudexError::InvalidData(
                "Cannot save empty metadata to MP4".to_string(),
            ));
        }

        let hdlr = MP4Atom::render(
            b"hdlr",
            &[
                0, 0, 0, 0, 0, 0, 0, 0, b'm', b'd', b'i', b'r', b'a', b'p', b'p', b'l', 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ],
        )?;

        let ilst_atom = MP4Atom::render(b"ilst", new_ilst_data)?;
        let meta_data = [&[0u8; 4], &hdlr[..], &ilst_atom[..]].concat();

        let path = if let Some(udta_path) = atoms.path("moov.udta") {
            udta_path
        } else {
            atoms
                .path("moov")
                .ok_or_else(|| AudexError::ParseError("No moov atom found".to_string()))?
        };

        let last_atom = path
            .last()
            .ok_or_else(|| AudexError::ParseError("Empty atom path for moov".to_string()))?;
        // Skip past the atom header to reach the payload. The header size is
        // derived from the atom's own fields (data_offset - offset), which
        // correctly handles both standard 8-byte and extended 16-byte headers.
        let header_size = last_atom
            .data_offset
            .checked_sub(last_atom.offset)
            .ok_or_else(|| {
                AudexError::ParseError("atom data_offset is before atom start".to_string())
            })?;
        let offset = last_atom.offset.checked_add(header_size).ok_or_else(|| {
            AudexError::ParseError("moov atom offset too large to add header size".to_string())
        })?;

        let file_size = file.seek(SeekFrom::End(0)).await.map_err(AudexError::Io)?;
        let content_size = file_size.checked_sub(offset).ok_or_else(|| {
            AudexError::ParseError(
                "moov offset exceeds file size, file may be truncated".to_string(),
            )
        })?;
        // Safe conversions: reject values exceeding i64::MAX to match the
        // validation used in save_existing and prevent silent wrapping
        let content_size_i64 = i64::try_from(content_size).map_err(|_| {
            AudexError::InvalidData(
                "content size exceeds maximum supported value for padding calculation".to_string(),
            )
        })?;
        let meta_data_len_i64 = i64::try_from(meta_data.len()).map_err(|_| {
            AudexError::InvalidData("metadata size exceeds maximum supported value".to_string())
        })?;
        let padding_size = -meta_data_len_i64;

        let info = PaddingInfo::new(padding_size, content_size_i64);
        // Safe conversion: clamp padding to a sane maximum (10 MB) and use
        // try_from to avoid wrapping on 32-bit targets.
        const MAX_PADDING: usize = 10 * 1024 * 1024;
        let new_padding = usize::try_from(info.get_default_padding().max(0))
            .unwrap_or(0)
            .min(MAX_PADDING);

        let free_atom = if new_padding > 0 {
            MP4Atom::render(b"free", &vec![0u8; new_padding])?
        } else {
            Vec::new()
        };

        let meta = MP4Atom::render(b"meta", &[&meta_data[..], &free_atom[..]].concat())?;

        let data = if last_atom.name != *b"udta" {
            MP4Atom::render(b"udta", &meta)?
        } else {
            meta
        };

        crate::util::insert_bytes_async(file, data.len() as u64, offset, None).await?;
        file.seek(SeekFrom::Start(offset)).await?;
        file.write_all(&data).await?;

        // Safe size conversion before updating parents and offsets
        let data_len_i64 = i64::try_from(data.len()).map_err(|_| {
            AudexError::InvalidData(
                "inserted data size exceeds maximum supported value".to_string(),
            )
        })?;
        self.update_parents_async(file, &path, data_len_i64, offset)
            .await?;
        self.update_offsets_async(file, atoms, data_len_i64, offset)
            .await?;

        Ok(())
    }

    /// Async version of `update_parents`.
    ///
    /// See [`update_parents`](Self::update_parents) for the offset-adjustment
    /// rationale.
    async fn update_parents_async(
        &self,
        file: &mut tokio::fs::File,
        path: &[&MP4Atom],
        delta: i64,
        resize_offset: u64,
    ) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        if delta == 0 {
            return Ok(());
        }

        for atom in path {
            // Adjust for data that shifted after the resize point
            let mut actual_offset = atom.offset;
            if actual_offset > resize_offset {
                actual_offset = (actual_offset as i64)
                    .checked_add(delta)
                    .filter(|&v| v >= 0)
                    .ok_or_else(|| {
                        AudexError::ParseError(format!(
                            "Parent atom offset underflow: {} + {}",
                            atom.offset, delta
                        ))
                    })? as u64;
            }

            file.seek(SeekFrom::Start(actual_offset)).await?;
            let mut size_bytes = [0u8; 4];
            file.read_exact(&mut size_bytes).await?;
            let size = u32::from_be_bytes(size_bytes);

            if size == 1 {
                // 64-bit extended size — use checked arithmetic to prevent overflow corruption
                file.seek(SeekFrom::Start(actual_offset + 8)).await?;
                let mut size_bytes = [0u8; 8];
                file.read_exact(&mut size_bytes).await?;
                let size = u64::from_be_bytes(size_bytes);
                // Safe conversion: reject sizes that exceed i64::MAX
                let size_i64 = i64::try_from(size).map_err(|_| {
                    AudexError::ParseError(format!(
                        "64-bit atom size {} exceeds maximum representable value",
                        size
                    ))
                })?;
                let new_size =
                    size_i64
                        .checked_add(delta)
                        .filter(|&s| s >= 8)
                        .ok_or_else(|| {
                            AudexError::ParseError(format!(
                                "Atom size overflow: {} + {} produces invalid size",
                                size, delta
                            ))
                        })?;
                file.seek(SeekFrom::Start(actual_offset + 8)).await?;
                file.write_all(&new_size.to_be_bytes()).await?;
            } else {
                // 32-bit size — use checked arithmetic to prevent truncation
                let new_size = (size as i64)
                    .checked_add(delta)
                    .filter(|&s| s >= 8 && s <= u32::MAX as i64)
                    .ok_or_else(|| {
                        AudexError::ParseError(format!(
                            "Atom size overflow: {} + {} produces invalid 32-bit size",
                            size, delta
                        ))
                    })? as u32;
                file.seek(SeekFrom::Start(actual_offset)).await?;
                file.write_all(&new_size.to_be_bytes()).await?;
            }
        }

        Ok(())
    }

    /// Async version of `update_offsets`.
    async fn update_offsets_async(
        &self,
        file: &mut tokio::fs::File,
        atoms: &Atoms,
        delta: i64,
        offset: u64,
    ) -> Result<()> {
        if delta == 0 {
            return Ok(());
        }

        if let Some(moov) = atoms.atoms.iter().find(|a| a.name == *b"moov") {
            for atom in moov.findall(b"stco", true) {
                self.update_offset_table_async(file, atom, delta, offset, false)
                    .await?;
            }
            for atom in moov.findall(b"co64", true) {
                self.update_offset_table_async(file, atom, delta, offset, true)
                    .await?;
            }
        }

        if let Some(moof) = atoms.atoms.iter().find(|a| a.name == *b"moof") {
            for atom in moof.findall(b"tfhd", true) {
                self.update_tfhd_async(file, atom, delta, offset).await?;
            }
        }

        Ok(())
    }

    /// Async version of `update_offset_table`.
    async fn update_offset_table_async(
        &self,
        file: &mut tokio::fs::File,
        atom: &crate::mp4::atom::MP4Atom,
        delta: i64,
        offset: u64,
        is_64bit: bool,
    ) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        let mut atom_offset = atom.offset;
        if atom_offset > offset {
            // Use checked arithmetic to prevent underflow
            atom_offset = (atom_offset as i64)
                .checked_add(delta)
                .filter(|&v| v >= 0)
                .ok_or_else(|| {
                    AudexError::ParseError(format!(
                        "Offset table atom position underflow: {} + {}",
                        atom_offset, delta
                    ))
                })? as u64;
        }

        file.seek(SeekFrom::Start(atom_offset + 12))
            .await
            .map_err(|e| AudexError::ParseError(format!("Seek failed: {}", e)))?;

        let mut count_buf = [0u8; 4];
        file.read_exact(&mut count_buf)
            .await
            .map_err(|e| AudexError::ParseError(format!("Read failed: {}", e)))?;
        // Safe to cast: validated against atom capacity below, which bounds
        // it well within addressable memory on any platform.
        let count = u32::from_be_bytes(count_buf) as usize;

        // Validate entry count against the atom's declared size.
        // The atom payload after the 8-byte header, 4-byte version/flags,
        // and 4-byte count field must hold count * entry_size bytes.
        let entry_size = if is_64bit { 8 } else { 4 };
        let header_overhead: u64 = 16; // 8 (atom header) + 4 (version/flags) + 4 (count)
        let max_data_bytes = atom.length.saturating_sub(header_overhead);
        let max_entries = max_data_bytes / entry_size as u64;
        if (count as u64) > max_entries {
            return Err(AudexError::ParseError(format!(
                "Offset table entry count ({}) exceeds atom capacity ({} entries fit in {} bytes)",
                count, max_entries, max_data_bytes
            )));
        }

        let alloc_size = count.checked_mul(entry_size).ok_or_else(|| {
            AudexError::ParseError(format!(
                "Offset table allocation overflow: {} entries * {} bytes",
                count, entry_size
            ))
        })?;
        let mut data = vec![0u8; alloc_size];
        file.read_exact(&mut data)
            .await
            .map_err(|e| AudexError::ParseError(format!("Read offsets failed: {}", e)))?;

        // Update offsets that are after the modification point
        for i in 0..count {
            let pos = i * entry_size;
            if is_64bit {
                let o = u64::from_be_bytes([
                    data[pos],
                    data[pos + 1],
                    data[pos + 2],
                    data[pos + 3],
                    data[pos + 4],
                    data[pos + 5],
                    data[pos + 6],
                    data[pos + 7],
                ]);
                if o > offset {
                    // Safe conversion: reject offsets that exceed i64::MAX
                    // to prevent silent wrapping to negative values
                    let o_i64 = i64::try_from(o).map_err(|_| {
                        AudexError::ParseError(format!(
                            "64-bit chunk offset {} exceeds maximum representable value",
                            o
                        ))
                    })?;
                    // Use checked arithmetic to prevent underflow wrapping
                    let new_o = o_i64
                        .checked_add(delta)
                        .filter(|&v| v >= 0)
                        .ok_or_else(|| {
                            AudexError::ParseError(format!(
                                "Chunk offset underflow: {} + {} is negative",
                                o, delta
                            ))
                        })? as u64;
                    data[pos..pos + 8].copy_from_slice(&new_o.to_be_bytes());
                }
            } else {
                let o =
                    u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
                if (o as u64) > offset {
                    // Use checked arithmetic to prevent underflow wrapping
                    let new_o = (o as i64)
                        .checked_add(delta)
                        .filter(|&v| v >= 0 && v <= u32::MAX as i64)
                        .ok_or_else(|| {
                            AudexError::ParseError(format!(
                                "Chunk offset underflow: {} + {} is invalid for 32-bit",
                                o, delta
                            ))
                        })? as u32;
                    data[pos..pos + 4].copy_from_slice(&new_o.to_be_bytes());
                }
            }
        }

        file.seek(SeekFrom::Start(atom_offset + 16))
            .await
            .map_err(|e| AudexError::ParseError(format!("Seek failed: {}", e)))?;
        file.write_all(&data)
            .await
            .map_err(|e| AudexError::ParseError(format!("Write offsets failed: {}", e)))?;

        Ok(())
    }

    /// Async version of `update_tfhd`.
    async fn update_tfhd_async(
        &self,
        file: &mut tokio::fs::File,
        atom: &crate::mp4::atom::MP4Atom,
        delta: i64,
        offset: u64,
    ) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        let mut atom_offset = atom.offset;
        if atom_offset > offset {
            // Use checked arithmetic to prevent overflow/underflow
            atom_offset = (atom_offset as i64)
                .checked_add(delta)
                .filter(|&v| v >= 0)
                .ok_or_else(|| {
                    AudexError::ParseError(format!(
                        "Atom offset overflow: {} + {}",
                        atom_offset, delta
                    ))
                })? as u64;
        }

        file.seek(SeekFrom::Start(atom_offset + 9))
            .await
            .map_err(|e| AudexError::ParseError(format!("Seek failed: {}", e)))?;

        let atom_len = usize::try_from(atom.length).map_err(|_| {
            AudexError::ParseError(format!(
                "Atom length {} exceeds addressable range",
                atom.length
            ))
        })?;
        if atom_len < 12 {
            return Err(AudexError::ParseError(format!(
                "tfhd atom too short for version and flags: {} bytes",
                atom.length
            )));
        }
        let alloc_size = atom_len - 9;
        // Guard against oversized allocations from crafted atom lengths
        let limits = crate::limits::ParseLimits::default();
        if alloc_size as u64 > limits.max_tag_size {
            return Err(AudexError::ParseError(format!(
                "tfhd atom data size ({} bytes) exceeds maximum allowed ({})",
                alloc_size, limits.max_tag_size
            )));
        }
        let mut data = vec![0u8; alloc_size];
        file.read_exact(&mut data)
            .await
            .map_err(|e| AudexError::ParseError(format!("Read tfhd failed: {}", e)))?;

        let flags = u32::from_be_bytes([0, data[0], data[1], data[2]]);

        if flags & 1 != 0 && data.len() >= 15 {
            let o = u64::from_be_bytes([
                data[7], data[8], data[9], data[10], data[11], data[12], data[13], data[14],
            ]);
            if o > offset {
                // Safe conversion: reject offsets that exceed i64::MAX
                // to prevent silent wrapping to negative values
                let o_i64 = i64::try_from(o).map_err(|_| {
                    AudexError::ParseError(format!(
                        "64-bit tfhd base_data_offset {} exceeds maximum representable value",
                        o
                    ))
                })?;
                // Use checked arithmetic to prevent underflow wrapping
                let new_o = o_i64
                    .checked_add(delta)
                    .filter(|&v| v >= 0)
                    .ok_or_else(|| {
                        AudexError::ParseError(format!(
                            "tfhd base_data_offset underflow: {} + {}",
                            o, delta
                        ))
                    })? as u64;
                file.seek(SeekFrom::Start(atom_offset + 16))
                    .await
                    .map_err(|e| AudexError::ParseError(format!("Seek failed: {}", e)))?;
                file.write_all(&new_o.to_be_bytes())
                    .await
                    .map_err(|e| AudexError::ParseError(format!("Write tfhd failed: {}", e)))?;
            }
        }

        Ok(())
    }

    /// Async version of `load`. Reads atom data with async I/O and parses
    /// metadata without buffering the entire file.
    pub async fn load_async(atoms: &Atoms, file: &mut tokio::fs::File) -> Result<Option<Self>> {
        if !Self::can_load(atoms) {
            return Ok(None);
        }

        let ilst_path = atoms
            .path("moov.udta.meta.ilst")
            .ok_or_else(|| AudexError::ParseError("Failed to get ilst path".to_string()))?;

        let ilst = ilst_path
            .last()
            .ok_or_else(|| AudexError::ParseError("Empty ilst path".to_string()))?;

        let mut tags = MP4Tags::new();

        // Check for adjacent free atom for padding calculation
        if let Some(meta_path) = atoms.path("moov.udta.meta") {
            if let Some(meta) = meta_path.last() {
                if let Some(children) = &meta.children {
                    for (i, child) in children.iter().enumerate() {
                        if child.name == *b"ilst" {
                            if i > 0 && children[i - 1].name == *b"free" {
                                tags.padding = usize::try_from(children[i - 1].data_length)
                                    .unwrap_or(usize::MAX);
                            } else if i + 1 < children.len() && children[i + 1].name == *b"free" {
                                tags.padding = usize::try_from(children[i + 1].data_length)
                                    .unwrap_or(usize::MAX);
                            }
                            break;
                        }
                    }
                }
            }
        }

        // Parse ilst children using async reads
        if let Some(children) = &ilst.children {
            let mut total_ilst_bytes = 0u64;
            for child in children {
                total_ilst_bytes =
                    total_ilst_bytes
                        .checked_add(child.data_length)
                        .ok_or_else(|| {
                            AudexError::InvalidData("MP4 metadata size overflow".to_string())
                        })?;
                if total_ilst_bytes > MAX_TOTAL_ILST_DATA_BYTES {
                    return Err(AudexError::InvalidData(format!(
                        "MP4 metadata exceeds cumulative {} byte limit",
                        MAX_TOTAL_ILST_DATA_BYTES
                    )));
                }
                let data = child.read_data_async(file).await?;
                if tags.parse_metadata_atom_data(child, &data).is_err() {
                    let key = crate::mp4::util::name2key(&child.name);
                    tags.failed_atoms.entry(key).or_default().push(data);
                }
            }
        }

        Ok(Some(tags))
    }
}

impl Tags for MP4Tags {
    fn get(&self, key: &str) -> Option<&[String]> {
        self.tags.get(key).map(|v| v.as_slice())
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        if values.is_empty() {
            self.tags.remove(key);
            self.freeforms.remove(key);
        } else if key.starts_with("----:") {
            // Freeform keys must go into the freeforms map so they are saved
            // as proper freeform atoms (mean + name + data).
            let template = self
                .freeforms
                .get(key)
                .and_then(|values| values.first())
                .cloned();
            let freeform_values: Vec<MP4FreeForm> = values
                .into_iter()
                .map(|v| {
                    if let Some(template) = &template {
                        MP4FreeForm::new(v.into_bytes(), template.dataformat, template.version)
                    } else {
                        MP4FreeForm::new_text(v.into_bytes())
                    }
                })
                .collect();
            self.freeforms.insert(key.to_string(), freeform_values);
        } else {
            self.tags.insert(key.to_string(), values);
        }
    }

    fn remove(&mut self, key: &str) {
        self.tags.remove(key);
        self.freeforms.remove(key);

        // Remove covers if key matches
        if key == "covr" {
            self.covers.clear();
        }
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.tags.keys().cloned().collect();
        keys.extend(self.freeforms.keys().cloned());
        if !self.covers.is_empty() {
            keys.push("covr".to_string());
        }
        keys.sort();
        keys.dedup();
        keys
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

impl Metadata for MP4Tags {
    type Error = crate::AudexError;

    fn new() -> Self {
        MP4Tags::default()
    }

    fn load_from_fileobj(filething: &mut crate::util::AnyFileThing) -> crate::Result<Self> {
        if let Some(path) = filething.filename() {
            let mp4 = crate::mp4::MP4::load(path)?;
            Ok(mp4.tags.unwrap_or_else(MP4Tags::new))
        } else {
            Err(crate::AudexError::InvalidOperation(
                "MP4Tags.load_from_fileobj requires a real file path".to_string(),
            ))
        }
    }

    fn save_to_fileobj(&self, filething: &mut crate::util::AnyFileThing) -> crate::Result<()> {
        let path = filething.filename().ok_or_else(|| {
            crate::AudexError::InvalidOperation(
                "MP4Tags.save_to_fileobj requires a real file path".to_string(),
            )
        })?;
        self.save(path)
    }

    fn delete_from_fileobj(filething: &mut crate::util::AnyFileThing) -> crate::Result<()> {
        let path = filething.filename().ok_or_else(|| {
            crate::AudexError::InvalidOperation(
                "MP4Tags.delete_from_fileobj requires a real file path".to_string(),
            )
        })?;
        MP4Tags::new().save(path)
    }
}

impl crate::tags::MetadataFields for MP4Tags {
    fn artist(&self) -> Option<&String> {
        self.get_first("©ART")
    }

    fn set_artist(&mut self, artist: String) {
        self.set_single("©ART", artist);
    }

    fn album(&self) -> Option<&String> {
        self.get_first("©alb")
    }

    fn set_album(&mut self, album: String) {
        self.set_single("©alb", album);
    }

    fn title(&self) -> Option<&String> {
        self.get_first("©nam")
    }

    fn set_title(&mut self, title: String) {
        self.set_single("©nam", title);
    }

    fn track_number(&self) -> Option<u32> {
        self.get_first("trkn")?.split('/').next()?.parse().ok()
    }

    fn set_track_number(&mut self, track: u32) {
        self.set_single("trkn", track.to_string());
    }

    fn date(&self) -> Option<&String> {
        self.get_first("©day")
    }

    fn set_date(&mut self, date: String) {
        self.set_single("©day", date);
    }

    fn genre(&self) -> Option<&String> {
        self.get_first("©gen")
    }

    fn set_genre(&mut self, genre: String) {
        self.set_single("©gen", genre);
    }
}

/// Audio stream information extracted from MP4/M4A files.
///
/// This struct contains technical information about the audio track, including
/// codec details, bitrate, sample rate, and duration. The data is extracted from
/// various atoms in the MP4 container structure, primarily from the `trak` (track)
/// and `mdia` (media) atoms.
///
/// # Field Availability
///
/// Some fields may be `None` if:
/// - The information is not present in the file
/// - The atom containing the data is missing or corrupted
/// - The codec doesn't support that property (e.g., bitrate for some lossless codecs)
///
/// # Examples
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mp4 = MP4::load("song.m4a").unwrap();
/// let info = &mp4.info;
///
/// // Access audio properties
/// println!("Codec: {}", info.codec);
/// println!("Description: {}", info.codec_description);
///
/// if let Some(bitrate) = info.bitrate {
///     println!("Bitrate: {} kbps", bitrate / 1000);
/// }
///
/// if let Some(sample_rate) = info.sample_rate {
///     println!("Sample rate: {} Hz", sample_rate);
/// }
///
/// if let Some(channels) = info.channels {
///     println!("Channels: {}", channels);
/// }
///
/// if let Some(duration) = info.length {
///     println!("Duration: {:.2} seconds", duration.as_secs_f64());
/// }
/// ```
///
/// # Common Codecs
///
/// - **mp4a**: AAC (Advanced Audio Coding) - Most common
/// - **alac**: Apple Lossless (ALAC)
/// - **mp4v**: MPEG-4 Visual (video, not audio)
/// - **ac-3**: Dolby Digital (AC-3)
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MP4Info {
    /// Average bitrate in bits per second, if available.
    ///
    /// This may be `None` for:
    /// - Lossless codecs where bitrate varies significantly
    /// - Files missing bitrate information
    /// - Streams where bitrate calculation is not applicable
    pub bitrate: Option<u32>,

    /// Total audio duration, if determinable.
    ///
    /// Calculated from the media header (`mdhd` atom) using:
    /// `duration = total_samples / sample_rate`
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::duration_as_secs_f64")
    )]
    pub length: Option<Duration>,

    /// Number of audio channels, if available.
    ///
    /// Common values:
    /// - `1`: Mono
    /// - `2`: Stereo
    /// - `6`: 5.1 surround
    /// - `8`: 7.1 surround
    pub channels: Option<u16>,

    /// Sample rate in Hz, if available.
    ///
    /// Common values:
    /// - 44100: CD quality
    /// - 48000: Professional audio, DAT
    /// - 96000, 192000: High-resolution audio
    pub sample_rate: Option<u32>,

    /// Bits per sample (bit depth), if available.
    ///
    /// Common values:
    /// - 16: CD quality
    /// - 24: High-resolution audio
    /// - 32: Float or high-resolution integer
    ///
    /// May be `None` for compressed codecs like AAC where
    /// bit depth is not directly applicable.
    pub bits_per_sample: Option<u16>,

    /// Four-character codec identifier (FourCC code).
    ///
    /// Common values: "mp4a" (AAC), "alac" (Apple Lossless), "ac-3" (Dolby Digital)
    pub codec: String,

    /// Human-readable codec description.
    ///
    /// Provides a more detailed description of the codec format,
    /// such as "AAC LC" (AAC Low Complexity) or "Apple Lossless".
    pub codec_description: String,
}

impl MP4Info {
    /// Load stream info from atoms
    pub fn load<R: Read + Seek>(atoms: &Atoms, reader: &mut R) -> Result<Self> {
        let mut info = MP4Info::default();

        // Find moov atom
        let moov = atoms
            .get("moov")
            .ok_or_else(|| AudexError::ParseError("not a MP4 file - no moov atom".to_string()))?;

        // Find audio track
        let audio_trak = Self::find_audio_track(moov, reader)?;

        // Parse media header for duration
        if let Some(mdhd) = audio_trak.get_child(&["mdia", "mdhd"]) {
            info.parse_mdhd(mdhd, reader)?;
        }

        // Parse sample description for codec info
        if let Some(stsd) = audio_trak.get_child(&["mdia", "minf", "stbl", "stsd"]) {
            info.parse_stsd(stsd, reader)?;
        }

        Ok(info)
    }

    /// Find the first audio track
    fn find_audio_track<'a, R: Read + Seek>(
        moov: &'a MP4Atom,
        reader: &mut R,
    ) -> Result<&'a MP4Atom> {
        if let Some(children) = &moov.children {
            for trak in children {
                if trak.name == *b"trak" {
                    if let Some(hdlr) = trak.get_child(&["mdia", "hdlr"]) {
                        let data = hdlr.read_data(reader)?;
                        if Self::is_audio_handler(&data) {
                            return Ok(trak);
                        }
                    }
                }
            }
        }

        Err(AudexError::ParseError(
            "track has no audio data".to_string(),
        ))
    }

    /// Check whether handler data indicates an audio track.
    fn is_audio_handler(data: &[u8]) -> bool {
        data.len() >= 12 && &data[8..12] == b"soun"
    }

    /// Parse media header atom for duration and timescale
    fn parse_mdhd<R: Read + Seek>(&mut self, mdhd: &MP4Atom, reader: &mut R) -> Result<()> {
        let data = mdhd.read_data(reader)?;
        self.parse_mdhd_data(&data)
    }

    /// Parse mdhd data that has already been read from the stream.
    fn parse_mdhd_data(&mut self, data: &[u8]) -> Result<()> {
        let (version, _flags, payload) = parse_full_atom(data)?;

        let (timescale, duration) = match version {
            0 => {
                if payload.len() < 16 {
                    return Err(AudexError::ParseError("mdhd payload too short".to_string()));
                }
                // Skip creation and modification time (8 bytes)
                let timescale =
                    u32::from_be_bytes([payload[8], payload[9], payload[10], payload[11]]);
                let duration =
                    u32::from_be_bytes([payload[12], payload[13], payload[14], payload[15]]) as u64;
                (timescale, duration)
            }
            1 => {
                if payload.len() < 28 {
                    return Err(AudexError::ParseError("mdhd payload too short".to_string()));
                }
                // Skip creation and modification time (16 bytes)
                let timescale =
                    u32::from_be_bytes([payload[16], payload[17], payload[18], payload[19]]);
                let duration = u64::from_be_bytes([
                    payload[20],
                    payload[21],
                    payload[22],
                    payload[23],
                    payload[24],
                    payload[25],
                    payload[26],
                    payload[27],
                ]);
                (timescale, duration)
            }
            _ => {
                return Err(AudexError::ParseError(format!(
                    "Unknown mdhd version {}",
                    version
                )));
            }
        };

        if timescale > 0 {
            // Clamp to a reasonable maximum (~292 years) to avoid panic
            // on crafted files with extreme duration/timescale ratios
            let seconds = (duration as f64 / timescale as f64).clamp(0.0, u64::MAX as f64 / 1e9);
            self.length = Duration::try_from_secs_f64(seconds).ok();
        }

        Ok(())
    }

    /// Parse sample description atom for codec information
    fn parse_stsd<R: Read + Seek>(&mut self, stsd: &MP4Atom, reader: &mut R) -> Result<()> {
        let data = stsd.read_data(reader)?;
        self.parse_stsd_data(&data)
    }

    /// Parse stsd data that has already been read from the stream.
    fn parse_stsd_data(&mut self, data: &[u8]) -> Result<()> {
        let (version, _flags, payload) = parse_full_atom(data)?;

        if version != 0 {
            return Err(AudexError::ParseError(format!(
                "Unsupported stsd version {}",
                version
            )));
        }

        if payload.len() < 4 {
            return Err(AudexError::ParseError("stsd payload too short".to_string()));
        }

        let num_entries = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);

        if num_entries == 0 {
            return Ok(());
        }

        // Minimum sample entry: 8-byte atom header + 28 bytes audio sample data
        if payload.len() < 4 + 36 {
            return Err(AudexError::ParseError(
                "stsd payload too short for a sample entry".to_string(),
            ));
        }

        // Parse the first sample entry from a cursor over the payload.
        // NOTE: Atoms parsed from this cursor have offset/data_offset values
        // relative to `entry_data`, not the original file. This is correct
        // because AudioSampleEntry::parse reads child atom data by slicing
        // the cursor's buffer, not by seeking in the original file reader.
        let entry_data = &payload[4..];
        let mut cursor = Cursor::new(entry_data);

        match MP4Atom::parse(&mut cursor, 1) {
            Ok(entry_atom) => {
                let entry = AudioSampleEntry::parse(&entry_atom, &mut cursor)?;

                self.channels = Some(entry.channels);
                self.sample_rate = Some(entry.sample_rate);
                self.bits_per_sample = Some(entry.sample_size);
                self.bitrate = Some(entry.bitrate);
                self.codec = entry.codec;
                self.codec_description = entry.codec_description;
            }
            Err(e) => {
                return Err(AudexError::ParseError(format!(
                    "Failed to parse sample entry: {}",
                    e
                )));
            }
        }

        Ok(())
    }
}

impl StreamInfo for MP4Info {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        self.bitrate
    }

    fn sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }

    fn channels(&self) -> Option<u16> {
        self.channels
    }

    fn bits_per_sample(&self) -> Option<u16> {
        self.bits_per_sample
    }
}

/// Main structure representing an MP4/M4A audio file.
///
/// This is the primary interface for reading and writing MP4 container files with
/// audio content, including M4A (MPEG-4 Audio), M4B (audiobooks), and M4P (protected) formats.
///
/// # Structure
///
/// - **`info`**: Audio stream information (bitrate, sample rate, codec, duration)
/// - **`tags`**: Optional iTunes-style metadata tags
/// - **`chapters`**: Optional chapter markers (common in audiobooks)
/// - **`path`**: Internal file path (used for save operations)
///
/// # Supported Codecs
///
/// - **AAC** (Advanced Audio Coding) - most common
/// - **ALAC** (Apple Lossless Audio Codec)
/// - **MP3** (MPEG-1 Audio Layer III) in MP4 container
/// - Other MPEG-4 audio codecs
///
/// # File Format
///
/// MP4 files use a hierarchical atom (box) structure:
/// ```text
/// ftyp - File type identification
/// moov - Movie/metadata container
///   ├─ mvhd - Movie header (timescale, duration)
///   ├─ trak - Track container (audio/video)
///   │   └─ mdia - Media information
///   │       └─ minf - Media info
///   │           └─ stbl - Sample table
///   │               └─ stsd - Sample descriptions (codec info)
///   └─ udta - User data
///       ├─ meta - Metadata container
///       │   └─ ilst - iTunes-style tag list
///       └─ chpl - Chapter list (audiobooks)
/// mdat - Media data (compressed audio)
/// ```
///
/// # Examples
///
/// ## Loading and reading file information
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mp4 = MP4::load("song.m4a").unwrap();
///
/// // Audio stream information
/// println!("Codec: {}", mp4.info.codec);
/// println!("Duration: {:?}", mp4.info.length);
/// println!("Sample rate: {:?} Hz", mp4.info.sample_rate);
/// println!("Bitrate: {:?} bps", mp4.info.bitrate);
///
/// if !mp4.info.codec_description.is_empty() {
///     println!("Codec details: {}", mp4.info.codec_description);
/// }
/// ```
///
/// ## Reading and modifying tags
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mut mp4 = MP4::load("song.m4a").unwrap();
///
/// // Read existing tags
/// if let Some(ref tags) = mp4.tags {
///     if let Some(title) = tags.tags.get("©nam") {
///         println!("Current title: {}", title.join(", "));
///     }
/// }
///
/// // Modify tags
/// if let Some(ref mut tags) = mp4.tags {
///     tags.tags.insert("©nam".to_string(), vec!["New Title".to_string()]);
///     tags.tags.insert("©ART".to_string(), vec!["Artist Name".to_string()]);
///     tags.tags.insert("©alb".to_string(), vec!["Album Name".to_string()]);
///     tags.tags.insert("©day".to_string(), vec!["2024".to_string()]);
/// }
///
/// // Save changes
/// mp4.save().unwrap();
/// ```
///
/// ## Creating tags if they don't exist
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mut mp4 = MP4::load("song.m4a").unwrap();
///
/// // Create tags if file has none
/// if mp4.tags.is_none() {
///     mp4.add_tags().unwrap();
/// }
///
/// // Now we can add metadata
/// if let Some(ref mut tags) = mp4.tags {
///     tags.tags.insert("©nam".to_string(), vec!["Title".to_string()]);
/// }
///
/// mp4.save().unwrap();
/// ```
///
/// ## Working with audiobook chapters
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mp4 = MP4::load("audiobook.m4b").unwrap();
///
/// if let Some(ref chapters) = mp4.chapters {
///     println!("Audiobook has {} chapters", chapters.len());
///
///     for (i, chapter) in chapters.iter().enumerate() {
///         let minutes = (chapter.start / 60.0).floor() as u32;
///         let seconds = (chapter.start % 60.0) as u32;
///         println!("{}. {} - {:02}:{:02}",
///             i + 1, chapter.title, minutes, seconds);
///     }
/// }
/// ```
///
/// ## Adding cover artwork
///
/// ```no_run
/// use audex::mp4::{MP4, MP4Cover, AtomDataType};
/// use std::fs;
/// use audex::FileType;
///
/// let mut mp4 = MP4::load("song.m4a").unwrap();
///
/// // Ensure tags exist
/// let tags = mp4.get_or_create_tags();
///
/// // Load cover image
/// let cover_data = fs::read("cover.jpg").unwrap();
/// let cover = MP4Cover::new(cover_data, AtomDataType::Jpeg);
///
/// // Add to file
/// tags.covers.push(cover);
///
/// mp4.save().unwrap();
/// ```
///
/// ## Removing all metadata
///
/// ```no_run
/// use audex::mp4::MP4;
/// use audex::FileType;
///
/// let mut mp4 = MP4::load("song.m4a").unwrap();
///
/// // Clear all tags
/// mp4.clear().unwrap();
///
/// println!("All metadata removed");
/// ```
#[derive(Debug, Default)]
pub struct MP4 {
    pub info: MP4Info,
    pub tags: Option<MP4Tags>,
    pub chapters: Option<MP4Chapters>,
    path: Option<std::path::PathBuf>,
}

impl FileType for MP4 {
    type Tags = MP4Tags;
    type Info = MP4Info;

    fn format_id() -> &'static str {
        "MP4"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing MP4 file");
        let file = File::open(&path)?;
        let mut reader = BufReader::new(file);

        // Parse all atoms (top-level atom traversal)
        let atoms = Atoms::parse(&mut reader)?;
        trace_event!("MP4 atom tree traversal complete");

        // Load stream info
        let info = MP4Info::load(&atoms, &mut reader)?;

        // Load tags if present
        let tags = MP4Tags::load(&atoms, &mut reader)?;
        debug_event!(tags_present = tags.is_some(), "MP4 tags parsed");

        // Load chapters if present
        let chapters = MP4Chapters::load(&atoms, &mut reader)?;

        Ok(MP4 {
            info,
            tags,
            chapters,
            path: Some(path.as_ref().to_path_buf()),
        })
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing MP4 file from reader");
        let mut reader = reader;
        let atoms = Atoms::parse(&mut reader)?;
        trace_event!("MP4 atom tree traversal complete");
        let info = MP4Info::load(&atoms, &mut reader)?;
        let tags = MP4Tags::load(&atoms, &mut reader)?;
        debug_event!(tags_present = tags.is_some(), "MP4 tags parsed");
        let chapters = MP4Chapters::load(&atoms, &mut reader)?;
        Ok(MP4 {
            info,
            tags,
            chapters,
            path: None,
        })
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving MP4 file metadata");
        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::ParseError("No file path available for saving".to_string())
        })?;

        if let Some(tags) = &self.tags {
            tags.save(path)?;
        } else {
            // If no tags, just create empty metadata structure for consistency
            let empty_tags = MP4Tags::new();
            empty_tags.save(path)?;
        }

        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        // Delete MP4 metadata by clearing tags and saving
        self.tags = None;

        // Save the file to remove metadata
        if let Some(path) = &self.path {
            // Create empty tags structure to clear metadata atoms
            let empty_tags = MP4Tags::new();
            empty_tags.save(path)?;
        }

        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        if let Some(tags) = &self.tags {
            tags.save_to_writer(writer)?;
        } else {
            let empty_tags = MP4Tags::new();
            empty_tags.save_to_writer(writer)?;
        }
        Ok(())
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        self.tags = None;
        let empty_tags = MP4Tags::new();
        empty_tags.save_to_writer(writer)?;
        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        if let Some(tags) = &self.tags {
            tags.save(path)?;
        } else {
            let empty_tags = MP4Tags::new();
            empty_tags.save(path)?;
        }
        Ok(())
    }

    /// Adds empty MP4 metadata tags to the file.
    ///
    /// Creates a new empty tag structure if none exists. If tags already exist,
    /// returns an error.
    ///
    /// # Errors
    ///
    /// Returns `AudexError::ParseError` if tags already exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::mp4::MP4;
    /// use audex::FileType;
    ///
    /// let mut mp4 = MP4::load("song.m4a")?;
    /// if mp4.tags.is_none() {
    ///     mp4.add_tags()?;
    /// }
    /// mp4.set("title", vec!["My Song".to_string()])?;
    /// mp4.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(MP4Tags::new());
        Ok(())
    }

    fn get(&self, key: &str) -> Option<Vec<String>> {
        // Map friendly name to MP4 atom code using EasyMP4 registry
        let registry = crate::easymp4::KeyRegistry::new();
        let mp4_key = if let Some(mapping) = registry.get_mp4_key(key) {
            &mapping.mp4_key
        } else {
            // If no mapping, use the key as-is (might be a raw MP4 atom code)
            key
        };

        let tags = self.tags.as_ref()?;

        // Check standard tags first
        if let Some(v) = tags.get(mp4_key) {
            return Some(v.to_vec());
        }

        // Check freeform tags — convert binary data to UTF-8 strings
        if let Some(freeforms) = tags.freeforms.get(mp4_key) {
            let values: Vec<String> = freeforms
                .iter()
                .filter_map(|ff| String::from_utf8(ff.data.clone()).ok())
                .collect();
            if !values.is_empty() {
                return Some(values);
            }
        }

        None
    }

    fn set(&mut self, key: &str, values: Vec<String>) -> Result<()> {
        // Map friendly name to MP4 atom code using EasyMP4 registry
        let registry = crate::easymp4::KeyRegistry::new();
        let mp4_key = if let Some(mapping) = registry.get_mp4_key(key) {
            mapping.mp4_key.clone()
        } else {
            // If no mapping, use the key as-is (might be a raw MP4 atom code)
            key.to_string()
        };

        // Set in MP4Tags
        if let Some(tags) = self.tags_mut() {
            tags.set(&mp4_key, values);
            Ok(())
        } else {
            Err(AudexError::Unsupported(
                "This format does not support tags".to_string(),
            ))
        }
    }

    fn remove(&mut self, key: &str) -> Result<()> {
        // Map friendly name to MP4 atom code using EasyMP4 registry
        let registry = crate::easymp4::KeyRegistry::new();
        let mp4_key = if let Some(mapping) = registry.get_mp4_key(key) {
            &mapping.mp4_key
        } else {
            // If no mapping, use the key as-is (might be a raw MP4 atom code)
            key
        };

        // Remove from MP4Tags
        if let Some(tags) = self.tags_mut() {
            tags.remove(mp4_key);
            Ok(())
        } else {
            Err(AudexError::Unsupported(
                "This format does not support tags".to_string(),
            ))
        }
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

        // Check for MP4 file type box
        if header.len() >= 8 && &header[4..8] == b"ftyp" {
            score += 10;
        }

        // Check for M4A/MP4/3G2 in the header
        if header.len() >= 12 {
            let ftyp_data = &header[8..std::cmp::min(header.len(), 20)];
            if ftyp_data.starts_with(b"M4A ")
                || ftyp_data.starts_with(b"M4B ")
                || ftyp_data.starts_with(b"M4P ")
                || ftyp_data.starts_with(b"mp41")
                || ftyp_data.starts_with(b"mp42")
                || ftyp_data.starts_with(b"isom")
                || ftyp_data.starts_with(b"3g2a")
                || ftyp_data.starts_with(b"3g2b")
                || ftyp_data.starts_with(b"3g2c")
            {
                score += 5;
            }
        }

        // Check file extension
        let lower = filename.to_lowercase();
        if lower.ends_with(".m4a")
            || lower.ends_with(".mp4")
            || lower.ends_with(".m4b")
            || lower.ends_with(".m4p")
            || lower.ends_with(".3g2")
            || lower.ends_with(".3gp")
        {
            score += 3;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/mp4", "audio/x-m4a", "audio/mpeg4", "audio/aac"]
    }

    fn filename(&self) -> Option<&str> {
        self.path.as_ref().and_then(|p| p.to_str())
    }
}

impl MP4 {
    /// Create a new MP4 from a path
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load(path)
    }

    /// Add tags to the file if none exist
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(MP4Tags::new());
        Ok(())
    }

    /// Get a mutable reference to tags, creating them if they don't exist
    pub fn get_or_create_tags(&mut self) -> &mut MP4Tags {
        self.tags.get_or_insert_with(MP4Tags::new)
    }

    /// Get padding amount
    pub fn padding(&self) -> usize {
        self.tags.as_ref().map(|t| t.padding()).unwrap_or(0)
    }

    /// Set a tag value (convenience method)
    pub fn set_tag(&mut self, key: &str, value: &str) -> Result<()> {
        self.get_or_create_tags().set_single(key, value.to_string());
        Ok(())
    }

    /// Get a tag value (convenience method)
    pub fn get_tag(&self, key: &str) -> Option<&String> {
        self.tags.as_ref()?.get_first(key)
    }

    /// Remove a tag (convenience method)
    pub fn remove_tag(&mut self, key: &str) {
        if let Some(tags) = &mut self.tags {
            tags.remove(key);
        }
    }

    /// Format for display
    pub fn pprint(&self) -> String {
        let mime_type = Self::mime_types().first().unwrap_or(&"audio/mp4");
        let mut result = format!("{} ({})", self.info.pprint(), mime_type);

        if let Some(tags) = &self.tags {
            let tag_info = tags.pprint();
            if !tag_info.trim().is_empty() {
                result.push('\n');
                result.push_str(&tag_info);
            }
        }

        if let Some(chapters) = &self.chapters {
            let chapter_info = chapters.pprint();
            if !chapter_info.is_empty() && chapter_info != "chapters=" {
                result.push('\n');
                result.push_str(&chapter_info);
            }
        }

        result
    }

    /// Load MP4 file asynchronously.
    ///
    /// # Arguments
    /// * `path` - Path to the MP4/M4A file
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded MP4 file
    /// * `Err(AudexError)` - Error occurred during loading
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Parse the atom tree using native async I/O
        let mut file = tokio::fs::File::open(path).await?;
        let atoms = Atoms::parse_async(&mut file).await?;

        // Load stream info, tags, and chapters using async I/O — each
        // loader seeks to specific atom offsets and reads small chunks,
        // avoiding any full-file buffering.
        let info = MP4Info::load_async(&atoms, &mut file).await?;
        let tags = MP4Tags::load_async(&atoms, &mut file).await?;
        let chapters = MP4Chapters::load_async(&atoms, &mut file).await?;

        Ok(MP4 {
            info,
            tags,
            chapters,
            path: Some(path.to_path_buf()),
        })
    }

    /// Save MP4 file asynchronously using native async I/O.
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        let path = self.path.as_ref().ok_or_else(|| {
            AudexError::ParseError("No file path available for saving".to_string())
        })?;

        if let Some(tags) = &self.tags {
            tags.save_async(path).await?;
        } else {
            let empty_tags = MP4Tags::new();
            empty_tags.save_async(path).await?;
        }

        Ok(())
    }

    /// Clear all tags from the MP4 file asynchronously.
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        self.tags = None;
        self.save_async().await
    }

    /// Delete the MP4 file asynchronously.
    #[cfg(feature = "async")]
    pub async fn delete_async(&mut self) -> Result<()> {
        if let Some(filename) = self.filename() {
            tokio::fs::remove_file(filename).await?;
        }
        Ok(())
    }
}

impl MP4Info {
    /// Format for display
    pub fn pprint(&self) -> String {
        let codec_desc = if self.codec_description.is_empty() {
            self.codec.clone()
        } else {
            self.codec_description.clone()
        };

        let length = self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let bitrate = self.bitrate.unwrap_or(0);

        format!(
            "MPEG-4 audio ({}), {:.2} seconds, {} bps",
            codec_desc, length, bitrate
        )
    }
}

// ---------------------------------------------------------------------------
// Async loaders for MP4Info and MP4Chapters — use targeted async reads
// instead of buffering the entire file into memory.
// ---------------------------------------------------------------------------

#[cfg(feature = "async")]
impl MP4Info {
    /// Async version of `load`. Reads only the specific atoms needed for
    /// stream info via async I/O, avoiding full-file buffering.
    pub async fn load_async(atoms: &Atoms, file: &mut tokio::fs::File) -> Result<Self> {
        let mut info = MP4Info::default();

        let moov = atoms
            .get("moov")
            .ok_or_else(|| AudexError::ParseError("not a MP4 file - no moov atom".to_string()))?;

        // Find audio track by reading handler atoms asynchronously
        let audio_trak = Self::find_audio_track_async(moov, file).await?;

        // Parse media header for duration
        if let Some(mdhd) = audio_trak.get_child(&["mdia", "mdhd"]) {
            let data = mdhd.read_data_async(file).await?;
            info.parse_mdhd_data(&data)?;
        }

        // Parse sample description for codec info
        if let Some(stsd) = audio_trak.get_child(&["mdia", "minf", "stbl", "stsd"]) {
            let data = stsd.read_data_async(file).await?;
            info.parse_stsd_data(&data)?;
        }

        Ok(info)
    }

    /// Async version of `find_audio_track`.
    async fn find_audio_track_async<'a>(
        moov: &'a MP4Atom,
        file: &mut tokio::fs::File,
    ) -> Result<&'a MP4Atom> {
        if let Some(children) = &moov.children {
            for trak in children {
                if trak.name == *b"trak" {
                    if let Some(hdlr) = trak.get_child(&["mdia", "hdlr"]) {
                        let data = hdlr.read_data_async(file).await?;
                        if Self::is_audio_handler(&data) {
                            return Ok(trak);
                        }
                    }
                }
            }
        }

        Err(AudexError::ParseError(
            "track has no audio data".to_string(),
        ))
    }
}

#[cfg(feature = "async")]
impl MP4Chapters {
    /// Async version of `load`. Reads chapter atoms via async I/O.
    pub async fn load_async(atoms: &Atoms, file: &mut tokio::fs::File) -> Result<Option<Self>> {
        if !Self::can_load(atoms) {
            return Ok(None);
        }

        let mut chapters = MP4Chapters::new();

        // Parse mvhd for timescale and duration
        if let Some(mvhd) = atoms.get("moov.mvhd") {
            let data = mvhd.read_data_async(file).await?;
            chapters.parse_mvhd_data(&data)?;
        }

        if chapters.timescale.is_none() {
            return Err(AudexError::ParseError(
                "Unable to get timescale".to_string(),
            ));
        }

        // Parse chpl for chapter data
        if let Some(chpl) = atoms.get("moov.udta.chpl") {
            let data = chpl.read_data_async(file).await?;
            chapters.parse_chpl_data(&data)?;
        }

        Ok(Some(chapters))
    }
}
