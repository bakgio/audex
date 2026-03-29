//! Standard MIDI File (SMF) format support
//!
//! This module provides **read-only** support for Standard MIDI Files.
//! SMF files contain musical performance data including notes, tempo changes,
//! and other MIDI events organized into tracks.
//!
//! ## Important Limitations
//!
//! **MIDI files do not support embedded metadata tags.** This module only extracts
//! stream information (duration). The `Tags` implementation is a no-op — `get()`,
//! `set()`, and `remove()` have no effect. Only [`StreamInfo::length()`] returns
//! meaningful data; `bitrate()`, `sample_rate()`, `channels()`, and
//! `bits_per_sample()` all return `None`.
//!
//! ## Supported Formats
//!
//! - **Format 0**: Single track
//! - **Format 1**: Multi-track (tempo from first track applies to all)
//! - **Timing**: Ticks per quarter note (SMPTE timing is not supported)

use crate::{AudexError, FileType, Result, StreamInfo, Tags};
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::util::loadfile_read_async;
#[cfg(feature = "async")]
use std::io::SeekFrom;
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// MIDI event types for internal processing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EventType {
    /// Tempo change meta event
    Tempo,
    /// Regular MIDI event (note on/off, control change, etc.)
    Midi,
}

/// Represents a single MIDI event with timing information
#[derive(Debug, Clone)]
struct MidiEvent {
    /// Cumulative delta time in ticks (u64 to prevent overflow on
    /// long files where many small deltas sum past u32::MAX)
    deltasum: u64,
    /// Type of event
    event_type: EventType,
    /// Event-specific data (delta for MIDI events, tempo for tempo events)
    data: u32,
}

/// Decodes a variable-length quantity (VLQ) from a byte array
///
/// MIDI uses VLQ encoding where each byte contributes 7 bits to the value,
/// and the high bit indicates whether more bytes follow.
///
/// # Arguments
/// * `data` - Byte slice containing the VLQ
/// * `offset` - Starting position in the slice
///
/// # Returns
/// Tuple of (decoded_value, new_offset)
fn var_int(data: &[u8], mut offset: usize) -> Result<(u32, usize)> {
    let mut val: u32 = 0;
    // MIDI VLQ is limited to 4 bytes (28 bits). More continuation bytes
    // would overflow the u32 accumulator via repeated 7-bit left shifts.
    let max_bytes = 4;
    let mut bytes_read = 0;

    loop {
        if offset >= data.len() {
            return Err(AudexError::ParseError(
                "Not enough data for VLQ".to_string(),
            ));
        }
        bytes_read += 1;
        if bytes_read > max_bytes {
            return Err(AudexError::ParseError(
                "VLQ exceeds 4-byte MIDI limit".to_string(),
            ));
        }
        let x = data[offset];
        offset += 1;
        val = (val << 7) | ((x & 0x7F) as u32);
        if (x & 0x80) == 0 {
            return Ok((val, offset));
        }
    }
}

/// Reads and parses a MIDI track chunk
///
/// Processes all events in a track, extracting MIDI events and tempo changes.
/// Implements running status for compact MIDI event encoding.
///
/// # Arguments
/// * `chunk` - Raw track data
///
/// # Returns
/// Vector of parsed MIDI events
fn read_track(chunk: &[u8]) -> Result<Vec<MidiEvent>> {
    // Defensive cap on event count. A 64 MB track could theoretically
    // contain ~64 million 1-byte events; each MidiEvent is ~24 bytes,
    // which would balloon to ~1.5 GB of heap. Cap at 1 million events
    // to keep memory usage reasonable for any well-formed MIDI file.
    const MAX_TRACK_EVENTS: usize = 1_000_000;

    let mut events = Vec::new();
    // Use u64 for cumulative delta to prevent overflow when many
    // small deltas sum past u32::MAX in long MIDI files.
    let mut deltasum: u64 = 0;
    let mut status: u8 = 0;
    let mut off: usize = 0;

    while off < chunk.len() {
        if events.len() >= MAX_TRACK_EVENTS {
            return Err(AudexError::ParseError(format!(
                "Track exceeds maximum event count ({})",
                MAX_TRACK_EVENTS
            )));
        }

        // Read delta time
        let delta;
        (delta, off) = var_int(chunk, off)?;
        deltasum = deltasum.saturating_add(delta as u64);

        if off >= chunk.len() {
            break;
        }

        let mut event_type = chunk[off];
        off += 1;

        if event_type == 0xFF {
            // Meta event
            if off >= chunk.len() {
                return Err(AudexError::ParseError("Truncated meta event".to_string()));
            }
            let meta_type = chunk[off];
            off += 1;

            let num;
            (num, off) = var_int(chunk, off)?;

            // Handle tempo change meta event (type 0x51)
            if meta_type == 0x51 {
                if off + (num as usize) > chunk.len() {
                    return Err(AudexError::ParseError("Truncated tempo data".to_string()));
                }
                let data = &chunk[off..off + (num as usize)];
                if data.len() != 3 {
                    return Err(AudexError::ParseError(
                        "Invalid tempo data length".to_string(),
                    ));
                }
                // Tempo is stored as 3 bytes, microseconds per quarter note
                let tempo = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | (data[2] as u32);
                events.push(MidiEvent {
                    deltasum,
                    event_type: EventType::Tempo,
                    data: tempo,
                });
            }
            // Bounds-check: the VLQ length must not advance past the track data
            let len = num as usize;
            if off + len > chunk.len() {
                return Err(AudexError::ParseError(format!(
                    "Meta event length {} exceeds remaining track data ({})",
                    len,
                    chunk.len() - off
                )));
            }
            off += len;
        } else if event_type == 0xF0 || event_type == 0xF7 {
            // SysEx event
            let val;
            (val, off) = var_int(chunk, off)?;
            let len = val as usize;
            if off + len > chunk.len() {
                return Err(AudexError::ParseError(format!(
                    "SysEx event length {} exceeds remaining track data ({})",
                    len,
                    chunk.len() - off
                )));
            }
            off += len;
        } else if (0xF1..=0xF6).contains(&event_type) {
            // System Common messages (F1-F6) — validate that enough
            // data remains before advancing past the data bytes
            match event_type {
                0xF1 | 0xF3 => {
                    // MIDI Time Code Quarter Frame / Song Select (1 data byte)
                    if off + 1 > chunk.len() {
                        return Err(AudexError::ParseError(format!(
                            "Truncated System Common message 0x{:02X} at offset {}",
                            event_type, off
                        )));
                    }
                    off += 1;
                }
                0xF2 => {
                    // Song Position Pointer (2 data bytes)
                    if off + 2 > chunk.len() {
                        return Err(AudexError::ParseError(format!(
                            "Truncated Song Position Pointer at offset {}",
                            off
                        )));
                    }
                    off += 2;
                }
                0xF6 => {} // Tune Request (no data bytes)
                _ => {}    // F4, F5 are undefined (no data bytes)
            }
        } else if event_type >= 0xF8 {
            // System Real-Time messages (F8-FF) - no data bytes
            // These include: Timing Clock, Start, Continue, Stop, Active Sensing, Reset
        } else {
            // MIDI channel voice message. Determine running status and
            // resolve the effective status byte before computing data length.
            let is_running_status = event_type < 0x80;
            if is_running_status {
                // Running status: reuse the previous status byte.
                // Reject if no valid status has been established yet.
                if status == 0 {
                    return Err(AudexError::ParseError(
                        "MIDI running status used before any valid status byte".to_string(),
                    ));
                }
                event_type = status;
            } else if event_type < 0xF0 {
                // New status byte — remember it for potential running status
                status = event_type;
            } else {
                return Err(AudexError::ParseError("Invalid event type".to_string()));
            }

            // Number of data bytes depends on the message type:
            //   0x80-0xBF: 2 bytes (Note Off/On, Key Pressure, Control Change)
            //   0xC0-0xCF: 1 byte  (Program Change)
            //   0xD0-0xDF: 1 byte  (Channel Pressure)
            //   0xE0-0xEF: 2 bytes (Pitch Bend)
            let data_bytes: usize = match event_type >> 4 {
                0xC | 0xD => 1,
                _ => 2,
            };

            // Advance past the remaining data bytes. For running status, the
            // first data byte was already consumed as event_type, so we skip
            // one fewer byte.
            if is_running_status {
                off += data_bytes - 1;
            } else {
                off += data_bytes;
            }

            // Guard: ensure the offset hasn't advanced past the chunk
            if off > chunk.len() {
                return Err(AudexError::ParseError(
                    "MIDI event data exceeds track bounds".to_string(),
                ));
            }

            events.push(MidiEvent {
                deltasum,
                event_type: EventType::Midi,
                data: delta,
            });
        }
    }

    Ok(events)
}

/// Reads a MIDI chunk (header or track)
///
/// MIDI chunks have a 4-byte identifier followed by a 4-byte length.
///
/// # Arguments
/// * `reader` - File reader positioned at chunk start
///
/// # Returns
/// Tuple of (chunk_identifier, chunk_data)
fn read_chunk<R: Read>(reader: &mut R) -> Result<([u8; 4], Vec<u8>)> {
    let mut info = [0u8; 8];
    reader
        .read_exact(&mut info)
        .map_err(|_| AudexError::ParseError("Truncated chunk header".to_string()))?;

    let identifier = [info[0], info[1], info[2], info[3]];
    let chunklen = u32::from_be_bytes([info[4], info[5], info[6], info[7]]) as usize;

    // Guard against crafted files claiming huge chunk sizes
    const MAX_CHUNK_SIZE: usize = 64 * 1024 * 1024; // 64 MB
    if chunklen > MAX_CHUNK_SIZE {
        return Err(AudexError::ParseError(format!(
            "Chunk size too large: {} bytes (max {})",
            chunklen, MAX_CHUNK_SIZE
        )));
    }

    let mut data = vec![0u8; chunklen];
    reader
        .read_exact(&mut data)
        .map_err(|_| AudexError::ParseError("Truncated chunk data".to_string()))?;

    Ok((identifier, data))
}

/// Calculates the duration of a MIDI file in seconds
///
/// Processes all tracks, handling tempo changes and delta times to compute
/// the total duration. For format 1 files, uses the first track's tempo map
/// for all tracks.
///
/// # Arguments
/// * `reader` - File reader positioned at file start
///
/// # Returns
/// Duration in seconds
fn read_midi_length<R: Read>(reader: &mut R) -> Result<f64> {
    // Read header chunk
    let (identifier, chunk) = read_chunk(reader)?;
    if &identifier != b"MThd" {
        return Err(AudexError::ParseError("Not a MIDI file".to_string()));
    }

    if chunk.len() != 6 {
        return Err(AudexError::ParseError("Invalid MIDI header".to_string()));
    }

    // Parse header
    let format = u16::from_be_bytes([chunk[0], chunk[1]]);
    let ntracks = u16::from_be_bytes([chunk[2], chunk[3]]);
    let tickdiv = u16::from_be_bytes([chunk[4], chunk[5]]);

    // Only support format 0 (single track) and format 1 (multi-track)
    if format > 1 {
        return Err(AudexError::ParseError(format!(
            "Unsupported MIDI format {}",
            format
        )));
    }

    // Check timing division format (must be ticks per quarter note, not SMPTE)
    if (tickdiv >> 15) != 0 {
        return Err(AudexError::ParseError(
            "SMPTE timing not supported".to_string(),
        ));
    }

    if tickdiv == 0 {
        return Err(AudexError::ParseError(
            "Invalid tick division: 0".to_string(),
        ));
    }

    // Read all tracks
    // Enforce a cumulative allocation budget to prevent crafted files with many
    // large track chunks from exhausting available memory.
    const MAX_CUMULATIVE_TRACK_BYTES: usize = 128 * 1024 * 1024; // 128 MB total
    let mut cumulative_bytes: usize = 0;

    let mut tracks: Vec<Vec<MidiEvent>> = Vec::new();
    let mut first_tempos: Option<Vec<MidiEvent>> = None;

    for _ in 0..ntracks {
        let (identifier, chunk) = read_chunk(reader)?;

        // Track cumulative allocation and bail out if budget is exceeded
        cumulative_bytes = cumulative_bytes.saturating_add(chunk.len());
        if cumulative_bytes > MAX_CUMULATIVE_TRACK_BYTES {
            return Err(AudexError::ParseError(format!(
                "Cumulative track data ({} bytes) exceeds {} byte limit",
                cumulative_bytes, MAX_CUMULATIVE_TRACK_BYTES
            )));
        }

        if &identifier != b"MTrk" {
            continue;
        }

        let mut events = read_track(&chunk)?;

        // Extract tempo events for format 1 handling
        let tempos: Vec<MidiEvent> = events
            .iter()
            .filter(|e| e.event_type == EventType::Tempo)
            .cloned()
            .collect();

        if first_tempos.is_none() {
            first_tempos = Some(tempos.clone());
        }

        // For format 1, use first track's tempos for all tracks
        if format == 1 {
            if let Some(ref ft) = first_tempos {
                // Remove existing tempo events and add first track's tempos
                events.retain(|e| e.event_type != EventType::Tempo);
                events.extend(ft.clone());
            }
        }

        // Sort events by deltasum
        events.sort_by_key(|e| (e.deltasum, e.event_type));
        tracks.push(events);
    }

    // Calculate duration for each track
    let mut durations = Vec::new();
    for events in tracks {
        let mut tempo: u32 = 500000; // Default tempo: 500000 microseconds per quarter note (120 BPM)
        let mut parts: Vec<(u64, u32)> = Vec::new();
        // Track the cumulative tick position of the last tempo boundary so we
        // can compute interval deltas from the sorted cumulative positions
        // rather than re-summing individual per-event deltas (which are
        // invalid after cross-track merging and sorting).
        let mut last_tempo_pos: u64 = 0;
        let mut last_event_pos: u64 = 0;

        for event in events {
            match event.event_type {
                EventType::Tempo => {
                    parts.push((event.deltasum.saturating_sub(last_tempo_pos), tempo));
                    tempo = event.data;
                    last_tempo_pos = event.deltasum;
                }
                EventType::Midi => {
                    last_event_pos = event.deltasum;
                }
            }
        }
        parts.push((last_event_pos.saturating_sub(last_tempo_pos), tempo));

        // Calculate total duration for this track
        let mut duration: f64 = 0.0;
        for (deltasum, tempo) in parts {
            let quarter = deltasum as f64 / tickdiv as f64;
            let tpq = tempo as f64;
            duration += quarter * tpq;
        }
        duration /= 1_000_000.0; // Convert microseconds to seconds

        durations.push(duration);
    }

    // Return the longest track duration
    durations
        .into_iter()
        .max_by(|a, b| a.total_cmp(b))
        .ok_or_else(|| AudexError::ParseError("No valid tracks found".to_string()))
}

/// Stream information for SMF files
#[derive(Debug, Clone)]
pub struct SMFInfo {
    /// Duration of the MIDI file
    length: Option<Duration>,
}

impl SMFInfo {
    /// Creates a new SMFInfo by parsing the MIDI file
    ///
    /// # Arguments
    /// * `reader` - File reader positioned at file start
    fn from_reader<R: Read>(reader: &mut R) -> Result<Self> {
        let length_secs = read_midi_length(reader)?;
        let length = if length_secs.is_finite() && length_secs >= 0.0 {
            Some(Duration::from_secs_f64(length_secs))
        } else {
            None
        };
        Ok(SMFInfo { length })
    }
}

impl Default for SMFInfo {
    /// Creates an empty SMFInfo with no data loaded
    fn default() -> Self {
        Self { length: None }
    }
}

impl StreamInfo for SMFInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }

    fn bitrate(&self) -> Option<u32> {
        None
    }

    fn sample_rate(&self) -> Option<u32> {
        None
    }

    fn channels(&self) -> Option<u16> {
        None
    }

    fn bits_per_sample(&self) -> Option<u16> {
        None
    }

    fn pprint(&self) -> String {
        if let Some(length) = self.length {
            format!("SMF, {:.2} seconds", length.as_secs_f64())
        } else {
            "SMF, unknown length".to_string()
        }
    }
}

impl std::fmt::Display for SMFInfo {
    /// Formats stream information for display
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pprint())
    }
}

/// No-op tags implementation for SMF (MIDI files don't support tags)
#[derive(Debug, Clone, Default)]
pub struct SMFTags;

impl Tags for SMFTags {
    fn get(&self, _key: &str) -> Option<&[String]> {
        None
    }

    fn set(&mut self, _key: &str, _values: Vec<String>) {
        // No-op: MIDI files don't support tags
    }

    fn remove(&mut self, _key: &str) {
        // No-op: MIDI files don't support tags
    }

    fn keys(&self) -> Vec<String> {
        Vec::new()
    }

    fn pprint(&self) -> String {
        String::new()
    }
}

/// Standard MIDI File format handler
#[derive(Debug)]
pub struct SMF {
    /// Stream information
    info: SMFInfo,
    /// Path to the file
    _path: Option<String>,
}

impl SMF {
    /// Creates a new empty SMF instance with no data loaded
    pub fn new() -> Self {
        Self {
            info: SMFInfo::default(),
            _path: None,
        }
    }

    /// Loads SMF data from any readable and seekable source
    ///
    /// # Arguments
    /// * `reader` - Any Read + Seek source (file, cursor, etc.)
    pub fn load_from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let info = SMFInfo::from_reader(reader)
            .map_err(|e| AudexError::ParseError(format!("Failed to parse MIDI file: {}", e)))?;

        Ok(SMF { info, _path: None })
    }

    /// Load SMF file asynchronously with non-blocking I/O
    ///
    /// Parses the MIDI header and tracks to extract timing information.
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut smf = SMF::new();
        smf._path = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info asynchronously
        smf.info = Self::parse_info_async(&mut file).await?;

        Ok(smf)
    }

    /// Parse stream information asynchronously.
    ///
    /// MIDI files are small (typically a few KB) and are parsed sequentially
    /// without seeking. This reads up to 10 MB via async I/O to protect
    /// against malicious files claiming absurd sizes, then delegates to the
    /// sync sequential parser.
    #[cfg(feature = "async")]
    async fn parse_info_async(file: &mut TokioFile) -> Result<SMFInfo> {
        // Cap the read to prevent OOM from a crafted file. Standard MIDI
        // files rarely exceed a few hundred KB.
        const MAX_MIDI_READ: u64 = 10 * 1024 * 1024;
        let file_size = file.seek(SeekFrom::End(0)).await?;
        let read_size = std::cmp::min(file_size, MAX_MIDI_READ) as usize;

        file.seek(SeekFrom::Start(0)).await?;
        let mut data = vec![0u8; read_size];
        file.read_exact(&mut data).await?;

        let mut cursor = std::io::Cursor::new(&data[..]);
        SMFInfo::from_reader(&mut cursor)
    }
}

impl Default for SMF {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for SMF {
    type Tags = SMFTags;
    type Info = SMFInfo;

    fn format_id() -> &'static str {
        "SMF"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing SMF stream info");
        let file = File::open(path.as_ref())?;
        let mut reader = BufReader::new(file);

        let info = SMFInfo::from_reader(&mut reader)
            .map_err(|e| AudexError::ParseError(format!("Failed to parse MIDI file: {}", e)))?;

        Ok(SMF {
            info,
            _path: Some(path.as_ref().to_string_lossy().to_string()),
        })
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        debug_event!("parsing SMF stream info from reader");
        let mut reader = reader;
        let info = SMFInfo::from_reader(&mut reader)
            .map_err(|e| AudexError::ParseError(format!("Failed to parse MIDI file: {}", e)))?;
        Ok(SMF { info, _path: None })
    }

    fn save(&mut self) -> Result<()> {
        Err(AudexError::Unsupported(
            "MIDI files don't support tags".to_string(),
        ))
    }

    fn clear(&mut self) -> Result<()> {
        Err(AudexError::Unsupported(
            "MIDI files don't support tags".to_string(),
        ))
    }

    /// MIDI format does not support metadata tags.
    ///
    /// This method always returns an error since MIDI files use meta events
    /// rather than embedded tag metadata.
    ///
    /// # Errors
    ///
    /// Always returns `AudexError::Unsupported`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::smf::SMF;
    /// use audex::FileType;
    ///
    /// let mut midi = SMF::load("song.mid")?;
    /// // MIDI doesn't support tags
    /// assert!(midi.add_tags().is_err());
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        Err(AudexError::Unsupported(
            "MIDI files don't support tags".to_string(),
        ))
    }

    fn tags(&self) -> Option<&Self::Tags> {
        None
    }

    fn tags_mut(&mut self) -> Option<&mut Self::Tags> {
        None
    }

    fn info(&self) -> &Self::Info {
        &self.info
    }

    fn score(filename: &str, header: &[u8]) -> i32 {
        let filename_lower = filename.to_lowercase();
        // Support .mid, .midi, and .kar (karaoke MIDI) extensions
        let has_extension = filename_lower.ends_with(".mid")
            || filename_lower.ends_with(".midi")
            || filename_lower.ends_with(".kar");
        let has_header = header.len() >= 4 && &header[0..4] == b"MThd";

        if has_header && has_extension {
            100
        } else if has_header {
            50
        } else if has_extension {
            // Extension alone provides some confidence
            10
        } else {
            0
        }
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/midi", "audio/x-midi"]
    }
}
