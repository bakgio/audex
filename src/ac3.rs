//! Support for AC-3 (Dolby Digital) and E-AC-3 audio files.
//!
//! This module provides support for AC-3 (Dolby Digital) and Enhanced AC-3 audio
//! formats, the industry-standard lossy surround sound codecs used in cinema, broadcast,
//! and home entertainment. AC-3 delivers multichannel audio in a compact format.
//!
//! **Note**: This module extracts stream information but does not handle embedded tags.
//! Use ID3v2 or APEv2 tagging for metadata on AC-3 files.
//!
//! # File Format
//!
//! AC-3/E-AC-3 are lossy surround sound formats:
//! - **AC-3 (Dolby Digital)**: Original format, up to 5.1 channels
//! - **E-AC-3 (Dolby Digital Plus)**: Enhanced version with improved efficiency
//! - **Bitstream format**: Frame-based audio with sync words
//! - **Widespread adoption**: DVD, Blu-ray, streaming, broadcast
//!
//! # Audio Characteristics
//!
//! ## AC-3 (Dolby Digital)
//!
//! - **Compression**: Lossy (perceptual coding)
//! - **Bitrate**: 32-640 kbps
//! - **Sample Rates**: 32 kHz, 44.1 kHz, 48 kHz
//! - **Channels**: 1.0 to 5.1 (mono to 5.1 surround)
//! - **File Extension**: `.ac3`
//! - **MIME Type**: `audio/ac3`, `audio/vnd.dolby.dd-raw`
//!
//! ## E-AC-3 (Enhanced AC-3 / Dolby Digital Plus)
//!
//! - **Compression**: Lossy (improved perceptual coding)
//! - **Bitrate**: 32 kbps to 6.144 Mbps
//! - **Sample Rates**: 32 kHz, 44.1 kHz, 48 kHz
//! - **Channels**: 1.0 to 7.1 (including height channels)
//! - **File Extension**: `.ec3`, `.eac3`
//! - **MIME Type**: `audio/eac3`
//!
//! # Channel Configurations
//!
//! Common AC-3 channel modes:
//! - **1/0 (Mono)**: Single channel
//! - **2/0 (Stereo)**: Left, Right
//! - **3/0**: Left, Center, Right
//! - **2/1**: Left, Right, Surround
//! - **3/1**: Left, Center, Right, Surround
//! - **2/2**: Left, Right, Left Surround, Right Surround
//! - **3/2**: Left, Center, Right, Left Surround, Right Surround
//! - **+ LFE**: Any configuration plus Low Frequency Effects (subwoofer)
//!
//! # Basic Usage
//!
//! ```no_run
//! use audex::ac3::AC3;
//! use audex::FileType;
//!
//! # fn main() -> audex::Result<()> {
//! let ac3 = AC3::load("audio.ac3")?;
//!
//! println!("Sample Rate: {} Hz", ac3.info.sample_rate());
//! println!("Channels: {}", ac3.info.channels());
//! println!("Bitrate: {} kbps", ac3.info.bitrate() / 1000);
//! println!("Format: {}", if ac3.info.is_eac3() { "E-AC-3" } else { "AC-3" });
//!
//! if let Some(duration) = ac3.info.length() {
//!     println!("Duration: {:.2} seconds", duration.as_secs_f64());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Tagging
//!
//! AC-3 files themselves do not contain embedded metadata. For tagging:
//! - **ID3v2**: Prepend ID3v2 tags before the AC-3 sync frame
//! - **APEv2**: Append APEv2 tags after the AC-3 data
//!
//! # References
//!
//! - [ATSC A/52 Standard (AC-3)](https://www.atsc.org/atsc-documents/a522016-digital-audio-compression-ac-3-e-ac-3/)
//! - [Dolby Digital (AC-3) Specification](https://professional.dolby.com/product/dolby-digital/)

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

/// Minimum AC-3 frame header size in bytes (sync word + CRC + header fields)
const AC3_HEADER_SIZE: u16 = 7;

/// AC-3 sync word bytes: `0x0B77`
const AC3_SYNC_WORD: [u8; 2] = [0x0B, 0x77];

/// AC-3 sample rate table indexed by `fscod` (2-bit field)
const AC3_SAMPLE_RATES: [u32; 3] = [48000, 44100, 32000];

/// AC-3 bitrate table (kbps) indexed by `frmsizecod >> 1`
const AC3_BITRATES: [u16; 19] = [
    32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640,
];

/// E-AC-3 audio block count table indexed by `numblkscod` (2-bit field)
const EAC3_BLOCKS: [u16; 4] = [1, 2, 3, 6];

/// AC-3 audio coding mode (`acmod`), determines speaker layout.
///
/// Each variant maps to a specific front/rear channel arrangement.
/// The LFE (subwoofer) channel is signaled separately via the `lfeon` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum ChannelMode {
    /// 1+1 dual mono (Ch1, Ch2)
    DualMono = 0,
    /// 1/0 mono (C)
    Mono = 1,
    /// 2/0 stereo (L, R)
    Stereo = 2,
    /// 3/0 (L, C, R)
    C3F = 3,
    /// 2/1 (L, R, S)
    C2F1R = 4,
    /// 3/1 (L, C, R, S)
    C3F1R = 5,
    /// 2/2 (L, R, SL, SR)
    C2F2R = 6,
    /// 3/2 (L, C, R, SL, SR)
    C3F2R = 7,
}

impl ChannelMode {
    /// Create from bit value
    fn from_bits(value: u64) -> Result<Self> {
        match value {
            0 => Ok(ChannelMode::DualMono),
            1 => Ok(ChannelMode::Mono),
            2 => Ok(ChannelMode::Stereo),
            3 => Ok(ChannelMode::C3F),
            4 => Ok(ChannelMode::C2F1R),
            5 => Ok(ChannelMode::C3F1R),
            6 => Ok(ChannelMode::C2F2R),
            7 => Ok(ChannelMode::C3F2R),
            _ => Err(AudexError::AC3Error(format!(
                "invalid channel mode: {}",
                value
            ))),
        }
    }

    fn base_channels(&self) -> u16 {
        match self {
            ChannelMode::DualMono => 2,
            ChannelMode::Mono => 1,
            ChannelMode::Stereo => 2,
            ChannelMode::C3F => 3,
            ChannelMode::C2F1R => 3,
            ChannelMode::C3F1R => 4,
            ChannelMode::C2F2R => 4,
            ChannelMode::C3F2R => 5,
        }
    }
}

/// E-AC-3 frame type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum EAC3FrameType {
    Independent = 0,
    Dependent = 1,
    AC3Convert = 2,
}

impl EAC3FrameType {
    /// Create from bit value
    fn from_bits(value: u64) -> Result<Self> {
        match value {
            0 => Ok(EAC3FrameType::Independent),
            1 => Ok(EAC3FrameType::Dependent),
            2 => Ok(EAC3FrameType::AC3Convert),
            3 => Err(AudexError::AC3Error(format!(
                "invalid frame type: {}",
                value
            ))),
            _ => Err(AudexError::AC3Error(format!(
                "invalid frame type: {}",
                value
            ))),
        }
    }
}

/// AC-3 stream information
#[derive(Debug, Clone, Default)]
pub struct AC3Info {
    /// Number of audio channels
    channels: u16,

    /// File length in seconds (estimated)
    length: Option<Duration>,

    /// Audio sampling rate in Hz
    sample_rate: u32,

    /// Audio bitrate in bits per second
    bitrate: u32,

    /// Codec type (ac-3 or ec-3)
    codec: String,
}

impl AC3Info {
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn bitrate(&self) -> u32 {
        self.bitrate
    }

    pub fn is_eac3(&self) -> bool {
        self.codec == "ec-3"
    }

    pub fn codec(&self) -> &str {
        &self.codec
    }

    pub fn length(&self) -> Option<Duration> {
        self.length
    }

    /// Parse AC-3 stream information from a file
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut header = [0u8; 6];
        reader.read_exact(&mut header)?;

        if header[0..2] != AC3_SYNC_WORD {
            return Err(AudexError::AC3Error("not an AC3 file".to_string()));
        }

        let bitstream_id = header[5] >> 3;
        if bitstream_id > 16 {
            return Err(AudexError::AC3Error(format!(
                "invalid bitstream_id {}",
                bitstream_id
            )));
        }

        // Seek back to position after sync word
        reader.seek(SeekFrom::Start(2))?;

        let mut info = Self::read_header(reader, bitstream_id)?;
        info.length = Some(info.guess_length(reader)?);

        Ok(info)
    }

    /// Read header based on bitstream ID
    pub(crate) fn read_header<R: Read + Seek>(reader: &mut R, bitstream_id: u8) -> Result<Self> {
        let mut bit_reader =
            BitReader::new(reader).map_err(|e| AudexError::ParseError(e.to_string()))?;

        if bitstream_id <= 10 {
            // Normal AC-3
            Self::read_header_normal(&mut bit_reader, bitstream_id)
        } else {
            // Enhanced AC-3
            Self::read_header_enhanced(&mut bit_reader)
        }
    }

    /// Read normal AC-3 header
    fn read_header_normal<R: Read + Seek>(
        bit_reader: &mut BitReader<R>,
        bitstream_id: u8,
    ) -> Result<Self> {
        // Skip CRC
        bit_reader
            .skip(16)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        // Sample rate code
        let sr_code = bit_reader
            .read_bits(2)
            .map_err(|e| AudexError::ParseError(e.to_string()))? as u8;
        if sr_code == 3 {
            return Err(AudexError::AC3Error(format!(
                "invalid sample rate code {}",
                sr_code
            )));
        }

        // Frame size code
        let frame_size_code = bit_reader
            .read_bits(6)
            .map_err(|e| AudexError::ParseError(e.to_string()))?
            as u8;
        if frame_size_code > 37 {
            return Err(AudexError::AC3Error(format!(
                "invalid frame size code {}",
                frame_size_code
            )));
        }

        // Skip bitstream ID (already read)
        bit_reader
            .skip(5)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        // Skip bitstream mode
        bit_reader
            .skip(3)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        // Channel mode
        let channel_mode_bits = bit_reader
            .read_bits(3)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let channel_mode = ChannelMode::from_bits(channel_mode_bits)?;

        // Skip dolby surround mode or surround mix level
        bit_reader
            .skip(2)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        // LFE on
        let lfe_on = bit_reader
            .read_bits(1)
            .map_err(|e| AudexError::ParseError(e.to_string()))? as u16;

        // Calculate sample rate shift for low sample rates
        let sr_shift = bitstream_id.max(8) - 8;

        let sample_rate = *AC3_SAMPLE_RATES.get(sr_code as usize).ok_or_else(|| {
            AudexError::AC3Error(format!("sample rate code {} out of range", sr_code))
        })? >> sr_shift;
        let bitrate = ((*AC3_BITRATES
            .get((frame_size_code >> 1) as usize)
            .ok_or_else(|| {
                AudexError::AC3Error(format!("frame size code {} out of range", frame_size_code))
            })? as u32)
            * 1000)
            >> sr_shift;
        let channels = channel_mode.base_channels() + lfe_on;

        // Skip unused header bits
        Self::skip_unused_header_bits_normal(bit_reader, channel_mode)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        Ok(AC3Info {
            channels,
            length: None,
            sample_rate,
            bitrate,
            codec: "ac-3".to_string(),
        })
    }

    /// Read enhanced AC-3 header
    fn read_header_enhanced<R: Read + Seek>(bit_reader: &mut BitReader<R>) -> Result<Self> {
        // Frame type
        let frame_type_bits = bit_reader
            .read_bits(2)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let frame_type = EAC3FrameType::from_bits(frame_type_bits)?;

        // Skip substream ID
        bit_reader
            .skip(3)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        // Frame size
        let frame_size_raw = (bit_reader
            .read_bits(11)
            .map_err(|e| AudexError::ParseError(e.to_string()))?
            + 1)
            << 1;
        let frame_size = u16::try_from(frame_size_raw).map_err(|_| {
            AudexError::AC3Error(format!(
                "E-AC-3 frame size {} exceeds u16 range",
                frame_size_raw
            ))
        })?;
        if frame_size < AC3_HEADER_SIZE {
            return Err(AudexError::AC3Error(format!(
                "invalid frame size {}",
                frame_size
            )));
        }

        // Sample rate code
        let sr_code = bit_reader
            .read_bits(2)
            .map_err(|e| AudexError::ParseError(e.to_string()))? as u8;

        let (numblocks_code, sample_rate) = if sr_code == 3 {
            // Half sample rate
            let sr_code2 = bit_reader
                .read_bits(2)
                .map_err(|e| AudexError::ParseError(e.to_string()))?
                as u8;
            if sr_code2 == 3 {
                return Err(AudexError::AC3Error(format!(
                    "invalid sample rate code {}",
                    sr_code2
                )));
            }
            (
                3u8,
                *AC3_SAMPLE_RATES.get(sr_code2 as usize).ok_or_else(|| {
                    AudexError::AC3Error(format!("sample rate code {} out of range", sr_code2))
                })? / 2,
            )
        } else {
            let numblocks_code = bit_reader
                .read_bits(2)
                .map_err(|e| AudexError::ParseError(e.to_string()))?
                as u8;
            (
                numblocks_code,
                *AC3_SAMPLE_RATES.get(sr_code as usize).ok_or_else(|| {
                    AudexError::AC3Error(format!("sample rate code {} out of range", sr_code))
                })?,
            )
        };

        // Channel mode
        let channel_mode_bits = bit_reader
            .read_bits(3)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;
        let channel_mode = ChannelMode::from_bits(channel_mode_bits)?;

        // LFE on
        let lfe_on = bit_reader
            .read_bits(1)
            .map_err(|e| AudexError::ParseError(e.to_string()))? as u16;

        // Calculate bitrate (bounds-check the lookup table index)
        let num_blocks = *EAC3_BLOCKS.get(numblocks_code as usize).ok_or_else(|| {
            AudexError::InvalidData(format!("E-AC-3 numblkscod {} out of range", numblocks_code))
        })? as u32;
        let bitrate = (8 * frame_size as u32 * sample_rate) / (num_blocks * 256);

        // Skip bitstream ID (already read)
        bit_reader
            .skip(5)
            .map_err(|e| AudexError::ParseError(e.to_string()))?;

        let channels = channel_mode.base_channels() + lfe_on;

        // Skip unused header bits
        Self::skip_unused_header_bits_enhanced(
            bit_reader,
            frame_type,
            channel_mode,
            sr_code,
            numblocks_code,
        )
        .map_err(|e| AudexError::ParseError(e.to_string()))?;

        Ok(AC3Info {
            channels,
            length: None,
            sample_rate,
            bitrate,
            codec: "ec-3".to_string(),
        })
    }

    /// Skip unused header bits for normal AC-3
    fn skip_unused_header_bits_normal<R: Read + Seek>(
        bit_reader: &mut BitReader<R>,
        channel_mode: ChannelMode,
    ) -> std::result::Result<(), crate::util::BitReaderError> {
        // Dialogue Normalization
        bit_reader.skip(5)?;

        // Compression Gain Word
        if bit_reader.read_bits(1)? == 1 {
            bit_reader.skip(8)?;
        }

        // Language Code
        if bit_reader.read_bits(1)? == 1 {
            bit_reader.skip(8)?;
        }

        // Audio Production Information
        if bit_reader.read_bits(1)? == 1 {
            bit_reader.skip(7)?; // Mixing Level (5) + Room Type (2)
        }

        // Dual mono specific fields
        if channel_mode == ChannelMode::DualMono {
            bit_reader.skip(5)?; // Dialogue Normalization, ch2

            if bit_reader.read_bits(1)? == 1 {
                bit_reader.skip(8)?; // Compression Gain Word, ch2
            }

            if bit_reader.read_bits(1)? == 1 {
                bit_reader.skip(8)?; // Language Code, ch2
            }

            if bit_reader.read_bits(1)? == 1 {
                bit_reader.skip(7)?; // Audio Production Information, ch2
            }
        }

        // Copyright Bit + Original Bit Stream
        bit_reader.skip(2)?;

        // Time codes
        let timecod1e = bit_reader.read_bits(1)?;
        let timecod2e = bit_reader.read_bits(1)?;

        if timecod1e == 1 {
            bit_reader.skip(14)?;
        }
        if timecod2e == 1 {
            bit_reader.skip(14)?;
        }

        // Additional Bit Stream Information
        if bit_reader.read_bits(1)? == 1 {
            let addbsil = bit_reader.read_bits(6)?;
            bit_reader.skip(((addbsil + 1) * 8) as i32)?;
        }

        Ok(())
    }

    /// Skip unused header bits for enhanced AC-3
    fn skip_unused_header_bits_enhanced<R: Read + Seek>(
        bit_reader: &mut BitReader<R>,
        frame_type: EAC3FrameType,
        channel_mode: ChannelMode,
        sr_code: u8,
        numblocks_code: u8,
    ) -> std::result::Result<(), crate::util::BitReaderError> {
        // Dialogue Normalization
        bit_reader.skip(5)?;

        // Compression Gain Word
        if bit_reader.read_bits(1)? == 1 {
            bit_reader.skip(8)?;
        }

        // Dual mono specific fields
        if channel_mode == ChannelMode::DualMono {
            bit_reader.skip(5)?; // Dialogue Normalization, ch2
            if bit_reader.read_bits(1)? == 1 {
                bit_reader.skip(8)?; // Compression Gain Word, ch2
            }
        }

        // Channel map for dependent streams
        if frame_type == EAC3FrameType::Dependent && bit_reader.read_bits(1)? == 1 {
            bit_reader.skip(16)?; // chanmap
        }

        // Mix metadata
        if bit_reader.read_bits(1)? == 1 {
            return Ok(());
        }

        // Informational Metadata
        if bit_reader.read_bits(1)? == 1 {
            // bsmod (3) + Copyright Bit (1) + Original Bit Stream (1)
            bit_reader.skip(5)?;

            if channel_mode == ChannelMode::Stereo {
                // dsurmod (2) + dheadphonmod (2)
                bit_reader.skip(4)?;
            } else if channel_mode >= ChannelMode::C2F2R {
                bit_reader.skip(2)?; // dsurexmod
            }

            // Audio Production Information
            if bit_reader.read_bits(1)? == 1 {
                // Mixing Level (5) + Room Type (2) + adconvtyp (1)
                bit_reader.skip(8)?;
            }

            // Dual mono production info
            if channel_mode == ChannelMode::DualMono && bit_reader.read_bits(1)? == 1 {
                bit_reader.skip(8)?; // Mixing Level, ch2 (5) + Room Type, ch2 (2) + adconvtyp, ch2 (1)
            }

            // Source sample rate
            if sr_code < 3 {
                bit_reader.skip(1)?; // sourcefscod
            }
        }

        // Converter synchronization flag
        if frame_type == EAC3FrameType::Independent && numblocks_code == 3 {
            bit_reader.skip(1)?; // convsync
        }

        // AC-3 convert frame
        if frame_type == EAC3FrameType::AC3Convert
            && numblocks_code != 3
            && bit_reader.read_bits(1)? == 1
        {
            bit_reader.skip(6)?; // frmsizecod
        }

        // Additional Bit Stream Information
        if bit_reader.read_bits(1)? == 1 {
            let addbsil = bit_reader.read_bits(6)?;
            bit_reader.skip(((addbsil + 1) * 8) as i32)?;
        }

        Ok(())
    }

    /// Guess file length from bitrate and file size
    pub(crate) fn guess_length<R: Read + Seek>(&self, reader: &mut R) -> Result<Duration> {
        if self.bitrate == 0 {
            return Ok(Duration::from_secs(0));
        }

        let start = reader.stream_position()?;
        let end = reader.seek(SeekFrom::End(0))?;
        // Use saturating subtraction to prevent underflow when start > end
        let data_length = end.saturating_sub(start);

        let seconds = (8.0 * data_length as f64) / self.bitrate as f64;
        Ok(Duration::from_secs_f64(seconds))
    }
}

impl StreamInfo for AC3Info {
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
            "{}, {} Hz, {:.2} seconds, {} channel(s), {} bps",
            self.codec,
            self.sample_rate,
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0),
            self.channels,
            self.bitrate
        )
    }
}

impl std::fmt::Display for AC3Info {
    /// Formats stream information for display
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pprint())
    }
}

/// AC-3 file type
///
/// Loads AC-3 or Enhanced AC-3 (E-AC-3) streams.
/// Tagging is not supported - use ID3 or APEv2 classes directly instead.
#[derive(Debug)]
pub struct AC3 {
    pub info: AC3Info,
    pub tags: Option<ID3Tags>,
    pub filename: Option<String>,
}

impl AC3 {
    /// Create a new AC-3 instance
    pub fn new() -> Self {
        AC3 {
            info: AC3Info::default(),
            tags: None,
            filename: None,
        }
    }

    /// Parse AC-3 file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        self.info = AC3Info::from_reader(reader)?;
        self.tags = None;
        Ok(())
    }

    /// Loads AC-3 data from any readable and seekable source
    ///
    /// # Arguments
    /// * `reader` - Any Read + Seek source (file, cursor, etc.)
    pub fn load_from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut ac3 = AC3::new();
        ac3.parse_file(reader)?;
        Ok(ac3)
    }

    /// Load AC-3 file asynchronously
    ///
    /// Parses AC-3 frame headers to extract stream information including
    /// sample rate, bitrate, channel configuration, and estimated duration.
    /// AC-3 files are read-only and do not support metadata tags.
    ///
    /// # Arguments
    /// * `path` - Path to the AC-3 file
    ///
    /// # Returns
    /// An AC3 instance with parsed stream information
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or is not a valid AC-3 file
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut ac3 = AC3::new();
        ac3.filename = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info from AC-3 frame headers
        ac3.info = Self::parse_info_async(&mut file).await?;

        Ok(ac3)
    }

    /// Parse AC-3 stream information asynchronously
    ///
    /// Reads a small header buffer (256 bytes) via async I/O — matching what
    /// the sync `from_reader` actually reads (~30 bytes of header via BitReader).
    /// The file size needed for duration is obtained via async seek, then
    /// `guess_length` is called on a size-aware Cursor. This avoids reading
    /// the entire file into memory.
    #[cfg(feature = "async")]
    async fn parse_info_async(file: &mut TokioFile) -> Result<AC3Info> {
        file.seek(SeekFrom::Start(0)).await?;

        // Read the header portion (sync reads ~30 bytes; 256 is generous headroom)
        let mut header_buf = [0u8; 256];
        let bytes_read = file.read(&mut header_buf).await?;

        // Get the real file size for duration calculation (mirrors sync SeekFrom::End)
        let file_size = file.seek(SeekFrom::End(0)).await?;

        // Create a buffer padded to file_size so Cursor's SeekFrom::End(0) returns
        // the correct file size for guess_length. We only allocate header_buf worth of
        // real data — guess_length only seeks to End, it doesn't read from there.
        // Instead, use a two-step approach: parse header on small Cursor, then
        // calculate length from the real file size.
        let mut cursor = std::io::Cursor::new(&header_buf[..bytes_read]);

        // Replicate from_reader logic: read 6-byte header, validate, seek to 2, read_header
        let mut header = [0u8; 6];
        std::io::Read::read_exact(&mut cursor, &mut header)
            .map_err(|_| AudexError::AC3Error("not enough data".to_string()))?;

        if header[0..2] != AC3_SYNC_WORD {
            return Err(AudexError::AC3Error("not an AC3 file".to_string()));
        }

        let bitstream_id = header[5] >> 3;
        if bitstream_id > 16 {
            return Err(AudexError::AC3Error(format!(
                "invalid bitstream_id {}",
                bitstream_id
            )));
        }

        cursor.set_position(2);
        let mut info = AC3Info::read_header(&mut cursor, bitstream_id)?;

        // Calculate duration from real file size (mirrors guess_length logic)
        // guess_length uses: data_length = end - current_position, seconds = 8 * data_length / bitrate
        // After read_header, cursor position corresponds to where sync from_reader would be
        let header_end_pos = cursor.position();
        if info.bitrate > 0 {
            let data_length = file_size.saturating_sub(header_end_pos);
            info.length = Some(Duration::from_secs_f64(
                (8.0 * data_length as f64) / info.bitrate as f64,
            ));
        } else {
            info.length = Some(Duration::from_secs(0));
        }

        Ok(info)
    }
}

impl Default for AC3 {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for AC3 {
    type Tags = ID3Tags;
    type Info = AC3Info;

    fn format_id() -> &'static str {
        "AC3"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing AC-3 stream info");
        let mut file = std::fs::File::open(&path)?;
        let mut ac3 = AC3::new();
        ac3.filename = Some(path.as_ref().to_string_lossy().to_string());
        ac3.parse_file(&mut file)?;
        Ok(ac3)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing AC-3 stream info from reader");
        let mut instance = Self::new();
        let mut reader = reader;
        instance.parse_file(&mut reader)?;
        Ok(instance)
    }

    fn save(&mut self) -> Result<()> {
        Err(AudexError::TagOperationUnsupported(
            "AC-3 doesn't support embedded tags".to_string(),
        ))
    }

    fn clear(&mut self) -> Result<()> {
        Err(AudexError::TagOperationUnsupported(
            "AC-3 doesn't support embedded tags".to_string(),
        ))
    }

    /// AC-3 format does not support embedded metadata tags.
    ///
    /// This method always returns an error since AC-3 is a read-only format
    /// for metadata purposes.
    ///
    /// # Errors
    ///
    /// Always returns `AudexError::TagOperationUnsupported`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::ac3::AC3;
    /// use audex::FileType;
    ///
    /// let mut ac3 = AC3::load("audio.ac3")?;
    /// // AC-3 doesn't support tags
    /// assert!(ac3.add_tags().is_err());
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        Err(AudexError::TagOperationUnsupported(
            "AC-3 doesn't support embedded tags".to_string(),
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
        let mut score = 0i32;

        // Check sync word
        if header.starts_with(&AC3_SYNC_WORD) {
            score += 2;
        }

        // Check file extension
        let filename_lower = filename.to_lowercase();
        if filename_lower.ends_with(".ac3") || filename_lower.ends_with(".eac3") {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &[
            "audio/ac3",
            "audio/x-ac3",
            "audio/eac3",
            "audio/vnd.dolby.dd-raw",
        ]
    }
}

/// Standalone functions for AC-3 operations
pub fn clear<P: AsRef<Path>>(_path: P) -> Result<()> {
    Err(AudexError::TagOperationUnsupported(
        "AC-3 doesn't support embedded tags".to_string(),
    ))
}
