//! Core file detection and loading functionality
//!
//! This module provides the central file loading and format detection system for audex.
//! It implements a dynamic dispatch mechanism that allows working with multiple audio
//! formats through a unified interface.
//!
//! # Dynamic Dispatch System
//!
//! The library uses a two-tiered approach for handling audio files:
//!
//! 1. **Static Types**: Format-specific types like [`crate::mp3::MP3`], [`crate::flac::FLAC`], etc.
//!    - Direct access to all format-specific features
//!    - Zero-cost abstractions at compile time
//!    - Type-safe operations
//!
//! 2. **Dynamic Types**: The `DynamicFileType` wrapper
//!    - Runtime format detection
//!    - Unified interface across all formats
//!    - Useful for applications that handle multiple formats
//!
//! ## When to Use Dynamic vs Static Types
//!
//! **Use Dynamic Types ([`File`]) when:**
//! - You need to handle multiple file formats in the same code path
//! - Format is determined at runtime (e.g., user-provided files)
//! - You want automatic format detection
//! - Building file management tools or media libraries
//!
//! ```no_run
//! use audex::File;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! // Automatically detects format and returns appropriate handler
//! let file = File::load("unknown_format.audio")?;
//! println!("Detected format: {}", file.format_name());
//! # Ok(())
//! # }
//! ```
//!
//! **Use Static Types when:**
//! - You know the exact format at compile time
//! - You need format-specific features not exposed in the dynamic interface
//! - Performance is critical (avoids virtual dispatch overhead)
//! - Building format-specific tools
//!
//! ```no_run
//! use audex::flac::FLAC;
//! use audex::FileType;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! // Direct access to FLAC-specific features
//! let flac = FLAC::load("audio.flac")?;
//! let info = flac.info();
//! // Access FLAC-specific stream information fields
//! # Ok(())
//! # }
//! ```
//!
//! # Format Detection
//!
//! Format detection uses a multi-stage scoring system:
//!
//! 1. **Magic Byte Detection**: Checks file headers for format signatures
//!    - MP3: ID3 tags or frame sync markers
//!    - FLAC: "fLaC" signature
//!    - MP4/M4A: "ftyp" atom
//!    - And more...
//!
//! 2. **Extension Matching**: Falls back to file extension when headers are ambiguous
//!
//! 3. **Content Analysis**: Some formats require deeper inspection
//!
//! Each format provides a scoring function that returns a confidence level as an `i32`.
//! Higher scores indicate higher confidence. The format with the highest positive
//! score is selected for loading.
//!
//! ```no_run
//! # fn main() -> Result<(), audex::AudexError> {
//! // Detect format without loading the entire file
//! let format_name = audex::detect_format("/path/to/audio.mp3")?;
//! println!("Detected format: {}", format_name);
//! # Ok(())
//! # }
//! ```
//!
//! # Examples
//!
//! ## Basic File Loading
//!
//! ```no_run
//! use audex::File;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! // Load any supported audio format
//! let mut file = File::load("music.mp3")?;
//!
//! // Access and modify tags using dictionary interface
//! file.set("artist", vec!["Artist Name".to_string()])?;
//! file.set("album", vec!["Album Title".to_string()])?;
//!
//! // Save changes back to file
//! file.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Loading from Custom Readers
//!
//! ```no_run
//! use audex::File;
//! use std::io::Cursor;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! // Load from any source that implements Read + Seek
//! let data = std::fs::read("audio.flac")?;
//! let cursor = Cursor::new(data);
//!
//! // Provide optional path hint for format detection
//! let file = File::load_from_reader(cursor, Some("audio.flac".into()))?;
//!
//! println!("Format: {}", file.format_name());
//! println!("Has tags: {}", file.has_tags());
//! # Ok(())
//! # }
//! ```
//!
//! ## Extracting Format-Specific Information
//!
//! ```no_run
//! use audex::File;
//! use audex::mp3::MP3;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! let file = File::load("song.mp3")?;
//!
//! // Downcast to specific format for advanced features
//! if let Some(mp3) = file.downcast_ref::<MP3>() {
//!     // Access MP3-specific fields
//!     println!("MP3-specific operations available");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Dynamic vs Static Type Access Patterns
//!
//! Understanding the difference between dynamic and static access is important for
//! effective use of the library.
//!
//! ### Dynamic Type (File) - Method-Based Access
//!
//! The [`File`] type (a factory struct whose `load()` returns `Result<``DynamicFileType``>`)
//! provides method-based access with a uniform interface regardless of the underlying format:
//!
//! ```no_run
//! use audex::File;
//! use audex::StreamInfo;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! // Dynamic type - automatic format detection
//! let file = File::load("song.mp3")?;
//!
//! // Access stream info through methods
//! let info = file.info(); // Returns DynamicStreamInfo
//! if let Some(bitrate) = info.bitrate() {
//!     println!("Bitrate: {} kbps", bitrate / 1000);
//! }
//!
//! // Access tags through dictionary interface
//! if let Some(title) = file.get("TIT2").or_else(|| file.get("TITLE")) {
//!     println!("Title: {:?}", title);
//! }
//!
//! // Format identification
//! println!("Format: {}", file.format_name());
//! # Ok(())
//! # }
//! ```
//!
//! ### Static Type (MP3, FLAC, etc.) - Direct Field Access
//!
//! Format-specific types provide direct field access for better performance and
//! access to format-specific features:
//!
//! ```no_run
//! use audex::mp3::MP3;
//! use audex::FileType;
//!
//! # fn main() -> Result<(), audex::AudexError> {
//! // Static type - explicit format
//! let mp3 = MP3::load("song.mp3")?;
//!
//! // Direct field access to stream info
//! // Note: Some fields are non-optional for MP3 (always available)
//! let bitrate = mp3.info.bitrate;        // u32, no Option unwrap needed
//! let sample_rate = mp3.info.sample_rate; // u32, no Option unwrap needed
//! let channels = mp3.info.channels;      // u16, no Option unwrap needed
//!
//! // Optional field (duration may not be determinable for all MP3s)
//! if let Some(length) = mp3.info.length {
//!     println!("Duration: {:.2}s", length.as_secs_f64());
//! }
//!
//! println!("Bitrate: {} kbps", bitrate / 1000);
//! println!("Sample rate: {} Hz", sample_rate);
//!
//! // Access tags if present
//! if let Some(tags) = &mp3.tags {
//!     if let Some(title) = tags.get("TIT2") {
//!         println!("Title: {:?}", title);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### Key Differences Summary
//!
//! | Aspect | Dynamic Type (File) | Static Type (MP3, FLAC, etc.) |
//! |--------|---------------------|-------------------------------|
//! | Format Detection | Automatic | Manual (you specify) |
//! | Stream Info Access | `file.info()` method | `mp3.info` field |
//! | Field Optionality | All wrapped in `Option` | Format-specific (some non-optional) |
//! | Tags Access | `file.get(key)` | `mp3.tags?.get(key)` |
//! | Performance | Slight overhead (virtual dispatch) | Zero-cost (direct access) |
//! | Format-Specific Features | Limited (common interface) | Full access |
//! | Use Case | Multi-format tools | Format-specific tools |
//!
//! Choose dynamic types for flexibility and automatic format handling, or static types
//! for performance and access to format-specific features.

use crate::tagmap::{ConversionReport, SkipReason, StandardField, TagMap};
use crate::tags::Tags;
use crate::util::AnyFileThing;
use crate::{AudexError, FileType, ReadSeek, ReadWriteSeek, Result, StreamInfo};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::fs::File as StdFile;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;

// Type aliases for complex function types to reduce clippy warnings
type ItemsFunc = fn(&dyn Any) -> Vec<(String, Vec<String>)>;
type PopFunc = fn(&mut dyn Any, &str) -> Result<Option<Vec<String>>>;
type PopOrFunc = fn(&mut dyn Any, &str, Vec<String>) -> Result<Vec<String>>;

/// Loader callback used by the format registry.
type LoaderFn = fn(&Path) -> Result<DynamicFileType>;

/// Reader-based loader callback for the format registry.
type ReaderLoaderFn = fn(&mut dyn ReadSeek) -> Result<DynamicFileType>;

/// Descriptor for every format audex can load. This keeps scoring, loading, and
/// metadata in sync so that detect_format never advertises an unsupported type.
struct FormatDescriptor {
    name: &'static str,
    _extensions: &'static [&'static str],
    score_fn: fn(&str, &[u8]) -> i32,
    load_fn: LoaderFn,
    /// Optional reader-based loader. When present, `load_from_reader` can load
    /// this format directly from any `Read + Seek` source without a temp file.
    load_from_reader_fn: Option<ReaderLoaderFn>,
}

impl FormatDescriptor {
    fn score(&self, filename: &str, header: &[u8]) -> i32 {
        (self.score_fn)(filename, header)
    }

    fn load(&self, path: &Path) -> Result<DynamicFileType> {
        (self.load_fn)(path)
    }
}

fn load_format<T: FileType + 'static>(path: &Path) -> Result<DynamicFileType> {
    let file = T::load(path)?;
    Ok(DynamicFileType::new(file, Some(path.to_path_buf())))
}

fn load_format_from_reader<T: FileType + 'static>(
    reader: &mut dyn ReadSeek,
) -> Result<DynamicFileType> {
    let file = T::load_from_reader(reader)?;
    Ok(DynamicFileType::new(file, None))
}

const MP4_EXT: &[&str] = &[".mp4", ".m4a", ".m4b", ".m4p", ".m4v", ".3gp", ".3g2"];
const ASF_EXT: &[&str] = &[".wma", ".asf"];
const OGG_VORBIS_EXT: &[&str] = &[".ogg"];
const OGG_FLAC_EXT: &[&str] = &[".oggflac", ".oga"];
const OGG_OPUS_EXT: &[&str] = &[".opus"];
const OGG_SPEEX_EXT: &[&str] = &[".spx"];
const OGG_THEORA_EXT: &[&str] = &[".ogv"];
const FLAC_EXT: &[&str] = &[".flac"];
const AIFF_EXT: &[&str] = &[".aiff", ".aif"];
const WAVE_EXT: &[&str] = &[".wav"];
const DSDIFF_EXT: &[&str] = &[".dff", ".dst"];
const DSF_EXT: &[&str] = &[".dsf"];
const MONKEYS_EXT: &[&str] = &[".ape"];
const WAVPACK_EXT: &[&str] = &[".wv"];
const TAK_EXT: &[&str] = &[".tak"];
const TRUEAUDIO_EXT: &[&str] = &[".tta"];
const OPTIMFROG_EXT: &[&str] = &[".ofr", ".ofs"];
const MP3_EXT: &[&str] = &[".mp3", ".mp2", ".mpg", ".mpeg"];
const MUSEPACK_EXT: &[&str] = &[".mpc", ".mpp", ".mp+"];
const AAC_EXT: &[&str] = &[".aac", ".adts", ".adif"];
const AC3_EXT: &[&str] = &[".ac3", ".eac3"];
const SMF_EXT: &[&str] = &[".mid", ".midi"];

const FORMAT_REGISTRY: &[FormatDescriptor] = &[
    FormatDescriptor {
        name: "MP4",
        _extensions: MP4_EXT,
        score_fn: crate::mp4::MP4::score,
        load_fn: load_format::<crate::mp4::MP4>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::mp4::MP4>),
    },
    FormatDescriptor {
        name: "ASF",
        _extensions: ASF_EXT,
        score_fn: crate::asf::ASF::score,
        load_fn: load_format::<crate::asf::ASF>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::asf::ASF>),
    },
    FormatDescriptor {
        name: "OggVorbis",
        _extensions: OGG_VORBIS_EXT,
        score_fn: crate::oggvorbis::OggVorbis::score,
        load_fn: load_format::<crate::oggvorbis::OggVorbis>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::oggvorbis::OggVorbis>),
    },
    FormatDescriptor {
        name: "OggFlac",
        _extensions: OGG_FLAC_EXT,
        score_fn: crate::oggflac::OggFlac::score,
        load_fn: load_format::<crate::oggflac::OggFlac>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::oggflac::OggFlac>),
    },
    FormatDescriptor {
        name: "OggOpus",
        _extensions: OGG_OPUS_EXT,
        score_fn: crate::oggopus::OggOpus::score,
        load_fn: load_format::<crate::oggopus::OggOpus>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::oggopus::OggOpus>),
    },
    FormatDescriptor {
        name: "OggSpeex",
        _extensions: OGG_SPEEX_EXT,
        score_fn: crate::oggspeex::OggSpeex::score,
        load_fn: load_format::<crate::oggspeex::OggSpeex>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::oggspeex::OggSpeex>),
    },
    FormatDescriptor {
        name: "OggTheora",
        _extensions: OGG_THEORA_EXT,
        score_fn: crate::oggtheora::OggTheora::score,
        load_fn: load_format::<crate::oggtheora::OggTheora>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::oggtheora::OggTheora>),
    },
    FormatDescriptor {
        name: "FLAC",
        _extensions: FLAC_EXT,
        score_fn: crate::flac::FLAC::score,
        load_fn: load_format::<crate::flac::FLAC>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::flac::FLAC>),
    },
    FormatDescriptor {
        name: "AIFF",
        _extensions: AIFF_EXT,
        score_fn: crate::aiff::AIFF::score,
        load_fn: load_format::<crate::aiff::AIFF>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::aiff::AIFF>),
    },
    FormatDescriptor {
        name: "WAVE",
        _extensions: WAVE_EXT,
        score_fn: crate::wave::WAVE::score,
        load_fn: load_format::<crate::wave::WAVE>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::wave::WAVE>),
    },
    FormatDescriptor {
        name: "DSDIFF",
        _extensions: DSDIFF_EXT,
        score_fn: crate::dsdiff::DSDIFF::score,
        load_fn: load_format::<crate::dsdiff::DSDIFF>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::dsdiff::DSDIFF>),
    },
    FormatDescriptor {
        name: "DSF",
        _extensions: DSF_EXT,
        score_fn: crate::dsf::DSF::score,
        load_fn: load_format::<crate::dsf::DSF>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::dsf::DSF>),
    },
    FormatDescriptor {
        name: "MonkeysAudio",
        _extensions: MONKEYS_EXT,
        score_fn: crate::monkeysaudio::MonkeysAudio::score,
        load_fn: load_format::<crate::monkeysaudio::MonkeysAudio>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::monkeysaudio::MonkeysAudio>),
    },
    FormatDescriptor {
        name: "WavPack",
        _extensions: WAVPACK_EXT,
        score_fn: crate::wavpack::WavPack::score,
        load_fn: load_format::<crate::wavpack::WavPack>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::wavpack::WavPack>),
    },
    FormatDescriptor {
        name: "TAK",
        _extensions: TAK_EXT,
        score_fn: crate::tak::TAK::score,
        load_fn: load_format::<crate::tak::TAK>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::tak::TAK>),
    },
    FormatDescriptor {
        name: "TrueAudio",
        _extensions: TRUEAUDIO_EXT,
        score_fn: crate::trueaudio::TrueAudio::score,
        load_fn: load_format::<crate::trueaudio::TrueAudio>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::trueaudio::TrueAudio>),
    },
    FormatDescriptor {
        name: "OptimFROG",
        _extensions: OPTIMFROG_EXT,
        score_fn: crate::optimfrog::OptimFROG::score,
        load_fn: load_format::<crate::optimfrog::OptimFROG>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::optimfrog::OptimFROG>),
    },
    FormatDescriptor {
        name: "MP3",
        _extensions: MP3_EXT,
        score_fn: crate::mp3::MP3::score,
        load_fn: load_format::<crate::mp3::MP3>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::mp3::MP3>),
    },
    FormatDescriptor {
        name: "Musepack",
        _extensions: MUSEPACK_EXT,
        score_fn: crate::musepack::Musepack::score,
        load_fn: load_format::<crate::musepack::Musepack>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::musepack::Musepack>),
    },
    FormatDescriptor {
        name: "APEv2",
        _extensions: &[],
        score_fn: crate::apev2::APEv2::score,
        load_fn: load_format::<crate::apev2::APEv2>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::apev2::APEv2>),
    },
    FormatDescriptor {
        name: "ID3",
        _extensions: &[],
        score_fn: crate::id3::ID3FileType::score,
        load_fn: load_format::<crate::id3::ID3>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::id3::ID3>),
    },
    FormatDescriptor {
        name: "AAC",
        _extensions: AAC_EXT,
        score_fn: crate::aac::AAC::score,
        load_fn: load_format::<crate::aac::AAC>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::aac::AAC>),
    },
    FormatDescriptor {
        name: "AC3",
        _extensions: AC3_EXT,
        score_fn: crate::ac3::AC3::score,
        load_fn: load_format::<crate::ac3::AC3>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::ac3::AC3>),
    },
    FormatDescriptor {
        name: "SMF",
        _extensions: SMF_EXT,
        score_fn: crate::smf::SMF::score,
        load_fn: load_format::<crate::smf::SMF>,
        load_from_reader_fn: Some(load_format_from_reader::<crate::smf::SMF>),
    },
];

/// Dynamic stream information wrapper for runtime polymorphism
///
/// This struct wraps any [`StreamInfo`] implementation and provides a concrete
/// type that can be returned from dynamic file operations. It caches the stream
/// information values to avoid repeated trait method calls.
///
/// # Purpose
///
/// When working with `DynamicFileType`, we can't return references to trait objects
/// for stream info because different formats have different concrete types. This wrapper
/// solves that problem by extracting and storing the values directly.
///
/// # Examples
///
/// ```no_run
/// use audex::File;
/// use audex::StreamInfo;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// let file = File::load("audio.mp3")?;
///
/// // Get stream information as DynamicStreamInfo
/// let info = file.info();
///
/// // Access audio properties
/// if let Some(duration) = info.length() {
///     println!("Duration: {:.2} seconds", duration.as_secs_f64());
/// }
///
/// if let Some(bitrate) = info.bitrate() {
///     println!("Bitrate: {} bps", bitrate);
/// }
///
/// if let Some(sample_rate) = info.sample_rate() {
///     println!("Sample rate: {} Hz", sample_rate);
/// }
///
/// if let Some(channels) = info.channels() {
///     println!("Channels: {}", channels);
/// }
///
/// if let Some(bits) = info.bits_per_sample() {
///     println!("Bits per sample: {}", bits);
/// }
/// # Ok(())
/// # }
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DynamicStreamInfo {
    /// Audio stream length (duration)
    #[cfg_attr(
        feature = "serde",
        serde(with = "crate::serde_helpers::duration_as_secs_f64")
    )]
    length: Option<Duration>,
    /// Bitrate in bits per second
    bitrate: Option<u32>,
    /// Sample rate in Hz
    sample_rate: Option<u32>,
    /// Number of audio channels
    channels: Option<u16>,
    /// Bits per sample (bit depth)
    bits_per_sample: Option<u16>,
}

impl DynamicStreamInfo {
    /// Create from a concrete StreamInfo implementation
    pub fn from_stream_info<T: StreamInfo>(info: &T) -> Self {
        Self {
            length: info.length(),
            bitrate: info.bitrate(),
            sample_rate: info.sample_rate(),
            channels: info.channels(),
            bits_per_sample: info.bits_per_sample(),
        }
    }
}

impl StreamInfo for DynamicStreamInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        self.bitrate
    }

    fn sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }

    fn channels(&self) -> Option<u16> {
        self.channels
    }

    fn bits_per_sample(&self) -> Option<u16> {
        self.bits_per_sample
    }
}

/// Polymorphic wrapper for any audio file format
///
/// This type provides a unified interface for working with different audio formats
/// at runtime. It uses a virtual table pattern to dispatch method calls to the
/// appropriate format-specific implementation.
///
/// # Design
///
/// `DynamicFileType` wraps any type implementing [`FileType`] and provides:
/// - Runtime format detection and loading
/// - Key-value tag access (`get`, `set`, `remove`, etc.)
/// - Format-agnostic metadata operations
/// - Type-safe downcasting to specific formats when needed
///
/// The wrapper uses a vtable (virtual function table) to efficiently dispatch method
/// calls without requiring heap-allocated trait objects for every operation.
///
/// # Examples
///
/// ## Basic Usage
///
/// ```no_run
/// use audex::File;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// // Load automatically detects the format
/// let mut file = File::load("song.mp3")?;
///
/// // Check what format was loaded
/// println!("Format: {}", file.format_name());
/// println!("Has tags: {}", file.has_tags());
///
/// // Key-value tag access
/// file.set("artist", vec!["New Artist".to_string()])?;
/// file.set("album", vec!["New Album".to_string()])?;
///
/// // Get tag values
/// if let Some(artist) = file.get("artist") {
///     println!("Artist: {}", artist[0]);
/// }
///
/// // Save changes
/// file.save()?;
/// # Ok(())
/// # }
/// ```
///
/// ## Pattern Matching on Format
///
/// ```no_run
/// use audex::File;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// let file = File::load("audio.mp3")?;
///
/// // Match against format name
/// match file.format_name() {
///     name if name.contains("MP3") => {
///         println!("Handling MP3 file");
///     },
///     name if name.contains("FLAC") => {
///         println!("Handling FLAC file");
///     },
///     _ => {
///         println!("Unknown or unsupported format");
///     }
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Downcasting to Specific Format
///
/// ```no_run
/// use audex::File;
/// use audex::flac::FLAC;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// let mut file = File::load("audio.flac")?;
///
/// // Access format-specific features through downcasting
/// if let Some(flac) = file.downcast_mut::<FLAC>() {
///     // Now we have direct access to FLAC-specific methods
///     println!("This is a FLAC file");
/// }
/// # Ok(())
/// # }
/// ```
///
/// ## Iterating Over Tags
///
/// ```no_run
/// use audex::File;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// let file = File::load("music.mp3")?;
///
/// // Iterate over all tag key-value pairs
/// for (key, values) in &file {
///     for value in values {
///         println!("{}: {}", key, value);
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct DynamicFileType {
    /// Type-erased format instance (could be MP3, FLAC, etc.)
    inner: Box<dyn Any>,
    /// Virtual function table for format-specific operations
    vtable: &'static DynamicFileVTable,
    /// Optional file path for this instance
    filename: Option<PathBuf>,
}

/// Virtual table for dynamic file operations
struct DynamicFileVTable {
    save: fn(&mut dyn Any) -> Result<()>,
    clear: fn(&mut dyn Any) -> Result<()>,
    save_to_writer: fn(&mut dyn Any, &mut dyn ReadWriteSeek) -> Result<()>,
    clear_writer: fn(&mut dyn Any, &mut dyn ReadWriteSeek) -> Result<()>,
    save_to_path: fn(&mut dyn Any, &Path) -> Result<()>,
    has_tags: fn(&dyn Any) -> bool,
    tags_pprint: fn(&dyn Any) -> Option<String>,
    info_pprint: fn(&dyn Any) -> String,
    info: fn(&dyn Any) -> DynamicStreamInfo,
    mime_types: fn() -> &'static [&'static str],
    format_name: &'static str,
    add_tags: fn(&mut dyn Any) -> Result<()>,
    // Key-value interface function pointers
    get: fn(&dyn Any, &str) -> Option<Vec<String>>,
    set: fn(&mut dyn Any, &str, Vec<String>) -> Result<()>,
    remove: fn(&mut dyn Any, &str) -> Result<()>,
    keys: fn(&dyn Any) -> Vec<String>,
    contains_key: fn(&dyn Any, &str) -> bool,
    items: ItemsFunc,
    len: fn(&dyn Any) -> usize,
    is_empty: fn(&dyn Any) -> bool,
    get_first: fn(&dyn Any, &str) -> Option<String>,
    set_single: fn(&mut dyn Any, &str, String) -> Result<()>,
    pop: PopFunc,
    pop_or: PopOrFunc,
    get_or: fn(&dyn Any, &str, Vec<String>) -> Vec<String>,
    // Tag conversion vtable entries
    to_tag_map: fn(&dyn Any) -> TagMap,
    apply_tag_map: fn(&mut dyn Any, &TagMap) -> Result<ConversionReport>,
}

// Safety: DynamicFileVTable contains only fn pointers and &'static str, all of
// which are Send + Sync.
unsafe impl Send for DynamicFileVTable {}
unsafe impl Sync for DynamicFileVTable {}

// Compile-time guard: assert Send+Sync directly on DynamicFileVTable itself.
// If a non-Send/Sync field is ever added (e.g. *mut T, Rc<T>), this will
// fail to compile, forcing the unsafe impls above to be re-evaluated.
const _: () = {
    fn _assert_send_sync<T: Send + Sync>() {}
    fn _check() {
        _assert_send_sync::<DynamicFileVTable>();
    }
};

/// Global vtable cache: one entry per concrete type `T` that has been used with
/// `DynamicFileType::new`.
///
/// SAFETY: Each vtable is created via `Box::leak`, which permanently transfers
/// ownership to the `'static` lifetime. This makes the `&'static` references
/// sound by construction — even if an entry were removed from the map, the
/// leaked memory would remain valid. The map stores bare `&'static` references,
/// not owned Boxes, so there is no destructor that could free the backing memory.
static VTABLE_CACHE: std::sync::RwLock<Option<HashMap<TypeId, &'static DynamicFileVTable>>> =
    std::sync::RwLock::new(None);

/// Return the cached vtable for type `T`, creating it on first use.
fn vtable_for<T: FileType + 'static>() -> &'static DynamicFileVTable {
    let type_id = TypeId::of::<T>();

    // Fast path: read lock
    {
        // Recover from lock poisoning rather than propagating the panic —
        // the cached data is still valid even if a prior thread panicked
        let cache = VTABLE_CACHE.read().unwrap_or_else(|e| e.into_inner());
        if let Some(map) = cache.as_ref() {
            if let Some(&vtable) = map.get(&type_id) {
                return vtable;
            }
        }
    }

    // Slow path: write lock, insert if missing
    // Recover from lock poisoning rather than propagating the panic
    let mut cache = VTABLE_CACHE.write().unwrap_or_else(|e| e.into_inner());
    let map = cache.get_or_insert_with(HashMap::new);

    // Use the entry API to atomically check-or-insert. Box::leak permanently
    // gives the vtable a 'static lifetime, making the reference sound even
    // if the map entry were ever removed (which we don't do, but the safety
    // no longer depends on that invariant).
    map.entry(type_id)
        .or_insert_with(|| Box::leak(Box::new(create_vtable_for::<T>())))
}

/// Build a `DynamicFileVTable` for the concrete type `T`.
fn create_vtable_for<T: FileType + 'static>() -> DynamicFileVTable {
    DynamicFileVTable {
        save: |any| {
            let file = any
                .downcast_mut::<T>()
                .ok_or_else(|| AudexError::InvalidOperation("Type mismatch in save".to_string()))?;
            file.save()
        },
        clear: |any| {
            let file = any.downcast_mut::<T>().ok_or_else(|| {
                AudexError::InvalidOperation("Type mismatch in clear".to_string())
            })?;
            file.clear()
        },
        save_to_writer: |any, writer| {
            let file = any.downcast_mut::<T>().ok_or_else(|| {
                AudexError::InvalidOperation("Type mismatch in save_to_writer".to_string())
            })?;
            file.save_to_writer(writer)
        },
        clear_writer: |any, writer| {
            let file = any.downcast_mut::<T>().ok_or_else(|| {
                AudexError::InvalidOperation("Type mismatch in clear_writer".to_string())
            })?;
            file.clear_writer(writer)
        },
        save_to_path: |any, path| {
            let file = any.downcast_mut::<T>().ok_or_else(|| {
                AudexError::InvalidOperation("Type mismatch in save_to_path".to_string())
            })?;
            file.save_to_path(path)
        },
        has_tags: |any| {
            if let Some(file) = any.downcast_ref::<T>() {
                // Check both the typed tags accessor and the key list.
                // Some formats (e.g. TrueAudio) support multiple tag systems
                // but the associated Tags type only covers one of them, so
                // tags() may return None even when tags are present.
                file.tags().is_some() || !file.keys().is_empty()
            } else {
                false
            }
        },
        tags_pprint: |any| {
            any.downcast_ref::<T>()
                .and_then(|file| file.tags())
                .map(|t| t.pprint())
        },
        info_pprint: |any| {
            any.downcast_ref::<T>()
                .map(|file| file.info().pprint())
                .unwrap_or_else(|| "<No stream information>".to_string())
        },
        info: |any| {
            any.downcast_ref::<T>()
                .map(|file| DynamicStreamInfo::from_stream_info(file.info()))
                .unwrap_or_else(|| DynamicStreamInfo {
                    length: None,
                    bitrate: None,
                    sample_rate: None,
                    channels: None,
                    bits_per_sample: None,
                })
        },
        mime_types: T::mime_types,
        // Use the explicit format_id() trait method rather than
        // std::any::type_name, which is not stable across compiler versions
        format_name: T::format_id(),
        add_tags: |any| {
            let file = any.downcast_mut::<T>().ok_or_else(|| {
                AudexError::InvalidOperation("Type mismatch in add_tags".to_string())
            })?;
            file.add_tags()
        },
        get: |any, key| any.downcast_ref::<T>().and_then(|file| file.get(key)),
        set: |any, key, values| {
            let file = any
                .downcast_mut::<T>()
                .ok_or_else(|| AudexError::InvalidOperation("Type mismatch in set".to_string()))?;
            file.set(key, values)
        },
        remove: |any, key| {
            let file = any.downcast_mut::<T>().ok_or_else(|| {
                AudexError::InvalidOperation("Type mismatch in remove".to_string())
            })?;
            file.remove(key)
        },
        keys: |any| {
            any.downcast_ref::<T>()
                .map(|file| file.keys())
                .unwrap_or_default()
        },
        contains_key: |any, key| {
            any.downcast_ref::<T>()
                .map(|file| file.contains_key(key))
                .unwrap_or(false)
        },
        items: |any| {
            any.downcast_ref::<T>()
                .map(|file| file.items())
                .unwrap_or_default()
        },
        len: |any| any.downcast_ref::<T>().map(|file| file.len()).unwrap_or(0),
        is_empty: |any| {
            any.downcast_ref::<T>()
                .map(|file| file.is_empty())
                .unwrap_or(true)
        },
        get_first: |any, key| any.downcast_ref::<T>().and_then(|file| file.get_first(key)),
        set_single: |any, key, value| {
            let file = any.downcast_mut::<T>().ok_or_else(|| {
                AudexError::InvalidOperation("Type mismatch in set_single".to_string())
            })?;
            file.set_single(key, value)
        },
        pop: |any, key| {
            let file = any
                .downcast_mut::<T>()
                .ok_or_else(|| AudexError::InvalidOperation("Type mismatch in pop".to_string()))?;
            file.pop(key)
        },
        pop_or: |any, key, default| {
            let file = any.downcast_mut::<T>().ok_or_else(|| {
                AudexError::InvalidOperation("Type mismatch in pop_or".to_string())
            })?;
            file.pop_or(key, default)
        },
        get_or: |any, key, default| {
            any.downcast_ref::<T>()
                .map(|file| file.get_or(key, default.clone()))
                .unwrap_or(default)
        },
        // Tag map extraction — delegates to the generic items-based converter
        to_tag_map: |any| {
            any.downcast_ref::<T>()
                .map(|file| crate::file::items_to_tag_map(file.items(), type_name_short::<T>()))
                .unwrap_or_default()
        },
        // Tag map application — delegates to the generic key-value converter
        apply_tag_map: |any, map| {
            let file = any.downcast_mut::<T>().ok_or_else(|| {
                AudexError::InvalidOperation("Type mismatch in apply_tag_map".to_string())
            })?;
            crate::file::tag_map_to_items(file, map, type_name_short::<T>())
        },
    }
}

/// Extract the short type name from a full Rust type path (e.g. "FLAC" from "audex::flac::FLAC").
fn type_name_short<T: 'static>() -> &'static str {
    let full = std::any::type_name::<T>();
    full.rsplit("::").next().unwrap_or(full)
}

// ---------------------------------------------------------------------------
// Generic tag map conversion helpers
// ---------------------------------------------------------------------------

/// Determine the tag system used by a format based on its short type name.
fn tag_system_for_format(format_name: &str) -> Option<crate::tagmap::normalize::TagSystem> {
    use crate::tagmap::normalize::TagSystem;
    match format_name {
        // ID3v2-based formats
        "MP3" | "AIFF" | "WAVE" | "DSF" | "DSDIFF" | "ID3" | "EasyMP3" => Some(TagSystem::ID3v2),
        // Vorbis Comment-based formats
        "FLAC" | "OggVorbis" | "OggFlac" | "OggOpus" | "OggSpeex" | "OggTheora" => {
            Some(TagSystem::VorbisComment)
        }
        // MP4/iTunes atom-based formats
        "MP4" | "M4A" => Some(TagSystem::MP4),
        // APEv2-based formats
        "MonkeysAudio" | "WavPack" | "Musepack" | "TAK" | "OptimFROG" | "APEv2" => {
            Some(TagSystem::APEv2)
        }
        // ASF/WMA
        "ASF" => Some(TagSystem::ASF),
        // Formats that support multiple tag systems (detection deferred to key analysis)
        "TrueAudio" => None,
        _ => None,
    }
}

/// Detect the tag system from key patterns when the format supports multiple systems.
/// ID3v2 frame IDs are 3-4 uppercase ASCII letters/digits (e.g. TIT2, TPE1, COMM).
/// APEv2 keys are mixed-case words (e.g. Title, Artist, Album).
fn detect_tag_system_from_keys(
    items: &[(String, Vec<String>)],
) -> Option<crate::tagmap::normalize::TagSystem> {
    use crate::tagmap::normalize::TagSystem;

    if items.is_empty() {
        return None;
    }

    // Well-known ID3v2 frame IDs used to distinguish real ID3v2 frames from
    // APEv2 keys that happen to be 4 uppercase ASCII characters.
    const KNOWN_ID3V2_FRAMES: &[&str] = &[
        "TIT2", "TALB", "TPE1", "TPE2", "TRCK", "TYER", "TDRC", "TCON", "COMM", "APIC", "TXXX",
        "TPOS", "TCOM", "TENC", "TBPM", "TLAN", "TPUB", "TSRC", "TCOP", "TEXT", "TDRL", "USLT",
        "WOAR", "WXXX",
    ];

    // Collect keys that look like 4-char uppercase frame IDs
    let four_char_keys: Vec<&str> = items
        .iter()
        .filter(|(k, _)| {
            k.len() == 4
                && k.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
        .map(|(k, _)| k.as_str())
        .collect();

    let id3_score = four_char_keys.len();

    if id3_score > items.len() / 2 {
        // Verify at least one key matches a known ID3v2 frame ID.
        // APEv2 tags occasionally use 4-char uppercase keys (e.g. "HDCD"),
        // so the shape check alone is not sufficient.
        let has_known_frame = four_char_keys
            .iter()
            .any(|k| KNOWN_ID3V2_FRAMES.contains(k));
        if has_known_frame {
            Some(TagSystem::ID3v2)
        } else {
            Some(TagSystem::APEv2)
        }
    } else {
        Some(TagSystem::APEv2)
    }
}

/// Convert a list of key-value items (from the FileType dictionary interface)
/// into a format-agnostic TagMap using the appropriate mapping table.
pub(crate) fn items_to_tag_map(items: Vec<(String, Vec<String>)>, format_name: &str) -> TagMap {
    use crate::tagmap::mappings;
    use crate::tagmap::normalize::{TagSystem, normalize_track_disc, resolve_id3_genre};

    let mut map = TagMap::new();
    let system = tag_system_for_format(format_name).or_else(|| detect_tag_system_from_keys(&items));
    debug_event!(format = %format_name, item_count = items.len(), "extracting tags to TagMap");

    for (key, values) in items {
        if values.is_empty() {
            continue;
        }

        // Try to map the raw key to a StandardField using the correct table
        let standard = match system {
            Some(TagSystem::ID3v2) => mappings::id3_to_standard(&key),
            Some(TagSystem::VorbisComment) => mappings::vorbis_to_standard(&key),
            Some(TagSystem::MP4) => mappings::mp4_to_standard(&key),
            Some(TagSystem::APEv2) => mappings::ape_to_standard(&key),
            Some(TagSystem::ASF) => mappings::asf_to_standard(&key),
            None => None,
        };

        if let Some(field) = standard {
            // Handle special fields that need normalization
            match (&field, system) {
                // ID3v2 TRCK frame: split "N/M" into TrackNumber + TotalTracks
                (StandardField::TrackNumber, Some(TagSystem::ID3v2))
                | (StandardField::TrackNumber, Some(TagSystem::APEv2)) => {
                    if let Some(raw) = values.first() {
                        let (num, total) = normalize_track_disc(raw);
                        if let Some(n) = num {
                            map.set(StandardField::TrackNumber, vec![n]);
                        }
                        if let Some(t) = total {
                            map.set(StandardField::TotalTracks, vec![t]);
                        }
                    }
                }
                // ID3v2 TPOS frame: split "N/M" into DiscNumber + TotalDiscs
                (StandardField::DiscNumber, Some(TagSystem::ID3v2))
                | (StandardField::DiscNumber, Some(TagSystem::APEv2)) => {
                    if let Some(raw) = values.first() {
                        let (num, total) = normalize_track_disc(raw);
                        if let Some(n) = num {
                            map.set(StandardField::DiscNumber, vec![n]);
                        }
                        if let Some(t) = total {
                            map.set(StandardField::TotalDiscs, vec![t]);
                        }
                    }
                }
                // ID3v2 TCON frame: resolve numeric genre references and
                // join multi-value genres into a single comma-separated string
                (StandardField::Genre, Some(TagSystem::ID3v2)) => {
                    let resolved: Vec<String> =
                        values.iter().map(|v| resolve_id3_genre(v)).collect();
                    if resolved.len() > 1 {
                        map.set(field, vec![resolved.join(", ")]);
                    } else {
                        map.set(field, resolved);
                    }
                }
                // Non-ID3 genres: join multi-value into a single string for consistency
                (StandardField::Genre, _) => {
                    if values.len() > 1 {
                        map.set(field, vec![values.join(", ")]);
                    } else {
                        map.set(field, values);
                    }
                }
                // MP4 trkn/disk atoms: may be stored as "N/M" strings
                (StandardField::TrackNumber, Some(TagSystem::MP4)) => {
                    if let Some(raw) = values.first() {
                        let (num, total) = normalize_track_disc(raw);
                        if let Some(n) = num {
                            map.set(StandardField::TrackNumber, vec![n]);
                        }
                        if let Some(t) = total {
                            map.set(StandardField::TotalTracks, vec![t]);
                        }
                    }
                }
                (StandardField::DiscNumber, Some(TagSystem::MP4)) => {
                    if let Some(raw) = values.first() {
                        let (num, total) = normalize_track_disc(raw);
                        if let Some(n) = num {
                            map.set(StandardField::DiscNumber, vec![n]);
                        }
                        if let Some(t) = total {
                            map.set(StandardField::TotalDiscs, vec![t]);
                        }
                    }
                }
                // Publisher and Label are semantically equivalent across formats
                // (ID3 uses TPUB→Publisher, APE/Vorbis use Label→Label for the
                // same concept: the record label). Set both for cross-format parity.
                (StandardField::Publisher, _) => {
                    map.set(StandardField::Publisher, values.clone());
                    if map.get(&StandardField::Label).is_none() {
                        map.set(StandardField::Label, values);
                    }
                }
                (StandardField::Label, _) => {
                    map.set(StandardField::Label, values.clone());
                    if map.get(&StandardField::Publisher).is_none() {
                        map.set(StandardField::Publisher, values);
                    }
                }
                // EncodedBy and Encoder are semantically equivalent across formats
                // (MP4 uses ©too→EncodedBy, ID3 uses TSSE→Encoder for the same
                // concept: the encoding software). Set both for cross-format parity.
                (StandardField::EncodedBy, _) => {
                    map.set(StandardField::EncodedBy, values.clone());
                    if map.get(&StandardField::Encoder).is_none() {
                        map.set(StandardField::Encoder, values);
                    }
                }
                (StandardField::Encoder, _) => {
                    map.set(StandardField::Encoder, values.clone());
                    if map.get(&StandardField::EncodedBy).is_none() {
                        map.set(StandardField::EncodedBy, values);
                    }
                }
                // Date: prefer the longer/fuller value when set multiple times
                // (APE stores both "Year" (year-only) and "Date" (full YYYY-MM-DD))
                (StandardField::Date, _) => {
                    if let Some(existing) = map.get(&StandardField::Date) {
                        let new_len = values.first().map(|v| v.len()).unwrap_or(0);
                        let old_len = existing.first().map(|v| v.len()).unwrap_or(0);
                        if new_len > old_len {
                            map.set(field, values);
                        }
                    } else {
                        map.set(field, values);
                    }
                }
                // Default: store values directly
                _ => {
                    map.set(field, values);
                }
            }
        } else {
            // No standard mapping — store as custom field with format prefix
            let prefix = match system {
                Some(TagSystem::ID3v2) => "id3",
                Some(TagSystem::VorbisComment) => "vorbis",
                Some(TagSystem::MP4) => "mp4",
                Some(TagSystem::APEv2) => "ape",
                Some(TagSystem::ASF) => "asf",
                None => "unknown",
            };
            map.set_custom(format!("{}:{}", prefix, key), values);
        }
    }

    debug_event!(
        standard = map.standard_fields().len(),
        custom = map.custom_fields().len(),
        "TagMap extraction complete"
    );
    map
}

/// Apply a TagMap to a FileType instance using its key-value interface.
///
/// Maps StandardField variants back to format-specific keys and writes them
/// using the FileType::set method.
pub(crate) fn tag_map_to_items<T: crate::FileType>(
    file: &mut T,
    map: &TagMap,
    format_name: &str,
) -> Result<ConversionReport> {
    use crate::tagmap::mappings;
    use crate::tagmap::normalize::{TagSystem, combine_track_disc};

    // For writing, we need a concrete tag system.  If the format supports
    // multiple systems (e.g. TrueAudio → APEv2 or ID3v2), detect from the
    // file's existing tags; if the file has no tags yet, fall back to APEv2
    // as the preferred default for dual-system formats.
    let system = tag_system_for_format(format_name).or_else(|| {
        let items = file.items();
        detect_tag_system_from_keys(&items).or(Some(TagSystem::APEv2))
    });
    let mut report = ConversionReport::default();

    // An empty map should be a true no-op. Creating a tag container here would
    // silently dirty tagless files even though there is nothing to transfer.
    if map.is_empty() {
        return Ok(report);
    }

    // Ensure the file has a tag container before writing (e.g. FLAC without Vorbis Comment).
    // Some formats (e.g. TrueAudio) support multiple tag systems but tags()
    // only exposes one, so add_tags() may fail with "Tags already exist" even
    // when tags() returns None.  Ignore that specific error.
    if file.tags().is_none() {
        let _ = file.add_tags();
    }

    debug_event!(format = %format_name, "applying TagMap to format");

    // Helper closure: look up the format-specific key for a StandardField.
    // For ID3v2, also check the TXXX map so that fields like Barcode,
    // CatalogNumber, ReplayGain are written as TXXX frames.
    // For MP4, also check the freeform map for ISRC, Barcode, etc.
    // For ASF, also check the aliases for TotalTracks, TotalDiscs, etc.
    let lookup =
        |field: &StandardField| -> Option<&'static str> {
            match system {
                Some(TagSystem::ID3v2) => mappings::standard_to_id3(field)
                    .or_else(|| mappings::standard_to_id3_txxx(field)),
                Some(TagSystem::VorbisComment) => mappings::standard_to_vorbis(field),
                Some(TagSystem::MP4) => mappings::standard_to_mp4(field)
                    .or_else(|| mappings::standard_to_mp4_freeform(field)),
                Some(TagSystem::APEv2) => mappings::standard_to_ape(field),
                Some(TagSystem::ASF) => mappings::standard_to_asf(field)
                    .or_else(|| mappings::standard_to_asf_alias(field)),
                None => None,
            }
        };

    // Collect track/disc pairs so we can combine them for formats that use "N/M"
    let track_num = map.get(&StandardField::TrackNumber).map(|v| v.to_vec());
    let total_tracks = map.get(&StandardField::TotalTracks).map(|v| v.to_vec());
    let disc_num = map.get(&StandardField::DiscNumber).map(|v| v.to_vec());
    let total_discs = map.get(&StandardField::TotalDiscs).map(|v| v.to_vec());

    for (field, values) in map.standard_fields() {
        // Skip track/disc pair fields — handled separately below
        if matches!(
            field,
            StandardField::TrackNumber
                | StandardField::TotalTracks
                | StandardField::DiscNumber
                | StandardField::TotalDiscs
        ) {
            continue;
        }

        if let Some(key) = lookup(field) {
            file.set(key, values.to_vec())?;
            report.transferred.push(field.clone());
        } else {
            report
                .skipped
                .push((field.to_string(), SkipReason::UnsupportedByTarget));
        }
    }

    // Handle track number: combine or write separately depending on format
    let needs_combined = matches!(
        system,
        Some(TagSystem::ID3v2) | Some(TagSystem::APEv2) | Some(TagSystem::MP4)
    );

    if track_num.is_some() || total_tracks.is_some() {
        if needs_combined {
            // Write as "N/M" to the combined field
            if let Some(key) = lookup(&StandardField::TrackNumber) {
                let combined = combine_track_disc(
                    track_num
                        .as_ref()
                        .and_then(|v| v.first())
                        .map(|s| s.as_str()),
                    total_tracks
                        .as_ref()
                        .and_then(|v| v.first())
                        .map(|s| s.as_str()),
                );
                file.set(key, vec![combined])?;
                if track_num.is_some() {
                    report.transferred.push(StandardField::TrackNumber);
                }
                if total_tracks.is_some() {
                    report.transferred.push(StandardField::TotalTracks);
                }
            }
        } else {
            // Vorbis / ASF: write as separate fields
            if let Some(ref vals) = track_num {
                if let Some(key) = lookup(&StandardField::TrackNumber) {
                    file.set(key, vals.clone())?;
                    report.transferred.push(StandardField::TrackNumber);
                }
            }
            if let Some(ref vals) = total_tracks {
                if let Some(key) = lookup(&StandardField::TotalTracks) {
                    file.set(key, vals.clone())?;
                    report.transferred.push(StandardField::TotalTracks);
                }
            }
        }
    }

    if disc_num.is_some() || total_discs.is_some() {
        if needs_combined {
            if let Some(key) = lookup(&StandardField::DiscNumber) {
                let combined = combine_track_disc(
                    disc_num
                        .as_ref()
                        .and_then(|v| v.first())
                        .map(|s| s.as_str()),
                    total_discs
                        .as_ref()
                        .and_then(|v| v.first())
                        .map(|s| s.as_str()),
                );
                file.set(key, vec![combined])?;
                if disc_num.is_some() {
                    report.transferred.push(StandardField::DiscNumber);
                }
                if total_discs.is_some() {
                    report.transferred.push(StandardField::TotalDiscs);
                }
            }
        } else {
            if let Some(ref vals) = disc_num {
                if let Some(key) = lookup(&StandardField::DiscNumber) {
                    file.set(key, vals.clone())?;
                    report.transferred.push(StandardField::DiscNumber);
                }
            }
            if let Some(ref vals) = total_discs {
                if let Some(key) = lookup(&StandardField::TotalDiscs) {
                    file.set(key, vals.clone())?;
                    report.transferred.push(StandardField::TotalDiscs);
                }
            }
        }
    }

    // Transfer custom fields — strip the source format prefix and wrapper,
    // then re-wrap in the destination format's convention.
    // Skip binary/picture keys that cannot be transferred as text tags.
    for (custom_key, values) in map.custom_fields() {
        let bare_key = normalize_custom_key_for_transfer(custom_key);

        // Skip binary frame keys (pictures, binary data) — these need
        // the dedicated picture API, not the text tag interface.
        let bare_lower = bare_key.to_lowercase();
        if bare_lower.starts_with("apic")
            || bare_lower.contains("picture")
            || bare_lower.contains("cover art")
            || bare_lower.starts_with("rva2")
        {
            continue;
        }

        // Re-wrap the bare key for the destination format
        let dest_key = match system {
            Some(TagSystem::ID3v2) => format!("TXXX:{}", bare_key),
            Some(TagSystem::MP4) => format!("----:com.apple.itunes:{}", bare_key),
            // Vorbis, APE, ASF use bare keys directly
            _ => bare_key,
        };

        if let Err(_e) = file.set(&dest_key, values.to_vec()) {
            warn_event!(key = %dest_key, error = %_e, "failed to set custom tag");
            continue;
        }
        report.custom_transferred.push(custom_key.to_string());
    }

    info_event!(
        transferred = report.transferred.len(),
        custom = report.custom_transferred.len(),
        skipped = report.skipped.len(),
        "TagMap applied to format"
    );

    Ok(report)
}

/// Strip format-specific prefixes and wrappers from a custom tag key so it
/// can be written to any destination format as a bare tag name.
///
/// `"id3:TXXX:Songwriter"` → `"Songwriter"`
/// `"mp4:----:com.apple.itunes:Songwriter"` → `"Songwriter"`
/// `"vorbis:songwriter"` → `"songwriter"`
/// `"ape:Songwriter"` → `"Songwriter"`
fn normalize_custom_key_for_transfer(key: &str) -> String {
    // Step 1: strip the format prefix
    let stripped = key
        .strip_prefix("id3:")
        .or_else(|| key.strip_prefix("vorbis:"))
        .or_else(|| key.strip_prefix("mp4:"))
        .or_else(|| key.strip_prefix("ape:"))
        .or_else(|| key.strip_prefix("asf:"))
        .or_else(|| key.strip_prefix("unknown:"))
        .unwrap_or(key);

    // Step 2: strip format-specific key wrappers
    if let Some(desc) = stripped.strip_prefix("TXXX:") {
        return desc.to_string();
    }

    if let Some(rest) = stripped.strip_prefix("----:") {
        if let Some(pos) = rest.find(':') {
            return rest[pos + 1..].to_string();
        }
        return rest.to_string();
    }

    stripped.to_string()
}

impl DynamicFileType {
    /// Create a new dynamic file type wrapper around a concrete [`FileType`] implementation.
    ///
    /// The vtable is created once per concrete type `T` and cached in a global
    /// registry, so repeated calls with the same `T` reuse the same vtable.
    pub fn new<T: FileType + 'static>(file: T, filename: Option<PathBuf>) -> Self {
        let vtable = vtable_for::<T>();

        Self {
            inner: Box::new(file),
            vtable,
            filename,
        }
    }

    /// Save metadata back to the file
    ///
    /// Writes any tag changes back to the file using the format-specific
    /// implementation. Read-only formats (AAC, AC3, SMF) will return an error.
    ///
    /// # Errors
    ///
    /// Returns `Err(AudexError)` if the format is read-only, the file cannot
    /// be written, or an I/O error occurs.
    pub fn save(&mut self) -> Result<()> {
        info_event!(format = %self.format_name(), "saving audio file");
        let result = (self.vtable.save)(self.inner.as_mut());
        if let Err(_e) = &result {
            warn_event!(error = %_e, "failed to save audio file");
        } else {
            info_event!("file saved successfully");
        }
        result
    }

    /// Save metadata to a writer that implements Read + Write + Seek.
    ///
    /// The writer must contain the complete original file data (audio + any
    /// existing tags). The method modifies the writer in-place, just as
    /// [`save`](Self::save) modifies a file on disk.
    ///
    /// This enables in-memory round-trip workflows:
    /// ```ignore
    /// let mut cursor = Cursor::new(file_data);
    /// let mut audio = File::load_from_reader(&mut cursor, Some("song.flac".into()))?;
    /// audio.set("title", vec!["New Title".to_string()])?;
    /// audio.save_to_writer(&mut cursor)?;
    /// ```
    pub fn save_to_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        info_event!(format = %self.format_name(), "saving audio file to writer");
        let result = (self.vtable.save_to_writer)(self.inner.as_mut(), writer);
        if let Err(_e) = &result {
            warn_event!(error = %_e, "failed to save audio file to writer");
        } else {
            info_event!("file saved to writer successfully");
        }
        result
    }

    /// Clear all metadata from a writer that implements Read + Write + Seek.
    ///
    /// The writer must contain the complete original file data.
    pub fn clear_writer(&mut self, writer: &mut dyn ReadWriteSeek) -> Result<()> {
        debug_event!(format = %self.format_name(), "clearing metadata via writer");
        let result = (self.vtable.clear_writer)(self.inner.as_mut(), writer);
        if let Err(_e) = &result {
            warn_event!(error = %_e, "failed to clear metadata via writer");
        } else {
            debug_event!("metadata cleared via writer successfully");
        }
        result
    }

    /// Save metadata to a file at the given path.
    ///
    /// The target file must already exist and contain valid audio data.
    /// The method opens it in read-write mode and modifies metadata in-place.
    ///
    /// This enables saving to a different path than the one used for loading,
    /// or saving to a path after loading from a reader.
    pub fn save_to_path(&mut self, path: &Path) -> Result<()> {
        info_event!(format = %self.format_name(), path = %path.display(), "saving audio file to path");
        let result = (self.vtable.save_to_path)(self.inner.as_mut(), path);
        if let Err(_e) = &result {
            warn_event!(error = %_e, "failed to save audio file to path");
        } else {
            info_event!("file saved to path successfully");
        }
        result
    }

    /// Save metadata back to the file asynchronously
    ///
    /// This method writes any tag changes back to the file using async I/O.
    /// The specific format implementation is used based on the detected file type.
    ///
    /// # Format Support
    ///
    /// Most formats support async saving. Read-only formats (AAC, AC3, SMF) will
    /// return an error if save is attempted.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Metadata saved successfully
    /// * `Err(AudexError)` - Error occurred during save operation
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Note: This example requires the `async` feature flag
    /// use audex::File;
    /// use audex::FileType;
    ///
    /// # async fn example() -> Result<(), audex::AudexError> {
    /// let mut file = File::load_async("song.mp3").await?;
    /// file.set("artist", vec!["New Artist".to_string()])?;
    /// file.save_async().await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        let format = self.vtable.format_name;

        // IMPORTANT: When adding a new format to FORMAT_REGISTRY, you must also
        // add a corresponding match arm here and in clear_async(). The test
        // `test_async_dispatch_coverage` verifies all formats are covered.
        match format {
            "MP3" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::mp3::MP3>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for MP3".to_string())
                    })?;
                file.save_async().await
            }
            "FLAC" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::flac::FLAC>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for FLAC".to_string())
                    })?;
                file.save_async().await
            }
            "MP4" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::mp4::MP4>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for MP4".to_string())
                    })?;
                file.save_async().await
            }
            "ASF" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::asf::ASF>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for ASF".to_string())
                    })?;
                file.save_async().await
            }
            "OggVorbis" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggvorbis::OggVorbis>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggVorbis".to_string())
                    })?;
                file.save_async().await
            }
            "OggOpus" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggopus::OggOpus>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggOpus".to_string())
                    })?;
                file.save_async().await
            }
            "OggFlac" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggflac::OggFlac>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggFlac".to_string())
                    })?;
                file.save_async().await
            }
            "OggSpeex" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggspeex::OggSpeex>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggSpeex".to_string())
                    })?;
                file.save_async().await
            }
            "OggTheora" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggtheora::OggTheora>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggTheora".to_string())
                    })?;
                file.save_async().await
            }
            "AIFF" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::aiff::AIFF>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for AIFF".to_string())
                    })?;
                file.save_async().await
            }
            "WAVE" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::wave::WAVE>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for WAVE".to_string())
                    })?;
                file.save_async().await
            }
            "DSF" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::dsf::DSF>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for DSF".to_string())
                    })?;
                file.save_async().await
            }
            "DSDIFF" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::dsdiff::DSDIFF>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for DSDIFF".to_string())
                    })?;
                file.save_async().await
            }
            "MonkeysAudio" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::monkeysaudio::MonkeysAudio>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for MonkeysAudio".to_string())
                    })?;
                file.save_async().await
            }
            "WavPack" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::wavpack::WavPack>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for WavPack".to_string())
                    })?;
                file.save_async().await
            }
            "TAK" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::tak::TAK>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for TAK".to_string())
                    })?;
                file.save_async().await
            }
            "TrueAudio" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::trueaudio::TrueAudio>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for TrueAudio".to_string())
                    })?;
                file.save_async().await
            }
            "OptimFROG" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::optimfrog::OptimFROG>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OptimFROG".to_string())
                    })?;
                file.save_async().await
            }
            "Musepack" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::musepack::Musepack>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for Musepack".to_string())
                    })?;
                file.save_async().await
            }
            "APEv2" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::apev2::APEv2>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for APEv2".to_string())
                    })?;
                file.save_async().await
            }
            "ID3" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::id3::ID3>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for ID3".to_string())
                    })?;
                file.save_async().await
            }
            // Read-only formats
            "AAC" | "AC3" | "SMF" => Err(AudexError::Unsupported(format!(
                "Format {} is read-only and does not support saving",
                format
            ))),
            _ => Err(AudexError::Unsupported(format!(
                "Async save not supported for format: {}",
                format
            ))),
        }
    }

    /// Clear all metadata from the file
    ///
    /// Removes all tags from the file. Read-only formats (AAC, AC3, SMF)
    /// will return an error.
    ///
    /// # Errors
    ///
    /// Returns `Err(AudexError)` if the format is read-only or an I/O error occurs.
    pub fn clear(&mut self) -> Result<()> {
        debug_event!(format = %self.format_name(), "clearing all metadata");
        let result = (self.vtable.clear)(self.inner.as_mut());
        if let Err(_e) = &result {
            warn_event!(error = %_e, "failed to clear metadata");
        } else {
            debug_event!("metadata cleared successfully");
        }
        result
    }

    /// Add tags to the file if not already present
    ///
    /// Creates an appropriate tag format for this file type.
    /// Behavior when tags already exist is format-dependent — some formats
    /// return an error, others are a no-op.
    ///
    /// # Errors
    ///
    /// Returns `Err(AudexError)` if the format does not support adding tags
    /// or if the operation fails.
    pub fn add_tags(&mut self) -> Result<()> {
        (self.vtable.add_tags)(self.inner.as_mut())
    }

    /// Clear all metadata from the file asynchronously
    ///
    /// This method removes all tags from the file using async I/O.
    /// The specific format implementation is used based on the detected file type.
    ///
    /// # Format Support
    ///
    /// Most formats support async clearing. Read-only formats (AAC, AC3, SMF) will
    /// return an error if clear is attempted.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Metadata cleared successfully
    /// * `Err(AudexError)` - Error occurred during clear operation
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Note: This example requires the `async` feature flag
    /// use audex::File;
    /// use audex::FileType;
    ///
    /// # async fn example() -> Result<(), audex::AudexError> {
    /// let mut file = File::load_async("song.mp3").await?;
    ///
    /// // Remove all metadata tags
    /// file.clear_async().await?;
    ///
    /// // Verify tags are cleared
    /// assert!(file.keys().is_empty());
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        let format = self.vtable.format_name;

        // IMPORTANT: When adding a new format to FORMAT_REGISTRY, you must also
        // add a corresponding match arm here and in save_async(). The test
        // `test_async_dispatch_coverage` verifies all formats are covered.
        match format {
            "MP3" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::mp3::MP3>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for MP3".to_string())
                    })?;
                file.clear_async().await
            }
            "FLAC" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::flac::FLAC>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for FLAC".to_string())
                    })?;
                file.clear_async().await
            }
            "MP4" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::mp4::MP4>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for MP4".to_string())
                    })?;
                file.clear_async().await
            }
            "ASF" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::asf::ASF>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for ASF".to_string())
                    })?;
                file.clear_async().await
            }
            "OggVorbis" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggvorbis::OggVorbis>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggVorbis".to_string())
                    })?;
                file.clear_async().await
            }
            "OggOpus" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggopus::OggOpus>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggOpus".to_string())
                    })?;
                file.clear_async().await
            }
            "OggFlac" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggflac::OggFlac>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggFlac".to_string())
                    })?;
                file.clear_async().await
            }
            "OggSpeex" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggspeex::OggSpeex>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggSpeex".to_string())
                    })?;
                file.clear_async().await
            }
            "OggTheora" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::oggtheora::OggTheora>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OggTheora".to_string())
                    })?;
                file.clear_async().await
            }
            "AIFF" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::aiff::AIFF>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for AIFF".to_string())
                    })?;
                file.clear_async().await
            }
            "WAVE" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::wave::WAVE>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for WAVE".to_string())
                    })?;
                file.clear_async().await
            }
            "DSF" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::dsf::DSF>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for DSF".to_string())
                    })?;
                file.clear_async().await
            }
            "DSDIFF" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::dsdiff::DSDIFF>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for DSDIFF".to_string())
                    })?;
                file.clear_async().await
            }
            "MonkeysAudio" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::monkeysaudio::MonkeysAudio>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for MonkeysAudio".to_string())
                    })?;
                file.clear_async().await
            }
            "WavPack" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::wavpack::WavPack>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for WavPack".to_string())
                    })?;
                file.clear_async().await
            }
            "TAK" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::tak::TAK>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for TAK".to_string())
                    })?;
                file.clear_async().await
            }
            "TrueAudio" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::trueaudio::TrueAudio>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for TrueAudio".to_string())
                    })?;
                file.clear_async().await
            }
            "OptimFROG" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::optimfrog::OptimFROG>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for OptimFROG".to_string())
                    })?;
                file.clear_async().await
            }
            "Musepack" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::musepack::Musepack>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for Musepack".to_string())
                    })?;
                file.clear_async().await
            }
            "APEv2" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::apev2::APEv2>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for APEv2".to_string())
                    })?;
                file.clear_async().await
            }
            "ID3" => {
                let file = self
                    .inner
                    .downcast_mut::<crate::id3::ID3>()
                    .ok_or_else(|| {
                        AudexError::InvalidOperation("Type mismatch for ID3".to_string())
                    })?;
                file.clear_async().await
            }
            // Read-only formats
            "AAC" | "AC3" | "SMF" => Err(AudexError::Unsupported(format!(
                "Format {} is read-only and does not support clearing tags",
                format
            ))),
            _ => Err(AudexError::Unsupported(format!(
                "Async clear not supported for format: {}",
                format
            ))),
        }
    }

    pub fn has_tags(&self) -> bool {
        (self.vtable.has_tags)(self.inner.as_ref())
    }

    pub fn tags_pprint(&self) -> Option<String> {
        (self.vtable.tags_pprint)(self.inner.as_ref())
    }

    pub fn info_pprint(&self) -> String {
        (self.vtable.info_pprint)(self.inner.as_ref())
    }

    pub fn info(&self) -> DynamicStreamInfo {
        (self.vtable.info)(self.inner.as_ref())
    }

    pub fn mime_types(&self) -> &'static [&'static str] {
        (self.vtable.mime_types)()
    }

    pub fn format_name(&self) -> &'static str {
        self.vtable.format_name
    }

    pub fn filename(&self) -> Option<&Path> {
        self.filename.as_deref()
    }

    pub fn downcast_ref<T: FileType + 'static>(&self) -> Option<&T> {
        self.inner.downcast_ref::<T>()
    }

    pub fn downcast_mut<T: FileType + 'static>(&mut self) -> Option<&mut T> {
        self.inner.downcast_mut::<T>()
    }

    /// Create a format-agnostic serializable snapshot of this file's metadata.
    ///
    /// Captures format name, stream info, and all tags as key-value pairs.
    /// The `raw_tags` field is left empty — use [`to_snapshot_with_raw`](Self::to_snapshot_with_raw)
    /// if you need lossless round-trip fidelity.
    #[cfg(feature = "serde")]
    pub fn to_snapshot(&self) -> crate::snapshot::TagSnapshot {
        trace_event!("building tag snapshot for {}", self.format_name());

        let tag_map: std::collections::HashMap<String, Vec<String>> =
            self.items().into_iter().collect();

        crate::snapshot::TagSnapshot {
            format: self.format_name().to_string(),
            filename: self.filename().map(|p| p.to_string_lossy().to_string()),
            stream_info: crate::snapshot::StreamInfoSnapshot::from_dynamic(&self.info()),
            tags: tag_map,
            raw_tags: None,
        }
    }

    /// Create a snapshot that also includes format-specific raw tag data.
    ///
    /// The `raw_tags` field is populated by attempting to serialize the
    /// underlying format-specific tag container to a `serde_json::Value`.
    /// This preserves information that the flat `tags` map cannot
    /// represent (e.g. MP4 freeform atoms, ID3 picture frames).
    ///
    /// Falls back to `None` for formats whose tags cannot be serialized.
    #[cfg(feature = "serde")]
    pub fn to_snapshot_with_raw(&self) -> crate::snapshot::TagSnapshot {
        trace_event!(
            "building tag snapshot (with raw) for {}",
            self.format_name()
        );

        let mut snapshot = self.to_snapshot();

        // Try serializing known format-specific tag containers via downcast
        snapshot.raw_tags = self.try_serialize_raw_tags();
        snapshot
    }

    /// Attempt to downcast the inner format and serialize its tags to JSON.
    ///
    /// Returns `None` if the format is not recognized or serialization fails.
    #[cfg(feature = "serde")]
    fn try_serialize_raw_tags(&self) -> Option<serde_json::Value> {
        use crate::flac::FLAC;
        use crate::mp3::MP3;
        use crate::mp4::MP4;

        // Try each known format that has serializable tags
        if let Some(mp3) = self.inner.downcast_ref::<MP3>() {
            if let Some(ref tags) = mp3.tags {
                return serde_json::to_value(tags).ok();
            }
        }
        if let Some(flac) = self.inner.downcast_ref::<FLAC>() {
            if let Some(ref tags) = flac.tags {
                return serde_json::to_value(tags).ok();
            }
        }
        if let Some(mp4) = self.inner.downcast_ref::<MP4>() {
            if let Some(ref tags) = mp4.tags {
                return serde_json::to_value(tags).ok();
            }
        }

        None
    }

    // Key-value interface methods for metadata access

    /// Get values for a tag key, returning owned strings
    ///
    /// Unlike [`Tags::get`] which returns `Option<&[String]>` (borrowed), this
    /// returns `Option<Vec<String>>` (owned) due to dynamic dispatch.
    pub fn get(&self, key: &str) -> Option<Vec<String>> {
        (self.vtable.get)(self.inner.as_ref(), key)
    }

    /// Set values for a tag key
    ///
    /// # Errors
    ///
    /// Returns `Err` if the format does not support tags or a type mismatch occurs.
    pub fn set(&mut self, key: &str, values: Vec<String>) -> Result<()> {
        (self.vtable.set)(self.inner.as_mut(), key, values)
    }

    /// Remove a tag key
    ///
    /// # Errors
    ///
    /// Returns `Err` if the format does not support tags or a type mismatch occurs.
    pub fn remove(&mut self, key: &str) -> Result<()> {
        (self.vtable.remove)(self.inner.as_mut(), key)
    }

    pub fn keys(&self) -> Vec<String> {
        (self.vtable.keys)(self.inner.as_ref())
    }

    pub fn contains_key(&self, key: &str) -> bool {
        (self.vtable.contains_key)(self.inner.as_ref(), key)
    }

    /// Get the first value for a key, returning an owned string
    ///
    /// Unlike [`Tags::get_first`] which returns `Option<&String>` (borrowed),
    /// this returns `Option<String>` (owned) due to dynamic dispatch.
    pub fn get_first(&self, key: &str) -> Option<String> {
        (self.vtable.get_first)(self.inner.as_ref(), key)
    }

    /// Set a single value for a key
    ///
    /// # Errors
    ///
    /// Returns `Err` if the format does not support tags or a type mismatch occurs.
    pub fn set_single(&mut self, key: &str, value: String) -> Result<()> {
        (self.vtable.set_single)(self.inner.as_mut(), key, value)
    }

    pub fn len(&self) -> usize {
        (self.vtable.len)(self.inner.as_ref())
    }

    pub fn is_empty(&self) -> bool {
        (self.vtable.is_empty)(self.inner.as_ref())
    }

    pub fn items(&self) -> Vec<(String, Vec<String>)> {
        (self.vtable.items)(self.inner.as_ref())
    }

    pub fn values(&self) -> Vec<Vec<String>> {
        self.items().into_iter().map(|(_, v)| v).collect()
    }

    /// Update tags from another set of key-value pairs
    ///
    /// # Errors
    ///
    /// Returns `Err` if any individual `set` call fails (e.g., format is read-only).
    pub fn update(&mut self, other: Vec<(String, Vec<String>)>) -> Result<()> {
        for (key, values) in other {
            self.set(&key, values)?;
        }
        Ok(())
    }

    /// Get a value with a default if not present
    pub fn get_or(&self, key: &str, default: Vec<String>) -> Vec<String> {
        (self.vtable.get_or)(self.inner.as_ref(), key, default)
    }

    /// Pop a key and return its values, removing it from the tags
    ///
    /// # Errors
    ///
    /// Returns `Err` if the underlying `remove` call fails.
    pub fn pop(&mut self, key: &str) -> Result<Option<Vec<String>>> {
        (self.vtable.pop)(self.inner.as_mut(), key)
    }

    /// Pop a key with a default value, removing it from the tags
    ///
    /// # Errors
    ///
    /// Returns `Err` if the underlying `remove` call fails.
    pub fn pop_or(&mut self, key: &str, default: Vec<String>) -> Result<Vec<String>> {
        (self.vtable.pop_or)(self.inner.as_mut(), key, default)
    }

    // ----- Tag diffing convenience methods -----

    /// Compare this file's tags against another file's tags.
    pub fn diff_tags(&self, other: &DynamicFileType) -> crate::diff::TagDiff {
        crate::diff::diff(self, other)
    }

    /// Compare this file's tags against another file's tags with options.
    pub fn diff_tags_with_options(
        &self,
        other: &DynamicFileType,
        options: &crate::diff::DiffOptions,
    ) -> crate::diff::TagDiff {
        crate::diff::diff_with_options(self, other, options)
    }

    /// Capture current tags as a snapshot for later diffing.
    pub fn tag_snapshot_items(&self) -> Vec<(String, Vec<String>)> {
        self.items()
    }

    /// Compare current tags against a previous snapshot.
    ///
    /// The snapshot is treated as the "before" (left) side.
    pub fn diff_against(&self, snapshot: &[(String, Vec<String>)]) -> crate::diff::TagDiff {
        crate::diff::diff_against_snapshot(self, snapshot)
    }

    /// Compare this file's tags against another using normalized field names.
    ///
    /// Both files are converted to a [`TagMap`] first, so cross-format
    /// comparisons (e.g. MP3 vs FLAC) match on canonical field names like
    /// `"Artist"` rather than raw keys like `"TPE1"` vs `"ARTIST"`.
    pub fn diff_tags_normalized(&self, other: &DynamicFileType) -> crate::diff::TagDiff {
        crate::diff::diff_normalized(self, other)
    }

    /// Compare using normalized field names with configurable options.
    pub fn diff_tags_normalized_with_options(
        &self,
        other: &DynamicFileType,
        options: &crate::diff::DiffOptions,
    ) -> crate::diff::TagDiff {
        crate::diff::diff_normalized_with_options(self, other, options)
    }

    // ----- Tag conversion methods -----

    /// Extract all tags into a format-agnostic [`TagMap`].
    ///
    /// The returned map contains both standard fields (mapped via the format's
    /// field table) and custom fields (prefixed with the format name).
    pub fn to_tag_map(&self) -> TagMap {
        (self.vtable.to_tag_map)(self.inner.as_ref())
    }

    /// Apply a [`TagMap`] to this file's tags.
    ///
    /// Returns a [`ConversionReport`] describing which fields were transferred,
    /// which were skipped, and why.
    pub fn apply_tag_map(&mut self, map: &TagMap) -> Result<ConversionReport> {
        (self.vtable.apply_tag_map)(self.inner.as_mut(), map)
    }

    /// Import all tags from another file, performing automatic cross-format mapping.
    ///
    /// Convenience wrapper around [`crate::tagmap::convert_tags`].
    pub fn import_tags_from(&mut self, source: &DynamicFileType) -> Result<ConversionReport> {
        crate::tagmap::convert_tags(source, self)
    }

    /// Import tags from another file with options controlling the transfer.
    ///
    /// Convenience wrapper around [`crate::tagmap::convert_tags_with_options`].
    pub fn import_tags_from_with_options(
        &mut self,
        source: &DynamicFileType,
        options: &crate::tagmap::ConversionOptions,
    ) -> Result<ConversionReport> {
        crate::tagmap::convert_tags_with_options(source, self, options)
    }
}

impl fmt::Debug for DynamicFileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DynamicFileType")
            .field("format", &self.vtable.format_name)
            .field("filename", &self.filename)
            .field("has_tags", &self.has_tags())
            .finish()
    }
}

impl fmt::Display for DynamicFileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}", self.format_name())?;

        if let Some(filename) = self.filename() {
            write!(f, " '{}'>", filename.display())?;
        } else {
            write!(f, " '(no filename)'>")?
        }
        Ok(())
    }
}

/// Iterator over `(key, values)` pairs from a `DynamicFileType`.
///
/// Produced by `for (key, values) in &file { ... }` via the `IntoIterator` impl
/// on `&DynamicFileType`.
pub struct DynamicFileTypeIter {
    items: std::vec::IntoIter<(String, Vec<String>)>,
}

impl Iterator for DynamicFileTypeIter {
    type Item = (String, Vec<String>);

    fn next(&mut self) -> Option<Self::Item> {
        self.items.next()
    }
}

impl IntoIterator for &DynamicFileType {
    type Item = (String, Vec<String>);
    type IntoIter = DynamicFileTypeIter;

    fn into_iter(self) -> Self::IntoIter {
        DynamicFileTypeIter {
            items: self.items().into_iter(),
        }
    }
}

/// Load file from path with format auto-detection
fn load_file_from_path(path: &Path) -> Result<DynamicFileType> {
    debug_event!(path = %path.display(), "opening file for format detection");
    let mut file = StdFile::open(path)?;
    let mut header = [0u8; 128];
    let bytes_read = file.read(&mut header)?;
    let descriptor = score_all_formats(path, &header[..bytes_read])?;
    debug_event!(format = %descriptor.name, "format detected, loading file");
    load_with_descriptor(descriptor, path, None)
}

/// Score all available formats and return the best descriptor.
fn score_all_formats(path: &Path, header: &[u8]) -> Result<&'static FormatDescriptor> {
    let filename = path.to_string_lossy();
    let mut best_score = 0;
    let mut best_descriptor: Option<&FormatDescriptor> = None;

    for descriptor in FORMAT_REGISTRY {
        let score = descriptor.score(&filename, header);
        // Log individual format scores at trace level for debugging detection issues
        trace_event!(format = %descriptor.name, score = score, "format score");
        if score > best_score {
            best_score = score;
            best_descriptor = Some(descriptor);
        }
    }

    match (best_descriptor, best_score) {
        (Some(descriptor), score) if score > 0 => {
            debug_event!(format = %descriptor.name, score = score, "winning format detected");
            Ok(descriptor)
        }
        _ => {
            warn_event!(path = %path.display(), "no format could handle this file");
            Err(AudexError::UnsupportedFormat(
                "No format could handle this file".to_string(),
            ))
        }
    }
}

/// Load file using a specific descriptor.
fn load_with_descriptor(
    descriptor: &FormatDescriptor,
    path: &Path,
    _file_thing: Option<AnyFileThing>,
) -> Result<DynamicFileType> {
    descriptor.load(path)
}

/// Auto-detect file format name using header-based scoring.
///
/// This function uses the same scoring mechanism as file loading to ensure
/// that only formats with registered loaders are returned.
///
/// # Arguments
/// * `path` - Path to the file to detect
///
/// # Returns
/// * `Ok(String)` - The detected format name
/// * `Err(AudexError::UnsupportedFormat)` - If no loader can handle the file
///
/// # Example
/// ```rust,no_run
/// use audex::detect_format;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let format = detect_format("/path/to/audio.mp3")?;
/// assert_eq!(format, "MP3");
/// # Ok(())
/// # }
/// ```
#[cfg_attr(feature = "tracing", tracing::instrument(skip_all, fields(path = %path.as_ref().display())))]
pub fn detect_format<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();

    debug_event!("detecting audio format");

    // Open file and read header for scoring
    let mut file = StdFile::open(path)?;
    let mut header = [0u8; 128];
    let bytes_read = file.read(&mut header)?;

    // Use the same scoring mechanism as load_file_from_path
    let descriptor = score_all_formats(path, &header[..bytes_read])?;

    debug_event!(format = %descriptor.name, "format detection complete");

    // Return the format name from the descriptor that can actually load this file
    Ok(descriptor.name.to_string())
}

/// Detect audio format from raw bytes using header-based scoring only.
///
/// Unlike [`File::load_from_reader`], this does not attempt a full parse —
/// it reads only the first 128 bytes for magic-byte and header scoring.
/// This makes it safe to call on partial buffers (e.g., only the first
/// few KB of a large file).
///
/// # Arguments
/// * `data` - Raw audio bytes (only the first 128 bytes are examined)
/// * `filename_hint` - Optional filename for extension-based scoring
///
/// # Returns
/// * `Ok(String)` - The detected format name (e.g., "MP3", "FLAC")
/// * `Err(AudexError::UnsupportedFormat)` - If no format matches
pub fn detect_format_from_bytes(data: &[u8], filename_hint: Option<&Path>) -> Result<String> {
    let fallback = PathBuf::from("unknown");
    let path = filename_hint.unwrap_or(&fallback);

    // Only the first 128 bytes are needed for scoring
    let header_len = data.len().min(128);
    let descriptor = score_all_formats(path, &data[..header_len])?;

    Ok(descriptor.name.to_string())
}

/// Primary entry point for loading audio files with automatic format detection.
///
/// `FileStruct` is a zero-sized type with only static methods. It is re-exported
/// as [`File`](crate::File) at the crate root.
///
/// All methods return `DynamicFileType`, which provides a unified interface
/// for working with any supported audio format via dynamic dispatch.
///
/// # Examples
///
/// ```no_run
/// use audex::File;
///
/// # fn main() -> Result<(), audex::AudexError> {
/// let mut file = File::load("song.mp3")?;
/// println!("Format: {}", file.format_name());
/// file.set("artist", vec!["New Artist".to_string()])?;
/// file.save()?;
/// # Ok(())
/// # }
/// ```
pub struct FileStruct;

impl FileStruct {
    /// Load a file using auto-detection, returning a DynamicFileType
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all, fields(path = %path.as_ref().display())))]
    pub fn load<P: AsRef<Path>>(path: P) -> Result<DynamicFileType> {
        info_event!("loading audio file");
        let result = load_file_from_path(path.as_ref());
        if let Ok(_file) = &result {
            let _format_name = _file.format_name();
            info_event!(format = %_format_name, "file loaded successfully");
        } else if let Err(_e) = &result {
            warn_event!(error = %_e, "failed to load audio file");
        }
        result
    }

    /// Load a file from a reader that implements Read + Seek
    ///
    /// This allows dynamic loading from sources other than filesystem paths,
    /// such as in-memory buffers or network streams.
    ///
    /// # Arguments
    /// * `reader` - Any type implementing Read + Seek
    /// * `origin_path` - Optional path hint used for format scoring (extension matching). Providing this significantly improves format detection accuracy
    ///
    /// # Example
    /// ```rust,no_run
    /// use audex::File;
    /// use std::io::Cursor;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let data = std::fs::read("/path/to/audio.mp3")?;
    /// let cursor = Cursor::new(data);
    /// let file = File::load_from_reader(cursor, None)?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "tracing", tracing::instrument(skip(reader), fields(origin_path = ?origin_path)))]
    pub fn load_from_reader<R: Read + Seek>(
        mut reader: R,
        origin_path: Option<PathBuf>,
    ) -> Result<DynamicFileType> {
        info_event!("loading audio file from reader");

        // Read header for format detection
        let mut header = [0u8; 128];
        let bytes_read = reader.read(&mut header)?;
        reader.seek(std::io::SeekFrom::Start(0))?;

        // Detect format using header and optional path hint
        let fallback_path = PathBuf::from("unknown");
        let detect_path = origin_path.as_ref().unwrap_or(&fallback_path);
        let descriptor = score_all_formats(detect_path, &header[..bytes_read])?;

        debug_event!(format = %descriptor.name, "format detected from reader");

        // Use reader-based loader directly — no temp file needed
        if let Some(reader_loader) = descriptor.load_from_reader_fn {
            let result = reader_loader(&mut reader);
            if let Err(_e) = &result {
                warn_event!(error = %_e, format = %descriptor.name, "failed to load from reader");
            } else {
                info_event!(format = %descriptor.name, "file loaded successfully from reader");
            }
            return result;
        }

        let err = AudexError::Unsupported(format!(
            "Format '{}' does not support loading from a reader",
            descriptor.name
        ));
        warn_event!(error = %err, "reader-based loading not supported for format");
        Err(err)
    }

    /// Load a file asynchronously using automatic format detection.
    ///
    /// This method reads the file header asynchronously to detect the format,
    /// then loads the file using the appropriate async handler.
    ///
    /// # Arguments
    /// * `path` - Path to the audio file
    ///
    /// # Returns
    /// * `Ok(DynamicFileType)` - Successfully loaded file with metadata
    /// * `Err(AudexError)` - Error occurred during loading
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<DynamicFileType> {
        use tokio::io::AsyncReadExt;

        let path = path.as_ref();

        // Open file and read header asynchronously
        let mut file = TokioFile::open(path).await?;
        let mut header = [0u8; 128];
        let bytes_read = file.read(&mut header).await?;

        // Score all formats to find the best match
        let filename = path.to_string_lossy();
        let mut best_score = 0;
        let mut best_format = "";

        for descriptor in FORMAT_REGISTRY {
            let score = descriptor.score(&filename, &header[..bytes_read]);
            if score > best_score {
                best_score = score;
                best_format = descriptor.name;
            }
        }

        if best_score == 0 {
            return Err(AudexError::UnsupportedFormat(
                "No format could handle this file".to_string(),
            ));
        }

        // Call format-specific async loaders
        let path_buf = Some(path.to_path_buf());
        match best_format {
            "MP3" => {
                let f = crate::mp3::MP3::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "FLAC" => {
                let f = crate::flac::FLAC::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "MP4" => {
                let f = crate::mp4::MP4::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "ASF" => {
                let f = crate::asf::ASF::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "OggVorbis" => {
                let f = crate::oggvorbis::OggVorbis::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "OggOpus" => {
                let f = crate::oggopus::OggOpus::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "OggFlac" => {
                let f = crate::oggflac::OggFlac::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "OggSpeex" => {
                let f = crate::oggspeex::OggSpeex::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "OggTheora" => {
                let f = crate::oggtheora::OggTheora::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "AIFF" => {
                let f = crate::aiff::AIFF::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "WAVE" => {
                let f = crate::wave::WAVE::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "DSF" => {
                let f = crate::dsf::DSF::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "DSDIFF" => {
                let f = crate::dsdiff::DSDIFF::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "MonkeysAudio" => {
                let f = crate::monkeysaudio::MonkeysAudio::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "WavPack" => {
                let f = crate::wavpack::WavPack::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "TAK" => {
                let f = crate::tak::TAK::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "TrueAudio" => {
                let f = crate::trueaudio::TrueAudio::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "OptimFROG" => {
                let f = crate::optimfrog::OptimFROG::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "Musepack" => {
                let f = crate::musepack::Musepack::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "APEv2" => {
                let f = crate::apev2::APEv2::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "AAC" => {
                let f = crate::aac::AAC::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            "SMF" => {
                let f = crate::smf::SMF::load_async(path).await?;
                Ok(DynamicFileType::new(f, path_buf))
            }
            _ => Err(AudexError::Unsupported(format!(
                "Async loading not supported for format: {}",
                best_format
            ))),
        }
    }

    /// Load a file from a buffer asynchronously.
    ///
    /// This method wraps the buffer in a [`Cursor`](std::io::Cursor) and uses
    /// the reader-based format loaders directly — no temporary file is created.
    ///
    /// # Arguments
    /// * `data` - The file data as bytes
    /// * `origin_path` - Optional path hint for format detection
    ///
    /// # Returns
    /// * `Ok(DynamicFileType)` - Successfully loaded file with metadata
    /// * `Err(AudexError)` - Error occurred during loading
    #[cfg(feature = "async")]
    pub async fn load_from_buffer_async(
        data: Vec<u8>,
        origin_path: Option<PathBuf>,
    ) -> Result<DynamicFileType> {
        // Detect format from buffer header
        let header_len = data.len().min(128);
        let fallback_path = PathBuf::from("unknown");
        let detect_path = origin_path.as_ref().unwrap_or(&fallback_path);
        let descriptor = score_all_formats(detect_path, &data[..header_len])?;

        if let Some(reader_loader) = descriptor.load_from_reader_fn {
            // Use reader-based loader with a Cursor wrapping the buffer.
            // All operations on a Cursor are purely in-memory (no blocking I/O),
            // so calling the sync loader directly is safe in an async context.
            let mut cursor = std::io::Cursor::new(data);
            return reader_loader(&mut cursor);
        }

        Err(AudexError::Unsupported(format!(
            "Format '{}' does not support loading from a reader",
            descriptor.name
        )))
    }
}

/// Export FileStruct as File for the struct API
pub use FileStruct as File;

/// Auto-detect file format asynchronously.
///
/// This function uses async I/O to read the file header and detect the format.
///
/// # Arguments
/// * `path` - Path to the file to detect
///
/// # Returns
/// * `Ok(String)` - The detected format name
/// * `Err(AudexError)` - If no format can handle the file
#[cfg(feature = "async")]
pub async fn detect_format_async<P: AsRef<Path>>(path: P) -> Result<String> {
    use tokio::io::AsyncReadExt;

    let path = path.as_ref();

    // Open file and read header asynchronously
    let mut file = TokioFile::open(path).await?;
    let mut header = [0u8; 128];
    let bytes_read = file.read(&mut header).await?;

    detect_format_from_header(path, &header[..bytes_read])
}

/// Detect file format from header bytes.
///
/// This function analyzes the file header to determine the audio format
/// without loading the entire file.
#[cfg(feature = "async")]
fn detect_format_from_header(path: &Path, header: &[u8]) -> Result<String> {
    let filename = path.to_string_lossy();

    // Check for common magic bytes
    let format = if header.len() >= 4 {
        if header.starts_with(b"ID3") {
            "MP3"
        } else if header.starts_with(b"fLaC") {
            "FLAC"
        } else if header.len() >= 8 && &header[4..8] == b"ftyp" {
            "MP4"
        } else if header.starts_with(b"OggS") {
            detect_ogg_format(header)
        } else if header.starts_with(b"RIFF") {
            "WAVE"
        } else if header.starts_with(b"FORM") {
            // IFF container — AIFF is the only supported IFF variant
            "AIFF"
        } else if header.starts_with(&[0x30, 0x26, 0xB2, 0x75]) {
            "ASF"
        } else if header.starts_with(b"DSD ") {
            "DSF"
        } else if header.starts_with(b"FRM8") {
            "DSDIFF"
        } else if header.starts_with(b"MAC ") {
            "MonkeysAudio"
        } else if header.starts_with(b"wvpk") {
            "WavPack"
        } else if header.starts_with(b"MThd") {
            "SMF"
        } else if is_mp3_frame_for_async(header) {
            "MP3"
        } else {
            detect_by_extension_for_async(&filename)
        }
    } else {
        detect_by_extension_for_async(&filename)
    };

    if format == "Unknown" {
        Err(AudexError::UnsupportedFormat(
            "No format could handle this file".to_string(),
        ))
    } else {
        Ok(format.to_string())
    }
}

/// Detect specific OGG format from header bytes.
/// Requires at least 36 bytes to identify sub-formats like Opus, Vorbis, etc.
pub fn detect_ogg_format(header: &[u8]) -> &'static str {
    // "OpusHead" at offset 28 needs 36 bytes minimum (28 + 8)
    if header.len() >= 36 {
        if header[28..].starts_with(b"OpusHead") {
            return "OggOpus";
        }
        if header[29..].starts_with(b"vorbis") {
            return "OggVorbis";
        }
        if header[29..].starts_with(b"FLAC") {
            return "OggFlac";
        }
        if header[28..].starts_with(b"Speex") {
            return "OggSpeex";
        }
        // Unrecognized Ogg sub-format — return generic Ogg instead of
        // silently assuming OggVorbis which would cause parse failures
        return "Ogg";
    }
    // Header too short to identify sub-format
    "Ogg"
}

/// Detect format by file extension.
#[cfg(feature = "async")]
fn detect_by_extension_for_async(filename: &str) -> &'static str {
    let lower = filename.to_lowercase();

    if lower.ends_with(".mp3") || lower.ends_with(".mp2") {
        "MP3"
    } else if lower.ends_with(".flac") {
        "FLAC"
    } else if lower.ends_with(".m4a")
        || lower.ends_with(".mp4")
        || lower.ends_with(".m4b")
        || lower.ends_with(".m4p")
    {
        "MP4"
    } else if lower.ends_with(".ogg") {
        "OggVorbis"
    } else if lower.ends_with(".opus") {
        "OggOpus"
    } else if lower.ends_with(".wav") {
        "WAVE"
    } else if lower.ends_with(".aiff") || lower.ends_with(".aif") {
        "AIFF"
    } else if lower.ends_with(".wma") || lower.ends_with(".asf") {
        "ASF"
    } else if lower.ends_with(".dsf") {
        "DSF"
    } else if lower.ends_with(".dff") {
        "DSDIFF"
    } else if lower.ends_with(".ape") {
        "MonkeysAudio"
    } else if lower.ends_with(".wv") {
        "WavPack"
    } else if lower.ends_with(".mid") || lower.ends_with(".midi") {
        "SMF"
    } else if lower.ends_with(".aac") {
        "AAC"
    } else if lower.ends_with(".ac3") {
        "AC3"
    } else if lower.ends_with(".mpc") {
        "Musepack"
    } else if lower.ends_with(".tak") {
        "TAK"
    } else if lower.ends_with(".tta") {
        "TrueAudio"
    } else if lower.ends_with(".ofr") || lower.ends_with(".ofs") {
        "OptimFROG"
    } else if lower.ends_with(".spx") {
        "OggSpeex"
    } else if lower.ends_with(".ogv") {
        "OggTheora"
    } else {
        "Unknown"
    }
}

/// Check if header looks like an MP3 frame sync.
#[cfg(feature = "async")]
fn is_mp3_frame_for_async(header: &[u8]) -> bool {
    if header.len() < 2 {
        return false;
    }
    // Check for frame sync (11 bits set)
    header[0] == 0xFF && (header[1] & 0xE0) == 0xE0
}
