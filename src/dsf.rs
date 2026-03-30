//! # DSF (DSD Stream File) Format Support
//!
//! This module provides support for reading and writing DSF files, which store
//! DSD (Direct Stream Digital) audio data. DSD uses 1-bit pulse-density
//! modulation at very high sample rates (2.8224 MHz for DSD64, 5.6448 MHz
//! for DSD128, and higher).
//!
//! ## Supported Features
//!
//! - **Stream information**: Sample rate, channels, bit depth, and duration
//!   from the DSD and fmt chunks
//! - **ID3v2 tagging**: Reading and writing ID3v2 tags stored at the end of
//!   the file (location specified by a pointer in the DSD chunk)
//! - **Multiple DSD rates**: DSD64 (2.8 MHz), DSD128 (5.6 MHz), DSD256 (11.2 MHz), etc.
//!
//! ## File Structure
//!
//! A DSF file consists of three mandatory chunks and an optional tag:
//! - **DSD chunk** (28 bytes): File signature, total size, and metadata offset
//! - **fmt chunk**: Audio format details (sample rate, channels, block size)
//! - **data chunk**: Interleaved DSD audio samples
//! - **ID3v2 tag** (optional): Metadata appended after the data chunk

use crate::tags::PaddingInfo;
use crate::{AudexError, FileType, Result, StreamInfo, id3::ID3Tags, tags::Tags};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::id3::{specs, tags::ID3Header};
#[cfg(feature = "async")]
use crate::util::{loadfile_read_async, loadfile_write_async};
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// DSD chunk - first chunk in DSF file (28 bytes)
#[derive(Debug, Default)]
pub struct DSDChunk {
    pub chunk_header: [u8; 4],      // "DSD "
    pub chunk_size: u64,            // Should be 28
    pub total_size: u64,            // Total file size
    pub offset_metadata_chunk: u64, // Pointer to ID3 metadata (0 if none)
}

/// Format chunk - contains audio format info (52 bytes)
#[derive(Debug, Default)]
pub struct FormatChunk {
    pub chunk_header: [u8; 4],       // "fmt "
    pub chunk_size: u64,             // Should be 52
    pub format_version: u32,         // Should be 1
    pub format_id: u32,              // Should be 0 (DSD Raw)
    pub channel_type: u32,           // Channel type
    pub channel_num: u32,            // Number of channels
    pub sampling_frequency: u32,     // Sample rate (2822400, 5644800, etc.)
    pub bits_per_sample: u32,        // Should be 1
    pub sample_count: u64,           // Total number of samples
    pub block_size_per_channel: u32, // Block size (usually 4096)
}

/// Data chunk header (12 bytes minimum)
#[derive(Debug, Default)]
pub struct DataChunk {
    pub chunk_header: [u8; 4], // "data"
    pub chunk_size: u64,       // Size of data chunk
}

/// DSF stream information
#[derive(Debug, Default, Clone)]
pub struct DSFStreamInfo {
    pub length: Option<Duration>,
    pub bitrate: Option<u32>,
    pub channels: u32,
    pub sample_rate: u32,
    pub bits_per_sample: u32,
    pub sample_count: u64,
}

impl StreamInfo for DSFStreamInfo {
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
        match u16::try_from(self.channels) {
            Ok(v) => Some(v),
            // Out-of-range channel count is nonsensical; report as unavailable
            Err(_) => {
                warn_event!(
                    channels = self.channels,
                    "DSF channel count exceeds u16 range"
                );
                None
            }
        }
    }
    fn bits_per_sample(&self) -> Option<u16> {
        match u16::try_from(self.bits_per_sample) {
            Ok(v) => Some(v),
            // Out-of-range bits_per_sample is nonsensical; report as unavailable
            Err(_) => {
                warn_event!(
                    bits_per_sample = self.bits_per_sample,
                    "DSF bits_per_sample exceeds u16 range"
                );
                None
            }
        }
    }
}

impl DSFStreamInfo {
    /// Parse DSF file and extract stream information
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        // Read DSD chunk (28 bytes)
        let mut dsd_data = [0u8; 28];
        reader
            .read_exact(&mut dsd_data)
            .map_err(|_| AudexError::InvalidData("not enough data for DSD chunk".to_string()))?;

        // Parse DSD chunk
        let _dsd_chunk = DSDChunk::from_bytes(&dsd_data)?;

        // Read format chunk (52 bytes)
        let mut fmt_data = [0u8; 52];
        reader
            .read_exact(&mut fmt_data)
            .map_err(|_| AudexError::InvalidData("not enough data for format chunk".to_string()))?;

        // Parse format chunk
        let fmt_chunk = FormatChunk::from_bytes(&fmt_data)?;

        // Build stream info
        let mut info = DSFStreamInfo {
            channels: fmt_chunk.channel_num,
            sample_rate: fmt_chunk.sampling_frequency,
            bits_per_sample: fmt_chunk.bits_per_sample,
            sample_count: fmt_chunk.sample_count,
            ..Default::default()
        };

        // Calculate length
        if info.sample_rate > 0 {
            let secs = info.sample_count as f64 / info.sample_rate as f64;
            info.length = Duration::try_from_secs_f64(secs).ok();
        } else {
            info.length = None;
        }

        // Calculate bitrate
        info.bitrate = Some(
            info.sample_rate
                .saturating_mul(info.bits_per_sample)
                .saturating_mul(info.channels),
        );

        Ok(info)
    }

    /// Pretty print stream info
    pub fn pprint(&self) -> String {
        format!(
            "{} channel DSF @ {} bits, {} Hz, {:.2} seconds",
            self.channels,
            self.bits_per_sample,
            self.sample_rate,
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0)
        )
    }

    pub fn is_lossless(&self) -> bool {
        true
    }
}

impl DSDChunk {
    const CHUNK_SIZE: u64 = 28;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != 28 {
            return Err(AudexError::InvalidData("DSD chunk truncated".to_string()));
        }

        let mut chunk = DSDChunk::default();
        chunk.chunk_header.copy_from_slice(&data[0..4]);

        if &chunk.chunk_header != b"DSD " {
            return Err(AudexError::InvalidData(
                "DSF DSD header not found".to_string(),
            ));
        }

        chunk.chunk_size = u64::from_le_bytes([
            data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11],
        ]);

        if chunk.chunk_size != Self::CHUNK_SIZE {
            return Err(AudexError::InvalidData(
                "DSF DSD header size mismatch".to_string(),
            ));
        }

        chunk.total_size = u64::from_le_bytes([
            data[12], data[13], data[14], data[15], data[16], data[17], data[18], data[19],
        ]);

        chunk.offset_metadata_chunk = u64::from_le_bytes([
            data[20], data[21], data[22], data[23], data[24], data[25], data[26], data[27],
        ]);

        Ok(chunk)
    }
}

impl FormatChunk {
    const CHUNK_SIZE: u64 = 52;
    const VERSION: u32 = 1;
    const FORMAT_DSD_RAW: u32 = 0;

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != 52 {
            return Err(AudexError::InvalidData(
                "Format chunk truncated".to_string(),
            ));
        }

        let mut chunk = FormatChunk::default();
        chunk.chunk_header.copy_from_slice(&data[0..4]);

        if &chunk.chunk_header != b"fmt " {
            return Err(AudexError::InvalidData(
                "DSF fmt header not found".to_string(),
            ));
        }

        chunk.chunk_size = u64::from_le_bytes([
            data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11],
        ]);

        if chunk.chunk_size != Self::CHUNK_SIZE {
            return Err(AudexError::InvalidData(
                "DSF fmt header size mismatch".to_string(),
            ));
        }

        chunk.format_version = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
        if chunk.format_version != Self::VERSION {
            return Err(AudexError::InvalidData(
                "Unsupported format version".to_string(),
            ));
        }

        chunk.format_id = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        if chunk.format_id != Self::FORMAT_DSD_RAW {
            return Err(AudexError::InvalidData("Unsupported format ID".to_string()));
        }

        chunk.channel_type = u32::from_le_bytes([data[20], data[21], data[22], data[23]]);
        chunk.channel_num = u32::from_le_bytes([data[24], data[25], data[26], data[27]]);
        chunk.sampling_frequency = u32::from_le_bytes([data[28], data[29], data[30], data[31]]);
        chunk.bits_per_sample = u32::from_le_bytes([data[32], data[33], data[34], data[35]]);

        chunk.sample_count = u64::from_le_bytes([
            data[36], data[37], data[38], data[39], data[40], data[41], data[42], data[43],
        ]);

        chunk.block_size_per_channel = u32::from_le_bytes([data[44], data[45], data[46], data[47]]);

        Ok(chunk)
    }
}

/// DSF file with ID3v2 tags
#[derive(Debug)]
pub struct DSF {
    pub info: DSFStreamInfo,
    pub tags: Option<ID3Tags>,
    pub filename: Option<String>,
    padding: usize,
}

impl DSF {
    /// Create a new empty DSF instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: DSFStreamInfo::default(),
            tags: None,
            filename: None,
            padding: 0,
        }
    }

    /// Parse DSF file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Parse stream info
        self.info = DSFStreamInfo::from_reader(reader)?;

        // Read DSD chunk to get metadata offset
        reader.seek(SeekFrom::Start(0))?;
        let mut dsd_data = [0u8; 28];
        reader.read_exact(&mut dsd_data)?;
        let dsd_chunk = DSDChunk::from_bytes(&dsd_data)?;

        // Parse ID3 tags from DSF file if metadata chunk exists
        // DSF files store ID3v2 tags at the location specified by offset_metadata_chunk
        if dsd_chunk.offset_metadata_chunk > 0 {
            // Seek to the ID3 metadata location
            reader.seek(SeekFrom::Start(dsd_chunk.offset_metadata_chunk))?;

            // Read the ID3 header first to determine the size
            let mut id3_header_data = [0u8; 10];
            reader.read_exact(&mut id3_header_data)?;

            // Verify this is ID3v2 data (should start with "ID3")
            if &id3_header_data[0..3] == b"ID3" {
                use crate::id3::{specs, tags::ID3Header};

                // Seek back to start of ID3 data
                reader.seek(SeekFrom::Start(dsd_chunk.offset_metadata_chunk))?;

                // Calculate total ID3 tag size from header
                // The size is stored as a synchsafe integer (7 bits per byte)
                let size = ((id3_header_data[6] as u32 & 0x7F) << 21)
                    | ((id3_header_data[7] as u32 & 0x7F) << 14)
                    | ((id3_header_data[8] as u32 & 0x7F) << 7)
                    | (id3_header_data[9] as u32 & 0x7F);

                // Total ID3 data size includes 10-byte header.
                // Use 16 MB as the format-level ceiling (more than enough
                // for any realistic embedded metadata), and also respect
                // the caller-configured ParseLimits if it is tighter.
                const MAX_ID3_SIZE: u64 = 16 * 1024 * 1024; // 16 MB
                let limits = crate::limits::ParseLimits::default();
                let effective_cap = MAX_ID3_SIZE.min(limits.max_tag_size) as usize;
                let total_size = (size as usize).checked_add(10).ok_or_else(|| {
                    AudexError::ParseError(
                        "ID3 tag size overflow: size + header exceeds addressable range"
                            .to_string(),
                    )
                })?;
                if total_size > effective_cap {
                    return Err(AudexError::ParseError(format!(
                        "ID3 tag too large: {} bytes",
                        total_size
                    )));
                }

                // Read the complete ID3 data
                let mut id3_data = vec![0u8; total_size];
                reader.read_exact(&mut id3_data)?;

                // Parse the ID3 header and tags
                match specs::ID3Header::from_bytes(&id3_data) {
                    Ok(specs_header) => {
                        let header = ID3Header::from_specs_header(&specs_header);
                        match ID3Tags::from_data(&id3_data, &header) {
                            Ok(tags) => {
                                self.tags = Some(tags);
                            }
                            Err(_) => {
                                // Failed to parse ID3 tags, leave as None
                                self.tags = None;
                            }
                        }
                    }
                    Err(_) => {
                        // Invalid ID3 header, leave tags as None
                        self.tags = None;
                    }
                }
            } else {
                // Not valid ID3 data at metadata location
                self.tags = None;
            }
        } else {
            // No metadata chunk, tags are None
            self.tags = None;
        }

        Ok(())
    }

    /// Add empty ID3v2 tags
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        self.tags = Some(ID3Tags::new());
        Ok(())
    }

    /// Clear ID3v2 tags
    pub fn clear_tags(&mut self) {
        self.tags = None;
    }

    pub fn mime(&self) -> Vec<&'static str> {
        vec!["audio/dsf"]
    }

    pub fn pprint(&self) -> String {
        self.info.pprint()
    }

    pub fn pprint_no_tags(&self) -> String {
        self.info.pprint()
    }

    pub fn save_no_tags(&mut self) -> Result<()> {
        // For DSF, this is the same as save since tags are optional
        self.save()
    }

    pub fn mime_type(&self) -> &'static str {
        "audio/dsf"
    }

    pub fn set_padding(&mut self, padding: usize) {
        self.padding = padding;
    }

    pub fn get_padding(&self) -> usize {
        self.padding
    }

    /// Save ID3 tags to DSF file with configurable options
    pub fn save_with_options(
        &mut self,
        file_path: Option<&str>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        let v2_version_option = v2_version.unwrap_or(3); // Default to v2.3 for DSF compatibility
        let v23_sep_string = v23_sep.unwrap_or("/").to_string();
        let target_path = match file_path {
            Some(path) => path.to_string(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };
        self.save_to_file_with_options(target_path, v2_version_option, Some(v23_sep_string))
    }

    /// Save ID3 tags to DSF file using direct pointer management.
    ///
    /// Operates directly on the file — only reads the 28-byte DSD header and
    /// writes the ID3 metadata at the end. Does not buffer the entire file.
    pub fn save_to_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.save_to_file_with_options(path, 3, Some("/".to_string()))
    }

    /// Save directly to an on-disk file handle without buffering the entire
    /// file into memory. Only reads the 28-byte DSD header and writes the
    /// ID3 metadata block at the end. Uses `set_len` for truncation.
    fn save_to_file_direct(
        &mut self,
        file: &mut std::fs::File,
        v2_version: u8,
        v23_sep: &str,
    ) -> Result<()> {
        // Read the 28-byte DSD chunk header to locate the metadata pointer
        file.seek(SeekFrom::Start(0))?;
        let mut dsd_data = [0u8; 28];
        file.read_exact(&mut dsd_data)?;
        let dsd_chunk = DSDChunk::from_bytes(&dsd_data)?;

        // Generate new ID3 data with dynamic padding
        let new_id3_data = if self.tags.is_some() {
            let minimal_data = self.generate_id3_data(v2_version, v23_sep, 0)?;
            let needed = minimal_data.len();
            let file_size = file.seek(SeekFrom::End(0))?;
            let (available, trailing_size) = if dsd_chunk.offset_metadata_chunk > 0 {
                let ts_u64 = file_size.saturating_sub(dsd_chunk.offset_metadata_chunk);
                let ts = i64::try_from(ts_u64).map_err(|_| {
                    AudexError::InvalidData(format!(
                        "trailing metadata size {} exceeds i64 range",
                        ts_u64
                    ))
                })?;
                (ts as usize, ts)
            } else {
                (0, 0)
            };
            let info = PaddingInfo::new(available as i64 - needed as i64, trailing_size);
            let padding = info.get_default_padding().max(0) as usize;
            self.generate_id3_data(v2_version, v23_sep, padding)?
        } else {
            Vec::new()
        };

        if self.tags.is_none() || new_id3_data.is_empty() {
            // Delete metadata — truncate at the metadata location
            if dsd_chunk.offset_metadata_chunk > 0 {
                file.set_len(dsd_chunk.offset_metadata_chunk)?;

                // Update DSD header: set total size and clear metadata pointer
                file.seek(SeekFrom::Start(12))?;
                file.write_all(&dsd_chunk.offset_metadata_chunk.to_le_bytes())?;
                file.seek(SeekFrom::Start(20))?;
                file.write_all(&0u64.to_le_bytes())?;
            }
            return Ok(());
        }

        // Determine metadata location (reuse existing or append at EOF)
        let metadata_offset = if dsd_chunk.offset_metadata_chunk > 0 {
            dsd_chunk.offset_metadata_chunk
        } else {
            file.seek(SeekFrom::End(0))?
        };

        // Write ID3 data at metadata location and truncate any trailing bytes
        file.seek(SeekFrom::Start(metadata_offset))?;
        file.write_all(&new_id3_data)?;
        let new_file_size = metadata_offset + new_id3_data.len() as u64;
        file.set_len(new_file_size)?;

        // Update DSD header with new total size and metadata pointer
        file.seek(SeekFrom::Start(12))?;
        file.write_all(&new_file_size.to_le_bytes())?;
        file.seek(SeekFrom::Start(20))?;
        file.write_all(&metadata_offset.to_le_bytes())?;

        Ok(())
    }

    /// Cursor-based save for trait-object writers that cannot call `set_len`.
    /// Used by `save_to_writer` where truncation must go through `Vec::truncate`.
    fn save_to_file_inner(&mut self, file: &mut std::io::Cursor<Vec<u8>>) -> Result<()> {
        // Read current DSD chunk to get metadata pointer
        std::io::Seek::seek(file, SeekFrom::Start(0))?;
        let mut dsd_data = [0u8; 28];
        std::io::Read::read_exact(file, &mut dsd_data)?;
        let dsd_chunk = DSDChunk::from_bytes(&dsd_data)?;

        // Generate new ID3 data if tags exist, using dynamic padding via PaddingInfo
        let new_id3_data = if self.tags.is_some() {
            // Compute the ID3 data size without padding to calculate PaddingInfo
            let minimal_data = self.generate_id3_data(3, "/", 0)?;
            let needed = minimal_data.len();
            let file_size = std::io::Seek::seek(file, SeekFrom::End(0))?;
            // For DSF, metadata is at end of file. trailing_size = data from tag position to EOF.
            let (available, trailing_size) = if dsd_chunk.offset_metadata_chunk > 0 {
                // Use saturating_sub to prevent underflow when offset exceeds file_size
                let raw = file_size.saturating_sub(dsd_chunk.offset_metadata_chunk);
                let ts = i64::try_from(raw).unwrap_or(i64::MAX);
                (ts as usize, ts)
            } else {
                (0, 0)
            };
            let info = PaddingInfo::new(available as i64 - needed as i64, trailing_size);
            let padding = info.get_default_padding().max(0) as usize;
            self.generate_id3_data(3, "/", padding)?
        } else {
            Vec::new()
        };

        if self.tags.is_none() || new_id3_data.is_empty() {
            // Delete metadata - set pointer to 0 and truncate
            if dsd_chunk.offset_metadata_chunk > 0 {
                // Truncate file at the metadata location
                let trunc_pos = usize::try_from(dsd_chunk.offset_metadata_chunk).map_err(|_| {
                    AudexError::InvalidData(format!(
                        "offset_metadata_chunk {} exceeds addressable range",
                        dsd_chunk.offset_metadata_chunk
                    ))
                })?;
                file.get_mut().truncate(trunc_pos);

                // Update DSD header: clear metadata pointer and update total size
                std::io::Seek::seek(file, SeekFrom::Start(12))?; // Seek to total_size field
                std::io::Write::write_all(file, &dsd_chunk.offset_metadata_chunk.to_le_bytes())?; // New total size
                std::io::Seek::seek(file, SeekFrom::Start(20))?; // Seek to offset_metadata_chunk field
                std::io::Write::write_all(file, &0u64.to_le_bytes())?; // Clear metadata pointer
            }
            return Ok(());
        }

        // Determine metadata location
        let metadata_offset = if dsd_chunk.offset_metadata_chunk > 0 {
            // Use existing location
            dsd_chunk.offset_metadata_chunk
        } else {
            // Create new location at end of file
            std::io::Seek::seek(file, SeekFrom::End(0))?
        };

        // Write ID3 data at metadata location
        std::io::Seek::seek(file, SeekFrom::Start(metadata_offset))?;
        std::io::Write::write_all(file, &new_id3_data)?;

        // Truncate everything after the metadata
        let new_file_size = metadata_offset + new_id3_data.len() as u64;
        let trunc_pos = usize::try_from(new_file_size).map_err(|_| {
            AudexError::InvalidData(format!(
                "new file size {} exceeds addressable range",
                new_file_size
            ))
        })?;
        file.get_mut().truncate(trunc_pos);

        // Update DSD header with new total size and metadata pointer
        std::io::Seek::seek(file, SeekFrom::Start(12))?; // Seek to total_size field
        std::io::Write::write_all(file, &new_file_size.to_le_bytes())?; // Update total size
        std::io::Seek::seek(file, SeekFrom::Start(20))?; // Seek to offset_metadata_chunk field
        std::io::Write::write_all(file, &metadata_offset.to_le_bytes())?; // Update metadata pointer

        Ok(())
    }

    /// Generate ID3v2 data for DSF metadata
    fn generate_id3_data(&self, v2_version: u8, v23_sep: &str, padding: usize) -> Result<Vec<u8>> {
        if let Some(ref tags) = self.tags {
            use crate::id3::tags::ID3SaveConfig;
            let config = ID3SaveConfig {
                v2_version,
                v2_minor: 0,
                v23_sep: v23_sep.to_string(),
                v23_separator: v23_sep.chars().next().unwrap_or('/') as u8,
                padding: if padding > 0 { Some(padding) } else { None },
                merge_frames: false,
                preserve_unknown: false,
                compress_frames: false,
                write_v1: crate::id3::file::ID3v1SaveOptions::REMOVE,
                unsync: false,
                extended_header: false,
                convert_v24_frames: false,
            };

            // Get frame data from tags
            let frame_data = tags.write_with_config(&config)?;

            // Create complete ID3v2 tag structure
            let mut complete_id3_data = Vec::new();

            // Write ID3v2 header: "ID3" + version + flags + size
            complete_id3_data.extend_from_slice(b"ID3");
            complete_id3_data.push(config.v2_version); // Major version
            complete_id3_data.push(0); // Minor version (revision)
            complete_id3_data.push(0); // Flags (no unsync, no extended header, etc.)

            // Validate that the frame data fits in a synchsafe integer
            // (4 bytes x 7 usable bits = 28 bits, max 0x0FFFFFFF).
            if frame_data.len() > 0x0FFF_FFFF {
                return Err(AudexError::InvalidData(format!(
                    "ID3v2 frame data size {} exceeds synchsafe maximum 0x0FFFFFFF",
                    frame_data.len()
                )));
            }

            // Write synchsafe size (frame data size)
            let frame_size = frame_data.len() as u32;
            let synchsafe_size = [
                ((frame_size >> 21) & 0x7F) as u8,
                ((frame_size >> 14) & 0x7F) as u8,
                ((frame_size >> 7) & 0x7F) as u8,
                (frame_size & 0x7F) as u8,
            ];
            complete_id3_data.extend_from_slice(&synchsafe_size);

            // Add frame data
            complete_id3_data.extend_from_slice(&frame_data);

            Ok(complete_id3_data)
        } else {
            Ok(Vec::new())
        }
    }

    /// Internal save method with configurable options.
    ///
    /// Operates directly on the file — no full-file buffering.
    fn save_to_file_with_options<P: AsRef<Path>>(
        &mut self,
        path: P,
        v2_version: u8,
        v23_sep: Option<String>,
    ) -> Result<()> {
        let file_path = path.as_ref();

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(file_path)?;

        let separator = v23_sep.unwrap_or_else(|| "/".to_string());
        self.save_to_file_direct(&mut file, v2_version, &separator)?;
        file.flush()?;
        Ok(())
    }

    pub fn len(&self) -> usize {
        match &self.tags {
            Some(tags) => tags.keys().len(),
            None => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Load DSF file asynchronously
    ///
    /// Opens the specified file and parses its DSF structure including DSD chunk,
    /// format chunk, and any ID3v2 metadata tags present at the metadata offset.
    ///
    /// # Arguments
    /// * `path` - Path to the DSF file to load
    ///
    /// # Returns
    /// * `Result<Self>` - The parsed DSF structure or an error
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut dsf = DSF::new();
        dsf.filename = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info and ID3 tags from the file
        dsf.parse_file_async(&mut file).await?;

        Ok(dsf)
    }

    /// Parse DSF file structure asynchronously
    ///
    /// Reads and validates the DSD chunk, format chunk, and extracts ID3v2 tags
    /// from the metadata offset location if present.
    ///
    /// # Arguments
    /// * `file` - Mutable reference to the tokio File handle
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error describing what went wrong
    #[cfg(feature = "async")]
    async fn parse_file_async(&mut self, file: &mut TokioFile) -> Result<()> {
        file.seek(SeekFrom::Start(0)).await?;

        // Read and parse DSD chunk (28 bytes) - contains file signature and metadata pointer
        let mut dsd_chunk = [0u8; 28];
        file.read_exact(&mut dsd_chunk).await?;

        // Verify DSF file signature
        if &dsd_chunk[0..4] != b"DSD " {
            return Err(AudexError::InvalidData("Not a DSF file".to_string()));
        }

        // Validate DSD chunk size (must be exactly 28 bytes per spec)
        let dsd_chunk_size = u64::from_le_bytes([
            dsd_chunk[4],
            dsd_chunk[5],
            dsd_chunk[6],
            dsd_chunk[7],
            dsd_chunk[8],
            dsd_chunk[9],
            dsd_chunk[10],
            dsd_chunk[11],
        ]);
        if dsd_chunk_size != 28 {
            return Err(AudexError::InvalidData(
                "DSF DSD header size mismatch".to_string(),
            ));
        }

        // Extract metadata offset from DSD chunk (bytes 20-27)
        let metadata_offset = u64::from_le_bytes([
            dsd_chunk[20],
            dsd_chunk[21],
            dsd_chunk[22],
            dsd_chunk[23],
            dsd_chunk[24],
            dsd_chunk[25],
            dsd_chunk[26],
            dsd_chunk[27],
        ]);

        // Read and parse format chunk (52 bytes) - contains audio format information
        let mut fmt_chunk = [0u8; 52];
        file.read_exact(&mut fmt_chunk).await?;

        // Verify format chunk signature
        if &fmt_chunk[0..4] != b"fmt " {
            return Err(AudexError::InvalidData(
                "Missing format chunk in DSF".to_string(),
            ));
        }

        // Validate format chunk size (must be exactly 52 bytes per spec)
        let fmt_chunk_size = u64::from_le_bytes([
            fmt_chunk[4],
            fmt_chunk[5],
            fmt_chunk[6],
            fmt_chunk[7],
            fmt_chunk[8],
            fmt_chunk[9],
            fmt_chunk[10],
            fmt_chunk[11],
        ]);
        if fmt_chunk_size != 52 {
            return Err(AudexError::InvalidData(
                "DSF fmt header size mismatch".to_string(),
            ));
        }

        // Validate format version (only version 1 is supported)
        let format_version =
            u32::from_le_bytes([fmt_chunk[12], fmt_chunk[13], fmt_chunk[14], fmt_chunk[15]]);
        if format_version != 1 {
            return Err(AudexError::InvalidData(
                "Unsupported format version".to_string(),
            ));
        }

        // Validate format ID (only DSD Raw = 0 is supported)
        let format_id =
            u32::from_le_bytes([fmt_chunk[16], fmt_chunk[17], fmt_chunk[18], fmt_chunk[19]]);
        if format_id != 0 {
            return Err(AudexError::InvalidData("Unsupported format ID".to_string()));
        }

        // Parse audio format parameters from format chunk
        let channels =
            u32::from_le_bytes([fmt_chunk[24], fmt_chunk[25], fmt_chunk[26], fmt_chunk[27]]);
        let sample_rate =
            u32::from_le_bytes([fmt_chunk[28], fmt_chunk[29], fmt_chunk[30], fmt_chunk[31]]);
        let bits_per_sample =
            u32::from_le_bytes([fmt_chunk[32], fmt_chunk[33], fmt_chunk[34], fmt_chunk[35]]);
        let sample_count = u64::from_le_bytes([
            fmt_chunk[36],
            fmt_chunk[37],
            fmt_chunk[38],
            fmt_chunk[39],
            fmt_chunk[40],
            fmt_chunk[41],
            fmt_chunk[42],
            fmt_chunk[43],
        ]);

        // Calculate audio duration from sample count and rate
        let length = if sample_rate > 0 {
            let secs = sample_count as f64 / sample_rate as f64;
            std::time::Duration::try_from_secs_f64(secs).ok()
        } else {
            None
        };

        // Calculate bitrate (channels * bits_per_sample * sample_rate)
        let bitrate = channels
            .saturating_mul(bits_per_sample)
            .saturating_mul(sample_rate);

        // Populate stream info structure
        self.info = DSFStreamInfo {
            length,
            bitrate: Some(bitrate),
            channels,
            sample_rate,
            bits_per_sample,
            sample_count,
        };

        // Load ID3v2 tags if metadata offset is present
        if metadata_offset > 0 {
            file.seek(SeekFrom::Start(metadata_offset)).await?;

            // Check for ID3 signature ("ID3")
            let mut id3_sig = [0u8; 3];
            if file.read_exact(&mut id3_sig).await.is_ok() && &id3_sig == b"ID3" {
                // Read full ID3 header (10 bytes)
                file.seek(SeekFrom::Start(metadata_offset)).await?;
                let mut id3_header = [0u8; 10];
                file.read_exact(&mut id3_header).await?;

                // Parse synchsafe ID3 size (7 bits per byte)
                let id3_size = ((id3_header[6] as u32 & 0x7F) << 21)
                    | ((id3_header[7] as u32 & 0x7F) << 14)
                    | ((id3_header[8] as u32 & 0x7F) << 7)
                    | (id3_header[9] as u32 & 0x7F);

                // Read complete ID3 data (header + tag body)
                const MAX_ID3_SIZE_ASYNC: u64 = 16 * 1024 * 1024; // 16 MB
                let limits = crate::limits::ParseLimits::default();
                let effective_cap = MAX_ID3_SIZE_ASYNC.min(limits.max_tag_size) as usize;
                let id3_total = 10 + id3_size as usize;
                if id3_total > effective_cap {
                    return Err(AudexError::ParseError(format!(
                        "ID3 tag too large: {} bytes",
                        id3_total
                    )));
                }
                file.seek(SeekFrom::Start(metadata_offset)).await?;
                let mut id3_data = vec![0u8; id3_total];
                file.read_exact(&mut id3_data).await?;

                // Parse ID3 tags using the specs parser
                match specs::ID3Header::from_bytes(&id3_data) {
                    Ok(specs_header) => {
                        let header = ID3Header::from_specs_header(&specs_header);
                        self.tags = ID3Tags::from_data(&id3_data, &header).ok();
                    }
                    Err(_) => self.tags = None,
                }
            }
        }

        Ok(())
    }

    /// Save ID3 tags to DSF file asynchronously
    ///
    /// Writes the current ID3v2 tags to the end of the DSF file and updates
    /// the metadata offset pointer in the DSD chunk header.
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if save fails
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        let filename = self
            .filename
            .clone()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        let mut file = loadfile_write_async(&filename).await?;

        // Read current metadata offset from DSD chunk (bytes 20-27)
        file.seek(SeekFrom::Start(20)).await?;
        let mut offset_bytes = [0u8; 8];
        file.read_exact(&mut offset_bytes).await?;
        let old_metadata_offset = u64::from_le_bytes(offset_bytes);

        // Generate new ID3v2 data from current tags with dynamic padding via PaddingInfo
        let new_id3_data = if self.tags.is_some() {
            let minimal_data = self.generate_id3_data(3, "/", 0)?;
            let needed = minimal_data.len();
            let fsize = file.seek(SeekFrom::End(0)).await?;
            let (available, trailing_size) = if old_metadata_offset > 0 {
                // Use saturating_sub to prevent underflow when offset exceeds file size
                let ts = fsize.saturating_sub(old_metadata_offset) as i64;
                (ts as usize, ts)
            } else {
                (0, 0)
            };
            let info = PaddingInfo::new(available as i64 - needed as i64, trailing_size);
            let padding = info.get_default_padding().max(0) as usize;
            self.generate_id3_data(3, "/", padding)?
        } else {
            Vec::new()
        };

        // Determine new metadata location
        let file_size = file.seek(SeekFrom::End(0)).await?;
        let new_metadata_offset = if new_id3_data.is_empty() {
            0u64 // No metadata
        } else if old_metadata_offset > 0 {
            old_metadata_offset // Reuse existing location
        } else {
            file_size // Append at end of file
        };

        // Write ID3 data or truncate if removing metadata
        if !new_id3_data.is_empty() {
            file.seek(SeekFrom::Start(new_metadata_offset)).await?;
            file.write_all(&new_id3_data).await?;
            file.set_len(new_metadata_offset + new_id3_data.len() as u64)
                .await?;
        } else if old_metadata_offset > 0 {
            // Remove existing metadata by truncating file
            file.set_len(old_metadata_offset).await?;
        }

        // Update metadata offset pointer in DSD chunk
        file.seek(SeekFrom::Start(20)).await?;
        file.write_all(&new_metadata_offset.to_le_bytes()).await?;

        // Update total file size in DSD chunk (bytes 12-19) to stay
        // consistent with the sync path and save_with_options_async
        let new_file_size = if new_id3_data.is_empty() {
            if old_metadata_offset > 0 {
                old_metadata_offset
            } else {
                file_size
            }
        } else {
            new_metadata_offset + new_id3_data.len() as u64
        };
        file.seek(SeekFrom::Start(12)).await?;
        file.write_all(&new_file_size.to_le_bytes()).await?;

        file.flush().await?;
        Ok(())
    }

    /// Save ID3 tags to DSF file asynchronously with configurable options
    ///
    /// Writes the current ID3v2 tags to the end of the DSF file with custom
    /// version and separator settings.
    ///
    /// # Arguments
    /// * `file_path` - Optional path to save to (uses stored filename if None)
    /// * `v2_version` - Optional ID3v2 version (2, 3, or 4), defaults to 3
    /// * `v23_sep` - Optional separator for multi-value fields in v2.3
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if save fails
    #[cfg(feature = "async")]
    pub async fn save_with_options_async(
        &mut self,
        file_path: Option<&str>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        let v2_version_option = v2_version.unwrap_or(3);
        let v23_sep_string = v23_sep.unwrap_or("/").to_string();
        let target_path = match file_path {
            Some(path) => path.to_string(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };

        let mut file = loadfile_write_async(&target_path).await?;

        // Read current metadata offset from DSD chunk (bytes 20-27)
        file.seek(SeekFrom::Start(20)).await?;
        let mut offset_bytes = [0u8; 8];
        file.read_exact(&mut offset_bytes).await?;
        let old_metadata_offset = u64::from_le_bytes(offset_bytes);

        // Generate new ID3v2 data from current tags with dynamic padding via PaddingInfo
        let new_id3_data = if self.tags.is_some() {
            let minimal_data = self.generate_id3_data(v2_version_option, &v23_sep_string, 0)?;
            let needed = minimal_data.len();
            let fsize = file.seek(SeekFrom::End(0)).await?;
            let (available, trailing_size) = if old_metadata_offset > 0 {
                // Use saturating_sub to prevent underflow when offset exceeds file size
                let ts = fsize.saturating_sub(old_metadata_offset) as i64;
                (ts as usize, ts)
            } else {
                (0, 0)
            };
            let info = PaddingInfo::new(available as i64 - needed as i64, trailing_size);
            let padding = info.get_default_padding().max(0) as usize;
            self.generate_id3_data(v2_version_option, &v23_sep_string, padding)?
        } else {
            Vec::new()
        };

        // Determine new metadata location
        let file_size = file.seek(SeekFrom::End(0)).await?;
        let new_metadata_offset = if new_id3_data.is_empty() {
            0u64 // No metadata
        } else if old_metadata_offset > 0 {
            old_metadata_offset // Reuse existing location
        } else {
            file_size // Append at end of file
        };

        // Write ID3 data or truncate if removing metadata
        if !new_id3_data.is_empty() {
            file.seek(SeekFrom::Start(new_metadata_offset)).await?;
            file.write_all(&new_id3_data).await?;
            file.set_len(new_metadata_offset + new_id3_data.len() as u64)
                .await?;
        } else if old_metadata_offset > 0 {
            // Remove existing metadata by truncating file
            file.set_len(old_metadata_offset).await?;
        }

        // Update metadata offset pointer in DSD chunk
        file.seek(SeekFrom::Start(20)).await?;
        file.write_all(&new_metadata_offset.to_le_bytes()).await?;

        // Also update total file size in DSD chunk (bytes 12-19)
        let new_file_size = if new_id3_data.is_empty() {
            if old_metadata_offset > 0 {
                old_metadata_offset
            } else {
                file_size
            }
        } else {
            new_metadata_offset + new_id3_data.len() as u64
        };
        file.seek(SeekFrom::Start(12)).await?;
        file.write_all(&new_file_size.to_le_bytes()).await?;

        file.flush().await?;
        Ok(())
    }

    /// Clear ID3 tags and save the file asynchronously
    ///
    /// Removes all ID3v2 tags from the DSF file and updates the file
    /// to reflect the removal.
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if operation fails
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        self.tags = None;
        self.save_async().await
    }

    /// Delete the DSF file asynchronously
    ///
    /// Permanently removes the DSF file from the filesystem.
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if deletion fails
    #[cfg(feature = "async")]
    pub async fn delete_async(&mut self) -> Result<()> {
        if let Some(ref filename) = self.filename {
            tokio::fs::remove_file(filename).await?;
            self.filename = None;
            Ok(())
        } else {
            Err(AudexError::InvalidData("No filename set".to_string()))
        }
    }
}

impl Default for DSF {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for DSF {
    type Tags = ID3Tags;
    type Info = DSFStreamInfo;

    fn format_id() -> &'static str {
        "DSF"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing DSF file");
        let mut file = std::fs::File::open(&path)?;
        let mut dsf = DSF::new();
        dsf.filename = Some(path.as_ref().to_string_lossy().to_string());

        dsf.parse_file(&mut file)?;
        Ok(dsf)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing DSF file from reader");
        let mut instance = Self::new();
        let mut reader = reader;
        instance.parse_file(&mut reader)?;
        Ok(instance)
    }

    fn save(&mut self) -> Result<()> {
        let filename = self
            .filename
            .clone()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        self.save_to_file(&filename)
    }

    fn clear(&mut self) -> Result<()> {
        self.clear_tags();
        if self.filename.is_some() {
            self.save()
        } else {
            Ok(())
        }
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        // Read the entire content into memory so we can operate on a Cursor,
        // which supports truncation needed by the DSF save logic.
        let data = crate::util::read_all_from_writer_limited(writer, "in-memory DSF save")?;
        let mut cursor = std::io::Cursor::new(data);

        self.save_to_file_inner(&mut cursor)?;

        // Write the modified data back to the original writer
        let result = cursor.into_inner();
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&result)?;

        // Zero or truncate stale trailing bytes using the shared chunked helper.
        crate::util::truncate_writer_dyn(writer, result.len() as u64)?;

        writer.flush()?;
        Ok(())
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        self.clear_tags();
        self.save_to_writer(writer)
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
            return Err(AudexError::InvalidData(
                "ID3 tag already exists".to_string(),
            ));
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

        // Check for DSF signature
        if header.len() >= 4 && header.starts_with(b"DSD ") {
            score += 2;
        }

        // Check file extension
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".dsf") {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/dsf"]
    }
}

/// Standalone functions for DSF operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut dsf = DSF::load(path)?;
    dsf.clear()
}
