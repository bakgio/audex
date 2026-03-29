//! Support for Monkey's Audio (APE) files.
//!
//! This module provides support for Monkey's Audio, a highly efficient lossless
//! audio compression format developed by Matt Ashland. Known for excellent
//! compression ratios, Monkey's Audio achieves typically 50-60% of original size
//! while maintaining perfect bit-for-bit reconstruction.
//!
//! # File Format
//!
//! Monkey's Audio is a lossless compression format supporting:
//! - **Compression Levels**: Fast, Normal, High, Extra High, Insane
//! - **Adaptive Prediction**: Automatically adjusts compression for each frame
//! - **Error Detection**: CRC checksums for data integrity
//!
//! # Audio Characteristics
//!
//! - **Compression**: Lossless (bit-perfect reproduction)
//! - **Sample Rates**: Up to 192 kHz
//! - **Bit Depth**: 8, 16, 24 bits per sample
//! - **Channels**: 1-2 (mono/stereo)
//! - **Compression Ratio**: Typically 50-60% of original size
//! - **File Extension**: `.ape`
//! - **MIME Type**: `audio/x-ape`, `audio/ape`
//!
//! # Tagging
//!
//! Monkey's Audio uses APEv2 tags:
//! - **Standard fields**: Title, Artist, Album, Year, Track, Genre
//! - **Binary support**: Embedded cover art
//! - **UTF-8 encoding**: Full Unicode support
//!
//! # Basic Usage
//!
//! ```no_run
//! use audex::monkeysaudio::MonkeysAudio;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut ape = MonkeysAudio::load("song.ape")?;
//!
//! // Read stream information
//! println!("Sample Rate: {} Hz", ape.info.sample_rate);
//! println!("Channels: {}", ape.info.channels);
//! println!("Version: {:.2}", ape.info.version);
//!
//! // Modify tags
//! if let Some(tags) = ape.tags_mut() {
//!     tags.set_text("Title", "Song Title".to_string())?;
//!     tags.set_text("Artist", "Artist Name".to_string())?;
//! }
//!
//! ape.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Version History
//!
//! Monkey's Audio has evolved through several versions:
//! - **< 3.98**: Legacy format with 32-byte header
//! - **>= 3.98**: Modern format with 76-byte header (most common)
//! - **>= 3.99**: Improved compression algorithms
//!
//! # References
//!
//! - [Monkey's Audio Official Site](http://www.monkeysaudio.com/)

use crate::{AudexError, FileType, Result, StreamInfo, apev2::APEv2Tags};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::util::loadfile_read_async;
#[cfg(feature = "async")]
use std::io::SeekFrom;
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Audio stream information for Monkey's Audio files.
///
/// Contains technical details extracted from the APE file header, including
/// sample rate, bit depth, channel count, and version information.
///
/// The header format varies between legacy (< 3.98) and modern (>= 3.98) versions.
#[derive(Debug, Default)]
pub struct MonkeysAudioStreamInfo {
    /// Audio duration
    pub length: Option<Duration>,
    /// Average bitrate in bps
    pub bitrate: Option<u32>,
    /// Number of audio channels (1-2)
    pub channels: u32,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Bits per sample (8, 16, or 24)
    pub bits_per_sample: u32,
    /// APE format version (e.g., 3.99, 4.11)
    pub version: f64,
}

impl StreamInfo for MonkeysAudioStreamInfo {
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

impl MonkeysAudioStreamInfo {
    /// Parse Monkey's Audio file and extract stream information
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self> {
        // Read 76-byte header
        let mut header = [0u8; 76];
        reader
            .read_exact(&mut header)
            .map_err(|_| AudexError::InvalidData("not enough data".to_string()))?;

        // Check signature
        if &header[0..4] != b"MAC " {
            return Err(AudexError::InvalidData(
                "not a Monkey's Audio file".to_string(),
            ));
        }

        // Parse version (little-endian u16)
        let version_raw = u16::from_le_bytes([header[4], header[5]]);
        let version = version_raw as f64 / 1000.0;

        let mut info = MonkeysAudioStreamInfo {
            version,
            ..Default::default()
        };

        if version_raw >= 3980 {
            // Modern format (>= 3.98)
            let blocks_per_frame =
                u32::from_le_bytes([header[56], header[57], header[58], header[59]]);
            let final_frame_blocks =
                u32::from_le_bytes([header[60], header[61], header[62], header[63]]);
            let total_frames = u32::from_le_bytes([header[64], header[65], header[66], header[67]]);
            info.bits_per_sample = u16::from_le_bytes([header[68], header[69]]) as u32;
            info.channels = u16::from_le_bytes([header[70], header[71]]) as u32;
            info.sample_rate = u32::from_le_bytes([header[72], header[73], header[74], header[75]]);

            // Calculate length (use u64 to avoid overflow with crafted values)
            if info.sample_rate > 0 && total_frames > 0 {
                let total_blocks =
                    (total_frames as u64 - 1) * blocks_per_frame as u64 + final_frame_blocks as u64;
                let seconds = total_blocks as f64 / info.sample_rate as f64;
                info.length = Duration::try_from_secs_f64(seconds).ok();
            } else {
                info.length = Some(Duration::from_secs(0));
            }
        } else {
            // Legacy format (< 3.98)
            let compression_level = u16::from_le_bytes([header[6], header[7]]);
            info.channels = u16::from_le_bytes([header[10], header[11]]) as u32;
            info.sample_rate = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
            let total_frames = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);
            let final_frame_blocks =
                u32::from_le_bytes([header[28], header[29], header[30], header[31]]);

            // Determine blocks per frame based on version and compression level
            let blocks_per_frame: u32 = if version_raw >= 3950 {
                73728 * 4
            } else if version_raw >= 3900 || (version_raw >= 3800 && compression_level == 4) {
                73728
            } else {
                9216
            };

            // Default to 0 meaning "unknown". Legacy APE headers do not
            // always include bits_per_sample. Callers should treat a value
            // of 0 as indeterminate and fall back to a sensible default
            // (e.g. 16) or skip bit-depth-dependent processing.
            info.bits_per_sample = 0;
            // Legacy files may embed a WAV header descriptor after the
            // 32-byte APE header. Only read bits_per_sample from it when
            // the WAVEfmt marker is present — otherwise the bytes at
            // offsets 48+ could be unrelated data.
            if &header[48..55] == b"WAVEfmt" {
                info.bits_per_sample = u16::from_le_bytes([header[74], header[75]]) as u32;
            }

            // Calculate length (use u64 to avoid overflow with crafted values)
            if info.sample_rate > 0 && total_frames > 0 {
                let total_blocks =
                    (total_frames as u64 - 1) * blocks_per_frame as u64 + final_frame_blocks as u64;
                let seconds = total_blocks as f64 / info.sample_rate as f64;
                info.length = Duration::try_from_secs_f64(seconds).ok();
            } else {
                info.length = Some(Duration::from_secs(0));
            }
        }

        Ok(info)
    }

    /// Pretty print stream info
    pub fn pprint(&self) -> String {
        format!(
            "Monkey's Audio {:.2}, {:.2} seconds, {} Hz",
            self.version,
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0),
            self.sample_rate
        )
    }
}

/// Represents a Monkey's Audio file with metadata and stream information.
///
/// Provides access to lossless audio stream details and APEv2 metadata tags.
/// Monkey's Audio files offer excellent compression ratios while maintaining
/// perfect bit-for-bit reproduction of the original audio.
///
/// File extension: `.ape`
#[derive(Debug)]
pub struct MonkeysAudio {
    /// Audio stream information
    pub info: MonkeysAudioStreamInfo,
    /// Optional APEv2 metadata tags
    pub tags: Option<APEv2Tags>,
    /// Path to the file (used for saving)
    pub filename: Option<String>,
}

impl MonkeysAudio {
    /// Create a new empty Monkey's Audio instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: MonkeysAudioStreamInfo::default(),
            tags: None,
            filename: None,
        }
    }

    /// Parse Monkey's Audio file and extract information
    fn parse_file<R: Read>(&mut self, reader: &mut R) -> Result<()> {
        // Parse stream info
        self.info = MonkeysAudioStreamInfo::from_reader(reader)?;

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
        vec!["audio/ape", "audio/x-ape"]
    }

    /// Pretty print file info
    pub fn pprint(&self) -> String {
        self.info.pprint()
    }

    /// Load Monkey's Audio file asynchronously
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut ma = MonkeysAudio::new();
        ma.filename = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info from header
        ma.info = Self::parse_info_async(&mut file).await?;

        // Load APEv2 tags
        match crate::apev2::APEv2::load_async(&path).await {
            Ok(ape) => ma.tags = Some(ape.tags),
            Err(AudexError::APENoHeader) => ma.tags = None,
            Err(e) => return Err(e),
        }

        Ok(ma)
    }

    /// Parse stream information asynchronously
    ///
    /// Reads the 76-byte header via async I/O, then delegates to the sync
    /// `from_reader` via a `Cursor` for correct, consistent parsing.
    #[cfg(feature = "async")]
    async fn parse_info_async(file: &mut TokioFile) -> Result<MonkeysAudioStreamInfo> {
        file.seek(SeekFrom::Start(0)).await?;

        let mut header = [0u8; 76];
        file.read_exact(&mut header).await?;

        let mut cursor = std::io::Cursor::new(&header[..]);
        MonkeysAudioStreamInfo::from_reader(&mut cursor)
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

impl Default for MonkeysAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for MonkeysAudio {
    type Tags = APEv2Tags;
    type Info = MonkeysAudioStreamInfo;

    fn format_id() -> &'static str {
        "MonkeysAudio"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing Monkey's Audio file");
        let mut file = std::fs::File::open(&path)?;
        let mut monkeys_audio = MonkeysAudio::new();
        monkeys_audio.filename = Some(path.as_ref().to_string_lossy().to_string());

        monkeys_audio.parse_file(&mut file)?;
        Ok(monkeys_audio)
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

        // Save APEv2 tags if they exist
        if let Some(ref tags) = self.tags {
            // Create an APEv2 instance to handle the saving
            let mut apev2 = crate::apev2::APEv2::new();
            apev2.filename = Some(filename.clone());

            // Copy tags from our MonkeysAudio tags to the APEv2 tags
            for (key, value) in tags.items() {
                let _ = apev2.tags.set(&key, value.clone());
            }

            // Save the tags
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
    /// use audex::monkeysaudio::MonkeysAudio;
    /// use audex::FileType;
    ///
    /// let mut ape = MonkeysAudio::load("song.ape")?;
    /// if ape.tags.is_none() {
    ///     ape.add_tags()?;
    /// }
    /// ape.set("title", vec!["My Song".to_string()])?;
    /// ape.save()?;
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

        // Check for Monkey's Audio signature
        if header.len() >= 4 && header.starts_with(b"MAC ") {
            score += 1;
        }

        // .ape files are always MonkeysAudio containers. Score high enough
        // to beat APEv2's header-only match (10) when the file starts with
        // an APE tag before the MAC header.
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".ape") {
            score += 11;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/ape", "audio/x-ape"]
    }
}

/// Standalone functions for Monkey's Audio operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut monkeys_audio = MonkeysAudio::load(path)?;
    monkeys_audio.clear()
}

/// Open Monkey's Audio file (alias)
pub fn open<P: AsRef<Path>>(path: P) -> Result<MonkeysAudio> {
    MonkeysAudio::load(path)
}
