//! ID3v1 and ID3v1.1 tag reading and writing
//!
//! ID3v1 is a legacy tag format stored in the last 128 bytes of an MP3 file.
//! It supports a fixed set of fields with limited lengths: title (30 chars),
//! artist (30 chars), album (30 chars), year (4 chars), comment (28–30 chars),
//! and genre (numeric index). ID3v1.1 extends the format by using the last two
//! bytes of the comment field for a track number.

use crate::constants;
use crate::id3::frames::{COMM, TCON, TextFrame};
use crate::id3::specs::TextEncoding;
use crate::{AudexError, Result};
use std::collections::HashMap;

/// Type alias for ID3v1 find result
type ID3v1FindResult = (
    Option<HashMap<String, Box<dyn crate::id3::frames::Frame>>>,
    i64,
);

/// Parsed ID3v1/v1.1 tag (128 bytes at end of file)
///
/// All string fields are parsed from fixed-width, null-padded fields and returned
/// as trimmed Rust strings. Track number is
/// `Some` for ID3v1.1 tags and `None` for plain ID3v1 tags.
#[derive(Debug, Clone)]
pub struct ID3v1Tag {
    /// Song title (max 30 characters)
    pub title: String,
    /// Artist name (max 30 characters)
    pub artist: String,
    /// Album name (max 30 characters)
    pub album: String,
    /// Year string (max 4 characters)
    pub year: String,
    /// Comment text (max 28 chars for v1.1, 30 chars for v1.0)
    pub comment: String,
    /// Track number (`Some` for ID3v1.1, `None` for v1.0)
    pub track: Option<u8>,
    /// Genre index into the standard ID3v1 genre table (see [`constants::GENRES`])
    pub genre: u8,
}

impl ID3v1Tag {
    /// Parse ID3v1 tag from 128 bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != 128 {
            return Err(AudexError::InvalidData(
                "ID3v1 tag must be exactly 128 bytes".to_string(),
            ));
        }

        if &data[0..3] != b"TAG" {
            return Err(AudexError::InvalidData(
                "Invalid ID3v1 tag header".to_string(),
            ));
        }

        // Extract fields with null termination handling
        let title = extract_field(&data[3..33]);
        let artist = extract_field(&data[33..63]);
        let album = extract_field(&data[63..93]);
        let year = extract_field(&data[93..97]);

        // Comment field - check for track number (ID3v1.1)
        let (comment, track) = if data[125] == 0 && data[126] != 0 {
            // ID3v1.1 - has track number
            (extract_field(&data[97..125]), Some(data[126]))
        } else {
            // ID3v1.0 - no track number
            (extract_field(&data[97..127]), None)
        };

        let genre = data[127];

        Ok(Self {
            title,
            artist,
            album,
            year,
            comment,
            track,
            genre,
        })
    }

    /// Convert to 128 bytes for writing
    pub fn to_bytes(&self) -> [u8; 128] {
        let mut data = [0u8; 128];

        // Header
        data[0..3].copy_from_slice(b"TAG");

        // Fields
        write_field(&mut data[3..33], &self.title);
        write_field(&mut data[33..63], &self.artist);
        write_field(&mut data[63..93], &self.album);
        write_field(&mut data[93..97], &self.year);

        // Comment and track
        if let Some(track) = self.track {
            // ID3v1.1 format
            write_field(&mut data[97..125], &self.comment);
            data[125] = 0; // Zero byte before track
            data[126] = track;
        } else {
            // ID3v1.0 format
            write_field(&mut data[97..127], &self.comment);
        }

        // Genre
        data[127] = self.genre;

        data
    }

    /// Get genre name
    pub fn genre_name(&self) -> Option<&'static str> {
        constants::get_genre(self.genre)
    }

    /// Set genre by name
    pub fn set_genre_name(&mut self, name: &str) -> bool {
        if let Some(id) = constants::find_genre_id(name) {
            self.genre = id;
            true
        } else {
            false
        }
    }

    /// Check if this is ID3v1.1 (has track number)
    pub fn is_v11(&self) -> bool {
        self.track.is_some()
    }

    /// Create new empty ID3v1 tag
    pub fn new() -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            year: String::new(),
            comment: String::new(),
            track: None,
            genre: 255, // No genre (was: 0 = Blues)
        }
    }

    /// Create ID3v1.1 tag with track number
    pub fn with_track(track: u8) -> Self {
        Self {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            year: String::new(),
            comment: String::new(),
            track: Some(track),
            genre: 255, // No genre
        }
    }

    /// Check if tag has any meaningful data (ignores genre field)
    pub fn is_empty(&self) -> bool {
        self.title.is_empty()
            && self.artist.is_empty()
            && self.album.is_empty()
            && self.year.is_empty()
            && self.comment.is_empty()
            && self.track.is_none()
    }
}

impl Default for ID3v1Tag {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract null-terminated string from fixed-size field
fn extract_field(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());

    // Use latin1 decoding as standard
    let latin1_bytes = &data[..end];
    let mut result = String::new();
    for &byte in latin1_bytes {
        result.push(byte as char);
    }
    result.trim().to_string()
}

/// Encode a Unicode string to Latin-1 (ISO 8859-1) bytes.
/// Characters in U+0000..U+00FF are mapped directly to their byte value.
/// Characters above U+00FF are replaced with '?' since they have no
/// Latin-1 representation.
fn encode_latin1(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| if c as u32 <= 0xFF { c as u8 } else { b'?' })
        .collect()
}

/// Write string to fixed-size field as Latin-1, padding with zeros.
fn write_field(field: &mut [u8], text: &str) {
    let bytes = encode_latin1(text);
    let len = bytes.len().min(field.len());

    field[..len].copy_from_slice(&bytes[..len]);
    // Rest is already zeroed
}

/// Find ID3v1 tag in file data
pub fn find_id3v1_tag(data: &[u8]) -> Option<ID3v1Tag> {
    if data.len() < 128 {
        return None;
    }

    let tag_data = &data[data.len() - 128..];
    if !validate_tag_heuristics(tag_data) {
        return None;
    }
    ID3v1Tag::from_bytes(tag_data).ok()
}

/// Remove ID3v1 tag from file data
pub fn remove_id3v1_tag(data: &mut Vec<u8>) -> bool {
    if data.len() >= 128 && &data[data.len() - 128..data.len() - 125] == b"TAG" {
        data.truncate(data.len() - 128);
        true
    } else {
        false
    }
}

/// Add or replace ID3v1 tag in file data
pub fn write_id3v1_tag(data: &mut Vec<u8>, tag: &ID3v1Tag) {
    // Remove existing tag if present
    remove_id3v1_tag(data);

    // Add new tag
    data.extend_from_slice(&tag.to_bytes());
}

/// Find ID3v1 tag in file
pub fn find_id3v1(
    data: &[u8],
    v2_version: u8,
    known_frames: Option<HashMap<String, String>>,
) -> Result<ID3v1FindResult> {
    if v2_version != 3 && v2_version != 4 {
        return Err(AudexError::InvalidData(
            "Only 3 and 4 possible for v2_version".to_string(),
        ));
    }

    // id3v1 is always at the end (after apev2)
    let extra_read = 3;

    if data.len() < 128 + extra_read {
        // If the file is too small, might be ok since we wrote too small
        // tags at some point. let's see how the parsing goes..
        if data.len() >= 124 {
            // Try parsing from start of available data
            let parse_data = &data[0..];
            if let Some(tag_idx) = find_tag_in_data(parse_data) {
                if let Ok(tag) = parse_id3v1(parse_data, tag_idx, v2_version, &known_frames) {
                    if tag.is_some() {
                        let offset = (tag_idx as i64) - (data.len() as i64);
                        return Ok((tag, offset));
                    }
                }
            }
        }
        return Ok((None, 0));
    }

    let start_pos = data.len() - 128 - extra_read;
    let search_data = &data[start_pos..];

    if let Some(idx) = find_tag_in_data(search_data) {
        // If TAG is part of APETAGEX, assume this is an APEv2 tag
        if let Some(ape_idx) = search_data.windows(8).position(|w| w == b"APETAGEX") {
            if idx == ape_idx + extra_read {
                return Ok((None, 0));
            }
        }

        if let Ok(tag) = parse_id3v1(search_data, idx, v2_version, &known_frames) {
            if tag.is_some() {
                let offset = (idx as i64) - (search_data.len() as i64);
                return Ok((tag, offset));
            }
        }
    }

    Ok((None, 0))
}

/// Seek-based variant of `find_id3v1` that only reads the tail of the stream.
///
/// ID3v1 tags are always the last 128 bytes of a file, optionally preceded by
/// an APEv2 footer. This function seeks to the tail and reads at most 256 bytes
/// instead of buffering the entire file, avoiding OOM on large media files.
pub fn find_id3v1_from_reader<R: std::io::Read + std::io::Seek + ?Sized>(
    reader: &mut R,
    v2_version: u8,
    known_frames: Option<HashMap<String, String>>,
) -> Result<ID3v1FindResult> {
    use std::io::SeekFrom;

    // Determine total stream length
    let end = reader.seek(SeekFrom::End(0))?;

    // Read at most 256 bytes from the tail — enough for ID3v1 (128 bytes)
    // plus the APEv2 preamble check and a small margin.
    let tail_size = std::cmp::min(end, 256) as usize;
    if tail_size == 0 {
        return Ok((None, 0));
    }

    reader.seek(SeekFrom::End(-(tail_size as i64)))?;
    let mut tail = vec![0u8; tail_size];
    reader.read_exact(&mut tail)?;

    find_id3v1(&tail, v2_version, known_frames)
}

/// Find TAG in data - helper function
///
/// After locating the "TAG" marker, applies heuristic validation to reduce
/// false positives from audio data that coincidentally contains the byte
/// sequence [0x54, 0x41, 0x47]. The checks are intentionally lenient to
/// avoid rejecting legitimate tags with unusual but valid content.
fn find_tag_in_data(data: &[u8]) -> Option<usize> {
    let mut search_from = 0;
    while search_from < data.len() {
        let candidate = data[search_from..].windows(3).position(|w| w == b"TAG");
        let idx = match candidate {
            Some(pos) => search_from + pos,
            None => return None,
        };

        let tag_data = &data[idx..];
        if validate_tag_heuristics(tag_data) {
            return Some(idx);
        }

        // Skip past this false match and keep searching
        search_from = idx + 1;
    }
    None
}

/// Heuristic validation for a candidate ID3v1 tag.
///
/// Checks that the data following a "TAG" marker looks plausible as an
/// actual ID3v1 tag rather than coincidental audio bytes. Returns true
/// if the candidate passes all checks.
fn validate_tag_heuristics(tag_data: &[u8]) -> bool {
    // Need at least 124 bytes for the shortest valid ID3v1 tag
    if tag_data.len() < 124 {
        return false;
    }

    // For full 128-byte tags, check that the genre byte is in a valid range.
    // Valid genre indices are 0..=191 (standard + Winamp extensions) or 255
    // (undefined/no genre). Values 192..=254 are not assigned by any known
    // convention and strongly suggest random audio data.
    if tag_data.len() >= 128 {
        let genre = tag_data[127];
        if genre > 191 && genre != 255 {
            return false;
        }
    }

    // Check that the title field (bytes 3..33) contains plausible text.
    // A real title is either empty (all nulls) or contains mostly printable
    // ASCII / Latin-1 characters. If more than half of the non-null bytes
    // are control characters (0x01..0x1F, 0x7F..0x9F), this is almost
    // certainly not a real tag.
    let title_field = &tag_data[3..33.min(tag_data.len())];
    let non_null: Vec<u8> = title_field.iter().copied().filter(|&b| b != 0).collect();
    if !non_null.is_empty() {
        let control_count = non_null
            .iter()
            .filter(|&&b| (0x01..=0x1F).contains(&b) || (0x7F..=0x9F).contains(&b))
            .count();
        // Reject if more than half the non-null bytes are control characters
        if control_count > non_null.len() / 2 {
            return false;
        }
    }

    true
}

/// Get frame type mapping based on known_frames parameter
fn get_frame_class_map(known_frames: &Option<HashMap<String, String>>) -> HashMap<String, bool> {
    let mut frame_class = HashMap::new();
    let frame_keys = vec![
        "TIT2", "TPE1", "TALB", "TYER", "TDRC", "COMM", "TRCK", "TCON",
    ];

    for key in frame_keys {
        if let Some(known) = known_frames {
            frame_class.insert(key.to_string(), known.contains_key(key));
        } else {
            frame_class.insert(key.to_string(), true);
        }
    }

    frame_class
}

/// Parse ID3v1 from data with dynamic format
fn parse_id3v1_from_data(data: &[u8], year_field_size: usize) -> Result<ID3v1Tag> {
    // Minimum length: TAG(3) + title(30) + artist(30) + album(30) +
    // year(0..=4) + comment(30) + genre(1) = 124..=128
    if data.len() < 124 {
        return Err(AudexError::InvalidData(format!(
            "ID3v1 data too short ({} bytes, minimum 124)",
            data.len()
        )));
    }
    if &data[0..3] != b"TAG" {
        return Err(AudexError::InvalidData(
            "Invalid ID3v1 tag header".to_string(),
        ));
    }

    // Layout: tag(3) + title(30) + artist(30) + album(30) + year(variable) + comment(30) + genre(1)

    let title = extract_field(&data[3..33]);
    let artist = extract_field(&data[33..63]);
    let album = extract_field(&data[63..93]);

    // Variable year field size
    let year_end = 93 + year_field_size;
    let year = extract_field(&data[93..year_end]);

    // Comment field spans bytes 97-126 (30 bytes) in a standard 128-byte tag.
    // For ID3v1.1 (with track number), extract_field stops at the null byte
    // at position 125, naturally limiting the comment to 28 bytes.
    let comment_start = year_end;
    let comment_end = (comment_start + 30).min(data.len());
    let comment = extract_field(&data[comment_start..comment_end]);

    // Track and genre occupy the last 2 bytes of a standard 128-byte tag.
    // For shorter tags (124-127 bytes), these positions fall inside the
    // comment field, so we default to "unknown" to avoid misinterpretation.
    let (track, genre) = if data.len() == 128 {
        let track_byte = data[126];
        let genre_byte = data[127];

        // ID3v1.1: byte 125 is a zero-separator, byte 126 is track number
        let track = if data[125] == 0 && track_byte != 0 {
            Some(track_byte)
        } else {
            None
        };
        (track, genre_byte)
    } else {
        // Short tag — cannot reliably extract track or genre
        (None, 255) // 255 = unknown genre
    };

    Ok(ID3v1Tag {
        title,
        artist,
        album,
        year,
        comment,
        track,
        genre,
    })
}

/// Parse ID3v1 tag to ID3v2 frames
pub fn parse_id3v1_to_frames(
    data: &[u8],
    v2_version: u8,
) -> Result<HashMap<String, Box<dyn crate::id3::frames::Frame>>> {
    // Wrapper for original function
    if let Some(idx) = find_tag_in_data(data) {
        if let Ok(Some(frames)) = parse_id3v1(data, idx, v2_version, &None) {
            return Ok(frames);
        }
    }
    Err(AudexError::InvalidData(
        "No valid ID3v1 tag found".to_string(),
    ))
}

/// Parse ID3v1 tag to ID3v2 frames
fn parse_id3v1(
    data: &[u8],
    idx: usize,
    v2_version: u8,
    known_frames: &Option<HashMap<String, String>>,
) -> Result<Option<HashMap<String, Box<dyn crate::id3::frames::Frame>>>> {
    if v2_version != 3 && v2_version != 4 {
        return Err(AudexError::InvalidData(
            "Only 3 and 4 possible for v2_version".to_string(),
        ));
    }

    let tag_data = &data[idx..];
    if tag_data.len() > 128 || tag_data.len() < 124 {
        return Ok(None);
    }

    // Issue #69 - Previous versions, when encountering
    // out-of-spec TDRC and TYER frames of less than four characters,
    // wrote only the characters available - e.g. "1" or "" - into the
    // year field. To parse those, reduce the size of the year field.
    // Dynamic struct format based on actual data length
    let year_field_size = tag_data.len() - 124;

    let id3v1_tag = parse_id3v1_from_data(tag_data, year_field_size)?;
    let mut frames: HashMap<String, Box<dyn crate::id3::frames::Frame>> = HashMap::new();

    // Frame type mapping
    let frame_class_enabled = get_frame_class_map(known_frames);

    // Convert to ID3v2 frames
    if !id3v1_tag.title.is_empty() && *frame_class_enabled.get("TIT2").unwrap_or(&true) {
        let mut frame = TextFrame::new("TIT2".to_string(), vec![id3v1_tag.title]);
        frame.encoding = TextEncoding::Latin1; // encoding=0
        frames.insert("TIT2".to_string(), Box::new(frame));
    }

    if !id3v1_tag.artist.is_empty() && *frame_class_enabled.get("TPE1").unwrap_or(&true) {
        let mut frame = TextFrame::new("TPE1".to_string(), vec![id3v1_tag.artist]);
        frame.encoding = TextEncoding::Latin1; // encoding=0, text=[artist] - list format
        frames.insert("TPE1".to_string(), Box::new(frame));
    }

    if !id3v1_tag.album.is_empty() && *frame_class_enabled.get("TALB").unwrap_or(&true) {
        let mut frame = TextFrame::new("TALB".to_string(), vec![id3v1_tag.album]);
        frame.encoding = TextEncoding::Latin1; // encoding=0
        frames.insert("TALB".to_string(), Box::new(frame));
    }

    // Year handling - priority: TDRC over TYER
    if !id3v1_tag.year.is_empty() {
        if v2_version == 3 && *frame_class_enabled.get("TYER").unwrap_or(&true) {
            let mut frame = TextFrame::new("TYER".to_string(), vec![id3v1_tag.year]);
            frame.encoding = TextEncoding::Latin1; // encoding=0
            frames.insert("TYER".to_string(), Box::new(frame));
        } else if *frame_class_enabled.get("TDRC").unwrap_or(&true) {
            let mut frame = TextFrame::new("TDRC".to_string(), vec![id3v1_tag.year]);
            frame.encoding = TextEncoding::Latin1; // encoding=0
            frames.insert("TDRC".to_string(), Box::new(frame));
        }
    }

    if !id3v1_tag.comment.is_empty() && *frame_class_enabled.get("COMM").unwrap_or(&true) {
        let comm_frame = COMM::new(
            TextEncoding::Latin1,
            *b"eng",
            "ID3v1 Comment".to_string(),
            id3v1_tag.comment,
        );
        frames.insert("COMM".to_string(), Box::new(comm_frame));
    }

    // Don't read a track number if it looks like the comment was
    // padded with spaces instead of nulls (thanks, WinAmp).
    if let Some(track) = id3v1_tag.track {
        if *frame_class_enabled.get("TRCK").unwrap_or(&true)
            && ((track != 32) || (tag_data.len() >= 3 && tag_data[tag_data.len() - 3] == 0))
        {
            let mut frame = TextFrame::new("TRCK".to_string(), vec![track.to_string()]);
            frame.encoding = TextEncoding::Latin1; // encoding=0
            frames.insert("TRCK".to_string(), Box::new(frame));
        }
    }

    if id3v1_tag.genre != 255 && *frame_class_enabled.get("TCON").unwrap_or(&true) {
        let mut tcon_frame = TCON::new("TCON".to_string(), vec![id3v1_tag.genre.to_string()]);
        tcon_frame.encoding = TextEncoding::Latin1; // encoding=0
        frames.insert("TCON".to_string(), Box::new(tcon_frame));
    }

    Ok(Some(frames))
}

/// Create ID3v1 tag from ID3v2 frames
pub fn make_id3v1_from_frames(
    frames: &HashMap<String, Box<dyn crate::id3::frames::Frame>>,
) -> [u8; 128] {
    let mut v1_data = [0u8; 128];

    // Header
    v1_data[0..3].copy_from_slice(b"TAG");

    // Extract and write text fields (30 bytes each, null-padded)
    let field_mappings = [
        ("TIT2", 3, 30),  // title
        ("TPE1", 33, 30), // artist
        ("TALB", 63, 30), // album
    ];

    for (frame_id, start, len) in field_mappings.iter() {
        if let Some(frame) = frames.get(&frame_id.to_string()) {
            if let Some(text) = extract_text_from_frame(frame.as_ref()) {
                // Encode as Latin-1 (not UTF-8) and truncate to field size
                let text_bytes = encode_latin1(&text);
                let copy_len = text_bytes.len().min(*len);
                v1_data[*start..*start + copy_len].copy_from_slice(&text_bytes[..copy_len]);
            }
        }
    }

    // Year field (4 bytes) - priority: TDRC over TYER
    // Trim whitespace from year (fix for issue 69)
    if let Some(tdrc_frame) = frames.get("TDRC") {
        if let Some(year_text) = extract_text_from_frame(tdrc_frame.as_ref()) {
            let trimmed_year = year_text.trim();
            // Encode as Latin-1 to match other ID3v1 fields (not UTF-8)
            let year_bytes = encode_latin1(trimmed_year);
            let copy_len = year_bytes.len().min(4);
            v1_data[93..93 + copy_len].copy_from_slice(&year_bytes[..copy_len]);
        }
    } else if let Some(tyer_frame) = frames.get("TYER") {
        if let Some(year_text) = extract_text_from_frame(tyer_frame.as_ref()) {
            let trimmed_year = year_text.trim();
            // Encode as Latin-1 to match other ID3v1 fields (not UTF-8)
            let year_bytes = encode_latin1(trimmed_year);
            let copy_len = year_bytes.len().min(4);
            v1_data[93..93 + copy_len].copy_from_slice(&year_bytes[..copy_len]);
        }
    }

    // Comment field
    let mut comment_len = 30; // ID3v1.0: comment field spans bytes 97-126 (30 bytes)
    let mut track_num = None;

    // Check for track number first - determines comment length
    if let Some(trck_frame) = frames.get("TRCK") {
        if let Some(track_text) = extract_text_from_frame(trck_frame.as_ref()) {
            // Handle "track/total" format - extract just the track number
            let track_part = if let Some(slash_pos) = track_text.find('/') {
                &track_text[..slash_pos]
            } else {
                &track_text
            };

            // Try to convert track number to byte, use null byte on error
            if let Ok(track) = track_part.parse::<u8>() {
                if track > 0 && track < 255 {
                    track_num = Some(track);
                    comment_len = 28; // ID3v1.1 format - limit comment to 28 bytes
                }
            }
        }
    }

    // Comment frame processing: encode to Latin-1 and truncate to 28 bytes
    if let Some(comm_frame) = frames.get("COMM") {
        if let Some(comment_text) = extract_text_from_frame(comm_frame.as_ref()) {
            let comment_bytes = encode_latin1(&comment_text);
            let copy_len = comment_bytes.len().min(comment_len);
            v1_data[97..97 + copy_len].copy_from_slice(&comment_bytes[..copy_len]);
        }
    }

    // Track number (ID3v1.1) - write after comment
    if let Some(track) = track_num {
        v1_data[125] = 0; // Zero separator
        v1_data[126] = track; // Track number byte
    }

    // Genre
    if let Some(tcon_frame) = frames.get("TCON") {
        if let Some(genre_text) = extract_text_from_frame(tcon_frame.as_ref()) {
            // Extract genre identifier from TCON text.
            // Handles all standard formats:
            //   "(13)"     — number in parentheses
            //   "(13)Pop"  — number with text suffix (ID3v2 standard)
            //   "13"       — bare number
            //   "Pop"      — genre name
            let clean_genre = if genre_text.starts_with('(') {
                // Find the closing parenthesis and extract the number inside
                if let Some(close) = genre_text.find(')') {
                    &genre_text[1..close]
                } else {
                    &genre_text
                }
            } else {
                &genre_text
            };

            // Convert genre name or number to numeric index if valid
            if let Ok(genre_id) = clean_genre.parse::<u8>() {
                if genre_id < 192 {
                    v1_data[127] = genre_id;
                } else {
                    v1_data[127] = 255;
                }
            } else if let Some(genre_id) = constants::find_genre_id(clean_genre) {
                if genre_id < 192 {
                    v1_data[127] = genre_id;
                } else {
                    v1_data[127] = 255;
                }
            } else {
                v1_data[127] = 255;
            }
        } else {
            v1_data[127] = 255; // No genre text
        }
    } else {
        v1_data[127] = 255; // No genre frame - if "genre" not in v1: v1["genre"] = b"\xff"
    }

    v1_data
}

/// Create ID3v1 tag from an ID3Tags dict (BTreeMap with hash_key-based keys).
///
/// This variant handles the difference between frame IDs and hash keys:
/// - Text frames like TIT2, TPE1 use their frame ID as the hash key
/// - COMM frames use "COMM::lang" or "COMM:desc:lang" as the hash key
/// - TRCK, TCON, TDRC, TYER use their frame ID as the hash key
pub fn make_id3v1_from_dict(
    dict: &std::collections::BTreeMap<String, Box<dyn crate::id3::frames::Frame>>,
) -> [u8; 128] {
    let mut v1_data = [0u8; 128];

    // Header
    v1_data[0..3].copy_from_slice(b"TAG");

    // Extract and write text fields (30 bytes each, null-padded)
    let field_mappings = [
        ("TIT2", 3, 30),  // title
        ("TPE1", 33, 30), // artist
        ("TALB", 63, 30), // album
    ];

    for (frame_id, start, len) in field_mappings.iter() {
        if let Some(frame) = dict.get(&frame_id.to_string()) {
            if let Some(text) = extract_text_from_frame(frame.as_ref()) {
                let text_bytes = encode_latin1(&text);
                let copy_len = text_bytes.len().min(*len);
                v1_data[*start..*start + copy_len].copy_from_slice(&text_bytes[..copy_len]);
            }
        }
    }

    // Year field (4 bytes) - TDRC over TYER
    if let Some(tdrc_frame) = dict.get("TDRC") {
        if let Some(year_text) = extract_text_from_frame(tdrc_frame.as_ref()) {
            let trimmed_year = year_text.trim();
            let year_bytes = encode_latin1(trimmed_year);
            let copy_len = year_bytes.len().min(4);
            v1_data[93..93 + copy_len].copy_from_slice(&year_bytes[..copy_len]);
        }
    } else if let Some(tyer_frame) = dict.get("TYER") {
        if let Some(year_text) = extract_text_from_frame(tyer_frame.as_ref()) {
            let trimmed_year = year_text.trim();
            let year_bytes = encode_latin1(trimmed_year);
            let copy_len = year_bytes.len().min(4);
            v1_data[93..93 + copy_len].copy_from_slice(&year_bytes[..copy_len]);
        }
    }

    // Comment field
    let mut comment_len = 30; // ID3v1.0: comment field spans bytes 97-126 (30 bytes)
    let mut track_num = None;

    if let Some(trck_frame) = dict.get("TRCK") {
        if let Some(track_text) = extract_text_from_frame(trck_frame.as_ref()) {
            let track_part = if let Some(slash_pos) = track_text.find('/') {
                &track_text[..slash_pos]
            } else {
                &track_text
            };
            if let Ok(track) = track_part.parse::<u8>() {
                if track > 0 && track < 255 {
                    track_num = Some(track);
                    comment_len = 28;
                }
            }
        }
    }

    // Find COMM frame - look for any key starting with "COMM"
    // Prefer "COMM::eng" (empty description) over others
    let comm_frame = dict.get("COMM::eng").or_else(|| {
        dict.keys()
            .find(|k| k.starts_with("COMM"))
            .and_then(|k| dict.get(k))
    });

    if let Some(frame) = comm_frame {
        if let Some(comment_text) = extract_text_from_frame(frame.as_ref()) {
            let comment_bytes = encode_latin1(&comment_text);
            let copy_len = comment_bytes.len().min(comment_len);
            v1_data[97..97 + copy_len].copy_from_slice(&comment_bytes[..copy_len]);
        }
    }

    // Track number (ID3v1.1)
    if let Some(track) = track_num {
        v1_data[125] = 0;
        v1_data[126] = track;
    }

    // Genre
    if let Some(tcon_frame) = dict.get("TCON") {
        if let Some(genre_text) = extract_text_from_frame(tcon_frame.as_ref()) {
            // Extract genre identifier — handles "(N)", "(N)Text", "N", and "Name"
            let clean_genre = if genre_text.starts_with('(') {
                if let Some(close) = genre_text.find(')') {
                    &genre_text[1..close]
                } else {
                    &genre_text
                }
            } else {
                &genre_text
            };
            if let Ok(genre_id) = clean_genre.parse::<u8>() {
                if genre_id < 192 {
                    v1_data[127] = genre_id;
                } else {
                    v1_data[127] = 255;
                }
            } else if let Some(genre_id) = constants::find_genre_id(clean_genre) {
                if genre_id < 192 {
                    v1_data[127] = genre_id;
                } else {
                    v1_data[127] = 255;
                }
            } else {
                v1_data[127] = 255;
            }
        } else {
            v1_data[127] = 255;
        }
    } else {
        v1_data[127] = 255;
    }

    v1_data
}

/// Helper to extract text from any frame type
fn extract_text_from_frame(frame: &dyn crate::id3::frames::Frame) -> Option<String> {
    // First try to get text values directly (preferred method)
    if let Some(values) = frame.text_values() {
        if !values.is_empty() {
            return Some(values[0].clone());
        }
    }

    // Fallback to parsing description for compatibility
    let description = frame.description();
    description
        .find(": ")
        .map(|colon_pos| description[colon_pos + 2..].to_string())
}
