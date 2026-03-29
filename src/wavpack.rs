//! Support for WavPack audio files.
//!
//! This module provides comprehensive support for WavPack files, a versatile audio
//! compression format offering lossless, lossy, and hybrid compression modes. WavPack
//! combines excellent compression ratios with fast encoding/decoding and robust error
//! correction capabilities.
//!
//! # File Format
//!
//! WavPack supports three compression modes:
//! - **Lossless**: Bit-perfect compression, typically 30-70% of original size
//! - **Lossy**: High-quality lossy compression similar to AAC/Vorbis
//! - **Hybrid**: Lossless file split into lossy base + correction file
//!
//! # Audio Characteristics
//!
//! - **Sample Rates**: 6 kHz to 192 kHz
//! - **Bit Depth**: 8, 16, 24, or 32 bits per sample
//! - **Channels**: 1-2 (stereo), with multichannel support in version 5+
//! - **Special**: DSD (Direct Stream Digital) support
//! - **File Extension**: `.wv` (main), `.wvc` (correction file for hybrid mode)
//! - **MIME Type**: `audio/x-wavpack`
//!
//! # Tagging
//!
//! WavPack uses APEv2 tags for metadata:
//! - **Standard fields**: Title, Artist, Album, Year, Track, Genre
//! - **Binary support**: Embedded cover art
//! - **UTF-8 encoding**: Full Unicode support
//!
//! # Basic Usage
//!
//! ```no_run
//! use audex::wavpack::WavPack;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut wv = WavPack::load("song.wv")?;
//!
//! // Read stream information
//! println!("Sample Rate: {} Hz", wv.info.sample_rate);
//! println!("Channels: {}", wv.info.channels);
//! println!("Bits per Sample: {}", wv.info.bits_per_sample);
//!
//! // Modify tags
//! if let Some(tags) = wv.tags_mut() {
//!     tags.set_text("Title", "Song Title".to_string())?;
//!     tags.set_text("Artist", "Artist Name".to_string())?;
//! }
//!
//! wv.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # References
//!
//! - [WavPack Official Site](http://www.wavpack.com/)
//! - [File Format Specification](http://www.wavpack.com/file_format.txt)

use crate::{AudexError, FileType, Result, StreamInfo, apev2::APEv2Tags};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::util::loadfile_read_async;
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// WavPack sample rates table
const RATES: [u32; 15] = [
    6000, 8000, 9600, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000,
    192000,
];

/// WavPack block header structure (32 bytes).
///
/// Each WavPack file consists of one or more blocks, each starting with this header.
/// The header contains essential stream information encoded in flags and fields.
///
/// # Header Layout
///
/// - Bytes 0-3: "wvpk" signature
/// - Bytes 4-7: Block size (excluding header)
/// - Bytes 8-9: Version number
/// - Bytes 10-11: Track and index numbers
/// - Bytes 12-15: Total samples in file (0xFFFFFFFF if unknown)
/// - Bytes 16-19: Block index (sample position)
/// - Bytes 20-23: Number of samples in this block
/// - Bytes 24-27: Flags (encoding mode, sample rate, channels, bits per sample)
/// - Bytes 28-31: CRC checksum
#[derive(Debug)]
pub struct WavPackHeader {
    /// Size of block data (excluding 32-byte header)
    pub block_size: u32,
    /// WavPack version (e.g., 0x410 for version 4.1)
    pub version: u16,
    /// Track number (optional, typically 0)
    pub track_no: u8,
    /// Index number (optional, typically 0)
    pub index_no: u8,
    /// Total samples in file (0xFFFFFFFF if unknown)
    pub total_samples: u32,
    /// Sample position of first sample in block
    pub block_index: u32,
    /// Number of samples in this block
    pub block_samples: u32,
    /// Encoding flags (mono, sample rate, bits per sample, DSD, etc.)
    pub flags: u32,
    /// CRC-32 checksum of block data
    pub crc: u32,
}

impl WavPackHeader {
    /// Parse WavPack header from reader
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self> {
        let mut header = [0u8; 32];
        reader
            .read_exact(&mut header)
            .map_err(|_| AudexError::WavPackHeaderError("not enough data".to_string()))?;

        // Check signature
        if &header[0..4] != b"wvpk" {
            return Err(AudexError::WavPackHeaderError(format!(
                "not a WavPack header: {:?}",
                &header[0..4]
            )));
        }

        // Parse fields (all little-endian)
        let block_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let version = u16::from_le_bytes([header[8], header[9]]);
        let track_no = header[10];
        let index_no = header[11];
        let total_samples = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
        let block_index = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);
        let block_samples = u32::from_le_bytes([header[20], header[21], header[22], header[23]]);
        let flags = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
        let crc = u32::from_le_bytes([header[28], header[29], header[30], header[31]]);

        Ok(WavPackHeader {
            block_size,
            version,
            track_no,
            index_no,
            total_samples,
            block_index,
            block_samples,
            flags,
            crc,
        })
    }
}

/// Audio stream information for WavPack files.
///
/// Contains technical details extracted from the WavPack block header, including
/// sample rate, bit depth, channel count, and version information.
///
/// # Fields
///
/// - **`sample_rate`**: Sample rate in Hz (6000-192000)
/// - **`channels`**: Number of audio channels (1-2, or more in v5+)
/// - **`bits_per_sample`**: Bits per sample (8, 16, 24, or 32)
/// - **`version`**: WavPack version number (e.g., 0x410 = version 4.1)
/// - **`length`**: Audio duration
/// - **`bitrate`**: Average bitrate in bps
#[derive(Debug, Default)]
pub struct WavPackStreamInfo {
    /// Audio duration
    pub length: Option<Duration>,
    /// Average bitrate in bps
    pub bitrate: Option<u32>,
    /// Number of audio channels
    pub channels: u32,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Bits per sample (8, 16, 24, or 32)
    pub bits_per_sample: u32,
    /// WavPack version number
    pub version: u16,
}

impl StreamInfo for WavPackStreamInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }
    fn bitrate(&self) -> Option<u32> {
        self.bitrate
    }
    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate)
    }
    fn channels(&self) -> Option<u16> {
        u16::try_from(self.channels).ok()
    }
    fn bits_per_sample(&self) -> Option<u16> {
        u16::try_from(self.bits_per_sample).ok()
    }
}

impl WavPackStreamInfo {
    /// Parse WavPack file and extract stream information
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        reader.seek(SeekFrom::Start(0))?;

        // Parse first header
        let mut header = WavPackHeader::from_reader(reader)?;

        let mut info = WavPackStreamInfo {
            version: header.version,
            ..Default::default()
        };

        // Extract channels from flags
        // Bit 2: mono flag (if set, mono; otherwise stereo)
        info.channels = if (header.flags & 4) != 0 { 1 } else { 2 };

        // Extract sample rate from flags (bits 23-26)
        let rate_index = ((header.flags >> 23) & 0xF) as usize;
        if rate_index >= RATES.len() {
            return Err(AudexError::WavPackHeaderError(
                "invalid sample rate index".to_string(),
            ));
        }
        info.sample_rate = RATES[rate_index];

        // Extract bits per sample from flags (bits 0-1)
        info.bits_per_sample = ((header.flags & 3) + 1) * 8;

        // Handle DSD format (Direct Stream Digital)
        // Bit 31: DSD flag
        if (header.flags >> 31) & 1 != 0 {
            // DSD64 multiplier (most common). Use saturating arithmetic
            // to guard against overflow if the rate table ever changes.
            info.sample_rate = info.sample_rate.saturating_mul(4);
            info.bits_per_sample = 1;
        }

        // Calculate length
        let samples = if header.total_samples == 0xFFFFFFFF || header.block_index != 0 {
            // Need to scan through all blocks to get total sample count
            let mut total_samples = header.block_samples;
            let initial_pos = reader.stream_position()?;

            // Cap the scan to prevent unbounded iteration on crafted
            // files. 1 million blocks is far beyond any realistic file.
            const MAX_BLOCK_SCAN: u32 = 1_000_000;
            let mut blocks_scanned: u32 = 0;

            loop {
                blocks_scanned += 1;
                if blocks_scanned > MAX_BLOCK_SCAN {
                    break;
                }
                // Skip past the current block's data to the next header.
                // block_size describes the data after the initial 8-byte
                // prefix (signature + size field). The remaining 24 bytes
                // of the 32-byte header are included in block_size, so
                // values below 24 are invalid and would cause the seek
                // to land mid-header, corrupting the scan.
                if header.block_size < 24 {
                    break;
                }
                // block_size counts bytes after the 8-byte prefix (sig + size).
                // We already consumed the remaining 24 header bytes in
                // from_reader(), so the payload left to skip is block_size - 24.
                let skip_size = header.block_size - 24;
                reader.seek(SeekFrom::Current(skip_size as i64))?;

                match WavPackHeader::from_reader(reader) {
                    Ok(next_header) => {
                        total_samples = total_samples.saturating_add(next_header.block_samples);
                        header = next_header;
                    }
                    Err(_) => break,
                }
            }

            // Restore position
            reader.seek(SeekFrom::Start(initial_pos))?;
            total_samples
        } else {
            header.total_samples
        };

        if info.sample_rate > 0 {
            let seconds = samples as f64 / info.sample_rate as f64;
            info.length = Duration::try_from_secs_f64(seconds).ok();
        } else {
            info.length = None;
        }

        Ok(info)
    }

    /// Pretty print stream info
    pub fn pprint(&self) -> String {
        format!(
            "WavPack, {:.2} seconds, {} Hz",
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0),
            self.sample_rate
        )
    }
}

/// Represents a WavPack audio file with metadata and stream information.
///
/// Provides access to both audio stream details (sample rate, channels, bit depth)
/// and APEv2 metadata tags. WavPack files can be lossless, lossy, or hybrid mode.
///
/// File extension: `.wv` (main file), `.wvc` (correction file for hybrid mode)
#[derive(Debug)]
pub struct WavPack {
    /// Audio stream information
    pub info: WavPackStreamInfo,
    /// Optional APEv2 metadata tags
    pub tags: Option<APEv2Tags>,
    /// Path to the file (used for saving)
    pub filename: Option<String>,
}

impl WavPack {
    /// Create a new empty WavPack instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: WavPackStreamInfo::default(),
            tags: None,
            filename: None,
        }
    }

    /// Parse WavPack file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Parse stream info
        self.info = WavPackStreamInfo::from_reader(reader)?;

        // Parse APEv2 tags
        if let Some(filename) = &self.filename {
            match crate::apev2::APEv2::load(filename) {
                Ok(ape) => self.tags = Some(ape.tags),
                Err(_) => self.tags = None, // No APE tags or parsing failed
            }
        }

        Ok(())
    }

    /// Add empty APEv2 tags
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "APEv2 tag already exists".to_string(),
            ));
        }
        self.tags = Some(APEv2Tags::new());
        Ok(())
    }

    /// Clear APEv2 tags
    pub fn clear(&mut self) -> Result<()> {
        if let Some(ref filename) = self.filename {
            // Use APEv2::clear to properly remove tags from the file
            crate::apev2::clear(filename)?;
        }
        self.tags = None;
        Ok(())
    }

    /// Get MIME types
    pub fn mime(&self) -> Vec<&'static str> {
        vec!["audio/x-wavpack"]
    }

    /// Pretty print file info
    pub fn pprint(&self) -> String {
        self.info.pprint()
    }

    /// Load WavPack file asynchronously
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut wv = WavPack::new();
        wv.filename = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info
        wv.info = Self::parse_info_async(&mut file).await?;

        // Load APEv2 tags
        match crate::apev2::APEv2::load_async(&path).await {
            Ok(ape) => wv.tags = Some(ape.tags),
            Err(AudexError::APENoHeader) => wv.tags = None,
            Err(e) => return Err(e),
        }

        Ok(wv)
    }

    /// Parse stream information asynchronously.
    ///
    /// WavPack files may need a full block-header scan to determine total
    /// sample count (when `total_samples == 0xFFFFFFFF`). This reads each
    /// 32-byte block header via async I/O and seeks past the audio payload,
    /// so only a few KB of metadata is ever buffered — regardless of how
    /// large the audio file is.
    #[cfg(feature = "async")]
    async fn parse_info_async(file: &mut TokioFile) -> Result<WavPackStreamInfo> {
        file.seek(SeekFrom::Start(0)).await?;

        // Read the first 32-byte block header
        let mut header = Self::read_header_async(file).await?;

        let mut info = WavPackStreamInfo {
            version: header.version,
            ..Default::default()
        };

        // Extract channels from flags (bit 2: mono flag)
        info.channels = if (header.flags & 4) != 0 { 1 } else { 2 };

        // Extract sample rate from flags (bits 23-26)
        let rate_index = ((header.flags >> 23) & 0xF) as usize;
        if rate_index >= RATES.len() {
            return Err(AudexError::WavPackHeaderError(
                "invalid sample rate index".to_string(),
            ));
        }
        info.sample_rate = RATES[rate_index];

        // Extract bits per sample from flags (bits 0-1)
        info.bits_per_sample = ((header.flags & 3) + 1) * 8;

        // Handle DSD format (bit 31)
        if (header.flags >> 31) & 1 != 0 {
            // DSD64 multiplier (most common). Use saturating arithmetic
            // to guard against overflow if the rate table ever changes.
            info.sample_rate = info.sample_rate.saturating_mul(4);
            info.bits_per_sample = 1;
        }

        // Determine total sample count
        let samples = if header.total_samples == 0xFFFFFFFF || header.block_index != 0 {
            // Scan all block headers to accumulate total samples.
            // Each iteration does one async seek + one async 32-byte read.
            // Cap the scan to prevent unbounded iteration on crafted files.
            const MAX_BLOCK_SCAN: u32 = 1_000_000;
            let mut blocks_scanned: u32 = 0;
            let mut total_samples = header.block_samples;
            let initial_pos = file.stream_position().await?;

            loop {
                blocks_scanned += 1;
                if blocks_scanned > MAX_BLOCK_SCAN {
                    break;
                }
                // block_size describes the data after the initial 8-byte prefix.
                // The remaining 24 bytes of the 32-byte header are included in
                // block_size, so values below 24 are invalid and would cause
                // the seek to land mid-header.
                if header.block_size < 24 {
                    break;
                }
                // Skip past the block data to reach the next header.
                let skip_size = header.block_size - 24;
                file.seek(SeekFrom::Current(skip_size as i64)).await?;

                match Self::read_header_async(file).await {
                    Ok(next_header) => {
                        total_samples = total_samples.saturating_add(next_header.block_samples);
                        header = next_header;
                    }
                    Err(_) => break,
                }
            }

            file.seek(SeekFrom::Start(initial_pos)).await?;
            total_samples
        } else {
            header.total_samples
        };

        if info.sample_rate > 0 {
            info.length = Some(Duration::from_secs_f64(
                samples as f64 / info.sample_rate as f64,
            ));
        } else {
            info.length = None;
        }

        Ok(info)
    }

    /// Read a single 32-byte WavPack block header via async I/O.
    #[cfg(feature = "async")]
    async fn read_header_async(file: &mut TokioFile) -> Result<WavPackHeader> {
        let mut buf = [0u8; 32];
        file.read_exact(&mut buf)
            .await
            .map_err(|_| AudexError::WavPackHeaderError("not enough data".to_string()))?;

        if &buf[0..4] != b"wvpk" {
            return Err(AudexError::WavPackHeaderError(format!(
                "not a WavPack header: {:?}",
                &buf[0..4]
            )));
        }

        Ok(WavPackHeader {
            block_size: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            version: u16::from_le_bytes([buf[8], buf[9]]),
            track_no: buf[10],
            index_no: buf[11],
            total_samples: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
            block_index: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
            block_samples: u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]),
            flags: u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]),
            crc: u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]),
        })
    }

    /// Save tags asynchronously
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        let filename = self
            .filename
            .clone()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        if let Some(ref tags) = self.tags {
            let mut ape = crate::apev2::APEv2::new();
            ape.filename = Some(filename);
            ape.tags = tags.clone();
            ape.save_async().await
        } else {
            Ok(())
        }
    }

    /// Clear tags asynchronously
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        if let Some(filename) = &self.filename {
            crate::apev2::clear_async(filename).await?;
        }
        self.tags = None;
        Ok(())
    }
}

impl Default for WavPack {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for WavPack {
    type Tags = APEv2Tags;
    type Info = WavPackStreamInfo;

    fn format_id() -> &'static str {
        "WavPack"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing WavPack file");
        let mut file = std::fs::File::open(&path)?;
        let mut wavpack = WavPack::new();
        wavpack.filename = Some(path.as_ref().to_string_lossy().to_string());

        wavpack.parse_file(&mut file)?;
        Ok(wavpack)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        let mut instance = Self::new();
        let mut reader = reader;
        instance.parse_file(&mut reader)?;

        // Parse APEv2 tags directly from the reader
        reader.seek(std::io::SeekFrom::Start(0))?;
        if let Ok(ape) = <crate::apev2::APEv2 as FileType>::load_from_reader(&mut reader) {
            instance.tags = Some(ape.tags);
        }

        Ok(instance)
    }

    fn save(&mut self) -> Result<()> {
        let filename = self
            .filename
            .as_ref()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        // Save APEv2 tags if present
        if let Some(tags) = &self.tags {
            let mut apev2 = crate::apev2::APEv2::new();
            apev2.filename = Some(filename.clone());

            // Copy all tag items
            for (key, value) in tags.items() {
                let _ = apev2.tags.set(&key, value.clone());
            }

            // Save to file
            apev2.save()?;
        }

        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        if let Some(ref filename) = self.filename {
            // Use APEv2::clear to properly remove tags from the file
            crate::apev2::clear(filename)?;
        }
        self.tags = None;
        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        if let Some(ref tags) = self.tags {
            let mut apev2 = crate::apev2::APEv2::new();
            for (key, value) in tags.items() {
                let _ = apev2.tags.set(&key, value.clone());
            }
            apev2.save_to_writer(writer)?;
        }
        Ok(())
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        let mut apev2 = crate::apev2::APEv2::new();
        apev2.clear_writer(writer)?;
        self.tags = None;
        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        if let Some(ref tags) = self.tags {
            let mut apev2 = crate::apev2::APEv2::new();
            apev2.filename = Some(path.to_string_lossy().to_string());
            for (key, value) in tags.items() {
                let _ = apev2.tags.set(&key, value.clone());
            }
            apev2.save()?;
        }
        Ok(())
    }

    /// Adds empty APEv2 tags to the file.
    ///
    /// Creates a new empty tag structure if none exists. If tags already exist,
    /// returns an error.
    ///
    /// # Errors
    ///
    /// Returns `AudexError::InvalidOperation` if tags already exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::wavpack::WavPack;
    /// use audex::FileType;
    ///
    /// let mut wv = WavPack::load("song.wv")?;
    /// if wv.tags.is_none() {
    ///     wv.add_tags()?;
    /// }
    /// wv.set("title", vec!["My Song".to_string()])?;
    /// wv.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(APEv2Tags::new());
        Ok(())
    }

    fn get(&self, key: &str) -> Option<Vec<String>> {
        // APEv2Tags stores values as APEValue, need to convert to Vec<String>
        self.tags.as_ref()?.get(key)?.as_text_list().ok()
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
        let mut score = 0;

        // Check for WavPack signature
        if header.len() >= 4 && &header[0..4] == b"wvpk" {
            score += 2;
        }

        // Check file extension
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".wv") {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/x-wavpack"]
    }
}

/// Standalone functions for WavPack operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut wavpack = WavPack::load(path)?;
    wavpack.clear()
}
