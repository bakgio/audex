//! # AAC (Advanced Audio Coding) Format Support
//!
//! This module provides support for reading AAC audio files in both ADTS and ADIF container formats.
//! AAC is a lossy audio codec that succeeds MP3, offering better sound quality at comparable bitrates.
//!
//! ## Overview
//!
//! AAC (Advanced Audio Coding) is a standardized audio codec for lossy digital audio compression.
//! This implementation supports:
//! - **ADTS** (Audio Data Transport Stream): Frame-based streaming format
//! - **ADIF** (Audio Data Interchange Format): Single-header file format
//! - **Stream information extraction**: Sample rate, channels, bitrate
//!
//! ## Important Limitations
//!
//! **AAC files do not natively support embedded metadata.** This module is **read-only**
//! and only extracts audio stream information. For metadata tagging:
//! - Use **ID3v2 tags** (prepended to AAC file)
//! - Use **APEv2 tags** (appended to AAC file)
//! - Consider using **M4A** (MP4 container) for native metadata support
//!
//! ## File Formats
//!
//! ### ADTS (Audio Data Transport Stream)
//! - **Structure**: Series of self-contained frames with headers
//! - **Extension**: `.aac` (common)
//! - **Use case**: Streaming, broadcasting
//! - **Header**: Each frame has sync word (0xFFF) and configuration
//! - **Advantages**: Error resilient, seekable, streamable
//!
//! ### ADIF (Audio Data Interchange Format)
//! - **Structure**: Single header followed by raw AAC data
//! - **Extension**: `.aac`
//! - **Use case**: Local file storage
//! - **Header**: One-time configuration at file start
//! - **Advantages**: Lower overhead for stored files
//!
//! ## Audio Characteristics
//!
//! - **Lossy compression**: Smaller file sizes than lossless formats
//! - **Profiles**: LC (Low Complexity), HE-AAC (High Efficiency), HE-AAC v2
//! - **Sample rates**: 7.35 kHz to 96 kHz
//! - **Channels**: Mono, stereo, or multichannel (up to 8 channels via standard configurations)
//! - **Bitrate**: Variable or constant (typically 128-320 kbps for music)
//!
//! ## Examples
//!
//! ### Loading and reading stream information
//!
//! ```no_run
//! use audex::aac::AAC;
//! use audex::{FileType, StreamInfo};
//!
//! let aac = AAC::load("audio.aac").unwrap();
//!
//! println!("AAC Stream Information:");
//! println!("  Sample rate: {} Hz", aac.info.sample_rate());
//! println!("  Channels: {}", aac.info.channels());
//! // Bitrate is in bits per second, divide by 1000 for display in kbps
//! println!("  Bitrate: {} kbps", aac.info.bitrate() / 1000);
//! println!("  Stream type: {}", aac.info.stream_type());
//!
//! if let Some(length) = aac.info.length() {
//!     let secs = length.as_secs();
//!     println!("  Duration: {}:{:02}", secs / 60, secs % 60);
//! }
//! ```
//!
//! ### Detecting container format
//!
//! ```no_run
//! use audex::aac::AAC;
//! use audex::FileType;
//!
//! let aac = AAC::load("audio.aac").unwrap();
//!
//! match aac.info.stream_type() {
//!     "ADTS" => println!("Frame-based streaming format"),
//!     "ADIF" => println!("Single-header file format"),
//!     _ => println!("Unknown format"),
//! }
//! ```
//!
//! ### Handling files with ID3 tags
//!
//! ```no_run
//! use audex::aac::AAC;
//! use audex::{FileType, StreamInfo};
//!
//! // AAC module skips ID3v2 tags automatically when reading stream info
//! let aac = AAC::load("tagged_audio.aac").unwrap();
//! println!("Stream info (ID3 tags skipped): {} Hz", aac.info.sample_rate());
//!
//! // Use ID3 module separately for tag access
//! match audex::id3::load("tagged_audio.aac") {
//!     Ok(id3_tags) => {
//!         if let Some(title) = id3_tags.get("TIT2") {
//!             println!("Title from ID3: {}", title[0]);
//!         }
//!     }
//!     Err(_) => {}
//! }
//! ```
//!
//! ## Tagging AAC Files
//!
//! Since raw AAC files lack native metadata support, use these approaches:
//!
//! ```no_run
//! // Option 1: Use ID3v2 tags (prepended to file)
//! use audex::id3::ID3;
//! use audex::FileType;
//!
//! // Load AAC file with ID3 tags
//! let mut id3 = ID3::from_file("audio.aac").unwrap();
//! // Modify ID3 tags using the tags() method
//! id3.save().unwrap();
//!
//! // Option 2: Convert to M4A (MP4 container with AAC)
//! // M4A provides native iTunes-style metadata support
//! use audex::mp4::MP4;
//!
//! let mp4 = MP4::load("audio.m4a").unwrap();
//! // Access native MP4 tags...
//! ```
//!
//! ## Technical Notes
//!
//! - ADTS sync word is 12 bits: `0xFFF`
//! - Maximum frame size is typically 8191 bytes
//! - Sample rate is encoded as a 4-bit index into a frequency table
//! - Channel configuration supports up to 7.1 surround sound
//!
//! ## References
//!
//! - ISO/IEC 13818-7: MPEG-2 Advanced Audio Coding
//! - ISO/IEC 14496-3: MPEG-4 Audio (includes AAC specification)

use crate::id3::ID3Tags;
use crate::util::BitReader;
use crate::{AudexError, FileType, Result, StreamInfo};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::util::loadfile_read_async;
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// AAC sampling frequency table indexed by `sampling_frequency_index` (4-bit field)
const FREQS: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

/// Fixed ADTS header fields used to verify stream consistency across frames.
type AdtsFixedHeader = (u8, u8, u8, u8, u8, u8, u8, u8, u8);

/// ADTS stream parser — accumulates frames belonging to the same audio stream.
///
/// Locates sync words (0xFFF), validates fixed header consistency across frames,
/// and collects statistics (sample count, payload bytes) for bitrate estimation.
struct ADTSStream<'a, R: Read + Seek> {
    /// BitReader for parsing frame data
    reader: &'a mut BitReader<R>,

    /// Number of successfully parsed frames
    parsed_frames: usize,

    /// Offset in bytes at which the stream starts (the first sync word)
    offset: i64,

    /// Fixed header key for stream validation
    fixed_header_key: Option<AdtsFixedHeader>,

    /// Total samples processed
    samples: u64,

    /// Total payload bytes (excluding framing/CRC)
    payload: u64,

    /// Stream start position in bytes
    start: i64,

    /// Last position in bytes
    last: i64,
}

impl<'a, R: Read + Seek> ADTSStream<'a, R> {
    /// Create a new ADTS stream parser
    fn new(reader: &'a mut BitReader<R>) -> Self {
        let start = reader.get_position() as i64 / 8;
        ADTSStream {
            reader,
            parsed_frames: 0,
            offset: -1,
            fixed_header_key: None,
            samples: 0,
            payload: 0,
            start,
            last: start,
        }
    }

    /// Find the next sync word (0xFFF)
    /// Returns true if found
    fn sync(&mut self, max_bytes: usize) -> bool {
        // At least 2 bytes for the sync
        let max_bytes = max_bytes.max(2);

        self.reader.align();
        let mut remaining = max_bytes;

        while remaining > 0 {
            let b = match self.reader.bytes(1) {
                Ok(bytes) => bytes[0],
                Err(_) => return false,
            };

            if b == 0xFF {
                match self.reader.read_bits(4) {
                    Ok(0xF) => return true,
                    Ok(_) => {
                        self.reader.align();
                        remaining = remaining.saturating_sub(2);
                    }
                    Err(_) => return false,
                }
            } else {
                remaining = remaining.saturating_sub(1);
            }
        }

        false
    }

    /// Find a stream starting from the current position
    /// Returns the offset if found
    fn find_stream(reader: &'a mut BitReader<R>, max_bytes: usize) -> Option<i64> {
        let mut stream = Self::new(reader);
        if stream.sync(max_bytes) {
            // Extract position from stream instead of reader (reader is borrowed by stream)
            let offset = ((stream.reader.get_position()) as i64 - 12) / 8;
            Some(offset)
        } else {
            None
        }
    }

    /// Get bitrate of the raw AAC blocks (excluding framing/CRC)
    fn bitrate(&self) -> u32 {
        if self.samples == 0 {
            return 0;
        }
        // Clamp to u32::MAX instead of truncating — crafted files with large
        // payload and few samples can produce values exceeding u32 range
        ((8 * self.payload * self.frequency() as u64) / self.samples).min(u32::MAX as u64) as u32
    }

    fn total_samples(&self) -> u64 {
        self.samples
    }

    /// Get bytes read in the stream so far (including framing)
    fn size(&self) -> i64 {
        self.last - self.start
    }

    /// Get number of channels.
    /// Returns 0 only when no fixed header is available. Configuration values
    /// of 0 (defined by PCE) and 8..=15 (reserved) are treated as unknown
    /// and mapped to 1 to avoid propagating an invalid zero channel count.
    fn channels(&self) -> u16 {
        if let Some(key) = &self.fixed_header_key {
            let b_index = key.6;
            match b_index {
                0 => 1, // PCE-defined layout; use 1 as safe fallback
                7 => 8,
                8..=15 => 1, // reserved values; use 1 as safe fallback
                _ => b_index as u16,
            }
        } else {
            0
        }
    }

    /// Get sampling frequency (0 means unknown)
    fn frequency(&self) -> u32 {
        if let Some(key) = &self.fixed_header_key {
            let f_index = key.4 as usize;
            FREQS.get(f_index).copied().unwrap_or(0)
        } else {
            0
        }
    }

    /// Parse a single ADTS frame
    /// Returns true if parsing was successful
    fn parse_frame(&mut self) -> bool {
        // Position of sync word
        let start = self.reader.get_position() as i64 - 12;

        // adts_fixed_header
        let id = match self.reader.read_bits(1) {
            Ok(v) => v as u8,
            Err(_) => return false,
        };
        let layer = match self.reader.read_bits(2) {
            Ok(v) => v as u8,
            Err(_) => return false,
        };
        let protection_absent = match self.reader.read_bits(1) {
            Ok(v) => v as u8,
            Err(_) => return false,
        };

        let profile = match self.reader.read_bits(2) {
            Ok(v) => v as u8,
            Err(_) => return false,
        };
        let sampling_frequency_index = match self.reader.read_bits(4) {
            Ok(v) => v as u8,
            Err(_) => return false,
        };
        let private_bit = match self.reader.read_bits(1) {
            Ok(v) => v as u8,
            Err(_) => return false,
        };
        let channel_configuration = match self.reader.read_bits(3) {
            Ok(v) => v as u8,
            Err(_) => return false,
        };
        let original_copy = match self.reader.read_bits(1) {
            Ok(v) => v as u8,
            Err(_) => return false,
        };
        let home = match self.reader.read_bits(1) {
            Ok(v) => v as u8,
            Err(_) => return false,
        };

        // The fixed header must be the same for every frame in the stream
        let fixed_header_key = (
            id,
            layer,
            protection_absent,
            profile,
            sampling_frequency_index,
            private_bit,
            channel_configuration,
            original_copy,
            home,
        );

        if let Some(existing_key) = &self.fixed_header_key {
            if existing_key != &fixed_header_key {
                return false;
            }
        } else {
            self.fixed_header_key = Some(fixed_header_key);
        }

        // adts_variable_header
        if self.reader.skip(2).is_err() {
            return false;
        }
        let frame_length = match self.reader.read_bits(13) {
            Ok(v) => v,
            Err(_) => return false,
        };
        if self.reader.skip(11).is_err() {
            return false;
        }
        let nordbif = match self.reader.read_bits(2) {
            Ok(v) => v,
            Err(_) => return false,
        };

        // Calculate CRC overhead
        let mut crc_overhead: u64 = 0;
        if protection_absent == 0 {
            crc_overhead += (nordbif + 1) * 16;
            if nordbif != 0 {
                crc_overhead *= 2;
            }
        }

        // Skip remaining frame data
        let current_pos = self.reader.get_position() as i64;
        let left = (frame_length as i64 * 8) - (current_pos - start);
        if left < 0 {
            return false;
        }

        // Reject frames where the remaining bits exceed i32 range
        if left >= i32::MAX as i64 {
            return false;
        }
        if self.reader.skip(left as i32).is_err() {
            return false;
        }

        // Update statistics
        self.payload = self
            .payload
            .saturating_add(((left as u64).saturating_sub(crc_overhead)) / 8);
        self.samples = self.samples.saturating_add((nordbif + 1) * 1024);
        self.last = self.reader.get_position() as i64 / 8;
        self.parsed_frames += 1;

        true
    }
}

/// Program config element parser for ADIF format
struct ProgramConfigElement {
    sampling_frequency_index: u8,
    channels: u16,
}

impl ProgramConfigElement {
    /// Parse a program_config_element from the bitstream
    fn parse<R: Read + Seek>(reader: &mut BitReader<R>) -> Result<Self> {
        let _ = reader
            .read_bits(4)
            .map_err(|e| AudexError::ParseError(e.to_string()))?; // element_instance_tag
        let _ = reader
            .read_bits(2)
            .map_err(|e| AudexError::ParseError(e.to_string()))?; // object_type
        let sampling_frequency_index = reader
            .read_bits(4)
            .map_err(|e| AudexError::ParseError(e.to_string()))?
            as u8;

        let num_front_channel_elements = reader
            .read_bits(4)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let num_side_channel_elements = reader
            .read_bits(4)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let num_back_channel_elements = reader
            .read_bits(4)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let num_lfe_channel_elements = reader
            .read_bits(2)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let num_assoc_data_elements = reader
            .read_bits(3)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let num_valid_cc_elements = reader
            .read_bits(4)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        // Mono mixdown
        let mono_mixdown_present = reader
            .read_bits(1)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        if mono_mixdown_present == 1 {
            reader
                .skip(4)
                .map_err(|e| AudexError::ParseError(e.to_string()))?;
        }

        // Stereo mixdown
        let stereo_mixdown_present = reader
            .read_bits(1)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        if stereo_mixdown_present == 1 {
            reader
                .skip(4)
                .map_err(|e| AudexError::ParseError(e.to_string()))?;
        }

        // Matrix mixdown
        let matrix_mixdown_idx_present = reader
            .read_bits(1)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        if matrix_mixdown_idx_present == 1 {
            reader
                .skip(3)
                .map_err(|e| AudexError::ParseError(e.to_string()))?;
        }

        // Calculate total channels
        let elms =
            num_front_channel_elements + num_side_channel_elements + num_back_channel_elements;
        let mut channels = 0u16;

        for _ in 0..elms {
            channels += 1;
            let element_is_cpe = reader
                .read_bits(1)
                .map_err(|e| AudexError::ParseError(e.to_string()))?;
            if element_is_cpe == 1 {
                channels += 1;
            }
            reader
                .skip(4)
                .map_err(|e| AudexError::ParseError(e.to_string()))?;
        }

        channels += num_lfe_channel_elements as u16;

        // Skip remaining fields
        reader
            .skip((4 * num_lfe_channel_elements) as i32)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        reader
            .skip((4 * num_assoc_data_elements) as i32)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        reader
            .skip((5 * num_valid_cc_elements) as i32)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        reader.align();

        let comment_field_bytes = reader
            .read_bits(8)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let comment_bits = (8 * comment_field_bytes) as u32;
        if comment_bits > 0 {
            // Validate remaining data before skipping to prevent reading
            // past the descriptor boundary with a crafted comment_field_bytes
            if !reader.can_read(comment_bits) {
                return Err(AudexError::ParseError(format!(
                    "comment_field_bytes ({}) exceeds remaining data in program config element",
                    comment_field_bytes
                )));
            }
            // Guard the u32 -> i32 cast to prevent a negative skip on overflow
            let skip_amount = i32::try_from(comment_bits).map_err(|_| {
                AudexError::ParseError(format!(
                    "comment field bit count {} exceeds maximum skip range",
                    comment_bits
                ))
            })?;
            reader
                .skip(skip_amount)
                .map_err(|e| AudexError::ParseError(e.to_string()))?;
        }

        Ok(ProgramConfigElement {
            sampling_frequency_index,
            channels,
        })
    }
}

/// Audio stream information for AAC files.
///
/// Contains technical details about the AAC audio stream extracted from
/// ADTS frame headers or ADIF file headers.
///
/// # Fields
///
/// - **`channels`**: Number of audio channels (1=mono, 2=stereo, up to 8 via standard ADTS configurations)
/// - **`length`**: Total duration of the audio file
/// - **`sample_rate`**: Audio sampling rate in Hz (7350 to 96000)
/// - **`bitrate`**: Average bitrate in bits per second
/// - **`stream_type`**: Container format ("ADTS" or "ADIF")
///
/// # Container Formats
///
/// ## ADTS (Audio Data Transport Stream)
/// Frame-based format where each frame contains:
/// - Sync word (0xFFF)
/// - Frame configuration
/// - Compressed audio data
/// - Optional CRC
///
/// ## ADIF (Audio Data Interchange Format)
/// Single-header format with:
/// - One-time configuration header
/// - Raw AAC bitstream
/// - Lower overhead for file storage
///
/// # Examples
///
/// ## Reading stream information
///
/// ```no_run
/// use audex::aac::AAC;
/// use audex::{FileType, StreamInfo};
///
/// let aac = AAC::load("audio.aac").unwrap();
/// let info = &aac.info;
///
/// println!("AAC Stream Information:");
/// println!("  Sample rate: {} Hz", info.sample_rate());
/// println!("  Channels: {}", info.channels());
/// println!("  Bitrate: {} kbps", info.bitrate() / 1000);
/// println!("  Format: {}", info.stream_type());
///
/// if let Some(length) = info.length() {
///     let minutes = length.as_secs() / 60;
///     let seconds = length.as_secs() % 60;
///     println!("  Duration: {}:{:02}", minutes, seconds);
/// }
/// ```
///
/// ## Determining audio quality
///
/// ```no_run
/// use audex::aac::AAC;
/// use audex::{FileType, StreamInfo};
///
/// let aac = AAC::load("audio.aac").unwrap();
/// let bitrate_kbps = aac.info.bitrate() / 1000;
///
/// let quality = match bitrate_kbps {
///     0..=96 => "Low quality",
///     97..=128 => "Standard quality",
///     129..=192 => "High quality",
///     193..=256 => "Very high quality",
///     _ => "Excellent quality",
/// };
///
/// println!("Audio quality: {} ({} kbps)", quality, bitrate_kbps);
/// ```
///
/// ## Checking channel configuration
///
/// ```no_run
/// use audex::aac::AAC;
/// use audex::{FileType, StreamInfo};
///
/// let aac = AAC::load("audio.aac").unwrap();
///
/// match aac.info.channels() {
///     1 => println!("Mono audio"),
///     2 => println!("Stereo audio"),
///     6 => println!("5.1 surround sound"),
///     8 => println!("7.1 surround sound"),
///     n => println!("{} channel audio", n),
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct AACInfo {
    /// Number of audio channels
    channels: u16,

    /// File length in seconds
    length: Option<Duration>,

    /// Audio sampling rate in Hz
    sample_rate: u32,

    /// Audio bitrate in bits per second
    bitrate: u32,

    /// Stream type (ADTS or ADIF)
    stream_type: String,
}

impl AACInfo {
    /// Returns the container stream type (ADTS or ADIF).
    ///
    /// # Returns
    /// - `"ADTS"` for Audio Data Transport Stream format
    /// - `"ADIF"` for Audio Data Interchange Format
    pub fn stream_type(&self) -> &str {
        &self.stream_type
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn bitrate(&self) -> u32 {
        self.bitrate
    }

    /// Parse AAC stream information from a file
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        // Skip ID3v2 header if present
        let mut header = [0u8; 10];
        reader.read_exact(&mut header)?;

        let start_offset = if &header[0..3] == b"ID3" {
            // Parse synchsafe integer for tag size
            let size = ((header[6] as u32 & 0x7F) << 21)
                | ((header[7] as u32 & 0x7F) << 14)
                | ((header[8] as u32 & 0x7F) << 7)
                | (header[9] as u32 & 0x7F);
            (size + 10) as u64
        } else {
            0
        };

        reader.seek(SeekFrom::Start(start_offset))?;

        // Check for ADIF or ADTS
        let mut format_header = [0u8; 4];
        reader.read_exact(&mut format_header)?;

        if &format_header == b"ADIF" {
            Self::parse_adif(reader)
        } else {
            reader.seek(SeekFrom::Start(start_offset))?;
            Self::parse_adts(reader, start_offset)
        }
    }

    /// Parse ADIF format
    fn parse_adif<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        Self::parse_adif_with_file_size(reader, None)
    }

    /// Core ADIF parser. When `known_file_size` is `Some`, that value is used
    /// for duration calculation instead of seeking to the end of the stream.
    fn parse_adif_with_file_size<R: Read + Seek>(
        reader: &mut R,
        known_file_size: Option<u64>,
    ) -> Result<Self> {
        let mut bit_reader =
            BitReader::new(reader).map_err(|e| AudexError::ParseError(e.to_string()))?;

        let copyright_id_present = bit_reader
            .read_bits(1)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        if copyright_id_present == 1 {
            bit_reader
                .skip(72)
                .map_err(|e| AudexError::ParseError(e.to_string()))?; // copyright_id
        }

        bit_reader
            .skip(2)
            .map_err(|e| AudexError::ParseError(e.to_string()))?; // original_copy, home

        let bitstream_type = bit_reader
            .read_bits(1)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let bitrate = bit_reader
            .read_bits(23)
            .map_err(|e| AudexError::ParseError(e.to_string()))? as u32;
        let npce = bit_reader
            .read_bits(4)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        if bitstream_type == 0 {
            bit_reader
                .skip(20)
                .map_err(|e| AudexError::ParseError(e.to_string()))?; // adif_buffer_fullness
        }

        // Parse first program config element
        let pce = ProgramConfigElement::parse(&mut bit_reader)?;

        let sample_rate = FREQS
            .get(pce.sampling_frequency_index as usize)
            .copied()
            .unwrap_or(0);

        let channels = pce.channels;

        // Parse remaining program config elements
        for _ in 0..npce {
            ProgramConfigElement::parse(&mut bit_reader)?;
        }

        bit_reader.align();

        // Estimate length from bitrate and data size
        let inner_reader = bit_reader.into_inner();
        let start = inner_reader.stream_position()?;
        let end = match known_file_size {
            Some(fs) => fs,
            None => inner_reader.seek(SeekFrom::End(0))?,
        };
        let data_length = end.saturating_sub(start);

        let length = if bitrate != 0 {
            Some(Duration::from_secs_f64(
                (8.0 * data_length as f64) / bitrate as f64,
            ))
        } else {
            Some(Duration::from_secs(0))
        };

        Ok(AACInfo {
            channels,
            length,
            sample_rate,
            bitrate,
            stream_type: "ADIF".to_string(),
        })
    }

    /// Parse ADTS format
    fn parse_adts<R: Read + Seek>(reader: &mut R, start_offset: u64) -> Result<Self> {
        Self::parse_adts_with_file_size(reader, start_offset, None)
    }

    /// Core ADTS parser. When `known_file_size` is `Some`, that value is used
    /// for duration calculation instead of seeking to the end of the stream.
    /// This lets callers that only buffer a prefix of the file supply the real
    /// file size externally.
    fn parse_adts_with_file_size<R: Read + Seek>(
        reader: &mut R,
        start_offset: u64,
        known_file_size: Option<u64>,
    ) -> Result<Self> {
        const MAX_INITIAL_READ: usize = 512;
        const MAX_RESYNC_READ: usize = 10;
        const MAX_SYNC_TRIES: usize = 10;
        const FRAMES_MAX: usize = 100;
        const FRAMES_NEEDED: usize = 3;

        let mut offset = start_offset;
        let mut final_stream: Option<(u32, u16, u32, u64, i64, i64)> = None;

        // Try up to MAX_SYNC_TRIES times to find a sync word and read frames
        for _ in 0..MAX_SYNC_TRIES {
            reader.seek(SeekFrom::Start(offset))?;

            let mut bit_reader =
                BitReader::new(&mut *reader).map_err(|e| AudexError::ParseError(e.to_string()))?;
            let stream_offset = match ADTSStream::find_stream(&mut bit_reader, MAX_INITIAL_READ) {
                Some(off) => off,
                None => return Err(AudexError::AACError("sync not found".to_string())),
            };

            // Validate that the stream offset is non-negative before casting to u64,
            // since a negative i64 would wrap to a very large unsigned value.
            if stream_offset < 0 {
                return Err(AudexError::AACError("negative stream offset".to_string()));
            }

            // Advance past the last found sync position
            offset = offset
                .checked_add(stream_offset as u64)
                .and_then(|v| v.checked_add(1))
                .ok_or_else(|| AudexError::AACError("stream offset overflow".to_string()))?;

            let mut stream = ADTSStream::new(&mut bit_reader);
            stream.offset = stream_offset;

            // Parse frames
            for _ in 0..FRAMES_MAX {
                if !stream.parse_frame() {
                    break;
                }
                if !stream.sync(MAX_RESYNC_READ) {
                    break;
                }
            }

            if stream.parsed_frames >= FRAMES_NEEDED {
                final_stream = Some((
                    stream.frequency(),
                    stream.channels(),
                    stream.bitrate(),
                    stream.total_samples(),
                    stream.size(),
                    stream.offset,
                ));
                break;
            }
        }

        let (frequency, channels, bitrate, samples, size, stream_offset) = final_stream
            .ok_or_else(|| AudexError::AACError("no valid stream found".to_string()))?;

        // Use the caller-supplied file size or seek to the end of the stream
        let end_pos = match known_file_size {
            Some(fs) => fs,
            None => reader.seek(SeekFrom::End(0))?,
        };
        let stream_size = end_pos.saturating_sub(offset + stream_offset as u64);

        let length = if frequency != 0 && size > 0 {
            let seconds = (samples as f64 * stream_size as f64) / (size as f64 * frequency as f64);
            Some(Duration::from_secs_f64(seconds))
        } else {
            Some(Duration::from_secs(0))
        };

        Ok(AACInfo {
            channels,
            length,
            sample_rate: frequency,
            bitrate,
            stream_type: "ADTS".to_string(),
        })
    }
}

impl StreamInfo for AACInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        Some(self.bitrate)
    }

    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate)
    }

    fn channels(&self) -> Option<u16> {
        Some(self.channels)
    }

    fn bits_per_sample(&self) -> Option<u16> {
        None
    }

    fn pprint(&self) -> String {
        format!(
            "AAC ({}), {} Hz, {:.2} seconds, {} channel(s), {} bps",
            self.stream_type,
            self.sample_rate,
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0),
            self.channels,
            self.bitrate
        )
    }
}

/// Main structure representing an AAC audio file.
///
/// This is the interface for reading AAC files in ADTS or ADIF container formats.
/// AAC files are **read-only** in this module - stream information is extracted but
/// metadata tagging is not natively supported.
///
/// # Structure
///
/// - **`info`**: Audio stream information (sample rate, channels, bitrate, duration)
/// - **`tags`**: Optional ID3 tags (not used for native AAC, always None)
/// - **`filename`**: Internal file path
///
/// # Important: No Native Tag Support
///
/// Raw AAC files (.aac) do not have a standardized metadata container. For tagging:
/// - **Option 1**: Use ID3v2 tags prepended to the AAC file (via ID3 module)
/// - **Option 2**: Use APEv2 tags appended to the AAC file (via APEv2 module)
/// - **Option 3**: Use M4A format (AAC in MP4 container) for native iTunes-style tags
///
/// # File Format
///
/// AAC audio can be stored in two container formats:
/// - **Extension**: `.aac` (standard)
/// - **MIME type**: `audio/aac`, `audio/aacp`
/// - **Codec**: AAC (Advanced Audio Coding)
///
/// # Examples
///
/// ## Loading and reading stream information
///
/// ```no_run
/// use audex::aac::AAC;
/// use audex::{FileType, StreamInfo};
///
/// let aac = AAC::load("audio.aac").unwrap();
///
/// // Access stream information
/// println!("Sample rate: {} Hz", aac.info.sample_rate());
/// println!("Channels: {}", aac.info.channels());
/// println!("Bitrate: {} kbps", aac.info.bitrate() / 1000);
/// println!("Container: {}", aac.info.stream_type());
///
/// if let Some(length) = aac.info.length() {
///     println!("Duration: {:.2} seconds", length.as_secs_f64());
/// }
/// ```
///
/// ## Detecting ADTS vs ADIF format
///
/// ```no_run
/// use audex::aac::AAC;
/// use audex::FileType;
///
/// let aac = AAC::load("audio.aac").unwrap();
///
/// match aac.info.stream_type() {
///     "ADTS" => {
///         println!("Frame-based ADTS format");
///         println!("Good for: streaming, broadcasting");
///     }
///     "ADIF" => {
///         println!("Single-header ADIF format");
///         println!("Good for: local file storage");
///     }
///     _ => println!("Unknown format"),
/// }
/// ```
///
/// ## Checking for ID3 tags separately
///
/// ```no_run
/// use audex::aac::AAC;
/// use audex::{FileType, StreamInfo};
/// use audex::id3::ID3;
///
/// // Read AAC stream info
/// let aac = AAC::load("audio.aac").unwrap();
/// println!("Sample rate: {} Hz", aac.info.sample_rate());
///
/// // AAC module automatically skips ID3v2 tags when reading stream info
/// // To access tags, use the ID3 module separately:
/// match audex::id3::load("audio.aac") {
///     Ok(id3_tags) => {
///         if let Some(title) = id3_tags.get("TIT2") {
///             println!("Title (from ID3): {}", title[0]);
///         }
///     }
///     Err(_) => println!("No ID3 tags found"),
/// }
/// ```
///
/// ## Displaying format details
///
/// ```no_run
/// use audex::aac::AAC;
/// use audex::{FileType, StreamInfo};
///
/// let aac = AAC::load("audio.aac").unwrap();
/// let info = &aac.info;
///
/// println!("AAC File Details:");
/// println!("  Format: {} ({})",
///     info.stream_type(),
///     if info.stream_type() == "ADTS" {
///         "streaming"
///     } else {
///         "file storage"
///     }
/// );
///
/// println!("  Audio:");
/// println!("    Sample rate: {} Hz", info.sample_rate());
/// println!("    Channels: {}", info.channels());
/// println!("    Bitrate: {} kbps", info.bitrate() / 1000);
///
/// if let Some(length) = info.length() {
///     println!("    Duration: {:.2} seconds", length.as_secs_f64());
/// }
/// ```
///
/// ## Why AAC is read-only
///
/// ```no_run
/// use audex::aac::AAC;
/// use audex::FileType;
///
/// let aac = AAC::load("audio.aac").unwrap();
///
/// // AAC module is read-only - no save() method
/// // Tags field is always None for raw AAC files
/// assert!(aac.tags.is_none());
///
/// // For metadata, use external tag formats or convert to M4A:
/// // 1. Add ID3v2 tags (prepended)
/// // 2. Add APEv2 tags (appended)
/// // 3. Convert to M4A (AAC in MP4 container) for native iTunes tags
/// ```
#[derive(Debug)]
pub struct AAC {
    pub info: AACInfo,
    pub tags: Option<ID3Tags>,
    pub filename: Option<String>,
}

impl AAC {
    /// Create a new AAC instance
    pub fn new() -> Self {
        AAC {
            info: AACInfo::default(),
            tags: None,
            filename: None,
        }
    }

    /// Parse AAC file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        self.info = AACInfo::from_reader(reader)?;
        self.tags = None;
        Ok(())
    }

    /// Load AAC file asynchronously
    ///
    /// Opens the file at the specified path and parses ADTS frame headers
    /// to extract stream information including sample rate, channels, and bitrate.
    /// AAC files are read-only and do not support metadata tag operations.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the AAC file to load
    ///
    /// # Returns
    ///
    /// Returns a Result containing the AAC instance with parsed stream information,
    /// or an error if the file cannot be opened or parsed.
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut aac = AAC::new();
        aac.filename = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info
        aac.info = Self::parse_info_async(&mut file).await?;

        Ok(aac)
    }

    /// Parse stream information from an AAC file asynchronously.
    ///
    /// Reads only the header portion of the file (up to 256 KB) via async I/O
    /// for ADTS/ADIF frame scanning, plus one async seek for total file size.
    /// The sync parser only ever reads a small number of ADTS frames from the
    /// start, so buffering the entire file is unnecessary.
    #[cfg(feature = "async")]
    async fn parse_info_async(file: &mut TokioFile) -> Result<AACInfo> {
        use std::io::Cursor;

        // Determine actual file size via async seek — needed for duration
        let file_size = file.seek(SeekFrom::End(0)).await?;

        // Read only the header portion. The sync parser scans at most ~50 KB
        // (10 sync tries × 512-byte search window + 100 small frame reads).
        // 256 KB is a generous margin without buffering multi-GB files.
        const MAX_HEADER_READ: u64 = 256 * 1024;
        let read_size = std::cmp::min(file_size, MAX_HEADER_READ) as usize;

        file.seek(SeekFrom::Start(0)).await?;
        let mut header_data = vec![0u8; read_size];
        file.read_exact(&mut header_data).await?;

        // Parse the header buffer with the sync parser. Disambiguate
        // std::io traits from the tokio async traits in scope.
        let mut cursor = Cursor::new(&header_data[..]);

        // Skip ID3v2 header if present (same logic as sync from_reader)
        let mut id3_header = [0u8; 10];
        if std::io::Read::read_exact(&mut cursor, &mut id3_header).is_err() {
            return Err(AudexError::AACError("file too small".to_string()));
        }

        let start_offset = if &id3_header[0..3] == b"ID3" {
            let size = ((id3_header[6] as u32 & 0x7F) << 21)
                | ((id3_header[7] as u32 & 0x7F) << 14)
                | ((id3_header[8] as u32 & 0x7F) << 7)
                | (id3_header[9] as u32 & 0x7F);
            (size + 10) as u64
        } else {
            0
        };

        std::io::Seek::seek(&mut cursor, SeekFrom::Start(start_offset))?;

        // Detect ADIF vs ADTS
        let mut format_header = [0u8; 4];
        std::io::Read::read_exact(&mut cursor, &mut format_header)
            .map_err(|_| AudexError::AACError("cannot read format header".to_string()))?;

        // Pass the real file size so duration is computed from the actual
        // stream length rather than the truncated header buffer.
        if &format_header == b"ADIF" {
            AACInfo::parse_adif_with_file_size(&mut cursor, Some(file_size))
        } else {
            std::io::Seek::seek(&mut cursor, SeekFrom::Start(start_offset))?;
            AACInfo::parse_adts_with_file_size(&mut cursor, start_offset, Some(file_size))
        }
    }
}

impl Default for AAC {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for AAC {
    type Tags = ID3Tags;
    type Info = AACInfo;

    fn format_id() -> &'static str {
        "AAC"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing AAC stream info");
        let mut file = std::fs::File::open(&path)?;
        let mut aac = AAC::new();
        aac.filename = Some(path.as_ref().to_string_lossy().to_string());
        aac.parse_file(&mut file)?;
        Ok(aac)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing AAC stream info from reader");
        let mut instance = Self::new();
        let mut reader = reader;
        instance.parse_file(&mut reader)?;
        Ok(instance)
    }

    fn save(&mut self) -> Result<()> {
        Err(AudexError::TagOperationUnsupported(
            "AAC doesn't support embedded tags".to_string(),
        ))
    }

    fn clear(&mut self) -> Result<()> {
        Err(AudexError::TagOperationUnsupported(
            "AAC doesn't support embedded tags".to_string(),
        ))
    }

    /// AAC format does not support embedded metadata tags.
    ///
    /// This method always returns an error since AAC is a read-only format
    /// for metadata purposes. AAC files (ADTS/ADIF) do not have a standard
    /// metadata container.
    ///
    /// # Errors
    ///
    /// Always returns `AudexError::TagOperationUnsupported`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::aac::AAC;
    /// use audex::FileType;
    ///
    /// let mut aac = AAC::load("audio.aac")?;
    /// // AAC doesn't support tags
    /// assert!(aac.add_tags().is_err());
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        Err(AudexError::TagOperationUnsupported(
            "AAC doesn't support embedded tags".to_string(),
        ))
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
        let filename_lower = filename.to_lowercase();
        let mut score = 0i32;

        // Support AAC file extensions (.aac, .aacp for AAC+, .adts, .adif)
        if filename_lower.ends_with(".aac")
            || filename_lower.ends_with(".aacp")
            || filename_lower.ends_with(".adts")
            || filename_lower.ends_with(".adif")
        {
            score += 1;
        }

        if header.len() >= 4 && &header[..4] == b"ADIF" {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/x-aac", "audio/aac", "audio/aacp"]
    }
}

/// Standalone functions for AAC operations
pub fn clear<P: AsRef<Path>>(_path: P) -> Result<()> {
    Err(AudexError::TagOperationUnsupported(
        "AAC doesn't support embedded tags".to_string(),
    ))
}
