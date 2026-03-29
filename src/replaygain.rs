//! ReplayGain utility module for loudness normalization metadata.
//!
//! Provides a unified interface for reading and writing ReplayGain metadata
//! across different audio formats (MP3, FLAC, Ogg Vorbis, Ogg Opus, etc.).
//!
//! # Overview
//!
//! ReplayGain is a standard for measuring and storing loudness information
//! in audio files. It uses an 89 dB SPL reference level and stores both
//! track gain (for individual song normalization) and album gain (for
//! preserving relative loudness within an album).
//!
//! # Quick Start
//!
//! ```ignore
//! // Note: This example requires actual audio files on the filesystem.
//! use audex::replaygain::{ReplayGainInfo, from_vorbis_comments};
//! use audex::flac::FLAC;
//! use audex::FileType;
//!
//! // Read ReplayGain from a FLAC file
//! let flac = FLAC::load("song.flac")?;
//! // Convert VCommentDict to HashMap for reading ReplayGain
//! let tags_map: std::collections::HashMap<String, Vec<String>> =
//!     flac.tags.as_ref().unwrap().iter()
//!         .map(|(k, v)| (k.clone(), v.clone()))
//!         .collect();
//! let rg_info = from_vorbis_comments(&tags_map);
//!
//! if let Some(track_gain) = rg_info.track_gain() {
//!     println!("Track gain: {:.2} dB", track_gain);
//! }
//! ```
//!
//! # Format-Specific Storage
//!
//! Different audio formats store ReplayGain information differently:
//!
//! ## FLAC, Ogg Vorbis, Ogg Opus (Vorbis Comments)
//!
//! These formats use Vorbis Comment tags with standardized field names:
//!
//! ```ignore
//! // Note: This example requires actual audio files on the filesystem.
//! use audex::flac::FLAC;
//! use audex::replaygain::{ReplayGainInfo, to_vorbis_comments};
//!
//! let mut flac = FLAC::load("song.flac")?;
//!
//! // Create ReplayGain information
//! let rg = ReplayGainInfo::with_both(
//!     -3.5,  // Track gain in dB
//!     0.95,  // Track peak (0.0 to 1.0+)
//!     -5.0,  // Album gain in dB
//!     0.98,  // Album peak
//! );
//!
//! // Note: FLAC uses VCommentDict which may require conversion
//! // to HashMap<String, Vec<String>> for this function
//!
//! flac.save()?;
//! ```
//!
//! The following Vorbis Comment fields are used:
//! - `REPLAYGAIN_TRACK_GAIN` - Track gain in dB (e.g., "+3.50 dB")
//! - `REPLAYGAIN_TRACK_PEAK` - Track peak amplitude (e.g., "0.995117")
//! - `REPLAYGAIN_ALBUM_GAIN` - Album gain in dB
//! - `REPLAYGAIN_ALBUM_PEAK` - Album peak amplitude
//! - `REPLAYGAIN_REFERENCE_LOUDNESS` - Reference level (usually 89.0 dB SPL)
//!
//! ## MP3 (ID3v2 TXXX Frames)
//!
//! ReplayGain can also be stored in ID3v2 tags using TXXX frames:
//!
//! ```ignore
//! // Note: This example requires actual audio files on the filesystem.
//! use audex::mp3::MP3;
//! use audex::id3::{ID3Tags, Frame};
//! use audex::replaygain::{format_gain, format_peak};
//! use audex::FileType;
//!
//! let mut mp3 = MP3::load("song.mp3")?;
//!
//! // Write ReplayGain as ID3v2 TXXX frames
//! // Access ID3 tags and add UserText frames for ReplayGain
//! // The format_gain and format_peak functions help format values correctly
//! let track_gain_str = format_gain(-3.5);  // "-3.50 dB"
//! let track_peak_str = format_peak(0.95);  // "0.950000"
//!
//! mp3.save()?;
//! ```
//!
//! # Advanced Examples
//!
//! ## Calculating Volume Adjustment
//!
//! Convert ReplayGain dB values to volume multipliers:
//!
//! ```rust
//! use audex::replaygain::ReplayGainInfo;
//!
//! let rg = ReplayGainInfo::with_track(-3.5, 0.95).unwrap();
//!
//! // Get adjustment factor for playback
//! if let Some(factor) = rg.track_adjustment_factor() {
//!     println!("Multiply audio samples by {:.3} for normalization", factor);
//!     // -3.5 dB = ~0.708x volume multiplier
//! }
//! ```
//!
//! ## Batch Processing Album ReplayGain
//!
//! Calculate and apply album ReplayGain across multiple tracks:
//!
//! ```ignore
//! // Note: This example requires actual audio files on the filesystem.
//! use audex::replaygain::{ReplayGainInfo, to_vorbis_comments};
//! use audex::flac::FLAC;
//! use audex::FileType;
//!
//! // Track information with individual gains
//! let tracks = vec![
//!     ("track1.flac", -2.5, 0.90),
//!     ("track2.flac", -3.5, 0.95),
//!     ("track3.flac", -1.5, 0.85),
//! ];
//!
//! // Album-wide values (calculated externally)
//! let album_gain = -4.0;
//! let album_peak = 0.95;
//!
//! // Apply to all tracks in the album
//! for (path, track_gain, track_peak) in tracks {
//!     let rg = ReplayGainInfo::with_both(
//!         track_gain, track_peak,
//!         album_gain, album_peak,
//!     );
//!
//!     // Note: Use ReplayGain info with format-specific tag implementations
//! }
//! ```
//!
//! ## Clearing ReplayGain Metadata
//!
//! Remove all ReplayGain information from a file:
//!
//! ```ignore
//! // Note: This example requires actual audio files on the filesystem.
//! use audex::replaygain::clear_vorbis_comments;
//! use audex::oggvorbis::OggVorbis;
//! use audex::FileType;
//!
//! let mut vorbis = OggVorbis::load("song.ogg")?;
//!
//! // Note: clear_vorbis_comments works with HashMap<String, Vec<String>>
//! // For OggVorbis tags (VCommentDict), use the Tags trait methods instead
//!
//! vorbis.save()?;
//! ```
//!
//! # Reference
//!
//! - [ReplayGain Specification](http://wiki.hydrogenaud.io/index.php?title=ReplayGain_specification)
//! - [EBU R 128 Loudness](https://tech.ebu.ch/docs/r/r128.pdf) (newer alternative)

use crate::{AudexError, Result};
use std::fmt;

/// Validate that a floating-point value is finite (not NaN or Infinity).
/// Returns an error instead of panicking so callers can handle it gracefully.
fn validate_finite(value: f32, label: &str) -> std::result::Result<(), AudexError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(AudexError::InvalidData(format!(
            "{} must be finite, got: {}",
            label, value
        )))
    }
}

/// Standard ReplayGain reference level in dB SPL
pub const REPLAYGAIN_REFERENCE_LEVEL: f32 = 89.0;

/// ReplayGain information for a single track or album
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(into = "ReplayGainInfoRaw"))]
pub struct ReplayGainInfo {
    /// Track gain in dB relative to 89 dB SPL.
    /// Use [`set_track_gain`](Self::set_track_gain) to modify; direct field
    /// access is intentionally private to enforce finiteness validation.
    track_gain: Option<f32>,
    /// Track peak amplitude (0.0 to 1.0+)
    track_peak: Option<f32>,
    /// Album gain in dB relative to 89 dB SPL
    album_gain: Option<f32>,
    /// Album peak amplitude (0.0 to 1.0+)
    album_peak: Option<f32>,
    /// Reference loudness level in dB SPL (typically 89.0)
    reference_level: f32,
    /// Diagnostic messages for fields that could not be parsed.
    ///
    /// Populated by [`from_vorbis_comments`] when individual ReplayGain
    /// fields are present but contain unparseable or rejected values.
    /// Empty when all fields parse successfully.
    pub warnings: Vec<String>,
}

/// Raw helper struct used for serde deserialization with finiteness validation.
/// The derived Deserialize populates this struct, then `TryFrom` validates all
/// float fields before producing the validated `ReplayGainInfo`.
#[cfg(feature = "serde")]
#[derive(serde::Deserialize, serde::Serialize)]
struct ReplayGainInfoRaw {
    track_gain: Option<f32>,
    track_peak: Option<f32>,
    album_gain: Option<f32>,
    album_peak: Option<f32>,
    reference_level: f32,
}

#[cfg(feature = "serde")]
impl From<ReplayGainInfo> for ReplayGainInfoRaw {
    fn from(info: ReplayGainInfo) -> Self {
        Self {
            track_gain: info.track_gain,
            track_peak: info.track_peak,
            album_gain: info.album_gain,
            album_peak: info.album_peak,
            reference_level: info.reference_level,
        }
    }
}

#[cfg(feature = "serde")]
impl TryFrom<ReplayGainInfoRaw> for ReplayGainInfo {
    type Error = String;

    fn try_from(raw: ReplayGainInfoRaw) -> std::result::Result<Self, Self::Error> {
        // Validate that all float values are finite (reject NaN and Infinity)
        if let Some(v) = raw.track_gain {
            if !v.is_finite() {
                return Err(format!("track_gain must be finite, got: {}", v));
            }
        }
        if let Some(v) = raw.track_peak {
            if !v.is_finite() {
                return Err(format!("track_peak must be finite, got: {}", v));
            }
        }
        if let Some(v) = raw.album_gain {
            if !v.is_finite() {
                return Err(format!("album_gain must be finite, got: {}", v));
            }
        }
        if let Some(v) = raw.album_peak {
            if !v.is_finite() {
                return Err(format!("album_peak must be finite, got: {}", v));
            }
        }
        if !raw.reference_level.is_finite() {
            return Err(format!(
                "reference_level must be finite, got: {}",
                raw.reference_level
            ));
        }

        Ok(ReplayGainInfo {
            track_gain: raw.track_gain,
            track_peak: raw.track_peak,
            album_gain: raw.album_gain,
            album_peak: raw.album_peak,
            reference_level: raw.reference_level,
            warnings: Vec::new(),
        })
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ReplayGainInfo {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ReplayGainInfoRaw::deserialize(deserializer)?;
        ReplayGainInfo::try_from(raw).map_err(serde::de::Error::custom)
    }
}

impl Default for ReplayGainInfo {
    fn default() -> Self {
        Self {
            track_gain: None,
            track_peak: None,
            album_gain: None,
            album_peak: None,
            reference_level: REPLAYGAIN_REFERENCE_LEVEL,
            warnings: Vec::new(),
        }
    }
}

impl ReplayGainInfo {
    /// Create a new ReplayGainInfo with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the track gain in dB, if present.
    pub fn track_gain(&self) -> Option<f32> {
        self.track_gain
    }

    /// Returns the track peak amplitude, if present.
    pub fn track_peak(&self) -> Option<f32> {
        self.track_peak
    }

    /// Returns the album gain in dB, if present.
    pub fn album_gain(&self) -> Option<f32> {
        self.album_gain
    }

    /// Returns the album peak amplitude, if present.
    pub fn album_peak(&self) -> Option<f32> {
        self.album_peak
    }

    /// Returns the reference loudness level in dB SPL.
    pub fn reference_level(&self) -> f32 {
        self.reference_level
    }

    /// Create ReplayGainInfo with track gain and peak.
    ///
    /// Returns an error if either value is not finite (NaN or Infinity).
    pub fn with_track(track_gain: f32, track_peak: f32) -> Result<Self> {
        validate_finite(track_gain, "track gain")?;
        validate_finite(track_peak, "track peak")?;
        Ok(Self {
            track_gain: Some(track_gain),
            track_peak: Some(track_peak),
            ..Default::default()
        })
    }

    /// Create ReplayGainInfo with album gain and peak.
    ///
    /// Returns an error if either value is not finite (NaN or Infinity).
    pub fn with_album(album_gain: f32, album_peak: f32) -> Result<Self> {
        validate_finite(album_gain, "album gain")?;
        validate_finite(album_peak, "album peak")?;
        Ok(Self {
            album_gain: Some(album_gain),
            album_peak: Some(album_peak),
            ..Default::default()
        })
    }

    /// Create ReplayGainInfo with both track and album information.
    ///
    /// Returns an error if any value is not finite (NaN or Infinity).
    pub fn with_both(
        track_gain: f32,
        track_peak: f32,
        album_gain: f32,
        album_peak: f32,
    ) -> Result<Self> {
        validate_finite(track_gain, "track gain")?;
        validate_finite(track_peak, "track peak")?;
        validate_finite(album_gain, "album gain")?;
        validate_finite(album_peak, "album peak")?;
        Ok(Self {
            track_gain: Some(track_gain),
            track_peak: Some(track_peak),
            album_gain: Some(album_gain),
            album_peak: Some(album_peak),
            reference_level: REPLAYGAIN_REFERENCE_LEVEL,
            warnings: Vec::new(),
        })
    }

    /// Check if any ReplayGain information is present
    pub fn has_info(&self) -> bool {
        self.track_gain.is_some()
            || self.track_peak.is_some()
            || self.album_gain.is_some()
            || self.album_peak.is_some()
    }

    /// Check if track ReplayGain information is complete
    pub fn has_track_info(&self) -> bool {
        self.track_gain.is_some() && self.track_peak.is_some()
    }

    /// Check if album ReplayGain information is complete
    pub fn has_album_info(&self) -> bool {
        self.album_gain.is_some() && self.album_peak.is_some()
    }

    /// Calculate the volume adjustment factor for track gain
    /// Returns a multiplier (1.0 = no change, < 1.0 = quieter, > 1.0 = louder)
    pub fn track_adjustment_factor(&self) -> Option<f32> {
        // Filter non-finite values that may have bypassed constructor validation
        self.track_gain
            .filter(|g| g.is_finite())
            .map(|gain| 10.0_f32.powf(gain / 20.0))
    }

    /// Calculate the volume adjustment factor for album gain
    /// Returns a multiplier (1.0 = no change, < 1.0 = quieter, > 1.0 = louder)
    pub fn album_adjustment_factor(&self) -> Option<f32> {
        // Filter non-finite values that may have bypassed constructor validation
        self.album_gain
            .filter(|g| g.is_finite())
            .map(|gain| 10.0_f32.powf(gain / 20.0))
    }

    /// Set track gain with finiteness validation.
    /// Prefer this over direct field assignment to ensure data integrity.
    pub fn set_track_gain(&mut self, gain: Option<f32>) -> Result<()> {
        if let Some(g) = gain {
            validate_finite(g, "track gain")?;
        }
        self.track_gain = gain;
        Ok(())
    }

    /// Set track peak with finiteness validation.
    /// Prefer this over direct field assignment to ensure data integrity.
    pub fn set_track_peak(&mut self, peak: Option<f32>) -> Result<()> {
        if let Some(p) = peak {
            validate_finite(p, "track peak")?;
        }
        self.track_peak = peak;
        Ok(())
    }

    /// Set album gain with finiteness validation.
    /// Prefer this over direct field assignment to ensure data integrity.
    pub fn set_album_gain(&mut self, gain: Option<f32>) -> Result<()> {
        if let Some(g) = gain {
            validate_finite(g, "album gain")?;
        }
        self.album_gain = gain;
        Ok(())
    }

    /// Set album peak with finiteness validation.
    /// Prefer this over direct field assignment to ensure data integrity.
    pub fn set_album_peak(&mut self, peak: Option<f32>) -> Result<()> {
        if let Some(p) = peak {
            validate_finite(p, "album peak")?;
        }
        self.album_peak = peak;
        Ok(())
    }

    /// Set reference level with finiteness validation.
    /// Prefer this over direct field assignment to ensure data integrity.
    pub fn set_reference_level(&mut self, level: f32) -> Result<()> {
        validate_finite(level, "reference level")?;
        self.reference_level = level;
        Ok(())
    }

    /// Clear all ReplayGain information
    pub fn clear(&mut self) {
        self.track_gain = None;
        self.track_peak = None;
        self.album_gain = None;
        self.album_peak = None;
        self.reference_level = REPLAYGAIN_REFERENCE_LEVEL;
    }
}

impl fmt::Display for ReplayGainInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ReplayGain [")?;

        if let Some(gain) = self.track_gain {
            write!(f, "track: {:.2} dB", gain)?;
            if let Some(peak) = self.track_peak {
                write!(f, " (peak: {:.6})", peak)?;
            }
        }

        if self.track_gain.is_some() && self.album_gain.is_some() {
            write!(f, ", ")?;
        }

        if let Some(gain) = self.album_gain {
            write!(f, "album: {:.2} dB", gain)?;
            if let Some(peak) = self.album_peak {
                write!(f, " (peak: {:.6})", peak)?;
            }
        }

        write!(f, "]")
    }
}

/// Parse a ReplayGain value from a string
///
/// Accepts formats like:
/// - "+3.5 dB"
/// - "-2.1 dB"
/// - "3.5"
/// - "-2.1"
pub fn parse_gain(s: &str) -> Result<f32> {
    let s = s.trim();

    // Remove " dB" suffix if present
    let s = if let Some(stripped) = s.strip_suffix(" dB") {
        stripped
    } else if let Some(stripped) = s.strip_suffix("dB") {
        stripped
    } else {
        s
    };

    let value = s
        .trim()
        .parse::<f32>()
        .map_err(|_| AudexError::InvalidData(format!("Invalid gain value: {}", s)))?;

    // Reject non-finite values that would corrupt volume calculations
    if !value.is_finite() {
        return Err(AudexError::InvalidData(format!(
            "Gain value must be finite, got: {}",
            value
        )));
    }
    Ok(value)
}

/// Parse a peak value from a string
///
/// Accepts formats like:
/// - "0.995117"
/// - "1.0"
/// - "0.5"
pub fn parse_peak(s: &str) -> Result<f32> {
    let s = s.trim();

    let value = s
        .parse::<f32>()
        .map_err(|_| AudexError::InvalidData(format!("Invalid peak value: {}", s)))?;

    if !value.is_finite() {
        return Err(AudexError::InvalidData(format!(
            "Peak value must be finite, got: {}",
            value
        )));
    }
    Ok(value)
}

/// Format a gain value as a string for storage.
///
/// Returns an error if the value is not finite (NaN or Infinity).
pub fn format_gain(gain: f32) -> Result<String> {
    if !gain.is_finite() {
        return Err(AudexError::InvalidData(format!(
            "Gain value must be finite, got: {}",
            gain
        )));
    }
    Ok(format!("{:+.2} dB", gain))
}

/// Format a peak value as a string for storage.
///
/// Returns an error if the value is not finite (NaN or Infinity).
pub fn format_peak(peak: f32) -> Result<String> {
    if !peak.is_finite() {
        return Err(AudexError::InvalidData(format!(
            "Peak value must be finite, got: {}",
            peak
        )));
    }
    Ok(format!("{:.6}", peak))
}

/// Vorbis Comment ReplayGain key names
pub mod vorbis_keys {
    /// Track gain key
    pub const TRACK_GAIN: &str = "REPLAYGAIN_TRACK_GAIN";
    /// Track peak key
    pub const TRACK_PEAK: &str = "REPLAYGAIN_TRACK_PEAK";
    /// Album gain key
    pub const ALBUM_GAIN: &str = "REPLAYGAIN_ALBUM_GAIN";
    /// Album peak key
    pub const ALBUM_PEAK: &str = "REPLAYGAIN_ALBUM_PEAK";
    /// Reference loudness key
    pub const REFERENCE_LOUDNESS: &str = "REPLAYGAIN_REFERENCE_LOUDNESS";
}

/// Extract ReplayGain information from Vorbis Comments.
///
/// This works with FLAC, Ogg Vorbis, Ogg Opus, and other formats
/// that use Vorbis-style comments.
///
/// Parse failures for individual fields are collected into the
/// `warnings` field of the returned [`ReplayGainInfo`]. Callers
/// that need to surface these diagnostics can inspect
/// `info.warnings` after the call.
pub fn from_vorbis_comments(
    comments: &std::collections::HashMap<String, Vec<String>>,
) -> ReplayGainInfo {
    let mut info = ReplayGainInfo::default();

    // Helper to get first value from a key (case-insensitive)
    let get_value = |key: &str| -> Option<&String> {
        comments
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .and_then(|(_, v)| v.first())
    };

    // Parse track gain — route through setter to enforce finiteness invariant
    if let Some(gain_str) = get_value(vorbis_keys::TRACK_GAIN) {
        match parse_gain(gain_str) {
            Ok(gain) => {
                if let Err(_e) = info.set_track_gain(Some(gain)) {
                    let msg = format!("{}: setter rejected value: {}", vorbis_keys::TRACK_GAIN, _e);
                    warn_event!(key = vorbis_keys::TRACK_GAIN, error = %_e, "ReplayGain setter rejected parsed value");
                    info.warnings.push(msg);
                }
            }
            Err(_) => {
                let msg = format!(
                    "{}: unparseable value: {}",
                    vorbis_keys::TRACK_GAIN,
                    gain_str
                );
                warn_event!(key = vorbis_keys::TRACK_GAIN, value = %gain_str, "unparseable ReplayGain field");
                info.warnings.push(msg);
            }
        }
    }

    // Parse track peak — route through setter to enforce finiteness invariant
    if let Some(peak_str) = get_value(vorbis_keys::TRACK_PEAK) {
        match parse_peak(peak_str) {
            Ok(peak) => {
                if let Err(_e) = info.set_track_peak(Some(peak)) {
                    let msg = format!("{}: setter rejected value: {}", vorbis_keys::TRACK_PEAK, _e);
                    warn_event!(key = vorbis_keys::TRACK_PEAK, error = %_e, "ReplayGain setter rejected parsed value");
                    info.warnings.push(msg);
                }
            }
            Err(_) => {
                let msg = format!(
                    "{}: unparseable value: {}",
                    vorbis_keys::TRACK_PEAK,
                    peak_str
                );
                warn_event!(key = vorbis_keys::TRACK_PEAK, value = %peak_str, "unparseable ReplayGain field");
                info.warnings.push(msg);
            }
        }
    }

    // Parse album gain — route through setter to enforce finiteness invariant
    if let Some(gain_str) = get_value(vorbis_keys::ALBUM_GAIN) {
        match parse_gain(gain_str) {
            Ok(gain) => {
                if let Err(_e) = info.set_album_gain(Some(gain)) {
                    let msg = format!("{}: setter rejected value: {}", vorbis_keys::ALBUM_GAIN, _e);
                    warn_event!(key = vorbis_keys::ALBUM_GAIN, error = %_e, "ReplayGain setter rejected parsed value");
                    info.warnings.push(msg);
                }
            }
            Err(_) => {
                let msg = format!(
                    "{}: unparseable value: {}",
                    vorbis_keys::ALBUM_GAIN,
                    gain_str
                );
                warn_event!(key = vorbis_keys::ALBUM_GAIN, value = %gain_str, "unparseable ReplayGain field");
                info.warnings.push(msg);
            }
        }
    }

    // Parse album peak — route through setter to enforce finiteness invariant
    if let Some(peak_str) = get_value(vorbis_keys::ALBUM_PEAK) {
        match parse_peak(peak_str) {
            Ok(peak) => {
                if let Err(_e) = info.set_album_peak(Some(peak)) {
                    let msg = format!("{}: setter rejected value: {}", vorbis_keys::ALBUM_PEAK, _e);
                    warn_event!(key = vorbis_keys::ALBUM_PEAK, error = %_e, "ReplayGain setter rejected parsed value");
                    info.warnings.push(msg);
                }
            }
            Err(_) => {
                let msg = format!(
                    "{}: unparseable value: {}",
                    vorbis_keys::ALBUM_PEAK,
                    peak_str
                );
                warn_event!(key = vorbis_keys::ALBUM_PEAK, value = %peak_str, "unparseable ReplayGain field");
                info.warnings.push(msg);
            }
        }
    }

    // Parse reference level — route through setter to enforce finiteness invariant
    if let Some(ref_str) = get_value(vorbis_keys::REFERENCE_LOUDNESS) {
        match parse_gain(ref_str) {
            Ok(reference) => {
                if let Err(_e) = info.set_reference_level(reference) {
                    let msg = format!(
                        "{}: setter rejected value: {}",
                        vorbis_keys::REFERENCE_LOUDNESS,
                        _e
                    );
                    warn_event!(key = vorbis_keys::REFERENCE_LOUDNESS, error = %_e, "ReplayGain setter rejected parsed value");
                    info.warnings.push(msg);
                }
            }
            Err(_) => {
                let msg = format!(
                    "{}: unparseable value: {}",
                    vorbis_keys::REFERENCE_LOUDNESS,
                    ref_str
                );
                warn_event!(key = vorbis_keys::REFERENCE_LOUDNESS, value = %ref_str, "unparseable ReplayGain field");
                info.warnings.push(msg);
            }
        }
    }

    info
}

/// Apply ReplayGain information to Vorbis Comments
///
/// This will update the provided HashMap with ReplayGain keys.
/// Only non-None values will be written. Returns an error if any
/// value is not finite (NaN or Infinity).
pub fn to_vorbis_comments(
    info: &ReplayGainInfo,
    comments: &mut std::collections::HashMap<String, Vec<String>>,
) -> Result<()> {
    // Helper to set a value (removes existing and adds new)
    let set_value =
        |key: &str, value: String, map: &mut std::collections::HashMap<String, Vec<String>>| {
            map.insert(key.to_uppercase(), vec![value]);
        };

    // Write track gain
    if let Some(gain) = info.track_gain {
        set_value(vorbis_keys::TRACK_GAIN, format_gain(gain)?, comments);
    }

    // Write track peak
    if let Some(peak) = info.track_peak {
        set_value(vorbis_keys::TRACK_PEAK, format_peak(peak)?, comments);
    }

    // Write album gain
    if let Some(gain) = info.album_gain {
        set_value(vorbis_keys::ALBUM_GAIN, format_gain(gain)?, comments);
    }

    // Write album peak
    if let Some(peak) = info.album_peak {
        set_value(vorbis_keys::ALBUM_PEAK, format_peak(peak)?, comments);
    }

    // Reject non-finite reference level — NaN would silently pass the
    // difference check below since NaN comparisons always return false
    if !info.reference_level.is_finite() {
        return Err(AudexError::InvalidData(format!(
            "Reference level must be finite, got: {}",
            info.reference_level
        )));
    }

    // Write reference level if not default
    if (info.reference_level - REPLAYGAIN_REFERENCE_LEVEL).abs() > 0.01 {
        set_value(
            vorbis_keys::REFERENCE_LOUDNESS,
            format_gain(info.reference_level)?,
            comments,
        );
    }

    Ok(())
}

/// Remove ReplayGain information from Vorbis Comments
pub fn clear_vorbis_comments(comments: &mut std::collections::HashMap<String, Vec<String>>) {
    // Remove all ReplayGain keys (case-insensitive)
    let keys_to_remove: Vec<String> = comments
        .keys()
        .filter(|k| {
            k.eq_ignore_ascii_case(vorbis_keys::TRACK_GAIN)
                || k.eq_ignore_ascii_case(vorbis_keys::TRACK_PEAK)
                || k.eq_ignore_ascii_case(vorbis_keys::ALBUM_GAIN)
                || k.eq_ignore_ascii_case(vorbis_keys::ALBUM_PEAK)
                || k.eq_ignore_ascii_case(vorbis_keys::REFERENCE_LOUDNESS)
        })
        .cloned()
        .collect();

    for key in keys_to_remove {
        comments.remove(&key);
    }
}
