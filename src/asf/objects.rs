//! ASF object implementations/asf
//!
//! Complete implementation of all ASF objects with context-aware parsing/rendering,
//! object registration system, and comprehensive error handling.

use super::attrs::{
    ASFAttribute, ASFAttributeType, ASFTags, CONTENT_DESCRIPTION_NAMES, parse_attribute,
};
use super::util::{ASFCodecs, ASFError, ASFGUIDs, ASFUtil};
use crate::tags::PaddingInfo;
use crate::{AudexError, Result};
use std::collections::HashMap;
use std::fmt;
use std::io::{Read, Seek, SeekFrom};

/// Type alias for ASF object constructor function
type ASFObjectConstructor = fn() -> Box<dyn ASFObject>;

// Macro to add as_any methods to ASFObject implementations
macro_rules! impl_as_any {
    () => {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    };
}

/// Shared state passed between ASF objects during parsing and rendering.
///
/// Objects populate `info` (stream properties) and `tags` (metadata) as
/// they are parsed. The context is then used to construct the final
/// [`ASF`](super::ASF) instance.
#[derive(Debug)]
pub struct ASFContext {
    /// Accumulated stream information from parsed objects
    pub info: ASFInfo,
    /// Accumulated metadata tags from Content Description and Extended objects
    pub tags: ASFTags,
    /// Parse limits captured at the start of the current operation.
    pub parse_limits: crate::limits::ParseLimits,
    /// Current nesting depth for recursive object parsing (e.g. header extensions).
    /// Used to prevent stack overflow from deeply nested or malformed files.
    pub nesting_depth: u16,
}

impl Default for ASFContext {
    fn default() -> Self {
        Self {
            info: ASFInfo::default(),
            tags: ASFTags::new(),
            parse_limits: crate::limits::ParseLimits::default(),
            nesting_depth: 0,
        }
    }
}

/// ASF stream information
#[derive(Debug, Default, Clone)]
pub struct ASFInfo {
    pub length: f64,
    pub sample_rate: u32,
    pub bitrate: u32,
    pub channels: u16,
    pub codec_type: String,
    pub codec_name: String,
    pub codec_description: String,
}

/// Trait implemented by each ASF object type (Header, FileProperties, etc.)
///
/// Each object knows its GUID, can parse itself from raw bytes (populating
/// the shared [`ASFContext`]), and can render itself back to bytes for saving.
pub trait ASFObject: std::fmt::Debug + Send + Sync {
    /// Static GUID identifying this object type (used for registration)
    fn guid() -> [u8; 16]
    where
        Self: Sized;

    /// Instance GUID (same as `guid()` but callable on trait objects)
    fn get_guid(&self) -> [u8; 16];

    /// Parse this object's payload from raw bytes, updating the shared context
    fn parse(&mut self, context: &mut ASFContext, data: &[u8]) -> Result<()>;

    /// Render object to bytes with ASF context (without GUID and size header)
    fn render(&self, context: &ASFContext) -> Result<Vec<u8>>;

    /// Get child objects (if any)
    fn children(&self) -> Vec<&dyn ASFObject> {
        Vec::new()
    }

    /// Get child objects mutably (if any) - simplified to avoid lifetime issues
    fn has_children(&self) -> bool {
        false
    }

    /// Find child object by GUID
    fn find_child(&self, _guid: &[u8; 16]) -> Option<&dyn ASFObject> {
        None
    }

    /// Find child object by GUID mutably - simplified to avoid lifetime issues
    fn has_child(&self, _guid: &[u8; 16]) -> bool {
        false
    }

    /// Pretty print for debugging
    fn pprint(&self) -> String {
        format!(
            "{}({})",
            std::any::type_name::<Self>()
                .split("::")
                .last()
                .unwrap_or("Unknown"),
            ASFUtil::bytes_to_guid(&self.get_guid())
        )
    }

    /// Clone the object as a boxed trait object
    fn clone_boxed(&self) -> Box<dyn ASFObject>;

    /// Get as Any for downcasting
    fn as_any(&self) -> &dyn std::any::Any;

    /// Get as Any mutable for downcasting
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// Object type registry - extensible factory system
#[derive(Debug)]
pub struct ObjectRegistry {
    types: HashMap<[u8; 16], ASFObjectConstructor>,
}

impl Default for ObjectRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            types: HashMap::new(),
        };
        registry.register_default_types();
        registry
    }

    /// Register an object type by GUID
    pub fn register<T>(&mut self)
    where
        T: ASFObject + Default + 'static,
    {
        self.types.insert(T::guid(), || Box::new(T::default()));
    }

    /// Create object by GUID
    pub fn create(&self, guid: [u8; 16]) -> Box<dyn ASFObject> {
        if let Some(factory) = self.types.get(&guid) {
            factory()
        } else {
            Box::new(UnknownObject::new(guid))
        }
    }

    /// Register all default ASF object types
    fn register_default_types(&mut self) {
        self.register::<HeaderObject>();
        self.register::<ContentDescriptionObject>();
        self.register::<ExtendedContentDescriptionObject>();
        self.register::<FilePropertiesObject>();
        self.register::<StreamPropertiesObject>();
        self.register::<CodecListObject>();
        self.register::<PaddingObject>();
        self.register::<StreamBitratePropertiesObject>();
        self.register::<ContentEncryptionObject>();
        self.register::<ExtendedContentEncryptionObject>();
        self.register::<HeaderExtensionObject>();
        self.register::<MetadataObject>();
        self.register::<MetadataLibraryObject>();
        self.register::<DigitalSignatureObject>();
        self.register::<ExtendedStreamPropertiesObject>();
        self.register::<BitrateMutualExclusionObject>();
    }
}

static GLOBAL_REGISTRY: std::sync::LazyLock<ObjectRegistry> =
    std::sync::LazyLock::new(ObjectRegistry::new);

/// Create object by GUID using global registry
pub fn create_object_by_guid(guid: [u8; 16]) -> Box<dyn ASFObject> {
    GLOBAL_REGISTRY.create(guid)
}

/// Unknown ASF object for unrecognized GUIDs
#[derive(Debug, Clone)]
pub struct UnknownObject {
    pub guid: [u8; 16],
    pub data: Vec<u8>,
}

impl UnknownObject {
    pub fn new(guid: [u8; 16]) -> Self {
        Self {
            guid,
            data: Vec::new(),
        }
    }
}

impl Default for UnknownObject {
    fn default() -> Self {
        Self::new([0; 16])
    }
}

impl ASFObject for UnknownObject {
    fn guid() -> [u8; 16] {
        [0; 16] // Placeholder, actual GUID is stored in instance
    }

    fn get_guid(&self) -> [u8; 16] {
        self.guid
    }

    fn parse(&mut self, _context: &mut ASFContext, data: &[u8]) -> Result<()> {
        _context
            .parse_limits
            .check_tag_size(data.len() as u64, "ASF unknown object")?;
        self.data = data.to_vec();
        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        Ok(self.data.clone())
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Header Object
#[derive(Debug, Default)]
pub struct HeaderObject {
    pub objects: Vec<Box<dyn ASFObject>>,
}

impl HeaderObject {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse full header from reader
    pub fn parse_full<R: Read + Seek>(reader: &mut R, context: &mut ASFContext) -> Result<Self> {
        let limits = context.parse_limits;

        // Read header signature and validate
        let mut header_data = [0u8; 30];
        reader
            .read_exact(&mut header_data)
            .map_err(|e| ASFError::InvalidHeader(format!("Failed to read header: {}", e)))?;

        if header_data[0..16] != ASFGUIDs::HEADER {
            error_event!("ASF header GUID mismatch — not an ASF file");
            return Err(ASFError::InvalidHeader("Not an ASF file".to_string()).into());
        }

        let total_size = ASFUtil::parse_u64_le(&header_data[16..24])
            .map_err(|e| ASFError::InvalidHeader(format!("Invalid header size: {}", e)))?;
        let num_objects = ASFUtil::parse_u32_le(&header_data[24..28])
            .map_err(|e| ASFError::InvalidHeader(format!("Invalid object count: {}", e)))?;

        // Cross-validate the claimed header size against the actual stream.
        // A crafted file could claim a multi-GB header in a tiny file,
        // causing downstream allocations proportional to the spoofed size.
        let stream_end = reader
            .seek(SeekFrom::End(0))
            .map_err(|e| ASFError::InvalidHeader(format!("Failed to seek: {}", e)))?;
        if total_size > stream_end {
            return Err(ASFError::InvalidHeader(format!(
                "ASF header size ({} bytes) exceeds file size ({} bytes)",
                total_size, stream_end
            ))
            .into());
        }
        // Restore position to continue reading objects
        reader
            .seek(SeekFrom::Start(30))
            .map_err(|e| ASFError::InvalidHeader(format!("Failed to seek: {}", e)))?;

        let mut remaining_size = total_size.saturating_sub(30);
        let mut objects = Vec::new();

        for i in 0..num_objects {
            if remaining_size < 24 {
                return Err(ASFError::InvalidHeader(format!(
                    "Invalid header size at object {}",
                    i
                ))
                .into());
            }

            // Read object header
            let mut obj_header = [0u8; 24];
            reader
                .read_exact(&mut obj_header)
                .map_err(|_e| ASFError::Truncated)?;
            remaining_size = remaining_size.saturating_sub(24);

            let guid = ASFUtil::parse_guid(&obj_header[0..16])
                .map_err(|e| ASFError::InvalidHeader(format!("Invalid object GUID: {}", e)))?;
            let obj_size = ASFUtil::parse_u64_le(&obj_header[16..24])
                .map_err(|e| ASFError::InvalidHeader(format!("Invalid object size: {}", e)))?;

            if obj_size < 24 {
                warn_event!(
                    object_index = i,
                    size = obj_size,
                    "ASF object size too small"
                );
                return Err(ASFError::InvalidHeader("Object size too small".to_string()).into());
            }

            let payload_size = obj_size - 24;

            if remaining_size < payload_size {
                return Err(ASFError::InvalidHeader("Invalid object size".to_string()).into());
            }

            // Enforce the library-wide tag allocation ceiling
            limits.check_tag_size(payload_size, "ASF object")?;

            // Read object payload (use try_from to prevent silent truncation
            // on 32-bit targets where payload_size could exceed usize::MAX)
            let payload_usize = usize::try_from(payload_size).map_err(|_| {
                ASFError::InvalidHeader(format!(
                    "ASF object payload {} bytes exceeds addressable range",
                    payload_size
                ))
            })?;
            let mut payload = vec![0u8; payload_usize];
            reader
                .read_exact(&mut payload)
                .map_err(|_| ASFError::Truncated)?;
            remaining_size = remaining_size.saturating_sub(payload_size);

            // Create appropriate object based on GUID
            let mut obj = create_object_by_guid(guid);

            // Parse with error handling
            match obj.parse(context, &payload) {
                Ok(()) => objects.push(obj),
                Err(_e) => {
                    // Create unknown object as fallback for malformed data
                    let mut unknown = UnknownObject::new(guid);
                    unknown.parse(context, &payload)?;
                    objects.push(Box::new(unknown));
                }
            }
        }

        Ok(Self { objects })
    }

    /// Determine header size needed for objects
    pub fn parse_size<R: Read>(reader: &mut R) -> Result<(u64, u32)> {
        let mut header = [0u8; 30];
        reader
            .read_exact(&mut header)
            .map_err(|_| ASFError::Truncated)?;

        if header[0..16] != ASFGUIDs::HEADER {
            return Err(ASFError::InvalidHeader("Not an ASF file".to_string()).into());
        }

        let size = ASFUtil::parse_u64_le(&header[16..24])?;
        let num_objects = ASFUtil::parse_u32_le(&header[24..28])?;

        Ok((size, num_objects))
    }

    /// Render full header with smart padding management
    pub fn render_full(
        &self,
        context: &ASFContext,
        available_size: u64,
        padding_func: Option<fn(i64) -> i64>,
        file_size: u64,
    ) -> Result<Vec<u8>> {
        // Render all objects except padding
        let mut rendered_objects = Vec::new();
        let mut num_objects: u32 = 0;

        for obj in &self.objects {
            // Skip padding objects - we'll add them strategically
            if obj.get_guid() == ASFGUIDs::PADDING {
                continue;
            }

            let rendered = obj.render(context)?;
            rendered_objects.push((obj.get_guid(), rendered));
            num_objects += 1;
        }

        // Calculate minimum needed size
        let header_overhead = 30u64; // Header object size
        let mut needed_size = header_overhead;

        for (_, rendered) in &rendered_objects {
            needed_size += 24 + rendered.len() as u64; // Object header + payload
        }

        // Add padding object if needed
        let _padding_obj = PaddingObject::new();
        let padding_overhead = 24; // Padding object header
        needed_size += padding_overhead;

        // Cap available_size to prevent absurd padding from malformed files
        // that claim a huge header size. 100MB is the same limit used for objects.
        let capped_available = available_size.min(100_000_000);
        // Use checked subtraction to guard against overflow when
        // needed_size exceeds capped_available with extreme inputs.
        let capped_available_i64 = i64::try_from(capped_available).unwrap_or(i64::MAX);
        let needed_size_i64 = i64::try_from(needed_size).unwrap_or(i64::MAX);
        let available_space = capped_available_i64
            .checked_sub(needed_size_i64)
            .unwrap_or(i64::MIN);
        let padding = if let Some(func) = padding_func {
            func(available_space).max(0) as u64
        } else {
            // Use PaddingInfo for smart padding calculation.
            // Guard against underflow when file_size < capped_available
            // (can happen with malformed headers that claim a larger header
            // than the actual file size).
            let content_size = file_size
                .checked_sub(capped_available)
                .map(|v| v as i64)
                .unwrap_or(0);
            let info = PaddingInfo::new(available_space, content_size);
            info.get_default_padding().max(0) as u64
        };

        // Cap padding to a reasonable maximum (100 MB) to prevent
        // out-of-memory conditions from a misbehaving callback
        const MAX_PADDING_BYTES: u64 = 100 * 1024 * 1024;
        let padding = padding.min(MAX_PADDING_BYTES);

        // Always add PaddingObject (even with 0 bytes of padding data),
        let padding_data = vec![0u8; padding as usize];
        rendered_objects.push((ASFGUIDs::PADDING, padding_data));
        needed_size += padding;
        num_objects += 1;

        // Build final header
        let mut data = Vec::new();

        // Header object header
        data.extend_from_slice(&ASFGUIDs::HEADER);
        data.extend_from_slice(&needed_size.to_le_bytes());
        data.extend_from_slice(&num_objects.to_le_bytes());
        data.extend_from_slice(&[0x01, 0x02]); // Reserved bytes

        // Write all objects
        for (guid, rendered) in rendered_objects {
            let obj_size = 24 + rendered.len() as u64;
            data.extend_from_slice(&guid);
            data.extend_from_slice(&obj_size.to_le_bytes());
            data.extend_from_slice(&rendered);
        }

        Ok(data)
    }

    /// Get child object by GUID
    pub fn get_child(&self, guid: &[u8; 16]) -> Option<&dyn ASFObject> {
        self.objects
            .iter()
            .find(|obj| &obj.get_guid() == guid)
            .map(|obj| obj.as_ref())
    }

    /// Get child object by GUID mutably
    pub fn get_child_mut(&mut self, guid: &[u8; 16]) -> Option<&mut Box<dyn ASFObject>> {
        self.objects.iter_mut().find(|obj| &obj.get_guid() == guid)
    }
}

impl ASFObject for HeaderObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::HEADER
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, _context: &mut ASFContext, _data: &[u8]) -> Result<()> {
        // Header object parsing is handled by parse_full
        Err(AudexError::InvalidData(
            "Use parse_full for HeaderObject".to_string(),
        ))
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        // Header object rendering is handled by render_full
        Err(AudexError::InvalidData(
            "Use render_full for HeaderObject".to_string(),
        ))
    }

    fn children(&self) -> Vec<&dyn ASFObject> {
        self.objects.iter().map(|obj| obj.as_ref()).collect()
    }

    fn has_children(&self) -> bool {
        !self.objects.is_empty()
    }

    fn find_child(&self, guid: &[u8; 16]) -> Option<&dyn ASFObject> {
        self.get_child(guid)
    }

    fn has_child(&self, guid: &[u8; 16]) -> bool {
        self.get_child(guid).is_some()
    }

    fn pprint(&self) -> String {
        let mut lines = vec![format!(
            "HeaderObject({})",
            ASFUtil::bytes_to_guid(&self.get_guid())
        )];
        for obj in &self.objects {
            for line in obj.pprint().lines() {
                lines.push(format!("  {}", line));
            }
        }
        lines.join("\n")
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        let mut cloned = HeaderObject::new();
        for obj in &self.objects {
            cloned.objects.push(obj.clone_boxed());
        }
        Box::new(cloned)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// Content Description Object
#[derive(Debug, Default, Clone)]
pub struct ContentDescriptionObject {
    pub title: Option<String>,
    pub author: Option<String>,
    pub copyright: Option<String>,
    pub description: Option<String>,
    pub rating: Option<String>,
}

impl ContentDescriptionObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for ContentDescriptionObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::CONTENT_DESCRIPTION
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, context: &mut ASFContext, data: &[u8]) -> Result<()> {
        if data.len() < 10 {
            return Err(ASFError::InvalidData("Content description too short".to_string()).into());
        }

        // Parse field lengths
        let lengths = [
            ASFUtil::parse_u16_le(&data[0..2])? as usize,
            ASFUtil::parse_u16_le(&data[2..4])? as usize,
            ASFUtil::parse_u16_le(&data[4..6])? as usize,
            ASFUtil::parse_u16_le(&data[6..8])? as usize,
            ASFUtil::parse_u16_le(&data[8..10])? as usize,
        ];

        // Validate that the sum of all field lengths fits within the available data.
        // Each length was parsed as u16, so the sum fits in usize without overflow.
        let total_field_bytes: usize = lengths.iter().sum();
        if total_field_bytes + 10 > data.len() {
            return Err(ASFError::InvalidData(
                "Content description field lengths exceed available data".to_string(),
            )
            .into());
        }

        let mut pos = 10;
        let mut values = [None, None, None, None, None];

        for (i, &length) in lengths.iter().enumerate() {
            if length > 0 {
                if pos + length > data.len() {
                    return Err(ASFError::InvalidData(format!("Invalid field {} length", i)).into());
                }

                let text = ASFUtil::parse_utf16_le(&data[pos..pos + length])?;
                if !text.is_empty() {
                    values[i] = Some(text);
                }
                pos += length;
            }
        }

        self.title = values[0].clone();
        self.author = values[1].clone();
        self.copyright = values[2].clone();
        self.description = values[3].clone();
        self.rating = values[4].clone();

        // Add to context tags
        let field_values = [
            &self.title,
            &self.author,
            &self.copyright,
            &self.description,
            &self.rating,
        ];
        for (i, value) in field_values.iter().enumerate() {
            if let Some(val) = value {
                let attr = ASFAttribute::unicode(val.clone());
                context
                    .tags
                    .add(CONTENT_DESCRIPTION_NAMES[i].to_string(), attr);
            }
        }

        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let texts = [
            self.title
                .as_ref()
                .map(|s| ASFUtil::encode_utf16_le(s))
                .unwrap_or_default(),
            self.author
                .as_ref()
                .map(|s| ASFUtil::encode_utf16_le(s))
                .unwrap_or_default(),
            self.copyright
                .as_ref()
                .map(|s| ASFUtil::encode_utf16_le(s))
                .unwrap_or_default(),
            self.description
                .as_ref()
                .map(|s| ASFUtil::encode_utf16_le(s))
                .unwrap_or_default(),
            self.rating
                .as_ref()
                .map(|s| ASFUtil::encode_utf16_le(s))
                .unwrap_or_default(),
        ];

        let mut data = Vec::new();

        // Write lengths — the ASF spec uses u16 for these fields
        for text in &texts {
            let len = u16::try_from(text.len()).map_err(|_| {
                crate::AudexError::InvalidData(format!(
                    "ASF content description field too long ({} bytes, max {})",
                    text.len(),
                    u16::MAX
                ))
            })?;
            data.extend_from_slice(&len.to_le_bytes());
        }

        // Write text data
        for text in &texts {
            data.extend_from_slice(text);
        }

        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Extended Content Description Object
#[derive(Debug, Default, Clone)]
pub struct ExtendedContentDescriptionObject {
    pub attributes: Vec<(String, ASFAttribute)>,
}

impl ExtendedContentDescriptionObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for ExtendedContentDescriptionObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::EXTENDED_CONTENT_DESCRIPTION
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, context: &mut ASFContext, data: &[u8]) -> Result<()> {
        if data.len() < 2 {
            return Err(ASFError::InvalidData(
                "Extended content description too short".to_string(),
            )
            .into());
        }

        let num_attributes = ASFUtil::parse_u16_le(&data[0..2])? as usize;
        let mut pos = 2;

        for i in 0..num_attributes {
            if pos + 2 > data.len() {
                return Err(
                    ASFError::InvalidData(format!("Invalid attribute {} name length", i)).into(),
                );
            }

            // Parse name
            let name_len = ASFUtil::parse_u16_le(&data[pos..pos + 2])? as usize;
            pos += 2;

            if pos + name_len > data.len() {
                return Err(
                    ASFError::InvalidData(format!("Invalid attribute {} name data", i)).into(),
                );
            }

            let name = ASFUtil::parse_utf16_le(&data[pos..pos + name_len])?;
            pos += name_len;

            // Parse value type and length
            if pos + 4 > data.len() {
                return Err(
                    ASFError::InvalidData(format!("Invalid attribute {} header", i)).into(),
                );
            }

            let attr_type =
                ASFAttributeType::try_from(ASFUtil::parse_u16_le(&data[pos..pos + 2])?)?;
            pos += 2;
            let value_len = ASFUtil::parse_u16_le(&data[pos..pos + 2])? as usize;
            pos += 2;

            if pos + value_len > data.len() {
                return Err(
                    ASFError::InvalidData(format!("Invalid attribute {} value data", i)).into(),
                );
            }

            // Parse value
            let attr = parse_attribute(attr_type as u16, &data[pos..pos + value_len], true)?;
            pos += value_len;
            self.attributes.push((name.clone(), attr.clone()));
            context.tags.add(name, attr);
        }

        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        let attr_count = u16::try_from(self.attributes.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF attribute count {} exceeds u16 maximum",
                self.attributes.len()
            ))
        })?;
        data.extend_from_slice(&attr_count.to_le_bytes());

        for (name, attr) in &self.attributes {
            data.extend_from_slice(&attr.render(name)?);
        }

        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// File Properties Object
#[derive(Debug, Default, Clone)]
pub struct FilePropertiesObject {
    pub file_id: [u8; 16],
    pub file_size: u64,
    pub creation_date: u64,
    pub data_packets_count: u64,
    pub play_duration: u64,
    pub send_duration: u64,
    pub preroll: u64,
    pub flags: u32,
    pub min_data_packet_size: u32,
    pub max_data_packet_size: u32,
    pub max_bitrate: u32,
    /// Any trailing bytes beyond the standard 80-byte payload.
    /// Preserved on round-trip to avoid data loss.
    pub trailing_data: Vec<u8>,
}

impl FilePropertiesObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for FilePropertiesObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::FILE_PROPERTIES
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, context: &mut ASFContext, data: &[u8]) -> Result<()> {
        // Minimum size is 80 bytes to read up to max_bitrate
        if data.len() < 80 {
            return Err(
                ASFError::InvalidData("File properties object too short".to_string()).into(),
            );
        }

        // Parse all fields according to ASF specification
        self.file_id.copy_from_slice(&data[0..16]);
        self.file_size = ASFUtil::parse_u64_le(&data[16..24])?;
        self.creation_date = ASFUtil::parse_u64_le(&data[24..32])?;
        self.data_packets_count = ASFUtil::parse_u64_le(&data[32..40])?;
        self.play_duration = ASFUtil::parse_u64_le(&data[40..48])?;
        self.send_duration = ASFUtil::parse_u64_le(&data[48..56])?;
        self.preroll = ASFUtil::parse_u64_le(&data[56..64])?;
        self.flags = ASFUtil::parse_u32_le(&data[64..68])?;
        self.min_data_packet_size = ASFUtil::parse_u32_le(&data[68..72])?;
        self.max_data_packet_size = ASFUtil::parse_u32_le(&data[72..76])?;
        self.max_bitrate = ASFUtil::parse_u32_le(&data[76..80])?;

        // Preserve any trailing bytes beyond the standard 80-byte payload
        if data.len() > 80 {
            self.trailing_data = data[80..].to_vec();
        }

        // Update context info - calculate length with preroll compensation
        let play_duration_sec = self.play_duration as f64 / 10_000_000.0;
        let preroll_sec = self.preroll as f64 / 1000.0;
        context.info.length = (play_duration_sec - preroll_sec).max(0.0);

        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(104);
        data.extend_from_slice(&self.file_id);
        data.extend_from_slice(&self.file_size.to_le_bytes());
        data.extend_from_slice(&self.creation_date.to_le_bytes());
        data.extend_from_slice(&self.data_packets_count.to_le_bytes());
        data.extend_from_slice(&self.play_duration.to_le_bytes());
        data.extend_from_slice(&self.send_duration.to_le_bytes());
        data.extend_from_slice(&self.preroll.to_le_bytes());
        data.extend_from_slice(&self.flags.to_le_bytes());
        data.extend_from_slice(&self.min_data_packet_size.to_le_bytes());
        data.extend_from_slice(&self.max_data_packet_size.to_le_bytes());
        data.extend_from_slice(&self.max_bitrate.to_le_bytes());

        // Re-emit any trailing bytes that were present in the original
        if !self.trailing_data.is_empty() {
            data.extend_from_slice(&self.trailing_data);
        }

        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Stream Properties Object
#[derive(Debug, Default, Clone)]
pub struct StreamPropertiesObject {
    pub stream_type: [u8; 16],
    pub error_correction_type: [u8; 16],
    pub time_offset: u64,
    pub type_specific_data_length: u32,
    pub error_correction_data_length: u32,
    pub flags: u16,
    pub reserved: u32,
    pub type_specific_data: Vec<u8>,
    pub error_correction_data: Vec<u8>,
}

impl StreamPropertiesObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for StreamPropertiesObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::STREAM_PROPERTIES
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, context: &mut ASFContext, data: &[u8]) -> Result<()> {
        if data.len() < 54 {
            return Err(
                ASFError::InvalidData("Stream properties object too short".to_string()).into(),
            );
        }

        self.stream_type.copy_from_slice(&data[0..16]);
        self.error_correction_type.copy_from_slice(&data[16..32]);
        self.time_offset = ASFUtil::parse_u64_le(&data[32..40])?;
        self.type_specific_data_length = ASFUtil::parse_u32_le(&data[40..44])?;
        self.error_correction_data_length = ASFUtil::parse_u32_le(&data[44..48])?;
        self.flags = ASFUtil::parse_u16_le(&data[48..50])?;
        self.reserved = ASFUtil::parse_u32_le(&data[50..54])?;

        let mut pos = 54;

        // Parse type specific data
        let type_data_len = self.type_specific_data_length as usize;
        if pos + type_data_len > data.len() {
            return Err(
                ASFError::InvalidData("Invalid type specific data length".to_string()).into(),
            );
        }
        self.type_specific_data = data[pos..pos + type_data_len].to_vec();
        pos += type_data_len;

        // Parse error correction data
        let error_data_len = self.error_correction_data_length as usize;
        if pos + error_data_len > data.len() {
            return Err(
                ASFError::InvalidData("Invalid error correction data length".to_string()).into(),
            );
        }
        self.error_correction_data = data[pos..pos + error_data_len].to_vec();

        // Extract audio stream properties from the type-specific data region.
        // Only applies to audio streams — video or other stream types
        // have different layouts. We validate against the parsed
        // type_specific_data length (not the raw buffer length) to avoid
        // reading error-correction bytes as audio format fields.
        if self.stream_type == ASFGUIDs::AUDIO_STREAM && self.type_specific_data.len() >= 12 {
            if let (Ok(channels), Ok(sample_rate), Ok(bitrate)) = (
                ASFUtil::parse_u16_le(&self.type_specific_data[2..4]),
                ASFUtil::parse_u32_le(&self.type_specific_data[4..8]),
                ASFUtil::parse_u32_le(&self.type_specific_data[8..12]),
            ) {
                context.info.channels = channels;
                context.info.sample_rate = sample_rate;
                // Convert bytes/sec to bits/sec using u64 intermediate to
                // prevent overflow for high bitrate streams
                context.info.bitrate = u32::try_from((bitrate as u64) * 8).unwrap_or(u32::MAX);
            }
        }

        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&self.stream_type);
        data.extend_from_slice(&self.error_correction_type);
        data.extend_from_slice(&self.time_offset.to_le_bytes());
        let type_data_size = u32::try_from(self.type_specific_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF type-specific data size {} exceeds u32 maximum",
                self.type_specific_data.len()
            ))
        })?;
        let ec_data_size = u32::try_from(self.error_correction_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF error correction data size {} exceeds u32 maximum",
                self.error_correction_data.len()
            ))
        })?;
        data.extend_from_slice(&type_data_size.to_le_bytes());
        data.extend_from_slice(&ec_data_size.to_le_bytes());
        data.extend_from_slice(&self.flags.to_le_bytes());
        data.extend_from_slice(&self.reserved.to_le_bytes());
        data.extend_from_slice(&self.type_specific_data);
        data.extend_from_slice(&self.error_correction_data);
        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Codec List Object
#[derive(Debug, Default, Clone)]
pub struct CodecListObject {
    pub codecs: Vec<CodecEntry>,
    /// Raw reserved field bytes for lossless round-trip
    pub raw_reserved: [u8; 16],
}

#[derive(Debug, Clone)]
pub struct CodecEntry {
    pub codec_type: u16,
    pub name: String,
    pub description: String,
    pub codec_info: String,
    /// Raw codec info bytes for lossless round-trip
    pub raw_codec_info: Vec<u8>,
}

impl CodecListObject {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse a single codec entry from data
    fn parse_entry(data: &[u8], offset: usize) -> Result<(usize, CodecEntry)> {
        let mut pos = offset;

        if pos + 2 > data.len() {
            return Err(ASFError::InvalidData("Codec entry too short".to_string()).into());
        }

        let codec_type = ASFUtil::parse_u16_le(&data[pos..pos + 2])?;
        pos += 2;

        // Parse name
        if pos + 2 > data.len() {
            return Err(ASFError::InvalidData("Codec name length missing".to_string()).into());
        }
        let name_units = ASFUtil::parse_u16_le(&data[pos..pos + 2])? as usize;
        pos += 2;

        // Use checked arithmetic for the UTF-16 unit-to-byte conversion
        let name_bytes = name_units.checked_mul(2).ok_or_else(|| {
            AudexError::from(ASFError::InvalidData(
                "Codec name byte length overflow".to_string(),
            ))
        })?;
        if pos + name_bytes > data.len() {
            return Err(ASFError::InvalidData("Codec name data truncated".to_string()).into());
        }

        let name = ASFUtil::parse_utf16_le(&data[pos..pos + name_bytes]).unwrap_or_default();
        pos += name_bytes;

        // Parse description
        if pos + 2 > data.len() {
            return Err(
                ASFError::InvalidData("Codec description length missing".to_string()).into(),
            );
        }
        let desc_units = ASFUtil::parse_u16_le(&data[pos..pos + 2])? as usize;
        pos += 2;

        // Use checked arithmetic for the UTF-16 unit-to-byte conversion
        let desc_bytes = desc_units.checked_mul(2).ok_or_else(|| {
            AudexError::from(ASFError::InvalidData(
                "Codec description byte length overflow".to_string(),
            ))
        })?;
        if pos + desc_bytes > data.len() {
            return Err(
                ASFError::InvalidData("Codec description data truncated".to_string()).into(),
            );
        }

        let description = ASFUtil::parse_utf16_le(&data[pos..pos + desc_bytes]).unwrap_or_default();
        pos += desc_bytes;

        // Parse codec info
        if pos + 2 > data.len() {
            return Err(ASFError::InvalidData("Codec info length missing".to_string()).into());
        }
        let info_bytes = ASFUtil::parse_u16_le(&data[pos..pos + 2])? as usize;
        pos += 2;

        if pos + info_bytes > data.len() {
            return Err(ASFError::InvalidData("Codec info data truncated".to_string()).into());
        }

        let raw_codec_info = data[pos..pos + info_bytes].to_vec();
        let mut codec_info = String::new();
        if info_bytes == 2 {
            let codec_id = ASFUtil::parse_u16_le(&data[pos..pos + 2])?;
            if let Some(codec_name) = ASFCodecs::get_codec_name(codec_id) {
                codec_info = codec_name.to_string();
            }
        }
        pos += info_bytes;

        Ok((
            pos,
            CodecEntry {
                codec_type,
                name,
                description,
                codec_info,
                raw_codec_info,
            },
        ))
    }
}

impl ASFObject for CodecListObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::CODEC_LIST
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, context: &mut ASFContext, data: &[u8]) -> Result<()> {
        // No real ASF file contains more than a handful of codecs. Cap the
        // iteration count to prevent crafted files from forcing millions of
        // allocations via an inflated count field.
        const MAX_CODEC_ENTRIES: usize = 1024;

        if data.len() < 20 {
            return Err(ASFError::InvalidData("Codec list object too short".to_string()).into());
        }

        // Preserve reserved field (16 bytes) and get count
        self.raw_reserved.copy_from_slice(&data[0..16]);
        let count = ASFUtil::parse_u32_le(&data[16..20])? as usize;
        let mut pos = 20;

        let limits = crate::limits::ParseLimits::default();
        let mut cumulative_bytes: u64 = 0;

        // Enforce a hard cap on the number of codec entries to prevent
        // excessive heap allocation from a crafted count field.
        let effective_count = count.min(MAX_CODEC_ENTRIES);

        for _i in 0..effective_count {
            match Self::parse_entry(data, pos) {
                Ok((new_pos, entry)) => {
                    // Track how many bytes of the data buffer have been consumed
                    let consumed = (new_pos - pos) as u64;
                    cumulative_bytes = cumulative_bytes.saturating_add(consumed);
                    if cumulative_bytes > limits.max_tag_size {
                        return Err(ASFError::InvalidData(format!(
                            "ASF codec list cumulative size ({} bytes) exceeds limit ({} bytes)",
                            cumulative_bytes, limits.max_tag_size
                        ))
                        .into());
                    }

                    pos = new_pos;

                    // Update context with first audio codec found
                    if entry.codec_type == 2 && context.info.codec_type.is_empty() {
                        context.info.codec_type = entry.codec_info.clone();
                        context.info.codec_name = entry.name.trim().to_string();
                        context.info.codec_description = entry.description.trim().to_string();
                    }

                    self.codecs.push(entry);
                }
                Err(_) => {
                    // Skip malformed entries rather than failing entirely
                    break;
                }
            }
        }

        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Reserved field (16 bytes) - preserve original
        data.extend_from_slice(&self.raw_reserved);

        // Codec count — validated to fit in u32
        let codec_count = u32::try_from(self.codecs.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF codec count {} exceeds u32 maximum",
                self.codecs.len()
            ))
        })?;
        data.extend_from_slice(&codec_count.to_le_bytes());

        // Codec entries
        for entry in &self.codecs {
            data.extend_from_slice(&entry.codec_type.to_le_bytes());

            // Name (length in UTF-16 code units including null terminator)
            let name_data = ASFUtil::encode_utf16_le(&entry.name);
            let name_units = u16::try_from(name_data.len() / 2).map_err(|_| {
                AudexError::InvalidData(format!(
                    "ASF codec name length {} exceeds u16 maximum",
                    name_data.len() / 2
                ))
            })?;
            data.extend_from_slice(&name_units.to_le_bytes());
            data.extend_from_slice(&name_data);

            // Description (empty string = 0 length, no data)
            if entry.description.is_empty() {
                data.extend_from_slice(&0u16.to_le_bytes());
            } else {
                let desc_data = ASFUtil::encode_utf16_le(&entry.description);
                let desc_units = u16::try_from(desc_data.len() / 2).map_err(|_| {
                    AudexError::InvalidData(format!(
                        "ASF codec description length {} exceeds u16 maximum",
                        desc_data.len() / 2
                    ))
                })?;
                data.extend_from_slice(&desc_units.to_le_bytes());
                data.extend_from_slice(&desc_data);
            }

            // Codec info (preserve original raw bytes)
            let info_len = u16::try_from(entry.raw_codec_info.len()).map_err(|_| {
                AudexError::InvalidData(format!(
                    "ASF codec info length {} exceeds u16 maximum",
                    entry.raw_codec_info.len()
                ))
            })?;
            data.extend_from_slice(&info_len.to_le_bytes());
            data.extend_from_slice(&entry.raw_codec_info);
        }

        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Padding Object
#[derive(Debug, Default, Clone)]
pub struct PaddingObject {
    pub data: Vec<u8>,
}

impl PaddingObject {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_size(size: usize) -> Self {
        Self {
            data: vec![0u8; size],
        }
    }
}

impl ASFObject for PaddingObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::PADDING
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, _context: &mut ASFContext, data: &[u8]) -> Result<()> {
        // Defense-in-depth: enforce the global tag size limit before allocating,
        // consistent with UnknownObject::parse. The caller typically checks too,
        // but this guard prevents uncapped allocations if this method is reused
        // from a different code path.
        crate::limits::ParseLimits::default()
            .check_tag_size(data.len() as u64, "ASF padding object")?;
        self.data = data.to_vec();
        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        Ok(self.data.clone())
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Stream Bitrate Properties Object
#[derive(Debug, Default, Clone)]
pub struct StreamBitratePropertiesObject {
    pub data: Vec<u8>,
}

impl StreamBitratePropertiesObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for StreamBitratePropertiesObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::STREAM_BITRATE_PROPERTIES
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, _context: &mut ASFContext, data: &[u8]) -> Result<()> {
        // Defense-in-depth: enforce the global tag size limit before allocating,
        // consistent with UnknownObject::parse and PaddingObject::parse.
        crate::limits::ParseLimits::default()
            .check_tag_size(data.len() as u64, "StreamBitratePropertiesObject::parse")?;
        self.data = data.to_vec();
        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        Ok(self.data.clone())
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Content Encryption Object
#[derive(Debug, Default, Clone)]
pub struct ContentEncryptionObject {
    pub data: Vec<u8>,
}

impl ContentEncryptionObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for ContentEncryptionObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::CONTENT_ENCRYPTION
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, _context: &mut ASFContext, data: &[u8]) -> Result<()> {
        // Defense-in-depth: enforce the global tag size limit before allocating,
        // consistent with UnknownObject::parse and PaddingObject::parse.
        crate::limits::ParseLimits::default()
            .check_tag_size(data.len() as u64, "ContentEncryptionObject::parse")?;
        self.data = data.to_vec();
        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        Ok(self.data.clone())
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Extended Content Encryption Object
#[derive(Debug, Default, Clone)]
pub struct ExtendedContentEncryptionObject {
    pub data: Vec<u8>,
}

impl ExtendedContentEncryptionObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for ExtendedContentEncryptionObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::EXTENDED_CONTENT_ENCRYPTION
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, _context: &mut ASFContext, data: &[u8]) -> Result<()> {
        // Defense-in-depth: enforce the global tag size limit before allocating,
        // consistent with UnknownObject::parse and PaddingObject::parse.
        crate::limits::ParseLimits::default()
            .check_tag_size(data.len() as u64, "ExtendedContentEncryptionObject::parse")?;
        self.data = data.to_vec();
        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        Ok(self.data.clone())
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Header Extension Object
#[derive(Debug, Default)]
pub struct HeaderExtensionObject {
    pub objects: Vec<Box<dyn ASFObject>>,
}

impl HeaderExtensionObject {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get child object by GUID
    pub fn get_child(&self, guid: &[u8; 16]) -> Option<&dyn ASFObject> {
        self.objects
            .iter()
            .find(|obj| &obj.get_guid() == guid)
            .map(|obj| obj.as_ref())
    }

    /// Get child object by GUID mutably
    pub fn get_child_mut(&mut self, guid: &[u8; 16]) -> Option<&mut Box<dyn ASFObject>> {
        self.objects.iter_mut().find(|obj| &obj.get_guid() == guid)
    }
}

impl ASFObject for HeaderExtensionObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::HEADER_EXTENSION
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, context: &mut ASFContext, data: &[u8]) -> Result<()> {
        // Guard against excessive nesting depth to prevent stack overflow
        // from malformed or adversarial files.
        const MAX_NESTING_DEPTH: u16 = 16;
        if context.nesting_depth >= MAX_NESTING_DEPTH {
            return Err(ASFError::InvalidHeader(
                "Maximum header extension nesting depth exceeded".to_string(),
            )
            .into());
        }
        context.nesting_depth += 1;

        if data.len() < 22 {
            context.nesting_depth -= 1;
            return Err(
                ASFError::InvalidData("Header extension object too short".to_string()).into(),
            );
        }

        // Skip reserved GUID (16 bytes) and size (2 bytes)
        let data_size = ASFUtil::parse_u32_le(&data[18..22])? as usize;
        let mut pos = 0;
        let ext_data = &data[22..];

        if ext_data.len() < data_size {
            context.nesting_depth -= 1;
            return Err(
                ASFError::InvalidData("Header extension data truncated".to_string()).into(),
            );
        }

        // Parse nested objects
        while pos < data_size {
            if pos + 24 > data_size {
                break; // Not enough data for object header
            }

            let guid = ASFUtil::parse_guid(&ext_data[pos..pos + 16])?;
            let obj_size_u64 = ASFUtil::parse_u64_le(&ext_data[pos + 16..pos + 24])?;

            // Convert to usize safely — on 32-bit platforms a u64 value above
            // usize::MAX would silently truncate with `as usize`, potentially
            // bypassing the bounds check below.
            let obj_size = usize::try_from(obj_size_u64).map_err(|_| {
                ASFError::InvalidHeader(format!(
                    "Object size {} exceeds addressable range",
                    obj_size_u64
                ))
            })?;

            if obj_size < 24 {
                return Err(ASFError::InvalidHeader(
                    "Object size too small in header extension".to_string(),
                )
                .into());
            }

            if obj_size > data_size.saturating_sub(pos) {
                break; // Object extends beyond extension data
            }

            let payload = &ext_data[pos + 24..pos + obj_size];
            let mut obj = create_object_by_guid(guid);

            // Parse with error recovery
            match obj.parse(context, payload) {
                Ok(()) => self.objects.push(obj),
                Err(_) => {
                    // Add as unknown object if parsing fails
                    let mut unknown = UnknownObject::new(guid);
                    unknown.parse(context, payload)?;
                    self.objects.push(Box::new(unknown));
                }
            }

            pos += obj_size;
        }

        context.nesting_depth -= 1;
        Ok(())
    }

    fn render(&self, context: &ASFContext) -> Result<Vec<u8>> {
        let mut nested_data = Vec::new();

        // Render nested objects (skip padding at this level)
        for obj in &self.objects {
            if obj.get_guid() == ASFGUIDs::PADDING {
                continue;
            }

            let rendered = obj.render(context)?;
            let obj_size = 24 + rendered.len() as u64;

            nested_data.extend_from_slice(&obj.get_guid());
            nested_data.extend_from_slice(&obj_size.to_le_bytes());
            nested_data.extend_from_slice(&rendered);
        }

        let mut data = Vec::new();

        // Reserved GUID (Header Extension Data GUID)
        data.extend_from_slice(&[
            0x11, 0xD2, 0xD3, 0xAB, 0xBA, 0xA9, 0xCF, 0x11, 0x8E, 0xE6, 0x00, 0xC0, 0x0C, 0x20,
            0x53, 0x65,
        ]);

        // Reserved (2 bytes)
        data.extend_from_slice(&[0x06, 0x00]);

        // Data size (validated to fit in u32)
        let nested_size = u32::try_from(nested_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF header extension nested data size {} exceeds u32 maximum",
                nested_data.len()
            ))
        })?;
        data.extend_from_slice(&nested_size.to_le_bytes());

        // Nested object data
        data.extend_from_slice(&nested_data);

        Ok(data)
    }

    fn children(&self) -> Vec<&dyn ASFObject> {
        self.objects.iter().map(|obj| obj.as_ref()).collect()
    }

    fn has_children(&self) -> bool {
        !self.objects.is_empty()
    }

    fn find_child(&self, guid: &[u8; 16]) -> Option<&dyn ASFObject> {
        self.get_child(guid)
    }

    fn has_child(&self, guid: &[u8; 16]) -> bool {
        self.get_child(guid).is_some()
    }

    fn pprint(&self) -> String {
        let mut lines = vec![format!(
            "HeaderExtensionObject({})",
            ASFUtil::bytes_to_guid(&self.get_guid())
        )];
        for obj in &self.objects {
            for line in obj.pprint().lines() {
                lines.push(format!("  {}", line));
            }
        }
        lines.join("\n")
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        let mut cloned = HeaderExtensionObject::new();
        for obj in &self.objects {
            cloned.objects.push(obj.clone_boxed());
        }
        Box::new(cloned)
    }

    impl_as_any!();
}

/// Metadata Object
#[derive(Debug, Default, Clone)]
pub struct MetadataObject {
    pub attributes: Vec<(String, ASFAttribute)>,
}

impl MetadataObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for MetadataObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::METADATA
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, context: &mut ASFContext, data: &[u8]) -> Result<()> {
        if data.len() < 2 {
            return Err(ASFError::InvalidData("Metadata object too short".to_string()).into());
        }

        let num_attributes = ASFUtil::parse_u16_le(&data[0..2])? as usize;
        let mut pos = 2;

        for i in 0..num_attributes {
            if pos + 12 > data.len() {
                return Err(ASFError::InvalidData(format!(
                    "Invalid metadata attribute {} header",
                    i
                ))
                .into());
            }

            let _reserved = ASFUtil::parse_u16_le(&data[pos..pos + 2])?;
            let stream = ASFUtil::parse_u16_le(&data[pos + 2..pos + 4])?;
            let name_len = ASFUtil::parse_u16_le(&data[pos + 4..pos + 6])? as usize;
            let value_type =
                ASFAttributeType::try_from(ASFUtil::parse_u16_le(&data[pos + 6..pos + 8])?)?;
            let value_len = ASFUtil::parse_u32_le(&data[pos + 8..pos + 12])? as usize;
            pos += 12;

            // Parse name
            let name_end = pos.checked_add(name_len).ok_or_else(|| {
                ASFError::InvalidData("attribute name offset overflow".to_string())
            })?;
            if name_end > data.len() {
                return Err(ASFError::InvalidData(format!(
                    "Invalid metadata attribute {} name",
                    i
                ))
                .into());
            }
            let name = ASFUtil::parse_utf16_le(&data[pos..name_end])?;
            pos = name_end;

            // Parse value
            let value_end = pos.checked_add(value_len).ok_or_else(|| {
                ASFError::InvalidData("attribute value offset overflow".to_string())
            })?;
            if value_end > data.len() {
                return Err(ASFError::InvalidData(format!(
                    "Invalid metadata attribute {} value",
                    i
                ))
                .into());
            }

            // MetadataObject uses WORD (2 bytes) for bools, so dword=false
            let mut attr = parse_attribute(value_type as u16, &data[pos..value_end], false)?;
            pos += value_len;

            // Set stream metadata
            attr.set_stream(Some(stream));
            self.attributes.push((name.clone(), attr.clone()));
            context.tags.add(name, attr);
        }

        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        let attr_count = u16::try_from(self.attributes.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF metadata attribute count {} exceeds u16 maximum",
                self.attributes.len()
            ))
        })?;
        data.extend_from_slice(&attr_count.to_le_bytes());

        for (name, attr) in &self.attributes {
            data.extend_from_slice(&attr.render_metadata(name)?);
        }

        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Metadata Library Object
#[derive(Debug, Default, Clone)]
pub struct MetadataLibraryObject {
    pub attributes: Vec<(String, ASFAttribute)>,
}

impl MetadataLibraryObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for MetadataLibraryObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::METADATA_LIBRARY
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, context: &mut ASFContext, data: &[u8]) -> Result<()> {
        if data.len() < 2 {
            return Err(
                ASFError::InvalidData("Metadata library object too short".to_string()).into(),
            );
        }

        let num_attributes = ASFUtil::parse_u16_le(&data[0..2])? as usize;
        let mut pos = 2;

        for i in 0..num_attributes {
            if pos + 12 > data.len() {
                return Err(ASFError::InvalidData(format!(
                    "Invalid metadata library attribute {} header",
                    i
                ))
                .into());
            }

            let language = ASFUtil::parse_u16_le(&data[pos..pos + 2])?;
            let stream = ASFUtil::parse_u16_le(&data[pos + 2..pos + 4])?;
            let name_len = ASFUtil::parse_u16_le(&data[pos + 4..pos + 6])? as usize;
            let value_type =
                ASFAttributeType::try_from(ASFUtil::parse_u16_le(&data[pos + 6..pos + 8])?)?;
            let value_len = ASFUtil::parse_u32_le(&data[pos + 8..pos + 12])? as usize;
            pos += 12;

            // Parse name
            let name_end = pos.checked_add(name_len).ok_or_else(|| {
                ASFError::InvalidData("attribute name offset overflow".to_string())
            })?;
            if name_end > data.len() {
                return Err(ASFError::InvalidData(format!(
                    "Invalid metadata library attribute {} name",
                    i
                ))
                .into());
            }
            let name = ASFUtil::parse_utf16_le(&data[pos..name_end])?;
            pos = name_end;

            // Parse value
            let value_end = pos.checked_add(value_len).ok_or_else(|| {
                ASFError::InvalidData("attribute value offset overflow".to_string())
            })?;
            if value_end > data.len() {
                return Err(ASFError::InvalidData(format!(
                    "Invalid metadata library attribute {} value",
                    i
                ))
                .into());
            }

            // MetadataLibraryObject uses WORD (2 bytes) for bools, so dword=false
            let mut attr = parse_attribute(value_type as u16, &data[pos..value_end], false)?;
            pos += value_len;

            // Set language and stream metadata
            attr.set_language(Some(language));
            attr.set_stream(Some(stream));
            self.attributes.push((name.clone(), attr.clone()));
            context.tags.add(name, attr);
        }

        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        let attr_count = u16::try_from(self.attributes.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF metadata library attribute count {} exceeds u16 maximum",
                self.attributes.len()
            ))
        })?;
        data.extend_from_slice(&attr_count.to_le_bytes());

        for (name, attr) in &self.attributes {
            data.extend_from_slice(&attr.render_metadata_library(name)?);
        }

        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Digital Signature Object
#[derive(Debug, Default, Clone)]
pub struct DigitalSignatureObject {
    pub signature_type: u32,
    pub signature_data: Vec<u8>,
}

impl DigitalSignatureObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for DigitalSignatureObject {
    fn guid() -> [u8; 16] {
        // Digital Signature Object GUID: 2211B3FC-BD23-11D2-B4B7-00A0C955FC6E
        [
            0xFC, 0xB3, 0x11, 0x22, 0x23, 0xBD, 0xD2, 0x11, 0xB4, 0xB7, 0x00, 0xA0, 0xC9, 0x55,
            0xFC, 0x6E,
        ]
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, _context: &mut ASFContext, data: &[u8]) -> Result<()> {
        _context
            .parse_limits
            .check_tag_size(data.len() as u64, "DigitalSignatureObject")?;

        if data.len() < 4 {
            return Err(ASFError::InvalidData("Digital signature too short".to_string()).into());
        }

        self.signature_type = ASFUtil::parse_u32_le(&data[0..4])?;
        self.signature_data = data[4..].to_vec();
        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&self.signature_type.to_le_bytes());
        data.extend_from_slice(&self.signature_data);
        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Extended Stream Properties Object
#[derive(Debug, Default, Clone)]
pub struct ExtendedStreamPropertiesObject {
    pub start_time: u64,
    pub end_time: u64,
    pub data_bitrate: u32,
    pub buffer_size: u32,
    pub initial_buffer_fullness: u32,
    pub alternate_data_bitrate: u32,
    pub alternate_buffer_size: u32,
    pub alternate_initial_buffer_fullness: u32,
    pub maximum_object_size: u32,
    pub flags: u32,
    pub stream_number: u16,
    pub stream_language_id_index: u16,
    pub average_time_per_frame: u64,
    pub stream_name_count: u16,
    pub payload_extension_system_count: u16,
    pub stream_names: Vec<String>,
    pub payload_extension_systems: Vec<Vec<u8>>,
    /// Raw bytes following the 64-byte fixed header (stream names,
    /// payload extension systems, and any future fields).
    pub trailing_data: Vec<u8>,
}

impl ExtendedStreamPropertiesObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for ExtendedStreamPropertiesObject {
    fn guid() -> [u8; 16] {
        // Extended Stream Properties Object GUID: 14E6A5CB-C672-4332-8399-A96952065B5A
        [
            0xCB, 0xA5, 0xE6, 0x14, 0x72, 0xC6, 0x32, 0x43, 0x83, 0x99, 0xA9, 0x69, 0x52, 0x06,
            0x5B, 0x5A,
        ]
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, _context: &mut ASFContext, data: &[u8]) -> Result<()> {
        if data.len() < 64 {
            return Err(
                ASFError::InvalidData("Extended stream properties too short".to_string()).into(),
            );
        }

        self.start_time = ASFUtil::parse_u64_le(&data[0..8])?;
        self.end_time = ASFUtil::parse_u64_le(&data[8..16])?;
        self.data_bitrate = ASFUtil::parse_u32_le(&data[16..20])?;
        self.buffer_size = ASFUtil::parse_u32_le(&data[20..24])?;
        self.initial_buffer_fullness = ASFUtil::parse_u32_le(&data[24..28])?;
        self.alternate_data_bitrate = ASFUtil::parse_u32_le(&data[28..32])?;
        self.alternate_buffer_size = ASFUtil::parse_u32_le(&data[32..36])?;
        self.alternate_initial_buffer_fullness = ASFUtil::parse_u32_le(&data[36..40])?;
        self.maximum_object_size = ASFUtil::parse_u32_le(&data[40..44])?;
        self.flags = ASFUtil::parse_u32_le(&data[44..48])?;
        self.stream_number = ASFUtil::parse_u16_le(&data[48..50])?;
        self.stream_language_id_index = ASFUtil::parse_u16_le(&data[50..52])?;
        self.average_time_per_frame = ASFUtil::parse_u64_le(&data[52..60])?;
        self.stream_name_count = ASFUtil::parse_u16_le(&data[60..62])?;
        self.payload_extension_system_count = ASFUtil::parse_u16_le(&data[62..64])?;

        // Preserve any data beyond the fixed header (stream names,
        // payload extension systems, etc.) for lossless round-tripping.
        if data.len() > 64 {
            self.trailing_data = data[64..].to_vec();
        }

        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&self.start_time.to_le_bytes());
        data.extend_from_slice(&self.end_time.to_le_bytes());
        data.extend_from_slice(&self.data_bitrate.to_le_bytes());
        data.extend_from_slice(&self.buffer_size.to_le_bytes());
        data.extend_from_slice(&self.initial_buffer_fullness.to_le_bytes());
        data.extend_from_slice(&self.alternate_data_bitrate.to_le_bytes());
        data.extend_from_slice(&self.alternate_buffer_size.to_le_bytes());
        data.extend_from_slice(&self.alternate_initial_buffer_fullness.to_le_bytes());
        data.extend_from_slice(&self.maximum_object_size.to_le_bytes());
        data.extend_from_slice(&self.flags.to_le_bytes());
        data.extend_from_slice(&self.stream_number.to_le_bytes());
        data.extend_from_slice(&self.stream_language_id_index.to_le_bytes());
        data.extend_from_slice(&self.average_time_per_frame.to_le_bytes());
        data.extend_from_slice(&self.stream_name_count.to_le_bytes());
        data.extend_from_slice(&self.payload_extension_system_count.to_le_bytes());

        // Re-emit the trailing data preserved during parse
        data.extend_from_slice(&self.trailing_data);

        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Bitrate Mutual Exclusion Object
#[derive(Debug, Default, Clone)]
pub struct BitrateMutualExclusionObject {
    pub exclusion_type: [u8; 16],
    pub stream_count: u16,
    pub stream_numbers: Vec<u16>,
    /// Any trailing bytes after the known fields, preserved for round-trip fidelity
    pub trailing_data: Vec<u8>,
}

impl BitrateMutualExclusionObject {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ASFObject for BitrateMutualExclusionObject {
    fn guid() -> [u8; 16] {
        ASFGUIDs::BITRATE_MUTUAL_EXCLUSION
    }

    fn get_guid(&self) -> [u8; 16] {
        Self::guid()
    }

    fn parse(&mut self, _context: &mut ASFContext, data: &[u8]) -> Result<()> {
        if data.len() < 18 {
            return Err(
                ASFError::InvalidData("Bitrate mutual exclusion too short".to_string()).into(),
            );
        }

        self.exclusion_type.copy_from_slice(&data[0..16]);
        self.stream_count = ASFUtil::parse_u16_le(&data[16..18])?;

        let expected_size = 18 + (self.stream_count as usize * 2);
        if data.len() < expected_size {
            return Err(
                ASFError::InvalidData("Insufficient data for stream numbers".to_string()).into(),
            );
        }

        self.stream_numbers.clear();
        for i in 0..self.stream_count {
            let offset = 18 + (i as usize * 2);
            let stream_number = ASFUtil::parse_u16_le(&data[offset..offset + 2])?;
            self.stream_numbers.push(stream_number);
        }

        // Preserve any trailing bytes beyond the known fields for round-trip fidelity
        if data.len() > expected_size {
            self.trailing_data = data[expected_size..].to_vec();
        }

        Ok(())
    }

    fn render(&self, _context: &ASFContext) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&self.exclusion_type);
        data.extend_from_slice(&self.stream_count.to_le_bytes());

        for &stream_number in &self.stream_numbers {
            data.extend_from_slice(&stream_number.to_le_bytes());
        }

        // Re-emit the trailing data preserved during parse
        data.extend_from_slice(&self.trailing_data);

        Ok(data)
    }

    fn clone_boxed(&self) -> Box<dyn ASFObject> {
        Box::new(self.clone())
    }

    impl_as_any!();
}

/// Convenience function to render object with header (GUID + size + data)
pub fn render_object_with_header(obj: &dyn ASFObject, context: &ASFContext) -> Result<Vec<u8>> {
    let payload = obj.render(context)?;
    let total_size = 24 + payload.len() as u64;

    let mut data = Vec::new();
    data.extend_from_slice(&obj.get_guid());
    data.extend_from_slice(&total_size.to_le_bytes());
    data.extend_from_slice(&payload);

    Ok(data)
}

impl fmt::Display for dyn ASFObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}({})",
            std::any::type_name::<Self>()
                .split("::")
                .last()
                .unwrap_or("ASFObject"),
            ASFUtil::bytes_to_guid(&self.get_guid())
        )
    }
}
