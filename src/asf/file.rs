//! ASF file handling/asf
//!
//! This module provides complete ASF file support with sophisticated tag distribution,
//! comprehensive file I/O operations, and advanced metadata management matching the
//! functionality following standard ASF specification.

use super::attrs::{ASFAttribute, ASFTags};
use super::objects::ASFObject;
use super::util::ASFGUIDs;
use crate::util::resize_bytes;
use crate::{AudexError, FileType, ReadWriteSeek, Result, StreamInfo};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(feature = "async")]
use crate::util::resize_bytes_async;
#[cfg(feature = "async")]
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// Audio stream information extracted from ASF/WMA file headers
///
/// Populated from the ASF Header Object's FileProperties, StreamProperties,
/// and CodecList objects during file loading.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFInfo {
    /// Audio duration in seconds (from FileProperties play_duration minus preroll)
    pub length: f64,
    /// Audio sample rate in Hz (from StreamProperties format data)
    pub sample_rate: u32,
    /// Average bitrate in bits per second (from StreamProperties format data)
    pub bitrate: u32,
    /// Number of audio channels (from StreamProperties format data)
    pub channels: u16,
    /// Codec info string (e.g. "Windows Media Audio 9 Standard")
    pub codec_type: String,
    /// Codec name (e.g. "Windows Media Audio V2")
    pub codec_name: String,
    /// Codec description string from the CodecList object
    pub codec_description: String,
    /// Maximum instantaneous bitrate in bps (from FileProperties)
    pub max_bitrate: Option<u32>,
    /// Preroll time in milliseconds (subtracted from play_duration)
    pub preroll: Option<u64>,
    /// Broadcast/seekable flags from FileProperties
    pub flags: Option<u32>,
    /// Total file size in bytes (from FileProperties)
    pub file_size: Option<u64>,
}

impl Default for ASFInfo {
    fn default() -> Self {
        Self {
            length: 0.0,
            sample_rate: 0,
            bitrate: 0,
            channels: 0,
            codec_type: String::new(),
            codec_name: String::new(),
            codec_description: String::new(),
            max_bitrate: None,
            preroll: None,
            flags: None,
            file_size: None,
        }
    }
}

impl ASFInfo {
    /// Create ASFInfo from parsed ASF objects
    pub fn from_objects(objects: &[Box<dyn ASFObject>]) -> Result<Self> {
        let mut info = Self::default();

        use super::objects::*;
        use super::util::ASFGUIDs;

        for obj in objects {
            let guid = obj.get_guid();

            // Extract file properties
            if guid == ASFGUIDs::FILE_PROPERTIES {
                if let Some(fp) = obj.as_any().downcast_ref::<FilePropertiesObject>() {
                    // Calculate duration from play_duration with preroll compensation
                    let play_duration_sec = fp.play_duration as f64 / 10_000_000.0;
                    let preroll_sec = fp.preroll as f64 / 1000.0;
                    info.length = (play_duration_sec - preroll_sec).max(0.0);
                    info.file_size = Some(fp.file_size);
                    info.max_bitrate = Some(fp.max_bitrate);
                    info.preroll = Some(fp.preroll);
                    info.flags = Some(fp.flags);
                }
            }
            // Extract stream properties
            else if guid == ASFGUIDs::STREAM_PROPERTIES {
                if let Some(sp) = obj.as_any().downcast_ref::<StreamPropertiesObject>() {
                    // Check if this is an audio stream
                    if sp.stream_type == ASFGUIDs::AUDIO_STREAM && sp.type_specific_data.len() >= 18
                    {
                        // Parse audio format from type-specific data
                        if let (Ok(channels), Ok(sample_rate), Ok(avg_bytes_per_sec)) = (
                            super::util::ASFUtil::parse_u16_le(&sp.type_specific_data[2..4]),
                            super::util::ASFUtil::parse_u32_le(&sp.type_specific_data[4..8]),
                            super::util::ASFUtil::parse_u32_le(&sp.type_specific_data[8..12]),
                        ) {
                            info.channels = channels;
                            info.sample_rate = sample_rate;
                            // Convert bytes/sec to bits/sec using u64 to prevent
                            // overflow for high bitrate streams (avg_bytes_per_sec > 536 million)
                            info.bitrate =
                                u32::try_from((avg_bytes_per_sec as u64) * 8).unwrap_or(u32::MAX);
                        }
                    }
                }
            }
            // Extract codec information
            else if guid == ASFGUIDs::CODEC_LIST {
                if let Some(cl) = obj.as_any().downcast_ref::<CodecListObject>() {
                    // Find first audio codec
                    for codec in &cl.codecs {
                        if codec.codec_type == 2 && info.codec_type.is_empty() {
                            // Audio codec
                            info.codec_type = if codec.codec_info.is_empty() {
                                codec.name.clone()
                            } else {
                                codec.codec_info.clone()
                            };
                            info.codec_name = codec.name.clone();
                            info.codec_description = codec.description.clone();
                            break;
                        }
                    }
                }
            }
        }

        Ok(info)
    }

    /// Format stream information for display
    pub fn pprint(&self) -> String {
        let codec = if !self.codec_type.is_empty() {
            &self.codec_type
        } else if !self.codec_name.is_empty() {
            &self.codec_name
        } else {
            "???"
        };

        format!(
            "ASF ({}) {} bps, {} Hz, {} channels, {:.2} seconds",
            codec, self.bitrate, self.sample_rate, self.channels, self.length
        )
    }
}

impl StreamInfo for ASFInfo {
    fn length(&self) -> Option<Duration> {
        if self.length > 0.0 {
            Duration::try_from_secs_f64(self.length).ok()
        } else {
            None
        }
    }

    fn bitrate(&self) -> Option<u32> {
        if self.bitrate > 0 {
            Some(self.bitrate)
        } else {
            None
        }
    }

    fn sample_rate(&self) -> Option<u32> {
        if self.sample_rate > 0 {
            Some(self.sample_rate)
        } else {
            None
        }
    }

    fn channels(&self) -> Option<u16> {
        if self.channels > 0 {
            Some(self.channels)
        } else {
            None
        }
    }

    fn bits_per_sample(&self) -> Option<u16> {
        // ASF doesn't store bits per sample in a standard way
        None
    }
}

/// ASF file structure for metadata and stream handling
#[derive(Debug)]
pub struct ASF {
    pub info: ASFInfo,
    pub tags: ASFTags,
    filename: Option<PathBuf>,
    // Internal tag storage by object GUID
    _tags: HashMap<[u8; 16], Vec<(String, ASFAttribute)>>,
    // Header objects
    header_objects: Vec<Box<dyn ASFObject>>,
    // Original file header size for padding calculations
    original_header_size: Option<u64>,
}

impl ASF {
    /// Create a new empty ASF file
    pub fn new() -> Self {
        Self {
            info: ASFInfo::default(),
            tags: ASFTags::new(),
            filename: None,
            _tags: HashMap::new(),
            header_objects: Vec::new(),
            original_header_size: None,
        }
    }

    /// Add tags functionality - initializes tags if they don't exist
    pub fn add_tags(&mut self) -> Result<()> {
        // Tags are always present; this is kept for API compatibility.
        Ok(())
    }

    /// Update info with additional fields from FilePropertiesObject
    fn update_info_from_objects(&mut self) {
        use super::objects::FilePropertiesObject;
        use super::util::ASFGUIDs;

        // Find FilePropertiesObject and extract additional fields
        for obj in &self.header_objects {
            if obj.get_guid() == ASFGUIDs::FILE_PROPERTIES {
                if let Some(fp) = obj.as_any().downcast_ref::<FilePropertiesObject>() {
                    self.info.max_bitrate = Some(fp.max_bitrate);
                    self.info.preroll = Some(fp.preroll);
                    self.info.flags = Some(fp.flags);
                    self.info.file_size = Some(fp.file_size);
                    break;
                }
            }
        }
    }

    /// Load ASF file from path
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all, fields(path = %path.as_ref().display())))]
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        debug_event!("parsing ASF file");

        // Validate file exists and is readable
        if !path.exists() {
            return Err(AudexError::InvalidData(format!(
                "File does not exist: {}",
                path.display()
            )));
        }

        if !path.is_file() {
            return Err(AudexError::InvalidData(format!(
                "Path is not a file: {}",
                path.display()
            )));
        }

        let mut file = File::open(path)?;

        // Check file size - ASF files need at least 30 bytes for a minimal header
        let metadata = file.metadata()?;
        if metadata.len() < 30 {
            return Err(AudexError::InvalidData(
                "File too small to be a valid ASF file".to_string(),
            ));
        }

        // Validate ASF header signature
        let mut header_guid = [0u8; 16];
        file.read_exact(&mut header_guid)?;
        if header_guid != ASFGUIDs::HEADER {
            return Err(AudexError::InvalidData(
                "Invalid ASF header GUID".to_string(),
            ));
        }

        // Reset file position for full parsing
        file.seek(SeekFrom::Start(0))?;

        let mut asf = Self::new();
        asf.filename = Some(path.to_path_buf());

        // Parse ASF header and get all objects
        let mut context = super::objects::ASFContext::default();

        match super::objects::HeaderObject::parse_full(&mut file, &mut context) {
            Ok(header) => {
                trace_event!(
                    object_count = header.objects.len(),
                    "ASF header objects parsed"
                );
                // Store the header objects
                asf.header_objects = header.objects;

                // Extract stream info from context (already populated during parsing)
                // Convert from objects::ASFInfo to file::ASFInfo
                asf.info = ASFInfo {
                    length: context.info.length,
                    sample_rate: context.info.sample_rate,
                    bitrate: context.info.bitrate,
                    channels: context.info.channels,
                    codec_type: context.info.codec_type,
                    codec_name: context.info.codec_name,
                    codec_description: context.info.codec_description,
                    max_bitrate: None, // These are populated from objects if available
                    preroll: None,
                    flags: None,
                    file_size: None,
                };

                // Now update with additional fields from FilePropertiesObject if present
                asf.update_info_from_objects();

                // Extract tags from context (already populated during parsing)
                asf.tags = context.tags;
                debug_event!(attribute_count = asf.tags.len(), "ASF attributes loaded");
            }
            Err(e) => {
                // Propagate parse errors so callers can distinguish
                // "no metadata" from "corrupted file"
                return Err(e);
            }
        }

        // Store original header size for padding calculations
        file.seek(std::io::SeekFrom::Start(0))?;
        let (header_size, _) = super::objects::HeaderObject::parse_size(&mut file)?;
        asf.original_header_size = Some(header_size);

        Ok(asf)
    }

    /// Save changes back to the file.
    ///
    /// # Memory considerations
    ///
    /// The ASF save path reads the **entire file** into an in-memory buffer so
    /// that the header can be resized and rewritten atomically. A dedicated
    /// whole-file size guard in `save_to_writer_inner` rejects files larger
    /// than 512 MB before any allocation occurs, returning
    /// `AudexError::InvalidData` instead.
    ///
    /// Files larger than this in-memory writer guard cannot currently be saved
    /// through this path. Peak memory usage for eligible files is roughly equal
    /// to the file size.
    pub fn save(&mut self) -> Result<()> {
        debug_event!("saving ASF attributes");
        if let Some(path) = self.filename.clone() {
            self.save_with_options(Some(path), None)
        } else {
            warn_event!("no filename available for ASF save");
            Err(AudexError::InvalidData(
                "No filename available for saving".to_string(),
            ))
        }
    }

    /// Save with advanced options
    pub fn save_with_options<P>(
        &mut self,
        path: Option<P>,
        padding_func: Option<fn(i64) -> i64>,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        self.save_internal(path, padding_func, true)
    }

    /// Internal save method with option to skip vendor tag
    fn save_internal<P>(
        &mut self,
        path: Option<P>,
        padding_func: Option<fn(i64) -> i64>,
        add_vendor_tag: bool,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let file_path = match path {
            Some(p) => p.as_ref().to_path_buf(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No filename available for saving".to_string())
            })?,
        };

        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&file_path)?;

        self.save_to_writer_inner(&mut file, padding_func, add_vendor_tag)?;

        // Update stored filename
        self.filename = Some(file_path);

        Ok(())
    }

    /// Core save logic that works with any reader/writer/seeker.
    ///
    /// Reads all data into an in-memory cursor, performs header resize and
    /// write operations, then writes the result back to the original stream.
    ///
    /// # Memory usage
    ///
    /// This method reads the entire file into memory so that the header can be
    /// resized in place. Peak memory consumption is roughly equal to the file
    /// size (the header is re-rendered separately but is small), bounded by
    /// `crate::limits::MAX_IN_MEMORY_WRITER_FILE`. For large ASF files this can be
    /// significant. The file-path-based [`save`](Self::save) method uses
    /// the same code path internally.
    fn save_to_writer_inner(
        &mut self,
        writer: &mut dyn ReadWriteSeek,
        padding_func: Option<fn(i64) -> i64>,
        add_vendor_tag: bool,
    ) -> Result<()> {
        // Add Audex vendor information (unless skipped for clear operation)
        if add_vendor_tag {
            self.add_tags()?;
            let tags = &mut self.tags;
            let audex_info = format!("Audex {}", crate::VERSION_STRING);
            tags.set(
                "WM/EncodingSettings".to_string(),
                vec![ASFAttribute::unicode(audex_info)],
            );
        }

        // Distribute tags to appropriate objects
        self.distribute_tags()?;

        // Ensure required objects exist
        self.ensure_required_objects()?;

        // Check file size before buffering to prevent OOM on large files.
        // This path reads the entire stream for in-memory header manipulation.
        let file_size = writer.seek(SeekFrom::End(0))?;
        let max_read_size = crate::limits::MAX_IN_MEMORY_WRITER_FILE;
        if file_size > max_read_size {
            return Err(AudexError::InvalidData(format!(
                "File size ({} bytes) exceeds maximum for in-memory ASF save ({} bytes)",
                file_size, max_read_size
            )));
        }

        // Read all data into memory so we can use resize_bytes (requires 'static).
        // NOTE: This buffers the entire file, which is safe given the size guard
        // above. A future optimisation could stream the audio data portion after
        // the header, only buffering the header object for in-place manipulation.
        // That would reduce peak memory from O(file_size) to O(header_size).
        writer.seek(SeekFrom::Start(0))?;
        let mut all_data = Vec::new();
        writer.read_to_end(&mut all_data)?;

        let mut cursor = std::io::Cursor::new(all_data);

        // Parse current header size and total data size
        let (old_header_size, _) = super::objects::HeaderObject::parse_size(&mut cursor)?;
        let total_file_size = std::io::Seek::seek(&mut cursor, SeekFrom::End(0))?;

        // Create context for rendering
        let context = super::objects::ASFContext {
            info: super::objects::ASFInfo {
                length: self.info.length,
                sample_rate: self.info.sample_rate,
                bitrate: self.info.bitrate,
                channels: self.info.channels,
                codec_type: self.info.codec_type.clone(),
                codec_name: self.info.codec_name.clone(),
                codec_description: self.info.codec_description.clone(),
            },
            tags: self.tags.clone(),
            parse_limits: crate::limits::ParseLimits::default(),
            nesting_depth: 0,
        };

        // Create header object from stored objects
        let mut header = super::objects::HeaderObject::new();
        for obj in &self.header_objects {
            header.objects.push(obj.clone_boxed());
        }

        // Render header with padding
        let header_data = header.render_full(
            &context,
            old_header_size,
            padding_func.map(|f| f as fn(i64) -> i64),
            total_file_size,
        )?;

        // Resize if needed and write new header
        let new_header_size = header_data.len() as u64;
        if new_header_size != old_header_size {
            resize_bytes(&mut cursor, old_header_size, new_header_size, 0)?;
        }

        std::io::Seek::seek(&mut cursor, SeekFrom::Start(0))?;
        cursor.get_mut()[..header_data.len()].copy_from_slice(&header_data);

        // Write the modified data back to the original writer
        let final_data = cursor.into_inner();
        writer.seek(SeekFrom::Start(0))?;
        writer.write_all(&final_data)?;

        // Truncate or zero-fill any stale trailing bytes from the original
        // content. When metadata shrinks, the output is shorter than the
        // input. Physical truncation requires a File handle; for generic
        // writers we zero the stale region to prevent old data leakage.
        let written_end = writer.stream_position()?;
        crate::util::truncate_writer_dyn(writer, written_end)?;

        writer.flush()?;

        Ok(())
    }

    /// Core clear logic that works with any reader/writer/seeker.
    ///
    /// Clears all tags and saves with minimal padding.
    fn clear_writer_inner(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        self.tags.clear();
        self._tags.clear();

        self.save_to_writer_inner(writer, Some(|_| 0), false)
    }

    /// Distribute tags to appropriate objects
    fn distribute_tags(&mut self) -> Result<()> {
        use super::attrs::{ASFAttributeType, CONTENT_DESCRIPTION_NAMES};

        let mut to_content_description = Vec::new();
        let mut to_extended_content_description = Vec::new();
        let mut to_metadata = Vec::new();
        let mut to_metadata_library = Vec::new();

        // Distribute each tag to appropriate object based on ASF specification
        for (name, value) in self.tags.items() {
            let library_only =
                value.data_size() > 0xFFFF || matches!(value.get_type(), ASFAttributeType::Guid);
            let can_cont_desc = matches!(value.get_type(), ASFAttributeType::Unicode);

            if library_only || value.language().is_some() {
                // Large values or language-specific values go to metadata library
                to_metadata_library.push((name.clone(), value.clone()));
            } else if value.stream().is_some() {
                // Stream-specific values go to metadata object
                if !to_metadata.iter().any(|(k, _)| k == name) {
                    to_metadata.push((name.clone(), value.clone()));
                } else {
                    // Duplicate - goes to metadata library
                    to_metadata_library.push((name.clone(), value.clone()));
                }
            } else if CONTENT_DESCRIPTION_NAMES.contains(&name.as_str()) {
                // Standard content description fields
                if !to_content_description.iter().any(|(k, _)| k == name) && can_cont_desc {
                    to_content_description.push((name.clone(), value.clone()));
                } else {
                    // Can't fit in content description - goes to metadata library
                    to_metadata_library.push((name.clone(), value.clone()));
                }
            } else {
                // Extended content description
                if !to_extended_content_description
                    .iter()
                    .any(|(k, _)| k == name)
                {
                    to_extended_content_description.push((name.clone(), value.clone()));
                } else {
                    // Duplicate - goes to metadata library
                    to_metadata_library.push((name.clone(), value.clone()));
                }
            }
        }

        // Update objects with distributed tags
        self.update_content_description_object(&to_content_description)?;
        self.update_extended_content_description_object(&to_extended_content_description)?;
        self.update_metadata_object(&to_metadata)?;
        self.update_metadata_library_object(&to_metadata_library)?;

        Ok(())
    }

    /// Update ContentDescriptionObject with tags
    fn update_content_description_object(&mut self, tags: &[(String, ASFAttribute)]) -> Result<()> {
        use super::attrs::ASFAttribute;
        use super::objects::ContentDescriptionObject;
        use super::util::ASFGUIDs;

        // First, check if object exists
        let mut found_index = None;
        for (i, obj) in self.header_objects.iter().enumerate() {
            if obj.get_guid() == ASFGUIDs::CONTENT_DESCRIPTION {
                found_index = Some(i);
                break;
            }
        }

        // If not found, create new one
        if found_index.is_none() {
            let new_cd = Box::new(ContentDescriptionObject::new());
            self.header_objects.push(new_cd);
            found_index = Some(self.header_objects.len() - 1);
        }

        // Now safely get mutable reference and update
        if let Some(index) = found_index {
            if let Some(cd) = self.header_objects[index]
                .as_any_mut()
                .downcast_mut::<ContentDescriptionObject>()
            {
                // Clear existing values
                cd.title = None;
                cd.author = None;
                cd.copyright = None;
                cd.description = None;
                cd.rating = None;

                // Set new values
                for (name, attr) in tags {
                    if let ASFAttribute::Unicode(unicode_attr) = attr {
                        match name.as_str() {
                            "Title" => cd.title = Some(unicode_attr.value.clone()),
                            "Author" => cd.author = Some(unicode_attr.value.clone()),
                            "Copyright" => cd.copyright = Some(unicode_attr.value.clone()),
                            "Description" => cd.description = Some(unicode_attr.value.clone()),
                            "Rating" => cd.rating = Some(unicode_attr.value.clone()),
                            _ => {} // Unknown field
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Update ExtendedContentDescriptionObject with tags
    fn update_extended_content_description_object(
        &mut self,
        tags: &[(String, ASFAttribute)],
    ) -> Result<()> {
        use super::objects::ExtendedContentDescriptionObject;
        use super::util::ASFGUIDs;

        // First pass: find existing object
        let mut found_index = None;
        for (i, obj) in self.header_objects.iter().enumerate() {
            if obj.get_guid() == ASFGUIDs::EXTENDED_CONTENT_DESCRIPTION {
                found_index = Some(i);
                break;
            }
        }

        // Update or create object
        if let Some(index) = found_index {
            // Update existing object
            if let Some(ecd) = self.header_objects[index]
                .as_any_mut()
                .downcast_mut::<ExtendedContentDescriptionObject>()
            {
                ecd.attributes.clear();
                // Preserve insertion order to match the ordering
                for (name, attr) in tags {
                    ecd.attributes.push((name.clone(), attr.clone()));
                }
            }
        } else {
            // Create new object
            let mut new_ecd = ExtendedContentDescriptionObject::new();
            // Preserve insertion order to match the ordering
            for (name, attr) in tags {
                new_ecd.attributes.push((name.clone(), attr.clone()));
            }
            self.header_objects.push(Box::new(new_ecd));
        }

        Ok(())
    }

    /// Update MetadataObject with tags
    fn update_metadata_object(&mut self, tags: &[(String, ASFAttribute)]) -> Result<()> {
        use super::objects::HeaderExtensionObject;
        use super::util::ASFGUIDs;

        if tags.is_empty() {
            return Ok(());
        }

        // First, find existing HeaderExtensionObject index
        let mut found_index = None;
        for (i, obj) in self.header_objects.iter().enumerate() {
            if obj.get_guid() == ASFGUIDs::HEADER_EXTENSION {
                found_index = Some(i);
                break;
            }
        }

        if let Some(index) = found_index {
            // Update existing HeaderExtensionObject
            if let Some(he) = self.header_objects[index]
                .as_any_mut()
                .downcast_mut::<HeaderExtensionObject>()
            {
                Self::update_metadata_in_header_ext_static(he, tags)?;
            }
        } else {
            // Create new HeaderExtensionObject
            let mut new_he = HeaderExtensionObject::new();
            Self::update_metadata_in_header_ext_static(&mut new_he, tags)?;
            self.header_objects.push(Box::new(new_he));
        }

        Ok(())
    }

    /// Helper method to update metadata in a HeaderExtensionObject
    fn update_metadata_in_header_ext_static(
        he: &mut super::objects::HeaderExtensionObject,
        tags: &[(String, ASFAttribute)],
    ) -> Result<()> {
        use super::objects::MetadataObject;
        use super::util::ASFGUIDs;

        // Find existing MetadataObject
        let mut found_metadata = false;
        for ext_obj in &mut he.objects {
            if ext_obj.get_guid() == ASFGUIDs::METADATA {
                if let Some(mo) = ext_obj.as_any_mut().downcast_mut::<MetadataObject>() {
                    found_metadata = true;
                    mo.attributes.clear();
                    for (name, attr) in tags {
                        mo.attributes.push((name.clone(), attr.clone()));
                    }
                    break;
                }
            }
        }

        if !found_metadata {
            // Create new MetadataObject
            let mut new_mo = MetadataObject::new();
            for (name, attr) in tags {
                new_mo.attributes.push((name.clone(), attr.clone()));
            }
            he.objects.push(Box::new(new_mo));
        }

        Ok(())
    }

    /// Update MetadataLibraryObject with tags
    fn update_metadata_library_object(&mut self, tags: &[(String, ASFAttribute)]) -> Result<()> {
        use super::objects::HeaderExtensionObject;
        use super::util::ASFGUIDs;

        if tags.is_empty() {
            return Ok(());
        }

        // First, find existing HeaderExtensionObject index
        let mut found_index = None;
        for (i, obj) in self.header_objects.iter().enumerate() {
            if obj.get_guid() == ASFGUIDs::HEADER_EXTENSION {
                found_index = Some(i);
                break;
            }
        }

        if let Some(index) = found_index {
            // Update existing HeaderExtensionObject
            if let Some(he) = self.header_objects[index]
                .as_any_mut()
                .downcast_mut::<HeaderExtensionObject>()
            {
                Self::update_metadata_library_in_header_ext_static(he, tags)?;
            }
        } else {
            // Create new HeaderExtensionObject
            let mut new_he = HeaderExtensionObject::new();
            Self::update_metadata_library_in_header_ext_static(&mut new_he, tags)?;
            self.header_objects.push(Box::new(new_he));
        }

        Ok(())
    }

    /// Helper method to update metadata library in a HeaderExtensionObject
    fn update_metadata_library_in_header_ext_static(
        he: &mut super::objects::HeaderExtensionObject,
        tags: &[(String, ASFAttribute)],
    ) -> Result<()> {
        use super::objects::MetadataLibraryObject;
        use super::util::ASFGUIDs;

        // Find existing MetadataLibraryObject
        let mut found_metadata_library = false;
        for ext_obj in &mut he.objects {
            if ext_obj.get_guid() == ASFGUIDs::METADATA_LIBRARY {
                if let Some(mlo) = ext_obj.as_any_mut().downcast_mut::<MetadataLibraryObject>() {
                    found_metadata_library = true;
                    mlo.attributes.clear();
                    for (name, attr) in tags {
                        mlo.attributes.push((name.clone(), attr.clone()));
                    }
                    break;
                }
            }
        }

        if !found_metadata_library {
            // Create new MetadataLibraryObject
            let mut new_mlo = MetadataLibraryObject::new();
            for (name, attr) in tags {
                new_mlo.attributes.push((name.clone(), attr.clone()));
            }
            he.objects.push(Box::new(new_mlo));
        }

        Ok(())
    }

    /// Ensure required objects exist in the header
    fn ensure_required_objects(&mut self) -> Result<()> {
        use super::objects::*;
        use super::util::ASFGUIDs;

        // Check for required objects and add if missing
        let mut has_file_props = false;
        let mut has_stream_props = false;
        let mut has_content_desc = false;
        let mut has_ext_content_desc = false;
        let mut has_header_ext = false;

        for obj in &self.header_objects {
            let guid = obj.get_guid();
            if guid == ASFGUIDs::FILE_PROPERTIES {
                has_file_props = true;
            } else if guid == ASFGUIDs::STREAM_PROPERTIES {
                has_stream_props = true;
            } else if guid == ASFGUIDs::CONTENT_DESCRIPTION {
                has_content_desc = true;
            } else if guid == ASFGUIDs::EXTENDED_CONTENT_DESCRIPTION {
                has_ext_content_desc = true;
            } else if guid == ASFGUIDs::HEADER_EXTENSION {
                has_header_ext = true;
            }
        }

        // Add missing required objects
        if !has_file_props {
            self.header_objects
                .push(Box::new(FilePropertiesObject::new()));
        }
        if !has_stream_props {
            self.header_objects
                .push(Box::new(StreamPropertiesObject::new()));
        }
        if !has_content_desc {
            self.header_objects
                .push(Box::new(ContentDescriptionObject::new()));
        }
        if !has_ext_content_desc {
            self.header_objects
                .push(Box::new(ExtendedContentDescriptionObject::new()));
        }
        if !has_header_ext {
            self.header_objects
                .push(Box::new(HeaderExtensionObject::new()));
        }

        // Ensure HeaderExtension has MetadataObject and MetadataLibraryObject
        for obj in &mut self.header_objects {
            if obj.get_guid() == ASFGUIDs::HEADER_EXTENSION {
                if let Some(he) = obj.as_any_mut().downcast_mut::<HeaderExtensionObject>() {
                    let has_metadata = he
                        .objects
                        .iter()
                        .any(|o| o.get_guid() == ASFGUIDs::METADATA);
                    let has_metadata_lib = he
                        .objects
                        .iter()
                        .any(|o| o.get_guid() == ASFGUIDs::METADATA_LIBRARY);

                    if !has_metadata {
                        he.objects.push(Box::new(MetadataObject::new()));
                    }
                    if !has_metadata_lib {
                        he.objects.push(Box::new(MetadataLibraryObject::new()));
                    }
                }
                break;
            }
        }

        Ok(())
    }

    /// Delete all tags from file
    pub fn clear(&mut self) -> Result<()> {
        // Clear all tags and save with minimal padding
        self.tags.clear();
        self._tags.clear();

        // Save changes using minimal padding, without adding vendor tag
        self.save_internal(None::<PathBuf>, Some(|_| 0), false)
    }

    /// Load ASF file asynchronously with non-blocking I/O
    ///
    /// This method provides the same functionality as `load()` but uses async I/O
    /// for better performance in concurrent applications.
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Validate file exists and is accessible (non-blocking)
        let pre_metadata = tokio::fs::metadata(path).await.map_err(|_| {
            AudexError::InvalidData(format!("File does not exist: {}", path.display()))
        })?;

        if !pre_metadata.is_file() {
            return Err(AudexError::InvalidData(format!(
                "Path is not a file: {}",
                path.display()
            )));
        }

        let mut file = TokioFile::open(path).await?;

        // Check minimum file size for valid ASF header
        let metadata = pre_metadata;
        if metadata.len() < 30 {
            return Err(AudexError::InvalidData(
                "File too small to be a valid ASF file".to_string(),
            ));
        }

        // Validate ASF header signature
        let mut header_guid = [0u8; 16];
        file.read_exact(&mut header_guid).await?;
        if header_guid != ASFGUIDs::HEADER {
            return Err(AudexError::InvalidData(
                "Invalid ASF header GUID".to_string(),
            ));
        }

        // Reset file position for complete parsing
        file.seek(SeekFrom::Start(0)).await?;

        let mut asf = Self::new();
        asf.filename = Some(path.to_path_buf());

        // Parse ASF header structure asynchronously
        asf.parse_header_async(&mut file).await?;

        // Store original header size for padding calculations during save
        file.seek(SeekFrom::Start(0)).await?;
        let (header_size, _) = Self::parse_size_async(&mut file).await?;
        asf.original_header_size = Some(header_size);

        Ok(asf)
    }

    /// Parse ASF header structure asynchronously
    ///
    /// Reads header objects and extracts stream information and metadata tags.
    #[cfg(feature = "async")]
    async fn parse_header_async(&mut self, file: &mut TokioFile) -> Result<()> {
        // Read and validate header signature
        let mut header_data = [0u8; 30];
        file.read_exact(&mut header_data).await?;

        if header_data[0..16] != ASFGUIDs::HEADER {
            return Err(AudexError::InvalidData("Not an ASF file".to_string()));
        }

        // Parse header size from little-endian bytes
        let total_size = u64::from_le_bytes([
            header_data[16],
            header_data[17],
            header_data[18],
            header_data[19],
            header_data[20],
            header_data[21],
            header_data[22],
            header_data[23],
        ]);

        // Parse number of header objects
        let num_objects = u32::from_le_bytes([
            header_data[24],
            header_data[25],
            header_data[26],
            header_data[27],
        ]);

        // Cross-validate the claimed header size against the actual stream
        // to prevent large allocations from spoofed headers in small files
        let stream_end = file.seek(SeekFrom::End(0)).await?;
        if total_size > stream_end {
            return Err(AudexError::InvalidData(format!(
                "ASF header size ({} bytes) exceeds file size ({} bytes)",
                total_size, stream_end
            )));
        }
        file.seek(SeekFrom::Start(30)).await?;

        let mut remaining_size = total_size.saturating_sub(30);
        let mut context = super::objects::ASFContext::default();

        // Parse each header object sequentially
        for i in 0..num_objects {
            if remaining_size < 24 {
                return Err(AudexError::InvalidData(format!(
                    "Invalid header size at object {}",
                    i
                )));
            }

            // Read object header (GUID + size)
            let mut obj_header = [0u8; 24];
            file.read_exact(&mut obj_header).await?;
            remaining_size = remaining_size.saturating_sub(24);

            let mut guid = [0u8; 16];
            guid.copy_from_slice(&obj_header[0..16]);

            let obj_size = u64::from_le_bytes([
                obj_header[16],
                obj_header[17],
                obj_header[18],
                obj_header[19],
                obj_header[20],
                obj_header[21],
                obj_header[22],
                obj_header[23],
            ]);

            if obj_size < 24 {
                return Err(AudexError::InvalidData("Object size too small".to_string()));
            }

            let payload_size = obj_size - 24;

            if remaining_size < payload_size {
                return Err(AudexError::InvalidData("Invalid object size".to_string()));
            }

            // Enforce the library-wide tag allocation ceiling
            crate::limits::ParseLimits::default()
                .check_tag_size(payload_size, "ASF object async")?;

            // Read object payload data
            // Use try_from to prevent silent truncation on 32-bit targets
            // where payload_size could exceed usize::MAX
            let payload_usize = usize::try_from(payload_size).map_err(|_| {
                AudexError::InvalidData(format!(
                    "ASF object payload {} bytes exceeds addressable range",
                    payload_size
                ))
            })?;
            let mut payload = vec![0u8; payload_usize];
            file.read_exact(&mut payload).await?;
            remaining_size = remaining_size.saturating_sub(payload_size);

            // Create and parse object based on GUID type
            let mut obj = super::objects::create_object_by_guid(guid);
            match obj.parse(&mut context, &payload) {
                Ok(()) => self.header_objects.push(obj),
                Err(_) => {
                    // Create unknown object as fallback for unrecognized GUIDs
                    let mut unknown = super::objects::UnknownObject::new(guid);
                    unknown.parse(&mut context, &payload)?;
                    self.header_objects.push(Box::new(unknown));
                }
            }
        }

        // Copy parsed stream information from context
        self.info = ASFInfo {
            length: context.info.length,
            sample_rate: context.info.sample_rate,
            bitrate: context.info.bitrate,
            channels: context.info.channels,
            codec_type: context.info.codec_type,
            codec_name: context.info.codec_name,
            codec_description: context.info.codec_description,
            max_bitrate: None,
            preroll: None,
            flags: None,
            file_size: None,
        };

        // Update with additional fields from FilePropertiesObject
        self.update_info_from_objects();

        // Copy tags from context
        self.tags = context.tags;

        Ok(())
    }

    /// Parse header size from file asynchronously
    #[cfg(feature = "async")]
    async fn parse_size_async(file: &mut TokioFile) -> Result<(u64, u32)> {
        let mut header = [0u8; 30];
        file.read_exact(&mut header).await?;

        if header[0..16] != ASFGUIDs::HEADER {
            return Err(AudexError::InvalidData("Not an ASF file".to_string()));
        }

        let size = u64::from_le_bytes([
            header[16], header[17], header[18], header[19], header[20], header[21], header[22],
            header[23],
        ]);

        let num_objects = u32::from_le_bytes([header[24], header[25], header[26], header[27]]);

        Ok((size, num_objects))
    }

    /// Save changes back to the file asynchronously
    ///
    /// This method provides the same functionality as `save()` but uses async I/O
    /// for better performance in concurrent applications.
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        if let Some(path) = self.filename.clone() {
            self.save_with_options_async(Some(path), None).await
        } else {
            Err(AudexError::InvalidData(
                "No filename available for saving".to_string(),
            ))
        }
    }

    /// Save with advanced options asynchronously
    #[cfg(feature = "async")]
    pub async fn save_with_options_async<P>(
        &mut self,
        path: Option<P>,
        padding_func: Option<fn(i64) -> i64>,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        self.save_internal_async(path, padding_func, true).await
    }

    /// Internal async save implementation with vendor tag option
    #[cfg(feature = "async")]
    async fn save_internal_async<P>(
        &mut self,
        path: Option<P>,
        padding_func: Option<fn(i64) -> i64>,
        add_vendor_tag: bool,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        let file_path = match path {
            Some(p) => p.as_ref().to_path_buf(),
            None => self.filename.clone().ok_or_else(|| {
                AudexError::InvalidData("No filename available for saving".to_string())
            })?,
        };

        // Add Audex vendor information if requested
        if add_vendor_tag {
            self.add_tags()?;
            let audex_info = format!("Audex {}", crate::VERSION_STRING);
            self.tags.set(
                "WM/EncodingSettings".to_string(),
                vec![ASFAttribute::unicode(audex_info)],
            );
        }

        // Distribute tags to appropriate ASF objects
        self.distribute_tags()?;

        // Ensure required objects exist in header
        self.ensure_required_objects()?;

        // Parse current header size and file size
        let (old_header_size, total_file_size) = {
            let mut file = TokioFile::open(&file_path).await?;
            let (size, _) = Self::parse_size_async(&mut file).await?;
            let fsize = file.seek(SeekFrom::End(0)).await?;
            (size, fsize)
        };

        // Create context for rendering header
        let context = super::objects::ASFContext {
            info: super::objects::ASFInfo {
                length: self.info.length,
                sample_rate: self.info.sample_rate,
                bitrate: self.info.bitrate,
                channels: self.info.channels,
                codec_type: self.info.codec_type.clone(),
                codec_name: self.info.codec_name.clone(),
                codec_description: self.info.codec_description.clone(),
            },
            tags: self.tags.clone(),
            parse_limits: crate::limits::ParseLimits::default(),
            nesting_depth: 0,
        };

        // Create header object from stored objects
        let mut header = super::objects::HeaderObject::new();
        for obj in &self.header_objects {
            header.objects.push(obj.clone_boxed());
        }

        // Render header with padding
        let header_data = header.render_full(
            &context,
            old_header_size,
            padding_func.map(|f| f as fn(i64) -> i64),
            total_file_size,
        )?;

        // Resize file if header size changed
        let new_header_size = header_data.len() as u64;
        if new_header_size != old_header_size {
            let mut file = TokioOpenOptions::new()
                .read(true)
                .write(true)
                .open(&file_path)
                .await?;
            resize_bytes_async(&mut file, old_header_size, new_header_size, 0).await?;
        }

        // Write new header data to file
        let mut file = TokioOpenOptions::new().write(true).open(&file_path).await?;
        file.write_all(&header_data).await?;
        file.flush().await?;

        // Update stored filename
        self.filename = Some(file_path);

        Ok(())
    }

    /// Delete all tags from file asynchronously
    ///
    /// Clears all metadata and saves with minimal padding.
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        self.tags.clear();
        self._tags.clear();
        self.save_internal_async(None::<PathBuf>, Some(|_| 0), false)
            .await
    }

    /// Delete the file from disk asynchronously
    #[cfg(feature = "async")]
    pub async fn delete_async(&mut self) -> Result<()> {
        if let Some(path) = &self.filename {
            tokio::fs::remove_file(path).await?;
            self.filename = None;
            Ok(())
        } else {
            Err(AudexError::InvalidData(
                "No filename available for deletion".to_string(),
            ))
        }
    }

    /// Get MIME types for ASF files
    pub fn mime_types() -> &'static [&'static str] {
        &[
            "audio/x-ms-wma",
            "audio/x-ms-wmv",
            "video/x-ms-asf",
            "audio/x-wma",
            "video/x-wmv",
        ]
    }

    /// Pretty print the ASF info
    pub fn pprint(&self) -> String {
        self.info.pprint()
    }

    pub fn get(&self, key: &str) -> Vec<&ASFAttribute> {
        self.tags.get(key)
    }

    pub fn remove(&mut self, key: &str) -> Result<()> {
        self.tags.remove(key);
        Ok(())
    }

    /// Values are always stored as Unicode attributes.
    /// Use `set_typed` if you need a specific ASF attribute type.
    pub fn set(&mut self, key: &str, values: Vec<String>) {
        self.add_tags().ok();
        let attributes: Vec<ASFAttribute> = values.into_iter().map(ASFAttribute::unicode).collect();
        self.tags.set(key.to_string(), attributes);
    }

    /// Value is always stored as a Unicode attribute.
    /// Use `set_typed` if you need a specific ASF attribute type.
    pub fn set_single(&mut self, key: &str, value: String) {
        self.add_tags().ok();
        let attribute = ASFAttribute::unicode(value);
        self.tags.set_single(key.to_string(), attribute);
    }

    /// Set values for a key with auto-detected types
    ///
    /// Unlike `set()`, this auto-detects the best ASF type from string values
    /// (e.g., numeric strings become Word/DWord/QWord).
    pub fn set_typed(&mut self, key: &str, values: Vec<String>) {
        self.add_tags().ok();
        let attributes: Vec<ASFAttribute> = values
            .into_iter()
            .map(|v| super::attrs::asf_value_from_string(&v))
            .collect();
        self.tags.set(key.to_string(), attributes);
    }

    /// Set binary data for a key (for cover art etc.)
    pub fn set_binary(&mut self, key: &str, data: Vec<u8>) {
        self.add_tags().ok();
        let attribute = ASFAttribute::byte_array(data);
        self.tags.set_single(key.to_string(), attribute);
    }
}

impl FileType for ASF {
    type Tags = ASFTags;
    type Info = ASFInfo;

    fn format_id() -> &'static str {
        "ASF"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load(path)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing ASF file from reader");
        // Validate ASF header signature
        let mut header_guid = [0u8; 16];
        reader.read_exact(&mut header_guid)?;
        if header_guid != ASFGUIDs::HEADER {
            return Err(AudexError::InvalidData(
                "Invalid ASF header GUID".to_string(),
            ));
        }

        // Reset position for full parsing
        reader.seek(SeekFrom::Start(0))?;

        let mut asf = Self::new();
        let mut reader = reader;

        // Parse ASF header and get all objects
        let mut context = super::objects::ASFContext::default();

        match super::objects::HeaderObject::parse_full(&mut reader, &mut context) {
            Ok(header) => {
                asf.header_objects = header.objects;

                asf.info = ASFInfo {
                    length: context.info.length,
                    sample_rate: context.info.sample_rate,
                    bitrate: context.info.bitrate,
                    channels: context.info.channels,
                    codec_type: context.info.codec_type,
                    codec_name: context.info.codec_name,
                    codec_description: context.info.codec_description,
                    max_bitrate: None,
                    preroll: None,
                    flags: None,
                    file_size: None,
                };

                asf.update_info_from_objects();
                asf.tags = context.tags;
            }
            Err(e) => {
                // Propagate parse errors so callers can distinguish
                // "no metadata" from "corrupted file"
                return Err(e);
            }
        }

        // Store original header size for padding calculations
        reader.seek(std::io::SeekFrom::Start(0))?;
        let (header_size, _) = super::objects::HeaderObject::parse_size(&mut reader)?;
        asf.original_header_size = Some(header_size);

        Ok(asf)
    }

    fn save(&mut self) -> Result<()> {
        self.save()
    }

    fn clear(&mut self) -> Result<()> {
        self.clear()
    }

    fn save_to_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        self.save_to_writer_inner(writer, None, true)
    }

    fn clear_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        self.clear_writer_inner(writer)
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        self.save_with_options(Some(path), None)
    }

    /// ASF tags are always present in this format.
    ///
    /// The trait-level `add_tags` returns an error since tags already exist.
    /// Note: the inherent method `ASF::add_tags()` returns `Ok(())` for API
    /// compatibility; this trait method is only reached via `FileType::add_tags(&mut asf)`.
    ///
    /// # Errors
    ///
    /// Always returns `AudexError::InvalidOperation`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::asf::ASF;
    /// use audex::FileType;
    ///
    /// let mut asf = ASF::load("file.wma")?;
    /// // Inherent method returns Ok (tags already present)
    /// assert!(asf.add_tags().is_ok());
    /// // Trait method returns Err
    /// assert!(FileType::add_tags(&mut asf).is_err());
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        // ASF tags are always present, cannot add what already exists
        Err(AudexError::InvalidOperation(
            "ASF tags already exist".to_string(),
        ))
    }

    fn get(&self, key: &str) -> Option<Vec<String>> {
        // ASFTags stores ASFAttribute, need to convert to Vec<String>
        let attributes = self.tags.get(key);
        if attributes.is_empty() {
            None
        } else {
            Some(attributes.iter().map(|attr| attr.to_string()).collect())
        }
    }

    fn tags(&self) -> Option<&Self::Tags> {
        Some(&self.tags)
    }

    fn tags_mut(&mut self) -> Option<&mut Self::Tags> {
        Some(&mut self.tags)
    }

    fn info(&self) -> &Self::Info {
        &self.info
    }

    fn score(filename: &str, header: &[u8]) -> i32 {
        let mut score = 0;

        // Check header for ASF GUID - score based on presence
        if header.len() >= 16 && header[0..16] == ASFGUIDs::HEADER {
            score += 2; // ASF header detected
        }

        // File extension scoring for additional confidence
        let filename_lower = filename.to_lowercase();
        if filename_lower.ends_with(".wma")
            || filename_lower.ends_with(".asf")
            || filename_lower.ends_with(".wmv")
        {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        Self::mime_types()
    }
}

impl Default for ASF {
    fn default() -> Self {
        Self::new()
    }
}
