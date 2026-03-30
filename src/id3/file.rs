//! ID3 file handling implementation
//!
//! Provides [`ID3`], a standalone ID3v2 tag file type that can load and save
//! ID3 tags independently of any specific audio format. Also provides
//! [`ID3FileType`], which wraps `ID3` to implement the [`FileType`] trait
//! for use with the dynamic format detection system.

use crate::id3::frames::{Frame, FrameRegistry};
#[cfg(feature = "async")]
use crate::id3::id3v1::find_id3v1;
use crate::id3::id3v1::find_id3v1_from_reader;
use crate::id3::specs::ID3Header;
use crate::id3::tags::{ID3SaveConfig, ID3Tags};
use crate::id3::util::BitPaddedInt;
use crate::tags::PaddingInfo;
use crate::util::{delete_bytes, insert_bytes, read_full};
use crate::{AudexError, FileType, Metadata, ReadWriteSeek, Result, StreamInfo, Tags};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(feature = "async")]
use crate::util::{delete_bytes_async, insert_bytes_async};
#[cfg(feature = "async")]
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};

/// Controls how ID3v1 tags are handled during save operations
///
/// ID3v1 is a legacy tag format stored in the last 128 bytes of an MP3 file.
/// This enum controls whether ID3v1 tags are created, updated, or removed
/// when saving ID3v2 tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ID3v1SaveOptions {
    /// Remove any existing ID3v1 tag from the file
    REMOVE = 0,
    /// Update an existing ID3v1 tag if present, but don't create one
    UPDATE = 1,
    /// Create a new ID3v1 tag or update an existing one
    CREATE = 2,
}

/// Standalone ID3v2 tag container with file I/O
///
/// Wraps [`ID3Tags`] with file loading and saving capabilities. Implements
/// both [`Tags`] (for key-value access) and [`Metadata`] (for file
/// I/O operations).
///
/// The `tags` field provides direct access to the underlying frame dictionary
/// for advanced frame-level manipulation beyond what the `Tags` trait offers.
#[derive(Debug)]
pub struct ID3 {
    /// Internal ID3Tags frame container — publicly accessible for direct
    /// frame dictionary access (e.g. `id3.tags.dict`)
    pub tags: ID3Tags,
    /// File path this tag was loaded from (used for save operations)
    pub filename: Option<String>,
    /// Parsed header from the loaded file
    _header: Option<ID3Header>,
    /// ID3 version as (2, major, revision) for ID3v2 — e.g. (2, 4, 0) for ID3v2.4.
    /// Set to (1, 1, 0) when only an ID3v1 tag is present.
    _version: (u8, u8, u8),
    /// Raw byte data of frames not recognized by the parser
    pub unknown_frames: Vec<Vec<u8>>,
    /// Padding bytes found after the last frame in the loaded tag
    _padding: usize,
    /// Strict-parsing mode (defaults to `true`)
    pub pedantic: bool,
    /// Cached text values extracted from frames, enabling `Tags::get`
    /// to return borrowed `&[String]` slices. Kept in sync with the
    /// underlying frame dictionary by `set`, `remove`, and load paths.
    values_cache: HashMap<String, Vec<String>>,
}

/// Implement Tags trait for ID3 to provide basic tag functionality
impl crate::Tags for ID3 {
    fn get(&self, key: &str) -> Option<&[String]> {
        self.values_cache.get(key).map(|v| v.as_slice())
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        self.tags.set(key, values);
        // Re-read from the frame to capture any encoding transformations
        if let Some(text_values) = self.tags.get_text_values(key) {
            self.values_cache.insert(key.to_string(), text_values);
        }
    }

    fn remove(&mut self, key: &str) {
        self.tags.remove(key);
        self.values_cache.remove(key);
    }

    fn keys(&self) -> Vec<String> {
        self.tags.keys()
    }

    fn pprint(&self) -> String {
        format!("ID3 tags: {} frames", Tags::keys(self).len())
    }
}

/// Implement Metadata trait for ID3 tag handling
impl Metadata for ID3 {
    type Error = AudexError;

    fn new() -> Self
    where
        Self: Sized,
    {
        Self::new()
    }

    fn load_from_fileobj(filething: &mut crate::util::AnyFileThing) -> Result<Self>
    where
        Self: Sized,
    {
        // Convert AnyFileThing to a path-like interface
        let mut instance = Self::new();

        // Get the path from the file thing
        let path = filething.display_name();
        instance.load(path, None, true, 4, true)?;
        Ok(instance)
    }

    fn save_to_fileobj(&self, _filething: &mut crate::util::AnyFileThing) -> Result<()> {
        // For ID3, saving requires mutable self, which we don't have here
        // Users should use the save() method on the ID3 instance instead
        Err(AudexError::Unsupported(
            "Use save() method directly instead".to_string(),
        ))
    }

    fn delete_from_fileobj(filething: &mut crate::util::AnyFileThing) -> Result<()>
    where
        Self: Sized,
    {
        let path = filething.display_name();
        clear(path, true, true)
    }
}

impl ID3 {
    /// Create a new empty ID3 instance
    pub fn new() -> Self {
        Self {
            tags: ID3Tags::new(),
            filename: None,
            _header: None,
            _version: (2, 4, 0), // Default to ID3v2.4.0
            unknown_frames: Vec::new(),
            _padding: 0,
            pedantic: true, // Pedantic mode for strict parsing
            values_cache: HashMap::new(),
        }
    }

    /// Rebuild the text values cache from the current frame dictionary.
    /// Called after loading or any bulk frame mutation to keep the cache
    /// in sync with the underlying data.
    fn refresh_values_cache(&mut self) {
        self.values_cache.clear();
        for key in self.tags.frame_keys() {
            if let Some(values) = self.tags.get_text_values(&key) {
                self.values_cache.insert(key, values);
            }
        }
    }

    /// Creates a new `ID3` instance by reading and parsing tags from the given file.
    ///
    /// The file is loaded with default settings: ID3v2.4, pedantic mode enabled,
    /// and frame translation turned on.
    pub fn with_file<P: AsRef<Path>>(filething: P) -> Result<Self> {
        let mut instance = Self::new();
        instance.load(filething, None, true, 4, true)?;
        Ok(instance)
    }

    /// Loads and parses ID3 tags from the given file path.
    ///
    /// This is an alias for [`with_file`](Self::with_file).
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::with_file(path)
    }

    /// Loads and parses ID3 tags from the given file path (legacy compatibility alias).
    ///
    /// Delegates to [`load_from_file`](Self::load_from_file).
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load_from_file(path)
    }

    /// Returns a reference to the parsed ID3 tags.
    ///
    /// Currently always returns `Some`; the `Option` wrapper is kept for API
    /// compatibility with callers that expect it.
    pub fn tags(&self) -> Option<&ID3Tags> {
        Some(&self.tags)
    }

    /// Returns a mutable reference to the parsed ID3 tags, allowing in-place edits.
    ///
    /// Currently always returns `Some`; the `Option` wrapper is kept for API
    /// compatibility with callers that expect it.
    pub fn tags_mut(&mut self) -> Option<&mut ID3Tags> {
        Some(&mut self.tags)
    }

    /// Simple save method without parameters
    pub fn save(&mut self) -> Result<()> {
        debug_event!("saving ID3 file");
        // Use the version from tags if available, otherwise default to v2.4
        // Only v2.3 and v2.4 are supported for writing
        let tag_version = self.tags.version().1;
        let v2_version = if tag_version == 3 || tag_version == 4 {
            tag_version
        } else {
            4 // Default to v2.4 for unsupported versions
        };
        self.save_with_options(None, ID3v1SaveOptions::UPDATE, v2_version, Some("/"))
    }

    /// Removes all ID3v1 and ID3v2 tags from the associated file.
    ///
    /// Delegates to [`delete_full`](Self::delete_full) with both `delete_v1`
    /// and `delete_v2` set to `true`, using the stored filename.
    pub fn clear(&mut self) -> Result<()> {
        self.delete_full(None, true, true)
    }

    /// Full save method with all parameters
    pub fn save_with_options(
        &mut self,
        filething: Option<&Path>,
        v1: ID3v1SaveOptions,
        v2_version: u8,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        self.save_with_padding(filething, v1, v2_version, v23_sep, None)
    }

    /// Internal save method with padding support
    fn save_with_padding(
        &mut self,
        filething: Option<&Path>,
        v1: ID3v1SaveOptions,
        v2_version: u8,
        v23_sep: Option<&str>,
        padding: Option<fn(&PaddingInfo) -> i64>,
    ) -> Result<()> {
        let filename = filething
            .map(Path::to_path_buf)
            .or_else(|| self.filename.as_ref().map(PathBuf::from));
        if let Some(path) = filename {
            trace_event!(
                path = %path.display(),
                v2_version = v2_version,
                "writing ID3 tags to file"
            );

            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)?;

            self.save_to_writer_inner(&mut file, v1, v2_version, v23_sep, padding)
        } else {
            warn_event!("no filename provided for ID3 save");
            Err(AudexError::InvalidData("No filename provided".to_string()))
        }
    }

    /// Core save implementation that writes to any seekable reader/writer.
    ///
    /// This contains the shared logic used by both file-based saving
    /// (`save_with_padding`) and writer-based saving (`save_to_writer`).
    /// The caller is responsible for providing an already-opened handle.
    ///
    /// For concrete `Sized + 'static` types (e.g. `File`), this method uses
    /// `insert_bytes` / `delete_bytes` for efficient in-place byte
    /// manipulation.
    fn save_to_writer_inner<W: Read + Write + Seek + 'static>(
        &mut self,
        writer: &mut W,
        v1: ID3v1SaveOptions,
        v2_version: u8,
        v23_sep: Option<&str>,
        padding: Option<fn(&PaddingInfo) -> i64>,
    ) -> Result<()> {
        // Try to read existing header
        let old_size = {
            writer.seek(SeekFrom::Start(0))?;
            let mut header_data = [0u8; 10];
            match writer.read_exact(&mut header_data) {
                Ok(()) if &header_data[0..3] == b"ID3" => {
                    let header = ID3Header::from_bytes(&header_data)?;
                    header.size
                }
                _ => 0, // No existing header
            }
        };

        // Prepare new tag data - use 0 for available to allow natural shrinking
        let data = self._prepare_data(
            writer, 0, 0, // Don't constrain by old size, allow natural shrinking
            v2_version, v23_sep, padding,
        )?;

        let new_size = data.len();
        // Widen to u64 before adding the 10-byte header length to prevent
        // overflow on 32-bit platforms where usize is 32 bits
        let old_total_size = if old_size > 0 {
            let total = (old_size as u64).checked_add(10).ok_or_else(|| {
                AudexError::InvalidData(
                    "Old tag size overflow when adding header length".to_string(),
                )
            })?;
            usize::try_from(total).map_err(|_| {
                AudexError::InvalidData(format!(
                    "Old tag total size ({} bytes) exceeds addressable range",
                    total
                ))
            })?
        } else {
            0
        }; // Include 10-byte header in old size only if tag exists

        // Adjust file size if needed
        if old_total_size < new_size {
            // Need to insert bytes
            insert_bytes(
                writer,
                (new_size - old_total_size) as u64,
                old_total_size as u64,
                None,
            )?;
        } else if old_total_size > new_size {
            // Need to delete bytes
            delete_bytes(
                writer,
                (old_total_size - new_size) as u64,
                new_size as u64,
                None,
            )?;
        }

        // Write the new tag data
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&data)?;

        // Handle ID3v1 tag
        self.__save_v1(writer, v1)?;

        Ok(())
    }

    /// Writer-based save that operates on any `Read + Write + Seek` trait
    /// object.
    ///
    /// Unlike `save_to_writer_inner`, this works with unsized trait objects
    /// (`dyn ReadWriteSeek`) by copying all data into an intermediate
    /// `Cursor<Vec<u8>>`, performing the in-place tag update there (which
    /// supports both growing and shrinking via `insert_bytes` /
    /// `delete_bytes`), and then writing the result back to the original
    /// writer.
    ///
    /// # Limitation: trailing zeros after tag shrink
    ///
    /// When the updated tag is smaller than the original, the stream's
    /// logical content shrinks but the writer cannot be truncated (trait
    /// objects do not expose `set_len`). The stale tail is overwritten
    /// with zeros to prevent data leakage, but the stream retains its
    /// original length. This is safe for all supported audio formats
    /// because the file structure's own size fields are authoritative —
    /// parsers ignore trailing zeros beyond the declared boundaries.
    ///
    /// If exact file sizing is required, use the file-path-based
    /// [`save`](Self::save) method instead, which can truncate the
    /// underlying file.
    ///
    /// # Memory usage
    ///
    /// This method reads the entire file into an in-memory buffer, applies
    /// tag modifications on a `Cursor`, then writes the result back. Peak
    /// memory consumption is approximately **2x the file size** (the read
    /// buffer plus the modified output). A 512 MB hard ceiling is enforced
    /// before allocation. For large files, prefer the file-path-based
    /// [`save`](Self::save) method which operates directly on the file
    /// handle without buffering the full stream.
    fn save_to_writer_dyn(
        &mut self,
        writer: &mut dyn ReadWriteSeek,
        v1: ID3v1SaveOptions,
        v2_version: u8,
        v23_sep: Option<&str>,
        padding: Option<fn(&PaddingInfo) -> i64>,
    ) -> Result<()> {
        // Determine total file size before buffering to prevent OOM
        // on very large files. This path reads the entire stream into
        // memory, so enforce a hard ceiling independent of tag-size limits.
        // 512 MB is generous for any audio file that needs in-memory saving.
        const MAX_IN_MEMORY_FILE: u64 = 512 * 1024 * 1024;
        let file_size = writer.seek(SeekFrom::End(0))?;
        if file_size > MAX_IN_MEMORY_FILE {
            return Err(crate::AudexError::InvalidData(format!(
                "file size ({} bytes) exceeds the {} byte limit for the in-memory save path; \
                 consider saving to a file directly instead",
                file_size, MAX_IN_MEMORY_FILE
            )));
        }

        // Read all existing data into memory.
        // NOTE: This buffers the entire file, which is safe given the size guard
        // above. A future optimisation could stream the audio data tail instead
        // of buffering it, reducing peak memory from O(file_size) to O(tag_size).
        writer.seek(SeekFrom::Start(0))?;
        let mut buf = Vec::new();
        writer.read_to_end(&mut buf)?;
        let original_len = buf.len();

        // Perform the save on an in-memory Cursor (Sized + 'static)
        let mut cursor = Cursor::new(buf);
        self.save_to_writer_inner(&mut cursor, v1, v2_version, v23_sep, padding)?;

        // Write the modified data back to the original writer
        let result = cursor.into_inner();
        let new_len = result.len();
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&result)?;

        // If the new data is shorter than the original, zero out the
        // stale trailing bytes. We cannot call set_len() on a trait
        // object, but zeroing ensures no leftover content leaks.
        // Write in fixed-size chunks to avoid a single large allocation
        // when the gap is very large.
        if new_len < original_len {
            let mut remaining = original_len - new_len;
            const ZERO_CHUNK: [u8; 8192] = [0u8; 8192];
            while remaining > 0 {
                let chunk = remaining.min(ZERO_CHUNK.len());
                writer.write_all(&ZERO_CHUNK[..chunk])?;
                remaining -= chunk;
            }
        }

        Ok(())
    }

    /// Get module name identifier
    pub const MODULE: &'static str = "audex.id3";

    /// Get ID3 version as (2, major, revision) -- e.g. (2, 4, 0) for ID3v2.4
    pub fn version(&self) -> (u8, u8, u8) {
        if let Some(ref header) = self._header {
            (2, header.major_version, header.revision)
        } else {
            self._version
        }
    }

    /// Set ID3 version
    pub fn set_version(&mut self, value: (u8, u8, u8)) {
        self._version = value;
    }

    /// Check if unsynchronization flag is set
    pub fn f_unsynch(&self) -> bool {
        if let Some(ref header) = self._header {
            (header.flags & 0x80) != 0 // Check unsynchronization flag
        } else {
            false
        }
    }

    /// Check if extended header flag is set
    pub fn f_extended(&self) -> bool {
        if let Some(ref header) = self._header {
            (header.flags & 0x40) != 0 // Check extended header flag
        } else {
            false
        }
    }

    /// Get total size of ID3 tag body (excludes the 10-byte header)
    pub fn size(&self) -> u32 {
        if let Some(ref header) = self._header {
            header.size
        } else {
            0
        }
    }

    /// Pre-load header hook for format-specific adjustments
    fn _pre_load_header<R: Read + Seek>(&mut self, _fileobj: &mut R) -> Result<()> {
        Ok(())
    }

    /// Load tags from a filename
    ///
    /// Args:
    ///     filething: filename or file object to load tag data from
    ///     known_frames: map of frame IDs to Frame objects
    ///     translate: Update all tags to ID3v2.3/4 internally. If you
    ///         intend to save, this must be true or you have to
    ///         call update_to_v23() / update_to_v24() manually.
    ///     v2_version: if update_to_v23 or update_to_v24 get called (3 or 4)
    ///     load_v1: Load tags from ID3v1 header if present. If both
    ///         ID3v1 and ID3v2 headers are present, combine the tags from
    ///         the two, with ID3v2 having precedence.
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all, fields(path = %filething.as_ref().display())))]
    pub fn load<P: AsRef<Path>>(
        &mut self,
        filething: P,
        known_frames: Option<HashMap<String, String>>,
        translate: bool,
        v2_version: u8,
        load_v1: bool,
    ) -> Result<()> {
        debug_event!("parsing ID3v2 tags");
        if v2_version != 3 && v2_version != 4 {
            return Err(AudexError::InvalidData(
                "Only 3 and 4 possible for v2_version".to_string(),
            ));
        }

        // Store filename for later use
        self.filename = Some(filething.as_ref().to_string_lossy().to_string());

        // Clear existing state
        self.unknown_frames.clear();
        self._header = None;
        self._padding = 0;

        // Open file for reading
        let mut file = File::open(filething.as_ref())?;

        // Pre-load header hook for format-specific adjustments
        self._pre_load_header(&mut file)?;

        // Try to parse ID3v2 header
        file.seek(SeekFrom::Start(0))?;
        let mut header_data = [0u8; 10];
        match file.read_exact(&mut header_data) {
            Ok(()) if &header_data[0..3] == b"ID3" => {
                let header = ID3Header::from_bytes(&header_data)?;
                // Successfully parsed ID3v2 header
                self._header = Some(header);

                // Store known frames in header if provided

                // Read the full tag data (header.size excludes the 10-byte tag header)
                let size = self.size();

                // Enforce the library-wide tag allocation ceiling
                crate::limits::ParseLimits::default().check_tag_size(size as u64, "ID3v2")?;

                // Cross-validate against actual stream size to prevent
                // allocating far more memory than the file contains
                let current_pos = file.stream_position()?;
                let stream_end = file.seek(SeekFrom::End(0))?;
                let available = stream_end.saturating_sub(current_pos);
                if (size as u64) > available {
                    return Err(AudexError::ParseError(format!(
                        "ID3 tag size ({} bytes) exceeds remaining file data ({} bytes)",
                        size, available
                    )));
                }
                file.seek(SeekFrom::Start(current_pos))?;

                let data = read_full(&mut file, size as usize)?;

                // Skip the extended header if present. Its size is encoded
                // in the first 4 bytes of the tag data:
                //   ID3v2.3: regular BE u32 (excludes the 4-byte size field)
                //   ID3v2.4: syncsafe u32 (includes the 4-byte size field)
                let header_ref = self._header.as_ref().ok_or_else(|| {
                    AudexError::InvalidData("ID3 header not set during load".to_string())
                })?;

                let frame_data = if self.f_extended() && data.len() >= 4 {
                    let ext_size = if header_ref.major_version == 4 {
                        crate::id3::util::decode_synchsafe_int_checked(&data[0..4])? as usize
                    } else {
                        // Use checked_add to prevent overflow on 32-bit platforms
                        // when the declared size is near u32::MAX
                        let raw = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as u64;
                        match raw.checked_add(4) {
                            Some(size) => size.min(data.len() as u64) as usize,
                            None => data.len(), // overflow: treat entire block as header
                        }
                    };
                    let skip = ext_size.min(data.len());
                    &data[skip..]
                } else {
                    &data
                };

                let header_clone = header_ref.clone();
                let remaining_data = self._read(&header_clone, frame_data)?;
                self._padding = remaining_data.len();

                // Load ID3v1 if requested and present — only read the tail
                if load_v1 {
                    let v1v2_ver = if self.version().1 == 4 { 4 } else { 3 };

                    if let Ok((Some(frames), _offset)) =
                        find_id3v1_from_reader(&mut file, v1v2_ver, known_frames.clone())
                    {
                        for (_, frame) in frames {
                            // Only add if no existing frame with same hash key
                            if self.tags.getall(&frame.hash_key()).is_empty() {
                                let _ = self.tags.add(frame);
                            }
                        }
                    }
                }
            }
            _ => {
                // No ID3v2 header found, try ID3v1 if requested
                if !load_v1 {
                    return Err(AudexError::ID3NoHeaderError);
                }

                // Only read the tail of the file for ID3v1 detection
                match find_id3v1_from_reader(&mut file, v2_version, known_frames) {
                    Ok((frames, _offset)) => {
                        if let Some(frames) = frames {
                            self._version = (1, 1, 0); // ID3v1.1
                            for (_, frame) in frames {
                                if self.tags.getall(&frame.hash_key()).is_empty() {
                                    let _ = self.tags.add(frame);
                                }
                            }
                        } else {
                            return Err(AudexError::InvalidData("No ID3 tags found".to_string()));
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Log frame count summary after parsing
        debug_event!(
            frame_count = self.tags.frames_by_id.len(),
            "ID3 frames loaded"
        );

        // Translate to requested version if needed
        if translate {
            if v2_version == 3 {
                self.tags.update_to_v23();
                self._version = (2, 3, 0);
            } else {
                self.tags.update_to_v24();
                self._version = (2, 4, 0);
            }
        }

        // Populate the text values cache so Tags::get returns real data
        self.refresh_values_cache();

        Ok(())
    }

    /// Prepare tag data for writing
    fn _prepare_data<R, F>(
        &self,
        fileobj: &mut R,
        start: u64,
        available: usize,
        v2_version: u8,
        v23_sep: Option<&str>,
        pad_func: Option<F>,
    ) -> Result<Vec<u8>>
    where
        R: Read + Seek,
        F: FnOnce(&PaddingInfo) -> i64,
    {
        if v2_version != 3 && v2_version != 4 {
            return Err(AudexError::InvalidData(
                "Only 3 or 4 allowed for v2_version".to_string(),
            ));
        }

        // Create save config
        let config = ID3SaveConfig::simple(v2_version, v23_sep.map(|s| s.to_string()))?;

        // Write frame data
        let framedata = self.tags.write_tags(&config)?;

        let needed = framedata.len() + 10;

        // Get file size for padding calculation
        fileobj.seek(SeekFrom::End(0))?;
        let file_end = fileobj.stream_position()?;
        // Use saturating subtraction to avoid panic if start exceeds
        // file_end due to corrupt metadata declaring an offset past EOF
        let trailing_size = file_end.saturating_sub(start);

        // Calculate padding using safe conversions to prevent silent
        // truncation of large usize values when cast to i64.
        let available_i64 = i64::try_from(available).map_err(|_| {
            AudexError::InvalidData("Available space too large for padding calculation".to_string())
        })?;
        let needed_i64 = i64::try_from(needed).map_err(|_| {
            AudexError::InvalidData("Needed space too large for padding calculation".to_string())
        })?;
        let trailing_i64 = i64::try_from(trailing_size).map_err(|_| {
            AudexError::InvalidData("Trailing size too large for padding calculation".to_string())
        })?;
        let info = PaddingInfo::new(available_i64 - needed_i64, trailing_i64);

        let new_padding = info.get_padding_with(pad_func);

        if new_padding < 0 {
            return Err(AudexError::InvalidData("Invalid padding".to_string()));
        }

        let padding_usize = usize::try_from(new_padding)
            .map_err(|_| AudexError::InvalidData("Padding value out of usize range".to_string()))?;
        let new_size = needed.checked_add(padding_usize).ok_or_else(|| {
            AudexError::InvalidData(
                "Tag size overflow: frame data + padding exceeds maximum".to_string(),
            )
        })?;

        // The ID3v2 header stores the tag body size (excluding the 10-byte
        // header) as a synchsafe 4-byte integer, which can represent at
        // most 2^28 - 1 = 268_435_455 bytes.  Reject sizes that would
        // silently truncate during encoding.
        let body_size = new_size - 10; // safe: needed >= 10, new_padding >= 0
        if body_size > 0x0FFF_FFFF {
            return Err(AudexError::InvalidData(format!(
                "Tag body size {} exceeds the ID3v2 synchsafe maximum (268_435_455 bytes)",
                body_size,
            )));
        }

        // Create synchsafe size for header
        let new_framesize =
            BitPaddedInt::to_str(body_size as u32, Some(7), Some(true), Some(4), Some(4))?;

        // Build header: ID3 + version + flags + size
        let mut header = Vec::new();
        header.extend_from_slice(b"ID3");
        header.push(v2_version);
        header.push(0); // revision
        header.push(0); // flags
        header.extend_from_slice(&new_framesize);

        // Combine header + frame data + padding
        let mut data = header;
        data.extend_from_slice(&framedata);

        // Add padding (zeros)
        let padding_needed = new_size - data.len();
        data.extend(vec![0u8; padding_needed]);

        if new_size != data.len() {
            return Err(AudexError::InvalidData(format!(
                "ID3 tag size mismatch: expected {} bytes but produced {} bytes",
                new_size,
                data.len()
            )));
        }

        Ok(data)
    }

    /// Save ID3v1 tag
    fn __save_v1<W: Write + Seek + Read + 'static>(
        &self,
        f: &mut W,
        v1: ID3v1SaveOptions,
    ) -> Result<()> {
        // Only read the tail to check for an existing ID3v1 tag.
        // Use i64::try_from to avoid silent truncation for files
        // whose length exceeds i64::MAX.
        let file_len = i64::try_from(f.seek(SeekFrom::End(0))?).map_err(|_| {
            AudexError::InvalidData(
                "File length exceeds i64::MAX; cannot safely compute seek positions".to_string(),
            )
        })?;

        let (existing_tag, offset) = match find_id3v1_from_reader(f, 4, None) {
            Ok((frames, offset)) => (frames.is_some(), offset),
            // No tag found — offset is unused since the !existing_tag
            // branch seeks to file_len directly, but use 0 for correctness.
            Err(_) => (false, 0),
        };

        // Position at the ID3v1 location (or end of file).
        // The offset from find_id3v1 is relative to the tail buffer end
        // and is typically negative (e.g., -128).
        // Use checked arithmetic to prevent signed integer overflow
        // when combining file length with the tag offset.
        let seek_pos = if existing_tag {
            file_len.checked_add(offset).ok_or_else(|| {
                AudexError::InvalidData(
                    "Seek position overflow when calculating ID3v1 tag location".to_string(),
                )
            })?
        } else {
            file_len
        };

        if seek_pos < 0 {
            return Err(AudexError::InvalidData(
                "ID3v1 seek position would be negative".to_string(),
            ));
        }
        f.seek(SeekFrom::Start(seek_pos as u64))?;

        match v1 {
            ID3v1SaveOptions::UPDATE if existing_tag => {
                // Update existing ID3v1 tag
                let id3v1_data = self.create_id3v1_data()?;
                f.write_all(&id3v1_data)?;
            }
            ID3v1SaveOptions::CREATE => {
                // Create or update ID3v1 tag
                let id3v1_data = self.create_id3v1_data()?;
                f.write_all(&id3v1_data)?;
            }
            ID3v1SaveOptions::REMOVE => {
                // Remove the existing 128-byte ID3v1 tag by truncating the file
                if existing_tag {
                    crate::util::delete_bytes(f, 128, seek_pos as u64, None)?;
                }
            }
            _ => {
                // UPDATE but no existing tag - do nothing
            }
        }

        Ok(())
    }

    /// Remove tags from a file with full parameters
    pub fn delete_full(
        &mut self,
        filething: Option<&Path>,
        delete_v1: bool,
        delete_v2: bool,
    ) -> Result<()> {
        let filename = filething.or_else(|| self.filename.as_ref().map(Path::new));
        if let Some(path) = filename {
            clear(path, delete_v1, delete_v2)?;
        }

        // Reset in-memory state directly to avoid infinite recursion
        // (self.clear() calls delete_full(), which calls self.clear(), ...)
        self.tags = ID3Tags::new();
        self.unknown_frames.clear();
        self._header = None;
        self._padding = 0;

        Ok(())
    }

    /// Save ID3 tags to a writer that implements `Read + Write + Seek`.
    ///
    /// The writer must contain the complete original file data (audio + any
    /// existing tags). The method modifies the writer in-place, identical to
    /// how [`save`](Self::save) modifies a file on disk.
    pub fn save_to_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        let tag_version = self.tags.version().1;
        let v2_version = if tag_version == 3 || tag_version == 4 {
            tag_version
        } else {
            4
        };
        self.save_to_writer_dyn(
            writer,
            ID3v1SaveOptions::UPDATE,
            v2_version,
            Some("/"),
            None,
        )
    }

    /// Remove all ID3v1 and ID3v2 tags from a writer that implements
    /// `Read + Write + Seek`.
    ///
    /// The writer must contain the complete original file data.
    pub fn clear_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        clear_from_writer(writer, true, true)
    }

    /// Create ID3v1 data from current tags
    fn create_id3v1_data(&self) -> Result<Vec<u8>> {
        let v1_data = crate::id3::id3v1::make_id3v1_from_dict(&self.tags.dict);
        Ok(v1_data.to_vec())
    }

    /// Read frames from tag data
    fn _read(&mut self, header: &crate::id3::specs::ID3Header, data: &[u8]) -> Result<Vec<u8>> {
        let mut cursor = Cursor::new(data);
        let mut remaining_data = Vec::new();
        let mut frame_count: usize = 0;

        // Legitimate tags rarely exceed a few hundred frames. Cap the total
        // to prevent excessive allocations from crafted tags packed with
        // millions of tiny frames (up to ~23 million within the 256 MB
        // syncsafe ceiling). This matches the limit used in the tags-level
        // parser for consistency.
        const MAX_FRAMES_PER_TAG: usize = 50_000;

        // Parse frames until we hit padding, end of data, or the frame cap
        while (cursor.position() as usize) < data.len() && frame_count < MAX_FRAMES_PER_TAG {
            let pos = cursor.position() as usize;

            // Check if we've hit padding by examining the next 10 bytes.
            // A full scan of all remaining bytes would be O(n) per iteration,
            // potentially O(n²) overall. Checking a small prefix is sufficient
            // since valid frame headers always have non-zero bytes.
            let check_len = 10.min(data.len() - pos);
            if data[pos..pos + check_len].iter().all(|&b| b == 0) {
                remaining_data = data[pos..].to_vec();
                break;
            }

            // Try to read frame header
            match self._read_frame_header(&mut cursor, header.major_version) {
                Ok(Some(frame_header)) => {
                    frame_count += 1;

                    // A zero-size frame indicates the start of padding; stop parsing.
                    let frame_size = frame_header.size as usize;
                    if frame_size == 0 {
                        remaining_data = data[pos..].to_vec();
                        break;
                    }

                    if (cursor.position() as usize) + frame_size > data.len() {
                        // Invalid frame size, treat as padding
                        remaining_data = data[pos..].to_vec();
                        break;
                    }

                    let mut frame_data = vec![0u8; frame_size];
                    cursor.read_exact(&mut frame_data)?;

                    // Try to parse the frame
                    match self._parse_frame(&frame_header, &frame_data) {
                        Ok(Some(frame)) => {
                            let _ = self.tags.add(frame);
                        }
                        Ok(None) => {
                            // Frame parsing succeeded but returned no frame (e.g., empty frame)
                        }
                        Err(_e) => {
                            // Unknown or invalid frame, store as unknown.
                            // Note: `_parse_frame` takes `&[u8]` (immutable reference),
                            // so `frame_data` still contains the original pre-processing
                            // bytes read from the cursor. Any unsync decoding or
                            // decompression happens on internal copies within `_parse_frame`,
                            // ensuring we store the untouched raw bytes here.
                            let mut unknown_frame = Vec::new();
                            unknown_frame.extend_from_slice(frame_header.frame_id.as_bytes());
                            // ID3v2.4 requires synchsafe frame sizes; v2.3 uses plain big-endian.
                            // Safety: frame_data was allocated from frame_header.size (u32),
                            // so its length always fits in u32. Use try_from as a defensive
                            // guard in case the allocation logic is ever changed.
                            let frame_len = u32::try_from(frame_data.len()).map_err(|_| {
                                AudexError::InvalidData(format!(
                                    "Unknown frame data length {} exceeds u32::MAX",
                                    frame_data.len(),
                                ))
                            })?;
                            let size_bytes = if frame_header.version == (2, 4) {
                                crate::id3::util::encode_synchsafe_int(frame_len)
                                    .unwrap_or(frame_len.to_be_bytes())
                            } else {
                                frame_len.to_be_bytes()
                            };
                            unknown_frame.extend_from_slice(&size_bytes);
                            unknown_frame.extend_from_slice(
                                &frame_header
                                    .flags
                                    .to_raw(frame_header.version)
                                    .to_be_bytes(),
                            );
                            unknown_frame.extend_from_slice(&frame_data);
                            self.unknown_frames.push(unknown_frame);
                        }
                    }
                }
                Ok(None) => {
                    // No more valid frame headers, rest is padding
                    remaining_data = data[pos..].to_vec();
                    break;
                }
                Err(_) => {
                    // Invalid frame header, treat rest as padding
                    remaining_data = data[pos..].to_vec();
                    break;
                }
            }
        }

        Ok(remaining_data)
    }

    /// Read frame header from cursor
    fn _read_frame_header(
        &self,
        cursor: &mut Cursor<&[u8]>,
        version: u8,
    ) -> Result<Option<crate::id3::specs::FrameHeader>> {
        use crate::id3::specs::FrameHeader;

        let pos = cursor.position() as usize;
        let data = cursor.get_ref();

        if pos >= data.len() {
            return Ok(None);
        }

        // Check for valid frame ID based on version
        let id_len = if version == 2 { 3 } else { 4 };
        if pos + id_len > data.len() {
            return Ok(None);
        }

        let frame_id = String::from_utf8_lossy(&data[pos..pos + id_len]);

        // Validate frame ID: spec requires uppercase ASCII letters and digits only.
        // Accepting lowercase would misidentify corrupted data as valid frames.
        if frame_id
            .chars()
            .any(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit()))
        {
            return Ok(None);
        }

        // Read frame header based on version
        match version {
            2 => {
                if pos + 6 > data.len() {
                    return Ok(None);
                }
                let size_bytes = &data[pos + 3..pos + 6];
                let size = u32::from_be_bytes([0, size_bytes[0], size_bytes[1], size_bytes[2]]);

                // Validate that the claimed frame size does not exceed the
                // remaining tag data after the 6-byte v2.2 header.
                let remaining_after_header = data.len().saturating_sub(pos + 6);
                if (size as usize) > remaining_after_header {
                    return Ok(None);
                }

                cursor.set_position((pos + 6) as u64);
                let mut header = FrameHeader::new(frame_id.to_string(), size, 0, (2, 2));
                // Propagate the tag-level unsynchronization flag so that
                // frame processors can apply unsync decoding when needed
                header.global_unsync = self.f_unsynch();
                Ok(Some(header))
            }
            3 | 4 => {
                if pos + 10 > data.len() {
                    return Ok(None);
                }
                let size = if version == 4 {
                    // ID3v2.4 uses synchsafe integers
                    BitPaddedInt::new((&data[pos + 4..pos + 8]).into(), Some(7), Some(true))?
                        .value()
                } else {
                    // ID3v2.3 uses regular big-endian integers
                    u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
                };
                let flags = u16::from_be_bytes([data[pos + 8], data[pos + 9]]);
                cursor.set_position((pos + 10) as u64);
                let mut header = FrameHeader::new(frame_id.to_string(), size, flags, (2, version));
                // Propagate the tag-level unsynchronization flag so that
                // frame processors can apply unsync decoding when needed
                header.global_unsync = self.f_unsynch();
                Ok(Some(header))
            }
            _ => Ok(None),
        }
    }

    /// Parse frame data into a Frame object
    fn _parse_frame(
        &self,
        header: &crate::id3::specs::FrameHeader,
        data: &[u8],
    ) -> Result<Option<Box<dyn Frame>>> {
        // Process frame data (handle compression, unsync, group ID, data length indicator, etc.)
        use crate::id3::specs::FrameProcessor;
        let processed_data = match FrameProcessor::process_read(header, data.to_vec()) {
            Ok(data) => data,
            Err(_) => {
                // If processing fails, skip this frame
                return Ok(None);
            }
        };

        // Use frame registry to create appropriate frame type
        match FrameRegistry::create_frame(&header.frame_id, &processed_data) {
            Ok(frame) => Ok(Some(frame)),
            Err(_) => Ok(None),
        }
    }

    /// Creates a new `ID3` instance by reading and parsing tags from the given
    /// file using non-blocking I/O.
    ///
    /// Both ID3v2 (at the start of the file) and ID3v1 (last 128 bytes) are
    /// checked. If only an ID3v1 tag is present, its frames are loaded into the
    /// tag collection. Requires the `async` feature and a Tokio runtime.
    #[cfg(feature = "async")]
    pub async fn with_file_async<P: AsRef<Path>>(filething: P) -> Result<Self> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};

        let mut instance = Self::new();
        instance.filename = Some(filething.as_ref().to_string_lossy().to_string());

        // Open file asynchronously
        let mut file = TokioFile::open(filething.as_ref()).await?;

        // Check for ID3v2 header
        let mut header_data = [0u8; 10];
        file.read_exact(&mut header_data).await?;

        // Track the file's ID3v2 major version (used for v1 loading below)
        let mut file_v2_major: u8 = 4; // Default to 4 if no v2 tag found
        if &header_data[0..3] == b"ID3" {
            let vmaj = header_data[3];
            file_v2_major = vmaj;
            let _vrev = header_data[4];
            let _flags = header_data[5];
            let size_bytes = &header_data[6..10];

            // Parse header
            let header = ID3Header::from_bytes(&header_data)?;

            // Get tag size
            let size = BitPaddedInt::new(size_bytes.into(), Some(7), Some(true))?.value();

            // Enforce the library-wide tag allocation ceiling
            crate::limits::ParseLimits::default().check_tag_size(size as u64, "ID3v2 async")?;

            // Cross-validate against actual stream size
            let current_pos = file.stream_position().await?;
            let stream_end = file.seek(SeekFrom::End(0)).await?;
            let available = stream_end.saturating_sub(current_pos);
            if (size as u64) > available {
                return Err(AudexError::ParseError(format!(
                    "ID3 tag size ({} bytes) exceeds remaining file data ({} bytes)",
                    size, available
                )));
            }
            file.seek(SeekFrom::Start(current_pos)).await?;

            // Read tag data
            let mut tag_data = vec![0u8; size as usize];
            file.read_exact(&mut tag_data).await?;

            // Skip extended header if present (flag bit 6)
            let frame_data = if (_flags & 0x40) != 0 && tag_data.len() >= 4 {
                let ext_size = if vmaj == 4 {
                    crate::id3::util::decode_synchsafe_int_checked(&tag_data[0..4])? as usize
                } else {
                    // Use checked arithmetic to prevent overflow on 32-bit platforms
                    // when the declared size is near u32::MAX
                    let raw =
                        u32::from_be_bytes([tag_data[0], tag_data[1], tag_data[2], tag_data[3]])
                            as u64;
                    match raw.checked_add(4) {
                        Some(size) => size.min(tag_data.len() as u64) as usize,
                        None => tag_data.len(), // overflow: treat entire block as header
                    }
                };
                let skip = ext_size.min(tag_data.len());
                &tag_data[skip..]
            } else {
                &tag_data
            };

            // Parse frames
            let _ = instance._read(&header, frame_data)?;
            instance._header = Some(header);
            instance._version = (2, vmaj, 0);
        }

        // Check for ID3v1 at end of file
        let file_size = file.seek(SeekFrom::End(0)).await?;
        if file_size >= 128 {
            file.seek(SeekFrom::End(-128)).await?;
            let mut v1_data = [0u8; 128];
            file.read_exact(&mut v1_data).await?;

            if &v1_data[0..3] == b"TAG" {
                // Load ID3v1 tags and merge with ID3v2 tags
                // Use the actual file version (not always 4) to match sync load behavior
                let v1v2_ver = if file_v2_major == 4 { 4 } else { 3 };
                if let Ok((Some(v1_frames), _)) = find_id3v1(&v1_data, v1v2_ver, None) {
                    for (_, frame) in v1_frames {
                        // Only add if no existing frame with same hash key
                        if instance.tags.getall(&frame.hash_key()).is_empty() {
                            let _ = instance.tags.add(frame);
                        }
                    }
                }
            }
        }

        // Translate to v2.4 (matching sync load behavior)
        instance.tags.update_to_v24();
        instance._version = (2, 4, 0);

        // Populate the text values cache so Tags::get returns real data
        instance.refresh_values_cache();

        Ok(instance)
    }

    /// Loads and parses ID3 tags from the given file path asynchronously.
    ///
    /// This is an async alias for [`with_file_async`](Self::with_file_async).
    #[cfg(feature = "async")]
    pub async fn load_from_file_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::with_file_async(path).await
    }

    /// Loads and parses ID3 tags from the given file path asynchronously
    /// (legacy compatibility alias).
    ///
    /// Delegates to [`load_from_file_async`](Self::load_from_file_async).
    #[cfg(feature = "async")]
    pub async fn from_file_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load_from_file_async(path).await
    }

    /// Save ID3 tags to file asynchronously.
    ///
    /// Writes the current tags back to the file using non-blocking I/O.
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        let filename = self
            .filename
            .clone()
            .ok_or_else(|| AudexError::InvalidData("No filename set".to_string()))?;

        // Generate tag data
        let tag_data = self.tags.to_bytes()?;

        // Open file for read/write
        let mut file = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(&filename)
            .await?;

        // Find existing ID3v2 tag size and cross-validate against the
        // actual file length to reject corrupt or oversized headers.
        let mut header = [0u8; 10];
        let old_size = if file.read_exact(&mut header).await.is_ok() && &header[0..3] == b"ID3" {
            let size = BitPaddedInt::new((&header[6..10]).into(), Some(7), Some(true))?.value();
            let total = (size as u64).checked_add(10).ok_or_else(|| {
                AudexError::InvalidData(
                    "Old tag size overflow when adding header length".to_string(),
                )
            })?;

            let stream_end = file.seek(SeekFrom::End(0)).await?;
            if total > stream_end {
                return Err(AudexError::ParseError(format!(
                    "ID3 tag size ({total} bytes) exceeds file size ({stream_end} bytes)",
                )));
            }

            total
        } else {
            0
        };

        let new_size = tag_data.len() as u64;

        // Resize file if needed
        if old_size != new_size {
            resize_bytes_async(&mut file, old_size, new_size, 0).await?;
        }

        // Write new tag
        file.seek(SeekFrom::Start(0)).await?;
        file.write_all(&tag_data).await?;
        file.flush().await?;

        Ok(())
    }

    /// Full save method with all parameters asynchronously.
    ///
    /// Writes the current tags back to the file using non-blocking I/O with
    /// custom ID3v1 and ID3v2 configuration options.
    ///
    /// # Arguments
    /// * `filething` - Optional path to save to (uses stored filename if None)
    /// * `v1` - ID3v1 save options (REMOVE, UPDATE, or CREATE)
    /// * `v2_version` - ID3v2 version (3 or 4)
    /// * `v23_sep` - Optional separator for multi-value fields in v2.3
    ///
    /// # Returns
    /// * `Result<()>` - Success or an error if save fails
    #[cfg(feature = "async")]
    pub async fn save_with_options_async(
        &mut self,
        filething: Option<&Path>,
        v1: ID3v1SaveOptions,
        v2_version: u8,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

        // Validate version before building config (must be 3 or 4)
        if v2_version != 3 && v2_version != 4 {
            return Err(AudexError::InvalidData(
                "Only 3 or 4 allowed for v2_version".to_string(),
            ));
        }

        let filename = filething
            .map(|p| p.to_path_buf())
            .or_else(|| {
                self.filename
                    .as_ref()
                    .map(Path::new)
                    .map(|p| p.to_path_buf())
            })
            .ok_or_else(|| AudexError::InvalidData("No filename set".to_string()))?;

        // Build save configuration
        let config = ID3SaveConfig {
            v2_version,
            v2_minor: 0,
            v23_sep: v23_sep.unwrap_or("/").to_string(),
            v23_separator: v23_sep.unwrap_or("/").chars().next().unwrap_or('/') as u8,
            padding: None,
            merge_frames: false,
            preserve_unknown: false,
            compress_frames: false,
            write_v1: v1,
            unsync: false,
            extended_header: false,
            convert_v24_frames: true,
        };

        // Generate frame data with config
        let frame_data = self.tags.write_with_config(&config)?;

        // Build complete ID3v2 tag with header
        let mut tag_data = Vec::new();
        tag_data.extend_from_slice(b"ID3");
        tag_data.push(v2_version);
        tag_data.push(0); // revision
        tag_data.push(0); // flags

        // Write synchsafe size — validate the frame data fits
        if frame_data.len() > 0x0FFF_FFFF {
            return Err(AudexError::InvalidData(format!(
                "Tag data size {} exceeds the ID3v2 synchsafe maximum (268_435_455 bytes)",
                frame_data.len(),
            )));
        }
        let size = frame_data.len() as u32;
        let synchsafe = [
            ((size >> 21) & 0x7F) as u8,
            ((size >> 14) & 0x7F) as u8,
            ((size >> 7) & 0x7F) as u8,
            (size & 0x7F) as u8,
        ];
        tag_data.extend_from_slice(&synchsafe);
        tag_data.extend_from_slice(&frame_data);

        // Open file for read/write
        let mut file = TokioOpenOptions::new()
            .read(true)
            .write(true)
            .open(&filename)
            .await?;

        // Find existing ID3v2 tag size and cross-validate against the
        // actual file length to reject corrupt or oversized headers.
        let mut header = [0u8; 10];
        let old_size = if file.read_exact(&mut header).await.is_ok() && &header[0..3] == b"ID3" {
            let size = BitPaddedInt::new((&header[6..10]).into(), Some(7), Some(true))?.value();
            let total = (size as u64).checked_add(10).ok_or_else(|| {
                AudexError::InvalidData(
                    "Old tag size overflow when adding header length".to_string(),
                )
            })?;

            let stream_end = file.seek(SeekFrom::End(0)).await?;
            if total > stream_end {
                return Err(AudexError::ParseError(format!(
                    "ID3 tag size ({total} bytes) exceeds file size ({stream_end} bytes)",
                )));
            }

            total
        } else {
            0
        };

        let new_size = tag_data.len() as u64;

        // Resize file if needed
        if old_size != new_size {
            resize_bytes_async(&mut file, old_size, new_size, 0).await?;
        }

        // Write new tag
        file.seek(SeekFrom::Start(0)).await?;
        file.write_all(&tag_data).await?;
        file.flush().await?;

        // Handle ID3v1 tag at end of file
        {
            let file_len = file.seek(SeekFrom::End(0)).await?;
            let has_existing_v1 = if file_len >= 128 {
                let mut tag_header = [0u8; 3];
                file.seek(SeekFrom::End(-128)).await?;
                file.read_exact(&mut tag_header).await.is_ok() && &tag_header == b"TAG"
            } else {
                false
            };

            match config.write_v1 {
                ID3v1SaveOptions::CREATE => {
                    let v1_data = crate::id3::id3v1::make_id3v1_from_dict(&self.tags.dict);
                    if has_existing_v1 {
                        file.seek(SeekFrom::End(-128)).await?;
                    } else {
                        file.seek(SeekFrom::End(0)).await?;
                    }
                    file.write_all(&v1_data).await?;
                    file.flush().await?;
                }
                ID3v1SaveOptions::UPDATE if has_existing_v1 => {
                    let v1_data = crate::id3::id3v1::make_id3v1_from_dict(&self.tags.dict);
                    file.seek(SeekFrom::End(-128)).await?;
                    file.write_all(&v1_data).await?;
                    file.flush().await?;
                }
                _ => {}
            }
        }

        Ok(())
    }

    /// Clear all tags from the file asynchronously.
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        self.tags.clear();
        self.save_async().await
    }

    /// Delete the file asynchronously.
    #[cfg(feature = "async")]
    pub async fn delete_async(&mut self) -> Result<()> {
        if let Some(filename) = &self.filename {
            tokio::fs::remove_file(filename).await?;
        }
        Ok(())
    }
}

/// Async resize bytes helper for ID3
#[cfg(feature = "async")]
async fn resize_bytes_async(
    file: &mut TokioFile,
    old_size: u64,
    new_size: u64,
    offset: u64,
) -> Result<()> {
    if new_size > old_size {
        // Need to insert bytes
        insert_bytes_async(file, new_size - old_size, offset, None).await?;
    } else if new_size < old_size {
        // Need to delete bytes
        delete_bytes_async(file, old_size - new_size, offset, None).await?;
    }
    Ok(())
}

impl Default for ID3 {
    fn default() -> Self {
        Self::new()
    }
}

/// Removes ID3 tags directly from a file on disk without loading them into memory.
///
/// When `clear_v1` is `true`, any ID3v1 tag (last 128 bytes starting with `TAG`)
/// is truncated from the end of the file. When `clear_v2` is `true`, the ID3v2
/// header and its tag data at the start of the file are removed and the remaining
/// audio bytes are shifted forward.
pub fn clear(filething: &Path, clear_v1: bool, clear_v2: bool) -> Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(filething)?;

    // Clear ID3v1 if requested — only read the tail to locate the tag
    if clear_v1 {
        // Use try_from to avoid silent truncation for files whose
        // length exceeds i64::MAX.
        let file_len = i64::try_from(file.seek(SeekFrom::End(0))?).map_err(|_| {
            crate::AudexError::InvalidData(
                "File length exceeds i64::MAX; cannot safely compute seek positions".to_string(),
            )
        })?;

        if let Ok((frames, offset)) = find_id3v1_from_reader(&mut file, 4, None) {
            if frames.is_some() {
                // Truncate file to remove the ID3v1 tag at the tail.
                // Guard against underflow: if offset is more negative
                // than file_len, the file is too small to contain a
                // valid tag at that position.
                if file_len + offset < 0 {
                    return Err(crate::AudexError::ParseError(
                        "ID3v1 tag offset exceeds file size".to_string(),
                    ));
                }
                let new_size = (file_len + offset) as u64;
                file.set_len(new_size)?;
            }
        }
    }

    // Clear ID3v2 if requested
    if clear_v2 {
        file.seek(SeekFrom::Start(0))?;

        // Read potential ID3v2 header
        let mut header_data = [0u8; 10];
        match file.read_exact(&mut header_data) {
            Ok(()) => {
                // Check if this is an ID3v2 header
                if &header_data[0..3] == b"ID3" {
                    let vmaj = header_data[3];
                    let _vrev = header_data[4];
                    let _flags = header_data[5];
                    let size_bytes = &header_data[6..10];

                    if [2, 3, 4].contains(&vmaj) {
                        // Parse synchsafe integer for size
                        let size =
                            BitPaddedInt::new(size_bytes.into(), Some(7), Some(true))?.value();
                        // BitPaddedInt value should always be valid here
                        // Delete the entire ID3v2 tag (header + data)
                        delete_bytes(&mut file, size as u64 + 10, 0, None)?;
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                // A short file cannot contain a full ID3v2 header.
            }
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}

/// Removes ID3 tags from a writer that implements `Read + Write + Seek`.
///
/// Writer-based equivalent of [`clear`]. When `clear_v1` is `true`, any ID3v1
/// tag (last 128 bytes starting with `TAG`) is removed. When `clear_v2` is
/// `true`, the ID3v2 header and its tag data at the start are removed and the
/// remaining bytes are shifted forward.
///
/// The data is copied into an intermediate `Cursor<Vec<u8>>` so that both
/// growing and shrinking operations work regardless of the concrete writer
/// type.
pub fn clear_from_writer(
    writer: &mut dyn ReadWriteSeek,
    clear_v1: bool,
    clear_v2: bool,
) -> Result<()> {
    // Check the total file size before reading into memory to prevent
    // OOM on multi-gigabyte audio files. We only need to manipulate tags
    // (typically a few MB), not the entire audio payload.
    let file_size = writer.seek(SeekFrom::End(0))?;
    let max_read_size = crate::limits::MAX_IN_MEMORY_WRITER_FILE;
    if file_size > max_read_size {
        return Err(AudexError::InvalidData(format!(
            "File size ({} bytes) exceeds maximum for in-memory tag clearing ({} bytes). \
             Use file-based clearing instead.",
            file_size, max_read_size
        )));
    }

    // Read all data into memory
    writer.seek(SeekFrom::Start(0))?;
    let mut buf = Vec::new();
    writer.read_to_end(&mut buf)?;

    // Perform clearing on the in-memory cursor
    let mut cursor = Cursor::new(buf);

    // Clear ID3v1 if requested — use the tail reader on the cursor
    if clear_v1 {
        let cursor_len = i64::try_from(cursor.get_ref().len()).map_err(|_| {
            AudexError::InvalidData(
                "Buffer length exceeds i64::MAX; cannot safely compute seek positions".to_string(),
            )
        })?;

        if let Ok((frames, offset)) = find_id3v1_from_reader(&mut cursor, 4, None) {
            if frames.is_some() {
                if cursor_len + offset < 0 {
                    return Err(AudexError::InvalidData(
                        "ID3v1 offset points before the start of the buffer".to_string(),
                    ));
                }
                let v1_start = (cursor_len + offset) as u64;
                let v1_len = (cursor_len as u64).saturating_sub(v1_start);
                delete_bytes(&mut cursor, v1_len, v1_start, None)?;
            }
        }
    }

    // Clear ID3v2 if requested
    if clear_v2 {
        cursor.seek(SeekFrom::Start(0))?;

        let mut header_data = [0u8; 10];
        match cursor.read_exact(&mut header_data) {
            Ok(()) => {
                if &header_data[0..3] == b"ID3" {
                    let vmaj = header_data[3];
                    let _vrev = header_data[4];
                    let _flags = header_data[5];
                    let size_bytes = &header_data[6..10];

                    if [2, 3, 4].contains(&vmaj) {
                        let size =
                            BitPaddedInt::new(size_bytes.into(), Some(7), Some(true))?.value();
                        delete_bytes(&mut cursor, size as u64 + 10, 0, None)?;
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                // A short buffer cannot contain a full ID3v2 header.
            }
            Err(err) => return Err(err.into()),
        }
    }

    // Write modified data back to the writer
    let result = cursor.into_inner();
    writer.seek(SeekFrom::Start(0))?;
    writer.write_all(&result)?;

    // Zero out any stale trailing bytes from the original content.
    // When tags are removed the output is shorter than the input, but
    // Cursor/File writers do not auto-truncate on write_all — the old
    // bytes remain accessible beyond the new content boundary.
    let written_end = writer.stream_position()?;
    crate::util::truncate_writer_dyn(writer, written_end)?;

    Ok(())
}

/// Native async version of [`clear`]. Removes ID3 tags from a file using tokio I/O.
#[cfg(feature = "async")]
pub async fn clear_async(filething: &Path, clear_v1: bool, clear_v2: bool) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    let mut file = TokioOpenOptions::new()
        .read(true)
        .write(true)
        .open(filething)
        .await?;

    // Clear ID3v1 if requested — only read the tail to locate the tag
    if clear_v1 {
        // Use try_from to avoid silent truncation for files whose
        // length exceeds i64::MAX (mirrors the sync clear() path).
        let file_len = i64::try_from(file.seek(SeekFrom::End(0)).await?).map_err(|_| {
            crate::AudexError::InvalidData(
                "File length exceeds i64::MAX; cannot safely compute seek positions".to_string(),
            )
        })?;
        let tail_size = std::cmp::min(file_len, 256) as usize;

        if tail_size > 0 {
            file.seek(SeekFrom::End(-(tail_size as i64))).await?;
            let mut tail = vec![0u8; tail_size];
            file.read_exact(&mut tail).await?;

            if let Ok((frames, offset)) = find_id3v1(&tail, 4, None) {
                if frames.is_some() {
                    // Guard against underflow: reject if offset is more
                    // negative than file_len (same check as sync path).
                    if file_len + offset < 0 {
                        return Err(crate::AudexError::ParseError(
                            "ID3v1 tag offset exceeds file size".to_string(),
                        ));
                    }
                    let new_size = (file_len + offset) as u64;
                    file.set_len(new_size).await?;
                }
            }
        }
    }

    // Clear ID3v2 if requested
    if clear_v2 {
        file.seek(SeekFrom::Start(0)).await?;

        let mut header_data = [0u8; 10];
        match file.read_exact(&mut header_data).await {
            Ok(_) => {
                if &header_data[0..3] == b"ID3" {
                    let vmaj = header_data[3];
                    let size_bytes = &header_data[6..10];

                    if [2, 3, 4].contains(&vmaj) {
                        let size =
                            BitPaddedInt::new(size_bytes.into(), Some(7), Some(true))?.value();
                        delete_bytes_async(&mut file, size as u64 + 10, 0, None).await?;
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                // A short file cannot contain a full ID3v2 header.
            }
            Err(err) => return Err(err.into()),
        }
    }

    Ok(())
}

/// A no-op [`StreamInfo`] implementation for files that contain only ID3 tags
/// and no audio stream (e.g., standalone `.id3` files).
///
/// Every accessor returns `None`, indicating that no stream metadata is available.
#[derive(Debug, Default)]
pub struct EmptyStreamInfo;

impl StreamInfo for EmptyStreamInfo {
    fn length(&self) -> Option<Duration> {
        None
    }
    fn bitrate(&self) -> Option<u32> {
        None
    }
    fn sample_rate(&self) -> Option<u32> {
        None
    }
    fn channels(&self) -> Option<u16> {
        None
    }
    fn bits_per_sample(&self) -> Option<u16> {
        None
    }
}

/// Stream info for ID3FileType
#[derive(Debug)]
pub struct _Info {
    pub length: Duration,
}

impl _Info {
    pub fn new<R: Read + Seek>(_fileobj: &mut R, _offset: Option<u64>) -> Self {
        Self {
            length: Duration::from_secs(0),
        }
    }

    pub fn pprint() -> String {
        "Unknown format with ID3 tag".to_string()
    }
}

impl StreamInfo for _Info {
    fn length(&self) -> Option<Duration> {
        if self.length.as_secs() == 0 {
            None
        } else {
            Some(self.length)
        }
    }
    fn bitrate(&self) -> Option<u32> {
        None
    }
    fn sample_rate(&self) -> Option<u32> {
        None
    }
    fn channels(&self) -> Option<u16> {
        None
    }
    fn bits_per_sample(&self) -> Option<u16> {
        None
    }
}

/// ID3FileType implementation
///
/// An unknown type of file with ID3 tags.
///
/// Args:
///     filething: A filename or file handle
///     id3: An ID3 type to use for tags.
///
/// Load stream and tag information from a file.
///
/// A custom tag reader may be used in instead of the default
/// ID3 implementation, e.g. a custom ID3 reader.
#[derive(Debug)]
pub struct ID3FileType {
    /// ID3 tags
    pub tags: Option<ID3>,
    /// Stream info
    pub info: _Info,
    /// ID3 type to use
    pub id3: fn() -> ID3,
}

impl ID3FileType {
    /// Create new ID3FileType
    pub fn new() -> Self {
        Self {
            tags: None,
            info: _Info::new(&mut std::io::Cursor::new(Vec::new()), None),
            id3: ID3::new,
        }
    }

    /// Score file compatibility
    pub fn score(_filename: &str, header_data: &[u8]) -> i32 {
        if header_data.starts_with(b"ID3") {
            1
        } else {
            0
        }
    }

    /// Add an empty ID3 tag to the file
    pub fn add_tags(&mut self, id3_class: Option<fn() -> ID3>) -> Result<()> {
        let id3_fn = id3_class.unwrap_or(self.id3);

        if self.tags.is_none() {
            self.id3 = id3_fn;
            self.tags = Some(id3_fn());
        } else {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }

        Ok(())
    }

    /// Load file
    pub fn load<P: AsRef<Path>>(
        &mut self,
        filething: P,
        id3_class: Option<fn() -> ID3>,
    ) -> Result<()> {
        let id3_fn = id3_class.unwrap_or(self.id3);
        self.id3 = id3_fn;

        let mut id3_instance = id3_fn();
        match id3_instance.load(filething, None, true, 4, true) {
            Ok(()) => {
                self.tags = Some(id3_instance);
            }
            Err(_) => {
                self.tags = None; // ID3NoHeaderError
            }
        }

        Ok(())
    }
}

impl FileType for ID3FileType {
    type Tags = ID3;
    type Info = _Info;

    fn format_id() -> &'static str {
        "ID3FileType"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut instance = Self::new();
        instance.load(path, None)?;
        Ok(instance)
    }

    fn save(&mut self) -> Result<()> {
        if let Some(ref mut tags) = self.tags {
            tags.save()
        } else {
            Err(AudexError::InvalidData("No tags to save".to_string()))
        }
    }

    fn clear(&mut self) -> Result<()> {
        if let Some(ref mut tags) = self.tags {
            tags.clear()?;
            self.tags = None;
        }
        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        if let Some(ref mut tags) = self.tags {
            tags.save_to_writer(writer)
        } else {
            Err(AudexError::InvalidData("No tags to save".to_string()))
        }
    }

    fn clear_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        if let Some(ref mut tags) = self.tags {
            tags.clear_writer(writer)?;
            self.tags = None;
        }
        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        if let Some(ref mut tags) = self.tags {
            tags.save_to_path(path)
        } else {
            Err(AudexError::InvalidData("No tags to save".to_string()))
        }
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
        // Delegate to the existing public add_tags method with default ID3 version
        ID3FileType::add_tags(self, None)
    }

    fn get(&self, key: &str) -> Option<Vec<String>> {
        // ID3Tags has a special get_text_values method that handles the mapping
        self.tags.as_ref()?.tags.get_text_values(key)
    }

    fn score(_filename: &str, header: &[u8]) -> i32 {
        if header.starts_with(b"ID3") { 10 } else { 0 }
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/mpeg", "audio/mp3", "audio/x-aiff"]
    }
}

impl Default for ID3FileType {
    fn default() -> Self {
        Self::new()
    }
}

/// Implement FileType for ID3 directly to match existing patterns
impl FileType for ID3 {
    type Tags = ID3Tags;
    type Info = EmptyStreamInfo;

    fn format_id() -> &'static str {
        "ID3"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing standalone ID3 file");
        Self::load_from_file(path)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing standalone ID3 file from reader");
        let mut instance = Self::new();
        let mut reader = reader;

        // Clear existing state
        instance.unknown_frames.clear();
        instance._header = None;
        instance._padding = 0;

        // Pre-load header hook
        instance._pre_load_header(&mut reader)?;

        // Try to parse ID3v2 header
        reader.seek(SeekFrom::Start(0))?;
        let mut header_data = [0u8; 10];
        match reader.read_exact(&mut header_data) {
            Ok(()) if &header_data[0..3] == b"ID3" => {
                let header = ID3Header::from_bytes(&header_data)?;
                instance._header = Some(header);

                let size = instance.size();

                // Enforce the library-wide tag allocation ceiling
                crate::limits::ParseLimits::default()
                    .check_tag_size(size as u64, "ID3v2 reader")?;

                // Cross-validate against actual stream size
                let current_pos = reader.stream_position()?;
                let stream_end = reader.seek(SeekFrom::End(0))?;
                let available = stream_end.saturating_sub(current_pos);
                if (size as u64) > available {
                    return Err(AudexError::ParseError(format!(
                        "ID3 tag size ({} bytes) exceeds remaining stream data ({} bytes)",
                        size, available
                    )));
                }
                reader.seek(SeekFrom::Start(current_pos))?;

                // Read the full tag data
                let mut data = vec![0u8; size as usize];
                reader.read_exact(&mut data).map_err(|e| {
                    AudexError::InvalidData(format!("Cannot read {} bytes: {}", size, e))
                })?;

                // Skip the extended header if present
                let header_ref = instance._header.as_ref().ok_or_else(|| {
                    AudexError::InvalidData("ID3 header not set during reader load".to_string())
                })?;

                let frame_data = if instance.f_extended() && data.len() >= 4 {
                    // Check if the first 4 bytes look like a frame ID rather
                    // than an extended header size (common tagger bug).
                    let looks_like_frame = std::str::from_utf8(&data[0..4])
                        .map(|s| {
                            s.chars()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                        })
                        .unwrap_or(false);

                    if looks_like_frame {
                        // Extended header flag was likely set incorrectly;
                        // treat the data as starting with frame headers.
                        &data
                    } else {
                        let ext_size = if header_ref.major_version == 4 {
                            crate::id3::util::decode_synchsafe_int_checked(&data[0..4])? as usize
                        } else {
                            // Use checked_add to prevent overflow on 32-bit platforms
                            // when the declared size is near u32::MAX
                            let raw =
                                u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as u64;
                            match raw.checked_add(4) {
                                Some(size) => size as usize,
                                None => {
                                    return Err(AudexError::InvalidData(
                                        "Extended header size overflow".to_string(),
                                    ));
                                }
                            }
                        };

                        // Reject extended header sizes that exceed the available data.
                        // A crafted file could set a huge size to skip all valid frames.
                        if ext_size > data.len() {
                            return Err(AudexError::InvalidData(
                                "Extended header size exceeds available tag data".to_string(),
                            ));
                        }

                        &data[ext_size..]
                    }
                } else {
                    &data
                };

                let header_clone = header_ref.clone();
                let remaining_data = instance._read(&header_clone, frame_data)?;
                instance._padding = remaining_data.len();

                // Load ID3v1 if present — only read the tail
                let v1v2_ver = if instance.version().1 == 4 { 4 } else { 3 };

                if let Ok((Some(frames), _offset)) = find_id3v1_from_reader(reader, v1v2_ver, None)
                {
                    for (_, frame) in frames {
                        if instance.tags.getall(&frame.hash_key()).is_empty() {
                            let _ = instance.tags.add(frame);
                        }
                    }
                }
            }
            _ => {
                // No ID3v2 header found, try ID3v1 from the tail
                match find_id3v1_from_reader(reader, 4, None) {
                    Ok((frames, _offset)) => {
                        if let Some(frames) = frames {
                            instance._version = (1, 1, 0);
                            for (_, frame) in frames {
                                if instance.tags.getall(&frame.hash_key()).is_empty() {
                                    let _ = instance.tags.add(frame);
                                }
                            }
                        } else {
                            return Err(AudexError::InvalidData("No ID3 tags found".to_string()));
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Translate to v2.4
        instance.tags.update_to_v24();
        instance._version = (2, 4, 0);

        // Populate the text values cache so Tags::get returns real data
        instance.refresh_values_cache();

        Ok(instance)
    }

    fn save(&mut self) -> Result<()> {
        self.save()
    }

    fn clear(&mut self) -> Result<()> {
        self.delete_full(None, true, true)
    }

    fn save_to_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        self.save_to_writer(writer)
    }

    fn clear_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        self.clear_writer(writer)
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        let tag_version = self.tags.version().1;
        let v2_version = if tag_version == 3 || tag_version == 4 {
            tag_version
        } else {
            4
        };
        self.save_with_options(Some(path), ID3v1SaveOptions::UPDATE, v2_version, Some("/"))
    }

    fn tags(&self) -> Option<&Self::Tags> {
        self.tags()
    }

    fn tags_mut(&mut self) -> Option<&mut Self::Tags> {
        self.tags_mut()
    }

    fn info(&self) -> &Self::Info {
        &EmptyStreamInfo
    }

    fn add_tags(&mut self) -> Result<()> {
        // Tags always exist for ID3 - they are created when the struct is instantiated
        Err(AudexError::InvalidOperation(
            "Tags already exist".to_string(),
        ))
    }

    fn get(&self, key: &str) -> Option<Vec<String>> {
        // ID3Tags has a special get_text_values method that handles the mapping
        self.tags().and_then(|tags| tags.get_text_values(key))
    }

    fn score(_filename: &str, header: &[u8]) -> i32 {
        if header.starts_with(b"ID3") { 10 } else { 0 }
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/mpeg", "audio/mp3", "audio/aiff", "audio/x-aiff"]
    }
}
