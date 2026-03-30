//! Support for OptimFROG audio files.
//!
//! This module provides support for OptimFROG, a lossless audio compression format
//! focused on achieving maximum compression ratios. OptimFROG specializes in reducing
//! file sizes to the absolute minimum while maintaining bit-perfect audio restoration.
//!
//! **Note**: The current parser reads OptimFROG stream headers without enforcing a
//! minimum encoder version.
//!
//! # File Format
//!
//! OptimFROG is a lossless format featuring:
//! - **Maximum compression**: Optimized for smallest possible file sizes
//! - **Specialized audio codec**: Tailored specifically for audio data compression
//! - **Lossless restoration**: Bit-identical reconstruction of original audio
//! - **DualStream mode**: Optional mode for better compression on specific content
//!
//! # Audio Characteristics
//!
//! - **Compression**: Lossless (bit-perfect reproduction)
//! - **Sample Rates**: Standard rates up to 192 kHz
//! - **Bit Depth**: 8-32 bits per sample
//! - **Channels**: 1-256 (typically mono/stereo; stored as 0-based in header)
//! - **Compression Ratio**: Among the highest for lossless audio (typically 55-65%)
//! - **File Extension**: `.ofr` (OptimFROG), `.ofs` (OptimFROG DualStream)
//! - **MIME Type**: `audio/x-optimfrog`
//!
//! # Tagging
//!
//! OptimFROG uses APEv2 tags:
//! - **Standard fields**: Title, Artist, Album, Year, Track, Genre
//! - **Binary support**: Embedded cover art
//! - **UTF-8 encoding**: Full Unicode support
//!
//! # Basic Usage
//!
//! ```no_run
//! use audex::optimfrog::OptimFROG;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut ofr = OptimFROG::load("song.ofr")?;
//!
//! println!("Sample Rate: {} Hz", ofr.info.sample_rate);
//! println!("Channels: {}", ofr.info.channels);
//! println!("Encoder: {}", ofr.info.encoder_info);
//!
//! if let Some(tags) = ofr.tags_mut() {
//!     tags.set_text("Title", "Song Title".to_string())?;
//!     tags.set_text("Artist", "Artist Name".to_string())?;
//! }
//!
//! ofr.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Compression Modes
//!
//! - **Normal mode**: Standard lossless compression
//! - **DualStream mode**: Separates audio into two streams for better compression
//!   on certain types of audio content
//!
//! # References
//!
//! - [OptimFROG Project](http://www.losslessaudio.org/)

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

/// Sample type bits mapping
const SAMPLE_TYPE_BITS: [(u8, u32); 8] = [
    (0, 8),
    (1, 8),
    (2, 16),
    (3, 16),
    (4, 24),
    (5, 24),
    (6, 32),
    (7, 32),
];

/// Get bits per sample from sample type
fn get_sample_type_bits(sample_type: u8) -> Option<u32> {
    SAMPLE_TYPE_BITS
        .iter()
        .find(|(st, _)| *st == sample_type)
        .map(|(_, bits)| *bits)
}

/// OptimFROG stream information
#[derive(Debug, Default)]
pub struct OptimFROGStreamInfo {
    pub length: Option<Duration>,
    pub bitrate: Option<u32>,
    pub channels: u32,
    pub sample_rate: u32,
    pub bits_per_sample: u32,
    pub encoder_info: String,
}

impl StreamInfo for OptimFROGStreamInfo {
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

impl OptimFROGStreamInfo {
    /// Parse OptimFROG file and extract stream information
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self> {
        // Read 76-byte header
        let mut header = [0u8; 76];
        reader
            .read_exact(&mut header)
            .map_err(|_| AudexError::OptimFROGHeaderError("not enough data".to_string()))?;

        // Check signature
        if &header[0..4] != b"OFR " {
            return Err(AudexError::OptimFROGHeaderError(
                "not an OptimFROG file".to_string(),
            ));
        }

        // Parse data size
        let data_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if data_size != 12 && data_size < 15 {
            return Err(AudexError::OptimFROGHeaderError(
                "not an OptimFROG file".to_string(),
            ));
        }

        // Parse stream parameters (little-endian)
        let total_samples = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let total_samples_high = u16::from_le_bytes([header[12], header[13]]);
        let sample_type = header[14];
        let channels = header[15];
        let sample_rate = u32::from_le_bytes([header[16], header[17], header[18], header[19]]);

        // Calculate total samples (64-bit)
        let total_samples_64 = total_samples as u64 + ((total_samples_high as u64) << 32);

        // Build stream info
        let mut info = OptimFROGStreamInfo {
            channels: (channels as u32) + 1, // Channels are stored as 0-based
            sample_rate,
            ..Default::default()
        };

        // Get bits per sample from sample type. Reject unknown types rather than
        // defaulting to 0, which would cause division-by-zero in consumers that
        // compute bytes-per-sample or similar derived values.
        info.bits_per_sample = get_sample_type_bits(sample_type).ok_or_else(|| {
            AudexError::OptimFROGHeaderError(format!(
                "Unknown sample type {}: cannot determine bits per sample",
                sample_type
            ))
        })?;

        // Calculate duration. In OptimFROG, total_samples represents
        // interleaved PCM samples (i.e. frames * channels), so dividing by
        // both channels and sample_rate yields the correct duration in seconds.
        if info.sample_rate > 0 {
            info.length = Duration::try_from_secs_f64(
                total_samples_64 as f64 / (info.channels as f64 * info.sample_rate as f64),
            )
            .ok();
        } else {
            // sample_rate of 0 means duration is indeterminate, not zero.
            info.length = None;
        }

        // Parse encoder info if available
        if data_size >= 15 {
            let encoder_id = u16::from_le_bytes([header[20], header[21]]);
            let version = (encoder_id >> 4) + 4500;
            let version_str = format!("{}", version);
            if version_str.len() >= 2 && version_str.is_ascii() {
                let first_char = version_str.chars().next().unwrap_or('0');
                let rest = &version_str[first_char.len_utf8()..];
                info.encoder_info = format!("{}.{}", first_char, rest);
            } else {
                info.encoder_info = String::new();
            }
        } else {
            info.encoder_info = String::new();
        }

        Ok(info)
    }

    /// Pretty print stream info
    pub fn pprint(&self) -> String {
        format!(
            "OptimFROG, {:.2} seconds, {} Hz",
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0),
            self.sample_rate
        )
    }
}

/// OptimFROG file with APEv2 tags
#[derive(Debug)]
pub struct OptimFROG {
    pub info: OptimFROGStreamInfo,
    pub tags: Option<APEv2Tags>,
    pub filename: Option<String>,
}

impl OptimFROG {
    /// Create a new empty OptimFROG instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: OptimFROGStreamInfo::default(),
            tags: None,
            filename: None,
        }
    }

    /// Parse OptimFROG file and extract information
    fn parse_file<R: Read>(&mut self, reader: &mut R) -> Result<()> {
        // Parse stream info
        self.info = OptimFROGStreamInfo::from_reader(reader)?;

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
                "Tags already exist".to_string(),
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
        vec!["audio/x-optimfrog"]
    }

    /// Pretty print file info
    pub fn pprint(&self) -> String {
        self.info.pprint()
    }

    /// Load OptimFROG file asynchronously
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut ofr = OptimFROG::new();
        ofr.filename = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info
        ofr.info = Self::parse_info_async(&mut file).await?;

        // Load APEv2 tags
        match crate::apev2::APEv2::load_async(&path).await {
            Ok(ape) => ofr.tags = Some(ape.tags),
            Err(AudexError::APENoHeader) => ofr.tags = None,
            Err(e) => return Err(e),
        }

        Ok(ofr)
    }

    /// Parse stream information asynchronously
    ///
    /// Reads the 76-byte header via async I/O, then delegates to the sync
    /// `from_reader` via a `Cursor` for correct, consistent parsing.
    #[cfg(feature = "async")]
    async fn parse_info_async(file: &mut TokioFile) -> Result<OptimFROGStreamInfo> {
        file.seek(SeekFrom::Start(0)).await?;

        let mut header = [0u8; 76];
        file.read_exact(&mut header).await?;

        let mut cursor = std::io::Cursor::new(&header[..]);
        OptimFROGStreamInfo::from_reader(&mut cursor)
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

impl Default for OptimFROG {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for OptimFROG {
    type Tags = APEv2Tags;
    type Info = OptimFROGStreamInfo;

    fn format_id() -> &'static str {
        "OptimFROG"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing OptimFROG file");
        let mut file = std::fs::File::open(&path)?;
        let mut optimfrog = OptimFROG::new();
        optimfrog.filename = Some(path.as_ref().to_string_lossy().to_string());

        optimfrog.parse_file(&mut file)?;
        Ok(optimfrog)
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
    /// # Errors
    ///
    /// Returns `AudexError::InvalidOperation` if tags already exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::optimfrog::OptimFROG;
    /// use audex::FileType;
    ///
    /// let mut ofr = OptimFROG::load("song.ofr")?;
    /// if ofr.tags.is_none() {
    ///     ofr.add_tags()?;
    /// }
    /// ofr.set("title", vec!["My Song".to_string()])?;
    /// ofr.save()?;
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

        // Check for OptimFROG signature
        if header.len() >= 3 && header.starts_with(b"OFR") {
            score += 1;
        }

        // Check file extension
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".ofr") {
            score += 1;
        }
        if lower_filename.ends_with(".ofs") {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/x-optimfrog"]
    }
}

/// Standalone functions for OptimFROG operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut optimfrog = OptimFROG::load(path)?;
    optimfrog.clear()
}
