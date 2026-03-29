//! ASF attribute handling
//!
//! This module provides a complete implementation of ASF attributes with
//! comprehensive functionality.
//!
//! Key features:
//! - Complete ASFBaseAttribute trait system with 7 attribute types
//! - Dynamic type registry for attribute creation
//! - Rich ASFTags collection with both sequential and key-value interfaces
//! - Comprehensive validation framework
//! - All 4 rendering contexts (basic, extended, metadata, metadata_library)

use super::util::{ASFError, ASFUtil};
use crate::tags::Tags;
use crate::{AudexError, Result};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

/// ASF attribute type constants
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ASFAttributeType {
    Unicode = 0x0000,
    ByteArray = 0x0001,
    Bool = 0x0002,
    DWord = 0x0003,
    QWord = 0x0004,
    Word = 0x0005,
    Guid = 0x0006,
}

impl TryFrom<u16> for ASFAttributeType {
    type Error = AudexError;

    fn try_from(value: u16) -> Result<Self> {
        match value {
            0x0000 => Ok(ASFAttributeType::Unicode),
            0x0001 => Ok(ASFAttributeType::ByteArray),
            0x0002 => Ok(ASFAttributeType::Bool),
            0x0003 => Ok(ASFAttributeType::DWord),
            0x0004 => Ok(ASFAttributeType::QWord),
            0x0005 => Ok(ASFAttributeType::Word),
            0x0006 => Ok(ASFAttributeType::Guid),
            _ => Err(AudexError::InvalidData(format!(
                "Unknown ASF attribute type: {}",
                value
            ))),
        }
    }
}

impl fmt::Display for ASFAttributeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04X}", *self as u16)
    }
}

/// Base trait for all ASF attributes
///
/// This trait provides the core functionality that all ASF attribute types must implement,
/// including parsing, rendering, validation, and size calculations.
pub trait ASFBaseAttribute: Clone + PartialEq + fmt::Debug + fmt::Display + Hash {
    /// The ASF type constant for this attribute type
    const TYPE: u16;

    /// Parse attribute data from bytes with optional parameters
    fn parse(data: &[u8], dword: bool) -> Result<Self>
    where
        Self: Sized;

    /// Validate a value before assignment
    fn validate(value: &dyn std::any::Any) -> Result<Self>
    where
        Self: Sized;

    /// Calculate the data size for this attribute
    fn data_size(&self) -> usize;

    /// Render the attribute data (internal representation)
    fn render_data(&self, dword: bool) -> Vec<u8>;

    /// Render for Extended Content Description Object.
    /// Returns an error if name or data exceeds the u16 length limit.
    fn render(&self, name: &str) -> Result<Vec<u8>> {
        let name_data = ASFUtil::encode_utf16_le(name);
        let data = self.render_data(true);

        // The ASF spec uses u16 for length fields — reject data that
        // would overflow and produce a corrupt length/data mismatch.
        let name_len = u16::try_from(name_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF attribute name too long ({} bytes, max {})",
                name_data.len(),
                u16::MAX
            ))
        })?;
        let data_len = u16::try_from(data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF attribute data too long ({} bytes, max {})",
                data.len(),
                u16::MAX
            ))
        })?;

        let mut result = Vec::new();
        result.extend_from_slice(&name_len.to_le_bytes());
        result.extend_from_slice(&name_data);
        result.extend_from_slice(&Self::TYPE.to_le_bytes());
        result.extend_from_slice(&data_len.to_le_bytes());
        result.extend_from_slice(&data);
        Ok(result)
    }

    /// Render for Metadata Object.
    /// Returns an error if the encoded name exceeds the u16 length limit.
    fn render_metadata(&self, name: &str, stream: u16) -> Result<Vec<u8>> {
        let name_data = ASFUtil::encode_utf16_le(name);
        let data = if Self::TYPE == ASFAttributeType::Bool as u16 {
            self.render_data(false) // Use WORD for bool in metadata
        } else {
            self.render_data(true)
        };

        let name_len = u16::try_from(name_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF metadata attribute name too long ({} bytes, max {})",
                name_data.len(),
                u16::MAX
            ))
        })?;

        let mut result = Vec::new();
        result.extend_from_slice(&0u16.to_le_bytes()); // reserved
        result.extend_from_slice(&stream.to_le_bytes());
        result.extend_from_slice(&name_len.to_le_bytes());
        result.extend_from_slice(&Self::TYPE.to_le_bytes());
        let data_len = u32::try_from(data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF attribute data length {} exceeds u32 maximum",
                data.len()
            ))
        })?;
        result.extend_from_slice(&data_len.to_le_bytes());
        result.extend_from_slice(&name_data);
        result.extend_from_slice(&data);
        Ok(result)
    }

    /// Render for Metadata Library Object.
    /// Returns an error if the encoded name exceeds the u16 length limit.
    fn render_metadata_library(&self, name: &str, language: u16, stream: u16) -> Result<Vec<u8>> {
        let name_data = ASFUtil::encode_utf16_le(name);
        let data = if Self::TYPE == ASFAttributeType::Bool as u16 {
            self.render_data(false) // Use WORD for bool in metadata library
        } else {
            self.render_data(true)
        };

        let name_len = u16::try_from(name_data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF metadata library attribute name too long ({} bytes, max {})",
                name_data.len(),
                u16::MAX
            ))
        })?;

        let mut result = Vec::new();
        result.extend_from_slice(&language.to_le_bytes());
        result.extend_from_slice(&stream.to_le_bytes());
        result.extend_from_slice(&name_len.to_le_bytes());
        result.extend_from_slice(&Self::TYPE.to_le_bytes());
        let data_len = u32::try_from(data.len()).map_err(|_| {
            AudexError::InvalidData(format!(
                "ASF attribute data length {} exceeds u32 maximum",
                data.len()
            ))
        })?;
        result.extend_from_slice(&data_len.to_le_bytes());
        result.extend_from_slice(&name_data);
        result.extend_from_slice(&data);
        Ok(result)
    }

    fn get_type(&self) -> ASFAttributeType {
        // This should always succeed since TYPE constants are properly defined
        match ASFAttributeType::try_from(Self::TYPE) {
            Ok(attr_type) => attr_type,
            Err(_) => {
                // This should never happen with properly defined TYPE constants
                // but providing fallback for safety
                ASFAttributeType::Unicode
            }
        }
    }

    /// Convert to bytes for comparison
    fn to_bytes(&self) -> Vec<u8>;

    fn to_string(&self) -> String;
}

/// Unicode string attribute
#[derive(Debug, Clone, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFUnicodeAttribute {
    pub value: String,
    pub language: Option<u16>,
    pub stream: Option<u16>,
}

impl ASFUnicodeAttribute {
    pub fn new(value: String) -> Self {
        Self {
            value,
            language: None,
            stream: None,
        }
    }

    pub fn with_metadata(value: String, language: Option<u16>, stream: Option<u16>) -> Self {
        Self {
            value,
            language,
            stream,
        }
    }
}

impl ASFBaseAttribute for ASFUnicodeAttribute {
    const TYPE: u16 = ASFAttributeType::Unicode as u16;

    fn parse(data: &[u8], _dword: bool) -> Result<Self> {
        let value = ASFUtil::parse_utf16_le(data)
            .map_err(|e| {
                AudexError::ASF(ASFError::InvalidData(format!("Unicode parse error: {}", e)))
            })?
            .trim_end_matches('\0')
            .to_string();
        Ok(Self::new(value))
    }

    fn validate(value: &dyn std::any::Any) -> Result<Self> {
        if let Some(s) = value.downcast_ref::<String>() {
            Ok(Self::new(s.clone()))
        } else if let Some(s) = value.downcast_ref::<&str>() {
            Ok(Self::new(s.to_string()))
        } else {
            Err(AudexError::ASF(ASFError::InvalidData(format!(
                "Expected String or &str, got {:?}",
                std::any::type_name_of_val(value)
            ))))
        }
    }

    fn data_size(&self) -> usize {
        ASFUtil::encode_utf16_le(&self.value).len()
    }

    fn render_data(&self, _dword: bool) -> Vec<u8> {
        ASFUtil::encode_utf16_le(&self.value)
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.value
            .encode_utf16()
            .flat_map(|c| c.to_le_bytes().to_vec())
            .collect()
    }

    fn to_string(&self) -> String {
        self.value.clone()
    }
}

impl fmt::Display for ASFUnicodeAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl PartialOrd for ASFUnicodeAttribute {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ASFUnicodeAttribute {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Eq for ASFUnicodeAttribute {}

/// Byte array attribute
#[derive(Debug, Clone, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFByteArrayAttribute {
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::bytes_as_base64")
    )]
    pub value: Vec<u8>,
    pub language: Option<u16>,
    pub stream: Option<u16>,
}

impl ASFByteArrayAttribute {
    pub fn new(value: Vec<u8>) -> Self {
        Self {
            value,
            language: None,
            stream: None,
        }
    }

    pub fn with_metadata(value: Vec<u8>, language: Option<u16>, stream: Option<u16>) -> Self {
        Self {
            value,
            language,
            stream,
        }
    }
}

impl ASFBaseAttribute for ASFByteArrayAttribute {
    const TYPE: u16 = ASFAttributeType::ByteArray as u16;

    fn parse(data: &[u8], _dword: bool) -> Result<Self> {
        Ok(Self::new(data.to_vec()))
    }

    fn validate(value: &dyn std::any::Any) -> Result<Self> {
        if let Some(b) = value.downcast_ref::<Vec<u8>>() {
            Ok(Self::new(b.clone()))
        } else if let Some(b) = value.downcast_ref::<&[u8]>() {
            Ok(Self::new(b.to_vec()))
        } else {
            Err(AudexError::ASF(ASFError::InvalidData(
                "Expected Vec<u8> or &[u8]".to_string(),
            )))
        }
    }

    fn data_size(&self) -> usize {
        self.value.len()
    }

    fn render_data(&self, _dword: bool) -> Vec<u8> {
        self.value.clone()
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.value.clone()
    }

    fn to_string(&self) -> String {
        format!("[binary data ({} bytes)]", self.value.len())
    }
}

impl fmt::Display for ASFByteArrayAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[binary data ({} bytes)]", self.value.len())
    }
}

impl PartialOrd for ASFByteArrayAttribute {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ASFByteArrayAttribute {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Eq for ASFByteArrayAttribute {}

/// Boolean attribute
#[derive(Debug, Clone, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFBoolAttribute {
    pub value: bool,
    pub language: Option<u16>,
    pub stream: Option<u16>,
}

impl ASFBoolAttribute {
    pub fn new(value: bool) -> Self {
        Self {
            value,
            language: None,
            stream: None,
        }
    }

    pub fn with_metadata(value: bool, language: Option<u16>, stream: Option<u16>) -> Self {
        Self {
            value,
            language,
            stream,
        }
    }
}

impl ASFBaseAttribute for ASFBoolAttribute {
    const TYPE: u16 = ASFAttributeType::Bool as u16;

    fn parse(data: &[u8], dword: bool) -> Result<Self> {
        let value = if dword {
            if data.len() < 4 {
                return Err(AudexError::ASF(ASFError::InvalidData(
                    "Not enough data for bool (DWORD)".to_string(),
                )));
            }
            ASFUtil::parse_u32_le(data)? == 1
        } else {
            if data.len() < 2 {
                return Err(AudexError::ASF(ASFError::InvalidData(
                    "Not enough data for bool (WORD)".to_string(),
                )));
            }
            ASFUtil::parse_u16_le(data)? == 1
        };
        Ok(Self::new(value))
    }

    fn validate(value: &dyn std::any::Any) -> Result<Self> {
        if let Some(b) = value.downcast_ref::<bool>() {
            Ok(Self::new(*b))
        } else if let Some(i) = value.downcast_ref::<u32>() {
            Ok(Self::new(*i != 0))
        } else if let Some(i) = value.downcast_ref::<i32>() {
            Ok(Self::new(*i != 0))
        } else {
            Err(AudexError::ASF(ASFError::InvalidData(
                "Expected bool or numeric value".to_string(),
            )))
        }
    }

    fn data_size(&self) -> usize {
        // Returns DWORD size (4) for Extended Content Description context.
        // Metadata/MetadataLibrary objects use WORD (2 bytes) for bools,
        // but those paths compute their size from render_data(false).len().
        4
    }

    fn render_data(&self, dword: bool) -> Vec<u8> {
        if dword {
            (self.value as u32).to_le_bytes().to_vec()
        } else {
            (self.value as u16).to_le_bytes().to_vec()
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.value.to_string().into_bytes()
    }

    fn to_string(&self) -> String {
        self.value.to_string()
    }
}

impl fmt::Display for ASFBoolAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl PartialOrd for ASFBoolAttribute {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ASFBoolAttribute {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Eq for ASFBoolAttribute {}

/// DWORD attribute
#[derive(Debug, Clone, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFDWordAttribute {
    pub value: u32,
    pub language: Option<u16>,
    pub stream: Option<u16>,
}

impl ASFDWordAttribute {
    pub fn new(value: u32) -> Self {
        Self {
            value,
            language: None,
            stream: None,
        }
    }

    pub fn with_metadata(value: u32, language: Option<u16>, stream: Option<u16>) -> Self {
        Self {
            value,
            language,
            stream,
        }
    }
}

impl ASFBaseAttribute for ASFDWordAttribute {
    const TYPE: u16 = ASFAttributeType::DWord as u16;

    fn parse(data: &[u8], _dword: bool) -> Result<Self> {
        let value = ASFUtil::parse_u32_le(data)?;
        Ok(Self::new(value))
    }

    fn validate(value: &dyn std::any::Any) -> Result<Self> {
        if let Some(d) = value.downcast_ref::<u32>() {
            Ok(Self::new(*d))
        } else if let Some(d) = value.downcast_ref::<i32>() {
            if *d < 0 {
                return Err(AudexError::ASF(ASFError::InvalidData(
                    "DWORD values must be non-negative".to_string(),
                )));
            }
            Ok(Self::new(*d as u32))
        } else if let Some(s) = value.downcast_ref::<String>() {
            match s.parse::<u32>() {
                Ok(val) => Ok(Self::new(val)),
                Err(_) => Err(AudexError::ASF(ASFError::InvalidData(format!(
                    "Cannot parse '{}' as DWORD",
                    s
                )))),
            }
        } else {
            Err(AudexError::ASF(ASFError::InvalidData(
                "Expected u32, i32, or String".to_string(),
            )))
        }
    }

    fn data_size(&self) -> usize {
        4
    }

    fn render_data(&self, _dword: bool) -> Vec<u8> {
        self.value.to_le_bytes().to_vec()
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.value.to_string().into_bytes()
    }

    fn to_string(&self) -> String {
        self.value.to_string()
    }
}

impl fmt::Display for ASFDWordAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl PartialOrd for ASFDWordAttribute {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ASFDWordAttribute {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Eq for ASFDWordAttribute {}

/// QWORD attribute
#[derive(Debug, Clone, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFQWordAttribute {
    pub value: u64,
    pub language: Option<u16>,
    pub stream: Option<u16>,
}

impl ASFQWordAttribute {
    pub fn new(value: u64) -> Self {
        Self {
            value,
            language: None,
            stream: None,
        }
    }

    pub fn with_metadata(value: u64, language: Option<u16>, stream: Option<u16>) -> Self {
        Self {
            value,
            language,
            stream,
        }
    }
}

impl ASFBaseAttribute for ASFQWordAttribute {
    const TYPE: u16 = ASFAttributeType::QWord as u16;

    fn parse(data: &[u8], _dword: bool) -> Result<Self> {
        let value = ASFUtil::parse_u64_le(data)?;
        Ok(Self::new(value))
    }

    fn validate(value: &dyn std::any::Any) -> Result<Self> {
        if let Some(q) = value.downcast_ref::<u64>() {
            Ok(Self::new(*q))
        } else if let Some(q) = value.downcast_ref::<i64>() {
            if *q < 0 {
                return Err(AudexError::ASF(ASFError::InvalidData(
                    "QWORD values must be non-negative".to_string(),
                )));
            }
            Ok(Self::new(*q as u64))
        } else if let Some(s) = value.downcast_ref::<String>() {
            match s.parse::<u64>() {
                Ok(val) => Ok(Self::new(val)),
                Err(_) => Err(AudexError::ASF(ASFError::InvalidData(format!(
                    "Cannot parse '{}' as QWORD",
                    s
                )))),
            }
        } else {
            Err(AudexError::ASF(ASFError::InvalidData(
                "Expected u64, i64, or String".to_string(),
            )))
        }
    }

    fn data_size(&self) -> usize {
        8
    }

    fn render_data(&self, _dword: bool) -> Vec<u8> {
        self.value.to_le_bytes().to_vec()
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.value.to_string().into_bytes()
    }

    fn to_string(&self) -> String {
        self.value.to_string()
    }
}

impl fmt::Display for ASFQWordAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl PartialOrd for ASFQWordAttribute {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ASFQWordAttribute {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Eq for ASFQWordAttribute {}

/// WORD attribute
#[derive(Debug, Clone, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFWordAttribute {
    pub value: u16,
    pub language: Option<u16>,
    pub stream: Option<u16>,
}

impl ASFWordAttribute {
    pub fn new(value: u16) -> Self {
        Self {
            value,
            language: None,
            stream: None,
        }
    }

    pub fn with_metadata(value: u16, language: Option<u16>, stream: Option<u16>) -> Self {
        Self {
            value,
            language,
            stream,
        }
    }
}

impl ASFBaseAttribute for ASFWordAttribute {
    const TYPE: u16 = ASFAttributeType::Word as u16;

    fn parse(data: &[u8], _dword: bool) -> Result<Self> {
        let value = ASFUtil::parse_u16_le(data)?;
        Ok(Self::new(value))
    }

    fn validate(value: &dyn std::any::Any) -> Result<Self> {
        if let Some(w) = value.downcast_ref::<u16>() {
            Ok(Self::new(*w))
        } else if let Some(w) = value.downcast_ref::<i16>() {
            if *w < 0 {
                return Err(AudexError::ASF(ASFError::InvalidData(
                    "WORD values must be non-negative".to_string(),
                )));
            }
            Ok(Self::new(*w as u16))
        } else if let Some(s) = value.downcast_ref::<String>() {
            match s.parse::<u16>() {
                Ok(val) => Ok(Self::new(val)),
                Err(_) => Err(AudexError::ASF(ASFError::InvalidData(format!(
                    "Cannot parse '{}' as WORD",
                    s
                )))),
            }
        } else {
            Err(AudexError::ASF(ASFError::InvalidData(
                "Expected u16, i16, or String".to_string(),
            )))
        }
    }

    fn data_size(&self) -> usize {
        2
    }

    fn render_data(&self, _dword: bool) -> Vec<u8> {
        self.value.to_le_bytes().to_vec()
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.value.to_string().into_bytes()
    }

    fn to_string(&self) -> String {
        self.value.to_string()
    }
}

impl fmt::Display for ASFWordAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl PartialOrd for ASFWordAttribute {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ASFWordAttribute {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Eq for ASFWordAttribute {}

/// GUID attribute
#[derive(Debug, Clone, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFGuidAttribute {
    pub value: [u8; 16],
    pub language: Option<u16>,
    pub stream: Option<u16>,
}

impl ASFGuidAttribute {
    pub fn new(value: [u8; 16]) -> Self {
        Self {
            value,
            language: None,
            stream: None,
        }
    }

    pub fn with_metadata(value: [u8; 16], language: Option<u16>, stream: Option<u16>) -> Self {
        Self {
            value,
            language,
            stream,
        }
    }
}

impl ASFBaseAttribute for ASFGuidAttribute {
    const TYPE: u16 = ASFAttributeType::Guid as u16;

    fn parse(data: &[u8], _dword: bool) -> Result<Self> {
        let guid = ASFUtil::parse_guid(data)?;
        Ok(Self::new(guid))
    }

    fn validate(value: &dyn std::any::Any) -> Result<Self> {
        if let Some(g) = value.downcast_ref::<[u8; 16]>() {
            Ok(Self::new(*g))
        } else if let Some(v) = value.downcast_ref::<Vec<u8>>() {
            if v.len() != 16 {
                return Err(AudexError::ASF(ASFError::InvalidData(
                    "GUID must be exactly 16 bytes".to_string(),
                )));
            }
            let mut guid = [0u8; 16];
            guid.copy_from_slice(v);
            Ok(Self::new(guid))
        } else {
            Err(AudexError::ASF(ASFError::InvalidData(
                "Expected [u8; 16] or Vec<u8> with 16 bytes".to_string(),
            )))
        }
    }

    fn data_size(&self) -> usize {
        16
    }

    fn render_data(&self, _dword: bool) -> Vec<u8> {
        self.value.to_vec()
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.value.to_vec()
    }

    fn to_string(&self) -> String {
        ASFUtil::bytes_to_guid(&self.value)
    }
}

impl fmt::Display for ASFGuidAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", ASFUtil::bytes_to_guid(&self.value))
    }
}

impl PartialOrd for ASFGuidAttribute {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ASFGuidAttribute {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl Eq for ASFGuidAttribute {}

/// Unified ASF attribute enum - holds any ASF attribute type
#[derive(Debug, Clone, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ASFAttribute {
    Unicode(ASFUnicodeAttribute),
    ByteArray(ASFByteArrayAttribute),
    Bool(ASFBoolAttribute),
    DWord(ASFDWordAttribute),
    QWord(ASFQWordAttribute),
    Word(ASFWordAttribute),
    Guid(ASFGuidAttribute),
}

impl ASFAttribute {
    pub fn get_type(&self) -> ASFAttributeType {
        match self {
            ASFAttribute::Unicode(_) => ASFAttributeType::Unicode,
            ASFAttribute::ByteArray(_) => ASFAttributeType::ByteArray,
            ASFAttribute::Bool(_) => ASFAttributeType::Bool,
            ASFAttribute::DWord(_) => ASFAttributeType::DWord,
            ASFAttribute::QWord(_) => ASFAttributeType::QWord,
            ASFAttribute::Word(_) => ASFAttributeType::Word,
            ASFAttribute::Guid(_) => ASFAttributeType::Guid,
        }
    }

    pub fn language(&self) -> Option<u16> {
        match self {
            ASFAttribute::Unicode(a) => a.language,
            ASFAttribute::ByteArray(a) => a.language,
            ASFAttribute::Bool(a) => a.language,
            ASFAttribute::DWord(a) => a.language,
            ASFAttribute::QWord(a) => a.language,
            ASFAttribute::Word(a) => a.language,
            ASFAttribute::Guid(a) => a.language,
        }
    }

    pub fn stream(&self) -> Option<u16> {
        match self {
            ASFAttribute::Unicode(a) => a.stream,
            ASFAttribute::ByteArray(a) => a.stream,
            ASFAttribute::Bool(a) => a.stream,
            ASFAttribute::DWord(a) => a.stream,
            ASFAttribute::QWord(a) => a.stream,
            ASFAttribute::Word(a) => a.stream,
            ASFAttribute::Guid(a) => a.stream,
        }
    }

    pub fn set_language(&mut self, language: Option<u16>) {
        match self {
            ASFAttribute::Unicode(a) => a.language = language,
            ASFAttribute::ByteArray(a) => a.language = language,
            ASFAttribute::Bool(a) => a.language = language,
            ASFAttribute::DWord(a) => a.language = language,
            ASFAttribute::QWord(a) => a.language = language,
            ASFAttribute::Word(a) => a.language = language,
            ASFAttribute::Guid(a) => a.language = language,
        }
    }

    pub fn set_stream(&mut self, stream: Option<u16>) {
        match self {
            ASFAttribute::Unicode(a) => a.stream = stream,
            ASFAttribute::ByteArray(a) => a.stream = stream,
            ASFAttribute::Bool(a) => a.stream = stream,
            ASFAttribute::DWord(a) => a.stream = stream,
            ASFAttribute::QWord(a) => a.stream = stream,
            ASFAttribute::Word(a) => a.stream = stream,
            ASFAttribute::Guid(a) => a.stream = stream,
        }
    }

    /// Calculate data size
    pub fn data_size(&self) -> usize {
        match self {
            ASFAttribute::Unicode(a) => a.data_size(),
            ASFAttribute::ByteArray(a) => a.data_size(),
            ASFAttribute::Bool(a) => a.data_size(),
            ASFAttribute::DWord(a) => a.data_size(),
            ASFAttribute::QWord(a) => a.data_size(),
            ASFAttribute::Word(a) => a.data_size(),
            ASFAttribute::Guid(a) => a.data_size(),
        }
    }

    /// Render for Extended Content Description Object.
    /// Returns an error if name or data exceeds the u16 length limit.
    pub fn render(&self, name: &str) -> Result<Vec<u8>> {
        match self {
            ASFAttribute::Unicode(a) => a.render(name),
            ASFAttribute::ByteArray(a) => a.render(name),
            ASFAttribute::Bool(a) => a.render(name),
            ASFAttribute::DWord(a) => a.render(name),
            ASFAttribute::QWord(a) => a.render(name),
            ASFAttribute::Word(a) => a.render(name),
            ASFAttribute::Guid(a) => a.render(name),
        }
    }

    /// Render for Metadata Object.
    /// Returns an error if the encoded name exceeds the u16 length limit.
    pub fn render_metadata(&self, name: &str) -> Result<Vec<u8>> {
        let stream = self.stream().unwrap_or(0);
        match self {
            ASFAttribute::Unicode(a) => a.render_metadata(name, stream),
            ASFAttribute::ByteArray(a) => a.render_metadata(name, stream),
            ASFAttribute::Bool(a) => a.render_metadata(name, stream),
            ASFAttribute::DWord(a) => a.render_metadata(name, stream),
            ASFAttribute::QWord(a) => a.render_metadata(name, stream),
            ASFAttribute::Word(a) => a.render_metadata(name, stream),
            ASFAttribute::Guid(a) => a.render_metadata(name, stream),
        }
    }

    /// Render for Metadata Library Object.
    /// Returns an error if the encoded name exceeds the u16 length limit.
    pub fn render_metadata_library(&self, name: &str) -> Result<Vec<u8>> {
        let language = self.language().unwrap_or(0);
        let stream = self.stream().unwrap_or(0);
        match self {
            ASFAttribute::Unicode(a) => a.render_metadata_library(name, language, stream),
            ASFAttribute::ByteArray(a) => a.render_metadata_library(name, language, stream),
            ASFAttribute::Bool(a) => a.render_metadata_library(name, language, stream),
            ASFAttribute::DWord(a) => a.render_metadata_library(name, language, stream),
            ASFAttribute::QWord(a) => a.render_metadata_library(name, language, stream),
            ASFAttribute::Word(a) => a.render_metadata_library(name, language, stream),
            ASFAttribute::Guid(a) => a.render_metadata_library(name, language, stream),
        }
    }

    pub fn unicode(s: String) -> Self {
        ASFAttribute::Unicode(ASFUnicodeAttribute::new(s))
    }

    pub fn byte_array(data: Vec<u8>) -> Self {
        ASFAttribute::ByteArray(ASFByteArrayAttribute::new(data))
    }

    pub fn bool(b: bool) -> Self {
        ASFAttribute::Bool(ASFBoolAttribute::new(b))
    }

    pub fn dword(d: u32) -> Self {
        ASFAttribute::DWord(ASFDWordAttribute::new(d))
    }

    pub fn qword(q: u64) -> Self {
        ASFAttribute::QWord(ASFQWordAttribute::new(q))
    }

    pub fn word(w: u16) -> Self {
        ASFAttribute::Word(ASFWordAttribute::new(w))
    }

    pub fn guid(g: [u8; 16]) -> Self {
        ASFAttribute::Guid(ASFGuidAttribute::new(g))
    }
}

impl fmt::Display for ASFAttribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ASFAttribute::Unicode(a) => write!(f, "{}", a),
            ASFAttribute::ByteArray(a) => write!(f, "{}", a),
            ASFAttribute::Bool(a) => write!(f, "{}", a),
            ASFAttribute::DWord(a) => write!(f, "{}", a),
            ASFAttribute::QWord(a) => write!(f, "{}", a),
            ASFAttribute::Word(a) => write!(f, "{}", a),
            ASFAttribute::Guid(a) => write!(f, "{}", a),
        }
    }
}

/// Parse attribute from data using type registry
pub fn parse_attribute(type_id: u16, data: &[u8], dword: bool) -> Result<ASFAttribute> {
    match type_id {
        0x0000 => ASFUnicodeAttribute::parse(data, dword).map(ASFAttribute::Unicode),
        0x0001 => ASFByteArrayAttribute::parse(data, dword).map(ASFAttribute::ByteArray),
        0x0002 => ASFBoolAttribute::parse(data, dword).map(ASFAttribute::Bool),
        0x0003 => ASFDWordAttribute::parse(data, dword).map(ASFAttribute::DWord),
        0x0004 => ASFQWordAttribute::parse(data, dword).map(ASFAttribute::QWord),
        0x0005 => ASFWordAttribute::parse(data, dword).map(ASFAttribute::Word),
        0x0006 => ASFGuidAttribute::parse(data, dword).map(ASFAttribute::Guid),
        _ => Err(AudexError::ASF(ASFError::InvalidData(format!(
            "Unknown ASF attribute type: 0x{:04X}",
            type_id
        )))),
    }
}

/// Collection of ASF attributes
///
/// This provides both sequential and key-value interfaces
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFTags {
    /// Internal storage as list of (name, attribute) pairs
    items: Vec<(String, ASFAttribute)>,
}

impl ASFTags {
    /// Create a new empty ASF tags collection
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an attribute
    pub fn push(&mut self, item: (String, ASFAttribute)) {
        self.items.push(item);
    }

    /// Add an attribute with separate key and value
    pub fn add(&mut self, name: String, attribute: ASFAttribute) {
        self.items.push((name, attribute));
    }

    /// Extend with multiple items
    pub fn extend(&mut self, items: Vec<(String, ASFAttribute)>) {
        self.items.extend(items);
    }

    pub fn get(&self, key: &str) -> Vec<&ASFAttribute> {
        self.items
            .iter()
            .filter_map(|(k, v)| if k == key { Some(v) } else { None })
            .collect()
    }

    pub fn get_mut(&mut self, key: &str) -> Vec<&mut ASFAttribute> {
        self.items
            .iter_mut()
            .filter_map(|(k, v)| if k == key { Some(v) } else { None })
            .collect()
    }

    pub fn get_first(&self, key: &str) -> Option<&ASFAttribute> {
        self.items
            .iter()
            .find_map(|(k, v)| if k == key { Some(v) } else { None })
    }

    pub fn get_first_mut(&mut self, key: &str) -> Option<&mut ASFAttribute> {
        self.items
            .iter_mut()
            .find_map(|(k, v)| if k == key { Some(v) } else { None })
    }

    pub fn remove(&mut self, key: &str) -> Vec<ASFAttribute> {
        let mut removed = Vec::new();
        self.items.retain(|(k, v)| {
            if k == key {
                removed.push(v.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    pub fn set(&mut self, key: String, attributes: Vec<ASFAttribute>) {
        self.remove(&key);
        for attr in attributes {
            self.add(key.clone(), attr);
        }
    }

    pub fn set_single(&mut self, key: String, attribute: ASFAttribute) {
        self.remove(&key);
        self.add(key, attribute);
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.items.iter().any(|(k, _)| k == key)
    }

    pub fn keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.items.iter().map(|(k, _)| k.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    pub fn keys_owned(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.items.iter().map(|(k, _)| k.clone()).collect();
        keys.sort();
        keys.dedup();
        keys
    }

    pub fn values(&self) -> Vec<&ASFAttribute> {
        self.items.iter().map(|(_, v)| v).collect()
    }

    pub fn items(&self) -> &[(String, ASFAttribute)] {
        &self.items
    }

    pub fn items_mut(&mut self) -> &mut [(String, ASFAttribute)] {
        &mut self.items
    }

    pub fn as_dict(&self) -> HashMap<String, Vec<ASFAttribute>> {
        let mut dict = HashMap::new();
        for (key, attr) in &self.items {
            dict.entry(key.clone())
                .or_insert_with(Vec::new)
                .push(attr.clone());
        }
        dict
    }

    /// Pretty print
    pub fn pprint(&self) -> String {
        let mut result = String::new();
        let dict = self.as_dict();

        // Sort keys for consistent output
        let mut keys: Vec<_> = dict.keys().collect();
        keys.sort();

        for key in keys {
            let values = &dict[key];
            if values.len() == 1 {
                result.push_str(&format!("  {}: [{}]\n", key, values[0]));
            } else {
                result.push_str(&format!("  {}: [\n", key));
                for (i, value) in values.iter().enumerate() {
                    result.push_str(&format!("    {}: {}", i, value));
                    if let Some(lang) = value.language() {
                        result.push_str(&format!(" (language: {})", lang));
                    }
                    if let Some(stream) = value.stream() {
                        result.push_str(&format!(" (stream: {})", stream));
                    }
                    result.push_str(",\n");
                }
                result.push_str("  ]\n");
            }
        }

        if result.is_empty() {
            "{}\n".to_string()
        } else {
            format!("{{\n{}}}\n", result)
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn get_by_index(&self, index: usize) -> Option<&(String, ASFAttribute)> {
        self.items.get(index)
    }

    pub fn get_by_index_mut(&mut self, index: usize) -> Option<&mut (String, ASFAttribute)> {
        self.items.get_mut(index)
    }

    pub fn remove_by_index(&mut self, index: usize) -> Option<(String, ASFAttribute)> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    /// Insert at index
    pub fn insert(&mut self, index: usize, item: (String, ASFAttribute)) {
        if index <= self.items.len() {
            self.items.insert(index, item);
        }
    }

    /// Iterate over items
    pub fn iter(&self) -> impl Iterator<Item = &(String, ASFAttribute)> {
        self.items.iter()
    }

    /// Iterate mutably over items
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (String, ASFAttribute)> {
        self.items.iter_mut()
    }
}

impl fmt::Display for ASFTags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, attr) in &self.items {
            writeln!(f, "{}={}", name, attr)?;
        }
        Ok(())
    }
}

// Iterator trait implementation
impl IntoIterator for ASFTags {
    type Item = (String, ASFAttribute);
    type IntoIter = std::vec::IntoIter<(String, ASFAttribute)>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a> IntoIterator for &'a ASFTags {
    type Item = &'a (String, ASFAttribute);
    type IntoIter = std::slice::Iter<'a, (String, ASFAttribute)>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<'a> IntoIterator for &'a mut ASFTags {
    type Item = &'a mut (String, ASFAttribute);
    type IntoIter = std::slice::IterMut<'a, (String, ASFAttribute)>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut()
    }
}

// From trait implementations for easy construction
impl From<Vec<(String, ASFAttribute)>> for ASFTags {
    fn from(items: Vec<(String, ASFAttribute)>) -> Self {
        Self { items }
    }
}

impl From<HashMap<String, Vec<ASFAttribute>>> for ASFTags {
    fn from(dict: HashMap<String, Vec<ASFAttribute>>) -> Self {
        let mut tags = ASFTags::new();
        for (key, values) in dict {
            for value in values {
                tags.add(key.clone(), value);
            }
        }
        tags
    }
}

// Partial equality implementation
impl PartialEq for ASFTags {
    fn eq(&self, other: &Self) -> bool {
        if self.items.len() != other.items.len() {
            return false;
        }

        // Convert to dictionaries for comparison to handle order independence
        let self_dict = self.as_dict();
        let other_dict = other.as_dict();
        self_dict == other_dict
    }
}

impl Eq for ASFTags {}

impl Tags for ASFTags {
    fn get(&self, _key: &str) -> Option<&[String]> {
        // For ASF, this is complex since we store ASFAttribute, not String
        // A proper implementation would need to cache string conversions
        None
    }

    fn set(&mut self, key: &str, values: Vec<String>) {
        self.remove(key);
        for value in values {
            self.add(key.to_string(), ASFAttribute::unicode(value));
        }
    }

    fn remove(&mut self, key: &str) {
        ASFTags::remove(self, key);
    }

    fn keys(&self) -> Vec<String> {
        ASFTags::keys_owned(self)
    }

    fn pprint(&self) -> String {
        let mut result = String::new();
        let mut keys: Vec<_> = self.keys();
        keys.sort();

        for key in keys {
            let values = ASFTags::get(self, key);
            for value in values {
                result.push_str(&format!("{}={}\n", key, value));
            }
        }

        result
    }
}

use crate::tags::Metadata;

impl Metadata for ASFTags {
    type Error = crate::AudexError;

    fn new() -> Self {
        ASFTags::new()
    }

    fn load_from_fileobj(_filething: &mut crate::util::AnyFileThing) -> crate::Result<Self> {
        Err(crate::AudexError::NotImplementedMethod(
            "load_from_fileobj not implemented for ASFTags".to_string(),
        ))
    }

    fn save_to_fileobj(&self, _filething: &mut crate::util::AnyFileThing) -> crate::Result<()> {
        Err(crate::AudexError::NotImplementedMethod(
            "save_to_fileobj not implemented for ASFTags".to_string(),
        ))
    }

    fn delete_from_fileobj(_filething: &mut crate::util::AnyFileThing) -> crate::Result<()> {
        Err(crate::AudexError::NotImplementedMethod(
            "delete_from_fileobj not implemented for ASFTags".to_string(),
        ))
    }
}

impl crate::tags::MetadataFields for ASFTags {
    fn artist(&self) -> Option<&String> {
        // Since ASFTags stores ASFAttribute, not String, we need to return None
        // A proper implementation would cache converted values
        None
    }

    fn set_artist(&mut self, artist: String) {
        self.remove("Author");
        self.add("Author".to_string(), ASFAttribute::unicode(artist));
    }

    fn album(&self) -> Option<&String> {
        None
    }

    fn set_album(&mut self, album: String) {
        self.remove("WM/AlbumTitle");
        self.add("WM/AlbumTitle".to_string(), ASFAttribute::unicode(album));
    }

    fn title(&self) -> Option<&String> {
        None
    }

    fn set_title(&mut self, title: String) {
        self.remove("Title");
        self.add("Title".to_string(), ASFAttribute::unicode(title));
    }

    fn track_number(&self) -> Option<u32> {
        self.get_first("WM/TrackNumber")
            .and_then(|attr| match attr {
                ASFAttribute::DWord(d) => Some(d.value),
                ASFAttribute::Unicode(u) => u.value.parse().ok(),
                _ => None,
            })
    }

    fn set_track_number(&mut self, track: u32) {
        self.remove("WM/TrackNumber");
        self.add("WM/TrackNumber".to_string(), ASFAttribute::dword(track));
    }

    fn date(&self) -> Option<&String> {
        None
    }

    fn set_date(&mut self, date: String) {
        self.remove("WM/Year");
        self.add("WM/Year".to_string(), ASFAttribute::unicode(date));
    }

    fn genre(&self) -> Option<&String> {
        None
    }

    fn set_genre(&mut self, genre: String) {
        self.remove("WM/Genre");
        self.add("WM/Genre".to_string(), ASFAttribute::unicode(genre));
    }
}

/// Picture type constants for WM/Picture attribute
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum ASFPictureType {
    Other = 0x00,
    FileIcon32x32 = 0x01,
    OtherFileIcon = 0x02,
    FrontCover = 0x03,
    BackCover = 0x04,
    LeafletPage = 0x05,
    Media = 0x06,
    LeadArtist = 0x07,
    Artist = 0x08,
    Conductor = 0x09,
    Band = 0x0A,
    Composer = 0x0B,
    Lyricist = 0x0C,
    RecordingLocation = 0x0D,
    DuringRecording = 0x0E,
    DuringPerformance = 0x0F,
    VideoScreenCapture = 0x10,
    Fish = 0x11,
    Illustration = 0x12,
    BandLogotype = 0x13,
    PublisherLogotype = 0x14,
}

impl ASFPictureType {
    /// Convert from byte value
    pub fn from_u8(value: u8) -> Self {
        match value {
            0x00 => ASFPictureType::Other,
            0x01 => ASFPictureType::FileIcon32x32,
            0x02 => ASFPictureType::OtherFileIcon,
            0x03 => ASFPictureType::FrontCover,
            0x04 => ASFPictureType::BackCover,
            0x05 => ASFPictureType::LeafletPage,
            0x06 => ASFPictureType::Media,
            0x07 => ASFPictureType::LeadArtist,
            0x08 => ASFPictureType::Artist,
            0x09 => ASFPictureType::Conductor,
            0x0A => ASFPictureType::Band,
            0x0B => ASFPictureType::Composer,
            0x0C => ASFPictureType::Lyricist,
            0x0D => ASFPictureType::RecordingLocation,
            0x0E => ASFPictureType::DuringRecording,
            0x0F => ASFPictureType::DuringPerformance,
            0x10 => ASFPictureType::VideoScreenCapture,
            0x11 => ASFPictureType::Fish,
            0x12 => ASFPictureType::Illustration,
            0x13 => ASFPictureType::BandLogotype,
            0x14 => ASFPictureType::PublisherLogotype,
            _ => ASFPictureType::Other,
        }
    }

    /// Convert to byte value
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// WM/Picture attribute structure
///
/// Represents an embedded image in ASF files, typically album cover art.
/// The structure follows the WM/Picture specification:
/// - Picture type (1 byte)
/// - Data size (4 bytes, little-endian)
/// - MIME type (UTF-16 LE null-terminated string)
/// - Description (UTF-16 LE null-terminated string)
/// - Picture data (binary)
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ASFPicture {
    /// Picture type (e.g., front cover, back cover, etc.)
    pub picture_type: ASFPictureType,
    /// MIME type of the image (e.g., "image/jpeg", "image/png")
    pub mime_type: String,
    /// Description of the picture
    pub description: String,
    /// Raw picture data
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::bytes_as_base64")
    )]
    pub data: Vec<u8>,
}

impl ASFPicture {
    /// Create a new ASF picture
    pub fn new(
        picture_type: ASFPictureType,
        mime_type: String,
        description: String,
        data: Vec<u8>,
    ) -> Self {
        Self {
            picture_type,
            mime_type,
            description,
            data,
        }
    }

    /// Parse WM/Picture from byte array
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            return Err(AudexError::ASF(ASFError::InvalidData(
                "WM/Picture data too short".to_string(),
            )));
        }

        let mut pos = 0;

        // Parse picture type (1 byte)
        let picture_type = ASFPictureType::from_u8(data[pos]);
        pos += 1;

        // Parse data size (4 bytes, little-endian)
        if pos + 4 > data.len() {
            return Err(AudexError::ASF(ASFError::InvalidData(
                "WM/Picture missing data size".to_string(),
            )));
        }
        let data_size = ASFUtil::parse_u32_le(&data[pos..pos + 4])? as usize;
        pos += 4;

        // Validate data_size early: if the declared picture data size
        // already exceeds the remaining buffer, reject immediately
        // before spending time scanning MIME and description strings.
        if data_size > data.len().saturating_sub(pos) {
            return Err(AudexError::ASF(ASFError::InvalidData(
                "WM/Picture data_size exceeds available buffer".to_string(),
            )));
        }

        // Parse MIME type (UTF-16 LE null-terminated)
        let mime_start = pos;
        while pos + 1 < data.len() {
            let char_bytes = [data[pos], data[pos + 1]];
            if char_bytes == [0, 0] {
                // Null terminator found
                break;
            }
            pos += 2;
        }

        if pos + 1 >= data.len() {
            return Err(AudexError::ASF(ASFError::InvalidData(
                "WM/Picture MIME type not null-terminated".to_string(),
            )));
        }

        let mime_type = ASFUtil::parse_utf16_le(&data[mime_start..pos])?;
        pos += 2; // Skip null terminator

        // Parse description (UTF-16 LE null-terminated)
        let desc_start = pos;
        while pos + 1 < data.len() {
            let char_bytes = [data[pos], data[pos + 1]];
            if char_bytes == [0, 0] {
                // Null terminator found
                break;
            }
            pos += 2;
        }

        if pos + 1 >= data.len() {
            return Err(AudexError::ASF(ASFError::InvalidData(
                "WM/Picture description not null-terminated".to_string(),
            )));
        }

        let description = ASFUtil::parse_utf16_le(&data[desc_start..pos])?;
        pos += 2; // Skip null terminator

        // Parse picture data
        if pos + data_size > data.len() {
            return Err(AudexError::ASF(ASFError::InvalidData(
                "WM/Picture data truncated".to_string(),
            )));
        }

        crate::limits::ParseLimits::default()
            .check_image_size(data_size as u64, "ASF WM/Picture image")?;

        let picture_data = data[pos..pos + data_size].to_vec();

        Ok(Self {
            picture_type,
            mime_type,
            description,
            data: picture_data,
        })
    }

    /// Render WM/Picture to byte array.
    ///
    /// Returns an error if the picture data exceeds the maximum size
    /// representable by the 4-byte length field (u32::MAX).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut result = Vec::new();

        // Picture type (1 byte)
        result.push(self.picture_type.to_u8());

        // Data size (4 bytes, little-endian) — guard against silent truncation
        let data_len = u32::try_from(self.data.len()).map_err(|_| {
            AudexError::ASF(ASFError::InvalidData(format!(
                "WM/Picture data length {} exceeds maximum of {}",
                self.data.len(),
                u32::MAX
            )))
        })?;
        result.extend_from_slice(&data_len.to_le_bytes());

        // MIME type (UTF-16 LE null-terminated)
        result.extend_from_slice(&ASFUtil::encode_utf16_le(&self.mime_type));

        // Description (UTF-16 LE null-terminated)
        result.extend_from_slice(&ASFUtil::encode_utf16_le(&self.description));

        // Picture data
        result.extend_from_slice(&self.data);

        Ok(result)
    }

    pub fn size(&self) -> usize {
        // Picture type (1) + data size (4) + MIME (variable) + description (variable) + data
        let mime_size = ASFUtil::encode_utf16_le(&self.mime_type).len();
        let desc_size = ASFUtil::encode_utf16_le(&self.description).len();
        1 + 4 + mime_size + desc_size + self.data.len()
    }
}

/// Helper functions for working with WM/Picture attributes
impl ASFAttribute {
    /// Create a picture attribute from ASFPicture.
    ///
    /// Returns an error if the picture data is too large for the WM/Picture
    /// binary format (data must fit within a 4-byte length field).
    pub fn picture(picture: ASFPicture) -> Result<Self> {
        Ok(ASFAttribute::ByteArray(ASFByteArrayAttribute::new(
            picture.to_bytes()?,
        )))
    }

    /// Try to parse this attribute as a WM/Picture
    pub fn as_picture(&self) -> Option<ASFPicture> {
        match self {
            ASFAttribute::ByteArray(byte_array) => ASFPicture::from_bytes(&byte_array.value).ok(),
            _ => None,
        }
    }

    /// Check if this attribute is a valid WM/Picture
    pub fn is_picture(&self) -> bool {
        self.as_picture().is_some()
    }
}

/// Content Description Object field names
pub const CONTENT_DESCRIPTION_NAMES: &[&str] =
    &["Title", "Author", "Copyright", "Description", "Rating"];

/// Helper function to create ASF attribute from string
///
/// This function attempts to automatically detect the appropriate ASF attribute type
/// based on the content of the string value.
pub fn asf_value_from_string(value: &str) -> ASFAttribute {
    // Try to auto-detect the best ASF type from the string value.
    // Check bool first
    if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false") {
        return ASFAttribute::bool(value.eq_ignore_ascii_case("true"));
    }
    // Try numeric types in order of size
    if let Ok(n) = value.parse::<u64>() {
        if n <= u16::MAX as u64 {
            return ASFAttribute::word(n as u16);
        } else if n <= u32::MAX as u64 {
            return ASFAttribute::dword(n as u32);
        } else {
            return ASFAttribute::qword(n);
        }
    }
    // Default to Unicode
    ASFAttribute::unicode(value.to_string())
}

/// Create ASF attribute with specific type
pub fn asf_value_with_type(value: &str, attr_type: ASFAttributeType) -> Result<ASFAttribute> {
    match attr_type {
        ASFAttributeType::Unicode => Ok(ASFAttribute::unicode(value.to_string())),
        ASFAttributeType::ByteArray => Ok(ASFAttribute::byte_array(value.as_bytes().to_vec())),
        ASFAttributeType::Bool => {
            match value.parse::<bool>() {
                Ok(b) => Ok(ASFAttribute::bool(b)),
                Err(_) => {
                    // Try parsing as integer
                    match value.parse::<i32>() {
                        Ok(i) => Ok(ASFAttribute::bool(i != 0)),
                        Err(_) => Err(AudexError::ASF(ASFError::InvalidData(format!(
                            "Cannot parse '{}' as bool",
                            value
                        )))),
                    }
                }
            }
        }
        ASFAttributeType::DWord => match value.parse::<u32>() {
            Ok(d) => Ok(ASFAttribute::dword(d)),
            Err(_) => Err(AudexError::ASF(ASFError::InvalidData(format!(
                "Cannot parse '{}' as DWORD",
                value
            )))),
        },
        ASFAttributeType::QWord => match value.parse::<u64>() {
            Ok(q) => Ok(ASFAttribute::qword(q)),
            Err(_) => Err(AudexError::ASF(ASFError::InvalidData(format!(
                "Cannot parse '{}' as QWORD",
                value
            )))),
        },
        ASFAttributeType::Word => match value.parse::<u16>() {
            Ok(w) => Ok(ASFAttribute::word(w)),
            Err(_) => Err(AudexError::ASF(ASFError::InvalidData(format!(
                "Cannot parse '{}' as WORD",
                value
            )))),
        },
        ASFAttributeType::Guid => {
            // Try to parse as GUID string or treat as raw bytes
            if value.len() == 36 {
                match ASFUtil::guid_to_bytes(value) {
                    Ok(guid) => Ok(ASFAttribute::guid(guid)),
                    Err(_) => Ok(ASFAttribute::byte_array(value.as_bytes().to_vec())),
                }
            } else {
                Ok(ASFAttribute::byte_array(value.as_bytes().to_vec()))
            }
        }
    }
}

/// Validation framework for key-specific type constraints
///
/// This provides validation logic similar to tag validation
pub struct ASFTagValidator;

impl ASFTagValidator {
    /// Validate and potentially convert a value for a specific key
    pub fn validate_and_convert(key: &str, value: &str) -> Result<ASFAttribute> {
        match key {
            // Numeric fields that should be DWORDs
            "WM/TrackNumber" | "WM/Track" | "WM/PartOfSet" | "WM/Disc" => {
                match value.parse::<u32>() {
                    Ok(num) => Ok(ASFAttribute::dword(num)),
                    Err(_) => Err(AudexError::ASF(ASFError::InvalidData(format!(
                        "Key '{}' requires a numeric value, got: '{}'",
                        key, value
                    )))),
                }
            }

            // Boolean fields
            "IsVBR" => {
                match value.parse::<bool>() {
                    Ok(b) => Ok(ASFAttribute::bool(b)),
                    Err(_) => {
                        // Try parsing as integer
                        match value.parse::<i32>() {
                            Ok(i) => Ok(ASFAttribute::bool(i != 0)),
                            Err(_) => Err(AudexError::ASF(ASFError::InvalidData(format!(
                                "Key '{}' requires a boolean value, got: '{}'",
                                key, value
                            )))),
                        }
                    }
                }
            }

            // Date fields - typically strings but could be years as numbers
            "WM/Year" | "WM/OriginalReleaseYear" => {
                // Try as year (DWORD) first, then fall back to string
                if let Ok(year) = value.parse::<u32>() {
                    if year > 1900 && year < 3000 {
                        return Ok(ASFAttribute::dword(year));
                    }
                }
                Ok(ASFAttribute::unicode(value.to_string()))
            }

            // Most other fields are Unicode strings
            _ => Ok(ASFAttribute::unicode(value.to_string())),
        }
    }

    /// Check if a key typically expects a specific type
    pub fn get_expected_type(key: &str) -> Option<ASFAttributeType> {
        match key {
            "WM/TrackNumber" | "WM/Track" | "WM/PartOfSet" | "WM/Disc" => {
                Some(ASFAttributeType::DWord)
            }
            "IsVBR" => Some(ASFAttributeType::Bool),
            _ => None, // Most fields are flexible
        }
    }
}

/// Enhanced ASFTags with validation
impl ASFTags {
    /// Set values with automatic validation and conversion
    pub fn set_validated(&mut self, key: &str, values: Vec<&str>) -> Result<()> {
        let mut attributes = Vec::new();

        for value in values {
            let attr = ASFTagValidator::validate_and_convert(key, value)?;
            attributes.push(attr);
        }

        self.set(key.to_string(), attributes);
        Ok(())
    }

    /// Set single value with validation
    pub fn set_single_validated(&mut self, key: &str, value: &str) -> Result<()> {
        let attr = ASFTagValidator::validate_and_convert(key, value)?;
        self.set_single(key.to_string(), attr);
        Ok(())
    }
}
