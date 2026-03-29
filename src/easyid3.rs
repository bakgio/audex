//! Simplified ID3v2 tag interface with key-value access
//!
//! This module provides `EasyID3`, a high-level wrapper around ID3v2 tags that
//! offers a simple key-value interface for common tag operations. Instead
//! of working with frame IDs and frame objects, you can use familiar field names
//! like "title", "artist", and "album".
//!
//! # Overview
//!
//! `EasyID3` simplifies ID3v2 tag manipulation by:
//! - Using human-readable key names instead of frame IDs
//! - Providing key-value get/set operations
//! - Automatically handling frame creation and conversion
//! - Supporting common metadata fields out of the box
//! - Allowing pattern-based keys for advanced use cases
//!
//! ## When to Use EasyID3
//!
//! - **Use EasyID3** when you need simple tag operations with standard fields
//! - **Use raw ID3Tags** when you need access to advanced frames, binary data,
//!   or precise control over frame structure
//!
//! # Key Mapping
//!
//! EasyID3 maps user-friendly key names to ID3v2 frame IDs:
//!
//! - `title` → `TIT2` (Title)
//! - `artist` → `TPE1` (Lead artist)
//! - `album` → `TALB` (Album)
//! - `albumartist` → `TPE2` (Album artist)
//! - `date` → `TDRC` (Recording date)
//! - `genre` → `TCON` (Genre)
//! - `tracknumber` → `TRCK` (Track number)
//! - `discnumber` → `TPOS` (Disc number)
//! - And many more...
//!
//! # Basic Usage
//!
//! ## Reading Tags
//!
//! ```no_run
//! use audex::easyid3::EasyID3;
//! use audex::FileType;
//!
//! // Load tags from MP3 file
//! let tags = EasyID3::load("song.mp3").unwrap();
//!
//! // Get individual fields
//! if let Some(title) = tags.get("title") {
//!     println!("Title: {}", title.join(", "));
//! }
//!
//! // Check if field exists
//! if tags.contains_key("artist") {
//!     println!("Artist field is present");
//! }
//!
//! // List all available keys
//! for key in tags.keys() {
//!     println!("Found key: {}", key);
//! }
//! ```
//!
//! ## Writing Tags
//!
//! ```no_run
//! use audex::easyid3::EasyID3;
//! use audex::FileType;
//!
//! let mut tags = EasyID3::new();
//!
//! // Set single values
//! tags.set("title", &["My Song".to_string()]).unwrap();
//! tags.set("artist", &["My Band".to_string()]).unwrap();
//! tags.set("album", &["My Album".to_string()]).unwrap();
//! tags.set("date", &["2024".to_string()]).unwrap();
//!
//! // Set multi-value fields (e.g., multiple artists)
//! tags.set("artist", &[
//!     "Artist 1".to_string(),
//!     "Artist 2".to_string()
//! ]).unwrap();
//!
//! // Save to file
//! tags.filename = Some("song.mp3".to_string());
//! tags.save_to_file().unwrap();
//! ```
//!
//! ## Modifying Existing Tags
//!
//! ```no_run
//! use audex::easyid3::EasyID3;
//! use audex::FileType;
//!
//! // Load existing tags
//! let mut tags = EasyID3::load("song.mp3").unwrap();
//!
//! // Modify fields
//! tags.set("title", &["New Title".to_string()]).unwrap();
//!
//! // Remove fields
//! tags.remove("comment").unwrap();
//!
//! // Save changes back to the file
//! tags.save_to_file().unwrap();
//! ```
//!
//! # Supported Keys
//!
//! ## Basic Metadata
//! - `title` - Track title
//! - `artist` - Lead artist/performer
//! - `album` - Album title
//! - `albumartist` - Album artist (for compilations)
//! - `date` - Release date (YYYY, YYYY-MM, or YYYY-MM-DD)
//! - `originaldate` - Original release date
//! - `genre` - Genre
//! - `comment` - Comment/notes
//!
//! ## Track Information
//! - `tracknumber` - Track number (e.g., "1" or "1/12")
//! - `discnumber` - Disc number (e.g., "1" or "1/2")
//! - `isrc` - International Standard Recording Code
//! - `copyright` - Copyright information
//! - `encodedby` - Encoder software/person
//!
//! ## Credits
//! - `composer` - Composer name(s)
//! - `lyricist` - Lyricist/text writer
//! - `conductor` - Conductor name
//! - `arranger` - Arranger name
//! - `performer:*` - Specific performer roles (e.g., `performer:guitar`)
//!
//! ## Other
//! - `bpm` - Beats per minute (tempo)
//! - `language` - Language code (ISO 639-2)
//! - `mood` - Mood description
//! - `media` - Media type
//! - `website` - Official website URL
//! - `replaygain_*_gain` - ReplayGain values
//! - `musicbrainz_*` - MusicBrainz IDs
//!
//! # Pattern Keys
//!
//! EasyID3 supports pattern-based keys for flexible field access:
//!
//! - `performer:guitar` - Guitarist credit
//! - `performer:vocals` - Vocalist credit
//! - `website:official` - Official website
//! - `replaygain_track_gain` - Track-level ReplayGain
//! - `musicbrainz_trackid` - MusicBrainz track ID
//!
//! # Limitations
//!
//! EasyID3 is designed for simplicity and doesn't support all ID3v2 features:
//! - No direct access to binary frames (e.g., `APIC` for pictures)
//! - No access to frame-level metadata (encoding, flags, etc.)
//! - Limited control over multi-value formatting
//!
//! For advanced use cases, use the full [`crate::id3::ID3Tags`] interface instead.
//!
//! # See Also
//!
//! - [`crate::id3::ID3Tags`]: Full ID3v2 tag interface
//! - [`crate::easymp4::EasyMP4Tags`]: Similar interface for MP4/M4A files
//! - Full ID3v2 frame reference in the `id3` module

use crate::id3::ID3Tags;
use crate::{AudexError, FileType, Result};
use globset::{Glob, GlobMatcher};
use std::collections::HashMap;
// Index/IndexMut intentionally not implemented — callers should use get()/set()
use std::path::Path;
use std::sync::LazyLock;
use std::sync::Mutex;

/// Acquire a mutex lock, recovering from poisoning with a warning.
///
/// The EasyID3 global registries store function pointers that are registered
/// atomically, so a poisoned mutex does not imply corrupted data. We recover
/// the inner guard but emit a warning so the condition is observable.
macro_rules! lock_or_warn {
    ($mutex:expr, $name:expr) => {
        $mutex.lock().unwrap_or_else(|e| {
            warn_event!(
                registry = $name,
                "mutex was poisoned — recovering inner guard"
            );
            e.into_inner()
        })
    };
}

/// Easy ID3 wrapper with key-value frame access
///
/// Provides a simplified API for reading and writing common ID3v2 tags
/// using human-readable key names (e.g., "title", "artist") instead of
/// frame IDs (e.g., "TIT2", "TPE1").
///
/// # Internal Structure
///
/// - `easy_id3.id3` — the underlying [`ID3Tags`] instance
/// - `easy_id3.id3.dict` — the raw frame dictionary (`BTreeMap<String, Box<dyn Frame>>`)
///
/// For advanced frame manipulation, access the internal ID3Tags dictionary directly:
/// ```ignore
/// easy_id3.id3.dict.insert("TDAT".to_string(), Box::new(some_frame));
/// ```
#[derive(Debug)]
pub struct EasyID3 {
    /// ID3Tags instance with direct frame dictionary access
    pub id3: crate::id3::tags::ID3Tags,
    /// File path for saving
    pub filename: Option<String>,
    trait_cache: HashMap<String, Vec<String>>,
}

/// Custom error for EasyID3 key operations
#[derive(Debug, thiserror::Error)]
pub enum EasyID3Error {
    /// The provided key name is not valid or cannot be mapped to an ID3 frame.
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    /// No getter/setter/deleter is registered for this key.
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    /// A glob pattern used for key matching is malformed.
    #[error("Pattern matching error: {0}")]
    PatternError(String),
    /// An error occurred while reading or writing the underlying ID3 frame data.
    #[error("Frame operation error: {0}")]
    FrameError(String),
}

/// Handler function types for EasyID3 operations
/// Using Box<dyn Fn> instead of fn pointers to allow closures
type GetterFn = Box<dyn Fn(&ID3Tags, &str) -> Result<Vec<String>> + Send + Sync>;
type SetterFn = Box<dyn Fn(&mut ID3Tags, &str, &[String]) -> Result<()> + Send + Sync>;
type DeleterFn = Box<dyn Fn(&mut ID3Tags, &str) -> Result<()> + Send + Sync>;

/// Pattern matcher for key patterns
struct PatternMatcher {
    matchers: Vec<(String, GlobMatcher)>,
}

impl PatternMatcher {
    fn new() -> Self {
        Self {
            matchers: Vec::new(),
        }
    }

    fn add_pattern(&mut self, pattern: &str) -> Result<()> {
        let glob = Glob::new(pattern).map_err(|e| {
            AudexError::InvalidData(format!("Invalid glob pattern {}: {}", pattern, e))
        })?;
        self.matchers
            .push((pattern.to_string(), glob.compile_matcher()));
        Ok(())
    }

    fn matches(&self, key: &str) -> Option<&str> {
        for (pattern, matcher) in &self.matchers {
            if matcher.is_match(key) {
                return Some(pattern);
            }
        }
        None
    }
}

/// Maximum number of entries allowed in each global registry. This prevents
/// unbounded growth from buggy callers in long-running processes.
const MAX_REGISTRY_ENTRIES: usize = 1024;

/// Global registry for getter functions (mutable for runtime registration)
static GET_REGISTRY: LazyLock<Mutex<HashMap<String, GetterFn>>> = LazyLock::new(|| {
    let mut registry = HashMap::new();
    register_standard_keys(&mut registry);
    Mutex::new(registry)
});

/// Global registry for setter functions (mutable for runtime registration)
static SET_REGISTRY: LazyLock<Mutex<HashMap<String, SetterFn>>> = LazyLock::new(|| {
    let mut registry = HashMap::new();
    register_standard_setters(&mut registry);
    Mutex::new(registry)
});

/// Global registry for deleter functions (mutable for runtime registration)
static DELETE_REGISTRY: LazyLock<Mutex<HashMap<String, DeleterFn>>> = LazyLock::new(|| {
    let mut registry = HashMap::new();
    register_standard_deleters(&mut registry);
    Mutex::new(registry)
});

/// Pattern matcher for complex keys
static PATTERN_MATCHER: LazyLock<std::sync::Mutex<PatternMatcher>> = LazyLock::new(|| {
    let mut matcher = PatternMatcher::new();
    let _ = matcher.add_pattern("performer:*");
    let _ = matcher.add_pattern("replaygain_*_gain");
    let _ = matcher.add_pattern("replaygain_*_peak");
    let _ = matcher.add_pattern("musicbrainz_*");
    let _ = matcher.add_pattern("website:*");
    std::sync::Mutex::new(matcher)
});

impl EasyID3 {
    /// Create a new empty EasyID3 instance with default ID3v2.4 tags and no file association.
    pub fn new() -> Self {
        let mut easy = Self {
            id3: crate::id3::tags::ID3Tags::new(),
            filename: None,
            trait_cache: HashMap::new(),
        };
        easy.refresh_trait_cache();
        easy
    }

    fn refresh_trait_cache(&mut self) {
        let keys = self.keys();
        let mut cache = HashMap::with_capacity(keys.len());
        for key in keys {
            if let Some(values) = EasyID3::get(self, &key) {
                cache.insert(key, values);
            }
        }
        self.trait_cache = cache;
    }

    pub fn keys(&self) -> Vec<String> {
        let mut keys = Vec::new();

        {
            let tags = &self.id3;
            // Check all registered getters for keys with values
            let registry = lock_or_warn!(GET_REGISTRY, "GET_REGISTRY");
            for (key, getter) in registry.iter() {
                if let Ok(values) = getter(tags, key) {
                    if !values.is_empty() {
                        keys.push(key.clone());
                    }
                }
            }

            // Check for TXXX frames (fallback keys)
            if let Some(txxx_frames) = tags.get_frames("TXXX") {
                for frame in txxx_frames {
                    if let Some((desc, _)) = self.extract_txxx_content(frame) {
                        let key = desc.to_lowercase();
                        if !keys.contains(&key) {
                            keys.push(key);
                        }
                    }
                }
            }

            // Expose one easy key per TMCL role so performer credits are
            // addressable through both the inherent API and the Tags trait.
            for (role, _) in Self::read_tmcl_entries(tags) {
                if !role.is_empty() {
                    let key = format!("performer:{}", role.to_lowercase());
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                }
            }
        }

        keys.sort();
        keys.dedup();
        keys
    }

    pub fn contains_key(&self, key: &str) -> bool {
        let key_lower = key.to_lowercase();

        let registry = lock_or_warn!(GET_REGISTRY, "GET_REGISTRY");
        if registry.contains_key(&key_lower) {
            let tags = &self.id3;
            if let Some(getter) = registry.get(&key_lower) {
                if let Ok(values) = getter(tags, &key_lower) {
                    return !values.is_empty();
                }
            }
        }
        drop(registry);

        // Check pattern matches — verify actual data exists, not just pattern match
        let matcher = lock_or_warn!(PATTERN_MATCHER, "PATTERN_MATCHER");
        if let Some(pattern) = matcher.matches(&key_lower) {
            let tags = &self.id3;
            if let Some(values) = self.handle_pattern_get(tags, &key_lower, pattern) {
                return !values.is_empty();
            }
        }

        false
    }

    pub fn get(&self, key: &str) -> Option<Vec<String>> {
        trace_event!(key = %key, "EasyID3 get");
        let key_lower = key.to_lowercase();

        {
            let tags = &self.id3;
            // Try direct lookup first
            let registry = lock_or_warn!(GET_REGISTRY, "GET_REGISTRY");
            if let Some(getter) = registry.get(&key_lower) {
                if let Ok(values) = getter(tags, &key_lower) {
                    if !values.is_empty() {
                        return Some(values);
                    }
                }
            }
            drop(registry);

            // Try pattern matching
            let matcher = lock_or_warn!(PATTERN_MATCHER, "PATTERN_MATCHER");
            if let Some(pattern) = matcher.matches(&key_lower) {
                return self.handle_pattern_get(tags, &key_lower, pattern);
            }
            drop(matcher);

            // Try fallback for unknown keys
            self.fallback_get(tags, &key_lower)
        }
    }

    pub fn set(&mut self, key: &str, values: &[String]) -> Result<()> {
        trace_event!(key = %key, count = values.len(), "EasyID3 set");
        let key_lower = key.to_lowercase();

        let result = {
            let tags = &mut self.id3;
            // Try direct lookup first
            let registry = lock_or_warn!(SET_REGISTRY, "SET_REGISTRY");
            if let Some(setter) = registry.get(&key_lower) {
                setter(tags, &key_lower, values)
            } else {
                drop(registry);

                // Try pattern matching
                let matcher = lock_or_warn!(PATTERN_MATCHER, "PATTERN_MATCHER");
                if let Some(pattern) = matcher.matches(&key_lower) {
                    EasyID3::handle_pattern_set_static(tags, &key_lower, pattern, values)
                } else {
                    drop(matcher);

                    // Try fallback for unknown keys
                    EasyID3::fallback_set_static(tags, &key_lower, values)
                }
            }
        };
        if result.is_ok() {
            self.refresh_trait_cache();
        }
        result
    }

    /// Remove value by key
    pub fn remove(&mut self, key: &str) -> Result<()> {
        let key_lower = key.to_lowercase();

        let result = {
            let tags = &mut self.id3;
            // Try direct lookup first
            let registry = lock_or_warn!(DELETE_REGISTRY, "DELETE_REGISTRY");
            if let Some(deleter) = registry.get(&key_lower) {
                deleter(tags, &key_lower)
            } else {
                drop(registry);

                // Try pattern matching
                let matcher = lock_or_warn!(PATTERN_MATCHER, "PATTERN_MATCHER");
                if let Some(pattern) = matcher.matches(&key_lower) {
                    EasyID3::handle_pattern_delete_static(tags, &key_lower, pattern)
                } else {
                    drop(matcher);

                    // Try fallback for unknown keys
                    EasyID3::fallback_delete_static(tags, &key_lower)
                }
            }
        };
        if result.is_ok() {
            self.refresh_trait_cache();
        }
        result
    }

    /// Register a text key to map to a specific frame ID.
    ///
    /// This allows adding custom mappings at runtime. The key will be converted
    /// to lowercase for case-insensitive matching.
    ///
    /// # Global State Warning
    ///
    /// This method mutates a **global** registry shared by all `EasyID3`
    /// instances in the process. A registration on one instance immediately
    /// affects every other instance. If two threads register the same key
    /// with different frame IDs, the last writer wins silently.
    ///
    /// For deterministic behavior, register all custom keys once during
    /// application startup before creating worker threads.
    ///
    /// # Arguments
    /// * `key` - The easy key name (e.g., "encoded")
    /// * `frame_id` - The ID3 frame ID (e.g., "TSSE")
    ///
    /// # Example
    /// ```rust,no_run
    /// use audex::easyid3::EasyID3;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut tags = EasyID3::new();
    /// tags.register_text_key("encoded", "TSSE")?;
    /// tags.set("encoded", &["Some encoder".to_string()])?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn register_text_key(&mut self, key: &str, frame_id: &str) -> Result<()> {
        debug_event!(key = %key, frame_id = %frame_id, "registered EasyID3 text key");
        let key_lower = key.to_lowercase();
        let frame_id = frame_id.to_string();

        // Create getter function
        let getter_frame_id = frame_id.clone();
        let getter: GetterFn = Box::new(move |tags, _key| {
            if let Some(text_values) = tags.get_text_values(&getter_frame_id) {
                Ok(text_values)
            } else {
                Ok(vec![])
            }
        });

        // Create setter function
        let setter_frame_id = frame_id.clone();
        let setter: SetterFn = Box::new(move |tags, _key, values| {
            if values.is_empty() {
                tags.remove(&setter_frame_id);
            } else {
                // Remove existing frames first
                tags.remove(&setter_frame_id);
                // Add all values as separate text values (ID3v2.4 supports multiple values)
                tags.add_text_frame(&setter_frame_id, values.to_vec())?;
            }
            Ok(())
        });

        // Create deleter function
        let deleter_frame_id = frame_id.clone();
        let deleter: DeleterFn = Box::new(move |tags, _key| {
            tags.remove(&deleter_frame_id);
            Ok(())
        });

        // Acquire all three registry locks before performing any insertions.
        // This ensures atomicity: either all registries are updated together,
        // or none are, preventing inconsistent state across registries.
        {
            let mut get_reg = lock_or_warn!(GET_REGISTRY, "GET_REGISTRY");
            let mut set_reg = lock_or_warn!(SET_REGISTRY, "SET_REGISTRY");
            let mut del_reg = lock_or_warn!(DELETE_REGISTRY, "DELETE_REGISTRY");

            if get_reg.contains_key(&key_lower)
                || set_reg.contains_key(&key_lower)
                || del_reg.contains_key(&key_lower)
            {
                return Err(crate::AudexError::InvalidData(format!(
                    "EasyID3 key '{}' is already registered and cannot be replaced",
                    key
                )));
            }

            if !get_reg.contains_key(&key_lower) && get_reg.len() >= MAX_REGISTRY_ENTRIES {
                return Err(crate::AudexError::InvalidData(format!(
                    "EasyID3 registry limit reached ({} entries); cannot register key '{}'",
                    MAX_REGISTRY_ENTRIES, key
                )));
            }
            if !set_reg.contains_key(&key_lower) && set_reg.len() >= MAX_REGISTRY_ENTRIES {
                return Err(crate::AudexError::InvalidData(format!(
                    "EasyID3 SET registry limit reached ({} entries); cannot register key '{}'",
                    MAX_REGISTRY_ENTRIES, key
                )));
            }
            if !del_reg.contains_key(&key_lower) && del_reg.len() >= MAX_REGISTRY_ENTRIES {
                return Err(crate::AudexError::InvalidData(format!(
                    "EasyID3 DELETE registry limit reached ({} entries); cannot register key '{}'",
                    MAX_REGISTRY_ENTRIES, key
                )));
            }

            get_reg.insert(key_lower.clone(), getter);
            set_reg.insert(key_lower.clone(), setter);
            del_reg.insert(key_lower.clone(), deleter);
        }

        self.refresh_trait_cache();
        Ok(())
    }

    /// Register a TXXX key to map to a TXXX frame with specific description.
    ///
    /// This allows adding custom TXXX frame mappings at runtime. The key will be
    /// converted to lowercase for case-insensitive matching.
    ///
    /// # Global State Warning
    ///
    /// This method mutates a **global** registry shared by all `EasyID3`
    /// instances in the process. See [`register_text_key`](Self::register_text_key)
    /// for details on the implications of global-state mutation.
    ///
    /// # Arguments
    /// * `key` - The easy key name (e.g., "compatible_brands")
    /// * `description` - The TXXX frame description (e.g., "compatible_brands")
    ///
    /// # Example
    /// ```rust,no_run
    /// use audex::easyid3::EasyID3;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut tags = EasyID3::new();
    /// tags.register_txxx_key("compatible_brands", "compatible_brands")?;
    /// tags.set("compatible_brands", &["mp41".to_string()])?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn register_txxx_key(&mut self, key: &str, description: &str) -> Result<()> {
        debug_event!(key = %key, description = %description, "registered EasyID3 TXXX key");
        let key_lower = key.to_lowercase();
        let description = description.to_string();

        // Create getter function — use structured frame API to avoid
        // misparsing descriptions that contain " = "
        let getter_desc = description.clone();
        let getter: GetterFn = Box::new(move |tags, _key| {
            if let Some(frames) = tags.get_frames("TXXX") {
                for frame in frames {
                    if let Some(txxx) = frame.as_any().downcast_ref::<crate::id3::frames::TXXX>() {
                        if txxx.description.eq_ignore_ascii_case(&getter_desc) {
                            return Ok(txxx.text.clone());
                        }
                    }
                }
            }
            Ok(vec![])
        });

        // Create setter function
        let setter_desc = description.clone();
        let setter: SetterFn = Box::new(move |tags, _key, values| {
            // Remove existing TXXX frames with this description first
            let pattern = format!("TXXX:{}", setter_desc);
            tags.delall(&pattern);

            if !values.is_empty() {
                // Use Latin1 for ASCII-safe text, UTF-16 for non-Latin1
                use crate::id3::{frames::TXXX, specs::TextEncoding};
                let all_latin1 = setter_desc.chars().all(|c| (c as u32) <= 255)
                    && values.iter().all(|v| v.chars().all(|c| (c as u32) <= 255));
                let encoding = if all_latin1 {
                    TextEncoding::Latin1
                } else {
                    TextEncoding::Utf16
                };
                let txxx_frame = TXXX::new(encoding, setter_desc.clone(), values.to_vec());
                tags.add(Box::new(txxx_frame))?;
            }

            Ok(())
        });

        // Create deleter function
        let deleter_desc = description.clone();
        let deleter: DeleterFn = Box::new(move |tags, _key| {
            // Use pattern-based deletion for TXXX frames
            let pattern = format!("TXXX:{}", deleter_desc);
            tags.delall(&pattern);
            Ok(())
        });

        // Acquire all three registry locks before performing any insertions.
        // This ensures atomicity: either all registries are updated together,
        // or none are, preventing inconsistent state across registries.
        {
            let mut get_reg = lock_or_warn!(GET_REGISTRY, "GET_REGISTRY");
            let mut set_reg = lock_or_warn!(SET_REGISTRY, "SET_REGISTRY");
            let mut del_reg = lock_or_warn!(DELETE_REGISTRY, "DELETE_REGISTRY");

            if !get_reg.contains_key(&key_lower) && get_reg.len() >= MAX_REGISTRY_ENTRIES {
                return Err(crate::AudexError::InvalidData(format!(
                    "EasyID3 registry limit reached ({} entries); cannot register key '{}'",
                    MAX_REGISTRY_ENTRIES, key
                )));
            }
            if !set_reg.contains_key(&key_lower) && set_reg.len() >= MAX_REGISTRY_ENTRIES {
                return Err(crate::AudexError::InvalidData(format!(
                    "EasyID3 SET registry limit reached ({} entries); cannot register key '{}'",
                    MAX_REGISTRY_ENTRIES, key
                )));
            }
            if !del_reg.contains_key(&key_lower) && del_reg.len() >= MAX_REGISTRY_ENTRIES {
                return Err(crate::AudexError::InvalidData(format!(
                    "EasyID3 DELETE registry limit reached ({} entries); cannot register key '{}'",
                    MAX_REGISTRY_ENTRIES, key
                )));
            }

            get_reg.insert(key_lower.clone(), getter);
            set_reg.insert(key_lower.clone(), setter);
            del_reg.insert(key_lower.clone(), deleter);
        }

        Ok(())
    }

    /// Remove a key and return its values
    ///
    /// This removes the key from the tags and returns its current values.
    /// If the key doesn't exist or has no values, returns None.
    ///
    /// # Arguments
    /// * `key` - The key to remove
    ///
    /// # Returns
    /// * `Some(Vec<String>)` - The values that were stored for this key
    /// * `None` - If the key had no values or didn't exist
    ///
    /// # Example
    /// ```rust
    /// use audex::easyid3::EasyID3;
    ///
    /// let mut tags = EasyID3::new();
    /// if let Some(values) = tags.pop("encoded") {
    ///     println!("Removed encoded: {:?}", values);
    /// }
    /// ```
    pub fn pop(&mut self, key: &str) -> Option<Vec<String>> {
        // First get the current values
        let values = self.get(key);

        // Then remove the key
        let _ = self.remove(key);

        values
    }

    // Pattern handling methods
    fn handle_pattern_get(&self, tags: &ID3Tags, key: &str, pattern: &str) -> Option<Vec<String>> {
        match pattern {
            "performer:*" => self.get_performer(tags, key),
            "replaygain_*_gain" => self.get_replaygain_gain(tags, key),
            "replaygain_*_peak" => self.get_replaygain_peak(tags, key),
            "musicbrainz_*" => self.get_musicbrainz(tags, key),
            "website:*" => self.get_website(tags, key),
            _ => None,
        }
    }

    // Static versions to avoid borrowing issues
    fn handle_pattern_set_static(
        tags: &mut ID3Tags,
        key: &str,
        pattern: &str,
        values: &[String],
    ) -> Result<()> {
        match pattern {
            "performer:*" => Self::set_performer_static(tags, key, values),
            "replaygain_*_gain" => Self::set_replaygain_gain_static(tags, key, values),
            "replaygain_*_peak" => Self::set_replaygain_peak_static(tags, key, values),
            "musicbrainz_*" => Self::set_musicbrainz_static(tags, key, values),
            "website:*" => Self::set_website_static(tags, key, values),
            _ => Err(AudexError::InvalidData(format!(
                "Unknown pattern: {}",
                pattern
            ))),
        }
    }

    fn handle_pattern_delete_static(tags: &mut ID3Tags, key: &str, pattern: &str) -> Result<()> {
        match pattern {
            "performer:*" => Self::delete_performer_static(tags, key),
            "replaygain_*_gain" => Self::delete_replaygain_gain_static(tags, key),
            "replaygain_*_peak" => Self::delete_replaygain_peak_static(tags, key),
            "musicbrainz_*" => Self::delete_musicbrainz_static(tags, key),
            "website:*" => Self::delete_website_static(tags, key),
            _ => Err(AudexError::InvalidData(format!(
                "Unknown pattern: {}",
                pattern
            ))),
        }
    }

    // Fallback methods for unregistered keys
    fn fallback_get(&self, tags: &ID3Tags, key: &str) -> Option<Vec<String>> {
        // Try to get TXXX frame with this description
        self.get_txxx_frame(tags, key)
    }

    fn fallback_set_static(tags: &mut ID3Tags, key: &str, values: &[String]) -> Result<()> {
        // Create/update TXXX frame with this description
        Self::set_txxx_frame_static(tags, key, values)
    }

    fn fallback_delete_static(tags: &mut ID3Tags, key: &str) -> Result<()> {
        // Delete TXXX frame with this description
        Self::delete_txxx_frame_static(tags, key)
    }

    // Specialized handler methods
    fn get_performer(&self, tags: &ID3Tags, key: &str) -> Option<Vec<String>> {
        // Extract role from key like "performer:guitar"
        if let Some(role) = key.strip_prefix("performer:") {
            self.get_performer_role(tags, role)
        } else {
            None
        }
    }

    fn get_performer_role(&self, tags: &ID3Tags, role: &str) -> Option<Vec<String>> {
        // Look for TMCL frame and extract performers with matching role
        if let Some(frames) = tags.get_frames("TMCL") {
            for frame in frames {
                if let Some(text_values) = self.extract_text_from_frame(frame) {
                    // TMCL format: role\0performer\0role\0performer...
                    let mut performers = Vec::new();
                    for i in (0..text_values.len()).step_by(2) {
                        if i + 1 < text_values.len() && text_values[i] == role {
                            performers.push(text_values[i + 1].clone());
                        }
                    }
                    if !performers.is_empty() {
                        return Some(performers);
                    }
                }
            }
        }
        None
    }

    fn get_replaygain_gain(&self, _tags: &ID3Tags, key: &str) -> Option<Vec<String>> {
        // Extract track type from key like "replaygain_track_gain"
        if let Some(track_type) = key.strip_prefix("replaygain_") {
            if let Some(track_type) = track_type.strip_suffix("_gain") {
                // Look for RVA2 frame with this identifier
                return self.get_rva2_gain(track_type);
            }
        }
        None
    }

    fn get_replaygain_peak(&self, _tags: &ID3Tags, key: &str) -> Option<Vec<String>> {
        // Similar to gain but for peak values
        if let Some(track_type) = key.strip_prefix("replaygain_") {
            if let Some(track_type) = track_type.strip_suffix("_peak") {
                return self.get_rva2_peak(track_type);
            }
        }
        None
    }

    fn get_musicbrainz(&self, tags: &ID3Tags, key: &str) -> Option<Vec<String>> {
        // MusicBrainz keys are stored as TXXX frames with conventional mixed-case descriptions
        let txxx_desc = Self::musicbrainz_txxx_desc(key);
        self.get_txxx_frame(tags, &txxx_desc)
    }

    fn get_website(&self, tags: &ID3Tags, key: &str) -> Option<Vec<String>> {
        // Website keys like "website:official" map to WOAR frames
        if key.starts_with("website:") {
            // Look for WOAR frames
            if let Some(frames) = tags.get_frames("WOAR") {
                let mut urls = Vec::new();
                for frame in frames {
                    if let Some(url) = self.extract_url_from_frame(frame) {
                        urls.push(url);
                    }
                }
                if !urls.is_empty() {
                    return Some(urls);
                }
            }
        }
        None
    }

    fn get_txxx_frame(&self, tags: &ID3Tags, description: &str) -> Option<Vec<String>> {
        // Look for TXXX frames with matching description
        if let Some(frames) = tags.get_frames("TXXX") {
            for frame in frames {
                if let Some((desc, value)) = self.extract_txxx_content(frame) {
                    if desc.to_uppercase() == description.to_uppercase() {
                        return Some(vec![value]);
                    }
                }
            }
        }
        None
    }

    fn get_rva2_gain(&self, track_type: &str) -> Option<Vec<String>> {
        // Get RVA2 frame and extract gain values for specific track type
        if let Some(tags) = self.tags() {
            // Look for RVA2 frame with matching identification
            let key = format!("RVA2:{}", track_type);
            if let Some(frame) = tags.get_frame(&key) {
                // Downcast to RVA2 to access fields
                if let Some(rva2) = frame.as_any().downcast_ref::<crate::id3::frames::RVA2>() {
                    // Get master channel gain
                    if let Some((gain, _)) = rva2.get_master() {
                        return Some(vec![format!("{:+.6} dB", gain)]);
                    }
                }
            }
        }
        None
    }

    fn get_rva2_peak(&self, track_type: &str) -> Option<Vec<String>> {
        // Get RVA2 frame and extract peak values for specific track type
        if let Some(tags) = self.tags() {
            // Look for RVA2 frame with matching identification
            let key = format!("RVA2:{}", track_type);
            if let Some(frame) = tags.get_frame(&key) {
                // Downcast to RVA2 to access fields
                if let Some(rva2) = frame.as_any().downcast_ref::<crate::id3::frames::RVA2>() {
                    // Get master channel peak
                    if let Some((_, peak)) = rva2.get_master() {
                        return Some(vec![format!("{:.6}", peak)]);
                    }
                }
            }
        }
        None
    }

    fn extract_text_from_frame(
        &self,
        frame: &dyn crate::id3::frames::Frame,
    ) -> Option<Vec<String>> {
        // Extract text values from frame based on its type
        if let Some(text_values) = frame.text_values() {
            Some(text_values)
        } else {
            // For non-text frames, try to extract from description
            let desc = frame.description();
            if let Some(colon_pos) = desc.find(": ") {
                Some(vec![desc[colon_pos + 2..].to_string()])
            } else {
                None
            }
        }
    }

    fn extract_url_from_frame(&self, frame: &dyn crate::id3::frames::Frame) -> Option<String> {
        // Extract URL from URL frame
        let frame_id = frame.frame_id();
        if frame_id.starts_with('W') {
            // For URL frames, extract the URL from description
            let desc = frame.description();
            desc.find(": ")
                .map(|colon_pos| desc[colon_pos + 2..].to_string())
        } else {
            None
        }
    }

    fn extract_txxx_content(
        &self,
        frame: &dyn crate::id3::frames::Frame,
    ) -> Option<(String, String)> {
        if frame.frame_id() != "TXXX" {
            return None;
        }

        // Use typed downcast to extract description and value directly,
        // avoiding fragile string splitting on " = " which breaks when
        // the description or value itself contains that pattern
        if let Some(txxx) = frame.as_any().downcast_ref::<crate::id3::frames::TXXX>() {
            let value = txxx.text.join("/");
            return Some((txxx.description.clone(), value));
        }

        // Fallback to string parsing for non-standard frame implementations
        let description = frame.description();
        if let Some(start) = description.find("TXXX: ") {
            if let Some(equals_pos) = description[start + 6..].find(" = ") {
                let desc_part = &description[start + 6..start + 6 + equals_pos];
                let value_part = &description[start + 6 + equals_pos + 3..];
                return Some((desc_part.to_string(), value_part.to_string()));
            }
        }

        None
    }

    /// Map a MusicBrainz key to its conventional TXXX description.
    /// Well-known keys use mixed-case descriptions that match the format
    /// expected by other tagging software. Unknown keys fall back to uppercase.
    fn musicbrainz_txxx_desc(key: &str) -> String {
        match key {
            "musicbrainz_trackid" => "MusicBrainz Track Id".to_string(),
            "musicbrainz_albumid" => "MusicBrainz Album Id".to_string(),
            "musicbrainz_artistid" => "MusicBrainz Artist Id".to_string(),
            "musicbrainz_albumartistid" => "MusicBrainz Album Artist Id".to_string(),
            "musicbrainz_releasegroupid" => "MusicBrainz Release Group Id".to_string(),
            "musicbrainz_workid" => "MusicBrainz Work Id".to_string(),
            "musicbrainz_trmid" => "MusicBrainz TRM Id".to_string(),
            "musicbrainz_discid" => "MusicBrainz Disc Id".to_string(),
            "musicbrainz_albumstatus" => "MusicBrainz Album Status".to_string(),
            "musicbrainz_albumtype" => "MusicBrainz Album Type".to_string(),
            "musicbrainz_releasetrackid" => "MusicBrainz Release Track Id".to_string(),
            // Unknown keys fall back to uppercase for compatibility
            other => other.to_uppercase(),
        }
    }

    /// Read TMCL frame entries as (role, performer) pairs.
    /// The TMCL frame stores alternating [role, performer, role, performer, ...] text values.
    fn read_tmcl_entries(tags: &ID3Tags) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        if let Some(frames) = tags.get_frames("TMCL") {
            for frame in frames {
                if let Some(text_values) = frame.text_values() {
                    for i in (0..text_values.len()).step_by(2) {
                        if i + 1 < text_values.len() {
                            pairs.push((text_values[i].clone(), text_values[i + 1].clone()));
                        }
                    }
                }
            }
        }
        pairs
    }

    // Static setter methods for patterns
    fn set_performer_static(tags: &mut ID3Tags, key: &str, values: &[String]) -> Result<()> {
        // Extract role from key if present (e.g., "performer:guitar" -> "guitar")
        let role = key.split(':').nth(1).unwrap_or("");

        // Read existing TMCL entries so we can preserve other roles
        let mut existing_entries = Self::read_tmcl_entries(tags);

        // Remove all entries for the targeted role
        if !role.is_empty() {
            existing_entries.retain(|(existing_role, _)| existing_role != role);
        } else {
            existing_entries.clear();
        }

        // Append new entries for the targeted role
        if !role.is_empty() {
            for name in values {
                existing_entries.push((role.to_string(), name.clone()));
            }
        } else {
            // No specific role — treat values as raw entries
            for name in values {
                existing_entries.push((String::new(), name.clone()));
            }
        }

        // Write the merged entries back, or remove the frame if empty
        if existing_entries.is_empty() {
            tags.remove("TMCL");
        } else {
            let text_entries: Vec<String> = existing_entries
                .into_iter()
                .flat_map(|(role, performer)| vec![role, performer])
                .collect();
            // Remove old frame first, then write the merged result
            tags.remove("TMCL");
            tags.add_text_frame("TMCL", text_entries)?;
        }
        Ok(())
    }

    fn set_replaygain_gain_static(tags: &mut ID3Tags, key: &str, values: &[String]) -> Result<()> {
        // Set RVA2 frame for gain
        // Extract track type from key (e.g., "replaygain_track_gain" -> "track")
        let track_type = if key.contains("track") {
            "track"
        } else if key.contains("album") {
            "album"
        } else {
            // Invalid key - must contain "track" or "album"
            return Err(AudexError::InvalidData(format!(
                "Invalid replaygain gain key: {}. Must contain 'track' or 'album'",
                key
            )));
        };

        if values.is_empty() {
            // Remove the RVA2 frame for this track type
            Self::delete_replaygain_gain_static(tags, key)?;
        } else {
            // Parse gain value and reject non-finite results
            let gain_str = values[0].split_whitespace().next().unwrap_or(&values[0]);
            let gain = gain_str
                .parse::<f32>()
                .map_err(|_| AudexError::InvalidData("Invalid gain value".to_string()))?;
            if !gain.is_finite() {
                return Err(AudexError::InvalidData(format!(
                    "Gain value must be finite, got: {}",
                    gain
                )));
            }

            // Use uppercase identification to match specification (e.g., "TRACK", "ALBUM")
            let frame_key = format!("RVA2:{}", track_type.to_uppercase());

            // Check if frame exists and get its peak value
            let existing_peak = if let Some(frame) = tags.get_frame(&frame_key) {
                if let Some(rva2) = frame.as_any().downcast_ref::<crate::id3::frames::RVA2>() {
                    rva2.get_master().map(|(_, peak)| peak)
                } else {
                    None
                }
            } else {
                None
            };

            // Remove old frame
            tags.remove(&frame_key);

            // Create new RVA2 frame with updated gain
            let peak = existing_peak.unwrap_or(0.0);
            let rva2 = crate::id3::frames::RVA2::new(
                track_type.to_uppercase(),
                vec![(crate::id3::frames::ChannelType::MasterVolume, gain, peak)],
            );
            tags.add(Box::new(rva2))?;
        }
        Ok(())
    }

    fn set_replaygain_peak_static(tags: &mut ID3Tags, key: &str, values: &[String]) -> Result<()> {
        // Set RVA2 frame for peak
        // Extract track type from key (e.g., "replaygain_track_peak" -> "track")
        let track_type = if key.contains("track") {
            "track"
        } else if key.contains("album") {
            "album"
        } else {
            // Invalid key - must contain "track" or "album"
            return Err(AudexError::InvalidData(format!(
                "Invalid replaygain peak key: {}. Must contain 'track' or 'album'",
                key
            )));
        };

        if values.is_empty() {
            // Remove the RVA2 frame for this track type
            Self::delete_replaygain_peak_static(tags, key)?;
        } else {
            // Parse peak value
            let peak = values[0]
                .parse::<f32>()
                .map_err(|_| AudexError::InvalidData("Invalid peak value".to_string()))?;

            if !(0.0..2.0).contains(&peak) {
                return Err(AudexError::InvalidData(
                    "Peak must be >= 0 and < 2".to_string(),
                ));
            }

            // Use uppercase identification to match specification (e.g., "TRACK", "ALBUM")
            let frame_key = format!("RVA2:{}", track_type.to_uppercase());

            // Check if frame exists and get its gain value
            let existing_gain = if let Some(frame) = tags.get_frame(&frame_key) {
                if let Some(rva2) = frame.as_any().downcast_ref::<crate::id3::frames::RVA2>() {
                    rva2.get_master().map(|(gain, _)| gain)
                } else {
                    None
                }
            } else {
                None
            };

            // Remove old frame
            tags.remove(&frame_key);

            // Create new RVA2 frame with updated peak
            let gain = existing_gain.unwrap_or(0.0);
            let rva2 = crate::id3::frames::RVA2::new(
                track_type.to_uppercase(),
                vec![(crate::id3::frames::ChannelType::MasterVolume, gain, peak)],
            );
            tags.add(Box::new(rva2))?;
        }
        Ok(())
    }

    fn set_musicbrainz_static(tags: &mut ID3Tags, key: &str, values: &[String]) -> Result<()> {
        // Set TXXX frame with the conventional mixed-case description
        let txxx_desc = Self::musicbrainz_txxx_desc(key);
        Self::set_txxx_frame_static(tags, &txxx_desc, values)
    }

    fn set_website_static(tags: &mut ID3Tags, _key: &str, values: &[String]) -> Result<()> {
        // Set WOAR frame for website
        if values.is_empty() {
            tags.remove("WOAR");
        } else {
            let website_text = values.join("/");
            let frame = crate::id3::frames::TextFrame::single("WOAR".to_string(), website_text);
            tags.add_text_frame(&frame.frame_id, frame.text)?;
        }
        Ok(())
    }

    fn set_txxx_frame_static(
        tags: &mut ID3Tags,
        description: &str,
        values: &[String],
    ) -> Result<()> {
        // Remove existing TXXX frames with this description
        Self::delete_txxx_frame_static(tags, description)?;

        if !values.is_empty() {
            // Use Latin1 for ASCII-safe text, UTF-16 for non-Latin1
            use crate::id3::{frames::TXXX, specs::TextEncoding};
            let all_latin1 = description.chars().all(|c| (c as u32) <= 255)
                && values.iter().all(|v| v.chars().all(|c| (c as u32) <= 255));
            let encoding = if all_latin1 {
                TextEncoding::Latin1
            } else {
                TextEncoding::Utf16
            };
            let txxx_frame = TXXX::new(encoding, description.to_string(), values.to_vec());
            tags.add(Box::new(txxx_frame))?;
        }

        Ok(())
    }

    // Static deleter methods for patterns
    fn delete_performer_static(tags: &mut ID3Tags, key: &str) -> Result<()> {
        // Extract the role from the key (e.g., "performer:guitar" -> "guitar")
        let role = key.split(':').nth(1).unwrap_or("");

        if role.is_empty() {
            // No specific role — remove entire TMCL frame
            tags.remove("TMCL");
            return Ok(());
        }

        // Read existing entries and keep only those that don't match the role
        let existing_entries = Self::read_tmcl_entries(tags);
        let remaining: Vec<(String, String)> = existing_entries
            .into_iter()
            .filter(|(existing_role, _)| existing_role != role)
            .collect();

        // Remove the old frame, then write back remaining entries if any
        tags.remove("TMCL");
        if !remaining.is_empty() {
            let text_entries: Vec<String> = remaining
                .into_iter()
                .flat_map(|(role, performer)| vec![role, performer])
                .collect();
            tags.add_text_frame("TMCL", text_entries)?;
        }
        Ok(())
    }

    fn delete_replaygain_gain_static(tags: &mut ID3Tags, key: &str) -> Result<()> {
        let track_type = if key.contains("track") {
            "TRACK"
        } else if key.contains("album") {
            "ALBUM"
        } else {
            "TRACK"
        };

        let frame_key = format!("RVA2:{}", track_type);

        // Preserve the peak value if one exists, since RVA2 stores both
        // gain and peak in a single frame
        let existing_peak = if let Some(frame) = tags.get_frame(&frame_key) {
            if let Some(rva2) = frame.as_any().downcast_ref::<crate::id3::frames::RVA2>() {
                rva2.get_master().map(|(_, peak)| peak)
            } else {
                None
            }
        } else {
            None
        };

        tags.remove(&frame_key);

        // Reconstruct the frame with zero gain but the original peak
        if let Some(peak) = existing_peak {
            if peak != 0.0 {
                let rva2 = crate::id3::frames::RVA2::new(
                    track_type.to_string(),
                    vec![(crate::id3::frames::ChannelType::MasterVolume, 0.0, peak)],
                );
                tags.add(Box::new(rva2))?;
            }
        }
        Ok(())
    }

    fn delete_replaygain_peak_static(tags: &mut ID3Tags, key: &str) -> Result<()> {
        let track_type = if key.contains("track") {
            "TRACK"
        } else if key.contains("album") {
            "ALBUM"
        } else {
            "TRACK"
        };

        let frame_key = format!("RVA2:{}", track_type);

        // Preserve the gain value if one exists, since RVA2 stores both
        // gain and peak in a single frame
        let existing_gain = if let Some(frame) = tags.get_frame(&frame_key) {
            if let Some(rva2) = frame.as_any().downcast_ref::<crate::id3::frames::RVA2>() {
                rva2.get_master().map(|(gain, _)| gain)
            } else {
                None
            }
        } else {
            None
        };

        tags.remove(&frame_key);

        // Reconstruct the frame with the original gain but zero peak
        if let Some(gain) = existing_gain {
            if gain != 0.0 {
                let rva2 = crate::id3::frames::RVA2::new(
                    track_type.to_string(),
                    vec![(crate::id3::frames::ChannelType::MasterVolume, gain, 0.0)],
                );
                tags.add(Box::new(rva2))?;
            }
        }
        Ok(())
    }

    fn delete_musicbrainz_static(tags: &mut ID3Tags, key: &str) -> Result<()> {
        // Delete TXXX frame using the conventional mixed-case description
        let txxx_desc = Self::musicbrainz_txxx_desc(key);
        Self::delete_txxx_frame_static(tags, &txxx_desc)
    }

    fn delete_website_static(tags: &mut ID3Tags, _key: &str) -> Result<()> {
        // Remove WOAR frame for website
        tags.remove("WOAR");
        Ok(())
    }

    fn delete_txxx_frame_static(tags: &mut ID3Tags, description: &str) -> Result<()> {
        // Use pattern-based deletion for TXXX frames
        // The pattern should match how hash keys are generated in the TXXX frame's hash_key() method
        let pattern = format!("TXXX:{}", description);
        tags.delall(&pattern);
        Ok(())
    }

    pub fn debug_tags(&self) -> &crate::id3::tags::ID3Tags {
        &self.id3
    }

    pub fn tags(&self) -> Option<&ID3Tags> {
        Some(&self.id3)
    }

    /// Save ID3 tags to the stored filename using efficient in-place modification
    pub fn save_to_file(&mut self) -> Result<()> {
        if let Some(filename) = &self.filename {
            // Use the new in-place ID3 modification - performs efficient byte manipulation
            // instead of rebuilding the entire file
            self.id3.save(filename, 1, 4, None, None)
        } else {
            Err(AudexError::InvalidData(
                "No filename stored for saving".to_string(),
            ))
        }
    }

    /// Load EasyID3 tags from file asynchronously.
    ///
    /// # Arguments
    /// * `path` - Path to the audio file
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded tags
    /// * `Err(AudexError)` - Error occurred during loading
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        use crate::id3::tags::ID3Tags;

        let path_str = path.as_ref().to_string_lossy().to_string();
        info_event!(path = %path_str, "loading EasyID3 tags (async)");
        let id3_file = crate::id3::ID3::load_from_file_async(&path).await?;
        let id3_tags = id3_file.tags().cloned().unwrap_or_else(ID3Tags::new);

        let mut easy = Self {
            id3: id3_tags,
            filename: Some(path_str),
            trait_cache: HashMap::new(),
        };
        easy.refresh_trait_cache();
        Ok(easy)
    }

    /// Save EasyID3 tags to file asynchronously.
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        let filename = self
            .filename
            .clone()
            .ok_or_else(|| AudexError::InvalidData("No filename set".to_string()))?;

        let config = crate::id3::tags::ID3SaveConfig {
            v2_version: 4,
            v2_minor: 0,
            v23_sep: "/".to_string(),
            v23_separator: b'/',
            padding: None,
            merge_frames: true,
            preserve_unknown: true,
            compress_frames: false,
            write_v1: crate::id3::file::ID3v1SaveOptions::CREATE,
            unsync: false,
            extended_header: false,
            convert_v24_frames: true,
        };

        self.id3.save_to_file_async(&filename, &config).await
    }

    /// Clear all tags asynchronously.
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        self.id3.dict.clear();
        self.id3.frames_by_id.clear();
        self.save_async().await
    }

    /// Delete the file asynchronously.
    #[cfg(feature = "async")]
    pub async fn delete_async(&mut self) -> Result<()> {
        if let Some(filename) = &self.filename {
            tokio::fs::remove_file(filename).await?;
            self.filename = None;
        }
        Ok(())
    }
}

impl Default for EasyID3 {
    fn default() -> Self {
        Self::new()
    }
}

// Registration functions for standard keys
fn register_standard_keys(registry: &mut HashMap<String, GetterFn>) {
    // Text frame mappings
    register_text_key(registry, "album", "TALB");
    register_tpe_key(registry, "albumartist", "TPE2"); // Use specialized TPE handler
    register_text_key(registry, "albumartistsort", "TSO2");
    register_text_key(registry, "albumsort", "TSOA");
    register_text_key(registry, "arranger", "TPE4");
    register_tpe_key(registry, "artist", "TPE1"); // Use specialized TPE handler
    register_text_key(registry, "artistsort", "TSOP");
    register_text_key(registry, "author", "TOLY");
    register_text_key(registry, "bpm", "TBPM");
    register_text_key(registry, "composer", "TCOM");
    register_text_key(registry, "composersort", "TSOC");
    register_text_key(registry, "conductor", "TPE3");
    register_text_key(registry, "copyright", "TCOP");
    register_date_key(registry);
    register_text_key(registry, "discnumber", "TPOS");
    register_text_key(registry, "discsubtitle", "TSST");
    register_text_key(registry, "encodedby", "TENC");
    register_text_key(registry, "encodersettings", "TSSE");
    register_text_key(registry, "fileowner", "TOWN");
    register_genre_key(registry);
    register_text_key(registry, "grouping", "TIT1");
    register_text_key(registry, "isrc", "TSRC");
    register_text_key(registry, "language", "TLAN");
    register_text_key(registry, "length", "TLEN");
    register_text_key(registry, "lyricist", "TEXT");
    register_text_key(registry, "media", "TMED");
    register_text_key(registry, "mood", "TMOO");
    register_text_key(registry, "organization", "TPUB");
    register_text_key(registry, "originalalbum", "TOAL");
    register_text_key(registry, "originalartist", "TOPE");
    register_text_key(registry, "originaldate", "TDOR");
    register_text_key(registry, "title", "TIT2");
    register_text_key(registry, "titlesort", "TSOT");
    register_text_key(registry, "compilation", "TCMP");
    register_text_key(registry, "tracknumber", "TRCK");
    register_text_key(registry, "version", "TIT3");
    register_comment_key(registry);

    // Plain "website" key returns all WOAR frame URLs
    let website_getter: GetterFn = Box::new(|tags, _key| {
        if let Some(frames) = tags.get_frames("WOAR") {
            let mut urls = Vec::new();
            for frame in frames {
                if let Some(text_values) = frame.text_values() {
                    urls.extend(text_values);
                }
            }
            if !urls.is_empty() {
                return Ok(urls);
            }
        }
        Ok(vec![])
    });
    registry.insert("website".to_string(), website_getter);

    // TXXX mappings for MusicBrainz and other keys
    // Descriptions must match the mixed-case conventions used by MusicBrainz Picard and other taggers
    register_txxx_key(registry, "acoustid_fingerprint", "Acoustid Fingerprint");
    register_txxx_key(registry, "acoustid_id", "Acoustid Id");
    register_txxx_key(
        registry,
        "musicbrainz_albumartistid",
        "MusicBrainz Album Artist Id",
    );
    register_txxx_key(registry, "musicbrainz_albumid", "MusicBrainz Album Id");
    register_txxx_key(
        registry,
        "musicbrainz_albumstatus",
        "MusicBrainz Album Status",
    );
    register_txxx_key(registry, "musicbrainz_albumtype", "MusicBrainz Album Type");
    register_txxx_key(registry, "musicbrainz_artistid", "MusicBrainz Artist Id");
    register_txxx_key(registry, "musicbrainz_discid", "MusicBrainz Disc Id");
    register_txxx_key(
        registry,
        "musicbrainz_releasegroupid",
        "MusicBrainz Release Group Id",
    );
    register_txxx_key(
        registry,
        "musicbrainz_releasetrackid",
        "MusicBrainz Release Track Id",
    );
    // musicbrainz_trackid uses UFID:http://musicbrainz.org frame, not TXXX
    let ufid_getter: GetterFn = Box::new(|tags, _key| {
        let frames = tags.getall("UFID:http://musicbrainz.org");
        if let Some(frame) = frames.first() {
            if let Some(ufid) = frame.as_any().downcast_ref::<crate::id3::frames::UFID>() {
                if let Ok(s) = String::from_utf8(ufid.data.clone()) {
                    return Ok(vec![s]);
                }
            }
        }
        Ok(vec![])
    });
    registry.insert("musicbrainz_trackid".to_string(), ufid_getter);
    register_txxx_key(registry, "musicbrainz_trmid", "MusicBrainz TRM Id");
    register_txxx_key(registry, "musicbrainz_workid", "MusicBrainz Work Id");
    register_txxx_key(registry, "musicip_fingerprint", "MusicMagic Fingerprint");
    register_txxx_key(registry, "musicip_puid", "MusicIP PUID");
    register_txxx_key(
        registry,
        "releasecountry",
        "MusicBrainz Album Release Country",
    );
    register_txxx_key(registry, "asin", "ASIN");
    register_txxx_key(registry, "barcode", "BARCODE");
    register_txxx_key(registry, "catalognumber", "CATALOGNUMBER");
    register_txxx_key(registry, "performer", "PERFORMER");

    // ReplayGain handlers (pattern-based keys)
    registry.insert(
        "replaygain_*_gain".to_string(),
        Box::new(|tags, key| {
            // Extract track type from key (e.g., "track" or "album")
            if let Some(track_type) = key.strip_prefix("replaygain_") {
                if let Some(track_type) = track_type.strip_suffix("_gain") {
                    // Find RVA2 frame with matching identification (uppercase per specification)
                    let frame_key = format!("RVA2:{}", track_type.to_uppercase());
                    if let Some(frame) = tags.get_frame(&frame_key) {
                        if let Some(rva2) =
                            frame.as_any().downcast_ref::<crate::id3::frames::RVA2>()
                        {
                            if let Some((gain, _)) = rva2.get_master() {
                                return Ok(vec![format!("{:+.6} dB", gain)]);
                            }
                        }
                    }
                }
            }
            Err(AudexError::InvalidData(format!("Key not found: {}", key)))
        }),
    );

    registry.insert(
        "replaygain_*_peak".to_string(),
        Box::new(|tags, key| {
            // Extract track type from key (e.g., "track" or "album")
            if let Some(track_type) = key.strip_prefix("replaygain_") {
                if let Some(track_type) = track_type.strip_suffix("_peak") {
                    // Find RVA2 frame with matching identification (uppercase per specification)
                    let frame_key = format!("RVA2:{}", track_type.to_uppercase());
                    if let Some(frame) = tags.get_frame(&frame_key) {
                        if let Some(rva2) =
                            frame.as_any().downcast_ref::<crate::id3::frames::RVA2>()
                        {
                            if let Some((_, peak)) = rva2.get_master() {
                                return Ok(vec![format!("{:.6}", peak)]);
                            }
                        }
                    }
                }
            }
            Err(AudexError::InvalidData(format!("Key not found: {}", key)))
        }),
    );
}

fn register_standard_setters(registry: &mut HashMap<String, SetterFn>) {
    // Same mappings as getters but for setters
    register_text_setter(registry, "album", "TALB");
    register_tpe_setter(registry, "albumartist", "TPE2"); // Use specialized TPE setter
    register_text_setter(registry, "albumartistsort", "TSO2");
    register_text_setter(registry, "albumsort", "TSOA");
    register_text_setter(registry, "arranger", "TPE4");
    register_tpe_setter(registry, "artist", "TPE1"); // Use specialized TPE setter
    register_text_setter(registry, "artistsort", "TSOP");
    register_text_setter(registry, "author", "TOLY");
    register_text_setter(registry, "bpm", "TBPM");
    register_text_setter(registry, "composer", "TCOM");
    register_text_setter(registry, "composersort", "TSOC");
    register_text_setter(registry, "conductor", "TPE3");
    register_text_setter(registry, "copyright", "TCOP");
    register_date_setter(registry);
    register_text_setter(registry, "discnumber", "TPOS");
    register_text_setter(registry, "discsubtitle", "TSST");
    register_text_setter(registry, "encodedby", "TENC");
    register_text_setter(registry, "encodersettings", "TSSE");
    register_text_setter(registry, "fileowner", "TOWN");
    register_genre_setter(registry);
    register_text_setter(registry, "grouping", "TIT1");
    register_text_setter(registry, "isrc", "TSRC");
    register_text_setter(registry, "language", "TLAN");
    register_text_setter(registry, "length", "TLEN");
    register_text_setter(registry, "lyricist", "TEXT");
    register_text_setter(registry, "media", "TMED");
    register_text_setter(registry, "mood", "TMOO");
    register_text_setter(registry, "organization", "TPUB");
    register_text_setter(registry, "originalalbum", "TOAL");
    register_text_setter(registry, "originalartist", "TOPE");
    register_text_setter(registry, "originaldate", "TDOR");
    register_text_setter(registry, "title", "TIT2");
    register_text_setter(registry, "titlesort", "TSOT");
    register_text_setter(registry, "compilation", "TCMP");
    register_text_setter(registry, "tracknumber", "TRCK");
    register_text_setter(registry, "version", "TIT3");
    register_comment_setter(registry);

    // TXXX setters - descriptions must match the getter descriptions
    register_txxx_setter(registry, "acoustid_fingerprint", "Acoustid Fingerprint");
    register_txxx_setter(registry, "acoustid_id", "Acoustid Id");
    register_txxx_setter(
        registry,
        "musicbrainz_albumartistid",
        "MusicBrainz Album Artist Id",
    );
    register_txxx_setter(registry, "musicbrainz_albumid", "MusicBrainz Album Id");
    register_txxx_setter(
        registry,
        "musicbrainz_albumstatus",
        "MusicBrainz Album Status",
    );
    register_txxx_setter(registry, "musicbrainz_albumtype", "MusicBrainz Album Type");
    register_txxx_setter(registry, "musicbrainz_artistid", "MusicBrainz Artist Id");
    register_txxx_setter(registry, "musicbrainz_discid", "MusicBrainz Disc Id");
    register_txxx_setter(
        registry,
        "musicbrainz_releasegroupid",
        "MusicBrainz Release Group Id",
    );
    register_txxx_setter(
        registry,
        "musicbrainz_releasetrackid",
        "MusicBrainz Release Track Id",
    );
    // musicbrainz_trackid uses UFID:http://musicbrainz.org frame
    let ufid_setter: SetterFn = Box::new(|tags, _key, values| {
        if values.len() != 1 {
            return Err(crate::AudexError::InvalidData(
                "only one track ID may be set per song".to_string(),
            ));
        }
        // Remove existing UFID frame for musicbrainz
        tags.delall("UFID:http://musicbrainz.org");
        let ufid = crate::id3::frames::UFID::new(
            "http://musicbrainz.org".to_string(),
            values[0].as_bytes().to_vec(),
        );
        tags.add(Box::new(ufid))?;
        Ok(())
    });
    registry.insert("musicbrainz_trackid".to_string(), ufid_setter);
    register_txxx_setter(registry, "musicbrainz_trmid", "MusicBrainz TRM Id");
    register_txxx_setter(registry, "musicbrainz_workid", "MusicBrainz Work Id");
    register_txxx_setter(registry, "musicip_fingerprint", "MusicMagic Fingerprint");
    register_txxx_setter(registry, "musicip_puid", "MusicIP PUID");
    register_txxx_setter(
        registry,
        "releasecountry",
        "MusicBrainz Album Release Country",
    );
    register_txxx_setter(registry, "asin", "ASIN");
    register_txxx_setter(registry, "barcode", "BARCODE");
    register_txxx_setter(registry, "catalognumber", "CATALOGNUMBER");
    register_txxx_setter(registry, "performer", "PERFORMER");

    // ReplayGain setters
    registry.insert(
        "replaygain_*_gain".to_string(),
        Box::new(EasyID3::set_replaygain_gain_static),
    );

    registry.insert(
        "replaygain_*_peak".to_string(),
        Box::new(EasyID3::set_replaygain_peak_static),
    );
}

fn register_standard_deleters(registry: &mut HashMap<String, DeleterFn>) {
    // Register individual deleter functions for each key
    register_text_deleter(registry, "album", "TALB");
    register_text_deleter(registry, "albumartist", "TPE2");
    register_text_deleter(registry, "artist", "TPE1");
    register_text_deleter(registry, "title", "TIT2");
    register_text_deleter(registry, "genre", "TCON");
    register_date_deleter(registry);
    register_text_deleter(registry, "originaldate", "TDOR");
    register_text_deleter(registry, "tracknumber", "TRCK");
    register_text_deleter(registry, "discnumber", "TPOS");
    // Add other common keys
    register_text_deleter(registry, "composer", "TCOM");
    register_text_deleter(registry, "conductor", "TPE3");
    register_text_deleter(registry, "copyright", "TCOP");
    register_text_deleter(registry, "encodedby", "TENC");
    register_text_deleter(registry, "grouping", "TIT1");
    register_text_deleter(registry, "lyricist", "TEXT");
    register_text_deleter(registry, "mood", "TMOO");
    register_text_deleter(registry, "organization", "TPUB");
    register_text_deleter(registry, "compilation", "TCMP");
    register_text_deleter(registry, "originalalbum", "TOAL");
    register_text_deleter(registry, "originalartist", "TOPE");
    register_comment_deleter(registry);

    // musicbrainz_trackid uses UFID frame
    let ufid_deleter: DeleterFn = Box::new(|tags, _key| {
        tags.delall("UFID:http://musicbrainz.org");
        Ok(())
    });
    registry.insert("musicbrainz_trackid".to_string(), ufid_deleter);
}

fn register_text_deleter(registry: &mut HashMap<String, DeleterFn>, key: &str, frame_id: &str) {
    let frame_id = frame_id.to_string();
    let deleter: DeleterFn = Box::new(move |tags, _key| {
        tags.remove(&frame_id);
        Ok(())
    });
    registry.insert(key.to_string(), deleter);
}

// Helper functions for registration
fn register_text_key(registry: &mut HashMap<String, GetterFn>, key: &str, frame_id: &str) {
    let frame_id = frame_id.to_string();
    let getter: GetterFn = Box::new(move |tags, _key| {
        // Use the new get_text_values method to properly handle multiple values
        if let Some(text_values) = tags.get_text_values(&frame_id) {
            Ok(text_values)
        } else {
            Ok(vec![])
        }
    });
    registry.insert(key.to_string(), getter);
}

// Specialized TPE key registration to work around TPE frame loading issues
fn register_tpe_key(registry: &mut HashMap<String, GetterFn>, key: &str, frame_id: &str) {
    let frame_id = frame_id.to_string();
    let getter: GetterFn = Box::new(move |tags, _key| {
        // Try multiple approaches to get TPE frame data

        // Approach 1: Try the normal text values method
        if let Some(text_values) = tags.get_text_values(&frame_id) {
            if !text_values.is_empty() {
                return Ok(text_values);
            }
        }

        // Approach 2: Try direct frame access
        if let Some(frames) = tags.get_frames(&frame_id) {
            for frame in frames {
                if let Some(text_values) = frame.text_values() {
                    if !text_values.is_empty() {
                        return Ok(text_values);
                    }
                }
                // Try extracting from frame description as fallback
                let desc = frame.description();
                if let Some(colon_pos) = desc.find(": ") {
                    let value = desc[colon_pos + 2..].trim().to_string();
                    if !value.is_empty() {
                        return Ok(vec![value]);
                    }
                }
            }
        }

        // Approach 3: Try get_text as fallback
        if let Some(text) = tags.get_text(&frame_id) {
            if !text.is_empty() {
                return Ok(vec![text]);
            }
        }

        Ok(vec![])
    });
    registry.insert(key.to_string(), getter);
}

fn register_text_setter(registry: &mut HashMap<String, SetterFn>, key: &str, frame_id: &str) {
    let frame_id = frame_id.to_string();
    let setter: SetterFn = Box::new(move |tags, _key, values| {
        if values.is_empty() {
            tags.remove(&frame_id);
        } else {
            // Remove existing frames first
            tags.remove(&frame_id);
            // Add all values as separate text values (ID3v2.4 supports multiple values)
            tags.add_text_frame(&frame_id, values.to_vec())?;
        }
        Ok(())
    });
    registry.insert(key.to_string(), setter);
}

// Specialized TPE setter to work around TPE frame saving issues
fn register_tpe_setter(registry: &mut HashMap<String, SetterFn>, key: &str, frame_id: &str) {
    let frame_id = frame_id.to_string();
    let setter: SetterFn = Box::new(move |tags, _key, values| {
        if values.is_empty() {
            tags.remove(&frame_id);
        } else {
            // Remove existing frames first
            tags.remove(&frame_id);

            // Try multiple approaches to set the TPE frame

            // Approach 1: Use add_text_frame (standard method)
            match tags.add_text_frame(&frame_id, values.to_vec()) {
                Ok(()) => {
                    // Verify the frame was added, fallback to set_text if not
                    if tags.get_frames(&frame_id).is_none() {
                        // Try approach 2
                        let joined_value = values.join("/");
                        tags.set_text(&frame_id, joined_value)?;
                    }
                    return Ok(());
                }
                Err(_) => {
                    // Try approach 2
                    let joined_value = values.join("/");
                    tags.set_text(&frame_id, joined_value)?;
                }
            }
        }

        Ok(())
    });
    registry.insert(key.to_string(), setter);
}

fn register_txxx_key(registry: &mut HashMap<String, GetterFn>, key: &str, txxx_desc: &str) {
    let desc = txxx_desc.to_string();
    let getter: GetterFn = Box::new(move |tags, _key| {
        // Filter TXXX frames by matching description field
        if let Some(frames) = tags.get_frames("TXXX") {
            for frame in frames {
                if let Some(txxx) = frame.as_any().downcast_ref::<crate::id3::frames::TXXX>() {
                    if txxx.description.eq_ignore_ascii_case(&desc) {
                        return Ok(txxx.text.clone());
                    }
                }
            }
        }
        Ok(vec![])
    });
    registry.insert(key.to_string(), getter);
}

fn register_txxx_setter(registry: &mut HashMap<String, SetterFn>, key: &str, txxx_desc: &str) {
    let desc = txxx_desc.to_string();
    let setter: SetterFn = Box::new(move |tags, _key, values| {
        // Remove only TXXX frames matching this specific description
        let pattern = format!("TXXX:{}", desc);
        tags.delall(&pattern);

        // Add new TXXX frame with correct description if values provided
        if !values.is_empty() {
            use crate::id3::{frames::TXXX, specs::TextEncoding};
            let all_latin1 = desc.chars().all(|c| (c as u32) <= 255)
                && values.iter().all(|v| v.chars().all(|c| (c as u32) <= 255));
            let encoding = if all_latin1 {
                TextEncoding::Latin1
            } else {
                TextEncoding::Utf16
            };
            let txxx_frame = TXXX::new(encoding, desc.clone(), values.to_vec());
            tags.add(Box::new(txxx_frame))?;
        }
        Ok(())
    });
    registry.insert(key.to_string(), setter);
}

// Specialized key registration functions
fn register_genre_key(registry: &mut HashMap<String, GetterFn>) {
    let getter: GetterFn = Box::new(|tags, _key| {
        // Handle TCON frame with genre list support
        if let Some(text_values) = tags.get_text_values("TCON") {
            // Process each genre value for numeric genres like "(13)" or mixed formats
            let mut all_genres = Vec::new();
            for text in text_values {
                all_genres.extend(parse_genre_text(&text));
            }
            Ok(all_genres)
        } else {
            Ok(vec![])
        }
    });
    registry.insert("genre".to_string(), getter);
}

fn register_genre_setter(registry: &mut HashMap<String, SetterFn>) {
    let setter: SetterFn = Box::new(|tags, _key, values| {
        if values.is_empty() {
            tags.remove("TCON");
        } else {
            // Join multiple genres with null separators for ID3v2.4
            let genre_text = values.join("\0");
            tags.set_text("TCON", genre_text)?;
        }
        Ok(())
    });
    registry.insert("genre".to_string(), setter);
}

// Dedicated COMM frame handler for the "comment" key.
// Reads the first COMM frame text, preferring empty-description frames.
fn register_comment_key(registry: &mut HashMap<String, GetterFn>) {
    let getter: GetterFn = Box::new(|tags, _key| {
        if let Some(frames) = tags.get_frames("COMM") {
            for frame in &frames {
                if let Some(comm) = frame.as_any().downcast_ref::<crate::id3::frames::COMM>() {
                    if comm.description.is_empty() || comm.description == "ID3v1 Comment" {
                        return Ok(vec![comm.text.clone()]);
                    }
                }
            }
            // Fall back to the first COMM frame if none had an empty description.
            if let Some(frame) = frames.first() {
                if let Some(comm) = frame.as_any().downcast_ref::<crate::id3::frames::COMM>() {
                    return Ok(vec![comm.text.clone()]);
                }
            }
        }
        Ok(vec![])
    });
    registry.insert("comment".to_string(), getter);
}

fn register_comment_setter(registry: &mut HashMap<String, SetterFn>) {
    let setter: SetterFn = Box::new(|tags, _key, values| {
        // Remove all existing COMM frames first.
        tags.remove("COMM");

        if !values.is_empty() {
            use crate::id3::{frames::COMM, specs::TextEncoding};
            let text = values[0].clone();
            let all_latin1 = text.chars().all(|c| (c as u32) <= 255);
            let encoding = if all_latin1 {
                TextEncoding::Latin1
            } else {
                TextEncoding::Utf16
            };
            let comm = COMM::new(encoding, *b"eng", String::new(), text);
            tags.add(Box::new(comm))?;
        }
        Ok(())
    });
    registry.insert("comment".to_string(), setter);
}

fn register_comment_deleter(registry: &mut HashMap<String, DeleterFn>) {
    let deleter: DeleterFn = Box::new(|tags, _key| {
        tags.remove("COMM");
        Ok(())
    });
    registry.insert("comment".to_string(), deleter);
}

fn register_date_key(registry: &mut HashMap<String, GetterFn>) {
    let getter: GetterFn = Box::new(|tags, _key| {
        // Try TDRC first (ID3v2.4), then fall back to other date frames
        if let Some(text) = tags.get_text("TDRC") {
            Ok(vec![text])
        } else if let Some(text) = tags.get_text("TYER") {
            // ID3v2.3 year frame
            Ok(vec![text])
        } else if let Some(text) = tags.get_text("TDAT") {
            // ID3v2.3 date frame (DDMM)
            Ok(vec![text])
        } else {
            Ok(vec![])
        }
    });
    registry.insert("date".to_string(), getter);
}

fn register_date_setter(registry: &mut HashMap<String, SetterFn>) {
    let setter: SetterFn = Box::new(|tags, _key, values| {
        if values.is_empty() {
            // Remove all date-related frames
            tags.remove("TDRC");
            tags.remove("TYER");
            tags.remove("TDAT");
        } else {
            // Set TDRC frame with timestamp format
            let date_text = &values[0];
            tags.set_text("TDRC", date_text.clone())?;
            // Remove old ID3v2.3 frames if present
            tags.remove("TYER");
            tags.remove("TDAT");
        }
        Ok(())
    });
    registry.insert("date".to_string(), setter);
}

fn register_date_deleter(registry: &mut HashMap<String, DeleterFn>) {
    let deleter: DeleterFn = Box::new(|tags, _key| {
        // Remove all date-related frames
        tags.remove("TDRC");
        tags.remove("TYER");
        tags.remove("TDAT");
        tags.remove("TIME");
        Ok(())
    });
    registry.insert("date".to_string(), deleter);
}

// Helper function to parse genre text
fn parse_genre_text(text: &str) -> Vec<String> {
    let mut genres = Vec::new();

    // Handle numeric genres in parentheses like "(13)" or "(13)Rock"
    if text.starts_with('(') {
        let mut chars = text.chars().peekable();
        let mut current = String::new();
        let mut in_parens = false;

        while let Some(c) = chars.next() {
            match c {
                '(' => {
                    in_parens = true;
                    current.clear();
                }
                ')' => {
                    if in_parens {
                        // Try to convert numeric genre to name; fall back to the
                        // raw number so unknown IDs are not silently dropped.
                        if let Ok(genre_num) = current.parse::<u8>() {
                            if let Some(genre_name) = crate::constants::get_genre(genre_num) {
                                genres.push(genre_name.to_string());
                            } else {
                                genres.push(genre_num.to_string());
                            }
                        } else if !current.is_empty() {
                            // Not a valid u8; preserve the raw content as a genre string
                            genres.push(current.clone());
                        }
                        in_parens = false;
                        current.clear();
                    }
                }
                _ => {
                    if in_parens {
                        current.push(c);
                    } else if !c.is_whitespace() {
                        // Start of text genre after numeric
                        current.push(c);
                        // Collect rest of the string
                        let rest: String = chars.collect();
                        current.push_str(&rest);
                        genres.push(current.trim().to_string());
                        break;
                    }
                }
            }
        }
    } else {
        // Plain text genre or multiple genres separated by nulls
        for genre in text.split('\0') {
            let genre = genre.trim();
            if !genre.is_empty() {
                genres.push(genre.to_string());
            }
        }
    }

    if genres.is_empty() && !text.is_empty() {
        // Fallback: return the text as-is
        genres.push(text.to_string());
    }

    genres
}

// ID3v1 genre table (partial - just the most common ones)
// Genre lookup now uses the full constants::GENRES table (192 entries)
// via crate::constants::get_genre() instead of a partial inline table.

// Conversion implementations
impl From<EasyID3Error> for AudexError {
    fn from(err: EasyID3Error) -> Self {
        AudexError::InvalidData(err.to_string())
    }
}

impl crate::tags::Tags for EasyID3 {
    fn get(&self, key: &str) -> Option<&[String]> {
        self.trait_cache.get(&key.to_lowercase()).map(Vec::as_slice)
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        let _ = EasyID3::set(self, key, &values);
    }

    fn remove(&mut self, key: &str) {
        let _ = EasyID3::remove(self, key);
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.trait_cache.keys().cloned().collect();
        keys.sort();
        keys
    }

    fn pprint(&self) -> String {
        let mut result = String::new();
        for key in crate::tags::Tags::keys(self) {
            if let Some(values) = crate::tags::Tags::get(self, &key) {
                result.push_str(&format!("{}: {:?}\n", key, values));
            }
        }
        result
    }
}

impl FileType for EasyID3 {
    type Tags = ID3Tags;
    type Info = crate::id3::file::EmptyStreamInfo;

    fn format_id() -> &'static str {
        "EasyID3"
    }

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(skip_all, fields(format = "EasyID3"))
    )]
    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();
        info_event!(path = %path_str, "loading EasyID3 tags");
        let id3_file = crate::id3::ID3::load_from_file(&path)?;
        let id3_tags = id3_file.tags().cloned().unwrap_or_else(ID3Tags::new);

        let mut easy = Self {
            id3: id3_tags,
            filename: Some(path_str),
            trait_cache: HashMap::new(),
        };
        easy.refresh_trait_cache();
        Ok(easy)
    }

    fn save(&mut self) -> Result<()> {
        // EasyID3 save should delegate to the underlying ID3Tags save
        // Use the new save_to_file method that uses stored filename
        self.save_to_file()
    }

    fn clear(&mut self) -> Result<()> {
        // Clear all frames
        self.id3.dict.clear();
        self.id3.frames_by_id.clear();
        self.save()
    }

    /// ID3 tags are always present in this format.
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
    /// use audex::easyid3::EasyID3;
    /// use audex::FileType;
    ///
    /// let mut easyid3 = EasyID3::load("file.mp3")?;
    /// // Tags are always present, so add_tags() will fail
    /// assert!(easyid3.add_tags().is_err());
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        // EasyID3 tags are always present, cannot add what already exists
        Err(AudexError::InvalidOperation(
            "ID3 tags already exist".to_string(),
        ))
    }

    fn tags(&self) -> Option<&Self::Tags> {
        Some(&self.id3)
    }

    fn tags_mut(&mut self) -> Option<&mut Self::Tags> {
        Some(&mut self.id3)
    }

    fn info(&self) -> &Self::Info {
        // Return a static empty stream info
        static EMPTY_INFO: crate::id3::file::EmptyStreamInfo = crate::id3::file::EmptyStreamInfo;
        &EMPTY_INFO
    }

    fn score(filename: &str, header: &[u8]) -> i32 {
        crate::id3::ID3::score(filename, header)
    }

    fn mime_types() -> &'static [&'static str] {
        crate::id3::ID3::mime_types()
    }
}
