//! APEv2 tag support for audio files
//!
//! APEv2 (APE Tag Version 2) is a flexible tag format originally developed for
//! Monkey's Audio (.ape) files, but now widely used by multiple lossless and
//! lossy audio formats including WavPack, Musepack, OptimFROG, and TAK.
//!
//! # Overview
//!
//! APEv2 tags store metadata as key-value pairs with typed values. Unlike ID3v2's
//! frame-based structure, APEv2 uses a simpler dictionary model with three value types:
//! text, binary, and external (URI references).
//!
//! ## Key Features
//!
//! - **Case-insensitive keys**: Keys are case-insensitive but preserve original case
//! - **Flexible keys**: Any ASCII printable characters (0x20-0x7E), 2-255 characters long
//! - **Typed values**: Text (UTF-8), Binary (raw data), or External (URI)
//! - **Multi-value support**: Multiple values separated by null bytes (0x00)
//! - **Cover art support**: Binary values for embedded album artwork
//! - **UTF-8 encoding**: All text is stored as UTF-8 (unlike ID3's multiple encodings)
//!
//! ## Value Types
//!
//! - **Text** (`APEValueType::Text`): UTF-8 encoded strings, can contain multiple
//!   values separated by null bytes
//! - **Binary** (`APEValueType::Binary`): Raw binary data (e.g., cover art images)
//! - **External** (`APEValueType::External`): URI references to external resources
//!
//! # Basic Usage
//!
//! ## Reading APEv2 Tags
//!
//! ```no_run
//! use audex::FileType;
//! use audex::wavpack::WavPack;
//!
//! // Load a WavPack file with APEv2 tags
//! let audio = WavPack::load("song.wv").unwrap();
//!
//! if let Some(tags) = &audio.tags {
//!     // Get a text value
//!     if let Some(title) = tags.get("Title") {
//!         println!("Title: {}", title.pprint());
//!     }
//!
//!     // Get all keys
//!     for key in tags.keys() {
//!         println!("Found tag: {}", key);
//!     }
//! }
//! ```
//!
//! ## Writing APEv2 Tags
//!
//! ```no_run
//! use audex::apev2::{APEv2Tags, APEValue};
//! use audex::FileType;
//! use audex::wavpack::WavPack;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut audio = WavPack::load("song.wv")?;
//!
//! // Create new tags if none exist
//! let mut tags = audio.tags.unwrap_or_else(|| APEv2Tags::new());
//!
//! // Set text values
//! tags.set_text("Title", "My Song".to_string())?;
//! tags.set_text("Artist", "My Band".to_string())?;
//!
//! // Set multi-value text (e.g., multiple artists)
//! tags.set_text_list("Artist", vec![
//!     "Artist 1".to_string(),
//!     "Artist 2".to_string(),
//! ])?;
//!
//! audio.tags = Some(tags);
//! audio.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Adding Cover Art
//!
//! ```no_run
//! use audex::apev2::APEv2Tags;
//! use std::fs;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut tags = APEv2Tags::new();
//!
//! // Read image file
//! let image_data = fs::read("cover.jpg")?;
//!
//! // Add as binary value
//! tags.add_cover_art("Cover Art (Front)", image_data)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Standard Tag Keys
//!
//! While APEv2 allows arbitrary keys, these are commonly used standard keys:
//!
//! ## Basic Metadata
//! - `Title`: Track title
//! - `Artist`: Track artist(s)
//! - `Album`: Album title
//! - `Album Artist`: Album artist (for compilations)
//! - `Year`: Release year
//! - `Genre`: Genre(s)
//! - `Track`: Track number (format: "1" or "1/12")
//! - `Disc`: Disc number (format: "1" or "1/2")
//!
//! ## Additional Info
//! - `Comment`: Freeform comment
//! - `Composer`: Composer name(s)
//! - `Conductor`: Conductor name
//! - `Publisher`: Record label/publisher
//! - `Copyright`: Copyright information
//! - `ISRC`: International Standard Recording Code
//! - `Catalog`: Catalog number
//! - `BPM`: Beats per minute (tempo)
//!
//! ## Cover Art
//! - `Cover Art (Front)`: Front cover image
//! - `Cover Art (Back)`: Back cover image
//! - `Cover Art (Leaflet)`: Booklet/leaflet pages
//! - `Cover Art (Media)`: Disc/media label
//! - `Cover Art (Artist)`: Artist/performer photo
//! - `Cover Art (Icon)`: Small icon
//!
//! ## ReplayGain
//! - `REPLAYGAIN_TRACK_GAIN`: Track gain value
//! - `REPLAYGAIN_TRACK_PEAK`: Track peak value
//! - `REPLAYGAIN_ALBUM_GAIN`: Album gain value
//! - `REPLAYGAIN_ALBUM_PEAK`: Album peak value
//!
//! # Key Constraints
//!
//! APEv2 key names must follow these rules:
//! - 2-255 characters long
//! - ASCII printable characters only (0x20-0x7E)
//! - Cannot be "ID3", "TAG", "OggS", or "MP+"
//! - Case-insensitive but case-preserving
//!
//! # File Format Compatibility
//!
//! APEv2 tags are supported by these formats:
//! - Monkey's Audio (.ape)
//! - WavPack (.wv)
//! - Musepack (.mpc)
//! - OptimFROG (.ofr, .ofs)
//! - TAK (.tak)
//! - MP3 (.mp3) - less common, ID3v2 preferred
//!
//! # See Also
//!
//! - `APEv2Tags`: Main tag container
//! - `APEValue`: Individual tag value
//! - `APEValueType`: Value type enumeration

use crate::{AudexError, FileType, ReadWriteSeek, Result, StreamInfo, tags::Tags};
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

/// Upper bound on the number of items allowed in a single APE tag.
/// Real-world tags rarely exceed a few hundred entries; anything above
/// this threshold almost certainly indicates a corrupt or malicious header.
const MAX_APE_ITEMS: u32 = 10_000;

#[cfg(feature = "async")]
use crate::util::{loadfile_read_async, loadfile_write_async, resize_bytes_async};
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// APEv2 value type identifier
///
/// Specifies the type of data stored in an APEv2 tag value. This determines
/// how the raw bytes should be interpreted and displayed.
///
/// # Value Types
///
/// - **Text**: UTF-8 encoded text, the most common type
/// - **Binary**: Raw binary data (images, etc.)
/// - **External**: URI/URL reference to external resource
///
/// # Examples
///
/// ```
/// use audex::apev2::{APEValueType, APEValue};
///
/// // Create different value types
/// let text = APEValue::text("Song Title");
/// assert_eq!(text.value_type, APEValueType::Text);
///
/// let binary = APEValue::binary(vec![0xFF, 0xD8, 0xFF]);
/// assert_eq!(binary.value_type, APEValueType::Binary);
///
/// let external = APEValue::external("http://example.com/cover.jpg");
/// assert_eq!(external.value_type, APEValueType::External);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum APEValueType {
    /// UTF-8 encoded text value (flag value: 0)
    ///
    /// Multiple text values can be stored separated by null bytes (0x00).
    /// This is the most commonly used type for metadata like titles, artists, etc.
    Text = 0,

    /// Binary data value (flag value: 1)
    ///
    /// Raw binary data, typically used for embedded images (cover art).
    /// The format is determined by content (JPEG, PNG, etc.).
    Binary = 1,

    /// External reference/URI value (flag value: 2)
    ///
    /// UTF-8 encoded URI pointing to an external resource.
    /// Stored as text but semantically represents a link.
    External = 2,
}

/// APE tag flags
const HAS_HEADER: u32 = 1 << 31; // Header present
const IS_HEADER: u32 = 1 << 29; // This is header (vs footer)

/// APEv2 tag value container
///
/// Represents a single value in an APEv2 tag. Each value has a type
/// (text, binary, or external) and associated raw data bytes.
///
/// # Fields
///
/// - `value_type`: The type of data stored (Text, Binary, or External)
/// - `data`: Raw bytes of the value (UTF-8 text or binary data)
///
/// # Examples
///
/// ## Creating Text Values
///
/// ```
/// use audex::apev2::APEValue;
///
/// // Single text value
/// let title = APEValue::text("My Song");
///
/// // Get it back as string
/// assert_eq!(title.as_string().unwrap(), "My Song");
/// ```
///
/// ## Creating Multi-Value Text
///
/// ```
/// use audex::apev2::APEValue;
///
/// // Multiple values joined by null bytes
/// let artists = APEValue::text("Artist 1\0Artist 2\0Artist 3");
///
/// // Split into individual values
/// let artist_list = artists.as_text_list().unwrap();
/// assert_eq!(artist_list.len(), 3);
/// assert_eq!(artist_list[0], "Artist 1");
/// ```
///
/// ## Creating Binary Values
///
/// ```
/// use audex::apev2::APEValue;
///
/// // Binary data (e.g., image file)
/// let image_data = vec![0xFF, 0xD8, 0xFF]; // JPEG header
/// let cover = APEValue::binary(image_data);
///
/// // Binary values can't be converted to strings
/// assert!(cover.as_string().is_err());
/// ```
///
/// ## Creating External References
///
/// ```
/// use audex::apev2::APEValue;
///
/// // URL to external resource
/// let url = APEValue::external("http://example.com/artwork.jpg");
/// assert_eq!(url.as_string().unwrap(), "http://example.com/artwork.jpg");
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct APEValue {
    /// The type of value (Text, Binary, or External)
    pub value_type: APEValueType,

    /// Raw byte data of the value
    ///
    /// For Text and External types, this should be valid UTF-8.
    /// For Binary types, this can be any data.
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::bytes_as_base64")
    )]
    pub data: Vec<u8>,
}

impl APEValue {
    /// Create new text value
    pub fn text<S: Into<String>>(text: S) -> Self {
        Self {
            value_type: APEValueType::Text,
            data: text.into().as_bytes().to_vec(),
        }
    }

    /// Create new binary value
    pub fn binary(data: Vec<u8>) -> Self {
        Self {
            value_type: APEValueType::Binary,
            data,
        }
    }

    /// Create new external value (URI)
    pub fn external<S: Into<String>>(uri: S) -> Self {
        Self {
            value_type: APEValueType::External,
            data: uri.into().as_bytes().to_vec(),
        }
    }

    pub fn as_string(&self) -> Result<String> {
        match self.value_type {
            APEValueType::Text | APEValueType::External => String::from_utf8(self.data.clone())
                .map_err(|e| AudexError::InvalidData(format!("Invalid UTF-8: {}", e))),
            APEValueType::Binary => Err(AudexError::InvalidData(
                "Cannot convert binary data to string".to_string(),
            )),
        }
    }

    pub fn as_text_list(&self) -> Result<Vec<String>> {
        let text = self.as_string()?;
        Ok(text.split('\0').map(|s| s.to_string()).collect())
    }

    pub fn pprint(&self) -> String {
        match self.value_type {
            APEValueType::Text => {
                if let Ok(text_list) = self.as_text_list() {
                    text_list.join(" / ")
                } else {
                    format!("[Invalid text: {} bytes]", self.data.len())
                }
            }
            APEValueType::Binary => {
                format!("[{} bytes]", self.data.len())
            }
            APEValueType::External => {
                if let Ok(uri) = self.as_string() {
                    format!("[External] {}", uri)
                } else {
                    format!("[Invalid external: {} bytes]", self.data.len())
                }
            }
        }
    }
}

/// APEv2 tag container with case-insensitive key lookup
///
/// A key-value container for APEv2 tags.
/// Keys are case-insensitive for lookup but preserve their original case.
///
/// # Key Features
///
/// - **Case-insensitive lookup**: `tags.get("title")` and `tags.get("Title")` return the same value
/// - **Case preservation**: Original key capitalization is maintained
/// - **Type-safe values**: Each value has an explicit type (Text, Binary, External)
/// - **Multiple values**: Text values can contain multiple entries separated by null bytes
/// - **Cover art support**: Convenience methods for adding and retrieving embedded images
///
/// # Examples
///
/// ## Basic Usage
///
/// ```
/// use audex::apev2::APEv2Tags;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut tags = APEv2Tags::new();
///
/// // Set text values
/// tags.set_text("Title", "My Song".to_string())?;
/// tags.set_text("Artist", "My Band".to_string())?;
///
/// // Get values (case-insensitive)
/// assert!(tags.get("title").is_some());
/// assert!(tags.get("TITLE").is_some());
/// assert!(tags.get("Title").is_some());
/// # Ok(())
/// # }
/// ```
///
/// ## Multi-Value Fields
///
/// ```
/// use audex::apev2::APEv2Tags;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut tags = APEv2Tags::new();
///
/// // Multiple artists
/// tags.set_text_list("Artist", vec![
///     "Artist 1".to_string(),
///     "Artist 2".to_string(),
/// ])?;
///
/// // Retrieve as list
/// let artist = tags.get("Artist").unwrap();
/// let artists = artist.as_text_list()?;
/// assert_eq!(artists.len(), 2);
/// # Ok(())
/// # }
/// ```
///
/// ## Cover Art
///
/// ```
/// use audex::apev2::APEv2Tags;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut tags = APEv2Tags::new();
/// let image_data = vec![0xFF, 0xD8, 0xFF]; // JPEG header
///
/// // Add cover art
/// tags.add_cover_art("Cover Art (Front)", image_data.clone())?;
///
/// // Retrieve cover art
/// if let Some(image) = tags.get_cover_art("Cover Art (Front)") {
///     assert_eq!(image, &image_data[..]);
/// }
/// # Ok(())
/// # }
/// ```
///
/// # Internal Structure
///
/// The struct maintains two hash maps:
/// - `items`: Stores lowercase keys → values
/// - `case_map`: Maps lowercase keys → original case keys
///
/// This allows efficient case-insensitive lookups while preserving
/// the user's preferred capitalization for display.
#[derive(Debug, Default, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct APEv2Tags {
    /// Internal storage: lowercase keys to values
    items: HashMap<String, APEValue>,

    /// Case preservation: lowercase keys to original case keys
    case_map: HashMap<String, String>,
}

impl Tags for APEv2Tags {
    fn get(&self, _key: &str) -> Option<&[String]> {
        None
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        let joined = values.join("\0");
        let _ = self.set_text(key, joined);
    }

    fn remove(&mut self, key: &str) {
        APEv2Tags::remove(self, key);
    }

    fn keys(&self) -> Vec<String> {
        APEv2Tags::keys(self)
    }

    fn pprint(&self) -> String {
        let mut result = String::new();
        let mut keys: Vec<_> = self.keys();
        keys.sort();

        for key in keys {
            if let Some(ape_value) = self.get(&key) {
                if let Ok(value_str) = ape_value.as_string() {
                    result.push_str(&format!("{}={}\n", key, value_str));
                }
            }
        }

        result
    }
}

impl APEv2Tags {
    /// Create a new empty APEv2 tag collection.
    pub fn new() -> Self {
        Self {
            items: HashMap::new(),
            case_map: HashMap::new(),
        }
    }

    /// Get value by key (case-insensitive)
    pub fn get(&self, key: &str) -> Option<&APEValue> {
        self.items.get(&key.to_lowercase())
    }

    /// Set value with key (preserves case)
    pub fn set(&mut self, key: &str, value: APEValue) -> Result<()> {
        if !is_valid_apev2_key(key) {
            return Err(AudexError::InvalidData(format!(
                "{:?} is not a valid APEv2 key",
                key
            )));
        }

        let lower_key = key.to_lowercase();
        self.case_map.insert(lower_key.clone(), key.to_string());
        self.items.insert(lower_key, value);
        Ok(())
    }

    pub fn set_text(&mut self, key: &str, text: String) -> Result<()> {
        self.set(key, APEValue::text(text))
    }

    pub fn set_text_list(&mut self, key: &str, texts: Vec<String>) -> Result<()> {
        self.set(key, APEValue::text(texts.join("\0")))
    }

    /// Remove key (case-insensitive)
    pub fn remove(&mut self, key: &str) -> Option<APEValue> {
        let lower_key = key.to_lowercase();
        self.case_map.remove(&lower_key);
        self.items.remove(&lower_key)
    }

    /// Get all keys (with preserved case)
    pub fn keys(&self) -> Vec<String> {
        self.items
            .keys()
            .map(|k| self.case_map.get(k).unwrap_or(k).clone())
            .collect()
    }

    pub fn values(&self) -> Vec<&APEValue> {
        self.items.values().collect()
    }

    pub fn items(&self) -> Vec<(String, &APEValue)> {
        self.items
            .iter()
            .map(|(k, v)| (self.case_map.get(k).unwrap_or(k).clone(), v))
            .collect()
    }

    /// Check if key exists (case-insensitive)
    pub fn contains_key(&self, key: &str) -> bool {
        self.items.contains_key(&key.to_lowercase())
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.case_map.clear();
    }

    /// Pretty print all tags
    pub fn pprint(&self) -> String {
        let mut lines: Vec<String> = self
            .items()
            .iter()
            .map(|(k, v)| format!("{}={}", k, v.pprint()))
            .collect();
        lines.sort();
        lines.join("\n")
    }

    /// Add cover art as a binary value
    ///
    /// Standard cover art keys in APEv2:
    /// - "Cover Art (Front)" - Front cover image
    /// - "Cover Art (Back)" - Back cover image
    /// - "Cover Art (Leaflet)" - Leaflet/booklet pages
    /// - "Cover Art (Media)" - Media/disc image
    /// - "Cover Art (Artist)" - Artist/performer image
    /// - "Cover Art (Icon)" - Small icon
    ///
    /// The image data is stored as a binary value.
    ///
    /// # Arguments
    /// * `key` - The cover art key (e.g., "Cover Art (Front)")
    /// * `image_data` - Raw binary image data (JPEG, PNG, etc.)
    ///
    /// # Example
    /// ```rust
    /// use audex::apev2::APEv2Tags;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut tags = APEv2Tags::new();
    /// let image_data = vec![0xFF, 0xD8, 0xFF]; // Minimal JPEG header
    /// tags.add_cover_art("Cover Art (Front)", image_data)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn add_cover_art(&mut self, key: &str, image_data: Vec<u8>) -> Result<()> {
        self.set(key, APEValue::binary(image_data))
    }

    /// Get cover art data for a specific key
    ///
    /// # Arguments
    /// * `key` - The cover art key (e.g., "Cover Art (Front)")
    ///
    /// # Returns
    /// The binary image data if found and is binary type, None otherwise
    ///
    /// # Example
    /// ```rust
    /// use audex::apev2::APEv2Tags;
    ///
    /// let mut tags = APEv2Tags::new();
    /// tags.add_cover_art("Cover Art (Front)", vec![0xFF, 0xD8, 0xFF]).unwrap();
    /// if let Some(image_data) = tags.get_cover_art("Cover Art (Front)") {
    ///     assert_eq!(image_data, &[0xFF, 0xD8, 0xFF]);
    /// }
    /// ```
    pub fn get_cover_art(&self, key: &str) -> Option<&[u8]> {
        self.get(key).and_then(|value| {
            if value.value_type == APEValueType::Binary {
                Some(&value.data[..])
            } else {
                None
            }
        })
    }

    /// Get all cover art images from the tags
    ///
    /// Returns a vector of (key, image_data) tuples for all keys starting with "Cover Art"
    ///
    /// # Example
    /// ```rust
    /// use audex::apev2::APEv2Tags;
    ///
    /// let mut tags = APEv2Tags::new();
    /// tags.add_cover_art("Cover Art (Front)", vec![0xFF, 0xD8, 0xFF]).unwrap();
    /// tags.add_cover_art("Cover Art (Back)", vec![0x89, 0x50, 0x4E, 0x47]).unwrap();
    /// for (key, image_data) in tags.get_all_cover_art() {
    ///     println!("Found cover art: {} ({} bytes)", key, image_data.len());
    /// }
    /// ```
    pub fn get_all_cover_art(&self) -> Vec<(String, &[u8])> {
        self.items()
            .into_iter()
            .filter_map(|(key, value)| {
                if key.to_lowercase().starts_with("cover art")
                    && value.value_type == APEValueType::Binary
                {
                    Some((key, &value.data[..]))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Remove cover art for a specific key
    ///
    /// # Arguments
    /// * `key` - The cover art key to remove
    ///
    /// # Example
    /// ```rust
    /// use audex::apev2::APEv2Tags;
    ///
    /// let mut tags = APEv2Tags::new();
    /// tags.add_cover_art("Cover Art (Front)", vec![0xFF, 0xD8, 0xFF]).unwrap();
    /// assert!(tags.get_cover_art("Cover Art (Front)").is_some());
    /// tags.remove_cover_art("Cover Art (Front)");
    /// assert!(tags.get_cover_art("Cover Art (Front)").is_none());
    /// ```
    pub fn remove_cover_art(&mut self, key: &str) -> Option<APEValue> {
        self.remove(key)
    }

    /// Remove all cover art from the tags
    ///
    /// Removes all keys starting with "Cover Art"
    ///
    /// # Example
    /// ```rust
    /// use audex::apev2::APEv2Tags;
    ///
    /// let mut tags = APEv2Tags::new();
    /// tags.add_cover_art("Cover Art (Front)", vec![0xFF, 0xD8, 0xFF]).unwrap();
    /// tags.add_cover_art("Cover Art (Back)", vec![0x89, 0x50, 0x4E, 0x47]).unwrap();
    /// assert_eq!(tags.get_all_cover_art().len(), 2);
    /// tags.clear_all_cover_art();
    /// assert_eq!(tags.get_all_cover_art().len(), 0);
    /// ```
    pub fn clear_all_cover_art(&mut self) {
        let cover_keys: Vec<String> = self
            .keys()
            .into_iter()
            .filter(|k| k.to_lowercase().starts_with("cover art"))
            .collect();

        for key in cover_keys {
            self.remove(&key);
        }
    }
}

/// APE tag data structure for parsing
#[derive(Debug)]
struct APEData {
    start: Option<u64>,
    header: Option<u64>,
    data: Option<u64>,
    footer: Option<u64>,
    end: Option<u64>,
    metadata: Option<u64>,

    version: u32,
    size: u32,
    items: u32,
    flags: u32,

    is_at_start: bool,
    tag_data: Option<Vec<u8>>,
}

impl APEData {
    fn new() -> Self {
        Self {
            start: None,
            header: None,
            data: None,
            footer: None,
            end: None,
            metadata: None,
            version: 0,
            size: 0,
            items: 0,
            flags: 0,
            is_at_start: false,
            tag_data: None,
        }
    }

    /// Find APE metadata in file
    fn find_metadata<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Check for simple footer at end
        if reader.seek(SeekFrom::End(-32)).is_ok() {
            let mut buf = [0u8; 8];
            if reader.read_exact(&mut buf).is_ok() && &buf == b"APETAGEX" {
                reader.seek(SeekFrom::Current(-8))?;
                self.footer = Some(reader.stream_position()?);
                return Ok(());
            }
        }

        // Check for APEv2 + ID3v1 at end
        if reader.seek(SeekFrom::End(-128)).is_ok() {
            let mut tag_buf = [0u8; 3];
            if reader.read_exact(&mut tag_buf).is_ok() && &tag_buf == b"TAG" {
                // Found ID3v1, check for APE before it
                reader.seek(SeekFrom::Current(-35))?; // "TAG" + header length
                let mut ape_buf = [0u8; 8];
                if reader.read_exact(&mut ape_buf).is_ok() && &ape_buf == b"APETAGEX" {
                    reader.seek(SeekFrom::Current(-8))?;
                    self.footer = Some(reader.stream_position()?);
                    return Ok(());
                }

                // Check for Lyrics3v2 before APE
                reader.seek(SeekFrom::Current(15))?;
                let mut lyrics_buf = [0u8; 9];
                if reader.read_exact(&mut lyrics_buf).is_ok() && &lyrics_buf == b"LYRICS200" {
                    reader.seek(SeekFrom::Current(-15))?;
                    let mut size_buf = [0u8; 6];
                    if reader.read_exact(&mut size_buf).is_ok() {
                        if let Ok(size_str) = std::str::from_utf8(&size_buf) {
                            if let Ok(offset) = size_str.parse::<i64>() {
                                // Validate that the Lyrics3v2 offset is
                                // positive and that the resulting seek
                                // target stays within the file.  A corrupt
                                // or crafted size field could produce a
                                // negative absolute position otherwise.
                                let current = reader.stream_position()?;
                                // Use checked arithmetic to guard against overflow
                                let seek_delta = match (-32i64)
                                    .checked_sub(offset)
                                    .and_then(|v| v.checked_sub(6))
                                {
                                    Some(d) => d,
                                    None => {
                                        return Err(AudexError::InvalidData(
                                            "Lyrics3v2 seek offset overflow".into(),
                                        ));
                                    }
                                };
                                // Use checked arithmetic to detect overflow
                                // instead of silently clamping to i64::MAX
                                let target = match (current as i64).checked_add(seek_delta) {
                                    Some(t) => t,
                                    None => {
                                        return Err(AudexError::InvalidData(
                                            "Lyrics3v2 seek target overflow".into(),
                                        ));
                                    }
                                };
                                if offset > 0 && target >= 0 {
                                    reader.seek(SeekFrom::Current(seek_delta))?;
                                    let mut ape_buf = [0u8; 8];
                                    if reader.read_exact(&mut ape_buf).is_ok()
                                        && &ape_buf == b"APETAGEX"
                                    {
                                        reader.seek(SeekFrom::Current(-8))?;
                                        self.footer = Some(reader.stream_position()?);
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check for tag at start
        reader.seek(SeekFrom::Start(0))?;
        let mut buf = [0u8; 8];
        if reader.read_exact(&mut buf).is_ok() && &buf == b"APETAGEX" {
            self.is_at_start = true;
            self.header = Some(0);
        }

        Ok(())
    }

    /// Fill missing metadata fields
    fn fill_missing<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Determine which metadata to read from
        self.metadata = if self.header.is_some() && self.footer.is_some() {
            std::cmp::max(self.header, self.footer)
        } else if self.header.is_some() {
            self.header
        } else {
            self.footer
        };

        let metadata_pos = self.metadata.ok_or(AudexError::APENoHeader)?;

        reader.seek(SeekFrom::Start(metadata_pos + 8))?;

        let mut buf = [0u8; 16];
        reader.read_exact(&mut buf)?;

        self.version = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        self.size = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        self.items = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
        self.flags = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);

        // Reject absurd item counts from malformed or malicious headers.
        // Real-world APE tags rarely exceed a few hundred items.
        if self.items > MAX_APE_ITEMS {
            return Err(AudexError::InvalidData(format!(
                "APEv2 item count {} exceeds maximum of {}",
                self.items, MAX_APE_ITEMS
            )));
        }

        if let Some(header) = self.header {
            // Use checked arithmetic to prevent overflow with malformed offsets
            let data = header
                .checked_add(32)
                .ok_or_else(|| AudexError::InvalidData("APE header offset overflow".into()))?;
            let end = data.checked_add(self.size as u64).ok_or_else(|| {
                AudexError::InvalidData("APEv2 tag end offset overflow".to_string())
            })?;
            self.data = Some(data);
            self.end = Some(end);

            // Check for footer (only valid when end can hold one)
            if end >= 32 {
                reader.seek(SeekFrom::Start(end - 32))?;
                let mut footer_buf = [0u8; 8];
                if reader.read_exact(&mut footer_buf).is_ok() && &footer_buf == b"APETAGEX" {
                    self.footer = Some(end - 32);
                }
            }
        } else if let Some(footer) = self.footer {
            let end = footer + 32;
            let tag_size = self.size as u64;

            // The claimed tag size must fit between the file start and end
            if tag_size > end {
                return Err(AudexError::InvalidData(format!(
                    "APE footer claims {} bytes but only {} bytes precede it",
                    tag_size, end
                )));
            }
            let data = end - tag_size;
            self.end = Some(end);
            self.data = Some(data);

            if self.flags & HAS_HEADER != 0 {
                // Header occupies 32 bytes before the data region
                if data < 32 {
                    return Err(AudexError::InvalidData(format!(
                        "APE header flag set but data starts at offset {} (need at least 32)",
                        data
                    )));
                }
                self.header = Some(data - 32);
            } else {
                self.header = self.data;
            }
        } else {
            return Err(AudexError::APENoHeader);
        }

        // Exclude footer from size (footer is 32 bytes)
        if self.footer.is_some() {
            if self.size < 32 {
                return Err(AudexError::InvalidData(format!(
                    "APE tag size {} is smaller than the 32-byte footer",
                    self.size
                )));
            }
            self.size -= 32;
        }

        Ok(())
    }

    /// Fix broken tags and find actual start
    fn fix_brokenness<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        let mut start = if let Some(header) = self.header {
            header
        } else if let Some(data) = self.data {
            data
        } else {
            return Ok(());
        };

        reader.seek(SeekFrom::Start(start))?;

        // Clean up broken writing from legacy implementations.
        // Cap the number of iterations to prevent unbounded scanning on
        // malformed files with many consecutive APETAGEX signatures.
        const MAX_SCAN_ITERATIONS: u32 = 1000;
        let mut iterations = 0u32;
        while start > 0 {
            if iterations >= MAX_SCAN_ITERATIONS {
                break;
            }
            iterations += 1;

            if reader.seek(SeekFrom::Current(-24)).is_err() {
                break;
            }

            let mut buf = [0u8; 8];
            if reader.read_exact(&mut buf).is_ok() && &buf == b"APETAGEX" {
                reader.seek(SeekFrom::Current(-8))?;
                start = reader.stream_position()?;
            } else {
                break;
            }
        }

        self.start = Some(start);
        Ok(())
    }

    /// Read tag data from file.
    ///
    /// Validates that the claimed tag size does not exceed either the hard
    /// cap or the actual remaining bytes in the stream, preventing large
    /// allocations from spoofed headers in small files.
    fn read_tag_data<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        limits: crate::limits::ParseLimits,
    ) -> Result<()> {
        if let Some(data_pos) = self.data {
            limits.check_tag_size(self.size as u64, "APEv2")?;

            // Cross-validate against actual stream size to prevent allocating
            // far more memory than the file contains
            let stream_end = reader.seek(SeekFrom::End(0))?;
            let available = stream_end.saturating_sub(data_pos);
            if (self.size as u64) > available {
                return Err(AudexError::ParseError(format!(
                    "APE tag size ({} bytes) exceeds remaining stream data ({} bytes)",
                    self.size, available
                )));
            }

            reader.seek(SeekFrom::Start(data_pos))?;
            let mut tag_data = vec![0u8; self.size as usize];
            reader.read_exact(&mut tag_data)?;
            self.tag_data = Some(tag_data);
        }
        Ok(())
    }
}

/// APE tag metadata structure for async parsing
#[cfg(feature = "async")]
#[derive(Debug)]
struct APEDataAsync {
    start: Option<u64>,
    header: Option<u64>,
    data: Option<u64>,
    footer: Option<u64>,
    end: Option<u64>,
    metadata: Option<u64>,

    version: u32,
    size: u32,
    items: u32,
    flags: u32,

    is_at_start: bool,
    tag_data: Option<Vec<u8>>,
}

#[cfg(feature = "async")]
impl APEDataAsync {
    fn new() -> Self {
        Self {
            start: None,
            header: None,
            data: None,
            footer: None,
            end: None,
            metadata: None,
            version: 0,
            size: 0,
            items: 0,
            flags: 0,
            is_at_start: false,
            tag_data: None,
        }
    }

    /// Find APE metadata in file asynchronously
    async fn find_metadata(&mut self, file: &mut TokioFile) -> Result<()> {
        // Check for simple footer at end of file
        if file.seek(SeekFrom::End(-32)).await.is_ok() {
            let mut buf = [0u8; 8];
            if file.read_exact(&mut buf).await.is_ok() && &buf == b"APETAGEX" {
                file.seek(SeekFrom::Current(-8)).await?;
                self.footer = Some(file.stream_position().await?);
                return Ok(());
            }
        }

        // Check for APEv2 + ID3v1 at end
        if file.seek(SeekFrom::End(-128)).await.is_ok() {
            let mut tag_buf = [0u8; 3];
            if file.read_exact(&mut tag_buf).await.is_ok() && &tag_buf == b"TAG" {
                // Found ID3v1, check for APE tag before it
                file.seek(SeekFrom::Current(-35)).await?;
                let mut ape_buf = [0u8; 8];
                if file.read_exact(&mut ape_buf).await.is_ok() && &ape_buf == b"APETAGEX" {
                    file.seek(SeekFrom::Current(-8)).await?;
                    self.footer = Some(file.stream_position().await?);
                    return Ok(());
                }

                // Check for Lyrics3v2 before APE
                file.seek(SeekFrom::Current(15)).await?;
                let mut lyrics_buf = [0u8; 9];
                if file.read_exact(&mut lyrics_buf).await.is_ok() && &lyrics_buf == b"LYRICS200" {
                    file.seek(SeekFrom::Current(-15)).await?;
                    let mut size_buf = [0u8; 6];
                    if file.read_exact(&mut size_buf).await.is_ok() {
                        if let Ok(size_str) = std::str::from_utf8(&size_buf) {
                            if let Ok(offset) = size_str.parse::<i64>() {
                                // Validate that the Lyrics3v2 offset is
                                // positive and that the resulting seek
                                // target stays within the file.
                                let current = file.stream_position().await?;
                                // Use checked arithmetic to guard against overflow
                                let seek_delta = match (-32i64)
                                    .checked_sub(offset)
                                    .and_then(|v| v.checked_sub(6))
                                {
                                    Some(d) => d,
                                    None => {
                                        return Err(AudexError::InvalidData(
                                            "Lyrics3v2 seek offset overflow".into(),
                                        ));
                                    }
                                };
                                // Use checked arithmetic to detect overflow
                                // instead of silently clamping to i64::MAX
                                let target = match (current as i64).checked_add(seek_delta) {
                                    Some(t) => t,
                                    None => {
                                        return Err(AudexError::InvalidData(
                                            "Lyrics3v2 seek target overflow".into(),
                                        ));
                                    }
                                };
                                if offset > 0 && target >= 0 {
                                    file.seek(SeekFrom::Current(seek_delta)).await?;
                                    let mut ape_buf = [0u8; 8];
                                    if file.read_exact(&mut ape_buf).await.is_ok()
                                        && &ape_buf == b"APETAGEX"
                                    {
                                        file.seek(SeekFrom::Current(-8)).await?;
                                        self.footer = Some(file.stream_position().await?);
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check for tag at start of file
        file.seek(SeekFrom::Start(0)).await?;
        let mut buf = [0u8; 8];
        if file.read_exact(&mut buf).await.is_ok() && &buf == b"APETAGEX" {
            self.is_at_start = true;
            self.header = Some(0);
        }

        Ok(())
    }

    /// Fill missing metadata fields asynchronously
    async fn fill_missing(&mut self, file: &mut TokioFile) -> Result<()> {
        // Determine which metadata position to use
        self.metadata = if self.header.is_some() && self.footer.is_some() {
            std::cmp::max(self.header, self.footer)
        } else if self.header.is_some() {
            self.header
        } else {
            self.footer
        };

        let metadata_pos = self.metadata.ok_or(AudexError::APENoHeader)?;

        // Read metadata header (16 bytes after signature)
        file.seek(SeekFrom::Start(metadata_pos + 8)).await?;
        let mut buf = [0u8; 16];
        file.read_exact(&mut buf).await?;

        // Parse metadata fields (little-endian)
        self.version = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        self.size = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
        self.items = u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]);
        self.flags = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);

        // Reject absurd item counts from malformed or malicious headers.
        if self.items > MAX_APE_ITEMS {
            return Err(AudexError::InvalidData(format!(
                "APEv2 item count {} exceeds maximum of {}",
                self.items, MAX_APE_ITEMS
            )));
        }

        // Calculate positions based on header or footer location
        if let Some(header) = self.header {
            // Use checked arithmetic to prevent overflow with malformed offsets
            let data = header
                .checked_add(32)
                .ok_or_else(|| AudexError::InvalidData("APE header offset overflow".into()))?;
            let end = data.checked_add(self.size as u64).ok_or_else(|| {
                AudexError::InvalidData("APEv2 tag end offset overflow".to_string())
            })?;
            self.data = Some(data);
            self.end = Some(end);

            // Check for footer (only valid when end can hold one)
            if end >= 32 {
                file.seek(SeekFrom::Start(end - 32)).await?;
                let mut footer_buf = [0u8; 8];
                if file.read_exact(&mut footer_buf).await.is_ok() && &footer_buf == b"APETAGEX" {
                    self.footer = Some(end - 32);
                }
            }
        } else if let Some(footer) = self.footer {
            let end = footer + 32;
            let tag_size = self.size as u64;

            // The claimed tag size must fit between the file start and end
            if tag_size > end {
                return Err(AudexError::InvalidData(format!(
                    "APE footer claims {} bytes but only {} bytes precede it",
                    tag_size, end
                )));
            }
            let data = end - tag_size;
            self.end = Some(end);
            self.data = Some(data);

            if self.flags & HAS_HEADER != 0 {
                // Header occupies 32 bytes before the data region
                if data < 32 {
                    return Err(AudexError::InvalidData(format!(
                        "APE header flag set but data starts at offset {} (need at least 32)",
                        data
                    )));
                }
                self.header = Some(data - 32);
            } else {
                self.header = self.data;
            }
        } else {
            return Err(AudexError::APENoHeader);
        }

        // Exclude footer from size (footer is 32 bytes)
        if self.footer.is_some() {
            if self.size < 32 {
                return Err(AudexError::InvalidData(format!(
                    "APE tag size {} is smaller than the 32-byte footer",
                    self.size
                )));
            }
            self.size -= 32;
        }

        Ok(())
    }

    /// Fix broken tags and find actual start asynchronously
    async fn fix_brokenness(&mut self, file: &mut TokioFile) -> Result<()> {
        let mut start = if let Some(header) = self.header {
            header
        } else if let Some(data) = self.data {
            data
        } else {
            return Ok(());
        };

        file.seek(SeekFrom::Start(start)).await?;

        // Clean up broken writing from legacy implementations.
        // Cap the number of iterations to prevent unbounded scanning on
        // malformed files with many consecutive APETAGEX signatures.
        const MAX_SCAN_ITERATIONS: u32 = 1000;
        let mut iterations = 0u32;
        while start > 0 {
            if iterations >= MAX_SCAN_ITERATIONS {
                break;
            }
            iterations += 1;
            if file.seek(SeekFrom::Current(-24)).await.is_err() {
                break;
            }

            let mut buf = [0u8; 8];
            if file.read_exact(&mut buf).await.is_ok() && &buf == b"APETAGEX" {
                file.seek(SeekFrom::Current(-8)).await?;
                start = file.stream_position().await?;
            } else {
                break;
            }
        }

        self.start = Some(start);
        Ok(())
    }

    /// Read tag data from file asynchronously.
    ///
    /// Same stream-size cross-validation as the sync version.
    async fn read_tag_data(
        &mut self,
        file: &mut TokioFile,
        limits: crate::limits::ParseLimits,
    ) -> Result<()> {
        if let Some(data_pos) = self.data {
            limits.check_tag_size(self.size as u64, "APEv2 async")?;

            // Cross-validate against actual stream size
            let stream_end = file.seek(SeekFrom::End(0)).await?;
            let available = stream_end.saturating_sub(data_pos);
            if (self.size as u64) > available {
                return Err(AudexError::ParseError(format!(
                    "APE tag size ({} bytes) exceeds remaining stream data ({} bytes)",
                    self.size, available
                )));
            }

            file.seek(SeekFrom::Start(data_pos)).await?;
            let mut tag_data = vec![0u8; self.size as usize];
            file.read_exact(&mut tag_data).await?;
            self.tag_data = Some(tag_data);
        }
        Ok(())
    }
}

/// Empty stream info for unknown format with APE tags
#[derive(Debug, Default)]
pub struct APEStreamInfo {
    pub length: Option<Duration>,
    pub bitrate: Option<u32>,
}

impl StreamInfo for APEStreamInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }
    fn bitrate(&self) -> Option<u32> {
        self.bitrate
    }
    fn sample_rate(&self) -> Option<u32> {
        None
    }
    fn channels(&self) -> Option<u16> {
        None
    }
    fn bits_per_sample(&self) -> Option<u16> {
        None
    }
}

impl APEStreamInfo {
    pub fn pprint(&self) -> String {
        "Unknown format with APEv2 tag.".to_string()
    }
}

/// APEv2 tag file
#[derive(Debug)]
pub struct APEv2 {
    pub tags: APEv2Tags,
    pub info: APEStreamInfo,
    pub filename: Option<String>,
}

impl APEv2 {
    /// Create a new empty APEv2 file handler with no tags loaded.
    pub fn new() -> Self {
        Self {
            tags: APEv2Tags::new(),
            info: APEStreamInfo::default(),
            filename: None,
        }
    }

    /// Parse APE tag data from a raw byte slice containing item entries.
    /// The `item_count` must match the number of items encoded in the data.
    pub fn parse_tag(&mut self, tag_data: &[u8], item_count: u32) -> Result<()> {
        let mut cursor = Cursor::new(tag_data);

        // Fetch the library-wide limits once for this parse run
        let limits = crate::limits::ParseLimits::default();

        // Track cumulative bytes allocated across all items so that many
        // moderate-sized items cannot collectively exhaust memory.
        let mut cumulative_size: u64 = 0;

        for _item_index in 0..item_count {
            // Read item header (8 bytes)
            let mut header = [0u8; 8];
            if Read::read_exact(&mut cursor, &mut header).is_err() {
                // The tag header claimed more items than actually exist in the data.
                // This is common with poorly written taggers; warn and continue with
                // whatever items were successfully parsed.
                warn_event!(
                    declared = item_count,
                    parsed = _item_index,
                    "APEv2: tag header declared more items than data contains (data truncated)"
                );
                break;
            }

            let size = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            let flags = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

            // Extract value type from flags (bits 1-2)
            let kind = (flags & 6) >> 1;
            let value_type = match kind {
                0 => APEValueType::Text,
                1 => APEValueType::Binary,
                2 => APEValueType::External,
                _ => {
                    return Err(AudexError::APEBadItem(
                        "value type must be 0, 1, or 2".to_string(),
                    ));
                }
            };

            // Read key (null-terminated, spec limit: 2-255 bytes)
            let mut key_bytes = Vec::new();
            loop {
                let mut byte = [0u8; 1];
                if Read::read_exact(&mut cursor, &mut byte).is_err() {
                    return Err(AudexError::APEBadItem("incomplete key".to_string()));
                }
                if byte[0] == 0 {
                    break;
                }
                key_bytes.push(byte[0]);
                if key_bytes.len() > 255 {
                    return Err(AudexError::APEBadItem(
                        "key exceeds 255-byte spec limit".to_string(),
                    ));
                }
            }

            let key = String::from_utf8(key_bytes)
                .map_err(|e| AudexError::APEBadItem(format!("invalid key encoding: {}", e)))?;

            if !is_valid_apev2_key(&key) {
                return Err(AudexError::APEBadItem(format!(
                    "{:?} is not a valid APEv2 key",
                    key
                )));
            }

            // Binary items are typically cover art — check against the
            // image ceiling.  Text/external items use the tag ceiling.
            if value_type == APEValueType::Binary {
                limits.check_image_size(size as u64, "APEv2 binary item")?;
            } else {
                limits.check_tag_size(size as u64, "APEv2 item")?;
            }

            // Reject if the running total would exceed the tag ceiling
            cumulative_size = cumulative_size.saturating_add(size as u64);
            limits.check_tag_size(cumulative_size, "APEv2 cumulative items")?;

            // Verify the cursor has enough remaining bytes before allocating,
            // preventing huge allocations from untrusted size fields
            let remaining = tag_data.len() as u64 - cursor.position();
            if (size as u64) > remaining {
                return Err(AudexError::APEBadItem(format!(
                    "item claims {} bytes but only {} remain",
                    size, remaining
                )));
            }
            let mut value_data = vec![0u8; size as usize];
            if Read::read_exact(&mut cursor, &mut value_data).is_err() {
                return Err(AudexError::APEBadItem("incomplete value data".to_string()));
            }

            let value = APEValue {
                value_type,
                data: value_data,
            };

            trace_event!(key = %key, size = size, "APEv2 item parsed");
            self.tags.set(&key, value)?;
        }

        Ok(())
    }

    /// Write APE tag to writer
    fn write_tag<W: Write>(&self, writer: &mut W) -> Result<u32> {
        let mut tag_items = Vec::new();

        // Collect all tag items
        for (key, value) in self.tags.items() {
            let mut item_data = Vec::new();

            // Value length (4 bytes, APEv2 format stores as u32)
            let value_len = u32::try_from(value.data.len()).map_err(|_| {
                AudexError::InvalidData("APEv2 item value too large (exceeds u32 max)".into())
            })?;
            item_data.extend_from_slice(&value_len.to_le_bytes());

            // Flags (4 bytes) - value type in bits 1-2
            let flags = (value.value_type as u32) << 1;
            item_data.extend_from_slice(&flags.to_le_bytes());

            // Key + null terminator
            item_data.extend_from_slice(key.as_bytes());
            item_data.push(0);

            // Value data
            item_data.extend_from_slice(&value.data);

            tag_items.push(item_data);
        }

        // Sort by size (recommended by spec)
        tag_items.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));

        let num_items = u32::try_from(tag_items.len()).map_err(|_| {
            AudexError::InvalidData("APEv2 item count exceeds u32 capacity".to_string())
        })?;
        let tag_data: Vec<u8> = tag_items.into_iter().flatten().collect();
        let tag_size = u32::try_from(tag_data.len()).map_err(|_| {
            AudexError::InvalidData("APEv2 tag data exceeds u32 capacity (4 GB limit)".to_string())
        })?;

        // Validate that header/footer sizes don't overflow the u32 field
        let size_with_footer = tag_size.checked_add(32).ok_or_else(|| {
            AudexError::InvalidData(
                "APEv2 tag size too large: adding footer exceeds u32 capacity".to_string(),
            )
        })?;
        let total_size = tag_size.checked_add(64).ok_or_else(|| {
            AudexError::InvalidData(
                "APEv2 tag size too large: adding header and footer exceeds u32 capacity"
                    .to_string(),
            )
        })?;

        // Write header
        writer.write_all(b"APETAGEX")?;
        writer.write_all(&2000u32.to_le_bytes())?; // version
        writer.write_all(&size_with_footer.to_le_bytes())?; // size including footer
        writer.write_all(&num_items.to_le_bytes())?; // item count
        writer.write_all(&(HAS_HEADER | IS_HEADER).to_le_bytes())?; // flags
        writer.write_all(&[0u8; 8])?; // reserved

        // Write tag data
        writer.write_all(&tag_data)?;

        // Write footer
        writer.write_all(b"APETAGEX")?;
        writer.write_all(&2000u32.to_le_bytes())?; // version
        writer.write_all(&size_with_footer.to_le_bytes())?; // size including footer
        writer.write_all(&num_items.to_le_bytes())?; // item count
        writer.write_all(&HAS_HEADER.to_le_bytes())?; // flags
        writer.write_all(&[0u8; 8])?; // reserved

        Ok(total_size) // header + tag data + footer
    }

    /// Load APEv2 tags asynchronously
    ///
    /// Searches for APE tags at the end or beginning of the file
    /// and parses all tag items.
    ///
    /// # Arguments
    /// * `path` - Path to the file containing APE tags
    ///
    /// # Example
    /// ```rust,no_run
    /// use audex::apev2::APEv2;
    ///
    /// # async fn example() -> Result<(), audex::AudexError> {
    /// let ape = APEv2::load_async("audio.ape").await?;
    /// if let Some(title) = ape.tags.get("Title") {
    ///     println!("Title: {}", title.as_string()?);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut ape = APEv2::new();
        ape.filename = Some(path.as_ref().to_string_lossy().to_string());
        let limits = crate::limits::ParseLimits::default();

        // Find and parse APE metadata
        let mut ape_data = APEDataAsync::new();
        ape_data.find_metadata(&mut file).await?;
        ape_data.fill_missing(&mut file).await?;
        ape_data.fix_brokenness(&mut file).await?;
        ape_data.read_tag_data(&mut file, limits).await?;

        if let Some(tag_data) = &ape_data.tag_data {
            ape.parse_tag(tag_data, ape_data.items)?;
        } else {
            return Err(AudexError::APENoHeader);
        }

        Ok(ape)
    }

    /// Save APEv2 tags asynchronously
    ///
    /// Writes the current tags to the file, replacing any existing APE tags.
    ///
    /// # Example
    /// ```rust,no_run
    /// use audex::apev2::APEv2;
    ///
    /// # async fn example() -> Result<(), audex::AudexError> {
    /// let mut ape = APEv2::load_async("audio.ape").await?;
    /// ape.tags.set_text("Title", "New Title".to_string())?;
    /// ape.save_async().await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        let filename = self
            .filename
            .as_ref()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?
            .clone();

        let mut file = loadfile_write_async(&filename).await?;

        // Find existing APE data to remove
        let mut ape_data = APEDataAsync::new();
        ape_data.find_metadata(&mut file).await?;

        // Remove old tag if present
        if ape_data.footer.is_some() || ape_data.header.is_some() {
            ape_data.fill_missing(&mut file).await?;
            ape_data.fix_brokenness(&mut file).await?;

            if let (Some(start), Some(end)) = (ape_data.start, ape_data.end) {
                let old_tag_size = end - start;
                resize_bytes_async(&mut file, old_tag_size, 0, start).await?;
            }
        }

        // Append new tag at end
        file.seek(SeekFrom::End(0)).await?;
        self.write_tag_async(&mut file).await?;

        Ok(())
    }

    /// Write APE tag to file asynchronously
    #[cfg(feature = "async")]
    async fn write_tag_async(&self, file: &mut TokioFile) -> Result<u32> {
        let mut tag_items = Vec::new();

        // Collect all tag items
        for (key, value) in self.tags.items() {
            let mut item_data = Vec::new();

            // Value length (4 bytes, APEv2 format stores as u32)
            let value_len = u32::try_from(value.data.len()).map_err(|_| {
                AudexError::InvalidData("APEv2 item value too large (exceeds u32 max)".into())
            })?;
            item_data.extend_from_slice(&value_len.to_le_bytes());

            // Flags (4 bytes) - value type in bits 1-2
            let flags = (value.value_type as u32) << 1;
            item_data.extend_from_slice(&flags.to_le_bytes());

            // Key + null terminator
            item_data.extend_from_slice(key.as_bytes());
            item_data.push(0);

            // Value data
            item_data.extend_from_slice(&value.data);

            tag_items.push(item_data);
        }

        // Sort by size (recommended by APE specification)
        tag_items.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));

        let num_items = u32::try_from(tag_items.len()).map_err(|_| {
            AudexError::InvalidData("APEv2 item count exceeds u32 capacity".to_string())
        })?;
        let tag_data: Vec<u8> = tag_items.into_iter().flatten().collect();
        let tag_size = u32::try_from(tag_data.len()).map_err(|_| {
            AudexError::InvalidData("APEv2 tag data exceeds u32 capacity (4 GB limit)".to_string())
        })?;

        // Validate that header/footer sizes don't overflow the u32 field
        let size_with_footer = tag_size.checked_add(32).ok_or_else(|| {
            AudexError::InvalidData(
                "APEv2 tag size too large: adding footer exceeds u32 capacity".to_string(),
            )
        })?;
        let total_size = tag_size.checked_add(64).ok_or_else(|| {
            AudexError::InvalidData(
                "APEv2 tag size too large: adding header and footer exceeds u32 capacity"
                    .to_string(),
            )
        })?;

        // Write header
        file.write_all(b"APETAGEX").await?;
        file.write_all(&2000u32.to_le_bytes()).await?; // version
        file.write_all(&size_with_footer.to_le_bytes()).await?; // size including footer
        file.write_all(&num_items.to_le_bytes()).await?; // item count
        file.write_all(&(HAS_HEADER | IS_HEADER).to_le_bytes())
            .await?; // flags
        file.write_all(&[0u8; 8]).await?; // reserved

        // Write tag data
        file.write_all(&tag_data).await?;

        // Write footer
        file.write_all(b"APETAGEX").await?;
        file.write_all(&2000u32.to_le_bytes()).await?; // version
        file.write_all(&size_with_footer.to_le_bytes()).await?; // size including footer
        file.write_all(&num_items.to_le_bytes()).await?; // item count
        file.write_all(&HAS_HEADER.to_le_bytes()).await?; // flags
        file.write_all(&[0u8; 8]).await?; // reserved

        file.flush().await?;

        Ok(total_size) // header + tag data + footer
    }

    /// Clear APEv2 tags asynchronously
    ///
    /// Removes all APE tags from the file.
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        let filename = self
            .filename
            .as_ref()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?
            .clone();

        let mut file = loadfile_write_async(&filename).await?;

        // Find and remove APE tags
        let mut ape_data = APEDataAsync::new();
        ape_data.find_metadata(&mut file).await?;
        ape_data.fill_missing(&mut file).await?;
        ape_data.fix_brokenness(&mut file).await?;

        if let (Some(start), Some(end)) = (ape_data.start, ape_data.end) {
            let old_tag_size = end - start;
            resize_bytes_async(&mut file, old_tag_size, 0, start).await?;
        }

        self.tags.clear();
        Ok(())
    }

    /// Delete APEv2 tags from file asynchronously
    ///
    /// Removes all APE tags without loading them first.
    #[cfg(feature = "async")]
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        let mut file = loadfile_write_async(&path).await?;

        let mut ape_data = APEDataAsync::new();
        ape_data.find_metadata(&mut file).await?;
        ape_data.fill_missing(&mut file).await?;
        ape_data.fix_brokenness(&mut file).await?;

        if let (Some(start), Some(end)) = (ape_data.start, ape_data.end) {
            let old_tag_size = end - start;
            resize_bytes_async(&mut file, old_tag_size, 0, start).await?;
        }

        Ok(())
    }
}

impl Default for APEv2 {
    fn default() -> Self {
        Self::new()
    }
}

impl APEv2 {
    /// Write `count` zero bytes to the writer in fixed-size chunks.
    /// Avoids allocating a single buffer for the entire region, which could
    /// exhaust memory when zeroing large tag gaps on big files.
    fn write_zeros(writer: &mut dyn ReadWriteSeek, count: u64) -> Result<()> {
        const CHUNK_SIZE: usize = 64 * 1024; // 64 KB per chunk
        let buf = [0u8; CHUNK_SIZE];
        let mut remaining = count;
        while remaining > 0 {
            let n = (remaining as usize).min(CHUNK_SIZE);
            writer.write_all(&buf[..n])?;
            remaining -= n as u64;
        }
        Ok(())
    }

    /// Core save logic that works with any reader/writer/seeker.
    ///
    /// Finds existing APE tag data in the stream, removes it, and appends
    /// the current tags as a new APE tag block at the end.
    fn save_to_writer_inner(&mut self, mut writer: &mut dyn ReadWriteSeek) -> Result<()> {
        // Find existing APE data to remove if present
        let mut ape_data = APEData::new();
        ape_data.find_metadata(&mut writer)?;

        // Only remove old tag if we successfully found it
        if ape_data.footer.is_some() || ape_data.header.is_some() {
            ape_data.fill_missing(&mut writer)?;
            ape_data.fix_brokenness(&mut writer)?;

            if let (Some(start), Some(end)) = (ape_data.start, ape_data.end) {
                let file_size = writer.seek(SeekFrom::End(0))?;
                if end >= file_size {
                    // Tag is at the end of the stream - seek to tag start and
                    // overwrite from there. Any leftover bytes from the old tag
                    // will be zeroed out after the new tag is written.
                    writer.seek(SeekFrom::Start(start))?;
                    self.write_tag(&mut writer)?;

                    // Zero out any remaining stale bytes from the old tag so
                    // that leftover APETAGEX signatures do not confuse readers.
                    let new_end = writer.stream_position()?;
                    if new_end < file_size {
                        Self::write_zeros(writer, file_size - new_end)?;
                        // Seek back to the logical end so callers can determine
                        // the valid file size and truncate if needed.
                        writer.seek(SeekFrom::Start(new_end))?;
                    }
                    return Ok(());
                } else {
                    // Tag is at the start or middle - shift trailing data to
                    // fill the gap left by the removed tag.
                    let old_tag_size = end - start;
                    let trailing = file_size - end;
                    crate::util::move_bytes(&mut writer, start, end, trailing, None)?;

                    // Zero out the now-unused region at the tail so stale data
                    // (including old APETAGEX magic bytes) does not persist.
                    let logical_end = file_size - old_tag_size;
                    writer.seek(SeekFrom::Start(logical_end))?;
                    Self::write_zeros(writer, old_tag_size)?;

                    // Seek back to the logical end for appending the new tag
                    writer.seek(SeekFrom::Start(logical_end))?;
                    self.write_tag(&mut writer)?;
                    return Ok(());
                }
            }
        }

        // No old tag found (or boundaries not resolved) - append at end
        writer.seek(SeekFrom::End(0))?;
        self.write_tag(&mut writer)?;

        Ok(())
    }

    /// Core clear logic that works with any reader/writer/seeker.
    ///
    /// Finds existing APE tag data in the stream and removes it, then
    /// clears the in-memory tag collection.
    fn clear_writer_inner(&mut self, mut writer: &mut dyn ReadWriteSeek) -> Result<()> {
        let mut ape_data = APEData::new();
        ape_data.find_metadata(&mut writer)?;
        ape_data.fill_missing(&mut writer)?;
        ape_data.fix_brokenness(&mut writer)?;

        if let (Some(start), Some(end)) = (ape_data.start, ape_data.end) {
            let file_size = writer.seek(SeekFrom::End(0))?;
            if end >= file_size {
                // Tag is at the end — zero it out, then seek back to the
                // logical end so callers can truncate the stream.
                writer.seek(SeekFrom::Start(start))?;
                Self::write_zeros(writer, file_size - start)?;
                writer.seek(SeekFrom::Start(start))?;
            } else {
                // Tag is in the middle — shift trailing data forward
                let old_tag_size = end - start;
                let trailing = file_size - end;
                crate::util::move_bytes(&mut writer, start, end, trailing, None)?;

                // Zero out the stale tail, then seek back to the logical end
                let logical_end = file_size - old_tag_size;
                writer.seek(SeekFrom::Start(logical_end))?;
                Self::write_zeros(writer, old_tag_size)?;
                writer.seek(SeekFrom::Start(logical_end))?;
            }
        }

        self.tags.clear();
        Ok(())
    }
}

impl FileType for APEv2 {
    type Tags = APEv2Tags;
    type Info = APEStreamInfo;

    fn format_id() -> &'static str {
        "APEv2"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing APEv2 tags");
        let mut file = std::fs::File::open(&path)?;
        let mut ape = APEv2::new();
        ape.filename = Some(path.as_ref().to_string_lossy().to_string());
        let limits = crate::limits::ParseLimits::default();

        let mut ape_data = APEData::new();
        ape_data.find_metadata(&mut file)?;
        ape_data.fill_missing(&mut file)?;
        ape_data.fix_brokenness(&mut file)?;
        ape_data.read_tag_data(&mut file, limits)?;
        trace_event!(item_count = ape_data.items, "APEv2 header info");

        if let Some(tag_data) = &ape_data.tag_data {
            ape.parse_tag(tag_data, ape_data.items)?;
        } else {
            return Err(AudexError::APENoHeader);
        }

        Ok(ape)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing APEv2 tags from reader");
        let mut ape = APEv2::new();
        let limits = crate::limits::ParseLimits::default();

        let mut reader = reader;
        let mut ape_data = APEData::new();
        ape_data.find_metadata(&mut reader)?;
        ape_data.fill_missing(&mut reader)?;
        ape_data.fix_brokenness(&mut reader)?;
        ape_data.read_tag_data(&mut reader, limits)?;
        trace_event!(item_count = ape_data.items, "APEv2 header info");

        if let Some(tag_data) = &ape_data.tag_data {
            ape.parse_tag(tag_data, ape_data.items)?;
        } else {
            return Err(AudexError::APENoHeader);
        }

        Ok(ape)
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving APEv2 tags");
        let filename = self
            .filename
            .as_ref()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(filename)?;

        let mut ape_data = APEData::new();
        ape_data.find_metadata(&mut file)?;

        if ape_data.footer.is_some() || ape_data.header.is_some() {
            ape_data.fill_missing(&mut file)?;
            ape_data.fix_brokenness(&mut file)?;

            if let (Some(start), Some(end)) = (ape_data.start, ape_data.end) {
                let file_size = file.seek(SeekFrom::End(0))?;
                if end >= file_size {
                    file.set_len(start)?;
                } else {
                    let old_tag_size = end - start;
                    crate::util::resize_bytes(&mut file, old_tag_size, 0, start)?;
                }
            }
        }

        file.seek(SeekFrom::End(0))?;
        trace_event!(item_count = self.items().len(), "writing APEv2 tag items");
        self.write_tag(&mut file)?;

        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        let filename = self
            .filename
            .as_ref()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(filename)?;

        let mut ape_data = APEData::new();
        ape_data.find_metadata(&mut file)?;
        ape_data.fill_missing(&mut file)?;
        ape_data.fix_brokenness(&mut file)?;

        if let (Some(start), Some(end)) = (ape_data.start, ape_data.end) {
            let file_size = file.seek(SeekFrom::End(0))?;
            if end >= file_size {
                // Tag is at the end - truncate
                file.set_len(start)?;
            } else {
                // Tag is in the middle - use resize_bytes to remove
                let old_tag_size = end - start;
                crate::util::resize_bytes(&mut file, old_tag_size, 0, start)?;
            }
        }

        self.tags.clear();
        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        self.save_to_writer_inner(writer)
    }

    fn clear_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        self.clear_writer_inner(writer)
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;

        let mut ape_data = APEData::new();
        ape_data.find_metadata(&mut file)?;

        if ape_data.footer.is_some() || ape_data.header.is_some() {
            ape_data.fill_missing(&mut file)?;
            ape_data.fix_brokenness(&mut file)?;

            if let (Some(start), Some(end)) = (ape_data.start, ape_data.end) {
                let file_size = file.seek(SeekFrom::End(0))?;
                if end >= file_size {
                    file.set_len(start)?;
                } else {
                    let old_tag_size = end - start;
                    crate::util::resize_bytes(&mut file, old_tag_size, 0, start)?;
                }
            }
        }

        file.seek(SeekFrom::End(0))?;
        self.write_tag(&mut file)?;

        Ok(())
    }

    /// APEv2 tags are always present in this format.
    ///
    /// This method returns an error since tags cannot be added to a format
    /// that inherently always contains tag metadata.
    ///
    /// # Errors
    ///
    /// Always returns `AudexError::InvalidOperation`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::apev2::APEv2;
    /// use audex::FileType;
    ///
    /// let mut ape = APEv2::load("file.ape")?;
    /// // Tags are always present, so add_tags() will fail
    /// assert!(ape.add_tags().is_err());
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        // APEv2 tags are always present, cannot add what already exists
        Err(AudexError::InvalidOperation(
            "Tags already exist".to_string(),
        ))
    }

    fn tags(&self) -> Option<&Self::Tags> {
        Some(&self.tags)
    }

    fn tags_mut(&mut self) -> Option<&mut Self::Tags> {
        Some(&mut self.tags)
    }

    fn info(&self) -> &Self::Info {
        &self.info
    }

    fn score(_filename: &str, header: &[u8]) -> i32 {
        let mut score = 0;

        // Check for APETAGEX signature
        if header.len() >= 8 && &header[0..8] == b"APETAGEX" {
            score += 10;
        }

        // Note: .ape extension is NOT scored here because .ape files are
        // MonkeysAudio containers that happen to contain APEv2 tags.
        // APEv2 is a tag format, not a file container format.

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["application/x-ape", "audio/x-ape"]
    }
}

/// Validate APEv2 key according to specification
pub fn is_valid_apev2_key(key: &str) -> bool {
    // Key must be 2-255 characters
    if key.len() < 2 || key.len() > 255 {
        return false;
    }

    // All characters must be in ASCII range 0x20-0x7E (space to tilde)
    for ch in key.chars() {
        if (ch as u32) < 0x20 || (ch as u32) > 0x7E {
            return false;
        }
    }

    // Reserved keys are forbidden (case-insensitive per APEv2 spec)
    let forbidden = ["OggS", "TAG", "ID3", "MP+"];
    !forbidden.iter().any(|f| f.eq_ignore_ascii_case(key))
}

/// Module functions for standalone operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    match APEv2::load(&path) {
        Ok(mut ape) => ape.clear(),
        Err(AudexError::APENoHeader) => Ok(()), // No tag to clear
        Err(e) => Err(e),
    }
}

/// Clear APEv2 tags from file asynchronously
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<Path>>(path: P) -> Result<()> {
    match APEv2::load_async(&path).await {
        Ok(mut ape) => ape.clear_async().await,
        Err(AudexError::APENoHeader) => Ok(()), // No tag to clear
        Err(e) => Err(e),
    }
}

/// Open APEv2 file asynchronously
#[cfg(feature = "async")]
pub async fn open_async<P: AsRef<Path>>(path: P) -> Result<APEv2> {
    APEv2::load_async(path).await
}
