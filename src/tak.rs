//! Support for TAK (Tom's lossless Audio Kompressor) files.
//!
//! This module provides support for TAK, a highly efficient lossless audio compression
//! format developed by Thomas Becker. TAK achieves exceptional compression ratios while
//! maintaining fast encoding and decoding speeds, making it ideal for archival purposes.
//!
//! # File Format
//!
//! TAK is a lossless compression format featuring:
//! - **High compression**: Among the best compression ratios for lossless audio
//! - **Fast processing**: Optimized encoding and decoding algorithms
//! - **Error detection**: Built-in CRC and MD5 checksums
//! - **Seeking support**: Fast random access to any position
//!
//! # Audio Characteristics
//!
//! - **Compression**: Lossless (bit-perfect reproduction)
//! - **Sample Rates**: Up to 384 kHz
//! - **Bit Depth**: 8-39 bits per sample
//! - **Channels**: 1-16 channels
//! - **Compression Ratio**: Typically 40-60% of original size
//! - **File Extension**: `.tak`
//! - **MIME Type**: `audio/x-tak`
//!
//! # Tagging
//!
//! TAK uses APEv2 tags:
//! - **Standard fields**: Title, Artist, Album, Year, Track, Genre
//! - **Binary support**: Embedded cover art
//! - **UTF-8 encoding**: Full Unicode support
//!
//! # Basic Usage
//!
//! ```no_run
//! use audex::tak::TAK;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut tak = TAK::load("song.tak")?;
//!
//! println!("Sample Rate: {} Hz", tak.info.sample_rate);
//! println!("Channels: {}", tak.info.channels);
//! println!("Bits per Sample: {}", tak.info.bits_per_sample);
//!
//! if let Some(tags) = tak.tags_mut() {
//!     tags.set_text("Title", "Song Title".to_string())?;
//!     tags.set_text("Artist", "Artist Name".to_string())?;
//! }
//!
//! tak.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # References
//!
//! - [TAK Official Site](http://www.thbeck.de/Tak/Tak.html)

use crate::{AudexError, FileType, Result, StreamInfo, apev2::APEv2Tags};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::apev2::APEv2;
#[cfg(feature = "async")]
use crate::util::loadfile_read_async;
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// TAK metadata types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TAKMetadata {
    End = 0,
    StreamInfo = 1,
    SeekTable = 2, // Removed in TAK 1.1.1
    SimpleWaveData = 3,
    EncoderInfo = 4,
    UnusedSpace = 5,   // New in TAK 1.0.3
    MD5 = 6,           // New in TAK 1.1.1
    LastFrameInfo = 7, // New in TAK 1.1.1
}

impl TAKMetadata {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(TAKMetadata::End),
            1 => Some(TAKMetadata::StreamInfo),
            2 => Some(TAKMetadata::SeekTable),
            3 => Some(TAKMetadata::SimpleWaveData),
            4 => Some(TAKMetadata::EncoderInfo),
            5 => Some(TAKMetadata::UnusedSpace),
            6 => Some(TAKMetadata::MD5),
            7 => Some(TAKMetadata::LastFrameInfo),
            _ => None,
        }
    }
}

// Constants
const CRC_SIZE: u32 = 3;

const ENCODER_INFO_CODEC_BITS: u8 = 6;
const ENCODER_INFO_PROFILE_BITS: u8 = 4;
const ENCODER_INFO_TOTAL_BITS: u8 = ENCODER_INFO_CODEC_BITS + ENCODER_INFO_PROFILE_BITS;

const SIZE_INFO_FRAME_DURATION_BITS: u8 = 4;
const SIZE_INFO_SAMPLE_NUM_BITS: u8 = 35;
const SIZE_INFO_TOTAL_BITS: u8 = SIZE_INFO_FRAME_DURATION_BITS + SIZE_INFO_SAMPLE_NUM_BITS;

const AUDIO_FORMAT_DATA_TYPE_BITS: u8 = 3;
const AUDIO_FORMAT_SAMPLE_RATE_BITS: u8 = 18;
const AUDIO_FORMAT_SAMPLE_BITS_BITS: u8 = 5;
const AUDIO_FORMAT_CHANNEL_NUM_BITS: u8 = 4;
const AUDIO_FORMAT_HAS_EXTENSION_BITS: u8 = 1;
const AUDIO_FORMAT_BITS_MIN: u8 = 31;
const AUDIO_FORMAT_BITS_MAX: u8 = 31 + 102;

const SAMPLE_RATE_MIN: u32 = 6000;
const SAMPLE_BITS_MIN: u32 = 8;
const CHANNEL_NUM_MIN: u32 = 1;

const STREAM_INFO_BITS_MIN: u32 =
    ENCODER_INFO_TOTAL_BITS as u32 + SIZE_INFO_TOTAL_BITS as u32 + AUDIO_FORMAT_BITS_MIN as u32;
const STREAM_INFO_BITS_MAX: u32 =
    ENCODER_INFO_TOTAL_BITS as u32 + SIZE_INFO_TOTAL_BITS as u32 + AUDIO_FORMAT_BITS_MAX as u32;
const STREAM_INFO_SIZE_MIN: u32 = STREAM_INFO_BITS_MIN.div_ceil(8);
const STREAM_INFO_SIZE_MAX: u32 = STREAM_INFO_BITS_MAX.div_ceil(8);

/// LSB (Least Significant Bit first) BitReader for TAK format
pub struct LSBBitReader<R: Read> {
    reader: R,
    buffer: u32,
    bits: u8,
}

impl<R: Read> LSBBitReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: 0,
            bits: 0,
        }
    }

    /// Read count bits from LSB first
    pub fn bits(&mut self, count: u8) -> Result<u64> {
        if count > 64 {
            return Err(AudexError::TAKHeaderError(format!(
                "bit read count {} exceeds maximum of 64 for a u64 return value",
                count
            )));
        }
        if count == 0 {
            return Ok(0);
        }

        let mut value = 0u64;
        let mut remaining = count;
        let mut shift = 0;

        // Use available bits in buffer first
        if self.bits > 0 {
            let available = std::cmp::min(remaining, self.bits);
            // Safe mask: avoid undefined overflow when available == 32
            let mask = if available >= 32 {
                u32::MAX
            } else {
                (1u32 << available) - 1
            };
            let bits_value = self.buffer & mask;
            value = bits_value as u64;
            self.buffer >>= available;
            self.bits -= available;
            remaining -= available;
            shift = available;
        }

        // Read additional bytes as needed
        while remaining > 0 {
            let mut byte = [0u8; 1];
            self.reader
                .read_exact(&mut byte)
                .map_err(|_| AudexError::TAKHeaderError("not enough data".to_string()))?;

            let byte_val = byte[0] as u32;

            if remaining >= 8 {
                // Use full byte
                value |= (byte_val as u64) << shift;
                shift += 8;
                remaining -= 8;
            } else {
                // Use partial byte, save remainder in buffer
                let mask = (1u32 << remaining) - 1;
                let bits_value = byte_val & mask;
                value |= (bits_value as u64) << shift;

                // Save remaining bits in buffer
                self.buffer = byte_val >> remaining;
                self.bits = 8 - remaining;
                remaining = 0;
            }
        }

        Ok(value)
    }

    /// Skip count bits
    pub fn skip(&mut self, count: u8) -> Result<()> {
        self.bits(count)?;
        Ok(())
    }

    /// Read count bytes
    pub fn bytes(&mut self, count: usize) -> Result<Vec<u8>> {
        if self.bits != 0 {
            return Err(AudexError::TAKHeaderError("not byte aligned".to_string()));
        }

        let mut data = vec![0u8; count];
        self.reader
            .read_exact(&mut data)
            .map_err(|_| AudexError::TAKHeaderError("not enough data".to_string()))?;
        Ok(data)
    }

    /// Check if reader is byte aligned
    pub fn is_aligned(&self) -> bool {
        self.bits == 0
    }
}

/// TAK stream information
#[derive(Debug, Default)]
pub struct TAKStreamInfo {
    pub length: Option<Duration>,
    pub bitrate: Option<u32>,
    pub channels: u32,
    pub sample_rate: u32,
    pub bits_per_sample: u32,
    pub number_of_samples: u64,
    pub encoder_info: String,
}

impl StreamInfo for TAKStreamInfo {
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

impl TAKStreamInfo {
    /// Parse TAK file and extract stream information
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        reader.seek(SeekFrom::Start(0))?;

        let mut stream_id = [0u8; 4];
        reader
            .read_exact(&mut stream_id)
            .map_err(|_| AudexError::TAKHeaderError("not a TAK file".to_string()))?;

        if &stream_id != b"tBaK" {
            return Err(AudexError::TAKHeaderError("not a TAK file".to_string()));
        }

        // Read metadata headers only — TAK metadata is always at the start
        // of the file and is small (typically under 1 KB). Cap the read to
        // avoid loading multi-GB audio data into memory.
        const MAX_METADATA_READ: u64 = 1024 * 1024; // 1 MB hard cap
        // Start with a small allocation; read_to_end will grow as needed
        // up to the hard cap. This avoids reserving 1 MB for tiny files.
        let mut all_data = Vec::with_capacity(4096);
        reader.take(MAX_METADATA_READ).read_to_end(&mut all_data)?;
        let mut cursor = Cursor::new(&all_data);

        Self::from_reader_data(&mut cursor)
    }

    /// Parse TAK metadata from a pre-read buffer (after the 4-byte signature).
    ///
    /// Shared by both the sync and async loaders — the caller is responsible
    /// for reading the capped metadata bytes and creating the Cursor.
    pub fn from_reader_data<R: Read + Seek>(mut cursor: &mut R) -> Result<Self> {
        let mut info = TAKStreamInfo::default();
        let mut found_stream_info = false;

        // Each metadata block has a 4-byte header at minimum, so the 1 MB
        // read cap bounds this to ~250K iterations. This explicit limit is
        // defense-in-depth against malformed headers that claim zero size.
        const MAX_METADATA_BLOCKS: usize = 262_144;
        let mut blocks_read: usize = 0;

        loop {
            if blocks_read >= MAX_METADATA_BLOCKS {
                return Err(AudexError::TAKHeaderError(format!(
                    "metadata block count exceeds limit ({})",
                    MAX_METADATA_BLOCKS
                )));
            }
            blocks_read += 1;
            let metadata_type;
            let size;
            let data_size;

            // Read metadata header in separate scope
            {
                let mut bitreader = LSBBitReader::new(&mut cursor);
                metadata_type = bitreader.bits(7)? as u8;
                bitreader.skip(1)?; // Unused bit

                let size_bytes = bitreader.bytes(3)?;
                size = u32::from_le_bytes([size_bytes[0], size_bytes[1], size_bytes[2], 0]);
                data_size = size.saturating_sub(CRC_SIZE);

                // Cap block size to prevent excessive allocation from
                // crafted headers — 1 MB matches the read cap used by
                // the public from_reader entry point
                const MAX_BLOCK_SIZE: u32 = 1024 * 1024;
                if data_size > MAX_BLOCK_SIZE {
                    return Err(AudexError::TAKHeaderError(format!(
                        "metadata block size {} exceeds maximum allowed ({})",
                        data_size, MAX_BLOCK_SIZE
                    )));
                }

                // Ensure byte alignment
                if !bitreader.is_aligned() {
                    return Err(AudexError::TAKHeaderError(
                        "metadata not byte aligned".to_string(),
                    ));
                }
            }

            match TAKMetadata::from_u8(metadata_type) {
                Some(TAKMetadata::End) => break,
                Some(TAKMetadata::StreamInfo) => {
                    // Read stream info data into buffer and parse it
                    let mut data = vec![0u8; data_size as usize];
                    Read::read_exact(&mut cursor, &mut data)?;
                    let mut data_cursor = Cursor::new(&data);
                    let mut bitreader = LSBBitReader::new(&mut data_cursor);
                    info.parse_stream_info(&mut bitreader, size)?;
                    found_stream_info = true;

                    // Skip the CRC (3 bytes)
                    Seek::seek(&mut cursor, SeekFrom::Current(CRC_SIZE as i64))?;
                }
                Some(TAKMetadata::EncoderInfo) => {
                    // Read encoder info data into buffer and parse it
                    let mut data = vec![0u8; data_size as usize];
                    Read::read_exact(&mut cursor, &mut data)?;
                    let mut data_cursor = Cursor::new(&data);
                    let mut bitreader = LSBBitReader::new(&mut data_cursor);
                    info.parse_encoder_info(&mut bitreader, data_size)?;

                    // Skip the CRC (3 bytes)
                    Seek::seek(&mut cursor, SeekFrom::Current(CRC_SIZE as i64))?;
                }
                _ => {
                    // Skip unknown metadata blocks (data + CRC)
                    Seek::seek(&mut cursor, SeekFrom::Current(size as i64))?;
                }
            }
        }

        if !found_stream_info {
            return Err(AudexError::TAKHeaderError(
                "missing stream info".to_string(),
            ));
        }

        // Calculate length
        if info.sample_rate > 0 {
            let seconds = info.number_of_samples as f64 / info.sample_rate as f64;
            info.length = Duration::try_from_secs_f64(seconds).ok();
        }

        Ok(info)
    }

    /// Parse stream info metadata block
    fn parse_stream_info<R: Read>(
        &mut self,
        bitreader: &mut LSBBitReader<R>,
        size: u32,
    ) -> Result<()> {
        if !(STREAM_INFO_SIZE_MIN..=STREAM_INFO_SIZE_MAX).contains(&size) {
            return Err(AudexError::TAKHeaderError(
                "stream info has invalid length".to_string(),
            ));
        }

        // Encoder Info
        bitreader.skip(ENCODER_INFO_CODEC_BITS)?;
        bitreader.skip(ENCODER_INFO_PROFILE_BITS)?;

        // Size Info
        bitreader.skip(SIZE_INFO_FRAME_DURATION_BITS)?;
        self.number_of_samples = bitreader.bits(SIZE_INFO_SAMPLE_NUM_BITS)?;

        // Audio Format
        bitreader.skip(AUDIO_FORMAT_DATA_TYPE_BITS)?;
        self.sample_rate =
            (bitreader.bits(AUDIO_FORMAT_SAMPLE_RATE_BITS)? as u32) + SAMPLE_RATE_MIN;
        self.bits_per_sample =
            (bitreader.bits(AUDIO_FORMAT_SAMPLE_BITS_BITS)? as u32) + SAMPLE_BITS_MIN;
        self.channels = (bitreader.bits(AUDIO_FORMAT_CHANNEL_NUM_BITS)? as u32) + CHANNEL_NUM_MIN;
        bitreader.skip(AUDIO_FORMAT_HAS_EXTENSION_BITS)?;

        Ok(())
    }

    /// Parse encoder info metadata block
    fn parse_encoder_info<R: Read>(
        &mut self,
        bitreader: &mut LSBBitReader<R>,
        _size: u32,
    ) -> Result<()> {
        let patch = bitreader.bits(8)? as u8;
        let minor = bitreader.bits(8)? as u8;
        let major = bitreader.bits(8)? as u8;
        self.encoder_info = format!("TAK {}.{}.{}", major, minor, patch);
        Ok(())
    }

    /// Pretty print stream info
    pub fn pprint(&self) -> String {
        let encoder = if self.encoder_info.is_empty() {
            "TAK"
        } else {
            &self.encoder_info
        };
        format!(
            "{}, {} Hz, {} bits, {:.2} seconds, {} channel(s)",
            encoder,
            self.sample_rate,
            self.bits_per_sample,
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0),
            self.channels
        )
    }
}

/// TAK file with APEv2 tags
#[derive(Debug)]
pub struct TAK {
    pub info: TAKStreamInfo,
    pub tags: Option<APEv2Tags>,
    pub filename: Option<String>,
}

impl TAK {
    /// Create a new empty TAK instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: TAKStreamInfo::default(),
            tags: None,
            filename: None,
        }
    }

    /// Parse TAK file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Parse stream info
        self.info = TAKStreamInfo::from_reader(reader)?;

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
            return Err(AudexError::TAKHeaderError(
                "APEv2 tag already exists".to_string(),
            ));
        }
        self.tags = Some(APEv2Tags::new());
        Ok(())
    }

    /// Clear APEv2 tags
    pub fn clear(&mut self) -> Result<()> {
        // Remove APE tags from file if they exist
        if let (Some(filename), Some(_)) = (&self.filename, &self.tags) {
            crate::apev2::clear(filename)?;
        }

        self.tags = None;
        Ok(())
    }

    /// Get MIME types
    pub fn mime(&self) -> Vec<&'static str> {
        vec!["audio/x-tak"]
    }

    /// Pretty print file info
    pub fn pprint(&self) -> String {
        self.info.pprint()
    }

    /// Load TAK file asynchronously with non-blocking I/O
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut tak = TAK::new();
        tak.filename = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info asynchronously
        tak.info = Self::parse_info_async(&mut file).await?;

        // Load APEv2 tags
        match APEv2::load_async(&path).await {
            Ok(ape) => tak.tags = Some(ape.tags),
            Err(AudexError::APENoHeader) => tak.tags = None,
            Err(e) => return Err(e),
        }

        Ok(tak)
    }

    /// Parse stream information asynchronously.
    ///
    /// TAK metadata is always located at the start of the file and is small
    /// (typically under 1 KB). This reads at most 1 MB of header data via
    /// async I/O — matching the sync parser's cap — then delegates to the
    /// sync parser through a Cursor.
    #[cfg(feature = "async")]
    async fn parse_info_async(file: &mut TokioFile) -> Result<TAKStreamInfo> {
        file.seek(SeekFrom::Start(0)).await?;

        // Read the 4-byte signature first
        let mut stream_id = [0u8; 4];
        file.read_exact(&mut stream_id)
            .await
            .map_err(|_| AudexError::TAKHeaderError("not a TAK file".to_string()))?;

        if &stream_id != b"tBaK" {
            return Err(AudexError::TAKHeaderError("not a TAK file".to_string()));
        }

        // Read up to 1 MB of metadata — TAK metadata lives at the file start
        // and the audio payload that follows can be multi-GB.
        const MAX_METADATA_READ: u64 = 1024 * 1024;
        let file_size = file.seek(SeekFrom::End(0)).await?;
        let remaining = file_size.saturating_sub(4); // already read 4 bytes
        let read_size = std::cmp::min(remaining, MAX_METADATA_READ) as usize;

        file.seek(SeekFrom::Start(4)).await?;
        let mut metadata = vec![0u8; read_size];
        file.read_exact(&mut metadata).await?;

        // Reconstruct a buffer that starts with the signature so the sync
        // parser's Cursor-based logic sees the same byte layout it expects.
        // (The sync parser already skips the 4-byte signature before its
        // .take() call, so we feed the metadata portion directly.)
        let mut cursor = std::io::Cursor::new(&metadata[..]);

        // Reuse the sync parser's metadata-header loop. The sync version
        // creates a Cursor from the same capped data after the signature.
        TAKStreamInfo::from_reader_data(&mut cursor)
    }

    /// Save tags asynchronously
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        let filename = self
            .filename
            .clone()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        if let Some(ref tags) = self.tags {
            let mut ape = APEv2::new();
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

    /// Delete file asynchronously
    #[cfg(feature = "async")]
    pub async fn delete_async(&mut self) -> Result<()> {
        if let Some(filename) = &self.filename {
            tokio::fs::remove_file(filename).await?;
            self.filename = None;
            Ok(())
        } else {
            Err(AudexError::InvalidData(
                "No filename available for deletion".to_string(),
            ))
        }
    }
}

impl Default for TAK {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for TAK {
    type Tags = APEv2Tags;
    type Info = TAKStreamInfo;

    fn format_id() -> &'static str {
        "TAK"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing TAK file");
        let mut file = std::fs::File::open(&path)?;
        let mut tak = TAK::new();
        tak.filename = Some(path.as_ref().to_string_lossy().to_string());

        tak.parse_file(&mut file)?;
        Ok(tak)
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
        // Remove APE tags from file if they exist
        if let (Some(filename), Some(_)) = (&self.filename, &self.tags) {
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
    /// Note: the inherent method `TAK::add_tags()` returns
    /// `AudexError::TAKHeaderError` on failure. This trait method
    /// returns `AudexError::InvalidOperation` and is reached via
    /// `FileType::add_tags(&mut tak)`.
    ///
    /// # Errors
    ///
    /// Returns `AudexError::InvalidOperation` if tags already exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::tak::TAK;
    /// use audex::FileType;
    ///
    /// let mut tak = TAK::load("song.tak")?;
    /// if tak.tags.is_none() {
    ///     tak.add_tags()?;
    /// }
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "APE tags already exist".to_string(),
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

        // Check for TAK signature
        if header.len() >= 4 && &header[0..4] == b"tBaK" {
            score += 1;
        }

        // Check file extension
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".tak") {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/x-tak"]
    }
}

/// Standalone functions for TAK operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut tak = TAK::load(path)?;
    tak.clear()
}
