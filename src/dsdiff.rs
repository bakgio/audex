//! DSDIFF (Direct Stream Digital Interchange File Format) format support
//!
//! DSDIFF is a file format for storing DSD (Direct Stream Digital) audio data.
//! It uses the IFF (Interchange File Format) structure with big-endian byte order.
//! The format supports both uncompressed DSD and compressed DST data.
//! Metadata is stored using ID3v2 tags.
//!
//! File structure:
//! - FRM8 chunk (root container)
//! - FVER chunk (format version)
//! - PROP chunk (properties container, form type "SND ")
//!   - FS chunk (sample rate)
//!   - CHNL chunk (channel configuration)
//!   - CMPR chunk (compression type)
//! - DSD/DST chunk (audio data)

use crate::tags::PaddingInfo;
use crate::util::resize_bytes;
use crate::{AudexError, FileType, Result, StreamInfo, id3::ID3Tags};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::util::{loadfile_read_async, loadfile_write_async, resize_bytes_async};
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// IFF chunk header (12 bytes: 4-byte ID + 8-byte size)
#[derive(Debug, Default, Clone)]
pub struct IffChunk {
    pub id: [u8; 4],
    pub size: u64,
    pub data_offset: u64,
}

/// DSDIFF stream information
#[derive(Debug, Default)]
pub struct DSDIFFStreamInfo {
    pub length: Option<Duration>,
    pub bitrate: Option<u32>,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub compression: String,
}

impl StreamInfo for DSDIFFStreamInfo {
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
}

impl DSDIFFStreamInfo {
    /// Parse DSDIFF file and extract stream information
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        // Read and validate FRM8 header
        let frm8_chunk = IffChunk::from_reader(reader)?;
        if &frm8_chunk.id != b"FRM8" {
            return Err(AudexError::InvalidData(
                "Not a DSDIFF file (missing FRM8 header)".to_string(),
            ));
        }

        // Read DSD form type (4 bytes after FRM8 header)
        let mut form_type = [0u8; 4];
        reader
            .read_exact(&mut form_type)
            .map_err(|_| AudexError::InvalidData("Cannot read DSDIFF form type".to_string()))?;

        if &form_type != b"DSD " {
            return Err(AudexError::InvalidData(
                "Invalid DSDIFF form type".to_string(),
            ));
        }

        let mut info = DSDIFFStreamInfo {
            bits_per_sample: 1,             // DSD is always 1 bit per sample
            compression: "DSD".to_string(), // Default to DSD
            ..Default::default()
        };

        // Skip FVER chunk if present
        Self::skip_chunk_if_present(reader, b"FVER")?;

        // Find and parse PROP chunk
        let prop_chunk = Self::find_chunk(reader, b"PROP")?;
        if prop_chunk.size < 4 {
            return Err(AudexError::InvalidData("PROP chunk too small".to_string()));
        }
        // Clamp prop_end to actual file size to prevent loops on malformed sizes
        let current_pos = reader.stream_position()?;
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(current_pos))?;
        let prop_end = current_pos
            .saturating_add(prop_chunk.size.saturating_sub(4))
            .min(file_size); // -4 for form type, clamped to file size

        // Read PROP form type
        let mut prop_form_type = [0u8; 4];
        reader
            .read_exact(&mut prop_form_type)
            .map_err(|_| AudexError::InvalidData("Cannot read PROP form type".to_string()))?;

        if &prop_form_type != b"SND " {
            return Err(AudexError::InvalidData(
                "Expected SND form type in PROP chunk".to_string(),
            ));
        }

        // Parse subchunks within PROP
        while reader.stream_position()? < prop_end {
            let chunk = IffChunk::from_reader(reader)?;

            // Guard against zero-size chunks to prevent infinite looping.
            // A zero-size skip does not advance the reader, causing the
            // loop to re-parse the same chunk header indefinitely.
            if chunk.size == 0 {
                break;
            }

            match &chunk.id {
                b"FS  " => {
                    // Sample rate chunk (4 bytes, big-endian)
                    if chunk.size != 4 {
                        return Err(AudexError::InvalidData("Invalid FS chunk size".to_string()));
                    }
                    let mut sample_rate_bytes = [0u8; 4];
                    reader.read_exact(&mut sample_rate_bytes)?;
                    info.sample_rate = u32::from_be_bytes(sample_rate_bytes);
                }
                b"CHNL" => {
                    // Channel configuration chunk (at least 2 bytes)
                    if chunk.size < 2 {
                        return Err(AudexError::InvalidData(
                            "Invalid CHNL chunk size".to_string(),
                        ));
                    }
                    let mut channels_bytes = [0u8; 2];
                    reader.read_exact(&mut channels_bytes)?;
                    info.channels = u16::from_be_bytes(channels_bytes);
                    // Skip remaining CHNL data using absolute seek to avoid
                    // i64 overflow on sizes > i64::MAX
                    if chunk.size > 2 {
                        let skip_to = reader
                            .stream_position()?
                            .checked_add(chunk.size - 2)
                            .ok_or_else(|| {
                                AudexError::InvalidData(
                                    "Seek target overflow in DSDIFF chunk skip".to_string(),
                                )
                            })?;
                        reader.seek(SeekFrom::Start(skip_to))?;
                    }
                }
                b"CMPR" => {
                    // Compression type chunk (at least 4 bytes)
                    if chunk.size < 4 {
                        return Err(AudexError::InvalidData(
                            "Invalid CMPR chunk size".to_string(),
                        ));
                    }
                    let mut compression_bytes = [0u8; 4];
                    reader.read_exact(&mut compression_bytes)?;
                    info.compression = String::from_utf8_lossy(&compression_bytes)
                        .trim_end_matches('\0')
                        .trim_end()
                        .to_string();
                    // Skip remaining CMPR data using absolute seek to avoid
                    // i64 overflow on sizes > i64::MAX
                    if chunk.size > 4 {
                        let skip_to = reader
                            .stream_position()?
                            .checked_add(chunk.size - 4)
                            .ok_or_else(|| {
                                AudexError::InvalidData(
                                    "Seek target overflow in DSDIFF chunk skip".to_string(),
                                )
                            })?;
                        reader.seek(SeekFrom::Start(skip_to))?;
                    }
                }
                _ => {
                    // Skip unknown chunk using absolute seek to avoid
                    // i64 overflow on sizes > i64::MAX
                    let skip_to = reader
                        .stream_position()?
                        .checked_add(chunk.size)
                        .ok_or_else(|| {
                            AudexError::InvalidData(
                                "Seek target overflow in DSDIFF chunk skip".to_string(),
                            )
                        })?;
                    reader.seek(SeekFrom::Start(skip_to))?;
                }
            }

            // Align to even boundary
            if chunk.size % 2 == 1 {
                reader.seek(SeekFrom::Current(1))?;
            }
        }

        // Calculate length and bitrate based on compression type
        if info.compression == "DSD" {
            // Find DSD data chunk
            reader.seek(SeekFrom::Start(frm8_chunk.data_offset + 4))?; // Start after form type
            if let Ok(dsd_chunk) = Self::find_chunk(reader, b"DSD ") {
                // Reject absurdly large DSD chunks that would produce
                // meaningless duration values (cap at 1 TB of raw data)
                const MAX_DSD_CHUNK_SIZE: u64 = 1_099_511_627_776; // 1 TB
                if dsd_chunk.size > MAX_DSD_CHUNK_SIZE {
                    return Err(AudexError::ParseError(format!(
                        "DSD data chunk size {} exceeds maximum ({})",
                        dsd_chunk.size, MAX_DSD_CHUNK_SIZE
                    )));
                }

                // DSD data has one bit per sample, 8 samples per byte
                let sample_count =
                    dsd_chunk.size.saturating_mul(8) as f64 / info.channels.max(1) as f64;

                if info.sample_rate > 0 {
                    info.length =
                        Duration::try_from_secs_f64(sample_count / info.sample_rate as f64).ok();
                }

                info.bitrate = Some(
                    (info.channels as u32)
                        .saturating_mul(info.bits_per_sample as u32)
                        .saturating_mul(info.sample_rate),
                );
            }
        } else if info.compression == "DST" {
            // Find DST frame chunk
            reader.seek(SeekFrom::Start(frm8_chunk.data_offset + 4))?; // Start after form type
            if let Ok(dst_chunk) = Self::find_chunk(reader, b"DST ") {
                // Look for FRTE chunk within DST
                let dst_end = reader.stream_position()?.saturating_add(dst_chunk.size);
                if let Ok(frte_chunk) = Self::find_chunk_until(reader, b"FRTE", dst_end) {
                    if frte_chunk.size >= 6 {
                        let mut frte_data = [0u8; 6];
                        reader.read_exact(&mut frte_data)?;
                        let frame_count = u32::from_be_bytes([
                            frte_data[0],
                            frte_data[1],
                            frte_data[2],
                            frte_data[3],
                        ]);
                        let frame_rate = u16::from_be_bytes([frte_data[4], frte_data[5]]);

                        if frame_rate > 0 {
                            info.length =
                                Duration::try_from_secs_f64(frame_count as f64 / frame_rate as f64)
                                    .ok();
                        }

                        if frame_count > 0 {
                            // Subtract FRTE chunk total size: 12-byte IFF header + data + padding.
                            // Use saturating arithmetic to prevent overflow on crafted chunk sizes.
                            let frte_total = 12u64
                                .saturating_add(frte_chunk.size)
                                .saturating_add(frte_chunk.size % 2);
                            let dst_data_size = dst_chunk.size.saturating_sub(frte_total);
                            let avg_frame_size = dst_data_size as f64 / frame_count as f64;
                            // Clamp before casting to avoid truncation on extreme values
                            let bitrate_raw = (avg_frame_size * 8.0 * frame_rate as f64).round();
                            info.bitrate = Some(if bitrate_raw > u32::MAX as f64 {
                                u32::MAX
                            } else {
                                bitrate_raw as u32
                            });
                        }
                    }
                }
            } else {
                // DST compression but no DST chunk found
                info.bitrate = Some(0);
                info.length = Duration::try_from_secs_f64(0.0).ok();
            }
        }

        // Ensure bitrate is always set
        if info.bitrate.is_none() {
            info.bitrate = Some(0);
        }

        Ok(info)
    }

    /// Find a chunk with the given ID
    fn find_chunk<R: Read + Seek>(reader: &mut R, target_id: &[u8; 4]) -> Result<IffChunk> {
        let saved_pos = reader.stream_position()?;
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(saved_pos))?;
        Self::find_chunk_until(reader, target_id, file_size)
    }

    /// Find a chunk with the given ID before the end position.
    /// Limits scanning to MAX_CHUNK_SCAN iterations to prevent
    /// excessive I/O on files with many tiny or malformed chunks.
    fn find_chunk_until<R: Read + Seek>(
        reader: &mut R,
        target_id: &[u8; 4],
        end_pos: u64,
    ) -> Result<IffChunk> {
        const MAX_CHUNK_SCAN: usize = 10_000;
        let mut iterations = 0;
        while reader.stream_position()? < end_pos {
            iterations += 1;
            if iterations > MAX_CHUNK_SCAN {
                return Err(AudexError::InvalidData(format!(
                    "Exceeded {} chunk scan iterations looking for {}",
                    MAX_CHUNK_SCAN,
                    String::from_utf8_lossy(target_id)
                )));
            }
            let chunk = IffChunk::from_reader(reader)?;
            if &chunk.id == target_id {
                return Ok(chunk);
            }

            // A zero-size chunk has no data to skip, and continuing would
            // just read the next header — causing slow scanning if the file
            // is filled with zero-size chunks. Break immediately instead.
            if chunk.size == 0 {
                break;
            }

            // Guard against u64 overflow when computing the seek target
            let skip_to = reader
                .stream_position()?
                .checked_add(chunk.size)
                .ok_or_else(|| AudexError::InvalidData("chunk size overflow".to_string()))?;
            reader.seek(SeekFrom::Start(skip_to))?;
            if reader.stream_position()? > end_pos {
                break;
            }

            // Align to even boundary
            if chunk.size % 2 == 1 {
                reader.seek(SeekFrom::Current(1))?;
            }
        }

        Err(AudexError::InvalidData(format!(
            "Chunk {} not found",
            String::from_utf8_lossy(target_id)
        )))
    }

    /// Public wrapper for find_chunk_until, exposed for testing only
    #[doc(hidden)]
    pub fn find_chunk_until_public<R: Read + Seek>(
        reader: &mut R,
        target_id: &[u8; 4],
        end_pos: u64,
    ) -> Result<IffChunk> {
        Self::find_chunk_until(reader, target_id, end_pos)
    }

    /// Skip a chunk if it's present at current position
    fn skip_chunk_if_present<R: Read + Seek>(reader: &mut R, chunk_id: &[u8; 4]) -> Result<()> {
        let pos = reader.stream_position()?;

        match IffChunk::from_reader(reader) {
            Ok(chunk) if &chunk.id == chunk_id => {
                // Guard against u64 overflow when computing the seek target
                let skip_to = reader
                    .stream_position()?
                    .checked_add(chunk.size)
                    .ok_or_else(|| AudexError::InvalidData("chunk size overflow".to_string()))?;
                reader.seek(SeekFrom::Start(skip_to))?;
                if chunk.size % 2 == 1 {
                    reader.seek(SeekFrom::Current(1))?;
                }
            }
            _ => {
                // Not the expected chunk, rewind
                reader.seek(SeekFrom::Start(pos))?;
            }
        }

        Ok(())
    }

    /// Pretty print stream info
    pub fn pprint(&self) -> String {
        format!(
            "{} channel DSDIFF ({}) @ {} bps, {} Hz, {:.2} seconds",
            self.channels,
            self.compression,
            self.bitrate.unwrap_or(0),
            self.sample_rate,
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0)
        )
    }
}

impl IffChunk {
    /// Read an IFF chunk header from a reader
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut header = [0u8; 12]; // 4 bytes ID + 8 bytes size
        reader
            .read_exact(&mut header)
            .map_err(|_| AudexError::InvalidData("Cannot read IFF chunk header".to_string()))?;

        let size = u64::from_be_bytes([
            header[4], header[5], header[6], header[7], header[8], header[9], header[10],
            header[11],
        ]);

        // Reject chunk sizes that don't fit in i64 (would wrap to negative in seeks)
        if size > i64::MAX as u64 {
            return Err(AudexError::InvalidData(format!(
                "IFF chunk size too large: {}",
                size
            )));
        }

        let chunk = IffChunk {
            id: [header[0], header[1], header[2], header[3]],
            size,
            data_offset: reader.stream_position()?,
        };

        Ok(chunk)
    }

    /// Read chunk data
    pub fn read_data<R: Read + Seek>(&self, reader: &mut R) -> Result<Vec<u8>> {
        // Enforce the library-wide tag allocation ceiling
        crate::limits::ParseLimits::default().check_tag_size(self.size, "DSDIFF chunk")?;
        // Guard against silent truncation on 32-bit targets where usize is 32 bits.
        let alloc_size = usize::try_from(self.size).map_err(|_| {
            AudexError::InvalidData(format!(
                "DSDIFF chunk size {} exceeds addressable memory on this platform",
                self.size
            ))
        })?;
        reader.seek(SeekFrom::Start(self.data_offset))?;
        let mut data = vec![0u8; alloc_size];
        reader.read_exact(&mut data)?;
        Ok(data)
    }

    /// Write data to chunk location
    pub fn write_data<W: std::io::Write + Seek>(&self, writer: &mut W, data: &[u8]) -> Result<()> {
        writer.seek(SeekFrom::Start(self.data_offset))?;
        writer.write_all(data)?;
        Ok(())
    }

    pub fn id_string(&self) -> String {
        String::from_utf8_lossy(&self.id).into_owned()
    }
}

/// DSDIFF file parser
#[derive(Debug, Clone)]
pub struct DSDIFFFile {
    pub file_type: String,
    pub chunks: Vec<IffChunk>,
    pub file_size: u64,
}

impl DSDIFFFile {
    /// Parse DSDIFF file structure
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        reader.seek(SeekFrom::Start(0))?;

        // Read FRM8 header
        let frm8_chunk = IffChunk::from_reader(reader)?;
        if &frm8_chunk.id != b"FRM8" {
            return Err(AudexError::InvalidData(
                "Expected FRM8 signature".to_string(),
            ));
        }

        // Read DSD form type (4 bytes after FRM8 header)
        let mut form_type = [0u8; 4];
        reader.read_exact(&mut form_type)?;
        let file_type = String::from_utf8_lossy(&form_type).into_owned();

        if file_type != "DSD " {
            return Err(AudexError::InvalidData("Expected DSD format".to_string()));
        }

        let mut chunks = Vec::new();
        // The content ends at: FRM8 header start (0) + 12 bytes header + FRM8 size
        // Clamp to actual file size to prevent loops on malformed sizes
        let saved_pos = reader.stream_position()?;
        let actual_file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(saved_pos))?;
        let file_end = (12u64.saturating_add(frm8_chunk.size)).min(actual_file_size);

        // Parse chunks
        while reader.stream_position()? < file_end {
            let chunk = IffChunk::from_reader(reader)?;

            // Break on zero-size chunks to prevent unbounded accumulation.
            // A zero-size chunk advances the reader by only 12 bytes (the header),
            // so a large file of NUL bytes would create millions of chunk objects.
            if chunk.size == 0 {
                break;
            }

            // Guard against u64 overflow when computing the seek target
            let skip_to = reader
                .stream_position()?
                .checked_add(chunk.size)
                .ok_or_else(|| AudexError::InvalidData("chunk size overflow".to_string()))?;
            reader.seek(SeekFrom::Start(skip_to))?;

            // Align to even boundary
            if chunk.size % 2 == 1 {
                reader.seek(SeekFrom::Current(1))?;
            }

            chunks.push(chunk);
        }

        Ok(DSDIFFFile {
            file_type,
            chunks,
            file_size: frm8_chunk.size.saturating_add(12), // Include FRM8 header (12 bytes)
        })
    }

    /// Find chunk by ID
    pub fn find_chunk(&self, id: &[u8; 4]) -> Option<&IffChunk> {
        self.chunks.iter().find(|chunk| &chunk.id == id)
    }

    pub fn has_chunk(&self, id: &[u8; 4]) -> bool {
        self.find_chunk(id).is_some()
    }
}

/// DSDIFF (DSD Interchange File Format) audio file with ID3v2 tagging.
///
/// Provides read/write access to DSD audio metadata stored in DSDIFF (.dff) files.
/// Tags are stored as ID3v2 frames (e.g., `TIT2` for title, `TPE1` for artist).
///
/// # Fields
///
/// - `info` — Stream properties (sample rate, channels, duration, compression type)
/// - `tags` — Optional ID3v2 tags; use [`add_tags()`](Self::add_tags) to create if absent
/// - `filename` — Path of the loaded file, if loaded from disk
///
/// # Examples
///
/// ```no_run
/// use audex::dsdiff::DSDIFF;
/// use audex::{FileType, Tags};
///
/// let mut dff = DSDIFF::load("music.dff")?;
///
/// // Read stream info
/// println!("Sample rate: {} Hz", dff.info.sample_rate);
/// println!("Channels: {}", dff.info.channels);
///
/// // Add tags if absent, then write metadata
/// if dff.tags.is_none() {
///     dff.add_tags()?;
/// }
/// if let Some(ref mut tags) = dff.tags {
///     tags.set("TIT2", vec!["My Song".to_string()]);
/// }
/// dff.save()?;
/// # Ok::<(), audex::AudexError>(())
/// ```
#[derive(Debug)]
pub struct DSDIFF {
    /// DSD stream properties (sample rate, channels, duration, compression)
    pub info: DSDIFFStreamInfo,
    /// Optional ID3v2 tags — `None` if the file has no metadata chunk
    pub tags: Option<ID3Tags>,
    /// Path of the file on disk, if loaded from a file
    pub filename: Option<String>,
    dsdiff_file: Option<DSDIFFFile>,
}

impl DSDIFF {
    /// Create a new empty DSDIFF instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: DSDIFFStreamInfo::default(),
            tags: None,
            filename: None,
            dsdiff_file: None,
        }
    }

    /// Parse DSDIFF file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Parse stream info
        self.info = DSDIFFStreamInfo::from_reader(reader)?;

        // Parse the DSDIFF file structure
        reader.seek(SeekFrom::Start(0))?;
        let dsdiff_file = DSDIFFFile::parse(reader)?;

        // Parse ID3 tags from DSDIFF file (ID3v2 tags stored in "ID3 " chunk)
        if let Some(id3_chunk) = dsdiff_file.find_chunk(b"ID3 ") {
            const MAX_ID3_CHUNK: u64 = 16 * 1024 * 1024; // 16 MB
            let limits = crate::limits::ParseLimits::default();
            let effective_cap = MAX_ID3_CHUNK.min(limits.max_tag_size);
            if id3_chunk.size > effective_cap {
                return Err(AudexError::ParseError(format!(
                    "DSDIFF ID3 chunk too large: {} bytes",
                    id3_chunk.size
                )));
            }
            let id3_data = id3_chunk.read_data(reader)?;

            // Verify this is ID3v2 data (should start with "ID3")
            if id3_data.len() >= 10 {
                use crate::id3::{specs, tags::ID3Header};
                match specs::ID3Header::from_bytes(&id3_data) {
                    Ok(specs_header) => {
                        let header = ID3Header::from_specs_header(&specs_header);
                        match ID3Tags::from_data(&id3_data, &header) {
                            Ok(tags) => {
                                self.tags = Some(tags);
                            }
                            Err(_) => {
                                self.tags = None;
                            }
                        }
                    }
                    Err(_) => {
                        self.tags = None;
                    }
                }
            } else {
                self.tags = None;
            }
        } else {
            self.tags = None;
        }

        self.dsdiff_file = Some(dsdiff_file);
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
    pub fn clear(&mut self) -> Result<()> {
        self.tags = None;
        if self.filename.is_some() {
            self.save()
        } else {
            Ok(())
        }
    }

    pub fn mime(&self) -> Vec<&'static str> {
        vec!["audio/x-dff"]
    }

    /// Pretty print file info
    pub fn pprint(&self) -> String {
        self.info.pprint()
    }

    /// Save ID3 tags to DSDIFF file with configurable options
    pub fn save_with_options(
        &mut self,
        file_path: Option<&str>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        let v2_version_option = v2_version.unwrap_or(3); // Default to v2.3 for DSDIFF compatibility
        let v23_sep_string = v23_sep.unwrap_or("/").to_string();
        let target_path = match file_path {
            Some(path) => path.to_string(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };
        self.save_to_file_with_options(target_path, v2_version_option, Some(v23_sep_string))
    }

    /// Save ID3 tags to DSDIFF file by modifying the ID3 chunk.
    ///
    /// Operates directly on the file handle — does not buffer the entire
    /// file into memory. Uses chunked `resize_bytes` for IFF manipulation.
    pub fn save_to_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        self.save_to_file_with_options(path, 3, Some("/".to_string()))
    }

    /// Save directly to an on-disk file handle without buffering the entire
    /// file into memory. Parses the IFF structure, locates or creates an ID3
    /// chunk, and writes the current tags using chunked `resize_bytes` which
    /// streams data in fixed-size blocks.
    fn save_to_file_direct(
        &mut self,
        file: &mut std::fs::File,
        v2_version: u8,
        v23_sep: &str,
    ) -> Result<()> {
        // Parse current file structure
        let dsdiff_file = DSDIFFFile::parse(file)?;
        self.dsdiff_file = Some(dsdiff_file.clone());

        // Find existing ID3 chunk
        let existing_id3_chunk = dsdiff_file.find_chunk(b"ID3 ");

        if self.tags.is_none() {
            // Delete existing ID3 chunk if present
            if let Some(id3_chunk) = existing_id3_chunk {
                let old_chunk_size = 12 + id3_chunk.size + (id3_chunk.size % 2);
                let chunk_offset = id3_chunk.data_offset.saturating_sub(12);
                resize_bytes(file, old_chunk_size, 0, chunk_offset)?;

                // Update FRM8 size header
                let new_frm8_size = dsdiff_file
                    .file_size
                    .saturating_sub(12)
                    .saturating_sub(old_chunk_size);
                file.seek(SeekFrom::Start(4))?;
                file.write_all(&new_frm8_size.to_be_bytes())?;
            }
            return Ok(());
        }

        // Generate new ID3 data with dynamic padding
        let new_id3_data = {
            let minimal_data = self.generate_id3_data(v2_version, v23_sep, 0)?;
            let needed = minimal_data.len();
            let available = existing_id3_chunk.as_ref().map_or(0, |c| c.size as usize);
            let file_size = file.seek(SeekFrom::End(0))?;
            let trailing_size = match existing_id3_chunk.as_ref() {
                Some(chunk) => {
                    let fs = i64::try_from(file_size).map_err(|_| {
                        AudexError::InvalidData("File size exceeds i64 range".to_string())
                    })?;
                    let offset = i64::try_from(chunk.data_offset).map_err(|_| {
                        AudexError::InvalidData("Chunk data offset exceeds i64 range".to_string())
                    })?;
                    fs - offset
                }
                None => 0,
            };
            let info = PaddingInfo::new(available as i64 - needed as i64, trailing_size);
            let padding = info.get_default_padding().max(0) as usize;
            self.generate_id3_data(v2_version, v23_sep, padding)?
        };
        let new_id3_size = new_id3_data.len() as u64;
        let new_chunk_size = 12 + new_id3_size + (new_id3_size % 2);

        if let Some(id3_chunk) = existing_id3_chunk {
            // Modify existing ID3 chunk
            let old_chunk_size = 12 + id3_chunk.size + (id3_chunk.size % 2);
            let chunk_offset = id3_chunk.data_offset.saturating_sub(12);

            resize_bytes(file, old_chunk_size, new_chunk_size, chunk_offset)?;

            // Write new chunk header + data
            file.seek(SeekFrom::Start(chunk_offset))?;
            file.write_all(b"ID3 ")?;
            file.write_all(&new_id3_size.to_be_bytes())?;
            file.write_all(&new_id3_data)?;
            if new_id3_size % 2 == 1 {
                file.write_all(&[0])?;
            }

            // Update FRM8 size header
            let new_chunk_i64 = i64::try_from(new_chunk_size).map_err(|_| {
                AudexError::InvalidData("New chunk size exceeds i64 range".to_string())
            })?;
            let old_chunk_i64 = i64::try_from(old_chunk_size).map_err(|_| {
                AudexError::InvalidData("Old chunk size exceeds i64 range".to_string())
            })?;
            let size_diff = new_chunk_i64 - old_chunk_i64;
            let frm8_base =
                i64::try_from(dsdiff_file.file_size.saturating_sub(12)).map_err(|_| {
                    AudexError::InvalidData("FRM8 base size exceeds i64 range".to_string())
                })?;
            let new_frm8_size = frm8_base + size_diff;
            // Reject negative results -- a negative FRM8 size indicates
            // a corrupted old chunk size or an impossible shrink.
            let new_frm8_size = u64::try_from(new_frm8_size).map_err(|_| {
                AudexError::InvalidData(format!(
                    "Computed FRM8 size is negative ({}), file structure may be corrupt",
                    new_frm8_size
                ))
            })?;
            file.seek(SeekFrom::Start(4))?;
            file.write_all(&new_frm8_size.to_be_bytes())?;
        } else {
            // Insert new ID3 chunk at end
            let file_end = dsdiff_file.file_size;

            resize_bytes(file, 0, new_chunk_size, file_end)?;

            file.seek(SeekFrom::Start(file_end))?;
            file.write_all(b"ID3 ")?;
            file.write_all(&new_id3_size.to_be_bytes())?;
            file.write_all(&new_id3_data)?;
            if new_id3_size % 2 == 1 {
                file.write_all(&[0])?;
            }

            // Update FRM8 size header
            let new_frm8_size = dsdiff_file.file_size.saturating_sub(12) + new_chunk_size;
            file.seek(SeekFrom::Start(4))?;
            file.write_all(&new_frm8_size.to_be_bytes())?;
        }

        Ok(())
    }

    /// Cursor-based save for trait-object writers that cannot call `set_len`.
    /// Used by `save_to_writer` where the caller may not be a real file.
    fn save_to_file_inner(
        &mut self,
        file: &mut std::io::Cursor<Vec<u8>>,
        v2_version: u8,
        v23_sep: &str,
    ) -> Result<()> {
        // Parse current file structure
        let dsdiff_file = DSDIFFFile::parse(file)?;
        self.dsdiff_file = Some(dsdiff_file.clone());

        // Find existing ID3 chunk
        let existing_id3_chunk = dsdiff_file.find_chunk(b"ID3 ");

        if self.tags.is_none() {
            // Delete existing ID3 chunk if present
            if let Some(id3_chunk) = existing_id3_chunk {
                let old_chunk_size = 12 + id3_chunk.size + (id3_chunk.size % 2); // Header + data + padding
                let chunk_offset = id3_chunk.data_offset.saturating_sub(12); // Include header
                resize_bytes(file, old_chunk_size, 0, chunk_offset)?;

                // Update FRM8 size header
                let new_frm8_size = dsdiff_file
                    .file_size
                    .saturating_sub(12)
                    .saturating_sub(old_chunk_size); // Subtract FRM8 header and deleted chunk
                std::io::Seek::seek(file, SeekFrom::Start(4))?; // Seek to FRM8 size field
                std::io::Write::write_all(file, &new_frm8_size.to_be_bytes())?;
            }
            return Ok(());
        }

        // Generate new ID3 data with dynamic padding via PaddingInfo
        let new_id3_data = {
            let minimal_data = self.generate_id3_data(v2_version, v23_sep, 0)?;
            let needed = minimal_data.len();
            let available = existing_id3_chunk.as_ref().map_or(0, |c| c.size as usize);
            let file_size = std::io::Seek::seek(file, SeekFrom::End(0))?;
            // trailing_size = data from tag position to end of file
            let trailing_size = match existing_id3_chunk.as_ref() {
                Some(chunk) => {
                    let fs = i64::try_from(file_size).map_err(|_| {
                        AudexError::InvalidData("File size exceeds i64 range".to_string())
                    })?;
                    let offset = i64::try_from(chunk.data_offset).map_err(|_| {
                        AudexError::InvalidData("Chunk data offset exceeds i64 range".to_string())
                    })?;
                    fs - offset
                }
                None => 0,
            };
            let info = PaddingInfo::new(available as i64 - needed as i64, trailing_size);
            let padding = info.get_default_padding().max(0) as usize;
            self.generate_id3_data(v2_version, v23_sep, padding)?
        };
        let new_id3_size = new_id3_data.len() as u64;
        let new_chunk_size = 12 + new_id3_size + (new_id3_size % 2); // Header + data + padding

        if let Some(id3_chunk) = existing_id3_chunk {
            // Modify existing ID3 chunk using resize pattern
            let old_chunk_size = 12 + id3_chunk.size + (id3_chunk.size % 2);
            let chunk_offset = id3_chunk.data_offset.saturating_sub(12); // Include header

            // Resize the chunk space
            resize_bytes(file, old_chunk_size, new_chunk_size, chunk_offset)?;

            // Write new chunk header
            std::io::Seek::seek(file, SeekFrom::Start(chunk_offset))?;
            std::io::Write::write_all(file, b"ID3 ")?;
            std::io::Write::write_all(file, &new_id3_size.to_be_bytes())?;

            // Write new ID3 data
            std::io::Write::write_all(file, &new_id3_data)?;

            // Add padding byte if needed
            if new_id3_size % 2 == 1 {
                std::io::Write::write_all(file, &[0])?;
            }

            // Update FRM8 size header
            let new_chunk_i64 = i64::try_from(new_chunk_size).map_err(|_| {
                AudexError::InvalidData("New chunk size exceeds i64 range".to_string())
            })?;
            let old_chunk_i64 = i64::try_from(old_chunk_size).map_err(|_| {
                AudexError::InvalidData("Old chunk size exceeds i64 range".to_string())
            })?;
            let size_diff = new_chunk_i64 - old_chunk_i64;
            let frm8_base =
                i64::try_from(dsdiff_file.file_size.saturating_sub(12)).map_err(|_| {
                    AudexError::InvalidData("FRM8 base size exceeds i64 range".to_string())
                })?;
            let new_frm8_size = frm8_base + size_diff;
            // Reject negative results -- a negative FRM8 size indicates
            // a corrupted old chunk size or an impossible shrink.
            let new_frm8_size = u64::try_from(new_frm8_size).map_err(|_| {
                AudexError::InvalidData(format!(
                    "Computed FRM8 size is negative ({}), file structure may be corrupt",
                    new_frm8_size
                ))
            })?;
            std::io::Seek::seek(file, SeekFrom::Start(4))?; // Seek to FRM8 size field
            std::io::Write::write_all(file, &new_frm8_size.to_be_bytes())?;
        } else {
            // Insert new ID3 chunk at end
            let file_end = dsdiff_file.file_size;

            // Insert space for new chunk
            resize_bytes(file, 0, new_chunk_size, file_end)?;

            // Write new chunk at end
            std::io::Seek::seek(file, SeekFrom::Start(file_end))?;
            std::io::Write::write_all(file, b"ID3 ")?;
            std::io::Write::write_all(file, &new_id3_size.to_be_bytes())?;
            std::io::Write::write_all(file, &new_id3_data)?;

            // Add padding byte if needed
            if new_id3_size % 2 == 1 {
                std::io::Write::write_all(file, &[0])?;
            }

            // Update FRM8 size header
            let new_frm8_size = dsdiff_file.file_size.saturating_sub(12) + new_chunk_size; // Subtract FRM8 header, add new chunk
            std::io::Seek::seek(file, SeekFrom::Start(4))?; // Seek to FRM8 size field
            std::io::Write::write_all(file, &new_frm8_size.to_be_bytes())?;
        }

        Ok(())
    }

    /// Generate ID3v2 data for DSDIFF chunk
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

            // Write synchsafe size (frame data size, max 28 bits = 268,435,455)
            let frame_size = frame_data.len() as u32;
            if frame_size > 0x0FFF_FFFF {
                return Err(AudexError::InvalidData(
                    "ID3 tag data exceeds synchsafe size limit (268,435,455 bytes)".to_string(),
                ));
            }
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

    /// Load a DSDIFF file asynchronously from the specified path.
    ///
    /// This method opens the file at the given path and parses its DSDIFF structure,
    /// extracting stream information and any embedded ID3 tags.
    ///
    /// # Arguments
    /// * `path` - The file path to load
    ///
    /// # Returns
    /// A `Result` containing the parsed DSDIFF instance or an error
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut dsdiff = DSDIFF::new();
        dsdiff.filename = Some(path.as_ref().to_string_lossy().to_string());

        dsdiff.parse_file_async(&mut file).await?;
        Ok(dsdiff)
    }

    /// Parse DSDIFF file structure asynchronously.
    ///
    /// Reads the FRM8 header, validates the DSD format type, and parses the PROP chunk
    /// to extract sample rate and channel configuration. This method populates the
    /// stream information fields of the DSDIFF instance.
    ///
    /// # Arguments
    /// * `file` - A mutable reference to a Tokio file handle
    ///
    /// # Returns
    /// A `Result` indicating success or an error if parsing fails
    #[cfg(feature = "async")]
    async fn parse_file_async(&mut self, file: &mut TokioFile) -> Result<()> {
        file.seek(SeekFrom::Start(0)).await?;

        // Read and validate FRM8 header (12 bytes: 4-byte ID + 8-byte size)
        let mut header = [0u8; 12];
        file.read_exact(&mut header).await?;

        // Verify DSDIFF signature
        if &header[0..4] != b"FRM8" {
            return Err(AudexError::InvalidData("Not a DSDIFF file".to_string()));
        }

        // Parse file size from header (big-endian 64-bit)
        let _file_size = u64::from_be_bytes([
            header[4], header[5], header[6], header[7], header[8], header[9], header[10],
            header[11],
        ]);

        // Read and validate DSD format type (4 bytes)
        let mut format_type = [0u8; 4];
        file.read_exact(&mut format_type).await?;

        if &format_type != b"DSD " {
            return Err(AudexError::InvalidData(
                "Not a DSD format DSDIFF file".to_string(),
            ));
        }

        // Initialize default values for parsing
        let mut offset = 16u64;
        let mut sample_rate = 2822400u32; // Default DSD64 sample rate
        let mut channels = 2u16;
        let mut compression = String::from("DSD");
        let mut dsd_chunk_size: Option<u64> = None;
        let mut dst_frame_count: Option<u32> = None;
        let mut dst_frame_rate: Option<u16> = None;
        let mut dst_chunk_size: Option<u64> = None;
        let mut dst_frte_total: Option<u64> = None;

        // Scan for PROP chunk to extract audio properties
        while offset < file.seek(SeekFrom::End(0)).await? {
            file.seek(SeekFrom::Start(offset)).await?;

            // Read chunk header (12 bytes)
            let mut chunk_header = [0u8; 12];
            if file.read_exact(&mut chunk_header).await.is_err() {
                break;
            }

            let chunk_id = &chunk_header[0..4];
            let chunk_size = u64::from_be_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
                chunk_header[8],
                chunk_header[9],
                chunk_header[10],
                chunk_header[11],
            ]);

            // Reject chunk sizes that would overflow seeks
            if chunk_size > i64::MAX as u64 {
                break;
            }

            // Parse PROP chunk for sound properties
            if chunk_id == b"PROP" {
                let mut prop_type = [0u8; 4];
                file.read_exact(&mut prop_type).await?;

                if &prop_type == b"SND " {
                    // Parse sound property sub-chunks, clamped to file size
                    let async_file_size = file.seek(SeekFrom::End(0)).await?;
                    file.seek(SeekFrom::Start(offset + 16)).await?;
                    let prop_end = offset
                        .saturating_add(12)
                        .saturating_add(chunk_size)
                        .min(async_file_size);
                    let mut prop_offset = offset + 16;

                    while prop_offset < prop_end {
                        file.seek(SeekFrom::Start(prop_offset)).await?;

                        let mut sub_header = [0u8; 12];
                        if file.read_exact(&mut sub_header).await.is_err() {
                            break;
                        }

                        let sub_id = &sub_header[0..4];
                        let sub_size = u64::from_be_bytes([
                            sub_header[4],
                            sub_header[5],
                            sub_header[6],
                            sub_header[7],
                            sub_header[8],
                            sub_header[9],
                            sub_header[10],
                            sub_header[11],
                        ]);

                        // Reject oversized sub-chunks
                        if sub_size > i64::MAX as u64 {
                            break;
                        }

                        // Extract sample rate from FS chunk
                        if sub_id == b"FS  " {
                            let mut rate_bytes = [0u8; 4];
                            file.read_exact(&mut rate_bytes).await?;
                            sample_rate = u32::from_be_bytes(rate_bytes);
                        }
                        // Extract channel count from CHNL chunk
                        else if sub_id == b"CHNL" {
                            let mut chan_bytes = [0u8; 2];
                            file.read_exact(&mut chan_bytes).await?;
                            channels = u16::from_be_bytes(chan_bytes);
                        }
                        // Extract compression type from CMPR chunk
                        else if sub_id == b"CMPR" && sub_size >= 4 {
                            let mut cmpr_bytes = [0u8; 4];
                            file.read_exact(&mut cmpr_bytes).await?;
                            compression = String::from_utf8_lossy(&cmpr_bytes)
                                .trim_end_matches('\0')
                                .trim_end()
                                .to_string();
                        }

                        // A zero-size sub-chunk indicates corruption or a
                        // zero-filled region — stop scanning rather than
                        // iterating 12 bytes at a time through the rest.
                        if sub_size == 0 {
                            break;
                        }

                        // Move to next sub-chunk (with even-byte alignment)
                        prop_offset = prop_offset.saturating_add(12).saturating_add(sub_size);
                        if sub_size % 2 == 1 {
                            prop_offset = prop_offset.saturating_add(1);
                        }
                    }
                }
            }

            // Detect DSD data chunk
            if chunk_id == b"DSD " {
                dsd_chunk_size = Some(chunk_size);
            }

            // Detect DST frame chunk and parse FRTE within it
            if chunk_id == b"DST " {
                dst_chunk_size = Some(chunk_size);
                // Look for FRTE sub-chunk within DST
                let dst_data_offset = offset + 12;
                let async_file_size = file.seek(SeekFrom::End(0)).await?;
                let dst_end = dst_data_offset
                    .saturating_add(chunk_size)
                    .min(async_file_size);
                let mut dst_offset = dst_data_offset;

                while dst_offset < dst_end {
                    file.seek(SeekFrom::Start(dst_offset)).await?;
                    let mut sub_header = [0u8; 12];
                    if file.read_exact(&mut sub_header).await.is_err() {
                        break;
                    }
                    let sub_id = &sub_header[0..4];
                    let sub_size = u64::from_be_bytes([
                        sub_header[4],
                        sub_header[5],
                        sub_header[6],
                        sub_header[7],
                        sub_header[8],
                        sub_header[9],
                        sub_header[10],
                        sub_header[11],
                    ]);

                    if sub_id == b"FRTE" && sub_size >= 6 {
                        let mut frte_data = [0u8; 6];
                        file.read_exact(&mut frte_data).await?;
                        dst_frame_count = Some(u32::from_be_bytes([
                            frte_data[0],
                            frte_data[1],
                            frte_data[2],
                            frte_data[3],
                        ]));
                        dst_frame_rate = Some(u16::from_be_bytes([frte_data[4], frte_data[5]]));
                        dst_frte_total = Some(12 + sub_size + (sub_size % 2));
                        break;
                    }

                    // A zero-size sub-chunk indicates corruption or a
                    // zero-filled region — stop scanning rather than
                    // iterating 12 bytes at a time through the rest.
                    if sub_size == 0 {
                        break;
                    }

                    dst_offset = dst_offset.saturating_add(12).saturating_add(sub_size);
                    if sub_size % 2 == 1 {
                        dst_offset = dst_offset.saturating_add(1);
                    }
                }
            }

            // A zero-size top-level chunk indicates corruption or a
            // zero-filled region. Break to match the sync parser behavior
            // and avoid scanning the entire file 12 bytes at a time.
            if chunk_size == 0 {
                break;
            }

            // Move to next chunk (with even-byte alignment)
            offset = offset.saturating_add(12).saturating_add(chunk_size);
            if chunk_size % 2 == 1 {
                offset = offset.saturating_add(1);
            }
        }

        // Calculate duration and bitrate based on compression type
        let mut length = None;
        let bitrate;

        if compression == "DSD" {
            if let Some(dsd_size) = dsd_chunk_size {
                // DSD data has one bit per sample, 8 samples per byte
                let sample_count = dsd_size.saturating_mul(8) as f64 / channels.max(1) as f64;
                if sample_rate > 0 {
                    length =
                        std::time::Duration::try_from_secs_f64(sample_count / sample_rate as f64)
                            .ok();
                }
                bitrate = (channels as u32)
                    .saturating_mul(1_u32)
                    .saturating_mul(sample_rate);
            } else {
                bitrate = (channels as u32).saturating_mul(sample_rate);
            }
        } else if compression == "DST" {
            if let (Some(frame_count), Some(frame_rate)) = (dst_frame_count, dst_frame_rate) {
                if frame_rate > 0 {
                    length = std::time::Duration::try_from_secs_f64(
                        frame_count as f64 / frame_rate as f64,
                    )
                    .ok();
                }
                if frame_count > 0 {
                    if let (Some(dst_size), Some(frte_total)) = (dst_chunk_size, dst_frte_total) {
                        let dst_data_size = dst_size.saturating_sub(frte_total);
                        let avg_frame_size = dst_data_size as f64 / frame_count as f64;
                        // Clamp before casting to avoid truncation on extreme values
                        let bitrate_raw = (avg_frame_size * 8.0 * frame_rate as f64).round();
                        bitrate = if bitrate_raw > u32::MAX as f64 {
                            u32::MAX
                        } else {
                            bitrate_raw as u32
                        };
                    } else {
                        bitrate = 0;
                    }
                } else {
                    bitrate = 0;
                }
            } else {
                bitrate = 0;
                length = std::time::Duration::try_from_secs_f64(0.0).ok();
            }
        } else {
            bitrate = (channels as u32).saturating_mul(sample_rate);
        }

        // Populate stream information
        self.info = DSDIFFStreamInfo {
            length,
            bitrate: Some(bitrate),
            channels,
            sample_rate,
            bits_per_sample: 1, // DSD is always 1-bit per sample
            compression,
        };

        // Parse ID3 tags - scan for "ID3 " chunk
        let file_size = file.seek(SeekFrom::End(0)).await?;
        let mut id3_offset = 16u64; // Start after FRM8 header + form type

        while id3_offset < file_size {
            file.seek(SeekFrom::Start(id3_offset)).await?;

            let mut chunk_header = [0u8; 12];
            if file.read_exact(&mut chunk_header).await.is_err() {
                break;
            }

            let chunk_id = &chunk_header[0..4];
            let chunk_size = u64::from_be_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
                chunk_header[8],
                chunk_header[9],
                chunk_header[10],
                chunk_header[11],
            ]);

            if chunk_id == b"ID3 " {
                // Found ID3 chunk — use 16 MB as the format-level ceiling
                // and also respect the caller-configured ParseLimits.
                const MAX_ID3_CHUNK: u64 = 16 * 1024 * 1024; // 16 MB
                let limits = crate::limits::ParseLimits::default();
                let effective_cap = MAX_ID3_CHUNK.min(limits.max_tag_size);
                if chunk_size > effective_cap {
                    return Err(AudexError::ParseError(format!(
                        "DSDIFF ID3 chunk too large: {} bytes",
                        chunk_size
                    )));
                }
                let id3_len = usize::try_from(chunk_size).map_err(|_| {
                    AudexError::InvalidData("chunk size exceeds platform address space".into())
                })?;
                let mut id3_data = vec![0u8; id3_len];
                if file.read_exact(&mut id3_data).await.is_ok() {
                    // Parse ID3 tags
                    if id3_data.len() >= 10 {
                        use crate::id3::{specs, tags::ID3Header};
                        match specs::ID3Header::from_bytes(&id3_data) {
                            Ok(specs_header) => {
                                let header = ID3Header::from_specs_header(&specs_header);
                                match ID3Tags::from_data(&id3_data, &header) {
                                    Ok(tags) => {
                                        self.tags = Some(tags);
                                    }
                                    Err(_) => {
                                        self.tags = None;
                                    }
                                }
                            }
                            Err(_) => {
                                self.tags = None;
                            }
                        }
                    }
                }
                break;
            }

            // Reject oversized chunks
            if chunk_size > i64::MAX as u64 {
                break;
            }

            // A zero-size chunk indicates corruption or a zero-filled
            // region. Break to match the sync parser behavior.
            if chunk_size == 0 {
                break;
            }

            // Move to next chunk (with even-byte alignment)
            id3_offset = id3_offset.saturating_add(12).saturating_add(chunk_size);
            if chunk_size % 2 == 1 {
                id3_offset = id3_offset.saturating_add(1);
            }
        }

        Ok(())
    }

    /// Save DSDIFF file asynchronously.
    ///
    /// Writes the current ID3v2 tags to the DSDIFF file by modifying or creating
    /// an ID3 chunk. Uses the default ID3v2.3 format.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if save fails
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        self.save_with_options_async(None, None, None).await
    }

    /// Save ID3 tags to DSDIFF file asynchronously with configurable options
    ///
    /// Writes the current ID3v2 tags to the DSDIFF file with custom version
    /// and separator settings. Modifies or creates an ID3 chunk as needed.
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

        // Read FRM8 header to get file size
        file.seek(SeekFrom::Start(0)).await?;
        let mut header = [0u8; 12];
        file.read_exact(&mut header).await?;

        if &header[0..4] != b"FRM8" {
            return Err(AudexError::InvalidData("Not a DSDIFF file".to_string()));
        }

        let frm8_size = u64::from_be_bytes([
            header[4], header[5], header[6], header[7], header[8], header[9], header[10],
            header[11],
        ]);
        // Clamp to actual file size to prevent loops on malformed sizes
        let actual_file_size = file.seek(SeekFrom::End(0)).await?;
        let file_size = frm8_size.saturating_add(12).min(actual_file_size);

        // Find existing ID3 chunk by scanning all chunks
        let mut id3_chunk_offset: Option<u64> = None;
        let mut id3_chunk_size: u64 = 0;
        let mut offset = 16u64; // Start after FRM8 header + form type

        while offset < file_size {
            file.seek(SeekFrom::Start(offset)).await?;

            let mut chunk_header = [0u8; 12];
            if file.read_exact(&mut chunk_header).await.is_err() {
                break;
            }

            let chunk_id = &chunk_header[0..4];
            let chunk_size = u64::from_be_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
                chunk_header[8],
                chunk_header[9],
                chunk_header[10],
                chunk_header[11],
            ]);

            if chunk_id == b"ID3 " {
                id3_chunk_offset = Some(offset);
                id3_chunk_size = chunk_size;
                break;
            }

            // Reject oversized chunks
            if chunk_size > i64::MAX as u64 {
                break;
            }

            // A zero-size chunk indicates corruption or a zero-filled
            // region. Break to match the sync parser behavior.
            if chunk_size == 0 {
                break;
            }

            // Move to next chunk (with even-byte alignment)
            offset = offset.saturating_add(12).saturating_add(chunk_size);
            if chunk_size % 2 == 1 {
                offset = offset.saturating_add(1);
            }
        }

        if self.tags.is_none() {
            // Delete existing ID3 chunk if present
            if let Some(chunk_offset) = id3_chunk_offset {
                let old_chunk_total = 12 + id3_chunk_size + (id3_chunk_size % 2);
                resize_bytes_async(&mut file, old_chunk_total, 0, chunk_offset).await?;

                // Update FRM8 size header — reject negative results to catch
                // corrupted chunk sizes that would underflow the FRM8 field.
                let new_frm8_size =
                    u64::try_from(frm8_size as i64 - old_chunk_total as i64).map_err(|_| {
                        AudexError::InvalidData(format!(
                            "Computed FRM8 size is negative ({} - {}), file structure may be corrupt",
                            frm8_size, old_chunk_total
                        ))
                    })?;
                file.seek(SeekFrom::Start(4)).await?;
                file.write_all(&new_frm8_size.to_be_bytes()).await?;
            }
            file.flush().await?;
            return Ok(());
        }

        // Generate new ID3 data with dynamic padding via PaddingInfo
        let new_id3_data = {
            let minimal_data = self.generate_id3_data(v2_version_option, &v23_sep_string, 0)?;
            let needed = minimal_data.len();
            let available = if id3_chunk_offset.is_some() {
                id3_chunk_size as usize
            } else {
                0
            };
            let file_size = file.seek(SeekFrom::End(0)).await?;
            // trailing_size = data from tag position to end of file
            let trailing_size = match id3_chunk_offset {
                Some(off) => file_size as i64 - off as i64,
                None => 0,
            };
            let info = PaddingInfo::new(available as i64 - needed as i64, trailing_size);
            let padding = info.get_default_padding().max(0) as usize;
            self.generate_id3_data(v2_version_option, &v23_sep_string, padding)?
        };
        let new_id3_size = new_id3_data.len() as u64;
        let new_chunk_total = 12 + new_id3_size + (new_id3_size % 2);

        if let Some(chunk_offset) = id3_chunk_offset {
            // Modify existing ID3 chunk
            let old_chunk_total = 12 + id3_chunk_size + (id3_chunk_size % 2);

            // Resize the chunk space
            resize_bytes_async(&mut file, old_chunk_total, new_chunk_total, chunk_offset).await?;

            // Write new chunk header
            file.seek(SeekFrom::Start(chunk_offset)).await?;
            file.write_all(b"ID3 ").await?;
            file.write_all(&new_id3_size.to_be_bytes()).await?;

            // Write new ID3 data
            file.write_all(&new_id3_data).await?;

            // Add padding byte if needed
            if new_id3_size % 2 == 1 {
                file.write_all(&[0]).await?;
            }

            // Update FRM8 size header — reject negative results to avoid
            // silent corruption from wrapping a negative i64 to u64.
            let size_diff = new_chunk_total as i64 - old_chunk_total as i64;
            let new_frm8_size = u64::try_from(frm8_size as i64 + size_diff).map_err(|_| {
                AudexError::InvalidData(format!(
                    "Computed FRM8 size is negative ({}), file structure may be corrupt",
                    frm8_size as i64 + size_diff
                ))
            })?;
            file.seek(SeekFrom::Start(4)).await?;
            file.write_all(&new_frm8_size.to_be_bytes()).await?;
        } else {
            // Insert new ID3 chunk at end
            resize_bytes_async(&mut file, 0, new_chunk_total, file_size).await?;

            // Write new chunk at end
            file.seek(SeekFrom::Start(file_size)).await?;
            file.write_all(b"ID3 ").await?;
            file.write_all(&new_id3_size.to_be_bytes()).await?;
            file.write_all(&new_id3_data).await?;

            // Add padding byte if needed
            if new_id3_size % 2 == 1 {
                file.write_all(&[0]).await?;
            }

            // Update FRM8 size header
            let new_frm8_size = frm8_size + new_chunk_total;
            file.seek(SeekFrom::Start(4)).await?;
            file.write_all(&new_frm8_size.to_be_bytes()).await?;
        }

        file.flush().await?;
        Ok(())
    }

    /// Clear all ID3 tags from the DSDIFF file asynchronously.
    ///
    /// Removes any embedded ID3 tags and saves the file asynchronously.
    ///
    /// # Returns
    /// A `Result` indicating success
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        self.tags = None;
        if self.filename.is_some() {
            self.save_async().await
        } else {
            Ok(())
        }
    }

    /// Delete the DSDIFF file from disk asynchronously.
    ///
    /// Removes the file at the path stored in `filename`. This operation
    /// is irreversible and will fail if no filename is set.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if deletion fails
    #[cfg(feature = "async")]
    pub async fn delete_async(&mut self) -> Result<()> {
        if let Some(ref filename) = self.filename {
            tokio::fs::remove_file(filename).await?;
        }
        Ok(())
    }
}

impl Default for DSDIFF {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for DSDIFF {
    type Tags = ID3Tags;
    type Info = DSDIFFStreamInfo;

    fn format_id() -> &'static str {
        "DSDIFF"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing DSDIFF file");
        let mut file = std::fs::File::open(&path)?;
        let mut dsdiff = DSDIFF::new();
        dsdiff.filename = Some(path.as_ref().to_string_lossy().to_string());

        dsdiff.parse_file(&mut file)?;
        Ok(dsdiff)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing DSDIFF file from reader");
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
        self.tags = None;
        if self.filename.is_some() {
            self.save()
        } else {
            Ok(())
        }
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        // Read the entire content into memory so we can operate on a Cursor,
        // which supports the resize_bytes operations needed for IFF chunk
        // manipulation (insert_bytes / delete_bytes require concrete types).
        let data = crate::util::read_all_from_writer_limited(writer, "in-memory DSDIFF save")?;
        let mut cursor = std::io::Cursor::new(data);

        self.save_to_file_inner(&mut cursor, 3, "/")?;

        // Write the modified data back to the original writer
        let result = cursor.into_inner();
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&result)?;

        // Zero out any stale trailing bytes from the original content.
        // When metadata shrinks, the output is shorter than the input but
        // Cursor/File writers do not auto-truncate — old bytes persist.
        // Write in fixed-size chunks to avoid a huge single allocation
        // if the size difference is large.
        let written_end = writer.stream_position()?;
        let total_end = writer.seek(SeekFrom::End(0))?;
        if written_end < total_end {
            const MAX_PADDING: u64 = 4 * 1024 * 1024; // 4 MB
            let padding_size = total_end - written_end;
            if padding_size > MAX_PADDING {
                return Err(AudexError::InvalidData(format!(
                    "DSDIFF stale tail too large: {} bytes exceeds {} byte cap",
                    padding_size, MAX_PADDING
                )));
            }
            const CHUNK: usize = 64 * 1024; // 64 KB per write
            let zero_chunk = vec![0u8; CHUNK];
            writer.seek(SeekFrom::Start(written_end))?;
            let mut remaining = padding_size as usize;
            while remaining > 0 {
                let n = remaining.min(CHUNK);
                writer.write_all(&zero_chunk[..n])?;
                remaining -= n;
            }
        }

        writer.flush()?;
        Ok(())
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        self.tags = None;
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
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
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

        // Check for DSDIFF signature
        if header.len() >= 4 && header.starts_with(b"FRM8") {
            score += 2;
        }

        // Check file extension
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".dff") {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/x-dff"]
    }
}

/// Standalone functions for DSDIFF operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut dsdiff = DSDIFF::load(path)?;
    dsdiff.clear()
}
