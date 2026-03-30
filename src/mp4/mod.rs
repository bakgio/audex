//! # MP4/M4A Format Support
//!
//! This module provides comprehensive support for reading and writing MPEG-4 audio files
//! with iTunes-style metadata, commonly known as M4A, M4B (audiobooks), and M4P (protected) files.
//!
//! ## Overview
//!
//! The MP4 container format (ISO/IEC 14496-12) is widely used for audio distribution,
//! particularly through Apple's ecosystem. This implementation supports:
//! - **Audio codecs**: AAC, ALAC (Apple Lossless), and other MPEG-4 audio formats
//! - **Metadata**: iTunes-style tags using the `©` atoms and custom metadata
//! - **Chapters**: Chapter markers for audiobooks and podcasts
//! - **Artwork**: Embedded cover art (commonly PNG/JPEG, with GIF/BMP support as well)
//! - **Atom structure**: Full atom tree parsing and manipulation
//!
//! ## File Extensions
//!
//! MP4 audio files use various extensions:
//! - **M4A**: Standard MPEG-4 audio (typically AAC)
//! - **M4B**: MPEG-4 audiobook files (with chapter markers)
//! - **M4P**: Protected MPEG-4 audio (FairPlay DRM)
//! - **MP4**: Generic MPEG-4 container (can contain audio)
//!
//! ## Atom Structure
//!
//! MP4 files are organized as a hierarchy of "atoms" (also called "boxes"):
//! - **ftyp**: File type identification
//! - **moov**: Movie metadata container
//!   - **udta**: User data (metadata)
//!     - **meta**: Metadata container
//!       - **ilst**: iTunes-style tag list
//!   - **trak**: Track information
//!     - **mdia**: Media information
//!
//! ## Basic Usage
//!
//! ```no_run
//! use audex::mp4::MP4;
//! use audex::{FileType, Tags};
//!
//! // Load an M4A file
//! let mut mp4 = MP4::load("song.m4a").unwrap();
//!
//! // Access audio information (most fields are Option types; codec/codec_description are String)
//! if let Some(duration) = mp4.info.length {
//!     println!("Duration: {:.2} seconds", duration.as_secs_f64());
//! }
//! if let Some(bitrate) = mp4.info.bitrate {
//!     println!("Bitrate: {} bps", bitrate);
//! }
//! if let Some(sample_rate) = mp4.info.sample_rate {
//!     println!("Sample rate: {} Hz", sample_rate);
//! }
//! println!("Codec: {}", mp4.info.codec_description);
//!
//! // Read iTunes tags using the Tags trait
//! if let Some(ref tags) = mp4.tags {
//!     if let Some(title) = tags.get("\u{00A9}nam") {  // ©nam = title
//!         println!("Title: {:?}", title);
//!     }
//!     if let Some(artist) = tags.get("\u{00A9}ART") {  // ©ART = artist
//!         println!("Artist: {:?}", artist);
//!     }
//! }
//!
//! // Modify tags and save using the Tags trait set method
//! if let Some(ref mut tags) = mp4.tags {
//!     tags.set("\u{00A9}nam", vec!["New Title".to_string()]);
//! }
//! mp4.save().unwrap();
//! ```
//!
//! ## iTunes Tag Atoms
//!
//! Common iTunes metadata atoms (using © character, U+00A9):
//! - **©nam**: Title
//! - **©ART**: Artist
//! - **©alb**: Album
//! - **©day**: Release date
//! - **©gen**: Genre
//! - **trkn**: Track number
//! - **disk**: Disc number
//! - **©wrt**: Composer
//! - **©grp**: Grouping
//! - **©cmt**: Comment
//! - **covr**: Cover artwork
//!
//! ## Chapters Support
//!
//! Audiobook files (M4B) often contain chapter markers:
//!
//! ```no_run
//! use audex::mp4::MP4;
//! use audex::FileType;
//!
//! let mp4 = MP4::load("audiobook.m4b").unwrap();
//!
//! // Access chapter information if available
//! if let Some(ref chapters) = mp4.chapters {
//!     for (i, chapter) in chapters.chapters.iter().enumerate() {
//!         println!("Chapter {}: {} at {:?}",
//!             i + 1, chapter.title, chapter.start);
//!     }
//! }
//! ```
//!
//! ## See Also
//!
//! - `MP4` - Main struct for MP4 file handling
//! - `MP4Info` - Audio stream information
//! - `MP4Tags` - iTunes-style metadata tags
//! - `MP4Atom` - Atom structure and parsing
//! - `Atoms` - Atom tree navigation

// Error types
#[allow(unused_imports)]
pub use error::{
    EasyMP4KeyError, MP4Error, MP4MetadataError, MP4MetadataValueError, MP4NoTrackError,
    MP4StreamInfoError,
};

// Core types
pub use atom::{AtomType, Atoms, MP4Atom};
pub use file::{AtomDataType, Chapter, MP4, MP4Chapters, MP4Cover, MP4FreeForm, MP4Info, MP4Tags};
// MP4Info is already exported above

// Utility functions
pub use util::{clear, key2name, name2key};

// Async utility functions (feature-gated)
#[cfg(feature = "async")]
pub use util::clear_async;

// Module declarations
pub mod as_entry;
pub mod atom;
pub mod error;
pub mod file;
pub mod util;
