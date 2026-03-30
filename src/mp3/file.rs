//! MP3 file type implementation and audio stream information.
//!
//! This module contains the main [`MP3`] and [`EasyMP3`] types for working with MP3 files,
//! as well as [`MPEGInfo`] for accessing audio stream properties.
//!
//! The implementation handles:
//! - MPEG frame parsing and synchronization
//! - ID3 tag reading and writing
//! - VBR header detection (Xing/Info/VBRI)
//! - Duration calculation for both CBR and VBR files
//! - Encoder information extraction (LAME, etc.)

use crate::id3::{ID3, ID3Tags};
use crate::mp3::util::{BitrateMode, MPEGFrame};
use crate::mp3::{ChannelMode, Emphasis, MPEGLayer, MPEGVersion};
use crate::{AudexError, FileType, Result, StreamInfo};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Represents an MP3 audio file with stream information and metadata tags.
///
/// This struct provides access to both the audio properties (bitrate, sample rate, duration, etc.)
/// and metadata tags (ID3v1, ID3v2) contained in an MP3 file.
///
/// # Structure
///
/// - **`info`**: Audio stream information including bitrate, sample rate, channels, and duration
/// - **`tags`**: Optional ID3 tags (ID3v1 and/or ID3v2)
/// - **`filename`**: Path to the file (used for save operations)
///
/// # Examples
///
/// ## Loading and Reading Tags
///
/// ```no_run
/// use audex::mp3::MP3;
/// use audex::FileType;
///
/// // Load MP3 file using FileType trait method
/// let mp3 = MP3::load("song.mp3").unwrap();
///
/// // Access audio stream information
/// println!("Duration: {:?}", mp3.info.length);
/// println!("Bitrate: {} bps", mp3.info.bitrate);
/// println!("Sample rate: {} Hz", mp3.info.sample_rate);
/// println!("Channels: {}", mp3.info.channels);
///
/// // Read ID3 tags if present
/// if let Some(ref tags) = mp3.tags {
///     if let Some(title) = tags.get_text_values("TIT2") {
///         println!("Title: {:?}", title);
///     }
/// }
/// ```
///
/// ## Modifying and Saving Tags
///
/// ```no_run
/// use audex::mp3::MP3;
/// use audex::{FileType, Tags};
///
/// // Load MP3 file using FileType trait method
/// let mut mp3 = MP3::load("song.mp3").unwrap();
///
/// // Modify ID3 tags using the Tags trait
/// if let Some(ref mut tags) = mp3.tags {
///     tags.set("TIT2", vec!["New Title".to_string()]);
///     tags.set("TPE1", vec!["New Artist".to_string()]);
/// }
///
/// // Save changes back to the original file
/// mp3.save().unwrap();
/// ```
///
/// ## Async Usage
///
/// ```ignore
/// // Note: This example requires the `async` feature to be enabled.
/// // Enable with: audex = { version = "...", features = ["async"] }
/// use audex::mp3::MP3;
/// use audex::Tags;
///
/// # async fn example() {
/// // Load MP3 file asynchronously
/// let mut mp3 = MP3::load_async("song.mp3").await.unwrap();
///
/// // Modify tags using the Tags trait
/// if let Some(ref mut tags) = mp3.tags {
///     tags.set("TIT2", vec!["Async Title".to_string()]);
/// }
///
/// // Save changes asynchronously
/// mp3.save_async().await.unwrap();
/// # }
/// ```
///
/// # See Also
///
/// - [`MPEGInfo`] - Audio stream information
/// - [`ID3Tags`] - ID3 tag access
/// - [`EasyMP3`] - Simplified interface for common tagging operations
#[derive(Debug)]
pub struct MP3 {
    /// Audio stream information (bitrate, sample rate, duration, etc.)
    pub info: MPEGInfo,

    /// ID3 tags (ID3v1 and/or ID3v2), if present in the file
    pub tags: Option<ID3Tags>,

    /// Path to the file (stored for save operations)
    pub filename: Option<String>,
}

impl MP3 {
    /// Extract ReplayGain value from an RVA2 frame.
    /// Returns the gain (plain number) or peak as a string.
    fn get_rva2_replaygain(&self, track_type: &str, is_gain: bool) -> Option<Vec<String>> {
        let tags = self.tags.as_ref()?;
        // Search for the RVA2 frame — try multiple key formats since
        // the identification may be stored in different cases
        let lower_type = track_type.to_lowercase();
        for (key, frame) in tags.dict.iter() {
            if !key.starts_with("RVA2") {
                continue;
            }
            if !key.to_lowercase().contains(&lower_type) {
                continue;
            }
            if let Some(rva2) = frame.as_any().downcast_ref::<crate::id3::frames::RVA2>() {
                if let Some((gain, peak)) = rva2.get_master() {
                    // Round to 2 decimal places for gain (matches text tag precision)
                    // and 7 for peak (f32 has ~7 significant digits)
                    return if is_gain {
                        let rounded = (gain * 100.0).round() / 100.0;
                        Some(vec![format!("{}", rounded)])
                    } else {
                        let rounded = (peak * 10000000.0).round() / 10000000.0;
                        Some(vec![format!("{}", rounded)])
                    };
                }
            }
        }
        None
    }

    /// Creates a new empty MP3 instance with default values.
    ///
    /// This creates an MP3 struct with default audio information and no tags.
    /// Typically you would use [`MP3::from_file`] or [`MP3::load`](FileType::load) instead.
    ///
    /// ```
    /// use audex::mp3::MP3;
    ///
    /// let mp3 = MP3::new();
    /// assert!(mp3.tags.is_none());
    /// assert_eq!(mp3.info.bitrate, 0);
    /// ```
    pub fn new() -> Self {
        Self {
            info: MPEGInfo::default(),
            tags: None,
            filename: None,
        }
    }

    /// Loads an MP3 file from the specified path.
    ///
    /// This method opens the file, parses the MPEG audio stream to extract audio properties,
    /// and loads any ID3 tags (ID3v1, ID3v2) present in the file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the MP3 file to load
    ///
    /// # Returns
    ///
    /// * `Ok(MP3)` - Successfully loaded MP3 file with audio info and tags
    /// * `Err(AudexError)` - Failed to open file or parse MPEG stream
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The file cannot be opened (doesn't exist, permission denied, etc.)
    /// - The file is not a valid MP3 file (no MPEG frame sync found)
    /// - The MPEG headers are corrupted or invalid
    ///
    /// Note: missing ID3 tags are not considered an error; the `tags` field will be `None`.
    /// The current implementation also treats ID3 parsing failures as `None` rather than
    /// surfacing them separately.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::mp3::MP3;
    /// use audex::Tags;
    ///
    /// let mp3 = MP3::from_file("song.mp3").unwrap();
    /// if let Some(length) = mp3.info.length {
    ///     println!("Duration: {:.2} seconds", length.as_secs_f64());
    /// }
    /// println!("Bitrate: {} kbps", mp3.info.bitrate / 1000);
    ///
    /// // Access tags via the Tags trait
    /// if let Some(tags) = &mp3.tags {
    ///     if let Some(title) = tags.get_text_values("TIT2") {
    ///         println!("Title: {:?}", title);
    ///     }
    /// }
    /// ```
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all, fields(path = %path.as_ref().display())))]
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        debug_event!("parsing MP3 file");
        let mut file = File::open(path)?;
        let mut mp3 = Self::new();

        // Store filename for save operations
        mp3.filename = Some(path.to_string_lossy().to_string());

        // Load ID3 tags - use the actual loaded tags
        match ID3::load_from_file(path) {
            Ok(id3) => {
                debug_event!("ID3v2 tags parsed for MP3");
                mp3.tags = Some(id3.tags);
            }
            Err(_) => {
                // If loading fails (e.g., "No ID3 tags found"), start with None
                // Tags can be created later when needed
                mp3.tags = None;
            }
        }

        // Parse MPEG stream info
        mp3.info = MPEGInfo::from_file(&mut file)?;
        debug_event!(
            bitrate = mp3.info.bitrate,
            sample_rate = mp3.info.sample_rate,
            channels = mp3.info.channels,
            "MPEG stream info parsed"
        );

        Ok(mp3)
    }

    /// Saves tag modifications back to the MP3 file with default options.
    ///
    /// This method writes any changes made to the tags back to the file. Only the tags
    /// are modified; the audio data remains unchanged. By default, this:
    /// - Saves the current ID3 tags if `self.tags` is present
    /// - Updates existing ID3v1 tags or creates them if tags are present
    /// - Preserves the original file's audio data
    ///
    /// The file path used is the one stored in the `filename` field, which is set
    /// automatically when loading via [`MP3::from_file`].
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Tags successfully saved to file
    /// * `Err(AudexError)` - Failed to save (file not writable, no filename stored, etc.)
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - No filename is stored and none was provided
    /// - The file cannot be written (permission denied, disk full, etc.)
    /// - The file was deleted or moved since loading
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::mp3::MP3;
    /// use audex::Tags;
    ///
    /// let mut mp3 = MP3::from_file("song.mp3").unwrap();
    ///
    /// // Modify tags
    /// if let Some(ref mut tags) = mp3.tags {
    ///     tags.set("TIT2", vec!["New Title".to_string()]);
    /// }
    ///
    /// // Save changes
    /// mp3.save().unwrap();
    /// ```
    ///
    /// # See Also
    ///
    /// - [`MP3::save_with_options`] - Save with specific ID3 version and options
    pub fn save(&mut self) -> Result<()> {
        debug_event!("saving MP3 tags");
        self.save_with_options(None, None, None, None)
    }

    /// Saves tag modifications with format-specific options.
    ///
    /// This method provides fine-grained control over how ID3 tags are saved, including:
    /// - Target ID3v2 version (2.3 or 2.4)
    /// - ID3v1 tag handling (remove, update, or create)
    /// - Custom separator for multi-value fields in ID3v2.3
    /// - Optional different file path
    ///
    /// # Arguments
    ///
    /// * `file_path` - Optional alternative path to save to (uses stored filename if `None`)
    /// * `v1` - ID3v1 option:
    ///   - `0` = Remove ID3v1 tags
    ///   - `1` = Update existing ID3v1 tags only
    ///   - `2` = Create ID3v1 tags if missing (default)
    /// * `v2_version` - Target ID3v2 version:
    ///   - `3` = ID3v2.3 (default, most compatible)
    ///   - `4` = ID3v2.4 (newer features, less compatible)
    /// * `v23_sep` - Separator for multi-value text frames in ID3v2.3 (default: "/")
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Tags successfully saved with specified options
    /// * `Err(AudexError)` - Failed to save
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::mp3::MP3;
    /// use audex::Tags;
    ///
    /// let mut mp3 = MP3::from_file("song.mp3").unwrap();
    ///
    /// // Modify tags
    /// if let Some(ref mut tags) = mp3.tags {
    ///     tags.set("TIT2", vec!["New Title".to_string()]);
    /// }
    ///
    /// // Save as ID3v2.4 without ID3v1 tags
    /// mp3.save_with_options(None, Some(0), Some(4), None).unwrap();
    /// ```
    ///
    /// ```no_run
    /// use audex::mp3::MP3;
    ///
    /// let mut mp3 = MP3::from_file("song.mp3").unwrap();
    ///
    /// // Save to a different file
    /// mp3.save_with_options(Some("output.mp3"), None, None, None).unwrap();
    /// ```
    pub fn save_with_options(
        &mut self,
        file_path: Option<&str>,
        v1: Option<u8>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        // Use provided file_path or fall back to stored filename
        let target_path = match file_path {
            Some(path) => path,
            None => self.filename.as_deref().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };

        // Set default values for format compatibility
        let v1_option = v1.unwrap_or(2); // Default to CREATE (2)
        let v2_version_option = v2_version.unwrap_or(3); // Default to v2.3
        let v23_sep_string = v23_sep.map(|s| s.to_string()); // Convert Option<&str> to Option<String>

        trace_event!(
            path = target_path,
            id3v1_option = v1_option,
            id3v2_version = v2_version_option,
            "writing MP3 ID3 tags to file"
        );

        if let Some(ref mut tags) = self.tags {
            // Use the new in-place ID3 modification - performs efficient byte manipulation
            // instead of rebuilding the entire file
            tags.save(
                target_path,
                v1_option,
                v2_version_option,
                v23_sep_string,
                None,
            )?;
        }

        Ok(())
    }
}

// Async methods for MP3 - feature-gated for async runtime support
#[cfg(feature = "async")]
impl MP3 {
    /// Load MP3 from file asynchronously
    ///
    /// Reads MPEG stream information and ID3 tags from the specified file
    /// using non-blocking I/O operations for improved concurrency.
    ///
    /// # Arguments
    /// * `path` - Path to the MP3 file
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded MP3 with metadata
    /// * `Err(AudexError)` - Error occurred during file reading or parsing
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file_async(path).await
    }

    /// Load MP3 from file asynchronously (alias for load_async)
    ///
    /// This is the primary async loading method that mirrors the synchronous
    /// `from_file` method behavior.
    pub async fn from_file_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let mut file = TokioFile::open(path).await?;
        let mut mp3 = Self::new();

        // Store filename for subsequent save operations
        mp3.filename = Some(path.to_string_lossy().to_string());

        // Load ID3 tags asynchronously using consolidated ID3 async methods
        match ID3::load_from_file_async(path).await {
            Ok(id3) => {
                mp3.tags = Some(id3.tags);
            }
            Err(_) => {
                // If loading fails (e.g., no ID3 tags present), start with None
                // Tags can be created and added later when needed
                mp3.tags = None;
            }
        }

        // Parse MPEG stream information from audio data
        mp3.info = Self::parse_mpeg_info_async(&mut file).await?;

        Ok(mp3)
    }

    /// Parse MPEG stream information from async file reader
    ///
    /// Analyzes the MPEG audio stream to extract duration, bitrate, sample rate,
    /// and other audio metadata using async I/O operations.
    async fn parse_mpeg_info_async(file: &mut TokioFile) -> Result<MPEGInfo> {
        // Skip ID3v2 tag if present at the start of file
        Self::skip_id3v2_async(file).await?;

        // Find and parse first valid MPEG frame using frame synchronization
        let (frame, overall_sketchy) = Self::find_and_parse_frame_async(file).await?;

        // Build MPEGInfo from parsed frame data
        let mut info = MPEGInfo {
            length: frame.length,
            bitrate: frame.bitrate,
            sample_rate: frame.sample_rate,
            channels: frame.channels(),
            version: frame.version,
            layer: frame.layer,
            channel_mode: frame.channel_mode,
            emphasis: frame.emphasis,
            protected: frame.protected,
            padding: frame.padding,
            private: frame.private,
            copyright: frame.copyright,
            original: frame.original,
            mode_extension: frame.mode_extension,
            sketchy: overall_sketchy,
            bitrate_mode: frame.bitrate_mode,
            encoder_info: frame.encoder_info,
            encoder_settings: frame.encoder_settings,
            track_gain: frame.track_gain,
            track_peak: frame.track_peak,
            album_gain: frame.album_gain,
            album_peak: None,
        };

        // Estimate duration from file size if VBR header not available
        if info.length.is_none() {
            Self::estimate_length_async(&mut info, file, frame.frame_offset).await?;
        }

        Ok(info)
    }

    /// Skip ID3v2 tag if present at file start asynchronously
    ///
    /// Windows Media Player and other software may write multiple ID3 tags,
    /// so this method skips all consecutive ID3v2 tags found.
    async fn skip_id3v2_async(reader: &mut TokioFile) -> Result<()> {
        reader.seek(SeekFrom::Start(0)).await?;
        let file_size = reader.seek(SeekFrom::End(0)).await?;
        reader.seek(SeekFrom::Start(0)).await?;

        // Skip multiple consecutive ID3v2 tags (some software writes multiple).
        // Cap iterations to prevent unbounded looping on pathological files.
        const MAX_ID3_SKIP_ITERATIONS: usize = 1000;
        let mut id3_iterations = 0usize;
        loop {
            id3_iterations += 1;
            if id3_iterations > MAX_ID3_SKIP_ITERATIONS {
                break;
            }

            let mut id3_header = [0u8; 10];
            let mut bytes_read = 0usize;
            while bytes_read < id3_header.len() {
                let read_now = reader.read(&mut id3_header[bytes_read..]).await?;
                if read_now == 0 {
                    break;
                }
                bytes_read += read_now;
            }

            if bytes_read < 10 {
                reader.seek(SeekFrom::Start(0)).await?;
                break;
            }

            if &id3_header[0..3] == b"ID3" {
                let tag_size =
                    crate::id3::util::decode_synchsafe_int_checked(&id3_header[6..10])? as u64;
                let current_pos = reader.stream_position().await?;
                if tag_size > 0 && current_pos + tag_size <= file_size {
                    let skip = i64::try_from(tag_size).map_err(|_| {
                        AudexError::InvalidData("ID3 tag size exceeds i64 range".to_string())
                    })?;
                    reader.seek(SeekFrom::Current(skip)).await?;
                    continue;
                }
            }

            // No more ID3 tags found, seek back to audio data start
            reader.seek(SeekFrom::Current(-(bytes_read as i64))).await?;
            break;
        }

        Ok(())
    }

    /// Find and parse first valid MPEG frame asynchronously
    ///
    /// Uses frame synchronization to locate valid MPEG audio frames and
    /// returns frame data along with confidence indicator.
    async fn find_and_parse_frame_async(reader: &mut TokioFile) -> Result<(MPEGFrame, bool)> {
        const MAX_READ: u64 = 1024 * 1024; // 1MB maximum search range
        const MAX_SYNCS: usize = 1500; // Maximum sync word attempts
        const ENOUGH_FRAMES: usize = 4; // Frames needed for high confidence
        const MIN_FRAMES: usize = 2; // Minimum acceptable frame count

        let mut max_syncs = MAX_SYNCS;
        let mut first_frame: Option<MPEGFrame> = None;
        let mut overall_sketchy = true;

        // Get all potential sync word positions
        let sync_positions = Self::iter_sync_async(reader, MAX_READ).await?;

        for sync_offset in sync_positions {
            if max_syncs == 0 {
                break;
            }
            max_syncs -= 1;

            reader.seek(SeekFrom::Start(sync_offset)).await?;
            let mut frames = Vec::new();

            // Attempt to parse consecutive frames from this sync position
            for _ in 0..ENOUGH_FRAMES {
                match Self::parse_frame_async(reader).await {
                    Ok(frame) => {
                        frames.push(frame);
                        // Non-sketchy frame (has valid VBR header) is definitive
                        if !frames
                            .last()
                            .expect("frames is non-empty after push")
                            .sketchy
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            // Save first valid frame sequence as fallback
            if frames.len() >= MIN_FRAMES && first_frame.is_none() {
                first_frame = Some(frames[0].clone());
            }

            // Prefer non-sketchy frame with VBR header information
            if let Some(last_frame) = frames.last() {
                if !last_frame.sketchy {
                    overall_sketchy = false;
                    return Ok((last_frame.clone(), overall_sketchy));
                }
            }

            // Sufficient consecutive frames indicate valid sync
            if frames.len() >= ENOUGH_FRAMES {
                overall_sketchy = false;
                return Ok((frames[0].clone(), overall_sketchy));
            }
        }

        // Return best available frame or error if none found
        if let Some(frame) = first_frame {
            Ok((frame, overall_sketchy))
        } else {
            Err(AudexError::InvalidData(
                "can't sync to MPEG frame".to_string(),
            ))
        }
    }

    /// Iterate over potential sync word positions asynchronously
    ///
    /// Scans file for MPEG sync word patterns (0xFF followed by 0xE0+)
    /// and returns byte offsets of all potential frame starts.
    async fn iter_sync_async(reader: &mut TokioFile, max_read: u64) -> Result<Vec<u64>> {
        let mut positions = Vec::new();
        let start_pos = reader.stream_position().await?;

        // Clamp the read buffer to the remaining file size to avoid
        // allocating more memory than the file contains
        let file_size = reader.seek(SeekFrom::End(0)).await?;
        reader.seek(SeekFrom::Start(start_pos)).await?;
        let remaining = file_size.saturating_sub(start_pos);
        let read_size = max_read.min(remaining) as usize;

        let mut buffer = vec![0u8; read_size];
        let bytes_read = reader.read(&mut buffer).await?;
        buffer.truncate(bytes_read);

        // Locate all potential MPEG sync word positions.
        // Cap the number of positions to prevent excessive memory usage.
        const MAX_SYNC_POSITIONS: usize = 100_000;
        for i in 0..buffer.len().saturating_sub(1) {
            // MPEG sync word: 0xFF followed by byte with top 3 bits set
            if buffer[i] == 0xFF && (buffer[i + 1] & 0xE0) == 0xE0 {
                positions.push(start_pos + i as u64);
                if positions.len() >= MAX_SYNC_POSITIONS {
                    break;
                }
            }
        }

        Ok(positions)
    }

    /// Parse single MPEG frame from current position asynchronously
    ///
    /// Reads the full frame data via async I/O, then uses a Cursor to delegate
    /// to the sync `from_reader` which correctly parses VBR headers (Xing/VBRI).
    async fn parse_frame_async(reader: &mut TokioFile) -> Result<MPEGFrame> {
        let offset = reader.stream_position().await?;

        // Read enough data for the frame header + VBR header parsing.
        // Xing header can be at offset 36 + up to 512 bytes = 548 bytes minimum.
        // Use 1024 bytes for safety to cover all VBR header variants.
        let mut buf = vec![0u8; 1024];
        let bytes_read = reader.read(&mut buf).await?;
        buf.truncate(bytes_read);

        if bytes_read < 4 {
            return Err(AudexError::InvalidData(
                "Not enough data for MPEG frame".to_string(),
            ));
        }

        // Parse via Cursor at position 0 — from_reader will set frame_offset=0
        let mut cursor = std::io::Cursor::new(&buf[..]);
        let mut frame = MPEGFrame::from_reader(&mut cursor)?;

        // Restore the actual file offset so downstream code has the real position
        frame.frame_offset = offset;

        // Advance the async reader past this frame
        reader
            .seek(SeekFrom::Start(offset + frame.frame_size as u64))
            .await?;

        Ok(frame)
    }

    /// Estimate audio duration from file size when VBR header unavailable
    async fn estimate_length_async(
        info: &mut MPEGInfo,
        reader: &mut TokioFile,
        audio_start: u64,
    ) -> Result<()> {
        let file_size = reader.seek(SeekFrom::End(0)).await?;
        let content_size = file_size.saturating_sub(audio_start);

        if info.bitrate > 0 && content_size > 0 {
            // Calculate duration: (file_size * 8 bits) / bitrate
            let seconds = content_size as f64 * 8.0 / info.bitrate as f64;
            info.length = Some(Duration::from_secs_f64(seconds));
        }

        Ok(())
    }

    /// Save MP3 tags asynchronously with default options
    ///
    /// Writes ID3 tag modifications back to the file using efficient
    /// in-place byte manipulation when possible.
    pub async fn save_async(&mut self) -> Result<()> {
        self.save_with_options_async(None, None, None, None).await
    }

    /// Save MP3 tags asynchronously with format-specific options
    ///
    /// # Arguments
    /// * `file_path` - Optional target path (uses stored filename if None)
    /// * `v1` - ID3v1 save option (0=REMOVE, 1=UPDATE, 2=CREATE)
    /// * `v2_version` - Target ID3v2 version (3 or 4)
    /// * `v23_sep` - Separator for multiple values in v2.3 format
    pub async fn save_with_options_async(
        &mut self,
        file_path: Option<&str>,
        v1: Option<u8>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        // Determine target file path
        let target_path = match file_path {
            Some(path) => path.to_string(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };

        // Apply default format options for compatibility
        let v1_option = v1.unwrap_or(2);
        let v2_version_option = v2_version.unwrap_or(3);
        let v23_sep_string = v23_sep.map(|s| s.to_string());

        if let Some(ref tags) = self.tags {
            let config = crate::id3::tags::ID3SaveConfig {
                v2_version: v2_version_option,
                v2_minor: 0,
                v23_sep: v23_sep_string.clone().unwrap_or_else(|| "/".to_string()),
                v23_separator: v23_sep_string
                    .as_deref()
                    .and_then(|s| s.as_bytes().first().copied())
                    .unwrap_or(b'/'),
                padding: None,
                merge_frames: true,
                preserve_unknown: true,
                compress_frames: false,
                write_v1: match v1_option {
                    0 => crate::id3::file::ID3v1SaveOptions::REMOVE,
                    1 => crate::id3::file::ID3v1SaveOptions::UPDATE,
                    _ => crate::id3::file::ID3v1SaveOptions::CREATE,
                },
                unsync: false,
                extended_header: false,
                convert_v24_frames: true,
            };

            tags.save_to_file_async(&target_path, &config).await?;
        }

        Ok(())
    }

    /// Clear all ID3 tags asynchronously
    ///
    /// Removes tag data from memory and saves the cleared state to disk.
    pub async fn clear_async(&mut self) -> Result<()> {
        if let Some(ref mut tags) = self.tags {
            tags.dict.clear();
            tags.frames_by_id.clear();
        }
        self.save_async().await
    }

    /// Delete all ID3 tags from file asynchronously
    ///
    /// Removes both ID3v1 and ID3v2 tags from the file on disk.
    /// This is a static method that operates directly on the file.
    pub async fn delete_async<P: AsRef<Path>>(path: P) -> Result<()> {
        crate::id3::file::clear_async(path.as_ref(), true, true).await
    }
}

impl Default for MP3 {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for MP3 {
    type Tags = ID3Tags;
    type Info = MPEGInfo;

    fn format_id() -> &'static str {
        "MP3"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file(path)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing MP3 file from reader");
        let mut mp3 = Self::new();

        // Load ID3 tags from the reader
        match <ID3 as FileType>::load_from_reader(reader) {
            Ok(id3) => {
                debug_event!("ID3v2 tags parsed for MP3");
                mp3.tags = Some(id3.tags);
            }
            Err(_) => {
                mp3.tags = None;
            }
        }

        // Seek back and parse MPEG stream info
        reader.seek(std::io::SeekFrom::Start(0))?;
        let mut reader = reader;
        mp3.info = MPEGInfo::from_file(&mut reader)?;
        debug_event!(
            bitrate = mp3.info.bitrate,
            sample_rate = mp3.info.sample_rate,
            channels = mp3.info.channels,
            "MPEG stream info parsed"
        );

        Ok(mp3)
    }

    fn save(&mut self) -> Result<()> {
        // Delegate to the MP3 save method with default options
        MP3::save(self)
    }

    fn clear(&mut self) -> Result<()> {
        // Clear all ID3 tags
        if let Some(ref mut tags) = self.tags {
            tags.dict.clear();
            tags.frames_by_id.clear();
        }
        self.save()
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        if let Some(ref tags) = self.tags {
            // Construct an ID3 instance from the MP3's tags and delegate saving
            let mut id3_file = ID3::new();
            id3_file.tags = tags.clone();
            id3_file.save_to_writer(writer)
        } else {
            Err(AudexError::InvalidData("No tags to save".to_string()))
        }
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        crate::id3::file::clear_from_writer(writer, true, true)?;
        self.tags = None;
        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        self.save_with_options(path.to_str(), None, None, None)
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
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "Tags already exist".to_string(),
            ));
        }
        let mut tags = ID3Tags::new();
        // Set the filename so tags can be saved
        if let Some(ref filename) = self.filename {
            tags.filename = Some(std::path::PathBuf::from(filename));
        }
        self.tags = Some(tags);
        Ok(())
    }

    fn get(&self, key: &str) -> Option<Vec<String>> {
        // Check for ReplayGain keys — EasyID3 stores these in RVA2 frames,
        // so the raw ID3Tags won't find them as text. Use EasyID3's reader.
        if key.eq_ignore_ascii_case("REPLAYGAIN_TRACK_GAIN") || key == "TXXX:REPLAYGAIN_TRACK_GAIN"
        {
            return self.get_rva2_replaygain("track", true);
        }
        if key.eq_ignore_ascii_case("REPLAYGAIN_TRACK_PEAK") || key == "TXXX:REPLAYGAIN_TRACK_PEAK"
        {
            return self.get_rva2_replaygain("track", false);
        }
        if key.eq_ignore_ascii_case("REPLAYGAIN_ALBUM_GAIN") || key == "TXXX:REPLAYGAIN_ALBUM_GAIN"
        {
            return self.get_rva2_replaygain("album", true);
        }
        if key.eq_ignore_ascii_case("REPLAYGAIN_ALBUM_PEAK") || key == "TXXX:REPLAYGAIN_ALBUM_PEAK"
        {
            return self.get_rva2_replaygain("album", false);
        }
        // ID3Tags has a special get_text_values method that handles the mapping
        self.tags.as_ref()?.get_text_values(key)
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .tags
            .as_ref()
            .map(|t| t.dict.keys().cloned().collect())
            .unwrap_or_default();
        // Expose RVA2-based ReplayGain as TXXX keys so they participate in
        // normalized diffs alongside text-based ReplayGain in other formats.
        // Check for any RVA2 key in dict (e.g. "RVA2:track", "RVA2:album")
        let has_track_rva2 = keys
            .iter()
            .any(|k| k.starts_with("RVA2") && k.to_lowercase().contains("track"));
        let has_album_rva2 = keys
            .iter()
            .any(|k| k.starts_with("RVA2") && k.to_lowercase().contains("album"));
        if has_track_rva2 {
            if !keys.iter().any(|k| k == "TXXX:REPLAYGAIN_TRACK_GAIN") {
                keys.push("TXXX:REPLAYGAIN_TRACK_GAIN".to_string());
            }
            if !keys.iter().any(|k| k == "TXXX:REPLAYGAIN_TRACK_PEAK") {
                keys.push("TXXX:REPLAYGAIN_TRACK_PEAK".to_string());
            }
        }
        if has_album_rva2 {
            if !keys.iter().any(|k| k == "TXXX:REPLAYGAIN_ALBUM_GAIN") {
                keys.push("TXXX:REPLAYGAIN_ALBUM_GAIN".to_string());
            }
            if !keys.iter().any(|k| k == "TXXX:REPLAYGAIN_ALBUM_PEAK") {
                keys.push("TXXX:REPLAYGAIN_ALBUM_PEAK".to_string());
            }
        }
        keys
    }

    fn score(filename: &str, header: &[u8]) -> i32 {
        let mut score = 0;
        let filename_lower = filename.to_lowercase();

        // Check for specific MPEG sync patterns - matches specification exactly
        if header.len() >= 2 {
            let _sync_word = (header[0] as u16) << 8 | header[1] as u16;

            // Check specific sync patterns that specification looks for
            if header.starts_with(&[0xFF, 0xF2]) ||  // MPEG-2 Layer III
               header.starts_with(&[0xFF, 0xF3]) ||  // MPEG-2 Layer III
               header.starts_with(&[0xFF, 0xFA]) ||  // MPEG-1 Layer III
               header.starts_with(&[0xFF, 0xFB])
            {
                // MPEG-1 Layer III
                score += 2;
            }
        }

        // Check for ID3v2 header
        if header.len() >= 3 && header.starts_with(b"ID3") {
            score += 2;
        }

        // Check file extensions - exact scoring from specification
        if filename_lower.ends_with(".mp3")
            || filename_lower.ends_with(".mp2")
            || filename_lower.ends_with(".mpg")
            || filename_lower.ends_with(".mpeg")
        {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/mpeg", "audio/mp3", "audio/mpg", "audio/mpeg3"]
    }
}

/// Detailed information about an MPEG audio stream.
///
/// This struct contains comprehensive technical information extracted from the MPEG audio
/// frames, including audio properties, encoder information, and ReplayGain data.
///
/// # Field Categories
///
/// ## Basic Audio Properties
/// - **`length`**: Duration of the audio stream
/// - **`bitrate`**: Bitrate in bits per second (bps, NOT kbps)
/// - **`sample_rate`**: Sample rate in Hz (e.g., 44100, 48000)
/// - **`channels`**: Number of audio channels (1=mono, 2=stereo)
///
/// ## MPEG Technical Details
/// - **`version`**: MPEG version (MPEG-1, MPEG-2, or MPEG-2.5)
/// - **`layer`**: MPEG layer (I, II, or III/MP3)
/// - **`channel_mode`**: Stereo mode (stereo, joint stereo, dual channel, mono)
/// - **`emphasis`**: Pre-emphasis filter applied
///
/// ## Frame Flags
/// - **`protected`**: CRC error protection enabled
/// - **`padding`**: Frame padding bit set
/// - **`private`**: Private bit set (application-specific)
/// - **`copyright`**: Copyright bit set
/// - **`original`**: Original media bit set
/// - **`mode_extension`**: Joint stereo mode extension data
///
/// ## Encoding Information
/// - **`sketchy`**: `true` if duration/bitrate is estimated (no VBR header found)
/// - **`bitrate_mode`**: CBR, VBR, ABR, or Unknown
/// - **`encoder_info`**: Encoder name/version (e.g., "LAME3.99r")
/// - **`encoder_settings`**: Encoder settings string
///
/// ## ReplayGain
/// - **`track_gain`**: Track-level ReplayGain adjustment in dB
/// - **`track_peak`**: Peak sample value for the track (0.0-1.0)
/// - **`album_gain`**: Album-level ReplayGain adjustment in dB
/// - **`album_peak`**: Peak sample value for the album (0.0-1.0)
///
/// # Examples
///
/// ```no_run
/// use audex::mp3::{MP3, MPEGInfo};
///
/// let mp3 = MP3::from_file("song.mp3").unwrap();
/// let info: &MPEGInfo = &mp3.info;
///
/// // Basic audio properties
/// println!("Duration: {:?}", info.length);
/// println!("Bitrate: {} kbps", info.bitrate / 1000);
/// println!("Sample rate: {} Hz", info.sample_rate);
/// println!("Channels: {}", info.channels);
///
/// // Encoding information
/// println!("Bitrate mode: {:?}", info.bitrate_mode);
/// if let Some(encoder) = &info.encoder_info {
///     println!("Encoder: {}", encoder);
/// }
///
/// // Quality indicator
/// if info.sketchy {
///     println!("Warning: Duration/bitrate is estimated (no VBR header)");
/// }
/// ```
///
/// # Notes
///
/// - **Bitrate units**: The `bitrate` field is in **bits per second** (bps), not kilobits per second.
///   Divide by 1000 to get kbps (e.g., 320000 bps = 320 kbps).
/// - **Sketchy flag**: When `true`, the duration and bitrate are estimated from file size rather
///   than read from a VBR header. This is less accurate but still usable.
/// - **VBR files**: For Variable Bitrate files, the `bitrate` represents the average bitrate.
///
/// # See Also
///
/// - [`MP3`] - The main MP3 file type that contains this info
/// - [`BitrateMode`] - Bitrate encoding mode
/// - [`MPEGVersion`] - MPEG version enum
/// - [`MPEGLayer`] - MPEG layer enum
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MPEGInfo {
    /// Duration of the audio stream, if determinable
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::duration_as_secs_f64")
    )]
    pub length: Option<Duration>,

    /// Bitrate in bits per second (bps, NOT kbps)
    ///
    /// For CBR files, this is the constant bitrate.
    /// For VBR files, this is the average bitrate.
    /// Divide by 1000 to convert to kbps (e.g., 320000 bps = 320 kbps)
    pub bitrate: u32,

    /// Sample rate in Hz (e.g., 44100, 48000, 22050)
    pub sample_rate: u32,

    /// Number of audio channels (1 = mono, 2 = stereo)
    pub channels: u16,

    /// MPEG version (MPEG-1, MPEG-2, or MPEG-2.5)
    pub version: MPEGVersion,

    /// MPEG layer (I, II, or III/MP3)
    pub layer: MPEGLayer,

    /// Channel mode (stereo, joint stereo, dual channel, or mono)
    pub channel_mode: ChannelMode,

    /// Pre-emphasis filter applied to the audio
    pub emphasis: Emphasis,

    /// CRC error protection is enabled
    pub protected: bool,

    /// Frame padding bit is set
    pub padding: bool,

    /// Private bit (application-specific use)
    pub private: bool,

    /// Copyright bit indicating copyrighted material
    pub copyright: bool,

    /// Original media bit (vs. copy)
    pub original: bool,

    /// Mode extension for joint stereo encoding
    pub mode_extension: u8,

    /// `true` if duration/bitrate is estimated (no VBR header found)
    ///
    /// When `true`, the `length` and `bitrate` values are calculated from file size
    /// rather than read from a VBR header (Xing/Info/VBRI). This makes them less
    /// accurate but still usable for most purposes.
    pub sketchy: bool,

    /// Bitrate encoding mode (CBR, VBR, ABR, or Unknown)
    pub bitrate_mode: BitrateMode,

    /// Encoder name and version (e.g., "LAME3.99r", "FhG"), if available
    pub encoder_info: Option<String>,

    /// Encoder settings string, if available
    pub encoder_settings: Option<String>,

    /// Track-level ReplayGain adjustment in dB
    pub track_gain: Option<f32>,

    /// Peak sample value for the track (0.0 to 1.0+)
    pub track_peak: Option<f32>,

    /// Album-level ReplayGain adjustment in dB
    pub album_gain: Option<f32>,

    /// Peak sample value for the album (0.0 to 1.0+)
    pub album_peak: Option<f32>,
}

impl MPEGInfo {
    /// Create new MPEG info
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse MPEG info from file
    pub fn from_file<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        // Skip ID3v2 tag if present
        Self::skip_id3v2(reader)?;

        // Find and parse first valid MPEG frame using frame synchronization
        let (frame, overall_sketchy) = Self::find_and_parse_frame(reader)?;

        // Create MPEGInfo from frame data
        let mut info = MPEGInfo {
            length: frame.length,
            bitrate: frame.bitrate,
            sample_rate: frame.sample_rate,
            channels: frame.channels(),
            version: frame.version,
            layer: frame.layer,
            channel_mode: frame.channel_mode,
            emphasis: frame.emphasis,
            protected: frame.protected,
            padding: frame.padding,
            private: frame.private,
            copyright: frame.copyright,
            original: frame.original,
            mode_extension: frame.mode_extension,
            sketchy: overall_sketchy, // Use overall sketchy status, not frame's
            bitrate_mode: frame.bitrate_mode,
            encoder_info: frame.encoder_info,
            encoder_settings: frame.encoder_settings,
            track_gain: frame.track_gain,
            track_peak: frame.track_peak,
            album_gain: frame.album_gain,
            album_peak: None, // Not available in frame data
        };

        // Estimate length if not found in VBR header
        if info.length.is_none() {
            info.estimate_length_from_file_size(reader, frame.frame_offset)?;
        }

        Ok(info)
    }

    /// Skip ID3v2 tag if present - matches specification implementation
    fn skip_id3v2<R: Read + Seek>(reader: &mut R) -> Result<()> {
        reader.seek(SeekFrom::Start(0))?;
        let file_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;

        // Windows Media Player writes multiple ID3 tags, so skip as many as we find.
        // Cap iterations to prevent unbounded looping on pathological files.
        const MAX_ID3_SKIP_ITERATIONS: usize = 1000;
        let mut id3_iterations = 0usize;
        loop {
            id3_iterations += 1;
            if id3_iterations > MAX_ID3_SKIP_ITERATIONS {
                break;
            }

            let mut id3_header = [0u8; 10];
            let mut bytes_read = 0usize;
            while bytes_read < id3_header.len() {
                let read_now = reader.read(&mut id3_header[bytes_read..])?;
                if read_now == 0 {
                    break;
                }
                bytes_read += read_now;
            }

            if bytes_read < 10 {
                reader.seek(SeekFrom::Start(0))?;
                break;
            }

            if &id3_header[0..3] == b"ID3" {
                let tag_size =
                    crate::id3::util::decode_synchsafe_int_checked(&id3_header[6..10])? as u64;
                // Validate that the tag doesn't extend past the actual file
                let current_pos = reader.stream_position()?;
                if tag_size > 0 && current_pos + tag_size <= file_size {
                    let skip = i64::try_from(tag_size).map_err(|_| {
                        AudexError::InvalidData("ID3 tag size exceeds i64 range".to_string())
                    })?;
                    reader.seek(SeekFrom::Current(skip))?;
                    continue;
                }
            }

            // No more ID3 tags, seek back to start of non-ID3 data
            reader.seek(SeekFrom::Current(-(bytes_read as i64)))?;
            break;
        }

        Ok(())
    }

    /// Find and parse first valid MPEG frame using synchronization
    fn find_and_parse_frame<R: Read + Seek>(reader: &mut R) -> Result<(MPEGFrame, bool)> {
        const MAX_READ: u64 = 1024 * 1024; // 1MB maximum search
        const MAX_SYNCS: usize = 1500; // Maximum sync attempts
        const ENOUGH_FRAMES: usize = 4; // Frames needed for confidence
        const MIN_FRAMES: usize = 2; // Minimum acceptable frames

        let mut max_syncs = MAX_SYNCS;
        let mut first_frame: Option<MPEGFrame> = None;
        let mut overall_sketchy = true; // Overall sketchy status - separate from frame's sketchy

        for sync_offset in crate::mp3::util::iter_sync(reader, MAX_READ)? {
            if max_syncs == 0 {
                break;
            }
            max_syncs -= 1;

            reader.seek(SeekFrom::Start(sync_offset))?;
            let mut frames = Vec::new();

            // Try to parse multiple consecutive frames
            for _ in 0..ENOUGH_FRAMES {
                match MPEGFrame::from_reader(reader) {
                    Ok(frame) => {
                        frames.push(frame);
                        // If frame is non-sketchy (has valid VBR header), use it immediately
                        if !frames
                            .last()
                            .expect("frames is non-empty after push")
                            .sketchy
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }

            // Save first valid frame sequence in case this is all we get
            if frames.len() >= MIN_FRAMES && first_frame.is_none() {
                first_frame = Some(frames[0].clone());
            }

            // If the last frame was non-sketchy (has valid VBR header), use that
            if let Some(last_frame) = frames.last() {
                if !last_frame.sketchy {
                    overall_sketchy = false; // Found non-sketchy frame
                    return Ok((last_frame.clone(), overall_sketchy));
                }
            }

            // If we have enough valid frames, use the first one
            if frames.len() >= ENOUGH_FRAMES {
                overall_sketchy = false; // Found enough frames to be confident
                return Ok((frames[0].clone(), overall_sketchy));
            }
        }

        // Return first_frame if found - overall_sketchy remains true
        if let Some(frame) = first_frame {
            Ok((frame, overall_sketchy))
        } else {
            Err(AudexError::InvalidData(
                "can't sync to MPEG frame".to_string(),
            ))
        }
    }

    /// Estimate length from file size when VBR header is not available
    fn estimate_length_from_file_size<R: Read + Seek>(
        &mut self,
        reader: &mut R,
        audio_start: u64,
    ) -> Result<()> {
        let file_size = reader.seek(SeekFrom::End(0))?;
        let content_size = file_size.saturating_sub(audio_start);

        if self.bitrate > 0 && content_size > 0 {
            // Bitrate is already in bps
            let seconds = content_size as f64 * 8.0 / self.bitrate as f64;
            self.length = Some(Duration::from_secs_f64(seconds));
            // Don't modify sketchy flag here - it's already set correctly from frame parsing
        }

        Ok(())
    }
}

impl Default for MPEGInfo {
    fn default() -> Self {
        Self {
            length: None,
            bitrate: 0,
            sample_rate: 0,
            channels: 0,
            version: MPEGVersion::MPEG1,
            layer: MPEGLayer::Layer3,
            channel_mode: ChannelMode::Stereo,
            emphasis: Emphasis::None,
            protected: false,
            padding: false,
            private: false,
            copyright: false,
            original: false,
            mode_extension: 0,
            sketchy: false,
            bitrate_mode: BitrateMode::Unknown,
            encoder_info: None,
            encoder_settings: None,
            track_gain: None,
            track_peak: None,
            album_gain: None,
            album_peak: None,
        }
    }
}

impl StreamInfo for MPEGInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        Some(self.bitrate) // Already in bps
    }

    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate)
    }

    fn channels(&self) -> Option<u16> {
        Some(self.channels)
    }

    fn bits_per_sample(&self) -> Option<u16> {
        None // MP3 is lossy, no meaningful bits per sample
    }
}

/// MP3 file with a simplified tag interface.
///
/// This is a convenience wrapper around [`MP3`] that uses the simplified
/// [`EasyID3`](crate::easyid3::EasyID3) tag interface instead of the full ID3 API.
/// EasyID3 provides a key-value interface for common tags, making it easier
/// to work with standard metadata fields.
///
/// # When to Use EasyMP3
///
/// Use `EasyMP3` when you:
/// - Only need to work with common tag fields (title, artist, album, etc.)
/// - Want a simpler, more intuitive API
/// - Don't need access to advanced ID3 frames
///
/// Use [`MP3`] when you:
/// - Need access to all ID3 frame types
/// - Want to work with embedded artwork (APIC frames)
/// - Need fine control over ID3 versions and frame formats
///
/// # Examples
///
/// ```no_run
/// use audex::mp3::EasyMP3;
///
/// let mut mp3 = EasyMP3::from_file("song.mp3").unwrap();
///
/// // Simple tag access using the EasyID3 interface
/// if let Some(ref mut tags) = mp3.tags {
///     tags.set("title", &["My Song".to_string()])?;
///     tags.set("artist", &["Artist Name".to_string()])?;
///     tags.set("album", &["Album Title".to_string()])?;
/// }
///
/// // Save changes
/// mp3.save().unwrap();
/// # Ok::<(), audex::AudexError>(())
/// ```
///
/// # See Also
///
/// - [`MP3`] - Full-featured MP3 file type
/// - [`EasyID3`](crate::easyid3::EasyID3) - Simplified ID3 tag interface
/// - [`MPEGInfo`] - Audio stream information
#[derive(Debug)]
pub struct EasyMP3 {
    /// Audio stream information (same as in MP3)
    pub info: MPEGInfo,

    /// Simplified EasyID3 tag interface
    pub tags: Option<crate::easyid3::EasyID3>,

    /// Path to the file (stored for save operations)
    pub filename: Option<String>,
}

impl EasyMP3 {
    fn parse_language_code(lang: &str) -> Result<[u8; 3]> {
        let bytes = lang.as_bytes();
        if bytes.len() != 3 || !bytes.iter().all(|b| b.is_ascii_alphabetic()) {
            return Err(AudexError::InvalidData(format!(
                "Language code must be a 3-letter ASCII identifier, got '{}'",
                lang
            )));
        }

        Ok([
            bytes[0].to_ascii_lowercase(),
            bytes[1].to_ascii_lowercase(),
            bytes[2].to_ascii_lowercase(),
        ])
    }

    fn ensure_easy_tags_mut(&mut self) -> Result<&mut crate::easyid3::EasyID3> {
        if self.tags.is_none() {
            self.add_tags()?;
        }

        self.tags
            .as_mut()
            .ok_or_else(|| AudexError::InvalidData("No tags available".to_string()))
    }

    fn load_easy_tags(path: &Path) -> Result<Option<crate::easyid3::EasyID3>> {
        match crate::easyid3::EasyID3::load(path) {
            Ok(tags) => Ok(Some(tags)),
            Err(AudexError::ID3NoHeaderError) | Err(AudexError::HeaderNotFound) => Ok(None),
            // No ID3v2 header AND no ID3v1 tags — treat as empty, tags can be created later
            Err(AudexError::InvalidData(ref msg)) if msg.contains("No ID3 tags found") => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Create new EasyMP3 instance
    pub fn new() -> Self {
        Self {
            info: MPEGInfo::default(),
            tags: None,
            filename: None,
        }
    }

    /// Load EasyMP3 from file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy().to_string();

        // Preserve actual parse failures instead of silently treating them as
        // "no tags present".
        let easy_tags = Self::load_easy_tags(path)?;

        // Parse MPEG stream info using the MP3 loader
        let mp3 = MP3::from_file(path)?;

        Ok(Self {
            info: mp3.info,
            tags: easy_tags,
            filename: Some(path_str),
        })
    }

    /// Register a text key mapping for dynamic key registration
    ///
    /// This allows mapping custom keys to ID3 text frames at runtime.
    pub fn register_text_key(&mut self, _key: &str, _frame_id: &str) -> Result<()> {
        self.ensure_easy_tags_mut()?
            .register_text_key(_key, _frame_id)
    }

    /// Register a TXXX key mapping for user-defined text frames
    ///
    /// This allows mapping custom keys to ID3 TXXX frames with descriptions.
    pub fn register_txxx_key(&mut self, _key: &str, _description: &str) -> Result<()> {
        self.ensure_easy_tags_mut()?
            .register_txxx_key(_key, _description)
    }

    /// Set a generic ID3 frame directly
    ///
    /// This provides direct frame manipulation for advanced use cases.
    pub fn set_frame(&mut self, frame_id: &str, frame_data: Vec<String>) -> Result<()> {
        self.ensure_easy_tags_mut()?
            .id3
            .add_text_frame(frame_id, frame_data)
    }

    /// Set TDAT frame for date information
    pub fn set_tdat_frame(&mut self, date_ddmm: &str) -> Result<()> {
        self.set_frame("TDAT", vec![date_ddmm.to_string()])
    }

    /// Set TPUB frame for publisher information
    pub fn set_tpub_frame(&mut self, publisher: &str) -> Result<()> {
        self.set_frame("TPUB", vec![publisher.to_string()])
    }

    /// Set TXXX frame for user-defined text information
    pub fn set_txxx_frame(&mut self, description: &str, text: &str) -> Result<()> {
        let frame_key = format!("TXXX:{}", description);
        self.set_frame(&frame_key, vec![text.to_string()])
    }

    /// Set COMM frame for comments
    pub fn set_comm_frame(&mut self, text: &str, _lang: &str) -> Result<()> {
        use crate::id3::{COMM, specs::TextEncoding};

        let lang = Self::parse_language_code(_lang)?;
        let frame = COMM::new(TextEncoding::Utf16, lang, String::new(), text.to_string());
        self.ensure_easy_tags_mut()?.id3.add(Box::new(frame))
    }

    /// Set USLT frame for unsynchronized lyrics
    pub fn set_uslt_frame(&mut self, lyrics: &str, _lang: &str) -> Result<()> {
        use crate::id3::{USLT, specs::TextEncoding};

        let lang = Self::parse_language_code(_lang)?;
        let frame = USLT::new(TextEncoding::Utf16, lang, String::new(), lyrics.to_string());
        self.ensure_easy_tags_mut()?.id3.add(Box::new(frame))
    }

    /// Set APIC frame for attached pictures
    pub fn set_apic_frame(
        &mut self,
        _data: &[u8],
        _mime: &str,
        _pic_type: u8,
        _description: &str,
    ) -> Result<()> {
        use crate::id3::{APIC, PictureType, specs::TextEncoding};

        let frame = APIC::new(
            TextEncoding::Utf16,
            _mime.to_string(),
            PictureType::from(_pic_type),
            _description.to_string(),
            _data.to_vec(),
        );
        self.ensure_easy_tags_mut()?.id3.add(Box::new(frame))
    }

    /// Save EasyMP3 with default options
    pub fn save(&mut self) -> Result<()> {
        debug_event!("saving EasyMP3 tags");
        self.save_with_options(None, None, None, None)
    }

    /// Save EasyMP3 with format-specific options
    pub fn save_with_options(
        &mut self,
        file_path: Option<&str>,
        v1: Option<u8>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        // Use provided file_path or fall back to stored filename
        let target_path = match file_path {
            Some(path) => path,
            None => self.filename.as_deref().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };

        // Set default values for format compatibility
        let v1_option = v1.unwrap_or(2); // Default to CREATE (2)
        let v2_version_option = v2_version.unwrap_or(3); // Default to v2.3
        let v23_sep_string = v23_sep.map(|s| s.to_string()); // Convert Option<&str> to Option<String>

        let tags = self
            .tags
            .as_mut()
            .ok_or_else(|| AudexError::InvalidData("No tags available for saving".to_string()))?;

        // Use the new in-place ID3 modification - performs efficient byte manipulation
        // instead of rebuilding the entire file.
        tags.id3.save(
            target_path,
            v1_option,
            v2_version_option,
            v23_sep_string,
            None,
        )?;

        Ok(())
    }
}

impl Default for EasyMP3 {
    fn default() -> Self {
        Self::new()
    }
}

// Async methods for EasyMP3 - feature-gated for async runtime support
#[cfg(feature = "async")]
impl EasyMP3 {
    /// Load EasyMP3 from file asynchronously
    ///
    /// Reads MPEG stream information and EasyID3 tags from the specified file
    /// using non-blocking I/O operations for improved concurrency.
    ///
    /// # Arguments
    /// * `path` - Path to the MP3 file
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded EasyMP3 with simplified tag interface
    /// * `Err(AudexError)` - Error occurred during file reading or parsing
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file_async(path).await
    }

    /// Load EasyMP3 from file asynchronously (alias for load_async)
    ///
    /// This is the primary async loading method that mirrors the synchronous
    /// `from_file` method behavior with the simplified EasyID3 tag interface.
    pub async fn from_file_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy().to_string();

        let easy_tags = match crate::easyid3::EasyID3::load_async(path).await {
            Ok(tags) => Some(tags),
            Err(AudexError::ID3NoHeaderError) | Err(AudexError::HeaderNotFound) => None,
            // No ID3v2 header AND no ID3v1 tags — treat as empty, tags can be created later
            Err(AudexError::InvalidData(ref msg)) if msg.contains("No ID3 tags found") => None,
            Err(err) => return Err(err),
        };

        // Parse MPEG stream info using async MP3 loader
        let mp3 = MP3::from_file_async(path).await?;

        Ok(Self {
            info: mp3.info,
            tags: easy_tags,
            filename: Some(path_str),
        })
    }

    /// Save EasyMP3 tags asynchronously with default options
    ///
    /// Writes tag modifications back to the file using efficient
    /// in-place byte manipulation when possible.
    pub async fn save_async(&mut self) -> Result<()> {
        self.save_with_options_async(None, None, None, None).await
    }

    /// Save EasyMP3 tags asynchronously with format-specific options
    ///
    /// # Arguments
    /// * `file_path` - Optional target path (uses stored filename if None)
    /// * `v1` - ID3v1 save option (0=REMOVE, 1=UPDATE, 2=CREATE)
    /// * `v2_version` - Target ID3v2 version (3 or 4)
    /// * `v23_sep` - Separator for multiple values in v2.3 format
    pub async fn save_with_options_async(
        &mut self,
        file_path: Option<&str>,
        v1: Option<u8>,
        v2_version: Option<u8>,
        v23_sep: Option<&str>,
    ) -> Result<()> {
        // Determine target file path
        let target_path = match file_path {
            Some(path) => path.to_string(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No file path provided and no filename stored".to_string())
            })?,
        };

        // Apply default format options for compatibility
        let v1_option = v1.unwrap_or(2);
        let v2_version_option = v2_version.unwrap_or(3);
        let v23_sep_string = v23_sep.map(|s| s.to_string());

        let tags = self
            .tags
            .as_ref()
            .ok_or_else(|| AudexError::InvalidData("No tags available for saving".to_string()))?;

        let config = crate::id3::tags::ID3SaveConfig {
            v2_version: v2_version_option,
            v2_minor: 0,
            v23_sep: v23_sep_string.clone().unwrap_or_else(|| "/".to_string()),
            v23_separator: v23_sep_string
                .as_deref()
                .and_then(|s| s.as_bytes().first().copied())
                .unwrap_or(b'/'),
            padding: None,
            merge_frames: true,
            preserve_unknown: true,
            compress_frames: false,
            write_v1: match v1_option {
                0 => crate::id3::file::ID3v1SaveOptions::REMOVE,
                1 => crate::id3::file::ID3v1SaveOptions::UPDATE,
                _ => crate::id3::file::ID3v1SaveOptions::CREATE,
            },
            unsync: false,
            extended_header: false,
            convert_v24_frames: true,
        };

        tags.id3.save_to_file_async(&target_path, &config).await?;

        Ok(())
    }

    /// Clear all tags asynchronously
    ///
    /// Removes tag data and saves the cleared state to disk.
    pub async fn clear_async(&mut self) -> Result<()> {
        if let Some(ref mut tags) = self.tags {
            tags.id3.dict.clear();
            tags.id3.frames_by_id.clear();
        }
        self.save_async().await
    }
}

impl FileType for EasyMP3 {
    type Tags = crate::easyid3::EasyID3;
    type Info = MPEGInfo;

    fn format_id() -> &'static str {
        "EasyMP3"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::from_file(path)
    }

    fn save(&mut self) -> Result<()> {
        // Delegate to the EasyMP3 save method with default options
        EasyMP3::save(self)
    }

    fn clear(&mut self) -> Result<()> {
        // Clear all ID3 tags
        if let Some(ref mut tags) = self.tags {
            tags.clear()?;
        }
        self.save()
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

        // Create new EasyID3 tags with filename propagation
        let mut tags = crate::easyid3::EasyID3::new();
        if let Some(ref filename) = self.filename {
            tags.filename = Some(filename.clone());
        }

        self.tags = Some(tags);
        Ok(())
    }

    fn score(filename: &str, header: &[u8]) -> i32 {
        // Same scoring as MP3
        MP3::score(filename, header)
    }

    fn mime_types() -> &'static [&'static str] {
        MP3::mime_types()
    }
}
