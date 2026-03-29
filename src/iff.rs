//! IFF (Interchange File Format) container support
//!
//! This module provides comprehensive support for the IFF (Interchange File Format) container,
//! which is a chunk-based file format used by AIFF, 8SVX, and other multimedia formats.
//! IFF is the big-endian variant that uses Motorola byte order, while RIFF is the
//! little-endian variant (Intel byte order) used by WAV and AVI files.
//!
//! # Container Format
//!
//! IFF is a hierarchical chunk-based format where each chunk has:
//! - **4-byte ID**: FOURCC identifier (e.g., "FORM", "COMM", "SSND")
//! - **4-byte size**: Big-endian data size
//! - **Variable data**: Chunk payload
//! - **Optional padding**: Byte padding for 16-bit alignment
//!
//! ## Chunk Types
//!
//! - **FORM**: Container chunk (file header)
//! - **LIST**: List container chunk
//! - **CAT **: Concatenation container
//! - **Data chunks**: Format-specific chunks (COMM, SSND, etc.)
//!
//! # Common IFF Formats
//!
//! - **AIFF**: Audio Interchange File Format
//! - **8SVX**: 8-bit sampled voice
//! - **ILBM**: InterLeaved BitMap (images)
//!
//! # Differences from RIFF
//!
//! - **IFF**: Big-endian byte order (Motorola/network order)
//! - **RIFF**: Little-endian byte order (Intel order)
//!
//! Both formats share the same structural concepts but differ in byte ordering.
//!
//! # Examples
//!
//! This module is typically used indirectly through format-specific modules like
//! [`crate::aiff`]. Direct usage of IFF primitives is for advanced or custom format handling.
//!
//! ```no_run
//! use audex::aiff::AIFF;
//! use audex::FileType;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // AIFF files use IFF container format internally
//! let aiff = AIFF::load("/path/to/audio.aiff")?;
//! println!("Loaded IFF-based AIFF file");
//! # Ok(())
//! # }
//! ```
//!
//! # References
//!
//! - [EA IFF 85 Standard for Interchange Format Files](http://www.martinreddy.net/gfx/2d/IFF.txt)
//! - [Audio IFF Specification](http://muratnkonar.com/aiff/index.html)

use crate::AudexError;
use std::cell::RefCell;
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::rc::{Rc, Weak};

#[cfg(feature = "async")]
use crate::Result;
#[cfg(feature = "async")]
use crate::util::{delete_bytes_async, insert_bytes_async, resize_bytes_async};
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// IFF-specific error type for chunk parsing and manipulation errors
#[derive(Debug, Clone)]
pub struct IffError(pub String);

impl fmt::Display for IffError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for IffError {}

impl From<IffError> for AudexError {
    fn from(err: IffError) -> Self {
        AudexError::IFFError(err.0)
    }
}

/// Type alias for IFF chunk constructor function pointer
type ChunkConstructor = fn(
    Rc<RefCell<File>>,
    &str,
    u32,
    u64,
    Option<Weak<RefCell<dyn IffContainerChunk>>>,
) -> std::result::Result<Box<dyn IffChunk>, IffError>;

impl From<std::io::Error> for IffError {
    fn from(err: std::io::Error) -> Self {
        IffError(format!("IO error: {}", err))
    }
}

/// Error for invalid chunks
#[derive(Debug, Clone)]
pub struct InvalidChunk(pub String);

impl fmt::Display for InvalidChunk {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid chunk: {}", self.0)
    }
}

impl std::error::Error for InvalidChunk {}

impl From<InvalidChunk> for IffError {
    fn from(err: InvalidChunk) -> Self {
        IffError(format!("Invalid chunk: {}", err.0))
    }
}

/// Error for empty chunks, extends InvalidChunk
#[derive(Debug, Clone)]
pub struct EmptyChunk(pub String);

impl fmt::Display for EmptyChunk {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Empty chunk: {}", self.0)
    }
}

impl std::error::Error for EmptyChunk {}

impl From<EmptyChunk> for InvalidChunk {
    fn from(err: EmptyChunk) -> Self {
        InvalidChunk(err.0)
    }
}

impl From<EmptyChunk> for IffError {
    fn from(err: EmptyChunk) -> Self {
        IffError(format!("Empty chunk: {}", err.0))
    }
}

/// Check if a string is a valid FOURCC chunk ID
pub fn is_valid_chunk_id(id: &str) -> bool {
    if id.len() != 4 {
        return false;
    }

    // Check that all characters are ASCII printable (space to tilde: 0x20 to 0x7E)
    id.chars().all(|c| (' '..='~').contains(&c))
}

/// Assert that a chunk ID is valid, returning an error if not
pub fn assert_valid_chunk_id(id: &str) -> std::result::Result<(), IffError> {
    if !is_valid_chunk_id(id) {
        Err(IffError(
            "IFF chunk ID must be four ASCII characters.".to_string(),
        ))
    } else {
        Ok(())
    }
}

/// Delete `size` bytes from `file` starting at `offset`, shifting subsequent data backward.
/// The size parameter is u64 to support formats with 64-bit chunk sizes (e.g. DSDIFF).
pub fn delete_bytes(file: &mut File, size: u64, offset: u64) -> std::result::Result<(), IffError> {
    const BUFFER_SIZE: usize = 64 * 1024; // 64KB buffer

    // Get file size
    file.seek(SeekFrom::End(0))?;
    let file_size = file.stream_position()?;

    if offset + size > file_size {
        return Err(IffError("Cannot delete bytes beyond file size".to_string()));
    }

    // Move data after the deleted region backward
    let mut pos = offset + size;
    while pos < file_size {
        let remaining = file_size - pos;
        let read_size = std::cmp::min(
            BUFFER_SIZE,
            usize::try_from(remaining).unwrap_or(usize::MAX),
        );

        // Read data from after the deleted region
        file.seek(SeekFrom::Start(pos))?;
        let mut buffer = vec![0u8; read_size];
        file.read_exact(&mut buffer)?;

        // Write it to the new position
        file.seek(SeekFrom::Start(pos - size))?;
        file.write_all(&buffer)?;

        pos += read_size as u64;
    }

    // Truncate the file
    let new_size = file_size - size;
    file.set_len(new_size)?;
    file.flush()?;

    Ok(())
}

/// Insert `size` zero bytes into `file` at `offset`, shifting subsequent data forward
pub fn insert_bytes(file: &mut File, size: u32, offset: u64) -> std::result::Result<(), IffError> {
    const BUFFER_SIZE: usize = 64 * 1024; // 64KB buffer
    // Cap the maximum insertion size to prevent excessive memory allocation
    // from malformed or crafted size values (256 MB is generous for any
    // legitimate audio metadata chunk)
    const MAX_INSERT_SIZE: u32 = 256 * 1024 * 1024; // 256 MB

    if size > MAX_INSERT_SIZE {
        return Err(IffError(format!(
            "Insert size {} exceeds maximum of {} bytes",
            size, MAX_INSERT_SIZE
        )));
    }

    // Get file size
    file.seek(SeekFrom::End(0))?;
    let file_size = file.stream_position()?;

    if offset > file_size {
        return Err(IffError("Cannot insert bytes beyond file size".to_string()));
    }

    // Extend the file
    let new_size = file_size + size as u64;
    file.set_len(new_size)?;

    // Move data backward from the end to make room
    let mut pos = file_size;
    while pos > offset {
        let move_start = std::cmp::max(offset, pos.saturating_sub(BUFFER_SIZE as u64));
        let read_size = (pos - move_start) as usize;

        // Read data from the original position
        file.seek(SeekFrom::Start(move_start))?;
        let mut buffer = vec![0u8; read_size];
        file.read_exact(&mut buffer)?;

        // Write it to the new position
        file.seek(SeekFrom::Start(move_start + size as u64))?;
        file.write_all(&buffer)?;

        pos = move_start;
    }

    // Zero out the inserted region using chunked writes to avoid
    // allocating the full insertion size (up to 256 MB) in one buffer.
    file.seek(SeekFrom::Start(offset))?;
    let zero_buf = [0u8; BUFFER_SIZE];
    let mut remaining = size as usize;
    while remaining > 0 {
        let n = remaining.min(BUFFER_SIZE);
        file.write_all(&zero_buf[..n])?;
        remaining -= n;
    }
    file.flush()?;

    Ok(())
}

/// Resize a region at `offset` from `old_size` to `new_size` bytes, inserting or deleting as needed
pub fn resize_bytes(
    file: &mut File,
    old_size: u32,
    new_size: u32,
    offset: u64,
) -> std::result::Result<(), IffError> {
    if old_size == new_size {
        return Ok(());
    }

    if new_size > old_size {
        // Insert additional bytes
        let diff = new_size - old_size;
        insert_bytes(file, diff, offset + old_size as u64)
    } else {
        // Delete excess bytes
        let diff = (old_size - new_size) as u64;
        delete_bytes(file, diff, offset + new_size as u64)
    }
}

/// Core IFF chunk trait
pub trait IffChunk: fmt::Debug {
    /// Parse chunk header from 8 bytes, returning (id, size)
    fn parse_header(header: &[u8]) -> std::result::Result<(String, u32), IffError>
    where
        Self: Sized;

    /// Write a new chunk header with the given ID and size
    fn write_new_header(&mut self, id: &str, size: u32) -> std::result::Result<(), IffError>;

    /// Write the size field to the file
    fn write_size(&mut self) -> std::result::Result<(), IffError>;

    /// Get the constructor for a specific chunk ID (factory pattern)
    fn get_class(_id: &str) -> ChunkConstructor
    where
        Self: Sized,
    {
        |fileobj, id, data_size, offset, parent| {
            Ok(Box::new(IffChunkImpl::new(
                fileobj, id, data_size, offset, parent,
            )?))
        }
    }

    /// Parse a chunk from a file
    fn parse(
        fileobj: Rc<RefCell<File>>,
        parent_chunk: Option<Weak<RefCell<dyn IffContainerChunk>>>,
    ) -> std::result::Result<Box<dyn IffChunk>, IffError>
    where
        Self: Sized;

    // Accessors
    fn id(&self) -> &str;
    fn data_size(&self) -> u32;
    fn offset(&self) -> u64;
    fn data_offset(&self) -> u64;
    fn size(&self) -> u32;

    /// Read chunk data
    fn read(&mut self) -> std::result::Result<Vec<u8>, IffError>;

    /// Write chunk data
    fn write(&mut self, data: &[u8]) -> std::result::Result<(), IffError>;

    /// Delete this chunk from the file
    fn delete(&mut self) -> std::result::Result<(), IffError>;

    /// Resize the chunk
    fn resize(&mut self, new_data_size: u32) -> std::result::Result<(), IffError>;

    /// Get padding size (0 or 1 byte)
    fn padding(&self) -> u32;

    /// Update size by a diff amount.
    /// Uses i64 to safely represent diffs between u32 values without overflow.
    fn update_size(
        &mut self,
        size_diff: i64,
        changed_subchunk: Option<&dyn IffChunk>,
    ) -> std::result::Result<(), IffError>;
}

/// Container chunk trait for chunks that contain other chunks
pub trait IffContainerChunk: IffChunk {
    /// Initialize as container with optional name size
    fn init_container(&mut self, name_size: usize) -> std::result::Result<(), IffError>;

    fn name(&self) -> Option<&str>;

    fn subchunks(&mut self) -> std::result::Result<&[Box<dyn IffChunk>], IffError>;

    fn contains(&mut self, id: &str) -> std::result::Result<bool, IffError> {
        assert_valid_chunk_id(id)?;
        match self.get_subchunk(id) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn get_subchunk(&mut self, id: &str) -> std::result::Result<&dyn IffChunk, IffError> {
        assert_valid_chunk_id(id)?;
        let subchunks = self.subchunks()?;
        for chunk in subchunks {
            if chunk.id() == id {
                return Ok(chunk.as_ref());
            }
        }
        Err(IffError(format!("No '{}' chunk found", id)))
    }

    /// Delete a subchunk by ID.
    fn delete_subchunk(&mut self, id: &str) -> std::result::Result<(), IffError>;

    /// Insert a new chunk
    fn insert_chunk(
        &mut self,
        id: &str,
        data: Option<&[u8]>,
    ) -> std::result::Result<Box<dyn IffChunk>, IffError>;

    /// Parse next subchunk
    fn parse_next_subchunk(
        &mut self,
        fileobj: Rc<RefCell<File>>,
    ) -> std::result::Result<Box<dyn IffChunk>, IffError>;
}

/// Concrete implementation of IffChunk
pub struct IffChunkImpl {
    fileobj: Rc<RefCell<File>>,
    pub id: String,
    pub data_size: u32,
    pub offset: u64,
    pub data_offset: u64,
    pub size: u32,
    parent_chunk: Option<Weak<RefCell<dyn IffContainerChunk>>>,
}

impl fmt::Debug for IffChunkImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IffChunkImpl")
            .field("id", &self.id)
            .field("data_size", &self.data_size)
            .field("offset", &self.offset)
            .field("data_offset", &self.data_offset)
            .field("size", &self.size)
            .finish()
    }
}

impl IffChunkImpl {
    pub fn new(
        fileobj: Rc<RefCell<File>>,
        id: &str,
        data_size: u32,
        offset: u64,
        parent_chunk: Option<Weak<RefCell<dyn IffContainerChunk>>>,
    ) -> std::result::Result<Self, IffError> {
        // Guard against offset overflow when computing data start position
        let data_offset = offset.checked_add(8).ok_or_else(|| {
            IffError(format!("IFF chunk data offset overflow: offset={}", offset))
        })?;
        let padding = data_size % 2;
        let size = 8u32
            .checked_add(data_size)
            .and_then(|s| s.checked_add(padding))
            .ok_or_else(|| {
                IffError(format!(
                    "IFF chunk size overflow: data_size={} + header + padding exceeds u32",
                    data_size
                ))
            })?;

        Ok(Self {
            fileobj,
            id: id.to_string(),
            data_size,
            offset,
            data_offset,
            size,
            parent_chunk,
        })
    }

    /// Get actual data size accounting for file truncation
    fn get_actual_data_size(&self) -> std::result::Result<u32, IffError> {
        let mut file = self.fileobj.borrow_mut();
        file.seek(SeekFrom::End(0))?;
        let file_size = file.stream_position()?;

        // Compute in u64 to avoid wrapping when data_size is u32::MAX
        let expected_size = self.data_size as u64 + self.padding() as u64;
        let max_size_possible = file_size.saturating_sub(self.data_offset);
        let actual = std::cmp::min(expected_size, max_size_possible);
        Ok(u32::try_from(actual).unwrap_or(u32::MAX))
    }

    /// Calculate total size including header and padding.
    /// Returns an error if the resulting size would overflow u32.
    fn calculate_size(&mut self) -> std::result::Result<(), IffError> {
        let padding = self.padding();
        self.size = 8u32
            .checked_add(self.data_size)
            .and_then(|s| s.checked_add(padding))
            .ok_or_else(|| {
                IffError(format!(
                    "IFF chunk size overflow: data_size={} + header + padding exceeds u32",
                    self.data_size
                ))
            })?;
        Ok(())
    }

    /// Write the current data_size to the file at the chunk's size field offset.
    /// IFF uses big-endian byte order for the 4-byte size field immediately
    /// following the 4-byte chunk ID.
    fn write_size_to_file(&self) -> std::result::Result<(), IffError> {
        let mut file = self.fileobj.borrow_mut();
        file.seek(SeekFrom::Start(self.offset + 4))?;
        file.write_all(&self.data_size.to_be_bytes())?;
        file.flush()?;
        Ok(())
    }
}

impl IffChunk for IffChunkImpl {
    fn parse_header(_header: &[u8]) -> std::result::Result<(String, u32), IffError> {
        // This needs to be implemented by specific format classes (AIFF vs RIFF)
        Err(IffError(
            "parse_header must be implemented by subclasses".to_string(),
        ))
    }

    fn write_new_header(&mut self, _id: &str, _size: u32) -> std::result::Result<(), IffError> {
        Err(IffError(
            "write_new_header must be implemented by subclasses".to_string(),
        ))
    }

    fn write_size(&mut self) -> std::result::Result<(), IffError> {
        Err(IffError(
            "write_size must be implemented by subclasses".to_string(),
        ))
    }

    fn parse(
        fileobj: Rc<RefCell<File>>,
        parent_chunk: Option<Weak<RefCell<dyn IffContainerChunk>>>,
    ) -> std::result::Result<Box<dyn IffChunk>, IffError> {
        const HEADER_SIZE: usize = 8;

        // Read header
        let header = {
            let mut file = fileobj.borrow_mut();
            let mut header_bytes = vec![0u8; HEADER_SIZE];
            file.read_exact(&mut header_bytes)?;
            header_bytes
        };

        if header.len() < HEADER_SIZE {
            return Err(EmptyChunk(format!("Header size < {}", HEADER_SIZE)).into());
        }

        // Parse header - this needs to be implemented by specific format classes
        let (id, data_size) = Self::parse_header(&header)?;

        // Validate ID
        if !is_valid_chunk_id(&id) {
            return Err(InvalidChunk(format!("Invalid chunk ID: {}", id)).into());
        }

        // Get current file position for offset calculation
        let offset = {
            let mut file = fileobj.borrow_mut();
            file.stream_position()? - HEADER_SIZE as u64
        };

        // Create chunk instance
        let factory = Self::get_class(&id);
        factory(fileobj, &id, data_size, offset, parent_chunk)
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn data_size(&self) -> u32 {
        self.data_size
    }

    fn offset(&self) -> u64 {
        self.offset
    }

    fn data_offset(&self) -> u64 {
        self.data_offset
    }

    fn size(&self) -> u32 {
        self.size
    }

    fn read(&mut self) -> std::result::Result<Vec<u8>, IffError> {
        // Enforce the library-wide tag allocation ceiling
        let limits = crate::limits::ParseLimits::default();
        if (self.data_size as u64) > limits.max_tag_size {
            return Err(IffError(format!(
                "IFF chunk data size {} exceeds global limit {} bytes",
                self.data_size, limits.max_tag_size
            )));
        }
        let mut file = self.fileobj.borrow_mut();
        file.seek(SeekFrom::Start(self.data_offset))?;
        let mut data = vec![0u8; self.data_size as usize];
        file.read_exact(&mut data)?;
        Ok(data)
    }

    fn write(&mut self, data: &[u8]) -> std::result::Result<(), IffError> {
        if data.len() > self.data_size as usize {
            return Err(IffError("Data too large for chunk".to_string()));
        }

        let mut file = self.fileobj.borrow_mut();
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

    fn delete(&mut self) -> std::result::Result<(), IffError> {
        let chunk_total_size = self.size;

        // Delete chunk bytes from the file
        {
            let mut file = self.fileobj.borrow_mut();
            delete_bytes(&mut file, chunk_total_size as u64, self.offset)?;
            file.flush()?;
        }

        // Propagate the size reduction to the parent container so its
        // header stays consistent with the actual file contents.
        if let Some(parent_weak) = &self.parent_chunk {
            if let Some(parent_rc) = parent_weak.upgrade() {
                parent_rc
                    .borrow_mut()
                    .update_size(-(chunk_total_size as i64), None)?;
            }
        }

        Ok(())
    }

    fn resize(&mut self, new_data_size: u32) -> std::result::Result<(), IffError> {
        let old_size = self.get_actual_data_size()?;
        let old_total = self.size;
        let padding = new_data_size % 2;

        // Guard against u32 overflow when computing the new region size.
        // The constructor and calculate_size() already use checked_add;
        // resize must do the same to avoid wrapping on pathological inputs.
        let new_region_size = new_data_size.checked_add(padding).ok_or_else(|| {
            IffError(format!(
                "IFF chunk resize overflow: new_data_size={} + padding={} exceeds u32",
                new_data_size, padding
            ))
        })?;

        // Resize the file region
        {
            let mut file = self.fileobj.borrow_mut();
            resize_bytes(&mut file, old_size, new_region_size, self.data_offset)?;
            file.flush()?;
        }

        // Update our own size tracking and persist to disk
        self.data_size = new_data_size;
        self.calculate_size()?;
        self.write_size_to_file()?;

        // Propagate the size difference to the parent container
        let size_diff = self.size as i64 - old_total as i64;
        if size_diff != 0 {
            if let Some(parent_weak) = &self.parent_chunk {
                if let Some(parent_rc) = parent_weak.upgrade() {
                    parent_rc.borrow_mut().update_size(size_diff, None)?;
                }
            }
        }

        Ok(())
    }

    fn padding(&self) -> u32 {
        self.data_size % 2
    }

    fn update_size(
        &mut self,
        size_diff: i64,
        _changed_subchunk: Option<&dyn IffChunk>,
    ) -> std::result::Result<(), IffError> {
        let new_data_size = self.data_size as i64 + size_diff;
        if new_data_size < 0 || new_data_size > u32::MAX as i64 {
            return Err(IffError(format!(
                "Chunk resize would produce invalid size: {}",
                new_data_size
            )));
        }
        self.data_size = new_data_size as u32;
        self.calculate_size()?;

        // Persist the updated size to the file on disk
        self.write_size_to_file()?;

        // Propagate the change upward so ancestor containers
        // remain consistent with the actual file layout
        if let Some(parent_weak) = &self.parent_chunk {
            if let Some(parent_rc) = parent_weak.upgrade() {
                parent_rc.borrow_mut().update_size(size_diff, None)?;
            }
        }

        Ok(())
    }
}

/// Container chunk implementation
pub struct IffContainerChunkImpl {
    base: IffChunkImpl,
    name: Option<String>,
    name_size: usize,
    subchunks: Vec<Box<dyn IffChunk>>,
    subchunks_loaded: bool,
}

impl fmt::Debug for IffContainerChunkImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IffContainerChunkImpl")
            .field("base", &self.base)
            .field("name", &self.name)
            .field("name_size", &self.name_size)
            .field("subchunks_loaded", &self.subchunks_loaded)
            .finish()
    }
}

impl IffContainerChunkImpl {
    pub fn new(
        fileobj: Rc<RefCell<File>>,
        id: &str,
        data_size: u32,
        offset: u64,
        parent_chunk: Option<Weak<RefCell<dyn IffContainerChunk>>>,
    ) -> std::result::Result<Self, IffError> {
        let base = IffChunkImpl::new(fileobj, id, data_size, offset, parent_chunk)?;
        Ok(Self {
            base,
            name: None,
            name_size: 4, // Default FOURCC name size
            subchunks: Vec::new(),
            subchunks_loaded: false,
        })
    }
}

impl IffChunk for IffContainerChunkImpl {
    fn parse_header(_header: &[u8]) -> std::result::Result<(String, u32), IffError> {
        // This should be implemented by specific format classes
        Err(IffError(
            "parse_header must be implemented by subclasses".to_string(),
        ))
    }

    fn write_new_header(&mut self, id: &str, size: u32) -> std::result::Result<(), IffError> {
        self.base.write_new_header(id, size)
    }

    fn write_size(&mut self) -> std::result::Result<(), IffError> {
        self.base.write_size()
    }

    fn parse(
        fileobj: Rc<RefCell<File>>,
        parent_chunk: Option<Weak<RefCell<dyn IffContainerChunk>>>,
    ) -> std::result::Result<Box<dyn IffChunk>, IffError> {
        let _base_chunk = IffChunkImpl::parse(fileobj.clone(), parent_chunk.clone())?;
        Err(IffError(
            "Container parsing not fully implemented".to_string(),
        ))
    }

    fn id(&self) -> &str {
        self.base.id()
    }
    fn data_size(&self) -> u32 {
        self.base.data_size()
    }
    fn offset(&self) -> u64 {
        self.base.offset()
    }
    fn data_offset(&self) -> u64 {
        self.base.data_offset()
    }
    fn size(&self) -> u32 {
        self.base.size()
    }

    fn read(&mut self) -> std::result::Result<Vec<u8>, IffError> {
        self.base.read()
    }
    fn write(&mut self, data: &[u8]) -> std::result::Result<(), IffError> {
        self.base.write(data)
    }
    fn delete(&mut self) -> std::result::Result<(), IffError> {
        self.base.delete()
    }
    fn resize(&mut self, new_data_size: u32) -> std::result::Result<(), IffError> {
        self.base.resize(new_data_size)
    }
    fn padding(&self) -> u32 {
        self.base.padding()
    }
    fn update_size(
        &mut self,
        size_diff: i64,
        changed_subchunk: Option<&dyn IffChunk>,
    ) -> std::result::Result<(), IffError> {
        self.base.update_size(size_diff, changed_subchunk)
    }
}

impl IffContainerChunk for IffContainerChunkImpl {
    fn init_container(&mut self, name_size: usize) -> std::result::Result<(), IffError> {
        if self.data_size() < name_size as u32 {
            return Err(InvalidChunk(format!("Container chunk data size < {}", name_size)).into());
        }

        self.name_size = name_size;

        // Read the container name if name_size > 0
        if name_size > 0 {
            let mut file = self.base.fileobj.borrow_mut();
            file.seek(SeekFrom::Start(self.data_offset()))?;
            let mut name_bytes = vec![0u8; name_size];
            file.read_exact(&mut name_bytes)?;

            match String::from_utf8(name_bytes) {
                Ok(name) => self.name = Some(name),
                Err(_) => return Err(IffError("Invalid container name encoding".to_string())),
            }
        } else {
            self.name = None;
        }

        // Initialize empty subchunks list
        self.subchunks = Vec::new();
        self.subchunks_loaded = false;

        Ok(())
    }

    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    fn subchunks(&mut self) -> std::result::Result<&[Box<dyn IffChunk>], IffError> {
        if !self.subchunks_loaded {
            // Lazy load subchunks
            self.subchunks.clear();
            let mut next_offset = self.data_offset() + self.name_size as u64;

            while next_offset < self.offset() + self.size() as u64 {
                let fileobj = self.base.fileobj.clone();
                {
                    let mut file = fileobj.borrow_mut();
                    file.seek(SeekFrom::Start(next_offset))?;
                }

                match self.parse_next_subchunk(fileobj) {
                    Ok(chunk) => {
                        let new_offset = chunk.offset() + chunk.size() as u64;
                        // Guard against zero-size or malformed chunks that would
                        // not advance the offset, preventing an infinite loop.
                        if new_offset <= next_offset {
                            break;
                        }
                        next_offset = new_offset;
                        self.subchunks.push(chunk);
                    }
                    Err(_) => break, // End of chunks or invalid chunk
                }
            }

            self.subchunks_loaded = true;
        }

        Ok(&self.subchunks)
    }

    fn delete_subchunk(&mut self, id: &str) -> std::result::Result<(), IffError> {
        assert_valid_chunk_id(id)?;

        // Ensure subchunks are loaded so we can search through them.
        let _ = self.subchunks()?;

        // Find the index of the target subchunk.
        let idx = self
            .subchunks
            .iter()
            .position(|c| c.id() == id)
            .ok_or_else(|| IffError(format!("No '{}' chunk found", id)))?;

        // Remove from the list and delete its bytes from the file.
        let mut chunk = self.subchunks.remove(idx);
        chunk.delete()
    }

    fn insert_chunk(
        &mut self,
        id: &str,
        data: Option<&[u8]>,
    ) -> std::result::Result<Box<dyn IffChunk>, IffError> {
        if !is_valid_chunk_id(id) {
            return Err(IffError("Invalid IFF chunk ID".to_string()));
        }

        // Calculate insertion point
        let actual_data_size = self.base.get_actual_data_size()?;
        let next_offset = self.data_offset() + actual_data_size as u64;

        // Calculate new chunk size, validating it fits in u32
        let data_size = u32::try_from(data.map(|d| d.len()).unwrap_or(0))
            .map_err(|_| IffError("IFF insert_chunk data length exceeds u32::MAX".to_string()))?;
        let padding = data_size % 2;
        let chunk_size = 8u32
            .checked_add(data_size)
            .and_then(|s| s.checked_add(padding))
            .ok_or_else(|| {
                IffError(format!(
                    "IFF chunk size overflow: data_size={} + header + padding exceeds u32",
                    data_size
                ))
            })?;

        // Insert space in file
        {
            let mut file = self.base.fileobj.borrow_mut();
            insert_bytes(&mut file, chunk_size, next_offset)?;
        }

        // Write chunk header
        {
            let mut file = self.base.fileobj.borrow_mut();
            file.seek(SeekFrom::Start(next_offset))?;
            // Header writing deferred to subclass
        }

        // Create new chunk
        let fileobj = self.base.fileobj.clone();
        let chunk = IffChunkImpl::new(fileobj, id, data_size, next_offset, None)?;
        let mut boxed_chunk = Box::new(chunk) as Box<dyn IffChunk>;

        // Write data if provided
        if let Some(data) = data {
            boxed_chunk.write(data)?;
        }

        // Update our size, checking for overflow
        self.base.data_size = self
            .base
            .data_size
            .checked_add(chunk_size)
            .ok_or_else(|| IffError("Container size would overflow u32".to_string()))?;
        self.base.calculate_size()?;

        // Add to subchunks if loaded
        if self.subchunks_loaded {
            let chunk_ref = Box::new(IffChunkImpl::new(
                self.base.fileobj.clone(),
                id,
                data_size,
                next_offset,
                None,
            )?) as Box<dyn IffChunk>;
            self.subchunks.push(chunk_ref);
            // Create a duplicate for return since we can't move out of vector
            Ok(Box::new(IffChunkImpl::new(
                self.base.fileobj.clone(),
                id,
                data_size,
                next_offset,
                None,
            )?))
        } else {
            Ok(boxed_chunk)
        }
    }

    fn parse_next_subchunk(
        &mut self,
        fileobj: Rc<RefCell<File>>,
    ) -> std::result::Result<Box<dyn IffChunk>, IffError> {
        // Use the standard chunk parsing
        IffChunkImpl::parse(fileobj, None)
    }
}

/// High-level IFF file interface
pub struct IffFile {
    pub root: Box<dyn IffContainerChunk>,
}

impl IffFile {
    pub fn new<T>(chunk_cls: T, fileobj: Rc<RefCell<File>>) -> std::result::Result<Self, IffError>
    where
        T: Fn(
            Rc<RefCell<File>>,
            Option<Weak<RefCell<dyn IffContainerChunk>>>,
        ) -> std::result::Result<Box<dyn IffContainerChunk>, IffError>,
    {
        let root = chunk_cls(fileobj, None)?;
        Ok(Self { root })
    }

    pub fn contains(&mut self, id: &str) -> std::result::Result<bool, IffError> {
        self.root.contains(id)
    }

    pub fn get_chunk(&mut self, id: &str) -> std::result::Result<&dyn IffChunk, IffError> {
        self.root.get_subchunk(id)
    }

    pub fn delete_chunk(&mut self, id: &str) -> std::result::Result<(), IffError> {
        self.root.delete_subchunk(id)
    }

    pub fn insert_chunk(
        &mut self,
        id: &str,
        data: Option<&[u8]>,
    ) -> std::result::Result<Box<dyn IffChunk>, IffError> {
        self.root.insert_chunk(id, data)
    }
}

/// Base trait for IFF files with ID3 support
pub trait IffID3 {
    /// Load the IFF file structure
    fn load_file(&self, fileobj: Rc<RefCell<File>>) -> std::result::Result<IffFile, IffError>;

    /// Pre-load header for ID3 processing
    fn pre_load_header(&self, fileobj: Rc<RefCell<File>>) -> std::result::Result<u64, IffError> {
        let _iff_file = self.load_file(fileobj)?;
        // Would need mutable access to search for ID3 chunk
        Err(IffError("ID3 chunk not found".to_string()))
    }

    /// Save ID3 data to the IFF file
    fn save(
        &mut self,
        _fileobj: &mut dyn Write,
        _v2_version: u32,
        _v23_sep: char,
        _padding: Option<u32>,
    ) -> std::result::Result<(), IffError> {
        // Implementation would depend on ID3 library integration
        Err(IffError("save not implemented".to_string()))
    }

    /// Delete ID3 data from the IFF file
    fn delete(&mut self, _fileobj: &mut dyn Write) -> std::result::Result<(), IffError> {
        // Implementation would depend on ID3 library integration
        Err(IffError("delete not implemented".to_string()))
    }
}

/// Async IFF chunk representation for asynchronous file operations
///
/// This struct provides a lightweight representation of an IFF chunk
/// suitable for async operations without the complex reference counting
/// of the sync implementation.
#[cfg(feature = "async")]
#[derive(Debug, Clone)]
pub struct IffChunkAsync {
    /// Four-character chunk identifier (e.g., "COMM", "SSND")
    pub id: String,
    /// Total size of chunk including header and padding
    pub size: u32,
    /// File offset where chunk starts
    pub offset: u64,
    /// File offset where chunk data starts (after 8-byte header)
    pub data_offset: u64,
    /// Size of chunk data (without header)
    pub data_size: u32,
}

#[cfg(feature = "async")]
impl IffChunkAsync {
    /// Create a new IFF chunk representation
    ///
    /// Calculates the total size including header and padding automatically.
    pub fn new(id: String, data_size: u32, offset: u64) -> std::result::Result<Self, IffError> {
        let padding = data_size % 2;
        // Use checked arithmetic to detect overflow, consistent with the sync IffChunkImpl::new
        let size = 8u32
            .checked_add(data_size)
            .and_then(|s| s.checked_add(padding))
            .ok_or_else(|| {
                IffError(format!(
                    "IFF chunk size overflow: data_size={} + header + padding exceeds u32",
                    data_size
                ))
            })?;
        // Guard against offset overflow when computing data start position
        let data_offset = offset.checked_add(8).ok_or_else(|| {
            IffError(format!("IFF chunk data offset overflow: offset={}", offset))
        })?;
        Ok(Self {
            id,
            size,
            offset,
            data_offset,
            data_size,
        })
    }

    /// Get padding size (0 or 1 byte for even alignment)
    ///
    /// IFF format requires chunks to be aligned to even byte boundaries.
    pub fn padding(&self) -> u32 {
        self.data_size % 2
    }

    /// Read chunk data asynchronously
    ///
    /// Seeks to the data offset and reads the entire chunk data.
    pub async fn read_data(&self, file: &mut TokioFile) -> Result<Vec<u8>> {
        // Enforce the library-wide tag allocation ceiling
        crate::limits::ParseLimits::default()
            .check_tag_size(self.data_size as u64, "IFF chunk async")?;
        file.seek(SeekFrom::Start(self.data_offset)).await?;
        let mut data = vec![0u8; self.data_size as usize];
        file.read_exact(&mut data).await?;
        Ok(data)
    }

    /// Write data to chunk asynchronously
    ///
    /// Writes data to the chunk and adds padding byte if needed.
    /// Returns error if data exceeds chunk capacity.
    pub async fn write_data(&self, file: &mut TokioFile, data: &[u8]) -> Result<()> {
        if data.len() > self.data_size as usize {
            return Err(AudexError::InvalidData(
                "Data too large for chunk".to_string(),
            ));
        }

        file.seek(SeekFrom::Start(self.data_offset)).await?;
        file.write_all(data).await?;

        // Write padding byte if needed for even alignment
        if self.padding() > 0 {
            file.seek(SeekFrom::Start(self.data_offset + self.data_size as u64))
                .await?;
            file.write_all(&[0]).await?;
        }

        Ok(())
    }
}

/// Async IFF file structure (big-endian, used by AIFF)
///
/// This struct represents the parsed structure of an IFF file,
/// containing the file type and all chunks found in the file.
#[cfg(feature = "async")]
#[derive(Debug, Clone)]
pub struct IffFileAsync {
    /// File type identifier (e.g., "AIFF")
    pub file_type: String,
    /// List of chunks in the file
    pub chunks: Vec<IffChunkAsync>,
    /// Total file size from FORM header
    pub file_size: u32,
}

#[cfg(feature = "async")]
impl IffFileAsync {
    /// Parse IFF file structure asynchronously (big-endian)
    ///
    /// Reads the FORM header and iterates through all chunks in the file,
    /// building a complete representation of the file structure.
    pub async fn parse(file: &mut TokioFile) -> Result<Self> {
        file.seek(SeekFrom::Start(0)).await?;

        // Read FORM header (12 bytes: "FORM" + size + type)
        let mut header = [0u8; 12];
        file.read_exact(&mut header).await?;

        // Validate FORM signature
        if &header[0..4] != b"FORM" {
            return Err(AudexError::IFFError("Expected FORM signature".to_string()));
        }

        // Parse file size and type (big-endian)
        let file_size = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
        let file_type = String::from_utf8_lossy(&header[8..12]).into_owned();

        let mut chunks = Vec::new();
        let mut offset = 12u64; // Start after FORM header

        // Clamp the loop bound to the actual file size
        let actual_end = file.seek(SeekFrom::End(0)).await.unwrap_or(u64::MAX);
        file.seek(SeekFrom::Start(offset)).await?;
        let end_bound = (file_size as u64 + 8).min(actual_end);

        // Parse all chunks in the file
        while offset < end_bound {
            file.seek(SeekFrom::Start(offset)).await?;

            // Read chunk header (8 bytes: ID + size)
            let mut chunk_header = [0u8; 8];
            if file.read_exact(&mut chunk_header).await.is_err() {
                break; // End of file reached
            }

            // Parse chunk ID and size (big-endian)
            let chunk_id = String::from_utf8_lossy(&chunk_header[0..4]).into_owned();
            let chunk_size = u32::from_be_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
            ]);

            let chunk = IffChunkAsync::new(chunk_id, chunk_size, offset)?;
            chunks.push(chunk);

            // Move to next chunk with even alignment
            offset += 8 + chunk_size as u64;
            if chunk_size % 2 == 1 {
                offset += 1; // IFF chunks are padded to even boundaries
            }
        }

        Ok(IffFileAsync {
            file_type,
            chunks,
            file_size,
        })
    }

    /// Find chunk by ID (case-insensitive)
    pub fn find_chunk(&self, id: &str) -> Option<&IffChunkAsync> {
        self.chunks
            .iter()
            .find(|chunk| chunk.id.eq_ignore_ascii_case(id))
    }

    /// Check if chunk exists
    pub fn has_chunk(&self, id: &str) -> bool {
        self.find_chunk(id).is_some()
    }
}

/// Async RIFF file structure (little-endian, used by WAV)
///
/// This struct represents the parsed structure of a RIFF file,
/// containing the file type and all chunks found in the file.
#[cfg(feature = "async")]
#[derive(Debug, Clone)]
pub struct RiffFileAsync {
    /// File type identifier (e.g., "WAVE")
    pub file_type: String,
    /// List of chunks in the file
    pub chunks: Vec<IffChunkAsync>,
    /// Total file size from RIFF header
    pub file_size: u32,
}

#[cfg(feature = "async")]
impl RiffFileAsync {
    /// Parse RIFF file structure asynchronously (little-endian)
    ///
    /// Reads the RIFF header and iterates through all chunks in the file,
    /// building a complete representation of the file structure.
    pub async fn parse(file: &mut TokioFile) -> Result<Self> {
        file.seek(SeekFrom::Start(0)).await?;

        // Read RIFF header (12 bytes: "RIFF" + size + type)
        let mut header = [0u8; 12];
        file.read_exact(&mut header).await?;

        // Validate RIFF signature
        if &header[0..4] != b"RIFF" {
            return Err(AudexError::WAVError("Expected RIFF signature".to_string()));
        }

        // Parse file size and type (little-endian)
        let file_size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let file_type = String::from_utf8_lossy(&header[8..12]).into_owned();

        let mut chunks = Vec::new();
        let mut offset = 12u64; // Start after RIFF header

        // Clamp the loop bound to the actual file size
        let actual_end = file.seek(SeekFrom::End(0)).await.unwrap_or(u64::MAX);
        file.seek(SeekFrom::Start(offset)).await?;
        let end_bound = (file_size as u64 + 8).min(actual_end);

        // Parse all chunks in the file
        while offset < end_bound {
            file.seek(SeekFrom::Start(offset)).await?;

            // Read chunk header (8 bytes: ID + size)
            let mut chunk_header = [0u8; 8];
            if file.read_exact(&mut chunk_header).await.is_err() {
                break; // End of file reached
            }

            // Parse chunk ID and size (little-endian)
            let chunk_id = String::from_utf8_lossy(&chunk_header[0..4]).into_owned();
            let chunk_size = u32::from_le_bytes([
                chunk_header[4],
                chunk_header[5],
                chunk_header[6],
                chunk_header[7],
            ]);

            let chunk = IffChunkAsync::new(chunk_id, chunk_size, offset)?;
            chunks.push(chunk);

            // Move to next chunk with even alignment
            offset += 8 + chunk_size as u64;
            if chunk_size % 2 == 1 {
                offset += 1; // RIFF chunks are padded to even boundaries
            }
        }

        Ok(RiffFileAsync {
            file_type,
            chunks,
            file_size,
        })
    }

    /// Find chunk by ID (case-insensitive)
    pub fn find_chunk(&self, id: &str) -> Option<&IffChunkAsync> {
        self.chunks
            .iter()
            .find(|chunk| chunk.id.eq_ignore_ascii_case(id))
    }

    /// Check if chunk exists
    pub fn has_chunk(&self, id: &str) -> bool {
        self.find_chunk(id).is_some()
    }
}

/// Insert a new chunk into an IFF file asynchronously (big-endian)
///
/// This function inserts space for a new chunk at the specified offset,
/// writes the chunk header and data, and returns the new chunk representation.
///
/// # Arguments
/// * `file` - The file to modify
/// * `id` - Four-character chunk identifier
/// * `data` - Chunk data to write
/// * `offset` - File offset where chunk should be inserted
///
/// # Returns
/// The newly created chunk representation
#[cfg(feature = "async")]
pub async fn insert_iff_chunk_async(
    file: &mut TokioFile,
    id: &str,
    data: &[u8],
    offset: u64,
) -> Result<IffChunkAsync> {
    let data_size = data.len() as u32;
    let padding = if data_size % 2 == 1 { 1 } else { 0 };
    // Use checked arithmetic to properly detect overflow instead of silent clamping
    let total_size = 8u32
        .checked_add(data_size)
        .and_then(|s| s.checked_add(padding))
        .ok_or_else(|| {
            AudexError::IFFError(format!(
                "IFF chunk total size overflow: data_size={} + header + padding exceeds u32",
                data_size
            ))
        })?;

    // Insert space for the new chunk
    insert_bytes_async(file, total_size as u64, offset, None).await?;

    // Write chunk header (big-endian for IFF)
    file.seek(SeekFrom::Start(offset)).await?;

    // Pad ID to 4 characters if needed
    let mut id_bytes = [b' '; 4];
    for (i, byte) in id.as_bytes().iter().take(4).enumerate() {
        id_bytes[i] = *byte;
    }
    file.write_all(&id_bytes).await?;
    file.write_all(&data_size.to_be_bytes()).await?;

    // Write chunk data
    file.write_all(data).await?;

    // Write padding byte if needed
    if padding > 0 {
        file.write_all(&[0]).await?;
    }

    file.flush().await?;

    Ok(IffChunkAsync::new(id.to_string(), data_size, offset)?)
}

/// Insert a new chunk into a RIFF file asynchronously (little-endian)
///
/// This function inserts space for a new chunk at the specified offset,
/// writes the chunk header and data, and returns the new chunk representation.
///
/// # Arguments
/// * `file` - The file to modify
/// * `id` - Four-character chunk identifier
/// * `data` - Chunk data to write
/// * `offset` - File offset where chunk should be inserted
///
/// # Returns
/// The newly created chunk representation
#[cfg(feature = "async")]
pub async fn insert_riff_chunk_async(
    file: &mut TokioFile,
    id: &str,
    data: &[u8],
    offset: u64,
) -> Result<IffChunkAsync> {
    let data_size = data.len() as u32;
    let padding = if data_size % 2 == 1 { 1 } else { 0 };
    // Use checked arithmetic to properly detect overflow instead of silent clamping
    let total_size = 8u32
        .checked_add(data_size)
        .and_then(|s| s.checked_add(padding))
        .ok_or_else(|| {
            AudexError::IFFError(format!(
                "RIFF chunk total size overflow: data_size={} + header + padding exceeds u32",
                data_size
            ))
        })?;

    // Insert space for the new chunk
    insert_bytes_async(file, total_size as u64, offset, None).await?;

    // Write chunk header (little-endian for RIFF)
    file.seek(SeekFrom::Start(offset)).await?;

    // Pad ID to 4 characters if needed
    let mut id_bytes = [b' '; 4];
    for (i, byte) in id.as_bytes().iter().take(4).enumerate() {
        id_bytes[i] = *byte;
    }
    file.write_all(&id_bytes).await?;
    file.write_all(&data_size.to_le_bytes()).await?;

    // Write chunk data
    file.write_all(data).await?;

    // Write padding byte if needed
    if padding > 0 {
        file.write_all(&[0]).await?;
    }

    file.flush().await?;

    Ok(IffChunkAsync::new(id.to_string(), data_size, offset)?)
}

/// Delete a chunk from an IFF/RIFF file asynchronously
///
/// Removes the entire chunk including header and padding from the file.
///
/// # Arguments
/// * `file` - The file to modify
/// * `chunk` - The chunk to delete
#[cfg(feature = "async")]
pub async fn delete_chunk_async(file: &mut TokioFile, chunk: &IffChunkAsync) -> Result<()> {
    // Calculate total bytes to delete including padding
    let total_size = chunk.size as u64;
    delete_bytes_async(file, total_size, chunk.offset, None).await?;
    file.flush().await?;
    Ok(())
}

/// Resize a chunk in an IFF file asynchronously (big-endian)
///
/// Adjusts the chunk's data region and updates the size in the header.
///
/// # Arguments
/// * `file` - The file to modify
/// * `chunk` - The chunk to resize
/// * `new_data_size` - New size for the chunk data
#[cfg(feature = "async")]
pub async fn resize_iff_chunk_async(
    file: &mut TokioFile,
    chunk: &IffChunkAsync,
    new_data_size: u32,
) -> Result<()> {
    let old_padded = chunk.data_size + chunk.padding();
    let new_padding = if new_data_size % 2 == 1 { 1 } else { 0 };
    let new_padded = new_data_size + new_padding;

    // Resize the data region
    resize_bytes_async(
        file,
        old_padded as u64,
        new_padded as u64,
        chunk.data_offset,
    )
    .await?;

    // Update chunk size in header (big-endian)
    file.seek(SeekFrom::Start(chunk.offset + 4)).await?;
    file.write_all(&new_data_size.to_be_bytes()).await?;

    file.flush().await?;
    Ok(())
}

/// Resize a chunk in a RIFF file asynchronously (little-endian)
///
/// Adjusts the chunk's data region and updates the size in the header.
///
/// # Arguments
/// * `file` - The file to modify
/// * `chunk` - The chunk to resize
/// * `new_data_size` - New size for the chunk data
#[cfg(feature = "async")]
pub async fn resize_riff_chunk_async(
    file: &mut TokioFile,
    chunk: &IffChunkAsync,
    new_data_size: u32,
) -> Result<()> {
    let old_padded = chunk.data_size + chunk.padding();
    let new_padding = if new_data_size % 2 == 1 { 1 } else { 0 };
    let new_padded = new_data_size + new_padding;

    // Resize the data region
    resize_bytes_async(
        file,
        old_padded as u64,
        new_padded as u64,
        chunk.data_offset,
    )
    .await?;

    // Update chunk size in header (little-endian)
    file.seek(SeekFrom::Start(chunk.offset + 4)).await?;
    file.write_all(&new_data_size.to_le_bytes()).await?;

    file.flush().await?;
    Ok(())
}

/// Update IFF file size in FORM header (big-endian)
///
/// # Arguments
/// * `file` - The file to modify
/// * `new_size` - New file size value
#[cfg(feature = "async")]
pub async fn update_iff_file_size_async(file: &mut TokioFile, new_size: u32) -> Result<()> {
    file.seek(SeekFrom::Start(4)).await?;
    file.write_all(&new_size.to_be_bytes()).await?;
    file.flush().await?;
    Ok(())
}

/// Update RIFF file size in header (little-endian)
///
/// # Arguments
/// * `file` - The file to modify
/// * `new_size` - New file size value
#[cfg(feature = "async")]
pub async fn update_riff_file_size_async(file: &mut TokioFile, new_size: u32) -> Result<()> {
    file.seek(SeekFrom::Start(4)).await?;
    file.write_all(&new_size.to_le_bytes()).await?;
    file.flush().await?;
    Ok(())
}
