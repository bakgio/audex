//! MP4 atom structure and parsing.
//!
//! This module provides low-level atom (box) parsing and navigation for MP4/M4A files.
//! MP4 files are structured as a hierarchy of atoms, where each atom contains either data
//! or child atoms. Atoms are identified by four-character codes and contain size information.
//!
//! # Atom Structure
//!
//! Each atom consists of:
//! - **Size** (4 or 8 bytes): Total size including header
//! - **Type** (4 bytes): Four-character code identifying the atom
//! - **Data** (variable): Either raw data or child atoms
//!
//! # Common Atoms
//!
//! - **ftyp**: File type identification
//! - **moov**: Movie/metadata container
//! - **mdat**: Media data (compressed audio/video)
//! - **udta**: User data container
//! - **meta**: Metadata container
//! - **ilst**: iTunes-style tag list
//! - **trak**: Track information
//!
//! # Examples
//!
//! ## Parsing atoms from a file
//!
//! ```no_run
//! use audex::mp4::Atoms;
//! use std::fs::File;
//! use std::io::BufReader;
//!
//! let file = File::open("song.m4a").unwrap();
//! let mut reader = BufReader::new(file);
//!
//! // Parse all atoms
//! let atoms = Atoms::parse(&mut reader).unwrap();
//!
//! // Navigate to specific atoms
//! if let Some(ilst) = atoms.get("moov.udta.meta.ilst") {
//!     println!("Found iTunes metadata atom at offset {}", ilst.offset);
//! }
//! ```
//!
//! ## Navigating the atom hierarchy
//!
//! ```no_run
//! use audex::mp4::Atoms;
//! use std::fs::File;
//! use std::io::BufReader;
//!
//! let file = File::open("song.m4a").unwrap();
//! let mut reader = BufReader::new(file);
//! let atoms = Atoms::parse(&mut reader).unwrap();
//!
//! // Check if a path exists
//! if atoms.contains("moov.udta.meta.ilst") {
//!     println!("File has iTunes metadata");
//! }
//!
//! // Get full path of atoms
//! if let Some(path) = atoms.path("moov.trak.mdia") {
//!     for atom in path {
//!         println!("Atom: {:?} at offset {}",
//!             std::str::from_utf8(&atom.name).unwrap(), atom.offset);
//!     }
//! }
//! ```

use crate::{AudexError, Result};
use std::io::{Read, Seek, SeekFrom, Write};

/// Container atoms that can contain child atoms
const CONTAINERS: &[&[u8; 4]] = &[
    b"moov", b"udta", b"trak", b"mdia", b"meta", b"ilst", b"stbl", b"minf", b"moof", b"traf",
];

fn is_container_atom(name: &[u8; 4], in_ilst_children: bool) -> bool {
    if in_ilst_children {
        return false;
    }
    CONTAINERS.contains(&name)
}

/// Atoms that skip some bytes before children (meta skips 4 bytes)
const SKIP_SIZE: &[(&[u8; 4], usize)] = &[(b"meta", 4)];

/// MP4 atom type identifier.
///
/// This enum represents the various atom types found in MP4/M4A files. Each atom
/// type serves a specific purpose in the file structure, from identifying file type
/// to storing audio data and metadata.
///
/// # Categories
///
/// ## File Structure Atoms
/// - **Ftyp**: File type identification (brand and compatibility)
/// - **Moov**: Movie container (holds all metadata)
/// - **Mvhd**: Movie header (timescale, duration)
/// - **Mdat**: Media data (compressed audio/video)
/// - **Free/Skip**: Free space for metadata growth
///
/// ## Metadata Atoms
/// - **Udta**: User data container
/// - **Meta**: Metadata container
/// - **Hdlr**: Handler reference (identifies metadata type)
/// - **Ilst**: iTunes-style tag list
///
/// ## Track Structure Atoms
/// - **Trak**: Track container
/// - **Tkhd**: Track header
/// - **Mdia**: Media information container
/// - **Mdhd**: Media header (track timescale)
/// - **Minf**: Media information
/// - **Stbl**: Sample table (describes media samples)
/// - **Stsd**: Sample descriptions (codec information)
/// - **Stts**: Time-to-sample mapping
/// - **Stsc**: Sample-to-chunk mapping
/// - **Stsz**: Sample size table
/// - **Stco**: Chunk offset table (32-bit)
/// - **Co64**: Chunk offset table (64-bit)
///
/// ## Audio Codec Atoms
/// - **Mp4a**: MPEG-4 audio (usually AAC)
/// - **Alac**: Apple Lossless audio codec
/// - **Ac3**: Dolby Digital audio
///
/// ## Descriptor Atoms
/// - **Esds**: Elementary stream descriptor (AAC config)
/// - **Dac3**: Dolby Digital audio descriptor
///
/// ## Data Atoms
/// - **Data**: Data value container (used in iTunes tags)
///
/// ## Movie Fragment Atoms
/// - **Moof**: Movie fragment
/// - **Mfhd**: Movie fragment header
/// - **Traf**: Track fragment
/// - **Tfhd**: Track fragment header
/// - **Trun**: Track fragment run
///
/// # Examples
///
/// ```
/// use audex::mp4::AtomType;
///
/// // Convert from byte array
/// let ftyp = AtomType::from_bytes(b"ftyp");
/// assert_eq!(ftyp, AtomType::Ftyp);
///
/// // Check if atom is a container
/// assert!(AtomType::Moov.is_container());
/// assert!(!AtomType::Mdat.is_container());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AtomType {
    Unknown([u8; 4]),
    // File structure
    Ftyp,
    Moov,
    Mvhd,
    Trak,
    Mdat,
    Free,
    Skip,
    // Metadata structure
    Udta,
    Meta,
    Hdlr,
    Ilst,
    // Track structure
    Tkhd,
    Mdia,
    Mdhd,
    Minf,
    Stbl,
    Stsd,
    Stts,
    Stsc,
    Stsz,
    Stco,
    Co64,
    // Audio codecs
    Mp4a,
    Alac,
    Ac3,
    // Descriptor atoms
    Esds,
    Dac3,
    // Data atoms
    Data,
    // Movie fragments
    Moof,
    Mfhd,
    Traf,
    Tfhd,
    Trun,
}

/// Error type for atom-level parsing failures.
#[derive(Debug, Clone)]
pub struct AtomError {
    pub message: String,
}

impl std::fmt::Display for AtomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Atom error: {}", self.message)
    }
}

impl std::error::Error for AtomError {}

/// Individual MP4 atom with structure and position information.
///
/// Represents a single atom (box) in an MP4 file, including its type, size, position,
/// and optional child atoms for container types.
///
/// # Structure
///
/// - **`atom_type`**: Parsed atom type identifier
/// - **`name`**: Raw four-byte atom name
/// - **`length`**: Total atom size including header (in bytes)
/// - **`offset`**: File position where atom starts
/// - **`data_offset`**: File position where atom data starts (after header)
/// - **`data_length`**: Size of atom data excluding header
/// - **`children`**: Child atoms if this is a container type
///
/// # Container vs. Leaf Atoms
///
/// - **Container atoms** (moov, udta, trak, etc.) contain child atoms
/// - **Leaf atoms** (mdat, ftyp, etc.) contain raw data
///
/// # Examples
///
/// ## Reading atom data
///
/// ```no_run
/// use audex::mp4::{Atoms, MP4Atom};
/// use std::fs::File;
/// use std::io::BufReader;
///
/// let file = File::open("song.m4a").unwrap();
/// let mut reader = BufReader::new(file);
/// let atoms = Atoms::parse(&mut reader).unwrap();
///
/// if let Some(ftyp) = atoms.get("ftyp") {
///     println!("File type atom:");
///     println!("  Name: {:?}", std::str::from_utf8(&ftyp.name).unwrap());
///     println!("  Total size: {} bytes", ftyp.length);
///     println!("  Data size: {} bytes", ftyp.data_length);
///     println!("  Position: byte {}", ftyp.offset);
///
///     // Read the atom's data
///     let data = ftyp.read_data(&mut reader).unwrap();
///     println!("  First 4 bytes of data: {:?}", &data[..4.min(data.len())]);
/// }
/// ```
///
/// ## Navigating container atoms
///
/// ```no_run
/// use audex::mp4::Atoms;
/// use std::fs::File;
/// use std::io::BufReader;
///
/// let file = File::open("song.m4a").unwrap();
/// let mut reader = BufReader::new(file);
/// let atoms = Atoms::parse(&mut reader).unwrap();
///
/// // Find moov atom and list its children
/// if let Some(moov) = atoms.get("moov") {
///     if let Some(ref children) = moov.children {
///         println!("moov atom contains {} children:", children.len());
///         for child in children {
///             let name = std::str::from_utf8(&child.name).unwrap();
///             println!("  - {}: {} bytes", name, child.data_length);
///         }
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct MP4Atom {
    pub atom_type: AtomType,
    pub name: [u8; 4],
    pub length: u64,
    pub offset: u64,
    pub data_offset: u64,
    pub data_length: u64,
    pub children: Option<Vec<MP4Atom>>,
}

impl AtomType {
    pub fn from_bytes(name: &[u8; 4]) -> Self {
        match name {
            b"ftyp" => AtomType::Ftyp,
            b"moov" => AtomType::Moov,
            b"mvhd" => AtomType::Mvhd,
            b"trak" => AtomType::Trak,
            b"mdat" => AtomType::Mdat,
            b"free" => AtomType::Free,
            b"skip" => AtomType::Skip,
            b"udta" => AtomType::Udta,
            b"meta" => AtomType::Meta,
            b"hdlr" => AtomType::Hdlr,
            b"ilst" => AtomType::Ilst,
            b"tkhd" => AtomType::Tkhd,
            b"mdia" => AtomType::Mdia,
            b"mdhd" => AtomType::Mdhd,
            b"minf" => AtomType::Minf,
            b"stbl" => AtomType::Stbl,
            b"stsd" => AtomType::Stsd,
            b"stts" => AtomType::Stts,
            b"stsc" => AtomType::Stsc,
            b"stsz" => AtomType::Stsz,
            b"stco" => AtomType::Stco,
            b"co64" => AtomType::Co64,
            b"mp4a" => AtomType::Mp4a,
            b"alac" => AtomType::Alac,
            b"ac-3" => AtomType::Ac3,
            b"esds" => AtomType::Esds,
            b"dac3" => AtomType::Dac3,
            b"data" => AtomType::Data,
            b"moof" => AtomType::Moof,
            b"mfhd" => AtomType::Mfhd,
            b"traf" => AtomType::Traf,
            b"tfhd" => AtomType::Tfhd,
            b"trun" => AtomType::Trun,
            _ => AtomType::Unknown(*name),
        }
    }

    pub fn to_bytes(self) -> [u8; 4] {
        match self {
            AtomType::Ftyp => *b"ftyp",
            AtomType::Moov => *b"moov",
            AtomType::Mvhd => *b"mvhd",
            AtomType::Trak => *b"trak",
            AtomType::Mdat => *b"mdat",
            AtomType::Free => *b"free",
            AtomType::Skip => *b"skip",
            AtomType::Udta => *b"udta",
            AtomType::Meta => *b"meta",
            AtomType::Hdlr => *b"hdlr",
            AtomType::Ilst => *b"ilst",
            AtomType::Tkhd => *b"tkhd",
            AtomType::Mdia => *b"mdia",
            AtomType::Mdhd => *b"mdhd",
            AtomType::Minf => *b"minf",
            AtomType::Stbl => *b"stbl",
            AtomType::Stsd => *b"stsd",
            AtomType::Stts => *b"stts",
            AtomType::Stsc => *b"stsc",
            AtomType::Stsz => *b"stsz",
            AtomType::Stco => *b"stco",
            AtomType::Co64 => *b"co64",
            AtomType::Mp4a => *b"mp4a",
            AtomType::Alac => *b"alac",
            AtomType::Ac3 => *b"ac-3",
            AtomType::Esds => *b"esds",
            AtomType::Dac3 => *b"dac3",
            AtomType::Data => *b"data",
            AtomType::Moof => *b"moof",
            AtomType::Mfhd => *b"mfhd",
            AtomType::Traf => *b"traf",
            AtomType::Tfhd => *b"tfhd",
            AtomType::Trun => *b"trun",
            AtomType::Unknown(name) => name,
        }
    }

    pub fn is_container(self) -> bool {
        let name = self.to_bytes();
        CONTAINERS.contains(&(&name))
    }

    pub fn skip_size(self) -> usize {
        let name = self.to_bytes();
        SKIP_SIZE
            .iter()
            .find(|(atom_name, _)| *atom_name == &name)
            .map(|(_, size)| *size)
            .unwrap_or(0)
    }
}

impl MP4Atom {
    /// Create a new atom with the given type and data.
    ///
    /// Returns an error if the data length plus the 8-byte header
    /// would overflow `u64`.
    pub fn new(atom_type: AtomType, data: Vec<u8>) -> Result<Self> {
        let name = atom_type.to_bytes();
        let data_length = data.len() as u64;
        // Use checked arithmetic to prevent silent overflow when data
        // length is extremely large (8 bytes reserved for the atom header)
        let length = data_length.checked_add(8).ok_or_else(|| {
            AudexError::InvalidData(
                "overflow computing atom length: data.len() + 8 exceeds u64::MAX".to_string(),
            )
        })?;

        Ok(Self {
            atom_type,
            name,
            length,
            offset: 0,
            data_offset: 8,
            data_length,
            children: if atom_type.is_container() {
                Some(Vec::new())
            } else {
                None
            },
        })
    }

    /// Parse an atom from a reader at the current position
    /// Implements graceful degradation with failed atom preservation
    pub fn parse<R: Read + Seek>(reader: &mut R, level: usize) -> Result<Self> {
        Self::parse_with_preservation(reader, level, true)
    }

    /// Parse an atom with optional failed atom preservation
    pub fn parse_with_preservation<R: Read + Seek>(
        reader: &mut R,
        level: usize,
        preserve_failed: bool,
    ) -> Result<Self> {
        Self::parse_with_context(reader, level, preserve_failed, false)
    }

    fn parse_with_context<R: Read + Seek>(
        reader: &mut R,
        level: usize,
        preserve_failed: bool,
        in_ilst_children: bool,
    ) -> Result<Self> {
        let offset = reader
            .stream_position()
            .map_err(|e| AudexError::ParseError(format!("Failed to get stream position: {}", e)))?;

        // Read atom header (size + name)
        let mut header = [0u8; 8];
        reader
            .read_exact(&mut header)
            .map_err(|e| AudexError::ParseError(format!("Failed to read atom header: {}", e)))?;

        let mut length = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let name = [header[4], header[5], header[6], header[7]];
        let mut data_offset = offset + 8;

        // Handle 64-bit size
        if length == 1 {
            let mut size_bytes = [0u8; 8];
            reader.read_exact(&mut size_bytes).map_err(|e| {
                AudexError::ParseError(format!("Failed to read 64-bit atom size: {}", e))
            })?;
            length = u64::from_be_bytes(size_bytes);
            data_offset += 8;
            if length < 16 {
                return Err(AudexError::ParseError(
                    "64 bit atom length can only be 16 and higher".to_string(),
                ));
            }
        } else if length == 0 {
            if level != 0 {
                return Err(AudexError::ParseError(
                    "only a top-level atom can have zero length".to_string(),
                ));
            }
            // Zero length means extends to end of file
            let current_pos = reader.stream_position().map_err(|e| {
                AudexError::ParseError(format!("Failed to get current position: {}", e))
            })?;
            let end_pos = reader
                .seek(SeekFrom::End(0))
                .map_err(|e| AudexError::ParseError(format!("Failed to seek to end: {}", e)))?;
            length = end_pos.checked_sub(offset).ok_or_else(|| {
                AudexError::ParseError(
                    "zero-length atom: end of stream is before atom offset".to_string(),
                )
            })?;
            reader
                .seek(SeekFrom::Start(current_pos))
                .map_err(|e| AudexError::ParseError(format!("Failed to seek back: {}", e)))?;
        } else if length < 8 {
            return Err(AudexError::ParseError(
                "atom length can only be 0, 1 or 8 and higher".to_string(),
            ));
        }

        let atom_type = AtomType::from_bytes(&name);
        let header_size = data_offset.checked_sub(offset).ok_or_else(|| {
            AudexError::ParseError("atom data offset is before atom start".to_string())
        })?;
        let data_length = length.checked_sub(header_size).ok_or_else(|| {
            AudexError::ParseError("atom length is smaller than its header size".to_string())
        })?;

        let mut atom = MP4Atom {
            atom_type,
            name,
            length,
            offset,
            data_offset,
            data_length,
            children: None,
        };

        // Parse children if this is a container atom
        if is_container_atom(&name, in_ilst_children) {
            let mut children = Vec::new();
            let skip_bytes = atom_type.skip_size();
            let child_in_ilst = name == *b"ilst";

            if skip_bytes > 0 {
                let mut skip_buffer = vec![0u8; skip_bytes];
                reader
                    .read_exact(&mut skip_buffer)
                    .map_err(|e| AudexError::ParseError(format!("Failed to skip bytes: {}", e)))?;
            }

            let children_end = offset.checked_add(length).ok_or_else(|| {
                AudexError::ParseError("Atom offset + length overflows u64".to_string())
            })?;
            while reader.stream_position().map_err(|e| {
                AudexError::ParseError(format!("Failed to get stream position: {}", e))
            })? < children_end
            {
                let current_pos = reader.stream_position().map_err(|e| {
                    AudexError::ParseError(format!("Failed to get position: {}", e))
                })?;

                if current_pos >= children_end {
                    break;
                }

                // Hard limit: refuse to recurse beyond the safe nesting depth.
                // This check is intentionally placed before the recursive call
                // so the error propagates immediately without being caught by
                // the preserve_failed recovery path below.
                if level + 1 > Self::MAX_ATOM_DEPTH {
                    return Err(AudexError::DepthLimitExceeded {
                        max_depth: Self::MAX_ATOM_DEPTH as u32,
                    });
                }

                // Reject containers with an unreasonable number of children
                // before allocating further.
                if children.len() >= Self::MAX_CHILDREN_PER_ATOM {
                    return Err(AudexError::InvalidData(format!(
                        "Container atom has more than {} children",
                        Self::MAX_CHILDREN_PER_ATOM,
                    )));
                }

                match Self::parse_with_context(reader, level + 1, preserve_failed, child_in_ilst) {
                    Ok(child) => children.push(child),
                    Err(parse_error) => {
                        // Depth limit violations are not recoverable — propagate
                        // immediately so callers at every level re-raise the error
                        // instead of silently preserving the failed atom.
                        if matches!(parse_error, AudexError::DepthLimitExceeded { .. }) {
                            return Err(parse_error);
                        }

                        if preserve_failed {
                            // Try to preserve the failed atom as raw data.
                            // Pass the position recorded before the parse so
                            // the reader is rewound to the atom's true start.
                            if let Ok(preserved) =
                                Self::preserve_failed_atom(reader, &parse_error, current_pos)
                            {
                                // Advance past the full atom length to avoid re-parsing
                                // the same bytes. Without this seek, the reader is only
                                // 8 bytes past where preserve_failed_atom started reading,
                                // which can cause the while loop to stall or repeat.
                                let atom_end = preserved.offset.saturating_add(preserved.length);
                                if reader.seek(SeekFrom::Start(atom_end)).is_err() {
                                    children.push(preserved);
                                    break;
                                }
                                children.push(preserved);
                                continue; // Successfully preserved, try next child
                            }
                        }
                        break; // Can't parse or preserve — stop to avoid infinite loop at EOF
                    }
                }
            }

            atom.children = Some(children);
        } else {
            // Skip to end of atom for non-container atoms
            let end_pos = offset.checked_add(length).ok_or_else(|| {
                AudexError::ParseError("Atom offset + length overflows u64".to_string())
            })?;
            reader.seek(SeekFrom::Start(end_pos)).map_err(|e| {
                AudexError::ParseError(format!("Failed to seek to end of atom: {}", e))
            })?;
        }

        Ok(atom)
    }

    /// Maximum allowed nesting depth for container atoms. Prevents stack overflow
    /// from pathologically deep atom hierarchies in crafted files.
    /// 128 levels is far beyond any legitimate MP4 structure while still being safe.
    const MAX_ATOM_DEPTH: usize = 128;

    /// Maximum number of children allowed per container atom. Prevents
    /// excessive allocation from crafted files that declare millions of
    /// tiny child atoms. 100,000 is far above any legitimate MP4 file
    /// while still capping memory and CPU usage to reasonable levels.
    const MAX_CHILDREN_PER_ATOM: usize = 100_000;

    /// Read the data payload of this atom
    pub fn read_data<R: Read + Seek>(&self, reader: &mut R) -> Result<Vec<u8>> {
        // Enforce the library-wide tag allocation ceiling
        crate::limits::ParseLimits::default().check_tag_size(self.data_length, "MP4 atom")?;

        // Guard against silent truncation on 32-bit platforms where
        // usize is 32 bits but data_length is u64.
        let alloc_size = usize::try_from(self.data_length).map_err(|_| {
            AudexError::ParseError(format!(
                "Atom data length {} exceeds addressable memory",
                self.data_length
            ))
        })?;

        reader
            .seek(SeekFrom::Start(self.data_offset))
            .map_err(|e| AudexError::ParseError(format!("Failed to seek to atom data: {}", e)))?;

        let mut data = vec![0u8; alloc_size];
        reader
            .read_exact(&mut data)
            .map_err(|e| AudexError::ParseError(format!("Failed to read atom data: {}", e)))?;

        Ok(data)
    }

    /// Find all child atoms with the given name recursively
    pub fn findall(&self, name: &[u8; 4], recursive: bool) -> Vec<&MP4Atom> {
        let mut result = Vec::new();

        if let Some(children) = &self.children {
            for child in children {
                if &child.name == name {
                    result.push(child);
                }
                if recursive {
                    result.extend(child.findall(name, true));
                }
            }
        }

        result
    }

    /// Get a child atom by path (e.g., ["udta", "meta"])
    pub fn get_child(&self, path: &[&str]) -> Option<&MP4Atom> {
        if path.is_empty() {
            return Some(self);
        }

        let target_name = path[0].as_bytes();
        if target_name.len() != 4 {
            return None;
        }

        let target = [
            target_name[0],
            target_name[1],
            target_name[2],
            target_name[3],
        ];

        if let Some(children) = &self.children {
            for child in children {
                if child.name == target {
                    return child.get_child(&path[1..]);
                }
            }
        }

        None
    }

    /// Get data length of atom payload
    pub fn datalength(&self) -> u64 {
        self.data_length
    }

    /// Read atom data, returning success flag and data payload
    pub fn read<R: Read + Seek>(&self, reader: &mut R) -> Result<(bool, Vec<u8>)> {
        let data = self.read_data(reader)?;
        let success = data.len() as u64 == self.data_length;
        Ok((success, data))
    }

    /// Iterator version of findall for lazy evaluation
    pub fn findall_iter(
        &self,
        name: &[u8; 4],
        recursive: bool,
    ) -> impl Iterator<Item = &MP4Atom> + '_ {
        FindallIterator::new(self, name, recursive)
    }

    /// Navigate to child atom using path traversal
    pub fn get_by_path(&self, path: &str) -> Option<&MP4Atom> {
        let parts: Vec<&str> = path.split('.').collect();
        self.get_by_path_parts(&parts)
    }

    /// Helper for path traversal with string slice
    fn get_by_path_parts(&self, remaining: &[&str]) -> Option<&MP4Atom> {
        if remaining.is_empty() {
            return Some(self);
        }

        let part_bytes = remaining[0].as_bytes();
        if part_bytes.len() != 4 {
            return None;
        }

        let target = [part_bytes[0], part_bytes[1], part_bytes[2], part_bytes[3]];

        if let Some(children) = &self.children {
            for child in children {
                if child.name == target {
                    return child.get_by_path_parts(&remaining[1..]);
                }
            }
        }

        None
    }

    /// Render atom to bytes.
    ///
    /// Returns an error if the data is too large for a valid atom header
    /// (i.e. the total size including header would overflow u64).
    pub fn render(name: &[u8; 4], data: &[u8]) -> Result<Vec<u8>> {
        let data_len = data.len() as u64;

        // Standard atom: 4-byte size + 4-byte name + data = 8 + data
        let size = data_len.checked_add(8).ok_or_else(|| {
            AudexError::InvalidData("Atom data too large: size overflows u64".to_string())
        })?;

        let mut result = Vec::new();

        if size <= 0xFFFF_FFFF {
            result.extend_from_slice(&(size as u32).to_be_bytes());
            result.extend_from_slice(name);
        } else {
            // Extended-size atom: marker(4) + name(4) + 64-bit size(8) + data
            let ext_size = size.checked_add(8).ok_or_else(|| {
                AudexError::InvalidData(
                    "Atom data too large: extended size overflows u64".to_string(),
                )
            })?;
            result.extend_from_slice(&1u32.to_be_bytes()); // Extended size marker
            result.extend_from_slice(name);
            result.extend_from_slice(&ext_size.to_be_bytes());
        }

        result.extend_from_slice(data);
        Ok(result)
    }

    /// Update atom size and propagate changes to parent atoms.
    /// Derives the header size from the parsed offsets (8 for standard,
    /// 16 for 64-bit extended) rather than checking the length marker,
    /// since parsing resolves the marker to the actual size.
    pub fn update_size(&mut self, new_data_size: u64) -> Result<()> {
        let header_size = self.data_offset.checked_sub(self.offset).ok_or_else(|| {
            AudexError::InvalidData("atom data_offset is smaller than offset".to_string())
        })?;
        self.data_length = new_data_size;
        self.length = header_size.checked_add(new_data_size).ok_or_else(|| {
            AudexError::InvalidData("atom header size + data size overflows u64".to_string())
        })?;
        Ok(())
    }

    /// Calculate the total size of all children atoms
    pub fn calculate_children_size(&self) -> Result<u64> {
        match self.children.as_ref() {
            Some(children) => children.iter().try_fold(0u64, |acc, child| {
                acc.checked_add(child.length).ok_or_else(|| {
                    AudexError::InvalidData("children atom sizes overflow u64".to_string())
                })
            }),
            None => Ok(0),
        }
    }

    /// Update offset tables (stco/co64) when metadata changes
    /// This is critical for maintaining file playability after modifications
    pub fn update_offset_tables<R: Read + Seek, W: Write + Seek>(
        &self,
        reader: &mut R,
        writer: &mut W,
        size_delta: i64,
        moov_offset: u64,
    ) -> Result<()> {
        if size_delta == 0 {
            return Ok(());
        }

        // Find all stco and co64 atoms recursively
        let stco_atoms = self.findall(b"stco", true);
        let co64_atoms = self.findall(b"co64", true);

        // Update each stco atom found
        for stco in stco_atoms {
            Self::update_stco_offsets(stco, reader, writer, size_delta, moov_offset)?;
        }

        // Update each co64 atom found
        for co64 in co64_atoms {
            Self::update_co64_offsets(co64, reader, writer, size_delta, moov_offset)?;
        }

        Ok(())
    }

    /// Update 32-bit chunk offset table (stco)
    /// Critical for maintaining trak -> mdat reference integrity
    fn update_stco_offsets<R: Read + Seek, W: Write + Seek>(
        stco: &MP4Atom,
        reader: &mut R,
        writer: &mut W,
        size_delta: i64,
        moov_offset: u64,
    ) -> Result<()> {
        let data = stco.read_data(reader)?;
        if data.len() < 8 {
            return Err(AudexError::ParseError("stco atom too short".to_string()));
        }

        let entry_count = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let required_size = entry_count
            .checked_mul(4)
            .and_then(|n| n.checked_add(8))
            .ok_or_else(|| AudexError::ParseError("stco entry count overflow".to_string()))?;
        if data.len() < required_size {
            return Err(AudexError::ParseError("stco atom truncated".to_string()));
        }

        let mut new_data = data.clone();

        // Update each offset - only update offsets that point after the moov atom
        for i in 0..entry_count {
            let offset_pos = 8 + i * 4;
            let old_offset = u32::from_be_bytes([
                data[offset_pos],
                data[offset_pos + 1],
                data[offset_pos + 2],
                data[offset_pos + 3],
            ]) as u64;

            // Only update offsets that point to data after the moved metadata
            let new_offset = if old_offset > moov_offset {
                if size_delta >= 0 {
                    old_offset.checked_add(size_delta as u64).ok_or_else(|| {
                        AudexError::ParseError(
                            "stco offset overflow: new offset exceeds u64 range".to_string(),
                        )
                    })?
                } else {
                    // Use unsigned_abs() to safely handle i64::MIN without
                    // overflow on negation
                    let abs_delta = size_delta.unsigned_abs();
                    if old_offset >= abs_delta {
                        old_offset - abs_delta
                    } else {
                        return Err(AudexError::ParseError(
                            "offset would become negative after metadata change".to_string(),
                        ));
                    }
                }
            } else {
                old_offset // Don't modify offsets before the moov atom
            };

            if new_offset > u32::MAX as u64 {
                return Err(AudexError::ParseError(
                    "offset too large for stco, file needs co64 atom conversion".to_string(),
                ));
            }

            let new_offset_bytes = (new_offset as u32).to_be_bytes();
            new_data[offset_pos..offset_pos + 4].copy_from_slice(&new_offset_bytes);
        }

        // Write updated data back
        writer.seek(SeekFrom::Start(stco.data_offset))?;
        writer.write_all(&new_data)?;

        Ok(())
    }

    /// Update 64-bit chunk offset table (co64)
    /// Critical for maintaining trak -> mdat reference integrity
    fn update_co64_offsets<R: Read + Seek, W: Write + Seek>(
        co64: &MP4Atom,
        reader: &mut R,
        writer: &mut W,
        size_delta: i64,
        moov_offset: u64,
    ) -> Result<()> {
        let data = co64.read_data(reader)?;
        if data.len() < 8 {
            return Err(AudexError::ParseError("co64 atom too short".to_string()));
        }

        let entry_count = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let required_size = entry_count
            .checked_mul(8)
            .and_then(|n| n.checked_add(8))
            .ok_or_else(|| AudexError::ParseError("co64 entry count overflow".to_string()))?;
        if data.len() < required_size {
            return Err(AudexError::ParseError("co64 atom truncated".to_string()));
        }

        let mut new_data = data.clone();

        // Update each offset - only update offsets that point after the moov atom
        for i in 0..entry_count {
            let offset_pos = 8 + i * 8;
            let old_offset = u64::from_be_bytes([
                data[offset_pos],
                data[offset_pos + 1],
                data[offset_pos + 2],
                data[offset_pos + 3],
                data[offset_pos + 4],
                data[offset_pos + 5],
                data[offset_pos + 6],
                data[offset_pos + 7],
            ]);

            // Only update offsets that point to data after the moved metadata
            let new_offset = if old_offset > moov_offset {
                if size_delta >= 0 {
                    old_offset.checked_add(size_delta as u64).ok_or_else(|| {
                        AudexError::ParseError(
                            "co64 offset overflow: new offset exceeds u64 range".to_string(),
                        )
                    })?
                } else {
                    // Use unsigned_abs() to safely handle i64::MIN without
                    // overflow on negation
                    let abs_delta = size_delta.unsigned_abs();
                    if old_offset >= abs_delta {
                        old_offset - abs_delta
                    } else {
                        return Err(AudexError::ParseError(
                            "offset would become negative after metadata change".to_string(),
                        ));
                    }
                }
            } else {
                old_offset // Don't modify offsets before the moov atom
            };

            let new_offset_bytes = new_offset.to_be_bytes();
            new_data[offset_pos..offset_pos + 8].copy_from_slice(&new_offset_bytes);
        }

        // Write updated data back
        writer.seek(SeekFrom::Start(co64.data_offset))?;
        writer.write_all(&new_data)?;

        Ok(())
    }

    /// Update movie fragment track header (tfhd) offsets
    /// Enhanced to handle moov atom position changes properly
    pub fn update_tfhd_offsets<R: Read + Seek, W: Write + Seek>(
        &self,
        reader: &mut R,
        writer: &mut W,
        size_delta: i64,
    ) -> Result<()> {
        let tfhd_atoms = self.findall(b"tfhd", true);

        for tfhd in tfhd_atoms {
            Self::update_single_tfhd_offset(tfhd, reader, writer, size_delta)?;
        }

        Ok(())
    }

    /// Update a single tfhd atom's base data offset
    /// Properly handles fragmented MP4 offset management
    fn update_single_tfhd_offset<R: Read + Seek, W: Write + Seek>(
        tfhd: &MP4Atom,
        reader: &mut R,
        writer: &mut W,
        size_delta: i64,
    ) -> Result<()> {
        let data = tfhd.read_data(reader)?;
        if data.len() < 8 {
            return Err(AudexError::ParseError("tfhd atom too short".to_string()));
        }

        let flags = u32::from_be_bytes([0, data[1], data[2], data[3]]);
        let base_data_offset_present = (flags & 0x000001) != 0;

        if !base_data_offset_present {
            return Ok(()); // Nothing to update
        }

        let offset_pos = 8; // Skip version/flags and track_id
        if data.len() < offset_pos + 8 {
            return Err(AudexError::ParseError(
                "tfhd atom too short for base data offset".to_string(),
            ));
        }

        let old_offset = u64::from_be_bytes([
            data[offset_pos],
            data[offset_pos + 1],
            data[offset_pos + 2],
            data[offset_pos + 3],
            data[offset_pos + 4],
            data[offset_pos + 5],
            data[offset_pos + 6],
            data[offset_pos + 7],
        ]);

        let new_offset = if size_delta >= 0 {
            old_offset.checked_add(size_delta as u64).ok_or_else(|| {
                AudexError::ParseError(
                    "tfhd offset overflow: new offset exceeds u64 range".to_string(),
                )
            })?
        } else {
            // Use unsigned_abs() to safely handle i64::MIN without
            // overflow on negation
            let abs_delta = size_delta.unsigned_abs();
            if old_offset >= abs_delta {
                old_offset - abs_delta
            } else {
                return Err(AudexError::ParseError(
                    "tfhd offset would become negative".to_string(),
                ));
            }
        };

        // Create updated data
        let mut new_data = data.clone();
        let new_offset_bytes = new_offset.to_be_bytes();
        new_data[offset_pos..offset_pos + 8].copy_from_slice(&new_offset_bytes);

        // Write updated data back
        writer.seek(SeekFrom::Start(tfhd.data_offset))?;
        writer.write_all(&new_data)?;

        Ok(())
    }

    /// Check if this atom requires offset updates when moved
    pub fn needs_offset_update(&self) -> bool {
        matches!(&self.name, b"stco" | b"co64" | b"tfhd")
    }

    /// Preserve a failed atom as raw data for graceful degradation.
    ///
    /// `pre_parse_offset` is the reader position recorded before the parse
    /// attempt that failed.  We seek back to that position so the header
    /// bytes we read belong to the actual atom, not wherever the failed
    /// parse left the cursor.
    fn preserve_failed_atom<R: Read + Seek>(
        reader: &mut R,
        error: &crate::AudexError,
        pre_parse_offset: u64,
    ) -> Result<Self> {
        // Seek back to where the atom truly starts
        reader.seek(SeekFrom::Start(pre_parse_offset))?;
        let offset = pre_parse_offset;
        let mut header = [0u8; 8];

        match reader.read_exact(&mut header) {
            Ok(()) => {
                let raw_length =
                    u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
                let name = [header[4], header[5], header[6], header[7]];

                // Handle 64-bit extended-size atoms: when the 32-bit length
                // field is 1, the actual size is in the following 8 bytes.
                // We must read those bytes to know how far to skip.
                let (length, data_offset) = if raw_length == 1 {
                    let mut size_bytes = [0u8; 8];
                    match reader.read_exact(&mut size_bytes) {
                        Ok(()) => {
                            let ext_len = u64::from_be_bytes(size_bytes);
                            // Even if the extended size is invalid (< 16),
                            // we consumed 16 bytes of header and need to
                            // report at least that much so the parser skips
                            // past the full header.
                            let actual = if ext_len >= 16 { ext_len } else { 16 };
                            (actual, offset + 16)
                        }
                        // If we can't read the extended size, fall back to
                        // the 8-byte header minimum
                        Err(_) => (8u64, offset + 8),
                    }
                } else {
                    let actual = if raw_length >= 8 { raw_length } else { 8 };
                    (actual, offset + 8)
                };

                Ok(MP4Atom {
                    atom_type: AtomType::Unknown(name),
                    name,
                    length,
                    offset,
                    data_offset,
                    data_length: length.saturating_sub(data_offset - offset),
                    children: None,
                })
            }
            Err(_) => Err(AudexError::ParseError(format!(
                "Cannot preserve failed atom: {}",
                error
            ))),
        }
    }

    /// Check if this atom is a movie fragment atom requiring special handling
    pub fn is_movie_fragment(&self) -> bool {
        matches!(&self.name, b"moof" | b"mfhd" | b"traf" | b"tfhd" | b"trun")
    }

    /// Get all movie fragment atoms from the file structure
    pub fn get_movie_fragments(&self) -> Vec<&MP4Atom> {
        let mut fragments = Vec::new();

        if self.is_movie_fragment() {
            fragments.push(self);
        }

        if let Some(children) = &self.children {
            for child in children {
                fragments.extend(child.get_movie_fragments());
            }
        }

        fragments
    }

    /// Integrate movie fragment offset updates
    pub fn update_fragment_offsets<R: Read + Seek, W: Write + Seek>(
        &self,
        reader: &mut R,
        writer: &mut W,
        size_delta: i64,
        moov_offset: u64,
    ) -> Result<()> {
        if size_delta == 0 {
            return Ok(());
        }

        // Update tfhd (track fragment header) base data offsets
        self.update_tfhd_offsets(reader, writer, size_delta)?;

        // Update trun (track fragment run) data offsets
        let trun_atoms = self.findall(b"trun", true);
        for trun in trun_atoms {
            Self::update_trun_offsets(trun, reader, writer, size_delta, moov_offset)?;
        }

        Ok(())
    }

    /// Update track fragment run (trun) data offsets
    fn update_trun_offsets<R: Read + Seek, W: Write + Seek>(
        trun: &MP4Atom,
        reader: &mut R,
        writer: &mut W,
        size_delta: i64,
        moov_offset: u64,
    ) -> Result<()> {
        let data = trun.read_data(reader)?;
        if data.len() < 8 {
            return Err(AudexError::ParseError("trun atom too short".to_string()));
        }

        let flags = u32::from_be_bytes([0, data[1], data[2], data[3]]);
        let data_offset_present = (flags & 0x000001) != 0;

        if !data_offset_present {
            return Ok(()); // No data offset to update
        }

        let _sample_count = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let offset_pos = 8;

        // Skip sample count, read data offset
        if offset_pos + 4 > data.len() {
            return Err(AudexError::ParseError("trun atom truncated".to_string()));
        }

        let old_offset = i32::from_be_bytes([
            data[offset_pos],
            data[offset_pos + 1],
            data[offset_pos + 2],
            data[offset_pos + 3],
        ]);

        // Only update if the offset points to data after the moov atom.
        // If moov_offset exceeds i64::MAX, every i32 offset is smaller,
        // so no adjustment is needed.  Otherwise cast safely for a
        // sign-preserving comparison.
        let new_offset = if moov_offset > i64::MAX as u64 {
            // All i32 values fit in i64 and are strictly less than moov_offset
            old_offset
        } else if (old_offset as i64) > (moov_offset as i64) {
            let delta_i32 = i32::try_from(size_delta).map_err(|_| {
                AudexError::ParseError(format!(
                    "trun offset delta {} exceeds i32 range",
                    size_delta
                ))
            })?;
            old_offset.checked_add(delta_i32).ok_or_else(|| {
                AudexError::ParseError(format!(
                    "trun offset overflow: {} + {}",
                    old_offset, delta_i32
                ))
            })?
        } else {
            old_offset
        };

        // Create updated data
        let mut new_data = data.clone();
        let new_offset_bytes = new_offset.to_be_bytes();
        new_data[offset_pos..offset_pos + 4].copy_from_slice(&new_offset_bytes);

        // Write updated data back
        writer.seek(SeekFrom::Start(trun.data_offset))?;
        writer.write_all(&new_data)?;

        Ok(())
    }
}

/// Collection of all atoms in an MP4 file with navigation methods.
///
/// This structure holds the complete atom hierarchy for an MP4 file and provides
/// convenient methods for navigating and accessing specific atoms by path.
///
/// # Structure
///
/// - **`atoms`**: Top-level atoms in the file (typically ftyp, moov, mdat)
///
/// # Path Syntax
///
/// Atoms can be accessed using dot-separated paths:
/// - `"moov"` - Top-level moov atom
/// - `"moov.udta"` - udta atom inside moov
/// - `"moov.udta.meta.ilst"` - iTunes metadata atom
/// - `"moov.trak.mdia.minf.stbl.stsd"` - Sample description atom
///
/// # Examples
///
/// ## Parsing and navigating atoms
///
/// ```no_run
/// use audex::mp4::Atoms;
/// use std::fs::File;
/// use std::io::BufReader;
///
/// let file = File::open("song.m4a").unwrap();
/// let mut reader = BufReader::new(file);
///
/// // Parse all atoms from file
/// let atoms = Atoms::parse(&mut reader).unwrap();
///
/// // Check if iTunes metadata exists
/// if atoms.contains("moov.udta.meta.ilst") {
///     println!("File has iTunes metadata");
/// }
///
/// // Get specific atom
/// if let Some(mvhd) = atoms.get("moov.mvhd") {
///     println!("Movie header found at offset {}", mvhd.offset);
/// }
/// ```
///
/// ## Finding audio track information
///
/// ```no_run
/// use audex::mp4::Atoms;
/// use std::fs::File;
/// use std::io::BufReader;
///
/// let file = File::open("song.m4a").unwrap();
/// let mut reader = BufReader::new(file);
/// let atoms = Atoms::parse(&mut reader).unwrap();
///
/// // Get sample description atom for codec info
/// if let Some(stsd) = atoms.get("moov.trak.mdia.minf.stbl.stsd") {
///     let data = stsd.read_data(&mut reader).unwrap();
///     println!("Sample description: {} bytes", data.len());
/// }
/// ```
///
/// ## Getting full atom path
///
/// ```no_run
/// use audex::mp4::Atoms;
/// use std::fs::File;
/// use std::io::BufReader;
///
/// let file = File::open("song.m4a").unwrap();
/// let mut reader = BufReader::new(file);
/// let atoms = Atoms::parse(&mut reader).unwrap();
///
/// // Get all atoms in path to ilst
/// if let Some(path) = atoms.path("moov.udta.meta.ilst") {
///     println!("Path to iTunes metadata:");
///     for atom in path {
///         let name = std::str::from_utf8(&atom.name).unwrap();
///         println!("  {}: {} bytes at offset {}",
///             name, atom.length, atom.offset);
///     }
/// }
/// ```
///
/// ## Listing all top-level atoms
///
/// ```no_run
/// use audex::mp4::Atoms;
/// use std::fs::File;
/// use std::io::BufReader;
///
/// let file = File::open("song.m4a").unwrap();
/// let mut reader = BufReader::new(file);
/// let atoms = Atoms::parse(&mut reader).unwrap();
///
/// println!("Top-level atoms:");
/// for atom in &atoms.atoms {
///     let name = std::str::from_utf8(&atom.name).unwrap();
///     println!("  {}: {} bytes", name, atom.length);
/// }
/// ```
#[derive(Debug)]
pub struct Atoms {
    pub atoms: Vec<MP4Atom>,
}

impl Atoms {
    /// Parse all atoms from a reader
    pub fn parse<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut atoms = Vec::new();

        let end = reader
            .seek(SeekFrom::End(0))
            .map_err(|e| AudexError::ParseError(format!("Failed to seek to end: {}", e)))?;

        reader
            .seek(SeekFrom::Start(0))
            .map_err(|e| AudexError::ParseError(format!("Failed to seek to start: {}", e)))?;

        while reader.stream_position().unwrap_or(0) + 8 <= end {
            match MP4Atom::parse_with_preservation(reader, 0, true) {
                Ok(atom) => atoms.push(atom),
                Err(_) => break, // Stop parsing on error with preservation enabled
            }
        }

        Ok(Atoms { atoms })
    }

    /// Get atom path like "moov.udta.meta.ilst"
    pub fn path(&self, path_str: &str) -> Option<Vec<&MP4Atom>> {
        let parts: Vec<&str> = path_str.split('.').collect();
        let mut result = Vec::new();
        let mut current_atoms = &self.atoms;

        for (i, part) in parts.iter().enumerate() {
            let part_bytes = part.as_bytes();
            if part_bytes.len() != 4 {
                return None;
            }

            let target = [part_bytes[0], part_bytes[1], part_bytes[2], part_bytes[3]];

            let found = current_atoms.iter().find(|atom| atom.name == target)?;
            result.push(found);

            // Only navigate to children if this is not the last part
            if i + 1 < parts.len() {
                current_atoms = found.children.as_ref()?;
            }
        }

        Some(result)
    }

    /// Check if a path exists
    pub fn contains(&self, path_str: &str) -> bool {
        self.path(path_str).is_some()
    }

    /// Get an atom by path
    pub fn get(&self, path_str: &str) -> Option<&MP4Atom> {
        self.path(path_str).and_then(|path| path.last().copied())
    }

    /// Get atom by array of names path
    pub fn get_by_names(&self, names: &[&str]) -> Option<&MP4Atom> {
        let mut current_atoms = &self.atoms;
        let mut found_atom: Option<&MP4Atom> = None;

        for name in names {
            let name_bytes = name.as_bytes();
            if name_bytes.len() != 4 {
                return None;
            }

            let target = [name_bytes[0], name_bytes[1], name_bytes[2], name_bytes[3]];

            found_atom = current_atoms.iter().find(|atom| atom.name == target);
            if let Some(atom) = found_atom {
                if let Some(children) = &atom.children {
                    current_atoms = children;
                } else if name
                    != names
                        .last()
                        .expect("names is non-empty; we are iterating over it")
                {
                    // If this isn't the last name and there are no children, fail
                    return None;
                }
            } else {
                return None;
            }
        }

        found_atom
    }

    /// Get atom by binary path with dot separation
    pub fn get_by_binary_path(&self, path: &[u8]) -> Option<&MP4Atom> {
        // Convert binary path to string and use existing path method
        let path_str = std::str::from_utf8(path).ok()?;
        self.get(path_str)
    }

    /// Get complete path of atoms including intermediate nodes
    /// Returns full path including intermediate atoms, not just final
    pub fn path_names(&self, names: &[&str]) -> Option<Vec<&MP4Atom>> {
        let mut result = Vec::new();
        let mut current_atoms = &self.atoms;

        for name in names {
            let name_bytes = name.as_bytes();
            if name_bytes.len() != 4 {
                return None;
            }

            let target = [name_bytes[0], name_bytes[1], name_bytes[2], name_bytes[3]];

            let found = current_atoms.iter().find(|atom| atom.name == target)?;
            result.push(found);

            if let Some(children) = &found.children {
                current_atoms = children;
            }
        }

        Some(result)
    }
}

/// Iterator for findall method
struct FindallIterator<'a> {
    stack: Vec<(&'a MP4Atom, bool)>, // (atom, has_yielded_self)
    target_name: [u8; 4],
    recursive: bool,
}

impl<'a> FindallIterator<'a> {
    fn new(root: &'a MP4Atom, name: &[u8; 4], recursive: bool) -> Self {
        let mut stack = Vec::new();
        if let Some(children) = &root.children {
            // Add children in reverse order so we process them in the correct order
            for child in children.iter().rev() {
                stack.push((child, false));
            }
        }

        Self {
            stack,
            target_name: *name,
            recursive,
        }
    }
}

impl<'a> Iterator for FindallIterator<'a> {
    type Item = &'a MP4Atom;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((atom, has_yielded)) = self.stack.pop() {
            if !has_yielded && atom.name == self.target_name {
                // If we need to recurse, add this atom back to the stack with has_yielded = true
                // so we can process its children
                if self.recursive {
                    self.stack.push((atom, true));
                }
                return Some(atom);
            }

            // Add children to stack if we haven't processed them yet and we're recursing
            if self.recursive && has_yielded {
                if let Some(children) = &atom.children {
                    for child in children.iter().rev() {
                        self.stack.push((child, false));
                    }
                }
            } else if self.recursive && !has_yielded {
                // Non-matching atom: push children directly without re-pushing
                // the atom itself, since it will never be yielded.
                if let Some(children) = &atom.children {
                    for child in children.iter().rev() {
                        self.stack.push((child, false));
                    }
                }
            }
        }
        None
    }
}

#[cfg(feature = "async")]
mod async_parse {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    impl MP4Atom {
        /// Async version of `parse_with_preservation`.
        /// Uses `Box::pin` for recursive async calls.
        pub fn parse_async(
            reader: &mut tokio::fs::File,
            level: usize,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self>> + Send + '_>>
        {
            Box::pin(Self::parse_with_preservation_async(reader, level, true))
        }

        /// Core async recursive atom parser.
        pub fn parse_with_preservation_async(
            reader: &mut tokio::fs::File,
            level: usize,
            preserve_failed: bool,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self>> + Send + '_>>
        {
            Self::parse_with_preservation_async_context(reader, level, preserve_failed, false)
        }

        fn parse_with_preservation_async_context(
            reader: &mut tokio::fs::File,
            level: usize,
            preserve_failed: bool,
            in_ilst_children: bool,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self>> + Send + '_>>
        {
            Box::pin(async move {
                let offset = reader.stream_position().await.map_err(|e| {
                    AudexError::ParseError(format!("Failed to get stream position: {}", e))
                })?;

                let mut header = [0u8; 8];
                reader.read_exact(&mut header).await.map_err(|e| {
                    AudexError::ParseError(format!("Failed to read atom header: {}", e))
                })?;

                let mut length =
                    u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
                let name = [header[4], header[5], header[6], header[7]];
                let mut data_offset = offset + 8;

                if length == 1 {
                    let mut size_bytes = [0u8; 8];
                    reader.read_exact(&mut size_bytes).await.map_err(|e| {
                        AudexError::ParseError(format!("Failed to read 64-bit atom size: {}", e))
                    })?;
                    length = u64::from_be_bytes(size_bytes);
                    data_offset += 8;
                    if length < 16 {
                        return Err(AudexError::ParseError(
                            "64 bit atom length can only be 16 and higher".to_string(),
                        ));
                    }
                } else if length == 0 {
                    if level != 0 {
                        return Err(AudexError::ParseError(
                            "only a top-level atom can have zero length".to_string(),
                        ));
                    }
                    let current_pos = reader.stream_position().await.map_err(|e| {
                        AudexError::ParseError(format!("Failed to get current position: {}", e))
                    })?;
                    let end_pos = reader.seek(SeekFrom::End(0)).await.map_err(|e| {
                        AudexError::ParseError(format!("Failed to seek to end: {}", e))
                    })?;
                    length = end_pos.checked_sub(offset).ok_or_else(|| {
                        AudexError::ParseError(
                            "zero-length atom: end of stream is before atom offset".to_string(),
                        )
                    })?;
                    reader
                        .seek(SeekFrom::Start(current_pos))
                        .await
                        .map_err(|e| {
                            AudexError::ParseError(format!("Failed to seek back: {}", e))
                        })?;
                } else if length < 8 {
                    return Err(AudexError::ParseError(
                        "atom length can only be 0, 1 or 8 and higher".to_string(),
                    ));
                }

                let atom_type = AtomType::from_bytes(&name);
                let header_size = data_offset.checked_sub(offset).ok_or_else(|| {
                    AudexError::ParseError("atom data offset is before atom start".to_string())
                })?;
                let data_length = length.checked_sub(header_size).ok_or_else(|| {
                    AudexError::ParseError(
                        "atom length is smaller than its header size".to_string(),
                    )
                })?;

                let mut atom = MP4Atom {
                    atom_type,
                    name,
                    length,
                    offset,
                    data_offset,
                    data_length,
                    children: None,
                };

                if is_container_atom(&name, in_ilst_children) {
                    let mut children = Vec::new();
                    let skip_bytes = atom_type.skip_size();
                    let child_in_ilst = name == *b"ilst";

                    if skip_bytes > 0 {
                        let mut skip_buffer = vec![0u8; skip_bytes];
                        reader.read_exact(&mut skip_buffer).await.map_err(|e| {
                            AudexError::ParseError(format!("Failed to skip bytes: {}", e))
                        })?;
                    }

                    let children_end = offset.checked_add(length).ok_or_else(|| {
                        AudexError::ParseError("Atom offset + length overflows u64".to_string())
                    })?;

                    while reader.stream_position().await.unwrap_or(0) < children_end {
                        let current_pos = reader.stream_position().await.map_err(|e| {
                            AudexError::ParseError(format!("Failed to get position: {}", e))
                        })?;

                        if current_pos >= children_end {
                            break;
                        }

                        // Hard limit: refuse to recurse beyond the safe nesting depth.
                        // Placed before the recursive call so the error propagates
                        // immediately without being caught by preserve_failed recovery.
                        if level + 1 > Self::MAX_ATOM_DEPTH {
                            return Err(AudexError::DepthLimitExceeded {
                                max_depth: Self::MAX_ATOM_DEPTH as u32,
                            });
                        }

                        // Reject containers with an unreasonable number of children
                        // before allocating further.
                        if children.len() >= Self::MAX_CHILDREN_PER_ATOM {
                            return Err(AudexError::InvalidData(format!(
                                "Container atom has more than {} children",
                                Self::MAX_CHILDREN_PER_ATOM,
                            )));
                        }

                        match Self::parse_with_preservation_async_context(
                            reader,
                            level + 1,
                            preserve_failed,
                            child_in_ilst,
                        )
                        .await
                        {
                            Ok(child) => children.push(child),
                            Err(parse_error) => {
                                // Depth limit violations are not recoverable — propagate
                                // immediately so callers at every level re-raise the error
                                // instead of silently preserving the failed atom.
                                if matches!(parse_error, AudexError::DepthLimitExceeded { .. }) {
                                    return Err(parse_error);
                                }

                                if preserve_failed {
                                    if let Ok(preserved) = Self::preserve_failed_atom_async(
                                        reader,
                                        &parse_error,
                                        current_pos,
                                    )
                                    .await
                                    {
                                        // Advance past the full atom length to avoid re-parsing
                                        // the same bytes in the while loop
                                        let atom_end =
                                            preserved.offset.saturating_add(preserved.length);
                                        if reader.seek(SeekFrom::Start(atom_end)).await.is_err() {
                                            children.push(preserved);
                                            break;
                                        }
                                        children.push(preserved);
                                        continue;
                                    }
                                }
                                break;
                            }
                        }
                    }

                    atom.children = Some(children);
                } else {
                    let end_pos = offset.checked_add(length).ok_or_else(|| {
                        AudexError::ParseError("Atom offset + length overflows u64".to_string())
                    })?;
                    reader.seek(SeekFrom::Start(end_pos)).await.map_err(|e| {
                        AudexError::ParseError(format!("Failed to seek to end of atom: {}", e))
                    })?;
                }

                Ok(atom)
            })
        }

        /// Async version of `read_data`. Seeks to the atom's data offset and
        /// reads exactly `data_length` bytes using async I/O.
        pub async fn read_data_async(&self, reader: &mut tokio::fs::File) -> Result<Vec<u8>> {
            // Enforce the library-wide tag allocation ceiling
            crate::limits::ParseLimits::default()
                .check_tag_size(self.data_length, "MP4 atom async")?;

            // Guard against silent truncation on 32-bit platforms
            let alloc_size = usize::try_from(self.data_length).map_err(|_| {
                AudexError::ParseError(format!(
                    "Atom data length {} exceeds addressable memory",
                    self.data_length
                ))
            })?;

            reader
                .seek(SeekFrom::Start(self.data_offset))
                .await
                .map_err(|e| {
                    AudexError::ParseError(format!("Failed to seek to atom data: {}", e))
                })?;

            let mut data = vec![0u8; alloc_size];
            reader
                .read_exact(&mut data)
                .await
                .map_err(|e| AudexError::ParseError(format!("Failed to read atom data: {}", e)))?;

            Ok(data)
        }

        /// Async version of `preserve_failed_atom`.
        ///
        /// `pre_parse_offset` is the reader position recorded before the
        /// parse attempt.  We seek back so the header read starts at the
        /// actual atom boundary.
        async fn preserve_failed_atom_async(
            reader: &mut tokio::fs::File,
            error: &crate::AudexError,
            pre_parse_offset: u64,
        ) -> Result<Self> {
            reader.seek(SeekFrom::Start(pre_parse_offset)).await?;
            let offset = pre_parse_offset;
            let mut header = [0u8; 8];

            match reader.read_exact(&mut header).await {
                Ok(_) => {
                    let raw_length =
                        u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
                    let name = [header[4], header[5], header[6], header[7]];

                    // Handle 64-bit extended-size atoms: when the 32-bit length
                    // field is 1, the actual size is in the following 8 bytes.
                    let (length, data_offset) = if raw_length == 1 {
                        let mut size_bytes = [0u8; 8];
                        match reader.read_exact(&mut size_bytes).await {
                            Ok(_) => {
                                let ext_len = u64::from_be_bytes(size_bytes);
                                let actual = if ext_len >= 16 { ext_len } else { 16 };
                                (actual, offset + 16)
                            }
                            Err(_) => (8u64, offset + 8),
                        }
                    } else {
                        let actual = if raw_length >= 8 { raw_length } else { 8 };
                        (actual, offset + 8)
                    };

                    Ok(MP4Atom {
                        atom_type: AtomType::Unknown(name),
                        name,
                        length,
                        offset,
                        data_offset,
                        data_length: length.saturating_sub(data_offset - offset),
                        children: None,
                    })
                }
                Err(_) => Err(AudexError::ParseError(format!(
                    "Cannot preserve failed atom: {}",
                    error
                ))),
            }
        }
    }

    impl Atoms {
        /// Parse all atoms from a tokio file asynchronously.
        pub async fn parse_async(reader: &mut tokio::fs::File) -> Result<Self> {
            let mut atoms = Vec::new();

            let end = reader
                .seek(SeekFrom::End(0))
                .await
                .map_err(|e| AudexError::ParseError(format!("Failed to seek to end: {}", e)))?;

            reader
                .seek(SeekFrom::Start(0))
                .await
                .map_err(|e| AudexError::ParseError(format!("Failed to seek to start: {}", e)))?;

            while reader.stream_position().await.unwrap_or(0) + 8 <= end {
                match MP4Atom::parse_async(reader, 0).await {
                    Ok(atom) => atoms.push(atom),
                    Err(_) => break,
                }
            }

            Ok(Atoms { atoms })
        }
    }
}
