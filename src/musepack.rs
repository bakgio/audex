//! Support for Musepack audio files.
//!
//! This module provides support for Musepack (MPC), a lossy audio compression format
//! optimized for transparency at mid-to-high bitrates. Musepack excels at preserving
//! audio quality while achieving smaller file sizes than MP3 at equivalent bitrates.
//!
//! # File Format
//!
//! Musepack supports two major stream versions:
//! - **SV7 (Stream Version 7)**: Legacy format with fixed frame structure
//! - **SV8 (Stream Version 8)**: Modern format with packet-based design
//! - **SV4-6**: Older versions also supported
//!
//! # Audio Characteristics
//!
//! - **Compression**: Lossy (psychoacoustic model)
//! - **Bitrate**: 120-500 kbps (quality levels 4-10)
//! - **Sample Rates**: 32 kHz, 37.8 kHz, 44.1 kHz, 48 kHz
//! - **Channels**: 1-2 (mono/stereo)
//! - **Quality Focus**: Transparency at 180+ kbps
//! - **File Extension**: `.mpc`
//! - **MIME Type**: `audio/x-musepack`
//!
//! # Tagging
//!
//! Musepack uses APEv2 tags:
//! - **Standard fields**: Title, Artist, Album, Year, Track, Genre
//! - **ReplayGain**: Built-in support for playback normalization
//! - **Binary support**: Embedded cover art
//!
//! # Basic Usage
//!
//! ```no_run
//! use audex::musepack::Musepack;
//! use audex::{FileType, Tags};
//!
//! # fn main() -> audex::Result<()> {
//! let mut mpc = Musepack::load("song.mpc")?;
//!
//! println!("Version: {}", mpc.info.version);
//! println!("Sample Rate: {} Hz", mpc.info.sample_rate);
//! println!("Channels: {}", mpc.info.channels);
//!
//! if let Some(tags) = mpc.tags_mut() {
//!     tags.set_text("Title", "Song Title".to_string())?;
//!     tags.set_text("Artist", "Artist Name".to_string())?;
//! }
//!
//! mpc.save()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Stream Versions
//!
//! - **SV4-SV6**: Legacy versions, limited support
//! - **SV7**: Most common, fixed 1152-sample frames
//! - **SV8**: Latest, variable packet sizes, better efficiency
//!
//! # References
//!
//! - [Musepack Official Site](https://musepack.net/)

use crate::{
    AudexError, FileType, Result, StreamInfo,
    apev2::{APEv2, APEv2Tags},
};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

#[cfg(feature = "async")]
use crate::util::loadfile_read_async;
#[cfg(feature = "async")]
use tokio::fs::File as TokioFile;
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Musepack stream versions 4-7 sample rates
const RATES: [u32; 4] = [44100, 48000, 37800, 32000];

/// Parse a Musepack SV8 variable-length integer from a reader.
///
/// SV8 encodes integers using a variable number of bytes (up to `limit`).
/// Each byte contributes 7 data bits; the MSB signals continuation (1 = more
/// bytes follow, 0 = last byte). Returns `(decoded_value, bytes_consumed)`.
pub fn parse_sv8_int<R: Read>(reader: &mut R, limit: usize) -> Result<(u64, usize)> {
    let mut num = 0u64;
    for i in 0..limit {
        let mut buf = [0u8; 1];
        reader
            .read_exact(&mut buf)
            .map_err(|_| AudexError::MusepackHeaderError("Unexpected end of file".to_string()))?;

        let byte = buf[0];

        // Guard against silent overflow: if the accumulated value already
        // occupies more than 57 bits, shifting left by 7 would discard the
        // upper bits of a u64, producing a silently corrupted result.
        if num > (u64::MAX >> 7) {
            return Err(AudexError::MusepackHeaderError(
                "SV8 variable-length integer overflow".to_string(),
            ));
        }

        num = (num << 7) | (byte as u64 & 0x7F);

        if (byte & 0x80) == 0 {
            return Ok((num, i + 1));
        }
    }

    if limit > 0 {
        Err(AudexError::MusepackHeaderError(
            "Invalid SV8 integer".to_string(),
        ))
    } else {
        Ok((0, 0))
    }
}

/// Calculate SV8 gain value
fn calc_sv8_gain(gain: i16) -> f64 {
    // 64.82 taken from mpcdec
    64.82 - (gain as f64) / 256.0
}

/// Calculate SV8 peak value
fn calc_sv8_peak(peak: i16) -> f64 {
    10_f64.powf((peak as f64) / (256.0 * 20.0)) / 65535.0
}

/// Musepack stream information
#[derive(Debug, Default)]
pub struct MusepackStreamInfo {
    pub length: Option<Duration>,
    pub bitrate: Option<u32>,
    pub channels: u16,
    pub sample_rate: u32,
    pub version: u8,
    pub samples: u64,

    // Optional replay gain fields (SV7/SV8 only)
    pub title_gain: Option<f64>,
    pub title_peak: Option<f64>,
    pub album_gain: Option<f64>,
    pub album_peak: Option<f64>,
}

impl StreamInfo for MusepackStreamInfo {
    fn length(&self) -> Option<Duration> {
        self.length
    }
    fn bitrate(&self) -> Option<u32> {
        self.bitrate
    }
    fn sample_rate(&self) -> Option<u32> {
        Some(self.sample_rate)
    }
    fn channels(&self) -> Option<u16> {
        Some(self.channels)
    }
    fn bits_per_sample(&self) -> Option<u16> {
        None
    }
}

impl MusepackStreamInfo {
    /// Parse Musepack file and extract stream information
    pub fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        reader.seek(SeekFrom::Start(0))?;

        let mut header = [0u8; 4];
        reader
            .read_exact(&mut header)
            .map_err(|_| AudexError::MusepackHeaderError("Not a Musepack file".to_string()))?;

        // Skip ID3v2 tags if present
        if &header[0..3] == b"ID3" {
            let mut id3_header = [0u8; 6];
            reader
                .read_exact(&mut id3_header)
                .map_err(|_| AudexError::MusepackHeaderError("Not a Musepack file".to_string()))?;

            // Parse ID3v2 size (syncsafe integer - each byte uses only lower 7 bits)
            let size = (((id3_header[2] & 0x7F) as u32) << 21)
                | (((id3_header[3] & 0x7F) as u32) << 14)
                | (((id3_header[4] & 0x7F) as u32) << 7)
                | ((id3_header[5] & 0x7F) as u32);

            reader.seek(SeekFrom::Start(10 + size as u64))?;
            reader
                .read_exact(&mut header)
                .map_err(|_| AudexError::MusepackHeaderError("Not a Musepack file".to_string()))?;
        }

        // Determine Musepack version and parse accordingly
        if &header == b"MPCK" {
            Self::parse_sv8(reader)
        } else {
            Self::parse_sv467(reader, &header)
        }
    }

    /// Parse SV8 format
    fn parse_sv8<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut info = MusepackStreamInfo {
            version: 8,
            ..Default::default()
        };

        let key_size = 2;
        let mandatory_packets = vec![b"SH".to_vec(), b"RG".to_vec()];
        let mut found_packets = Vec::new();

        loop {
            let mut frame_type = [0u8; 2];
            match reader.read_exact(&mut frame_type) {
                Ok(_) => {}
                Err(_) => break, // End of stream
            }

            // Validate frame key
            if frame_type[0] < b'A'
                || frame_type[0] > b'Z'
                || frame_type[1] < b'A'
                || frame_type[1] > b'Z'
            {
                return Err(AudexError::MusepackHeaderError(
                    "Invalid frame key".to_string(),
                ));
            }

            // Stop at AP (APEv2) or SE (Stream End) packets
            if &frame_type == b"AP" || &frame_type == b"SE" {
                break;
            }

            let (frame_size, size_len) = parse_sv8_int(reader, 9)?;

            // A zero-length frame cannot even contain its own key bytes,
            // so it is always malformed. Reject it before computing
            // data_size to avoid a zero-byte seek that causes an infinite loop.
            if frame_size == 0 {
                return Err(AudexError::MusepackHeaderError(
                    "Malformed packet: zero-length frame".to_string(),
                ));
            }

            let data_size = frame_size
                .saturating_sub(key_size as u64)
                .saturating_sub(size_len as u64);

            // A zero data_size means the frame_size was smaller than its
            // own header overhead. Seeking by 0 bytes would make no forward
            // progress, so reject the malformed packet immediately.
            if data_size == 0 {
                return Err(AudexError::MusepackHeaderError(
                    "Malformed packet: frame_size smaller than header overhead".to_string(),
                ));
            }

            // Reject absurdly large packets to prevent OOM
            if data_size > 4 * 1024 * 1024 {
                return Err(AudexError::MusepackHeaderError(
                    "Packet size too large".to_string(),
                ));
            }

            match &frame_type {
                b"SH" => {
                    if found_packets.contains(&b"SH".to_vec()) {
                        return Err(AudexError::MusepackHeaderError(
                            "Duplicate SH packet".to_string(),
                        ));
                    }
                    found_packets.push(b"SH".to_vec());
                    Self::parse_stream_header(reader, data_size, &mut info)?;
                }
                b"RG" => {
                    if found_packets.contains(&b"RG".to_vec()) {
                        return Err(AudexError::MusepackHeaderError(
                            "Duplicate RG packet".to_string(),
                        ));
                    }
                    found_packets.push(b"RG".to_vec());
                    Self::parse_replaygain_packet(reader, data_size, &mut info)?;
                }
                _ => {
                    // Skip unknown packets — use checked cast to guard
                    // against future cap changes that might exceed i64
                    let seek_offset = i64::try_from(data_size).map_err(|_| {
                        AudexError::MusepackHeaderError(
                            "Packet data_size too large for seek offset".to_string(),
                        )
                    })?;
                    reader.seek(SeekFrom::Current(seek_offset))?;
                }
            }
        }

        // Check for mandatory packets
        for packet in &mandatory_packets {
            if !found_packets.contains(packet) {
                return Err(AudexError::MusepackHeaderError(format!(
                    "Missing mandatory packet: {:?}",
                    String::from_utf8_lossy(packet)
                )));
            }
        }

        // Calculate length and bitrate
        if info.sample_rate > 0 {
            info.length =
                Duration::try_from_secs_f64(info.samples as f64 / info.sample_rate as f64).ok();
        }

        Ok(info)
    }

    /// Parse SV8 Stream Header packet
    fn parse_stream_header<R: Read>(
        reader: &mut R,
        data_size: u64,
        info: &mut MusepackStreamInfo,
    ) -> Result<()> {
        // Skip CRC
        let mut crc = [0u8; 4];
        reader.read_exact(&mut crc)?;
        let remaining_size = data_size
            .checked_sub(4)
            .ok_or_else(|| AudexError::MusepackHeaderError("SH packet too small".to_string()))?;

        // Read version
        let mut version_buf = [0u8; 1];
        reader.read_exact(&mut version_buf).map_err(|_| {
            AudexError::MusepackHeaderError("SH packet ended unexpectedly".to_string())
        })?;
        info.version = version_buf[0];
        let remaining_size = remaining_size.checked_sub(1).ok_or_else(|| {
            AudexError::MusepackHeaderError("SH packet ended unexpectedly".to_string())
        })?;

        // Read sample counts
        let (samples, len1) = parse_sv8_int(reader, 9).map_err(|_| {
            AudexError::MusepackHeaderError("SH packet: Invalid sample counts".to_string())
        })?;
        let (samples_skip, len2) = parse_sv8_int(reader, 9).map_err(|_| {
            AudexError::MusepackHeaderError("SH packet: Invalid sample counts".to_string())
        })?;

        info.samples = samples.saturating_sub(samples_skip);
        let remaining_size = remaining_size
            .checked_sub(len1 as u64 + len2 as u64)
            .ok_or_else(|| {
                AudexError::MusepackHeaderError("SH packet ended unexpectedly".to_string())
            })?;

        // Read rate and channel info
        if remaining_size < 2 {
            return Err(AudexError::MusepackHeaderError(
                "SH packet ended unexpectedly".to_string(),
            ));
        }

        // Only read the 2 bytes needed for rate and channel data,
        // then discard the rest. Allocating the full remaining_size
        // (potentially megabytes) wastes memory for just 2 bytes.
        let mut rate_chan_data = [0u8; 2];
        reader.read_exact(&mut rate_chan_data).map_err(|_| {
            AudexError::MusepackHeaderError("SH packet ended unexpectedly".to_string())
        })?;

        // Discard remaining bytes in the SH packet by reading in
        // small chunks (reader may not support Seek)
        let mut to_skip = remaining_size.saturating_sub(2);
        let mut skip_buf = [0u8; 1024];
        while to_skip > 0 {
            let n = (to_skip as usize).min(skip_buf.len());
            reader.read_exact(&mut skip_buf[..n]).map_err(|_| {
                AudexError::MusepackHeaderError("SH packet ended unexpectedly".to_string())
            })?;
            to_skip -= n as u64;
        }

        let rate_index = (rate_chan_data[0] >> 5) as usize;
        if rate_index >= RATES.len() {
            return Err(AudexError::MusepackHeaderError(
                "Invalid sample rate".to_string(),
            ));
        }
        info.sample_rate = RATES[rate_index];
        let channels = ((rate_chan_data[1] >> 4) + 1) as u16;

        // Musepack only supports mono (1) and stereo (2) configurations
        if channels > 2 {
            return Err(AudexError::MusepackHeaderError(format!(
                "Unsupported channel count {}: Musepack supports at most 2 channels",
                channels
            )));
        }
        info.channels = channels;

        Ok(())
    }

    /// Parse SV8 Replay Gain packet.
    ///
    /// `data_size` is validated independently of the caller so that this
    /// function remains safe to use even if invoked from a different context
    /// that does not enforce the same upper bound.
    fn parse_replaygain_packet<R: Read>(
        reader: &mut R,
        data_size: u64,
        info: &mut MusepackStreamInfo,
    ) -> Result<()> {
        // Reject oversized packets to prevent unbounded allocation.
        // 4 MB matches the cap enforced by the SV8 packet loop, but is
        // checked here as well for defense-in-depth.
        const MAX_RG_PACKET_SIZE: u64 = 4 * 1024 * 1024;
        if data_size > MAX_RG_PACKET_SIZE {
            return Err(AudexError::MusepackHeaderError(
                "RG packet data_size too large".to_string(),
            ));
        }

        if data_size < 9 {
            return Err(AudexError::MusepackHeaderError(
                "Invalid RG packet size".to_string(),
            ));
        }

        let mut data = vec![0u8; data_size as usize];
        reader.read_exact(&mut data).map_err(|_| {
            AudexError::MusepackHeaderError("RG packet ended unexpectedly".to_string())
        })?;

        // Parse gain and peak values (big-endian)
        let title_gain = i16::from_be_bytes([data[1], data[2]]);
        let title_peak = i16::from_be_bytes([data[3], data[4]]);
        let album_gain = i16::from_be_bytes([data[5], data[6]]);
        let album_peak = i16::from_be_bytes([data[7], data[8]]);

        if title_gain != 0 {
            info.title_gain = Some(calc_sv8_gain(title_gain));
        }
        if title_peak != 0 {
            info.title_peak = Some(calc_sv8_peak(title_peak));
        }
        if album_gain != 0 {
            info.album_gain = Some(calc_sv8_gain(album_gain));
        }
        if album_peak != 0 {
            info.album_peak = Some(calc_sv8_peak(album_peak));
        }

        Ok(())
    }

    /// Parse SV4, SV5, SV6, SV7 formats  
    fn parse_sv467<R: Read + Seek>(reader: &mut R, _initial_header: &[u8]) -> Result<Self> {
        reader.seek(SeekFrom::Current(-4))?; // Go back to start of header

        let mut header = [0u8; 32];
        reader
            .read_exact(&mut header)
            .map_err(|_| AudexError::MusepackHeaderError("Not a Musepack file".to_string()))?;

        let mut info = MusepackStreamInfo {
            channels: 2, // All SV4-7 are stereo
            ..Default::default()
        };

        if &header[0..3] == b"MP+" {
            // SV7 format
            info.version = header[3] & 0xF;
            if info.version < 7 {
                return Err(AudexError::MusepackHeaderError(
                    "Not a Musepack file".to_string(),
                ));
            }

            let frames = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
            let flags = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);

            // Parse replay gain data
            let title_peak = u16::from_le_bytes([header[12], header[13]]);
            let title_gain = i16::from_le_bytes([header[14], header[15]]);
            let album_peak = u16::from_le_bytes([header[16], header[17]]);
            let album_gain = i16::from_le_bytes([header[18], header[19]]);

            // Only populate replay gain fields when non-zero; zero means
            // the encoder did not write a value (consistent with SV8 handling).
            if title_gain != 0 {
                info.title_gain = Some(title_gain as f64 / 100.0);
            }
            if album_gain != 0 {
                info.album_gain = Some(album_gain as f64 / 100.0);
            }
            if title_peak != 0 {
                info.title_peak = Some(title_peak as f64 / 65535.0);
            }
            if album_peak != 0 {
                info.album_peak = Some(album_peak as f64 / 65535.0);
            }

            let rate_index = ((flags >> 16) & 0x0003) as usize;
            if rate_index >= RATES.len() {
                return Err(AudexError::MusepackHeaderError(
                    "Invalid sample rate".to_string(),
                ));
            }
            info.sample_rate = RATES[rate_index];
            info.bitrate = Some(0); // Will be calculated later

            info.samples = (frames as u64).saturating_mul(1152).saturating_sub(576);
        } else {
            // SV4-SV6 format
            let header_dword = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            // Extract the 10-bit version field and validate before
            // truncating to u8 — raw values 256+ would wrap around
            // and could land in the valid 4-6 range by accident
            let raw_version = (header_dword >> 11) & 0x03FF;
            if !(4..=6).contains(&raw_version) {
                return Err(AudexError::MusepackHeaderError(
                    "Not a Musepack file".to_string(),
                ));
            }
            // Safe to truncate: validated to be in range 4-6
            info.version = raw_version as u8;

            // The 9-bit field stores bitrate in kbps; convert to bps
            // for consistency with other formats and the StreamInfo trait.
            let raw_kbps = (header_dword >> 23) & 0x01FF;
            info.bitrate = Some(raw_kbps.saturating_mul(1000));
            info.sample_rate = 44100;

            let frames = if info.version >= 5 {
                u32::from_le_bytes([header[4], header[5], header[6], header[7]])
            } else {
                u16::from_le_bytes([header[6], header[7]]) as u32
            };

            let actual_frames = if info.version < 6 {
                frames.saturating_sub(1)
            } else {
                frames
            };
            info.samples = (actual_frames as u64)
                .saturating_mul(1152)
                .saturating_sub(576);
        }

        if info.sample_rate > 0 {
            info.length =
                Duration::try_from_secs_f64(info.samples as f64 / info.sample_rate as f64).ok();
        }

        Ok(info)
    }

    /// Pretty print stream info
    pub fn pprint(&self) -> String {
        let mut rg_data = Vec::new();

        if let Some(title_gain) = self.title_gain {
            rg_data.push(format!("{:+0.2} (title)", title_gain));
        }
        if let Some(album_gain) = self.album_gain {
            rg_data.push(format!("{:+0.2} (album)", album_gain));
        }

        let rg_str = if rg_data.is_empty() {
            String::new()
        } else {
            format!(", Gain: {}", rg_data.join(", "))
        };

        format!(
            "Musepack SV{}, {:.2} seconds, {} Hz, {} bps{}",
            self.version,
            self.length.map(|d| d.as_secs_f64()).unwrap_or(0.0),
            self.sample_rate,
            self.bitrate.unwrap_or(0),
            rg_str
        )
    }
}

/// Musepack file with APEv2 tags
#[derive(Debug)]
pub struct Musepack {
    pub info: MusepackStreamInfo,
    pub tags: Option<APEv2Tags>,
    pub filename: Option<String>,
}

impl Musepack {
    /// Create a new empty Musepack instance with default stream info and no tags.
    pub fn new() -> Self {
        Self {
            info: MusepackStreamInfo::default(),
            tags: None,
            filename: None,
        }
    }

    /// Parse Musepack file and extract information
    fn parse_file<R: Read + Seek>(&mut self, reader: &mut R) -> Result<()> {
        // Parse stream info
        self.info = MusepackStreamInfo::from_reader(reader)?;

        // Calculate file-based bitrate if not already set
        if self.info.bitrate.is_none() || self.info.bitrate == Some(0) {
            let current_pos = reader.stream_position()?;
            reader.seek(SeekFrom::End(0))?;
            let file_size = reader.stream_position()?;
            reader.seek(SeekFrom::Start(current_pos))?;

            if let Some(length) = self.info.length {
                if length.as_secs_f64() > 0.0 {
                    // Clamp before casting to avoid truncation on extreme values
                    let bitrate_f64 =
                        (file_size.saturating_mul(8) as f64 / length.as_secs_f64()).round();
                    let bitrate = if bitrate_f64 > u32::MAX as f64 {
                        u32::MAX
                    } else {
                        bitrate_f64 as u32
                    };
                    self.info.bitrate = Some(bitrate);
                }
            }
        }

        // Parse APEv2 tags
        if let Some(filename) = &self.filename {
            match APEv2::load(filename) {
                Ok(ape) => self.tags = Some(ape.tags),
                Err(_) => self.tags = None, // No APE tags or parsing failed
            }
        }

        Ok(())
    }

    /// Add empty APEv2 tags
    pub fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::MusepackHeaderError(
                "APEv2 tag already exists".to_string(),
            ));
        }
        self.tags = Some(APEv2Tags::new());
        Ok(())
    }

    /// Clear APEv2 tags
    pub fn clear(&mut self) -> Result<()> {
        if let Some(ref filename) = self.filename {
            // Use APEv2::clear to properly remove tags from the file
            crate::apev2::clear(filename)?;
        }
        self.tags = None;
        Ok(())
    }

    /// Get MIME types
    pub fn mime(&self) -> Vec<&'static str> {
        vec!["audio/x-musepack", "audio/x-mpc"]
    }

    /// Pretty print file info
    pub fn pprint(&self) -> String {
        self.info.pprint()
    }

    /// Load Musepack file asynchronously
    #[cfg(feature = "async")]
    pub async fn load_async<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut file = loadfile_read_async(&path).await?;
        let mut mpc = Musepack::new();
        mpc.filename = Some(path.as_ref().to_string_lossy().to_string());

        // Parse stream info
        mpc.info = Self::parse_info_async(&mut file).await?;

        // Load APEv2 tags
        match crate::apev2::APEv2::load_async(&path).await {
            Ok(ape) => mpc.tags = Some(ape.tags),
            Err(AudexError::APENoHeader) => mpc.tags = None,
            Err(e) => return Err(e),
        }

        Ok(mpc)
    }

    /// Parse stream information asynchronously.
    ///
    /// Musepack headers are small — SV4-7 uses a fixed 32-byte header, SV8
    /// reads sequential packets that typically total under 1 KB. This reads
    /// at most 64 KB of header data via async I/O (far more than needed),
    /// then delegates to the sync parser. Bitrate is recalculated from the
    /// real file size to match the sync `parse_file` logic.
    #[cfg(feature = "async")]
    async fn parse_info_async(file: &mut TokioFile) -> Result<MusepackStreamInfo> {
        let file_size = file.seek(SeekFrom::End(0)).await?;

        // Musepack headers live at the start and are tiny. 64 KB is a very
        // generous cap — the actual header is typically well under 1 KB.
        const MAX_HEADER_READ: u64 = 64 * 1024;
        let read_size = std::cmp::min(file_size, MAX_HEADER_READ) as usize;

        file.seek(SeekFrom::Start(0)).await?;
        let mut data = vec![0u8; read_size];
        file.read_exact(&mut data).await?;

        let mut cursor = std::io::Cursor::new(&data[..]);
        let mut info = MusepackStreamInfo::from_reader(&mut cursor)?;

        // Recalculate bitrate from the real file size (the Cursor only saw
        // the header portion, so any file-size-based calculations need the
        // true value).
        if info.bitrate.is_none() || info.bitrate == Some(0) {
            if let Some(length) = info.length {
                if length.as_secs_f64() > 0.0 {
                    // Clamp before casting to avoid truncation on extreme values
                    let bitrate_f64 =
                        (file_size.saturating_mul(8) as f64 / length.as_secs_f64()).round();
                    let bitrate = if bitrate_f64 > u32::MAX as f64 {
                        u32::MAX
                    } else {
                        bitrate_f64 as u32
                    };
                    info.bitrate = Some(bitrate);
                }
            }
        }

        Ok(info)
    }

    /// Save tags asynchronously
    #[cfg(feature = "async")]
    pub async fn save_async(&mut self) -> Result<()> {
        let filename = self
            .filename
            .clone()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        if let Some(ref tags) = self.tags {
            let mut ape = crate::apev2::APEv2::new();
            ape.filename = Some(filename);
            ape.tags = tags.clone();
            ape.save_async().await
        } else {
            Ok(())
        }
    }

    /// Clear tags asynchronously
    #[cfg(feature = "async")]
    pub async fn clear_async(&mut self) -> Result<()> {
        if let Some(filename) = &self.filename {
            crate::apev2::clear_async(filename).await?;
        }
        self.tags = None;
        Ok(())
    }
}

impl Default for Musepack {
    fn default() -> Self {
        Self::new()
    }
}

impl FileType for Musepack {
    type Tags = APEv2Tags;
    type Info = MusepackStreamInfo;

    fn format_id() -> &'static str {
        "Musepack"
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        debug_event!("parsing Musepack file");
        let mut file = std::fs::File::open(&path)?;
        let mut musepack = Musepack::new();
        musepack.filename = Some(path.as_ref().to_string_lossy().to_string());

        musepack.parse_file(&mut file)?;
        Ok(musepack)
    }

    fn load_from_reader(reader: &mut dyn crate::ReadSeek) -> Result<Self> {
        let mut instance = Self::new();
        let mut reader = reader;
        instance.parse_file(&mut reader)?;

        // Parse APEv2 tags directly from the reader
        reader.seek(std::io::SeekFrom::Start(0))?;
        if let Ok(ape) = <crate::apev2::APEv2 as FileType>::load_from_reader(&mut reader) {
            instance.tags = Some(ape.tags);
        }

        Ok(instance)
    }

    fn save(&mut self) -> Result<()> {
        let filename = self
            .filename
            .as_ref()
            .ok_or(AudexError::InvalidData("No filename set".to_string()))?;

        // Save APEv2 tags if they exist
        if let Some(ref tags) = self.tags {
            // Create an APEv2 instance to handle the saving
            let mut apev2 = crate::apev2::APEv2::new();
            apev2.filename = Some(filename.clone());

            // Copy tags from our Musepack tags to the APEv2 tags
            for (key, value) in tags.items() {
                let _ = apev2.tags.set(&key, value.clone());
            }

            // Save the tags
            apev2.save()?;
        }

        Ok(())
    }

    fn clear(&mut self) -> Result<()> {
        if let Some(ref filename) = self.filename {
            // Use APEv2::clear to properly remove tags from the file
            crate::apev2::clear(filename)?;
        }
        self.tags = None;
        Ok(())
    }

    fn save_to_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        if let Some(ref tags) = self.tags {
            let mut apev2 = crate::apev2::APEv2::new();
            for (key, value) in tags.items() {
                let _ = apev2.tags.set(&key, value.clone());
            }
            apev2.save_to_writer(writer)?;
        }
        Ok(())
    }

    fn clear_writer(&mut self, writer: &mut dyn crate::ReadWriteSeek) -> Result<()> {
        let mut apev2 = crate::apev2::APEv2::new();
        apev2.clear_writer(writer)?;
        self.tags = None;
        Ok(())
    }

    fn save_to_path(&mut self, path: &Path) -> Result<()> {
        if let Some(ref tags) = self.tags {
            let mut apev2 = crate::apev2::APEv2::new();
            apev2.filename = Some(path.to_string_lossy().to_string());
            for (key, value) in tags.items() {
                let _ = apev2.tags.set(&key, value.clone());
            }
            apev2.save()?;
        }
        Ok(())
    }

    /// Adds empty APEv2 tags to the file.
    ///
    /// Creates a new empty tag structure if none exists. If tags already exist,
    /// returns an error.
    ///
    /// # Errors
    ///
    /// Returns `AudexError::InvalidOperation` if tags already exist.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use audex::musepack::Musepack;
    /// use audex::FileType;
    ///
    /// let mut mpc = Musepack::load("song.mpc")?;
    /// if mpc.tags.is_none() {
    ///     mpc.add_tags()?;
    /// }
    /// mpc.set("title", vec!["My Song".to_string()])?;
    /// mpc.save()?;
    /// # Ok::<(), audex::AudexError>(())
    /// ```
    fn add_tags(&mut self) -> Result<()> {
        if self.tags.is_some() {
            return Err(AudexError::InvalidOperation(
                "APE tags already exist".to_string(),
            ));
        }
        self.tags = Some(APEv2Tags::new());
        Ok(())
    }

    fn get(&self, key: &str) -> Option<Vec<String>> {
        // APEv2Tags stores values as APEValue, need to convert to Vec<String>
        self.tags.as_ref()?.get(key)?.as_text_list().ok()
    }

    fn tags(&self) -> Option<&Self::Tags> {
        self.tags.as_ref()
    }

    fn tags_mut(&mut self) -> Option<&mut Self::Tags> {
        self.tags.as_mut()
    }

    fn info(&self) -> &Self::Info {
        &self.info
    }

    fn score(filename: &str, header: &[u8]) -> i32 {
        let mut score = 0;

        // Check for Musepack signatures
        if header.len() >= 4 {
            if &header[0..3] == b"MP+" {
                score += 2;
            }
            if &header[0..4] == b"MPCK" {
                score += 2;
            }
        }

        // Check file extension
        let lower_filename = filename.to_lowercase();
        if lower_filename.ends_with(".mpc") {
            score += 1;
        }

        score
    }

    fn mime_types() -> &'static [&'static str] {
        &["audio/x-musepack", "audio/x-mpc"]
    }
}

/// Standalone functions for Musepack operations
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut musepack = Musepack::load(path)?;
    musepack.clear()
}
