//! Support for TrueAudio (TTA) files.
//!
//! This module provides support for TrueAudio, a free lossless audio compression
//! format designed for real-time encoding and decoding. TrueAudio emphasizes
//! simplicity and speed while achieving competitive compression ratios.
//!
//! # File Format
//!
//! TrueAudio is a lossless format featuring:
//! - **Real-time processing**: Fast enough for real-time encoding/decoding
//! - **Simple design**: Straightforward format specification
//! - **Hardware support**: Codec implementations for various platforms
//! - **Error detection**: CRC-32 checksums per frame
//!
//! # Audio Characteristics
//!
//! - **Compression**: Lossless (bit-perfect reproduction)
//! - **Sample Rates**: 8 kHz to 192 kHz (and higher)
//! - **Bit Depth**: 8, 16, or 24 bits per sample
//! - **Channels**: 1-8 channels
//! - **Frame Size**: Fixed at 1.04 seconds of audio
//! - **File Extension**: `.tta`
//! - **MIME Type**: `audio/x-tta`
//!
//! # Tagging
//!
//! TrueAudio supports both ID3v1/v2 and APEv2 tags:
//! - **ID3v2**: More common, extensive metadata support
//! - **APEv2**: Alternative tagging format
//! - **Standard fields**: Title, Artist, Album, Year, Track, Genre
//! - **Binary support**: Embedded cover art (ID3v2 APIC/APEv2)
//!
//! # Basic Usage
//!
//! ```no_run
//! use audex::trueaudio::TrueAudio;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut tta = TrueAudio::load("song.tta")?;
//!
//! // Access stream information
//! println!("Sample Rate: {} Hz", tta.info.sample_rate);
//!
//! // TrueAudio supports both ID3 and APEv2 tags via the unified tags_mut() interface
//! if let Some(tags) = tta.tags_mut() {
//!     tags.set("TIT2", vec!["Song Title".to_string()]);
//!     tags.set("TPE1", vec!["Artist Name".to_string()]);
//! }
//!
//! tta.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # References
//!
//! - [TrueAudio Official Site](http://www.true-audio.com/)
//! - [TrueAudio Codec Specification](http://en.true-audio.com/TTA_Lossless_Audio_Codec_-_Format_Description)

use crate::{
    AudexError, FileType, Result, StreamInfo,
    apev2::{APEValue, APEv2Tags},
    id3::ID3Tags,
    tags::Tags,
};
use std::io::{Read, Seek, SeekFrom};
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

/// True Audio stream information
#[derive(Debug, Default)]
pub struct TrueAudioStreamInfo {
    pub length: Option<Duration>,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
}

impl StreamInfo for TrueAudioStreamInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }
    fn bitrate(&self) -> Option<u32> {
        None
    } // Not available in TTA header
    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate)
    }
    fn channels(&self) -> Option<u16> {
        if self.channels > 0 {
            Some(self.channels)
        } else {
            None
        }
    }
    fn bits_per_sample(&self) -> Option<u16> {
        if self.bits_per_sample > 0 {
            Some(self.bits_per_sample)
        } else {
            None
        }
    }
}

impl TrueAudioStreamInfo {
    /// Pretty-print the stream information
    pub fn pprint(&self) -> String {
        let mut output = String::new();
        output.push_str("True Audio, ");

        if let Some(length) = self.length {
            output.push_str(&format!("{:.2} seconds, ", length.as_secs_f64()));
        }

        output.push_str(&format!("{} Hz", self.sample_rate));
        output
    }

    /// Parse TrueAudio file and extract stream information
    pub fn from_reader<R: Read + Seek>(reader: &mut R, offset: Option<u64>) -> Result<Self> {
        reader.seek(SeekFrom::Start(offset.unwrap_or(0)))?;

        let mut header = [0u8; 18];
        reader
            .read_exact(&mut header)
            .map_err(|_| AudexError::TrueAudioHeaderError("TTA header not found".to_string()))?;

        // Check for the full TTA1 signature (4 bytes). Checking only
        // 3 bytes ("TTA") would also match TTA2 files, which have a
        // different header layout and would produce wrong metadata.
        if &header[0..4] != b"TTA1" {
            return Err(AudexError::TrueAudioHeaderError(
                "TTA1 header not found".to_string(),
            ));
        }

        // Parse header fields (little-endian)
        // TTA1 header layout (18 bytes):
        //  0-3:   "TTA1" signature
        //  4-5:   Audio format (u16 LE)
        //  6-7:   Channels (u16 LE)
        //  8-9:   Bits per sample (u16 LE)
        // 10-13:  Sample rate (u32 LE)
        // 14-17:  Total samples (u32 LE)

        let channels = u16::from_le_bytes([header[6], header[7]]);
        let bits_per_sample = u16::from_le_bytes([header[8], header[9]]);
        let sample_rate = u32::from_le_bytes([header[10], header[11], header[12], header[13]]);
        let samples = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);

        // A zero value for channels or bits_per_sample means the header
        // does not carry that information. The StreamInfo accessors already
        // return None for zero, so callers should treat it as "unknown".
        if channels == 0 {
            warn_event!("TrueAudio: channels is zero — treating as unknown");
        }
        if bits_per_sample == 0 {
            warn_event!("TrueAudio: bits_per_sample is zero — treating as unknown");
        }

        // Return None when sample_rate is zero to avoid division by zero
        // and to correctly indicate that the duration is indeterminate.
        let length = if sample_rate != 0 {
            Some(Duration::from_secs_f64(samples as f64 / sample_rate as f64))
        } else {
            None
        };

        Ok(TrueAudioStreamInfo {
            length,
            sample_rate,
            channels,
            bits_per_sample,
        })
    }
}

/// Tag types supported by TrueAudio
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum TrueAudioTags {
    /// Default ID3 tags (like the ID3FileType)
    ID3(ID3Tags),
    /// APE tags (: tagger.tags = APEv2())
    APE(APEv2Tags),
}

impl TrueAudioTags {
    /// Get keys for any tag type (unified interface)
    pub fn keys(&self) -> Vec<String> {
        match self {
            TrueAudioTags::ID3(tags) => tags.keys(),
            TrueAudioTags::APE(tags) => tags.keys(),
        }
    }

    /// Check if tags are empty
    pub fn is_empty(&self) -> bool {
        self.keys().is_empty()
    }

    /// Set a tag value (unified interface for both ID3 and APE)
    pub fn set(&mut self, key: &str, value: APEValue) -> Result<()> {
        match self {
            TrueAudioTags::ID3(_) => {
                // ID3 tags don't support APEValue, this shouldn't be called for ID3
                Err(AudexError::InvalidOperation(
                    "Cannot set APEValue on ID3 tags".to_string(),
                ))
            }
            TrueAudioTags::APE(tags) => tags.set(key, value),
        }
    }

    /// Get a tag value (unified interface)
    pub fn get(&self, key: &str) -> Option<&APEValue> {
        match self {
            TrueAudioTags::ID3(_) => None, // ID3 tags don't use APEValue
            TrueAudioTags::APE(tags) => tags.get(key),
        }
    }

    /// Remove a tag (unified interface)
    pub fn remove(&mut self, key: &str) -> Result<()> {
        match self {
            TrueAudioTags::ID3(_) => {
                // ID3 tags don't support this interface
                Err(AudexError::InvalidOperation(
                    "Cannot remove APE-style tags from ID3".to_string(),
                ))
            }
            TrueAudioTags::APE(tags) => {
                tags.remove(key);
                Ok(())
            }
        }
    }

    /// Clear all tags (unified interface)
    pub fn clear(&mut self) {
        match self {
            TrueAudioTags::ID3(tags) => tags.clear(),
            TrueAudioTags::APE(tags) => tags.clear(),
        }
    }
}

/// TrueAudio file supporting both ID3 and APE tags
#[derive(Debug)]
pub struct TrueAudio {
    pub info: TrueAudioStreamInfo,
    pub tags: Option<TrueAudioTags>,
    pub filename: Option<String>,
}

impl TrueAudio {
    /// Create a new empty TrueAudio instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: TrueAudioStreamInfo::default(),
            tags: None,
            filename: None,
        }
    }

    /// Add ID3 tags if none exist (default behavior )
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }

        // Create new empty ID3 tags (default like the ID3FileType)
        self.tags = Some(TrueAudioTags::ID3(ID3Tags::new()));
        Ok(())
    }

    /// Manually assign APE tags (: tagger.tags = APEv2())
    /// This exactly mimics the pattern: tagger.tags = APEv2()
    pub fn assign_ape_tags(&mut self) {
        self.tags = Some(TrueAudioTags::APE(APEv2Tags::new()));
    }

    /// Set tags directly (unified interface)
    /// This allows both: tagger.tags = ID3Tags or tagger.tags = APEv2Tags
    pub fn set_tags(&mut self, tags: TrueAudioTags) {
        self.tags = Some(tags);
    }

    /// Get mutable reference to APE tags if present
    pub fn ape_tags_mut(&mut self) -> Option<&mut APEv2Tags> {
        match &mut self.tags {
            Some(TrueAudioTags::APE(tags)) => Some(tags),
            _ => None,
        }
    }

    /// Get reference to APE tags if present
    pub fn ape_tags(&self) -> Option<&APEv2Tags> {
        match &self.tags {
            Some(TrueAudioTags::APE(tags)) => Some(tags),
            _ => None,
        }
    }

    /// Get tag keys for extraction (delegates to underlying tags)
    pub fn keys(&self) -> Vec<String> {
        match &self.tags {
            Some(tags) => tags.keys(),
            None => Vec::new(),
        }
    }

    /// Get tag value for extraction (delegates to underlying tags)
    pub fn get(&self, key: &str) -> Option<&APEValue> {
        match &self.tags {
            Some(tags) => tags.get(key),
            None => None,
        }
    }

    /// Pretty print the file information
    pub fn pprint(&self) -> String {
        let mut output = String::new();
        output.push_str("True Audio, ");

        if let Some(length) = self.info.length {
            output.push_str(&format!("{:.2} seconds, ", length.as_secs_f64()));
        }

        output.push_str(&format!("{} Hz", self.info.sample_rate));

        if let Some(tags) = &self.tags {
            if !tags.keys().is_empty() {
                output.push_str(" (with ID3 tags)");
            }
        }

        output
    }

    /// Get MIME types for this format
    pub fn mime(&self) -> &'static [&'static str] {
        TrueAudio::mime_types()
    }

    /// Parse TrueAudio file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // First, try to find the TTA header by checking for ID3 tags
        reader.seek(SeekFrom::Start(0))?;
        let mut buffer = [0u8; 10];
        reader.read_exact(&mut buffer)?;

        let tta_offset = if &buffer[0..3] == b"ID3" {
            // Skip ID3v2 tag
            let flags = buffer[5];
            let size_bytes = [buffer[6], buffer[7], buffer[8], buffer[9]];

            // ID3v2 uses synchsafe integers (7 usable bits per byte, MSB ignored)
            let size = (((size_bytes[0] & 0x7F) as u32) << 21)
                | (((size_bytes[1] & 0x7F) as u32) << 14)
                | (((size_bytes[2] & 0x7F) as u32) << 7)
                | ((size_bytes[3] & 0x7F) as u32);

            let header_size = 10u64;
            let footer_size = if (flags & 0x10) != 0 { 10u64 } else { 0u64 };
            Some(header_size + size as u64 + footer_size)
        } else if &buffer[0..3] == b"TTA" {
            // TTA header starts immediately
            Some(0)
        } else {
            // Look for TTA header in the file
            None
        };

        // Parse stream info
        self.info = TrueAudioStreamInfo::from_reader(reader, tta_offset)?;

        // Parse tags from TrueAudio file.
        // TrueAudio can have either ID3v2 tags (at beginning) or APEv2 tags (at end).
        // Try APE tags first (more common for TrueAudio), then fall back to ID3.
        //
        // When a filename is available we load tags from the file on disk; otherwise
        // we fall back to parsing directly from the reader so that reader-only callers
        // (e.g. in-memory buffers) still get their tags.

        if let Some(ref filename) = self.filename {
            // File-based tag loading path
            match crate::apev2::APEv2::load(filename) {
                Ok(ape_file) => {
                    let mut new_tags = APEv2Tags::new();
                    for key in ape_file.tags.keys() {
                        if let Some(value) = ape_file.tags.get(&key) {
                            let _ = new_tags.set(&key, value.clone());
                        }
                    }
                    self.tags = Some(TrueAudioTags::APE(new_tags));
                    return Ok(());
                }
                Err(_) => {
                    // No APE tags on disk, try ID3
                }
            }

            match crate::id3::ID3::with_file(filename) {
                Ok(id3_file) => {
                    self.tags = Some(TrueAudioTags::ID3(id3_file.tags.clone()));
                }
                Err(_) => {
                    self.tags = None;
                }
            }
        } else {
            // Reader-based tag loading path (no filename available)
            reader.seek(SeekFrom::Start(0))?;
            if let Ok(ape) = <crate::apev2::APEv2 as FileType>::load_from_reader(reader) {
                let mut new_tags = APEv2Tags::new();
                for key in ape.tags.keys() {
                    if let Some(value) = ape.tags.get(&key) {
                        let _ = new_tags.set(&key, value.clone());
                    }
                }
                self.tags = Some(TrueAudioTags::APE(new_tags));
            } else {
                // No APE tags in the stream, try ID3
                reader.seek(SeekFrom::Start(0))?;
                if let Ok(id3) = <crate::id3::ID3 as FileType>::load_from_reader(reader) {
                    self.tags = Some(TrueAudioTags::ID3(id3.tags.clone()));
                } else {
                    self.tags = None;
                }
            }
        }

        Ok(())
    }

    /// Clear all tags (ID3 or APE)
    pub fn clear(&mut self) -> Result<()> {
        // Remove tags from file based on tag type using standalone clear functions
        if let Some(filename) = &self.filename {
            match &self.tags {
                Some(TrueAudioTags::ID3(_)) => {
                    // Remove ID3 tags from file using standalone function
                    crate::id3::clear(filename)?;
                }
                Some(TrueAudioTags::APE(_)) => {
                    // Remove APE tags from file using standalone function
                    crate::apev2::clear(filename)?;
                }
                None => {
                    // No tags to clear
                }
            }
        }

        self.tags = None;
        Ok(())
    }

    /// Load a TrueAudio file asynchronously from the given path.
    ///
    /// This method reads the file, parses the TTA header to extract stream information
    /// (sample rate, duration), and loads any APEv2 tags present in the file.
    ///
    /// # Arguments
    /// * `path` - Path to the TrueAudio file
    ///
    /// # Returns
    /// A `Result` containing the populated `TrueAudio` instance or an error
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut tta = TrueAudio::new();
        tta.filename = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info from TTA header
        tta.info = Self::parse_info_async(&mut file).await?;

        // Load APEv2 tags if present
        match APEv2::load_async(&path).await {
            Ok(ape) => {
                // Convert APEv2 tags to TrueAudioTags::APE
                let mut new_tags = APEv2Tags::new();
                for key in ape.tags.keys() {
                    if let Some(value) = ape.tags.get(&key) {
                        let _ = new_tags.set(&key, value.clone());
                    }
                }
                tta.tags = Some(TrueAudioTags::APE(new_tags));
            }
            Err(AudexError::APENoHeader) => tta.tags = None,
            Err(e) => return Err(e),
        }

        Ok(tta)
    }

    /// Parse TrueAudio stream information asynchronously
    ///
    /// Detects an ID3v2 header (10 bytes) to find the TTA offset, then reads
    /// the 18-byte TTA header via async I/O and delegates to the sync
    /// `from_reader` via a `Cursor`. Mirrors the sync `parse_file` I/O pattern.
    #[cfg(feature = "async")]
    async fn parse_info_async(file: &mut TokioFile) -> Result<TrueAudioStreamInfo> {
        file.seek(SeekFrom::Start(0)).await?;

        // Detect ID3v2 header to find TTA offset (mirrors sync parse_file logic)
        let mut buffer = [0u8; 10];
        file.read_exact(&mut buffer).await?;

        let tta_offset = if &buffer[0..3] == b"ID3" {
            let flags = buffer[5];
            // ID3v2 sizes are synchsafe: only the lower 7 bits per byte
            // carry data.  Mask with 0x7F to match the sync path.
            let size = (((buffer[6] & 0x7F) as u64) << 21)
                | (((buffer[7] & 0x7F) as u64) << 14)
                | (((buffer[8] & 0x7F) as u64) << 7)
                | ((buffer[9] & 0x7F) as u64);
            let footer_size = if (flags & 0x10) != 0 { 10u64 } else { 0u64 };
            10 + size + footer_size
        } else {
            0
        };

        // Read just the 18-byte TTA header from the correct offset
        file.seek(SeekFrom::Start(tta_offset)).await?;
        let mut header = [0u8; 18];
        file.read_exact(&mut header).await?;

        let mut cursor = std::io::Cursor::new(&header[..]);
        TrueAudioStreamInfo::from_reader(&mut cursor, Some(0))
    }

    /// Save tags to the TrueAudio file asynchronously.
    ///
    /// Writes the current APE tags back to the file. If the file has ID3 tags,
    /// they will be converted and saved as APE tags (the async implementation
    /// only supports APEv2 tag format).
    ///
    /// # Returns
    /// A `Result` indicating success or an error
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        let filename = self
            .filename
            .clone()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        match &self.tags {
            Some(TrueAudioTags::APE(tags)) => {
                let mut ape = APEv2::new();
                ape.filename = Some(filename);
                ape.tags = tags.clone();
                ape.save_async().await
            }
            Some(TrueAudioTags::ID3(_)) => {
                // For async, we save ID3 tags as APE tags
                // ID3 async save is not supported in this implementation
                Err(AudexError::InvalidOperation(
                    "Async save for ID3 tags is not supported, use APE tags instead".to_string(),
                ))
            }
            None => Ok(()),
        }
    }

    /// Clear all tags from the TrueAudio file asynchronously.
    ///
    /// Removes tags from the file based on the current tag type (ID3 or APE)
    /// and clears the in-memory tag reference.
    /// This permanently deletes tag data from the file.
    ///
    /// # Returns
    /// A `Result` indicating success or an error
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        // Remove tags from file based on tag type using standalone clear functions
        if let Some(filename) = &self.filename {
            match &self.tags {
                Some(TrueAudioTags::ID3(_)) => {
                    // Remove ID3 tags from file using standalone function
                    crate::id3::file::clear_async(std::path::Path::new(filename), true, true)
                        .await?;
                }
                Some(TrueAudioTags::APE(_)) => {
                    // Remove APE tags from file using standalone function
                    crate::apev2::clear_async(filename).await?;
                }
                None => {
                    // No tags to clear
                }
            }
        }
        self.tags = None;
        Ok(())
    }

    /// Delete the TrueAudio file from disk asynchronously.
    ///
    /// Permanently removes the file associated with this TrueAudio instance.
    /// This operation cannot be undone.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if the file cannot be deleted
    #[cfg(feature = "async")]
    pub async fn delete_async(&mut self) -> Result<()> {
        if let Some(filename) = &self.filename {
            tokio::fs::remove_file(filename).await?;
            self.filename = None;
            Ok(())
        } else {
            Err(AudexError::InvalidData("No filename set".to_string()))
        }
    }
}

impl Default for TrueAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for TrueAudio {
    type Tags = ID3Tags;
    type Info = TrueAudioStreamInfo;

    fn format_id() -> &'static str {
        "TrueAudio"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing TrueAudio file");
        let mut file = std::fs::File::open(&path)?;
        let mut trueaudio = TrueAudio::new();
        trueaudio.filename = Some(path.as_ref().to_string_lossy().to_string());

        trueaudio.parse_file(&mut file)?;
        Ok(trueaudio)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        let mut instance = Self::new();
        let mut reader = reader;

        // Parse stream info (replicate the header-skipping logic from parse_file)
        reader.seek(std::io::SeekFrom::Start(0))?;
        let mut buffer = [0u8; 10];
        reader.read_exact(&mut buffer)?;

        let tta_offset = if &buffer[0..3] == b"ID3" {
            let flags = buffer[5];
            let size_bytes = [buffer[6], buffer[7], buffer[8], buffer[9]];
            // ID3v2 uses synchsafe integers (7 usable bits per byte, MSB ignored)
            let size = (((size_bytes[0] & 0x7F) as u32) << 21)
                | (((size_bytes[1] & 0x7F) as u32) << 14)
                | (((size_bytes[2] & 0x7F) as u32) << 7)
                | ((size_bytes[3] & 0x7F) as u32);
            let header_size = 10u64;
            let footer_size = if (flags & 0x10) != 0 { 10u64 } else { 0u64 };
            Some(header_size + size as u64 + footer_size)
        } else if &buffer[0..3] == b"TTA" {
            Some(0)
        } else {
            None
        };

        instance.info = TrueAudioStreamInfo::from_reader(&mut reader, tta_offset)?;

        // Try APE tags from reader first, then ID3
        reader.seek(std::io::SeekFrom::Start(0))?;
        if let Ok(ape) = <crate::apev2::APEv2 as FileType>::load_from_reader(&mut reader) {
            let mut new_tags = crate::apev2::APEv2Tags::new();
            for key in ape.tags.keys() {
                if let Some(value) = ape.tags.get(&key) {
                    let _ = new_tags.set(&key, value.clone());
                }
            }
            instance.tags = Some(TrueAudioTags::APE(new_tags));
        } else {
            // Try ID3 tags from reader
            reader.seek(std::io::SeekFrom::Start(0))?;
            if let Ok(id3) = <crate::id3::ID3 as FileType>::load_from_reader(&mut reader) {
                instance.tags = Some(TrueAudioTags::ID3(id3.tags.clone()));
            }
        }

        Ok(instance)
    }

    fn save(&mut self) -> Result<()> {
        let filename = self
            .filename
            .as_ref()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        // Save tags based on their type
        match &mut self.tags {
            Some(TrueAudioTags::ID3(_id3_tags)) => {
                // Save ID3 tags to TrueAudio file
                // Create an ID3 file instance to handle the saving
                let mut id3_file = crate::id3::ID3::new();
                id3_file.filename = Some(filename.clone());

                // Copy tags from our TrueAudio ID3 tags to the ID3 file
                if let Some(TrueAudioTags::ID3(tags)) = &self.tags {
                    // Copy all the ID3 tag data
                    id3_file.tags = tags.clone();
                }

                // Save the ID3 tags to file
                id3_file.save()
            }
            Some(TrueAudioTags::APE(_ape_tags)) => {
                // Save APE tags to TrueAudio file (: tagger.tags = APEv2())
                // Create an APEv2 instance to handle the saving (like MonkeysAudio does)
                let mut apev2 = crate::apev2::APEv2::new();
                apev2.filename = Some(filename.clone());

                // Copy tags from our TrueAudio APE tags to the APEv2 instance
                if let Some(TrueAudioTags::APE(tags)) = &self.tags {
                    // Copy all the APE tag items
                    for (key, value) in tags.items() {
                        let _ = apev2.tags.set(&key, value.clone());
                    }
                }

                // Save the APE tags to file
                apev2.save()
            }
            None => {
                // No tags to save
                Ok(())
            }
        }
    }

    fn clear(&mut self) -> Result<()> {
        // Remove tags from file based on tag type using standalone clear functions
        if let Some(filename) = &self.filename {
            match &self.tags {
                Some(TrueAudioTags::ID3(_)) => {
                    // Remove ID3 tags from file using standalone function
                    crate::id3::clear(filename)?;
                }
                Some(TrueAudioTags::APE(_)) => {
                    // Remove APE tags from file using standalone function
                    crate::apev2::clear(filename)?;
                }
                None => {
                    // No tags to clear
                }
            }
        }

        self.tags = None;
        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        match &self.tags {
            Some(TrueAudioTags::ID3(id3_tags)) => {
                let mut id3_file = crate::id3::ID3::new();
                id3_file.tags = id3_tags.clone();
                id3_file.save_to_writer(writer)
            }
            Some(TrueAudioTags::APE(ape_tags)) => {
                let mut apev2 = crate::apev2::APEv2::new();
                for (key, value) in ape_tags.items() {
                    let _ = apev2.tags.set(&key, value.clone());
                }
                apev2.save_to_writer(writer)?;
                Ok(())
            }
            None => Ok(()),
        }
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        match &self.tags {
            Some(TrueAudioTags::ID3(_)) => {
                let mut id3_file = crate::id3::ID3::new();
                id3_file.clear_writer(writer)?;
            }
            Some(TrueAudioTags::APE(_)) | None => {
                let mut apev2 = crate::apev2::APEv2::new();
                apev2.clear_writer(writer)?;
            }
        }
        self.tags = None;
        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy().to_string();
        match &self.tags {
            Some(TrueAudioTags::ID3(_id3_tags)) => {
                let mut id3_file = crate::id3::ID3::new();
                id3_file.filename = Some(path_str);
                if let Some(TrueAudioTags::ID3(tags)) = &self.tags {
                    id3_file.tags = tags.clone();
                }
                id3_file.save()
            }
            Some(TrueAudioTags::APE(_ape_tags)) => {
                let mut apev2 = crate::apev2::APEv2::new();
                apev2.filename = Some(path_str);
                if let Some(TrueAudioTags::APE(tags)) = &self.tags {
                    for (key, value) in tags.items() {
                        let _ = apev2.tags.set(&key, value.clone());
                    }
                }
                apev2.save()
            }
            None => Ok(()),
        }
    }

    /// Adds empty APEv2 tags to the file.
    ///
    /// Creates a new empty APE tag structure if none exists. If tags already exist,
    /// returns an error. TrueAudio files typically use APE tags rather than ID3.
    ///
    /// # Errors
    ///
    /// Returns `AudexError::InvalidOperation` if tags already exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::trueaudio::TrueAudio;
    /// use audex::FileType;
    ///
    /// let mut tta = TrueAudio::load("song.tta")?;
    /// if tta.tags.is_none() {
    ///     tta.add_tags()?;
    /// }
    /// tta.set("title", vec!["My Song".to_string()])?;
    /// tta.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        // Check the actual tags field — TrueAudio supports both ID3 and APE,
        // so we must check the underlying enum, not the trait's tags() method
        // which only exposes the ID3 variant.
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(TrueAudioTags::APE(APEv2Tags::new()));
        Ok(())
    }

    fn keys(&self) -> Vec<String> {
        // TrueAudio supports both ID3 and APE tags
        match &self.tags {
            Some(TrueAudioTags::ID3(tags)) => tags.keys(),
            Some(TrueAudioTags::APE(tags)) => tags.keys(),
            None => Vec::new(),
        }
    }

    fn get(&self, key: &str) -> Option<Vec<String>> {
        // TrueAudio supports both ID3 and APE tags
        match &self.tags {
            Some(TrueAudioTags::ID3(tags)) => tags.get_text_values(key),
            Some(TrueAudioTags::APE(tags)) => tags.get(key)?.as_text_list().ok(),
            None => None,
        }
    }

    fn set(&mut self, key: &str, values: Vec<String>) -> Result<()> {
        // TrueAudio supports both ID3 and APE tags
        match &mut self.tags {
            Some(TrueAudioTags::ID3(tags)) => tags.setall_text(key, values),
            Some(TrueAudioTags::APE(tags)) => {
                // Convert Vec<String> to APEValue using the text() constructor
                // Join multiple values with null bytes (APE tag standard)
                let text = values.join("\0");
                let ape_value = crate::apev2::APEValue::text(text);
                tags.set(key, ape_value)
            }
            None => Err(AudexError::Unsupported(
                "No tags present. Call add_tags() first.".to_string(),
            )),
        }
    }

    fn remove(&mut self, key: &str) -> Result<()> {
        // TrueAudio supports both ID3 and APE tags
        match &mut self.tags {
            Some(TrueAudioTags::ID3(tags)) => {
                tags.remove(key);
                Ok(())
            }
            Some(TrueAudioTags::APE(tags)) => {
                tags.remove(key);
                Ok(())
            }
            None => Ok(()), // Nothing to remove
        }
    }

    fn tags(&self) -> Option<&Self::Tags> {
        // For FileType compatibility, only return ID3 tags
        match &self.tags {
            Some(TrueAudioTags::ID3(tags)) => Some(tags),
            _ => None,
        }
    }

    fn tags_mut(&mut self) -> Option<&mut Self::Tags> {
        // For FileType compatibility, only return ID3 tags
        match &mut self.tags {
            Some(TrueAudioTags::ID3(tags)) => Some(tags),
            _ => None,
        }
    }

    fn info(&self) -> &Self::Info {
        &self.info
    }

    fn score(filename: &str, header: &[u8]) -> i32 {
        let mut score = 0;

        // Check for TrueAudio signatures
        if header.len() >= 3 {
            if &header[0..3] == b"TTA" {
                score += 1;
            }
            if &header[0..3] == b"ID3" {
                score += 1;
            }
        }

        // Check file extension (weighted heavily)
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".tta") {
            score += 2;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/x-tta"]
    }
}

/// Clear all metadata from a TrueAudio file
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut file = TrueAudio::load(path)?;
    file.clear()
}
