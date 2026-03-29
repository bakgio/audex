//! Resource Interchange File Format (RIFF) container support
//!
//! This module provides comprehensive support for the RIFF (Resource Interchange File Format)
//! container format, which is a generic container format for storing data in tagged chunks.
//! RIFF is the little-endian variant of IFF (Interchange File Format) and is used by many
//! multimedia file formats.
//!
//! # Container Format
//!
//! RIFF is a chunk-based file format where data is organized into a hierarchical structure:
//!
//! - **Chunks**: Basic units of data, each with a 4-character ID and size field
//! - **Container chunks**: Special chunks (RIFF, LIST) that contain other chunks
//! - **Data chunks**: Leaf chunks containing actual data
//!
//! ## Chunk Structure
//!
//! Each RIFF chunk consists of:
//! - **ID** (4 bytes): Four-character identifier (e.g., "WAVE", "fmt ", "data")
//! - **Size** (4 bytes): Data size in bytes (little-endian)
//! - **Data** (variable): Chunk payload
//! - **Padding** (0-1 bytes): Optional padding byte to ensure 16-bit alignment
//!
//! ## File Structure
//!
//! A typical RIFF file has this structure:
//! ```text
//! RIFF
//! ├─ File Type (4 bytes, e.g., "WAVE")
//! ├─ Chunk 1 (e.g., "fmt ")
//! ├─ Chunk 2 (e.g., "data")
//! └─ Chunk N
//! ```
//!
//! # Common File Formats
//!
//! RIFF is used as the container for many common formats:
//! - **WAV** (WAVE): Waveform audio format
//! - **AVI** (AVI ): Audio Video Interleave
//! - **WEBP** (WEBP): WebP image format
//! - **RMI** (RMID): RIFF MIDI files
//! - **ANI** (ACON): Animated cursor files
//!
//! # Differences from IFF
//!
//! RIFF differs from IFF primarily in byte order:
//! - **RIFF**: Little-endian byte order (Intel format)
//! - **IFF**: Big-endian byte order (Motorola format)
//!
//! # Examples
//!
//! ## Reading chunks from a RIFF file
//!
//! ```no_run
//! use audex::riff::RiffFile;
//! use std::fs::File;
//! use std::rc::Rc;
//! use std::cell::RefCell;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Open a RIFF file
//! let file = File::open("/path/to/file.wav")?;
//! let file_rc = Rc::new(RefCell::new(file));
//! let mut riff = RiffFile::new(file_rc)?;
//!
//! // Check file type
//! println!("File type: {:?}", riff.file_type);
//!
//! // Check if specific chunk exists
//! if riff.contains("fmt ")? {
//!     println!("Format chunk found");
//!
//!     // Read chunk data
//!     let fmt_chunk = riff.get_chunk("fmt ")?;
//!     let data = fmt_chunk.read()?;
//!     println!("Format chunk size: {} bytes", data.len());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Working with chunk data
//!
//! ```no_run
//! use audex::riff::RiffFile;
//! use std::fs::File;
//! use std::rc::Rc;
//! use std::cell::RefCell;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let file = File::options().read(true).write(true).open("/path/to/file.wav")?;
//! let file_rc = Rc::new(RefCell::new(file));
//! let mut riff = RiffFile::new(file_rc)?;
//!
//! // Insert a new chunk
//! let data = vec![1, 2, 3, 4, 5, 6];
//! let chunk = riff.insert_chunk("TEST", Some(&data))?;
//! println!("Inserted chunk at offset {}", chunk.offset());
//!
//! // Modify chunk data
//! let new_data = vec![7, 8, 9];
//! let mut chunk = riff.get_chunk("TEST")?;
//! chunk.resize(new_data.len() as u32)?;
//! chunk.write(&new_data)?;
//!
//! // Delete a chunk
//! riff.delete_chunk("TEST")?;
//! # Ok(())
//! # }
//! ```
//!
//! # References
//!
//! - [Multimedia Programming Interface and Data Specifications 1.0](https://www.aelius.com/njh/wavemetatools/doc/riffmci.pdf)
//! - [Microsoft RIFF Specification](https://learn.microsoft.com/en-us/windows/win32/xaudio2/resource-interchange-file-format--riff-)

use crate::iff::{IffError, InvalidChunk, assert_valid_chunk_id, is_valid_chunk_id};
use byteorder::{LittleEndian, WriteBytesExt};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::rc::{Rc, Weak};

/// Generic RIFF chunk implementation
///
/// Represents a single RIFF chunk, which is the fundamental data unit in RIFF files.
/// Each chunk has a 4-character identifier, size information, and payload data.
///
/// # Structure
///
/// A RIFF chunk consists of:
/// - **Header** (8 bytes): 4-byte ID + 4-byte little-endian size
/// - **Data** (variable): Chunk payload
/// - **Padding** (0-1 bytes): Optional padding byte for 16-bit alignment
///
/// # Fields
///
/// - **id**: Four-character chunk identifier (e.g., "fmt ", "data", "LIST")
/// - **data_size**: Size of chunk data in bytes (excludes header and padding)
/// - **offset**: File offset where the chunk begins (includes header)
/// - **data_offset**: File offset where chunk data begins (after header)
/// - **size**: Total chunk size including header, data, and padding
///
/// # Examples
///
/// ```rust
/// use audex::riff::RiffChunk;
///
/// // Create a new chunk
/// let chunk = RiffChunk::new("data", 1000, 100).unwrap();
/// assert_eq!(chunk.id(), "data");
/// assert_eq!(chunk.data_size(), 1000);
/// assert_eq!(chunk.offset(), 100);
/// assert_eq!(chunk.data_offset(), 108); // offset + 8-byte header
/// assert_eq!(chunk.padding(), 0); // even size, no padding needed
///
/// // Odd-sized chunk requires padding
/// let odd_chunk = RiffChunk::new("TEST", 1001, 200).unwrap();
/// assert_eq!(odd_chunk.padding(), 1); // odd size needs padding
/// assert_eq!(odd_chunk.size(), 1010); // 8 (header) + 1001 (data) + 1 (padding)
/// ```
#[derive(Debug, Clone)]
pub struct RiffChunk {
    /// Four-character chunk identifier (e.g., "fmt ", "data")
    pub id: String,

    /// Size of chunk data in bytes (excludes header and padding)
    pub data_size: u32,

    /// File offset where the chunk begins (includes header)
    pub offset: u64,

    /// File offset where chunk data begins (after 8-byte header)
    pub data_offset: u64,

    /// Total chunk size including header, data, and padding
    pub size: u32,
}

impl RiffChunk {
    /// Creates a new `RiffChunk` with the given ID, data size, and file offset.
    ///
    /// Returns an error if `8 + data_size + padding` would overflow a u32,
    /// preventing silently incorrect size headers from being written to disk.
    pub fn new(id: &str, data_size: u32, offset: u64) -> std::result::Result<Self, IffError> {
        let data_offset = offset.checked_add(8).ok_or_else(|| {
            IffError(format!(
                "RIFF chunk data offset overflow at offset={}",
                offset
            ))
        })?;
        let padding = data_size % 2;
        let size = data_size.checked_add(8 + padding).ok_or_else(|| {
            IffError(format!(
                "RIFF chunk size overflow: data_size={} + header + padding exceeds u32",
                data_size
            ))
        })?;

        Ok(Self {
            id: id.to_string(),
            data_size,
            offset,
            data_offset,
            size,
        })
    }

    /// Alias for `new` — both perform checked arithmetic and return an error on overflow.
    pub fn try_new(id: &str, data_size: u32, offset: u64) -> crate::Result<Self> {
        Self::new(id, data_size, offset).map_err(|e| crate::AudexError::InvalidData(e.to_string()))
    }

    /// Returns the chunk's four-character identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the size of the chunk's data in bytes (excludes header and padding).
    pub fn data_size(&self) -> u32 {
        self.data_size
    }

    /// Returns the file offset where this chunk begins (including header).
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Returns the file offset where the chunk's data begins (after the 8-byte header).
    pub fn data_offset(&self) -> u64 {
        self.data_offset
    }

    /// Returns the total chunk size including header, data, and padding.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Parses an 8-byte RIFF chunk header, returning the chunk ID and data size (little-endian).
    pub fn parse_header(header: &[u8]) -> std::result::Result<(String, u32), IffError> {
        if header.len() < 8 {
            return Err(IffError("Header too short".to_string()));
        }

        let id_bytes = &header[0..4];
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);

        // Convert ID bytes to string - preserve exact 4-char ID (trailing spaces are significant in RIFF)
        let id = String::from_utf8(id_bytes.to_vec())
            .map_err(|_| IffError("Invalid chunk ID encoding".to_string()))?;

        Ok((id, size))
    }

    /// Returns the number of padding bytes (0 or 1) needed for 16-bit alignment.
    pub fn padding(&self) -> u32 {
        self.data_size % 2
    }

    /// Reads and returns the chunk's data payload from the file.
    pub fn read(&self, file: &mut File) -> std::result::Result<Vec<u8>, IffError> {
        // Enforce the library-wide tag allocation ceiling
        let limits = crate::limits::ParseLimits::default();
        if (self.data_size as u64) > limits.max_tag_size {
            return Err(IffError(format!(
                "RIFF chunk data size {} exceeds global limit {} bytes",
                self.data_size, limits.max_tag_size
            )));
        }
        file.seek(SeekFrom::Start(self.data_offset))?;
        let mut data = vec![0u8; self.data_size as usize];
        file.read_exact(&mut data)?;
        Ok(data)
    }

    /// Writes data to the chunk's payload region, including any necessary padding bytes.
    pub fn write(&self, file: &mut File, data: &[u8]) -> std::result::Result<(), IffError> {
        if data.len() > self.data_size as usize {
            return Err(IffError("Data too large for chunk".to_string()));
        }

        file.seek(SeekFrom::Start(self.data_offset))?;
        file.write_all(data)?;

        // Write padding bytes if needed
        let padding = self.padding();
        if padding > 0 {
            file.seek(SeekFrom::Start(self.data_offset + self.data_size as u64))?;
            file.write_all(&vec![0u8; padding as usize])?;
        }

        Ok(())
    }

    /// Resizes the chunk's data region in the file, returning the size change and new total size.
    pub fn resize(
        &mut self,
        file: &mut File,
        new_data_size: u32,
    ) -> std::result::Result<(i64, u32), IffError> {
        let old_size = self.get_actual_data_size(file)?;
        let padding = new_data_size % 2;
        let new_total_size = new_data_size.checked_add(padding).ok_or_else(|| {
            IffError(format!(
                "RIFF chunk resize overflow: data_size={} + padding exceeds u32",
                new_data_size
            ))
        })?;

        // Calculate size change for parent update
        let size_change = new_total_size as i64 - old_size as i64;

        // Resize the file region
        crate::iff::resize_bytes(file, old_size, new_total_size, self.data_offset)?;
        file.flush()?;

        // Update our size tracking
        self.data_size = new_data_size;
        let new_padding = self.padding();
        self.size = 8u32
            .checked_add(self.data_size)
            .and_then(|s| s.checked_add(new_padding))
            .ok_or_else(|| {
                IffError(format!(
                    "RIFF chunk total size overflow: 8 + {} + {} exceeds u32",
                    self.data_size, new_padding
                ))
            })?;

        // Update the size field in the file
        file.seek(SeekFrom::Start(self.offset + 4))?;
        file.write_u32::<LittleEndian>(self.data_size)?;

        Ok((size_change, self.size))
    }

    /// Deletes this chunk's bytes (header, data, and padding) from the file.
    pub fn delete(&mut self, file: &mut File) -> std::result::Result<(), IffError> {
        // Delete chunk from file
        crate::iff::delete_bytes(file, self.size as u64, self.offset)?;
        file.flush()?;
        Ok(())
    }

    fn get_actual_data_size(&self, file: &mut File) -> std::result::Result<u32, IffError> {
        file.seek(SeekFrom::End(0))?;
        let file_size = file.stream_position()?;

        // Compute in u64 to avoid wrapping when data_size is u32::MAX
        let expected_size = self.data_size as u64 + self.padding() as u64;
        let max_size_possible = file_size.saturating_sub(self.data_offset);
        let actual = std::cmp::min(expected_size, max_size_possible);
        Ok(u32::try_from(actual).unwrap_or(u32::MAX))
    }
}

/// A RIFF container chunk containing other chunks (LIST or RIFF)
///
/// Represents a special chunk type that can contain other chunks, creating a hierarchical
/// structure. Container chunks are identified by the "RIFF" or "LIST" chunk IDs.
///
/// # Structure
///
/// A container chunk has this structure:
/// ```text
/// ┌─────────────────────┐
/// │ Chunk Header (8 bytes)
/// │  - ID (RIFF/LIST)   │
/// │  - Size             │
/// ├─────────────────────┤
/// │ Name/Type (4 bytes) │
/// │  (e.g., "WAVE")     │
/// ├─────────────────────┤
/// │ Subchunk 1          │
/// ├─────────────────────┤
/// │ Subchunk 2          │
/// ├─────────────────────┤
/// │ ...                 │
/// └─────────────────────┘
/// ```
///
/// # Fields
///
/// - **base**: The underlying RiffChunk structure
/// - **name**: Container type name (e.g., "WAVE", "INFO")
/// - **name_size**: Size of the name field in bytes (typically 4)
/// - **subchunks**: Map of subchunks indexed by their IDs
/// - **subchunks_loaded**: Whether subchunks have been loaded from file
///
/// # Examples
///
/// ```no_run
/// use audex::riff::{RiffFile, RiffListChunk};
/// use std::fs::File;
/// use std::rc::Rc;
/// use std::cell::RefCell;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Open a RIFF file
/// let file = File::open("/path/to/file.wav")?;
/// let file_rc = Rc::new(RefCell::new(file));
/// let mut riff = RiffFile::new(file_rc)?;
///
/// // Get root container
/// let root = riff.get_root()?;
/// println!("Container ID: {}", root.id());
/// println!("Container name: {:?}", root.name);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct RiffListChunk {
    /// The underlying chunk structure
    pub base: RiffChunk,

    /// Container type name (e.g., "WAVE" for WAV files)
    pub name: Option<String>,

    /// Size of the name field in bytes
    pub name_size: usize,

    /// Map of subchunks indexed by chunk ID
    subchunks: HashMap<String, RiffChunk>,

    /// Whether subchunks have been loaded from file
    subchunks_loaded: bool,
}

impl RiffListChunk {
    /// Creates a new container chunk. The `id` must be `"RIFF"` or `"LIST"`.
    pub fn new(id: &str, data_size: u32, offset: u64) -> std::result::Result<Self, IffError> {
        if id != "RIFF" && id != "LIST" {
            return Err(InvalidChunk(format!("Expected RIFF or LIST chunk, got {}", id)).into());
        }

        let base = RiffChunk::new(id, data_size, offset)?;
        Ok(Self {
            base,
            name: None,
            name_size: 4,
            subchunks: HashMap::new(),
            subchunks_loaded: false,
        })
    }

    /// Reads the container's type name (e.g., `"WAVE"`) from the file and stores it.
    pub fn init_container(
        &mut self,
        file: &mut File,
        name_size: usize,
    ) -> std::result::Result<(), IffError> {
        if self.base.data_size < name_size as u32 {
            return Err(InvalidChunk(format!("Container chunk data size < {}", name_size)).into());
        }

        self.name_size = name_size;

        // Read the container name if name_size > 0
        if name_size > 0 {
            file.seek(SeekFrom::Start(self.base.data_offset))?;
            let mut name_bytes = vec![0u8; name_size];
            file.read_exact(&mut name_bytes)?;

            match String::from_utf8(name_bytes) {
                Ok(name) => self.name = Some(name.trim_end().to_string()),
                Err(_) => return Err(IffError("Invalid container name encoding".to_string())),
            }
        } else {
            self.name = None;
        }

        Ok(())
    }

    /// Loads all subchunks from the file, caching them for subsequent access.
    pub fn load_subchunks(&mut self, file: &mut File) -> std::result::Result<(), IffError> {
        if self.subchunks_loaded {
            return Ok(());
        }

        self.subchunks.clear();
        // Subchunks start after the chunk header (8 bytes) plus the container
        // name field. Use the chunk's own offset so this works correctly even
        // when the RIFF container is embedded inside a composite format.
        let mut next_offset = self.base.offset + 8 + self.name_size as u64;

        // Calculate the end of the container using checked arithmetic to
        // prevent overflow with malformed or adversarial chunk sizes.
        let container_end = self
            .base
            .offset
            .checked_add(8)
            .and_then(|v| v.checked_add(self.base.data_size as u64))
            .ok_or_else(|| IffError("RIFF container end offset overflow".into()))?;

        while next_offset < container_end {
            file.seek(SeekFrom::Start(next_offset))?;

            let mut header = [0u8; 8];
            if file.read_exact(&mut header).is_err() {
                break; // End of file or invalid chunk
            }

            let (id, data_size) = RiffChunk::parse_header(&header)?;

            if !is_valid_chunk_id(&id) {
                break; // Invalid chunk ID
            }

            let chunk = match RiffChunk::new(&id, data_size, next_offset) {
                Ok(c) => c,
                Err(_) => break, // Chunk header overflows — stop scanning
            };
            let chunk_end = next_offset + chunk.size as u64;

            // Guard against zero-advancement which would cause an infinite loop.
            // With the 8-byte header included in chunk.size, this cannot happen
            // for valid headers, but we guard against it defensively.
            if chunk_end <= next_offset {
                break;
            }

            // Keep the first occurrence of each chunk ID to avoid silent data
            // loss when the same ID appears more than once.
            if self.subchunks.contains_key(&id) {
                warn_event!(chunk_id = %id, "Duplicate RIFF chunk ID; keeping first occurrence");
            } else {
                self.subchunks.insert(id.clone(), chunk);
            }

            // Move to next chunk, aligned to word boundary
            next_offset = chunk_end;
            if next_offset % 2 == 1 {
                next_offset += 1; // Skip padding byte
            }
        }
        self.subchunks_loaded = true;
        Ok(())
    }

    /// Returns whether a subchunk with the given ID exists in this container.
    pub fn contains(&mut self, file: &mut File, id: &str) -> std::result::Result<bool, IffError> {
        assert_valid_chunk_id(id)?;
        self.load_subchunks(file)?;
        Ok(self.subchunks.contains_key(id))
    }

    /// Returns an immutable reference to the subchunk with the given ID.
    pub fn get_chunk(
        &mut self,
        file: &mut File,
        id: &str,
    ) -> std::result::Result<&RiffChunk, IffError> {
        assert_valid_chunk_id(id)?;
        self.load_subchunks(file)?;
        self.subchunks
            .get(id)
            .ok_or_else(|| IffError(format!("No '{}' chunk found", id)))
    }

    /// Returns a mutable reference to the subchunk with the given ID.
    pub fn get_chunk_mut(
        &mut self,
        file: &mut File,
        id: &str,
    ) -> std::result::Result<&mut RiffChunk, IffError> {
        assert_valid_chunk_id(id)?;
        self.load_subchunks(file)?;
        self.subchunks
            .get_mut(id)
            .ok_or_else(|| IffError(format!("No '{}' chunk found", id)))
    }

    /// Inserts a new subchunk with the given ID and optional data at the end of this container.
    pub fn insert_chunk(
        &mut self,
        file: &mut File,
        id: &str,
        data: Option<&[u8]>,
    ) -> std::result::Result<(), IffError> {
        if !is_valid_chunk_id(id) {
            return Err(IffError("Invalid IFF chunk ID".to_string()));
        }

        // Calculate insertion point
        let actual_data_size = self.get_actual_data_size(file)?;
        let next_offset = self.base.data_offset + actual_data_size as u64;

        // Calculate new chunk size
        let raw_len = data.map(|d| d.len()).unwrap_or(0);
        let data_size = u32::try_from(raw_len).map_err(|_| {
            IffError(format!(
                "RIFF insert_chunk data length {} exceeds u32::MAX",
                raw_len
            ))
        })?;
        let padding = data_size % 2;
        // Use checked arithmetic to prevent silent wraparound on large data sizes
        let chunk_size = 8u32
            .checked_add(data_size)
            .and_then(|s| s.checked_add(padding))
            .ok_or_else(|| {
                IffError(format!(
                    "chunk size overflow: 8 + {} + {} exceeds u32::MAX",
                    data_size, padding
                ))
            })?;

        // Insert space in file
        crate::iff::insert_bytes(file, chunk_size, next_offset)?;

        // Write chunk header
        file.seek(SeekFrom::Start(next_offset))?;

        // Write ID (4 bytes, padded with spaces)
        let id_bytes = id.as_bytes();
        let mut padded_id = [b' '; 4];
        let copy_len = std::cmp::min(id_bytes.len(), 4);
        padded_id[..copy_len].copy_from_slice(&id_bytes[..copy_len]);
        file.write_all(&padded_id)?;

        // Write size (4 bytes, little-endian)
        file.write_u32::<LittleEndian>(data_size)?;

        // Create new chunk
        let chunk = RiffChunk::new(id, data_size, next_offset)?;

        // Write data if provided
        if let Some(data) = data {
            chunk.write(file, data)?;
        }

        // Update our size - for RIFF containers, size is just header + data_size
        self.base.data_size = self
            .base
            .data_size
            .checked_add(chunk_size)
            .ok_or_else(|| IffError("Container size would overflow u32".to_string()))?;
        self.base.size = 8u32.checked_add(self.base.data_size).ok_or_else(|| {
            IffError("Container total size overflow: 8 + data_size exceeds u32".to_string())
        })?;

        // Update size in file
        file.seek(SeekFrom::Start(self.base.offset + 4))?;
        file.write_u32::<LittleEndian>(self.base.data_size)?;

        // Add to subchunks if loaded
        if self.subchunks_loaded {
            self.subchunks.insert(id.to_string(), chunk);
        }

        file.flush()?;
        Ok(())
    }

    /// Deletes the subchunk with the given ID from the file and updates the container size.
    pub fn delete_chunk(&mut self, file: &mut File, id: &str) -> std::result::Result<(), IffError> {
        assert_valid_chunk_id(id)?;
        self.load_subchunks(file)?;

        let chunk = self
            .subchunks
            .get(id)
            .ok_or_else(|| IffError(format!("No '{}' chunk found", id)))?;

        let chunk_offset = chunk.offset;
        let chunk_size = chunk.size;

        // Delete from file
        crate::iff::delete_bytes(file, chunk_size as u64, chunk_offset)?;
        file.flush()?;

        // Remove from subchunks and update size
        self.subchunks.remove(id);
        self.base.data_size = self
            .base
            .data_size
            .checked_sub(chunk_size)
            .ok_or_else(|| IffError("Chunk size exceeds container data size".to_string()))?;
        self.base.size = 8u32.checked_add(self.base.data_size).ok_or_else(|| {
            IffError("Container total size overflow: 8 + data_size exceeds u32".to_string())
        })?;

        // Update size in file
        file.seek(SeekFrom::Start(self.base.offset + 4))?;
        file.write_u32::<LittleEndian>(self.base.data_size)?;

        // Invalidate subchunks cache so offsets get reloaded
        self.subchunks_loaded = false;

        Ok(())
    }

    /// Updates the container's size in memory and on disk after a subchunk has been resized.
    pub fn update_after_chunk_resize(
        &mut self,
        file: &mut File,
        _chunk_id: &str,
        size_change: i64,
    ) -> std::result::Result<(), IffError> {
        // Invalidate subchunks cache to force reload with correct offsets
        self.subchunks_loaded = false;

        // Update container size with overflow protection
        if size_change != 0 {
            let new_size = self.base.data_size as i64 + size_change;
            if new_size < 0 || new_size > u32::MAX as i64 {
                return Err(IffError(format!(
                    "Chunk resize would produce invalid container size: {}",
                    new_size
                )));
            }
            self.base.data_size = new_size as u32;
            self.base.size = 8u32.checked_add(self.base.data_size).ok_or_else(|| {
                IffError("Container total size overflow: 8 + data_size exceeds u32".to_string())
            })?;

            // Write updated size to file
            file.seek(SeekFrom::Start(self.base.offset + 4))?;
            file.write_u32::<LittleEndian>(self.base.data_size)?;
        }

        Ok(())
    }

    fn get_actual_data_size(&self, file: &mut File) -> std::result::Result<u32, IffError> {
        file.seek(SeekFrom::End(0))?;
        let file_size = file.stream_position()?;

        // Compute in u64 to avoid wrapping when data_size is u32::MAX
        let expected_size = self.base.data_size as u64 + self.base.padding() as u64;
        let max_size_possible = file_size.saturating_sub(self.base.data_offset);
        let actual = std::cmp::min(expected_size, max_size_possible);
        Ok(u32::try_from(actual).unwrap_or(u32::MAX))
    }

    /// Returns the container's chunk ID (`"RIFF"` or `"LIST"`).
    pub fn id(&self) -> &str {
        &self.base.id
    }

    /// Returns the total size of this container chunk including its 8-byte header.
    pub fn size(&self) -> u32 {
        self.base.size
    }
}

/// Mutable wrapper around a [`RiffChunk`] that provides read/write/resize operations
pub struct MutableChunk {
    chunk: RiffChunk,
    fileobj: Rc<RefCell<File>>,
    parent_root: Weak<RefCell<RiffListChunk>>,
}

impl MutableChunk {
    fn new(
        chunk: RiffChunk,
        fileobj: Rc<RefCell<File>>,
        parent_root: Weak<RefCell<RiffListChunk>>,
    ) -> Self {
        Self {
            chunk,
            fileobj,
            parent_root,
        }
    }

    /// Returns the chunk's four-character identifier.
    pub fn id(&self) -> &str {
        &self.chunk.id
    }

    /// Returns the size of the chunk's data in bytes.
    pub fn data_size(&self) -> u32 {
        // Return the updated cached value after resize
        self.chunk.data_size
    }

    /// Returns the file offset where this chunk begins, reloading from the parent if possible.
    pub fn offset(&self) -> u64 {
        // Reload chunk from parent to get updated offset
        if let Some(parent_root) = self.parent_root.upgrade() {
            if let Ok(mut file) = self.fileobj.try_borrow_mut() {
                if let Ok(mut root) = parent_root.try_borrow_mut() {
                    if root.load_subchunks(&mut file).is_ok() {
                        if let Ok(chunk) = root.get_chunk(&mut file, &self.chunk.id) {
                            return chunk.offset;
                        }
                    }
                }
            }
        }
        // Fall back to cached value if we can't reload
        self.chunk.offset
    }

    /// Returns the file offset where the chunk's data begins, reloading from the parent if possible.
    pub fn data_offset(&self) -> u64 {
        // Reload chunk from parent to get updated offset
        if let Some(parent_root) = self.parent_root.upgrade() {
            if let Ok(mut file) = self.fileobj.try_borrow_mut() {
                if let Ok(mut root) = parent_root.try_borrow_mut() {
                    if root.load_subchunks(&mut file).is_ok() {
                        if let Ok(chunk) = root.get_chunk(&mut file, &self.chunk.id) {
                            return chunk.data_offset;
                        }
                    }
                }
            }
        }
        // Fall back to cached value if we can't reload
        self.chunk.data_offset
    }

    /// Returns the total chunk size including header, data, and padding.
    pub fn size(&self) -> u32 {
        self.chunk.size
    }

    /// Reads and returns the chunk's data payload from the file.
    pub fn read(&self) -> std::result::Result<Vec<u8>, IffError> {
        let mut file = self.fileobj.borrow_mut();
        self.chunk.read(&mut file)
    }

    /// Writes data to the chunk's payload region.
    pub fn write(&mut self, data: &[u8]) -> std::result::Result<(), IffError> {
        let mut file = self.fileobj.borrow_mut();
        self.chunk.write(&mut file, data)
    }

    /// Resizes the chunk and updates the parent container's size accordingly.
    pub fn resize(&mut self, new_data_size: u32) -> std::result::Result<(), IffError> {
        let (size_change, new_chunk_size) = {
            let mut file = self.fileobj.borrow_mut();
            self.chunk.resize(&mut file, new_data_size)?
        };

        // Update the chunk's cached size
        self.chunk.size = new_chunk_size;
        self.chunk.data_size = new_data_size;

        // Update parent container
        if let Some(parent_root) = self.parent_root.upgrade() {
            let mut file = self.fileobj.borrow_mut();
            parent_root.borrow_mut().update_after_chunk_resize(
                &mut file,
                &self.chunk.id,
                size_change,
            )?;
        }

        Ok(())
    }

    /// Deletes this chunk's bytes from the file.
    pub fn delete(&mut self) -> std::result::Result<(), IffError> {
        let mut file = self.fileobj.borrow_mut();
        self.chunk.delete(&mut file)?;
        Ok(())
    }

    /// Returns the number of padding bytes (0 or 1) needed for 16-bit alignment.
    pub fn padding(&self) -> u32 {
        self.chunk.padding()
    }
}

/// Representation of a RIFF file
///
/// Provides high-level access to a RIFF file, managing the root container chunk
/// and providing methods for reading, writing, and manipulating chunks.
///
/// # Structure
///
/// A RIFF file always starts with a root "RIFF" chunk containing:
/// - **File type**: 4-character type identifier (e.g., "WAVE", "AVI ")
/// - **Subchunks**: One or more data or container chunks
///
/// # Fields
///
/// - **fileobj**: Reference-counted file handle for reading/writing
/// - **file_type**: The RIFF file type (e.g., "WAVE", "AVI ")
/// - **root**: Root container chunk containing all subchunks
///
/// # Examples
///
/// ## Opening and inspecting a RIFF file
///
/// ```no_run
/// use audex::riff::RiffFile;
/// use std::fs::File;
/// use std::rc::Rc;
/// use std::cell::RefCell;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let file = File::open("/path/to/file.wav")?;
/// let file_rc = Rc::new(RefCell::new(file));
/// let mut riff = RiffFile::new(file_rc)?;
///
/// // Check file type
/// match riff.file_type.as_deref() {
///     Some("WAVE") => println!("This is a WAV file"),
///     Some("AVI ") => println!("This is an AVI file"),
///     Some(other) => println!("Unknown RIFF type: {}", other),
///     None => println!("No file type"),
/// }
///
/// // Check for specific chunks
/// if riff.contains("fmt ")? {
///     println!("Format chunk present");
/// }
/// if riff.contains("data")? {
///     println!("Data chunk present");
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Modifying chunks
///
/// ```no_run
/// use audex::riff::RiffFile;
/// use std::fs::File;
/// use std::rc::Rc;
/// use std::cell::RefCell;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let file = File::options().read(true).write(true).open("/path/to/file.wav")?;
/// let file_rc = Rc::new(RefCell::new(file));
/// let mut riff = RiffFile::new(file_rc)?;
///
/// // Read a chunk
/// let chunk = riff.get_chunk("fmt ")?;
/// let data = chunk.read()?;
/// println!("Format chunk: {} bytes", data.len());
///
/// // Insert a new chunk
/// let metadata = b"Created with Audex";
/// riff.insert_chunk("INFO", Some(metadata))?;
///
/// // Delete a chunk
/// riff.delete_chunk("INFO")?;
/// # Ok(())
/// # }
/// ```
pub struct RiffFile {
    /// Reference-counted file handle
    fileobj: Rc<RefCell<File>>,

    /// RIFF file type (e.g., "WAVE", "AVI ")
    pub file_type: Option<String>,

    /// Root RIFF container chunk
    root: Rc<RefCell<RiffListChunk>>,
}

impl RiffFile {
    /// Opens and parses a RIFF file, validating the root chunk header and reading the file type.
    pub fn new(fileobj: Rc<RefCell<File>>) -> std::result::Result<Self, IffError> {
        let (id, data_size, file_type) = {
            let mut file = fileobj.borrow_mut();
            file.seek(SeekFrom::Start(0))?;

            // Parse the root chunk header (first 8 bytes)
            let mut header = [0u8; 8];
            file.read_exact(&mut header)?;

            let (id, data_size) = RiffChunk::parse_header(&header)?;

            if id != "RIFF" {
                return Err(
                    InvalidChunk(format!("Root chunk must be a RIFF chunk, got {}", id)).into(),
                );
            }

            // Read the file type (next 4 bytes)
            let mut type_bytes = [0u8; 4];
            file.read_exact(&mut type_bytes)?;
            let file_type = String::from_utf8(type_bytes.to_vec())
                .map_err(|_| IffError("Invalid file type encoding".to_string()))?;

            (id, data_size, Some(file_type))
        };

        // Create root chunk starting at offset 0, but subchunks start after the RIFF header + type (12 bytes)
        let mut root = RiffListChunk::new(&id, data_size, 0)?;
        root.name_size = 4; // The WAVE type takes 4 bytes
        root.name = file_type.clone();

        Ok(Self {
            fileobj,
            file_type,
            root: Rc::new(RefCell::new(root)),
        })
    }

    /// Returns a clone of the root container chunk.
    pub fn get_root(&self) -> std::result::Result<RiffListChunk, IffError> {
        Ok(self.root.borrow().clone())
    }

    /// Returns whether a chunk with the given ID exists in this file.
    pub fn contains(&mut self, id: &str) -> std::result::Result<bool, IffError> {
        let mut file = self.fileobj.borrow_mut();
        self.root.borrow_mut().contains(&mut file, id)
    }

    /// Returns a [`MutableChunk`] wrapper for the chunk with the given ID.
    pub fn get_chunk(&mut self, id: &str) -> std::result::Result<MutableChunk, IffError> {
        let chunk = {
            let mut file = self.fileobj.borrow_mut();
            let mut root = self.root.borrow_mut();
            root.load_subchunks(&mut file)?;
            root.get_chunk(&mut file, id)?.clone()
        };
        Ok(MutableChunk::new(
            chunk,
            self.fileobj.clone(),
            Rc::downgrade(&self.root),
        ))
    }

    /// Deletes the chunk with the given ID from the file.
    pub fn delete_chunk(&mut self, id: &str) -> std::result::Result<(), IffError> {
        let mut file = self.fileobj.borrow_mut();
        self.root.borrow_mut().delete_chunk(&mut file, id)
    }

    /// Inserts a new chunk with the given ID and optional data, returning a [`MutableChunk`] wrapper.
    pub fn insert_chunk(
        &mut self,
        id: &str,
        data: Option<&[u8]>,
    ) -> std::result::Result<MutableChunk, IffError> {
        {
            let mut file = self.fileobj.borrow_mut();
            self.root.borrow_mut().insert_chunk(&mut file, id, data)?;
        }
        // Return the newly created chunk as a mutable wrapper
        self.get_chunk(id)
    }
}
