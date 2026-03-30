//! WAV/WAVE format support
//!
//! Comprehensive support for the Microsoft WAVE audio file format with ID3v2 tag support.
//! WAVE files use the RIFF container format to store uncompressed, lossless, or lossy
//! audio data depending on the codec in the `fmt ` chunk.
//!
//! # Format Overview
//!
//! WAVE (Waveform Audio File Format) is a widely-used audio file format developed by Microsoft
//! and IBM. It is a variant of the RIFF (Resource Interchange File Format) container format
//! specifically designed for storing audio data.
//!
//! ## File Structure
//!
//! A WAVE file consists of a RIFF container with several chunks:
//! ```text
//! RIFF
//! ├─ "WAVE" (type identifier)
//! ├─ fmt  (format chunk - required)
//! │   └─ Audio format, sample rate, channels, bit depth
//! ├─ data (data chunk - required)
//! │   └─ Raw audio samples
//! └─ id3  (optional ID3v2 tags)
//! ```
//!
//! ## Tag Support
//!
//! This implementation supports ID3v2 tags stored in "id3 " or "ID3 " chunks.
//!
//! ## Audio Formats
//!
//! Common WAVE audio formats:
//! - **PCM (0x0001)**: Uncompressed linear pulse-code modulation
//! - **IEEE Float (0x0003)**: Floating-point audio data
//! - **ADPCM (0x0002)**: Adaptive differential PCM compression
//! - **Extensible (0xFFFE)**: Extended format for more than 2 channels
//!
//! # Usage Examples
//!
//! ## Loading and reading stream information
//!
//! ```no_run
//! use audex::wave::WAVE;
//! use audex::FileType;
//! use audex::StreamInfo;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load a WAVE file
//! let wave = WAVE::load("/path/to/audio.wav")?;
//!
//! // Access stream information
//! println!("Sample rate: {} Hz", wave.info.sample_rate);
//! println!("Channels: {}", wave.info.channels);
//! println!("Bit depth: {} bits", wave.info.bits_per_sample);
//!
//! if let Some(duration) = wave.info.length() {
//!     println!("Duration: {:.2} seconds", duration.as_secs_f64());
//! }
//!
//! if let Some(bitrate) = wave.info.bitrate() {
//!     println!("Bitrate: {} bps", bitrate);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Working with ID3v2 tags
//!
//! ```no_run
//! use audex::wave::WAVE;
//! use audex::FileType;
//! use audex::tags::Tags;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load WAVE file
//! let mut wave = WAVE::load("/path/to/audio.wav")?;
//!
//! // Read tags using the Tags trait
//! if let Some(ref tags) = wave.tags {
//!     if let Some(title) = tags.get("TIT2") {
//!         println!("Title: {:?}", title);
//!     }
//! } else {
//!     // Create tags if they don't exist
//!     wave.add_tags()?;
//! }
//!
//! // Modify tags using the Tags trait
//! if let Some(ref mut tags) = wave.tags {
//!     tags.set("TIT2", vec!["New Title".to_string()]);
//!     tags.set("TPE1", vec!["Artist Name".to_string()]);
//!     tags.set("TALB", vec!["Album Name".to_string()]);
//! }
//!
//! // Save changes
//! wave.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Clearing tags
//!
//! ```no_run
//! use audex::wave::WAVE;
//! use audex::FileType;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut wave = WAVE::load("/path/to/audio.wav")?;
//!
//! // Remove all ID3 tags
//! wave.clear()?;
//! wave.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # References
//!
//! - [Microsoft WAVE PCM Soundfile Format](http://soundfile.sapp.org/doc/WaveFormat/)
//! - [Multimedia Programming Interface and Data Specifications](https://www.aelius.com/njh/wavemetatools/doc/riffmci.pdf)

use crate::tags::{PaddingInfo, Tags};
use crate::{
    AudexError, FileType, Result, StreamInfo,
    id3::{ID3Tags, specs, tags::ID3Header},
};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::iff::{
    IffChunkAsync, RiffFileAsync, resize_riff_chunk_async, update_riff_file_size_async,
};
#[cfg(feature = "async")]
use crate::util::{
    delete_bytes_async, insert_bytes_async, loadfile_read_async, loadfile_write_async,
};
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

/// A parsed RIFF chunk within a WAVE file
///
/// Represents a single chunk's location and size within the file.
/// Use [`read_data`](RiffChunk::read_data) to read the chunk's payload.
#[derive(Debug, Clone)]
pub struct RiffChunk {
    /// FOURCC chunk identifier (e.g., "fmt ", "data", "id3 ")
    pub id: String,
    /// Chunk size as declared in the header (excludes the 8-byte chunk header)
    pub size: u32,
    /// Absolute file offset of the chunk header
    pub offset: u64,
    /// Absolute file offset where chunk data begins (after the 8-byte header)
    pub data_offset: u64,
    /// Actual data size (same as `size` in this implementation)
    pub data_size: u32,
}

impl RiffChunk {
    /// Read this chunk's data payload from the given reader
    pub fn read_data<R: Read + Seek>(&self, reader: &mut R) -> Result<Vec<u8>> {
        // Enforce the library-wide tag allocation ceiling
        crate::limits::ParseLimits::default()
            .check_tag_size(self.data_size as u64, "RIFF chunk")?;
        reader.seek(SeekFrom::Start(self.data_offset))?;
        let mut data = vec![0u8; self.data_size as usize];
        reader.read_exact(&mut data)?;
        Ok(data)
    }
}

/// Parsed RIFF/WAVE file structure containing all chunk locations
#[derive(Debug, Clone)]
pub struct RiffFile {
    /// RIFF form type (should be "WAVE" for WAV files)
    pub file_type: String,
    /// All chunks found in the file, in order
    pub chunks: Vec<RiffChunk>,
    /// Total file size as declared in the RIFF header
    pub file_size: u32,
}

impl RiffFile {
    /// Parse RIFF file structure from a reader, extracting all chunk locations
    pub fn parse<R: Read + Seek + ?Sized>(reader: &mut R) -> Result<Self> {
        // Read RIFF header
        let mut header = [0u8; 12];
        reader.read_exact(&mut header)?;

        if &header[0..4] != b"RIFF" {
            return Err(AudexError::WAVError("Expected RIFF signature".to_string()));
        }

        let file_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let file_type = String::from_utf8_lossy(&header[8..12]).into_owned();

        if file_type != "WAVE" {
            return Err(AudexError::WAVError("Expected WAVE format".to_string()));
        }

        let mut chunks = Vec::new();
        let mut offset = 12u64; // After RIFF header

        // Clamp the loop bound to the actual stream size to avoid
        // seeking past EOF when the declared size is inflated
        let actual_end = reader.seek(SeekFrom::End(0)).unwrap_or(u64::MAX);
        reader.seek(SeekFrom::Start(offset))?;
        let end_bound = (file_size as u64 + 8).min(actual_end);

        // Parse chunks
        let mut consecutive_zero_chunks = 0u32;
        while offset < end_bound {
            reader.seek(SeekFrom::Start(offset))?;

            let mut chunk_header = [0u8; 8];
            if reader.read_exact(&mut chunk_header).is_err() {
                break; // End of file
            }

            let chunk_id = String::from_utf8_lossy(&chunk_header[0..4]).into_owned();
            let chunk_size = u32::from_le_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
            ]);

            // A zero-size chunk is valid per the RIFF spec. Skip past its header
            // and continue parsing to avoid hiding valid trailing chunks.
            // Guard against infinite loops from consecutive zero-size chunks
            // by limiting how many we tolerate in a row.
            if chunk_size == 0 {
                consecutive_zero_chunks += 1;
                if consecutive_zero_chunks > 64 {
                    break; // Too many consecutive zero-size chunks; stop parsing
                }
                offset += 8; // Advance past the chunk header
                continue;
            }
            consecutive_zero_chunks = 0;

            let chunk = RiffChunk {
                id: chunk_id,
                size: chunk_size,
                offset,
                data_offset: offset + 8,
                data_size: chunk_size,
            };

            chunks.push(chunk);

            // Move to next chunk (pad to even boundary).
            // Use checked_add to prevent wrapping on malformed size fields.
            let advance = 8u64 + chunk_size as u64 + if chunk_size % 2 == 1 { 1 } else { 0 };
            offset = match offset.checked_add(advance) {
                Some(next) => next,
                None => break, // Offset would overflow — stop parsing
            };
        }

        Ok(RiffFile {
            file_type,
            chunks,
            file_size,
        })
    }

    /// Find chunk by ID (handles both "fmt" and "fmt " style IDs)
    pub fn find_chunk(&self, id: &str) -> Option<&RiffChunk> {
        self.chunks.iter().find(|chunk| {
            // Try exact match first
            if chunk.id.eq_ignore_ascii_case(id) {
                return true;
            }

            // Try with padding (RIFF chunk IDs are 4 bytes, padded with spaces)
            let padded_id = if id.len() < 4 {
                format!("{:<4}", id) // Pad to 4 characters with spaces
            } else {
                id.to_string()
            };

            chunk.id.eq_ignore_ascii_case(&padded_id)
                || chunk.id.trim_end().eq_ignore_ascii_case(id.trim_end())
        })
    }

    /// Check if a chunk with the given ID exists
    pub fn has_chunk(&self, id: &str) -> bool {
        self.find_chunk(id).is_some()
    }
}

/// WAV stream information
///
/// Contains audio stream information extracted from the WAVE file's format chunk.
/// This struct implements the `StreamInfo` trait, providing a standardized interface
/// for accessing audio properties.
///
/// # Audio Format Codes
///
/// Common audio format values:
/// - **0x0001**: PCM (uncompressed)
/// - **0x0002**: Microsoft ADPCM
/// - **0x0003**: IEEE Float
/// - **0x0006**: ITU G.711 a-law
/// - **0x0007**: ITU G.711 μ-law
/// - **0xFFFE**: WAVE_FORMAT_EXTENSIBLE
///
/// # Fields
///
/// - **length**: Total duration of the audio stream
/// - **bitrate**: Bitrate in bits per second (calculated as: channels × bits_per_sample × sample_rate)
/// - **channels**: Number of audio channels (1 = mono, 2 = stereo, etc.)
/// - **sample_rate**: Sample rate in Hz (e.g., 44100, 48000, 96000)
/// - **bits_per_sample**: Bit depth per sample (e.g., 8, 16, 24, 32)
/// - **audio_format**: WAVE audio format code
/// - **number_of_samples**: Total number of audio samples
///
/// # Examples
///
/// ```no_run
/// use audex::wave::WAVE;
/// use audex::FileType;
/// use audex::StreamInfo;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let wave = WAVE::load("/path/to/audio.wav")?;
///
/// // Access format information
/// println!("Format: {} Hz, {} channels, {} bits",
///          wave.info.sample_rate,
///          wave.info.channels,
///          wave.info.bits_per_sample);
///
/// // Check audio quality
/// match (wave.info.sample_rate, wave.info.bits_per_sample) {
///     (44100, 16) => println!("CD quality"),
///     (48000, 16) => println!("DAT quality"),
///     (48000, 24) => println!("Studio quality"),
///     (96000, 24) => println!("High-resolution audio"),
///     _ => println!("Custom quality"),
/// }
///
/// // Calculate file size estimate
/// if let (Some(duration), Some(bitrate)) = (wave.info.length(), wave.info.bitrate()) {
///     let estimated_size = duration.as_secs() * bitrate as u64 / 8;
///     println!("Estimated audio size: {} bytes", estimated_size);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Default)]
pub struct WAVEStreamInfo {
    /// Duration of the audio stream
    pub length: Option<Duration>,

    /// Bitrate in bits per second
    pub bitrate: Option<u32>,

    /// Number of audio channels
    pub channels: u16,

    /// Sample rate in Hz
    pub sample_rate: u32,

    /// Bit depth per sample
    pub bits_per_sample: u16,

    /// WAVE audio format code (0x0001 = PCM, 0x0003 = IEEE Float, etc.)
    pub audio_format: u16,

    /// Total number of audio samples
    pub number_of_samples: u64,
}

impl StreamInfo for WAVEStreamInfo {
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
        Some(self.channels)
    }
    fn bits_per_sample(&self) -> Option<u16> {
        Some(self.bits_per_sample)
    }

    /// Custom pprint for WAVE format that matches expected test format
    fn pprint(&self) -> String {
        let mut output = String::new();

        if let Some(length) = self.length {
            output.push_str(&format!("{:.2} seconds\n", length.as_secs_f64()));
        }

        if let Some(bitrate) = self.bitrate {
            output.push_str(&format!("{} bps\n", bitrate));
        }

        output.push_str(&format!("{} Hz\n", self.sample_rate));

        let channel_text = if self.channels == 1 {
            "channel"
        } else {
            "channels"
        };
        output.push_str(&format!("{} {}\n", self.channels, channel_text));

        output.push_str(&format!("{} bit\n", self.bits_per_sample));

        // sample_rate, channels, and bits_per_sample are always formatted above,
        // so output is guaranteed to be non-empty at this point.
        output.trim_end().to_string()
    }
}

impl WAVEStreamInfo {
    /// Parse from RIFF file
    pub fn from_riff_file<R: Read + Seek>(riff: &RiffFile, reader: &mut R) -> Result<Self> {
        // Find format chunk
        let fmt_chunk = riff
            .find_chunk("fmt")
            .ok_or_else(|| AudexError::WAVError("No 'fmt' chunk found".to_string()))?;

        if fmt_chunk.data_size < 16 {
            return Err(AudexError::WAVInvalidChunk(
                "Format chunk too small".to_string(),
            ));
        }

        // Read format chunk data
        let fmt_data = fmt_chunk.read_data(reader)?;

        // Parse format chunk (minimum 16 bytes, little-endian)
        // - audio_format (2 bytes)
        // - channels (2 bytes)
        // - sample_rate (4 bytes)
        // - byte_rate (4 bytes)
        // - block_align (2 bytes)
        // - bits_per_sample (2 bytes)

        let audio_format = u16::from_le_bytes([fmt_data[0], fmt_data[1]]);
        let channels = u16::from_le_bytes([fmt_data[2], fmt_data[3]]);
        let sample_rate = u32::from_le_bytes([fmt_data[4], fmt_data[5], fmt_data[6], fmt_data[7]]);
        let _byte_rate = u32::from_le_bytes([fmt_data[8], fmt_data[9], fmt_data[10], fmt_data[11]]);
        let block_align = u16::from_le_bytes([fmt_data[12], fmt_data[13]]);
        let bits_per_sample = u16::from_le_bytes([fmt_data[14], fmt_data[15]]);

        // block_align must be nonzero for all audio formats.
        // A zero value indicates a corrupt file header and would cause
        // incorrect sample count calculations.
        if block_align == 0 {
            return Err(AudexError::WAVInvalidChunk(
                "block_align must be nonzero".to_string(),
            ));
        }

        // Zero channels or bits_per_sample indicates a malformed header but we
        // still attempt to parse the rest of the file. The resulting bitrate will
        // be zero, which callers should treat as "unknown".
        if channels == 0 {
            warn_event!("WAVE: channels is zero in fmt chunk — bitrate will be reported as 0");
        }
        if bits_per_sample == 0 {
            warn_event!(
                "WAVE: bits_per_sample is zero in fmt chunk — bitrate will be reported as 0"
            );
        }

        // Calculate bitrate; report None when any component is zero
        // so callers can distinguish "unknown" from a genuine zero rate.
        let bitrate = if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
            None
        } else {
            Some(
                (channels as u32)
                    .saturating_mul(bits_per_sample as u32)
                    .saturating_mul(sample_rate),
            )
        };

        // Calculate duration from data chunk
        let mut number_of_samples = 0u64;
        let mut length = None;

        if let Some(data_chunk) = riff.find_chunk("data") {
            if sample_rate > 0 {
                number_of_samples = data_chunk.data_size as u64 / block_align as u64;
                let duration_secs = number_of_samples as f64 / sample_rate as f64;
                length = Some(Duration::from_secs_f64(duration_secs));
            }
            // When sample_rate is 0, number_of_samples and length remain at
            // their defaults (0 and None) since the values are meaningless
            // without a valid sample rate.
        }

        Ok(WAVEStreamInfo {
            length,
            bitrate,
            channels,
            sample_rate,
            bits_per_sample,
            audio_format,
            number_of_samples,
        })
    }
}

/// WAV file with ID3v2 tags in RIFF chunks
///
/// Represents a complete WAVE audio file with stream information and optional ID3v2 tags.
/// This type provides methods for loading, reading, modifying, and saving WAVE files.
///
/// # Structure
///
/// - **info**: Audio stream information (sample rate, channels, bit depth, etc.)
/// - **tags**: Optional ID3v2 tags stored in the RIFF "id3 " chunk
/// - **filename**: Path to the source file (if loaded from disk)
/// - **riff_file**: Internal RIFF file structure for chunk management
///
/// # Tag Storage
///
/// ID3v2 tags are stored in a special RIFF chunk with the ID "id3 " (note the trailing space).
/// This is the standard way to embed ID3 tags in WAVE files, supported by most audio software.
///
/// # File Format Compatibility
///
/// - Uses ID3v2.3 by default for maximum compatibility
/// - Properly handles RIFF chunk alignment (16-bit boundaries)
/// - Preserves all audio data during tag modifications
/// - Supports reading from and writing to the same file
///
/// # Examples
///
/// ## Loading and inspecting a file
///
/// ```no_run
/// use audex::wave::WAVE;
/// use audex::FileType;
/// use audex::StreamInfo;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Load a WAVE file
/// let wave = WAVE::load("/path/to/audio.wav")?;
///
/// // Print file information
/// println!("{}", wave.pprint());
///
/// // Access stream info
/// println!("Sample rate: {} Hz", wave.info.sample_rate);
/// println!("Channels: {}", wave.info.channels);
/// println!("Bit depth: {}", wave.info.bits_per_sample);
///
/// // Check for tags
/// if wave.tags.is_some() {
///     println!("File has ID3 tags");
/// } else {
///     println!("No tags found");
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Editing tags
///
/// ```no_run
/// use audex::wave::WAVE;
/// use audex::FileType;
/// use audex::tags::Tags;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Load file
/// let mut wave = WAVE::load("/path/to/audio.wav")?;
///
/// // Create tags if they don't exist
/// if wave.tags.is_none() {
///     wave.add_tags()?;
/// }
///
/// // Set tag values
/// if let Some(ref mut tags) = wave.tags {
///     tags.set_single("TIT2", "Song Title".to_string());
///     tags.set_single("TPE1", "Artist Name".to_string());
///     tags.set_single("TALB", "Album Name".to_string());
///     tags.set_single("TDRC", "2024".to_string());
/// }
///
/// // Save changes
/// wave.save()?;
/// # Ok(())
/// # }
/// ```
///
/// ## Removing tags
///
/// ```no_run
/// use audex::wave::WAVE;
/// use audex::FileType;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut wave = WAVE::load("/path/to/audio.wav")?;
///
/// // Clear all tags
/// wave.clear()?;
///
/// // Save the file without tags
/// wave.save()?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct WAVE {
    /// Audio stream information
    pub info: WAVEStreamInfo,

    /// Optional ID3v2 tags
    pub tags: Option<ID3Tags>,

    /// Source file path (if loaded from disk)
    pub filename: Option<String>,

    /// Internal RIFF file structure for chunk management
    riff_file: Option<RiffFile>,
}

impl WAVE {
    /// Create a new empty WAVE instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: WAVEStreamInfo::default(),
            tags: None,
            filename: None,
            riff_file: None,
        }
    }

    /// Parse WAVE file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Parse RIFF structure
        reader.seek(SeekFrom::Start(0))?;
        let riff_file = RiffFile::parse(reader)?;
        for _chunk in &riff_file.chunks {
            trace_event!(chunk_id = %_chunk.id, chunk_size = _chunk.size, "WAVE chunk");
        }

        // Parse stream info from format and data chunks
        self.info = WAVEStreamInfo::from_riff_file(&riff_file, reader)?;

        // Parse ID3 tags from 'id3' or 'ID3' chunk
        self.tags = if let Some(id3_chunk) = riff_file
            .find_chunk("id3")
            .or_else(|| riff_file.find_chunk("ID3"))
        {
            let id3_data = id3_chunk.read_data(reader)?;

            // Parse ID3 header first
            if id3_data.len() >= 10 {
                match specs::ID3Header::from_bytes(&id3_data) {
                    Ok(specs_header) => {
                        let header = ID3Header::from_specs_header(&specs_header);
                        ID3Tags::from_data(&id3_data, &header).ok()
                    }
                    Err(_) => None, // Invalid ID3 header
                }
            } else {
                None // ID3 data too small
            }
        } else {
            None
        };

        self.riff_file = Some(riff_file);
        Ok(())
    }

    /// Clear ID3 tags
    pub fn clear(&mut self) -> Result<()> {
        self.tags = None;

        // Remove ID3 chunk from RIFF file if present
        let has_id3_chunk = if let Some(ref riff_file) = self.riff_file {
            riff_file
                .chunks
                .iter()
                .any(|chunk| chunk.id == "id3 " || chunk.id == "ID3 ")
        } else {
            false
        };

        if has_id3_chunk {
            if let Some(filename) = self.filename.clone() {
                self.remove_id3_chunk(&filename)?;
            }
        }

        Ok(())
    }

    /// Remove ID3 chunk from RIFF file.
    fn remove_id3_chunk(&mut self, filename: &str) -> Result<()> {
        use std::fs::OpenOptions;

        let mut file = OpenOptions::new().read(true).write(true).open(filename)?;
        // Compute how many bytes the ID3 chunk occupies so we can truncate afterward
        let id3_total_size = self.riff_file.as_ref().and_then(|rf| {
            rf.chunks
                .iter()
                .find(|c| c.id == "id3 " || c.id == "ID3 ")
                .map(|chunk| {
                    let pad = if chunk.data_size % 2 == 1 { 1u64 } else { 0 };
                    8 + chunk.data_size as u64 + pad
                })
        });
        let size_before = file.seek(SeekFrom::End(0))?;
        self.remove_id3_chunk_writer(&mut file)?;
        // Truncate file to the new logical size (the writer path cannot
        // truncate through a trait object, so we do it here with the File).
        if let Some(removed) = id3_total_size {
            let new_size = size_before.saturating_sub(removed);
            file.set_len(new_size)?;
        }
        Ok(())
    }

    /// Remove ID3 chunk from RIFF data in-place on any Read + Write + Seek handle.
    ///
    /// Deletes the ID3 chunk bytes in-place and updates the RIFF header size.
    fn remove_id3_chunk_writer(&mut self, file: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        if let Some(ref riff_file) = self.riff_file {
            if let Some(chunk) = riff_file
                .chunks
                .iter()
                .find(|c| c.id == "id3 " || c.id == "ID3 ")
            {
                let pad = if chunk.data_size % 2 == 1 { 1u64 } else { 0 };
                let total_size = 8 + chunk.data_size as u64 + pad; // 8 = id(4) + size(4)
                let chunk_offset = chunk.offset;
                let old_riff_size = riff_file.file_size;

                // Remove chunk bytes in-place
                Self::delete_bytes_dyn(file, total_size, chunk_offset)?;

                // Perform subtraction in u64 space to avoid truncating total_size,
                // then convert back to u32 for the RIFF header
                let new_riff_size = (old_riff_size as u64)
                    .checked_sub(total_size)
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| {
                        AudexError::InvalidData(
                            "ID3 chunk size exceeds RIFF container size".to_string(),
                        )
                    })?;
                file.seek(SeekFrom::Start(4))?;
                file.write_all(&new_riff_size.to_le_bytes())?;
                file.flush()?;
            }
        }

        // Update internal representation
        if let Some(ref mut riff_file) = self.riff_file {
            riff_file
                .chunks
                .retain(|chunk| chunk.id != "id3 " && chunk.id != "ID3 ");
        }

        Ok(())
    }

    /// Add/ensure tags exist (creates empty tags if none exist)
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::WAVError("ID3 tag already exists".to_string()));
        }
        use crate::id3::ID3Tags;
        self.tags = Some(ID3Tags::new());
        Ok(())
    }

    /// Get MIME types for this format
    pub fn mime(&self) -> &'static [&'static str] {
        WAVE::mime_types()
    }

    /// Pretty print file information
    pub fn pprint(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "WAVE File: {}\n",
            self.filename.as_deref().unwrap_or("<unnamed>")
        ));
        output.push_str(&self.info.pprint());

        if let Some(tags) = &self.tags {
            output.push_str("\n\nID3v2 Tags:\n");
            for key in tags.keys() {
                if let Some(values) = tags.get(&key) {
                    for value in values {
                        output.push_str(&format!("{}: {}\n", key, value));
                    }
                }
            }
        }

        output
    }

    /// Save WAVE with format-specific options
    pub fn save_with_options(
        &mut self,
        file_path: Option<&str>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        // Set default values for format compatibility
        let v2_version_option = v2_version.unwrap_or(3); // Default to v2.3 for WAVE compatibility
        let v23_sep_string = v23_sep.unwrap_or("/").to_string(); // Convert Option<&str> to String

        // Use provided file_path or fall back to stored filename
        let target_path = match file_path {
            Some(path) => path.to_string(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };

        // Call save_to_file_with_options with the specific parameters
        self.save_to_file_with_options(target_path, v2_version_option, Some(v23_sep_string))
    }

    /// Save ID3 tags to WAV file by modifying the ID3 chunk
    pub fn save_to_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        // Use ID3v2.3 by default for WAVE compatibility
        self.save_to_file_with_options(path.as_ref(), 3, Some("/".to_string()))
    }

    /// Save ID3 tags to WAV file with specific ID3 version options
    fn save_to_file_with_options<P: AsRef<Path>>(
        &mut self,
        path: P,
        v2_version: u8,
        v23_sep: Option<String>,
    ) -> Result<()> {
        use std::fs::OpenOptions;

        let file_path = path.as_ref();

        let mut file = OpenOptions::new().read(true).write(true).open(file_path)?;
        let size_before = file.metadata()?.len();

        self.save_to_writer_impl(&mut file, v2_version, v23_sep)?;

        // If the file shrank (e.g. smaller tags), truncate to the new RIFF
        // size. The RIFF header at offset 4 stores the file size minus 8.
        if let Some(ref rf) = self.riff_file {
            let logical_size = rf.file_size as u64 + 8;
            if logical_size < size_before {
                file.set_len(logical_size)?;
            }
        }

        Ok(())
    }

    /// Core save implementation that operates on any Read + Write + Seek handle.
    ///
    /// Parses the RIFF structure from the writer, locates or creates an ID3 chunk,
    /// and writes the current tags into it. Uses v2_version and v23_sep to control
    /// the ID3v2 encoding.
    fn save_to_writer_impl(
        &mut self,
        file: &mut dyn crate::ReadWriteSeek,
        v2_version: u8,
        v23_sep: Option<String>,
    ) -> Result<()> {
        // Parse RIFF structure to locate/create ID3 chunk
        let mut riff_file = RiffFile::parse(file)?;

        // Find existing ID3 chunk or determine where to insert it
        let id3_chunk = riff_file
            .find_chunk("id3")
            .or_else(|| riff_file.find_chunk("ID3"));

        // Generate new ID3 data if tags exist, using dynamic padding via PaddingInfo
        let new_id3_data = if let Some(ref tags) = self.tags {
            // First, compute the ID3 data size without padding to calculate PaddingInfo
            let minimal_data = self.generate_id3_data(tags, v2_version, v23_sep.clone(), 0)?;
            let needed = minimal_data.len();
            let available = id3_chunk.as_ref().map_or(0, |c| c.data_size as usize);
            let file_size = file.seek(SeekFrom::End(0))?;
            // trailing_size = data from the tag position to end of file
            let trailing_size = match id3_chunk.as_ref() {
                Some(chunk) => file_size as i64 - chunk.data_offset as i64,
                None => 0,
            };
            let info = PaddingInfo::new(available as i64 - needed as i64, trailing_size);
            let padding = info.get_default_padding().max(0) as usize;
            self.generate_id3_data(tags, v2_version, v23_sep, padding)?
        } else {
            Vec::new() // Empty tags - will effectively delete the ID3 chunk
        };

        if let Some(existing_chunk) = id3_chunk {
            // Existing ID3 chunk - resize it in place
            let old_size = existing_chunk.data_size as u64;
            let new_size = new_id3_data.len() as u64;

            // Add padding to align to even boundary (RIFF requirement)
            let padded_new_size = if new_size % 2 == 1 {
                new_size + 1
            } else {
                new_size
            };
            let old_padded_size = if old_size % 2 == 1 {
                old_size + 1
            } else {
                old_size
            };

            // Resize the chunk region in-place
            Self::resize_bytes_dyn(
                file,
                old_padded_size,
                padded_new_size,
                existing_chunk.data_offset,
            )?;

            // Write new ID3 data
            file.seek(SeekFrom::Start(existing_chunk.data_offset))?;
            file.write_all(&new_id3_data)?;

            // Write padding byte if needed
            if new_size % 2 == 1 {
                file.write_all(&[0])?;
            }

            // Update chunk size header (4 bytes before data_offset)
            let chunk_size_u32 = u32::try_from(new_size).map_err(|_| {
                AudexError::InvalidData("chunk size exceeds u32::MAX (> 4 GB)".to_string())
            })?;
            file.seek(SeekFrom::Start(existing_chunk.data_offset - 4))?;
            file.write_all(&chunk_size_u32.to_le_bytes())?;

            // Update RIFF file size header when chunk size changes.
            // Use checked arithmetic to prevent silent wrapping on overflow.
            if padded_new_size != old_padded_size {
                let size_diff = padded_new_size as i64 - old_padded_size as i64;
                let computed = (riff_file.file_size as i64)
                    .checked_add(size_diff)
                    .ok_or_else(|| {
                        AudexError::InvalidData("RIFF file size arithmetic overflow".to_string())
                    })?;
                let new_riff_size = u32::try_from(computed).map_err(|_| {
                    AudexError::InvalidData("RIFF file size does not fit in u32".to_string())
                })?;
                file.seek(SeekFrom::Start(4))?; // RIFF size is at offset 4
                file.write_all(&new_riff_size.to_le_bytes())?; // RIFF uses little-endian
                riff_file.file_size = new_riff_size;
            }
        } else if !new_id3_data.is_empty() {
            // No existing ID3 chunk - insert new one at the end before any "data" chunk
            self.insert_id3_chunk(file, &mut riff_file, new_id3_data)?;
        }

        // Update our cached RIFF structure
        self.riff_file = Some(riff_file);

        // Zero out any stale trailing bytes left by a shrink operation.
        // After zeroing, the cursor sits at the physical end of the writer.
        // Seek back to the logical RIFF end so that Cursor-based writers
        // (which cannot physically truncate) expose the correct position
        // to callers inspecting the stream afterward.
        Self::zero_stale_tail(file)?;
        if let Some(ref rf) = self.riff_file {
            let logical_end = rf.file_size as u64 + 8;
            file.seek(SeekFrom::Start(logical_end))?;
        }

        Ok(())
    }

    /// Generate ID3v2 data with proper header
    fn generate_id3_data(
        &self,
        tags: &ID3Tags,
        v2_version: u8,
        v23_sep: Option<String>,
        padding: usize,
    ) -> Result<Vec<u8>> {
        // Use provided version and separator parameters
        let default = crate::id3::tags::ID3SaveConfig::default();
        let config = crate::id3::tags::ID3SaveConfig {
            v2_version,
            v23_sep: v23_sep.unwrap_or(default.v23_sep),
            padding: if padding > 0 { Some(padding) } else { None },
            ..default
        };
        let tag_data = tags.write_with_config(&config)?;

        if tag_data.is_empty() {
            return Ok(Vec::new());
        }

        let mut id3v2_data = Vec::new();
        // Write ID3v2 header
        id3v2_data.extend_from_slice(b"ID3"); // File identifier
        id3v2_data.push(v2_version); // Major version
        id3v2_data.push(0); // Revision
        id3v2_data.push(0); // Flags

        // Write synchsafe size (tag data length, max 28 bits = 268,435,455)
        // Safe conversion: tag_data length must fit in u32 for the ID3v2 header
        let size = u32::try_from(tag_data.len()).map_err(|_| {
            AudexError::InvalidData("ID3 tag data length exceeds u32::MAX".to_string())
        })?;
        if size > 0x0FFF_FFFF {
            return Err(AudexError::InvalidData(
                "ID3 tag data exceeds synchsafe size limit (268,435,455 bytes)".to_string(),
            ));
        }
        let synchsafe = [
            ((size >> 21) & 0x7F) as u8,
            ((size >> 14) & 0x7F) as u8,
            ((size >> 7) & 0x7F) as u8,
            (size & 0x7F) as u8,
        ];
        id3v2_data.extend_from_slice(&synchsafe);

        // Write tag data
        id3v2_data.extend_from_slice(&tag_data);
        Ok(id3v2_data)
    }

    /// Insert new ID3 chunk into RIFF file
    fn insert_id3_chunk(
        &self,
        file: &mut dyn crate::ReadWriteSeek,
        riff_file: &mut RiffFile,
        id3_data: Vec<u8>,
    ) -> Result<()> {
        // Find a good place to insert the ID3 chunk - typically before "data" chunk
        let insert_offset = if let Some(data_chunk) = riff_file.find_chunk("data") {
            data_chunk.offset // Insert right before data chunk
        } else {
            // No data chunk found, append at end
            file.seek(SeekFrom::End(0))?;
            file.stream_position()?
        };

        // Calculate chunk size with padding
        let data_size = id3_data.len();
        let padding_size = if data_size % 2 == 1 { 1 } else { 0 };
        let total_chunk_size = 8 + data_size + padding_size; // 8 bytes header + data + padding

        // Insert space for the new chunk
        Self::insert_bytes_dyn(file, total_chunk_size as u64, insert_offset)?;

        // Write the new ID3 chunk at the inserted position
        file.seek(SeekFrom::Start(insert_offset))?;
        file.write_all(b"id3 ")?; // Chunk ID (4 bytes)
        // Guard against silent truncation when casting chunk data size to u32
        let data_size_u32 = u32::try_from(data_size)
            .map_err(|_| AudexError::InvalidData("ID3 chunk data size exceeds u32::MAX".into()))?;
        file.write_all(&data_size_u32.to_le_bytes())?; // Chunk size (4 bytes, little-endian)
        file.write_all(&id3_data)?; // Chunk data

        // Write padding byte if needed
        if padding_size > 0 {
            file.write_all(&[0])?;
        }

        // Update RIFF file size header (checked to prevent silent corruption)
        let new_file_size = riff_file
            .file_size
            // Guard against silent truncation when casting total chunk size to u32
            .checked_add(u32::try_from(total_chunk_size).map_err(|_| {
                AudexError::InvalidData("ID3 chunk total size exceeds u32::MAX".into())
            })?)
            .ok_or_else(|| {
                AudexError::InvalidData(
                    "RIFF file size would exceed u32::MAX after inserting ID3 chunk".to_string(),
                )
            })?;
        file.seek(SeekFrom::Start(4))?; // RIFF size is at offset 4
        file.write_all(&new_file_size.to_le_bytes())?;

        // Update our cached RIFF structure
        riff_file.file_size = new_file_size;
        let new_chunk = RiffChunk {
            id: "id3 ".to_string(),
            size: data_size as u32,
            offset: insert_offset,
            data_offset: insert_offset + 8,
            data_size: data_size as u32,
        };

        // Insert the chunk in the correct position in our vector
        let insert_index = riff_file
            .chunks
            .iter()
            .position(|chunk| chunk.offset >= insert_offset)
            .unwrap_or(riff_file.chunks.len());
        riff_file.chunks.insert(insert_index, new_chunk);

        Ok(())
    }
}

/// Buffer size for byte-shifting operations on trait-object writers.
const WAVE_IO_BUF: usize = 64 * 1024;

impl WAVE {
    /// Shift bytes within a writer from `src` to `dest`, moving `count` bytes.
    /// Handles overlapping regions correctly.
    fn move_bytes_dyn(
        file: &mut dyn crate::ReadWriteSeek,
        dest: u64,
        src: u64,
        count: u64,
    ) -> Result<()> {
        if count == 0 || src == dest {
            return Ok(());
        }
        let chunk_size = std::cmp::min(WAVE_IO_BUF as u64, count) as usize;
        let mut buf = vec![0u8; chunk_size];

        if src < dest {
            // Copy backwards to avoid overlap corruption
            let mut remaining = count;
            while remaining > 0 {
                let cur = std::cmp::min(chunk_size as u64, remaining) as usize;
                let s = src + remaining - cur as u64;
                let d = dest + remaining - cur as u64;
                file.seek(SeekFrom::Start(s))?;
                file.read_exact(&mut buf[..cur])?;
                file.seek(SeekFrom::Start(d))?;
                file.write_all(&buf[..cur])?;
                remaining -= cur as u64;
            }
        } else {
            // Copy forwards
            let mut moved = 0u64;
            while moved < count {
                let cur = std::cmp::min(chunk_size as u64, count - moved) as usize;
                file.seek(SeekFrom::Start(src + moved))?;
                file.read_exact(&mut buf[..cur])?;
                file.seek(SeekFrom::Start(dest + moved))?;
                file.write_all(&buf[..cur])?;
                moved += cur as u64;
            }
        }
        Ok(())
    }

    /// Delete `size` bytes starting at `offset` in the writer, shifting
    /// subsequent data left. The logical file size shrinks by `size` bytes.
    /// For std::fs::File the file is truncated; for Cursor<Vec<u8>> the vec
    /// is truncated; for other writers trailing bytes may remain but the RIFF
    /// header size will be authoritative.
    fn delete_bytes_dyn(file: &mut dyn crate::ReadWriteSeek, size: u64, offset: u64) -> Result<()> {
        if size == 0 {
            return Ok(());
        }
        let file_size = file.seek(SeekFrom::End(0))?;
        let delete_end = offset + size;
        let trailing = file_size.saturating_sub(delete_end);
        if trailing > 0 {
            Self::move_bytes_dyn(file, offset, delete_end, trailing)?;
        }
        // Best-effort truncation (saturating to avoid underflow if size > file_size)
        let new_size = file_size.saturating_sub(size);
        Self::truncate_stream(file, new_size);
        Ok(())
    }

    /// Insert `size` zero bytes at `offset` in the writer, shifting
    /// subsequent data right. The logical file size grows by `size` bytes.
    fn insert_bytes_dyn(file: &mut dyn crate::ReadWriteSeek, size: u64, offset: u64) -> Result<()> {
        if size == 0 {
            return Ok(());
        }
        let file_size = file.seek(SeekFrom::End(0))?;
        // Extend the stream
        let zero_buf = vec![0u8; WAVE_IO_BUF];
        let mut remaining = size;
        file.seek(SeekFrom::End(0))?;
        while remaining > 0 {
            let chunk = std::cmp::min(remaining, WAVE_IO_BUF as u64) as usize;
            file.write_all(&zero_buf[..chunk])?;
            remaining -= chunk as u64;
        }
        // Shift existing data right
        let bytes_to_move = file_size - offset;
        if bytes_to_move > 0 {
            Self::move_bytes_dyn(file, offset + size, offset, bytes_to_move)?;
            // Zero out the inserted region
            file.seek(SeekFrom::Start(offset))?;
            let mut rem = size;
            while rem > 0 {
                let chunk = std::cmp::min(rem, WAVE_IO_BUF as u64) as usize;
                file.write_all(&vec![0u8; chunk])?;
                rem -= chunk as u64;
            }
        }
        Ok(())
    }

    /// Resize a region at `offset` from `old_size` to `new_size` bytes.
    fn resize_bytes_dyn(
        file: &mut dyn crate::ReadWriteSeek,
        old_size: u64,
        new_size: u64,
        offset: u64,
    ) -> Result<()> {
        if old_size == new_size {
            return Ok(());
        }
        if new_size > old_size {
            let diff = new_size - old_size;
            Self::insert_bytes_dyn(file, diff, offset + old_size)
        } else {
            let diff = old_size - new_size;
            Self::delete_bytes_dyn(file, diff, offset + new_size)
        }
    }

    /// Best-effort truncation of the underlying stream to `new_size`.
    ///
    /// For types that support truncation (File, Cursor<Vec<u8>>), this will
    /// actually truncate. For others it is a no-op; the RIFF header size
    /// field is authoritative for determining the logical file extent.
    fn truncate_stream(file: &mut dyn crate::ReadWriteSeek, new_size: u64) {
        // We cannot call set_len through a trait object directly.
        // Seek to the desired end so that at least the stream position is
        // correct for subsequent writes.
        let _ = file.seek(SeekFrom::Start(new_size));
        let _ = file.flush();
    }

    /// Zero out any bytes between the logical RIFF end and the physical
    /// end of the writer. This prevents stale content from remaining
    /// recoverable after a shrink operation on non-truncatable writers.
    fn zero_stale_tail(writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        // Read the RIFF data size from the header (offset 4, little-endian u32)
        writer.seek(SeekFrom::Start(4))?;
        let mut size_buf = [0u8; 4];
        writer.read_exact(&mut size_buf)?;
        let riff_data_size = u32::from_le_bytes(size_buf) as u64;
        // Logical file end = "RIFF" (4 bytes) + size field (4 bytes) + data
        let logical_end = riff_data_size + 8;

        let physical_end = writer.seek(SeekFrom::End(0))?;
        if logical_end < physical_end {
            writer.seek(SeekFrom::Start(logical_end))?;
            let mut remaining = (physical_end - logical_end) as usize;
            // Write zeros in fixed-size chunks to avoid unbounded allocation
            // from a corrupted RIFF header with a small declared size.
            const CHUNK_SIZE: usize = 64 * 1024;
            let zeroes = [0u8; CHUNK_SIZE];
            while remaining > 0 {
                let n = remaining.min(CHUNK_SIZE);
                writer.write_all(&zeroes[..n])?;
                remaining -= n;
            }
            writer.flush()?;
        }

        Ok(())
    }
}

impl Default for WAVE {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "async")]
impl WAVE {
    /// Load WAV file asynchronously
    ///
    /// Parses the file structure, extracts stream information from the fmt chunk,
    /// and loads any ID3 tags present in the file.
    ///
    /// # Arguments
    /// * `path` - Path to the WAV file
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut wave = WAVE::new();
        wave.filename = Some(path.as_ref().to_string_lossy().to_string());

        wave.parse_file_async(&mut file).await?;
        Ok(wave)
    }

    /// Parse WAV file structure asynchronously
    async fn parse_file_async(&mut self, file: &mut TokioFile) -> Result<()> {
        // Parse RIFF structure
        file.seek(SeekFrom::Start(0)).await?;
        let riff_file = RiffFileAsync::parse(file).await?;

        // Validate file type
        if riff_file.file_type != "WAVE" {
            return Err(AudexError::WAVError("Expected WAVE format".to_string()));
        }

        // Parse stream info from fmt chunk
        self.info = Self::parse_stream_info_async(&riff_file, file).await?;

        // Parse ID3 tags from 'id3 ' or 'ID3 ' chunk
        self.tags = if let Some(id3_chunk) = riff_file
            .find_chunk("id3 ")
            .or_else(|| riff_file.find_chunk("ID3 "))
        {
            let id3_data = id3_chunk.read_data(file).await?;

            // Parse ID3 header and tags
            if id3_data.len() >= 10 {
                match specs::ID3Header::from_bytes(&id3_data) {
                    Ok(specs_header) => {
                        let header = ID3Header::from_specs_header(&specs_header);
                        ID3Tags::from_data(&id3_data, &header).ok()
                    }
                    Err(_) => None,
                }
            } else {
                None
            }
        } else {
            None
        };

        // Store RIFF structure in sync format for later use
        self.riff_file = Some(Self::convert_riff_to_riff_file(&riff_file));
        Ok(())
    }

    /// Convert RiffFileAsync to RiffFile structure
    fn convert_riff_to_riff_file(riff: &RiffFileAsync) -> RiffFile {
        let chunks = riff
            .chunks
            .iter()
            .map(|chunk| RiffChunk {
                id: chunk.id.clone(),
                size: chunk.size,
                offset: chunk.offset,
                data_offset: chunk.data_offset,
                data_size: chunk.data_size,
            })
            .collect();

        RiffFile {
            file_type: riff.file_type.clone(),
            chunks,
            file_size: riff.file_size,
        }
    }

    /// Parse stream information from fmt chunk asynchronously
    async fn parse_stream_info_async(
        riff: &RiffFileAsync,
        file: &mut TokioFile,
    ) -> Result<WAVEStreamInfo> {
        // Find and read fmt chunk
        let fmt_chunk = riff
            .find_chunk("fmt ")
            .ok_or_else(|| AudexError::WAVError("No 'fmt' chunk found".to_string()))?;

        if fmt_chunk.data_size < 16 {
            return Err(AudexError::WAVInvalidChunk(
                "Format chunk too small".to_string(),
            ));
        }

        let fmt_data = fmt_chunk.read_data(file).await?;

        // Parse format chunk fields (little-endian)
        let audio_format = u16::from_le_bytes([fmt_data[0], fmt_data[1]]);
        let channels = u16::from_le_bytes([fmt_data[2], fmt_data[3]]);
        let sample_rate = u32::from_le_bytes([fmt_data[4], fmt_data[5], fmt_data[6], fmt_data[7]]);
        let _byte_rate = u32::from_le_bytes([fmt_data[8], fmt_data[9], fmt_data[10], fmt_data[11]]);
        let block_align = u16::from_le_bytes([fmt_data[12], fmt_data[13]]);
        let bits_per_sample = u16::from_le_bytes([fmt_data[14], fmt_data[15]]);

        // block_align must be nonzero for all audio formats.
        // A zero value indicates a corrupt file header and would cause
        // incorrect sample count calculations.
        if block_align == 0 {
            return Err(AudexError::WAVInvalidChunk(
                "block_align must be nonzero".to_string(),
            ));
        }

        // Zero channels or bits_per_sample indicates a malformed header but we
        // still attempt to parse the rest of the file. The resulting bitrate will
        // be zero, which callers should treat as "unknown".
        if channels == 0 {
            warn_event!("WAVE: channels is zero in fmt chunk — bitrate will be reported as 0");
        }
        if bits_per_sample == 0 {
            warn_event!(
                "WAVE: bits_per_sample is zero in fmt chunk — bitrate will be reported as 0"
            );
        }

        // Calculate bitrate; report None when any component is zero
        // so callers can distinguish "unknown" from a genuine zero rate.
        let bitrate = if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
            None
        } else {
            Some(
                (channels as u32)
                    .saturating_mul(bits_per_sample as u32)
                    .saturating_mul(sample_rate),
            )
        };

        // Calculate duration from data chunk
        let mut number_of_samples = 0u64;
        let mut length = None;

        if let Some(data_chunk) = riff.find_chunk("data") {
            if sample_rate > 0 {
                number_of_samples = data_chunk.data_size as u64 / block_align as u64;
                let duration_secs = number_of_samples as f64 / sample_rate as f64;
                length = Some(Duration::from_secs_f64(duration_secs));
            }
            // When sample_rate is 0, number_of_samples and length remain at
            // their defaults (0 and None) since the values are meaningless
            // without a valid sample rate.
        }

        Ok(WAVEStreamInfo {
            length,
            bitrate,
            channels,
            sample_rate,
            bits_per_sample,
            audio_format,
            number_of_samples,
        })
    }

    /// Save ID3 tags to WAV file asynchronously
    ///
    /// Writes the current tags to the file, creating or updating the ID3 chunk as needed.
    pub async fn save_async(&mut self) -> Result<()> {
        let filename = self
            .filename
            .clone()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        self.save_to_file_async(&filename).await
    }

    /// Save ID3 tags to specified file asynchronously
    pub async fn save_to_file_async<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        // Use ID3v2.3 by default for WAV compatibility
        self.save_to_file_with_options_async(path.as_ref(), 3, Some("/".to_string()))
            .await
    }

    /// Save with configurable options asynchronously
    pub async fn save_with_options_async(
        &mut self,
        file_path: Option<&str>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        let version = v2_version.unwrap_or(3);
        let sep = v23_sep.unwrap_or("/").to_string();
        let target_path = match file_path {
            Some(path) => path.to_string(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };
        self.save_to_file_with_options_async(target_path, version, Some(sep))
            .await
    }

    /// Internal async save method with configurable options
    async fn save_to_file_with_options_async<P: AsRef<Path>>(
        &mut self,
        path: P,
        v2_version: u8,
        v23_sep: Option<String>,
    ) -> Result<()> {
        let mut file = loadfile_write_async(&path).await?;

        // Parse RIFF structure
        let mut riff_file = RiffFileAsync::parse(&mut file).await?;

        // Validate file type
        if riff_file.file_type != "WAVE" {
            return Err(AudexError::WAVError("Expected WAVE format".to_string()));
        }

        // Find existing ID3 chunk
        let id3_chunk = riff_file
            .find_chunk("id3 ")
            .or_else(|| riff_file.find_chunk("ID3 "))
            .cloned();

        // Generate new ID3 data with dynamic padding via PaddingInfo
        let new_id3_data = if let Some(ref tags) = self.tags {
            let minimal_data = self.generate_id3_data(tags, v2_version, v23_sep.clone(), 0)?;
            let needed = minimal_data.len();
            let available = id3_chunk.as_ref().map_or(0, |c| c.data_size as usize);
            let file_size = file.seek(SeekFrom::End(0)).await?;
            let trailing_size = match id3_chunk.as_ref() {
                Some(chunk) => file_size as i64 - chunk.data_offset as i64,
                None => 0,
            };
            let info = PaddingInfo::new(available as i64 - needed as i64, trailing_size);
            let padding = info.get_default_padding().max(0) as usize;
            self.generate_id3_data(tags, v2_version, v23_sep, padding)?
        } else {
            Vec::new()
        };

        if let Some(existing_chunk) = id3_chunk {
            // Update existing ID3 chunk
            let old_size = existing_chunk.data_size;
            let new_size = new_id3_data.len() as u32;

            // Resize chunk if needed
            if old_size != new_size {
                resize_riff_chunk_async(&mut file, &existing_chunk, new_size).await?;

                // Update RIFF file size
                let old_padded = old_size + (old_size % 2);
                let new_padded = new_size + (new_size % 2);
                // Use checked arithmetic to prevent silent wrapping on overflow.
                let size_diff = new_padded as i64 - old_padded as i64;
                let computed = (riff_file.file_size as i64)
                    .checked_add(size_diff)
                    .ok_or_else(|| {
                        AudexError::InvalidData("RIFF file size arithmetic overflow".to_string())
                    })?;
                let new_riff_size = u32::try_from(computed).map_err(|_| {
                    AudexError::InvalidData("RIFF file size does not fit in u32".to_string())
                })?;
                update_riff_file_size_async(&mut file, new_riff_size).await?;
                riff_file.file_size = new_riff_size;
            }

            // Write new ID3 data
            file.seek(SeekFrom::Start(existing_chunk.data_offset))
                .await?;
            file.write_all(&new_id3_data).await?;

            // Write padding byte if needed
            if new_size % 2 == 1 {
                file.write_all(&[0]).await?;
            }
        } else if !new_id3_data.is_empty() {
            // Insert new ID3 chunk before data or at end
            let insert_offset = if let Some(data_chunk) = riff_file.find_chunk("data") {
                data_chunk.offset
            } else {
                file.seek(SeekFrom::End(0)).await?
            };

            let data_size = new_id3_data.len() as u32;
            let padding = if data_size % 2 == 1 { 1 } else { 0 };
            let total_chunk_size = 8 + data_size + padding;

            // Insert space and write chunk
            insert_bytes_async(&mut file, total_chunk_size as u64, insert_offset, None).await?;

            file.seek(SeekFrom::Start(insert_offset)).await?;
            file.write_all(b"id3 ").await?;
            file.write_all(&data_size.to_le_bytes()).await?;
            file.write_all(&new_id3_data).await?;

            if padding > 0 {
                file.write_all(&[0]).await?;
            }

            // Update RIFF file size (checked to prevent silent corruption)
            let new_riff_size = riff_file
                .file_size
                .checked_add(total_chunk_size)
                .ok_or_else(|| {
                    AudexError::InvalidData(
                        "RIFF file size would exceed u32::MAX after inserting ID3 chunk"
                            .to_string(),
                    )
                })?;
            update_riff_file_size_async(&mut file, new_riff_size).await?;
            riff_file.file_size = new_riff_size;

            // Add chunk to structure
            let new_chunk = IffChunkAsync::new("id3 ".to_string(), data_size, insert_offset)?;
            let insert_index = riff_file
                .chunks
                .iter()
                .position(|c| c.offset >= insert_offset)
                .unwrap_or(riff_file.chunks.len());
            riff_file.chunks.insert(insert_index, new_chunk);
        }

        file.flush().await.map_err(AudexError::Io)?;

        // Update internal structure
        self.riff_file = Some(Self::convert_riff_to_riff_file(&riff_file));
        Ok(())
    }

    /// Clear ID3 tags asynchronously
    ///
    /// Removes all ID3 tags from the file by deleting the ID3 chunk.
    pub async fn clear_async(&mut self) -> Result<()> {
        self.tags = None;

        // Remove ID3 chunk from file if present
        let has_id3_chunk = if let Some(ref riff_file) = self.riff_file {
            riff_file
                .chunks
                .iter()
                .any(|chunk| chunk.id == "id3 " || chunk.id == "ID3 ")
        } else {
            false
        };

        if has_id3_chunk {
            if let Some(filename) = self.filename.clone() {
                self.remove_id3_chunk_async(&filename).await?;
            }
        }

        Ok(())
    }

    /// Remove ID3 chunk from file asynchronously using in-place deletion.
    ///
    /// Deletes the ID3 chunk bytes in-place and updates the RIFF header size.
    async fn remove_id3_chunk_async(&mut self, filename: &str) -> Result<()> {
        use tokio::fs::OpenOptions;

        if let Some(ref riff_file) = self.riff_file {
            if let Some(chunk) = riff_file
                .chunks
                .iter()
                .find(|c| c.id == "id3 " || c.id == "ID3 ")
            {
                let pad = if chunk.data_size % 2 == 1 { 1u64 } else { 0 };
                let total_size = 8 + chunk.data_size as u64 + pad;
                let chunk_offset = chunk.offset;
                let old_riff_size = riff_file.file_size;

                // Open file for in-place modification
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(filename)
                    .await?;

                // Remove chunk bytes in-place
                delete_bytes_async(&mut file, total_size, chunk_offset, None).await?;

                // Perform subtraction in u64 space to avoid truncating total_size,
                // then convert back to u32 for the RIFF header
                let new_riff_size = (old_riff_size as u64)
                    .checked_sub(total_size)
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| {
                        AudexError::InvalidData(
                            "ID3 chunk size exceeds RIFF container size".to_string(),
                        )
                    })?;
                file.seek(SeekFrom::Start(4)).await?;
                file.write_all(&new_riff_size.to_le_bytes()).await?;
                file.flush().await?;
            }
        }

        // Update internal structure
        if let Some(ref mut riff_file) = self.riff_file {
            riff_file
                .chunks
                .retain(|chunk| chunk.id != "id3 " && chunk.id != "ID3 ");
        }

        Ok(())
    }

    /// Delete WAV file asynchronously
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        tokio::fs::remove_file(path).await?;
        Ok(())
    }
}

/// Standalone functions for WAVE operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut wave = WAVE::load(path)?;
    wave.clear()
}

/// Clear ID3 tags from WAV file asynchronously
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut wave = WAVE::load_async(path).await?;
    wave.clear_async().await
}

/// Open WAV file asynchronously (alias)
#[cfg(feature = "async")]
pub async fn open_async<P: AsRef<Path>>(path: P) -> Result<WAVE> {
    WAVE::load_async(path).await
}

impl FileType for WAVE {
    type Tags = ID3Tags;
    type Info = WAVEStreamInfo;

    fn format_id() -> &'static str {
        "WAVE"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing WAVE file");
        let mut file = std::fs::File::open(&path)?;
        let mut wave = WAVE::new();
        wave.filename = Some(path.as_ref().to_string_lossy().to_string());

        wave.parse_file(&mut file)?;
        Ok(wave)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing WAVE file from reader");
        let mut wave = Self::new();
        let mut reader = reader;
        wave.parse_file(&mut reader)?;
        Ok(wave)
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving WAVE metadata");
        let filename = self
            .filename
            .clone()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        self.save_to_file(&filename)
    }

    fn clear(&mut self) -> Result<()> {
        self.tags = None;

        // Remove ID3 chunk from RIFF file if present
        let has_id3_chunk = if let Some(ref riff_file) = self.riff_file {
            riff_file
                .chunks
                .iter()
                .any(|chunk| chunk.id == "id3 " || chunk.id == "ID3 ")
        } else {
            false
        };

        if has_id3_chunk {
            if let Some(filename) = self.filename.clone() {
                self.remove_id3_chunk(&filename)?;
            }
        }

        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        self.save_to_writer_impl(writer, 3, Some("/".to_string()))
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        self.tags = None;

        // Re-parse RIFF structure from the writer to get current chunk layout
        writer.seek(SeekFrom::Start(0))?;
        self.riff_file = Some(RiffFile::parse(writer)?);

        // Remove ID3 chunk from RIFF data if present
        let has_id3_chunk = if let Some(ref riff_file) = self.riff_file {
            riff_file
                .chunks
                .iter()
                .any(|chunk| chunk.id == "id3 " || chunk.id == "ID3 ")
        } else {
            false
        };

        if has_id3_chunk {
            self.remove_id3_chunk_writer(writer)?;

            // Zero out any stale trailing bytes left beyond the logical EOF.
            // Non-truncatable writers (Cursor<Vec<u8>>, trait objects) retain
            // old content past the new end after a shrink operation.
            Self::zero_stale_tail(writer)?;

            // Seek back to the logical RIFF end so that Cursor-based writers
            // expose the correct stream position to callers.
            if let Some(ref rf) = self.riff_file {
                let logical_end = rf.file_size as u64 + 8;
                writer.seek(SeekFrom::Start(logical_end))?;
            }
        }

        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        self.save_to_file(path)
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

    fn add_tags(&mut self) -> Result<()> {
        // Check if tags already exist
        if self.tags.is_some() {
            return Err(AudexError::WAVError("ID3 tag already exists".to_string()));
        }

        // Create new ID3 tags with filename propagation
        let mut tags = ID3Tags::new();
        if let Some(ref filename) = self.filename {
            tags.filename = Some(std::path::PathBuf::from(filename));
        }

        self.tags = Some(tags);
        Ok(())
    }

    fn get(&self, key: &str) -> Option<Vec<String>> {
        // ID3Tags has a special get_text_values method that handles the mapping
        self.tags.as_ref()?.get_text_values(key)
    }

    fn score(filename: &str, header: &[u8]) -> i32 {
        let mut score = 0;

        // Check for RIFF + WAVE signature
        if header.len() >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"WAVE" {
            score += 10;
        }

        // Check file extensions
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".wav") {
            score += 3;
        } else if lower_filename.ends_with(".wave") {
            score += 2;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/wav", "audio/wave"]
    }
}
