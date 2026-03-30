//! Tag conversion and cross-format metadata transfer
//!
//! This module provides a unified system for converting audio metadata between
//! different tagging formats. It defines [`TagMap`] as a format-agnostic
//! intermediate representation, along with traits for extracting tags from
//! and applying tags to any supported format.
//!
//! # Supported Tag Systems
//!
//! - **ID3v2** — MP3, AIFF, WAV, DSF, DSDIFF
//! - **Vorbis Comments** — FLAC, Ogg Vorbis, Ogg Opus, Ogg Speex
//! - **iTunes/MP4 atoms** — M4A, MP4, AAC containers
//! - **APEv2** — Monkey's Audio, WavPack, Musepack, TAK, OptimFROG
//! - **ASF attributes** — WMA, ASF
//!
//! # Usage
//!
//! ```no_run
//! use audex::File;
//! use audex::tagmap::convert_tags;
//!
//! let source = File::load("song.mp3")?;
//! let mut dest = File::load("song.flac")?;
//! let report = convert_tags(&source, &mut dest)?;
//! println!("Transferred {} fields", report.transferred.len());
//! dest.save()?;
//! # Ok::<(), audex::AudexError>(())
//! ```

pub mod mappings;
pub mod normalize;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

use crate::Result;
use crate::file::DynamicFileType;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// Re-export sub-module items used in the public API
pub use normalize::TagSystem;

// ---------------------------------------------------------------------------
// StandardField — canonical field names shared by all tagging systems
// ---------------------------------------------------------------------------

/// Canonical metadata field names that can be mapped across all tag formats.
///
/// Each variant represents a semantic concept (e.g. "the track title") rather
/// than a format-specific key, enabling conversion between ID3v2 frames,
/// Vorbis Comment keys, MP4 atoms, APEv2 keys, and ASF attributes. Note that
/// round-trip conversion may be lossy (e.g. ID3v2.3 TYER/TDAT merge into a
/// single field, and not all formats map all fields).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum StandardField {
    Title,
    Artist,
    Album,
    AlbumArtist,
    TrackNumber,
    TotalTracks,
    DiscNumber,
    TotalDiscs,
    Date,
    Year,
    Genre,
    Comment,
    Description,
    Composer,
    Performer,
    Conductor,
    Lyricist,
    Publisher,
    Copyright,
    EncodedBy,
    Encoder,
    Language,
    Mood,
    BPM,
    ISRC,
    Barcode,
    CatalogNumber,
    Label,
    Compilation,
    Lyrics,
    Work,
    Movement,
    MovementCount,
    MovementIndex,
    SortTitle,
    SortArtist,
    SortAlbum,
    SortAlbumArtist,
    SortComposer,
    ReplayGainTrackGain,
    ReplayGainTrackPeak,
    ReplayGainAlbumGain,
    ReplayGainAlbumPeak,
    MusicBrainzTrackId,
    MusicBrainzAlbumId,
    MusicBrainzArtistId,
    MusicBrainzReleaseGroupId,
    AcoustIdFingerprint,
    AcoustIdId,
}

impl fmt::Display for StandardField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Album => "Album",
            Self::AlbumArtist => "Album Artist",
            Self::TrackNumber => "Track Number",
            Self::TotalTracks => "Total Tracks",
            Self::DiscNumber => "Disc Number",
            Self::TotalDiscs => "Total Discs",
            Self::Date => "Date",
            Self::Year => "Year",
            Self::Genre => "Genre",
            Self::Comment => "Comment",
            Self::Description => "Description",
            Self::Composer => "Composer",
            Self::Performer => "Performer",
            Self::Conductor => "Conductor",
            Self::Lyricist => "Lyricist",
            Self::Publisher => "Publisher",
            Self::Copyright => "Copyright",
            Self::EncodedBy => "Encoded By",
            Self::Encoder => "Encoder",
            Self::Language => "Language",
            Self::Mood => "Mood",
            Self::BPM => "BPM",
            Self::ISRC => "ISRC",
            Self::Barcode => "Barcode",
            Self::CatalogNumber => "Catalog Number",
            Self::Label => "Label",
            Self::Compilation => "Compilation",
            Self::Lyrics => "Lyrics",
            Self::Work => "Work",
            Self::Movement => "Movement",
            Self::MovementCount => "Movement Count",
            Self::MovementIndex => "Movement Index",
            Self::SortTitle => "Sort Title",
            Self::SortArtist => "Sort Artist",
            Self::SortAlbum => "Sort Album",
            Self::SortAlbumArtist => "Sort Album Artist",
            Self::SortComposer => "Sort Composer",
            Self::ReplayGainTrackGain => "ReplayGain Track Gain",
            Self::ReplayGainTrackPeak => "ReplayGain Track Peak",
            Self::ReplayGainAlbumGain => "ReplayGain Album Gain",
            Self::ReplayGainAlbumPeak => "ReplayGain Album Peak",
            Self::MusicBrainzTrackId => "MusicBrainz Track Id",
            Self::MusicBrainzAlbumId => "MusicBrainz Album Id",
            Self::MusicBrainzArtistId => "MusicBrainz Artist Id",
            Self::MusicBrainzReleaseGroupId => "MusicBrainz Release Group Id",
            Self::AcoustIdFingerprint => "AcoustID Fingerprint",
            Self::AcoustIdId => "AcoustID Id",
        };
        write!(f, "{}", name)
    }
}

impl FromStr for StandardField {
    type Err = String;

    /// Parse a standard field from a human-readable name or common raw key.
    ///
    /// Matching is case-insensitive and tolerates underscores in place of spaces.
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        // Normalize: lowercase, collapse whitespace, replace underscores with spaces
        let normalized = s
            .to_lowercase()
            .replace('_', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        match normalized.as_str() {
            "title" => Ok(Self::Title),
            "artist" => Ok(Self::Artist),
            "album" => Ok(Self::Album),
            "album artist" | "albumartist" => Ok(Self::AlbumArtist),
            "track number" | "tracknumber" | "track" => Ok(Self::TrackNumber),
            "total tracks" | "totaltracks" | "tracktotal" => Ok(Self::TotalTracks),
            "disc number" | "discnumber" | "disc" => Ok(Self::DiscNumber),
            "total discs" | "totaldiscs" | "disctotal" => Ok(Self::TotalDiscs),
            "date" => Ok(Self::Date),
            "year" => Ok(Self::Year),
            "genre" => Ok(Self::Genre),
            "comment" => Ok(Self::Comment),
            "description" => Ok(Self::Description),
            "composer" => Ok(Self::Composer),
            "performer" => Ok(Self::Performer),
            "conductor" => Ok(Self::Conductor),
            "lyricist" => Ok(Self::Lyricist),
            "publisher" => Ok(Self::Publisher),
            "copyright" => Ok(Self::Copyright),
            "encoded by" | "encodedby" => Ok(Self::EncodedBy),
            "encoder" => Ok(Self::Encoder),
            "language" => Ok(Self::Language),
            "mood" => Ok(Self::Mood),
            "bpm" => Ok(Self::BPM),
            "isrc" => Ok(Self::ISRC),
            "barcode" => Ok(Self::Barcode),
            "catalog number" | "catalognumber" => Ok(Self::CatalogNumber),
            "label" => Ok(Self::Label),
            "compilation" => Ok(Self::Compilation),
            "lyrics" => Ok(Self::Lyrics),
            "work" => Ok(Self::Work),
            "movement" | "movementname" => Ok(Self::Movement),
            "movement count" | "movementcount" => Ok(Self::MovementCount),
            "movement index" | "movementindex" | "movement number" => Ok(Self::MovementIndex),
            "sort title" | "sorttitle" => Ok(Self::SortTitle),
            "sort artist" | "sortartist" => Ok(Self::SortArtist),
            "sort album" | "sortalbum" => Ok(Self::SortAlbum),
            "sort album artist" | "sortalbumartist" => Ok(Self::SortAlbumArtist),
            "sort composer" | "sortcomposer" => Ok(Self::SortComposer),
            "replaygain track gain" => Ok(Self::ReplayGainTrackGain),
            "replaygain track peak" => Ok(Self::ReplayGainTrackPeak),
            "replaygain album gain" => Ok(Self::ReplayGainAlbumGain),
            "replaygain album peak" => Ok(Self::ReplayGainAlbumPeak),
            "musicbrainz track id" | "musicbrainz trackid" => Ok(Self::MusicBrainzTrackId),
            "musicbrainz album id" | "musicbrainz albumid" => Ok(Self::MusicBrainzAlbumId),
            "musicbrainz artist id" | "musicbrainz artistid" => Ok(Self::MusicBrainzArtistId),
            "musicbrainz release group id" | "musicbrainz releasegroupid" => {
                Ok(Self::MusicBrainzReleaseGroupId)
            }
            "acoustid fingerprint" => Ok(Self::AcoustIdFingerprint),
            "acoustid id" => Ok(Self::AcoustIdId),
            _ => Err(format!("Unknown standard field: {}", s)),
        }
    }
}

// ---------------------------------------------------------------------------
// TagMapPicture
// ---------------------------------------------------------------------------

/// Embedded artwork in tag conversions.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TagMapPicture {
    /// Raw image bytes
    pub data: Vec<u8>,
    /// MIME type (e.g. "image/jpeg")
    pub mime_type: String,
    /// Picture type (front cover, back cover, etc.)
    pub picture_type: u8,
    /// Optional description
    pub description: String,
}

// ---------------------------------------------------------------------------
// TagMap — format-agnostic intermediate tag container
// ---------------------------------------------------------------------------

/// Format-agnostic intermediate container for audio metadata.
///
/// `TagMap` acts as a bridge between different tagging systems. Tags are
/// extracted from a source format into a `TagMap`, optionally transformed,
/// and then applied to a destination format.
///
/// Standard fields are stored by [`StandardField`] enum variants, while
/// non-standard or format-specific fields are stored as custom key-value pairs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TagMap {
    /// Standard metadata fields mapped by canonical name
    fields: HashMap<StandardField, Vec<String>>,
    /// Non-standard or format-specific fields (prefixed with format origin)
    custom: HashMap<String, Vec<String>>,
    /// Embedded artwork (reserved for future use)
    pictures: Vec<TagMapPicture>,
}

impl TagMap {
    /// Create a new empty tag map.
    pub fn new() -> Self {
        Self {
            fields: HashMap::new(),
            custom: HashMap::new(),
            pictures: Vec::new(),
        }
    }

    /// Get the values for a standard field, if present.
    pub fn get(&self, field: &StandardField) -> Option<&[String]> {
        self.fields.get(field).map(|v| v.as_slice())
    }

    /// Get the values for a custom (non-standard) field, if present.
    pub fn get_custom(&self, key: &str) -> Option<&[String]> {
        self.custom.get(key).map(|v| v.as_slice())
    }

    /// Set the values for a standard field.
    ///
    /// Replaces any existing values. To remove a field, use [`remove`](Self::remove).
    pub fn set(&mut self, field: StandardField, values: Vec<String>) {
        if values.is_empty() {
            self.fields.remove(&field);
        } else {
            self.fields.insert(field, values);
        }
    }

    /// Set the values for a custom (non-standard) field.
    pub fn set_custom(&mut self, key: String, values: Vec<String>) {
        if values.is_empty() {
            self.custom.remove(&key);
        } else {
            self.custom.insert(key, values);
        }
    }

    /// Remove a standard field.
    pub fn remove(&mut self, field: &StandardField) {
        self.fields.remove(field);
    }

    /// Remove a custom field.
    pub fn remove_custom(&mut self, key: &str) {
        self.custom.remove(key);
    }

    /// Return all standard fields and their values.
    pub fn standard_fields(&self) -> Vec<(&StandardField, &[String])> {
        self.fields.iter().map(|(k, v)| (k, v.as_slice())).collect()
    }

    /// Return all custom fields and their values.
    pub fn custom_fields(&self) -> Vec<(&str, &[String])> {
        self.custom
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect()
    }

    /// Merge another `TagMap` into this one.
    ///
    /// When `overwrite` is true, fields from `other` replace existing values.
    /// When false, only fields not already present are copied.
    pub fn merge(&mut self, other: &TagMap, overwrite: bool) {
        for (field, values) in &other.fields {
            if overwrite || !self.fields.contains_key(field) {
                self.fields.insert(field.clone(), values.clone());
            }
        }
        for (key, values) in &other.custom {
            if overwrite || !self.custom.contains_key(key) {
                self.custom.insert(key.clone(), values.clone());
            }
        }

        // Merge pictures: overwrite replaces all, otherwise append only new ones
        if overwrite {
            if !other.pictures.is_empty() {
                self.pictures = other.pictures.clone();
            }
        } else {
            let mut seen: HashSet<TagMapPicture> = self.pictures.iter().cloned().collect();
            for pic in &other.pictures {
                if seen.insert(pic.clone()) {
                    self.pictures.push(pic.clone());
                }
            }
        }
    }

    /// Returns true if the tag map contains no fields (standard, custom, or pictures).
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.custom.is_empty() && self.pictures.is_empty()
    }

    /// Remove all fields from the tag map.
    pub fn clear(&mut self) {
        self.fields.clear();
        self.custom.clear();
        self.pictures.clear();
    }
}

impl Default for TagMap {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// IntoTagMap — extract tags from a format into a TagMap
// ---------------------------------------------------------------------------

/// Extract tags from a concrete tag type into a format-agnostic [`TagMap`].
///
/// Each tagging system (ID3v2, Vorbis Comments, MP4, APEv2, ASF) implements
/// this trait to map its native keys to [`StandardField`] variants.
pub trait IntoTagMap {
    /// Convert this tag container into a [`TagMap`].
    fn to_tag_map(&self) -> TagMap;
}

// ---------------------------------------------------------------------------
// FromTagMap — apply a TagMap to a format
// ---------------------------------------------------------------------------

/// Apply a format-agnostic [`TagMap`] to a concrete tag type.
///
/// Each tagging system implements this trait to translate [`StandardField`]
/// variants back into its native keys.
pub trait FromTagMap {
    /// Apply the fields from `map` into this tag container.
    ///
    /// Returns a [`ConversionReport`] summarising which fields were
    /// transferred, which were skipped, and any warnings.
    fn apply_tag_map(&mut self, map: &TagMap) -> Result<ConversionReport>;
}

// ---------------------------------------------------------------------------
// ConversionReport — summary of a tag transfer operation
// ---------------------------------------------------------------------------

/// Summary of a tag conversion operation.
///
/// After calling [`FromTagMap::apply_tag_map`] or [`convert_tags`], this
/// report describes exactly which fields were written, which were skipped,
/// and why.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ConversionReport {
    /// Standard fields successfully written to the destination.
    pub transferred: Vec<StandardField>,
    /// Custom (non-standard) field keys that were written.
    pub custom_transferred: Vec<String>,
    /// Fields that were skipped, with the reason.
    pub skipped: Vec<(String, SkipReason)>,
    /// Non-fatal warnings generated during conversion.
    pub warnings: Vec<String>,
}

impl fmt::Display for ConversionReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Transferred: {} standard, {} custom",
            self.transferred.len(),
            self.custom_transferred.len()
        )?;
        if !self.skipped.is_empty() {
            writeln!(f, "Skipped: {}", self.skipped.len())?;
            for (field, reason) in &self.skipped {
                writeln!(f, "  {} — {}", field, reason)?;
            }
        }
        if !self.warnings.is_empty() {
            writeln!(f, "Warnings:")?;
            for warning in &self.warnings {
                writeln!(f, "  {}", warning)?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// SkipReason — why a field was not transferred
// ---------------------------------------------------------------------------

/// Reason a field was not written during tag conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum SkipReason {
    /// The destination format has no equivalent field.
    UnsupportedByTarget,
    /// The destination format is read-only.
    ReadOnlyFormat,
    /// The value exceeds the destination format's length limit.
    ValueTooLong {
        /// Maximum length the target format allows.
        max_len: usize,
    },
    /// The value type cannot be represented in the destination format.
    IncompatibleType,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedByTarget => write!(f, "unsupported by target format"),
            Self::ReadOnlyFormat => write!(f, "target format is read-only"),
            Self::ValueTooLong { max_len } => {
                write!(f, "value too long (max {} bytes)", max_len)
            }
            Self::IncompatibleType => write!(f, "incompatible value type"),
        }
    }
}

// ---------------------------------------------------------------------------
// ConversionOptions — control which fields are transferred
// ---------------------------------------------------------------------------

/// Options controlling which fields are transferred during tag conversion.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ConversionOptions {
    /// Only transfer these standard fields (None = transfer all).
    pub include_fields: Option<HashSet<StandardField>>,
    /// Exclude these standard fields from transfer.
    pub exclude_fields: HashSet<StandardField>,
    /// Whether to transfer custom (non-standard) fields.
    pub transfer_custom: bool,
    /// Whether to overwrite existing values in the destination.
    pub overwrite: bool,
    /// Whether to clear all destination tags before applying.
    pub clear_destination: bool,
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            include_fields: None,
            exclude_fields: HashSet::new(),
            transfer_custom: true,
            overwrite: true,
            clear_destination: false,
        }
    }
}

// ---------------------------------------------------------------------------
// High-level conversion functions
// ---------------------------------------------------------------------------

/// Transfer all compatible tags from source to destination.
///
/// Extracts tags from `source` into a [`TagMap`], then applies them to `dest`.
/// Returns a [`ConversionReport`] describing the outcome.
pub fn convert_tags(
    source: &DynamicFileType,
    dest: &mut DynamicFileType,
) -> Result<ConversionReport> {
    info_event!("converting tags between formats");
    let map = source.to_tag_map();
    let report = dest.apply_tag_map(&map)?;
    info_event!(
        transferred = report.transferred.len(),
        custom = report.custom_transferred.len(),
        skipped = report.skipped.len(),
        "tag conversion complete"
    );
    report
        .warnings
        .iter()
        .for_each(|_w| warn_event!(warning = %_w, "conversion warning"));
    Ok(report)
}

/// Transfer tags with fine-grained control over which fields are included.
///
/// See [`ConversionOptions`] for available settings.
pub fn convert_tags_with_options(
    source: &DynamicFileType,
    dest: &mut DynamicFileType,
    options: &ConversionOptions,
) -> Result<ConversionReport> {
    info_event!("converting tags with options");

    // Extract all source tags into an intermediate TagMap
    let mut map = source.to_tag_map();

    // Apply include/exclude filters to standard fields
    if let Some(ref include) = options.include_fields {
        map.fields.retain(|field, _| include.contains(field));
    }
    map.fields
        .retain(|field, _| !options.exclude_fields.contains(field));
    map.fields.retain(|_, values| !values.is_empty());

    // Strip custom fields if not requested
    if !options.transfer_custom {
        map.custom.clear();
    }

    // Optionally clear destination before applying.
    // Note: when clear_destination is true, the overwrite check below is
    // a no-op since the destination is empty. We skip it explicitly to
    // avoid the unnecessary to_tag_map() call on the cleared destination.
    if options.clear_destination {
        for key in dest.keys() {
            let _ = dest.remove(&key);
        }
    } else if !options.overwrite {
        // Only check for existing fields when the destination was NOT cleared,
        // since clearing makes the overwrite guard redundant.
        let existing = dest.to_tag_map();
        map.fields
            .retain(|field, _| !existing.fields.contains_key(field));
        map.custom
            .retain(|key, _| !existing.custom.contains_key(key));
    }
    map.custom.retain(|_, values| !values.is_empty());

    let report = dest.apply_tag_map(&map)?;
    info_event!(
        transferred = report.transferred.len(),
        custom = report.custom_transferred.len(),
        skipped = report.skipped.len(),
        "tag conversion with options complete"
    );
    Ok(report)
}
