//! # AIFF Format Support
//!
//! This module provides support for reading and writing Audio Interchange File Format (AIFF)
//! files, including AIFF-C (compressed) variants.
//!
//! ## Overview
//!
//! AIFF is an uncompressed audio format developed by Apple, stored in an IFF/FORM
//! container structure. This module handles:
//!
//! - **Stream information**: Sample rate, channels, bit depth, and duration from the COMM chunk
//! - **ID3v2 tagging**: Reading and writing ID3v2 tags embedded in the IFF structure
//! - **Chunk navigation**: Parsing and locating IFF chunks (COMM, SSND, ID3, etc.)
//!
//! ## Basic Usage
//!
//! ```no_run
//! use audex::aiff::AIFF;
//! use audex::FileType;
//!
//! let mut aiff = AIFF::load("audio.aif")?;
//! println!("Sample rate: {} Hz", aiff.info.sample_rate);
//! println!("Channels: {}", aiff.info.channels);
//! # Ok::<(), audex::AudexError>(())
//! ```

use crate::tags::PaddingInfo;
use crate::util::delete_bytes;
use crate::{
    AudexError, FileType, ReadWriteSeek, Result, StreamInfo,
    id3::{ID3Tags, specs, tags::ID3Header},
};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::iff::{IffChunkAsync, IffFileAsync, resize_iff_chunk_async, update_iff_file_size_async};
#[cfg(feature = "async")]
use crate::util::{
    delete_bytes_async, insert_bytes_async, loadfile_read_async, loadfile_write_async,
};
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncSeekExt, AsyncWriteExt};

/// An IFF/AIFF chunk within the FORM container.
///
/// Each chunk has a 4-character ID, a size, and data payload. Chunks are the
/// fundamental building blocks of AIFF files (e.g., `COMM`, `SSND`, `ID3 `).
#[derive(Debug, Clone)]
pub struct AIFFChunk {
    /// 4-character chunk identifier (e.g., `"COMM"`, `"SSND"`, `"ID3 "`)
    pub id: String,
    /// Total size of the chunk including the 8-byte header (ID + size fields)
    pub size: u32,
    /// Byte offset of the chunk header from the start of the file
    pub offset: u64,
    /// Byte offset of the chunk's data payload from the start of the file
    pub data_offset: u64,
    /// Size of the chunk's data payload in bytes
    pub data_size: u32,
}

impl AIFFChunk {
    /// Read this chunk's data payload from the given reader.
    pub fn read_data<R: Read + Seek>(&self, reader: &mut R) -> Result<Vec<u8>> {
        // Enforce the library-wide tag allocation ceiling
        crate::limits::ParseLimits::default()
            .check_tag_size(self.data_size as u64, "AIFF chunk")?;
        reader.seek(SeekFrom::Start(self.data_offset))?;
        let mut data = vec![0u8; self.data_size as usize];
        reader.read_exact(&mut data)?;
        Ok(data)
    }

    /// Write data to this chunk's payload region in the given writer.
    pub fn write_data<W: std::io::Write + Seek>(&self, writer: &mut W, data: &[u8]) -> Result<()> {
        writer.seek(SeekFrom::Start(self.data_offset))?;
        writer.write_all(data)?;
        Ok(())
    }
}

/// Parsed IFF/AIFF file structure.
///
/// Represents the top-level FORM container, containing the form type
/// (e.g., `"AIFF"` or `"AIFC"`) and all parsed chunks.
#[derive(Debug, Clone)]
pub struct AIFFFile {
    /// Form type identifier (`"AIFF"` for standard, `"AIFC"` for compressed)
    pub file_type: String,
    /// All chunks found in the file, in order
    pub chunks: Vec<AIFFChunk>,
    /// Total file size as declared in the FORM header
    pub file_size: u32,
}

impl AIFFFile {
    /// Parse AIFF file structure from a reader, returning the FORM type and all chunks.
    pub fn parse<R: Read + Seek + ?Sized>(reader: &mut R) -> Result<Self> {
        reader.seek(SeekFrom::Start(0))?;

        // Read FORM header
        let mut header = [0u8; 12];
        reader.read_exact(&mut header)?;

        if &header[0..4] != b"FORM" {
            return Err(AudexError::IFFError("Expected FORM signature".to_string()));
        }

        let file_size = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
        let file_type = String::from_utf8_lossy(&header[8..12]).into_owned();

        if file_type != "AIFF" && file_type != "AIFC" {
            return Err(AudexError::AIFFError(
                "Expected AIFF or AIFC format".to_string(),
            ));
        }

        let mut chunks = Vec::new();
        let mut offset = 12u64; // After FORM header

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
            let chunk_size = u32::from_be_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
            ]);

            // A zero-size chunk is valid per the IFF spec. Skip past its header
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

            // Use checked arithmetic so a corrupt chunk_size near u32::MAX
            // produces a clear error instead of silently clamping to u32::MAX.
            let total_size = chunk_size.checked_add(8).ok_or_else(|| {
                AudexError::InvalidData(format!(
                    "chunk '{}' size {} overflows when adding 8-byte header",
                    chunk_id, chunk_size
                ))
            })?;

            let chunk = AIFFChunk {
                id: chunk_id,
                size: total_size,
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

        Ok(AIFFFile {
            file_type,
            chunks,
            file_size,
        })
    }

    /// Find a chunk by its 4-character ID (case-insensitive).
    pub fn find_chunk(&self, id: &str) -> Option<&AIFFChunk> {
        self.chunks
            .iter()
            .find(|chunk| chunk.id.eq_ignore_ascii_case(id))
    }

    /// Returns `true` if a chunk with the given ID exists.
    pub fn has_chunk(&self, id: &str) -> bool {
        self.find_chunk(id).is_some()
    }
}

/// Read an IEEE 754 80-bit extended precision floating point value from a 10-byte slice.
///
/// Used to decode sample rates in AIFF COMM chunks. Returns an error if the
/// slice is not exactly 10 bytes or the value is infinity/NaN.
pub fn read_float(data: &[u8]) -> Result<f64> {
    if data.len() != 10 {
        return Err(AudexError::AIFFError(
            "Float data must be 10 bytes".to_string(),
        ));
    }

    let expon = i16::from_be_bytes([data[0], data[1]]);
    let himant = u32::from_be_bytes([data[2], data[3], data[4], data[5]]);
    let lomant = u32::from_be_bytes([data[6], data[7], data[8], data[9]]);

    let mut sign = 1.0;
    let mut expon = expon as i32;

    if expon < 0 {
        sign = -1.0;
        expon += 0x8000;
    }

    if expon == 0 && himant == 0 && lomant == 0 {
        return Ok(0.0);
    } else if expon == 0x7FFF {
        return Err(AudexError::AIFFError(
            "inf and nan not supported".to_string(),
        ));
    }

    expon -= 16383;
    let f = (himant as f64 * 4_294_967_296.0 + lomant as f64) * 2.0f64.powi(expon - 63);
    let result = sign * f;

    // Check for infinity and NaN does
    if result.is_infinite() || result.is_nan() {
        return Err(AudexError::AIFFError(
            "inf and nan not supported".to_string(),
        ));
    }

    Ok(result)
}

/// Audio stream properties extracted from an AIFF file's COMM chunk.
///
/// Contains sample rate, channel count, bit depth, duration, and related
/// properties needed to describe the audio stream.
#[derive(Debug, Default)]
pub struct AIFFStreamInfo {
    /// Duration of the audio stream, if calculable from frame count and sample rate
    pub length: Option<Duration>,
    /// Bitrate in bits per second (channels × sample_size × sample_rate)
    pub bitrate: Option<u32>,
    /// Number of audio channels (1 = mono, 2 = stereo)
    pub channels: u16,
    /// Sample rate in Hz (e.g., 44100, 48000)
    pub sample_rate: u32,
    /// Bits per sample (bit depth, e.g., 16, 24)
    pub bits_per_sample: u16,
    /// Raw sample size from the COMM chunk (alias for `bits_per_sample`, kept for compatibility)
    pub sample_size: u16,
    /// Total number of sample frames in the audio stream
    pub frame_count: u32,
}

impl StreamInfo for AIFFStreamInfo {
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

impl AIFFStreamInfo {
    /// Parse stream info from the COMM chunk of a parsed AIFF file.
    pub fn from_aiff_file<R: Read + Seek>(aiff: &AIFFFile, reader: &mut R) -> Result<Self> {
        // Find COMM chunk
        let comm_chunk = aiff
            .find_chunk("COMM")
            .ok_or_else(|| AudexError::AIFFError("No 'COMM' chunk found".to_string()))?;

        if comm_chunk.data_size < 18 {
            return Err(AudexError::AIFFError("COMM chunk too small".to_string()));
        }

        // Read COMM chunk data
        let comm_data = comm_chunk.read_data(reader)?;

        // Parse COMM chunk (minimum 18 bytes, big-endian)
        // - channels (2 bytes)
        // - frame_count (4 bytes)
        // - sample_size (2 bytes)
        // - sample_rate (10 bytes IEEE float)

        let channels = u16::from_be_bytes([comm_data[0], comm_data[1]]);
        let frame_count =
            u32::from_be_bytes([comm_data[2], comm_data[3], comm_data[4], comm_data[5]]);
        let sample_size = u16::from_be_bytes([comm_data[6], comm_data[7]]);
        let sample_rate_data = &comm_data[8..18];

        // Parse IEEE 80-bit float for sample rate, with bounds checking
        let sample_rate_f64 = read_float(sample_rate_data)?;
        if sample_rate_f64 < 0.0 || sample_rate_f64 > u32::MAX as f64 {
            return Err(AudexError::AIFFError(format!(
                "Sample rate {} is out of valid range for u32",
                sample_rate_f64
            )));
        }
        let sample_rate = sample_rate_f64 as u32;

        if sample_rate == 0 {
            return Err(AudexError::AIFFError("Invalid sample rate".to_string()));
        }

        // Calculate length
        let length = if sample_rate != 0 {
            Some(Duration::from_secs_f64(
                frame_count as f64 / sample_rate as f64,
            ))
        } else {
            None
        };

        // Calculate bitrate
        let bitrate = (channels as u32)
            .saturating_mul(sample_size as u32)
            .saturating_mul(sample_rate);

        Ok(AIFFStreamInfo {
            length,
            bitrate: Some(bitrate),
            channels,
            sample_rate,
            bits_per_sample: sample_size,
            sample_size, // For backward compatibility
            frame_count,
        })
    }

    /// Pretty print audio info
    pub fn pprint(&self) -> String {
        format!(
            "{} channel AIFF @ {} bps, {} Hz, {:.2} seconds",
            self.channels,
            self.bitrate.unwrap_or(0),
            self.sample_rate,
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0)
        )
    }
}

/// AIFF audio file with ID3v2 tags embedded in IFF chunks.
///
/// Provides access to stream properties via `info` and ID3v2 metadata via `tags`.
/// Tags are stored in an `ID3 ` chunk within the IFF container.
#[derive(Debug)]
pub struct AIFF {
    /// Audio stream properties (sample rate, channels, duration, etc.)
    pub info: AIFFStreamInfo,
    /// ID3v2 tags, if present in the file
    pub tags: Option<ID3Tags>,
    /// Path to the source file, if loaded from disk
    pub filename: Option<String>,
    aiff_file: Option<AIFFFile>,
}

impl AIFF {
    /// Create a new empty AIFF instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: AIFFStreamInfo::default(),
            tags: None,
            filename: None,
            aiff_file: None,
        }
    }

    /// Parse AIFF file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Parse AIFF structure
        reader.seek(SeekFrom::Start(0))?;
        let aiff_file = AIFFFile::parse(reader)?;
        for _chunk in &aiff_file.chunks {
            trace_event!(chunk_id = %_chunk.id, chunk_size = _chunk.size, "AIFF chunk");
        }

        // Parse stream info from COMM chunk
        self.info = AIFFStreamInfo::from_aiff_file(&aiff_file, reader)?;

        // Parse ID3 tags from 'ID3 ' chunk (note the trailing space)
        self.tags = if let Some(id3_chunk) = aiff_file.find_chunk("ID3 ") {
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

        self.aiff_file = Some(aiff_file);
        Ok(())
    }

    /// Add empty ID3 tags
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::AIFFError("ID3 tag already exists".to_string()));
        }
        self.tags = Some(ID3Tags::new());
        Ok(())
    }

    /// Clear ID3 tags
    pub fn clear(&mut self) -> Result<()> {
        self.tags = None;

        // Remove ID3 chunk from AIFF file if present
        let has_id3_chunk = if let Some(ref aiff_file) = self.aiff_file {
            aiff_file.chunks.iter().any(|chunk| chunk.id == "ID3 ")
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

    /// Remove ID3 chunk from AIFF file.
    fn remove_id3_chunk(&mut self, filename: &str) -> Result<()> {
        use std::fs::OpenOptions;

        let mut file = OpenOptions::new().read(true).write(true).open(filename)?;

        if let Some(ref aiff_file) = self.aiff_file {
            if let Some(chunk) = aiff_file.chunks.iter().find(|c| c.id == "ID3 ") {
                let pad = if chunk.data_size % 2 == 1 { 1u64 } else { 0 };
                let total_size = 8 + chunk.data_size as u64 + pad; // 8 = id(4) + size(4)
                let chunk_offset = chunk.offset;
                let old_form_size = aiff_file.file_size;

                // Remove chunk bytes
                delete_bytes(&mut file, total_size, chunk_offset, None)?;

                // Update FORM header size at offset 4 (big-endian).
                // Perform the subtraction in u64 to avoid truncating
                // total_size before the comparison. If the result does
                // not fit back into u32, the file is structurally invalid.
                let new_form_size_u64 =
                    (old_form_size as u64)
                        .checked_sub(total_size)
                        .ok_or_else(|| {
                            AudexError::InvalidData(
                                "ID3 chunk size exceeds FORM container size".to_string(),
                            )
                        })?;
                let new_form_size = u32::try_from(new_form_size_u64).map_err(|_| {
                    AudexError::InvalidData("New FORM size does not fit in u32".to_string())
                })?;
                file.seek(SeekFrom::Start(4))?;
                file.write_all(&new_form_size.to_be_bytes())?;
                file.flush()?;
            }
        }

        // Update internal representation
        if let Some(ref mut aiff_file) = self.aiff_file {
            aiff_file.chunks.retain(|chunk| chunk.id != "ID3 ");
        }

        Ok(())
    }

    pub fn mime(&self) -> Vec<&'static str> {
        vec!["audio/aiff", "audio/x-aiff"]
    }

    /// Pretty print file info
    pub fn pprint(&self) -> String {
        self.info.pprint()
    }

    /// Save ID3 tags to AIFF file with configurable options
    pub fn save_with_options(
        &mut self,
        file_path: Option<&str>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        let v2_version_option = v2_version.unwrap_or(3); // Default to v2.3 for AIFF compatibility
        let v23_sep_string = v23_sep.unwrap_or("/").to_string();
        let target_path = match file_path {
            Some(path) => path.to_string(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };
        self.save_to_file_with_options(target_path, v2_version_option, Some(v23_sep_string))
    }

    /// Save ID3 tags to AIFF file by modifying the ID3 chunk
    pub fn save_to_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        // Use ID3v2.3 by default for AIFF compatibility
        self.save_to_file_with_options(path.as_ref(), 3, Some("/".to_string()))
    }

    /// Internal save method with configurable options
    fn save_to_file_with_options<P: AsRef<Path>>(
        &mut self,
        path: P,
        v2_version: u8,
        v23_sep: Option<String>,
    ) -> Result<()> {
        use std::fs::OpenOptions;

        let file_path = path.as_ref();
        let mut file = OpenOptions::new().read(true).write(true).open(file_path)?;
        self.save_to_writer_impl(&mut file, v2_version, v23_sep)
    }

    /// Core save implementation that operates on any Read + Write + Seek handle.
    ///
    /// Parses the IFF structure from the writer, locates or creates an ID3 chunk,
    /// and writes the current tags into it. Uses v2_version and v23_sep to control
    /// the ID3v2 encoding.
    ///
    /// This method uses inline byte-shifting logic rather than the utility functions
    /// (`resize_bytes`, `insert_bytes`, `delete_bytes`) because those require a
    /// `'static` bound that `dyn ReadWriteSeek` trait objects cannot satisfy.
    fn save_to_writer_impl(
        &mut self,
        file: &mut dyn ReadWriteSeek,
        v2_version: u8,
        v23_sep: Option<String>,
    ) -> Result<()> {
        // Parse IFF structure to locate/create ID3 chunk
        let mut aiff_file = AIFFFile::parse(file)?;

        // Find existing ID3 chunk or determine where to insert it
        let id3_chunk = aiff_file.find_chunk("ID3 ");

        // Generate new ID3 data if tags exist, using dynamic padding via PaddingInfo
        let new_id3_data = if let Some(ref tags) = self.tags {
            // First, compute the ID3 data size without padding to calculate PaddingInfo
            let minimal_data = self.generate_id3_data(tags, v2_version, v23_sep.clone(), 0)?;
            let needed = minimal_data.len();
            let available = id3_chunk.as_ref().map_or(0, |c| c.data_size as usize);
            let file_size = file.seek(SeekFrom::End(0))?;
            // trailing_size = data from the tag position to end of file
            // For existing chunk: file_size - chunk.data_offset
            // For new chunk (appended at end): ~0
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

            // Add padding to align to even boundary (IFF requirement)
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

            // Resize the chunk data region in-place using inline byte shifting
            Self::resize_chunk_region(
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

            // Update chunk size header (4 bytes before data_offset) using big-endian for IFF
            let chunk_size_u32 = u32::try_from(new_size).map_err(|_| {
                AudexError::InvalidData("chunk size exceeds u32::MAX (> 4 GB)".to_string())
            })?;
            file.seek(SeekFrom::Start(existing_chunk.data_offset - 4))?;
            file.write_all(&chunk_size_u32.to_be_bytes())?;

            // Update FORM file size header when chunk size changes.
            // Use checked arithmetic to prevent silent wrapping on overflow.
            if padded_new_size != old_padded_size {
                let size_diff = padded_new_size as i64 - old_padded_size as i64;
                let computed = (aiff_file.file_size as i64)
                    .checked_add(size_diff)
                    .ok_or_else(|| {
                        AudexError::InvalidData("FORM file size arithmetic overflow".to_string())
                    })?;
                let new_form_size = u32::try_from(computed).map_err(|_| {
                    AudexError::InvalidData("FORM file size does not fit in u32".to_string())
                })?;
                file.seek(SeekFrom::Start(4))?; // FORM size is at offset 4
                file.write_all(&new_form_size.to_be_bytes())?;
                aiff_file.file_size = new_form_size;
            }
        } else if !new_id3_data.is_empty() {
            // No existing ID3 chunk - insert new one at the end before any "SSND" chunk
            Self::insert_id3_chunk_writer(file, &mut aiff_file, new_id3_data)?;
        }

        // Update our cached IFF structure
        self.aiff_file = Some(aiff_file);

        Ok(())
    }

    /// Resize a region within the stream, shifting trailing data as needed.
    ///
    /// This is an inline implementation that operates on `dyn ReadWriteSeek`
    /// without requiring a `'static` bound. When shrinking, trailing bytes
    /// may remain in the underlying storage; callers using `Cursor<Vec<u8>>`
    /// should truncate after this returns if needed.
    ///
    /// **Limitation**: When the new size is smaller than the old size, the
    /// file is *not* truncated. Stale data may remain on disk past the
    /// logical end indicated by the FORM header. Callers that need a clean
    /// file should truncate to the logical size after saving.
    fn resize_chunk_region(
        file: &mut dyn ReadWriteSeek,
        old_size: u64,
        new_size: u64,
        offset: u64,
    ) -> Result<()> {
        if old_size == new_size {
            return Ok(());
        }

        let file_size = file.seek(SeekFrom::End(0))?;
        let buffer_size: usize = 64 * 1024;

        if new_size > old_size {
            // Region grew -- shift trailing data to the right to make room.
            let grow = new_size - old_size;
            let src_start = offset + old_size;

            // Guard against underflow when a corrupt chunk declares a region
            // that extends past the physical end of the file.
            if src_start > file_size {
                return Err(AudexError::InvalidData(
                    "chunk region extends past end of file".into(),
                ));
            }

            let bytes_to_move = file_size - src_start;

            // Extend the stream by writing zeroes at the end.
            file.seek(SeekFrom::End(0))?;
            let mut remaining = grow;
            let zero_buf = vec![0u8; buffer_size];
            while remaining > 0 {
                let chunk = std::cmp::min(remaining, buffer_size as u64) as usize;
                file.write_all(&zero_buf[..chunk])?;
                remaining -= chunk as u64;
            }

            // Move data from right to left (reverse order to avoid overlap corruption).
            if bytes_to_move > 0 {
                let mut pos = bytes_to_move;
                let mut buf = vec![0u8; buffer_size];
                while pos > 0 {
                    let chunk = std::cmp::min(pos, buffer_size as u64) as usize;
                    let read_offset = src_start + pos - chunk as u64;
                    let write_offset = read_offset + grow;

                    file.seek(SeekFrom::Start(read_offset))?;
                    file.read_exact(&mut buf[..chunk])?;
                    file.seek(SeekFrom::Start(write_offset))?;
                    file.write_all(&buf[..chunk])?;

                    pos -= chunk as u64;
                }
            }
        } else {
            // Region shrank -- shift trailing data to the left.
            let shrink = old_size - new_size;
            let src_start = offset + old_size;
            let dst_start = offset + new_size;

            // Guard against underflow when a corrupt chunk declares a region
            // that extends past the physical end of the file.
            if src_start > file_size {
                return Err(AudexError::InvalidData(
                    "chunk region extends past end of file".into(),
                ));
            }

            let bytes_to_move = file_size - src_start;

            // Move data left in forward order.
            let mut moved = 0u64;
            let mut buf = vec![0u8; buffer_size];
            while moved < bytes_to_move {
                let chunk = std::cmp::min(bytes_to_move - moved, buffer_size as u64) as usize;
                file.seek(SeekFrom::Start(src_start + moved))?;
                file.read_exact(&mut buf[..chunk])?;
                file.seek(SeekFrom::Start(dst_start + moved))?;
                file.write_all(&buf[..chunk])?;
                moved += chunk as u64;
            }

            // Zero out the stale trailing bytes so they don't contain
            // leftover data from the old (larger) chunk. The FORM header
            // size is authoritative, but zeroing prevents data leakage.
            let new_total = file_size - shrink;
            file.seek(SeekFrom::Start(new_total))?;
            let zero_buf = vec![0u8; std::cmp::min(shrink as usize, buffer_size)];
            let mut remaining = shrink;
            while remaining > 0 {
                let chunk = std::cmp::min(remaining, zero_buf.len() as u64) as usize;
                file.write_all(&zero_buf[..chunk])?;
                remaining -= chunk as u64;
            }
            // Seek back to the new logical end
            file.seek(SeekFrom::Start(new_total))?;
        }

        file.flush()?;
        Ok(())
    }

    /// Insert bytes into the stream at the given offset, shifting trailing data right.
    ///
    /// Inline implementation for `dyn ReadWriteSeek` (no `'static` required).
    fn insert_bytes_writer(file: &mut dyn ReadWriteSeek, size: u64, offset: u64) -> Result<()> {
        if size == 0 {
            return Ok(());
        }

        let file_size = file.seek(SeekFrom::End(0))?;
        let buffer_size: usize = 64 * 1024;

        if offset > file_size {
            return Err(AudexError::InvalidData(format!(
                "Offset beyond file size: {} > {}",
                offset, file_size
            )));
        }

        // Extend the stream by writing zeroes at the end.
        file.seek(SeekFrom::End(0))?;
        let mut remaining = size;
        let zero_buf = vec![0u8; buffer_size];
        while remaining > 0 {
            let chunk = std::cmp::min(remaining, buffer_size as u64) as usize;
            file.write_all(&zero_buf[..chunk])?;
            remaining -= chunk as u64;
        }

        // Shift existing data after offset to the right (reverse order).
        let bytes_to_move = file_size - offset;
        if bytes_to_move > 0 {
            let mut pos = bytes_to_move;
            let mut buf = vec![0u8; buffer_size];
            while pos > 0 {
                let chunk = std::cmp::min(pos, buffer_size as u64) as usize;
                let read_offset = offset + pos - chunk as u64;
                let write_offset = read_offset + size;

                file.seek(SeekFrom::Start(read_offset))?;
                file.read_exact(&mut buf[..chunk])?;
                file.seek(SeekFrom::Start(write_offset))?;
                file.write_all(&buf[..chunk])?;

                pos -= chunk as u64;
            }
        }

        // Clear the inserted region with null bytes
        file.seek(SeekFrom::Start(offset))?;
        remaining = size;
        while remaining > 0 {
            let chunk = std::cmp::min(remaining, buffer_size as u64) as usize;
            file.write_all(&zero_buf[..chunk])?;
            remaining -= chunk as u64;
        }

        file.flush()?;
        Ok(())
    }

    /// Delete bytes from the stream at the given offset, shifting trailing data left.
    ///
    /// Inline implementation for `dyn ReadWriteSeek` (no `'static` required).
    /// After this call the logical file size is reduced, but the underlying storage
    /// may still have trailing bytes (for `Cursor<Vec<u8>>` callers).
    fn delete_bytes_writer(file: &mut dyn ReadWriteSeek, size: u64, offset: u64) -> Result<()> {
        if size == 0 {
            return Ok(());
        }

        let file_size = file.seek(SeekFrom::End(0))?;
        let buffer_size: usize = 64 * 1024;

        if offset + size > file_size {
            return Err(AudexError::InvalidData(
                "Delete region extends beyond file size".to_string(),
            ));
        }

        let delete_end = offset + size;
        let bytes_to_move = file_size - delete_end;

        // Move data after the deleted region to fill the gap (forward order).
        let mut moved = 0u64;
        let mut buf = vec![0u8; buffer_size];
        while moved < bytes_to_move {
            let chunk = std::cmp::min(bytes_to_move - moved, buffer_size as u64) as usize;
            file.seek(SeekFrom::Start(delete_end + moved))?;
            file.read_exact(&mut buf[..chunk])?;
            file.seek(SeekFrom::Start(offset + moved))?;
            file.write_all(&buf[..chunk])?;
            moved += chunk as u64;
        }

        // Seek to the new logical end.
        let new_total = file_size - size;
        file.seek(SeekFrom::Start(new_total))?;
        file.flush()?;
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
        let size = tag_data.len() as u32;
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

    /// Insert new ID3 chunk into IFF file using a `dyn ReadWriteSeek` handle.
    fn insert_id3_chunk_writer(
        file: &mut dyn ReadWriteSeek,
        aiff_file: &mut AIFFFile,
        id3_data: Vec<u8>,
    ) -> Result<()> {
        // Find a good place to insert the ID3 chunk - typically before "SSND" chunk
        let insert_offset = if let Some(ssnd_chunk) = aiff_file.find_chunk("SSND") {
            ssnd_chunk.offset // Insert right before sound data chunk
        } else {
            // No SSND chunk found, append at end
            file.seek(SeekFrom::End(0))?;
            file.stream_position()?
        };

        // Calculate chunk size with padding
        let data_size = id3_data.len();
        let padding_size = if data_size % 2 == 1 { 1 } else { 0 };
        let total_chunk_size = 8 + data_size + padding_size; // 8 bytes header + data + padding

        // Insert space for the new chunk
        Self::insert_bytes_writer(file, total_chunk_size as u64, insert_offset)?;

        // Write the new ID3 chunk at the inserted position
        file.seek(SeekFrom::Start(insert_offset))?;
        // Validate that the data size fits in a u32 IFF chunk header field.
        let data_size_u32 = u32::try_from(data_size).map_err(|_| {
            AudexError::InvalidData("ID3 chunk data size exceeds u32::MAX".to_string())
        })?;

        file.write_all(b"ID3 ")?; // Chunk ID (4 bytes)
        file.write_all(&data_size_u32.to_be_bytes())?; // Chunk size (4 bytes, big-endian for IFF)
        file.write_all(&id3_data)?; // Chunk data

        // Write padding byte if needed
        if padding_size > 0 {
            file.write_all(&[0])?;
        }

        // Update FORM file size header (checked to prevent silent corruption)
        let new_file_size = aiff_file
            .file_size
            .checked_add(u32::try_from(total_chunk_size).map_err(|_| {
                AudexError::InvalidData("ID3 total chunk size exceeds u32::MAX".to_string())
            })?)
            .ok_or_else(|| {
                AudexError::InvalidData(
                    "FORM file size would exceed u32::MAX after inserting ID3 chunk".to_string(),
                )
            })?;
        file.seek(SeekFrom::Start(4))?; // FORM size is at offset 4
        file.write_all(&new_file_size.to_be_bytes())?;

        // Update our cached IFF structure
        aiff_file.file_size = new_file_size;
        let new_chunk = AIFFChunk {
            id: "ID3 ".to_string(),
            size: data_size_u32.checked_add(8).ok_or_else(|| {
                AudexError::InvalidData("ID3 chunk total size overflows u32".to_string())
            })?,
            offset: insert_offset,
            data_offset: insert_offset + 8,
            data_size: data_size_u32,
        };

        // Insert the chunk in the correct position in our vector
        let insert_index = aiff_file
            .chunks
            .iter()
            .position(|chunk| chunk.offset >= insert_offset)
            .unwrap_or(aiff_file.chunks.len());
        aiff_file.chunks.insert(insert_index, new_chunk);

        Ok(())
    }

    /// Remove ID3 chunk from AIFF data via a writer handle.
    ///
    /// Deletes the ID3 chunk bytes in-place and updates the FORM header size.
    fn remove_id3_chunk_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        if let Some(ref aiff_file) = self.aiff_file {
            if let Some(chunk) = aiff_file.chunks.iter().find(|c| c.id == "ID3 ") {
                let pad = if chunk.data_size % 2 == 1 { 1u64 } else { 0 };
                let total_size = 8 + chunk.data_size as u64 + pad; // 8 = id(4) + size(4)
                let chunk_offset = chunk.offset;
                let old_form_size = aiff_file.file_size;
                let file_size = writer.seek(SeekFrom::End(0))?;
                let new_total = file_size.checked_sub(total_size).ok_or_else(|| {
                    AudexError::InvalidData(
                        "ID3 chunk size exceeds writer length during removal".to_string(),
                    )
                })?;

                // Remove chunk bytes in-place
                Self::delete_bytes_writer(writer, total_size, chunk_offset)?;

                // Update FORM header size at offset 4 (big-endian).
                // Perform the subtraction in u64 to avoid truncating
                // total_size before the comparison. If the result does
                // not fit back into u32, the file is structurally invalid.
                let new_form_size_u64 =
                    (old_form_size as u64)
                        .checked_sub(total_size)
                        .ok_or_else(|| {
                            AudexError::InvalidData(
                                "ID3 chunk size exceeds FORM container size".to_string(),
                            )
                        })?;
                let new_form_size = u32::try_from(new_form_size_u64).map_err(|_| {
                    AudexError::InvalidData("New FORM size does not fit in u32".to_string())
                })?;
                writer.seek(SeekFrom::Start(4))?;
                writer.write_all(&new_form_size.to_be_bytes())?;
                crate::util::truncate_writer_dyn(writer, new_total)?;
                writer.flush()?;
            }
        }

        // Update internal representation
        if let Some(ref mut aiff_file) = self.aiff_file {
            aiff_file.chunks.retain(|chunk| chunk.id != "ID3 ");
        }

        Ok(())
    }
}

impl Default for AIFF {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "async")]
impl AIFF {
    /// Load AIFF file asynchronously
    ///
    /// Parses the file structure, extracts stream information from the COMM chunk,
    /// and loads any ID3 tags present in the file.
    ///
    /// # Arguments
    /// * `path` - Path to the AIFF file
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut aiff = AIFF::new();
        aiff.filename = Some(path.as_ref().to_string_lossy().to_string());

        aiff.parse_file_async(&mut file).await?;
        Ok(aiff)
    }

    /// Parse AIFF file structure asynchronously
    async fn parse_file_async(&mut self, file: &mut TokioFile) -> Result<()> {
        // Parse AIFF structure using IffFileAsync
        file.seek(SeekFrom::Start(0)).await?;
        let aiff_file = IffFileAsync::parse(file).await?;

        // Validate file type (accept both standard AIFF and compressed AIFC)
        if aiff_file.file_type != "AIFF" && aiff_file.file_type != "AIFC" {
            return Err(AudexError::AIFFError(
                "Expected AIFF or AIFC format".to_string(),
            ));
        }

        // Parse stream info from COMM chunk
        self.info = Self::parse_stream_info_async(&aiff_file, file).await?;

        // Parse ID3 tags from 'ID3 ' chunk
        self.tags = if let Some(id3_chunk) = aiff_file.find_chunk("ID3 ") {
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

        // Store IFF structure in sync format for later use
        self.aiff_file = Some(Self::convert_iff_to_aiff_file(&aiff_file));
        Ok(())
    }

    /// Convert IffFileAsync to AIFFFile structure
    fn convert_iff_to_aiff_file(iff: &IffFileAsync) -> AIFFFile {
        let chunks = iff
            .chunks
            .iter()
            .map(|chunk| AIFFChunk {
                id: chunk.id.clone(),
                size: chunk.size,
                offset: chunk.offset,
                data_offset: chunk.data_offset,
                data_size: chunk.data_size,
            })
            .collect();

        AIFFFile {
            file_type: iff.file_type.clone(),
            chunks,
            file_size: iff.file_size,
        }
    }

    /// Parse stream information from COMM chunk asynchronously
    async fn parse_stream_info_async(
        aiff: &IffFileAsync,
        file: &mut TokioFile,
    ) -> Result<AIFFStreamInfo> {
        // Find and read COMM chunk
        let comm_chunk = aiff
            .find_chunk("COMM")
            .ok_or_else(|| AudexError::AIFFError("No 'COMM' chunk found".to_string()))?;

        if comm_chunk.data_size < 18 {
            return Err(AudexError::AIFFError("COMM chunk too small".to_string()));
        }

        let comm_data = comm_chunk.read_data(file).await?;

        // Parse COMM chunk fields (big-endian)
        let channels = u16::from_be_bytes([comm_data[0], comm_data[1]]);
        let frame_count =
            u32::from_be_bytes([comm_data[2], comm_data[3], comm_data[4], comm_data[5]]);
        let sample_size = u16::from_be_bytes([comm_data[6], comm_data[7]]);

        // Parse IEEE 80-bit float for sample rate, with bounds checking
        let sample_rate_f64 = read_float(&comm_data[8..18])?;
        if sample_rate_f64 < 0.0 || sample_rate_f64 > u32::MAX as f64 {
            return Err(AudexError::AIFFError(format!(
                "Sample rate {} is out of valid range for u32",
                sample_rate_f64
            )));
        }
        let sample_rate = sample_rate_f64 as u32;

        if sample_rate == 0 {
            return Err(AudexError::AIFFError("Invalid sample rate".to_string()));
        }

        // Calculate duration and bitrate
        let length = Some(Duration::from_secs_f64(
            frame_count as f64 / sample_rate as f64,
        ));
        let bitrate = (channels as u32)
            .saturating_mul(sample_size as u32)
            .saturating_mul(sample_rate);

        Ok(AIFFStreamInfo {
            length,
            bitrate: Some(bitrate),
            channels,
            sample_rate,
            bits_per_sample: sample_size,
            sample_size,
            frame_count,
        })
    }

    /// Save ID3 tags to AIFF file asynchronously
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
        // Use ID3v2.3 by default for AIFF compatibility
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

        // Parse IFF structure
        let mut aiff_file = IffFileAsync::parse(&mut file).await?;

        // Validate file type (accept both standard AIFF and compressed AIFC)
        if aiff_file.file_type != "AIFF" && aiff_file.file_type != "AIFC" {
            return Err(AudexError::AIFFError(
                "Expected AIFF or AIFC format".to_string(),
            ));
        }

        // Find existing ID3 chunk
        let id3_chunk = aiff_file.find_chunk("ID3 ").cloned();

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
            // Validate that the new data size fits in a u32 IFF chunk field.
            let new_size = u32::try_from(new_id3_data.len()).map_err(|_| {
                AudexError::InvalidData("ID3 data size exceeds u32::MAX".to_string())
            })?;

            // Resize chunk if needed
            if old_size != new_size {
                resize_iff_chunk_async(&mut file, &existing_chunk, new_size).await?;

                // Update FORM file size with checked arithmetic to prevent
                // silent wrapping on overflow (matching the sync path)
                let old_padded = old_size + (old_size % 2);
                let new_padded = new_size + (new_size % 2);
                let size_diff = new_padded as i64 - old_padded as i64;
                let computed = (aiff_file.file_size as i64)
                    .checked_add(size_diff)
                    .ok_or_else(|| {
                        AudexError::InvalidData("FORM file size arithmetic overflow".to_string())
                    })?;
                let new_form_size = u32::try_from(computed).map_err(|_| {
                    AudexError::InvalidData("FORM file size does not fit in u32".to_string())
                })?;
                update_iff_file_size_async(&mut file, new_form_size).await?;
                aiff_file.file_size = new_form_size;
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
            // Insert new ID3 chunk before SSND or at end
            let insert_offset = if let Some(ssnd_chunk) = aiff_file.find_chunk("SSND") {
                ssnd_chunk.offset
            } else {
                file.seek(SeekFrom::End(0)).await?
            };

            // Validate that the data size fits in a u32 IFF chunk field.
            let data_size = u32::try_from(new_id3_data.len()).map_err(|_| {
                AudexError::InvalidData("ID3 chunk data size exceeds u32::MAX".to_string())
            })?;
            let padding = if data_size % 2 == 1 { 1 } else { 0 };
            let total_chunk_size = 8 + data_size + padding;

            // Insert space and write chunk
            insert_bytes_async(&mut file, total_chunk_size as u64, insert_offset, None).await?;

            file.seek(SeekFrom::Start(insert_offset)).await?;
            file.write_all(b"ID3 ").await?;
            file.write_all(&data_size.to_be_bytes()).await?;
            file.write_all(&new_id3_data).await?;

            if padding > 0 {
                file.write_all(&[0]).await?;
            }

            // Update FORM file size (checked to prevent silent corruption)
            let new_form_size = aiff_file
                .file_size
                .checked_add(total_chunk_size)
                .ok_or_else(|| {
                    AudexError::InvalidData(
                        "FORM file size would exceed u32::MAX after inserting ID3 chunk"
                            .to_string(),
                    )
                })?;
            update_iff_file_size_async(&mut file, new_form_size).await?;
            aiff_file.file_size = new_form_size;

            // Add chunk to structure
            let new_chunk = IffChunkAsync::new("ID3 ".to_string(), data_size, insert_offset)?;
            let insert_index = aiff_file
                .chunks
                .iter()
                .position(|c| c.offset >= insert_offset)
                .unwrap_or(aiff_file.chunks.len());
            aiff_file.chunks.insert(insert_index, new_chunk);
        }

        file.flush().await.map_err(AudexError::Io)?;

        // Update internal structure
        self.aiff_file = Some(Self::convert_iff_to_aiff_file(&aiff_file));
        Ok(())
    }

    /// Clear ID3 tags asynchronously
    ///
    /// Removes all ID3 tags from the file by deleting the ID3 chunk.
    pub async fn clear_async(&mut self) -> Result<()> {
        self.tags = None;

        // Remove ID3 chunk from file if present
        let has_id3_chunk = if let Some(ref aiff_file) = self.aiff_file {
            aiff_file.chunks.iter().any(|chunk| chunk.id == "ID3 ")
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
    /// Deletes the ID3 chunk bytes in-place and updates the FORM header size.
    async fn remove_id3_chunk_async(&mut self, filename: &str) -> Result<()> {
        use tokio::fs::OpenOptions;

        if let Some(ref aiff_file) = self.aiff_file {
            if let Some(chunk) = aiff_file.chunks.iter().find(|c| c.id == "ID3 ") {
                let pad = if chunk.data_size % 2 == 1 { 1u64 } else { 0 };
                let total_size = 8 + chunk.data_size as u64 + pad;
                let chunk_offset = chunk.offset;
                let old_form_size = aiff_file.file_size;

                // Open file for in-place modification
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(filename)
                    .await?;

                // Remove chunk bytes in-place
                delete_bytes_async(&mut file, total_size, chunk_offset, None).await?;

                // Update FORM header size at offset 4 (big-endian).
                // Perform the subtraction in u64 to avoid truncating
                // total_size before the comparison.
                let new_form_size_u64 =
                    (old_form_size as u64)
                        .checked_sub(total_size)
                        .ok_or_else(|| {
                            AudexError::InvalidData(
                                "ID3 chunk size exceeds FORM container size".to_string(),
                            )
                        })?;
                let new_form_size = u32::try_from(new_form_size_u64).map_err(|_| {
                    AudexError::InvalidData("New FORM size does not fit in u32".to_string())
                })?;
                file.seek(SeekFrom::Start(4)).await?;
                file.write_all(&new_form_size.to_be_bytes()).await?;
                file.flush().await?;
            }
        }

        // Update internal structure
        if let Some(ref mut aiff_file) = self.aiff_file {
            aiff_file.chunks.retain(|chunk| chunk.id != "ID3 ");
        }

        Ok(())
    }

    /// Delete AIFF file asynchronously
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        tokio::fs::remove_file(path).await?;
        Ok(())
    }
}

impl FileType for AIFF {
    type Tags = ID3Tags;
    type Info = AIFFStreamInfo;

    fn format_id() -> &'static str {
        "AIFF"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing AIFF file");
        let mut file = std::fs::File::open(&path)?;
        let mut aiff = AIFF::new();
        aiff.filename = Some(path.as_ref().to_string_lossy().to_string());

        aiff.parse_file(&mut file)?;
        Ok(aiff)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing AIFF file from reader");
        let mut aiff = Self::new();
        let mut reader = reader;
        aiff.parse_file(&mut reader)?;
        Ok(aiff)
    }

    fn save(&mut self) -> Result<()> {
        debug_event!("saving AIFF metadata");
        let filename = self
            .filename
            .clone()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        self.save_to_file(&filename)
    }

    fn clear(&mut self) -> Result<()> {
        self.tags = None;

        // Remove ID3 chunk from AIFF file if present
        let has_id3_chunk = if let Some(ref aiff_file) = self.aiff_file {
            aiff_file.chunks.iter().any(|chunk| chunk.id == "ID3 ")
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

    fn save_to_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        self.save_to_writer_impl(writer, 3, Some("/".to_string()))
    }

    fn clear_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        self.tags = None;

        let has_id3_chunk = if let Some(ref aiff_file) = self.aiff_file {
            aiff_file.chunks.iter().any(|chunk| chunk.id == "ID3 ")
        } else {
            false
        };

        if has_id3_chunk {
            self.remove_id3_chunk_writer(writer)?;
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
            return Err(AudexError::AIFFError("ID3 tag already exists".to_string()));
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

        // Check for FORM + AIFF signature
        if header.len() >= 12 && &header[0..4] == b"FORM" && &header[8..12] == b"AIFF" {
            score += 10;
        }

        // Check file extensions
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".aiff") {
            score += 3;
        } else if lower_filename.ends_with(".aif") {
            score += 2;
        } else if lower_filename.ends_with(".aifc") {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/aiff", "audio/x-aiff"]
    }
}

/// Standalone functions for AIFF operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut aiff = AIFF::load(path)?;
    aiff.clear()
}

/// Clear ID3 tags from AIFF file asynchronously
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut aiff = AIFF::load_async(path).await?;
    aiff.clear_async().await
}

/// Open AIFF file asynchronously (alias)
#[cfg(feature = "async")]
pub async fn open_async<P: AsRef<Path>>(path: P) -> Result<AIFF> {
    AIFF::load_async(path).await
}
