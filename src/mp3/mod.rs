//! # MP3 Format Support
//!
//! This module provides comprehensive support for reading and writing MPEG Layer III (MP3) audio files.
//!
//! ## Overview
//!
//! The MP3 format is one of the most widely used audio compression formats. This module supports:
//! - **MPEG-1, MPEG-2, and MPEG-2.5** audio versions
//! - **Layer I, II, and III** encoding (with primary focus on Layer III/MP3)
//! - **Multiple tagging formats**:
//!   - ID3v2 (versions 2.2, 2.3, and 2.4) - Primary tagging format
//!   - ID3v1 and ID3v1.1 - Legacy tagging format
//! - **Bitrate detection**: Constant Bitrate (CBR), Variable Bitrate (VBR), and Average Bitrate (ABR)
//! - **VBR header support**:
//!   - Xing/Info headers (used by LAME and other encoders)
//!   - VBRI headers (Fraunhofer encoder format)
//!   - Both provide accurate duration calculation and seeking support
//! - **LAME encoder information** extraction including ReplayGain values
//!
//! ## Supported Features
//!
//! - Read/write audio properties (bitrate, sample rate, channels, duration)
//! - Read/write metadata tags (artist, title, album, etc.)
//! - Support for embedded artwork via ID3v2 APIC frames
//! - Accurate duration calculation for VBR files using Xing/VBRI headers
//! - Multiple tag format handling in a single file
//!
//! ## Basic Usage
//!
//! ```no_run
//! use audex::mp3::MP3;
//! use audex::{FileType, Tags};
//!
//! // Load an MP3 file
//! let mut mp3 = MP3::load("song.mp3").unwrap();
//!
//! // Access audio information
//! if let Some(length) = mp3.info.length {
//!     println!("Duration: {:.2} seconds", length.as_secs_f64());
//! }
//! println!("Bitrate: {} kbps", mp3.info.bitrate / 1000);
//! println!("Sample rate: {} Hz", mp3.info.sample_rate);
//!
//! // Read ID3 tags using the Tags trait
//! if let Some(ref tags) = mp3.tags {
//!     if let Some(title) = tags.get("TIT2") {
//!         println!("Title: {:?}", title);
//!     }
//! }
//!
//! // Modify tags and save
//! if let Some(ref mut tags) = mp3.tags {
//!     tags.set("TIT2", vec!["New Title".to_string()]);
//! }
//! mp3.save().unwrap();
//! ```
//!
//! ## Limitations and Known Issues
//!
//! - **Read-only audio properties**: This library does not support re-encoding audio data.
//!   Only metadata can be modified.
//! - **Tag preservation**: When saving, the library preserves all existing tag formats by default.
//!   Use the `clear()` method to remove specific tag types if needed.
//! - **Memory usage**: Large MP3 files with extensive ID3v2 tags (especially with embedded artwork)
//!   will be fully loaded into memory.
//! - **MPEG Layer I/II**: While basic support exists, this module is primarily tested with Layer III (MP3) files.
//!
//! ## VBR Header Types
//!
//! This module supports two VBR header formats for accurate duration and seeking:
//!
//! - **Xing/Info headers** (`XingHeader`): Used by LAME and most encoders. The "Xing"
//!   identifier is used for VBR files, while "Info" is used for CBR files encoded by LAME.
//! - **VBRI headers** (`VBRIHeader`): Fraunhofer's alternative format. Less common but
//!   still encountered in files from Fraunhofer-based encoders.
//!
//! Both header types are automatically detected and parsed. The higher-level `MP3` type
//! handles this transparently.
//!
//! ## See Also
//!
//! - `MP3` - Main struct for MP3 file handling
//! - `MPEGInfo` - Audio stream information
//! - `EasyMP3` - Simplified interface for common tagging operations
//! - `XingHeader` - Xing/Info VBR header structure
//! - `VBRIHeader` - VBRI VBR header structure (Fraunhofer format)
//! - `LAMEHeader` - Extended LAME encoder information
//! - `MPEGFrame` - Low-level MPEG frame structure
//! - `BitrateMode` - CBR/VBR/ABR detection
//! - [`crate::id3`] - ID3 tag format support

use crate::AudexError;
use std::fmt;

pub use file::{EasyMP3, MP3, MPEGInfo};
pub use util::{BitrateMode, LAMEHeader, MPEGFrame, VBRIHeader, XingHeader, iter_sync, skip_id3};

pub mod file;
pub mod util;

/// MP3-specific error type for handling MP3 format errors.
///
/// This error type represents general MP3 format errors that can occur during
/// file parsing, validation, or processing. It automatically converts to
/// [`AudexError`] for use with the library's error handling system.
///
/// # Examples
///
/// ```
/// use audex::mp3::Error;
/// use audex::AudexError;
///
/// let mp3_error = Error {
///     message: "Invalid frame sync".to_string(),
/// };
///
/// // Automatically converts to AudexError
/// let audex_error: AudexError = mp3_error.into();
/// ```
#[derive(Debug, Clone)]
pub struct Error {
    /// Human-readable error message describing what went wrong
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MP3 error: {}", self.message)
    }
}

impl std::error::Error for Error {}

impl From<Error> for AudexError {
    fn from(err: Error) -> Self {
        AudexError::InvalidData(err.message)
    }
}

/// Error indicating that a required MPEG header could not be found.
///
/// This error occurs when the parser cannot locate a valid MPEG frame header
/// in the expected position within the file. This typically indicates either:
/// - The file is not a valid MP3 file
/// - The file is corrupted
/// - The file contains excessive leading data before the first MPEG frame
///
/// # Examples
///
/// ```
/// use audex::mp3::HeaderNotFoundError;
///
/// let error = HeaderNotFoundError {
///     message: "No valid MPEG header found in first 8KB".to_string(),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct HeaderNotFoundError {
    /// Description of where the header search failed
    pub message: String,
}

impl fmt::Display for HeaderNotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Header not found: {}", self.message)
    }
}

impl std::error::Error for HeaderNotFoundError {}

impl From<HeaderNotFoundError> for Error {
    fn from(_err: HeaderNotFoundError) -> Self {
        Error {
            message: _err.message,
        }
    }
}

impl From<HeaderNotFoundError> for AudexError {
    fn from(_err: HeaderNotFoundError) -> Self {
        AudexError::HeaderNotFound
    }
}

/// Error indicating that an MPEG header was found but contains invalid data.
///
/// This error occurs when a potential MPEG header is detected (valid sync bits)
/// but the header data itself is malformed or contains invalid values. Common causes:
/// - Invalid bitrate index (all 1s or all 0s in bitrate field)
/// - Invalid sample rate index
/// - Reserved MPEG version or layer values
/// - Inconsistent header values across frames
///
/// # Examples
///
/// ```
/// use audex::mp3::InvalidMPEGHeader;
///
/// let error = InvalidMPEGHeader {
///     message: "Invalid bitrate index: 15".to_string(),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct InvalidMPEGHeader {
    /// Details about why the header is considered invalid
    pub message: String,
}

impl fmt::Display for InvalidMPEGHeader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid MPEG header: {}", self.message)
    }
}

impl std::error::Error for InvalidMPEGHeader {}

impl From<InvalidMPEGHeader> for Error {
    fn from(err: InvalidMPEGHeader) -> Self {
        Error {
            message: err.message,
        }
    }
}

impl From<InvalidMPEGHeader> for AudexError {
    fn from(err: InvalidMPEGHeader) -> Self {
        AudexError::InvalidData(err.message)
    }
}

/// Channel mode constants as defined in the MPEG audio specification.
///
/// These constants represent the four possible channel modes encoded in MPEG audio frame headers.
/// Stereo mode - Two independent channels (left and right).
///
/// Both channels contain different audio data with no inter-channel compression.
pub const STEREO: i32 = 0;

/// Joint stereo mode - Channels share some information for better compression.
///
/// Uses mid/side stereo or intensity stereo encoding to exploit inter-channel redundancy.
/// Common in modern MP3 encoders for improved compression efficiency.
pub const JOINTSTEREO: i32 = 1;

/// Dual channel mode - Two independent mono channels.
///
/// Similar to stereo but typically used for bilingual or multi-language audio
/// where each channel contains a separate mono program.
pub const DUALCHANNEL: i32 = 2;

/// Mono mode - Single channel audio.
///
/// The audio stream contains only one channel of audio data.
pub const MONO: i32 = 3;

/// Determines the bitrate mode (CBR/VBR/ABR) from a Xing/Info header.
///
/// This function analyzes the Xing or Info header to determine how the MP3 file
/// was encoded. The detection logic uses several heuristics:
///
/// 1. **LAME header VBR method field** (if present) - Most reliable indicator
/// 2. **Info vs Xing tag** - Info tags are only written for CBR files
/// 3. **VBR scale presence** - Indicates variable bitrate encoding
/// 4. **LAME version string** - Older LAME versions using VBR
///
/// # Arguments
///
/// * `xing` - Reference to a parsed Xing or Info header
///
/// # Returns
///
/// Returns the detected [`BitrateMode`], or `BitrateMode::Unknown` if the mode
/// cannot be reliably determined.
///
/// # Implementation Notes
///
/// The LAME encoder uses different VBR method codes:
/// - 1, 8: Constant Bitrate (CBR)
/// - 2, 9: Average Bitrate (ABR)
/// - 3-6: Variable Bitrate (VBR) with different quality settings
#[allow(dead_code)]
pub(crate) fn guess_xing_bitrate_mode(xing: &XingHeader) -> BitrateMode {
    if let Some(lame) = &xing.lame_header {
        match lame.vbr_method {
            1 | 8 => return BitrateMode::CBR,
            2 | 9 => return BitrateMode::ABR,
            3..=6 => return BitrateMode::VBR,
            _ => {} // Continue guessing
        }
    }

    // Info tags get written by LAME only for CBR files
    if xing.is_info {
        return BitrateMode::CBR;
    }

    // Older LAME and non-LAME with some variant of VBR
    if xing.vbr_scale != -1 || !xing.lame_version_desc.is_empty() {
        return BitrateMode::VBR;
    }

    BitrateMode::Unknown
}

/// Clear (remove) all ID3 tags from an MP3 file
///
/// Removes both ID3v1 and ID3v2 tags from the file.
///
/// # Arguments
/// * `filething` - Path to the MP3 file
///
/// # Example
/// ```no_run
/// use audex::mp3;
///
/// mp3::clear("song.mp3").unwrap();
/// ```
pub fn clear<P: AsRef<std::path::Path>>(filething: P) -> Result<(), AudexError> {
    // Clear both ID3v1 and ID3v2 tags using the id3 module's clear function
    crate::id3::ID3Tags::clear_file(filething, true, true)
}

/// Clear (remove) ID3 tags from an MP3 file with options
///
/// # Arguments
/// * `filething` - Path to the MP3 file
/// * `clear_v1` - Whether to clear ID3v1 tags
/// * `clear_v2` - Whether to clear ID3v2 tags
///
/// # Example
/// ```no_run
/// use audex::mp3;
///
/// // Clear only ID3v2 tags, keep ID3v1
/// mp3::clear_with_options("song.mp3", false, true).unwrap();
/// ```
pub fn clear_with_options<P: AsRef<std::path::Path>>(
    filething: P,
    clear_v1: bool,
    clear_v2: bool,
) -> Result<(), AudexError> {
    // Clear ID3 tags with specific options
    crate::id3::ID3Tags::clear_file(filething, clear_v1, clear_v2)
}

/// Native async version of [`clear`]. Removes both ID3v1 and ID3v2 tags
/// from an MP3 file using tokio I/O.
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<std::path::Path>>(filething: P) -> Result<(), AudexError> {
    crate::id3::file::clear_async(filething.as_ref(), true, true).await
}

/// MPEG audio version as specified in the frame header.
///
/// MPEG audio has three standardized versions, each with different
/// supported sample rates and features:
///
/// - **MPEG-1**: Original standard, supports 32/44.1/48 kHz
/// - **MPEG-2**: Extension for lower sample rates (16/22.05/24 kHz)
/// - **MPEG-2.5**: Unofficial extension for even lower rates (8/11.025/12 kHz)
///
/// The version affects the supported sample rates, bit reservoir size,
/// and some technical encoding parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MPEGVersion {
    /// MPEG-1 Audio (ISO/IEC 11172-3)
    ///
    /// The original MPEG audio standard. Supports sample rates of
    /// 32 kHz, 44.1 kHz, and 48 kHz.
    MPEG1,

    /// MPEG-2 Audio (ISO/IEC 13818-3)
    ///
    /// Extension to support lower sample rates: 16 kHz, 22.05 kHz, and 24 kHz.
    /// Maintains backward compatibility with MPEG-1 decoders.
    MPEG2,

    /// MPEG-2.5 Audio (Unofficial extension)
    ///
    /// Unofficial extension for very low bitrate encoding.
    /// Supports 8 kHz, 11.025 kHz, and 12 kHz sample rates.
    /// Not part of any official standard but widely supported.
    MPEG25,
}

/// MPEG audio layer as specified in the frame header.
///
/// MPEG audio defines three layers, each representing a different
/// complexity/quality tradeoff:
///
/// - **Layer I**: Simplest, lowest compression, rarely used today
/// - **Layer II**: Medium complexity, used in broadcasting (DAB, DVB)
/// - **Layer III**: Most complex, best compression, the "MP3" format
///
/// Higher layer numbers provide better compression but require more
/// processing power to encode and decode.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MPEGLayer {
    /// MPEG Layer I
    ///
    /// The simplest MPEG audio layer. Uses 384 samples per frame for MPEG-1
    /// (192 for MPEG-2). Provides the lowest compression ratio but fastest
    /// encoding/decoding. Rarely used in practice.
    Layer1,

    /// MPEG Layer II (MP2)
    ///
    /// Medium complexity layer. Uses 1152 samples per frame. Commonly used
    /// in digital broadcasting (DAB radio, DVB television) and Video CD.
    /// Better compression than Layer I with moderate complexity.
    Layer2,

    /// MPEG Layer III (MP3)
    ///
    /// The most complex and widely-used MPEG audio layer. Uses 1152 samples
    /// per frame. Provides the best compression ratio through advanced
    /// psychoacoustic modeling and Huffman coding. This is the "MP3" format.
    Layer3,
}

/// Audio channel configuration mode.
///
/// Specifies how audio channels are encoded in the MPEG stream.
/// The channel mode affects both the number of channels and how
/// they are compressed.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ChannelMode {
    /// Standard stereo - Two independent channels.
    ///
    /// Left and right channels are encoded completely independently
    /// with no inter-channel compression. This provides the best
    /// stereo separation but uses more bits than joint stereo.
    Stereo,

    /// Joint stereo - Channels share information for better compression.
    ///
    /// Exploits similarities between channels using one of two techniques:
    /// - **Mid/Side (M/S) stereo**: Encodes mid (L+R) and side (L-R) signals
    /// - **Intensity stereo**: Shares high-frequency information between channels
    ///
    /// Modern encoders typically use this mode for improved compression efficiency
    /// at the cost of some stereo separation at lower bitrates.
    JointStereo,

    /// Dual channel - Two independent mono programs.
    ///
    /// Similar to stereo but conceptually represents two separate mono channels
    /// rather than a stereo pair. Commonly used for bilingual audio where each
    /// channel contains a different language track.
    DualChannel,

    /// Single channel mono audio.
    ///
    /// The stream contains only one channel of audio. Uses half the bitrate
    /// of stereo modes for the same quality level.
    Mono,
}

/// Pre-emphasis filter applied to the audio signal.
///
/// Emphasis refers to a pre-processing filter that boosts high frequencies
/// during encoding. The decoder applies the inverse filter (de-emphasis) to
/// restore the original frequency balance. This technique reduces high-frequency
/// noise in analog recording systems.
///
/// In practice, emphasis is rarely used in modern digital audio and most
/// MP3 files use `None`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Emphasis {
    /// No emphasis applied.
    ///
    /// This is the standard and most common setting for digital audio sources.
    /// The audio has not been pre-emphasized and requires no de-emphasis on playback.
    None,

    /// 50/15 microsecond emphasis.
    ///
    /// A specific emphasis curve defined in the MPEG standard. The "50/15"
    /// refers to the time constants of the emphasis/de-emphasis filters.
    /// Rarely used in practice except for some legacy content.
    MS50_15,

    /// Reserved value.
    ///
    /// This value is reserved in the MPEG specification and should not appear
    /// in valid files. If encountered, the file may be corrupted or non-compliant.
    Reserved,

    /// CCITT J.17 emphasis.
    ///
    /// An emphasis curve defined by the CCITT (now ITU-T) J.17 standard.
    /// Used in some telecommunications applications but rare in music files.
    CCITT,
}
