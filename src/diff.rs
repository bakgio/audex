//! # Tag Diffing
//!
//! Compare metadata between two audio files or between two states
//! of the same file. The diff output is structured and programmatically
//! consumable, with support for filtering, pretty-printing, and
//! optional stream info comparison.
//!
//! ## Basic usage
//!
//! ```no_run
//! use audex::File;
//! use audex::diff;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let file_a = File::load("original.mp3")?;
//! let file_b = File::load("retagged.mp3")?;
//! let result = diff::diff(&file_a, &file_b);
//!
//! if result.is_identical() {
//!     println!("Tags are identical");
//! } else {
//!     println!("{}", result.summary());
//!     println!("{}", result);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Diffing with options
//!
//! ```no_run
//! use audex::File;
//! use audex::diff::{self, DiffOptions};
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let file_a = File::load("song_v1.flac")?;
//! let file_b = File::load("song_v2.flac")?;
//!
//! let options = DiffOptions {
//!     compare_stream_info: true,
//!     case_insensitive_keys: true,
//!     trim_values: true,
//!     ..Default::default()
//! };
//!
//! let result = diff::diff_with_options(&file_a, &file_b, &options);
//! println!("{}", result);
//! # Ok(())
//! # }
//! ```
//!
//! ## Snapshot-based diffing
//!
//! ```no_run
//! use audex::File;
//! use audex::diff;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let mut file = File::load("track.mp3")?;
//! let before = diff::snapshot_tags(&file);
//!
//! file.set("artist", vec!["New Artist".to_string()])?;
//!
//! let result = diff::diff_against_snapshot(&file, &before);
//! println!("{}", result);
//! # Ok(())
//! # }
//! ```

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::StreamInfo;
use crate::file::DynamicFileType;
use crate::tagmap::TagMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// The complete diff result between two sets of tags.
///
/// Each field categorises tag entries by their relationship between the
/// left (source) and right (target) sides of the comparison.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TagDiff {
    /// Fields present in both sources with different values.
    pub changed: Vec<FieldChange>,
    /// Fields present only in the left/source.
    pub left_only: Vec<FieldEntry>,
    /// Fields present only in the right/target.
    pub right_only: Vec<FieldEntry>,
    /// Fields present in both with identical values.
    pub unchanged: Vec<FieldEntry>,
    /// Stream info differences (populated when `compare_stream_info` is enabled).
    pub stream_info_diff: Option<StreamInfoDiff>,
}

/// A single field whose value changed between the two sides.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FieldChange {
    /// The tag key that changed.
    pub key: String,
    /// Values from the left/source side.
    pub left: Vec<String>,
    /// Values from the right/target side.
    pub right: Vec<String>,
}

/// A tag field with its key and values (used for left-only, right-only, and unchanged entries).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FieldEntry {
    /// The tag key.
    pub key: String,
    /// The associated values.
    pub values: Vec<String>,
}

/// Differences in stream-level audio properties between two files.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StreamInfoDiff {
    /// Duration difference in seconds.
    pub length: Option<ValueChange<f64>>,
    /// Bitrate difference in bits per second.
    pub bitrate: Option<ValueChange<u32>>,
    /// Sample rate difference in Hz.
    pub sample_rate: Option<ValueChange<u32>>,
    /// Channel count difference.
    pub channels: Option<ValueChange<u16>>,
    /// Bits-per-sample difference.
    pub bits_per_sample: Option<ValueChange<u16>>,
}

/// A before/after value pair for a single scalar property.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ValueChange<T> {
    /// Value from the left/source side.
    pub left: Option<T>,
    /// Value from the right/target side.
    pub right: Option<T>,
}

// ---------------------------------------------------------------------------
// DiffOptions — configurable comparison behaviour
// ---------------------------------------------------------------------------

/// Options that control how the diff engine compares tags.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DiffOptions {
    /// Compare stream info (duration, bitrate, etc.) as well. Default: `false`.
    pub compare_stream_info: bool,
    /// Normalise keys to lowercase before comparison. Default: `false`.
    pub case_insensitive_keys: bool,
    /// Trim leading/trailing whitespace from values before comparison. Default: `false`.
    pub trim_values: bool,
    /// If set, only these keys are compared. `None` means all keys. Default: `None`.
    pub include_keys: Option<HashSet<String>>,
    /// Keys to exclude from comparison. Default: empty.
    pub exclude_keys: HashSet<String>,
    /// Whether to populate the `unchanged` vec in the result. Default: `false`.
    pub include_unchanged: bool,
    /// Strip format-specific prefixes from custom tag keys so that freeform
    /// tags can be compared across formats.  For example,
    /// `"id3:TXXX:Songwriter"` and `"vorbis:Songwriter"` both become
    /// `"Songwriter"`.  Default: `false`.
    pub normalize_custom_keys: bool,
}

// ---------------------------------------------------------------------------
// StreamInfoDiff helpers
// ---------------------------------------------------------------------------

impl StreamInfoDiff {
    /// Returns `true` if no stream properties differ.
    pub fn is_identical(&self) -> bool {
        self.length.is_none()
            && self.bitrate.is_none()
            && self.sample_rate.is_none()
            && self.channels.is_none()
            && self.bits_per_sample.is_none()
    }
}

// ---------------------------------------------------------------------------
// TagDiff query / inspection methods
// ---------------------------------------------------------------------------

impl TagDiff {
    /// Returns `true` when the two sides are completely identical
    /// (no changed, added, or removed fields, and no stream info differences).
    pub fn is_identical(&self) -> bool {
        self.changed.is_empty()
            && self.left_only.is_empty()
            && self.right_only.is_empty()
            && self
                .stream_info_diff
                .as_ref()
                .is_none_or(|s| s.is_identical())
    }

    /// Total number of differences (changed + left-only + right-only).
    pub fn diff_count(&self) -> usize {
        self.changed.len() + self.left_only.len() + self.right_only.len()
    }

    /// Collect every key that differs in any way (changed, left-only, or right-only).
    pub fn differing_keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = Vec::new();
        for c in &self.changed {
            keys.push(&c.key);
        }
        for e in &self.left_only {
            keys.push(&e.key);
        }
        for e in &self.right_only {
            keys.push(&e.key);
        }
        keys.sort();
        keys
    }

    /// Look up the change record for a specific key, if it was changed.
    pub fn get_change(&self, key: &str) -> Option<&FieldChange> {
        self.changed.iter().find(|c| c.key == key)
    }

    /// Produce a new `TagDiff` containing only the listed keys.
    pub fn filter_keys(&self, keys: &[&str]) -> TagDiff {
        let set: HashSet<&str> = keys.iter().copied().collect();
        TagDiff {
            changed: self
                .changed
                .iter()
                .filter(|c| set.contains(c.key.as_str()))
                .cloned()
                .collect(),
            left_only: self
                .left_only
                .iter()
                .filter(|e| set.contains(e.key.as_str()))
                .cloned()
                .collect(),
            right_only: self
                .right_only
                .iter()
                .filter(|e| set.contains(e.key.as_str()))
                .cloned()
                .collect(),
            unchanged: self
                .unchanged
                .iter()
                .filter(|e| set.contains(e.key.as_str()))
                .cloned()
                .collect(),
            stream_info_diff: self.stream_info_diff.clone(),
        }
    }

    /// Produce a new `TagDiff` with the listed keys removed.
    pub fn exclude_keys(&self, keys: &[&str]) -> TagDiff {
        let set: HashSet<&str> = keys.iter().copied().collect();
        TagDiff {
            changed: self
                .changed
                .iter()
                .filter(|c| !set.contains(c.key.as_str()))
                .cloned()
                .collect(),
            left_only: self
                .left_only
                .iter()
                .filter(|e| !set.contains(e.key.as_str()))
                .cloned()
                .collect(),
            right_only: self
                .right_only
                .iter()
                .filter(|e| !set.contains(e.key.as_str()))
                .cloned()
                .collect(),
            unchanged: self
                .unchanged
                .iter()
                .filter(|e| !set.contains(e.key.as_str()))
                .cloned()
                .collect(),
            stream_info_diff: self.stream_info_diff.clone(),
        }
    }

    /// One-line summary: "N changed, N removed, N added, N unchanged".
    pub fn summary(&self) -> String {
        format!(
            "{} changed, {} removed, {} added, {} unchanged",
            self.changed.len(),
            self.left_only.len(),
            self.right_only.len(),
            self.unchanged.len(),
        )
    }

    /// Human-readable pretty-print with right-aligned keys.
    pub fn pprint(&self) -> String {
        self.format_pretty(false)
    }

    /// Pretty-print including unchanged fields (prefixed with `=`).
    pub fn pprint_full(&self) -> String {
        self.format_pretty(true)
    }

    /// Internal helper that builds the aligned pretty-print output.
    fn format_pretty(&self, show_unchanged: bool) -> String {
        // Determine the widest key for alignment
        let max_key_len = self
            .changed
            .iter()
            .map(|c| c.key.len())
            .chain(self.left_only.iter().map(|e| e.key.len()))
            .chain(self.right_only.iter().map(|e| e.key.len()))
            .chain(if show_unchanged {
                Box::new(self.unchanged.iter().map(|e| e.key.len()))
                    as Box<dyn Iterator<Item = usize>>
            } else {
                Box::new(std::iter::empty()) as Box<dyn Iterator<Item = usize>>
            })
            .max()
            .unwrap_or(0);

        let mut out = String::new();

        // Changed fields
        for c in &self.changed {
            out.push_str(&format!(
                "~ {:>width$}: {:?} → {:?}\n",
                c.key,
                c.left,
                c.right,
                width = max_key_len,
            ));
        }

        // Left-only (removed) fields
        for e in &self.left_only {
            out.push_str(&format!(
                "- {:>width$}: {:?}\n",
                e.key,
                e.values,
                width = max_key_len,
            ));
        }

        // Right-only (added) fields
        for e in &self.right_only {
            out.push_str(&format!(
                "+ {:>width$}: {:?}\n",
                e.key,
                e.values,
                width = max_key_len,
            ));
        }

        // Unchanged fields (only in full mode)
        if show_unchanged {
            for e in &self.unchanged {
                out.push_str(&format!(
                    "= {:>width$}: {:?}\n",
                    e.key,
                    e.values,
                    width = max_key_len,
                ));
            }
        }

        // Stream info differences
        if let Some(ref si) = self.stream_info_diff {
            if !si.is_identical() {
                out.push_str(&format!("{}", si));
            }
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Display implementations
// ---------------------------------------------------------------------------

impl fmt::Display for FieldChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?} → {:?}", self.key, self.left, self.right)
    }
}

impl fmt::Display for FieldEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.key, self.values)
    }
}

impl fmt::Display for StreamInfoDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref v) = self.length {
            writeln!(
                f,
                "~ length: {}s → {}s",
                v.left.map_or("?".to_string(), |l| format!("{:.1}", l)),
                v.right.map_or("?".to_string(), |r| format!("{:.1}", r)),
            )?;
        }
        if let Some(ref v) = self.bitrate {
            writeln!(
                f,
                "~ bitrate: {} → {}",
                v.left.map_or("?".to_string(), |l| l.to_string()),
                v.right.map_or("?".to_string(), |r| r.to_string()),
            )?;
        }
        if let Some(ref v) = self.sample_rate {
            writeln!(
                f,
                "~ sample_rate: {} → {}",
                v.left.map_or("?".to_string(), |l| l.to_string()),
                v.right.map_or("?".to_string(), |r| r.to_string()),
            )?;
        }
        if let Some(ref v) = self.channels {
            writeln!(
                f,
                "~ channels: {} → {}",
                v.left.map_or("?".to_string(), |l| l.to_string()),
                v.right.map_or("?".to_string(), |r| r.to_string()),
            )?;
        }
        if let Some(ref v) = self.bits_per_sample {
            writeln!(
                f,
                "~ bits_per_sample: {} → {}",
                v.left.map_or("?".to_string(), |l| l.to_string()),
                v.right.map_or("?".to_string(), |r| r.to_string()),
            )?;
        }
        Ok(())
    }
}

impl fmt::Display for TagDiff {
    /// Renders a unified-diff style summary:
    /// - `~` for changed fields
    /// - `-` for left-only (removed) fields
    /// - `+` for right-only (added) fields
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_identical() {
            return write!(f, "No differences");
        }

        writeln!(f, "--- left")?;
        writeln!(f, "+++ right")?;

        for c in &self.changed {
            writeln!(f, "~ {}", c)?;
        }
        for e in &self.left_only {
            writeln!(f, "- {}", e)?;
        }
        for e in &self.right_only {
            writeln!(f, "+ {}", e)?;
        }

        if let Some(ref si) = self.stream_info_diff {
            if !si.is_identical() {
                write!(f, "{}", si)?;
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Diff computation — public API
// ---------------------------------------------------------------------------

/// Compare tags between two loaded audio files using default options.
///
/// This performs a raw-key comparison (keys are compared exactly as the
/// format stores them). Unchanged fields are not included in the output.
pub fn diff(left: &DynamicFileType, right: &DynamicFileType) -> TagDiff {
    debug_event!("computing tag diff between two files");
    let result = diff_items(&left.items(), &right.items());
    debug_event!(
        changed = result.changed.len(),
        removed = result.left_only.len(),
        added = result.right_only.len(),
        "diff complete"
    );
    result
}

/// Compare tags between two loaded audio files with configurable options.
pub fn diff_with_options(
    left: &DynamicFileType,
    right: &DynamicFileType,
    options: &DiffOptions,
) -> TagDiff {
    debug_event!(?options, "computing tag diff with options");

    let mut result = diff_items_with_options(&left.items(), &right.items(), options);

    // Optionally compare stream-level properties
    if options.compare_stream_info {
        result.stream_info_diff = Some(compute_stream_info_diff(&left.info(), &right.info()));
    }

    debug_event!(
        changed = result.changed.len(),
        removed = result.left_only.len(),
        added = result.right_only.len(),
        "diff with options complete"
    );
    result
}

/// Compare two sets of tag key-value pairs directly (without loading files).
///
/// Useful for diffing in-memory tag collections, `BasicTags`, or any other
/// source that can produce `(String, Vec<String>)` pairs.
pub fn diff_items(left: &[(String, Vec<String>)], right: &[(String, Vec<String>)]) -> TagDiff {
    diff_items_with_options(left, right, &DiffOptions::default())
}

/// Compare two sets of tag items with the given options.
pub fn diff_items_with_options(
    left: &[(String, Vec<String>)],
    right: &[(String, Vec<String>)],
    options: &DiffOptions,
) -> TagDiff {
    // Build lookup maps, optionally normalising keys
    let normalise_key = |k: &str| -> String {
        if options.case_insensitive_keys {
            k.to_lowercase()
        } else {
            k.to_string()
        }
    };

    let normalise_values = |vals: &[String]| -> Vec<String> {
        if options.trim_values {
            vals.iter().map(|v| v.trim().to_string()).collect()
        } else {
            vals.to_vec()
        }
    };

    // Should this key be included in the comparison?
    // Normalize both the tested key and the filter keys so that
    // case-insensitive mode works correctly for exclude/include lists.
    let should_include = |key: &str| -> bool {
        let norm = normalise_key(key);
        if options
            .exclude_keys
            .iter()
            .any(|k| normalise_key(k) == norm)
        {
            return false;
        }
        if let Some(ref include) = options.include_keys {
            return include.iter().any(|k| normalise_key(k) == norm);
        }
        true
    };

    // Build HashMaps keyed by the (possibly normalised) key.
    // Collect into Vec<Vec<String>> to preserve duplicate keys that
    // collapse under case-insensitive normalisation (e.g., Vorbis
    // Comments with "ARTIST" and "artist" as separate entries).
    let mut left_map: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    for (k, v) in left.iter().filter(|(k, _)| should_include(k)) {
        left_map
            .entry(normalise_key(k))
            .or_default()
            .push(normalise_values(v));
    }

    let mut right_map: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    for (k, v) in right.iter().filter(|(k, _)| should_include(k)) {
        right_map
            .entry(normalise_key(k))
            .or_default()
            .push(normalise_values(v));
    }

    // Flatten grouped values into a single Vec<String> per key so that
    // the comparison treats all values for a normalised key as one
    // logical field. This ensures duplicate keys (e.g., two "artist"
    // entries with different casing) are all accounted for. Value order
    // within each group is preserved so that ["A","B"] != ["B","A"].
    let flatten = |groups: &Vec<Vec<String>>| -> Vec<String> {
        groups.iter().flat_map(|v| v.iter().cloned()).collect()
    };

    let mut changed = Vec::new();
    let mut left_only = Vec::new();
    let mut unchanged = Vec::new();

    // Walk left keys -- classify each as unchanged, changed, or left-only
    for (key, left_groups) in &left_map {
        let left_vals = flatten(left_groups);
        if let Some(right_groups) = right_map.get(key) {
            let right_vals = flatten(right_groups);
            if left_vals == right_vals {
                if options.include_unchanged {
                    trace_event!(key = %key, "unchanged field");
                    unchanged.push(FieldEntry {
                        key: key.clone(),
                        values: left_vals,
                    });
                }
            } else {
                trace_event!(key = %key, "changed field");
                changed.push(FieldChange {
                    key: key.clone(),
                    left: left_vals,
                    right: right_vals,
                });
            }
        } else {
            trace_event!(key = %key, "left-only field");
            left_only.push(FieldEntry {
                key: key.clone(),
                values: left_vals,
            });
        }
    }

    // Walk right keys not already seen -- these are right-only
    let mut right_only: Vec<FieldEntry> = right_map
        .iter()
        .filter(|(k, _)| !left_map.contains_key(*k))
        .map(|(k, v)| {
            trace_event!(key = %k, "right-only field");
            FieldEntry {
                key: k.clone(),
                values: flatten(v),
            }
        })
        .collect();

    // Sort each category by key for deterministic output
    changed.sort_by(|a, b| a.key.cmp(&b.key));
    left_only.sort_by(|a, b| a.key.cmp(&b.key));
    right_only.sort_by(|a, b| a.key.cmp(&b.key));
    unchanged.sort_by(|a, b| a.key.cmp(&b.key));

    TagDiff {
        changed,
        left_only,
        right_only,
        unchanged,
        stream_info_diff: None,
    }
}

/// Capture the current tag state of a file for later comparison.
pub fn snapshot_tags(file: &DynamicFileType) -> Vec<(String, Vec<String>)> {
    file.items()
}

/// Compare a file's current tags against a previously captured snapshot.
///
/// The snapshot is treated as the "left" (before) side.
pub fn diff_against_snapshot(
    file: &DynamicFileType,
    snapshot: &[(String, Vec<String>)],
) -> TagDiff {
    diff_items(snapshot, &file.items())
}

// ---------------------------------------------------------------------------
// Normalized diff — cross-format comparison via TagMap / StandardField
// ---------------------------------------------------------------------------

/// Compare tags between two files using normalized
/// [`StandardField`](crate::tagmap::StandardField) names.
///
/// Both files are first converted to a [`TagMap`] (via
/// [`IntoTagMap`](crate::tagmap::IntoTagMap)), which maps format-specific keys
/// (e.g. ID3v2 `TPE1`, Vorbis `ARTIST`, MP4 `©ART`) to their canonical
/// [`StandardField`](crate::tagmap::StandardField) display names. The diff
/// then compares those human-readable names, making cross-format comparison
/// meaningful.
///
/// Custom (non-standard) fields are included with their format-prefixed keys
/// (e.g. `"id3:TXXX:MY_FIELD"`, `"vorbis:CUSTOM_KEY"`).
///
/// # Example
///
/// ```no_run
/// use audex::File;
/// use audex::diff;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// let mp3  = File::load("song.mp3")?;
/// let flac = File::load("song.flac")?;
///
/// // Raw diff would compare "TPE1" vs "ARTIST" — never matching.
/// // Normalized diff compares "Artist" vs "Artist" — correct.
/// let d = diff::diff_normalized(&mp3, &flac);
/// println!("{}", d.summary());
/// # Ok(())
/// # }
/// ```
#[cfg_attr(feature = "tracing", tracing::instrument(skip(left, right)))]
pub fn diff_normalized(left: &DynamicFileType, right: &DynamicFileType) -> TagDiff {
    debug_event!("computing normalized tag diff via TagMap");

    let left_items = tag_map_to_items(&left.to_tag_map());
    let right_items = tag_map_to_items(&right.to_tag_map());

    let result = diff_items(&left_items, &right_items);
    debug_event!(
        changed = result.changed.len(),
        removed = result.left_only.len(),
        added = result.right_only.len(),
        "normalized diff complete"
    );
    result
}

/// Compare tags using normalized field names with configurable options.
///
/// Combines the cross-format normalization of [`diff_normalized`] with the
/// filtering and comparison tweaks from [`DiffOptions`] (key filters, value
/// trimming, stream info comparison, etc.).
#[cfg_attr(feature = "tracing", tracing::instrument(skip(left, right)))]
pub fn diff_normalized_with_options(
    left: &DynamicFileType,
    right: &DynamicFileType,
    options: &DiffOptions,
) -> TagDiff {
    debug_event!(?options, "computing normalized tag diff with options");

    let left_items = if options.normalize_custom_keys {
        tag_map_to_items_normalized(&left.to_tag_map())
    } else {
        tag_map_to_items(&left.to_tag_map())
    };
    let right_items = if options.normalize_custom_keys {
        tag_map_to_items_normalized(&right.to_tag_map())
    } else {
        tag_map_to_items(&right.to_tag_map())
    };

    let mut result = diff_items_with_options(&left_items, &right_items, options);

    // Optionally compare stream-level properties
    if options.compare_stream_info {
        result.stream_info_diff = Some(compute_stream_info_diff(&left.info(), &right.info()));
    }

    debug_event!(
        changed = result.changed.len(),
        removed = result.left_only.len(),
        added = result.right_only.len(),
        "normalized diff with options complete"
    );
    result
}

/// Flatten a [`TagMap`] into key-value pairs using human-readable field names.
///
/// Standard fields are keyed by their `StandardField::to_string()` display name
/// (e.g. `"Artist"`, `"Track Number"`). Custom fields keep their format-prefixed
/// keys as-is (e.g. `"id3:TXXX:MY_FIELD"`).
fn tag_map_to_items(map: &TagMap) -> Vec<(String, Vec<String>)> {
    let mut items: Vec<(String, Vec<String>)> = Vec::new();

    // Standard fields → human-readable display names
    for (field, values) in map.standard_fields() {
        items.push((field.to_string(), values.to_vec()));
    }

    // Custom fields → keep their prefixed keys
    for (key, values) in map.custom_fields() {
        items.push((key.to_string(), values.to_vec()));
    }

    items
}

/// Like [`tag_map_to_items`] but strips format-specific prefixes and wrappers
/// from custom keys so freeform tags can be compared across formats.
///
/// `"id3:TXXX:Songwriter"`, `"vorbis:Songwriter"`,
/// `"mp4:----:com.apple.itunes:Songwriter"`, `"ape:Songwriter"`, and
/// `"asf:Songwriter"` all become `"Songwriter"`.
fn tag_map_to_items_normalized(map: &TagMap) -> Vec<(String, Vec<String>)> {
    let mut items: Vec<(String, Vec<String>)> = Vec::new();

    for (field, values) in map.standard_fields() {
        items.push((field.to_string(), values.to_vec()));
    }

    for (key, values) in map.custom_fields() {
        items.push((normalize_custom_key(key), values.to_vec()));
    }

    items
}

/// Strip format-specific prefixes and wrappers from a custom tag key and
/// normalize to lowercase so that cross-format comparisons are case-insensitive.
///
/// The format prefix (`id3:`, `vorbis:`, `mp4:`, `ape:`, `asf:`, `unknown:`)
/// is removed first, then format-specific key wrappers are stripped:
///
/// - ID3 `TXXX:` prefix (user-defined text frames)
/// - MP4 freeform `----:com.apple.itunes:` and `----:TXXX:` prefixes
///
/// Finally the result is lowercased because different tag systems use
/// different casing conventions (e.g. ID3 TXXX `"Songwriter"` vs Vorbis
/// `"songwriter"` vs APE `"Songwriter"`).
fn normalize_custom_key(key: &str) -> String {
    // Step 1: strip the format prefix added by items_to_tag_map
    let stripped = if let Some(rest) = key.strip_prefix("id3:") {
        rest
    } else if let Some(rest) = key.strip_prefix("vorbis:") {
        rest
    } else if let Some(rest) = key.strip_prefix("mp4:") {
        rest
    } else if let Some(rest) = key.strip_prefix("ape:") {
        rest
    } else if let Some(rest) = key.strip_prefix("asf:") {
        rest
    } else if let Some(rest) = key.strip_prefix("unknown:") {
        rest
    } else {
        key
    };

    // Step 2: strip format-specific key wrappers
    // ID3 TXXX frames: "TXXX:Description" → "Description"
    if let Some(desc) = stripped.strip_prefix("TXXX:") {
        return desc.to_lowercase();
    }

    // MP4 freeform atoms: "----:com.apple.itunes:Songwriter" → "songwriter"
    if let Some(rest) = stripped.strip_prefix("----:") {
        // "com.apple.itunes:Songwriter" → "songwriter"
        // "TXXX:Merchant" → "merchant"
        if let Some(pos) = rest.find(':') {
            return rest[pos + 1..].to_lowercase();
        }
        return rest.to_lowercase();
    }

    stripped.to_lowercase()
}

// ---------------------------------------------------------------------------
// Internal: stream info comparison
// ---------------------------------------------------------------------------

/// Build a `StreamInfoDiff` by comparing two `DynamicStreamInfo` values field by field.
/// Only fields that actually differ produce a `Some(ValueChange)`.
fn compute_stream_info_diff(
    left: &crate::file::DynamicStreamInfo,
    right: &crate::file::DynamicStreamInfo,
) -> StreamInfoDiff {
    let length = {
        let l = left.length().map(|d| d.as_secs_f64());
        let r = right.length().map(|d| d.as_secs_f64());
        // Use epsilon-based comparison for f64 to avoid false change
        // reports from floating-point rounding differences
        let equal = match (l, r) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-6,
            (None, None) => true,
            _ => false,
        };
        if equal {
            None
        } else {
            Some(ValueChange { left: l, right: r })
        }
    };

    let bitrate = diff_optional_field(left.bitrate(), right.bitrate());
    let sample_rate = diff_optional_field(left.sample_rate(), right.sample_rate());
    let channels = diff_optional_field(left.channels(), right.channels());
    let bits_per_sample = diff_optional_field(left.bits_per_sample(), right.bits_per_sample());

    StreamInfoDiff {
        length,
        bitrate,
        sample_rate,
        channels,
        bits_per_sample,
    }
}

/// Compare two `Option<T>` values — returns `Some(ValueChange)` only when they differ.
fn diff_optional_field<T: PartialEq + Copy>(
    left: Option<T>,
    right: Option<T>,
) -> Option<ValueChange<T>> {
    if left != right {
        Some(ValueChange { left, right })
    } else {
        None
    }
}
