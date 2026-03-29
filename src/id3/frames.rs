//! ID3v2 frame implementations
//!
//! This module contains comprehensive implementations for all ID3v2 frame types,
//! providing full compatibility with ID3v2.2, ID3v2.3, and ID3v2.4 standards.
//! Over 185+ frame types are supported, covering everything from basic text
//! information to complex binary data like pictures and chapter markers.
//!
//! # Frame Type Categories
//!
//! ## Text Frames (T***)
//! Standard text information frames that store textual metadata:
//! - `TIT2`: Title/Song name
//! - `TPE1`: Lead artist/Performer
//! - `TALB`: Album title
//! - `TCON`: Genre/Content type
//! - `TRCK`: Track number/Position in set
//! - `TYER`/`TDRC`: Year/Recording time
//! - And 80+ more text frames...
//!
//! ## URL Frames (W***)
//! Frames containing web links and online resources:
//! - `WOAR`: Official artist webpage
//! - `WCOM`: Commercial information URL
//! - `WPAY`: Payment URL
//! - `WXXX`: User-defined URL frames
//!
//! ## Picture Frames
//! - `APIC`: Attached picture (cover art, artist photos, etc.)
//! - `PIC`: ID3v2.2 legacy picture frame
//!
//! ## Comment and Lyrics Frames
//! Language-aware frames for textual content:
//! - `COMM`: Comments with language code and description
//! - `USLT`: Unsynchronized lyrics/text transcription
//! - `SYLT`: Synchronized lyrics with timing information
//!
//! ## Special Purpose Frames
//! - `PRIV`: Private data frames
//! - `GEOB`: General encapsulated objects
//! - `UFID`: Unique file identifiers
//! - `POPM`: Popularimeter (rating and play count)
//! - `PCNT`: Play counter
//! - `RVA2`: Relative volume adjustment (ReplayGain)
//! - `CHAP`/`CTOC`: Chapter markers and table of contents
//! - `USER`: Terms of use
//! - `OWNE`: Ownership information
//! - `COMR`: Commercial frames
//!
//! ## Legacy Frames (ID3v2.2)
//! Three-character frame IDs from the older ID3v2.2 standard:
//! - Automatically upgraded to 4-character IDs when saving
//! - Examples: `TT2` → `TIT2`, `TP1` → `TPE1`, `PIC` → `APIC`
//!
//! # Frame Structure
//!
//! All frames implement the [`Frame`] trait, which provides common functionality:
//! - `frame_id()`: Get the 4-character (or 3-character for v2.2) frame ID
//! - `to_data()`: Serialize frame to binary data
//! - `description()`: Get human-readable description of frame contents
//! - `text_values()`: Extract text values (for text-based frames)
//!
//! # Examples
//!
//! ## Creating a Text Frame
//!
//! ```
//! use audex::id3::TextFrame;
//!
//! // Create a title frame
//! let title = TextFrame::new(
//!     "TIT2".to_string(),
//!     vec!["My Song Title".to_string()]
//! );
//! ```
//!
//! ## Creating a Picture Frame
//!
//! ```no_run
//! use audex::id3::{APIC, PictureType};
//! use audex::id3::specs::TextEncoding;
//! use std::fs;
//!
//! // Read image file
//! let image_data = fs::read("cover.jpg").unwrap();
//!
//! // Create picture frame
//! let picture = APIC::new(
//!     TextEncoding::Utf8,
//!     "image/jpeg".to_string(),
//!     PictureType::CoverFront,
//!     "Front Cover".to_string(),
//!     image_data,
//! );
//! ```
//!
//! ## Creating a Comment Frame
//!
//! ```
//! use audex::id3::COMM;
//! use audex::id3::specs::TextEncoding;
//!
//! // Create comment with language code
//! let comment = COMM::new(
//!     TextEncoding::Utf8,
//!     *b"eng",  // English language code
//!     "Comment Description".to_string(),
//!     "This is my comment text".to_string(),
//! );
//! ```
//!
//! # Version Compatibility
//!
//! - **ID3v2.4**: Full support for all frame types with UTF-8 encoding
//! - **ID3v2.3**: Full support with automatic UTF-8 to UTF-16 conversion
//! - **ID3v2.2**: Read support with automatic upgrade to v2.3/v2.4 frame IDs
//!
//! Frame IDs are automatically converted between versions when needed:
//! - v2.2 → v2.3: `TT2` becomes `TIT2`, `TP1` becomes `TPE1`, etc.
//! - v2.2 → v2.4: Additional date frame consolidation (e.g., `TYE` → `TDRC`)
//!
//! # Text Encoding
//!
//! Text frames support multiple character encodings:
//! - **Latin-1** (ISO-8859-1): Single-byte encoding, compatible with all versions
//! - **UTF-16** with BOM: Wide character support, compatible with all versions
//! - **UTF-16BE**: Big-endian without BOM (v2.4 only)
//! - **UTF-8**: Modern encoding, recommended for v2.4 tags
//!
//! # Picture Types
//!
//! The `APIC` frame supports 21 different picture types defined by the ID3v2 standard,
//! including front/back covers, artist photos, and more. See [`PictureType`] for details.

use crate::id3::specs::{FrameFlags, FrameHeader, FrameProcessor, ID3TimeStamp, TextEncoding};
use crate::id3::util::JunkFrameRecovery;
use crate::{AudexError, Result};
use std::any::Any;
use std::fmt;

/// ID3v1 Genre table - standardized genre list from the ID3v1 specification
pub const ID3V1_GENRES: &[&str] = &[
    "Blues",
    "Classic Rock",
    "Country",
    "Dance",
    "Disco",
    "Funk",
    "Grunge",
    "Hip-Hop",
    "Jazz",
    "Metal",
    "New Age",
    "Oldies",
    "Other",
    "Pop",
    "R&B",
    "Rap",
    "Reggae",
    "Rock",
    "Techno",
    "Industrial",
    "Alternative",
    "Ska",
    "Death Metal",
    "Pranks",
    "Soundtrack",
    "Euro-Techno",
    "Ambient",
    "Trip-Hop",
    "Vocal",
    "Jazz+Funk",
    "Fusion",
    "Trance",
    "Classical",
    "Instrumental",
    "Acid",
    "House",
    "Game",
    "Sound Clip",
    "Gospel",
    "Noise",
    "Alt. Rock",
    "Bass",
    "Soul",
    "Punk",
    "Space",
    "Meditative",
    "Instrumental Pop",
    "Instrumental Rock",
    "Ethnic",
    "Gothic",
    "Darkwave",
    "Techno-Industrial",
    "Electronic",
    "Pop-Folk",
    "Eurodance",
    "Dream",
    "Southern Rock",
    "Comedy",
    "Cult",
    "Gangsta Rap",
    "Top 40",
    "Christian Rap",
    "Pop/Funk",
    "Jungle",
    "Native American",
    "Cabaret",
    "New Wave",
    "Psychedelic",
    "Rave",
    "Showtunes",
    "Trailer",
    "Lo-Fi",
    "Tribal",
    "Acid Punk",
    "Acid Jazz",
    "Polka",
    "Retro",
    "Musical",
    "Rock & Roll",
    "Hard Rock",
    "Folk",
    "Folk-Rock",
    "National Folk",
    "Swing",
    "Fast-Fusion",
    "Bebop",
    "Latin",
    "Revival",
    "Celtic",
    "Bluegrass",
    "Avantgarde",
    "Gothic Rock",
    "Progressive Rock",
    "Psychedelic Rock",
    "Symphonic Rock",
    "Slow Rock",
    "Big Band",
    "Chorus",
    "Easy Listening",
    "Acoustic",
    "Humour",
    "Speech",
    "Chanson",
    "Opera",
    "Chamber Music",
    "Sonata",
    "Symphony",
    "Booty Bass",
    "Primus",
    "Porn Groove",
    "Satire",
    "Slow Jam",
    "Club",
    "Tango",
    "Samba",
    "Folklore",
    "Ballad",
    "Power Ballad",
    "Rhythmic Soul",
    "Freestyle",
    "Duet",
    "Punk Rock",
    "Drum Solo",
    "A Cappella",
    "Euro-House",
    "Dance Hall",
    "Goa",
    "Drum & Bass",
    "Club-House",
    "Hardcore",
    "Terror",
    "Indie",
    "BritPop",
    "Afro-Punk",
    "Polsk Punk",
    "Beat",
    "Christian Gangsta Rap",
    "Heavy Metal",
    "Black Metal",
    "Crossover",
    "Contemporary Christian",
    "Christian Rock",
    "Merengue",
    "Salsa",
    "Thrash Metal",
    "Anime",
    "JPop",
    "Synthpop",
    "Abstract",
    "Art Rock",
    "Baroque",
    "Bhangra",
    "Big Beat",
    "Breakbeat",
    "Chillout",
    "Downtempo",
    "Dub",
    "EBM",
    "Eclectic",
    "Electro",
    "Electroclash",
    "Emo",
    "Experimental",
    "Garage",
    "Global",
    "IDM",
    "Illbient",
    "Industro-Goth",
    "Jam Band",
    "Krautrock",
    "Leftfield",
    "Lounge",
    "Math Rock",
    "New Romantic",
    "Nu-Breakz",
    "Post-Punk",
    "Post-Rock",
    "Psytrance",
    "Shoegaze",
    "Space Rock",
    "Trop Rock",
    "World Music",
    "Neoclassical",
    "Audiobook",
    "Audio Theatre",
    "Neue Deutsche Welle",
    "Podcast",
    "Indie Rock",
    "G-Funk",
    "Dubstep",
    "Garage Rock",
    "Psybient",
];

/// ID3v2.2 to v2.3/v2.4 frame ID upgrade mappings
pub const ID3V22_UPGRADE_MAP: &[(&str, &str)] = &[
    // Text frames
    ("TT1", "TIT1"), // Content group description
    ("TT2", "TIT2"), // Title/songname/content description
    ("TT3", "TIT3"), // Subtitle/description refinement
    ("TP1", "TPE1"), // Lead performer(s)/Soloist(s)
    ("TP2", "TPE2"), // Band/orchestra/accompaniment
    ("TP3", "TPE3"), // Conductor/performer refinement
    ("TP4", "TPE4"), // Interpreted, remixed, or otherwise modified by
    ("TCM", "TCOM"), // Composer
    ("TXT", "TEXT"), // Lyricist/text writer
    ("TLA", "TLAN"), // Language(s)
    ("TCO", "TCON"), // Content type
    ("TAL", "TALB"), // Album/movie/show title
    ("TPA", "TPOS"), // Part of a set
    ("TRK", "TRCK"), // Track number/position in set
    ("TRC", "TSRC"), // International Standard Recording Code
    ("TYE", "TYER"), // Year
    ("TDA", "TDAT"), // Date
    ("TIM", "TIME"), // Time
    ("TRD", "TRDA"), // Recording dates
    ("TMT", "TMED"), // Media type
    ("TFT", "TFLT"), // File type
    ("TBP", "TBPM"), // BPM (beats per minute)
    ("TCR", "TCOP"), // Copyright message
    ("TPB", "TPUB"), // Publisher
    ("TEN", "TENC"), // Encoded by
    ("TSS", "TSSE"), // Software/hardware and settings used for encoding
    ("TOF", "TOFN"), // Original filename
    ("TLE", "TLEN"), // Length
    ("TSI", "TSIZ"), // Size
    ("TDY", "TDLY"), // Playlist delay
    ("TKE", "TKEY"), // Initial key
    ("TOT", "TOAL"), // Original album/movie/show title
    ("TOA", "TOPE"), // Original artist(s)/performer(s)
    ("TOL", "TOLY"), // Original lyricist(s)/text writer(s)
    ("TOR", "TORY"), // Original release year
    // URL frames
    ("WAF", "WOAF"), // Official audio file webpage
    ("WAR", "WOAR"), // Official artist/performer webpage
    ("WAS", "WOAS"), // Official audio source webpage
    ("WCM", "WCOM"), // Commercial information
    ("WCP", "WCOP"), // Copyright/legal information
    ("WPB", "WPUB"), // Publishers official webpage
    // Comment-like frames
    ("COM", "COMM"), // Comments
    ("ULT", "USLT"), // Unsynchronized lyric/text transcription
    // Other frames
    ("UFI", "UFID"), // Unique file identifier
    ("MCI", "MCDI"), // Music CD identifier
    ("ETC", "ETCO"), // Event timing codes
    ("MLL", "MLLT"), // MPEG location lookup table
    ("STC", "SYTC"), // Synchronized tempo codes
    ("SLT", "SYLT"), // Synchronized lyric/text
    ("RVA", "RVAD"), // Relative volume adjustment
    ("EQU", "EQUA"), // Equalization
    ("REV", "RVRB"), // Reverb
    ("PIC", "APIC"), // Attached picture
    ("GEO", "GEOB"), // General encapsulated object
    ("CNT", "PCNT"), // Play counter
    ("POP", "POPM"), // Popularimeter
    ("BUF", "RBUF"), // Recommended buffer size
    ("CRA", "AENC"), // Audio encryption
    ("LNK", "LINK"), // Linked information
];

/// ID3v2.2 to v2.4 specific upgrades (frames that changed between v2.3 and v2.4)
pub const ID3V22_TO_V24_UPGRADES: &[(&str, &str)] = &[
    ("TDA", "TDRC"), // Date -> Recording time
    ("TIM", "TDRC"), // Time -> Recording time
    ("TRD", "TDRC"), // Recording dates -> Recording time
    ("TYE", "TDRC"), // Year -> Recording time
    ("TOR", "TDOR"), // Original release year -> Original release time
    ("IPL", "TIPL"), // Involved people list -> Involved people list
    ("RVA", "RVA2"), // Relative volume adjustment -> Relative volume adjustment (2)
    ("EQU", "EQU2"), // Equalization -> Equalization (2)
];

/// Frame flags for ID3v2.3
pub mod flags_v23 {
    pub const ALTER_TAG: u16 = 0x8000;
    pub const ALTER_FILE: u16 = 0x4000;
    pub const READ_ONLY: u16 = 0x2000;
    pub const COMPRESS: u16 = 0x0080;
    pub const ENCRYPT: u16 = 0x0040;
    pub const GROUP: u16 = 0x0020;
}

/// Frame flags for ID3v2.4
pub mod flags_v24 {
    pub const ALTER_TAG: u16 = 0x4000;
    pub const ALTER_FILE: u16 = 0x2000;
    pub const READ_ONLY: u16 = 0x1000;
    pub const GROUP_ID: u16 = 0x0040;
    pub const COMPRESS: u16 = 0x0008;
    pub const ENCRYPT: u16 = 0x0004;
    pub const UNSYNCH: u16 = 0x0002;
    pub const DATA_LEN: u16 = 0x0001;
}

/// Picture type identifier for APIC frames
///
/// Defines the type/purpose of an attached picture in an ID3v2 APIC frame.
/// These types help applications display appropriate images in their UI
/// (e.g., showing the front cover as album art).
///
/// # Standard Picture Types
///
/// The ID3v2.4 specification defines 21 standard picture types. While a file
/// can have multiple pictures of different types, most players only use the
/// front cover image.
///
/// # Examples
///
/// ```
/// use audex::id3::PictureType;
///
/// // Most common: front cover
/// let cover_type = PictureType::CoverFront;
/// assert_eq!(cover_type as u8, 0x03);
///
/// // Artist photo
/// let artist_photo = PictureType::Artist;
/// ```
///
/// # See Also
///
/// - [`APIC`]: The attached picture frame that uses this type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum PictureType {
    /// Other/Undefined picture type
    Other = 0x00,

    /// 32x32 pixels file icon (PNG only)
    FileIcon = 0x01,

    /// Other file icon
    OtherFileIcon = 0x02,

    /// Cover (front) - Most commonly used for album artwork
    CoverFront = 0x03,

    /// Cover (back)
    CoverBack = 0x04,

    /// Leaflet page
    LeafletPage = 0x05,

    /// Media (e.g., label side of CD)
    Media = 0x06,

    /// Lead artist/lead performer/soloist
    LeadArtist = 0x07,

    /// Artist/performer
    Artist = 0x08,

    /// Conductor
    Conductor = 0x09,

    /// Band/Orchestra
    Band = 0x0A,

    /// Composer
    Composer = 0x0B,

    /// Lyricist/text writer
    Lyricist = 0x0C,

    /// Recording location
    RecordingLocation = 0x0D,

    /// During recording
    DuringRecording = 0x0E,

    /// During performance
    DuringPerformance = 0x0F,

    /// Movie/video screen capture
    VideoScreenCapture = 0x10,

    /// A bright coloured fish (yes, this is in the spec!)
    BrightColoredFish = 0x11,

    /// Illustration
    Illustration = 0x12,

    /// Band/artist logotype
    BandLogo = 0x13,

    /// Publisher/Studio logotype
    PublisherLogo = 0x14,
}

impl From<u8> for PictureType {
    fn from(value: u8) -> Self {
        match value {
            0x01 => PictureType::FileIcon,
            0x02 => PictureType::OtherFileIcon,
            0x03 => PictureType::CoverFront,
            0x04 => PictureType::CoverBack,
            0x05 => PictureType::LeafletPage,
            0x06 => PictureType::Media,
            0x07 => PictureType::LeadArtist,
            0x08 => PictureType::Artist,
            0x09 => PictureType::Conductor,
            0x0A => PictureType::Band,
            0x0B => PictureType::Composer,
            0x0C => PictureType::Lyricist,
            0x0D => PictureType::RecordingLocation,
            0x0E => PictureType::DuringRecording,
            0x0F => PictureType::DuringPerformance,
            0x10 => PictureType::VideoScreenCapture,
            0x11 => PictureType::BrightColoredFish,
            0x12 => PictureType::Illustration,
            0x13 => PictureType::BandLogo,
            0x14 => PictureType::PublisherLogo,
            _ => PictureType::Other,
        }
    }
}

impl fmt::Display for PictureType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            PictureType::Other => "Other",
            PictureType::FileIcon => "32x32 pixels 'file icon' (PNG only)",
            PictureType::OtherFileIcon => "Other file icon",
            PictureType::CoverFront => "Cover (front)",
            PictureType::CoverBack => "Cover (back)",
            PictureType::LeafletPage => "Leaflet page",
            PictureType::Media => "Media (e.g. label side of CD)",
            PictureType::LeadArtist => "Lead artist/lead performer/soloist",
            PictureType::Artist => "Artist/performer",
            PictureType::Conductor => "Conductor",
            PictureType::Band => "Band/Orchestra",
            PictureType::Composer => "Composer",
            PictureType::Lyricist => "Lyricist/text writer",
            PictureType::RecordingLocation => "Recording Location",
            PictureType::DuringRecording => "During recording",
            PictureType::DuringPerformance => "During performance",
            PictureType::VideoScreenCapture => "Movie/video screen capture",
            PictureType::BrightColoredFish => "A bright coloured fish",
            PictureType::Illustration => "Illustration",
            PictureType::BandLogo => "Band/artist logotype",
            PictureType::PublisherLogo => "Publisher/Studio logotype",
        };
        write!(f, "{}", name)
    }
}

/// Audio channel identifier for volume adjustment frames
///
/// Used in RVA2 (Relative Volume Adjustment 2) and RVAD (Relative Volume Adjustment)
/// frames to specify which audio channel(s) the volume adjustment applies to.
/// This allows per-channel volume normalization (e.g., ReplayGain).
///
/// # Channel Types
///
/// Most audio uses stereo (2 channels) or surround sound (5.1, 7.1, etc.).
/// The MasterVolume channel applies to all channels uniformly.
///
/// # Examples
///
/// ```
/// use audex::id3::frames::ChannelType;
///
/// // Master volume affects all channels
/// let master = ChannelType::MasterVolume;
/// assert_eq!(master as u8, 1);
///
/// // Individual channels for surround sound
/// let front_left = ChannelType::FrontLeft;
/// let subwoofer = ChannelType::Subwoofer;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChannelType {
    /// Other/undefined channel
    Other = 0,

    /// Master volume (affects all channels)
    MasterVolume = 1,

    /// Front right channel
    FrontRight = 2,

    /// Front left channel
    FrontLeft = 3,

    /// Back right channel (surround sound)
    BackRight = 4,

    /// Back left channel (surround sound)
    BackLeft = 5,

    /// Front centre channel (surround sound)
    FrontCentre = 6,

    /// Back centre channel (surround sound)
    BackCentre = 7,

    /// Subwoofer/LFE channel
    Subwoofer = 8,
}

impl From<u8> for ChannelType {
    fn from(value: u8) -> Self {
        match value {
            1 => ChannelType::MasterVolume,
            2 => ChannelType::FrontRight,
            3 => ChannelType::FrontLeft,
            4 => ChannelType::BackRight,
            5 => ChannelType::BackLeft,
            6 => ChannelType::FrontCentre,
            7 => ChannelType::BackCentre,
            8 => ChannelType::Subwoofer,
            _ => ChannelType::Other,
        }
    }
}

const CHANNEL_NAMES: [&str; 9] = [
    "Other",
    "Master volume",
    "Front right",
    "Front left",
    "Back right",
    "Back left",
    "Front centre",
    "Back centre",
    "Subwoofer",
];

impl fmt::Display for ChannelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Use bounds-checked access to guard against out-of-range discriminants.
        let name = CHANNEL_NAMES.get(*self as usize).unwrap_or(&"Unknown");
        write!(f, "{}", name)
    }
}

/// Parsed ID3v2 frame data, one variant per frame type.
///
/// Each variant holds the decoded payload of an ID3v2 frame. Text frames
/// (T***) store their values as `Vec<String>` to support multi-value fields,
/// while binary frames like APIC carry raw bytes alongside metadata fields.
///
/// This enum is used internally by the frame parsing/serialization pipeline
/// and by [`FrameRegistry`] to decode raw frame bytes into structured data.
#[derive(Debug, Clone)]
pub enum FrameData {
    /// Text information frames (T***)
    Text {
        id: String,
        encoding: TextEncoding,
        text: Vec<String>,
    },

    /// User-defined text frame (TXXX)
    UserText {
        encoding: TextEncoding,
        description: String,
        text: Vec<String>,
    },

    /// URL link frames (W***)
    Url { id: String, url: String },

    /// User-defined URL frame (WXXX)
    UserUrl {
        encoding: TextEncoding,
        description: String,
        url: String,
    },

    /// Comment frame (COMM)
    Comment {
        encoding: TextEncoding,
        language: [u8; 3],
        description: String,
        text: String,
    },

    /// Unsynchronized lyrics (USLT)
    UnsyncLyrics {
        encoding: TextEncoding,
        language: [u8; 3],
        description: String,
        lyrics: String,
    },

    /// Attached picture (APIC)
    AttachedPicture {
        encoding: TextEncoding,
        mime_type: String,
        picture_type: PictureType,
        description: String,
        data: Vec<u8>,
    },

    /// General encapsulated object (GEOB)
    GeneralObject {
        encoding: TextEncoding,
        mime_type: String,
        filename: String,
        description: String,
        data: Vec<u8>,
    },

    /// Play counter (PCNT)
    PlayCounter { count: u64 },

    /// Popularimeter (POPM)
    Popularimeter {
        email: String,
        rating: u8,
        count: u64,
    },

    /// Private frame (PRIV)
    Private { owner: String, data: Vec<u8> },

    /// Unique file identifier (UFID)
    UniqueFileId { owner: String, identifier: Vec<u8> },

    /// Terms of use (USER)
    TermsOfUse {
        encoding: TextEncoding,
        language: [u8; 3],
        text: String,
    },

    /// Ownership frame (OWNE)
    Ownership {
        encoding: TextEncoding,
        price: String,
        date: String,
        seller: String,
    },

    /// Commercial frame (COMR)
    Commercial {
        encoding: TextEncoding,
        price: String,
        valid_until: String,
        contact_url: String,
        received_as: u8,
        seller: String,
        description: String,
        picture_mime: String,
        picture: Vec<u8>,
    },

    /// Encryption method registration (ENCR)
    EncryptionMethod {
        owner: String,
        method_symbol: u8,
        encryption_data: Vec<u8>,
    },

    /// Group identification (GRID)
    GroupIdentification {
        owner: String,
        group_symbol: u8,
        group_data: Vec<u8>,
    },

    /// Linked information (LINK)
    LinkedInfo {
        frame_id: String,
        url: String,
        id_data: Vec<u8>,
    },

    /// Music CD identifier (MCDI)
    MusicCdId { cd_toc: Vec<u8> },

    /// Event timing codes (ETCO)
    EventTiming {
        format: TimeStampFormat,
        events: Vec<(u8, u32)>, // (event_type, timestamp)
    },

    /// MPEG location lookup table (MLLT)
    MpegLocationLookup {
        frames_between_reference: u16,
        bytes_between_reference: u32,
        milliseconds_between_reference: u32,
        bits_for_bytes: u8,
        bits_for_milliseconds: u8,
        references: Vec<(u32, u32)>, // (bytes_deviation, milliseconds_deviation)
    },

    /// Synchronised tempo codes (SYTC)
    SyncTempo {
        format: TimeStampFormat,
        tempo_data: Vec<u8>,
    },

    /// Synchronized lyrics/text (SYLT)
    SyncLyrics {
        encoding: TextEncoding,
        language: [u8; 3],
        format: TimeStampFormat,
        content_type: u8,
        description: String,
        lyrics: Vec<(String, u32)>, // (text, timestamp)
    },

    /// Relative volume adjustment (RVA2/RVAD)
    RelativeVolumeAdjustment {
        identification: String,
        channels: Vec<(ChannelType, i16, u8)>, // (channel, adjustment, peak_bits)
    },

    /// Equalisation (EQU2/EQUA)
    Equalisation {
        method: u8,
        identification: String,
        adjustments: Vec<(u16, i16)>, // (frequency, adjustment)
    },

    /// Reverb (RVRB)
    Reverb {
        reverb_left: u16,
        reverb_right: u16,
        reverb_bounces_left: u8,
        reverb_bounces_right: u8,
        reverb_feedback_left_to_left: u8,
        reverb_feedback_left_to_right: u8,
        reverb_feedback_right_to_right: u8,
        reverb_feedback_right_to_left: u8,
        premix_left_to_right: u8,
        premix_right_to_left: u8,
    },

    /// Recommended buffer size (RBUF)
    RecommendedBufferSize {
        buffer_size: u32,
        embedded_info_flag: bool,
        offset_to_next_tag: u32,
    },

    /// Audio encryption (AENC)
    AudioEncryption {
        owner: String,
        preview_start: u16,
        preview_length: u16,
        encryption_info: Vec<u8>,
    },

    /// Position synchronisation (POSS)
    PositionSync {
        format: TimeStampFormat,
        position: u32,
    },

    /// Signature frame (SIGN)
    Signature {
        group_symbol: u8,
        signature: Vec<u8>,
    },

    /// Seek frame (SEEK)
    Seek { minimum_offset: u32 },

    /// Chapter frame (CHAP)
    Chapter {
        element_id: String,
        start_time: u32,
        end_time: u32,
        start_offset: u32,
        end_offset: u32,
        sub_frames: Vec<FrameData>,
    },

    /// Table of contents frame (CTOC)
    TableOfContents {
        element_id: String,
        flags: u8,
        child_elements: Vec<String>,
        sub_frames: Vec<FrameData>,
    },

    /// Genre frame with sophisticated genre parsing (TCON)
    Genre {
        encoding: TextEncoding,
        genres: Vec<String>,
    },

    /// Paired text frames for key-value associations (TIPL, TMCL)
    PairedText {
        id: String,
        encoding: TextEncoding,
        people: Vec<(String, String)>,
    },

    /// Numeric text frames that store numeric values (TBPM, TLEN, etc.)
    NumericText {
        id: String,
        encoding: TextEncoding,
        text: Vec<String>,
        value: Option<u64>,
    },

    /// Numeric part text frames that extract first number (like track "4/15")
    NumericPartText {
        id: String,
        encoding: TextEncoding,
        text: Vec<String>,
        value: Option<u64>,
    },

    /// Timestamp text frames with parsed time information (TDRC, TDRL, etc.)
    TimeStampText {
        id: String,
        encoding: TextEncoding,
        timestamps: Vec<ID3TimeStamp>,
    },

    /// Unknown/unsupported frame
    Unknown { id: String, data: Vec<u8> },
}

/// Strip a single trailing null terminator from raw frame data.
pub fn strip_trailing_null<'a>(data: &'a [u8], encoding: &TextEncoding) -> &'a [u8] {
    let term = encoding.null_terminator();
    if data.len() >= term.len() && &data[data.len() - term.len()..] == term {
        &data[..data.len() - term.len()]
    } else {
        data
    }
}

impl FrameData {
    /// Parse a frame from header and data
    pub fn from_bytes(header: &FrameHeader, mut data: Vec<u8>) -> Result<Self> {
        // Process frame flags (decompress, remove unsync, etc.)
        data = FrameProcessor::process_read(header, data)?;

        // Check for corrupted data
        if JunkFrameRecovery::is_frame_corrupted(
            &data,
            &header.frame_id,
            header.flags.to_raw(header.version),
        ) {
            // Try to reconstruct the frame
            data = JunkFrameRecovery::reconstruct_frame(&data, &header.frame_id, header.size)?;
        }

        // Parse frame based on ID
        match header.frame_id.as_str() {
            // Special text frames
            "TCON" => Self::parse_genre_frame(&data),
            "TIPL" | "TMCL" => Self::parse_paired_text_frame(header.frame_id.as_str(), &data),

            // Numeric text frames
            "TBPM" | "TLEN" | "TDAT" | "TDLY" | "TORY" | "TYER" | "TSIZ" | "TCMP" => {
                Self::parse_numeric_text_frame(header.frame_id.as_str(), &data)
            }

            // Numeric part text frames (for track numbers like "4/15")
            "TRCK" | "TPOS" => Self::parse_numeric_part_text_frame(header.frame_id.as_str(), &data),

            // Timestamp text frames
            "TDRC" | "TDRL" | "TDTG" | "TDOR" | "TDEN" => {
                Self::parse_timestamp_text_frame(header.frame_id.as_str(), &data)
            }

            // Standard text frames
            id if id.starts_with('T') && id != "TXXX" => Self::parse_text_frame(id, &data),
            "TXXX" => Self::parse_user_text_frame(&data),

            // URL frames
            id if id.starts_with('W') && id != "WXXX" => Self::parse_url_frame(id, &data),
            "WXXX" => Self::parse_user_url_frame(&data),

            // Comment and lyrics
            "COMM" => Self::parse_comment_frame(&data),
            "USLT" => Self::parse_unsync_lyrics_frame(&data),
            "SYLT" => Self::parse_sync_lyrics_frame(&data),

            // Picture and objects
            "APIC" => Self::parse_attached_picture_frame(&data),
            "GEOB" => Self::parse_general_object_frame(&data),

            // Play count and popularity
            "PCNT" => Self::parse_play_counter_frame(&data),
            "POPM" => Self::parse_popularimeter_frame(&data),

            // Technical frames
            "PRIV" => Self::parse_private_frame(&data),
            "UFID" => Self::parse_unique_file_id_frame(&data),
            "USER" => Self::parse_terms_of_use_frame(&data),
            "OWNE" => Self::parse_ownership_frame(&data),
            "COMR" => Self::parse_commercial_frame(&data),
            "ENCR" => Self::parse_encryption_method_frame(&data),
            "GRID" => Self::parse_group_id_frame(&data),
            "LINK" => Self::parse_linked_info_frame(&data),
            "MCDI" => Self::parse_music_cd_id_frame(&data),
            "ETCO" => Self::parse_event_timing_frame(&data),
            "MLLT" => Self::parse_mpeg_location_lookup_frame(&data),
            "SYTC" => Self::parse_sync_tempo_frame(&data),
            "RVA2" | "RVAD" => Self::parse_relative_volume_frame(&data),
            "EQU2" | "EQUA" => Self::parse_equalisation_frame(&data),
            "RVRB" => Self::parse_reverb_frame(&data),
            "RBUF" => Self::parse_recommended_buffer_size_frame(&data),
            "AENC" => Self::parse_audio_encryption_frame(&data),
            "POSS" => Self::parse_position_sync_frame(&data),
            "SIGN" => Self::parse_signature_frame(&data),
            "SEEK" => Self::parse_seek_frame(&data),
            "CHAP" => Self::parse_chapter_frame_depth(&data, header.version, 0),
            "CTOC" => Self::parse_table_of_contents_frame_depth(&data, header.version, 0),

            // Unknown frame
            _ => Ok(FrameData::Unknown {
                id: header.frame_id.clone(),
                data,
            }),
        }
    }

    /// Convert frame to bytes for writing
    pub fn to_bytes(&self, version: (u8, u8)) -> Result<Vec<u8>> {
        match self {
            FrameData::Text { encoding, text, .. } => {
                Self::write_text_frame(*encoding, text, version)
            }
            FrameData::UserText {
                encoding,
                description,
                text,
            } => Self::write_user_text_frame(*encoding, description, text, version),
            FrameData::Url { url, .. } => Ok(url.as_bytes().to_vec()),
            FrameData::UserUrl {
                encoding,
                description,
                url,
            } => Self::write_user_url_frame(*encoding, description, url, version),
            FrameData::Comment {
                encoding,
                language,
                description,
                text,
            } => Self::write_comment_frame(*encoding, *language, description, text, version),
            FrameData::UnsyncLyrics {
                encoding,
                language,
                description,
                lyrics,
            } => {
                Self::write_unsync_lyrics_frame(*encoding, *language, description, lyrics, version)
            }
            FrameData::AttachedPicture {
                encoding,
                mime_type,
                picture_type,
                description,
                data,
            } => Self::write_attached_picture_frame(
                *encoding,
                mime_type,
                *picture_type,
                description,
                data,
                version,
            ),
            FrameData::GeneralObject {
                encoding,
                mime_type,
                filename,
                description,
                data,
            } => Self::write_general_object_frame(
                *encoding,
                mime_type,
                filename,
                description,
                data,
                version,
            ),
            FrameData::PlayCounter { count } => Self::write_play_counter_frame(*count),
            FrameData::Popularimeter {
                email,
                rating,
                count,
            } => Self::write_popularimeter_frame(email, *rating, *count),
            FrameData::Private { owner, data } => Self::write_private_frame(owner, data),
            FrameData::UniqueFileId { owner, identifier } => {
                Self::write_unique_file_id_frame(owner, identifier)
            }
            FrameData::TermsOfUse {
                encoding,
                language,
                text,
            } => Self::write_terms_of_use_frame(*encoding, *language, text, version),
            FrameData::Ownership {
                encoding,
                price,
                date,
                seller,
            } => Self::write_ownership_frame(*encoding, price, date, seller, version),
            FrameData::Commercial {
                encoding,
                price,
                valid_until,
                contact_url,
                received_as,
                seller,
                description,
                picture_mime,
                picture,
            } => {
                let params = CommercialFrameParams {
                    encoding: *encoding,
                    price,
                    valid_until,
                    contact_url,
                    received_as: *received_as,
                    seller,
                    description,
                    picture_mime,
                    picture,
                    _version: version,
                };
                Self::write_commercial_frame(&params)
            }
            FrameData::EncryptionMethod {
                owner,
                method_symbol,
                encryption_data,
            } => Self::write_encryption_method_frame(owner, *method_symbol, encryption_data),
            FrameData::GroupIdentification {
                owner,
                group_symbol,
                group_data,
            } => Self::write_group_id_frame(owner, *group_symbol, group_data),
            FrameData::LinkedInfo {
                frame_id,
                url,
                id_data,
            } => Self::write_linked_info_frame(frame_id, url, id_data),
            FrameData::MusicCdId { cd_toc } => Ok(cd_toc.clone()),
            FrameData::EventTiming { format, events } => {
                Self::write_event_timing_frame(*format, events)
            }
            FrameData::MpegLocationLookup {
                frames_between_reference,
                bytes_between_reference,
                milliseconds_between_reference,
                bits_for_bytes,
                bits_for_milliseconds,
                references,
            } => Self::write_mpeg_location_lookup_frame(
                *frames_between_reference,
                *bytes_between_reference,
                *milliseconds_between_reference,
                *bits_for_bytes,
                *bits_for_milliseconds,
                references,
            ),
            FrameData::SyncTempo { format, tempo_data } => {
                Self::write_sync_tempo_frame(*format, tempo_data)
            }
            FrameData::SyncLyrics {
                encoding,
                language,
                format,
                content_type,
                description,
                lyrics,
            } => Self::write_sync_lyrics_frame(
                *encoding,
                *language,
                *format,
                *content_type,
                description,
                lyrics,
                version,
            ),
            FrameData::RelativeVolumeAdjustment {
                identification,
                channels,
            } => Self::write_relative_volume_frame(identification, channels),
            FrameData::Equalisation {
                method,
                identification,
                adjustments,
            } => Self::write_equalisation_frame(*method, identification, adjustments),
            FrameData::Reverb {
                reverb_left,
                reverb_right,
                reverb_bounces_left,
                reverb_bounces_right,
                reverb_feedback_left_to_left,
                reverb_feedback_left_to_right,
                reverb_feedback_right_to_right,
                reverb_feedback_right_to_left,
                premix_left_to_right,
                premix_right_to_left,
            } => {
                let params = ReverbFrameParams {
                    reverb_left: *reverb_left,
                    reverb_right: *reverb_right,
                    reverb_bounces_left: *reverb_bounces_left,
                    reverb_bounces_right: *reverb_bounces_right,
                    reverb_feedback_left_to_left: *reverb_feedback_left_to_left,
                    reverb_feedback_left_to_right: *reverb_feedback_left_to_right,
                    reverb_feedback_right_to_right: *reverb_feedback_right_to_right,
                    reverb_feedback_right_to_left: *reverb_feedback_right_to_left,
                    premix_left_to_right: *premix_left_to_right,
                    premix_right_to_left: *premix_right_to_left,
                };
                Self::write_reverb_frame(&params)
            }
            FrameData::RecommendedBufferSize {
                buffer_size,
                embedded_info_flag,
                offset_to_next_tag,
            } => Self::write_recommended_buffer_size_frame(
                *buffer_size,
                *embedded_info_flag,
                *offset_to_next_tag,
            ),
            FrameData::AudioEncryption {
                owner,
                preview_start,
                preview_length,
                encryption_info,
            } => Self::write_audio_encryption_frame(
                owner,
                *preview_start,
                *preview_length,
                encryption_info,
            ),
            FrameData::PositionSync { format, position } => {
                Self::write_position_sync_frame(*format, *position)
            }
            FrameData::Signature {
                group_symbol,
                signature,
            } => Self::write_signature_frame(*group_symbol, signature),
            FrameData::Seek { minimum_offset } => Self::write_seek_frame(*minimum_offset),
            FrameData::Chapter {
                element_id,
                start_time,
                end_time,
                start_offset,
                end_offset,
                sub_frames,
            } => Self::write_chapter_frame(
                element_id,
                *start_time,
                *end_time,
                *start_offset,
                *end_offset,
                sub_frames,
                version,
            ),
            FrameData::TableOfContents {
                element_id,
                flags,
                child_elements,
                sub_frames,
            } => Self::write_table_of_contents_frame(
                element_id,
                *flags,
                child_elements,
                sub_frames,
                version,
            ),
            FrameData::Genre { encoding, genres } => {
                // Pass each genre as a separate entry so write_text_frame
                // can apply the correct version-aware separator (v2.3 uses "/", v2.4 uses null).
                Self::write_text_frame(*encoding, genres, version)
            }
            FrameData::PairedText {
                encoding, people, ..
            } => {
                // Flatten people pairs into alternating key-value entries so
                // write_text_frame applies the correct version-aware separator.
                let text_parts: Vec<String> = people
                    .iter()
                    .flat_map(|(key, value)| [key.clone(), value.clone()])
                    .collect();
                Self::write_text_frame(*encoding, &text_parts, version)
            }
            FrameData::NumericText { encoding, text, .. } => {
                Self::write_text_frame(*encoding, text, version)
            }
            FrameData::NumericPartText { encoding, text, .. } => {
                Self::write_text_frame(*encoding, text, version)
            }
            FrameData::TimeStampText {
                encoding,
                timestamps,
                ..
            } => {
                // Pass each timestamp as a separate entry so write_text_frame
                // applies the correct version-aware separator (null byte for
                // v2.4, "/" for v2.3) instead of joining with a literal comma.
                let text: Vec<String> = timestamps.iter().map(|ts| ts.text.clone()).collect();
                Self::write_text_frame(*encoding, &text, version)
            }
            FrameData::Unknown { data, .. } => Ok(data.clone()),
        }
    }

    /// Get the frame ID
    pub fn id(&self) -> &str {
        match self {
            FrameData::Text { id, .. } => id,
            FrameData::UserText { .. } => "TXXX",
            FrameData::Url { id, .. } => id,
            FrameData::UserUrl { .. } => "WXXX",
            FrameData::Comment { .. } => "COMM",
            FrameData::UnsyncLyrics { .. } => "USLT",
            FrameData::AttachedPicture { .. } => "APIC",
            FrameData::GeneralObject { .. } => "GEOB",
            FrameData::PlayCounter { .. } => "PCNT",
            FrameData::Popularimeter { .. } => "POPM",
            FrameData::Private { .. } => "PRIV",
            FrameData::UniqueFileId { .. } => "UFID",
            FrameData::TermsOfUse { .. } => "USER",
            FrameData::Ownership { .. } => "OWNE",
            FrameData::Commercial { .. } => "COMR",
            FrameData::EncryptionMethod { .. } => "ENCR",
            FrameData::GroupIdentification { .. } => "GRID",
            FrameData::LinkedInfo { .. } => "LINK",
            FrameData::MusicCdId { .. } => "MCDI",
            FrameData::EventTiming { .. } => "ETCO",
            FrameData::MpegLocationLookup { .. } => "MLLT",
            FrameData::SyncTempo { .. } => "SYTC",
            FrameData::SyncLyrics { .. } => "SYLT",
            FrameData::RelativeVolumeAdjustment { .. } => "RVA2",
            FrameData::Equalisation { .. } => "EQU2",
            FrameData::Reverb { .. } => "RVRB",
            FrameData::RecommendedBufferSize { .. } => "RBUF",
            FrameData::AudioEncryption { .. } => "AENC",
            FrameData::PositionSync { .. } => "POSS",
            FrameData::Signature { .. } => "SIGN",
            FrameData::Seek { .. } => "SEEK",
            FrameData::Chapter { .. } => "CHAP",
            FrameData::TableOfContents { .. } => "CTOC",
            FrameData::Genre { .. } => "TCON",
            FrameData::PairedText { id, .. } => id,
            FrameData::NumericText { id, .. } => id,
            FrameData::NumericPartText { id, .. } => id,
            FrameData::TimeStampText { id, .. } => id,
            FrameData::Unknown { id, .. } => id,
        }
    }

    /// Parse text frame (T*** except TXXX)
    fn parse_text_frame(frame_id: &str, data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Ok(FrameData::Text {
                id: frame_id.to_string(),
                encoding: TextEncoding::Latin1,
                text: vec![],
            });
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let text_data = &data[1..];
        let decoded = encoding.decode_text(text_data)?;
        let text_parts = decoded
            .split('\u{0}')
            .filter(|part| !part.is_empty())
            .map(|part| part.trim_start_matches('\u{feff}').to_string())
            .collect::<Vec<_>>();

        Ok(FrameData::Text {
            id: frame_id.to_string(),
            encoding,
            text: text_parts,
        })
    }

    /// Parse user-defined text frame (TXXX)
    fn parse_user_text_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 2 {
            warn_event!(frame = "TXXX", len = data.len(), "frame data too short");
            return Err(AudexError::InvalidData("TXXX frame too short".to_string()));
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let text_data = &data[1..];
        let null_term = encoding.null_terminator();

        // Find first null terminator to separate description from text
        let desc_end = if null_term.len() == 1 {
            text_data
                .iter()
                .position(|&b| b == null_term[0])
                .unwrap_or(text_data.len())
        } else {
            // UTF-16 double null
            // Truncate to even length so the 2-byte stepping stays aligned
            (0..(text_data.len() & !1).saturating_sub(1))
                .step_by(2)
                .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                .unwrap_or(text_data.len())
        };

        let description = encoding.decode_text(&text_data[..desc_end])?;
        let text_start = desc_end + null_term.len();

        let text_parts = if text_start < text_data.len() {
            let remaining = &text_data[text_start..];
            if null_term.len() == 1 {
                remaining
                    .split(|&b| b == null_term[0])
                    .filter(|part| !part.is_empty())
                    .map(|part| encoding.decode_text(part))
                    .collect::<Result<Vec<_>>>()?
            } else {
                vec![encoding.decode_text(remaining)?]
            }
        } else {
            vec![]
        };

        Ok(FrameData::UserText {
            encoding,
            description,
            text: text_parts,
        })
    }

    /// Parse URL frame (W*** except WXXX)
    fn parse_url_frame(frame_id: &str, data: &[u8]) -> Result<Self> {
        let url = String::from_utf8_lossy(data)
            .trim_end_matches('\0')
            .to_string();
        Ok(FrameData::Url {
            id: frame_id.to_string(),
            url,
        })
    }

    /// Parse user-defined URL frame (WXXX)
    fn parse_user_url_frame(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(AudexError::InvalidData("WXXX frame too short".to_string()));
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let text_data = &data[1..];
        let null_term = encoding.null_terminator();

        // Find description end
        let desc_end = if null_term.len() == 1 {
            text_data
                .iter()
                .position(|&b| b == null_term[0])
                .unwrap_or(text_data.len())
        } else {
            // Truncate to even length so the 2-byte stepping stays aligned
            (0..(text_data.len() & !1).saturating_sub(1))
                .step_by(2)
                .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                .unwrap_or(text_data.len())
        };

        let description = encoding.decode_text(&text_data[..desc_end])?;
        let url_start = desc_end + null_term.len();
        let url = if url_start < text_data.len() {
            String::from_utf8_lossy(&text_data[url_start..])
                .trim_end_matches('\0')
                .to_string()
        } else {
            String::new()
        };

        Ok(FrameData::UserUrl {
            encoding,
            description,
            url,
        })
    }

    /// Parse comment frame (COMM)
    fn parse_comment_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            warn_event!(frame = "COMM", len = data.len(), "frame data too short");
            return Err(AudexError::ID3FrameTooShort {
                expected: 5,
                actual: data.len(),
            });
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let language = [data[1], data[2], data[3]];
        let text_data = &data[4..];
        let null_term = encoding.null_terminator();

        // Find description end
        let desc_end = if null_term.len() == 1 {
            text_data
                .iter()
                .position(|&b| b == null_term[0])
                .unwrap_or(text_data.len())
        } else {
            // Truncate to even length so the 2-byte stepping stays aligned
            (0..(text_data.len() & !1).saturating_sub(1))
                .step_by(2)
                .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                .unwrap_or(text_data.len())
        };

        let description = encoding.decode_text(&text_data[..desc_end])?;
        let comment_start = desc_end + null_term.len();
        let comment_text = if comment_start < text_data.len() {
            let raw = &text_data[comment_start..];
            // Strip trailing null terminator (written per ID3v2 convention, not part of text)
            let raw = strip_trailing_null(raw, &encoding);
            encoding.decode_text(raw)?
        } else {
            String::new()
        };

        Ok(FrameData::Comment {
            encoding,
            language,
            description,
            text: comment_text,
        })
    }

    /// Parse unsynchronized lyrics frame (USLT)
    fn parse_unsync_lyrics_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            warn_event!(frame = "USLT", len = data.len(), "frame data too short");
            return Err(AudexError::InvalidData("USLT frame too short".to_string()));
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let language = [data[1], data[2], data[3]];
        let text_data = &data[4..];
        let null_term = encoding.null_terminator();

        // Find description end
        let desc_end = if null_term.len() == 1 {
            text_data
                .iter()
                .position(|&b| b == null_term[0])
                .unwrap_or(text_data.len())
        } else {
            // Truncate to even length so the 2-byte stepping stays aligned
            (0..(text_data.len() & !1).saturating_sub(1))
                .step_by(2)
                .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                .unwrap_or(text_data.len())
        };

        let description = encoding.decode_text(&text_data[..desc_end])?;
        let lyrics_start = desc_end + null_term.len();
        let lyrics = if lyrics_start < text_data.len() {
            let raw = &text_data[lyrics_start..];
            // Strip trailing null terminator (written per ID3v2 convention, not part of text)
            let raw = strip_trailing_null(raw, &encoding);
            encoding.decode_text(raw)?
        } else {
            String::new()
        };

        Ok(FrameData::UnsyncLyrics {
            encoding,
            language,
            description,
            lyrics,
        })
    }

    /// Parse attached picture frame (APIC)
    fn parse_attached_picture_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            warn_event!(frame = "APIC", len = data.len(), "frame data too short");
            return Err(AudexError::ID3FrameTooShort {
                expected: 5,
                actual: data.len(),
            });
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let mut offset = 1;

        // Find MIME type (null-terminated ASCII)
        let mime_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No MIME type terminator in APIC frame".to_string())
        })? + offset;
        let mime_type = String::from_utf8_lossy(&data[offset..mime_end]).into_owned();
        offset = mime_end + 1;

        if offset >= data.len() {
            return Err(AudexError::InvalidData(
                "APIC frame missing picture type".to_string(),
            ));
        }

        // Picture type
        let picture_type = PictureType::from(data[offset]);
        offset += 1;

        let remaining_data = &data[offset..];
        let null_term = encoding.null_terminator();

        // Find description end
        let desc_end = if null_term.len() == 1 {
            remaining_data
                .iter()
                .position(|&b| b == null_term[0])
                .unwrap_or(remaining_data.len())
        } else {
            // Truncate to even length so the 2-byte stepping stays aligned
            (0..(remaining_data.len() & !1).saturating_sub(1))
                .step_by(2)
                .find(|&i| remaining_data[i] == 0 && remaining_data[i + 1] == 0)
                .unwrap_or(remaining_data.len())
        };

        let description = encoding.decode_text(&remaining_data[..desc_end])?;

        if desc_end == remaining_data.len() {
            return Err(AudexError::InvalidData(
                "APIC frame description is not null-terminated".to_string(),
            ));
        }

        let picture_start = desc_end + null_term.len();
        crate::limits::ParseLimits::default().check_image_size(
            remaining_data[picture_start..].len() as u64,
            "ID3 APIC image",
        )?;

        let picture_data = if picture_start < remaining_data.len() {
            remaining_data[picture_start..].to_vec()
        } else {
            Vec::new()
        };

        Ok(FrameData::AttachedPicture {
            encoding,
            mime_type,
            picture_type,
            description,
            data: picture_data,
        })
    }

    /// Parse play counter frame (PCNT)
    fn parse_play_counter_frame(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Ok(FrameData::PlayCounter { count: 0 });
        }

        // A u64 can hold at most 8 bytes of big-endian data.
        // Reject frames that exceed this limit to avoid silent truncation.
        if data.len() > 8 {
            return Err(crate::AudexError::InvalidData(format!(
                "PCNT play counter too large: {} bytes exceeds the 8-byte u64 maximum",
                data.len(),
            )));
        }

        // PCNT uses variable-length big-endian integer
        let mut count = 0u64;
        for &byte in data.iter() {
            count = (count << 8) | byte as u64;
        }

        Ok(FrameData::PlayCounter { count })
    }

    /// Parse popularimeter frame (POPM)
    fn parse_popularimeter_frame(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            warn_event!(frame = "POPM", "frame data is empty");
            return Err(AudexError::InvalidData("POPM frame too short".to_string()));
        }

        // Find email terminator
        let email_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No email terminator in POPM frame".to_string())
        })?;
        let email = String::from_utf8_lossy(&data[..email_end]).into_owned();

        if email_end + 1 >= data.len() {
            return Err(AudexError::InvalidData(
                "POPM frame missing rating".to_string(),
            ));
        }

        let rating = data[email_end + 1];

        // Count is variable-length big-endian integer (at most 8 bytes for u64)
        let count_data = &data[email_end + 2..];
        if count_data.len() > 8 {
            return Err(AudexError::InvalidData(format!(
                "POPM play counter too large: {} bytes exceeds the 8-byte u64 maximum",
                count_data.len(),
            )));
        }
        let mut count = 0u64;
        for &byte in count_data.iter() {
            count = (count << 8) | byte as u64;
        }

        Ok(FrameData::Popularimeter {
            email,
            rating,
            count,
        })
    }

    /// Parse private frame (PRIV)
    fn parse_private_frame(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(AudexError::InvalidData("PRIV frame too short".to_string()));
        }

        let owner_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No owner terminator in PRIV frame".to_string())
        })?;
        let owner = String::from_utf8_lossy(&data[..owner_end]).into_owned();
        let private_data = data[owner_end + 1..].to_vec();

        Ok(FrameData::Private {
            owner,
            data: private_data,
        })
    }

    /// Parse unique file identifier frame (UFID)
    fn parse_unique_file_id_frame(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(AudexError::InvalidData("UFID frame too short".to_string()));
        }

        let owner_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No owner terminator in UFID frame".to_string())
        })?;
        let owner = String::from_utf8_lossy(&data[..owner_end]).into_owned();
        let identifier = data[owner_end + 1..].to_vec();

        Ok(FrameData::UniqueFileId { owner, identifier })
    }

    /// Parse synchronized lyrics frame (SYLT)
    fn parse_sync_lyrics_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 6 {
            return Err(AudexError::InvalidData("SYLT frame too short".to_string()));
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let language = [data[1], data[2], data[3]];
        let format = TimeStampFormat::from(data[4]);
        let content_type = data[5];
        let text_data = &data[6..];
        let null_term = encoding.null_terminator();

        // Find description end
        let desc_end = if null_term.len() == 1 {
            text_data
                .iter()
                .position(|&b| b == null_term[0])
                .unwrap_or(text_data.len())
        } else {
            // Truncate to even length so the 2-byte stepping stays aligned
            (0..(text_data.len() & !1).saturating_sub(1))
                .step_by(2)
                .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                .unwrap_or(text_data.len())
        };

        let description = encoding.decode_text(&text_data[..desc_end])?;
        let sync_data_start = desc_end + null_term.len();

        let mut lyrics = Vec::new();
        if sync_data_start < text_data.len() {
            let sync_data = &text_data[sync_data_start..];
            let mut pos = 0;

            // Guard against pathological inputs: cap the number of entries
            // to prevent excessive memory use, and ensure forward progress
            // on every iteration to avoid infinite loops.
            const MAX_LYRICS_ENTRIES: usize = 50_000;

            while pos + 4 < sync_data.len() && lyrics.len() < MAX_LYRICS_ENTRIES {
                let prev_pos = pos;
                // Find text end
                let text_end = if null_term.len() == 1 {
                    sync_data[pos..]
                        .iter()
                        .position(|&b| b == null_term[0])
                        .unwrap_or(sync_data.len() - pos)
                        + pos
                } else {
                    // Truncate to even length so the 2-byte stepping stays aligned
                    (pos..(sync_data.len() & !1).saturating_sub(1))
                        .step_by(2)
                        .find(|&i| sync_data[i] == 0 && sync_data[i + 1] == 0)
                        .unwrap_or(sync_data.len())
                };

                if text_end + null_term.len() + 4 > sync_data.len() {
                    break;
                }

                let text = encoding.decode_text(&sync_data[pos..text_end])?;
                let timestamp_pos = text_end + null_term.len();
                let timestamp = u32::from_be_bytes([
                    sync_data[timestamp_pos],
                    sync_data[timestamp_pos + 1],
                    sync_data[timestamp_pos + 2],
                    sync_data[timestamp_pos + 3],
                ]);

                lyrics.push((text, timestamp));
                pos = timestamp_pos + 4;

                // If the position did not advance, the data is malformed
                // and continuing would loop forever.
                if pos <= prev_pos {
                    break;
                }
            }
        }

        Ok(FrameData::SyncLyrics {
            encoding,
            language,
            format,
            content_type,
            description,
            lyrics,
        })
    }

    /// Parse general encapsulated object frame (GEOB)
    fn parse_general_object_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 2 {
            return Err(AudexError::InvalidData("GEOB frame too short".to_string()));
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let mut offset = 1;

        // Find MIME type (null-terminated ASCII)
        let mime_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No MIME type terminator in GEOB frame".to_string())
        })? + offset;
        let mime_type = String::from_utf8_lossy(&data[offset..mime_end]).into_owned();
        offset = mime_end + 1;

        let remaining_data = &data[offset..];
        let null_term = encoding.null_terminator();

        // Find filename end
        let filename_end = if null_term.len() == 1 {
            remaining_data
                .iter()
                .position(|&b| b == null_term[0])
                .unwrap_or(remaining_data.len())
        } else {
            // Truncate to even length so the 2-byte stepping stays aligned
            (0..(remaining_data.len() & !1).saturating_sub(1))
                .step_by(2)
                .find(|&i| remaining_data[i] == 0 && remaining_data[i + 1] == 0)
                .unwrap_or(remaining_data.len())
        };

        let filename = encoding.decode_text(&remaining_data[..filename_end])?;
        let desc_start = filename_end + null_term.len();

        let mut description = String::new();
        let mut data_start = desc_start;

        if desc_start < remaining_data.len() {
            // Find description end
            let desc_end = if null_term.len() == 1 {
                remaining_data[desc_start..]
                    .iter()
                    .position(|&b| b == null_term[0])
                    .map(|pos| pos + desc_start)
                    .unwrap_or(remaining_data.len())
            } else {
                // Truncate to even length so the 2-byte stepping stays aligned
                (desc_start..(remaining_data.len() & !1).saturating_sub(1))
                    .step_by(2)
                    .find(|&i| remaining_data[i] == 0 && remaining_data[i + 1] == 0)
                    .unwrap_or(remaining_data.len())
            };

            description = encoding.decode_text(&remaining_data[desc_start..desc_end])?;
            data_start = desc_end + null_term.len();
        }

        let object_data = if data_start < remaining_data.len() {
            remaining_data[data_start..].to_vec()
        } else {
            Vec::new()
        };

        Ok(FrameData::GeneralObject {
            encoding,
            mime_type,
            filename,
            description,
            data: object_data,
        })
    }

    /// Parse terms of use frame (USER)
    fn parse_terms_of_use_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            return Err(AudexError::InvalidData("USER frame too short".to_string()));
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let language = [data[1], data[2], data[3]];
        let text = encoding.decode_text(&data[4..])?;

        Ok(FrameData::TermsOfUse {
            encoding,
            language,
            text,
        })
    }

    /// Parse ownership frame (OWNE)
    fn parse_ownership_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 2 {
            return Err(AudexError::InvalidData("OWNE frame too short".to_string()));
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let text_data = &data[1..];

        // The price field is always Latin-1 and always uses a single-byte null
        // terminator, regardless of the frame's text encoding byte.
        let price_end = text_data
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(text_data.len());

        let price = TextEncoding::Latin1.decode_text(&text_data[..price_end])?;
        let date_start = price_end.saturating_add(1);

        let mut date = String::new();
        let mut seller_start = date_start;

        if date_start < text_data.len() {
            // Date field must be exactly 8 bytes (YYYYMMDD format).
            // Reject payloads that have a partial date rather than
            // silently producing a truncated string.
            let available = text_data.len() - date_start;
            if available < 8 {
                return Err(AudexError::InvalidData(format!(
                    "OWNE date field too short: {} bytes (expected 8)",
                    available
                )));
            }
            let date_end = date_start + 8;
            date = String::from_utf8_lossy(&text_data[date_start..date_end]).into_owned();
            seller_start = date_end;
        }

        let seller = if seller_start < text_data.len() {
            encoding.decode_text(&text_data[seller_start..])?
        } else {
            String::new()
        };

        Ok(FrameData::Ownership {
            encoding,
            price,
            date,
            seller,
        })
    }

    /// Parse commercial frame (COMR)
    fn parse_commercial_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 2 {
            return Err(AudexError::InvalidData("COMR frame too short".to_string()));
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let mut offset = 1;

        // Parse price string (null-terminated)
        let price_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No price terminator in COMR frame".to_string())
        })? + offset;
        let price = String::from_utf8_lossy(&data[offset..price_end]).into_owned();
        offset = price_end + 1;

        // Parse valid until date (8 bytes YYYYMMDD)
        if offset + 8 > data.len() {
            return Err(AudexError::InvalidData(
                "COMR frame missing valid until date".to_string(),
            ));
        }
        let valid_until = String::from_utf8_lossy(&data[offset..offset + 8]).into_owned();
        offset += 8;

        // Parse contact URL (null-terminated)
        if offset >= data.len() {
            return Err(AudexError::InvalidData(
                "COMR frame missing contact URL".to_string(),
            ));
        }
        let contact_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No contact URL terminator in COMR frame".to_string())
        })? + offset;
        let contact_url = String::from_utf8_lossy(&data[offset..contact_end]).into_owned();
        offset = contact_end + 1;

        // Parse received as byte
        if offset >= data.len() {
            return Err(AudexError::InvalidData(
                "COMR frame missing received as byte".to_string(),
            ));
        }
        let received_as = data[offset];
        offset += 1;

        let remaining_data = &data[offset..];
        let null_term = encoding.null_terminator();

        // Parse seller name
        let seller_end = if null_term.len() == 1 {
            remaining_data
                .iter()
                .position(|&b| b == null_term[0])
                .unwrap_or(remaining_data.len())
        } else {
            // Truncate to even length so the 2-byte stepping stays aligned
            (0..(remaining_data.len() & !1).saturating_sub(1))
                .step_by(2)
                .find(|&i| remaining_data[i] == 0 && remaining_data[i + 1] == 0)
                .unwrap_or(remaining_data.len())
        };

        let seller = encoding.decode_text(&remaining_data[..seller_end])?;
        let desc_start = seller_end + null_term.len();

        let mut description = String::new();
        let mut picture_start = desc_start;

        if desc_start < remaining_data.len() {
            // Parse description
            let desc_end = if null_term.len() == 1 {
                remaining_data[desc_start..]
                    .iter()
                    .position(|&b| b == null_term[0])
                    .map(|pos| pos + desc_start)
                    .unwrap_or(remaining_data.len())
            } else {
                // Truncate to even length so the 2-byte stepping stays aligned
                (desc_start..(remaining_data.len() & !1).saturating_sub(1))
                    .step_by(2)
                    .find(|&i| remaining_data[i] == 0 && remaining_data[i + 1] == 0)
                    .unwrap_or(remaining_data.len())
            };

            description = encoding.decode_text(&remaining_data[desc_start..desc_end])?;
            picture_start = desc_end + null_term.len();
        }

        // Parse picture MIME type (null-terminated)
        let mut picture_mime = String::new();
        let mut picture_data_start = picture_start;

        if picture_start < remaining_data.len() {
            let mime_end = remaining_data[picture_start..]
                .iter()
                .position(|&b| b == 0)
                .map(|pos| pos + picture_start)
                .unwrap_or(remaining_data.len());
            picture_mime =
                String::from_utf8_lossy(&remaining_data[picture_start..mime_end]).into_owned();
            picture_data_start = mime_end + 1;
        }

        // Parse picture data
        let picture = if picture_data_start < remaining_data.len() {
            remaining_data[picture_data_start..].to_vec()
        } else {
            Vec::new()
        };

        Ok(FrameData::Commercial {
            encoding,
            price,
            valid_until,
            contact_url,
            received_as,
            seller,
            description,
            picture_mime,
            picture,
        })
    }

    /// Parse encryption method registration frame (ENCR)
    fn parse_encryption_method_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 3 {
            return Err(AudexError::InvalidData("ENCR frame too short".to_string()));
        }

        // Find owner identifier (null-terminated)
        let owner_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No owner terminator in ENCR frame".to_string())
        })?;
        let owner = String::from_utf8_lossy(&data[..owner_end]).into_owned();

        if owner_end + 2 > data.len() {
            return Err(AudexError::InvalidData(
                "ENCR frame missing method symbol".to_string(),
            ));
        }

        let method_symbol = data[owner_end + 1];
        let encryption_data = data[owner_end + 2..].to_vec();

        Ok(FrameData::EncryptionMethod {
            owner,
            method_symbol,
            encryption_data,
        })
    }

    /// Parse group identification frame (GRID)
    fn parse_group_id_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 3 {
            return Err(AudexError::InvalidData("GRID frame too short".to_string()));
        }

        // Find owner identifier (null-terminated)
        let owner_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No owner terminator in GRID frame".to_string())
        })?;
        let owner = String::from_utf8_lossy(&data[..owner_end]).into_owned();

        if owner_end + 2 > data.len() {
            return Err(AudexError::InvalidData(
                "GRID frame missing group symbol".to_string(),
            ));
        }

        let group_symbol = data[owner_end + 1];
        let group_data = data[owner_end + 2..].to_vec();

        Ok(FrameData::GroupIdentification {
            owner,
            group_symbol,
            group_data,
        })
    }

    /// Parse linked information frame (LINK)
    fn parse_linked_info_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            return Err(AudexError::InvalidData("LINK frame too short".to_string()));
        }

        // Parse frame identifier (4 bytes)
        let frame_id = String::from_utf8_lossy(&data[..4]).into_owned();
        let mut offset = 4;

        // Parse URL (null-terminated)
        let url_end = data[offset..].iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No URL terminator in LINK frame".to_string())
        })? + offset;
        let url = String::from_utf8_lossy(&data[offset..url_end]).into_owned();
        offset = url_end + 1;

        // Parse additional ID data
        let id_data = if offset < data.len() {
            data[offset..].to_vec()
        } else {
            Vec::new()
        };

        Ok(FrameData::LinkedInfo {
            frame_id,
            url,
            id_data,
        })
    }

    /// Parse music CD identifier frame (MCDI)
    fn parse_music_cd_id_frame(data: &[u8]) -> Result<Self> {
        Ok(FrameData::MusicCdId {
            cd_toc: data.to_vec(),
        })
    }

    /// Parse event timing codes frame (ETCO)
    fn parse_event_timing_frame(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(AudexError::InvalidData("ETCO frame too short".to_string()));
        }

        let format = TimeStampFormat::from(data[0]);
        let mut events = Vec::new();
        let mut pos = 1;

        while pos + 4 < data.len() {
            let event_type = data[pos];
            let timestamp =
                u32::from_be_bytes([data[pos + 1], data[pos + 2], data[pos + 3], data[pos + 4]]);

            events.push((event_type, timestamp));
            pos += 5;
        }

        Ok(FrameData::EventTiming { format, events })
    }

    /// Parse MPEG location lookup table frame (MLLT)
    fn parse_mpeg_location_lookup_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 10 {
            return Err(AudexError::InvalidData("MLLT frame too short".to_string()));
        }

        let frames_between_reference = u16::from_be_bytes([data[0], data[1]]);
        let bytes_between_reference = u32::from_be_bytes([0, data[2], data[3], data[4]]);
        let milliseconds_between_reference = u32::from_be_bytes([0, data[5], data[6], data[7]]);
        let bits_for_bytes = data[8];
        let bits_for_milliseconds = data[9];

        let mut references = Vec::new();
        let mut pos = 10;

        // Calculate bytes needed per reference entry.
        // Widen to u16 before adding to avoid u8 overflow when both values are large.
        let total_bits = bits_for_bytes as u16 + bits_for_milliseconds as u16;
        let bytes_per_entry = total_bits.div_ceil(8) as u8;

        // Both bit-width fields are zero, so each entry is zero bytes wide
        // and the loop below would never advance the read position.
        if bytes_per_entry == 0 {
            return Ok(FrameData::MpegLocationLookup {
                frames_between_reference,
                bytes_between_reference,
                milliseconds_between_reference,
                bits_for_bytes,
                bits_for_milliseconds,
                references,
            });
        }

        while pos + bytes_per_entry as usize <= data.len() {
            // Parse bytes deviation and milliseconds deviation based on bit widths
            let mut bytes_deviation = 0u32;
            let mut milliseconds_deviation = 0u32;

            // Simplified parsing - assume byte boundaries
            let byte_bytes = bits_for_bytes.div_ceil(8);
            let ms_bytes = bits_for_milliseconds.div_ceil(8);

            if pos + byte_bytes as usize <= data.len() {
                for i in 0..byte_bytes as usize {
                    bytes_deviation = (bytes_deviation << 8) | data[pos + i] as u32;
                }
                pos += byte_bytes as usize;
            }

            if pos + ms_bytes as usize <= data.len() {
                for i in 0..ms_bytes as usize {
                    milliseconds_deviation = (milliseconds_deviation << 8) | data[pos + i] as u32;
                }
                pos += ms_bytes as usize;
            }

            references.push((bytes_deviation, milliseconds_deviation));
        }

        Ok(FrameData::MpegLocationLookup {
            frames_between_reference,
            bytes_between_reference,
            milliseconds_between_reference,
            bits_for_bytes,
            bits_for_milliseconds,
            references,
        })
    }

    /// Parse synchronized tempo codes frame (SYTC)
    fn parse_sync_tempo_frame(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(AudexError::InvalidData("SYTC frame too short".to_string()));
        }

        let format = TimeStampFormat::from(data[0]);
        let tempo_data = data[1..].to_vec();

        Ok(FrameData::SyncTempo { format, tempo_data })
    }

    /// Parse relative volume adjustment frame (RVA2/RVAD)
    fn parse_relative_volume_frame(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Err(AudexError::InvalidData(
                "RVA2/RVAD frame too short".to_string(),
            ));
        }

        // Find identification string (null-terminated)
        let id_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No identification terminator in RVA2/RVAD frame".to_string())
        })?;
        // RVA2 identification is Latin-1 (ISO-8859-1) encoded, not UTF-8
        let identification = TextEncoding::Latin1.decode_text(&data[..id_end])?;

        let mut channels = Vec::new();
        let mut pos = id_end + 1;

        while pos + 4 <= data.len() {
            let channel_type = ChannelType::from(data[pos]);
            let adjustment = i16::from_be_bytes([data[pos + 1], data[pos + 2]]);
            let peak_bits = data[pos + 3];

            // Validate that the declared peak data actually exists in the
            // frame before accepting this channel entry
            let peak_bytes = peak_bits.div_ceil(8) as usize;
            if pos + 4 + peak_bytes > data.len() {
                return Err(AudexError::InvalidData(format!(
                    "RVA2 channel peak data ({} bytes for {} bits) extends \
                     beyond frame boundary at offset {}",
                    peak_bytes, peak_bits, pos
                )));
            }

            channels.push((channel_type, adjustment, peak_bits));
            pos += 4 + peak_bytes;
        }

        Ok(FrameData::RelativeVolumeAdjustment {
            identification,
            channels,
        })
    }

    /// Parse equalisation frame (EQU2/EQUA)
    fn parse_equalisation_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 2 {
            return Err(AudexError::InvalidData(
                "EQU2/EQUA frame too short".to_string(),
            ));
        }

        let method = data[0];

        // Find identification string (null-terminated)
        let id_end = data[1..].iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No identification terminator in EQU2/EQUA frame".to_string())
        })? + 1;
        let identification = String::from_utf8_lossy(&data[1..id_end]).into_owned();

        let mut adjustments = Vec::new();
        let mut pos = id_end + 1;

        while pos + 4 <= data.len() {
            let frequency = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let adjustment = i16::from_be_bytes([data[pos + 2], data[pos + 3]]);
            adjustments.push((frequency, adjustment));
            pos += 4;
        }

        Ok(FrameData::Equalisation {
            method,
            identification,
            adjustments,
        })
    }

    /// Parse reverb frame (RVRB)
    fn parse_reverb_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 12 {
            return Err(AudexError::InvalidData("RVRB frame too short".to_string()));
        }

        let reverb_left = u16::from_be_bytes([data[0], data[1]]);
        let reverb_right = u16::from_be_bytes([data[2], data[3]]);
        let reverb_bounces_left = data[4];
        let reverb_bounces_right = data[5];
        let reverb_feedback_left_to_left = data[6];
        let reverb_feedback_left_to_right = data[7];
        let reverb_feedback_right_to_right = data[8];
        let reverb_feedback_right_to_left = data[9];
        let premix_left_to_right = data[10];
        let premix_right_to_left = data[11];

        Ok(FrameData::Reverb {
            reverb_left,
            reverb_right,
            reverb_bounces_left,
            reverb_bounces_right,
            reverb_feedback_left_to_left,
            reverb_feedback_left_to_right,
            reverb_feedback_right_to_right,
            reverb_feedback_right_to_left,
            premix_left_to_right,
            premix_right_to_left,
        })
    }

    /// Parse recommended buffer size frame (RBUF)
    fn parse_recommended_buffer_size_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(AudexError::InvalidData("RBUF frame too short".to_string()));
        }

        let buffer_size = u32::from_be_bytes([0, data[0], data[1], data[2]]);
        let embedded_info_flag = data.len() > 3 && data[3] & 0x01 != 0;
        let offset_to_next_tag = if data.len() >= 8 {
            u32::from_be_bytes([data[4], data[5], data[6], data[7]])
        } else {
            0
        };

        Ok(FrameData::RecommendedBufferSize {
            buffer_size,
            embedded_info_flag,
            offset_to_next_tag,
        })
    }

    /// Parse audio encryption frame (AENC)
    fn parse_audio_encryption_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            return Err(AudexError::InvalidData("AENC frame too short".to_string()));
        }

        // Find owner identifier (null-terminated)
        let owner_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No owner terminator in AENC frame".to_string())
        })?;
        let owner = String::from_utf8_lossy(&data[..owner_end]).into_owned();

        if owner_end + 5 > data.len() {
            return Err(AudexError::InvalidData(
                "AENC frame missing preview data".to_string(),
            ));
        }

        let preview_start = u16::from_be_bytes([data[owner_end + 1], data[owner_end + 2]]);
        let preview_length = u16::from_be_bytes([data[owner_end + 3], data[owner_end + 4]]);
        let encryption_info = data[owner_end + 5..].to_vec();

        Ok(FrameData::AudioEncryption {
            owner,
            preview_start,
            preview_length,
            encryption_info,
        })
    }

    /// Parse position synchronisation frame (POSS)
    fn parse_position_sync_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 5 {
            return Err(AudexError::InvalidData("POSS frame too short".to_string()));
        }

        let format = TimeStampFormat::from(data[0]);
        let position = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);

        Ok(FrameData::PositionSync { format, position })
    }

    /// Parse signature frame (SIGN)
    fn parse_signature_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 2 {
            return Err(AudexError::InvalidData("SIGN frame too short".to_string()));
        }

        let group_symbol = data[0];
        let signature = data[1..].to_vec();

        Ok(FrameData::Signature {
            group_symbol,
            signature,
        })
    }

    /// Parse seek frame (SEEK)
    fn parse_seek_frame(data: &[u8]) -> Result<Self> {
        if data.len() < 4 {
            return Err(AudexError::InvalidData("SEEK frame too short".to_string()));
        }

        let minimum_offset = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);

        Ok(FrameData::Seek { minimum_offset })
    }

    /// Maximum recursion depth for CHAP/CTOC sub-frame parsing.
    /// Prevents stack overflow from crafted nested chapter structures.
    const MAX_SUBFRAME_DEPTH: usize = 4;

    /// Parse chapter frame (CHAP) with recursion depth tracking.
    fn parse_chapter_frame_depth(data: &[u8], version: (u8, u8), depth: usize) -> Result<Self> {
        if data.len() < 17 {
            return Err(AudexError::InvalidData("CHAP frame too short".to_string()));
        }

        // Find element ID (null-terminated)
        let id_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No element ID terminator in CHAP frame".to_string())
        })?;
        let element_id = String::from_utf8_lossy(&data[..id_end]).into_owned();

        if id_end + 17 > data.len() {
            return Err(AudexError::InvalidData(
                "CHAP frame missing timing data".to_string(),
            ));
        }

        let mut offset = id_end + 1;
        let start_time = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;
        let end_time = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;
        let start_offset = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;
        let end_offset = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        // Parse embedded sub-frames with depth tracking to prevent
        // unbounded recursion from nested CHAP/CTOC structures
        let sub_frames = Self::parse_sub_frames_depth(&data[offset..], version, depth)?;

        Ok(FrameData::Chapter {
            element_id,
            start_time,
            end_time,
            start_offset,
            end_offset,
            sub_frames,
        })
    }

    /// Parse table of contents frame (CTOC)
    fn parse_table_of_contents_frame_depth(
        data: &[u8],
        version: (u8, u8),
        depth: usize,
    ) -> Result<Self> {
        if data.len() < 3 {
            return Err(AudexError::InvalidData("CTOC frame too short".to_string()));
        }

        // Find element ID (null-terminated)
        let id_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
            AudexError::InvalidData("No element ID terminator in CTOC frame".to_string())
        })?;
        let element_id = String::from_utf8_lossy(&data[..id_end]).into_owned();

        if id_end + 3 > data.len() {
            return Err(AudexError::InvalidData(
                "CTOC frame missing flags and count".to_string(),
            ));
        }

        let mut offset = id_end + 1;
        let flags = data[offset];
        offset += 1;
        let entry_count = data[offset];
        offset += 1;

        let mut child_elements = Vec::new();
        for _ in 0..entry_count {
            if offset >= data.len() {
                break;
            }

            // Find child element ID (null-terminated)
            let child_end = data[offset..]
                .iter()
                .position(|&b| b == 0)
                .map(|pos| pos + offset)
                .unwrap_or(data.len());
            let child_id = String::from_utf8_lossy(&data[offset..child_end]).into_owned();
            child_elements.push(child_id);
            offset = child_end + 1;
        }

        // Parse embedded sub-frames with depth tracking
        let sub_frames = Self::parse_sub_frames_depth(&data[offset..], version, depth)?;

        Ok(FrameData::TableOfContents {
            element_id,
            flags,
            child_elements,
            sub_frames,
        })
    }

    /// Parse sub-frames from a byte slice with recursion depth tracking.
    /// Used by both CHAP and CTOC parsers to prevent unbounded recursion
    /// when a sub-frame itself contains CHAP/CTOC with further sub-frames.
    fn parse_sub_frames_depth(
        data: &[u8],
        version: (u8, u8),
        current_depth: usize,
    ) -> Result<Vec<FrameData>> {
        let next_depth = current_depth + 1;
        if next_depth > Self::MAX_SUBFRAME_DEPTH {
            // Silently stop parsing sub-frames beyond the depth limit
            // rather than returning an error, so the parent frame is still usable
            return Ok(Vec::new());
        }

        let mut sub_frames = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            if offset + 10 > data.len() {
                break;
            }

            let frame_id = String::from_utf8_lossy(&data[offset..offset + 4]).into_owned();
            if !crate::id3::util::is_valid_frame_id(&frame_id) {
                break;
            }

            let frame_size = if version.1 == 4 {
                super::util::decode_synchsafe_int_checked(&data[offset + 4..offset + 8])? as usize
            } else {
                u32::from_be_bytes([
                    data[offset + 4],
                    data[offset + 5],
                    data[offset + 6],
                    data[offset + 7],
                ]) as usize
            };
            let flags = u16::from_be_bytes([data[offset + 8], data[offset + 9]]);
            offset += 10;

            let end = offset
                .checked_add(frame_size)
                .ok_or_else(|| AudexError::InvalidData("Sub-frame offset overflow".to_string()))?;
            if end > data.len() {
                break;
            }

            let frame_data = data[offset..end].to_vec();
            offset = end;

            let header = FrameHeader::new(frame_id.clone(), frame_size as u32, flags, version);

            // For CHAP/CTOC sub-frames, use depth-tracked parsing.
            // All other frame types go through the normal from_bytes path.
            let parse_result = match frame_id.as_str() {
                "CHAP" => Self::parse_chapter_frame_depth(&frame_data, version, next_depth),
                "CTOC" => {
                    Self::parse_table_of_contents_frame_depth(&frame_data, version, next_depth)
                }
                _ => {
                    // Non-recursive frames are safe to parse normally
                    Self::from_bytes(&header, frame_data)
                }
            };

            if let Ok(sub_frame) = parse_result {
                sub_frames.push(sub_frame);
            }
        }

        Ok(sub_frames)
    }

    /// Write text frame
    fn write_text_frame(
        encoding: TextEncoding,
        text: &[String],
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let encoding = if !encoding.is_valid_for_version(version) {
            TextEncoding::Utf16 // Fallback for ID3v2.3
        } else {
            encoding
        };

        let mut data = vec![encoding.to_byte()];

        let separator = if version == (2, 3) { "/" } else { "\0" };
        let text_str = text.join(separator);
        data.extend(encoding.encode_text(&text_str)?);

        Ok(data)
    }

    /// Write user text frame
    fn write_user_text_frame(
        encoding: TextEncoding,
        description: &str,
        text: &[String],
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let encoding = if !encoding.is_valid_for_version(version) {
            TextEncoding::Utf16
        } else {
            encoding
        };

        let mut data = vec![encoding.to_byte()];
        data.extend(encoding.encode_text(description)?);
        data.extend(encoding.null_terminator());

        let separator = if version == (2, 3) { "/" } else { "\0" };
        let text_str = text.join(separator);
        data.extend(encoding.encode_text(&text_str)?);

        Ok(data)
    }

    /// Write comment frame
    fn write_comment_frame(
        encoding: TextEncoding,
        language: [u8; 3],
        description: &str,
        text: &str,
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let encoding = if !encoding.is_valid_for_version(version) {
            TextEncoding::Utf16
        } else {
            encoding
        };

        let mut data = vec![encoding.to_byte()];
        data.extend_from_slice(&language);
        data.extend(encoding.encode_text(description)?);
        data.extend(encoding.null_terminator());
        data.extend(encoding.encode_text(text)?);
        data.extend(encoding.null_terminator());

        Ok(data)
    }

    /// Write attached picture frame
    fn write_attached_picture_frame(
        encoding: TextEncoding,
        mime_type: &str,
        picture_type: PictureType,
        description: &str,
        picture_data: &[u8],
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let encoding = if !encoding.is_valid_for_version(version) {
            TextEncoding::Utf16
        } else {
            encoding
        };

        let mut data = vec![encoding.to_byte()];
        data.extend_from_slice(mime_type.as_bytes());
        data.push(0); // MIME type null terminator
        data.push(picture_type as u8);
        data.extend(encoding.encode_text(description)?);
        data.extend(encoding.null_terminator());
        data.extend_from_slice(picture_data);

        Ok(data)
    }

    /// Write play counter frame
    fn write_play_counter_frame(count: u64) -> Result<Vec<u8>> {
        // Write as variable-length big-endian integer
        let mut data = Vec::new();
        let mut remaining = count;
        if remaining == 0 {
            data.push(0);
        } else {
            let mut bytes = Vec::new();
            while remaining > 0 {
                bytes.push((remaining & 0xFF) as u8);
                remaining >>= 8;
            }
            bytes.reverse();
            data.extend(bytes);
        }
        Ok(data)
    }

    /// Write popularimeter frame
    fn write_popularimeter_frame(email: &str, rating: u8, count: u64) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(email.as_bytes());
        data.push(0); // Email null terminator
        data.push(rating);

        // Write count as variable-length big-endian integer
        let mut remaining = count;
        if remaining == 0 {
            data.push(0);
        } else {
            let mut bytes = Vec::new();
            while remaining > 0 {
                bytes.push((remaining & 0xFF) as u8);
                remaining >>= 8;
            }
            bytes.reverse();
            data.extend(bytes);
        }

        Ok(data)
    }

    /// Write user URL frame
    fn write_user_url_frame(
        encoding: TextEncoding,
        description: &str,
        url: &str,
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let encoding = if !encoding.is_valid_for_version(version) {
            TextEncoding::Utf16
        } else {
            encoding
        };

        let mut data = vec![encoding.to_byte()];
        data.extend(encoding.encode_text(description)?);
        data.extend(encoding.null_terminator());
        data.extend_from_slice(url.as_bytes());

        Ok(data)
    }

    /// Write unsynchronized lyrics frame
    fn write_unsync_lyrics_frame(
        encoding: TextEncoding,
        language: [u8; 3],
        description: &str,
        lyrics: &str,
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let encoding = if !encoding.is_valid_for_version(version) {
            TextEncoding::Utf16
        } else {
            encoding
        };

        let mut data = vec![encoding.to_byte()];
        data.extend_from_slice(&language);
        data.extend(encoding.encode_text(description)?);
        data.extend(encoding.null_terminator());
        data.extend(encoding.encode_text(lyrics)?);
        data.extend(encoding.null_terminator());

        Ok(data)
    }

    /// Write general encapsulated object frame
    fn write_general_object_frame(
        encoding: TextEncoding,
        mime_type: &str,
        filename: &str,
        description: &str,
        object_data: &[u8],
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let encoding = if !encoding.is_valid_for_version(version) {
            TextEncoding::Utf16
        } else {
            encoding
        };

        let mut data = vec![encoding.to_byte()];
        data.extend_from_slice(mime_type.as_bytes());
        data.push(0); // MIME type null terminator
        data.extend(encoding.encode_text(filename)?);
        data.extend(encoding.null_terminator());
        data.extend(encoding.encode_text(description)?);
        data.extend(encoding.null_terminator());
        data.extend_from_slice(object_data);

        Ok(data)
    }

    /// Write private frame
    fn write_private_frame(owner: &str, private_data: &[u8]) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(owner.as_bytes());
        data.push(0); // Owner null terminator
        data.extend_from_slice(private_data);

        Ok(data)
    }

    /// Write unique file identifier frame
    fn write_unique_file_id_frame(owner: &str, identifier: &[u8]) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(owner.as_bytes());
        data.push(0); // Owner null terminator
        data.extend_from_slice(identifier);

        Ok(data)
    }

    /// Write terms of use frame
    fn write_terms_of_use_frame(
        encoding: TextEncoding,
        language: [u8; 3],
        text: &str,
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let encoding = if !encoding.is_valid_for_version(version) {
            TextEncoding::Utf16
        } else {
            encoding
        };

        let mut data = vec![encoding.to_byte()];
        data.extend_from_slice(&language);
        data.extend(encoding.encode_text(text)?);

        Ok(data)
    }

    /// Write ownership frame
    fn write_ownership_frame(
        encoding: TextEncoding,
        price: &str,
        date: &str,
        seller: &str,
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let encoding = if !encoding.is_valid_for_version(version) {
            TextEncoding::Utf16
        } else {
            encoding
        };

        let mut data = vec![encoding.to_byte()];
        data.extend(encoding.encode_text(price)?);
        data.extend(encoding.null_terminator());
        data.extend_from_slice(date.as_bytes());
        data.extend(encoding.encode_text(seller)?);

        Ok(data)
    }

    /// Write commercial frame
    fn write_commercial_frame(params: &CommercialFrameParams) -> Result<Vec<u8>> {
        let mut data = vec![params.encoding.to_byte()];
        data.extend_from_slice(params.price.as_bytes());
        data.push(0);
        data.extend_from_slice(params.valid_until.as_bytes());
        data.extend_from_slice(params.contact_url.as_bytes());
        data.push(0);
        data.push(params.received_as);
        data.extend(params.encoding.encode_text(params.seller)?);
        data.extend(params.encoding.null_terminator());
        data.extend(params.encoding.encode_text(params.description)?);
        data.extend(params.encoding.null_terminator());
        data.extend_from_slice(params.picture_mime.as_bytes());
        data.push(0);
        data.extend_from_slice(params.picture);
        Ok(data)
    }

    /// Write encryption method frame
    fn write_encryption_method_frame(
        owner: &str,
        method_symbol: u8,
        encryption_data: &[u8],
    ) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(owner.as_bytes());
        data.push(0);
        data.push(method_symbol);
        data.extend_from_slice(encryption_data);
        Ok(data)
    }

    /// Write group identification frame
    fn write_group_id_frame(owner: &str, group_symbol: u8, group_data: &[u8]) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(owner.as_bytes());
        data.push(0);
        data.push(group_symbol);
        data.extend_from_slice(group_data);
        Ok(data)
    }

    /// Write linked information frame
    fn write_linked_info_frame(frame_id: &str, url: &str, id_data: &[u8]) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(frame_id.as_bytes());
        data.extend_from_slice(url.as_bytes());
        data.push(0);
        data.extend_from_slice(id_data);
        Ok(data)
    }

    /// Write event timing codes frame
    fn write_event_timing_frame(format: TimeStampFormat, events: &[(u8, u32)]) -> Result<Vec<u8>> {
        let mut data = vec![format as u8];
        for &(event_type, timestamp) in events {
            data.push(event_type);
            data.extend_from_slice(&timestamp.to_be_bytes());
        }
        Ok(data)
    }

    /// Write MPEG location lookup table frame
    fn write_mpeg_location_lookup_frame(
        frames_between_reference: u16,
        bytes_between_reference: u32,
        milliseconds_between_reference: u32,
        bits_for_bytes: u8,
        bits_for_milliseconds: u8,
        references: &[(u32, u32)],
    ) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&frames_between_reference.to_be_bytes());
        data.extend_from_slice(&bytes_between_reference.to_be_bytes()[1..]);
        data.extend_from_slice(&milliseconds_between_reference.to_be_bytes()[1..]);
        data.push(bits_for_bytes);
        data.push(bits_for_milliseconds);

        for &(bytes_deviation, milliseconds_deviation) in references {
            let byte_bytes = bits_for_bytes.div_ceil(8);
            let ms_bytes = bits_for_milliseconds.div_ceil(8);

            for i in 0..byte_bytes {
                let shift = (byte_bytes - 1 - i) * 8;
                data.push((bytes_deviation >> shift) as u8);
            }

            for i in 0..ms_bytes {
                let shift = (ms_bytes - 1 - i) * 8;
                data.push((milliseconds_deviation >> shift) as u8);
            }
        }
        Ok(data)
    }

    /// Write synchronized tempo codes frame
    fn write_sync_tempo_frame(format: TimeStampFormat, tempo_data: &[u8]) -> Result<Vec<u8>> {
        let mut data = vec![format as u8];
        data.extend_from_slice(tempo_data);
        Ok(data)
    }

    /// Write synchronized lyrics frame
    fn write_sync_lyrics_frame(
        encoding: TextEncoding,
        language: [u8; 3],
        format: TimeStampFormat,
        content_type: u8,
        description: &str,
        lyrics: &[(String, u32)],
        _version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let mut data = vec![encoding.to_byte()];
        data.extend_from_slice(&language);
        data.push(format as u8);
        data.push(content_type);
        data.extend(encoding.encode_text(description)?);
        data.extend(encoding.null_terminator());

        for (text, timestamp) in lyrics {
            data.extend(encoding.encode_text(text)?);
            data.extend(encoding.null_terminator());
            data.extend_from_slice(&timestamp.to_be_bytes());
        }
        Ok(data)
    }

    /// Write relative volume adjustment frame
    fn write_relative_volume_frame(
        identification: &str,
        channels: &[(ChannelType, i16, u8)],
    ) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(identification.as_bytes());
        data.push(0);

        for &(channel_type, adjustment, peak_bits) in channels {
            data.push(channel_type as u8);
            data.extend_from_slice(&adjustment.to_be_bytes());
            data.push(peak_bits);
            let peak_bytes = peak_bits.div_ceil(8);
            data.extend(std::iter::repeat_n(0, peak_bytes as usize));
        }
        Ok(data)
    }

    /// Write equalisation frame
    fn write_equalisation_frame(
        method: u8,
        identification: &str,
        adjustments: &[(u16, i16)],
    ) -> Result<Vec<u8>> {
        let mut data = vec![method];
        data.extend_from_slice(identification.as_bytes());
        data.push(0);

        for &(frequency, adjustment) in adjustments {
            data.extend_from_slice(&frequency.to_be_bytes());
            data.extend_from_slice(&adjustment.to_be_bytes());
        }
        Ok(data)
    }

    /// Write reverb frame
    fn write_reverb_frame(params: &ReverbFrameParams) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&params.reverb_left.to_be_bytes());
        data.extend_from_slice(&params.reverb_right.to_be_bytes());
        data.push(params.reverb_bounces_left);
        data.push(params.reverb_bounces_right);
        data.push(params.reverb_feedback_left_to_left);
        data.push(params.reverb_feedback_left_to_right);
        data.push(params.reverb_feedback_right_to_right);
        data.push(params.reverb_feedback_right_to_left);
        data.push(params.premix_left_to_right);
        data.push(params.premix_right_to_left);
        Ok(data)
    }

    /// Write recommended buffer size frame
    fn write_recommended_buffer_size_frame(
        buffer_size: u32,
        embedded_info_flag: bool,
        offset_to_next_tag: u32,
    ) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&buffer_size.to_be_bytes()[1..]);
        data.push(if embedded_info_flag { 0x01 } else { 0x00 });
        data.extend_from_slice(&offset_to_next_tag.to_be_bytes());
        Ok(data)
    }

    /// Write audio encryption frame
    fn write_audio_encryption_frame(
        owner: &str,
        preview_start: u16,
        preview_length: u16,
        encryption_info: &[u8],
    ) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(owner.as_bytes());
        data.push(0);
        data.extend_from_slice(&preview_start.to_be_bytes());
        data.extend_from_slice(&preview_length.to_be_bytes());
        data.extend_from_slice(encryption_info);
        Ok(data)
    }

    /// Write position synchronisation frame
    fn write_position_sync_frame(format: TimeStampFormat, position: u32) -> Result<Vec<u8>> {
        let mut data = vec![format as u8];
        data.extend_from_slice(&position.to_be_bytes());
        Ok(data)
    }

    /// Write signature frame
    fn write_signature_frame(group_symbol: u8, signature: &[u8]) -> Result<Vec<u8>> {
        let mut data = vec![group_symbol];
        data.extend_from_slice(signature);
        Ok(data)
    }

    /// Write seek frame
    fn write_seek_frame(minimum_offset: u32) -> Result<Vec<u8>> {
        Ok(minimum_offset.to_be_bytes().to_vec())
    }

    /// Write chapter frame
    fn write_chapter_frame(
        element_id: &str,
        start_time: u32,
        end_time: u32,
        start_offset: u32,
        end_offset: u32,
        sub_frames: &[FrameData],
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(element_id.as_bytes());
        data.push(0);
        data.extend_from_slice(&start_time.to_be_bytes());
        data.extend_from_slice(&end_time.to_be_bytes());
        data.extend_from_slice(&start_offset.to_be_bytes());
        data.extend_from_slice(&end_offset.to_be_bytes());

        for sub_frame in sub_frames {
            let sub_frame_data = sub_frame.to_bytes(version)?;
            append_embedded_frame(&mut data, sub_frame.id(), &sub_frame_data, version)?;
        }
        Ok(data)
    }

    /// Write table of contents frame
    fn write_table_of_contents_frame(
        element_id: &str,
        flags: u8,
        child_elements: &[String],
        sub_frames: &[FrameData],
        version: (u8, u8),
    ) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(element_id.as_bytes());
        data.push(0);
        data.push(flags);
        data.push(child_elements.len() as u8);

        for child in child_elements {
            data.extend_from_slice(child.as_bytes());
            data.push(0);
        }

        for sub_frame in sub_frames {
            let sub_frame_data = sub_frame.to_bytes(version)?;
            append_embedded_frame(&mut data, sub_frame.id(), &sub_frame_data, version)?;
        }
        Ok(data)
    }

    /// Parse TCON genre frame with sophisticated genre parsing
    fn parse_genre_frame(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Ok(FrameData::Genre {
                encoding: TextEncoding::Latin1,
                genres: vec![],
            });
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let text_data = &data[1..];
        let text = encoding.decode_text(text_data)?;

        let genres = Self::parse_genre_text(&text);

        Ok(FrameData::Genre { encoding, genres })
    }

    /// Parse genre text with ID3v1 genre lookup and complex expressions
    pub fn parse_genre_text(text: &str) -> Vec<String> {
        let mut all_genres = Vec::new();

        // First, split by null separators to handle multiple genres
        let genre_parts: Vec<&str> = text.split('\0').filter(|s| !s.is_empty()).collect();

        for part in genre_parts {
            let mut genres = Vec::new();

            // Handle simple numeric case first
            if part.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(genre_id) = part.parse::<usize>() {
                    if let Some(genre) = ID3V1_GENRES.get(genre_id) {
                        genres.push(genre.to_string());
                    } else {
                        genres.push("Unknown".to_string());
                    }
                    all_genres.extend(genres);
                    continue;
                }
            }

            // Handle special cases
            if part == "CR" {
                all_genres.push("Cover".to_string());
                continue;
            }
            if part == "RX" {
                all_genres.push("Remix".to_string());
                continue;
            }

            // Parse complex genre expressions like "(17)(4)Goa"
            let mut remaining = part;

            // Extract parenthesized genre IDs
            while remaining.starts_with('(') {
                if let Some(end_paren) = remaining.find(')') {
                    let genre_id_str = &remaining[1..end_paren];
                    remaining = &remaining[end_paren + 1..];

                    if let Ok(genre_id) = genre_id_str.parse::<usize>() {
                        if let Some(genre) = ID3V1_GENRES.get(genre_id) {
                            genres.push(genre.to_string());
                        } else {
                            genres.push("Unknown".to_string());
                        }
                    } else if genre_id_str == "CR" {
                        genres.push("Cover".to_string());
                    } else if genre_id_str == "RX" {
                        genres.push("Remix".to_string());
                    } else {
                        genres.push("Unknown".to_string());
                    }
                } else {
                    break;
                }
            }

            // Add any remaining text as a literal genre
            if !remaining.is_empty() {
                let mut name = remaining.to_string();
                // Handle escaped parentheses "((something" -> "(something"
                if name.starts_with("((") {
                    name = name[1..].to_string();
                }
                if !genres.contains(&name) {
                    genres.push(name);
                }
            }

            // If no genres parsed for this part and part is not empty, add as literal
            if genres.is_empty() && !part.is_empty() {
                genres.push(part.to_string());
            }

            all_genres.extend(genres);
        }

        // If no genres parsed and text is not empty, add as literal (fallback)
        if all_genres.is_empty() && !text.is_empty() {
            all_genres.push(text.to_string());
        }

        all_genres
    }

    /// Parse paired text frames (TIPL, TMCL) for key-value associations
    pub fn parse_paired_text_frame(frame_id: &str, data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Ok(FrameData::PairedText {
                id: frame_id.to_string(),
                encoding: TextEncoding::Latin1,
                people: vec![],
            });
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let text_data = &data[1..];

        // Parse paired text data (alternating key-value pairs)
        let null_term = encoding.null_terminator();
        let mut people = Vec::new();

        if null_term.len() == 1 {
            // Single-byte encodings
            let parts: Vec<_> = text_data
                .split(|&b| b == null_term[0])
                .filter(|part| !part.is_empty())
                .map(|part| encoding.decode_text(part))
                .collect::<Result<Vec<_>>>()?;

            // Group into pairs
            for pair in parts.chunks(2) {
                if pair.len() == 2 {
                    people.push((pair[0].clone(), pair[1].clone()));
                }
            }
        } else {
            // Multi-byte encodings (UTF-16)
            let mut parts = Vec::new();
            let mut start = 0;

            while start < text_data.len() {
                let mut end = start;
                while end + 1 < text_data.len() {
                    if text_data[end] == 0 && text_data[end + 1] == 0 {
                        break;
                    }
                    end += 2;
                }

                if end > start {
                    let part = encoding.decode_text(&text_data[start..end])?;
                    if !part.is_empty() {
                        parts.push(part);
                    }
                }
                start = end + 2;
            }

            // Group into pairs
            for pair in parts.chunks(2) {
                if pair.len() == 2 {
                    people.push((pair[0].clone(), pair[1].clone()));
                }
            }
        }

        Ok(FrameData::PairedText {
            id: frame_id.to_string(),
            encoding,
            people,
        })
    }

    /// Parse numeric text frames that store numbers (TBPM, TLEN, etc.)
    pub fn parse_numeric_text_frame(frame_id: &str, data: &[u8]) -> Result<Self> {
        let text_frame = Self::parse_text_frame(frame_id, data)?;

        if let FrameData::Text { id, encoding, text } = text_frame {
            let value = text.first().and_then(|s| s.parse::<u64>().ok());

            Ok(FrameData::NumericText {
                id,
                encoding,
                text,
                value,
            })
        } else {
            Err(AudexError::InvalidData(
                "Failed to parse as text frame".to_string(),
            ))
        }
    }

    /// Parse numeric part text frames that extract first number (track numbers like "4/15")
    pub fn parse_numeric_part_text_frame(frame_id: &str, data: &[u8]) -> Result<Self> {
        let text_frame = Self::parse_text_frame(frame_id, data)?;

        if let FrameData::Text { id, encoding, text } = text_frame {
            let value = text
                .first()
                .and_then(|s| s.split('/').next())
                .and_then(|s| s.parse::<u64>().ok());

            Ok(FrameData::NumericPartText {
                id,
                encoding,
                text,
                value,
            })
        } else {
            Err(AudexError::InvalidData(
                "Failed to parse as text frame".to_string(),
            ))
        }
    }

    /// Parse timestamp text frames with parsed time information
    pub fn parse_timestamp_text_frame(frame_id: &str, data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Ok(FrameData::TimeStampText {
                id: frame_id.to_string(),
                encoding: TextEncoding::Latin1,
                timestamps: vec![],
            });
        }

        let encoding = TextEncoding::from_byte(data[0])?;
        let text_data = &data[1..];
        let text = encoding.decode_text(text_data)?;

        // Parse multiple timestamps separated by commas
        let timestamps: Vec<_> = text
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .map(|s| ID3TimeStamp::parse(s.trim()))
            .collect();

        Ok(FrameData::TimeStampText {
            id: frame_id.to_string(),
            encoding,
            timestamps,
        })
    }

    /// Upgrade ID3v2.2 frame ID to v2.3/v2.4 equivalent
    pub fn upgrade_v22_frame_id(v22_id: &str, target_version: (u8, u8)) -> String {
        // First check for v2.4 specific upgrades if targeting v2.4
        if target_version == (2, 4) {
            for &(old_id, new_id) in ID3V22_TO_V24_UPGRADES {
                if old_id == v22_id {
                    return new_id.to_string();
                }
            }
        }

        // Then check standard v2.2 to v2.3 upgrades
        for &(old_id, new_id) in ID3V22_UPGRADE_MAP {
            if old_id == v22_id {
                return new_id.to_string();
            }
        }

        // If no mapping found, return original ID (might be valid in later versions)
        v22_id.to_string()
    }

    /// Get the equivalent v2.3 frame for compatibility
    pub fn get_v23_frame(&self) -> Result<FrameData> {
        match self {
            // v2.4 frames that need downgrade to v2.3
            FrameData::TimeStampText {
                id,
                encoding,
                timestamps,
            } => {
                match id.as_str() {
                    "TDRC" => {
                        // Convert back to separate TYER, TDAT, TIME frames
                        // For simplicity, we'll create a TYER frame with just the year
                        if let Some(first_timestamp) = timestamps.first() {
                            if let Some(year) = first_timestamp.year {
                                Ok(FrameData::Text {
                                    id: "TYER".to_string(),
                                    encoding: *encoding,
                                    text: vec![year.to_string()],
                                })
                            } else {
                                Ok(self.clone())
                            }
                        } else {
                            Ok(self.clone())
                        }
                    }
                    "TDOR" => {
                        // Convert TDOR to TORY
                        if let Some(first_timestamp) = timestamps.first() {
                            if let Some(year) = first_timestamp.year {
                                Ok(FrameData::Text {
                                    id: "TORY".to_string(),
                                    encoding: *encoding,
                                    text: vec![year.to_string()],
                                })
                            } else {
                                Ok(self.clone())
                            }
                        } else {
                            Ok(self.clone())
                        }
                    }
                    _ => Ok(self.clone()),
                }
            }
            _ => Ok(self.clone()),
        }
    }

    /// Merge another frame into this frame (for handling duplicates)
    pub fn merge_frame(&mut self, other: FrameData) -> Result<()> {
        match (self, other) {
            // Text frame merging - combine text arrays
            (
                FrameData::Text {
                    text: self_text, ..
                },
                FrameData::Text {
                    text: other_text, ..
                },
            ) => {
                for text in other_text {
                    if !self_text.contains(&text) {
                        self_text.push(text);
                    }
                }
                Ok(())
            }

            // Genre merging - combine genre lists
            (
                FrameData::Genre {
                    genres: self_genres,
                    ..
                },
                FrameData::Genre {
                    genres: other_genres,
                    ..
                },
            ) => {
                for genre in other_genres {
                    if !self_genres.contains(&genre) {
                        self_genres.push(genre);
                    }
                }
                Ok(())
            }

            // Paired text merging - combine people lists
            (
                FrameData::PairedText {
                    people: self_people,
                    ..
                },
                FrameData::PairedText {
                    people: other_people,
                    ..
                },
            ) => {
                for person in other_people {
                    if !self_people.contains(&person) {
                        self_people.push(person);
                    }
                }
                Ok(())
            }

            // Numeric text merging - keep first value, merge text
            (
                FrameData::NumericText {
                    text: self_text, ..
                },
                FrameData::NumericText {
                    text: other_text, ..
                },
            ) => {
                for text in other_text {
                    if !self_text.contains(&text) {
                        self_text.push(text);
                    }
                }
                Ok(())
            }

            // Numeric part text merging - keep first value, merge text
            (
                FrameData::NumericPartText {
                    text: self_text, ..
                },
                FrameData::NumericPartText {
                    text: other_text, ..
                },
            ) => {
                for text in other_text {
                    if !self_text.contains(&text) {
                        self_text.push(text);
                    }
                }
                Ok(())
            }

            // Timestamp merging - combine timestamp arrays
            (
                FrameData::TimeStampText {
                    timestamps: self_timestamps,
                    ..
                },
                FrameData::TimeStampText {
                    timestamps: other_timestamps,
                    ..
                },
            ) => {
                for timestamp in other_timestamps {
                    // Check if timestamp already exists (by text comparison)
                    if !self_timestamps.iter().any(|ts| ts.text == timestamp.text) {
                        self_timestamps.push(timestamp);
                    }
                }
                Ok(())
            }

            // For other frame types, replacement is the default behavior
            _ => Err(AudexError::Unsupported(
                "Frame merging not supported for this frame type combination".to_string(),
            )),
        }
    }

    /// Generate a hash key for frame uniqueness
    pub fn hash_key(&self) -> String {
        match self {
            FrameData::Text { id, .. } => id.clone(),
            FrameData::UserText { description, .. } => format!("TXXX:{}", description),
            FrameData::Url { id, .. } => id.clone(),
            FrameData::UserUrl { description, .. } => format!("WXXX:{}", description),
            FrameData::Comment {
                description,
                language,
                ..
            } => {
                let lang_str = std::str::from_utf8(language).unwrap_or("unknown");
                format!("COMM:{}:{}", lang_str, description)
            }
            FrameData::UnsyncLyrics {
                description,
                language,
                ..
            } => {
                let lang_str = std::str::from_utf8(language).unwrap_or("unknown");
                format!("USLT:{}:{}", lang_str, description)
            }
            FrameData::AttachedPicture {
                picture_type,
                description,
                ..
            } => {
                format!("APIC:{}:{}", *picture_type as u8, description)
            }
            FrameData::GeneralObject { description, .. } => format!("GEOB:{}", description),
            FrameData::PlayCounter { .. } => "PCNT".to_string(),
            FrameData::Popularimeter { email, .. } => format!("POPM:{}", email),
            FrameData::Private { owner, .. } => format!("PRIV:{}", owner),
            FrameData::UniqueFileId { owner, .. } => format!("UFID:{}", owner),
            FrameData::TermsOfUse { language, .. } => {
                let lang_str = std::str::from_utf8(language).unwrap_or("unknown");
                format!("USER:{}", lang_str)
            }
            FrameData::Genre { .. } => "TCON".to_string(),
            FrameData::PairedText { id, .. } => id.clone(),
            FrameData::NumericText { id, .. } => id.clone(),
            FrameData::NumericPartText { id, .. } => id.clone(),
            FrameData::TimeStampText { id, .. } => id.clone(),
            _ => self.id().to_string(),
        }
    }
}

/// Base trait for all ID3 frame types supporting type-safe downcasting
pub trait Frame: fmt::Debug + Send + Sync + Any {
    fn frame_id(&self) -> &str;
    fn to_data(&self) -> Result<Vec<u8>>;
    fn description(&self) -> String;
    fn text_values(&self) -> Option<Vec<String>> {
        None
    }
    fn hash_key(&self) -> String {
        self.frame_id().to_string()
    }

    /// Merge another frame of the same type into this frame
    fn merge_frame(&mut self, _other: Box<dyn Frame>) -> Result<()> {
        // Default implementation: not supported
        Err(AudexError::Unsupported(
            "Frame merging not supported for this frame type".to_string(),
        ))
    }

    /// Get reference as Any trait object for type-safe downcasting
    fn as_any(&self) -> &dyn Any;

    /// Get mutable reference as Any trait object for type-safe downcasting
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Convert text encoding if needed for target ID3 version
    ///
    /// This method provides automatic encoding conversion for frames that
    /// support text encoding. For ID3v2.3 compatibility, UTF-8 and UTF-16BE
    /// encodings are automatically converted to UTF-16.
    ///
    /// Default implementation is a no-op for frames without encoding fields.
    /// Frames implementing HasEncoding should override this method.
    ///
    /// # Arguments
    /// * `version` - Target ID3 version as (major, minor) tuple
    fn convert_encoding_for_version(&mut self, _version: (u8, u8)) {
        // Default: no-op for frames without encoding
    }

    /// Return the original frame flags from when this frame was parsed.
    /// Used during save to preserve compression, unsync, and other flags.
    /// Default returns empty flags (all cleared) for backwards compatibility.
    fn frame_flags(&self) -> FrameFlags {
        FrameFlags::new()
    }

    /// Set the frame flags (e.g., after parsing from a tagged file)
    fn set_frame_flags(&mut self, _flags: FrameFlags) {
        // Default: no-op for frame types that don't store flags
    }
}

pub(crate) fn serialize_frame_for_version(frame: &dyn Frame, version: (u8, u8)) -> Result<Vec<u8>> {
    if let Some(chap) = frame.as_any().downcast_ref::<CHAP>() {
        return chap.to_data_for_version(version);
    }
    if let Some(ctoc) = frame.as_any().downcast_ref::<CTOC>() {
        return ctoc.to_data_for_version(version);
    }
    frame.to_data()
}

fn append_embedded_frame(
    data: &mut Vec<u8>,
    frame_id: &str,
    frame_data: &[u8],
    version: (u8, u8),
) -> Result<()> {
    let size = u32::try_from(frame_data.len()).map_err(|_| {
        AudexError::InvalidData(format!(
            "Embedded frame '{}' too large: {} bytes",
            frame_id,
            frame_data.len()
        ))
    })?;

    data.extend_from_slice(frame_id.as_bytes());
    if version.1 == 4 {
        data.extend_from_slice(&super::util::encode_synchsafe_int(size)?);
    } else {
        data.extend_from_slice(&size.to_be_bytes());
    }
    data.extend_from_slice(&0u16.to_be_bytes());
    data.extend_from_slice(frame_data);
    Ok(())
}

/// Trait for frames that contain a text encoding field
///
/// This trait provides a consistent interface for accessing and modifying
/// the text encoding of frames that support encoded text. It enables
/// automatic encoding conversion when writing tags with version-specific
/// encoding restrictions (e.g., ID3v2.3 only supports LATIN1 and UTF-16).
pub trait HasEncoding {
    /// Get the current text encoding for this frame
    fn get_encoding(&self) -> TextEncoding;

    /// Set the text encoding for this frame
    ///
    /// # Arguments
    /// * `encoding` - The new text encoding to use
    fn set_encoding(&mut self, encoding: TextEncoding);

    /// Convert encoding if needed for target ID3 version
    ///
    /// This method checks if the current encoding is valid for the target
    /// version and converts it if necessary. For ID3v2.3, prefers Latin1
    /// when all text fits, otherwise falls back to UTF-16.
    ///
    /// # Arguments
    /// * `version` - Target ID3 version as (major, minor) tuple
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        let current = self.get_encoding();
        if !current.is_valid_for_version(version) {
            // Default: convert to UTF-16 for v2.3 (overridden by frames with text)
            let new_encoding = match current {
                TextEncoding::Utf8 | TextEncoding::Utf16Be => TextEncoding::Utf16,
                enc => enc,
            };
            self.set_encoding(new_encoding);
        }
    }
}

/// Commercial frame parameters
#[derive(Debug, Clone)]
struct CommercialFrameParams<'a> {
    encoding: TextEncoding,
    price: &'a str,
    valid_until: &'a str,
    contact_url: &'a str,
    received_as: u8,
    seller: &'a str,
    description: &'a str,
    picture_mime: &'a str,
    picture: &'a [u8],
    _version: (u8, u8),
}

/// Reverb frame parameters
#[derive(Debug, Clone)]
struct ReverbFrameParams {
    reverb_left: u16,
    reverb_right: u16,
    reverb_bounces_left: u8,
    reverb_bounces_right: u8,
    reverb_feedback_left_to_left: u8,
    reverb_feedback_left_to_right: u8,
    reverb_feedback_right_to_right: u8,
    reverb_feedback_right_to_left: u8,
    premix_left_to_right: u8,
    premix_right_to_left: u8,
}

/// Generic ID3v2 text information frame (T*** and W*** families).
///
/// Used as the concrete type for all standard text frames (TIT2, TPE1, TALB, etc.)
/// and URL frames. Stores one or more text values with a text encoding.
/// Most frame-ID type aliases (e.g., `pub type TIT2 = TextFrame`) resolve to this.
#[derive(Debug, Clone, Default)]
pub struct TextFrame {
    /// Four-character frame identifier (e.g., "TIT2", "TPE1")
    pub frame_id: String,
    /// Text encoding used for serialization
    pub encoding: TextEncoding,
    /// One or more text values (multi-value for ID3v2.4 null-separated fields)
    pub text: Vec<String>,
    /// Original frame flags from the source file (compression, unsync, etc.)
    pub flags: FrameFlags,
}

impl TextFrame {
    pub fn new(frame_id: String, text: Vec<String>) -> Self {
        Self {
            frame_id,
            encoding: TextEncoding::Utf16, // Default to UTF-16 encoding
            text,
            flags: FrameFlags::new(),
        }
    }

    pub fn with_encoding(frame_id: String, encoding: TextEncoding, text: Vec<String>) -> Self {
        Self {
            frame_id,
            encoding,
            text,
            flags: FrameFlags::new(),
        }
    }

    /// Create a TextFrame with explicit frame flags (for preserving flags from parsed data)
    pub fn with_flags(frame_id: String, text: Vec<String>, flags: FrameFlags) -> Self {
        Self {
            frame_id,
            encoding: TextEncoding::Utf16,
            text,
            flags,
        }
    }

    pub fn single(frame_id: String, text: String) -> Self {
        Self::new(frame_id, vec![text])
    }
}

impl TextFrame {
    pub fn get_text(&self) -> String {
        self.text.join("\u{0000}")
    }

    pub fn set_text(&mut self, text: String) {
        self.text = vec![text];
    }
}

/// Implementation of encoding trait for text frames
impl HasEncoding for TextFrame {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }

    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        let current = self.get_encoding();
        if !current.is_valid_for_version(version) {
            self.set_encoding(TextEncoding::Utf16);
        }
    }
}

impl Frame for TextFrame {
    fn frame_id(&self) -> &str {
        &self.frame_id
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];
        // For UTF-16 multi-value frames, encode each value separately so each gets
        // its own BOM
        if matches!(self.encoding, TextEncoding::Utf16) && self.text.len() > 1 {
            for (i, val) in self.text.iter().enumerate() {
                if i > 0 {
                    data.extend(self.encoding.null_terminator());
                }
                data.extend(self.encoding.encode_text(val)?);
            }
        } else {
            let text_str = self.text.join("\0");
            data.extend(self.encoding.encode_text(&text_str)?);
        }
        // Add trailing null terminator (standard ID3v2 convention)
        data.extend(self.encoding.null_terminator());
        Ok(data)
    }

    fn description(&self) -> String {
        format!("{}: {}", self.frame_id, self.text.join("; "))
    }

    fn text_values(&self) -> Option<Vec<String>> {
        Some(self.text.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }

    fn frame_flags(&self) -> FrameFlags {
        self.flags.clone()
    }

    fn set_frame_flags(&mut self, flags: FrameFlags) {
        self.flags = flags;
    }
}

/// TXXX frame (user-defined text)
#[derive(Debug, Clone, Default)]
pub struct TXXX {
    pub encoding: TextEncoding,
    pub description: String,
    pub text: Vec<String>,
}

impl TXXX {
    pub fn new(encoding: TextEncoding, desc: String, text: Vec<String>) -> Self {
        Self {
            encoding,
            description: desc,
            text,
        }
    }

    pub fn single(encoding: TextEncoding, desc: String, text: String) -> Self {
        Self::new(encoding, desc, vec![text])
    }
}

/// Implementation of encoding trait for user-defined text frames
impl HasEncoding for TXXX {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }

    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        let current = self.get_encoding();
        if !current.is_valid_for_version(version) {
            self.set_encoding(TextEncoding::Utf16);
        }
    }
}

impl Frame for TXXX {
    fn frame_id(&self) -> &str {
        "TXXX"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];

        // Add description
        data.extend(self.encoding.encode_text(&self.description)?);
        data.extend(self.encoding.null_terminator());

        // Add text
        let text_str = self.text.join("\0");
        data.extend(self.encoding.encode_text(&text_str)?);
        // Add trailing null terminator (standard ID3v2 convention)
        data.extend(self.encoding.null_terminator());

        Ok(data)
    }

    fn description(&self) -> String {
        format!("TXXX: {} = {}", self.description, self.text.join("; "))
    }

    fn text_values(&self) -> Option<Vec<String>> {
        Some(self.text.clone())
    }

    fn hash_key(&self) -> String {
        format!("TXXX:{}", self.description)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

/// COMM frame (comment)
#[derive(Debug, Clone)]
pub struct COMM {
    pub encoding: TextEncoding,
    pub language: [u8; 3],
    pub description: String,
    pub text: String,
}

impl COMM {
    pub fn new(encoding: TextEncoding, lang: [u8; 3], desc: String, text: String) -> Self {
        Self {
            encoding,
            language: lang,
            description: desc,
            text,
        }
    }
}

/// Implementation of encoding trait for comment frames
impl HasEncoding for COMM {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }

    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        let current = self.get_encoding();
        if !current.is_valid_for_version(version) {
            self.set_encoding(TextEncoding::Utf16);
        }
    }
}

impl Frame for COMM {
    fn frame_id(&self) -> &str {
        "COMM"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];
        data.extend_from_slice(&self.language);
        data.extend(self.encoding.encode_text(&self.description)?);
        data.extend(self.encoding.null_terminator());
        data.extend(self.encoding.encode_text(&self.text)?);
        data.extend(self.encoding.null_terminator());
        Ok(data)
    }

    fn description(&self) -> String {
        format!("COMM: {} ({})", self.text, self.description)
    }

    fn text_values(&self) -> Option<Vec<String>> {
        Some(vec![self.text.clone()])
    }

    fn hash_key(&self) -> String {
        let lang_str = std::str::from_utf8(&self.language).unwrap_or("unknown");
        // Standard format: "COMM::lang" when description is empty, "COMM:desc:lang" otherwise
        if self.description.is_empty() {
            format!("COMM::{}", lang_str)
        } else {
            format!("COMM:{}:{}", self.description, lang_str)
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

impl Default for COMM {
    fn default() -> Self {
        Self {
            encoding: TextEncoding::default(), // Uses Utf8 per specs.rs
            language: *b"XXX",                 // Default language code
            description: String::new(),
            text: String::new(),
        }
    }
}

/// TCON frame (content type/genre)
pub type TCON = TextFrame;

// ID3v2.4/2.3 Text Frame Aliases - All Standard Text Frames
pub type TALB = TextFrame; // Album/Movie/Show title
pub type TBPM = TextFrame; // BPM (beats per minute)
pub type TCOM = TextFrame; // Composer
pub type TCOP = TextFrame; // Copyright message
pub type TCMP = TextFrame; // iTunes compilation flag
pub type TDAT = TextFrame; // Date
pub type TDEN = TextFrame; // Encoding time
pub type TDES = TextFrame; // Podcast description
pub type TDLY = TextFrame; // Playlist delay
pub type TDOR = TextFrame; // Original release time
pub type TDRC = TextFrame; // Recording time
pub type TDRL = TextFrame; // Release time
pub type TDTG = TextFrame; // Tagging time
pub type TENC = TextFrame; // Encoded by
pub type TEXT = TextFrame; // Lyricist/Text writer
pub type TFLT = TextFrame; // File type
pub type TGID = TextFrame; // Podcast identifier
pub type TIME = TextFrame; // Time
pub type TIT1 = TextFrame; // Content group description
pub type TIT2 = TextFrame; // Title/songname/content description
pub type TIT3 = TextFrame; // Subtitle/Description refinement
pub type TKEY = TextFrame; // Initial key
pub type TKWD = TextFrame; // Podcast keywords
pub type TLAN = TextFrame; // Language(s)
pub type TLEN = TextFrame; // Length
pub type TMED = TextFrame; // Media type
pub type TMOO = TextFrame; // Mood
pub type TOAL = TextFrame; // Original album/movie/show title
pub type TOFN = TextFrame; // Original filename
pub type TOLY = TextFrame; // Original lyricist(s)/text writer(s)
pub type TOPE = TextFrame; // Original artist(s)/performer(s)
pub type TORY = TextFrame; // Original release year
pub type TOWN = TextFrame; // File owner/licensee
pub type TPE1 = TextFrame; // Lead performer(s)/Soloist(s)
pub type TPE2 = TextFrame; // Band/orchestra/accompaniment
pub type TPE3 = TextFrame; // Conductor/performer refinement
pub type TPE4 = TextFrame; // Interpreted, remixed, or otherwise modified by
pub type TPOS = TextFrame; // Part of a set
pub type TPRO = TextFrame; // Produced notice
pub type TPUB = TextFrame; // Publisher
pub type TRCK = TextFrame; // Track number/Position in set
pub type TRDA = TextFrame; // Recording dates
pub type TRSN = TextFrame; // Internet radio station name
pub type TRSO = TextFrame; // Internet radio station owner
pub type TSIZ = TextFrame; // Size
pub type TSOA = TextFrame; // Album sort order
pub type TSOC = TextFrame; // Composer sort order
pub type TSOP = TextFrame; // Performer sort order
pub type TSOT = TextFrame; // Title sort order
pub type TSRC = TextFrame; // ISRC
pub type TSSE = TextFrame; // Software/Hardware and settings used for encoding
pub type TSST = TextFrame; // Set subtitle
pub type TYER = TextFrame; // Year
pub type TSO2 = TextFrame; // Album artist sort order
pub type TCAT = TextFrame; // Podcast category
pub type MVNM = TextFrame; // Movement name
pub type MVIN = TextFrame; // Movement number
pub type GRP1 = TextFrame; // Grouping

// ID3v2.4/2.3 URL Frame Aliases - All Standard URL Frames
pub type WCOM = TextFrame; // Commercial information
pub type WCOP = TextFrame; // Copyright/Legal information
pub type WFED = TextFrame; // Podcast feed
pub type WOAF = TextFrame; // Official audio file webpage
pub type WOAR = TextFrame; // Official artist/performer webpage
pub type WOAS = TextFrame; // Official audio source webpage
pub type WORS = TextFrame; // Official Internet radio station homepage
pub type WPAY = TextFrame; // Payment
pub type WPUB = TextFrame; // Publishers official webpage
// User defined URL link frame (WXXX) is implemented as a struct below

// ID3v2.4/2.3 Special Frame Aliases - Binary and Complex Frames
// SYLT, ETCO, SYTC, RVA2, and RVAD are now full implementations (see below)

/// EQU2 - Equalisation (2) frame
/// Allows adjustment of frequency equalization in the audio file
#[derive(Debug, Clone)]
pub struct EQU2 {
    /// Interpolation method (0=band, 1=linear)
    pub interpolation_method: u8,
    /// Device/system identification
    pub identification: String,
    /// Frequency/volume adjustment pairs (frequency in Hz * 2, adjustment in 1/512 dB)
    pub adjustments: Vec<(u16, i16)>,
}

impl EQU2 {
    pub fn new(method: u8, id: String, adjustments: Vec<(u16, i16)>) -> Self {
        Self {
            interpolation_method: method,
            identification: id,
            adjustments,
        }
    }
}

impl Frame for EQU2 {
    fn frame_id(&self) -> &str {
        "EQU2"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.interpolation_method];
        data.extend_from_slice(self.identification.as_bytes());
        data.push(0); // Null terminator
        for (freq, adj) in &self.adjustments {
            data.extend_from_slice(&freq.to_be_bytes());
            data.extend_from_slice(&adj.to_be_bytes());
        }
        Ok(data)
    }

    fn description(&self) -> String {
        format!(
            "EQU2: {} ({} adjustments)",
            self.identification,
            self.adjustments.len()
        )
    }

    fn hash_key(&self) -> String {
        format!("EQU2:{}", self.identification)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// MLLT - MPEG Location Lookup Table frame
/// Contains lookup table for rapid seeking in MPEG files
#[derive(Debug, Clone)]
pub struct MLLT {
    /// MPEG frames between each reference point
    pub frames_between_reference: u16,
    /// Bytes between each reference point
    pub bytes_between_reference: u32, // 24-bit value
    /// Milliseconds between each reference point
    pub millis_between_reference: u32, // 24-bit value
    /// Bits used for bytes deviation field
    pub bits_for_bytes_deviation: u8,
    /// Bits used for milliseconds deviation field
    pub bits_for_millis_deviation: u8,
    /// Binary deviation data
    pub data: Vec<u8>,
}

impl MLLT {
    pub fn new(
        frames: u16,
        bytes: u32,
        millis: u32,
        bits_bytes: u8,
        bits_millis: u8,
        data: Vec<u8>,
    ) -> Self {
        Self {
            frames_between_reference: frames,
            bytes_between_reference: bytes & 0xFFFFFF, // Ensure 24-bit
            millis_between_reference: millis & 0xFFFFFF, // Ensure 24-bit
            bits_for_bytes_deviation: bits_bytes,
            bits_for_millis_deviation: bits_millis,
            data,
        }
    }
}

impl Frame for MLLT {
    fn frame_id(&self) -> &str {
        "MLLT"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        data.extend_from_slice(&self.frames_between_reference.to_be_bytes());
        // Write 24-bit values (3 bytes each)
        data.extend_from_slice(&[
            (self.bytes_between_reference >> 16) as u8,
            (self.bytes_between_reference >> 8) as u8,
            self.bytes_between_reference as u8,
        ]);
        data.extend_from_slice(&[
            (self.millis_between_reference >> 16) as u8,
            (self.millis_between_reference >> 8) as u8,
            self.millis_between_reference as u8,
        ]);
        data.push(self.bits_for_bytes_deviation);
        data.push(self.bits_for_millis_deviation);
        data.extend_from_slice(&self.data);
        Ok(data)
    }

    fn description(&self) -> String {
        format!(
            "MLLT: {} frames between references",
            self.frames_between_reference
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// USER - Terms of Use frame
/// Contains licensing and usage terms for the audio file
#[derive(Debug, Clone)]
pub struct USER {
    /// Text encoding used for the text field
    pub encoding: TextEncoding,
    /// Language code (3 characters, ISO-639-2)
    pub language: [u8; 3],
    /// Terms of use text
    pub text: String,
}

impl USER {
    pub fn new(encoding: TextEncoding, lang: [u8; 3], text: String) -> Self {
        Self {
            encoding,
            language: lang,
            text,
        }
    }
}

/// Implementation of encoding trait for terms of use frames
impl HasEncoding for USER {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }
}

impl Frame for USER {
    fn frame_id(&self) -> &str {
        "USER"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];
        data.extend_from_slice(&self.language);
        data.extend(self.encoding.encode_text(&self.text)?);
        Ok(data)
    }

    fn description(&self) -> String {
        let lang_str = std::str::from_utf8(&self.language).unwrap_or("unknown");
        format!("USER: {} (lang: {})", self.text, lang_str)
    }

    fn text_values(&self) -> Option<Vec<String>> {
        Some(vec![self.text.clone()])
    }

    fn hash_key(&self) -> String {
        let lang_str = std::str::from_utf8(&self.language).unwrap_or("unknown");
        format!("USER:{}", lang_str)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

/// TIPL - Involved People List frame (ID3v2.4)
/// Contains list of people involved in the recording and their roles
#[derive(Debug, Clone)]
pub struct TIPL {
    /// Text encoding for the people list
    pub encoding: TextEncoding,
    /// List of involvement/person pairs (role, person name)
    pub people: Vec<(String, String)>,
}

impl TIPL {
    pub fn new(encoding: TextEncoding, people: Vec<(String, String)>) -> Self {
        Self { encoding, people }
    }
}

/// Implementation of encoding trait for involved people list frames
impl HasEncoding for TIPL {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }
}

impl Frame for TIPL {
    fn frame_id(&self) -> &str {
        "TIPL"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];
        for (role, person) in &self.people {
            data.extend(self.encoding.encode_text(role)?);
            data.extend(self.encoding.null_terminator());
            data.extend(self.encoding.encode_text(person)?);
            data.extend(self.encoding.null_terminator());
        }
        Ok(data)
    }

    fn description(&self) -> String {
        let roles: Vec<String> = self
            .people
            .iter()
            .map(|(role, person)| format!("{}: {}", role, person))
            .collect();
        format!("TIPL: {}", roles.join(", "))
    }

    fn text_values(&self) -> Option<Vec<String>> {
        Some(
            self.people
                .iter()
                .map(|(role, person)| format!("{}={}", role, person))
                .collect(),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

/// TMCL - Musician Credits List frame (ID3v2.4)
/// Contains list of musicians and the instruments they played
#[derive(Debug, Clone)]
pub struct TMCL {
    /// Text encoding for the credits list
    pub encoding: TextEncoding,
    /// List of instrument/musician pairs (instrument, musician name)
    pub credits: Vec<(String, String)>,
}

impl TMCL {
    pub fn new(encoding: TextEncoding, credits: Vec<(String, String)>) -> Self {
        Self { encoding, credits }
    }
}

/// Implementation of encoding trait for musician credits list frames
impl HasEncoding for TMCL {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }
}

impl Frame for TMCL {
    fn frame_id(&self) -> &str {
        "TMCL"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];
        for (instrument, musician) in &self.credits {
            data.extend(self.encoding.encode_text(instrument)?);
            data.extend(self.encoding.null_terminator());
            data.extend(self.encoding.encode_text(musician)?);
            data.extend(self.encoding.null_terminator());
        }
        Ok(data)
    }

    fn description(&self) -> String {
        let credits: Vec<String> = self
            .credits
            .iter()
            .map(|(inst, musician)| format!("{}: {}", inst, musician))
            .collect();
        format!("TMCL: {}", credits.join(", "))
    }

    fn text_values(&self) -> Option<Vec<String>> {
        Some(
            self.credits
                .iter()
                .map(|(inst, musician)| format!("{}={}", inst, musician))
                .collect(),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

pub type RVRB = TextFrame; // Reverb
// MCDI is now a full implementation (see below)
// UFID is now a full implementation (see below)
// OWNE is now a full implementation (see below)
// COMR is now a full implementation (see below)
// PRIV is now a full implementation (see below)
// CHAP and CTOC are now full implementations (see below)
// ASPI is now a full implementation (see below)
// POSS is now a full implementation (see below)
// ENCR is now a full implementation (see below)
// GRID is now a full implementation (see below)
// SIGN is now a full implementation (see below)
// SEEK is now a full implementation (see below)
// RBUF is now a full implementation (see below)
// AENC is now a full implementation (see below)
// LINK is now a full implementation (see below)
// PCNT is now a full implementation (see below)
pub type PCST = TextFrame; // Podcast flag

// ID3v2.2 Frame Aliases - All Legacy Frames
pub type TAL = TextFrame; // Album/Movie/Show title (v2.2)
pub type TBP = TextFrame; // BPM (beats per minute) (v2.2)
pub type TCM = TextFrame; // Composer (v2.2)
pub type TCO = TextFrame; // Content type (v2.2)
pub type TCR = TextFrame; // Copyright message (v2.2)
pub type TDA = TextFrame; // Date (v2.2)
pub type TDY = TextFrame; // Playlist delay (v2.2)
pub type TEN = TextFrame; // Encoded by (v2.2)
pub type TFT = TextFrame; // File type (v2.2)
pub type TIM = TextFrame; // Time (v2.2)
pub type TT1 = TextFrame; // Content group description (v2.2)
pub type TT2 = TextFrame; // Title/songname/content description (v2.2)
pub type TT3 = TextFrame; // Subtitle/Description refinement (v2.2)
pub type TKE = TextFrame; // Initial key (v2.2)
pub type TLA = TextFrame; // Language(s) (v2.2)
pub type TLE = TextFrame; // Length (v2.2)
pub type TMT = TextFrame; // Media type (v2.2)
pub type TOA = TextFrame; // Original artist(s)/performer(s) (v2.2)
pub type TOF = TextFrame; // Original filename (v2.2)
pub type TOL = TextFrame; // Original lyricist(s)/text writer(s) (v2.2)
pub type TOR = TextFrame; // Original release year (v2.2)
pub type TOT = TextFrame; // Original album/movie/show title (v2.2)
pub type TP1 = TextFrame; // Lead performer(s)/Soloist(s) (v2.2)
pub type TP2 = TextFrame; // Band/orchestra/accompaniment (v2.2)
pub type TP3 = TextFrame; // Conductor/performer refinement (v2.2)
pub type TP4 = TextFrame; // Interpreted, remixed, or otherwise modified by (v2.2)
pub type TPA = TextFrame; // Part of a set (v2.2)
pub type TPB = TextFrame; // Publisher (v2.2)
pub type TRC = TextFrame; // ISRC (v2.2)
pub type TRD = TextFrame; // Recording dates (v2.2)
pub type TRK = TextFrame; // Track number/position in set (v2.2)
pub type TSI = TextFrame; // Size (v2.2)
pub type TSS = TextFrame; // Software/Hardware and settings used for encoding (v2.2)
pub type TXX = TXXX; // User defined text information frame (v2.2)
pub type TYE = TextFrame; // Year (v2.2)

// ID3v2.2 Special Frame Aliases
pub type BUF = RBUF; // Recommended buffer size (v2.2) -> RBUF
pub type CNT = PCNT; // Play counter (v2.2) -> PCNT
pub type COM = COMM; // Comments (v2.2) -> COMM
pub type CRA = AENC; // Audio encryption (v2.2) -> AENC
pub type CRM = TextFrame; // Encrypted meta frame (v2.2)
pub type ETC = ETCO; // Event timing codes (v2.2) -> ETCO
pub type EQU = TextFrame; // Equalisation (v2.2)
pub type GEO = GEOB; // General encapsulated object (v2.2) -> GEOB
pub type IPL = TextFrame; // Involved people list (v2.2)
pub type LNK = LINK; // Linked information (v2.2) -> LINK
pub type MCI = MCDI; // Music CD Identifier (v2.2) -> MCDI
pub type MLL = TextFrame; // MPEG location lookup table (v2.2)
pub type PIC = APIC; // Attached picture (v2.2) -> APIC
pub type POP = POPM; // Popularimeter (v2.2) -> POPM
pub type REV = TextFrame; // Reverb (v2.2)
pub type RVA = RVAD; // Relative volume adjustment (v2.2) -> RVAD
pub type SLT = SYLT; // Synchronized lyric/text (v2.2) -> SYLT
pub type STC = SYTC; // Synced tempo codes (v2.2) -> SYTC
pub type UFI = UFID; // Unique file identifier (v2.2) -> UFID
pub type ULT = USLT; // Unsynchronised lyric/text transcription (v2.2) -> USLT

// ID3v2.2 URL Frame Aliases
pub type WAF = TextFrame; // Official audio file webpage (v2.2)
pub type WAR = TextFrame; // Official artist/performer webpage (v2.2)
pub type WAS = TextFrame; // Official audio source webpage (v2.2)
pub type WCM = TextFrame; // Commercial information (v2.2)
pub type WCP = TextFrame; // Copyright/Legal information (v2.2)
pub type WPB = TextFrame; // Publishers official webpage (v2.2)
pub type WXX = WXXX; // User defined URL link frame (v2.2)

/// Factory for decoding raw ID3v2 frame bytes into typed [`Frame`] objects.
///
/// Given a frame ID (e.g., `"COMM"`, `"APIC"`, `"TIT2"`) and raw payload bytes,
/// [`FrameRegistry::create_frame`] returns the appropriate strongly-typed frame.
/// Unknown frame IDs fall back to a generic `TextFrame`.
#[derive(Debug)]
pub struct FrameRegistry;

impl Default for FrameRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameRegistry {
    pub fn new() -> Self {
        Self
    }

    pub fn create_frame(frame_id: &str, data: &[u8]) -> Result<Box<dyn Frame>> {
        match frame_id {
            "COMM" => {
                if data.len() < 5 {
                    return Err(AudexError::InvalidData("COMM frame too short".to_string()));
                }

                let encoding = TextEncoding::from_byte(data[0])?;
                let language = [data[1], data[2], data[3]];
                let text_data = &data[4..];
                let null_term = encoding.null_terminator();

                // Find description terminator (encoding-aware)
                let desc_end = if null_term.len() == 1 {
                    // Single-byte encoding (UTF-8, Latin-1)
                    text_data
                        .iter()
                        .position(|&b| b == null_term[0])
                        .unwrap_or(text_data.len())
                } else {
                    // Multi-byte encoding (UTF-16); truncate to even length for alignment
                    (0..(text_data.len() & !1).saturating_sub(1))
                        .step_by(2)
                        .find(|&i| text_data[i] == 0 && text_data[i + 1] == 0)
                        .unwrap_or(text_data.len())
                };

                let description = encoding.decode_text(&text_data[..desc_end])?;
                let comment_start = desc_end + null_term.len();
                let text = if comment_start < text_data.len() {
                    encoding
                        .decode_text(&text_data[comment_start..])?
                        .trim_end_matches('\0')
                        .to_string()
                } else {
                    String::new()
                };

                // Use the actual encoding read from the frame
                Ok(Box::new(COMM::new(encoding, language, description, text)))
            }
            "APIC" => {
                if data.len() < 3 {
                    return Err(AudexError::InvalidData("APIC frame too short".to_string()));
                }

                let encoding = TextEncoding::from_byte(data[0])?;

                // Find MIME type null terminator
                let mime_end = data[1..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(data.len() - 1)
                    + 1;

                let mime_type = String::from_utf8_lossy(&data[1..mime_end]).into_owned();

                if mime_end + 2 >= data.len() {
                    return Err(AudexError::InvalidData("APIC frame corrupted".to_string()));
                }

                let picture_type = PictureType::from(data[mime_end + 1]);

                // Find description null terminator
                let desc_start = mime_end + 2;
                let remaining_data = &data[desc_start..];
                let null_term = encoding.null_terminator();

                // Find description end based on encoding
                let desc_end = if null_term.len() == 1 {
                    remaining_data
                        .iter()
                        .position(|&b| b == null_term[0])
                        .unwrap_or(remaining_data.len())
                } else {
                    // UTF-16 double null; truncate to even length for alignment
                    (0..(remaining_data.len() & !1).saturating_sub(1))
                        .step_by(2)
                        .find(|&i| remaining_data[i] == 0 && remaining_data[i + 1] == 0)
                        .unwrap_or(remaining_data.len())
                } + desc_start;

                let description = encoding.decode_text(&data[desc_start..desc_end])?;

                if desc_end == data.len() {
                    return Err(AudexError::InvalidData(
                        "APIC frame description is not null-terminated".to_string(),
                    ));
                }

                // Get image data
                let image_start = desc_end + encoding.null_terminator().len();
                crate::limits::ParseLimits::default()
                    .check_image_size(data[image_start..].len() as u64, "ID3 APIC image")?;
                let image_data = if image_start < data.len() {
                    data[image_start..].to_vec()
                } else {
                    Vec::new()
                };

                Ok(Box::new(APIC::new(
                    TextEncoding::Utf8,
                    mime_type,
                    picture_type,
                    description,
                    image_data,
                )))
            }
            "POPM" => {
                if data.is_empty() {
                    return Err(AudexError::InvalidData("POPM frame is empty".to_string()));
                }

                // Find email null terminator
                let email_end = data.iter().position(|&b| b == 0).unwrap_or(data.len());

                let email = String::from_utf8_lossy(&data[..email_end]).into_owned();

                if email_end + 1 >= data.len() {
                    return Err(AudexError::InvalidData(
                        "POPM frame missing rating".to_string(),
                    ));
                }

                let rating = data[email_end + 1];

                // Count is optional and can be 0, 1, 2, 3, or 4 bytes
                let count = if email_end + 2 < data.len() {
                    let count_bytes = &data[email_end + 2..];
                    match count_bytes.len() {
                        4 => Some(u32::from_be_bytes([
                            count_bytes[0],
                            count_bytes[1],
                            count_bytes[2],
                            count_bytes[3],
                        ])),
                        _ => {
                            // Handle partial counts by padding with zeros
                            let mut padded = [0u8; 4];
                            let copy_len = count_bytes.len().min(4);
                            padded[4 - copy_len..].copy_from_slice(&count_bytes[..copy_len]);
                            Some(u32::from_be_bytes(padded))
                        }
                    }
                } else {
                    None
                };

                Ok(Box::new(POPM::new(email, rating, count)))
            }
            "TXXX" => {
                if data.is_empty() {
                    return Err(AudexError::InvalidData("TXXX frame is empty".to_string()));
                }

                let encoding = TextEncoding::from_byte(data[0])?;
                let null_term = encoding.null_terminator();

                // Find description null terminator
                let desc_end = if null_term.len() == 1 {
                    data[1..]
                        .iter()
                        .position(|&b| b == null_term[0])
                        .unwrap_or(data.len() - 1)
                        + 1
                } else {
                    // UTF-16 double null; truncate to even length for alignment
                    (0..(data.len() & !1).saturating_sub(2))
                        .skip(1)
                        .step_by(2)
                        .find(|&i| data[i] == 0 && data[i + 1] == 0)
                        .unwrap_or(data.len())
                };

                let description = encoding.decode_text(&data[1..desc_end])?;

                // Get text data
                let text_start = desc_end + null_term.len();
                let text_data = if text_start < data.len() {
                    &data[text_start..]
                } else {
                    &[]
                };

                let text = if text_data.is_empty() {
                    vec![]
                } else {
                    let text_str = encoding.decode_text(text_data)?;
                    text_str
                        .split('\0')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect()
                };

                Ok(Box::new(TXXX::new(encoding, description, text)))
            }
            "USLT" => {
                if data.len() < 5 {
                    return Err(AudexError::InvalidData("USLT frame too short".to_string()));
                }

                let encoding = TextEncoding::from_byte(data[0])?;
                let language = [data[1], data[2], data[3]];

                // Find description null terminator
                let desc_end = data[4..]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(data.len() - 4)
                    + 4;

                let description = encoding.decode_text(&data[4..desc_end])?;

                // Get lyrics text
                let text_start = desc_end + encoding.null_terminator().len();
                let text = if text_start < data.len() {
                    encoding
                        .decode_text(&data[text_start..])?
                        .trim_end_matches('\0')
                        .to_string()
                } else {
                    String::new()
                };

                Ok(Box::new(USLT::new(encoding, language, description, text)))
            }
            "RVA2" | "RVAD" => {
                if data.is_empty() {
                    return Err(AudexError::InvalidData("RVA2 frame is empty".to_string()));
                }

                // Find identification null terminator
                let id_end = data.iter().position(|&b| b == 0).ok_or_else(|| {
                    AudexError::InvalidData(
                        "No identification terminator in RVA2 frame".to_string(),
                    )
                })?;

                // RVA2 identification is Latin-1 (ISO-8859-1) encoded, not UTF-8
                let identification = TextEncoding::Latin1.decode_text(&data[..id_end])?;

                let mut channels = Vec::new();
                let mut pos = id_end + 1;

                // Parse channel adjustments
                while pos + 4 <= data.len() {
                    let channel_type = ChannelType::from(data[pos]);
                    let volume_adjustment = i16::from_be_bytes([data[pos + 1], data[pos + 2]]);
                    let gain_db = volume_adjustment as f32 / 512.0;
                    let peak_bits = data[pos + 3];

                    // Read peak volume if present
                    let peak_bytes = (peak_bits as usize).div_ceil(8);
                    if pos + 4 + peak_bytes > data.len() {
                        break;
                    }

                    let peak = if peak_bytes > 0 && peak_bits > 0 {
                        // Read peak value as big-endian integer
                        let mut peak_value = 0u64;
                        for i in 0..peak_bytes.min(8) {
                            peak_value = (peak_value << 8) | data[pos + 4 + i] as u64;
                        }
                        // ID3v2.4 spec: fixed-point with 1 bit integer, (bits-1) fraction
                        // Normalize by dividing by 2^(bits-1), range 0.0 to ~2.0
                        let clamped_bits = peak_bits.min(63);
                        peak_value as f32 / (1u64 << (clamped_bits - 1)).max(1) as f32
                    } else {
                        0.0
                    };

                    channels.push((channel_type, gain_db, peak));
                    pos += 4 + peak_bytes;
                }

                Ok(Box::new(RVA2::new(identification, channels)))
            }
            _ => {
                // Default to text frame for T*** frames
                if frame_id.starts_with('T') {
                    let encoding = if data.is_empty() {
                        TextEncoding::Utf8
                    } else {
                        TextEncoding::from_byte(data[0])?
                    };

                    let text_data = if data.is_empty() {
                        Vec::new()
                    } else {
                        data[1..].to_vec()
                    };

                    let text = if text_data.is_empty() {
                        vec![]
                    } else {
                        let decoded = encoding.decode_text(&text_data)?;
                        decoded
                            .split('\u{0}')
                            .filter(|part| !part.is_empty())
                            .map(|part| part.trim_start_matches('\u{feff}').to_string())
                            .collect()
                    };

                    let mut frame = TextFrame::new(frame_id.to_string(), text);
                    frame.encoding = encoding;
                    Ok(Box::new(frame))
                } else {
                    Err(AudexError::UnsupportedFormat(format!(
                        "Unknown frame type: {}",
                        frame_id
                    )))
                }
            }
        }
    }
}

/// Event timing format for ETCO frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TimeStampFormat {
    Mpeg = 1,
    Milliseconds = 2,
}

impl From<u8> for TimeStampFormat {
    fn from(value: u8) -> Self {
        match value {
            2 => TimeStampFormat::Milliseconds,
            _ => TimeStampFormat::Mpeg,
        }
    }
}

/// Event types for ETCO frames
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventType {
    Padding = 0x00,
    EndOfInitialSilence = 0x01,
    IntroStart = 0x02,
    MainpartStart = 0x03,
    OutroStart = 0x04,
    OutroEnd = 0x05,
    VerseStart = 0x06,
    RefrainStart = 0x07,
    InterludeStart = 0x08,
    ThemeStart = 0x09,
    VariationStart = 0x0A,
    KeyChange = 0x0B,
    TimeChange = 0x0C,
    MomentaryUnwantedNoise = 0x0D,
    SustainedNoise = 0x0E,
    SustainedNoiseEnd = 0x0F,
    IntroEnd = 0x10,
    MainpartEnd = 0x11,
    VerseEnd = 0x12,
    RefrainEnd = 0x13,
    ThemeEnd = 0x14,
    Profanity = 0x15,
    ProfanityEnd = 0x16,
    NotPredefined = 0xFF,
}

impl From<u8> for EventType {
    fn from(value: u8) -> Self {
        match value {
            0x01 => EventType::EndOfInitialSilence,
            0x02 => EventType::IntroStart,
            0x03 => EventType::MainpartStart,
            0x04 => EventType::OutroStart,
            0x05 => EventType::OutroEnd,
            0x06 => EventType::VerseStart,
            0x07 => EventType::RefrainStart,
            0x08 => EventType::InterludeStart,
            0x09 => EventType::ThemeStart,
            0x0A => EventType::VariationStart,
            0x0B => EventType::KeyChange,
            0x0C => EventType::TimeChange,
            0x0D => EventType::MomentaryUnwantedNoise,
            0x0E => EventType::SustainedNoise,
            0x0F => EventType::SustainedNoiseEnd,
            0x10 => EventType::IntroEnd,
            0x11 => EventType::MainpartEnd,
            0x12 => EventType::VerseEnd,
            0x13 => EventType::RefrainEnd,
            0x14 => EventType::ThemeEnd,
            0x15 => EventType::Profanity,
            0x16 => EventType::ProfanityEnd,
            0xFF => EventType::NotPredefined,
            _ => EventType::Padding,
        }
    }
}

/// APIC frame (Attached picture)
#[derive(Clone)]
pub struct APIC {
    pub encoding: TextEncoding,
    pub mime: String,
    pub type_: PictureType,
    pub desc: String,
    pub data: Vec<u8>,
}

impl fmt::Debug for APIC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Custom Debug implementation that shows ALL data bytes without truncation
        // Vec<u8>'s default Debug truncates, so we format manually
        struct AllBytes<'a>(&'a [u8]);
        impl<'a> fmt::Debug for AllBytes<'a> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "[")?;
                for (i, byte) in self.0.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", byte)?;
                }
                write!(f, "]")
            }
        }

        f.debug_struct("APIC")
            .field("encoding", &self.encoding)
            .field("mime", &self.mime)
            .field("type_", &self.type_)
            .field("desc", &self.desc)
            .field("data", &AllBytes(&self.data))
            .finish()
    }
}

impl APIC {
    pub fn new(
        encoding: TextEncoding,
        mime: String,
        type_: PictureType,
        desc: String,
        data: Vec<u8>,
    ) -> Self {
        Self {
            encoding,
            mime,
            type_,
            desc,
            data,
        }
    }
}

/// POPM frame (Popularimeter)
#[derive(Debug, Clone)]
pub struct POPM {
    pub email: String,
    pub rating: u8,
    pub count: Option<u32>,
}

impl POPM {
    pub fn new(email: String, rating: u8, count: Option<u32>) -> Self {
        Self {
            email,
            rating,
            count,
        }
    }
}

/// Implementation of encoding trait for picture frames
impl HasEncoding for APIC {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }
}

impl Frame for APIC {
    fn frame_id(&self) -> &str {
        "APIC"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];

        // Add MIME type
        data.extend(self.mime.as_bytes());
        data.push(0); // null terminator

        // Add picture type
        data.push(self.type_ as u8);

        // Add description
        data.extend(self.encoding.encode_text(&self.desc)?);
        data.extend(self.encoding.null_terminator());

        // Add image data
        data.extend(&self.data);

        Ok(data)
    }

    fn description(&self) -> String {
        format!(
            "APIC: {} ({}) - {} bytes",
            self.desc,
            self.type_,
            self.data.len()
        )
    }

    fn hash_key(&self) -> String {
        format!("APIC:{}", self.desc)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

impl Default for APIC {
    fn default() -> Self {
        Self {
            encoding: TextEncoding::default(), // Uses Utf8 per specs.rs
            mime: String::new(),
            type_: PictureType::CoverFront, // Default picture type
            desc: String::new(),
            data: Vec::new(),
        }
    }
}

impl Frame for POPM {
    fn frame_id(&self) -> &str {
        "POPM"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Add email
        data.extend(self.email.as_bytes());
        data.push(0); // null terminator

        // Add rating
        data.push(self.rating);

        // Add count if present
        if let Some(count) = self.count {
            let count_bytes = count.to_be_bytes();
            data.extend(&count_bytes);
        }

        Ok(data)
    }

    fn description(&self) -> String {
        match self.count {
            Some(count) => format!(
                "POPM: {} - rating {}/255, count {}",
                self.email, self.rating, count
            ),
            None => format!("POPM: {} - rating {}/255", self.email, self.rating),
        }
    }

    fn hash_key(&self) -> String {
        format!("POPM:{}", self.email)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// USLT frame (Unsynchronised lyrics/text transcription)
#[derive(Debug, Clone)]
pub struct USLT {
    pub encoding: TextEncoding,
    pub language: [u8; 3],
    pub description: String,
    pub text: String,
}

impl USLT {
    pub fn new(encoding: TextEncoding, lang: [u8; 3], desc: String, text: String) -> Self {
        Self {
            encoding,
            language: lang,
            description: desc,
            text,
        }
    }
}

/// Implementation of encoding trait for unsynchronized lyrics frames
impl HasEncoding for USLT {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }

    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        let current = self.get_encoding();
        if !current.is_valid_for_version(version) {
            self.set_encoding(TextEncoding::Utf16);
        }
    }
}

impl Frame for USLT {
    fn frame_id(&self) -> &str {
        "USLT"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];
        data.extend_from_slice(&self.language);
        data.extend(self.encoding.encode_text(&self.description)?);
        data.extend(self.encoding.null_terminator());
        data.extend(self.encoding.encode_text(&self.text)?);
        data.extend(self.encoding.null_terminator());
        Ok(data)
    }

    fn description(&self) -> String {
        let lang_str = std::str::from_utf8(&self.language).unwrap_or("unknown");
        format!("USLT: {} ({})", self.description, lang_str)
    }

    fn text_values(&self) -> Option<Vec<String>> {
        Some(vec![self.text.clone()])
    }

    fn hash_key(&self) -> String {
        let lang_str = std::str::from_utf8(&self.language).unwrap_or("unknown");
        if self.description.is_empty() {
            format!("USLT::{}", lang_str)
        } else {
            format!("USLT:{}:{}", self.description, lang_str)
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

impl Default for USLT {
    fn default() -> Self {
        Self {
            encoding: TextEncoding::default(), // Uses Utf8 per specs.rs
            language: *b"XXX",                 // Default language code
            description: String::new(),
            text: String::new(),
        }
    }
}

/// GEOB frame (General encapsulated object)
#[derive(Debug, Clone)]
pub struct GEOB {
    pub encoding: TextEncoding,
    pub mime_type: String,
    pub filename: String,
    pub description: String,
    pub data: Vec<u8>,
}

impl GEOB {
    pub fn new(mime_type: String, filename: String, description: String, data: Vec<u8>) -> Self {
        Self {
            encoding: TextEncoding::Utf8,
            mime_type,
            filename,
            description,
            data,
        }
    }
}

/// Implementation of encoding trait for general encapsulated object frames
impl HasEncoding for GEOB {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }
}

impl Frame for GEOB {
    fn frame_id(&self) -> &str {
        "GEOB"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];
        data.extend(self.mime_type.as_bytes());
        data.push(0); // null terminator
        data.extend(self.encoding.encode_text(&self.filename)?);
        data.extend(self.encoding.null_terminator());
        data.extend(self.encoding.encode_text(&self.description)?);
        data.extend(self.encoding.null_terminator());
        data.extend(&self.data);
        Ok(data)
    }

    fn description(&self) -> String {
        format!(
            "GEOB: {} ({}) - {} bytes",
            self.description,
            self.mime_type,
            self.data.len()
        )
    }

    fn hash_key(&self) -> String {
        format!("GEOB:{}", self.description)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

/// WXXX frame (User defined URL link frame)
#[derive(Debug, Clone)]
pub struct WXXX {
    pub encoding: TextEncoding,
    pub description: String,
    pub url: String,
}

impl WXXX {
    pub fn new(description: String, url: String) -> Self {
        Self {
            encoding: TextEncoding::Utf8,
            description,
            url,
        }
    }
}

/// Implementation of encoding trait for user-defined URL frames
impl HasEncoding for WXXX {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }
}

impl Frame for WXXX {
    fn frame_id(&self) -> &str {
        "WXXX"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = vec![self.encoding.to_byte()];
        data.extend(self.encoding.encode_text(&self.description)?);
        data.extend(self.encoding.null_terminator());
        data.extend(self.url.as_bytes());
        Ok(data)
    }

    fn description(&self) -> String {
        format!("WXXX: {} -> {}", self.description, self.url)
    }

    fn text_values(&self) -> Option<Vec<String>> {
        Some(vec![self.url.clone()])
    }

    fn hash_key(&self) -> String {
        format!("WXXX:{}", self.description)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

/// CHAP frame (Chapter)
///
/// Represents a single chapter or section within an audio file, commonly used
/// for audiobooks, podcasts, and other segmented content.
#[derive(Debug)]
pub struct CHAP {
    /// Unique element identifier for this chapter (null-terminated Latin-1 string)
    pub element_id: String,
    /// Start time in milliseconds from the beginning of the file
    pub start_time: u32,
    /// End time in milliseconds from the beginning of the file
    pub end_time: u32,
    /// Start byte offset in the file (0xFFFFFFFF if not used)
    pub start_offset: u32,
    /// End byte offset in the file (0xFFFFFFFF if not used)
    pub end_offset: u32,
    /// Embedded frames providing chapter metadata (e.g., TIT2 for chapter title, APIC for chapter image)
    pub sub_frames: std::collections::HashMap<String, Box<dyn Frame>>,
}

impl CHAP {
    pub fn new(
        element_id: String,
        start_time: u32,
        end_time: u32,
        start_offset: u32,
        end_offset: u32,
    ) -> Self {
        Self {
            element_id,
            start_time,
            end_time,
            start_offset,
            end_offset,
            sub_frames: std::collections::HashMap::new(),
        }
    }

    /// Add an embedded frame to this chapter
    pub fn add_frame(&mut self, frame: Box<dyn Frame>) {
        let hash_key = frame.hash_key();
        self.sub_frames.insert(hash_key, frame);
    }

    /// Get a reference to an embedded frame by its hash key
    pub fn get_frame(&self, hash_key: &str) -> Option<&dyn Frame> {
        self.sub_frames.get(hash_key).map(|b| b.as_ref())
    }

    /// Remove an embedded frame by its hash key
    pub fn remove_frame(&mut self, hash_key: &str) -> Option<Box<dyn Frame>> {
        self.sub_frames.remove(hash_key)
    }

    pub(crate) fn to_data_for_version(&self, version: (u8, u8)) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        data.extend(self.element_id.as_bytes());
        data.push(0);
        data.extend(&self.start_time.to_be_bytes());
        data.extend(&self.end_time.to_be_bytes());
        data.extend(&self.start_offset.to_be_bytes());
        data.extend(&self.end_offset.to_be_bytes());

        for frame in self.sub_frames.values() {
            let frame_data = serialize_frame_for_version(frame.as_ref(), version)?;
            append_embedded_frame(&mut data, frame.frame_id(), &frame_data, version)?;
        }

        Ok(data)
    }
}

impl Frame for CHAP {
    fn frame_id(&self) -> &str {
        "CHAP"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        self.to_data_for_version((2, 4))
    }

    fn description(&self) -> String {
        format!(
            "CHAP: {} ({}ms - {}ms)",
            self.element_id, self.start_time, self.end_time
        )
    }

    fn hash_key(&self) -> String {
        format!("CHAP:{}", self.element_id)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for CHAP {
    fn default() -> Self {
        Self {
            element_id: String::new(),
            start_time: 0,
            end_time: 0,
            start_offset: 0xFFFFFFFF,
            end_offset: 0xFFFFFFFF,
            sub_frames: std::collections::HashMap::new(),
        }
    }
}

/// Table of Contents flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CTOCFlags(pub u8);

impl CTOCFlags {
    /// Child elements are ordered
    pub const ORDERED: u8 = 0x01;
    /// This is the top-level table of contents
    pub const TOP_LEVEL: u8 = 0x02;

    pub fn new(value: u8) -> Self {
        Self(value)
    }

    pub fn is_ordered(&self) -> bool {
        (self.0 & Self::ORDERED) != 0
    }

    pub fn is_top_level(&self) -> bool {
        (self.0 & Self::TOP_LEVEL) != 0
    }

    pub fn set_ordered(&mut self, ordered: bool) {
        if ordered {
            self.0 |= Self::ORDERED;
        } else {
            self.0 &= !Self::ORDERED;
        }
    }

    pub fn set_top_level(&mut self, top_level: bool) {
        if top_level {
            self.0 |= Self::TOP_LEVEL;
        } else {
            self.0 &= !Self::TOP_LEVEL;
        }
    }
}

impl From<u8> for CTOCFlags {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<CTOCFlags> for u8 {
    fn from(flags: CTOCFlags) -> u8 {
        flags.0
    }
}

/// CTOC frame (Table of Contents)
///
/// Provides a hierarchical structure for organizing chapters and other CTOC elements,
/// allowing for complex nested table of contents structures.
#[derive(Debug)]
pub struct CTOC {
    /// Unique element identifier for this TOC entry (null-terminated Latin-1 string)
    pub element_id: String,
    /// Flags indicating properties of this TOC entry
    pub flags: CTOCFlags,
    /// List of child element IDs (references to CHAP or other CTOC frames)
    pub child_element_ids: Vec<String>,
    /// Embedded frames providing TOC metadata (e.g., TIT2 for TOC title)
    pub sub_frames: std::collections::HashMap<String, Box<dyn Frame>>,
}

impl CTOC {
    pub fn new(element_id: String, flags: CTOCFlags, child_element_ids: Vec<String>) -> Self {
        Self {
            element_id,
            flags,
            child_element_ids,
            sub_frames: std::collections::HashMap::new(),
        }
    }

    /// Add an embedded frame to this table of contents entry
    pub fn add_frame(&mut self, frame: Box<dyn Frame>) {
        let hash_key = frame.hash_key();
        self.sub_frames.insert(hash_key, frame);
    }

    /// Get a reference to an embedded frame by its hash key
    pub fn get_frame(&self, hash_key: &str) -> Option<&dyn Frame> {
        self.sub_frames.get(hash_key).map(|b| b.as_ref())
    }

    /// Remove an embedded frame by its hash key
    pub fn remove_frame(&mut self, hash_key: &str) -> Option<Box<dyn Frame>> {
        self.sub_frames.remove(hash_key)
    }

    /// Add a child element ID
    pub fn add_child(&mut self, element_id: String) {
        self.child_element_ids.push(element_id);
    }

    pub(crate) fn to_data_for_version(&self, version: (u8, u8)) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        data.extend(self.element_id.as_bytes());
        data.push(0);
        data.push(self.flags.0);
        data.push(self.child_element_ids.len() as u8);

        for child_id in &self.child_element_ids {
            data.extend(child_id.as_bytes());
            data.push(0);
        }

        for frame in self.sub_frames.values() {
            let frame_data = serialize_frame_for_version(frame.as_ref(), version)?;
            append_embedded_frame(&mut data, frame.frame_id(), &frame_data, version)?;
        }

        Ok(data)
    }
}

impl Frame for CTOC {
    fn frame_id(&self) -> &str {
        "CTOC"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        self.to_data_for_version((2, 4))
    }

    fn description(&self) -> String {
        format!(
            "CTOC: {} (flags={}, children={})",
            self.element_id,
            self.flags.0,
            self.child_element_ids.join(",")
        )
    }

    fn hash_key(&self) -> String {
        format!("CTOC:{}", self.element_id)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for CTOC {
    fn default() -> Self {
        Self {
            element_id: String::new(),
            flags: CTOCFlags::new(0),
            child_element_ids: Vec::new(),
            sub_frames: std::collections::HashMap::new(),
        }
    }
}

/// Chapter hierarchy navigation and utility functions
pub mod chapters {
    use super::{CHAP, CTOC};
    use std::collections::HashMap;

    /// Chapter tree node for hierarchical navigation (using references to avoid cloning)
    #[derive(Debug)]
    pub struct ChapterNode<'a> {
        /// Chapter or TOC element ID
        pub element_id: String,
        /// Reference to chapter data (if this is a CHAP element)
        pub chapter: Option<&'a CHAP>,
        /// Reference to TOC data (if this is a CTOC element)
        pub toc: Option<&'a CTOC>,
        /// Child nodes in the hierarchy
        pub children: Vec<ChapterNode<'a>>,
    }

    impl<'a> ChapterNode<'a> {
        /// Check if this node is a chapter (CHAP)
        pub fn is_chapter(&self) -> bool {
            self.chapter.is_some()
        }

        /// Check if this node is a table of contents (CTOC)
        pub fn is_toc(&self) -> bool {
            self.toc.is_some()
        }

        /// Get all chapters in this subtree (depth-first)
        pub fn get_all_chapters(&self) -> Vec<&'a CHAP> {
            let mut chapters = Vec::new();
            if let Some(chap) = self.chapter {
                chapters.push(chap);
            }
            for child in &self.children {
                chapters.extend(child.get_all_chapters());
            }
            chapters
        }
    }

    /// Build a chapter tree from CHAP and CTOC frames
    ///
    /// Takes all CHAP and CTOC frames and constructs a hierarchical tree structure.
    /// Returns the root CTOC node if a top-level CTOC is found, or None if no structure exists.
    pub fn build_chapter_tree<'a>(
        chapters: &'a HashMap<String, CHAP>,
        tocs: &'a HashMap<String, CTOC>,
    ) -> Option<ChapterNode<'a>> {
        // Find the top-level TOC (if any)
        let root_toc = tocs.values().find(|toc| toc.flags.is_top_level())?;

        Some(build_node_recursive(&root_toc.element_id, chapters, tocs))
    }

    /// Recursively build a chapter tree node
    fn build_node_recursive<'a>(
        element_id: &str,
        chapters: &'a HashMap<String, CHAP>,
        tocs: &'a HashMap<String, CTOC>,
    ) -> ChapterNode<'a> {
        // Check if this element is a chapter
        if let Some(chapter) = chapters.get(element_id) {
            return ChapterNode {
                element_id: element_id.to_string(),
                chapter: Some(chapter),
                toc: None,
                children: Vec::new(),
            };
        }

        // Check if this element is a TOC
        if let Some(toc) = tocs.get(element_id) {
            let children: Vec<ChapterNode<'a>> = toc
                .child_element_ids
                .iter()
                .map(|child_id| build_node_recursive(child_id, chapters, tocs))
                .collect();

            return ChapterNode {
                element_id: element_id.to_string(),
                chapter: None,
                toc: Some(toc),
                children,
            };
        }

        // Element not found - return empty node
        ChapterNode {
            element_id: element_id.to_string(),
            chapter: None,
            toc: None,
            children: Vec::new(),
        }
    }

    /// Find the chapter that contains a given timestamp (in milliseconds)
    ///
    /// Searches all chapters and returns the one whose time range includes the timestamp.
    /// If multiple chapters contain the timestamp, returns the first match.
    pub fn find_chapter_at_time(
        chapters: &HashMap<String, CHAP>,
        timestamp_ms: u32,
    ) -> Option<&CHAP> {
        chapters
            .values()
            .find(|chapter| timestamp_ms >= chapter.start_time && timestamp_ms < chapter.end_time)
    }

    /// Validate chapter structure for consistency
    ///
    /// Checks for common issues like:
    /// - Overlapping chapters
    /// - Invalid time ranges (start >= end)
    /// - Missing chapter references in TOC
    pub fn validate_chapter_structure(
        chapters: &HashMap<String, CHAP>,
        tocs: &HashMap<String, CTOC>,
    ) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate individual chapters
        for (id, chapter) in chapters {
            // Check time range validity
            if chapter.start_time >= chapter.end_time {
                errors.push(format!(
                    "Chapter '{}' has invalid time range: {} >= {}",
                    id, chapter.start_time, chapter.end_time
                ));
            }
        }

        // Check for overlapping chapters
        let mut chapter_list: Vec<_> = chapters.values().collect();
        chapter_list.sort_by_key(|c| c.start_time);

        for i in 0..chapter_list.len().saturating_sub(1) {
            let current = chapter_list[i];
            let next = chapter_list[i + 1];
            if current.end_time > next.start_time {
                errors.push(format!(
                    "Chapters '{}' and '{}' overlap: {} > {}",
                    current.element_id, next.element_id, current.end_time, next.start_time
                ));
            }
        }

        // Validate TOC references
        for (toc_id, toc) in tocs {
            for child_id in &toc.child_element_ids {
                if !chapters.contains_key(child_id) && !tocs.contains_key(child_id) {
                    errors.push(format!(
                        "TOC '{}' references missing element '{}'",
                        toc_id, child_id
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get total duration from all chapters
    ///
    /// Returns the end time of the last chapter (assuming chapters are sequential)
    pub fn get_total_duration(chapters: &HashMap<String, CHAP>) -> u32 {
        chapters.values().map(|c| c.end_time).max().unwrap_or(0)
    }

    /// Get chapter count
    pub fn get_chapter_count(chapters: &HashMap<String, CHAP>) -> usize {
        chapters.len()
    }
}

/// SYLT frame (Synchronized Lyrics/Text)
///
/// Contains time-synchronized lyrics or text, commonly used for karaoke,
/// subtitles, or other time-aligned text content.
#[derive(Debug, Clone)]
pub struct SYLT {
    /// Text encoding for description and lyrics
    pub encoding: TextEncoding,
    /// Three-character ISO-639-2 language code
    pub language: [u8; 3],
    /// Timestamp format (MPEG frames or milliseconds)
    pub format: TimeStampFormat,
    /// Content type (lyrics, text transcription, movement, events, chord, trivia, etc.)
    pub content_type: u8,
    /// Content descriptor
    pub description: String,
    /// Synchronized text entries: (text, timestamp) pairs
    pub lyrics: Vec<(String, u32)>,
}

impl SYLT {
    pub fn new(
        encoding: TextEncoding,
        language: [u8; 3],
        format: TimeStampFormat,
        content_type: u8,
        description: String,
        lyrics: Vec<(String, u32)>,
    ) -> Self {
        Self {
            encoding,
            language,
            format,
            content_type,
            description,
            lyrics,
        }
    }

    /// Add a synchronized text entry
    pub fn add_lyric(&mut self, text: String, timestamp: u32) {
        self.lyrics.push((text, timestamp));
    }

    /// Sort lyrics by timestamp
    pub fn sort_lyrics(&mut self) {
        self.lyrics.sort_by_key(|(_, timestamp)| *timestamp);
    }

    /// Get lyric text at a specific timestamp
    pub fn get_lyric_at(&self, timestamp: u32) -> Option<&str> {
        self.lyrics
            .iter()
            .rev()
            .find(|(_, ts)| *ts <= timestamp)
            .map(|(text, _)| text.as_str())
    }
}

/// Implementation of encoding trait for synchronized lyrics frames
impl HasEncoding for SYLT {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }
}

impl Frame for SYLT {
    fn frame_id(&self) -> &str {
        "SYLT"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Encoding byte
        data.push(self.encoding.to_byte());

        // Language (3 bytes)
        data.extend(&self.language);

        // Timestamp format
        data.push(self.format as u8);

        // Content type
        data.push(self.content_type);

        // Description (null-terminated)
        data.extend(self.encoding.encode_text(&self.description)?);
        data.extend(self.encoding.null_terminator());

        // Synchronized text entries
        for (text, timestamp) in &self.lyrics {
            data.extend(self.encoding.encode_text(text)?);
            data.extend(self.encoding.null_terminator());
            data.extend(&timestamp.to_be_bytes());
        }

        Ok(data)
    }

    fn description(&self) -> String {
        let unit = if self.format == TimeStampFormat::Mpeg {
            "frames"
        } else {
            "ms"
        };
        format!(
            "SYLT: {} ({} {} entries)",
            self.description,
            self.lyrics.len(),
            unit
        )
    }

    fn hash_key(&self) -> String {
        format!(
            "SYLT:{}:{}",
            self.description,
            String::from_utf8_lossy(&self.language)
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

impl Default for SYLT {
    fn default() -> Self {
        Self {
            encoding: TextEncoding::default(),
            language: *b"XXX",
            format: TimeStampFormat::Milliseconds,
            content_type: 1, // Lyrics
            description: String::new(),
            lyrics: Vec::new(),
        }
    }
}

/// ETCO frame (Event Timing Codes)
///
/// Provides timing information for events that occur during playback,
/// such as intro start, outro start, key changes, etc.
#[derive(Debug)]
pub struct ETCO {
    /// Timestamp format (MPEG frames or milliseconds)
    pub format: TimeStampFormat,
    /// Event list: (event_type, timestamp) pairs
    pub events: Vec<(EventType, u32)>,
}

impl ETCO {
    pub fn new(format: TimeStampFormat, events: Vec<(EventType, u32)>) -> Self {
        Self { format, events }
    }

    /// Add an event
    pub fn add_event(&mut self, event_type: EventType, timestamp: u32) {
        self.events.push((event_type, timestamp));
    }

    /// Sort events by timestamp
    pub fn sort_events(&mut self) {
        self.events.sort_by_key(|(_, timestamp)| *timestamp);
    }

    /// Get event at or before a specific timestamp
    pub fn get_event_at(&self, timestamp: u32) -> Option<&EventType> {
        self.events
            .iter()
            .rev()
            .find(|(_, ts)| *ts <= timestamp)
            .map(|(event_type, _)| event_type)
    }
}

impl Frame for ETCO {
    fn frame_id(&self) -> &str {
        "ETCO"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Timestamp format
        data.push(self.format as u8);

        // Events (1 byte event type + 4 bytes timestamp)
        for (event_type, timestamp) in &self.events {
            data.push(*event_type as u8);
            data.extend(&timestamp.to_be_bytes());
        }

        Ok(data)
    }

    fn description(&self) -> String {
        let unit = if self.format == TimeStampFormat::Mpeg {
            "frames"
        } else {
            "ms"
        };
        format!("ETCO: {} events ({})", self.events.len(), unit)
    }

    fn hash_key(&self) -> String {
        "ETCO".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for ETCO {
    fn default() -> Self {
        Self {
            format: TimeStampFormat::Milliseconds,
            events: Vec::new(),
        }
    }
}

/// SYTC frame (Synchronized Tempo Codes)
///
/// Contains tempo information synchronized with the audio, allowing
/// for tempo changes throughout the track.
#[derive(Debug)]
pub struct SYTC {
    /// Timestamp format (MPEG frames or milliseconds)
    pub format: TimeStampFormat,
    /// Binary tempo data (format: 1 byte BPM + 4 bytes timestamp, repeated)
    pub tempo_data: Vec<u8>,
}

impl SYTC {
    pub fn new(format: TimeStampFormat, tempo_data: Vec<u8>) -> Self {
        Self { format, tempo_data }
    }

    /// Parse tempo changes from binary data
    ///
    /// Returns a list of (tempo_bpm, timestamp) pairs
    pub fn parse_tempo_changes(&self) -> Vec<(u8, u32)> {
        let mut changes = Vec::new();
        let mut pos = 0;

        while pos + 5 <= self.tempo_data.len() {
            let tempo = self.tempo_data[pos];
            let timestamp = u32::from_be_bytes([
                self.tempo_data[pos + 1],
                self.tempo_data[pos + 2],
                self.tempo_data[pos + 3],
                self.tempo_data[pos + 4],
            ]);
            changes.push((tempo, timestamp));
            pos += 5;
        }

        changes
    }

    /// Set tempo changes from a list of (tempo_bpm, timestamp) pairs
    pub fn set_tempo_changes(&mut self, changes: &[(u8, u32)]) {
        self.tempo_data.clear();
        for (tempo, timestamp) in changes {
            self.tempo_data.push(*tempo);
            self.tempo_data.extend(&timestamp.to_be_bytes());
        }
    }

    /// Get tempo at a specific timestamp
    pub fn get_tempo_at(&self, timestamp: u32) -> Option<u8> {
        let changes = self.parse_tempo_changes();
        changes
            .iter()
            .rev()
            .find(|(_, ts)| *ts <= timestamp)
            .map(|(tempo, _)| *tempo)
    }
}

impl Frame for SYTC {
    fn frame_id(&self) -> &str {
        "SYTC"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Timestamp format
        data.push(self.format as u8);

        // Tempo data
        data.extend(&self.tempo_data);

        Ok(data)
    }

    fn description(&self) -> String {
        let changes = self.parse_tempo_changes();
        let unit = if self.format == TimeStampFormat::Mpeg {
            "frames"
        } else {
            "ms"
        };
        format!("SYTC: {} tempo changes ({})", changes.len(), unit)
    }

    fn hash_key(&self) -> String {
        "SYTC".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for SYTC {
    fn default() -> Self {
        Self {
            format: TimeStampFormat::Milliseconds,
            tempo_data: Vec::new(),
        }
    }
}

/// RVA2 frame (Relative Volume Adjustment v2)
///
/// Used for volume scaling and normalization, particularly for ReplayGain.
/// This is the ID3v2.4 version with improved precision.
#[derive(Debug, Default)]
pub struct RVA2 {
    /// Description or context of this adjustment (e.g., "track", "album")
    pub identification: String,
    /// Channel adjustments: (channel, gain_db, peak_volume)
    pub channels: Vec<(ChannelType, f32, f32)>,
}

impl RVA2 {
    pub fn new(identification: String, channels: Vec<(ChannelType, f32, f32)>) -> Self {
        Self {
            identification,
            channels,
        }
    }

    /// Add a channel adjustment entry.
    ///
    /// Returns an error if gain_db or peak is not finite (NaN or Infinity),
    /// since non-finite values cannot represent valid audio levels.
    pub fn add_channel(
        &mut self,
        channel: ChannelType,
        gain_db: f32,
        peak: f32,
    ) -> crate::Result<()> {
        if !gain_db.is_finite() {
            return Err(crate::AudexError::InvalidData(format!(
                "Gain value must be finite, got: {}",
                gain_db
            )));
        }
        if !peak.is_finite() {
            return Err(crate::AudexError::InvalidData(format!(
                "Peak value must be finite, got: {}",
                peak
            )));
        }
        self.channels.push((channel, gain_db, peak));
        Ok(())
    }

    /// Get adjustment for a specific channel
    pub fn get_channel(&self, channel: ChannelType) -> Option<(f32, f32)> {
        self.channels
            .iter()
            .find(|(ch, _, _)| *ch == channel)
            .map(|(_, gain, peak)| (*gain, *peak))
    }

    /// Get master volume adjustment (most common)
    pub fn get_master(&self) -> Option<(f32, f32)> {
        self.get_channel(ChannelType::MasterVolume)
    }

    /// Create a ReplayGain track adjustment (standard usage)
    pub fn track_gain(gain_db: f32, peak: f32) -> Self {
        Self {
            identification: "track".to_string(),
            channels: vec![(ChannelType::MasterVolume, gain_db, peak)],
        }
    }

    /// Create a ReplayGain album adjustment (standard usage)
    pub fn album_gain(gain_db: f32, peak: f32) -> Self {
        Self {
            identification: "album".to_string(),
            channels: vec![(ChannelType::MasterVolume, gain_db, peak)],
        }
    }
}

impl fmt::Display for RVA2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format  output: "Master volume: +0.1699 dB/0.9912"
        let parts: Vec<String> = self
            .channels
            .iter()
            .map(|(channel, gain_db, peak)| {
                // Format gain with explicit sign and 4 decimal places
                let sign = if *gain_db >= 0.0 { "+" } else { "" };
                format!("{}: {}{:.4} dB/{:.4}", channel, sign, gain_db, peak)
            })
            .collect();
        write!(f, "{}", parts.join(", "))
    }
}

impl Frame for RVA2 {
    fn frame_id(&self) -> &str {
        "RVA2"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Identification (null-terminated Latin-1)
        // Must encode as Latin-1, not UTF-8
        data.extend(TextEncoding::Latin1.encode_text(&self.identification)?);
        data.push(0);

        // Channel adjustments
        for (channel, gain_db, peak) in &self.channels {
            // Channel type (1 byte)
            data.push(*channel as u8);

            // Volume adjustment (2 bytes signed, value * 512)
            // This converts decibels to the fixed-point representation
            let gain_value = (*gain_db * 512.0).round() as i16;
            data.extend(&gain_value.to_be_bytes());

            // Peak volume (variable-length based on bit depth)
            // ID3v2.4 spec: fixed-point with 1 bit integer, (bits-1) fraction
            // Formula: peak_value = peak * 2^(bits-1)
            // For 16 bits: peak_value = peak * 32768, range 0.0 to ~2.0
            let peak_bits: u8 = 16;
            data.push(peak_bits);
            let max_value = (1u32 << (peak_bits - 1)) as f32;
            let peak_value = (*peak * max_value).round() as u16;
            data.extend(&peak_value.to_be_bytes());
        }

        Ok(data)
    }

    fn description(&self) -> String {
        format!("{}", self)
    }

    fn hash_key(&self) -> String {
        format!("RVA2:{}", self.identification)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// RVAD frame (Relative Volume Adjustment - ID3v2.3 legacy)
///
/// Legacy volume adjustment frame. RVA2 is preferred for new files.
#[derive(Debug)]
pub struct RVAD {
    /// Increment/decrement flags for each channel
    pub flags: u8,
    /// Bits per adjustment value
    pub bits: u8,
    /// Volume adjustment values (right, left, back right, back left, center, bass)
    pub adjustments: Vec<i32>,
}

impl RVAD {
    pub fn new(flags: u8, bits: u8, adjustments: Vec<i32>) -> Self {
        Self {
            flags,
            bits,
            adjustments,
        }
    }

    /// Get right channel adjustment
    pub fn get_right(&self) -> Option<i32> {
        self.adjustments.first().copied()
    }

    /// Get left channel adjustment
    pub fn get_left(&self) -> Option<i32> {
        self.adjustments.get(1).copied()
    }

    /// Create stereo adjustment
    pub fn stereo(right: i32, left: i32) -> Self {
        // Flags: bit 0 = right increment, bit 1 = left increment
        let mut flags = 0u8;
        if right >= 0 {
            flags |= 0x01;
        }
        if left >= 0 {
            flags |= 0x02;
        }

        Self {
            flags,
            bits: 16, // Use 16 bits for values
            adjustments: vec![right.abs(), left.abs()],
        }
    }
}

impl Frame for RVAD {
    fn frame_id(&self) -> &str {
        "RVAD"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Flags (1 byte)
        data.push(self.flags);

        // Bits per value (1 byte)
        data.push(self.bits);

        // Calculate bytes needed per value
        let bytes_per_value = self.bits.div_ceil(8) as usize;

        // Write adjustment values
        for (i, value) in self.adjustments.iter().enumerate() {
            let mut abs_value = value.unsigned_abs();

            // Convert to bytes (big-endian)
            let mut value_bytes = vec![0u8; bytes_per_value];
            for j in (0..bytes_per_value).rev() {
                value_bytes[j] = (abs_value & 0xFF) as u8;
                abs_value >>= 8;
            }
            data.extend(&value_bytes);

            // Limit to 12 values max (6 channels * 2)
            if i >= 11 {
                break;
            }
        }

        Ok(data)
    }

    fn description(&self) -> String {
        format!(
            "RVAD: {} adjustments ({} bits)",
            self.adjustments.len(),
            self.bits
        )
    }

    fn hash_key(&self) -> String {
        "RVAD".to_string()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for RVAD {
    fn default() -> Self {
        Self {
            flags: 0,
            bits: 16,
            adjustments: Vec::new(),
        }
    }
}

/// ASPI - Audio Seek Point Index
///
/// Enables fast seeking within an audio file by storing index points at regular intervals.
/// Each index point represents a fraction of the file at that position.
#[derive(Debug)]
pub struct ASPI {
    /// Data start: byte offset from beginning of file
    pub data_start: u32,
    /// Data length: total bytes of audio data
    pub data_length: u32,
    /// Number of index points
    pub num_points: u16,
    /// Bits per index point (8 or 16)
    pub bits_per_point: u8,
    /// Fraction at index: list of index values
    /// Values are either u8 or u16 depending on bits_per_point
    pub index_points: Vec<u16>,
}

impl ASPI {
    /// Creates a new ASPI frame with the given parameters
    pub fn new(data_start: u32, data_length: u32, index_points: Vec<u16>) -> Self {
        let num_points = index_points.len() as u16;
        // Use 8-bit if all values fit in u8, otherwise use 16-bit
        let bits_per_point = if index_points.iter().all(|&v| v <= 255) {
            8
        } else {
            16
        };

        Self {
            data_start,
            data_length,
            num_points,
            bits_per_point,
            index_points,
        }
    }

    /// Gets the fraction at a specific index point
    pub fn get_fraction(&self, index: usize) -> Option<u16> {
        self.index_points.get(index).copied()
    }

    /// Calculates the approximate byte offset for a given index point
    pub fn offset_at_index(&self, index: usize) -> Option<u64> {
        if index >= self.index_points.len() {
            return None;
        }

        let fraction = self.index_points[index] as f64;
        let max_value = if self.bits_per_point == 8 {
            255.0
        } else {
            65535.0
        };
        let ratio = fraction / max_value;

        Some(self.data_start as u64 + (self.data_length as f64 * ratio) as u64)
    }
}

impl Frame for ASPI {
    fn frame_id(&self) -> &str {
        "ASPI"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Data start (4 bytes)
        data.extend_from_slice(&self.data_start.to_be_bytes());
        // Data length (4 bytes)
        data.extend_from_slice(&self.data_length.to_be_bytes());
        // Number of index points (2 bytes)
        data.extend_from_slice(&self.num_points.to_be_bytes());
        // Bits per index point (1 byte)
        data.push(self.bits_per_point);

        // Index points
        for &point in &self.index_points {
            if self.bits_per_point == 8 {
                data.push(point as u8);
            } else {
                data.extend_from_slice(&point.to_be_bytes());
            }
        }

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Audio Seek Point Index ({} points)", self.num_points)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for ASPI {
    fn default() -> Self {
        Self {
            data_start: 0,
            data_length: 0,
            num_points: 0,
            bits_per_point: 16,
            index_points: Vec::new(),
        }
    }
}

/// MCDI - Music CD Identifier
///
/// Contains a binary dump of the Table of Contents (TOC) from an audio CD.
/// This can be used to uniquely identify the CD.
#[derive(Debug, Default)]
pub struct MCDI {
    /// Binary CD TOC data
    pub data: Vec<u8>,
}

impl MCDI {
    /// Creates a new MCDI frame with the given TOC data
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl Frame for MCDI {
    fn frame_id(&self) -> &str {
        "MCDI"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        Ok(self.data.clone())
    }

    fn description(&self) -> String {
        format!("Music CD Identifier ({} bytes)", self.data.len())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// PCNT - Play Counter
///
/// Stores the number of times a file has been played.
/// This frame has been largely obsoleted by POPM (Popularimeter).
#[derive(Debug, Default)]
pub struct PCNT {
    /// Number of times the file has been played
    pub count: u64,
}

impl PCNT {
    /// Creates a new PCNT frame with the given count
    pub fn new(count: u64) -> Self {
        Self { count }
    }

    /// Increments the play count by one
    pub fn increment(&mut self) {
        self.count = self.count.saturating_add(1);
    }

    /// Increments the play count by a specific amount
    pub fn increment_by(&mut self, amount: u64) {
        self.count = self.count.saturating_add(amount);
    }
}

impl Frame for PCNT {
    fn frame_id(&self) -> &str {
        "PCNT"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        // Variable-length integer encoding - only use as many bytes as needed
        let mut count = self.count;
        let mut data = Vec::new();

        if count == 0 {
            data.push(0);
        } else {
            while count > 0 {
                data.insert(0, (count & 0xFF) as u8);
                count >>= 8;
            }
        }

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Play Counter ({})", self.count)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// UFID - Unique File Identifier
///
/// Used to register a unique identifier for the file with a specific system.
/// Commonly used for MusicBrainz IDs and other database identifiers.
#[derive(Debug, Default)]
pub struct UFID {
    /// Owner identifier (typically a URL or email)
    pub owner: String,
    /// Unique identifier data (binary)
    pub data: Vec<u8>,
}

impl UFID {
    /// Creates a new UFID frame
    pub fn new(owner: String, data: Vec<u8>) -> Self {
        Self { owner, data }
    }

    /// Creates a MusicBrainz recording ID UFID
    pub fn musicbrainz_recording_id(mbid: &str) -> Self {
        Self {
            owner: "http://musicbrainz.org".to_string(),
            data: mbid.as_bytes().to_vec(),
        }
    }
}

impl Frame for UFID {
    fn frame_id(&self) -> &str {
        "UFID"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Owner (Latin-1, null-terminated)
        data.extend_from_slice(self.owner.as_bytes());
        data.push(0);

        // Binary data
        data.extend_from_slice(&self.data);

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Unique File Identifier ({})", self.owner)
    }

    fn hash_key(&self) -> String {
        format!("UFID:{}", self.owner)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// PRIV - Private Frame
///
/// Contains private data for applications. Each private frame has an owner
/// identifier and arbitrary binary data.
#[derive(Debug, Default)]
pub struct PRIV {
    /// Owner identifier (typically a URL or application name)
    pub owner: String,
    /// Private binary data
    pub data: Vec<u8>,
}

impl PRIV {
    /// Creates a new PRIV frame
    pub fn new(owner: String, data: Vec<u8>) -> Self {
        Self { owner, data }
    }
}

impl Frame for PRIV {
    fn frame_id(&self) -> &str {
        "PRIV"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Owner (Latin-1, null-terminated)
        data.extend_from_slice(self.owner.as_bytes());
        data.push(0);

        // Binary data
        data.extend_from_slice(&self.data);

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Private Frame ({})", self.owner)
    }

    fn hash_key(&self) -> String {
        // Include a hash of the data to make each PRIV frame unique
        let data_hash = if self.data.is_empty() {
            String::new()
        } else {
            format!(
                "{:02x}{:02x}",
                self.data.first().unwrap_or(&0),
                self.data.last().unwrap_or(&0)
            )
        };
        format!("PRIV:{}:{}", self.owner, data_hash)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// LINK - Linked Information
///
/// Links to information in another frame, potentially in another file.
#[derive(Debug, Default)]
pub struct LINK {
    /// Frame identifier to link to (4 characters)
    pub frameid: String,
    /// URL where the frame can be found
    pub url: String,
    /// Additional ID data
    pub data: Vec<u8>,
}

impl LINK {
    /// Creates a new LINK frame
    pub fn new(frameid: String, url: String, data: Vec<u8>) -> Self {
        Self { frameid, url, data }
    }
}

impl Frame for LINK {
    fn frame_id(&self) -> &str {
        "LINK"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Frame ID (4 bytes, padded with spaces if needed)
        let mut frameid_bytes = self.frameid.as_bytes().to_vec();
        frameid_bytes.resize(4, b' ');
        data.extend_from_slice(&frameid_bytes[..4]);

        // URL (Latin-1, null-terminated)
        data.extend_from_slice(self.url.as_bytes());
        data.push(0);

        // Additional data
        data.extend_from_slice(&self.data);

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Linked Information ({} -> {})", self.frameid, self.url)
    }

    fn hash_key(&self) -> String {
        let data_hash = if self.data.is_empty() {
            String::new()
        } else {
            format!(
                "{:02x}{:02x}",
                self.data.first().unwrap_or(&0),
                self.data.last().unwrap_or(&0)
            )
        };
        format!("LINK:{}:{}:{}", self.frameid, self.url, data_hash)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// SEEK - Seek Frame
///
/// Indicates the minimum offset to the next tag. This allows for faster
/// scanning when multiple tags are present in a file.
#[derive(Debug, Default)]
pub struct SEEK {
    /// Minimum offset to next tag (bytes from end of this tag)
    pub offset: u32,
}

impl SEEK {
    /// Creates a new SEEK frame
    pub fn new(offset: u32) -> Self {
        Self { offset }
    }
}

impl Frame for SEEK {
    fn frame_id(&self) -> &str {
        "SEEK"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        // Variable-length integer encoding
        let mut offset = self.offset;
        let mut data = Vec::new();

        if offset == 0 {
            data.push(0);
        } else {
            while offset > 0 {
                data.insert(0, (offset & 0xFF) as u8);
                offset >>= 8;
            }
        }

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Seek Frame (offset: {} bytes)", self.offset)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// POSS - Position Synchronization Frame
///
/// Allows synchronization with other playback or recording devices.
/// Can be used to indicate the current position during playback.
#[derive(Debug)]
pub struct POSS {
    /// Format of the position (1 = MPEG frames, 2 = milliseconds)
    pub format: u8,
    /// Current position in the file
    pub position: u64,
}

impl POSS {
    /// Creates a new POSS frame
    pub fn new(format: u8, position: u64) -> Self {
        Self { format, position }
    }

    /// Creates a POSS frame with position in MPEG frames
    pub fn mpeg_frames(position: u64) -> Self {
        Self {
            format: 1,
            position,
        }
    }

    /// Creates a POSS frame with position in milliseconds
    pub fn milliseconds(position: u64) -> Self {
        Self {
            format: 2,
            position,
        }
    }
}

impl Frame for POSS {
    fn frame_id(&self) -> &str {
        "POSS"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Format (1 byte)
        data.push(self.format);

        // Position (variable-length integer)
        let mut pos = self.position;
        if pos == 0 {
            data.push(0);
        } else {
            while pos > 0 {
                data.insert(1, (pos & 0xFF) as u8);
                pos >>= 8;
            }
        }

        Ok(data)
    }

    fn description(&self) -> String {
        let format_str = match self.format {
            1 => "MPEG frames",
            2 => "milliseconds",
            _ => "unknown",
        };
        format!(
            "Position Synchronization ({} {})",
            self.position, format_str
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for POSS {
    fn default() -> Self {
        Self {
            format: 1,
            position: 0,
        }
    }
}

/// OWNE - Ownership Frame
///
/// Contains information about the ownership/purchase of the audio file.
#[derive(Debug, Clone)]
pub struct OWNE {
    /// Text encoding
    pub encoding: TextEncoding,
    /// Price paid (e.g., "USD 12.99")
    pub price: String,
    /// Date of purchase (YYYYMMDD format)
    pub date: String,
    /// Seller name
    pub seller: String,
}

impl OWNE {
    /// Creates a new OWNE frame
    pub fn new(encoding: TextEncoding, price: String, date: String, seller: String) -> Self {
        Self {
            encoding,
            price,
            date,
            seller,
        }
    }
}

/// Implementation of encoding trait for ownership frames
impl HasEncoding for OWNE {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }
}

impl Frame for OWNE {
    fn frame_id(&self) -> &str {
        "OWNE"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Text encoding
        data.push(self.encoding as u8);

        // Price (Latin-1, null-terminated)
        data.extend_from_slice(self.price.as_bytes());
        data.push(0);

        // Date (8 bytes, no null terminator)
        let date_bytes = self.date.as_bytes();
        if date_bytes.len() >= 8 {
            data.extend_from_slice(&date_bytes[..8]);
        } else {
            data.extend_from_slice(date_bytes);
            data.resize(data.len() + (8 - date_bytes.len()), b'0');
        }

        // Seller (encoded text, null-terminated)
        data.extend_from_slice(self.seller.as_bytes());
        data.push(0);

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Ownership ({}, {})", self.price, self.seller)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

impl Default for OWNE {
    fn default() -> Self {
        Self {
            encoding: TextEncoding::Utf8,
            price: String::new(),
            date: "19700101".to_string(),
            seller: String::new(),
        }
    }
}

/// COMR - Commercial Frame
///
/// Contains information about a commercial transaction related to the audio file.
#[derive(Debug, Clone)]
pub struct COMR {
    /// Text encoding
    pub encoding: TextEncoding,
    /// Price string (e.g., "USD 12.99")
    pub price: String,
    /// Valid until date (YYYYMMDD format)
    pub valid_until: String,
    /// Contact URL
    pub contact: String,
    /// Received as (0 = other, 1 = standard CD album, etc.)
    pub received_as: u8,
    /// Name of seller
    pub seller: String,
    /// Description
    pub description: String,
    /// Picture MIME type (optional)
    pub picture_mime: Option<String>,
    /// Picture data (optional)
    pub picture: Option<Vec<u8>>,
}

impl COMR {
    /// Creates a new COMR frame
    pub fn new(
        encoding: TextEncoding,
        price: String,
        valid_until: String,
        contact: String,
        received_as: u8,
        seller: String,
        description: String,
    ) -> Self {
        Self {
            encoding,
            price,
            valid_until,
            contact,
            received_as,
            seller,
            description,
            picture_mime: None,
            picture: None,
        }
    }

    /// Adds a picture to the commercial frame
    pub fn with_picture(mut self, mime: String, data: Vec<u8>) -> Self {
        self.picture_mime = Some(mime);
        self.picture = Some(data);
        self
    }
}

/// Implementation of encoding trait for commercial frames
impl HasEncoding for COMR {
    fn get_encoding(&self) -> TextEncoding {
        self.encoding
    }

    fn set_encoding(&mut self, encoding: TextEncoding) {
        self.encoding = encoding;
    }
}

impl Frame for COMR {
    fn frame_id(&self) -> &str {
        "COMR"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Text encoding
        data.push(self.encoding as u8);

        // Price (Latin-1, null-terminated)
        data.extend_from_slice(self.price.as_bytes());
        data.push(0);

        // Valid until (8 bytes, no null terminator)
        let date_bytes = self.valid_until.as_bytes();
        if date_bytes.len() >= 8 {
            data.extend_from_slice(&date_bytes[..8]);
        } else {
            data.extend_from_slice(date_bytes);
            data.resize(data.len() + (8 - date_bytes.len()), b'0');
        }

        // Contact URL (Latin-1, null-terminated)
        data.extend_from_slice(self.contact.as_bytes());
        data.push(0);

        // Received as (1 byte)
        data.push(self.received_as);

        // Seller (encoded text, null-terminated)
        data.extend_from_slice(self.seller.as_bytes());
        data.push(0);

        // Description (encoded text, null-terminated)
        data.extend_from_slice(self.description.as_bytes());
        data.push(0);

        // Optional: Picture MIME and data
        if let (Some(mime), Some(pic)) = (&self.picture_mime, &self.picture) {
            // MIME type (Latin-1, null-terminated)
            data.extend_from_slice(mime.as_bytes());
            data.push(0);
            // Picture data
            data.extend_from_slice(pic);
        }

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Commercial ({}, {})", self.price, self.seller)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Override to provide automatic encoding conversion for ID3v2.3 compatibility
    fn convert_encoding_for_version(&mut self, version: (u8, u8)) {
        HasEncoding::convert_encoding_for_version(self, version);
    }
}

impl Default for COMR {
    fn default() -> Self {
        Self {
            encoding: TextEncoding::Utf8,
            price: String::new(),
            valid_until: "19700101".to_string(),
            contact: String::new(),
            received_as: 0,
            seller: String::new(),
            description: String::new(),
            picture_mime: None,
            picture: None,
        }
    }
}

/// ENCR - Encryption Method Registration
///
/// Registers an encryption method for use in the file.
/// The standard does not allow multiple ENCR frames with the same owner.
#[derive(Debug)]
pub struct ENCR {
    /// Owner identifier (URL or email)
    pub owner: String,
    /// Method symbol (must be > 0x80)
    pub method_symbol: u8,
    /// Encryption data
    pub data: Vec<u8>,
}

impl ENCR {
    /// Creates a new ENCR frame
    pub fn new(owner: String, method_symbol: u8, data: Vec<u8>) -> Self {
        Self {
            owner,
            method_symbol,
            data,
        }
    }
}

impl Frame for ENCR {
    fn frame_id(&self) -> &str {
        "ENCR"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Owner (Latin-1, null-terminated)
        data.extend_from_slice(self.owner.as_bytes());
        data.push(0);

        // Method symbol (1 byte)
        data.push(self.method_symbol);

        // Encryption data
        data.extend_from_slice(&self.data);

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Encryption Method Registration ({})", self.owner)
    }

    fn hash_key(&self) -> String {
        format!("ENCR:{}", self.owner)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for ENCR {
    fn default() -> Self {
        Self {
            owner: String::new(),
            method_symbol: 0x80,
            data: Vec::new(),
        }
    }
}

/// GRID - Group Identification Registration
///
/// Registers a group symbol for use in the file.
#[derive(Debug)]
pub struct GRID {
    /// Owner identifier (URL or email)
    pub owner: String,
    /// Group symbol (must be > 0x80)
    pub group_symbol: u8,
    /// Group data
    pub data: Vec<u8>,
}

impl GRID {
    /// Creates a new GRID frame
    pub fn new(owner: String, group_symbol: u8, data: Vec<u8>) -> Self {
        Self {
            owner,
            group_symbol,
            data,
        }
    }
}

impl Frame for GRID {
    fn frame_id(&self) -> &str {
        "GRID"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Owner (Latin-1, null-terminated)
        data.extend_from_slice(self.owner.as_bytes());
        data.push(0);

        // Group symbol (1 byte)
        data.push(self.group_symbol);

        // Group data
        data.extend_from_slice(&self.data);

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Group Identification Registration ({})", self.owner)
    }

    fn hash_key(&self) -> String {
        format!("GRID:{}", self.group_symbol)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for GRID {
    fn default() -> Self {
        Self {
            owner: String::new(),
            group_symbol: 0x80,
            data: Vec::new(),
        }
    }
}

/// AENC - Audio Encryption
///
/// Indicates that the audio is encrypted. Contains information about
/// preview data (unencrypted portions).
///
/// Note: This library does not support decryption.
#[derive(Debug, Default)]
pub struct AENC {
    /// Owner identifier (encryption system URL/email)
    pub owner: String,
    /// Preview start: offset to first unencrypted block
    pub preview_start: u16,
    /// Preview length: number of unencrypted blocks
    pub preview_length: u16,
    /// Data required for decryption (optional)
    pub data: Vec<u8>,
}

impl AENC {
    /// Creates a new AENC frame
    pub fn new(owner: String, preview_start: u16, preview_length: u16, data: Vec<u8>) -> Self {
        Self {
            owner,
            preview_start,
            preview_length,
            data,
        }
    }
}

impl Frame for AENC {
    fn frame_id(&self) -> &str {
        "AENC"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Owner (Latin-1, null-terminated)
        data.extend_from_slice(self.owner.as_bytes());
        data.push(0);

        // Preview start (2 bytes)
        data.extend_from_slice(&self.preview_start.to_be_bytes());

        // Preview length (2 bytes)
        data.extend_from_slice(&self.preview_length.to_be_bytes());

        // Encryption data
        data.extend_from_slice(&self.data);

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Audio Encryption ({})", self.owner)
    }

    fn hash_key(&self) -> String {
        format!("AENC:{}", self.owner)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// RBUF - Recommended Buffer Size
///
/// Recommends a buffer size for optimal playback.
#[derive(Debug, Default)]
pub struct RBUF {
    /// Recommended buffer size in bytes (24-bit value)
    pub size: u32,
    /// Whether embedded info flag is set (optional)
    pub embedded_info: Option<bool>,
    /// Offset to next tag (optional)
    pub offset: Option<u32>,
}

impl RBUF {
    /// Creates a new RBUF frame with just the buffer size
    pub fn new(size: u32) -> Self {
        Self {
            size: size & 0x00FFFFFF, // Ensure 24-bit
            embedded_info: None,
            offset: None,
        }
    }

    /// Creates a RBUF frame with all fields
    pub fn with_info(size: u32, embedded_info: bool, offset: u32) -> Self {
        Self {
            size: size & 0x00FFFFFF,
            embedded_info: Some(embedded_info),
            offset: Some(offset),
        }
    }
}

impl Frame for RBUF {
    fn frame_id(&self) -> &str {
        "RBUF"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Buffer size (3 bytes, big-endian)
        let size_bytes = self.size.to_be_bytes();
        data.extend_from_slice(&size_bytes[1..4]); // Skip first byte for 24-bit

        // Optional fields
        if let Some(info) = self.embedded_info {
            data.push(if info { 1 } else { 0 });

            if let Some(offset) = self.offset {
                data.extend_from_slice(&offset.to_be_bytes());
            }
        }

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Recommended Buffer Size ({} bytes)", self.size)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// SIGN - Signature Frame
///
/// Contains a digital signature for the tag.
#[derive(Debug)]
pub struct SIGN {
    /// Group symbol
    pub group_symbol: u8,
    /// Signature data
    pub signature: Vec<u8>,
}

impl SIGN {
    /// Creates a new SIGN frame
    pub fn new(group_symbol: u8, signature: Vec<u8>) -> Self {
        Self {
            group_symbol,
            signature,
        }
    }
}

impl Frame for SIGN {
    fn frame_id(&self) -> &str {
        "SIGN"
    }

    fn to_data(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Group symbol (1 byte)
        data.push(self.group_symbol);

        // Signature
        data.extend_from_slice(&self.signature);

        Ok(data)
    }

    fn description(&self) -> String {
        format!("Signature Frame (group {})", self.group_symbol)
    }

    fn hash_key(&self) -> String {
        let sig_hash = if self.signature.is_empty() {
            String::new()
        } else {
            format!(
                "{:02x}{:02x}",
                self.signature.first().unwrap_or(&0),
                self.signature.last().unwrap_or(&0)
            )
        };
        format!("SIGN:{}:{}", self.group_symbol, sig_hash)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for SIGN {
    fn default() -> Self {
        Self {
            group_symbol: 0x80,
            signature: Vec::new(),
        }
    }
}
