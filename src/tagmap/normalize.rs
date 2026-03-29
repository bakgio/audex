//! Value normalization for cross-format tag conversion.
//!
//! Different tagging systems encode the same semantic value in different ways:
//! ID3v2 stores track numbers as `"5/12"`, Vorbis uses separate keys, and
//! MP4 uses integer pairs. Genre fields may contain numeric references to
//! the ID3v1 genre table. This module provides the normalization functions
//! that handle these quirks during conversion.

use crate::constants;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// TagSystem — identifies the origin format during normalization
// ---------------------------------------------------------------------------

/// Identifies which tagging system a value originated from, allowing
/// normalization functions to apply the correct parsing rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TagSystem {
    /// ID3v2 (used by MP3, AIFF, WAV, DSF, DSDIFF)
    ID3v2,
    /// Vorbis Comments (used by FLAC, Ogg Vorbis, Ogg Opus, Ogg Speex)
    VorbisComment,
    /// iTunes/MP4 atoms (used by M4A, MP4)
    MP4,
    /// APEv2 (used by Monkey's Audio, WavPack, Musepack, TAK, OptimFROG)
    APEv2,
    /// ASF attributes (used by WMA)
    ASF,
}

// ---------------------------------------------------------------------------
// Track/disc number splitting and combining
// ---------------------------------------------------------------------------

/// Split a combined track or disc number string into (number, total).
///
/// Handles formats like `"5/12"`, `"5"`, or `"05"`. Returns None for parts
/// that are not present or cannot be parsed.
///
/// # Examples
///
/// ```
/// use audex::tagmap::normalize::normalize_track_disc;
///
/// let (num, total) = normalize_track_disc("5/12");
/// assert_eq!(num.as_deref(), Some("5"));
/// assert_eq!(total.as_deref(), Some("12"));
///
/// let (num, total) = normalize_track_disc("7");
/// assert_eq!(num.as_deref(), Some("7"));
/// assert_eq!(total, None);
/// ```
pub fn normalize_track_disc(raw: &str) -> (Option<String>, Option<String>) {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (None, None);
    }

    // Split on "/" which is the standard separator in ID3v2 and APEv2.
    // Using split_once avoids raw byte-index slicing, which is safer
    // and more idiomatic than manual find + index arithmetic.
    // Validate that extracted parts contain only digits. Non-numeric values
    // (e.g. "hello/world") are mapped to None since track/disc fields must
    // be numeric for downstream consumers like MP4 integer atoms.
    let is_numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());

    // Strip leading zeros from a numeric string, preserving at least one digit
    // so that "00" becomes "0" rather than an empty string.
    let strip_leading_zeros = |s: &str| -> String {
        let stripped = s.trim_start_matches('0');
        if stripped.is_empty() {
            "0".to_string()
        } else {
            stripped.to_string()
        }
    };

    if let Some((left, right)) = trimmed.split_once('/') {
        let number_part = left.trim();
        let total_part = right.trim();

        let number = if is_numeric(number_part) {
            Some(strip_leading_zeros(number_part))
        } else {
            None
        };
        let total = if is_numeric(total_part) {
            Some(strip_leading_zeros(total_part))
        } else {
            None
        };
        (number, total)
    } else if is_numeric(trimmed) {
        (Some(strip_leading_zeros(trimmed)), None)
    } else {
        (None, None)
    }
}

/// Combine separate number and total values into a single string.
///
/// Produces the `"N/M"` format expected by ID3v2 TRCK/TPOS frames and APEv2.
///
/// # Examples
///
/// ```
/// use audex::tagmap::normalize::combine_track_disc;
///
/// assert_eq!(combine_track_disc(Some("5"), Some("12")), "5/12");
/// assert_eq!(combine_track_disc(Some("3"), None), "3");
/// ```
pub fn combine_track_disc(number: Option<&str>, total: Option<&str>) -> String {
    match (number, total) {
        (Some(n), Some(t)) => format!("{}/{}", n, t),
        (Some(n), None) => n.to_string(),
        (None, Some(t)) => format!("0/{}", t),
        (None, None) => String::new(),
    }
}

// ---------------------------------------------------------------------------
// ID3v1 genre reference resolution
// ---------------------------------------------------------------------------

/// Resolve ID3v1 numeric genre references to human-readable names.
///
/// The TCON frame in ID3v2 may contain numeric references like `"(17)"`,
/// `"(17)Rock"`, `"(17)(18)"`, or a bare number `"17"`. This function
/// resolves all parenthesized numeric references using the standard genre
/// table. Multiple references are joined with `"/"`. A trailing free-text
/// suffix (not wrapped in parentheses) replaces the last numeric lookup.
///
/// # Examples
///
/// ```
/// use audex::tagmap::normalize::resolve_id3_genre;
///
/// assert_eq!(resolve_id3_genre("(17)"), "Rock");
/// assert_eq!(resolve_id3_genre("(17)Rock"), "Rock");
/// assert_eq!(resolve_id3_genre("(17)(18)"), "Rock/Techno");
/// assert_eq!(resolve_id3_genre("Electronic"), "Electronic");
/// assert_eq!(resolve_id3_genre("17"), "Rock");
/// ```
pub fn resolve_id3_genre(raw: &str) -> String {
    trace_event!(raw = %raw, "resolving ID3 genre reference");

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Pattern 1: one or more "(N)" parenthesized numeric references,
    // optionally followed by a free-text suffix. ID3v2 allows compound
    // genre strings like "(17)(18)" meaning Rock + Techno, or
    // "(17)(18)Custom" where the text refines the last reference.
    if trimmed.starts_with('(') {
        let mut resolved: Vec<String> = Vec::new();
        let mut rest = trimmed;

        // Walk through consecutive "(N)" groups
        while let Some(after_open) = rest.strip_prefix('(') {
            if let Some((num_str, remainder)) = after_open.split_once(')') {
                if let Ok(genre_id) = num_str.parse::<u16>() {
                    // Resolve the numeric ID to a genre name, or keep the
                    // raw number if the ID falls outside the standard table.
                    let name = u8::try_from(genre_id)
                        .ok()
                        .and_then(|id| constants::get_genre(id))
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| genre_id.to_string());
                    resolved.push(name);
                    rest = remainder;
                    continue;
                }
            }
            // Non-numeric content in parentheses — stop parsing groups
            break;
        }

        // If we successfully parsed at least one group, check for a
        // trailing free-text suffix that refines the last genre.
        if !resolved.is_empty() {
            let suffix = rest.trim();
            if !suffix.is_empty() {
                // The trailing text replaces the last numeric lookup
                // (e.g. "(17)Rock" → "Rock", "(17)(18)Custom" → "Rock/Custom")
                if let Some(last) = resolved.last_mut() {
                    *last = suffix.to_string();
                }
            }
            return resolved.join("/");
        }

        // Could not parse any group — return as-is
        return trimmed.to_string();
    }

    // Pattern 2: bare number like "17"
    if let Ok(genre_id) = trimmed.parse::<u16>() {
        if let Some(genre_name) = u8::try_from(genre_id)
            .ok()
            .and_then(|id| constants::get_genre(id))
        {
            return genre_name.to_string();
        }
        // Numeric but outside the standard genre table — return as-is
        return trimmed.to_string();
    }

    // Pattern 3: already a text genre — return unchanged
    trimmed.to_string()
}

// ---------------------------------------------------------------------------
// Date format normalization
// ---------------------------------------------------------------------------

/// Trim whitespace from a date value.
///
/// Currently performs a trim-only pass-through: the date string is returned
/// unchanged except for leading/trailing whitespace removal. The `_source_format`
/// parameter is accepted for future use but does not affect output.
pub fn normalize_date(raw: &str, _source_format: TagSystem) -> String {
    trace_event!(raw = %raw, "normalizing date value");

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Already in a reasonable format — pass through
    trimmed.to_string()
}

// ---------------------------------------------------------------------------
// Boolean field normalization
// ---------------------------------------------------------------------------

/// Normalize a boolean-like value to a canonical `"1"` or `"0"` string.
///
/// Different formats represent compilation flags differently:
/// - ID3v2 / Vorbis: `"1"` or `"0"`
/// - MP4: boolean atom (represented as `"true"` / `"false"` when stringified)
///
/// This function accepts common truthy/falsy representations and normalizes
/// them to `"1"` (true) or `"0"` (false).
pub fn normalize_boolean(raw: &str, _source_format: TagSystem) -> String {
    trace_event!(raw = %raw, "normalizing boolean value");

    let lower = raw.trim().to_lowercase();
    match lower.as_str() {
        "1" | "true" | "yes" => "1".to_string(),
        "0" | "false" | "no" | "" => "0".to_string(),
        // Unrecognized values are returned unchanged to avoid silent data loss
        _ => raw.trim().to_string(),
    }
}
