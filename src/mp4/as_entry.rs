//! MP4 audio sample entry parsing.
//!
//! This module handles parsing of audio sample entries from MP4/M4A files,
//! including the MPEG-4 descriptor hierarchy (ES_Descriptor, DecoderConfigDescriptor,
//! DecoderSpecificInfo) used to extract codec parameters, bitrate, sample rate,
//! and channel configuration.
//!
//! # Supported Codecs
//!
//! - **AAC** (mp4a + esds): Full descriptor parsing with SBR/PS detection
//! - **ALAC** (alac): Apple Lossless codec parameters
//! - **AC-3** (ac-3 + dac3): Dolby Digital channel/bitrate extraction
//!
//! # Descriptor Hierarchy
//!
//! For AAC (mp4a) atoms, the descriptor structure is:
//! ```text
//! esds
//!   └── ES_Descriptor (tag 0x03)
//!         └── DecoderConfigDescriptor (tag 0x04)
//!               └── DecoderSpecificInfo (tag 0x05)
//! ```

use crate::mp4::atom::MP4Atom;
use crate::mp4::util::parse_full_atom;
use crate::util::{BitReader, BitReaderError};
use crate::{AudexError, Result};
use std::io::{Cursor, Read, Seek, SeekFrom};

/// Descriptor parsing error
#[derive(Debug)]
pub struct DescriptorError {
    pub message: String,
}

impl std::fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for DescriptorError {}

impl From<BitReaderError> for DescriptorError {
    fn from(err: BitReaderError) -> Self {
        DescriptorError {
            message: err.message,
        }
    }
}

impl From<std::io::Error> for DescriptorError {
    fn from(err: std::io::Error) -> Self {
        DescriptorError {
            message: format!("IO error: {}", err),
        }
    }
}

/// Base descriptor trait for MP4 descriptors
pub trait BaseDescriptor: Sized {
    const TAG: u8;

    /// Parse descriptor length using variable-length encoding
    fn parse_desc_length_file<R: Read>(
        reader: &mut R,
    ) -> std::result::Result<u32, DescriptorError> {
        let mut value: u32 = 0;

        let mut found_end = false;
        for _ in 0..4 {
            let mut buf = [0u8; 1];
            reader.read_exact(&mut buf).map_err(|e| DescriptorError {
                message: format!("Failed to read length byte: {}", e),
            })?;
            let b = buf[0];

            value = (value << 7) | ((b & 0x7f) as u32);
            if (b >> 7) == 0 {
                found_end = true;
                break;
            }
        }

        if !found_end {
            return Err(DescriptorError {
                message: "invalid descriptor length".to_string(),
            });
        }

        Ok(value)
    }

    /// Parse a descriptor from a reader
    fn parse<R: Read + Seek>(reader: &mut R) -> std::result::Result<Self, DescriptorError> {
        let length = Self::parse_desc_length_file(reader)?;
        let pos = reader.stream_position().map_err(|e| DescriptorError {
            message: format!("Failed to get position: {}", e),
        })?;

        // Validate that the declared descriptor length does not exceed the
        // remaining data in the stream. Without this check, a crafted file
        // could claim up to 268 MB and cause reads past the atom boundary.
        let end = reader.seek(SeekFrom::End(0)).map_err(|e| DescriptorError {
            message: format!("Failed to seek to end: {}", e),
        })?;
        let remaining = end.saturating_sub(pos);
        if (length as u64) > remaining {
            return Err(DescriptorError {
                message: format!(
                    "descriptor length {} exceeds remaining data ({} bytes)",
                    length, remaining
                ),
            });
        }
        reader
            .seek(SeekFrom::Start(pos))
            .map_err(|e| DescriptorError {
                message: format!("Failed to restore position: {}", e),
            })?;

        let instance = Self::new(reader, length)?;

        let current_pos = reader.stream_position().map_err(|e| DescriptorError {
            message: format!("Failed to get current position: {}", e),
        })?;

        // Safely calculate remaining bytes to avoid overflow
        let consumed = current_pos.saturating_sub(pos);
        let left = (length as u64).saturating_sub(consumed);

        // Use absolute positioning to avoid the i64 cast that could wrap
        // to negative for extremely large skip values
        if left > 0 {
            let target = current_pos.saturating_add(left);
            reader
                .seek(SeekFrom::Start(target))
                .map_err(|e| DescriptorError {
                    message: format!("Failed to seek: {}", e),
                })?;
        }

        Ok(instance)
    }

    /// Create new descriptor instance from reader
    fn new<R: Read + Seek>(
        reader: &mut R,
        length: u32,
    ) -> std::result::Result<Self, DescriptorError>;
}

/// Elementary Stream Descriptor (ISO 14496-1, tag 0x03).
///
/// Top-level descriptor in the MPEG-4 descriptor hierarchy. Contains stream
/// identification, dependency info, and a nested [`DecoderConfigDescriptor`]
/// with codec-specific parameters.
#[derive(Debug)]
pub struct ESDescriptor {
    pub dec_config_descr: DecoderConfigDescriptor,
}

impl BaseDescriptor for ESDescriptor {
    const TAG: u8 = 0x3;

    fn new<R: Read + Seek>(
        reader: &mut R,
        _length: u32,
    ) -> std::result::Result<Self, DescriptorError> {
        let mut bit_reader = BitReader::new(reader)?;

        let _es_id = bit_reader.bits(16)?;
        let stream_dependence_flag = bit_reader.bits(1)? != 0;
        let url_flag = bit_reader.bits(1)? != 0;
        let ocr_stream_flag = bit_reader.bits(1)? != 0;
        let _stream_priority = bit_reader.bits(5)?;

        if stream_dependence_flag {
            let _depends_on_es_id = bit_reader.bits(16)?;
        }

        if url_flag {
            let url_length = bit_reader.bits(8)?;
            let _url_string = bit_reader.bytes(url_length)?;
        }

        if ocr_stream_flag {
            let _ocr_es_id = bit_reader.bits(16)?;
        }

        let tag = bit_reader.bits(8)? as u8;
        if tag != DecoderConfigDescriptor::TAG {
            return Err(DescriptorError {
                message: format!("unexpected DecoderConfigDescrTag {}", tag),
            });
        }

        if !bit_reader.is_aligned() {
            return Err(DescriptorError {
                message: "BitReader not aligned before DecoderConfigDescriptor".to_string(),
            });
        }

        // Get the underlying reader back
        let reader = bit_reader.into_inner();
        let dec_config_descr = DecoderConfigDescriptor::parse(reader)?;

        Ok(ESDescriptor { dec_config_descr })
    }
}

/// Decoder Configuration Descriptor (ISO 14496-1, tag 0x04).
///
/// Contains the object type indication (codec identifier), stream type,
/// bitrate information, and an optional [`DecoderSpecificInfo`] with
/// AAC-specific audio configuration.
#[derive(Debug)]
pub struct DecoderConfigDescriptor {
    /// MPEG-4 object type indication (0x40 = AAC, 0x69 = MP3, etc.)
    pub object_type_indication: u8,
    /// Maximum bitrate in bits per second
    pub max_bitrate: u32,
    /// Average bitrate in bits per second
    pub avg_bitrate: u32,
    /// AAC-specific decoder configuration (present when object_type=0x40, stream_type=0x05)
    pub dec_specific_info: Option<DecoderSpecificInfo>,
}

impl BaseDescriptor for DecoderConfigDescriptor {
    const TAG: u8 = 0x4;

    fn new<R: Read + Seek>(
        reader: &mut R,
        length: u32,
    ) -> std::result::Result<Self, DescriptorError> {
        let mut bit_reader = BitReader::new(reader)?;

        let object_type_indication = bit_reader.bits(8)? as u8;
        let stream_type = bit_reader.bits(6)? as u8;
        let _up_stream = bit_reader.bits(1)?;
        let _reserved = bit_reader.bits(1)?;
        let _buffer_size_db = bit_reader.bits(24)?;
        let max_bitrate = bit_reader.read_bits(32)? as u32;
        let avg_bitrate = bit_reader.read_bits(32)? as u32;

        // Check if this is AAC (object_type_indication=0x40, stream_type=0x5)
        let dec_specific_info = if (object_type_indication, stream_type) == (0x40, 0x5) {
            // Check if we have more data for DecoderSpecificInfo
            if (length as u64 * 8) == bit_reader.get_position() {
                None
            } else {
                let tag = bit_reader.bits(8)? as u8;
                if tag == DecoderSpecificInfo::TAG {
                    if !bit_reader.is_aligned() {
                        return Err(DescriptorError {
                            message: "BitReader not aligned before DecoderSpecificInfo".to_string(),
                        });
                    }

                    // Get the underlying reader back
                    let reader = bit_reader.into_inner();
                    Some(DecoderSpecificInfo::parse(reader)?)
                } else {
                    None
                }
            }
        } else {
            None
        };

        Ok(DecoderConfigDescriptor {
            object_type_indication,
            max_bitrate,
            avg_bitrate,
            dec_specific_info,
        })
    }
}

impl DecoderConfigDescriptor {
    /// Get codec parameter string
    pub fn codec_param(&self) -> String {
        let mut param = format!(".{:X}", self.object_type_indication);
        if let Some(info) = &self.dec_specific_info {
            param.push_str(&format!(".{}", info.audio_object_type));
        }
        param
    }

    /// Get codec description
    pub fn codec_desc(&self) -> Option<String> {
        self.dec_specific_info
            .as_ref()
            .and_then(|info| info.description())
    }
}

/// AAC Decoder Specific Info (ISO 14496-3, tag 0x05).
///
/// Contains the AudioSpecificConfig parsed from the elementary stream,
/// including audio object type (AAC-LC, HE-AAC, etc.), sampling frequency,
/// channel layout, and SBR/PS extension signaling.
#[derive(Debug)]
pub struct DecoderSpecificInfo {
    /// AAC audio object type (1=MAIN, 2=LC, 5=SBR, 29=PS, etc.)
    pub audio_object_type: u32,
    /// Base sampling frequency in Hz
    pub sampling_frequency: u32,
    /// Channel configuration index (1=mono, 2=stereo, etc.)
    pub channel_configuration: u8,
    /// SBR (Spectral Band Replication) presence: -1=unknown, 0=absent, 1=present
    pub sbr_present_flag: i8,
    /// PS (Parametric Stereo) presence: -1=unknown, 0=absent, 1=present
    pub ps_present_flag: i8,
    /// Extension sampling frequency (doubled rate when SBR is present)
    pub extension_sampling_frequency: Option<u32>,
    /// Extension channel configuration (for ER AAC ELD)
    pub extension_channel_configuration: Option<u8>,
    /// Channel count from ProgramConfigElement (when channel_configuration=0)
    pub pce_channels: Option<u8>,
}

impl BaseDescriptor for DecoderSpecificInfo {
    const TAG: u8 = 0x5;

    fn new<R: Read + Seek>(
        reader: &mut R,
        length: u32,
    ) -> std::result::Result<Self, DescriptorError> {
        let mut bit_reader = BitReader::new(reader)?;
        let info = DecoderSpecificInfo::parse_internal(&mut bit_reader, length)?;
        Ok(info)
    }
}

impl DecoderSpecificInfo {
    /// AAC audio object type names
    const TYPE_NAMES: &'static [Option<&'static str>] = &[
        None,
        Some("AAC MAIN"),
        Some("AAC LC"),
        Some("AAC SSR"),
        Some("AAC LTP"),
        Some("SBR"),
        Some("AAC scalable"),
        Some("TwinVQ"),
        Some("CELP"),
        Some("HVXC"),
        None,
        None,
        Some("TTSI"),
        Some("Main synthetic"),
        Some("Wavetable synthesis"),
        Some("General MIDI"),
        Some("Algorithmic Synthesis and Audio FX"),
        Some("ER AAC LC"),
        None,
        Some("ER AAC LTP"),
        Some("ER AAC scalable"),
        Some("ER Twin VQ"),
        Some("ER BSAC"),
        Some("ER AAC LD"),
        Some("ER CELP"),
        Some("ER HVXC"),
        Some("ER HILN"),
        Some("ER Parametric"),
        Some("SSC"),
        Some("PS"),
        Some("MPEG Surround"),
        None,
        Some("Layer-1"),
        Some("Layer-2"),
        Some("Layer-3"),
        Some("DST"),
        Some("ALS"),
        Some("SLS"),
        Some("SLS non-core"),
        Some("ER AAC ELD"),
        Some("SMR Simple"),
        Some("SMR Main"),
        Some("USAC"),
        Some("SAOC"),
        Some("LD MPEG Surround"),
        Some("USAC"),
    ];

    /// AAC sampling frequencies
    const FREQS: &'static [u32] = &[
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ];

    /// Get audio object type description with SBR/PS extensions
    pub fn description(&self) -> Option<String> {
        let name = if (self.audio_object_type as usize) < Self::TYPE_NAMES.len() {
            Self::TYPE_NAMES[self.audio_object_type as usize]?
        } else {
            return None;
        };

        let mut desc = name.to_string();
        if self.sbr_present_flag == 1 {
            desc.push_str("+SBR");
        }
        if self.ps_present_flag == 1 {
            desc.push_str("+PS");
        }
        Some(desc)
    }

    /// Get effective sample rate considering SBR
    pub fn sample_rate(&self) -> u32 {
        if self.sbr_present_flag == 1 {
            self.extension_sampling_frequency.unwrap_or(0)
        } else if self.sbr_present_flag == 0 {
            self.sampling_frequency
        } else {
            // Check if audio object type can have SBR
            let aot_can_sbr = [1, 2, 3, 4, 6, 17, 19, 20, 22];
            if !aot_can_sbr.contains(&{ self.audio_object_type }) {
                return self.sampling_frequency;
            }
            // No SBR for > 24KHz
            if self.sampling_frequency > 24000 {
                return self.sampling_frequency;
            }
            // Ambiguous case - could be samplingFrequency or samplingFrequency * 2
            0
        }
    }

    /// Get effective channel count
    pub fn channels(&self) -> u32 {
        // Use program config element channels if available
        if let Some(pce_channels) = self.pce_channels {
            return pce_channels as u32;
        }

        let conf = self
            .extension_channel_configuration
            .unwrap_or(self.channel_configuration);

        match conf {
            1 => {
                if self.ps_present_flag == -1 {
                    0 // Unknown
                } else if self.ps_present_flag == 1 {
                    2 // PS converts mono to stereo
                } else {
                    1 // Mono
                }
            }
            7 => 8,
            c if c > 7 => 0, // Unknown
            _ => conf as u32,
        }
    }

    /// Get audio object type with potential extension
    fn get_audio_object_type(
        r: &mut BitReader<impl Read + Seek>,
    ) -> std::result::Result<u32, BitReaderError> {
        let audio_object_type = r.bits(5)? as u32;
        if audio_object_type == 31 {
            let audio_object_type_ext = r.bits(6)? as u32;
            Ok(32 + audio_object_type_ext)
        } else {
            Ok(audio_object_type)
        }
    }

    /// Get sampling frequency from index or explicit value
    fn get_sampling_freq(
        r: &mut BitReader<impl Read + Seek>,
    ) -> std::result::Result<u32, BitReaderError> {
        let sampling_frequency_index = r.bits(4)? as usize;
        if sampling_frequency_index == 0xf {
            Ok(r.bits(24)? as u32)
        } else if sampling_frequency_index < Self::FREQS.len() {
            Ok(Self::FREQS[sampling_frequency_index])
        } else {
            Ok(0) // Unknown frequency
        }
    }

    /// Internal parsing implementation
    fn parse_internal<R: Read + Seek>(
        r: &mut BitReader<R>,
        length: u32,
    ) -> std::result::Result<DecoderSpecificInfo, DescriptorError> {
        let audio_object_type = Self::get_audio_object_type(r)?;
        let sampling_frequency = Self::get_sampling_freq(r)?;
        let channel_configuration = r.bits(4)? as u8;

        let mut sbr_present_flag = -1i8;
        let mut ps_present_flag = -1i8;
        let mut extension_audio_object_type = 0u32;
        let mut extension_sampling_frequency = None;
        let mut extension_channel_configuration = None;
        let mut pce_channels = None;

        let mut final_audio_object_type = audio_object_type;

        // Handle explicit SBR/PS signaling
        if audio_object_type == 5 || audio_object_type == 29 {
            extension_audio_object_type = 5;
            sbr_present_flag = 1;
            if audio_object_type == 29 {
                ps_present_flag = 1;
            }
            extension_sampling_frequency = Some(Self::get_sampling_freq(r)?);
            let new_audio_object_type = Self::get_audio_object_type(r)?;

            if new_audio_object_type == 22 {
                extension_channel_configuration = Some(r.bits(4)? as u8);
            }

            // Continue with the new audio object type for GASpecificConfig
            final_audio_object_type = new_audio_object_type;
        }

        // Parse GASpecificConfig for supported audio object types
        if [1, 2, 3, 4, 6, 7, 17, 19, 20, 21, 22, 23].contains(&final_audio_object_type) {
            let result = ga_specific_config(r, channel_configuration, final_audio_object_type);
            match result {
                Ok(pce) => pce_channels = pce,
                Err(_) => {
                    // Unsupported configuration, return basic info
                    return Ok(DecoderSpecificInfo {
                        audio_object_type: final_audio_object_type,
                        sampling_frequency,
                        channel_configuration,
                        sbr_present_flag,
                        ps_present_flag,
                        extension_sampling_frequency,
                        extension_channel_configuration,
                        pce_channels,
                    });
                }
            }
        } else {
            // Unsupported audio object type
            return Ok(DecoderSpecificInfo {
                audio_object_type: final_audio_object_type,
                sampling_frequency,
                channel_configuration,
                sbr_present_flag,
                ps_present_flag,
                extension_sampling_frequency,
                extension_channel_configuration,
                pce_channels,
            });
        }

        // Handle error resilience
        if [17, 19, 20, 21, 22, 23, 24, 25, 26, 27, 39].contains(&final_audio_object_type) {
            let ep_config = r.bits(2)?;
            if ep_config == 2 || ep_config == 3 {
                // Unsupported error resilience configuration
                return Ok(DecoderSpecificInfo {
                    audio_object_type: final_audio_object_type,
                    sampling_frequency,
                    channel_configuration,
                    sbr_present_flag,
                    ps_present_flag,
                    extension_sampling_frequency,
                    extension_channel_configuration,
                    pce_channels,
                });
            }
        }

        // Handle implicit SBR/PS signaling
        // Use u64 arithmetic to prevent overflow for large descriptor lengths
        let bits_left = (length as u64 * 8).saturating_sub(r.get_position());
        if extension_audio_object_type != 5 && bits_left >= 16 {
            let sync_extension_type = r.bits(11)? as u32;
            if sync_extension_type == 0x2b7 {
                extension_audio_object_type = Self::get_audio_object_type(r)?;

                if extension_audio_object_type == 5 {
                    sbr_present_flag = r.bits(1)? as i8;
                    if sbr_present_flag == 1 {
                        extension_sampling_frequency = Some(Self::get_sampling_freq(r)?);
                        let bits_left = (length as u64 * 8).saturating_sub(r.get_position());
                        if bits_left >= 12 {
                            let sync_extension_type = r.bits(11)? as u32;
                            if sync_extension_type == 0x548 {
                                ps_present_flag = r.bits(1)? as i8;
                            }
                        }
                    }
                }

                if extension_audio_object_type == 22 {
                    sbr_present_flag = r.bits(1)? as i8;
                    if sbr_present_flag == 1 {
                        extension_sampling_frequency = Some(Self::get_sampling_freq(r)?);
                    }
                    extension_channel_configuration = Some(r.bits(4)? as u8);
                }
            }
        }

        Ok(DecoderSpecificInfo {
            audio_object_type: final_audio_object_type,
            sampling_frequency,
            channel_configuration,
            sbr_present_flag,
            ps_present_flag,
            extension_sampling_frequency,
            extension_channel_configuration,
            pce_channels,
        })
    }
}

/// Parse ProgramConfigElement and return channel count
/// This is a specialized implementation that works with the util.rs BitReader
fn parse_program_config_element<R: Read + Seek>(
    r: &mut BitReader<R>,
) -> std::result::Result<u8, DescriptorError> {
    // Parse program_config_element() structure
    let _element_instance_tag = r.bits(4)?; // 4 bits
    let _object_type = r.bits(2)?; // 2 bits
    let _sampling_frequency_index = r.bits(4)?; // 4 bits
    let num_front_channel_elements = r.bits(4)? as u8; // 4 bits
    let num_side_channel_elements = r.bits(4)? as u8; // 4 bits
    let num_back_channel_elements = r.bits(4)? as u8; // 4 bits
    let num_lfe_channel_elements = r.bits(2)? as u8; // 2 bits
    let num_assoc_data_elements = r.bits(3)? as u8; // 3 bits
    let num_valid_cc_elements = r.bits(4)? as u8; // 4 bits

    // Handle mixdown flags
    let mono_mixdown_present = r.bits(1)?; // 1 bit
    if mono_mixdown_present == 1 {
        r.skip(4)?; // mono_mixdown_element_number (4 bits)
    }

    let stereo_mixdown_present = r.bits(1)?; // 1 bit
    if stereo_mixdown_present == 1 {
        r.skip(4)?; // stereo_mixdown_element_number (4 bits)
    }

    let matrix_mixdown_idx_present = r.bits(1)?; // 1 bit
    if matrix_mixdown_idx_present == 1 {
        r.skip(3)?; // matrix_mixdown_idx (3 bits)
    }

    // Use u16 for intermediate arithmetic to prevent overflow from crafted
    // descriptors — the final result is validated before converting to u8
    let elms = num_front_channel_elements as u16
        + num_side_channel_elements as u16
        + num_back_channel_elements as u16;
    let mut channels: u16 = 0;

    // Process front, side, and back channel elements
    for _ in 0..elms {
        channels += 1;
        let element_is_cpe = r.bits(1)?; // 1 bit: 0=SCE (single), 1=CPE (channel pair)
        if element_is_cpe == 1 {
            channels += 1; // CPE adds another channel
        }
        r.skip(4)?; // element_tag_select (4 bits)
    }

    // Add LFE channels
    channels += num_lfe_channel_elements as u16;

    // Skip remaining element arrays.
    // These counts come from narrow bit fields (2-, 3-, and 4-bit respectively),
    // so the maximum products are small (12, 28, 75) and cannot overflow i32.
    // Defensive: assert the upper bounds to guard against future refactoring.
    debug_assert!(
        num_lfe_channel_elements <= 3,
        "2-bit field exceeded expected range"
    );
    debug_assert!(
        num_assoc_data_elements <= 7,
        "3-bit field exceeded expected range"
    );
    debug_assert!(
        num_valid_cc_elements <= 15,
        "4-bit field exceeded expected range"
    );
    r.skip(4 * num_lfe_channel_elements as i32)?; // lfe_element_tag_select
    r.skip(4 * num_assoc_data_elements as i32)?; // assoc_data_element_tag_select
    r.skip(5 * num_valid_cc_elements as i32)?; // cc_element_is_ind_sw + valid_cc_element_tag_select

    // Byte align and read comment field
    r.align(); // align to byte boundary

    let comment_field_bytes = r.bits(8)? as u8; // 8 bits
    let comment_bits = 8 * comment_field_bytes as i32;
    if comment_bits > 0 {
        // Validate we have enough data before skipping to prevent reading
        // past the descriptor boundary with a crafted comment_field_bytes
        if !r.can_read(comment_bits as u32) {
            return Err(DescriptorError {
                message: format!(
                    "comment_field_bytes ({}) exceeds remaining descriptor data",
                    comment_field_bytes
                ),
            });
        }
        r.skip(comment_bits)?; // comment field data
    }

    // Convert back to u8, capping at 255 for malformed descriptors
    Ok(u8::try_from(channels).unwrap_or(u8::MAX))
}

/// Parse GASpecificConfig for AAC configuration
/// Returns channel count from ProgramConfigElement if present
fn ga_specific_config<R: Read + Seek>(
    r: &mut BitReader<R>,
    channel_configuration: u8,
    audio_object_type: u32,
) -> std::result::Result<Option<u8>, DescriptorError> {
    r.skip(1)?; // frameLengthFlag
    let depends_on_core_coder = r.bits(1)?;
    if depends_on_core_coder != 0 {
        r.skip(14)?; // coreCoderDelay
    }
    let extension_flag = r.bits(1)?;

    let mut pce_channels = None;

    // Parse ProgramConfigElement if no channel configuration
    if channel_configuration == 0 {
        match parse_program_config_element(r) {
            Ok(channels) => pce_channels = Some(channels),
            Err(_) => pce_channels = None, // Failed to parse PCE, continue without it
        }
    }

    // Handle layer-specific configurations
    if audio_object_type == 6 || audio_object_type == 20 {
        r.skip(3)?; // layerNr
    }

    if extension_flag != 0 {
        if audio_object_type == 22 {
            r.skip(5 + 11)?; // numOfSubFrame + layer_length
        }
        if [17, 19, 20, 23].contains(&audio_object_type) {
            r.skip(1 + 1 + 1)?; // aacSectionDataResilienceFlag + aacScalefactorDataResilienceFlag + aacSpectralDataResilienceFlag
        }
        let extension_flag3 = r.bits(1)?;
        if extension_flag3 != 0 {
            return Err(DescriptorError {
                message: "extensionFlag3 set - not supported".to_string(),
            });
        }
    }

    Ok(pce_channels)
}

/// Parsed audio sample entry from an MP4 `stsd` (sample description) atom.
///
/// Represents the audio codec configuration extracted from format-specific
/// sub-atoms (esds for AAC, alac for Apple Lossless, dac3 for AC-3).
#[derive(Debug, Clone, Default)]
pub struct AudioSampleEntry {
    /// Number of audio channels
    pub channels: u16,
    /// Bits per sample
    pub sample_size: u16,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Bitrate in bits per second
    pub bitrate: u32,
    /// Codec identifier string (e.g., "mp4a.40.2" for AAC-LC)
    pub codec: String,
    /// Human-readable codec description (e.g., "AAC LC+SBR")
    pub codec_description: String,
}

impl AudioSampleEntry {
    /// Parse an AudioSampleEntry from atom data.
    ///
    /// Child atoms discovered during parsing have `offset` and `data_offset`
    /// fields relative to the internal data buffer, not the original file.
    /// All child atom data is read from that buffer, so file-absolute offsets
    /// are not needed.
    pub fn parse<R: Read + Seek>(atom: &MP4Atom, reader: &mut R) -> Result<Self> {
        let data = atom.read_data(reader)?;
        let mut cursor = Cursor::new(&data);

        if data.len() < 28 {
            return Err(AudexError::ParseError(format!(
                "too short {:?} atom",
                atom.name
            )));
        }

        // Skip SampleEntry fields (6 bytes reserved + 2 bytes data_ref_index)
        cursor.seek(SeekFrom::Current(8))?;

        // Skip AudioSampleEntry reserved fields (8 bytes)
        cursor.seek(SeekFrom::Current(8))?;

        // Read AudioSampleEntry fields
        let mut buffer = [0u8; 2];
        cursor.read_exact(&mut buffer)?;
        let channels = u16::from_be_bytes(buffer);

        cursor.read_exact(&mut buffer)?;
        let sample_size = u16::from_be_bytes(buffer);

        // Skip pre_defined (2 bytes) and reserved (2 bytes)
        cursor.seek(SeekFrom::Current(4))?;

        let mut buffer = [0u8; 4];
        cursor.read_exact(&mut buffer)?;
        let sample_rate = (u32::from_be_bytes(buffer) >> 16) & 0xFFFF;

        let mut entry = AudioSampleEntry {
            channels,
            sample_size,
            sample_rate,
            bitrate: 0,
            codec: String::from_utf8_lossy(&atom.name).into_owned(),
            codec_description: String::from_utf8_lossy(&atom.name).to_uppercase(),
        };

        // Parse extra descriptor atoms
        while cursor.position() < data.len() as u64 {
            let remaining_data = &data[cursor.position() as usize..];
            if remaining_data.len() < 8 {
                break;
            }

            match MP4Atom::parse(&mut cursor, 1) {
                Ok(extra) => {
                    match (atom.name, extra.name) {
                        ([b'm', b'p', b'4', b'a'], [b'e', b's', b'd', b's']) => {
                            // Read esds data directly from the data slice.
                            // Use checked_add to prevent usize overflow on the bounds check.
                            if extra.data_length > 0 && extra.data_offset < data.len() as u64 {
                                let start_pos = extra.data_offset as usize;
                                if let Some(end_pos) =
                                    start_pos.checked_add(extra.data_length as usize)
                                {
                                    if end_pos <= data.len() {
                                        let esds_data = &data[start_pos..end_pos];
                                        if let Err(_e) = entry.parse_esds_data(esds_data) {
                                            // ESDS parsing failed - non-critical, continue without codec details
                                        }
                                    }
                                }
                            }
                        }
                        ([b'a', b'l', b'a', b'c'], [b'a', b'l', b'a', b'c']) => {
                            // Read alac data directly from the data slice.
                            // Use checked_add to prevent usize overflow on the bounds check.
                            if extra.data_length > 0 && extra.data_offset < data.len() as u64 {
                                let start_pos = extra.data_offset as usize;
                                if let Some(end_pos) =
                                    start_pos.checked_add(extra.data_length as usize)
                                {
                                    if end_pos <= data.len() {
                                        let alac_data = &data[start_pos..end_pos];
                                        let _ = entry.parse_alac_data(alac_data);
                                    }
                                }
                            }
                        }
                        ([b'a', b'c', b'-', b'3'], [b'd', b'a', b'c', b'3']) => {
                            // Read dac3 data directly from the data slice.
                            // Use checked_add to prevent usize overflow on the bounds check.
                            if extra.data_length > 0 && extra.data_offset < data.len() as u64 {
                                let start_pos = extra.data_offset as usize;
                                if let Some(end_pos) =
                                    start_pos.checked_add(extra.data_length as usize)
                                {
                                    if end_pos <= data.len() {
                                        let dac3_data = &data[start_pos..end_pos];
                                        let _ = entry.parse_dac3_data(dac3_data);
                                    }
                                }
                            }
                        }
                        _ => {} // Unknown descriptor, skip
                    }
                }
                Err(_) => break,
            }
        }

        Ok(entry)
    }

    /// Parse ESDS data directly from a byte slice
    fn parse_esds_data(&mut self, data: &[u8]) -> Result<()> {
        let (version, _flags, payload) = parse_full_atom(data)?;
        if version != 0 {
            return Err(AudexError::ParseError(format!(
                "Unsupported esds version {}",
                version
            )));
        }

        // Parse the descriptor hierarchy
        let mut cursor = Cursor::new(payload);

        // Read the tag to identify descriptor type
        let mut tag_buf = [0u8; 1];
        if cursor.read_exact(&mut tag_buf).is_err() {
            return Err(AudexError::ParseError(
                "Failed to read ESDS tag".to_string(),
            ));
        }
        let tag = tag_buf[0];

        if tag != ESDescriptor::TAG {
            return Err(AudexError::ParseError(format!(
                "Expected ES_Descriptor tag 0x{:02X}, got 0x{:02X}",
                ESDescriptor::TAG,
                tag
            )));
        }

        match ESDescriptor::parse(&mut cursor) {
            Ok(es_desc) => {
                // Extract information from the descriptor hierarchy
                let decoder_config = &es_desc.dec_config_descr;

                // Set bitrate information
                if decoder_config.avg_bitrate > 0 {
                    self.bitrate = decoder_config.avg_bitrate;
                } else if decoder_config.max_bitrate > 0 {
                    self.bitrate = decoder_config.max_bitrate;
                }

                // Set codec information with parameters
                self.codec = format!("mp4a{}", decoder_config.codec_param());
                if let Some(codec_desc) = &decoder_config.codec_desc() {
                    self.codec_description = codec_desc.clone();
                } else {
                    self.codec_description = "AAC".to_string();
                }

                // Update sample rate and channels if available from DecoderSpecificInfo
                if let Some(dec_specific) = &decoder_config.dec_specific_info {
                    if dec_specific.sample_rate() > 0 {
                        self.sample_rate = dec_specific.sample_rate();
                    }
                    if dec_specific.channels() > 0 {
                        self.channels = dec_specific.channels() as u16;
                    }
                }
            }
            Err(_e) => {
                // Log through structured tracing, continue with basic info
                warn_event!("Failed to parse ES_Descriptor: {}", _e);
                self.codec_description = "AAC".to_string();
            }
        }

        Ok(())
    }

    /// Parse ALAC data directly from a byte slice
    fn parse_alac_data(&mut self, data: &[u8]) -> Result<()> {
        let (version, _flags, payload) = parse_full_atom(data)?;
        if version != 0 {
            return Err(AudexError::ParseError(format!(
                "Unsupported alac version {}",
                version
            )));
        }

        if payload.len() < 24 {
            return Err(AudexError::ParseError("ALAC atom too short".to_string()));
        }

        // Skip frameLength (4 bytes)
        let mut cursor = Cursor::new(payload);
        cursor.seek(SeekFrom::Current(4))?;

        let mut buffer = [0u8; 1];
        cursor.read_exact(&mut buffer)?;
        let compatible_version = buffer[0];

        if compatible_version != 0 {
            return Ok(()); // Unsupported version
        }

        cursor.read_exact(&mut buffer)?;
        self.sample_size = buffer[0] as u16;

        // Skip some fields (3 bytes)
        cursor.seek(SeekFrom::Current(3))?;

        cursor.read_exact(&mut buffer)?;
        self.channels = buffer[0] as u16;

        // Skip more fields (6 bytes)
        cursor.seek(SeekFrom::Current(6))?;

        let mut buffer = [0u8; 4];
        cursor.read_exact(&mut buffer)?;
        self.bitrate = u32::from_be_bytes(buffer);

        cursor.read_exact(&mut buffer)?;
        self.sample_rate = u32::from_be_bytes(buffer);

        self.codec_description = "ALAC".to_string();

        Ok(())
    }

    /// Parse DAC3 data directly from a byte slice
    fn parse_dac3_data(&mut self, data: &[u8]) -> Result<()> {
        if data.len() < 3 {
            return Err(AudexError::ParseError("DAC3 atom too short".to_string()));
        }

        // Parse AC-3 specific data
        let byte1 = data[0];
        let byte2 = data[1];
        let byte3 = data[2];

        let _fscod = (byte1 >> 6) & 0x03;
        let _bsid = (byte1 >> 1) & 0x1F;
        let _bsmod = ((byte1 & 0x01) << 2) | ((byte2 >> 6) & 0x03);
        let acmod = (byte2 >> 3) & 0x07;
        let lfeon = (byte2 >> 2) & 0x01;
        let bit_rate_code = ((byte2 & 0x03) << 3) | ((byte3 >> 5) & 0x07);

        // Calculate channels from acmod and lfeon
        self.channels = match acmod {
            0 => 2, // Dual mono (1+1)
            1 => 1, // Mono (1/0)
            2 => 2, // Stereo (2/0)
            3 => 3, // 3/0
            4 => 3, // 2/1
            5 => 4, // 3/1
            6 => 4, // 2/2
            7 => 5, // 3/2
            _ => 2, // Default to stereo
        } + lfeon as u16;

        // Calculate bitrate from bit_rate_code
        let bitrates = [
            32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640,
        ];
        if (bit_rate_code as usize) < bitrates.len() {
            self.bitrate = bitrates[bit_rate_code as usize] * 1000;
        }

        self.codec_description = "AC-3".to_string();

        Ok(())
    }
}
