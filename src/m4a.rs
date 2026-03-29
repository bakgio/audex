//! M4A (MPEG-4 Audio) format compatibility module.
//!
//! This module provides a compatibility layer for code that specifically works with M4A files.
//! It re-exports all MP4 and EasyMP4 structs and functions with M4A-specific type aliases.
//!
//! # M4A vs MP4
//!
//! M4A is simply a renamed MP4 container specifically for audio files (no video track).
//! Internally, this module uses the exact same implementation as the [`mp4`](crate::mp4) module.
//! The file format, structure, and metadata handling are identical.
//!
//! **File Extensions:**
//! - `.m4a` - MPEG-4 Audio (typically AAC or ALAC codec)
//! - `.m4b` - MPEG-4 Audiobook (includes chapter markers)
//! - `.m4p` - MPEG-4 Protected (DRM-protected audio)
//! - `.mp4` - Generic MPEG-4 container (may contain audio and/or video)
//!
//! # When to Use This Module
//!
//! Use this module if:
//! - Your code specifically targets M4A files
//! - You want type names that match the file extension (M4A vs MP4)
//! - You're porting code from other libraries that use "M4A" terminology
//!
//! Otherwise, the [`mp4`](crate::mp4) module works identically for all MPEG-4 audio files.
//!
//! # Examples
//!
//! ## Basic M4A File Loading
//!
//! ```no_run
//! use audex::m4a::M4A;
//! use audex::FileType;
//! use audex::Tags;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let audio = M4A::load("song.m4a")?;
//!
//! // Access audio stream information
//! println!("Duration: {:?}", audio.info().length);
//! println!("Bitrate: {:?} kbps", audio.info().bitrate.map(|b| b / 1000));
//! println!("Codec: {}", audio.info().codec);
//!
//! // Access iTunes-style metadata
//! if let Some(ref tags) = audio.tags {
//!     if let Some(title) = tags.get("©nam") {
//!         println!("Title: {:?}", title);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Clearing All Metadata
//!
//! ```no_run
//! use audex::m4a;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! // Remove all iTunes metadata from an M4A file
//! m4a::clear("song.m4a")?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Simplified Metadata Access with EasyM4A
//!
//! Use `EasyM4A` for a simplified key-value interface:
//!
//! ```no_run
//! use audex::m4a::EasyM4A;
//! use audex::FileType;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let mut tags = EasyM4A::load("song.m4a")?;
//!
//! // Read standard tags using simple field names
//! if let Some(title) = tags.get("title") {
//!     println!("Title: {:?}", title);
//! }
//!
//! if let Some(artist) = tags.get("artist") {
//!     println!("Artist: {:?}", artist);
//! }
//!
//! // Modify tags
//! tags.set("album", vec!["Greatest Hits".to_string()])?;
//! tags.set("date", vec!["2024".to_string()])?;
//!
//! // Save changes (saves to original file)
//! tags.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Working with Cover Art
//!
//! ```no_run
//! use audex::m4a::{M4A, MP4Cover};
//! use audex::FileType;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let mut audio = M4A::load("song.m4a")?;
//!
//! // Read existing cover art
//! if let Some(ref tags) = audio.tags {
//!     if let Some(covers) = tags.covers.first() {
//!         println!("Cover art: {} bytes, format: {:?}",
//!                  covers.data.len(), covers.imageformat);
//!     }
//! }
//!
//! // Add new cover art
//! let cover_data = std::fs::read("album_art.jpg")?;
//! let cover = MP4Cover::new_jpeg(cover_data);
//!
//! if let Some(ref mut tags) = audio.tags {
//!     tags.covers.clear();
//!     tags.covers.push(cover);
//! }
//!
//! // Save changes to the original file
//! audio.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Re-exported Types
//!
//! All types from the [`mp4`](crate::mp4) and [`easymp4`](crate::easymp4) modules are
//! re-exported here. Key aliases:
//!
//! - `M4A` = `MP4`
//! - `EasyM4A` = `EasyMP4`
//! - `EasyM4ATags` = `EasyMP4Tags`
//!
//! All other types (errors, atoms, chapters, etc.) are identical to the MP4 module.

pub use crate::mp4::{
    AtomDataType, AtomType, Atoms, Chapter, EasyMP4KeyError, MP4, MP4Atom, MP4Chapters, MP4Cover,
    MP4Error, MP4FreeForm, MP4Info, MP4MetadataError, MP4MetadataValueError, MP4NoTrackError,
    MP4StreamInfoError, MP4Tags, clear, key2name, name2key,
};

#[cfg(feature = "async")]
pub use crate::mp4::clear_async;

pub use crate::easymp4::{EasyMP4, EasyMP4Tags, KeyMapping, KeyRegistry, KeyType};

/// Alias for MP4 (compatibility name)
pub use crate::mp4::MP4 as M4A;

/// Alias for EasyMP4 (compatibility name)
pub use crate::easymp4::EasyMP4 as EasyM4A;

/// Alias for EasyMP4Tags (compatibility name)
pub use crate::easymp4::EasyMP4Tags as EasyM4ATags;
