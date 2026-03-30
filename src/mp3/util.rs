//! MP3 frame parsing and VBR header handling utilities.
//!
//! This module provides low-level MPEG frame parsing functionality and VBR header support.
//! It handles:
//!
//! - **Frame Parsing**: MPEG audio frame header decoding and validation
//! - **VBR Headers**: Xing/Info and VBRI header parsing for accurate duration
//! - **LAME Headers**: Extended encoder information from LAME-encoded files
//! - **Bitrate Detection**: CBR/VBR/ABR mode identification
//! - **Stream Synchronization**: Finding valid frame sync words in data
//!
//! Most users won't need to use this module directly; the higher-level [`MP3`](super::MP3)
//! type handles these details automatically.

use crate::mp3::{ChannelMode, Emphasis, MPEGLayer, MPEGVersion};
use crate::{AudexError, Result};
use byteorder::{BigEndian, ReadBytesExt};
use std::fmt;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::time::Duration;

/// Type alias for MPEG header parse result tuple
type MPEGHeaderInfo = (
    MPEGVersion,
    MPEGLayer,
    u32,
    u32,
    ChannelMode,
    Emphasis,
    bool,
    bool,
    bool,
    bool,
    bool,
    u8,
);

/// Error type for LAME header parsing failures.
///
/// This error occurs when attempting to parse a LAME encoder info header
/// that is malformed or contains invalid data.
#[derive(Debug, Clone)]
pub struct LAMEError {
    /// Description of the parsing error
    pub message: String,
}

impl fmt::Display for LAMEError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LAME Error: {}", self.message)
    }
}

impl std::error::Error for LAMEError {}

impl LAMEError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

/// Error type for Xing/Info header parsing failures.
///
/// This error occurs when attempting to parse a Xing or Info VBR header
/// that is malformed, incomplete, or contains invalid data.
#[derive(Debug, Clone)]
pub struct XingHeaderError {
    /// Description of the parsing error
    pub message: String,
}

impl fmt::Display for XingHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Xing Header Error: {}", self.message)
    }
}

impl std::error::Error for XingHeaderError {}

impl XingHeaderError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

/// Error type for VBRI header parsing failures.
///
/// This error occurs when attempting to parse a VBRI VBR header (Fraunhofer's
/// VBR header format) that is malformed or contains invalid data.
#[derive(Debug, Clone)]
pub struct VBRIHeaderError {
    /// Description of the parsing error
    pub message: String,
}

impl fmt::Display for VBRIHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VBRI Header Error: {}", self.message)
    }
}

impl std::error::Error for VBRIHeaderError {}

impl VBRIHeaderError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

/// Bit-level reader for precise parsing of MPEG frame headers and VBR data.
///
/// Reads individual bits or groups of bits from a byte buffer, tracking the
/// current byte and bit position. Used internally by LAME and Xing header parsers.
#[derive(Debug)]
pub struct BitReader {
    data: Vec<u8>,
    byte_pos: usize,
    bit_pos: u8, // 0-7, current bit position within the byte
}

impl BitReader {
    pub fn new(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Read specified number of bits
    pub fn bits(&mut self, count: usize) -> std::result::Result<u32, LAMEError> {
        if count == 0 {
            return Ok(0);
        }

        if count > 32 {
            return Err(LAMEError::new("Cannot read more than 32 bits at once"));
        }

        let mut result = 0u32;
        let mut bits_read = 0;

        while bits_read < count {
            if self.byte_pos >= self.data.len() {
                return Err(LAMEError::new("Not enough data"));
            }

            let current_byte = self.data[self.byte_pos];
            let bits_in_current_byte = 8 - self.bit_pos as usize;
            let bits_needed = count - bits_read;
            let bits_to_read = std::cmp::min(bits_in_current_byte, bits_needed);

            // Extract bits from current byte
            let shift = bits_in_current_byte - bits_to_read;
            let mask = if bits_to_read >= 8 {
                0xFF
            } else {
                (1u8 << bits_to_read) - 1
            };
            let bits = (current_byte >> shift) & mask;

            // Add to result
            result = (result << bits_to_read) | (bits as u32);
            bits_read += bits_to_read;

            // Update position
            self.bit_pos += bits_to_read as u8;
            if self.bit_pos >= 8 {
                self.bit_pos = 0;
                self.byte_pos += 1;
            }
        }

        Ok(result)
    }

    /// Read bytes directly
    pub fn bytes(&mut self, count: usize) -> std::result::Result<Vec<u8>, LAMEError> {
        if self.bit_pos != 0 {
            return Err(LAMEError::new(
                "Cannot read bytes from non-byte-aligned position",
            ));
        }

        if self.byte_pos + count > self.data.len() {
            return Err(LAMEError::new("Not enough data"));
        }

        let result = self.data[self.byte_pos..self.byte_pos + count].to_vec();
        self.byte_pos += count;
        Ok(result)
    }

    /// Skip bits
    pub fn skip(&mut self, count: usize) -> std::result::Result<(), LAMEError> {
        self.bits(count)?;
        Ok(())
    }

    /// Check if we're byte-aligned
    pub fn is_aligned(&self) -> bool {
        self.bit_pos == 0
    }
}

/// MPEG audio bitrate encoding mode.
///
/// This enum represents how the bitrate varies across an MP3 file. Different
/// modes provide different tradeoffs between file size, quality consistency,
/// and encoding complexity.
///
/// # Modes
///
/// - **CBR (Constant Bitrate)**: Every frame uses the same bitrate. Predictable
///   file size but may waste space on simple audio or compromise quality on complex passages.
///
/// - **VBR (Variable Bitrate)**: Bitrate varies based on audio complexity. Uses
///   lower bitrates for simple passages and higher for complex ones, optimizing
///   quality-to-size ratio.
///
/// - **ABR (Average Bitrate)**: Targets a specific average bitrate while allowing
///   some variation. Compromise between CBR's predictability and VBR's efficiency.
///
/// - **Unknown**: The mode couldn't be determined (no VBR header or unclear data).
///
/// # Detection
///
/// The bitrate mode is determined by:
/// 1. Xing/Info header VBR method field (LAME encoder)
/// 2. Presence of "Info" tag (indicates CBR in LAME files)
/// 3. VBR scale field presence
/// 4. VBRI header (Fraunhofer encoder)
///
/// # Examples
///
/// ```
/// use audex::mp3::{MP3, BitrateMode};
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mp3 = MP3::from_file("song.mp3")?;
///
/// match mp3.info.bitrate_mode {
///     BitrateMode::CBR => println!("Constant bitrate encoding"),
///     BitrateMode::VBR => println!("Variable bitrate encoding"),
///     BitrateMode::ABR => println!("Average bitrate encoding"),
///     BitrateMode::Unknown => println!("Bitrate mode unknown"),
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BitrateMode {
    /// Bitrate mode could not be determined
    ///
    /// This typically means the file lacks a VBR header (Xing/Info/VBRI)
    /// or the header data is ambiguous.
    #[default]
    Unknown = 0,

    /// Constant Bitrate - All frames use the same bitrate
    ///
    /// Every MPEG frame in the file has the same bitrate. This makes file
    /// size very predictable (filesize = bitrate × duration) but is less
    /// efficient than VBR for variable-complexity audio.
    CBR = 1,

    /// Variable Bitrate - Bitrate changes based on audio complexity
    ///
    /// The encoder adjusts bitrate frame-by-frame to maintain consistent
    /// quality. Simple passages use lower bitrates; complex passages use
    /// higher bitrates. This provides the best quality-to-size ratio.
    VBR = 2,

    /// Average Bitrate - Targets a specific average bitrate
    ///
    /// Similar to VBR but constrains the bitrate to average to a specific
    /// target value. Provides some of VBR's efficiency while maintaining
    /// more predictable file sizes than pure VBR.
    ABR = 3,
}

/// Flags indicating which optional fields are present in a Xing/Info header.
///
/// The Xing/Info header uses a flags field to indicate which optional data
/// is included. These constants can be bitwise-ORed together.
#[derive(Debug, Clone)]
pub struct XingHeaderFlags;

impl XingHeaderFlags {
    /// Frame count field is present (4 bytes)
    pub const FRAMES: u32 = 0x1;

    /// File size in bytes field is present (4 bytes)
    pub const BYTES: u32 = 0x2;

    /// Table of Contents (TOC) for seeking is present (100 bytes)
    pub const TOC: u32 = 0x4;

    /// VBR quality scale field is present (4 bytes)
    pub const VBR_SCALE: u32 = 0x8;
}

// Legacy constants for backward compatibility
const XING_FRAMES_FLAG: u32 = XingHeaderFlags::FRAMES;
const XING_BYTES_FLAG: u32 = XingHeaderFlags::BYTES;
const XING_TOC_FLAG: u32 = XingHeaderFlags::TOC;
const XING_VBR_SCALE_FLAG: u32 = XingHeaderFlags::VBR_SCALE;

/// Xing or Info VBR header data.
///
/// The Xing header (or "Info" header for CBR files) is added by the LAME encoder
/// and other encoders to provide accurate file information for Variable Bitrate
/// (VBR) MP3 files. Without this header, calculating duration and seeking in VBR
/// files would require scanning the entire file.
///
/// # Xing vs Info
///
/// - **"Xing"** header: Used for VBR and ABR files
/// - **"Info"** header: Used for CBR files encoded by LAME
///
/// Both have the same structure; the tag name indicates the encoding mode.
///
/// # Purpose
///
/// - **Accurate duration**: Provides total frame count for precise duration calculation
/// - **Fast seeking**: Contains a 100-entry table of contents for quick seeking
/// - **Encoder info**: May contain LAME encoder version and settings
/// - **Quality indicator**: VBR quality scale from 0-100
///
/// # Examples
///
/// ```
/// use audex::mp3::MP3;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mp3 = MP3::from_file("song.mp3")?;
///
/// // Duration is accurate thanks to Xing header frame count
/// if let Some(duration) = mp3.info.length {
///     println!("Duration: {:?}", duration);
/// }
///
/// // Check encoder information
/// if let Some(encoder) = &mp3.info.encoder_info {
///     println!("Encoded with: {}", encoder);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct XingHeader {
    /// Total number of MPEG frames in the file, or `None` if the FRAMES
    /// flag was not set in the header.
    pub frames: Option<u32>,

    /// Total file size in bytes (excluding ID3 tags), or `None` if the
    /// BYTES flag was not set in the header.
    pub bytes: Option<u32>,

    /// Table of Contents (TOC) for seeking
    ///
    /// A 100-entry array where each value (0-255) represents the byte offset
    /// (as a percentage of file size) to reach that percentage of playtime.
    /// For example, `toc[50]` tells you what byte offset corresponds to the
    /// 50% playtime mark.
    ///
    /// Empty if the TOC flag was not set in the header.
    pub toc: Vec<i32>,

    /// VBR quality indicator (0-100)
    ///
    /// Higher values indicate higher quality VBR encoding.
    /// `-1` if the VBR_SCALE flag was not set.
    pub vbr_scale: i32,

    /// Extended LAME encoder information, if present
    ///
    /// Contains detailed encoder settings, ReplayGain values, and other
    /// LAME-specific metadata.
    pub lame_header: Option<LAMEHeader>,

    /// LAME version as (major, minor) tuple
    ///
    /// For example, LAME 3.99 would be `(3, 99)`.
    /// `(0, 0)` if version information is not available.
    pub lame_version: (i32, i32),

    /// LAME version string (e.g., "3.99.0", "3.100")
    ///
    /// Human-readable version string parsed from the header.
    /// Empty string if not available.
    pub lame_version_desc: String,

    /// `true` if this is an "Info" header (CBR), `false` if "Xing" (VBR/ABR)
    ///
    /// LAME writes "Info" headers for CBR files and "Xing" headers for VBR/ABR files.
    /// This distinction helps identify the bitrate mode.
    pub is_info: bool,
}

impl Default for XingHeader {
    fn default() -> Self {
        Self {
            frames: None,
            bytes: None,
            toc: Vec::new(),
            vbr_scale: -1,
            lame_header: None,
            lame_version: (0, 0),
            lame_version_desc: String::new(),
            is_info: false,
        }
    }
}

/// Extended LAME encoder information.
///
/// The LAME encoder appends additional metadata to the Xing/Info header,
/// providing detailed encoding parameters, quality settings, and ReplayGain data.
/// This information is useful for understanding how the file was encoded and for
/// audio analysis.
///
/// # VBR Method Codes
///
/// - `0`: Unknown/not set
/// - `1`: Constant Bitrate (CBR)
/// - `2`: Average Bitrate (ABR)
/// - `3`: VBR method 1 (old/rh)
/// - `4`: VBR method 2 (mtrh)
/// - `5`: VBR method 3 (mt)
/// - `6`: VBR method 4
/// - `8`: CBR (2-pass)
/// - `9`: ABR (2-pass)
///
/// # Examples
///
/// ```
/// use audex::mp3::MP3;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mp3 = MP3::from_file("song.mp3")?;
///
/// // Check if file has LAME encoder information
/// if let Some(encoder_info) = &mp3.info.encoder_info {
///     println!("Encoded with: {}", encoder_info);
/// }
///
/// // Access ReplayGain values
/// if let Some(track_gain) = mp3.info.track_gain {
///     println!("Track gain: {:.2} dB", track_gain);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LAMEHeader {
    /// VBR encoding method used
    ///
    /// - `0`: Unknown
    /// - `1`, `8`: Constant Bitrate (CBR)
    /// - `2`, `9`: Average Bitrate (ABR)
    /// - `3-6`: Variable Bitrate (VBR) with different quality algorithms
    pub vbr_method: i32,

    /// Lowpass filter cutoff frequency in Hz
    ///
    /// `0` if unknown. Indicates the frequency above which audio content
    /// was removed during encoding to improve compression.
    pub lowpass_filter: i32,

    /// Encoding quality setting (0-9, or -1 if unknown)
    ///
    /// - `0`: Highest quality (slowest encoding)
    /// - `9`: Lowest quality (fastest encoding)
    /// - `-1`: Unknown / not set (only from the `new()` code path;
    ///   `from_bytes()` produces `0` when the VBR scale is unset)
    ///
    /// LAME's `-q` parameter.
    pub quality: i32,

    /// VBR quality setting (0-10, or -1 if unknown)
    ///
    /// For VBR modes, this indicates the target quality level:
    /// - `0`: Highest quality (~245 kbps average)
    /// - `9`-`10`: Lowest quality (~65 kbps average)
    /// - `-1`: Unknown / not set (only from the `new()` code path;
    ///   `from_bytes()` produces `0` when the VBR scale is unset)
    ///
    /// LAME's `-V` parameter.
    pub vbr_quality: i32,

    /// Peak signal amplitude (0.0 to 1.0+)
    ///
    /// Maximum sample value in the decoded audio. `1.0` represents the
    /// maximum amplitude without clipping. Values above `1.0` indicate
    /// clipping occurred. `None` if not available.
    pub track_peak: Option<f32>,

    /// Radio ReplayGain origin code
    ///
    /// Indicates how the track gain value was determined:
    /// - `1`: Set by user
    /// - `2`: Set by user but verified by software
    /// - `3`: Determined automatically
    pub track_gain_origin: i32,

    /// Track ReplayGain adjustment in dB
    ///
    /// Adjustment needed to normalize this track to 89 dB SPL.
    /// Positive values mean the track should be made louder;
    /// negative values mean it should be made quieter.
    pub track_gain_adjustment: Option<f32>,

    /// Audiophile ReplayGain origin code
    ///
    /// Same as `track_gain_origin` but for album gain.
    pub album_gain_origin: i32,

    /// Album ReplayGain adjustment in dB
    ///
    /// Adjustment needed to normalize the entire album to 89 dB SPL.
    pub album_gain_adjustment: Option<f32>,

    /// Encoding flags bitfield
    ///
    /// Various encoding option flags. Interpretation depends on LAME version.
    pub encoding_flags: i32,

    /// ATH (Absolute Threshold of Hearing) type
    ///
    /// Indicates which psychoacoustic model was used for masking calculations.
    /// Values depend on LAME version.
    pub ath_type: i32,

    /// Target or minimum bitrate in kbps
    ///
    /// - For CBR: The constant bitrate used
    /// - For ABR: The target average bitrate
    /// - For VBR: The minimum allowed bitrate
    pub bitrate: i32,

    /// Encoder delay in samples
    ///
    /// Number of silence samples added at the start due to encoder delay.
    /// Should be skipped during playback for gapless playback.
    pub encoder_delay_start: i32,

    /// Encoder padding in samples
    ///
    /// Number of silence samples added at the end to complete the last frame.
    /// Should be skipped during playback for gapless playback.
    pub encoder_padding_end: i32,

    /// Source sample frequency code
    ///
    /// Indicates the source audio's original sample rate before any resampling.
    pub source_sample_frequency_enum: i32,

    /// `true` if encoder detected potentially problematic settings
    ///
    /// LAME sets this flag when encoding parameters may produce poor quality.
    pub unwise_setting_used: bool,

    /// Stereo mode code
    ///
    /// Indicates the stereo encoding mode used. Interpretation is encoder-specific.
    pub stereo_mode: i32,

    /// Noise shaping method
    ///
    /// Indicates which noise shaping algorithm was used (0-3 in LAME).
    pub noise_shaping: i32,

    /// MP3 gain adjustment (-128 to 127)
    ///
    /// Global gain adjustment applied to all audio. The actual gain factor
    /// is calculated as `2^(mp3_gain / 4)`.
    pub mp3_gain: i32,

    /// Preset ID or encoding mode
    pub surround_info: i32,
    /// lame preset
    pub preset_used: i32,
    /// Length in bytes excluding any ID3 tags.
    /// Stored as u32 to correctly represent files larger than 2 GB.
    pub music_length: u32,
    /// CRC16 of the data specified by music_length
    pub music_crc: i32,
    /// CRC16 of this header and everything before (not checked)
    pub header_crc: i32,
}

impl Default for LAMEHeader {
    fn default() -> Self {
        Self {
            vbr_method: 0,
            lowpass_filter: 0,
            quality: -1,
            vbr_quality: -1,
            track_peak: None,
            track_gain_origin: 0,
            track_gain_adjustment: None,
            album_gain_origin: 0,
            album_gain_adjustment: None,
            encoding_flags: 0,
            ath_type: -1,
            bitrate: -1,
            encoder_delay_start: 0,
            encoder_padding_end: 0,
            source_sample_frequency_enum: -1,
            unwise_setting_used: false,
            stereo_mode: 0,
            noise_shaping: 0,
            mp3_gain: 0,
            surround_info: 0,
            preset_used: 0,
            music_length: 0,
            music_crc: -1,
            header_crc: -1,
        }
    }
}

impl LAMEHeader {
    /// Create new LAMEHeader from XingHeader and data.
    ///
    /// NOTE: This method and `from_bytes()` both parse the same LAME header
    /// structure using different approaches (BitReader vs Cursor). Changes to
    /// one should be mirrored in the other. Caveat: when `vbr_scale < 0`,
    /// `new()` keeps the default quality/vbr_quality of -1, while `from_bytes()`
    /// sets them to 0.
    pub fn new(xing: &XingHeader, data: &[u8]) -> std::result::Result<Self, LAMEError> {
        if data.len() < 27 {
            return Err(LAMEError::new("Not enough data"));
        }

        let mut result = LAMEHeader::default();
        let mut reader = BitReader::new(data);

        // Parse extended LAME header (27 bytes)
        let revision = reader.bits(4)?;
        if revision != 0 {
            return Err(LAMEError::new(&format!(
                "unsupported header revision {}",
                revision
            )));
        }

        result.vbr_method = reader.bits(4)? as i32;
        result.lowpass_filter = (reader.bits(8)? * 100) as i32;

        // Derive LAME quality settings from the Xing VBR scale indicator.
        // A value of -1 means "not set", so skip the calculation to avoid
        // producing out-of-range results (e.g. vbr_quality = 10).
        if xing.vbr_scale >= 0 {
            let vbr_scale = xing.vbr_scale.clamp(0, 100);
            result.quality = (100 - vbr_scale) % 10;
            result.vbr_quality = (100 - vbr_scale) / 10;
        }

        // Track peak (4 bytes)
        let track_peak_data = reader.bytes(4)?;
        if track_peak_data == vec![0, 0, 0, 0] {
            result.track_peak = None;
        } else {
            // see PutLameVBR() in LAME's VbrTag.c
            let peak_value = u32::from_be_bytes([
                track_peak_data[0],
                track_peak_data[1],
                track_peak_data[2],
                track_peak_data[3],
            ]);
            result.track_peak = Some(peak_value as f32 / (2.0_f32).powi(23));
        }

        // Track gain
        let track_gain_type = reader.bits(3)? as i32;
        result.track_gain_origin = reader.bits(3)? as i32;
        let sign = reader.bits(1)? != 0;
        let mut gain_adj = reader.bits(9)? as f32 / 10.0;
        if sign {
            gain_adj *= -1.0;
        }
        if track_gain_type == 1 {
            result.track_gain_adjustment = Some(gain_adj);
        } else {
            result.track_gain_adjustment = None;
        }

        if !reader.is_aligned() {
            return Err(LAMEError::new("Reader not aligned after track gain"));
        }

        // Album gain
        let album_gain_type = reader.bits(3)? as i32;
        result.album_gain_origin = reader.bits(3)? as i32;
        let sign = reader.bits(1)? != 0;
        let mut album_gain_adj = reader.bits(9)? as f32 / 10.0;
        if sign {
            album_gain_adj *= -1.0;
        }
        if album_gain_type == 2 {
            result.album_gain_adjustment = Some(album_gain_adj);
        } else {
            result.album_gain_adjustment = None;
        }

        result.encoding_flags = reader.bits(4)? as i32;
        result.ath_type = reader.bits(4)? as i32;
        result.bitrate = reader.bits(8)? as i32;

        result.encoder_delay_start = reader.bits(12)? as i32;
        result.encoder_padding_end = reader.bits(12)? as i32;
        result.source_sample_frequency_enum = reader.bits(2)? as i32;
        result.unwise_setting_used = reader.bits(1)? != 0;
        result.stereo_mode = reader.bits(3)? as i32;
        result.noise_shaping = reader.bits(2)? as i32;

        // Read all 8 bits and interpret as two's complement, matching the
        // Cursor-based path and the LAME specification.
        let mp3_gain_raw = reader.bits(8)? as u8;
        result.mp3_gain = (mp3_gain_raw as i8) as i32;

        reader.skip(2)?; // Skip 2 bits
        result.surround_info = reader.bits(3)? as i32;
        result.preset_used = reader.bits(11)? as i32;
        result.music_length = reader.bits(32)?;
        result.music_crc = reader.bits(16)? as i32;
        result.header_crc = reader.bits(16)? as i32;

        if !reader.is_aligned() {
            return Err(LAMEError::new("Reader not aligned at end"));
        }

        Ok(result)
    }

    /// Parse LAME version string and determine if extended header follows
    pub fn parse_version(input_data: &[u8]) -> Result<((u8, u8), String, bool)> {
        if input_data.len() < 20 {
            return Err(AudexError::InvalidData("Not a lame header".to_string()));
        }

        // Take exactly 20 bytes
        let data = &input_data[..20];

        // Check for LAME header signature
        if !data.starts_with(b"LAME") && !data.starts_with(b"L3.99") {
            return Err(AudexError::InvalidData("Not a lame header".to_string()));
        }

        // Strip LAME prefix characters
        let mut data_stripped = data;
        while !data_stripped.is_empty()
            && (data_stripped[0] == b'E'
                || data_stripped[0] == b'M'
                || data_stripped[0] == b'A'
                || data_stripped[0] == b'L')
        {
            data_stripped = &data_stripped[1..];
        }

        // Extract major version, skip dots
        if data_stripped.is_empty() {
            return Err(AudexError::InvalidData("Invalid version".to_string()));
        }
        let major = data_stripped[0];
        data_stripped = &data_stripped[1..];

        // Skip dots
        while !data_stripped.is_empty() && data_stripped[0] == b'.' {
            data_stripped = &data_stripped[1..];
        }

        // Parse minor version digits
        let mut minor_bytes = Vec::new();
        while !data_stripped.is_empty() && data_stripped[0].is_ascii_digit() {
            minor_bytes.push(data_stripped[0]);
            data_stripped = &data_stripped[1..];
        }

        // Convert major and minor to integers
        let major_int = (major as char)
            .to_digit(10)
            .ok_or_else(|| AudexError::InvalidData("Invalid major version".to_string()))?
            as u8;
        let minor_str = std::str::from_utf8(&minor_bytes)
            .map_err(|_| AudexError::InvalidData("Invalid minor version".to_string()))?;
        let minor_int = minor_str
            .parse::<u8>()
            .map_err(|_| AudexError::InvalidData("Invalid minor version".to_string()))?;

        // The extended header was added in the 3.90 cycle.
        // Versions < 3.90, or "LAME3.90 (alpha)" (which has "(" in the data),
        // do NOT have an extended header.
        if (major_int, minor_int) < (3, 90)
            || ((major_int, minor_int) == (3, 90)
                && data_stripped.len() >= 11
                && data_stripped[data_stripped.len() - 11..data_stripped.len() - 10] == [b'('])
        {
            // Strip nulls and trailing whitespace, then decode as flag text
            let flag: Vec<u8> = data_stripped.to_vec();
            let flag_trimmed: Vec<u8> = flag
                .iter()
                .rev()
                .skip_while(|&&b| b == 0)
                .copied()
                .collect::<Vec<u8>>()
                .into_iter()
                .rev()
                .collect();
            // rstrip whitespace
            let flag_final: Vec<u8> = flag_trimmed
                .iter()
                .rev()
                .skip_while(|&&b| b == b' ')
                .copied()
                .collect::<Vec<u8>>()
                .into_iter()
                .rev()
                .collect();

            let flag_string = std::str::from_utf8(&flag_final)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| " (?)".to_string());

            let version_desc = format!("{}.{}{}", major_int, minor_int, flag_string);
            return Ok(((major_int, minor_int), version_desc, false));
        }

        // Has extended header (3.90+)
        if data_stripped.len() < 11 {
            return Err(AudexError::InvalidData(
                "Invalid version: too long".to_string(),
            ));
        }

        // Extract flag: everything before the last 11 bytes, with trailing nulls stripped
        let flag_end = data_stripped.len() - 11;
        let flag: Vec<u8> = data_stripped[..flag_end]
            .iter()
            .rev()
            .skip_while(|&&b| b == 0)
            .copied()
            .collect::<Vec<u8>>()
            .into_iter()
            .rev()
            .collect();

        // Build version string based on flag (same logic for all 3.90+ versions)
        let mut patch = String::new();
        let mut flag_string = String::new();

        if flag == b"a" {
            flag_string = " (alpha)".to_string();
        } else if flag == b"b" {
            flag_string = " (beta)".to_string();
        } else if flag == b"r" {
            patch = ".1+".to_string();
        } else if flag == b" " {
            if (major_int, minor_int) > (3, 96) {
                patch = ".0".to_string();
            } else {
                patch = ".0+".to_string();
            }
        } else if flag.is_empty() || flag == b"." {
            patch = ".0+".to_string();
        } else {
            flag_string = " (?)".to_string();
        }

        let version_desc = format!("{}.{}{}{}", major_int, minor_int, patch, flag_string);
        Ok(((major_int, minor_int), version_desc, true))
    }

    /// Parse extended LAME header.
    ///
    /// NOTE: This method and `new()` both parse the same LAME header structure
    /// using different approaches (Cursor vs BitReader). Changes to one should
    /// be mirrored in the other. Caveat: when `vbr_scale < 0`, `from_bytes()`
    /// sets quality/vbr_quality to 0, while `new()` keeps the default of -1.
    pub fn from_bytes(data: &[u8], xing: &XingHeader) -> Result<Self> {
        // Skip the 20-byte version string to get to the extended data
        if data.len() < 20 + 27 {
            return Err(AudexError::InvalidData("LAME header too short".to_string()));
        }

        let mut cursor = Cursor::new(&data[20..]);

        // Parse extended LAME header (27 bytes)
        let revision = cursor.read_u8()? >> 4; // upper 4 bits
        if revision != 0 {
            return Err(AudexError::InvalidData(format!(
                "Unsupported LAME revision {}",
                revision
            )));
        }

        cursor.set_position(cursor.position() - 1); // Back up to re-read the byte
        let vbr_method = cursor.read_u8()? & 0x0F; // lower 4 bits

        let lowpass_filter = cursor.read_u8()? as u16 * 100;

        // Derive LAME quality settings from the Xing VBR scale indicator.
        // A value of -1 means "not set", so skip the calculation to avoid
        // producing out-of-range results (e.g. vbr_quality = 10).
        let (quality, vbr_quality) = if xing.vbr_scale >= 0 {
            let vbr_scale = xing.vbr_scale.clamp(0, 100) as u32;
            (
                ((100 - vbr_scale) % 10) as u8,
                ((100 - vbr_scale) / 10) as u8,
            )
        } else {
            (0, 0)
        };

        // Track peak (4 bytes)
        let peak_bytes = [
            cursor.read_u8()?,
            cursor.read_u8()?,
            cursor.read_u8()?,
            cursor.read_u8()?,
        ];
        let track_peak = if peak_bytes == [0, 0, 0, 0] {
            None
        } else {
            Some(u32::from_be_bytes(peak_bytes) as f32 / (2_u32.pow(23) as f32))
        };

        // Track gain (2 bytes): type(3) + origin(3) + sign(1) + value(9)
        let gain_byte1 = cursor.read_u8()?;
        let gain_byte2 = cursor.read_u8()?;
        let track_gain_type = (gain_byte1 >> 5) & 0x07;
        let track_gain_origin = (gain_byte1 >> 2) & 0x07;
        let track_gain_sign = (gain_byte1 >> 1) & 0x01;
        let track_gain_value = (((gain_byte1 & 0x01) as u16) << 8) | (gain_byte2 as u16);

        let track_gain_adjustment = if track_gain_type == 1 {
            let mut gain = track_gain_value as f32 / 10.0;
            if track_gain_sign == 1 {
                gain = -gain;
            }
            Some(gain)
        } else {
            None
        };

        // Album gain (2 bytes): type(3) + origin(3) + sign(1) + value(9)
        let gain_byte1 = cursor.read_u8()?;
        let gain_byte2 = cursor.read_u8()?;
        let album_gain_type = (gain_byte1 >> 5) & 0x07;
        let album_gain_origin = (gain_byte1 >> 2) & 0x07;
        let album_gain_sign = (gain_byte1 >> 1) & 0x01;
        let album_gain_value = (((gain_byte1 & 0x01) as u16) << 8) | (gain_byte2 as u16);

        let album_gain_adjustment = if album_gain_type == 2 {
            let mut gain = album_gain_value as f32 / 10.0;
            if album_gain_sign == 1 {
                gain = -gain;
            }
            Some(gain)
        } else {
            None
        };

        let encoding_flags = cursor.read_u8()?;
        let ath_type = encoding_flags & 0x0F;
        let encoding_flags = encoding_flags >> 4;

        let bitrate = cursor.read_u8()?;

        // Encoder delay and padding (3 bytes)
        let delay_pad_bytes = [cursor.read_u8()?, cursor.read_u8()?, cursor.read_u8()?];
        let encoder_delay_start =
            ((delay_pad_bytes[0] as u16) << 4) | (((delay_pad_bytes[1] >> 4) & 0x0F) as u16);
        let encoder_padding_end =
            (((delay_pad_bytes[1] & 0x0F) as u16) << 8) | (delay_pad_bytes[2] as u16);

        // Misc byte
        let misc = cursor.read_u8()?;
        let source_sample_frequency_enum = (misc >> 6) & 0x03;
        let unwise_setting_used = (misc >> 5) & 0x01 != 0;
        let stereo_mode = (misc >> 2) & 0x07;
        let noise_shaping = misc & 0x03;

        // MP3 gain — stored as signed two's complement per the LAME spec
        let gain_byte = cursor.read_u8()?;
        let mp3_gain = gain_byte as i8;

        // Surround info and preset (2 bits skip + 3 bits surround + 11 bits preset = 16 bits = 2 bytes)
        let preset_bytes = [cursor.read_u8()?, cursor.read_u8()?];
        let surround_info = (preset_bytes[0] >> 3) & 0x07;
        let preset_used = (((preset_bytes[0] & 0x07) as u16) << 8) | (preset_bytes[1] as u16);

        // Music length and CRC
        let music_length = cursor.read_u32::<BigEndian>()?;
        let music_crc = cursor.read_u16::<BigEndian>()?;
        let header_crc = cursor.read_u16::<BigEndian>()?;

        Ok(LAMEHeader {
            vbr_method: vbr_method as i32,
            lowpass_filter: lowpass_filter as i32,
            track_peak,
            track_gain_origin: track_gain_origin as i32,
            track_gain_adjustment,
            album_gain_origin: album_gain_origin as i32,
            album_gain_adjustment,
            encoding_flags: encoding_flags as i32,
            ath_type: ath_type as i32,
            bitrate: bitrate as i32,
            encoder_delay_start: encoder_delay_start as i32,
            encoder_padding_end: encoder_padding_end as i32,
            quality: quality as i32,
            vbr_quality: vbr_quality as i32,
            source_sample_frequency_enum: source_sample_frequency_enum as i32,
            unwise_setting_used,
            stereo_mode: stereo_mode as i32,
            noise_shaping: noise_shaping as i32,
            mp3_gain: mp3_gain as i32,
            surround_info: surround_info as i32,
            preset_used: preset_used as i32,
            music_length,
            music_crc: music_crc as i32,
            header_crc: header_crc as i32,
        })
    }

    /// Guess encoder settings based on LAME version and parameters
    pub fn guess_settings(&self, major: u8, minor: u8) -> String {
        let version = (major, minor);

        // ABR mode
        if self.vbr_method == 2 {
            if matches!(version, (3, 90) | (3, 91) | (3, 92)) && self.encoding_flags != 0 {
                if self.bitrate < 255 {
                    return format!("--alt-preset {}", self.bitrate);
                } else {
                    return format!("--alt-preset {}+", self.bitrate);
                }
            }
            if self.preset_used != 0 {
                return format!("--preset {}", self.preset_used);
            } else if self.bitrate < 255 {
                return format!("--abr {}", self.bitrate);
            } else {
                return format!("--abr {}+", self.bitrate);
            }
        }

        // CBR mode
        if self.vbr_method == 1 {
            if self.preset_used == 0 {
                if self.bitrate < 255 {
                    return format!("-b {}", self.bitrate);
                } else {
                    return "-b 255+".to_string();
                }
            } else if self.preset_used == 1003 {
                return "--preset insane".to_string();
            }
            return format!("-b {}", self.preset_used);
        }

        // VBR modes
        if matches!(version, (3, 90) | (3, 91) | (3, 92)) {
            let preset_key = (
                self.vbr_quality,
                self.quality,
                self.vbr_method,
                self.lowpass_filter,
                self.ath_type,
            );

            match preset_key {
                (1, 2, 4, 19500, 3) => return "--preset r3mix".to_string(),
                (2, 2, 3, 19000, 4) => return "--alt-preset standard".to_string(),
                (2, 2, 3, 19500, 2) => return "--alt-preset extreme".to_string(),
                _ => {}
            }

            match self.vbr_method {
                3 => return format!("-V {}", self.vbr_quality),
                4 | 5 => return format!("-V {} --vbr-new", self.vbr_quality),
                _ => {}
            }
        } else if matches!(version, (3, 93) | (3, 94) | (3, 95) | (3, 96) | (3, 97)) {
            match self.preset_used {
                1001 => return "--preset standard".to_string(),
                1002 => return "--preset extreme".to_string(),
                1004 => return "--preset fast standard".to_string(),
                1005 => return "--preset fast extreme".to_string(),
                1006 => return "--preset medium".to_string(),
                1007 => return "--preset fast medium".to_string(),
                _ => {}
            }

            match self.vbr_method {
                3 => return format!("-V {}", self.vbr_quality),
                4 | 5 => return format!("-V {} --vbr-new", self.vbr_quality),
                // vbr_method >= 8 means "new" variant - treat 8 like 1 (CBR), 9 like 2 (ABR), etc.
                // But for broken LAME 3.97 files, method 8 with high vbr_quality means VBR
                8.. if self.vbr_quality >= 0 => return format!("-V {}", self.vbr_quality),
                _ => {}
            }
        } else if version == (3, 98) {
            match self.vbr_method {
                3 => return format!("-V {} --vbr-old", self.vbr_quality),
                4 | 5 => return format!("-V {}", self.vbr_quality),
                _ => {}
            }
        } else if major > 3 || (major == 3 && minor >= 99) {
            match self.vbr_method {
                3 => return format!("-V {} --vbr-old", self.vbr_quality),
                4 | 5 => {
                    let mut quality = self.vbr_quality;
                    // Handle special quality adjustments for newer LAME versions
                    let adjust_key = (quality, self.bitrate, self.lowpass_filter);
                    quality = match adjust_key {
                        (5, 32, 0) => 7,
                        (5, 8, 0) => 8,
                        (6, 8, 0) => 9,
                        _ => quality,
                    };
                    return format!("-V {}", quality);
                }
                _ => {}
            }
        }

        String::new()
    }
}

impl XingHeader {
    /// Create new XingHeader from data
    pub fn new(data: &[u8]) -> std::result::Result<Self, XingHeaderError> {
        if data.len() < 8 || (!data.starts_with(b"Xing") && !data.starts_with(b"Info")) {
            return Err(XingHeaderError::new("Not a Xing header"));
        }

        let mut result = XingHeader {
            is_info: data.starts_with(b"Info"),
            ..Default::default()
        };

        let flags = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let mut pos = 8;

        if flags & XingHeaderFlags::FRAMES != 0 {
            if data.len() < pos + 4 {
                return Err(XingHeaderError::new("Xing header truncated"));
            }
            result.frames = Some(u32::from_be_bytes([
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
            ]));
            pos += 4;
        }

        if flags & XingHeaderFlags::BYTES != 0 {
            if data.len() < pos + 4 {
                return Err(XingHeaderError::new("Xing header truncated"));
            }
            result.bytes = Some(u32::from_be_bytes([
                data[pos],
                data[pos + 1],
                data[pos + 2],
                data[pos + 3],
            ]));
            pos += 4;
        }

        if flags & XingHeaderFlags::TOC != 0 {
            if data.len() < pos + 100 {
                return Err(XingHeaderError::new("Xing header truncated"));
            }
            result.toc = data[pos..pos + 100].iter().map(|&b| b as i32).collect();
            pos += 100;
        }

        if flags & XingHeaderFlags::VBR_SCALE != 0 {
            if data.len() < pos + 4 {
                return Err(XingHeaderError::new("Xing header truncated"));
            }
            let raw_scale =
                u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            // Clamp values above i32::MAX to i32::MAX to prevent wrapping to negative
            result.vbr_scale = i32::try_from(raw_scale).unwrap_or(i32::MAX);
            pos += 4;
        }

        // Try to parse LAME version and extended header
        if data.len() >= pos + 20 {
            if let Ok((version, desc, has_extended)) = LAMEHeader::parse_version(&data[pos..]) {
                result.lame_version = (version.0 as i32, version.1 as i32);
                result.lame_version_desc = desc;

                if has_extended {
                    // The LAME version field is 20 bytes, but the extended header
                    // starts 9 bytes into it. The extended header data overlaps with
                    // the last 11 bytes of the version string area.
                    if let Ok(lame) = LAMEHeader::new(&result, &data[pos + 9..]) {
                        result.lame_header = Some(lame);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Returns the guessed encoder settings
    pub fn get_encoder_settings(&self) -> String {
        if let Some(ref lame_header) = self.lame_header {
            // Clamp version components to valid u8 range to prevent truncation
            let major = self.lame_version.0.clamp(0, 255) as u8;
            let minor = self.lame_version.1.clamp(0, 255) as u8;
            return lame_header.guess_settings(major, minor);
        }
        String::new()
    }

    /// Get offset to Xing header based on MPEG frame info.
    /// Only valid for Layer III frames.
    pub fn get_offset(frame: &MPEGFrame) -> u32 {
        debug_assert_eq!(frame.layer, MPEGLayer::Layer3);

        match (frame.version, frame.channel_mode) {
            (MPEGVersion::MPEG1, ChannelMode::Mono) => 21,
            (MPEGVersion::MPEG1, _) => 36,
            (MPEGVersion::MPEG2, ChannelMode::Mono) | (MPEGVersion::MPEG25, ChannelMode::Mono) => {
                13
            }
            (MPEGVersion::MPEG2, _) | (MPEGVersion::MPEG25, _) => 21,
        }
    }

    /// Parse Xing/Info header from bytes (legacy method for backward compatibility)
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(AudexError::InvalidData("Xing header too short".to_string()));
        }

        let header_type = &data[0..4];
        let is_info = match header_type {
            b"Xing" => false,
            b"Info" => true,
            _ => {
                return Err(AudexError::InvalidData(
                    "Not a Xing/Info header".to_string(),
                ));
            }
        };

        if data.len() < 8 {
            return Err(AudexError::InvalidData("Xing header truncated".to_string()));
        }

        let flags = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let mut cursor = Cursor::new(&data[8..]);

        let mut xing = Self {
            frames: None,
            bytes: None,
            toc: Vec::new(),
            vbr_scale: -1,
            is_info,
            lame_header: None,
            lame_version: (0, 0),
            lame_version_desc: String::new(),
        };

        // Parse fields based on flags
        if flags & XING_FRAMES_FLAG != 0 {
            // Frames field present
            xing.frames = Some(cursor.read_u32::<BigEndian>()?);
        }

        if flags & XING_BYTES_FLAG != 0 {
            // Bytes field present
            xing.bytes = Some(cursor.read_u32::<BigEndian>()?);
        }

        if flags & XING_TOC_FLAG != 0 {
            // TOC field present
            let mut toc = Vec::new();
            for _ in 0..100 {
                toc.push(cursor.read_u8()? as i32);
            }
            xing.toc = toc;
        }

        if flags & XING_VBR_SCALE_FLAG != 0 {
            // VBR scale field present
            // Clamp values above i32::MAX to i32::MAX to prevent wrapping to negative
            let raw_scale = cursor.read_u32::<BigEndian>()?;
            xing.vbr_scale = i32::try_from(raw_scale).unwrap_or(i32::MAX);
        }

        // Try to parse LAME version and extended header
        let position = 8 + cursor.position() as usize;
        if position + 20 <= data.len() {
            if let Ok((version, desc, has_extended)) = LAMEHeader::parse_version(&data[position..])
            {
                xing.lame_version = (version.0 as i32, version.1 as i32);
                xing.lame_version_desc = desc;

                if has_extended {
                    // Parse extended LAME header
                    if let Ok(lame) = LAMEHeader::from_bytes(&data[position..], &xing) {
                        xing.lame_header = Some(lame);
                    }
                }
            }
        }

        Ok(xing)
    }

    /// Convert to bytes for writing
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = Vec::new();

        // Header identifier
        if self.is_info {
            data.extend_from_slice(b"Info");
        } else {
            data.extend_from_slice(b"Xing");
        }

        // Calculate flags
        let mut flags = 0u32;
        if self.frames.is_some() {
            flags |= XING_FRAMES_FLAG;
        }
        if self.bytes.is_some() {
            flags |= XING_BYTES_FLAG;
        }
        if !self.toc.is_empty() {
            flags |= XING_TOC_FLAG;
        }
        if self.vbr_scale != -1 {
            flags |= XING_VBR_SCALE_FLAG;
        }

        data.extend_from_slice(&flags.to_be_bytes());

        // Add fields
        if let Some(frames) = self.frames {
            data.extend_from_slice(&frames.to_be_bytes());
        }

        if let Some(bytes) = self.bytes {
            data.extend_from_slice(&bytes.to_be_bytes());
        }

        if !self.toc.is_empty() {
            for &entry in &self.toc {
                data.push(entry.clamp(0, 255) as u8);
            }
        }

        if self.vbr_scale != -1 {
            // Clamp negative values to 0 to prevent wrapping via `as u32`
            data.extend_from_slice(&(self.vbr_scale.max(0) as u32).to_be_bytes());
        }

        data
    }
}

/// VBRI VBR header data (Fraunhofer encoder format).
///
/// VBRI (Variable Bitrate Information) is an alternative VBR header format
/// developed by Fraunhofer, used primarily in files encoded with the Fraunhofer
/// encoder instead of LAME. Like the Xing header, it provides frame count and
/// file size information for accurate duration calculation and seeking.
///
/// # Differences from Xing/Info
///
/// - **Location**: VBRI headers appear at a fixed offset (36 bytes) after the
///   first MPEG sync, while Xing headers appear at variable offsets.
/// - **Encoder**: Used by Fraunhofer encoder; LAME uses Xing/Info headers.
/// - **TOC Format**: VBRI uses a different table of contents structure with
///   configurable entry counts and scaling.
///
/// # Structure
///
/// VBRI headers are less common than Xing headers but follow a similar purpose.
/// They are typically found in older MP3 files or those encoded with Fraunhofer tools.
///
/// # Examples
///
/// ```
/// use audex::mp3::MP3;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mp3 = MP3::from_file("song.mp3")?;
///
/// // Duration is calculated from VBRI header if present
/// if let Some(duration) = mp3.info.length {
///     println!("Duration: {:?}", duration);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct VBRIHeader {
    /// VBRI header format version
    ///
    /// Currently only version 1 is defined and supported.
    pub version: i32,

    /// Audio quality indicator (0-65535)
    ///
    /// Higher values indicate higher quality encoding. Interpretation
    /// is encoder-specific. Stored as a 16-bit unsigned integer.
    pub quality: i32,

    /// Total file size in bytes (excluding ID3 tags)
    ///
    /// Used for duration calculation and progress indication.
    pub bytes: u32,

    /// Total number of MPEG frames
    ///
    /// Used to calculate precise duration: `duration = frames × samples_per_frame / sample_rate`
    pub frames: u32,

    /// Scale factor for TOC entries
    ///
    /// TOC values are scaled by this factor. Used to store larger offsets
    /// in smaller entry sizes.
    pub toc_scale_factor: i32,

    /// Number of frames represented by each TOC entry
    ///
    /// Unlike Xing's fixed 100-entry TOC, VBRI TOCs can have variable
    /// entry counts, with each entry spanning multiple frames.
    pub toc_frames: i32,

    /// Table of Contents for seeking
    ///
    /// Each entry represents the byte offset for a group of frames.
    /// The number of entries varies based on file length and `toc_frames`.
    pub toc: Vec<i32>,
}

impl VBRIHeader {
    /// Create new VBRIHeader from data
    pub fn new(data: &[u8]) -> std::result::Result<Self, VBRIHeaderError> {
        if data.len() < 26 || !data.starts_with(b"VBRI") {
            return Err(VBRIHeaderError::new("Not a VBRI header"));
        }

        let mut result = VBRIHeader {
            version: 0,
            quality: 0,
            bytes: 0,
            frames: 0,
            toc_scale_factor: 0,
            toc_frames: 0,
            toc: Vec::new(),
        };
        let mut pos = 4;

        result.version = u16::from_be_bytes([data[pos], data[pos + 1]]) as i32;
        pos += 2;

        if result.version != 1 {
            return Err(VBRIHeaderError::new(&format!(
                "Unsupported header version: {}",
                result.version
            )));
        }

        pos += 2; // skip delay (float16)
        result.quality = u16::from_be_bytes([data[pos], data[pos + 1]]) as i32;
        pos += 2;

        result.bytes = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        result.frames =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;

        let toc_num_entries = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;

        result.toc_scale_factor = u16::from_be_bytes([data[pos], data[pos + 1]]) as i32;
        pos += 2;

        let toc_entry_size = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
        pos += 2;

        result.toc_frames = u16::from_be_bytes([data[pos], data[pos + 1]]) as i32;
        pos += 2;

        let toc_size = toc_entry_size
            .checked_mul(toc_num_entries)
            .ok_or_else(|| VBRIHeaderError::new("VBRI TOC size overflow"))?;
        let required = pos
            .checked_add(toc_size)
            .ok_or_else(|| VBRIHeaderError::new("VBRI TOC size overflow"))?;
        if data.len() < required {
            return Err(VBRIHeaderError::new("VBRI header truncated"));
        }

        // Accept entry sizes 1, 2, and 4 to stay consistent with from_bytes()
        result.toc = Vec::new();
        if toc_entry_size == 1 {
            for i in 0..toc_size {
                let entry = data[pos + i] as i32;
                result.toc.push(entry);
            }
        } else if toc_entry_size == 2 {
            for i in (0..toc_size).step_by(2) {
                let entry = u16::from_be_bytes([data[pos + i], data[pos + i + 1]]) as i32;
                // u16 always fits in i32, no clamping needed
                result.toc.push(entry);
            }
        } else if toc_entry_size == 4 {
            for i in (0..toc_size).step_by(4) {
                let raw = u32::from_be_bytes([
                    data[pos + i],
                    data[pos + i + 1],
                    data[pos + i + 2],
                    data[pos + i + 3],
                ]);
                // Clamp to i32::MAX to prevent wrapping on large TOC entries
                let entry = i32::try_from(raw).unwrap_or(i32::MAX);
                result.toc.push(entry);
            }
        } else {
            return Err(VBRIHeaderError::new("Invalid TOC entry size"));
        }

        Ok(result)
    }

    /// Get offset to VBRI header — always at offset 36 from MPEG header.
    /// Only valid for Layer III frames.
    pub fn get_offset(_frame: &MPEGFrame) -> u32 {
        debug_assert_eq!(_frame.layer, MPEGLayer::Layer3);
        36
    }

    /// Parse VBRI header from bytes (legacy method for backward compatibility)
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 4 || &data[0..4] != b"VBRI" {
            return Err(AudexError::InvalidData("Not a VBRI header".to_string()));
        }

        if data.len() < 26 {
            return Err(AudexError::InvalidData("VBRI header too short".to_string()));
        }

        let mut cursor = Cursor::new(&data[4..]);

        let version = cursor.read_u16::<BigEndian>()?;
        let _delay = cursor.read_u16::<BigEndian>()?;
        let quality = cursor.read_u16::<BigEndian>()?;
        let bytes = cursor.read_u32::<BigEndian>()?;
        let frames = cursor.read_u32::<BigEndian>()?;
        let toc_entries = cursor.read_u16::<BigEndian>()?;
        let toc_scale = cursor.read_u16::<BigEndian>()?;
        let toc_entry_size = cursor.read_u16::<BigEndian>()?;
        let toc_frames_per_entry = cursor.read_u16::<BigEndian>()?;

        // Validate that the buffer holds enough bytes for the claimed TOC.
        // Without this check, a crafted header claiming 65535 entries would
        // force the loop to iterate thousands of times before hitting EOF.
        let toc_data_needed = (toc_entries as usize).saturating_mul(toc_entry_size as usize);
        let remaining = data.len().saturating_sub(26); // 26 = 4 (magic) + 22 (fixed fields)
        if toc_data_needed > remaining {
            return Err(AudexError::InvalidData(format!(
                "VBRI TOC claims {} entries of {} bytes each ({} bytes) \
                 but only {} bytes remain",
                toc_entries, toc_entry_size, toc_data_needed, remaining
            )));
        }

        // Read TOC
        let mut toc = Vec::with_capacity(toc_entries as usize);
        for _ in 0..toc_entries {
            let entry = match toc_entry_size {
                1 => cursor.read_u8()? as u32,
                2 => cursor.read_u16::<BigEndian>()? as u32,
                4 => cursor.read_u32::<BigEndian>()?,
                _ => {
                    return Err(AudexError::InvalidData(
                        "Invalid VBRI TOC entry size".to_string(),
                    ));
                }
            };
            toc.push(entry);
        }

        Ok(Self {
            version: version as i32,
            quality: quality as i32,
            bytes,
            frames,
            toc_scale_factor: toc_scale as i32,
            toc_frames: toc_frames_per_entry as i32,
            // Clamp TOC entries to i32::MAX to prevent wrapping on large u32 values
            toc: toc
                .into_iter()
                .map(|t| i32::try_from(t).unwrap_or(i32::MAX))
                .collect(),
        })
    }
}

/// Parsed MPEG audio frame with stream information and VBR header data.
///
/// This struct represents a fully parsed MPEG frame, including both the basic
/// frame header information and any associated VBR header data (Xing/Info/VBRI).
/// It contains all the information needed to describe an MP3 file's audio properties.
///
/// # Frame vs File Information
///
/// - **Basic frame fields** (version, layer, bitrate, etc.): Parsed directly from
///   the 4-byte MPEG frame header
/// - **VBR header fields** (encoder_info, track_gain, length, etc.): Extracted from
///   Xing/Info/VBRI headers if present
///
/// # Sketchy Flag
///
/// The `sketchy` field indicates whether the duration and bitrate information is
/// reliable:
/// - `false`: VBR header found; duration is accurate
/// - `true`: No VBR header; duration estimated from file size (less accurate)
///
/// # Examples
///
/// ```no_run
/// // MPEGFrame is typically not used directly; MP3 and MPEGInfo provide
/// // higher-level access to frame data
/// use audex::mp3::MP3;
///
/// let mp3 = MP3::from_file("song.mp3").unwrap();
/// println!("MPEG version: {:?}", mp3.info.version);
/// println!("Layer: {:?}", mp3.info.layer);
/// println!("Bitrate: {} bps", mp3.info.bitrate);
/// ```
#[derive(Debug, Clone)]
pub struct MPEGFrame {
    /// MPEG version (MPEG-1, MPEG-2, or MPEG-2.5)
    pub version: MPEGVersion,

    /// MPEG layer (I, II, or III/MP3)
    pub layer: MPEGLayer,

    /// Bitrate in bits per second (bps, NOT kbps)
    ///
    /// For CBR files, this is the constant bitrate.
    /// For VBR files with a VBR header, this is the average bitrate.
    /// Divide by 1000 to get kbps.
    pub bitrate: u32,

    /// Sample rate in Hz (e.g., 44100, 48000, 22050)
    pub sample_rate: u32,

    /// Channel mode (stereo, joint stereo, dual channel, or mono)
    pub channel_mode: ChannelMode,

    /// Pre-emphasis filter applied to the audio
    pub emphasis: Emphasis,

    /// CRC error protection is enabled in this frame
    pub protected: bool,

    /// Padding bit is set in this frame
    pub padding: bool,

    /// Private bit (application-specific use)
    pub private: bool,

    /// Copyright bit indicating copyrighted material
    pub copyright: bool,

    /// Original media bit (vs. copy)
    pub original: bool,

    /// Mode extension for joint stereo encoding
    pub mode_extension: u8,

    /// Size of this frame in bytes
    ///
    /// Calculated from bitrate, sample rate, and padding bit.
    pub frame_size: u32,

    /// Byte offset of this frame in the file
    ///
    /// Indicates where this frame was found during parsing.
    pub frame_offset: u64,

    /// `true` if duration/bitrate is uncertain (no VBR header found)
    ///
    /// When `true`, duration and bitrate are estimated rather than precise.
    pub sketchy: bool,

    /// Bitrate encoding mode (CBR, VBR, ABR, or Unknown)
    ///
    /// Determined from Xing/Info/VBRI header if present.
    pub bitrate_mode: BitrateMode,

    /// Encoder name and version (e.g., "LAME3.99r"), if available
    ///
    /// Extracted from LAME header if present.
    pub encoder_info: Option<String>,

    /// Encoder settings string, if available
    ///
    /// Contains encoding parameters used by the encoder.
    pub encoder_settings: Option<String>,

    /// Track ReplayGain adjustment in dB, if available
    ///
    /// From LAME header ReplayGain data.
    pub track_gain: Option<f32>,

    /// Peak sample value for the track (0.0 to 1.0+), if available
    ///
    /// From LAME header peak data.
    pub track_peak: Option<f32>,

    /// Album ReplayGain adjustment in dB, if available
    ///
    /// From LAME header ReplayGain data.
    pub album_gain: Option<f32>,

    /// Total audio duration, if determinable
    ///
    /// Calculated from VBR header frame count, or `None` if no VBR header.
    pub length: Option<Duration>,
}

impl MPEGFrame {
    /// Parse MPEG frame from header bytes at specified offset
    ///
    /// Creates a basic MPEGFrame from raw header bytes without VBR header parsing.
    /// This is useful for async operations where the header has been read separately.
    ///
    /// # Arguments
    /// * `header` - 4-byte MPEG frame header
    /// * `offset` - File offset where this header was found
    ///
    /// # Returns
    /// * `Ok(MPEGFrame)` - Basic frame information parsed from header
    /// * `Err(AudexError)` - Invalid MPEG header data
    pub fn from_header_at_offset(header: &[u8; 4], offset: u64) -> Result<Self> {
        let (
            version,
            layer,
            bitrate,
            sample_rate,
            channel_mode,
            emphasis,
            protected,
            padding,
            private,
            copyright,
            original,
            mode_extension,
        ) = parse_mpeg_header(header)?;

        let frame_size = calculate_frame_size(version, layer, bitrate, sample_rate, padding);

        Ok(MPEGFrame {
            version,
            layer,
            bitrate,
            sample_rate,
            channel_mode,
            emphasis,
            protected,
            padding,
            private,
            copyright,
            original,
            mode_extension,
            frame_size,
            frame_offset: offset,
            sketchy: true, // Always sketchy when created from header only (no VBR parsing)
            bitrate_mode: BitrateMode::Unknown,
            encoder_info: None,
            encoder_settings: None,
            track_gain: None,
            track_peak: None,
            album_gain: None,
            length: None,
        })
    }

    /// Parse MPEG frame from file at current position
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let frame_offset = reader.stream_position()?;

        let mut header = [0u8; 4];
        reader.read_exact(&mut header)?;

        let (
            version,
            layer,
            bitrate,
            sample_rate,
            channel_mode,
            emphasis,
            protected,
            padding,
            private,
            copyright,
            original,
            mode_extension,
        ) = parse_mpeg_header(&header)?;

        let frame_size = calculate_frame_size(version, layer, bitrate, sample_rate, padding);

        let mut frame = MPEGFrame {
            version,
            layer,
            bitrate,
            sample_rate,
            channel_mode,
            emphasis,
            protected,
            padding,
            private,
            copyright,
            original,
            mode_extension,
            frame_size,
            frame_offset,
            sketchy: true, // Starts as sketchy, cleared only if VBR header found -
            bitrate_mode: BitrateMode::Unknown,
            encoder_info: None,
            encoder_settings: None,
            track_gain: None,
            track_peak: None,
            album_gain: None,
            length: None,
        };

        // For Layer III, try to parse VBR header
        if layer == MPEGLayer::Layer3 {
            frame.parse_vbr_header(reader)?;
        }

        // Seek to end of frame
        reader.seek(SeekFrom::Start(frame_offset + frame_size as u64))?;

        Ok(frame)
    }

    /// Get number of samples per frame
    pub fn samples_per_frame(&self) -> u32 {
        match (self.version, self.layer) {
            (MPEGVersion::MPEG1, MPEGLayer::Layer1) => 384,
            (MPEGVersion::MPEG1, _) => 1152,
            (_, MPEGLayer::Layer1) => 384,
            (_, MPEGLayer::Layer3) => 576,
            (_, _) => 1152, // MPEG-2/2.5 Layer II uses 1152
        }
    }

    /// Guess bitrate mode from VBR header information
    pub fn guess_bitrate_mode(&self, xing: &XingHeader) -> BitrateMode {
        if let Some(lame) = &xing.lame_header {
            match lame.vbr_method {
                1 | 8 => {
                    return BitrateMode::CBR;
                }
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

    /// Get channel count
    pub fn channels(&self) -> u16 {
        match self.channel_mode {
            ChannelMode::Mono => 1,
            _ => 2,
        }
    }

    /// Parse VBR header if present
    pub(crate) fn parse_vbr_header<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Try Xing/Info header first
        let xing_offset = XingHeader::get_offset(self);
        reader.seek(SeekFrom::Start(self.frame_offset + xing_offset as u64))?;

        // Try Xing/Info header first
        // Allocate enough space for full Xing header (8 bytes) + all fields (4+4+100+4=112) + LAME header (9 + 47 bytes)
        // AND for VBRI header which can have a large TOC (base 26 bytes + potentially 200+ TOC entries × 2-4 bytes each)
        // Use 512 bytes to be safe for both Xing and VBRI headers
        let mut vbr_data = vec![0u8; 512];
        if reader.read_exact(&mut vbr_data).is_ok() {
            if let Ok(xing) = XingHeader::new(&vbr_data) {
                // VBR header found, clear sketchy flag - matches behavior
                self.sketchy = false;
                self.bitrate_mode = if xing.is_info {
                    BitrateMode::CBR
                } else {
                    BitrateMode::VBR // Will be refined by LAME header if present
                };

                if let Some(frames) = xing.frames {
                    // Calculate total samples from frames
                    let samples_per_frame = self.samples_per_frame() as i64;
                    let mut total_samples = frames as i64 * samples_per_frame;

                    // Recalculate bitrate first if we have byte count
                    if let Some(bytes) = xing.bytes {
                        if total_samples > 0 {
                            // The first frame is only included in xing.bytes but not in xing.frames, skip it
                            let audio_bytes =
                                (bytes as i64).saturating_sub(self.frame_size as i64).max(0);
                            // formula: intround((audio_bytes * 8 * sample_rate) / float(samples))
                            // Result is in bps, stored directly
                            // Clamp to valid u32 range before casting to prevent incorrect metadata
                            let bitrate = (audio_bytes * 8 * self.sample_rate as i64) as f64
                                / total_samples as f64;
                            if bitrate.is_finite() {
                                self.bitrate = bitrate.round().clamp(0.0, u32::MAX as f64) as u32;
                            }
                        }
                    }

                    // Adjust for LAME encoder delay and padding
                    if let Some(lame) = &xing.lame_header {
                        let delay = lame.encoder_delay_start as i64;
                        let padding = lame.encoder_padding_end as i64;

                        total_samples -= delay;
                        total_samples -= padding;

                        if total_samples < 0 {
                            // Older LAME versions wrote bogus delay/padding for short files with low bitrate
                            total_samples = 0;
                        }
                    }

                    // Calculate length from adjusted samples
                    if total_samples > 0 && self.sample_rate > 0 {
                        self.length = Some(Duration::from_secs_f64(
                            total_samples as f64 / self.sample_rate as f64,
                        ));
                    } else {
                        // Xing header explicitly reports 0 frames (or LAME
                        // delay/padding reduced samples to 0) — honour that
                        // instead of falling through to file-size estimation.
                        self.length = Some(Duration::ZERO);
                    }
                }

                // Set encoder info, settings, and ReplayGain regardless of frames count
                if !xing.lame_version_desc.is_empty() {
                    self.encoder_info = Some(format!("LAME {}", xing.lame_version_desc));
                }
                if let Some(lame) = &xing.lame_header {
                    let settings = lame.guess_settings(
                        xing.lame_version.0.clamp(0, 255) as u8,
                        xing.lame_version.1.clamp(0, 255) as u8,
                    );
                    if !settings.is_empty() {
                        self.encoder_settings = Some(settings);
                    }
                    self.track_gain = lame.track_gain_adjustment;
                    self.track_peak = lame.track_peak;
                    self.album_gain = lame.album_gain_adjustment;
                }

                self.bitrate_mode = self.guess_bitrate_mode(&xing);
                return Ok(());
            }
        }

        // Try VBRI header at fixed offset
        let vbri_offset = VBRIHeader::get_offset(self);
        reader.seek(SeekFrom::Start(self.frame_offset + vbri_offset as u64))?;
        if reader.read_exact(&mut vbr_data).is_ok() {
            if let Ok(vbri) = VBRIHeader::from_bytes(&vbr_data) {
                // VBR header found, clear sketchy flag - matches behavior
                self.sketchy = false;
                self.bitrate_mode = BitrateMode::VBR;
                self.encoder_info = Some("FhG".to_string());

                if vbri.frames > 0 && self.sample_rate > 0 {
                    let total_samples = self.samples_per_frame() as f64 * vbri.frames as f64;
                    let length_secs = total_samples / self.sample_rate as f64;
                    self.length = Some(Duration::from_secs_f64(length_secs));

                    if vbri.bytes > 0 && length_secs > 0.0 {
                        // Result is in bps, stored directly
                        // Clamp to valid u32 range before casting
                        self.bitrate = ((vbri.bytes as f64 * 8.0) / length_secs)
                            .round()
                            .clamp(0.0, u32::MAX as f64)
                            as u32;
                    }
                }
            }
        }

        Ok(())
    }
}

/// Parse MPEG frame header - matches specification implementation
pub fn parse_mpeg_header(header: &[u8]) -> Result<MPEGHeaderInfo> {
    if header.len() < 4 {
        return Err(AudexError::InvalidData("MPEG header too short".to_string()));
    }

    // Check sync pattern - must be 0xFFE (11 bits)
    let sync = ((header[0] as u16) << 3) | ((header[1] as u16) >> 5);
    if sync != 0x7FF {
        return Err(AudexError::InvalidData("Invalid MPEG sync".to_string()));
    }

    // Version (2 bits — masked to 0..=3, all branches covered)
    let version_bits = (header[1] >> 3) & 0x03;
    let version = match version_bits {
        0b00 => MPEGVersion::MPEG25,
        0b01 => return Err(AudexError::InvalidData("Reserved MPEG version".to_string())),
        0b10 => MPEGVersion::MPEG2,
        0b11 => MPEGVersion::MPEG1,
        _ => {
            return Err(AudexError::InternalError(
                "2-bit mask produced value > 3".to_string(),
            ));
        }
    };

    // Layer (2 bits — masked to 0..=3, all branches covered)
    let layer_bits = (header[1] >> 1) & 0x03;
    let layer = match layer_bits {
        0b00 => return Err(AudexError::InvalidData("Reserved layer".to_string())),
        0b01 => MPEGLayer::Layer3,
        0b10 => MPEGLayer::Layer2,
        0b11 => MPEGLayer::Layer1,
        _ => {
            return Err(AudexError::InternalError(
                "2-bit mask produced value > 3".to_string(),
            ));
        }
    };

    // Protection bit
    let protected = (header[1] & 0x01) == 0;

    // Bitrate index (4 bits)
    let bitrate_index = (header[2] >> 4) & 0x0F;
    if bitrate_index == 0x0F || bitrate_index == 0x00 {
        return Err(AudexError::InvalidData("Invalid bitrate index".to_string()));
    }
    let bitrate = get_bitrate(version, layer, bitrate_index)?;

    // Sample rate index (2 bits)
    let samplerate_index = (header[2] >> 2) & 0x03;
    if samplerate_index == 0x03 {
        return Err(AudexError::InvalidData("Reserved sample rate".to_string()));
    }
    let sample_rate = get_sample_rate(version, samplerate_index)?;

    // Padding bit
    let padding = (header[2] & 0x02) != 0;

    // Private bit
    let private = (header[2] & 0x01) != 0;

    // Channel mode (2 bits — masked to 0..=3, all branches covered)
    let channel_mode = match (header[3] >> 6) & 0x03 {
        0b00 => ChannelMode::Stereo,
        0b01 => ChannelMode::JointStereo,
        0b10 => ChannelMode::DualChannel,
        0b11 => ChannelMode::Mono,
        _ => {
            return Err(AudexError::InternalError(
                "2-bit mask produced value > 3".to_string(),
            ));
        }
    };

    // Mode extension (2 bits)
    let mode_extension = (header[3] >> 4) & 0x03;

    // Copyright bit
    let copyright = (header[3] & 0x08) != 0;

    // Original bit
    let original = (header[3] & 0x04) != 0;

    // Emphasis (2 bits — masked to 0..=3, all branches covered)
    let emphasis = match header[3] & 0x03 {
        0b00 => Emphasis::None,
        0b01 => Emphasis::MS50_15,
        0b10 => Emphasis::Reserved,
        0b11 => Emphasis::CCITT,
        _ => {
            return Err(AudexError::InternalError(
                "2-bit mask produced value > 3".to_string(),
            ));
        }
    };

    Ok((
        version,
        layer,
        bitrate,
        sample_rate,
        channel_mode,
        emphasis,
        protected,
        padding,
        private,
        copyright,
        original,
        mode_extension,
    ))
}

/// Get bitrate from version, layer, and bitrate index
/// Returns bitrate in bps (bits per second) - matches specification implementation exactly
fn get_bitrate(version: MPEGVersion, layer: MPEGLayer, index: u8) -> Result<u32> {
    if index == 0 || index == 15 {
        return Err(AudexError::InvalidData("Invalid bitrate index".to_string()));
    }

    // Bitrate tables from standard MPEG specifications (in kbps)
    // Note: Index 0 is free format, index 15 is bad - both excluded above
    let bitrates_kbps: &[u32] = match (version, layer) {
        // MPEG-1 Layer I
        (MPEGVersion::MPEG1, MPEGLayer::Layer1) => &[
            0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448,
        ],
        // MPEG-1 Layer II
        (MPEGVersion::MPEG1, MPEGLayer::Layer2) => &[
            0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
        ],
        // MPEG-1 Layer III
        (MPEGVersion::MPEG1, MPEGLayer::Layer3) => &[
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
        ],
        // MPEG-2 Layer I (also used for MPEG-2.5 Layer I)
        (MPEGVersion::MPEG2, MPEGLayer::Layer1) | (MPEGVersion::MPEG25, MPEGLayer::Layer1) => &[
            0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256,
        ],
        // MPEG-2 Layer II & III (also used for MPEG-2.5 Layer II & III)
        (MPEGVersion::MPEG2, MPEGLayer::Layer2)
        | (MPEGVersion::MPEG2, MPEGLayer::Layer3)
        | (MPEGVersion::MPEG25, MPEGLayer::Layer2)
        | (MPEGVersion::MPEG25, MPEGLayer::Layer3) => {
            &[0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160]
        }
    };

    if index as usize >= bitrates_kbps.len() {
        return Err(AudexError::InvalidData(
            "Bitrate index out of range".to_string(),
        ));
    }

    let bitrate_kbps = bitrates_kbps[index as usize];
    if bitrate_kbps == 0 {
        return Err(AudexError::InvalidData(
            "Free format not supported".to_string(),
        ));
    }

    // Convert kbps to bps
    Ok(bitrate_kbps * 1000)
}

/// Get sample rate from version and sample rate index
fn get_sample_rate(version: MPEGVersion, index: u8) -> Result<u32> {
    if index == 3 {
        return Err(AudexError::InvalidData("Reserved sample rate".to_string()));
    }

    let rates = match version {
        MPEGVersion::MPEG1 => [44100, 48000, 32000],
        MPEGVersion::MPEG2 => [22050, 24000, 16000],
        MPEGVersion::MPEG25 => [11025, 12000, 8000],
    };

    rates
        .get(index as usize)
        .copied()
        .ok_or_else(|| AudexError::InvalidData("Sample rate index out of range".to_string()))
}

/// Iterator over MPEG sync positions - matches specification iter_sync function
///
/// This function finds MPEG frame synchronization patterns in a stream.
/// It looks for the 11-bit sync pattern (0xFFE) followed by valid MPEG data.
pub fn iter_sync<R: Read + Seek>(reader: &mut R, max_read: u64) -> Result<Vec<u64>> {
    // Hard ceiling on collected positions to prevent unbounded memory
    // growth when scanning adversarial or malformed streams.
    const MAX_SYNC_POSITIONS: usize = 100_000;

    let mut syncs = Vec::new();
    let mut read = 0u64;
    let mut size = 2usize;
    let mut last_byte = 0u8;

    while read < max_read {
        let data_offset = reader.stream_position()?;
        let bytes_to_read = std::cmp::min(max_read - read, size as u64) as usize;
        let mut new_data = vec![0u8; bytes_to_read];
        let bytes_read = reader.read(&mut new_data)?;

        if bytes_read == 0 {
            break;
        }

        new_data.truncate(bytes_read);
        read += bytes_read as u64;

        // Check if last byte from previous read + first byte forms sync
        let is_second = |b: u8| (b & 0xE0) == 0xE0;

        if last_byte == 0xFF && !new_data.is_empty() && is_second(new_data[0]) {
            syncs.push(data_offset - 1);
            if syncs.len() >= MAX_SYNC_POSITIONS {
                return Ok(syncs);
            }
        }

        // Look for sync pattern within current data
        let mut find_offset = 0;
        while let Some(index) = new_data[find_offset..].iter().position(|&b| b == 0xFF) {
            let abs_index = find_offset + index;

            // If not the last byte and next byte forms valid sync
            if abs_index < new_data.len() - 1 && is_second(new_data[abs_index + 1]) {
                syncs.push(data_offset + abs_index as u64);
                if syncs.len() >= MAX_SYNC_POSITIONS {
                    return Ok(syncs);
                }
            }

            find_offset = abs_index + 1;
        }

        if !new_data.is_empty() {
            last_byte = *new_data.last().expect("checked non-empty above");
        }

        // Double the read buffer up to a 64 KB ceiling. Beyond that,
        // larger reads give diminishing throughput returns while wasting
        // memory on multi-GB files.
        const MAX_BUF_SIZE: usize = 64 * 1024;
        if size < MAX_BUF_SIZE {
            size = (size * 2).min(MAX_BUF_SIZE);
        }
        reader.seek(SeekFrom::Start(data_offset + bytes_read as u64))?;
    }

    Ok(syncs)
}

/// Smallest valid MPEG audio frame in bytes. Frames below this size
/// indicate a pathological header and should be rejected to avoid
/// excessive per-byte seeking during sync validation.
const MIN_FRAME_SIZE: u32 = 24;

/// Calculate frame size in bytes - matches specification implementation.
///
/// Returns `MIN_FRAME_SIZE` (24) if `sample_rate` is 0, since no valid frame
/// can exist without a defined sample rate.
pub fn calculate_frame_size(
    version: MPEGVersion,
    layer: MPEGLayer,
    bitrate: u32,     // bps (bits per second)
    sample_rate: u32, // Hz
    padding: bool,
) -> u32 {
    if sample_rate == 0 {
        // No valid frame can exist without a sample rate — return the
        // minimum valid frame size to prevent byte-by-byte seeking
        return MIN_FRAME_SIZE;
    }

    let samples_per_frame = match (version, layer) {
        (MPEGVersion::MPEG1, MPEGLayer::Layer1) => 384,
        (MPEGVersion::MPEG1, _) => 1152,
        (_, MPEGLayer::Layer1) => 384,
        (_, MPEGLayer::Layer3) => 576,
        (_, _) => 1152, // MPEG-2/2.5 Layer II uses 1152
    };

    // Calculate basic frame size
    let mut frame_size = if layer == MPEGLayer::Layer1 {
        // Layer I uses 32-bit slots
        (((samples_per_frame as u64 * bitrate as u64) / 8) / sample_rate as u64) as u32 * 4
    } else {
        // Layer II and III use 8-bit slots
        (((samples_per_frame as u64 * bitrate as u64) / 8) / sample_rate as u64) as u32
    };

    // Add padding if present
    if padding {
        frame_size += match layer {
            MPEGLayer::Layer1 => 4, // 4 bytes for Layer I
            _ => 1,                 // 1 byte for Layer II & III
        };
    }

    // Ensure the frame size meets the minimum for a valid MPEG audio
    // frame. Sizes below this threshold result from pathological
    // bitrate/sample-rate combinations and would cause excessive
    // seek operations during frame validation.
    if frame_size < MIN_FRAME_SIZE {
        frame_size = MIN_FRAME_SIZE;
    }

    frame_size
}

/// Skip ID3v2 tags at the beginning of a file.
///
/// Determines the actual stream length up front so that a malformed header
/// declaring a body size larger than the file cannot cause an unbounded
/// seek past EOF.
pub fn skip_id3<R: Read + Seek>(fileobj: &mut R) -> Result<u64> {
    fileobj.seek(SeekFrom::Start(0))?;
    let file_size = fileobj.seek(SeekFrom::End(0))?;
    fileobj.seek(SeekFrom::Start(0))?;

    // Windows Media Player writes multiple ID3 tags, so skip as many as we find.
    // Cap iterations to prevent unbounded looping on pathological files that
    // contain thousands of consecutive ID3 headers.
    const MAX_ID3_SKIP_ITERATIONS: usize = 1000;
    let mut id3_iterations = 0usize;
    loop {
        id3_iterations += 1;
        if id3_iterations > MAX_ID3_SKIP_ITERATIONS {
            break;
        }

        let mut id3_header = [0u8; 10];
        let bytes_read = fileobj.read(&mut id3_header)?;

        if bytes_read < 10 {
            // Not enough data for a header — rewind to where we were
            fileobj.seek(SeekFrom::Current(-(bytes_read as i64)))?;
            break;
        }

        if &id3_header[0..3] == b"ID3" {
            let tag_size =
                crate::id3::util::decode_synchsafe_int_checked(&id3_header[6..10])? as u64;
            let current_pos = fileobj.stream_position()?;

            // Only seek forward when the declared size actually fits
            // within the remaining file data
            if tag_size > 0 && current_pos + tag_size <= file_size {
                // Guard against tag sizes that exceed i64::MAX — an unchecked
                // cast would wrap to a negative offset and seek backward.
                let seek_offset = i64::try_from(tag_size).map_err(|_| {
                    AudexError::ParseError(format!(
                        "ID3 tag size {} exceeds maximum seekable offset",
                        tag_size
                    ))
                })?;
                fileobj.seek(SeekFrom::Current(seek_offset))?;
                continue;
            }

            // The tag header is valid but the declared body size is
            // either zero or extends past EOF (truncated/corrupt file).
            // Stay positioned right after the 10-byte header so the
            // caller knows an ID3 tag was present.
            break;
        }

        // No more ID3 tags, seek back to start of non-ID3 data
        fileobj.seek(SeekFrom::Current(-(bytes_read as i64)))?;
        break;
    }

    Ok(fileobj.stream_position()?)
}
