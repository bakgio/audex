//! Advanced Systems Format (ASF) container support
//!
//! This module provides comprehensive support for the Advanced Systems Format (ASF),
//! a Microsoft container format primarily used for Windows Media Audio (WMA) and
//! Windows Media Video (WMV) files. ASF is a flexible, extensible format that supports
//! streaming media and various codecs.
//!
//! # Overview
//!
//! ASF files are structured as a collection of **objects** (similar to atoms/chunks
//! in other container formats). Each object has a 128-bit GUID identifier, size,
//! and data. Objects can contain other objects, forming a hierarchical structure.
//!
//! ## File Extensions
//!
//! - `.wma` - Windows Media Audio (audio only)
//! - `.wmv` - Windows Media Video (video with optional audio)
//! - `.asf` - Generic ASF container
//!
//! ## Key Objects
//!
//! - **Header Object**: Contains file metadata, codec info, and stream properties
//! - **Data Object**: Contains the actual audio/video packet data
//! - **Index Objects**: Optional objects for seeking support
//! - **Content Description Object**: Basic metadata (title, artist, etc.)
//! - **Extended Content Description Object**: Extended metadata with typed attributes
//!
//! # Metadata Support
//!
//! ASF supports rich metadata through several object types:
//!
//! ## Content Description (Basic Fields)
//!
//! Five standard string fields stored in the Content Description Object:
//! - Title
//! - Author (Artist)
//! - Copyright
//! - Description
//! - Rating
//!
//! ## Extended Content Description
//!
//! Arbitrary key-value pairs with typed values:
//! - Unicode strings (most common)
//! - Byte arrays (binary data, images)
//! - Boolean values
//! - DWORD (32-bit unsigned integers)
//! - QWORD (64-bit unsigned integers)
//! - WORD (16-bit unsigned integers)
//! - GUID (128-bit identifiers)
//!
//! # Basic Usage
//!
//! ## Reading ASF/WMA Files
//!
//! ```no_run
//! use audex::FileType;
//! use audex::asf::ASF;
//!
//! // Load a WMA file
//! let audio = ASF::load("song.wma").unwrap();
//!
//! // Access stream information (length is in seconds as f64)
//! println!("Duration: {:.2} seconds", audio.info.length);
//! println!("Bitrate: {} kbps", audio.info.bitrate / 1000);
//! println!("Sample rate: {} Hz", audio.info.sample_rate);
//! println!("Channels: {}", audio.info.channels);
//!
//! // Access tags (tags is directly available, not wrapped in Option)
//! let tags = &audio.tags;
//! if let Some(title) = tags.get_first("Title") {
//!     println!("Title: {}", title);
//! }
//! ```
//!
//! ## Writing Metadata
//!
//! ```no_run
//! use audex::FileType;
//! use audex::asf::{ASF, ASFTags, ASFAttribute};
//!
//! let mut audio = ASF::load("song.wma").unwrap();
//!
//! // Access and modify tags directly using ASFAttribute::unicode()
//! audio.tags.set_single("Title".to_string(), ASFAttribute::unicode("My Song".to_string()));
//! audio.tags.set_single("Author".to_string(), ASFAttribute::unicode("My Band".to_string()));
//! audio.tags.set_single("WM/AlbumTitle".to_string(), ASFAttribute::unicode("My Album".to_string()));
//! audio.tags.set_single("WM/Year".to_string(), ASFAttribute::unicode("2024".to_string()));
//!
//! // Save changes to the original file
//! audio.save().unwrap();
//! ```
//!
//! ## Adding Cover Art
//!
//! ```no_run
//! use audex::asf::{ASFTags, ASFPicture, ASFPictureType, ASFAttribute, ASFByteArrayAttribute};
//! use std::fs;
//!
//! let mut tags = ASFTags::new();
//!
//! // Read image file
//! let image_data = fs::read("cover.jpg").unwrap();
//!
//! // Create picture structure with correct field names
//! let picture = ASFPicture {
//!     picture_type: ASFPictureType::FrontCover,
//!     mime_type: "image/jpeg".to_string(),
//!     description: "Front Cover".to_string(),
//!     data: image_data,
//! };
//!
//! // Convert picture to bytes and add as byte array attribute
//! let picture_bytes = picture.to_bytes().unwrap();
//! let attr = ASFAttribute::ByteArray(ASFByteArrayAttribute::new(picture_bytes));
//! tags.set_single("WM/Picture".to_string(), attr);
//! ```
//!
//! # Attribute Types
//!
//! ASF attributes can have different data types identified by type codes:
//!
//! - `UNICODE` (0x0000): UTF-16LE encoded strings
//! - `BYTEARRAY` (0x0001): Raw binary data
//! - `BOOL` (0x0002): Boolean true/false
//! - `DWORD` (0x0003): 32-bit unsigned integer
//! - `QWORD` (0x0004): 64-bit unsigned integer
//! - `WORD` (0x0005): 16-bit unsigned integer
//! - `GUID` (0x0006): 128-bit GUID/UUID
//!
//! # Standard Metadata Fields
//!
//! Common field names used in WMA files:
//!
//! - `Title`: Track title (Content Description)
//! - `Author`: Artist name (Content Description)
//! - `WM/AlbumTitle`: Album name
//! - `WM/AlbumArtist`: Album artist
//! - `WM/Year`: Release year
//! - `WM/Genre`: Genre
//! - `WM/TrackNumber`: Track number
//! - `WM/PartOfSet`: Disc number
//! - `WM/Composer`: Composer name
//! - `WM/Publisher`: Publisher/label
//! - `WM/Picture`: Embedded cover art
//!
//! # GUID Identifiers
//!
//! ASF objects are identified by 128-bit GUIDs. Common GUIDs are available
//! through the `ASFGUIDs` constant. Each object type has a unique GUID,
//! allowing format extensions without breaking compatibility.
//!
//! # Codec Support
//!
//! ASF containers can hold various codecs:
//! - **WMA** (Windows Media Audio): Lossy audio codec
//! - **WMA Pro**: Enhanced WMA with surround sound support
//! - **WMA Lossless**: Lossless audio compression
//! - **WMV** (Windows Media Video): Video codec
//! - And others through extensible codec framework
//!
//! # See Also
//!
//! - `ASF`: Main ASF file structure
//! - `ASFInfo`: Stream information structure
//! - `ASFTags`: Metadata container
//! - `ASFAttribute`: Individual metadata attribute
//! - `ASFGUIDs`: Object GUID constants

pub use attrs::{
    ASFAttribute, ASFAttributeType, ASFBoolAttribute, ASFByteArrayAttribute, ASFDWordAttribute,
    ASFGuidAttribute, ASFPicture, ASFPictureType, ASFQWordAttribute, ASFTags, ASFUnicodeAttribute,
    ASFWordAttribute, CONTENT_DESCRIPTION_NAMES, asf_value_from_string, asf_value_with_type,
    parse_attribute,
};
pub use file::{ASF, ASFInfo};
pub use util::{ASFCodecs, ASFError, ASFGUIDs};

/// Unicode string attribute type (0x0000)
///
/// Represents UTF-16LE encoded string data. This is the most common
/// attribute type, used for text metadata like titles, artists, and comments.
pub const UNICODE: u16 = 0x0000;

/// Byte array attribute type (0x0001)
///
/// Represents raw binary data. Used for embedded images, arbitrary data,
/// and other non-text content.
pub const BYTEARRAY: u16 = 0x0001;

/// Boolean attribute type (0x0002)
///
/// Represents a true/false value. Stored as a 32-bit value in Extended Content
/// Description context, or a 16-bit value in Metadata/MetadataLibrary context.
/// Exactly 1 = true and all other values (including other non-zero) = false.
pub const BOOL: u16 = 0x0002;

/// Double Word (32-bit) attribute type (0x0003)
///
/// Represents an unsigned 32-bit integer value.
/// Used for counters, IDs, and other numeric data.
pub const DWORD: u16 = 0x0003;

/// Quad Word (64-bit) attribute type (0x0004)
///
/// Represents an unsigned 64-bit integer value.
/// Used for large numbers, timestamps, and file sizes.
pub const QWORD: u16 = 0x0004;

/// Word (16-bit) attribute type (0x0005)
///
/// Represents an unsigned 16-bit integer value.
/// Less commonly used than DWORD or QWORD.
pub const WORD: u16 = 0x0005;

/// GUID attribute type (0x0006)
///
/// Represents a 128-bit Globally Unique Identifier.
/// Used for format identifiers and object references.
pub const GUID: u16 = 0x0006;

// Sub-modules
pub mod attrs;
pub mod file;
pub mod objects;
pub mod util;
