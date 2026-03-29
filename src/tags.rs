//! Core tagging interfaces and metadata handling
//!
//! This module defines the fundamental traits and types for working with audio metadata
//! tags across different file formats. It provides a unified interface that abstracts
//! over format-specific tagging systems.
//!
//! # Tagging System Overview
//!
//! The library uses a layered trait system for tag operations:
//!
//! - **[`Tags`]**: Base trait for key-value tag access (get, set, remove)
//! - **[`Metadata`]**: Extends Tags with file I/O operations (load, save, delete)
//! - **[`MetadataFields`]**: Convenience accessors for common tag fields
//!
//! ## Tags vs Metadata vs MetadataFields
//!
//! ### Tags Trait
//! The fundamental interface for tag manipulation. Provides:
//! - Key-value operations: `get()`, `set()`, `remove()`, `keys()`
//! - Multi-value tag support (most formats support multiple values per key)
//! - Format-agnostic tag access
//!
//! Use this when you need to work with tags on an already-loaded file.
//!
//! ### Metadata Trait
//! Extends Tags with file persistence operations:
//! - `load_from_path()` / `load_from_fileobj()`: Read tags from files
//! - `save_to_path()` / `save_to_fileobj()`: Write tags back to files
//! - `delete_from_path()` / `delete_from_fileobj()`: Remove tags entirely
//!
//! Use this for standalone tag formats like ID3v2, APEv2, or Vorbis Comments.
//!
//! ### MetadataFields Trait
//! Convenience accessors for common fields:
//! - `artist()`, `album()`, `title()`, `track_number()`, `date()`, `genre()`
//! - Automatic field name mapping (handles format-specific variations)
//! - Simpler API for common operations
//!
//! Use this when you just need standard music tags.
//!
//! # Common Tag Operations
//!
//! ## Reading Tags
//!
//! ```no_run
//! use audex::File;
//! use audex::FileType;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let file = File::load("music.mp3")?;
//!
//! // Get a single tag value
//! if let Some(artists) = file.get("artist") {
//!     for artist in artists {
//!         println!("Artist: {}", artist);
//!     }
//! }
//!
//! // Get the first value only
//! if let Some(title) = file.get_first("title") {
//!     println!("Title: {}", title);
//! }
//!
//! // List all tags
//! for key in file.keys() {
//!     println!("Tag: {}", key);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Writing Tags
//!
//! ```no_run
//! use audex::File;
//! use audex::FileType;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let mut file = File::load("music.mp3")?;
//!
//! // Set single value
//! file.set("artist", vec!["Artist Name".to_string()])?;
//!
//! // Set multiple values (for formats that support it)
//! file.set("artist", vec![
//!     "First Artist".to_string(),
//!     "Second Artist".to_string(),
//! ])?;
//!
//! // Remove a tag
//! file.remove("comment")?;
//!
//! // Save changes
//! file.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Multi-Value Tags
//!
//! Many formats support multiple values for the same tag key:
//!
//! ```no_run
//! use audex::File;
//! use audex::FileType;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let mut file = File::load("music.flac")?;
//!
//! // FLAC/Vorbis supports multiple artists
//! file.set("artist", vec![
//!     "Primary Artist".to_string(),
//!     "Featured Artist".to_string(),
//! ])?;
//!
//! // Get all values
//! if let Some(artists) = file.get("artist") {
//!     println!("Artists: {}", artists.len());
//!     for artist in artists {
//!         println!("  - {}", artist);
//!     }
//! }
//!
//! file.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Format-Specific Considerations
//!
//! Different formats have different tagging capabilities:
//!
//! ## ID3v2 (MP3)
//! - Frame-based structure
//! - Most frames support single values
//! - Some frames (like TXXX) support multiple instances
//! - Binary data support (e.g., album art)
//!
//! ## Vorbis Comments (FLAC, Ogg Vorbis, Ogg Opus)
//! - Simple key=value pairs
//! - Native multi-value support
//! - Case-insensitive keys (by convention)
//! - Text-only (binary data uses base64 encoding)
//!
//! ## iTunes-style Tags (MP4/M4A)
//! - Atom-based structure
//! - Predefined atoms for common tags
//! - Custom atoms via "----" freeform boxes
//!
//! ## APEv2 (APE, WavPack, Musepack)
//! - Key-value items
//! - Supports both text and binary values
//! - Case-sensitive keys
//!
//! # Padding Information
//!
//! Some formats support padding to minimize file rewrites. See [`PaddingInfo`]
//! for details on how padding is calculated and managed during save operations.

use crate::util::{AnyFileThing, loadfile_read, loadfile_write};
use crate::{AudexError, Result};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::collections::HashMap;
use std::fmt;
use std::io::{Cursor, ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Padding information for optimizing tag write operations
///
/// Many audio formats support padding - extra empty space reserved after metadata
/// tags. Padding allows tag updates without rewriting the entire file, which is
/// much faster for large audio files.
///
/// This struct calculates optimal padding amounts based on:
/// - Current available padding
/// - Size of trailing data (audio stream)
/// - Heuristics for minimizing future rewrites
///
/// # How Padding Works
///
/// When tags are written:
/// 1. If new tags fit in existing padding → fast update (no file rewrite)
/// 2. If new tags are larger → entire file must be shifted (slow)
/// 3. If new tags are much smaller → excessive wasted space
///
/// The default padding algorithm balances these concerns.
///
/// # Padding Calculation
///
/// The `get_default_padding()` method implements a smart algorithm:
/// - **High threshold**: 10 KiB + 1% of trailing data
/// - **Low threshold**: 1 KiB + 0.1% of trailing data
///
/// If current padding exceeds the high threshold, it's reduced to the low threshold.
/// If current padding is insufficient (negative), padding is added to reach the low threshold.
///
/// # Examples
///
/// ## Using Default Padding
///
/// ```no_run
/// use audex::tags::PaddingInfo;
///
/// // Simulate a file with 100 bytes of current padding
/// // and 10 MB of trailing audio data
/// let info = PaddingInfo {
///     padding: 100,
///     size: 10_000_000,
/// };
///
/// // Get recommended padding amount
/// let recommended = info.get_default_padding();
/// println!("Recommended padding: {} bytes", recommended);
/// // Output: 100 bytes (padding is below the high threshold, so it is kept as-is)
/// ```
///
/// ## Custom Padding Strategy
///
/// ```no_run
/// use audex::tags::PaddingInfo;
///
/// let info = PaddingInfo {
///     padding: -5000,  // Need 5000 more bytes
///     size: 50_000_000,
/// };
///
/// // Use a custom strategy (e.g., always 64 KiB)
/// let custom_padding = 65536;
///
/// // Or use the default algorithm
/// let default_padding = info.get_default_padding();
///
/// println!("Custom: {} bytes, Default: {} bytes",
///          custom_padding, default_padding);
/// ```
///
/// # Format Support
///
/// Padding is commonly used in:
/// - **ID3v2**: Padding bytes after tag frames
/// - **FLAC**: Padding metadata blocks
/// - **Ogg**: Not typically used (packets are page-based)
/// - **MP4**: Usually not used (atoms are size-prefixed)
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PaddingInfo {
    /// Current padding available (in bytes)
    ///
    /// - **Positive**: Bytes of available padding space
    /// - **Zero**: No padding (tags fit exactly)
    /// - **Negative**: Additional bytes needed (current padding insufficient)
    pub padding: i64,

    /// Size of data following the tag region (in bytes)
    ///
    /// Typically the size of the audio stream. Used to calculate
    /// percentage-based padding amounts.
    pub size: i64,
}

impl PaddingInfo {
    /// Create a new PaddingInfo with the given padding and trailing data size
    pub fn new(padding: i64, size: i64) -> Self {
        Self { padding, size }
    }

    /// The default implementation which tries to select a reasonable
    /// amount of padding and which might change in future versions.
    ///
    /// Returns the amount of padding after saving
    pub fn get_default_padding(&self) -> i64 {
        // Clamp size to non-negative — a negative trailing data size is
        // meaningless and would produce nonsensical padding calculations.
        let clamped_size = self.size.max(0);
        let high = (1024i64 * 10).saturating_add(clamped_size / 100); // 10 KiB + 1% of trailing data
        let low = 1024i64.saturating_add(clamped_size / 1000); // 1 KiB + 0.1% of trailing data

        if self.padding >= 0 {
            // enough padding left
            if self.padding > high {
                // padding too large, reduce
                low
            } else {
                // just use existing padding as is
                self.padding
            }
        } else {
            // not enough padding, add some
            low
        }
    }

    /// Get padding using a user-provided function, falling back to default calculation.
    ///
    /// This method is an internal implementation detail used by format implementations.
    /// It may change without notice.
    pub(crate) fn get_padding_with<F>(&self, user_func: Option<F>) -> i64
    where
        F: FnOnce(&PaddingInfo) -> i64,
    {
        match user_func {
            Some(func) => func(self),
            None => self.get_default_padding(),
        }
    }
}

impl fmt::Display for PaddingInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "<PaddingInfo size={} padding={}>",
            self.size, self.padding
        )
    }
}

/// Base trait for key-value tag access
///
/// This trait defines the fundamental interface for reading and writing metadata tags.
/// It provides a key-value API where each key can map to multiple string values,
/// allowing formats to naturally express multi-value tags (e.g., multiple artists).
///
/// # Design Philosophy
///
/// - **Keys are strings**: Tag names are format-specific strings (e.g., "ARTIST", "TPE1", "©ART")
/// - **Values are string lists**: Each key maps to a vector of strings, supporting multi-value tags
/// - **Case sensitivity**: Depends on the format (ID3 is case-sensitive, Vorbis is not)
/// - **Empty values**: Setting an empty vector typically removes the key
///
/// # Examples
///
/// ## Basic Tag Operations
///
/// Note: These examples use [`crate::File`] which returns a
/// `DynamicFileType`. The `DynamicFileType` wrapper provides
/// convenience methods that mirror this trait but return `Result<()>` instead
/// of `()`. The raw `Tags` trait methods (`set`, `remove`, `set_single`) do
/// not return `Result`.
///
/// ```no_run
/// use audex::File;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// let mut file = File::load("song.mp3")?;
///
/// // Set a single-value tag (DynamicFileType::set returns Result<()>)
/// file.set("artist", vec!["Artist Name".to_string()])?;
///
/// // Set a multi-value tag
/// file.set("genre", vec!["Rock".to_string(), "Alternative".to_string()])?;
///
/// // Get tag values (DynamicFileType::get returns Option<Vec<String>>)
/// if let Some(artists) = file.get("artist") {
///     println!("Found {} artist(s)", artists.len());
/// }
///
/// // Remove a tag
/// file.remove("comment")?;
///
/// // Check if tag exists
/// if file.contains_key("album") {
///     println!("Album tag is present");
/// }
///
/// file.save()?;
/// # Ok(())
/// # }
/// ```
///
/// ## Direct Tags Trait Usage
///
/// When working with a concrete `Tags` implementor directly:
///
/// ```rust
/// use audex::tags::{BasicTags, Tags};
///
/// let mut tags = BasicTags::new();
///
/// // Tags::set() returns () (no Result)
/// tags.set("artist", vec!["Artist Name".to_string()]);
/// tags.set("genre", vec!["Rock".to_string()]);
///
/// // Tags::get() returns Option<&[String]> (borrowed slice)
/// if let Some(artists) = tags.get("artist") {
///     println!("Found {} artist(s)", artists.len());
/// }
///
/// // Tags::remove() returns () (no Result)
/// tags.remove("genre");
/// ```
pub trait Tags {
    /// Get all values for a tag key
    ///
    /// Returns a borrowed slice of strings if the key exists, or `None` if not found.
    ///
    /// Note: `DynamicFileType::get` returns `Option<Vec<String>>` (owned)
    /// instead of `Option<&[String]>` (borrowed) due to dynamic dispatch.
    ///
    /// # Arguments
    /// * `key` - The tag key to look up (format-specific, e.g., "ARTIST", "artist")
    ///
    /// # Returns
    /// * `Some(&[String])` - Slice of all values for this key
    /// * `None` - Key does not exist
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let mut tags = BasicTags::new();
    /// tags.set("artist", vec!["Artist Name".to_string()]);
    ///
    /// if let Some(values) = tags.get("artist") {
    ///     for value in values {
    ///         println!("Artist: {}", value);
    ///     }
    /// }
    /// ```
    fn get(&self, key: &str) -> Option<&[String]>;

    /// Set all values for a tag key, replacing any existing values
    ///
    /// If values is empty, this typically removes the key entirely.
    /// Some formats may have restrictions on multi-value tags.
    ///
    /// Note: This method returns `()`. The `DynamicFileType::set`
    /// wrapper returns `Result<()>` instead, adding error handling for the
    /// dynamic dispatch layer.
    ///
    /// # Arguments
    /// * `key` - The tag key to set
    /// * `values` - Vector of string values (empty to remove)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let mut tags = BasicTags::new();
    ///
    /// // Set single value
    /// tags.set("artist", vec!["New Artist".to_string()]);
    ///
    /// // Set multiple values
    /// tags.set("genre", vec!["Rock".to_string(), "Alternative".to_string()]);
    ///
    /// // Remove by setting empty
    /// tags.set("comment", vec![]);
    /// ```
    fn set(&mut self, key: &str, values: Vec<String>);

    /// Remove a tag key and all its values
    ///
    /// After this operation, the key will no longer exist in the tag collection.
    ///
    /// Note: This method returns `()`. The `DynamicFileType::remove`
    /// wrapper returns `Result<()>` instead.
    ///
    /// # Arguments
    /// * `key` - The tag key to remove
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let mut tags = BasicTags::new();
    /// tags.set("comment", vec!["test".to_string()]);
    /// tags.set("encoded_by", vec!["encoder".to_string()]);
    ///
    /// // Remove unwanted tags
    /// tags.remove("comment");
    /// tags.remove("encoded_by");
    /// ```
    fn remove(&mut self, key: &str);

    /// Get a vector of all tag keys present
    ///
    /// The order of keys is typically undefined. Use this to iterate over
    /// all available tags or check what tags are present.
    ///
    /// # Returns
    /// Vector of all tag key names
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let mut tags = BasicTags::new();
    /// tags.set("artist", vec!["Test".to_string()]);
    /// tags.set("title", vec!["Song".to_string()]);
    ///
    /// println!("Available tags:");
    /// for key in tags.keys() {
    ///     println!("  - {}", key);
    /// }
    /// ```
    fn keys(&self) -> Vec<String>;

    /// Check if a tag key exists
    ///
    /// # Arguments
    /// * `key` - The tag key to check
    ///
    /// # Returns
    /// `true` if the key exists, `false` otherwise
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let mut tags = BasicTags::new();
    /// tags.set("artist", vec!["Test".to_string()]);
    ///
    /// if tags.contains_key("artist") {
    ///     println!("Artist tag is present");
    /// }
    /// ```
    fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Get the first value for a key
    ///
    /// This is a convenience method for when you only need one value
    /// and don't care about additional values.
    ///
    /// Note: This returns `Option<&String>` (borrowed). The
    /// `DynamicFileType::get_first` wrapper returns
    /// `Option<String>` (owned) instead.
    ///
    /// # Arguments
    /// * `key` - The tag key to look up
    ///
    /// # Returns
    /// * `Some(&String)` - Reference to the first value
    /// * `None` - Key does not exist or has no values
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let mut tags = BasicTags::new();
    /// tags.set("title", vec!["My Song".to_string()]);
    ///
    /// if let Some(title) = tags.get_first("title") {
    ///     println!("Title: {}", title);
    /// }
    /// ```
    fn get_first(&self, key: &str) -> Option<&String> {
        self.get(key)?.first()
    }

    /// Set a single value for a key
    ///
    /// Convenience method equivalent to `set(key, vec![value])`.
    /// Returns `()` like `set()`. The `DynamicFileType::set_single`
    /// wrapper returns `Result<()>` instead.
    ///
    /// # Arguments
    /// * `key` - The tag key to set
    /// * `value` - The single value to set
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let mut tags = BasicTags::new();
    ///
    /// // Simpler than set(key, vec![...])
    /// tags.set_single("title", "Song Title".to_string());
    /// ```
    fn set_single(&mut self, key: &str, value: String) {
        self.set(key, vec![value]);
    }

    /// Get all values from all tags as a list
    ///
    /// Returns a vector containing the value vectors for each key.
    /// The order corresponds to the order returned by `keys()`.
    ///
    /// # Returns
    /// Vector of value vectors
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let mut tags = BasicTags::new();
    /// tags.set("artist", vec!["Test".to_string()]);
    ///
    /// let all_values = tags.values();
    /// println!("Total tag groups: {}", all_values.len());
    /// ```
    fn values(&self) -> Vec<Vec<String>> {
        let keys = self.keys();
        keys.iter()
            .filter_map(|k| self.get(k).map(|v| v.to_vec()))
            .collect()
    }

    /// Get all key-value pairs as tuples
    ///
    /// Returns a vector of (key, values) tuples representing all tags.
    /// Useful for iterating over the entire tag collection.
    ///
    /// # Returns
    /// Vector of `(String, Vec<String>)` tuples
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let mut tags = BasicTags::new();
    /// tags.set("artist", vec!["Test".to_string()]);
    ///
    /// for (key, values) in tags.items() {
    ///     println!("{}: {:?}", key, values);
    /// }
    /// ```
    fn items(&self) -> Vec<(String, Vec<String>)> {
        let keys = self.keys();
        keys.iter()
            .filter_map(|k| self.get(k).map(|v| (k.clone(), v.to_vec())))
            .collect()
    }

    /// Returns tag information as a formatted string
    ///
    /// Creates a human-readable representation of all tags, typically
    /// in the format "key=value" with one tag per line.
    ///
    /// # Returns
    /// Formatted string representation of all tags
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::tags::{BasicTags, Tags};
    ///
    /// let tags = BasicTags::new();
    /// // Print tags in human-readable format
    /// println!("{}", tags.pprint());
    /// ```
    fn pprint(&self) -> String;

    /// Returns the module name for this tag implementation
    ///
    /// Used internally for debugging and introspection.
    ///
    /// # Returns
    /// Static string identifying the module
    fn module_name(&self) -> &'static str {
        "audex"
    }
}

/// Trait for standalone metadata formats with file I/O operations
///
/// This trait extends [`Tags`] with methods to load, save, and delete metadata
/// from files. It's used for standalone tag formats that can exist independently
/// of the audio stream, such as ID3v2, APEv2, or Vorbis Comments.
///
/// # Design
///
/// The trait provides both path-based and file object-based I/O:
/// - `load_from_path()` / `save_to_path()` / `delete_from_path()` - Work with file paths
/// - `load_from_fileobj()` / `save_to_fileobj()` / `delete_from_fileobj()` - Work with open file handles
///
/// This dual API supports both convenience (path-based) and advanced use cases
/// (working with already-open files, custom I/O, testing).
///
/// # Examples
///
/// ## Loading and Saving Standalone Tags
///
/// ```no_run
/// use audex::id3::ID3;
/// use audex::tags::Metadata;
/// use audex::Tags;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// // Load ID3v2 tags from file
/// let mut id3 = ID3::load_from_path("song.mp3")?;
///
/// // Modify tags
/// id3.set("TIT2", vec!["New Title".to_string()]);
/// id3.set("TPE1", vec!["New Artist".to_string()]);
///
/// // Save back to file
/// id3.save_to_path(Some("song.mp3"))?;
/// # Ok(())
/// # }
/// ```
///
/// ## Deleting All Tags
///
/// ```no_run
/// use audex::id3::ID3;
/// use audex::tags::Metadata;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// // Remove all ID3v2 tags from a file
/// ID3::delete_from_path(Some("song.mp3"))?;
/// # Ok(())
/// # }
/// ```
pub trait Metadata: Tags {
    /// Associated error type for this metadata format
    ///
    /// Each format can define its own error type, which will be
    /// converted to [`AudexError`] when necessary.
    type Error: Into<AudexError>;

    /// Create a new empty metadata instance
    ///
    /// Creates a new, empty tag structure with no tags set.
    /// Useful for creating tags for files that don't have any yet.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::id3::ID3;
    /// use audex::tags::Metadata;
    /// use audex::Tags;
    ///
    /// # fn main() -> Result<(), audex::AudexError> {
    /// // Create new empty ID3v2 tags
    /// let mut id3 = ID3::new();
    /// id3.set("TIT2", vec!["Title".to_string()]);
    /// # Ok(())
    /// # }
    /// ```
    fn new() -> Self
    where
        Self: Sized;

    /// Load metadata from a file path
    ///
    /// Opens the file, reads the metadata, and returns a new instance
    /// containing the loaded tags.
    ///
    /// # Arguments
    /// * `path` - Path to the audio file
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded metadata
    /// * `Err(AudexError)` - Failed to read or parse metadata
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::id3::ID3;
    /// use audex::tags::Metadata;
    ///
    /// # fn main() -> Result<(), audex::AudexError> {
    /// let id3 = ID3::load_from_path("song.mp3")?;
    /// # Ok(())
    /// # }
    /// ```
    fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self>
    where
        Self: Sized,
    {
        let mut file_thing = loadfile_read(path)?;
        Self::load_from_fileobj(&mut file_thing)
    }

    /// Load metadata from an open file handle
    ///
    /// Reads metadata from an already-opened file or any type that
    /// implements Read + Seek. Useful for custom I/O scenarios.
    ///
    /// # Arguments
    /// * `filething` - Mutable reference to an open file handle
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded metadata
    /// * `Err(AudexError)` - Failed to read or parse metadata
    fn load_from_fileobj(filething: &mut AnyFileThing) -> Result<Self>
    where
        Self: Sized;

    /// Save metadata to a file path
    ///
    /// Writes the current tag state back to the file. The file must
    /// already exist; this method modifies it in place.
    ///
    /// # Arguments
    /// * `path` - Optional path to the file (Some for path-based, None returns error)
    ///
    /// # Returns
    /// * `Ok(())` - Successfully saved
    /// * `Err(AudexError)` - Failed to write
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::id3::ID3;
    /// use audex::tags::Metadata;
    /// use audex::Tags;
    ///
    /// # fn main() -> Result<(), audex::AudexError> {
    /// let mut id3 = ID3::load_from_path("song.mp3")?;
    /// id3.set("TIT2", vec!["New Title".to_string()]);
    /// id3.save_to_path(Some("song.mp3"))?;
    /// # Ok(())
    /// # }
    /// ```
    fn save_to_path<P: AsRef<Path>>(&self, path: Option<P>) -> Result<()> {
        match path {
            Some(path) => {
                let mut file_thing = loadfile_write(path)?;
                self.save_to_fileobj(&mut file_thing)?;
                file_thing.write_back()
            }
            None => Err(AudexError::InvalidOperation(
                "No file path provided".to_string(),
            )),
        }
    }

    /// Save metadata to an open file handle
    ///
    /// Writes the tag data to an already-opened file or any type
    /// that implements Write + Seek.
    ///
    /// # Arguments
    /// * `filething` - Mutable reference to an open file handle
    ///
    /// # Returns
    /// * `Ok(())` - Successfully saved
    /// * `Err(AudexError)` - Failed to write
    fn save_to_fileobj(&self, filething: &mut AnyFileThing) -> Result<()>;

    /// Remove all metadata from a file
    ///
    /// Deletes all tag data from the file, removing any traces of the
    /// metadata structure. This operation is typically irreversible.
    ///
    /// # Arguments
    /// * `path` - Optional path to the file
    ///
    /// # Returns
    /// * `Ok(())` - Successfully deleted tags
    /// * `Err(AudexError)` - Failed to delete
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::id3::ID3;
    /// use audex::tags::Metadata;
    ///
    /// # fn main() -> Result<(), audex::AudexError> {
    /// // Remove all ID3v2 tags from file
    /// ID3::delete_from_path(Some("song.mp3"))?;
    /// # Ok(())
    /// # }
    /// ```
    fn delete_from_path<P: AsRef<Path>>(path: Option<P>) -> Result<()>
    where
        Self: Sized,
    {
        match path {
            Some(path) => {
                let mut file_thing = loadfile_write(path)?;
                Self::delete_from_fileobj(&mut file_thing)?;
                file_thing.write_back()
            }
            None => Err(AudexError::InvalidOperation(
                "No file path provided".to_string(),
            )),
        }
    }

    /// Remove all metadata from an open file handle
    ///
    /// Deletes tag data from an already-opened file.
    ///
    /// # Arguments
    /// * `filething` - Mutable reference to an open file handle
    ///
    /// # Returns
    /// * `Ok(())` - Successfully deleted tags
    /// * `Err(AudexError)` - Failed to delete
    fn delete_from_fileobj(filething: &mut AnyFileThing) -> Result<()>
    where
        Self: Sized;
}

/// Convenience interface for common metadata fields
///
/// This trait provides a simplified API for accessing the most commonly used
/// tag fields. It automatically handles format-specific field name variations
/// and provides a consistent interface across all audio formats.
///
/// # Field Mapping
///
/// Different formats use different names for the same conceptual field:
/// - **Artist**: "ARTIST" (Vorbis), "TPE1" (ID3v2), "©ART" (MP4)
/// - **Album**: "ALBUM" (Vorbis), "TALB" (ID3v2), "©alb" (MP4)
/// - **Title**: "TITLE" (Vorbis), "TIT2" (ID3v2), "©nam" (MP4)
/// - **Track Number**: "TRACKNUMBER" (Vorbis), "TRCK" (ID3v2), "trkn" (MP4)
/// - **Date**: "DATE" (Vorbis), "TDRC" (ID3v2), "©day" (MP4)
/// - **Genre**: "GENRE" (Vorbis), "TCON" (ID3v2), "©gen" (MP4)
///
/// This trait abstracts these differences, providing a uniform API.
///
/// # Trait Independence
///
/// `MetadataFields` is an independent trait -- it does **not** require [`Tags`]
/// as a supertrait. Types may implement `MetadataFields` alone, `Tags` alone,
/// or both. [`BasicTags`] implements both.
///
/// # Examples
///
/// ## Using MetadataFields Convenience Accessors
///
/// ```rust
/// use audex::tags::{BasicTags, MetadataFields, Tags};
///
/// let mut tags = BasicTags::new();
///
/// // Use field-specific methods for common tags
/// tags.set_artist("Artist Name".to_string());
/// tags.set_album("Album Title".to_string());
/// tags.set_title("Track Title".to_string());
/// tags.set_date("2024".to_string());
/// tags.set_track_number(1);
///
/// // Read back using convenience accessors
/// assert_eq!(tags.artist().map(|s| s.as_str()), Some("Artist Name"));
/// assert_eq!(tags.track_number(), Some(1));
/// ```
pub trait MetadataFields {
    /// Get the artist name
    ///
    /// Returns the primary artist/performer of the track.
    ///
    /// # Returns
    /// * `Some(&String)` - Artist name if present
    /// * `None` - Artist tag not set
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use audex::tags::{BasicTags, MetadataFields};
    /// # fn example() -> Option<String> {
    /// # let tags = BasicTags::new();
    /// if let Some(artist) = tags.artist() {
    ///     println!("Artist: {}", artist);
    /// }
    /// # None
    /// # }
    /// ```
    fn artist(&self) -> Option<&String>;

    /// Set the artist name
    ///
    /// Sets the primary artist/performer for the track.
    ///
    /// # Arguments
    /// * `artist` - Artist name to set
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use audex::tags::{BasicTags, MetadataFields};
    /// # fn example() {
    /// # let mut tags = BasicTags::new();
    /// tags.set_artist("Artist Name".to_string());
    /// # }
    /// ```
    fn set_artist(&mut self, artist: String);

    /// Get the album title
    ///
    /// Returns the album or collection title for the track.
    ///
    /// # Returns
    /// * `Some(&String)` - Album title if present
    /// * `None` - Album tag not set
    fn album(&self) -> Option<&String>;

    /// Set the album title
    ///
    /// Sets the album or collection title for the track.
    ///
    /// # Arguments
    /// * `album` - Album title to set
    fn set_album(&mut self, album: String);

    /// Get the track title
    ///
    /// Returns the title/name of the track itself.
    ///
    /// # Returns
    /// * `Some(&String)` - Track title if present
    /// * `None` - Title tag not set
    fn title(&self) -> Option<&String>;

    /// Set the track title
    ///
    /// Sets the title/name of the track.
    ///
    /// # Arguments
    /// * `title` - Track title to set
    fn set_title(&mut self, title: String);

    /// Get the track number
    ///
    /// Returns the track's position number in the album.
    /// This is typically parsed from a string field (e.g., "3" or "3/12").
    ///
    /// # Returns
    /// * `Some(u32)` - Track number if present and valid
    /// * `None` - Track number not set or not parseable as integer
    fn track_number(&self) -> Option<u32>;

    /// Set the track number
    ///
    /// Sets the track's position number in the album.
    ///
    /// # Arguments
    /// * `track` - Track number to set
    fn set_track_number(&mut self, track: u32);

    /// Get the release date
    ///
    /// Returns the release date, typically in YYYY, YYYY-MM, or YYYY-MM-DD format.
    ///
    /// # Returns
    /// * `Some(&String)` - Date string if present
    /// * `None` - Date tag not set
    fn date(&self) -> Option<&String>;

    /// Set the release date
    ///
    /// Sets the release date. Recommended formats: YYYY, YYYY-MM, or YYYY-MM-DD.
    ///
    /// # Arguments
    /// * `date` - Date string to set
    fn set_date(&mut self, date: String);

    /// Get the genre
    ///
    /// Returns the musical genre classification.
    ///
    /// # Returns
    /// * `Some(&String)` - Genre name if present
    /// * `None` - Genre tag not set
    fn genre(&self) -> Option<&String>;

    /// Set the genre
    ///
    /// Sets the musical genre classification.
    ///
    /// # Arguments
    /// * `genre` - Genre name to set
    fn set_genre(&mut self, genre: String);
}

const BASIC_TAGS_MAGIC: &[u8; 8] = b"ADXBTAGS";

fn write_len_prefixed_string<W: Write>(writer: &mut W, value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    if bytes.len() > u32::MAX as usize {
        return Err(AudexError::InvalidData(
            "String too large for tag serialization".to_string(),
        ));
    }
    writer.write_u32::<LittleEndian>(bytes.len() as u32)?;
    writer.write_all(bytes)?;
    Ok(())
}

/// Maximum byte length for a single string in BasicTags binary format (10 MB).
/// No realistic tag key or value should approach this size.
const MAX_STRING_LENGTH: u32 = 10 * 1024 * 1024;

/// Maximum number of entries (keys) in a BasicTags binary file.
const MAX_ENTRY_COUNT: u32 = 100_000;

/// Maximum number of values per entry in a BasicTags binary file.
const MAX_VALUE_COUNT: u32 = 100_000;

/// Maximum total number of values across all entries combined.
/// Prevents the product of entry_count * value_count from causing
/// excessive memory allocation during deserialization.
const MAX_TOTAL_VALUES: u64 = 1_000_000;
const MAX_TOTAL_STRING_BYTES: u64 = 100 * 1024 * 1024;

fn read_len_prefixed_string_counted<R: Read>(
    reader: &mut R,
    total_bytes: &mut u64,
) -> Result<String> {
    let len = reader.read_u32::<LittleEndian>()?;
    if len > MAX_STRING_LENGTH {
        return Err(AudexError::InvalidData(format!(
            "BasicTags string length {} exceeds {} byte limit",
            len, MAX_STRING_LENGTH
        )));
    }
    *total_bytes = total_bytes.checked_add(len as u64).ok_or_else(|| {
        AudexError::InvalidData("BasicTags string byte count overflow".to_string())
    })?;
    if *total_bytes > MAX_TOTAL_STRING_BYTES {
        return Err(AudexError::InvalidData(format!(
            "BasicTags cumulative string bytes {} exceed {} byte limit",
            *total_bytes, MAX_TOTAL_STRING_BYTES
        )));
    }
    let mut buf = vec![0u8; len as usize];
    reader.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|e| {
        AudexError::InvalidData(format!("Invalid UTF-8 data in BasicTags store: {}", e))
    })
}

/// Simple hash-map based tag implementation for formats that support it
///
/// `BasicTags` is the simplest implementation of the [`Tags`], [`Metadata`],
/// and [`MetadataFields`] traits. It stores tags in an in-memory `HashMap<String, Vec<String>>`
/// and serializes to a custom binary format (prefixed with `ADXBTAGS` magic bytes).
///
/// # Examples
///
/// ```rust
/// use audex::tags::{BasicTags, Tags, MetadataFields};
///
/// let mut tags = BasicTags::new();
/// tags.set("artist", vec!["Artist".to_string()]);
/// tags.set_title("My Song".to_string());
///
/// assert_eq!(tags.get("artist"), Some(["Artist".to_string()].as_slice()));
/// assert_eq!(tags.title().map(|s| s.as_str()), Some("My Song"));
/// ```
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BasicTags {
    tags: HashMap<String, Vec<String>>,
}

impl BasicTags {
    /// Create a new empty `BasicTags` instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new `BasicTags` with pre-allocated capacity for the given number of tags
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            tags: HashMap::with_capacity(capacity),
        }
    }
}

impl Tags for BasicTags {
    fn get(&self, key: &str) -> Option<&[String]> {
        self.tags.get(key).map(|v| v.as_slice())
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        if values.is_empty() {
            self.tags.remove(key);
        } else {
            self.tags.insert(key.to_string(), values);
        }
    }

    fn remove(&mut self, key: &str) {
        self.tags.remove(key);
    }

    fn keys(&self) -> Vec<String> {
        self.tags.keys().cloned().collect()
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

impl Metadata for BasicTags {
    type Error = AudexError;

    fn new() -> Self {
        BasicTags::new()
    }

    fn load_from_fileobj(filething: &mut AnyFileThing) -> Result<Self> {
        filething.seek(SeekFrom::Start(0))?;

        let mut magic = [0u8; BASIC_TAGS_MAGIC.len()];
        match filething.read_exact(&mut magic) {
            Ok(()) => {}
            Err(ref err) if err.kind() == ErrorKind::UnexpectedEof => {
                // Empty file, treat as no tags
                return Ok(BasicTags::new());
            }
            Err(err) => return Err(err.into()),
        }

        if magic != *BASIC_TAGS_MAGIC {
            return Err(AudexError::InvalidData(
                "Invalid BasicTags storage header".to_string(),
            ));
        }

        let mut tags = BasicTags::new();
        let entry_count = filething.read_u32::<LittleEndian>()?;
        if entry_count > MAX_ENTRY_COUNT {
            return Err(AudexError::InvalidData(format!(
                "BasicTags entry count {} exceeds {} limit",
                entry_count, MAX_ENTRY_COUNT
            )));
        }
        // Track cumulative value count to prevent the product of
        // entry_count * value_count from causing excessive allocation
        let mut cumulative_values: u64 = 0;
        let mut cumulative_string_bytes: u64 = 0;

        for _ in 0..entry_count {
            let key = read_len_prefixed_string_counted(filething, &mut cumulative_string_bytes)?;
            let value_count = filething.read_u32::<LittleEndian>()?;
            if value_count > MAX_VALUE_COUNT {
                return Err(AudexError::InvalidData(format!(
                    "BasicTags value count {} exceeds {} limit",
                    value_count, MAX_VALUE_COUNT
                )));
            }
            cumulative_values += value_count as u64;
            if cumulative_values > MAX_TOTAL_VALUES {
                return Err(AudexError::InvalidData(format!(
                    "BasicTags cumulative value count {} exceeds {} limit",
                    cumulative_values, MAX_TOTAL_VALUES
                )));
            }
            // Cap pre-allocation to avoid excessive memory usage from
            // untrusted input. The Vec will grow dynamically if needed.
            let capped_capacity = std::cmp::min(value_count as usize, 256);
            let mut values = Vec::with_capacity(capped_capacity);
            for _ in 0..value_count {
                values.push(read_len_prefixed_string_counted(
                    filething,
                    &mut cumulative_string_bytes,
                )?);
            }
            tags.set(&key, values);
        }

        Ok(tags)
    }

    fn save_to_fileobj(&self, filething: &mut AnyFileThing) -> Result<()> {
        let mut buffer = Cursor::new(Vec::new());
        buffer.write_all(BASIC_TAGS_MAGIC)?;

        // Validate tag count fits in u32 before casting to prevent silent truncation
        if self.tags.len() > u32::MAX as usize {
            return Err(AudexError::InvalidData(format!(
                "Tag count {} exceeds maximum of {}",
                self.tags.len(),
                u32::MAX
            )));
        }
        buffer.write_u32::<LittleEndian>(self.tags.len() as u32)?;

        for (key, values) in &self.tags {
            write_len_prefixed_string(&mut buffer, key)?;
            // Validate value count fits in u32
            if values.len() > u32::MAX as usize {
                return Err(AudexError::InvalidData(format!(
                    "Value count for key '{}' exceeds maximum of {}",
                    key,
                    u32::MAX
                )));
            }
            buffer.write_u32::<LittleEndian>(values.len() as u32)?;
            for value in values {
                write_len_prefixed_string(&mut buffer, value)?;
            }
        }

        let data = buffer.into_inner();
        filething.truncate(0)?;
        filething.seek(SeekFrom::Start(0))?;
        filething.write_all(&data)?;
        filething.flush()?;
        Ok(())
    }

    fn delete_from_fileobj(filething: &mut AnyFileThing) -> Result<()> {
        filething.truncate(0)?;
        filething.seek(SeekFrom::Start(0))?;
        filething.flush()?;
        Ok(())
    }
}

// Key casing convention: setters always store keys in uppercase. Getters check
// both uppercase and lowercase variants for backward compatibility with tags
// that may have been written with lowercase keys by other software.
impl MetadataFields for BasicTags {
    fn artist(&self) -> Option<&String> {
        self.get_first("ARTIST")
            .or_else(|| self.get_first("artist"))
    }

    fn set_artist(&mut self, artist: String) {
        self.set_single("ARTIST", artist);
    }

    fn album(&self) -> Option<&String> {
        self.get_first("ALBUM").or_else(|| self.get_first("album"))
    }

    fn set_album(&mut self, album: String) {
        self.set_single("ALBUM", album);
    }

    fn title(&self) -> Option<&String> {
        self.get_first("TITLE").or_else(|| self.get_first("title"))
    }

    fn set_title(&mut self, title: String) {
        self.set_single("TITLE", title);
    }

    fn track_number(&self) -> Option<u32> {
        self.get_first("TRACKNUMBER")
            .or_else(|| self.get_first("tracknumber"))
            .and_then(|s| s.parse().ok())
    }

    fn set_track_number(&mut self, track: u32) {
        self.set_single("TRACKNUMBER", track.to_string());
    }

    fn date(&self) -> Option<&String> {
        self.get_first("DATE").or_else(|| self.get_first("date"))
    }

    fn set_date(&mut self, date: String) {
        self.set_single("DATE", date);
    }

    fn genre(&self) -> Option<&String> {
        self.get_first("GENRE").or_else(|| self.get_first("genre"))
    }

    fn set_genre(&mut self, genre: String) {
        self.set_single("GENRE", genre);
    }
}
