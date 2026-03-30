//! Simplified MP4/M4A tag interface with key-value access
//!
//! This module provides `EasyMP4Tags`, a high-level wrapper around MP4 tags that
//! offers a simple key-value interface for common tag operations. Instead
//! of working with iTunes atom names and complex MP4 structures, you can use familiar
//! field names like "title", "artist", and "album".
//!
//! # Overview
//!
//! `EasyMP4Tags` simplifies MP4/M4A tag manipulation by:
//! - Using human-readable key names instead of iTunes atom codes
//! - Providing key-value get/set operations
//! - Automatically handling value type conversion
//! - Supporting common metadata fields out of the box
//! - Managing freeform tags with intuitive names
//! - Handling corrupted atom names (common in older files)
//!
//! ## When to Use EasyMP4Tags
//!
//! - **Use EasyMP4Tags** when you need simple tag operations with standard fields
//! - **Use raw MP4Tags** when you need access to cover art, chapters, custom atoms,
//!   or precise control over tag structure
//!
//! # Key Mapping
//!
//! EasyMP4Tags maps user-friendly key names to iTunes atom codes:
//!
//! - `title` → `©nam` (Name/Title)
//! - `artist` → `©ART` (Artist)
//! - `album` → `©alb` (Album)
//! - `albumartist` → `aART` (Album Artist)
//! - `date` → `©day` (Year/Date)
//! - `genre` → `©gen` (Genre)
//! - `composer` → `©wrt` (Writer/Composer)
//! - `tracknumber` → `trkn` (Track Number)
//! - `discnumber` → `disk` (Disc Number)
//! - And many more...
//!
//! # Basic Usage
//!
//! ## Reading Tags
//!
//! ```no_run
//! use audex::FileType;
//! use audex::mp4::MP4;
//! use audex::easymp4::EasyMP4Tags;
//!
//! // Load M4A file
//! let audio = MP4::load("song.m4a").unwrap();
//!
//! if let Some(tags) = audio.tags {
//!     let easy_tags = EasyMP4Tags::new(tags);
//!
//!     // Get individual fields
//!     if let Ok(Some(title)) = easy_tags.get("title") {
//!         println!("Title: {}", title.join(", "));
//!     }
//!
//!     // List all available keys
//!     for key in easy_tags.keys() {
//!         println!("Found key: {}", key);
//!     }
//! }
//! ```
//!
//! ## Writing Tags
//!
//! ```no_run
//! use audex::FileType;
//! use audex::mp4::MP4;
//! use audex::easymp4::EasyMP4Tags;
//!
//! let mut audio = MP4::load("song.m4a").unwrap();
//! let tags = audio.tags.unwrap_or_else(|| audex::mp4::MP4Tags::new());
//! let mut easy_tags = EasyMP4Tags::new(tags);
//!
//! // Set single values using Vec instead of slice reference
//! easy_tags.set("title", vec!["My Song".to_string()]).unwrap();
//! easy_tags.set("artist", vec!["My Band".to_string()]).unwrap();
//! easy_tags.set("album", vec!["My Album".to_string()]).unwrap();
//! easy_tags.set("date", vec!["2024".to_string()]).unwrap();
//!
//! // Set track and disc numbers
//! easy_tags.set("tracknumber", vec!["5/12".to_string()]).unwrap();
//! easy_tags.set("discnumber", vec!["1/2".to_string()]).unwrap();
//!
//! // Set BPM
//! easy_tags.set("bpm", vec!["120".to_string()]).unwrap();
//!
//! // Access inner tags and save
//! audio.tags = Some(easy_tags.tags);
//! audio.save().unwrap();
//! ```
//!
//! ## Working with Freeform Tags
//!
//! ```no_run
//! use audex::easymp4::EasyMP4Tags;
//! use audex::mp4::MP4Tags;
//!
//! let tags = MP4Tags::new();
//! let mut easy_tags = EasyMP4Tags::new(tags);
//!
//! // Set MusicBrainz IDs using Vec instead of slice reference
//! easy_tags.set("musicbrainz_trackid", vec![
//!     "b3b4d3c1-1234-5678-9abc-def012345678".to_string()
//! ]).unwrap();
//!
//! easy_tags.set("musicbrainz_artistid", vec![
//!     "a1b2c3d4-5678-90ab-cdef-012345678901".to_string()
//! ]).unwrap();
//! ```
//!
//! # Supported Keys
//!
//! ## Basic Metadata
//! - `title` - Track title
//! - `artist` - Artist name
//! - `album` - Album title
//! - `albumartist` - Album artist (for compilations)
//! - `date` - Release date/year
//! - `genre` - Genre
//! - `comment` - Comment
//! - `description` - Long description (podcasts)
//! - `grouping` - Grouping/content group
//!
//! ## Track Information
//! - `tracknumber` - Track number (format: "5" or "5/12")
//! - `discnumber` - Disc number (format: "1" or "1/2")
//! - `bpm` - Beats per minute (tempo)
//! - `compilation` - Compilation flag ("1" or "0")
//! - `copyright` - Copyright notice
//!
//! ## Credits
//! - `composer` - Composer name
//!
//! ## Sorting
//! - `albumsort` - Album sort order
//! - `albumartistsort` - Album artist sort order
//! - `artistsort` - Artist sort order
//! - `titlesort` - Title sort order
//! - `composersort` - Composer sort order
//!
//! ## MusicBrainz IDs (Freeform)
//! - `musicbrainz_artistid` - MusicBrainz Artist ID
//! - `musicbrainz_trackid` - MusicBrainz Track ID
//! - `musicbrainz_albumid` - MusicBrainz Release ID
//! - `musicbrainz_albumartistid` - MusicBrainz Album Artist ID
//! - `musicbrainz_albumstatus` - Release status
//! - `musicbrainz_albumtype` - Release type
//! - `releasecountry` - Release country
//! - `musicip_puid` - MusicIP PUID
//!
//! ## Other
//! - `encodingsoftware` - Encoding software name
//!
//! # Value Types
//!
//! EasyMP4Tags automatically handles different value types:
//!
//! - **Text**: Most fields (title, artist, etc.) - stored as UTF-8 strings
//! - **Integer**: Numeric fields (bpm, compilation) - stored as integers
//! - **Integer Pairs**: Track/disc numbers - stored as numerator/denominator pairs
//! - **Freeform**: Custom fields - stored in iTunes freeform atoms
//!
//! # Corrupted Atom Names
//!
//! The library automatically handles files with corrupted atom names where the
//! copyright symbol (©) has been replaced with a replacement character (�).
//! This is common in files that have been processed incorrectly.
//!
//! # Limitations
//!
//! EasyMP4Tags is designed for simplicity and doesn't support all MP4 features:
//! - No direct access to cover art (`covr` atom)
//! - No access to chapters (`chpl` atom)
//! - No access to custom/unknown atoms
//! - Limited control over freeform tag namespaces
//!
//! For advanced use cases, use the full `MP4Tags` interface instead.
//!
//! # See Also
//!
//! - `MP4Tags`: Full MP4 tag interface
//! - [`crate::easyid3::EasyID3`]: Similar interface for ID3v2/MP3 files
//! - Full MP4 atom reference in the `mp4` module

use crate::mp4::{AtomDataType, EasyMP4KeyError, MP4, MP4FreeForm, MP4Info, MP4Tags};
use crate::tags::{Metadata, MetadataFields, Tags};
use crate::{AudexError, FileType, Result};
use std::collections::HashMap;
use std::path::Path;

/// Key mapping types for different kinds of metadata values
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum KeyType {
    /// Text values (strings)  
    Text,
    /// Integer values (BPM, etc.)
    Integer,
    /// Integer pair values (track number, disc number)
    IntegerPair,
    /// Freeform values with custom namespaces
    Freeform,
}

/// Key mapping entry storing the relationship between easy keys and MP4 keys
#[derive(Debug, Clone)]
pub struct KeyMapping {
    pub mp4_key: String,
    pub easy_key: String,
    pub key_type: KeyType,
}

/// Registry for key mappings between easy keys and MP4 atom keys
#[derive(Debug, Default)]
pub struct KeyRegistry {
    /// Map from easy key to MP4 key and type
    easy_to_mp4: HashMap<String, KeyMapping>,
    /// Map from MP4 key to easy key and type  
    mp4_to_easy: HashMap<String, KeyMapping>,
}

impl KeyRegistry {
    /// Create a new registry with default mappings
    pub fn new() -> Self {
        let mut registry = Self::default();
        registry.register_default_keys();
        registry
    }

    /// Register a text key mapping
    pub fn register_text_key(&mut self, mp4_key: &str, easy_key: &str) {
        debug_event!(key = %easy_key, mp4_key = %mp4_key, "registered EasyMP4 text key");
        let mapping = KeyMapping {
            mp4_key: mp4_key.to_string(),
            easy_key: easy_key.to_string(),
            key_type: KeyType::Text,
        };

        self.easy_to_mp4
            .insert(easy_key.to_lowercase(), mapping.clone());
        self.mp4_to_easy.insert(mp4_key.to_string(), mapping);
    }

    /// Register an integer key mapping
    pub fn register_int_key(&mut self, mp4_key: &str, easy_key: &str) {
        let mapping = KeyMapping {
            mp4_key: mp4_key.to_string(),
            easy_key: easy_key.to_string(),
            key_type: KeyType::Integer,
        };

        self.easy_to_mp4
            .insert(easy_key.to_lowercase(), mapping.clone());
        self.mp4_to_easy.insert(mp4_key.to_string(), mapping);
    }

    /// Register an integer pair key mapping
    pub fn register_int_pair_key(&mut self, mp4_key: &str, easy_key: &str) {
        let mapping = KeyMapping {
            mp4_key: mp4_key.to_string(),
            easy_key: easy_key.to_string(),
            key_type: KeyType::IntegerPair,
        };

        self.easy_to_mp4
            .insert(easy_key.to_lowercase(), mapping.clone());
        self.mp4_to_easy.insert(mp4_key.to_string(), mapping);
    }

    /// Register a freeform key mapping
    pub fn register_freeform_key(&mut self, freeform_key: &str, easy_key: &str) {
        debug_event!(key = %easy_key, freeform_key = %freeform_key, "registered EasyMP4 freeform key");
        let mp4_key = format!("----:com.apple.itunes:{}", freeform_key);
        let mapping = KeyMapping {
            mp4_key,
            easy_key: easy_key.to_string(),
            key_type: KeyType::Freeform,
        };

        self.easy_to_mp4
            .insert(easy_key.to_lowercase(), mapping.clone());
        self.mp4_to_easy.insert(mapping.mp4_key.clone(), mapping);
    }

    pub fn get_mp4_key(&self, easy_key: &str) -> Option<&KeyMapping> {
        self.easy_to_mp4.get(&easy_key.to_lowercase())
    }

    pub fn get_easy_key(&self, mp4_key: &str) -> Option<&KeyMapping> {
        self.mp4_to_easy.get(mp4_key)
    }

    /// Register all default key mappings
    fn register_default_keys(&mut self) {
        // Text keys
        self.register_text_key("©nam", "title");
        self.register_text_key("©alb", "album");
        self.register_text_key("©ART", "artist");
        self.register_text_key("aART", "albumartist");
        self.register_text_key("©day", "date");
        self.register_text_key("©cmt", "comment");
        self.register_text_key("desc", "description");
        self.register_text_key("©grp", "grouping");
        self.register_text_key("©gen", "genre");
        self.register_text_key("©wrt", "composer");
        self.register_text_key("cprt", "copyright");
        self.register_text_key("soal", "albumsort");
        self.register_text_key("soaa", "albumartistsort");
        self.register_text_key("soar", "artistsort");
        self.register_text_key("sonm", "titlesort");
        self.register_text_key("soco", "composersort");

        // Integer keys (includes boolean values stored as integers)
        self.register_int_key("tmpo", "bpm");
        self.register_int_key("cpil", "compilation");

        // Integer pair keys
        self.register_int_pair_key("trkn", "tracknumber");
        self.register_int_pair_key("disk", "discnumber");

        // Special key for encoding software (©too)
        self.register_text_key("\u{00A9}too", "encodingsoftware");

        // Freeform keys
        self.register_freeform_key("MusicBrainz Artist Id", "musicbrainz_artistid");
        self.register_freeform_key("MusicBrainz Track Id", "musicbrainz_trackid");
        self.register_freeform_key("MusicBrainz Album Id", "musicbrainz_albumid");
        self.register_freeform_key("MusicBrainz Album Artist Id", "musicbrainz_albumartistid");
        self.register_freeform_key("MusicIP PUID", "musicip_puid");
        self.register_freeform_key("MusicBrainz Album Status", "musicbrainz_albumstatus");
        self.register_freeform_key("MusicBrainz Album Type", "musicbrainz_albumtype");
        self.register_freeform_key("MusicBrainz Release Country", "releasecountry");
    }
}

/// EasyMP4 tags providing key-value interface
#[derive(Debug)]
pub struct EasyMP4Tags {
    /// Underlying MP4 tags (public for direct access when needed)
    pub tags: MP4Tags,
    /// Key registry for mapping between easy and MP4 keys
    registry: KeyRegistry,
}

impl EasyMP4Tags {
    /// Create new EasyMP4Tags wrapping MP4Tags
    pub fn new(tags: MP4Tags) -> Self {
        Self {
            tags,
            registry: KeyRegistry::new(),
        }
    }

    /// Create empty EasyMP4Tags
    pub fn empty() -> Self {
        Self::new(MP4Tags::new())
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<String>>> {
        trace_event!(key = %key, "EasyMP4 get");
        let mapping = self
            .registry
            .get_mp4_key(key)
            .ok_or_else(|| EasyMP4KeyError::new(key))?;

        match mapping.key_type {
            KeyType::Text | KeyType::Integer => {
                // First try the primary mapping
                if let Some(values) = self.tags.get(&mapping.mp4_key) {
                    Ok(Some(values.to_vec()))
                } else {
                    // If not found, try the corrupted version (© -> �)
                    let corrupted_key = mapping.mp4_key.replace('©', "�");
                    if corrupted_key != mapping.mp4_key {
                        if let Some(values) = self.tags.get(&corrupted_key) {
                            Ok(Some(values.to_vec()))
                        } else {
                            Ok(None)
                        }
                    } else {
                        Ok(None)
                    }
                }
            }
            KeyType::IntegerPair => {
                // First try the primary mapping
                if let Some(values) = self.tags.get(&mapping.mp4_key) {
                    Ok(Some(values.to_vec()))
                } else {
                    // If not found, try the corrupted version (© -> �)
                    let corrupted_key = mapping.mp4_key.replace('©', "�");
                    if corrupted_key != mapping.mp4_key {
                        if let Some(values) = self.tags.get(&corrupted_key) {
                            Ok(Some(values.to_vec()))
                        } else {
                            Ok(None)
                        }
                    } else {
                        Ok(None)
                    }
                }
            }
            KeyType::Freeform => {
                if let Some(freeforms) = self.tags.freeforms.get(&mapping.mp4_key) {
                    let values: Vec<String> = freeforms
                        .iter()
                        .map(|f| String::from_utf8_lossy(&f.data).into_owned())
                        .collect();
                    Ok(Some(values))
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub fn set(&mut self, key: &str, values: Vec<String>) -> Result<()> {
        trace_event!(key = %key, count = values.len(), "EasyMP4 set");
        let mapping = self
            .registry
            .get_mp4_key(key)
            .ok_or_else(|| EasyMP4KeyError::new(key))?
            .clone();

        if values.is_empty() {
            return self.remove(key);
        }

        match mapping.key_type {
            KeyType::Text => {
                // Normalize corrupted keys back to their correct form before saving
                let normalized_key = self.normalize_mp4_key(&mapping.mp4_key);
                self.tags.set(&normalized_key, values);
                Ok(())
            }
            KeyType::Integer => {
                // Convert strings to integers and validate
                let int_values: Result<Vec<String>> = values
                    .into_iter()
                    .map(|v| {
                        let i = v.parse::<i32>().map_err(|_| {
                            AudexError::ParseError(format!("'{}' is not an integer", v))
                        })?;
                        if i < 0 {
                            return Err(AudexError::ParseError(format!(
                                "'{}' is negative — integer tag values must be non-negative",
                                v
                            )));
                        }
                        if i > 65535 {
                            return Err(AudexError::ParseError(format!(
                                "Value {} exceeds MP4 integer limit of 65535",
                                i
                            )));
                        }
                        Ok(i.to_string())
                    })
                    .collect();

                let normalized_key = self.normalize_mp4_key(&mapping.mp4_key);
                self.tags.set(&normalized_key, int_values?);
                Ok(())
            }
            KeyType::IntegerPair => {
                // Parse and validate integer pairs (format: "track" or "track/total")
                let pair_values: Result<Vec<String>> = values
                    .into_iter()
                    .map(|v| {
                        if v.contains('/') {
                            let parts: Vec<&str> = v.split('/').collect();
                            if parts.len() != 2 {
                                return Err(AudexError::ParseError(format!(
                                    "'{}' is not a valid integer pair",
                                    v
                                )));
                            }
                            let track: u32 = parts[0].parse().map_err(|_| {
                                AudexError::ParseError(format!(
                                    "'{}' is not a valid integer pair",
                                    v
                                ))
                            })?;
                            let total: u32 = parts[1].parse().map_err(|_| {
                                AudexError::ParseError(format!(
                                    "'{}' is not a valid integer pair",
                                    v
                                ))
                            })?;
                            if track > 65535 || total > 65535 {
                                return Err(AudexError::ParseError(format!(
                                    "Value in '{}' exceeds MP4 integer limit of 65535",
                                    v
                                )));
                            }
                            Ok(format!("{}/{}", track, total))
                        } else {
                            let track: u32 = v.parse().map_err(|_| {
                                AudexError::ParseError(format!("'{}' is not a valid integer", v))
                            })?;
                            if track > 65535 {
                                return Err(AudexError::ParseError(format!(
                                    "Value {} exceeds MP4 integer limit of 65535",
                                    track
                                )));
                            }
                            Ok(track.to_string())
                        }
                    })
                    .collect();

                let normalized_key = self.normalize_mp4_key(&mapping.mp4_key);
                self.tags.set(&normalized_key, pair_values?);
                Ok(())
            }
            KeyType::Freeform => {
                // Convert strings to MP4FreeForm values with UTF-8 encoding
                let freeforms: Vec<MP4FreeForm> = values
                    .into_iter()
                    .map(|v| MP4FreeForm::new(v.into_bytes(), AtomDataType::Utf8, 0))
                    .collect();

                self.tags.freeforms.insert(mapping.mp4_key, freeforms);
                Ok(())
            }
        }
    }

    pub fn remove(&mut self, key: &str) -> Result<()> {
        let mapping = self
            .registry
            .get_mp4_key(key)
            .ok_or_else(|| EasyMP4KeyError::new(key))?;

        match mapping.key_type {
            KeyType::Text | KeyType::Integer | KeyType::IntegerPair => {
                self.tags.remove(&mapping.mp4_key);
                // Also remove the corrupted variant (© replaced with U+FFFD)
                // so stale data from older files does not linger.
                let corrupted_key = mapping.mp4_key.replace('\u{00A9}', "\u{FFFD}");
                if corrupted_key != mapping.mp4_key {
                    self.tags.remove(&corrupted_key);
                }
                Ok(())
            }
            KeyType::Freeform => {
                self.tags.freeforms.remove(&mapping.mp4_key);
                Ok(())
            }
        }
    }

    pub fn keys(&self) -> Vec<String> {
        let mut keys = Vec::new();

        // Check MP4 tags for mapped keys
        for mp4_key in self.tags.keys() {
            if let Some(mapping) = self.registry.get_easy_key(&mp4_key) {
                keys.push(mapping.easy_key.clone());
            }
        }

        // Check freeform tags for mapped keys
        for mp4_key in self.tags.freeforms.keys() {
            if let Some(mapping) = self.registry.get_easy_key(mp4_key) {
                keys.push(mapping.easy_key.clone());
            }
        }

        keys.sort();
        keys.dedup();
        keys
    }

    pub fn contains_key(&self, key: &str) -> bool {
        if let Ok(values) = self.get(key) {
            values.is_some()
        } else {
            false
        }
    }

    /// Normalize a corrupted MP4 key by replacing � with ©
    fn normalize_mp4_key(&self, key: &str) -> String {
        // Replace the Unicode replacement character with the copyright symbol
        key.replace('�', "©")
    }

    pub fn mp4_tags(&self) -> &MP4Tags {
        &self.tags
    }

    pub fn mp4_tags_mut(&mut self) -> &mut MP4Tags {
        &mut self.tags
    }

    /// Load EasyMP4Tags from atoms (used internally)
    pub fn load<R: std::io::Read + std::io::Seek>(
        atoms: &crate::mp4::atom::Atoms,
        reader: &mut R,
    ) -> Result<Option<Self>> {
        trace_event!("loading EasyMP4Tags from atoms");
        if let Some(mp4_tags) = MP4Tags::load(atoms, reader)? {
            Ok(Some(Self::new(mp4_tags)))
        } else {
            Ok(None)
        }
    }

    /// Check if EasyMP4Tags can be loaded from atoms
    pub fn can_load(atoms: &crate::mp4::atom::Atoms) -> bool {
        MP4Tags::can_load(atoms)
    }
}

impl Tags for EasyMP4Tags {
    /// Get tag values by key via the [`Tags`] trait.
    ///
    /// **Note:** This trait method returns `None` for freeform keys (e.g.,
    /// `musicbrainz_trackid`) because freeform values are stored as `MP4FreeForm`
    /// bytes, not as `Vec<String>`, and cannot be returned as `&[String]`.
    /// Use [`EasyMP4Tags::get()`] (the inherent method) instead, which returns
    /// `Result<Option<Vec<String>>>` and handles freeform decoding.
    fn get(&self, key: &str) -> Option<&[String]> {
        let matched_key = self.resolve_easy_key(key)?;

        // Use fully-qualified call to avoid ambiguity with the trait method
        if let Ok(Some(_values)) = EasyMP4Tags::get(self, &matched_key) {
            if let Some(mapping) = self.registry.get_mp4_key(&matched_key) {
                match mapping.key_type {
                    KeyType::Text | KeyType::Integer | KeyType::IntegerPair => {
                        self.tags.get(&mapping.mp4_key)
                    }
                    KeyType::Freeform => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        if let Some(matched_key) = self.resolve_easy_key(key) {
            // Use fully-qualified call to avoid ambiguity with the trait method
            if let Err(_e) = EasyMP4Tags::set(self, &matched_key, values) {
                warn_event!(key = %matched_key, error = %_e, "EasyMP4 trait set failed");
            }
        } else {
            // Try direct set - this will fail if key is invalid
            if let Err(_e) = EasyMP4Tags::set(self, key, values) {
                warn_event!(key = %key, error = %_e, "EasyMP4 trait set failed");
            }
        }
    }

    fn remove(&mut self, key: &str) {
        if let Some(matched_key) = self.resolve_easy_key(key) {
            // Use fully-qualified call to avoid ambiguity with the trait method
            if let Err(_e) = EasyMP4Tags::remove(self, &matched_key) {
                warn_event!(key = %matched_key, error = %_e, "EasyMP4 trait remove failed");
            }
        } else if let Err(_e) = EasyMP4Tags::remove(self, key) {
            warn_event!(key = %key, error = %_e, "EasyMP4 trait remove failed");
        }
    }

    fn keys(&self) -> Vec<String> {
        self.keys()
    }

    fn pprint(&self) -> String {
        let mut result = String::new();
        let keys = self.keys();

        for key in keys {
            if let Ok(Some(values)) = self.get(&key) {
                for value in values {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&format!("{}={}", key, value));
                }
            }
        }

        result
    }
}

impl EasyMP4Tags {
    fn resolve_easy_key(&self, key: &str) -> Option<String> {
        self.keys()
            .into_iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(key))
            .or_else(|| {
                self.registry
                    .get_mp4_key(key)
                    .map(|mapping| mapping.easy_key.clone())
            })
    }
}

impl Metadata for EasyMP4Tags {
    type Error = AudexError;

    fn new() -> Self {
        Self::empty()
    }

    fn load_from_fileobj(filething: &mut crate::util::AnyFileThing) -> Result<Self> {
        // Parse atoms from the file
        let atoms = crate::mp4::atom::Atoms::parse(filething)?;

        // Load MP4Tags if present
        if let Some(mp4_tags) = MP4Tags::load(&atoms, filething)? {
            Ok(Self::new(mp4_tags))
        } else {
            Ok(Self::empty())
        }
    }

    fn save_to_fileobj(&self, filething: &mut crate::util::AnyFileThing) -> Result<()> {
        if let Some(path) = filething.filename() {
            self.tags.save(path)
        } else {
            Err(AudexError::NotImplementedMethod(
                "save_to_fileobj requires a file path for MP4 format".to_string(),
            ))
        }
    }

    fn delete_from_fileobj(filething: &mut crate::util::AnyFileThing) -> Result<()> {
        // For deletion, we create empty tags and save them
        if let Some(path) = filething.filename() {
            let empty_tags = MP4Tags::new();
            empty_tags.save(path)
        } else {
            Err(AudexError::NotImplementedMethod(
                "delete_from_fileobj requires a file path for MP4 format".to_string(),
            ))
        }
    }
}

impl MetadataFields for EasyMP4Tags {
    fn artist(&self) -> Option<&String> {
        // Try to get through easy interface, fall back to MP4 tags
        if let Ok(Some(_values)) = self.get("artist") {
            // Can't return reference to temporary - use MP4 tags directly
            self.tags.get_first("©ART")
        } else {
            None
        }
    }

    fn set_artist(&mut self, artist: String) {
        let _ = self.set("artist", vec![artist]);
    }

    fn album(&self) -> Option<&String> {
        self.tags.get_first("©alb")
    }

    fn set_album(&mut self, album: String) {
        let _ = self.set("album", vec![album]);
    }

    fn title(&self) -> Option<&String> {
        self.tags.get_first("©nam")
    }

    fn set_title(&mut self, title: String) {
        let _ = self.set("title", vec![title]);
    }

    fn track_number(&self) -> Option<u32> {
        if let Ok(Some(values)) = self.get("tracknumber") {
            values.first()?.split('/').next()?.parse().ok()
        } else {
            None
        }
    }

    fn set_track_number(&mut self, track: u32) {
        let _ = self.set("tracknumber", vec![track.to_string()]);
    }

    fn date(&self) -> Option<&String> {
        self.tags.get_first("©day")
    }

    fn set_date(&mut self, date: String) {
        let _ = self.set("date", vec![date]);
    }

    fn genre(&self) -> Option<&String> {
        self.tags.get_first("©gen")
    }

    fn set_genre(&mut self, genre: String) {
        let _ = self.set("genre", vec![genre]);
    }
}

/// EasyMP4 file wrapper providing key-value interface for MP4 metadata
#[derive(Debug)]
pub struct EasyMP4 {
    /// File path
    path: Option<std::path::PathBuf>,
    /// Stream info
    pub info: MP4Info,
    /// Easy MP4 tags
    tags: Option<EasyMP4Tags>,
}

impl EasyMP4 {
    /// Create a new EasyMP4 instance
    pub fn new() -> Self {
        Self {
            path: None,
            info: MP4Info::default(),
            tags: None,
        }
    }

    /// Add tags to the file if none exist
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::ParseError("tags already exist".to_string()));
        }
        self.tags = Some(EasyMP4Tags::empty());
        Ok(())
    }

    /// Get a mutable reference to tags, creating them if they don't exist
    pub fn get_or_create_tags(&mut self) -> Result<&mut EasyMP4Tags> {
        if self.tags.is_none() {
            self.tags = Some(EasyMP4Tags::empty());
        }
        self.tags
            .as_mut()
            .ok_or_else(|| AudexError::InvalidOperation("No tags available".to_string()))
    }

    /// Register a text key mapping for dynamic key registration
    ///
    /// This allows mapping custom keys to MP4 atom paths at runtime.
    /// Automatically detects if the key should be freeform or standard text based on the atom path.
    pub fn register_text_key(&mut self, easy_key: &str, mp4_atom_path: &str) -> Result<()> {
        // Get or create tags to access the registry
        let tags = self.get_or_create_tags()?;

        // Detect if this is a freeform key (starts with "----:") or standard text key
        if mp4_atom_path.starts_with("----:") {
            // For freeform keys, use the full path as-is instead of hardcoding namespace
            // This preserves custom namespaces like "TXXX" instead of forcing "com.apple.itunes"
            let mapping = KeyMapping {
                mp4_key: mp4_atom_path.to_string(),
                easy_key: easy_key.to_string(),
                key_type: KeyType::Freeform,
            };

            tags.registry
                .easy_to_mp4
                .insert(easy_key.to_lowercase(), mapping.clone());
            tags.registry
                .mp4_to_easy
                .insert(mp4_atom_path.to_string(), mapping);
        } else {
            // Register as standard text key
            tags.registry.register_text_key(mp4_atom_path, easy_key);
        }

        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<Vec<String>> {
        self.tags.as_ref()?.get(key).ok().flatten()
    }

    pub fn set(&mut self, key: &str, values: Vec<String>) -> Result<()> {
        let tags = self.get_or_create_tags()?;
        tags.set(key, values)
    }

    pub fn remove(&mut self, key: &str) -> Result<()> {
        if let Some(ref mut tags) = self.tags {
            tags.remove(key)
        } else {
            Ok(())
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tags.as_ref().is_none_or(|tags| tags.keys().is_empty())
    }

    pub fn keys(&self) -> Vec<String> {
        self.tags.as_ref().map_or(Vec::new(), |tags| tags.keys())
    }
}

impl Default for EasyMP4 {
    fn default() -> Self {
        Self::new()
    }
}

// Async methods for EasyMP4 - feature-gated for async runtime support
#[cfg(feature = "async")]
impl EasyMP4 {
    /// Load EasyMP4 file asynchronously
    ///
    /// Reads MP4 stream information and converts tags to easy interface
    /// using non-blocking I/O operations for improved concurrency.
    ///
    /// # Arguments
    /// * `path` - Path to the MP4 audio file
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded file with easy metadata access
    /// * `Err(AudexError)` - Error occurred during loading
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Load the underlying MP4 file using async method
        let mp4 = crate::mp4::MP4::load_async(path).await?;

        // Convert MP4Tags to EasyMP4Tags if present
        let tags = mp4.tags.map(EasyMP4Tags::new);

        Ok(EasyMP4 {
            path: Some(path.to_path_buf()),
            info: mp4.info,
            tags,
        })
    }

    /// Save EasyMP4 metadata asynchronously using native async I/O.
    pub async fn save_async(&mut self) -> Result<()> {
        if let Some(path) = &self.path {
            if let Some(tags) = &self.tags {
                tags.mp4_tags().save_async(path).await?;
            }
        } else {
            return Err(AudexError::ParseError(
                "No file path available for saving".to_string(),
            ));
        }
        Ok(())
    }

    /// Clear all metadata asynchronously
    ///
    /// Removes tag data and saves the cleared state to disk.
    pub async fn clear_async(&mut self) -> Result<()> {
        self.tags = Some(EasyMP4Tags::empty());
        if self.path.is_some() {
            self.save_async().await
        } else {
            Ok(())
        }
    }

    /// Delete all metadata from file asynchronously
    ///
    /// Removes all metadata from the file on disk.
    /// This is a static method that operates directly on the file.
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        crate::mp4::util::clear_async(path).await
    }
}

impl FileType for EasyMP4 {
    type Tags = EasyMP4Tags;
    type Info = MP4Info;

    fn format_id() -> &'static str {
        "EasyMP4"
    }

    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(skip_all, fields(format = "EasyMP4"))
    )]
    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        info_event!(path = %path.as_ref().display(), "loading EasyMP4 tags");
        // Load the underlying MP4 file
        let mp4 = MP4::load(&path)?;

        // Convert MP4Tags to EasyMP4Tags if present
        let tags = mp4.tags.map(EasyMP4Tags::new);

        Ok(EasyMP4 {
            path: Some(path.as_ref().to_path_buf()),
            info: mp4.info,
            tags,
        })
    }

    fn save(&mut self) -> Result<()> {
        if let Some(path) = &self.path {
            if let Some(tags) = &self.tags {
                // Convert EasyMP4Tags back to MP4Tags and save
                tags.mp4_tags().save(path)
            } else {
                // No tags to save
                Ok(())
            }
        } else {
            Err(AudexError::ParseError(
                "No file path available for saving".to_string(),
            ))
        }
    }

    fn clear(&mut self) -> Result<()> {
        // Create empty tags instead of None, so save() will write empty tags to file
        self.tags = Some(EasyMP4Tags::empty());
        if self.path.is_some() {
            self.save()
        } else {
            Ok(())
        }
    }

    /// Adds empty MP4 metadata tags to the file.
    ///
    /// Creates a new empty tag structure if none exists. If tags already exist,
    /// returns an error.
    ///
    /// Note: the inherent method `EasyMP4::add_tags()` returns
    /// `AudexError::ParseError` on failure. This trait method returns
    /// `AudexError::InvalidOperation` and is reached via
    /// `FileType::add_tags(&mut mp4)`.
    ///
    /// # Errors
    ///
    /// Returns `AudexError::InvalidOperation` if tags already exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::easymp4::EasyMP4;
    /// use audex::FileType;
    ///
    /// let mut mp4 = EasyMP4::load("song.m4a")?;
    /// if mp4.tags().is_none() {
    ///     mp4.add_tags()?;
    /// }
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(EasyMP4Tags::empty());
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
        // Use the same scoring as MP4, but with slightly lower priority
        // so MP4 is preferred when both are available
        MP4::score(filename, header).saturating_sub(1)
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/mp4", "audio/x-m4a"]
    }
}

// Convenience functions

/// Register a text key mapping
pub fn register_text_key(_mp4_key: &str, _easy_key: &str) {
    warn_event!("register_text_key not implemented - keys are hardcoded");
}

/// Register an integer key mapping
pub fn register_int_key(_mp4_key: &str, _easy_key: &str) {
    warn_event!("register_int_key not implemented - keys are hardcoded");
}

/// Register an integer pair key mapping
pub fn register_int_pair_key(_mp4_key: &str, _easy_key: &str) {
    warn_event!("register_int_pair_key not implemented - keys are hardcoded");
}

/// Register a freeform key mapping
pub fn register_freeform_key(_freeform_key: &str, _easy_key: &str) {
    warn_event!("register_freeform_key not implemented - keys are hardcoded");
}
