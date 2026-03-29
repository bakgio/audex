//! Ogg container format support
//!
//! This module provides comprehensive support for the Ogg multimedia container format,
//! which is a free, open container format for digital multimedia. Ogg is designed for
//! efficient streaming and manipulation of high-quality digital multimedia.
//!
//! # Container Structure
//!
//! The Ogg container format is based on a page-oriented structure:
//!
//! - **Pages**: The fundamental unit of an Ogg file. Each page contains a header
//!   and one or more packet segments.
//! - **Packets**: Logical data units that may span multiple pages. Packets contain
//!   codec-specific data (audio samples, video frames, metadata, etc.).
//! - **Streams**: Logical bitstreams identified by serial numbers. Multiple streams
//!   can be multiplexed within a single Ogg file.
//!
//! ## Page Structure
//!
//! Each Ogg page consists of:
//! - **Header** (27 bytes): Contains capture pattern "OggS", version, flags,
//!   granule position, serial number, sequence number, and CRC checksum
//! - **Segment Table**: Array of segment sizes (lacing values)
//! - **Packet Data**: The actual payload data divided into segments
//!
//! ## Header Flags
//!
//! - **Continued (0x01)**: First packet continues from previous page
//! - **Beginning of Stream (0x02)**: First page of logical bitstream
//! - **End of Stream (0x04)**: Last page of logical bitstream
//!
//! # Multiplexing
//!
//! Ogg supports multiplexing multiple independent streams within a single file.
//! Each stream has a unique serial number, and pages from different streams can
//! be interleaved. This allows for:
//! - Multiple audio tracks
//! - Combined audio and video (Ogg Theora)
//! - Metadata and subtitle streams
//! - Low-latency streaming with proper interleaving
//!
//! # Supported Codecs
//!
//! Common codecs used within Ogg containers:
//! - **Vorbis**: Lossy audio compression
//! - **Opus**: Low-latency audio codec
//! - **FLAC**: Lossless audio compression
//! - **Speex**: Speech codec
//! - **Theora**: Video codec
//!
//! # Examples
//!
//! ## Loading an Ogg file
//!
//! ```no_run
//! use audex::ogg::OggFile;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Load an Ogg file and parse all streams
//! let ogg = OggFile::load("/path/to/audio.ogg")?;
//!
//! // Access streams
//! println!("Found {} streams", ogg.streams.len());
//! for (serial, stream) in &ogg.streams {
//!     println!("Stream {}: codec = {}", serial, stream.codec);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Parsing individual pages
//!
//! ```rust
//! use audex::ogg::OggPage;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Build a valid Ogg page with proper CRC, then parse it back
//! let mut page = OggPage::new();
//! page.serial = 42;
//! page.sequence = 1;
//! let ogg_data = page.write()?;
//!
//! let parsed = OggPage::from_bytes(&ogg_data)?;
//! println!("Page version: {}", parsed.version);
//! println!("Serial: {}", parsed.serial);
//! assert_eq!(parsed.serial, 42);
//! # Ok(())
//! # }
//! ```
//!
//! ## Working with multi-stream files
//!
//! ```no_run
//! use audex::ogg::OggFile;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let ogg = OggFile::load("/path/to/multistream.ogg")?;
//!
//! // Get pages for a specific stream
//! if let Some(stream) = ogg.streams.get(&1234) {
//!     let pages = ogg.get_pages_for_stream(1234);
//!     println!("Stream 1234 has {} pages", pages.len());
//!
//!     // Extract packets from pages
//!     let packets = ogg.get_packets(1234)?;
//!     println!("Stream 1234 has {} packets", packets.len());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Converting packets to pages
//!
//! ```rust
//! use audex::ogg::OggPage;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create some example packets
//! let packets = vec![
//!     vec![1, 2, 3, 4, 5],
//!     vec![6, 7, 8, 9, 10],
//! ];
//!
//! // Convert to Ogg pages (sequence starts at 0, max page size 4096, wiggle room 2048)
//! let pages = OggPage::from_packets(packets, 0, 4096, 2048);
//! println!("Created {} pages", pages.len());
//! # Ok(())
//! # }
//! ```
//!
//! # References
//!
//! - [RFC 3533 - The Ogg Encapsulation Format Version 0](https://www.rfc-editor.org/rfc/rfc3533.html)
//! - [Xiph.Org Ogg Documentation](https://xiph.org/ogg/)

use crate::limits::ParseLimits;
use crate::util::{insert_bytes, resize_bytes};
use crate::{AudexError, Result, StreamInfo};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::util::{insert_bytes_async, resize_bytes_async};
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

/// Maximum number of Ogg pages to scan before bailing out.
/// Prevents excessive CPU usage on malformed files with many
/// tiny pages (e.g., zero-segment pages that are only 27 bytes each).
/// A typical 2-hour stereo Vorbis file at 128kbps has ~150,000 pages,
/// so 500,000 provides generous headroom for legitimate files.
const MAX_OGG_PAGES: u32 = 500_000;

/// Ogg CRC-32 lookup table for polynomial 0x04C11DB7 (unreflected).
/// Generated at compile time — each entry is CRC(i) for a single byte i.
const OGG_CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0u32;
    while i < 256 {
        let mut crc = i << 24;
        let mut j = 0;
        while j < 8 {
            if crc & 0x80000000 != 0 {
                crc = (crc << 1) ^ 0x04C11DB7;
            } else {
                crc <<= 1;
            }
            j += 1;
        }
        table[i as usize] = crc;
        i += 1;
    }
    table
};

/// Compute the Ogg CRC-32 checksum over a byte slice.
///
/// Uses the Ogg-specific polynomial 0x04C11DB7, MSB-first with no
/// input/output reflection, initial value 0, and no final XOR.
fn ogg_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &byte in data {
        let index = ((crc >> 24) as u8) ^ byte;
        crc = (crc << 8) ^ OGG_CRC_TABLE[index as usize];
    }
    crc
}

/// File manipulation utilities
mod file_utils {
    use crate::Result;
    use std::io::{Read, Seek, SeekFrom};

    /// Seek backwards from end, ensuring we don't go before start
    pub fn seek_end<F: Read + Seek>(fileobj: &mut F, offset: u64) -> Result<()> {
        fileobj.seek(SeekFrom::End(0))?;
        let filesize = fileobj.stream_position()?;
        let seek_pos = filesize.saturating_sub(offset);
        fileobj.seek(SeekFrom::Start(seek_pos))?;
        Ok(())
    }
}

/// Ogg page header and data
///
/// Represents a single Ogg page, which is the fundamental unit of the Ogg container format.
/// Each page contains header information and packet data for a specific logical stream.
///
/// # Structure
///
/// An Ogg page consists of:
/// - A 27-byte header containing metadata
/// - A segment table that describes packet boundaries
/// - One or more packet fragments (segments)
///
/// # Page Header Fields
///
/// - **version**: Stream structure version (always 0 for current Ogg specification)
/// - **header_type**: Bitfield containing flags (continued, BOS, EOS)
/// - **position**: Granule position (codec-specific time/position marker, -1 for incomplete)
/// - **serial**: Unique serial number identifying the logical stream
/// - **sequence**: Page sequence number within the stream
/// - **checksum**: CRC32 checksum of the entire page
/// - **segments**: Segment table (lacing values) describing packet fragment sizes
/// - **packets**: Actual packet data fragments
/// - **offset**: Optional file offset where this page was found
/// - **complete**: Whether all packets on this page are complete (not spanning to next page)
///
/// # Examples
///
/// ## Creating an empty page
///
/// ```rust
/// use audex::ogg::OggPage;
///
/// let page = OggPage::new();
/// assert_eq!(page.version, 0);
/// assert_eq!(page.serial, 0);
/// ```
///
/// ## Parsing a page from data
///
/// ```rust
/// use audex::ogg::OggPage;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Build a valid page, serialize it, then parse it back
/// let mut page = OggPage::new();
/// page.serial = 1;
/// page.sequence = 5;
/// let data = page.write()?;
///
/// let parsed = OggPage::from_bytes(&data)?;
/// assert_eq!(parsed.serial, 1);
/// assert_eq!(parsed.sequence, 5);
/// # Ok(())
/// # }
/// ```
///
/// ## Checking page flags
///
/// ```rust
/// use audex::ogg::OggPage;
///
/// let mut page = OggPage::new();
/// page.set_first(true);
/// page.set_last(true);
///
/// assert!(page.is_first());
/// assert!(page.is_last());
/// assert!(page.first()); // Alternative method name
/// assert!(page.last());  // Alternative method name
/// ```
#[derive(Debug, Clone)]
pub struct OggPage {
    /// Stream structure version (always 0 for current specification)
    pub version: u8,

    /// Header type flags bitfield
    /// - Bit 0 (0x01): Continued packet flag
    /// - Bit 1 (0x02): Beginning of stream (BOS) flag
    /// - Bit 2 (0x04): End of stream (EOS) flag
    pub header_type: u8,

    /// Granule position - codec-specific time/position marker
    /// Set to -1 for pages with incomplete packets
    pub position: i64,

    /// Bitstream serial number - unique identifier for this logical stream
    pub serial: u32,

    /// Page sequence number - increments for each page in the stream.
    /// The Ogg specification treats this as an unsigned 32-bit integer.
    pub sequence: u32,

    /// CRC32 checksum calculated over the entire page
    pub checksum: u32,

    /// Segment table (lacing values) - array of segment sizes
    /// Each value indicates the size of a segment (0-255 bytes)
    pub segments: Vec<u8>,

    /// Packet data - actual payload divided into packets
    /// Packets may span multiple pages
    pub packets: Vec<Vec<u8>>,

    /// File offset where this page was found (if parsed from a file)
    pub offset: Option<i64>,

    /// Whether all packets on this page are complete
    /// False if the last packet continues on the next page
    pub complete: bool,
}

impl OggPage {
    // Property-style methods for flag access

    /// Check if this is the first page of a stream
    pub fn first(&self) -> bool {
        (self.header_type & 0x02) != 0
    }

    /// Check if this is the last page of a stream  
    pub fn last(&self) -> bool {
        (self.header_type & 0x04) != 0
    }

    /// Check if first packet continues from previous page
    pub fn continued(&self) -> bool {
        (self.header_type & 0x01) != 0
    }
    /// Create new empty Ogg page
    pub fn new() -> Self {
        Self {
            version: 0,
            header_type: 0,
            position: 0,
            serial: 0,
            sequence: 0,
            checksum: 0,
            segments: Vec::new(),
            packets: Vec::new(),
            offset: None,
            complete: true,
        }
    }

    /// Constructor that accepts optional reader
    ///
    /// # Arguments
    ///
    /// * `fileobj` - Optional reader to parse page from. If None, creates empty page
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::ogg::OggPage;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Create empty page
    /// let empty_page = OggPage::new_from_fileobj(None::<&mut std::io::Cursor<Vec<u8>>>)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```rust
    /// use audex::ogg::OggPage;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Build a valid page, serialize it, then parse via reader
    /// let ogg_data = OggPage::new().write()?;
    /// let mut reader = std::io::Cursor::new(ogg_data);
    /// let page = OggPage::new_from_fileobj(Some(&mut reader))?;
    /// assert_eq!(page.version, 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_from_fileobj<R: Read + Seek>(fileobj: Option<&mut R>) -> Result<Self> {
        match fileobj {
            Some(reader) => Self::from_reader(reader),
            None => Ok(Self::new()),
        }
    }

    /// Creates an OggPage by immediately reading and parsing from the provided reader
    ///
    /// # Arguments
    ///
    /// * `fileobj` - Reader to parse page from
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::ogg::OggPage;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Build a valid page, serialize it, then parse via from_fileobj
    /// let ogg_data = OggPage::new().write()?;
    /// let mut reader = std::io::Cursor::new(ogg_data);
    /// let page = OggPage::from_fileobj(&mut reader)?;
    /// assert_eq!(page.version, 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_fileobj<R: Read + Seek>(fileobj: &mut R) -> Result<Self> {
        Self::from_reader(fileobj)
    }

    /// Convenience constructor for file paths
    /// Opens the file and reads the first Ogg page
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the Ogg file
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use audex::ogg::OggPage;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let page = OggPage::from_file("/path/to/file.ogg")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = File::open(path.as_ref()).map_err(|e| {
            AudexError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to open file '{}': {}", path.as_ref().display(), e),
            ))
        })?;
        Self::from_reader(&mut file)
    }

    /// Constructor for byte slice data
    /// Creates an OggPage from byte data
    ///
    /// # Arguments
    ///
    /// * `data` - Byte slice containing Ogg page data
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::ogg::OggPage;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Build a valid page with proper CRC, then parse from bytes
    /// let ogg_page_data = OggPage::new().write()?;
    /// let page = OggPage::from_bytes(&ogg_page_data)?;
    /// assert_eq!(page.version, 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        Self::from_reader(&mut cursor)
    }

    /// Constructor from `Vec<u8>` (consumes the vector)
    /// Creates an OggPage from owned byte data
    ///
    /// # Arguments
    ///
    /// * `data` - Vector containing Ogg page data
    ///
    /// # Examples
    ///
    /// ```rust
    /// use audex::ogg::OggPage;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // Build a valid page with proper CRC, then parse from owned bytes
    /// let ogg_page_data = OggPage::new().write()?;
    /// let page = OggPage::from_vec(ogg_page_data)?;
    /// assert_eq!(page.version, 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_vec(data: Vec<u8>) -> Result<Self> {
        let mut cursor = Cursor::new(data);
        Self::from_reader(&mut cursor)
    }

    /// Parse Ogg page from reader
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let offset = reader.stream_position().ok().map(|o| o as i64);

        // Read header (27 bytes)
        let mut header = [0u8; 27];
        match reader.read_exact(&mut header) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(AudexError::from(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "End of file",
                )));
            }
            Err(e) => return Err(AudexError::from(e)),
        }

        // Check OggS signature
        if &header[0..4] != b"OggS" {
            error_event!("invalid OGG page signature");
            return Err(AudexError::InvalidData(format!(
                "Invalid OGG signature: {:?}",
                &header[0..4]
            )));
        }

        let version = header[4];
        if version != 0 {
            warn_event!(version = version, "unsupported OGG version");
            return Err(AudexError::UnsupportedFormat(format!(
                "Unsupported OGG version: {}",
                version
            )));
        }

        let header_type = header[5];
        let position_u64 = u64::from_le_bytes([
            header[6], header[7], header[8], header[9], header[10], header[11], header[12],
            header[13],
        ]);
        // Interpret as signed i64 via two's complement — the Ogg spec
        // treats granule position as signed, where -1 (all bits set)
        // means "no position". The `as i64` cast is a bit reinterpretation
        // and handles all values correctly, including u64::MAX → -1.
        let position = position_u64 as i64;

        let serial = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);

        let sequence = u32::from_le_bytes([header[18], header[19], header[20], header[21]]);

        let checksum = u32::from_le_bytes([header[22], header[23], header[24], header[25]]);
        let segment_count = header[26];

        // Read segment table
        let mut segments = vec![0u8; segment_count as usize];
        reader.read_exact(&mut segments)?;

        // Calculate packet boundaries and read data
        let mut packets = Vec::new();
        let mut current_packet = Vec::new();

        for &segment_size in &segments {
            let mut segment_data = vec![0u8; segment_size as usize];
            reader.read_exact(&mut segment_data)?;
            current_packet.extend_from_slice(&segment_data);

            // If segment size < 255, packet is complete
            if segment_size < 255 {
                packets.push(current_packet);
                current_packet = Vec::new();
            }
        }

        // If we have remaining data in current_packet, it's an incomplete packet
        let complete = if !current_packet.is_empty() {
            packets.push(current_packet);
            false // Last packet is incomplete
        } else {
            // Check if last segment is < 255 (complete packet)
            segments.last().is_none_or(|&s| s < 255)
        };

        let page = Self {
            version,
            header_type,
            position,
            serial,
            sequence,
            checksum,
            segments,
            packets,
            offset,
            complete,
        };

        // Verify the CRC32 checksum against the actual page contents.
        // The spec requires readers to validate this to detect corruption.
        //
        // The CRC check happens after packet assembly because the Ogg CRC
        // covers the entire serialized page (header + segment table + payload).
        // We must read the payload to compute the checksum, so there is no way
        // to reject a bad CRC before the data is in memory. However, per-page
        // payload size is bounded by the segment table (max 255 segments of
        // 255 bytes each = ~64 KB), so the allocation cost before rejection is
        // inherently limited regardless of total file size.
        let computed_crc = page.compute_read_crc()?;
        if computed_crc != checksum {
            return Err(AudexError::InvalidData(format!(
                "OGG page CRC32 mismatch: stored={:#010X}, computed={:#010X}",
                checksum, computed_crc
            )));
        }

        Ok(page)
    }

    /// Reconstruct the raw page bytes using the stored segment table
    /// and compute CRC32 with the checksum field zeroed. This gives
    /// the same CRC that the original writer computed, allowing us
    /// to detect any corruption that occurred after the page was written.
    ///
    /// NOTE: This uses the segment table and packet data as stored in the struct.
    /// It is only valid on freshly-parsed pages. After modifying packets, the
    /// segment table may be stale, producing an incorrect CRC. Use write_to()
    /// for serialization — it recomputes the CRC from current data.
    fn compute_read_crc(&self) -> Result<u32> {
        let mut raw = Vec::new();

        // Header (27 bytes)
        raw.extend_from_slice(b"OggS");
        raw.push(self.version);
        raw.push(self.header_type);
        let position_u64 = self.position as u64;
        raw.extend_from_slice(&position_u64.to_le_bytes());
        raw.extend_from_slice(&self.serial.to_le_bytes());
        raw.extend_from_slice(&self.sequence.to_le_bytes());
        // Checksum field must be zeroed for CRC computation
        raw.extend_from_slice(&[0u8; 4]);
        // The Ogg spec limits each page to 255 segments. Reject pages that
        // violate this, since truncating the count would produce a wrong CRC.
        if self.segments.len() > 255 {
            return Err(AudexError::InvalidData(format!(
                "Ogg page has {} segments, exceeding the 255 segment limit",
                self.segments.len()
            )));
        }
        raw.push(self.segments.len() as u8);

        // Segment table — use the original lacing values, not rebuilt ones
        raw.extend_from_slice(&self.segments);

        // Packet payload data
        for packet in &self.packets {
            raw.extend_from_slice(packet);
        }

        Ok(self.calculate_ogg_crc32(&raw))
    }

    /// Write Ogg page to writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<Vec<u8>> {
        let mut data = Vec::new();

        // Write header
        data.extend_from_slice(b"OggS");
        data.push(self.version);
        data.push(self.header_type);

        // The granule position is stored as a raw 64-bit field. The i64 value
        // is reinterpreted as u64 for serialization (-1 becomes 0xFFFFFFFFFFFFFFFF,
        // which is the standard "no position" sentinel in the Ogg spec).
        data.extend_from_slice(&(self.position as u64).to_le_bytes());

        data.extend_from_slice(&self.serial.to_le_bytes());
        data.extend_from_slice(&self.sequence.to_le_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]); // Placeholder for checksum

        // Build segment table from packets
        let mut lacing_data = Vec::new();
        for packet in &self.packets {
            let packet_len = packet.len();
            let (full_segments, remainder) = (packet_len / 255, packet_len % 255);

            // Add full segments (255 bytes each)
            lacing_data.extend(std::iter::repeat_n(255u8, full_segments));

            // Add remainder segment
            lacing_data.push(remainder as u8);
        }

        // Handle incomplete pages - strip trailing zero segment
        if !self.complete && lacing_data.last() == Some(&0) {
            lacing_data.pop();
        }

        // The Ogg spec limits each page to 255 segments. If the lacing table
        // exceeds this, the page is too large and must be split by the caller.
        if lacing_data.len() > 255 {
            return Err(AudexError::InvalidData(format!(
                "Ogg page has {} lacing segments, exceeding the 255 segment limit",
                lacing_data.len()
            )));
        }

        data.push(lacing_data.len() as u8);
        data.extend_from_slice(&lacing_data);

        // Write packet data
        for packet in &self.packets {
            data.extend_from_slice(packet);
        }

        // Calculate CRC32 using bit-swapped algorithm to match Ogg specification
        let crc = self.calculate_ogg_crc32(&data);

        // Update checksum in data
        data[22..26].copy_from_slice(&crc.to_le_bytes());

        writer.write_all(&data)?;
        Ok(data)
    }

    /// Calculate Ogg-specific CRC-32 using the standard Ogg polynomial.
    ///
    /// The Ogg format uses polynomial 0x04C11DB7 (MSB-first, no reflection,
    /// init=0, final XOR=0). This is computed directly via a 256-entry
    /// lookup table rather than adapting a different CRC-32 variant.
    fn calculate_ogg_crc32(&self, data: &[u8]) -> u32 {
        ogg_crc32(data)
    }

    /// Write page as bytes without writing to a writer
    pub fn write(&self) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        self.write_to(&mut buffer)?;
        Ok(buffer)
    }

    /// Calculate CRC32 checksum for this page.
    /// Returns an error if the page cannot be serialized (e.g., too many segments).
    pub fn calculate_crc(&self) -> Result<u32> {
        let mut data = Vec::new();
        self.write_to(&mut data)?;

        // Clear checksum field for calculation
        if data.len() >= 26 {
            data[22..26].copy_from_slice(&[0, 0, 0, 0]);
        }

        Ok(self.calculate_ogg_crc32(&data))
    }

    /// Get total page size in bytes
    pub fn size(&self) -> usize {
        let mut size = 27; // Header size

        // Add segment table size
        let mut segment_count = 0;
        for packet in &self.packets {
            let packet_len = packet.len();
            let full_segments = packet_len / 255;
            segment_count += full_segments + 1; // +1 for remainder segment
        }

        // If last packet ends at segment boundary and page is incomplete,
        // we don't need the final zero byte (it gets removed by write_to)
        if !self.packets.is_empty() && !self.complete {
            // Safe: we just verified packets is non-empty above.
            let last_packet_len = self
                .packets
                .last()
                .expect("packets confirmed non-empty")
                .len();
            // Strip the trailing zero segment to match write_to(). This
            // covers both segment-boundary packets and empty packets.
            if last_packet_len % 255 == 0 && segment_count > 0 {
                segment_count -= 1;
            }
        }

        size += segment_count; // Segment table
        size += self.packets.iter().map(|p| p.len()).sum::<usize>(); // Packet data
        size
    }

    /// Check if page starts with "OggS"
    pub fn validate_sync(data: &[u8]) -> bool {
        data.len() >= 4 && &data[0..4] == b"OggS"
    }

    /// Check if this is the first page of a stream
    pub fn is_first(&self) -> bool {
        (self.header_type & 0x02) != 0
    }

    /// Check if this is the last page of a stream
    pub fn is_last(&self) -> bool {
        (self.header_type & 0x04) != 0
    }

    /// Check if first packet continues from previous page
    pub fn is_continued(&self) -> bool {
        (self.header_type & 0x01) != 0
    }

    /// Set first page flag
    pub fn set_first(&mut self, first: bool) {
        if first {
            self.header_type |= 0x02;
        } else {
            self.header_type &= !0x02;
        }
    }

    /// Set last page flag
    pub fn set_last(&mut self, last: bool) {
        if last {
            self.header_type |= 0x04;
        } else {
            self.header_type &= !0x04;
        }
    }

    /// Set continued packet flag
    pub fn set_continued(&mut self, continued: bool) {
        if continued {
            self.header_type |= 0x01;
        } else {
            self.header_type &= !0x01;
        }
    }

    /// Check if this page completes all packets
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Set complete flag
    pub fn set_complete(&mut self, complete: bool) {
        self.complete = complete;
    }

    /// Find the last page with given serial number
    pub fn find_last<R: Read + Seek>(
        reader: &mut R,
        serial: u32,
        finishing: bool,
    ) -> Result<Option<Self>> {
        Self::find_last_with_finishing(reader, serial, finishing)
    }

    #[doc(hidden)]
    pub fn accumulate_page_bytes_with_limit(
        limits: ParseLimits,
        cumulative_bytes: &mut u64,
        page: &OggPage,
        context: &str,
    ) -> Result<()> {
        let page_bytes: u64 = page.packets.iter().map(|pkt| pkt.len() as u64).sum();
        *cumulative_bytes = cumulative_bytes.saturating_add(page_bytes);
        limits.check_tag_size(*cumulative_bytes, context)
    }

    /// Find the last page with given serial number (legacy version for backward compatibility)
    pub fn find_last_u32<R: Read + Seek>(reader: &mut R, serial: u32) -> Result<Option<Self>> {
        Self::find_last(reader, serial, false)
    }

    /// Find the last page with the given serial number, optionally requiring
    /// a valid finishing position.
    ///
    /// The method first attempts a **fast path**: it reads the trailing 64 KiB
    /// of the file and searches backwards for the last `OggS` signature. If
    /// the target serial is found there with an EOS flag, it returns
    /// immediately.
    ///
    /// When the fast path fails (e.g. multiplexed streams, or missing EOS),
    /// a **slow path** scans from the beginning of the file page-by-page.
    /// This is bounded by `MAX_OGG_PAGES` and the cumulative byte budget
    /// from `ParseLimits`, but it can still be significantly slower and will
    /// hold the most-recently-matched page in memory until the scan completes.
    pub fn find_last_with_finishing<R: Read + Seek>(
        reader: &mut R,
        serial: u32,
        finishing: bool,
    ) -> Result<Option<Self>> {
        // For non-multiplexed streams, check the last page first (fast path)
        file_utils::seek_end(reader, 256 * 256)?;

        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer)?;

        // Find last OggS signature
        if let Some(index) = buffer.windows(4).rposition(|w| w == b"OggS") {
            let mut cursor = Cursor::new(&buffer[index..]);
            if let Ok(page) = Self::from_reader(&mut cursor) {
                if page.serial == serial {
                    let is_valid = !finishing || page.position != -1;
                    if is_valid && page.last() {
                        return Ok(Some(page));
                    }
                    // Continue searching for EOS page, but keep this as backup
                }
            }
        }

        // Stream is multiplexed or we need to find EOS page - use slow method.
        // Track cumulative packet data to prevent memory exhaustion on crafted
        // files with many large pages (matching the budget in OggFile::load).
        // Use a reduced page cap for this fallback scan to limit CPU time;
        // the fast path above handles well-formed files, so the slow path
        // only needs enough headroom for multiplexed or truncated streams.
        const MAX_SLOW_PATH_PAGES: u32 = 50_000;
        reader.seek(SeekFrom::Start(0))?;
        let mut best_page = None;
        let mut pages_scanned = 0u32;
        let mut cumulative_bytes: u64 = 0;
        let limits = ParseLimits::default();

        loop {
            // Cap the number of pages to prevent excessive scanning on
            // malformed files with many tiny (e.g., zero-segment) pages
            if pages_scanned >= MAX_SLOW_PATH_PAGES {
                break;
            }

            match Self::from_reader(reader) {
                Ok(page) => {
                    pages_scanned += 1;

                    Self::accumulate_page_bytes_with_limit(
                        limits,
                        &mut cumulative_bytes,
                        &page,
                        "OGG cumulative page data",
                    )?;

                    if page.serial == serial {
                        let is_valid = !finishing || page.position != -1;
                        if is_valid {
                            best_page = Some(page.clone());
                        }
                        if page.last() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    if let AudexError::Io(io_err) = &e {
                        if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                            break;
                        }
                    }
                    return Ok(best_page);
                }
            }
        }

        Ok(best_page)
    }

    /// Convert pages to packets for a specific serial
    pub fn to_packets(pages: &[OggPage], strict: bool) -> Result<Vec<Vec<u8>>> {
        Self::to_packets_strict(pages, strict)
    }

    /// Convert pages to packets with strict validation
    /// If strict is true, first page must start new packet and last page must end packet
    pub fn to_packets_strict(pages: &[OggPage], strict: bool) -> Result<Vec<Vec<u8>>> {
        // Safety cap: reject reconstructed packet data exceeding 256 MB total.
        // This prevents runaway memory usage from malformed or adversarial streams.
        const MAX_TOTAL_PACKET_BYTES: usize = 256 * 1024 * 1024;

        if pages.is_empty() {
            return Ok(Vec::new());
        }

        let serial = pages[0].serial;
        let mut packets: Vec<Vec<u8>> = Vec::new();
        let mut total_bytes: usize = 0;

        // Strict mode validation
        if strict {
            if pages[0].continued() {
                return Err(AudexError::InvalidData(
                    "first packet is continued".to_string(),
                ));
            }
            // Safe: early return above guarantees pages is non-empty.
            if !pages
                .last()
                .expect("pages confirmed non-empty")
                .is_complete()
            {
                return Err(AudexError::InvalidData(
                    "last packet does not complete".to_string(),
                ));
            }
        } else if !pages.is_empty() && pages[0].continued() {
            // Non-strict mode with continued first packet - start with empty packet
            packets.push(vec![]);
        }

        // Sequences must be consecutive
        // Start with the first page's sequence number, not necessarily 0
        let mut expected_sequence = pages[0].sequence;

        for page in pages.iter() {
            if page.serial != serial {
                return Err(AudexError::InvalidData(format!(
                    "invalid serial number in page: expected {}, got {}",
                    serial, page.serial
                )));
            }

            // Check sequence numbers - must be consecutive
            if page.sequence != expected_sequence {
                warn_event!(
                    expected = expected_sequence,
                    actual = page.sequence,
                    "OGG page sequence number mismatch"
                );
                return Err(AudexError::InvalidData(format!(
                    "bad sequence number in page: expected {}, got {}",
                    expected_sequence, page.sequence
                )));
            }

            // Increment for next page (wrapping to handle max sequence)
            expected_sequence = expected_sequence.wrapping_add(1);

            if !page.packets.is_empty() {
                if page.continued() {
                    // Continue the last packet
                    if let Some(last_packet) = packets.last_mut() {
                        total_bytes += page.packets[0].len();
                        if total_bytes > MAX_TOTAL_PACKET_BYTES {
                            return Err(AudexError::InvalidData(
                                "cumulative packet data exceeds 256 MB limit".to_string(),
                            ));
                        }
                        last_packet.extend_from_slice(&page.packets[0]);
                    } else {
                        // Should not happen in valid streams
                        return Err(AudexError::InvalidData(
                            "Continued packet with no previous packet".to_string(),
                        ));
                    }

                    // Add remaining complete packets
                    for packet in &page.packets[1..] {
                        total_bytes += packet.len();
                        if total_bytes > MAX_TOTAL_PACKET_BYTES {
                            return Err(AudexError::InvalidData(
                                "cumulative packet data exceeds 256 MB limit".to_string(),
                            ));
                        }
                        packets.push(packet.clone());
                    }
                } else {
                    // All packets on this page are complete
                    for packet in &page.packets {
                        total_bytes += packet.len();
                        if total_bytes > MAX_TOTAL_PACKET_BYTES {
                            return Err(AudexError::InvalidData(
                                "cumulative packet data exceeds 256 MB limit".to_string(),
                            ));
                        }
                        packets.push(packet.clone());
                    }
                }
            }
        }

        Ok(packets)
    }

    /// Create pages from packet data with default parameters
    pub fn from_packets_simple(packets: Vec<Vec<u8>>) -> Vec<OggPage> {
        Self::from_packets(packets, 0, 4096, 2048)
    }

    /// Create pages from packet data with custom sequence
    pub fn from_packets_sequence(packets: Vec<Vec<u8>>, sequence: u32) -> Vec<OggPage> {
        Self::from_packets(packets, sequence, 4096, 2048)
    }

    /// Internal method to extract packets without sequence validation
    /// Used by from_packets_try_preserve for size comparison
    fn to_packets_no_sequence_validation(pages: &[OggPage]) -> Result<Vec<Vec<u8>>> {
        // Same cumulative byte budget as to_packets_strict to prevent
        // memory exhaustion from adversarial or malformed page sequences.
        const MAX_TOTAL_PACKET_BYTES: usize = 256 * 1024 * 1024;

        if pages.is_empty() {
            return Ok(Vec::new());
        }

        let serial = pages[0].serial;
        let mut packets: Vec<Vec<u8>> = Vec::new();
        let mut total_bytes: usize = 0;

        // No sequence validation - just reconstruct packets
        for page in pages.iter() {
            if page.serial != serial {
                return Err(AudexError::InvalidData(format!(
                    "invalid serial number in page: expected {}, got {}",
                    serial, page.serial
                )));
            }

            if !page.packets.is_empty() {
                if page.continued() {
                    // Continue the last packet
                    if let Some(last_packet) = packets.last_mut() {
                        total_bytes += page.packets[0].len();
                        if total_bytes > MAX_TOTAL_PACKET_BYTES {
                            return Err(AudexError::InvalidData(
                                "cumulative packet data exceeds 256 MB limit".to_string(),
                            ));
                        }
                        last_packet.extend_from_slice(&page.packets[0]);
                    } else {
                        // Continued flag set but no preceding packet to append to.
                        // This indicates a truncated or out-of-order stream. The
                        // first segment is dropped because there is no context to
                        // reconstruct the full packet.
                        warn_event!(
                            serial = serial,
                            sequence = page.sequence,
                            dropped_bytes = page.packets[0].len(),
                            "OGG continued packet has no predecessor; dropping first segment"
                        );
                    }
                    // Add remaining packets from this page
                    for packet in &page.packets[1..] {
                        total_bytes += packet.len();
                        if total_bytes > MAX_TOTAL_PACKET_BYTES {
                            return Err(AudexError::InvalidData(
                                "cumulative packet data exceeds 256 MB limit".to_string(),
                            ));
                        }
                        packets.push(packet.clone());
                    }
                } else {
                    // Add all packets from this page
                    for packet in &page.packets {
                        total_bytes += packet.len();
                        if total_bytes > MAX_TOTAL_PACKET_BYTES {
                            return Err(AudexError::InvalidData(
                                "cumulative packet data exceeds 256 MB limit".to_string(),
                            ));
                        }
                        packets.push(packet.clone());
                    }
                }
            }
        }

        Ok(packets)
    }

    /// Try to preserve original page layout when packet sizes match
    /// Falls back to regular from_packets if sizes don't match
    /// EXACTLY matches the _from_packets_try_preserve behavior
    pub fn from_packets_try_preserve(packets: Vec<Vec<u8>>, old_pages: &[OggPage]) -> Vec<OggPage> {
        if old_pages.is_empty() {
            return Vec::new();
        }

        // Extract old packets and compare sizes - use non-validating version
        // This allows comparison even when sequences don't start at 0
        let old_packets = match Self::to_packets_no_sequence_validation(old_pages) {
            Ok(packets) => packets,
            Err(_) => return Self::from_packets(packets, old_pages[0].sequence, 4096, 2048),
        };

        // Check if packet sizes match
        let new_sizes: Vec<usize> = packets.iter().map(|p| p.len()).collect();
        let old_sizes: Vec<usize> = old_packets.iter().map(|p| p.len()).collect();

        if new_sizes != old_sizes {
            // Sizes don't match, fall back to regular from_packets
            return Self::from_packets(packets, old_pages[0].sequence, 4096, 2048);
        }

        // Sizes match - preserve page layout exactly.
        // Keep a clone for fallback in case the drain loop hits an inconsistency.
        let packets_backup = packets.clone();
        let mut new_data: Vec<u8> = packets.into_iter().flatten().collect();
        let mut new_pages = Vec::new();
        let sequence = old_pages[0].sequence;

        for old_page in old_pages {
            let mut new_page = OggPage::new();
            new_page.sequence = old_page.sequence;
            new_page.serial = old_page.serial;
            new_page.complete = old_page.complete;
            new_page.set_continued(old_page.continued());
            new_page.position = old_page.position;

            for old_packet in &old_page.packets {
                let packet_len = old_packet.len();
                if new_data.len() >= packet_len {
                    let packet_data = new_data.drain(..packet_len).collect::<Vec<u8>>();
                    new_page.packets.push(packet_data);
                } else {
                    // Page layout inconsistency — fall back to standard page generation
                    return Self::from_packets(packets_backup, sequence, 4096, 2048);
                }
            }

            new_pages.push(new_page);
        }

        // All data should be consumed after distributing across pages
        if !new_data.is_empty() {
            // Leftover data means the page layout didn't account for all bytes.
            // Fall back to standard page generation to avoid data loss.
            return Self::from_packets(packets_backup, sequence, 4096, 2048);
        }

        new_pages
    }

    /// Create pages from packet data
    ///
    /// Returns an empty vector if an internal error occurs during page
    /// construction. In practice this cannot happen because the granule
    /// position is fixed at 0 (always fits in i64) and the internal page
    /// list is always non-empty after a push, but we avoid panicking to
    /// keep the API robust.
    pub fn from_packets(
        packets: Vec<Vec<u8>>,
        sequence: u32,
        default_size: usize,
        wiggle_room: usize,
    ) -> Vec<OggPage> {
        Self::from_packets_with_options(packets, sequence, default_size, wiggle_room, 0)
            .unwrap_or_default()
    }

    /// Create pages from packet data with position setting
    /// EXACT port of the from_packets algorithm
    pub fn from_packets_with_options(
        packets: Vec<Vec<u8>>,
        sequence: u32,
        default_size: usize,
        wiggle_room: usize,
        granule_position: u64,
    ) -> Result<Vec<OggPage>> {
        // Cap chunk size so a single chunk never exceeds 255 lacing segments.
        // A chunk of N bytes needs (N / 255) + 1 segments, so the maximum
        // single-chunk payload for 255 segments is 254 * 255 = 64770 bytes.
        const MAX_CHUNK: usize = 254 * 255;
        let chunk_size = ((default_size / 255) * 255).min(MAX_CHUNK);
        let mut pages = Vec::new();
        let mut page = OggPage::new();
        page.sequence = sequence;

        // Track lacing segments per page — Ogg limits each page to 255.
        // Each packet contributes (byte_len / 255) + 1 segments.
        let mut page_segment_count: usize = 0;

        for packet in packets {
            page.packets.push(Vec::new());
            let mut remaining_packet = packet;

            while !remaining_packet.is_empty() {
                let data_len = chunk_size.min(remaining_packet.len());
                let data = remaining_packet.drain(..data_len).collect::<Vec<u8>>();
                let packet_len = remaining_packet.len();

                // Compute how many lacing segments the page would need
                // if we added this data to the current last packet.
                let last_pkt_len = page.packets.last().map_or(0, |p| p.len());
                let new_pkt_len = last_pkt_len + data.len();
                let segments_before = (last_pkt_len / 255) + if last_pkt_len > 0 { 1 } else { 0 };
                let segments_after = (new_pkt_len / 255) + 1;
                let extra_segments = segments_after.saturating_sub(segments_before);

                if page.size() < default_size && page_segment_count + extra_segments <= 255 {
                    // Add data to the last packet (the one we just created for this iteration)
                    if let Some(last_packet) = page.packets.last_mut() {
                        last_packet.extend(data);
                    }
                    page_segment_count += extra_segments;
                } else {
                    // logic for page overflow
                    if let Some(last_packet) = page.packets.last() {
                        if !last_packet.is_empty() {
                            // If we've put any packet data into this page yet,
                            // we need to mark it incomplete.
                            page.complete = false;
                            if page.packets.len() == 1 {
                                // Set position to -1 for incomplete page
                                page.position = -1;
                            }
                        } else {
                            // However, we can also have just started this packet on an already
                            // full page, in which case, just start the new page with this packet.
                            page.packets.pop();
                        }
                    }

                    pages.push(page);
                    page = OggPage::new();
                    // Safe: pages is non-empty after the push above
                    let prev = pages.last().ok_or_else(|| {
                        AudexError::InternalError("page list empty after push".to_string())
                    })?;
                    page.set_continued(!prev.complete);
                    page.sequence = prev.sequence.checked_add(1).ok_or_else(|| {
                        AudexError::InvalidData(
                            "Ogg page sequence counter overflow while building pages".to_string(),
                        )
                    })?;
                    // Reset segment counter for the new page
                    let new_pkt_segments = (data.len() / 255) + 1;
                    page_segment_count = new_pkt_segments;
                    page.packets.push(data);
                }

                // wiggle room logic — absorb the remainder into the
                // current packet if it's small enough and won't exceed
                // the 255 segment limit per Ogg page
                if packet_len < wiggle_room {
                    if let Some(last_packet) = page.packets.last_mut() {
                        let before = (last_packet.len() / 255) + 1;
                        let projected_len = last_packet.len() + remaining_packet.len();
                        let after = (projected_len / 255) + 1;
                        let extra = after.saturating_sub(before);

                        // Only absorb if the total segment count stays within
                        // the Ogg page maximum of 255 segments
                        if page_segment_count + extra <= 255 {
                            last_packet.extend_from_slice(&remaining_packet);
                            page_segment_count += extra;
                            remaining_packet.clear();
                        }
                    }
                }
            }
        }

        if !page.packets.is_empty() {
            pages.push(page);
        }

        // Validate that the granule position fits in i64 to prevent
        // silent truncation for extremely long streams.
        let signed_position = i64::try_from(granule_position).map_err(|_| {
            AudexError::InvalidData(format!(
                "Granule position {} exceeds i64::MAX and cannot be represented",
                granule_position
            ))
        })?;

        // Set the final granule position on the last page
        if let Some(last_page) = pages.last_mut() {
            last_page.position = signed_position;
        }

        Ok(pages)
    }

    /// Renumber pages for a given serial starting at sequence number
    pub fn renumber<R: Read + Write + Seek>(
        reader: &mut R,
        serial: u32,
        start_sequence: u32,
    ) -> Result<()> {
        let mut sequence = start_sequence;
        let mut pages_scanned: u32 = 0;

        loop {
            // Enforce the same page limit as OggFile::load() to prevent
            // unbounded iteration from crafted files with many tiny pages
            if pages_scanned >= MAX_OGG_PAGES {
                return Err(AudexError::ParseError(format!(
                    "Ogg renumber exceeded maximum page count ({})",
                    MAX_OGG_PAGES
                )));
            }

            let page_offset = reader.stream_position()?;

            match OggPage::from_reader(reader) {
                Ok(mut page) => {
                    if page.serial == serial {
                        // Update sequence number
                        page.sequence = sequence;

                        // Changing the sequence number cannot change the page size,
                        // so we can safely seek back and overwrite
                        reader.seek(SeekFrom::Start(page_offset))?;

                        // Write updated page with new sequence and recalculated CRC
                        let page_data = page.write()?;
                        reader.write_all(&page_data)?;

                        // Seek to end of this page to continue
                        reader.seek(SeekFrom::Start(page_offset + page_data.len() as u64))?;

                        sequence = sequence.checked_add(1).ok_or_else(|| {
                            AudexError::InvalidData(
                                "Ogg page sequence counter overflow".to_string(),
                            )
                        })?;
                    }
                    pages_scanned += 1;
                }
                Err(e) => {
                    if let AudexError::Io(io_err) = &e {
                        if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                            break; // Normal end of file
                        }
                    }
                    // For any other error, we might have hit invalid data
                    // Return the error to indicate the issue
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Replace old pages with new pages in a file
    pub fn replace<R: Read + Write + Seek + 'static>(
        reader: &mut R,
        old_pages: &[OggPage],
        new_pages: Vec<OggPage>,
    ) -> Result<()> {
        if old_pages.is_empty() || new_pages.is_empty() {
            return Err(AudexError::InvalidData(
                "empty pages list not allowed".to_string(),
            ));
        }

        let mut updated_pages = new_pages;

        // Number the new pages starting from the first old page
        let first_sequence = old_pages[0].sequence;
        for (i, page) in updated_pages.iter_mut().enumerate() {
            page.sequence = first_sequence.checked_add(i as u32).ok_or_else(|| {
                AudexError::ParseError("Ogg page sequence number overflow".to_string())
            })?;
            page.serial = old_pages[0].serial;
        }

        // Copy flags from old pages
        updated_pages[0].set_first(old_pages[0].first());
        updated_pages[0].set_last(old_pages[0].last());
        updated_pages[0].set_continued(old_pages[0].continued());

        // Only copy BOS to the last page when it IS the first page
        // (i.e. single-page replacement).  When comments expand across
        // multiple pages, BOS must never appear on a non-initial page.
        let old_last = old_pages
            .last()
            .ok_or_else(|| AudexError::InternalError("old_pages empty in replace".to_string()))?;
        let old_last_first = old_last.first();
        let old_last_last = old_last.last();
        let old_last_complete = old_last.is_complete();

        if updated_pages.len() == 1 {
            updated_pages
                .last_mut()
                .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
                .set_first(old_last_first);
        } else {
            updated_pages
                .last_mut()
                .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
                .set_first(false);
        }
        updated_pages
            .last_mut()
            .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
            .set_last(old_last_last);
        updated_pages
            .last_mut()
            .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
            .set_complete(old_last_complete);

        // Handle incomplete single-packet pages
        let last_page = updated_pages
            .last()
            .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?;
        if !last_page.is_complete() && last_page.packets.len() == 1 {
            updated_pages
                .last_mut()
                .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
                .position = -1;
        }

        // Write new page data
        let new_data: Result<Vec<Vec<u8>>> = updated_pages.iter().map(|p| p.write()).collect();
        let new_data = new_data?;

        // Add dummy data or merge remaining data for different page counts
        let mut final_data = new_data;
        let pages_diff = old_pages.len() as i64 - final_data.len() as i64;

        if pages_diff > 0 {
            // More old pages than new - add empty data
            for _ in 0..pages_diff {
                final_data.push(Vec::new());
            }
        } else if pages_diff < 0 {
            // When new pages exceed old pages, merge excess page data into the last
            // old-page slot so the file replacement sees exactly old_pages.len() entries.
            let split_idx = old_pages.len().saturating_sub(1);
            if split_idx < final_data.len() {
                let merged: Vec<u8> = final_data.drain(split_idx..).flatten().collect();
                final_data.push(merged);
            }
        }

        // Replace pages - handle case where we have more new pages than old pages
        #[allow(unused_assignments)]
        let mut offset_adjust: i64 = 0;
        let mut new_data_end: Option<u64> = None;
        let min_pages = old_pages.len().min(final_data.len());

        // First, replace existing old pages with corresponding new data
        for i in 0..min_pages {
            let old_page = &old_pages[i];
            let data = &final_data[i];

            if let Some(offset) = old_page.offset {
                let adjusted = offset.checked_add(offset_adjust).ok_or_else(|| {
                    AudexError::InvalidData("Page offset arithmetic overflow".to_string())
                })?;
                if adjusted < 0 {
                    return Err(AudexError::InvalidData(format!(
                        "Adjusted page offset is negative: {}",
                        adjusted
                    )));
                }
                let adjusted_offset = adjusted as u64;
                let data_size = data.len() as u64;
                let old_size = old_page.size() as u64;

                // Resize the area in the file
                resize_bytes(reader, old_size, data_size, adjusted_offset)?;

                // Write the new data
                reader.seek(SeekFrom::Start(adjusted_offset))?;
                reader.write_all(data)?;

                new_data_end = Some(adjusted_offset + data_size);
                // Use checked conversion to avoid wrapping on extremely large sizes
                let data_i64 = i64::try_from(data_size).map_err(|_| {
                    AudexError::InvalidData("Page data size exceeds i64 range".to_string())
                })?;
                let old_i64 = i64::try_from(old_size).map_err(|_| {
                    AudexError::InvalidData("Old page size exceeds i64 range".to_string())
                })?;
                // Accumulate offset using checked arithmetic to prevent overflow
                let delta = data_i64.checked_sub(old_i64).ok_or_else(|| {
                    AudexError::InvalidData(
                        "page size delta overflow during offset calculation".to_string(),
                    )
                })?;
                offset_adjust = offset_adjust.checked_add(delta).ok_or_else(|| {
                    AudexError::InvalidData(
                        "cumulative page offset adjustment overflow".to_string(),
                    )
                })?;
            }
        }

        // Now handle additional new pages (if any)
        if final_data.len() > old_pages.len() {
            if let Some(insert_offset) = new_data_end {
                for data in &final_data[min_pages..] {
                    let data_size = data.len() as u64;

                    if !data.is_empty() {
                        // Insert space for this page
                        insert_bytes(reader, data_size, insert_offset, None)?;

                        // Write the data
                        reader.seek(SeekFrom::Start(insert_offset))?;
                        reader.write_all(data)?;

                        new_data_end = Some(insert_offset + data_size);
                    }
                }
            }
        }

        // Renumber remaining pages if page count changed
        if old_pages.len() != updated_pages.len() {
            if let Some(end_offset) = new_data_end {
                reader.seek(SeekFrom::Start(end_offset))?;
                let last_updated = updated_pages.last().ok_or_else(|| {
                    AudexError::InternalError("updated_pages empty during renumber".to_string())
                })?;
                let serial = last_updated.serial;
                let next_sequence = last_updated.sequence.checked_add(1).ok_or_else(|| {
                    AudexError::InvalidData(
                        "Ogg page sequence counter overflow before renumber".to_string(),
                    )
                })?;
                Self::renumber(reader, serial, next_sequence)?;
            }
        }

        Ok(())
    }
}

/// Base Ogg file implementation
///
/// Represents a complete Ogg file with all its pages and logical streams.
/// This type provides high-level access to the structure of an Ogg container,
/// allowing you to work with multiple multiplexed streams.
///
/// # Structure
///
/// - **pages**: All Ogg pages in the file, in order
/// - **streams**: Logical streams indexed by serial number, with packets extracted
///
/// # Examples
///
/// ```no_run
/// use audex::ogg::OggFile;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Load an Ogg file
/// let ogg = OggFile::load("/path/to/audio.ogg")?;
///
/// // Iterate through streams
/// for (serial, stream) in &ogg.streams {
///     println!("Stream {}: {} codec, {} packets",
///              serial, stream.codec, stream.packets.len());
/// }
///
/// // Find Vorbis stream
/// if let Some(vorbis) = ogg.get_stream_by_codec("vorbis") {
///     println!("Found Vorbis stream with {} packets", vorbis.packets.len());
/// }
///
/// // Get pages for a specific stream
/// let stream_pages = ogg.get_pages_for_stream(12345);
/// println!("Found {} pages for stream 12345", stream_pages.len());
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct OggFile {
    /// All pages in the file, in the order they appear
    pub pages: Vec<OggPage>,

    /// Logical streams indexed by serial number
    /// Each stream contains extracted packets and codec information
    pub streams: HashMap<u32, OggStream>,
}

impl OggFile {
    /// Create a new empty OggFile with no pages or streams.
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            streams: HashMap::new(),
        }
    }

    /// Load Ogg file from path
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let mut ogg_file = Self::new();
        let mut pages_scanned = 0u32;
        let limits = ParseLimits::default();
        let mut cumulative_bytes: u64 = 0;

        loop {
            // Cap the number of pages to prevent excessive scanning on
            // malformed files with many tiny (e.g., zero-segment) pages
            if pages_scanned >= MAX_OGG_PAGES {
                break;
            }

            match OggPage::from_reader(&mut reader) {
                Ok(page) => {
                    pages_scanned += 1;

                    // Track cumulative allocation across all pages to prevent
                    // a crafted file from exhausting memory. Each page's segment
                    // and packet data counts toward the global tag-size budget.
                    let page_bytes: u64 = page.packets.iter().map(|pkt| pkt.len() as u64).sum();
                    cumulative_bytes = cumulative_bytes.saturating_add(page_bytes);

                    if cumulative_bytes > limits.max_tag_size {
                        return Err(AudexError::InvalidData(format!(
                            "OGG cumulative page data ({} bytes) exceeds configured limit ({} bytes)",
                            cumulative_bytes, limits.max_tag_size
                        )));
                    }

                    let serial = page.serial;

                    // Add to streams
                    let stream = ogg_file
                        .streams
                        .entry(serial)
                        .or_insert_with(|| OggStream::new(serial));

                    // Copy packets into the stream for sequential access (codec
                    // detection, identification_packet(), comment_packet(), etc.).
                    // Pages also retain their own packet data for round-trip
                    // serialization fidelity, so both copies are required.
                    for packet in &page.packets {
                        stream.packets.push(packet.clone());
                    }

                    ogg_file.pages.push(page);
                }
                Err(e) => {
                    if let AudexError::Io(io_err) = &e {
                        if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                            break; // Normal end of file
                        }
                    }
                    return Err(e);
                }
            }
        }

        // Detect codecs for all streams
        for stream in ogg_file.streams.values_mut() {
            stream.detect_codec();
        }

        Ok(ogg_file)
    }

    /// Get first stream with specific codec
    pub fn get_stream_by_codec(&self, codec: &str) -> Option<&OggStream> {
        self.streams.values().find(|stream| stream.codec == codec)
    }

    /// Get pages for a specific stream
    pub fn get_pages_for_stream(&self, serial: u32) -> Vec<&OggPage> {
        self.pages
            .iter()
            .filter(|page| page.serial == serial)
            .collect()
    }

    /// Reconstruct packets from pages for a specific stream
    pub fn get_packets(&self, serial: u32) -> Result<Vec<Vec<u8>>> {
        let pages: Vec<OggPage> = self
            .pages
            .iter()
            .filter(|page| page.serial == serial)
            .cloned()
            .collect();

        OggPage::to_packets(&pages, false)
    }
}

/// Ogg logical stream
///
/// Represents a single logical bitstream within an Ogg container file.
/// Each stream is identified by a unique serial number and contains packets
/// for a specific codec (Vorbis, Opus, FLAC, Theora, etc.).
///
/// # Structure
///
/// - **serial_number**: Unique identifier for this stream
/// - **codec**: Detected codec name (e.g., "vorbis", "opus", "flac", "theora")
/// - **packets**: All packets belonging to this stream
///
/// The codec is automatically detected by examining the first packet's header.
///
/// # Packet Types
///
/// Most Ogg codecs use a three-packet header structure:
/// 1. **Identification packet**: Codec signature and basic parameters
/// 2. **Comment packet**: Metadata and tags (Vorbis Comments)
/// 3. **Setup packet**: Codec-specific configuration (if needed)
///
/// # Examples
///
/// ```no_run
/// use audex::ogg::OggFile;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let ogg = OggFile::load("/path/to/audio.ogg")?;
///
/// for (serial, stream) in &ogg.streams {
///     println!("Stream {}:", serial);
///     println!("  Codec: {}", stream.codec);
///     println!("  Packets: {}", stream.packets.len());
///
///     // Access identification packet
///     if let Some(id_packet) = stream.identification_packet() {
///         println!("  ID packet size: {} bytes", id_packet.len());
///     }
///
///     // Access comment packet (metadata)
///     if let Some(comment_packet) = stream.comment_packet() {
///         println!("  Comment packet size: {} bytes", comment_packet.len());
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct OggStream {
    /// Unique serial number for this logical stream
    pub serial_number: u32,

    /// Codec name detected from the identification packet
    /// Common values: "vorbis", "opus", "flac", "theora", "speex"
    pub codec: String,

    /// All packets belonging to this stream, extracted from pages
    pub packets: Vec<Vec<u8>>,
}

impl OggStream {
    /// Create new stream
    pub fn new(serial: u32) -> Self {
        Self {
            serial_number: serial,
            codec: String::new(),
            packets: Vec::new(),
        }
    }

    /// Detect codec from first packet
    pub fn detect_codec(&mut self) {
        if let Some(first_packet) = self.packets.first() {
            if first_packet.len() >= 8 && first_packet.starts_with(b"\x01vorbis") {
                self.codec = "vorbis".to_string();
            } else if first_packet.len() >= 8 && first_packet.starts_with(b"OpusHead") {
                self.codec = "opus".to_string();
            } else if first_packet.len() >= 8 && first_packet.starts_with(b"\x80theora") {
                self.codec = "theora".to_string();
            } else if first_packet.len() >= 5 && first_packet.starts_with(b"\x7FFLAC") {
                self.codec = "flac".to_string();
            }
        }
    }

    /// Get first packet (usually contains codec identification)
    pub fn identification_packet(&self) -> Option<&Vec<u8>> {
        self.packets.first()
    }

    /// Get comment packet (usually second packet)
    pub fn comment_packet(&self) -> Option<&Vec<u8>> {
        self.packets.get(1)
    }

    /// Get setup packet (usually third packet for some codecs)
    pub fn setup_packet(&self) -> Option<&Vec<u8>> {
        self.packets.get(2)
    }
}

/// Base Ogg stream information
///
/// Contains audio stream information extracted from an Ogg logical bitstream.
/// This struct implements the `StreamInfo` trait, providing a standardized
/// interface for accessing audio properties.
///
/// # Fields
///
/// - **length**: Total duration of the audio stream
/// - **bitrate**: Average bitrate in bits per second (may be VBR)
/// - **sample_rate**: Audio sample rate in Hz (e.g., 44100, 48000)
/// - **channels**: Number of audio channels (1 = mono, 2 = stereo, etc.)
/// - **serial**: Serial number of the logical stream
///
/// # Examples
///
/// ```no_run
/// use audex::oggvorbis::OggVorbis;
/// use audex::FileType;
/// use audex::StreamInfo;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let vorbis = OggVorbis::load("/path/to/audio.ogg")?;
///
/// // Access stream information (info is Option<OggVorbisInfo>)
/// if let Some(ref info) = vorbis.info {
///     if let Some(duration) = info.length() {
///         println!("Duration: {:?}", duration);
///     }
///
///     if let Some(bitrate) = info.bitrate() {
///         println!("Bitrate: {} kbps", bitrate / 1000);
///     }
///
///     if let Some(sample_rate) = info.sample_rate() {
///         println!("Sample rate: {} Hz", sample_rate);
///     }
///
///     if let Some(channels) = info.channels() {
///         println!("Channels: {}", channels);
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Default)]
pub struct OggStreamInfo {
    /// Duration of the audio stream
    pub length: Option<Duration>,

    /// Average bitrate in bits per second
    pub bitrate: Option<u32>,

    /// Sample rate in Hz
    pub sample_rate: u32,

    /// Number of audio channels
    pub channels: u16,

    /// Logical stream serial number
    pub serial: u32,
}

impl StreamInfo for OggStreamInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        self.bitrate
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
        None // Varies by codec
    }
}

impl Default for OggFile {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for OggPage {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for OggPage {
    /// Two Ogg pages are equal if they write the same data
    fn eq(&self, other: &Self) -> bool {
        match (self.write(), other.write()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
}

/// Seek backwards from end position in an async reader
///
/// Seeks to an offset from the end of the file, ensuring we don't seek
/// before the start of the file.
#[cfg(feature = "async")]
pub async fn seek_end_async<F: AsyncRead + AsyncSeek + Unpin>(
    fileobj: &mut F,
    offset: u64,
) -> Result<()> {
    fileobj.seek(SeekFrom::End(0)).await?;
    let filesize = fileobj.stream_position().await?;
    let seek_pos = filesize.saturating_sub(offset);
    fileobj.seek(SeekFrom::Start(seek_pos)).await?;
    Ok(())
}

#[cfg(feature = "async")]
impl OggPage {
    /// Parse Ogg page from async reader
    ///
    /// Reads and parses an Ogg page from the current position in an async reader.
    /// This is the async equivalent of `OggPage::from_reader`.
    ///
    /// # Arguments
    ///
    /// * `reader` - Async reader positioned at the start of an Ogg page
    ///
    /// # Returns
    ///
    /// The parsed `OggPage` or an error if the data is invalid
    pub async fn from_reader_async<R: AsyncRead + AsyncSeek + Unpin>(
        reader: &mut R,
    ) -> Result<Self> {
        let offset = reader.stream_position().await.ok().map(|o| o as i64);

        // Read header (27 bytes) - standard Ogg page header size
        let mut header = [0u8; 27];
        match reader.read_exact(&mut header).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(AudexError::from(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "End of file",
                )));
            }
            Err(e) => return Err(AudexError::from(e)),
        }

        // Validate OggS magic signature
        if &header[0..4] != b"OggS" {
            return Err(AudexError::InvalidData(format!(
                "Invalid OGG signature: {:?}",
                &header[0..4]
            )));
        }

        let version = header[4];
        if version != 0 {
            return Err(AudexError::UnsupportedFormat(format!(
                "Unsupported OGG version: {}",
                version
            )));
        }

        // Parse header fields
        let header_type = header[5];
        let position_u64 = u64::from_le_bytes([
            header[6], header[7], header[8], header[9], header[10], header[11], header[12],
            header[13],
        ]);
        // Interpret as signed i64 via two's complement — the Ogg spec
        // treats granule position as signed, where -1 (all bits set)
        // means "no position". The `as i64` cast is a bit reinterpretation
        // and handles all values correctly, including u64::MAX → -1.
        let position = position_u64 as i64;

        let serial = u32::from_le_bytes([header[14], header[15], header[16], header[17]]);

        let sequence = u32::from_le_bytes([header[18], header[19], header[20], header[21]]);

        let checksum = u32::from_le_bytes([header[22], header[23], header[24], header[25]]);
        let segment_count = header[26];

        // Read segment table
        let mut segments = vec![0u8; segment_count as usize];
        reader.read_exact(&mut segments).await?;

        // Calculate packet boundaries and read data
        let mut packets = Vec::new();
        let mut current_packet = Vec::new();

        for &segment_size in &segments {
            let mut segment_data = vec![0u8; segment_size as usize];
            reader.read_exact(&mut segment_data).await?;
            current_packet.extend_from_slice(&segment_data);

            // Segment size < 255 indicates packet boundary
            if segment_size < 255 {
                packets.push(current_packet);
                current_packet = Vec::new();
            }
        }

        // Handle incomplete packets
        let complete = if !current_packet.is_empty() {
            packets.push(current_packet);
            false // Last packet is incomplete
        } else {
            segments.last().is_none_or(|&s| s < 255)
        };

        let page = Self {
            version,
            header_type,
            position,
            serial,
            sequence,
            checksum,
            segments,
            packets,
            offset,
            complete,
        };

        // Verify CRC32 checksum to detect corruption (same as sync path)
        let computed_crc = page.compute_read_crc()?;
        if computed_crc != checksum {
            return Err(AudexError::InvalidData(format!(
                "OGG page CRC32 mismatch: stored={:#010X}, computed={:#010X}",
                checksum, computed_crc
            )));
        }

        Ok(page)
    }

    /// Write Ogg page to async writer
    ///
    /// Serializes this page and writes it to the provided async writer.
    /// This is the async equivalent of `OggPage::write_to`.
    ///
    /// # Arguments
    ///
    /// * `writer` - Async writer to write the page data to
    ///
    /// # Returns
    ///
    /// The serialized page data as a byte vector
    pub async fn write_to_async<W: AsyncWrite + Unpin>(&self, writer: &mut W) -> Result<Vec<u8>> {
        // Use the sync write method which calculates CRC properly
        let data = self.write()?;
        writer.write_all(&data).await?;
        Ok(data)
    }

    /// Find the last page with given serial number (async version)
    ///
    /// Searches the file for the last Ogg page belonging to the specified stream.
    /// This is the async equivalent of `OggPage::find_last`.
    ///
    /// # Arguments
    ///
    /// * `reader` - Async reader to search
    /// * `serial` - Stream serial number to match
    /// * `finishing` - If true, only return pages with valid granule positions
    ///
    /// # Returns
    ///
    /// The last matching page, or None if not found
    pub async fn find_last_async<R: AsyncRead + AsyncSeek + Unpin>(
        reader: &mut R,
        serial: u32,
        finishing: bool,
    ) -> Result<Option<Self>> {
        // Fast path: check the last page first for non-multiplexed streams
        seek_end_async(reader, 256 * 256).await?;

        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer).await?;

        // Find last OggS signature in buffer
        if let Some(index) = buffer.windows(4).rposition(|w| w == b"OggS") {
            let mut cursor = std::io::Cursor::new(&buffer[index..]);
            if let Ok(page) = Self::from_reader(&mut cursor) {
                if page.serial == serial {
                    let is_valid = !finishing || page.position != -1;
                    if is_valid && page.last() {
                        return Ok(Some(page));
                    }
                }
            }
        }

        // Slow path: scan entire file for multiplexed streams.
        // Cap the number of pages to prevent excessive scanning on
        // malformed files with many tiny (e.g., zero-segment) pages.
        // This mirrors the sync find_last behavior.
        reader.seek(SeekFrom::Start(0)).await?;
        let mut best_page = None;
        let mut pages_scanned = 0u32;
        let mut cumulative_bytes: u64 = 0;
        let limits = ParseLimits::default();

        loop {
            if pages_scanned >= MAX_OGG_PAGES {
                break;
            }

            match Self::from_reader_async(reader).await {
                Ok(page) => {
                    pages_scanned += 1;
                    Self::accumulate_page_bytes_with_limit(
                        limits,
                        &mut cumulative_bytes,
                        &page,
                        "OGG cumulative page data",
                    )?;
                    if page.serial == serial {
                        let is_valid = !finishing || page.position != -1;
                        if is_valid {
                            best_page = Some(page.clone());
                        }
                        if page.last() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    if let AudexError::Io(io_err) = &e {
                        if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                            break;
                        }
                    }
                    return Ok(best_page);
                }
            }
        }

        Ok(best_page)
    }

    /// Renumber pages for a given serial starting at sequence number (async version)
    ///
    /// Updates page sequence numbers in place for pages belonging to the specified stream.
    /// This is the async equivalent of `OggPage::renumber`.
    ///
    /// # Arguments
    ///
    /// * `reader` - Async file handle with read/write access
    /// * `serial` - Stream serial number to renumber
    /// * `start_sequence` - Starting sequence number
    pub async fn renumber_async(
        reader: &mut TokioFile,
        serial: u32,
        start_sequence: u32,
    ) -> Result<()> {
        let mut sequence = start_sequence;

        loop {
            let page_offset = reader.stream_position().await?;

            match OggPage::from_reader_async(reader).await {
                Ok(mut page) => {
                    if page.serial == serial {
                        // Update sequence number
                        page.sequence = sequence;

                        // Seek back and overwrite
                        reader.seek(SeekFrom::Start(page_offset)).await?;

                        // Write updated page with recalculated CRC
                        let page_data = page.write()?;
                        reader.write_all(&page_data).await?;

                        // Seek to end of this page to continue
                        reader
                            .seek(SeekFrom::Start(page_offset + page_data.len() as u64))
                            .await?;

                        sequence = sequence.checked_add(1).ok_or_else(|| {
                            AudexError::InvalidData(
                                "Ogg page sequence counter overflow".to_string(),
                            )
                        })?;
                    }
                }
                Err(e) => {
                    if let AudexError::Io(io_err) = &e {
                        if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                            break; // Normal end of file
                        }
                    }
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Replace old pages with new pages in a file (async version)
    ///
    /// Replaces a sequence of Ogg pages with new pages, adjusting file size as needed.
    /// This is the async equivalent of `OggPage::replace`.
    ///
    /// # Arguments
    ///
    /// * `reader` - Async file handle with read/write access
    /// * `old_pages` - Pages to replace
    /// * `new_pages` - Replacement pages
    pub async fn replace_async(
        reader: &mut TokioFile,
        old_pages: &[OggPage],
        new_pages: Vec<OggPage>,
    ) -> Result<()> {
        if old_pages.is_empty() || new_pages.is_empty() {
            return Err(AudexError::InvalidData(
                "empty pages list not allowed".to_string(),
            ));
        }

        let mut updated_pages = new_pages;

        // Number the new pages starting from the first old page
        let first_sequence = old_pages[0].sequence;
        for (i, page) in updated_pages.iter_mut().enumerate() {
            page.sequence = first_sequence.checked_add(i as u32).ok_or_else(|| {
                AudexError::ParseError("Ogg page sequence number overflow".to_string())
            })?;
            page.serial = old_pages[0].serial;
        }

        // Copy flags from old pages to preserve stream structure
        updated_pages[0].set_first(old_pages[0].first());
        updated_pages[0].set_last(old_pages[0].last());
        updated_pages[0].set_continued(old_pages[0].continued());

        let old_last = old_pages.last().ok_or_else(|| {
            AudexError::InternalError("old_pages empty in replace_async".to_string())
        })?;
        let old_last_first = old_last.first();
        let old_last_last = old_last.last();
        let old_last_complete = old_last.is_complete();

        // Only copy BOS to the last page when it IS the first page
        // (i.e. single-page replacement).  When comments expand across
        // multiple pages, BOS must never appear on a non-initial page.
        if updated_pages.len() == 1 {
            updated_pages
                .last_mut()
                .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
                .set_first(old_last_first);
        } else {
            updated_pages
                .last_mut()
                .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
                .set_first(false);
        }
        updated_pages
            .last_mut()
            .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
            .set_last(old_last_last);
        updated_pages
            .last_mut()
            .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
            .set_complete(old_last_complete);

        // Handle incomplete single-packet pages
        let last_page = updated_pages
            .last()
            .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?;
        if !last_page.is_complete() && last_page.packets.len() == 1 {
            updated_pages
                .last_mut()
                .ok_or_else(|| AudexError::InternalError("updated_pages empty".to_string()))?
                .position = -1;
        }

        // Serialize new page data
        let new_data: Result<Vec<Vec<u8>>> = updated_pages.iter().map(|p| p.write()).collect();
        let new_data = new_data?;

        // Adjust for page count differences
        let mut final_data = new_data;
        let pages_diff = old_pages.len() as i64 - final_data.len() as i64;

        if pages_diff > 0 {
            // More old pages than new - add empty data
            for _ in 0..pages_diff {
                final_data.push(Vec::new());
            }
        } else if pages_diff < 0 {
            // When new pages exceed old pages, merge excess page data into the last
            // old-page slot so the file replacement sees exactly old_pages.len() entries.
            let split_idx = old_pages.len().saturating_sub(1);
            if split_idx < final_data.len() {
                let merged: Vec<u8> = final_data.drain(split_idx..).flatten().collect();
                final_data.push(merged);
            }
        }

        // Replace pages in file
        let mut offset_adjust: i64 = 0;
        let mut new_data_end: Option<u64> = None;
        let min_pages = old_pages.len().min(final_data.len());

        // Replace existing old pages with corresponding new data
        for i in 0..min_pages {
            let old_page = &old_pages[i];
            let data = &final_data[i];

            if let Some(offset) = old_page.offset {
                let adjusted = offset.checked_add(offset_adjust).ok_or_else(|| {
                    AudexError::InvalidData("Page offset arithmetic overflow".to_string())
                })?;
                if adjusted < 0 {
                    return Err(AudexError::InvalidData(format!(
                        "Adjusted page offset is negative: {}",
                        adjusted
                    )));
                }
                let adjusted_offset = adjusted as u64;
                let data_size = data.len() as u64;
                let old_size = old_page.size() as u64;

                // Resize the file region
                resize_bytes_async(reader, old_size, data_size, adjusted_offset).await?;

                // Write the new data
                reader.seek(SeekFrom::Start(adjusted_offset)).await?;
                reader.write_all(data).await?;

                new_data_end = Some(adjusted_offset + data_size);
                // Use checked conversion to avoid wrapping on extremely large sizes
                let data_i64 = i64::try_from(data_size).map_err(|_| {
                    AudexError::InvalidData("Page data size exceeds i64 range".to_string())
                })?;
                let old_i64 = i64::try_from(old_size).map_err(|_| {
                    AudexError::InvalidData("Old page size exceeds i64 range".to_string())
                })?;
                // Accumulate offset using checked arithmetic to prevent overflow
                let delta = data_i64.checked_sub(old_i64).ok_or_else(|| {
                    AudexError::InvalidData(
                        "page size delta overflow during offset calculation".to_string(),
                    )
                })?;
                offset_adjust = offset_adjust.checked_add(delta).ok_or_else(|| {
                    AudexError::InvalidData(
                        "cumulative page offset adjustment overflow".to_string(),
                    )
                })?;
            }
        }

        // Handle additional new pages (if any)
        if final_data.len() > old_pages.len() {
            if let Some(insert_offset) = new_data_end {
                for data in &final_data[min_pages..] {
                    let data_size = data.len() as u64;

                    if !data.is_empty() {
                        // Insert space for this page
                        insert_bytes_async(reader, data_size, insert_offset, None).await?;

                        // Write the data
                        reader.seek(SeekFrom::Start(insert_offset)).await?;
                        reader.write_all(data).await?;

                        new_data_end = Some(insert_offset + data_size);
                    }
                }
            }
        }

        // Renumber remaining pages if page count changed
        if old_pages.len() != updated_pages.len() {
            if let Some(end_offset) = new_data_end {
                reader.seek(SeekFrom::Start(end_offset)).await?;
                let last_updated = updated_pages.last().ok_or_else(|| {
                    AudexError::InternalError("updated_pages empty during renumber".to_string())
                })?;
                let serial = last_updated.serial;
                let next_sequence = last_updated.sequence.checked_add(1).ok_or_else(|| {
                    AudexError::InvalidData(
                        "Ogg page sequence counter overflow before renumber".to_string(),
                    )
                })?;
                Self::renumber_async(reader, serial, next_sequence).await?;
            }
        }

        reader.flush().await?;
        Ok(())
    }
}
