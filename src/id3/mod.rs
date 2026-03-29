//! ID3v2 tag support for MP3 and other audio files
//!
//! This module provides comprehensive ID3v2 tag reading and writing capabilities
//! for audio files. ID3v2 is the de facto standard for metadata tagging in MP3 files,
//! though it can also be used with other formats like AIFF, WAV, and TTA.
//!
//! # Overview
//!
//! ID3v2 tags store metadata as a collection of **frames**, where each frame contains
//! a specific piece of information (title, artist, artwork, etc.). Unlike ID3v1 which
//! has fixed fields, ID3v2 is extensible and supports a wide variety of data types.
//!
//! ## Supported Versions
//!
//! This library supports reading and writing all major ID3v2 versions:
//!
//! - **ID3v2.2**: Legacy format with 3-character frame IDs (e.g., `TT2`, `TP1`, `PIC`)
//! - **ID3v2.3**: Most common format with 4-character frame IDs (e.g., `TIT2`, `TPE1`, `APIC`)
//! - **ID3v2.4**: Latest standard with UTF-8 support and improved features
//!
//! When writing tags, ID3v2.4 is used by default, but you can specify other versions
//! if needed for compatibility with older players.
//!
//! ## Frame Types
//!
//! ID3v2 frames are categorized into several types:
//!
//! ### Text Frames
//! Store textual information like title, artist, album, genre, etc.
//! - `TIT2`: Title/Song name
//! - `TPE1`: Lead artist/Performer
//! - `TALB`: Album title
//! - `TCON`: Genre/Content type
//! - `TRCK`: Track number
//! - `TYER`/`TDRC`: Year/Recording time
//! - And many more...
//!
//! ### URL Frames
//! Store web links and online resources.
//! - `WOAR`: Official artist webpage
//! - `WCOM`: Commercial information
//! - `WPAY`: Payment information
//!
//! ### Binary Frames
//! Store non-textual data.
//! - `APIC`: Attached picture (cover art)
//! - `GEOB`: General encapsulated object
//! - `PRIV`: Private data
//!
//! ### Special Frames
//! Provide advanced functionality.
//! - `COMM`: Comments with language and description
//! - `USLT`: Unsynchronized lyrics
//! - `SYLT`: Synchronized lyrics with timing
//! - `TXXX`: User-defined text frames
//! - `POPM`: Popularimeter (rating/play count)
//! - `RVA2`: Relative volume adjustment (ReplayGain)
//! - `CHAP`/`CTOC`: Chapter markers and table of contents
//!
//! # Basic Usage
//!
//! ## Reading Tags
//!
//! ```no_run
//! use audex::id3::{ID3Tags, load};
//!
//! // Load ID3 tags from a file
//! let tags = load("song.mp3").unwrap();
//!
//! // Get text frames
//! let title_frames = tags.getall("TIT2");
//! if let Some(title) = title_frames.first() {
//!     println!("Title: {}", title.description());
//! }
//!
//! // Get all frames of a certain type
//! for frame in tags.getall("COMM") {
//!     println!("Comment: {}", frame.description());
//! }
//! ```
//!
//! ## Writing Tags
//!
//! ```no_run
//! use audex::id3::ID3Tags;
//!
//! // Create new tag collection
//! let mut tags = ID3Tags::new();
//!
//! // Add text frames
//! tags.add_text_frame("TIT2", vec!["Song Title".to_string()]).unwrap();
//! tags.add_text_frame("TPE1", vec!["Artist Name".to_string()]).unwrap();
//! tags.add_text_frame("TALB", vec!["Album Name".to_string()]).unwrap();
//!
//! // Save to file
//! tags.save("song.mp3", 1, 4, None, None).unwrap();
//! ```
//!
//! ## Working with Pictures
//!
//! ```no_run
//! use audex::id3::{ID3Tags, APIC, PictureType};
//! use audex::id3::specs::TextEncoding;
//! use std::fs;
//!
//! // Create new ID3 tag collection
//! let mut tags = ID3Tags::new();
//!
//! // Read image data from file
//! let image_data = fs::read("cover.jpg").unwrap();
//!
//! // Create picture frame with proper String types
//! // Note: mime and desc parameters must be String, not &str
//! let picture = APIC::new(
//!     TextEncoding::Utf8,                  // Text encoding for description field
//!     "image/jpeg".to_string(),            // MIME type (must be String)
//!     PictureType::CoverFront,             // Picture type (front cover)
//!     "Album Cover".to_string(),           // Description (must be String)
//!     image_data,                          // Raw image bytes
//! );
//!
//! // Add the picture frame to the tag collection
//! tags.add(Box::new(picture));
//!
//! // Save tags to file (version 2.4)
//! tags.save("song.mp3", 1, 4, None, None).unwrap();
//! ```
//!
//! ## Working with Comments
//!
//! ```no_run
//! use audex::id3::{ID3Tags, COMM};
//! use audex::id3::specs::TextEncoding;
//!
//! // Create new ID3 tag collection
//! let mut tags = ID3Tags::new();
//!
//! // Create comment frame with language code and description
//! // COMM frames support multiple comments distinguished by description
//! let comment = COMM::new(
//!     TextEncoding::Utf8,                  // Text encoding for strings
//!     *b"eng",                             // ISO 639-2 language code (3 bytes)
//!     "Description".to_string(),           // Short content description
//!     "This is my comment".to_string(),    // Actual comment text
//! );
//!
//! // Add the comment frame to the tag collection
//! tags.add(Box::new(comment));
//! ```
//!
//! ## Modifying Existing Tags
//!
//! ```no_run
//! use audex::id3::load;
//!
//! // Load existing tags
//! let mut tags = load("song.mp3").unwrap();
//!
//! // Remove all frames of a specific type
//! tags.delall("TIT2");
//!
//! // Add new value
//! tags.add_text_frame("TIT2", vec!["New Title".to_string()]).unwrap();
//!
//! // Save changes
//! tags.save("song.mp3", 1, 4, None, None).unwrap();
//! ```
//!
//! # Advanced Features
//!
//! ## Unsynchronization
//!
//! ID3v2 supports unsynchronization to prevent false MPEG sync signals within tag data.
//! This is handled automatically during reading and writing.
//!
//! ## Extended Headers
//!
//! ID3v2.3 and 2.4 support extended headers for additional tag-level metadata.
//! This includes CRC checksums, restrictions, and tag update flags.
//!
//! ## Padding
//!
//! Tags can include padding to allow in-place updates without rewriting the entire file.
//! You can control padding size when saving tags.
//!
//! # Version Compatibility
//!
//! When reading tags, all versions are automatically detected and parsed. When writing:
//!
//! - **ID3v2.4** (default): Full Unicode support, improved date handling
//! - **ID3v2.3**: Better compatibility with older software
//! - **ID3v2.2**: Maximum compatibility, limited features
//!
//! Frame IDs are automatically converted between versions when possible.
//!
//! # Error Handling
//!
//! Operations return `Result<T, AudexError>` for proper error handling:
//!
//! ```no_run
//! use audex::id3::load;
//!
//! match load("song.mp3") {
//!     Ok(tags) => {
//!         // Process tags
//!     }
//!     Err(e) => {
//!         eprintln!("Failed to load tags: {}", e);
//!     }
//! }
//! ```
//!
//! # See Also
//!
//! - [`ID3Tags`](crate::id3::ID3Tags): Main container for ID3v2 frames
//! - `TextFrame`: Text-based frames (most common)
//! - `APIC`: Picture/artwork frames
//! - `COMM`: Comment frames
//! - Frame types and specifications in the ID3v2 standard

/// The ID3 file handler (loads/saves ID3 tags from/to files)
pub use file::{ID3, ID3FileType};
/// ID3v2 tag container holding all frames
pub use tags::{ID3SaveConfig, ID3Tags};

// Error types
pub use crate::{AudexError, Result};
/// Type alias for ID3 header-not-found errors (maps to `AudexError`)
pub type ID3NoHeaderError = crate::AudexError;
/// Type alias for ID3 unsynchronization errors (maps to `AudexError`)
pub type ID3BadUnsynchData = crate::AudexError;

// Frame types - ALL frame classes for complete API compatibility
pub use frames::{
    AENC,
    APIC,
    ASPI,
    // ID3v2.2 Special frames - complete set (legacy compatibility)
    BUF,
    CHAP,
    CNT,
    COM,
    COMM,
    COMR,
    CRA,
    CRM,
    CTOC,
    ENCR,
    EQU,
    EQU2,
    ETC,
    ETCO,
    FrameRegistry,

    GEO,
    GEOB,
    GRID,
    GRP1,
    IPL,
    LINK,
    LNK,
    MCDI,
    MCI,
    MLL,
    MLLT,
    MVIN,
    MVNM,
    OWNE,
    PCNT,
    PCST,

    PIC,
    POP,
    POPM,
    POSS,
    PRIV,
    RBUF,
    REV,
    RVA,
    RVA2,
    RVAD,
    RVRB,
    SEEK,
    SIGN,
    SLT,
    STC,
    // ID3v2.4/v2.3 Special frames - complete set
    SYLT,
    SYTC,
    // ID3v2.2 Text frames - complete set (legacy compatibility)
    TAL,
    // ID3v2.4/v2.3 Text frames - complete set
    TALB,
    TBP,
    TBPM,
    TCAT,
    TCM,
    TCMP,
    TCO,
    TCOM,
    TCON,

    TCOP,
    TCR,
    TDA,
    TDAT,
    TDEN,
    TDES,
    TDLY,
    TDOR,
    TDRC,
    TDRL,
    TDTG,
    TDY,
    TEN,
    TENC,
    TEXT,
    TFLT,
    TFT,
    TGID,
    TIM,
    TIME,
    TIPL,
    TIT1,
    TIT2,
    TIT3,
    TKE,
    TKEY,
    TKWD,
    TLA,
    TLAN,
    TLE,
    TLEN,
    TMCL,
    TMED,
    TMOO,
    TMT,
    TOA,
    TOAL,
    TOF,
    TOFN,
    TOL,
    TOLY,
    TOPE,
    TOR,
    TORY,
    TOT,
    TOWN,
    TP1,
    TP2,
    TP3,
    TP4,
    TPA,
    TPB,
    TPE1,
    TPE2,
    TPE3,
    TPE4,
    TPOS,
    TPRO,
    TPUB,
    TRC,
    TRCK,
    TRD,
    TRDA,
    TRK,
    TRSN,
    TRSO,
    TSI,
    TSIZ,
    TSO2,
    TSOA,
    TSOC,
    TSOP,
    TSOT,
    TSRC,
    TSS,
    TSSE,
    TSST,
    TT1,
    TT2,
    TT3,
    TXX,
    TXXX,
    TYE,

    TYER,
    // Core frame base types
    TextFrame,
    UFID,
    ULT,

    USER,
    USLT,
    // ID3v2.2 URL frames - complete set (legacy compatibility)
    WAF,
    WAR,
    WAS,
    WCM,
    // ID3v2.4/v2.3 URL frames - complete set
    WCOM,
    WCOP,
    WCP,
    WFED,
    WOAF,
    WOAR,
    WOAS,
    WORS,
    WPAY,
    WPB,
    WPUB,

    WXX,
    WXXX,
};

/// Alias for `APIC` (compatibility)
pub use APIC as PictureFrame;
/// Alias for `COMM` (compatibility)
pub use COMM as CommentFrame;

// Specifications and enums
pub use frames::PictureType;
pub use specs::{CTOCFlags, FrameFlags, FrameHeader, ID3Header, ID3TimeStamp, TextEncoding};

/// ID3v2 version identifier
///
/// Represents the different ID3v2 tag versions supported by this library.
/// Each version has different capabilities and compatibility characteristics.
///
/// # Versions
///
/// - **V2_2**: ID3v2.2.0 - Legacy format with 3-character frame IDs
/// - **V2_3**: ID3v2.3.0 - Most widely supported format
/// - **V2_4**: ID3v2.4.0 - Latest standard with UTF-8 and improved features
///
/// # Examples
///
/// ```
/// use audex::id3::ID3Version;
///
/// let version = ID3Version::V2_4;
/// assert_eq!(version, ID3Version::V2_4);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ID3Version {
    /// ID3v2.2.0: 3-character frame IDs (e.g., TT2, TP1, PIC)
    V2_2,
    /// ID3v2.3.0: 4-character frame IDs, widely compatible
    V2_3,
    /// ID3v2.4.0: Latest version with UTF-8 support
    V2_4,
}

/// Frame type identifier
///
/// Categorizes ID3v2 frames by their primary data type.
/// Used internally for frame classification and processing.
///
/// # Frame Categories
///
/// - **TextFrame**: Frames containing text data (e.g., TIT2, TPE1, TALB)
/// - **UrlFrame**: Frames containing URL data (e.g., WOAR, WCOM, WPAY)
/// - **Other**: Special frames with custom structures (e.g., APIC, COMM, PRIV)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameID {
    /// Text information frame
    TextFrame,
    /// URL link frame
    UrlFrame,
    /// Other frame type (binary, special structure, etc.)
    Other,
}

/// Text encoding byte identifier
///
/// Represents the encoding byte value used in ID3v2 frames to indicate
/// the character encoding of text data.
///
/// # Encoding Types
///
/// - **Latin1**: ISO-8859-1 encoding (single byte)
/// - **Utf16**: UTF-16 with BOM (byte order mark)
/// - **Utf16Be**: UTF-16 big-endian without BOM
/// - **Utf8**: UTF-8 encoding (ID3v2.4 only)
///
/// # Examples
///
/// ```
/// use audex::id3::EncodingByte;
///
/// // UTF-8 is recommended for ID3v2.4
/// let encoding = EncodingByte::Utf8;
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EncodingByte {
    /// ISO-8859-1 (Latin1) encoding - byte value 0x00
    Latin1,
    /// UTF-16 with BOM - byte value 0x01
    Utf16,
    /// UTF-16 big-endian without BOM - byte value 0x02
    Utf16Be,
    /// UTF-8 encoding (ID3v2.4 only) - byte value 0x03
    Utf8,
}

/// Convenience alias: re-exports [`TextFrame`] as `Frame` for simple usage.
///
/// Note: This is not a trait — it is a concrete type alias for `TextFrame`.
/// For the `Frame` trait (with `description()`, `text_values()`, etc.),
/// see [`frames::Frame`].
pub use frames::TextFrame as Frame;

// Specification types exported for compatibility
pub use specs::{
    ASPIIndexSpec, CTOCFlagsSpec, PictureTypeSpec, RVASpec, SynchronizedTextSpec, TimeStampSpec,
    VolumeAdjustmentSpec, VolumePeakSpec,
};

// Utility functions exported for testing
pub use util::*;

// Function aliases for compatibility with tests
pub use id3v1::make_id3v1_from_frames as MakeID3v1;
pub use id3v1::parse_id3v1_to_frames as ParseID3v1;

// Convenience functions

/// Create a new empty ID3Tags instance
///
/// Creates a new ID3v2.4 tag container with no frames. This is useful
/// when you want to create tags from scratch rather than loading from a file.
///
/// # Examples
///
/// ```
/// use audex::id3::new_id3_tags;
///
/// let mut tags = new_id3_tags();
/// tags.add_text_frame("TIT2", vec!["My Song".to_string()]).unwrap();
/// ```
pub fn new_id3_tags() -> ID3Tags {
    ID3Tags::new()
}

/// Remove all ID3 tags from a file
///
/// This function removes both ID3v2 (at the beginning) and ID3v1 (at the end)
/// tags from the specified audio file. The file is modified in-place.
///
/// # Parameters
///
/// * `filething` - Path to the audio file to clear tags from
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if the file cannot be accessed
/// or modified.
///
/// # Examples
///
/// ```no_run
/// use audex::id3::clear;
///
/// // Remove all ID3 tags from a file
/// clear("song.mp3").unwrap();
/// ```
pub fn clear<P: AsRef<std::path::Path>>(filething: P) -> Result<()> {
    ID3Tags::clear_file(filething, true, true)
}

/// Load ID3 tags from a file
///
/// Reads and parses ID3v2 tags from the specified audio file. Supports all
/// ID3v2 versions (2.2, 2.3, and 2.4) and automatically detects the version.
///
/// # Parameters
///
/// * `filething` - Path to the audio file to read tags from
///
/// # Returns
///
/// Returns an `ID3Tags` instance containing all parsed frames, or an error
/// if the file cannot be read or contains invalid tag data.
///
/// # Examples
///
/// ```no_run
/// use audex::id3::load;
///
/// // Load tags from file
/// let tags = load("song.mp3").unwrap();
///
/// // Access frames
/// for frame in tags.getall("TIT2") {
///     println!("Title: {}", frame.description());
/// }
/// ```
pub fn load<P: AsRef<std::path::Path>>(filething: P) -> Result<ID3Tags> {
    ID3Tags::load(filething, None, true, 4, true)
}

pub mod file;
pub mod frames;
pub mod id3v1;
pub mod specs;
pub mod tags;
pub mod util;

/// ID3v2 version constants
///
/// This module provides constants for the supported ID3v2 versions.
/// Each constant is a tuple of (minor_version, revision) values.
pub mod version {
    /// ID3v2.2.0 version identifier
    ///
    /// Legacy format with 3-character frame IDs. Limited features but
    /// maximum compatibility with very old players.
    ///
    /// Format: (minor_version=2, revision=0)
    pub const ID3V22: (u8, u8) = (2, 0);

    /// ID3v2.3.0 version identifier
    ///
    /// Most widely supported ID3v2 version. 4-character frame IDs,
    /// good balance of features and compatibility.
    ///
    /// Format: (minor_version=3, revision=0)
    pub const ID3V23: (u8, u8) = (3, 0);

    /// ID3v2.4.0 version identifier
    ///
    /// Latest ID3v2 standard with full UTF-8 support, improved date handling,
    /// and additional features. Used as default for new tags.
    ///
    /// Format: (minor_version=4, revision=0)
    pub const ID3V24: (u8, u8) = (4, 0);
}

/// ID3v2 header flag constants
///
/// This module defines bit flags used in the ID3v2 tag header to indicate
/// various tag-level features and properties.
pub mod flags {
    /// Unsynchronization flag (bit 7)
    ///
    /// Indicates that unsynchronization has been applied to prevent false
    /// MPEG sync signals within the tag data. When set, all 0xFF bytes
    /// followed by bytes >= 0xE0 have a 0x00 byte inserted between them.
    pub const UNSYNCHRONIZATION: u8 = 0x80;

    /// Extended header flag (bit 6)
    ///
    /// Indicates the presence of an extended header immediately after the
    /// main tag header. The extended header contains additional tag-level
    /// information like CRC-32, restrictions, and update flags.
    pub const EXTENDED_HEADER: u8 = 0x40;

    /// Experimental flag (bit 5)
    ///
    /// Indicates that the tag is in an experimental stage. Applications
    /// should be cautious when processing tags with this flag set, as
    /// the data might not follow the standard format exactly.
    pub const EXPERIMENTAL: u8 = 0x20;

    /// Footer present flag (bit 4)
    ///
    /// Indicates that a footer is present at the end of the tag. The footer
    /// is a copy of the header located after all frames and padding, allowing
    /// the tag to be found by scanning from the end of the file.
    pub const FOOTER_PRESENT: u8 = 0x10;
}
