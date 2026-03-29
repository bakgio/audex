//! ASF low-level utilities, error types, and GUID constants
//!
//! Provides the error types ([`ASFError`], [`ASFUtilError`]), attribute type
//! constants for the Extended Content Description Object, and the well-known
//! GUID constants ([`ASFGUIDs`]) used to identify ASF object types.

use crate::{AudexError, Result};
use std::fmt;
use thiserror::Error;

/// ASF-specific error types for parsing and validation failures
#[derive(Debug, Clone)]
pub enum ASFError {
    /// Data within an ASF object is invalid or malformed
    InvalidData(String),
    /// The ASF Header Object is invalid or missing required sub-objects
    InvalidHeader(String),
    /// File ended unexpectedly during parsing
    Truncated,
    /// An ASF object or feature is not supported by this library
    UnsupportedFormat(String),
}

impl fmt::Display for ASFError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ASFError::InvalidData(msg) => write!(f, "Invalid ASF data: {}", msg),
            ASFError::InvalidHeader(msg) => write!(f, "Invalid ASF header: {}", msg),
            ASFError::Truncated => write!(f, "Truncated ASF file"),
            ASFError::UnsupportedFormat(msg) => write!(f, "Unsupported ASF format: {}", msg),
        }
    }
}

impl std::error::Error for ASFError {}

/// Comprehensive ASF utility error types
#[derive(Debug, Error)]
pub enum ASFUtilError {
    #[error("Invalid GUID format: {0}")]
    InvalidGuid(String),

    #[error("Invalid integer format: {0}")]
    InvalidInteger(String),

    #[error("Invalid boolean format: {0}")]
    InvalidBoolean(String),

    #[error("Invalid byte array format: {0}")]
    InvalidByteArray(String),

    #[error("Invalid Unicode string: {0}")]
    InvalidUnicode(String),

    #[error("Codec not found for GUID: {guid:?}")]
    CodecNotFound { guid: [u8; 16] },

    #[error("ASF parsing error: {0}")]
    ParseError(String),
}

/// ASF attribute type constants (Extended Content Description value types)
///
/// These constants identify the data type of attribute values stored in the
/// ASF Extended Content Description Object.
pub const UNICODE: u16 = 0x0000;
/// Byte array attribute type
pub const BYTEARRAY: u16 = 0x0001;
/// Boolean (32-bit) attribute type
pub const BOOL: u16 = 0x0002;
/// 32-bit unsigned integer attribute type
pub const DWORD: u16 = 0x0003;
/// 64-bit unsigned integer attribute type
pub const QWORD: u16 = 0x0004;
/// 16-bit unsigned integer attribute type
pub const WORD: u16 = 0x0005;
/// 128-bit GUID attribute type
pub const GUID: u16 = 0x0006;

/// Well-known GUIDs for identifying ASF object types
///
/// Every ASF object begins with a 16-byte GUID that identifies its type.
/// This struct provides the standard GUIDs defined in the ASF specification
/// as associated constants.
pub struct ASFGUIDs;

impl ASFGUIDs {
    // Header Object GUID: 75B22630-668E-11CF-A6D9-00AA0062CE6C
    pub const HEADER: [u8; 16] = [
        0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62, 0xCE,
        0x6C,
    ];

    // Header Object GUID (alias for consistency)
    pub const HEADER_OBJECT: [u8; 16] = Self::HEADER;

    // Content Description Object GUID: 75B22633-668E-11CF-A6D9-00AA0062CE6C
    pub const CONTENT_DESCRIPTION: [u8; 16] = [
        0x33, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62, 0xCE,
        0x6C,
    ];

    // Extended Content Description Object GUID: D2D0A440-E307-11D2-97F0-00A0C95EA850
    pub const EXTENDED_CONTENT_DESCRIPTION: [u8; 16] = [
        0x40, 0xA4, 0xD0, 0xD2, 0x07, 0xE3, 0xD2, 0x11, 0x97, 0xF0, 0x00, 0xA0, 0xC9, 0x5E, 0xA8,
        0x50,
    ];

    // File Properties Object GUID: 8CABDCA1-A947-11CF-8EE4-00C00C205365
    pub const FILE_PROPERTIES: [u8; 16] = [
        0xA1, 0xDC, 0xAB, 0x8C, 0x47, 0xA9, 0xCF, 0x11, 0x8E, 0xE4, 0x00, 0xC0, 0x0C, 0x20, 0x53,
        0x65,
    ];

    // Stream Properties Object GUID: B7DC0791-A9B7-11CF-8EE6-00C00C205365
    pub const STREAM_PROPERTIES: [u8; 16] = [
        0x91, 0x07, 0xDC, 0xB7, 0xB7, 0xA9, 0xCF, 0x11, 0x8E, 0xE6, 0x00, 0xC0, 0x0C, 0x20, 0x53,
        0x65,
    ];

    // Codec List Object GUID: 86D15240-311D-11D0-A3A4-00A0C90348F6
    pub const CODEC_LIST: [u8; 16] = [
        0x40, 0x52, 0xD1, 0x86, 0x1D, 0x31, 0xD0, 0x11, 0xA3, 0xA4, 0x00, 0xA0, 0xC9, 0x03, 0x48,
        0xF6,
    ];

    // Padding Object GUID: 1806D474-CADF-4509-A4BA-9AABCB96AAE8
    pub const PADDING: [u8; 16] = [
        0x74, 0xD4, 0x06, 0x18, 0xDF, 0xCA, 0x09, 0x45, 0xA4, 0xBA, 0x9A, 0xAB, 0xCB, 0x96, 0xAA,
        0xE8,
    ];

    // Stream Bitrate Properties Object GUID: 7BF875CE-468D-11D1-8D82-006097C9A2B2
    pub const STREAM_BITRATE_PROPERTIES: [u8; 16] = [
        0xCE, 0x75, 0xF8, 0x7B, 0x8D, 0x46, 0xD1, 0x11, 0x8D, 0x82, 0x00, 0x60, 0x97, 0xC9, 0xA2,
        0xB2,
    ];

    // Content Encryption Object GUID: 2211B3FB-BD23-11D2-B4B7-00A0C955FC6E
    pub const CONTENT_ENCRYPTION: [u8; 16] = [
        0xFB, 0xB3, 0x11, 0x22, 0x23, 0xBD, 0xD2, 0x11, 0xB4, 0xB7, 0x00, 0xA0, 0xC9, 0x55, 0xFC,
        0x6E,
    ];

    // Extended Content Encryption Object GUID: 298AE614-2622-4C17-B935-DAE07EE9289C
    pub const EXTENDED_CONTENT_ENCRYPTION: [u8; 16] = [
        0x14, 0xE6, 0x8A, 0x29, 0x22, 0x26, 0x17, 0x4C, 0xB9, 0x35, 0xDA, 0xE0, 0x7E, 0xE9, 0x28,
        0x9C,
    ];

    // Header Extension Object GUID: 5FBF03B5-A92E-11CF-8EE3-00C00C205365
    pub const HEADER_EXTENSION: [u8; 16] = [
        0xB5, 0x03, 0xBF, 0x5F, 0x2E, 0xA9, 0xCF, 0x11, 0x8E, 0xE3, 0x00, 0xC0, 0x0C, 0x20, 0x53,
        0x65,
    ];

    // Metadata Object GUID: C5F8CBEA-5BAF-4877-8467-AA8C44FA4CCA
    pub const METADATA: [u8; 16] = [
        0xEA, 0xCB, 0xF8, 0xC5, 0xAF, 0x5B, 0x77, 0x48, 0x84, 0x67, 0xAA, 0x8C, 0x44, 0xFA, 0x4C,
        0xCA,
    ];

    // Metadata Library Object GUID: 44231C94-9498-49D1-A141-1D134E457054
    pub const METADATA_LIBRARY: [u8; 16] = [
        0x94, 0x1C, 0x23, 0x44, 0x98, 0x94, 0xD1, 0x49, 0xA1, 0x41, 0x1D, 0x13, 0x4E, 0x45, 0x70,
        0x54,
    ];

    // Digital Signature Object GUID: 2211B3FC-BD23-11D2-B4B7-00A0C955FC6E
    pub const DIGITAL_SIGNATURE: [u8; 16] = [
        0xFC, 0xB3, 0x11, 0x22, 0x23, 0xBD, 0xD2, 0x11, 0xB4, 0xB7, 0x00, 0xA0, 0xC9, 0x55, 0xFC,
        0x6E,
    ];

    // Extended Stream Properties Object GUID: 14E6A5CB-C672-4332-8399-A96952065B5A
    pub const EXTENDED_STREAM_PROPERTIES: [u8; 16] = [
        0xCB, 0xA5, 0xE6, 0x14, 0x72, 0xC6, 0x32, 0x43, 0x83, 0x99, 0xA9, 0x69, 0x52, 0x06, 0x5B,
        0x5A,
    ];

    // Bitrate Mutual Exclusion Object GUID: D6E229DC-35DA-11D1-9034-00A0C90349BE
    pub const BITRATE_MUTUAL_EXCLUSION: [u8; 16] = [
        0xDC, 0x29, 0xE2, 0xD6, 0xDA, 0x35, 0xD1, 0x11, 0x90, 0x34, 0x00, 0xA0, 0xC9, 0x03, 0x49,
        0xBE,
    ];

    // Stream Type GUIDs
    // Audio Stream GUID: F8699E40-5B4D-11CF-A8FD-00805F5C442B
    pub const AUDIO_STREAM: [u8; 16] = [
        0x40, 0x9E, 0x69, 0xF8, 0x4D, 0x5B, 0xCF, 0x11, 0xA8, 0xFD, 0x00, 0x80, 0x5F, 0x5C, 0x44,
        0x2B,
    ];

    // Video Stream GUID: BC19EFC0-5B4D-11CF-A8FD-00805F5C442B
    pub const VIDEO_STREAM: [u8; 16] = [
        0xC0, 0xEF, 0x19, 0xBC, 0x4D, 0x5B, 0xCF, 0x11, 0xA8, 0xFD, 0x00, 0x80, 0x5F, 0x5C, 0x44,
        0x2B,
    ];

    // Command Stream GUID: 59DACFC0-59E6-11D0-A3AC-00A0C90348F6
    pub const COMMAND_STREAM: [u8; 16] = [
        0xC0, 0xCF, 0xDA, 0x59, 0xE6, 0x59, 0xD0, 0x11, 0xA3, 0xAC, 0x00, 0xA0, 0xC9, 0x03, 0x48,
        0xF6,
    ];

    // JFIF Stream GUID: B61BE100-5B4E-11CF-A8FD-00805F5C442B
    pub const JFIF_STREAM: [u8; 16] = [
        0x00, 0xE1, 0x1B, 0xB6, 0x4E, 0x5B, 0xCF, 0x11, 0xA8, 0xFD, 0x00, 0x80, 0x5F, 0x5C, 0x44,
        0x2B,
    ];

    // Degradable JPEG Stream GUID: 35907DE0-E415-11CF-A917-00805F5C442B
    pub const DEGRADABLE_JPEG_STREAM: [u8; 16] = [
        0xE0, 0x7D, 0x90, 0x35, 0x15, 0xE4, 0xCF, 0x11, 0xA9, 0x17, 0x00, 0x80, 0x5F, 0x5C, 0x44,
        0x2B,
    ];

    // File Transfer Stream GUID: 91BD222C-F21C-497A-8B6D-5AA86BFC0185
    pub const FILE_TRANSFER_STREAM: [u8; 16] = [
        0x2C, 0x22, 0xBD, 0x91, 0x1C, 0xF2, 0x7A, 0x49, 0x8B, 0x6D, 0x5A, 0xA8, 0x6B, 0xFC, 0x01,
        0x85,
    ];

    // Binary Stream GUID: 3AFB65E2-47EF-40F2-AC2C-70A90D71D343
    pub const BINARY_STREAM: [u8; 16] = [
        0xE2, 0x65, 0xFB, 0x3A, 0xEF, 0x47, 0xF2, 0x40, 0xAC, 0x2C, 0x70, 0xA9, 0x0D, 0x71, 0xD3,
        0x43,
    ];
}

/// Complete codec database with comprehensive codec mappings
pub struct ASFCodecs;

impl ASFCodecs {
    /// Get codec information for a given codec ID
    pub fn get_codec_info(codec_id: u16) -> Option<&'static str> {
        Self::get_codec_name(codec_id)
    }

    /// Get codec name for a given codec ID
    pub fn get_codec_name(codec_id: u16) -> Option<&'static str> {
        match codec_id {
            0x0000 => Some("Unknown Wave Format"),
            0x0001 => Some("Microsoft PCM Format"),
            0x0002 => Some("Microsoft ADPCM Format"),
            0x0003 => Some("IEEE Float"),
            0x0004 => Some("Compaq Computer VSELP"),
            0x0005 => Some("IBM CVSD"),
            0x0006 => Some("Microsoft CCITT A-Law"),
            0x0007 => Some("Microsoft CCITT u-Law"),
            0x0008 => Some("Microsoft DTS"),
            0x0009 => Some("Microsoft DRM"),
            0x000A => Some("Windows Media Audio 9 Voice"),
            0x000B => Some("Windows Media Audio 10 Voice"),
            0x000C => Some("OGG Vorbis"),
            0x000D => Some("FLAC"),
            0x000E => Some("MOT AMR"),
            0x000F => Some("Nice Systems IMBE"),
            0x0010 => Some("OKI ADPCM"),
            0x0011 => Some("Intel IMA ADPCM"),
            0x0012 => Some("Videologic MediaSpace ADPCM"),
            0x0013 => Some("Sierra Semiconductor ADPCM"),
            0x0014 => Some("Antex Electronics G.723 ADPCM"),
            0x0015 => Some("DSP Solutions DIGISTD"),
            0x0016 => Some("DSP Solutions DIGIFIX"),
            0x0017 => Some("Dialogic OKI ADPCM"),
            0x0018 => Some("MediaVision ADPCM"),
            0x0019 => Some("Hewlett-Packard CU codec"),
            0x001A => Some("Hewlett-Packard Dynamic Voice"),
            0x0020 => Some("Yamaha ADPCM"),
            0x0021 => Some("Speech Compression SONARC"),
            0x0022 => Some("DSP Group True Speech"),
            0x0023 => Some("Echo Speech EchoSC1"),
            0x0024 => Some("Ahead Inc. Audiofile AF36"),
            0x0025 => Some("Audio Processing Technology APTX"),
            0x0026 => Some("Ahead Inc. AudioFile AF10"),
            0x0027 => Some("Aculab Prosody 1612"),
            0x0028 => Some("Merging Technologies S.A. LRC"),
            0x0030 => Some("Dolby Labs AC2"),
            0x0031 => Some("Microsoft GSM 6.10"),
            0x0032 => Some("Microsoft MSNAudio"),
            0x0033 => Some("Antex Electronics ADPCME"),
            0x0034 => Some("Control Resources VQLPC"),
            0x0035 => Some("DSP Solutions Digireal"),
            0x0036 => Some("DSP Solutions DigiADPCM"),
            0x0037 => Some("Control Resources CR10"),
            0x0038 => Some("Natural MicroSystems VBXADPCM"),
            0x0039 => Some("Crystal Semiconductor IMA ADPCM"),
            0x003A => Some("Echo Speech EchoSC3"),
            0x003B => Some("Rockwell ADPCM"),
            0x003C => Some("Rockwell DigiTalk"),
            0x003D => Some("Xebec Multimedia Solutions"),
            0x0040 => Some("Antex Electronics G.721 ADPCM"),
            0x0041 => Some("Antex Electronics G.728 CELP"),
            0x0042 => Some("Intel G.723"),
            0x0043 => Some("Intel G.723.1"),
            0x0044 => Some("Intel G.729 Audio"),
            0x0045 => Some("Sharp G.726 Audio"),
            0x0050 => Some("Microsoft MPEG-1"),
            0x0052 => Some("InSoft RT24"),
            0x0053 => Some("InSoft PAC"),
            0x0055 => Some("MP3 - MPEG Layer III"),
            0x0059 => Some("Lucent G.723"),
            0x0060 => Some("Cirrus Logic"),
            0x0061 => Some("ESS Technology ESPCM"),
            0x0062 => Some("Voxware File-Mode"),
            0x0063 => Some("Canopus Atrac"),
            0x0064 => Some("APICOM G.726 ADPCM"),
            0x0065 => Some("APICOM G.722 ADPCM"),
            0x0066 => Some("Microsoft DSAT"),
            0x0067 => Some("Microsoft DSAT Display"),
            0x0069 => Some("Voxware Byte Aligned"),
            0x0070 => Some("Voxware AC8"),
            0x0071 => Some("Voxware AC10"),
            0x0072 => Some("Voxware AC16"),
            0x0073 => Some("Voxware AC20"),
            0x0074 => Some("Voxware RT24 MetaVoice"),
            0x0075 => Some("Voxware RT29 MetaSound"),
            0x0076 => Some("Voxware RT29HW"),
            0x0077 => Some("Voxware VR12"),
            0x0078 => Some("Voxware VR18"),
            0x0079 => Some("Voxware TQ40"),
            0x007A => Some("Voxware SC3"),
            0x007B => Some("Voxware SC3"),
            0x0080 => Some("Softsound"),
            0x0081 => Some("Voxware TQ60"),
            0x0082 => Some("Microsoft MSRT24"),
            0x0083 => Some("AT&T Labs G.729A"),
            0x0084 => Some("Motion Pixels MVI MV12"),
            0x0085 => Some("DataFusion Systems G.726"),
            0x0086 => Some("DataFusion Systems GSM610"),
            0x0088 => Some("Iterated Systems ISIAudio"),
            0x0089 => Some("Onlive"),
            0x008A => Some("Multitude FT SX20"),
            0x008B => Some("Infocom ITS ACM G.721"),
            0x008C => Some("Convedia G.729"),
            0x008D => Some("Congruency Audio"),
            0x0091 => Some("Siemens Business Communications SBC24"),
            0x0092 => Some("Sonic Foundry Dolby AC3 SPDIF"),
            0x0093 => Some("MediaSonic G.723"),
            0x0094 => Some("Aculab Prosody 8KBPS"),
            0x0097 => Some("ZyXEL ADPCM"),
            0x0098 => Some("Philips LPCBB"),
            0x0099 => Some("Studer Professional Audio AG Packed"),
            0x00A0 => Some("Malden Electronics PHONYTALK"),
            0x00A1 => Some("Racal Recorder GSM"),
            0x00A2 => Some("Racal Recorder G720.a"),
            0x00A3 => Some("Racal Recorder G723.1"),
            0x00A4 => Some("Racal Recorder Tetra ACELP"),
            0x00B0 => Some("NEC AAC"),
            0x00FF => Some("CoreAAC Audio"),
            0x0100 => Some("Rhetorex ADPCM"),
            0x0101 => Some("BeCubed Software IRAT"),
            0x0111 => Some("Vivo G.723"),
            0x0112 => Some("Vivo Siren"),
            0x0120 => Some("Philips CELP"),
            0x0121 => Some("Philips Grundig"),
            0x0123 => Some("Digital G.723"),
            0x0125 => Some("Sanyo ADPCM"),
            0x0130 => Some("Sipro Lab Telecom ACELP.net"),
            0x0131 => Some("Sipro Lab Telecom ACELP.4800"),
            0x0132 => Some("Sipro Lab Telecom ACELP.8V3"),
            0x0133 => Some("Sipro Lab Telecom ACELP.G.729"),
            0x0134 => Some("Sipro Lab Telecom ACELP.G.729A"),
            0x0135 => Some("Sipro Lab Telecom ACELP.KELVIN"),
            0x0136 => Some("VoiceAge AMR"),
            0x0140 => Some("Dictaphone G.726 ADPCM"),
            0x0141 => Some("Dictaphone CELP68"),
            0x0142 => Some("Dictaphone CELP54"),
            0x0150 => Some("Qualcomm PUREVOICE"),
            0x0151 => Some("Qualcomm HALFRATE"),
            0x0155 => Some("Ring Zero Systems TUBGSM"),
            0x0160 => Some("Windows Media Audio Standard"),
            0x0161 => Some("Windows Media Audio 9 Standard"),
            0x0162 => Some("Windows Media Audio 9 Professional"),
            0x0163 => Some("Windows Media Audio 9 Lossless"),
            0x0164 => Some("Windows Media Audio Pro over SPDIF"),
            0x0170 => Some("Unisys NAP ADPCM"),
            0x0171 => Some("Unisys NAP ULAW"),
            0x0172 => Some("Unisys NAP ALAW"),
            0x0173 => Some("Unisys NAP 16K"),
            0x0174 => Some("Sycom ACM SYC008"),
            0x0175 => Some("Sycom ACM SYC701 G725"),
            0x0176 => Some("Sycom ACM SYC701 CELP54"),
            0x0177 => Some("Sycom ACM SYC701 CELP68"),
            0x0178 => Some("Knowledge Adventure ADPCM"),
            0x0180 => Some("Fraunhofer IIS MPEG-2 AAC"),
            0x0190 => Some("Digital Theater Systems DTS"),
            0x0200 => Some("Creative Labs ADPCM"),
            0x0202 => Some("Creative Labs FastSpeech8"),
            0x0203 => Some("Creative Labs FastSpeech10"),
            0x0210 => Some("UHER informatic GmbH ADPCM"),
            0x0215 => Some("Ulead DV Audio"),
            0x0216 => Some("Ulead DV Audio"),
            0x0220 => Some("Quarterdeck"),
            0x0230 => Some("I-link Worldwide ILINK VC"),
            0x0240 => Some("Aureal Semiconductor RAW SPORT"),
            0x0249 => Some("Generic Passthru"),
            0x0250 => Some("Interactive Products HSX"),
            0x0251 => Some("Interactive Products RPELP"),
            0x0260 => Some("Consistent Software CS2"),
            0x0270 => Some("Sony SCX"),
            0x0271 => Some("Sony SCY"),
            0x0272 => Some("Sony ATRAC3"),
            0x0273 => Some("Sony SPC"),
            0x0280 => Some("Telum Audio"),
            0x0281 => Some("Telum IA Audio"),
            0x0285 => Some("Norcom Voice Systems ADPCM"),
            0x0300 => Some("Fujitsu TOWNS SND"),
            0x0350 => Some("Micronas SC4 Speech"),
            0x0351 => Some("Micronas CELP833"),
            0x0400 => Some("Brooktree BTV Digital"),
            0x0401 => Some("Intel Music Coder"),
            0x0402 => Some("Intel Audio"),
            0x0450 => Some("QDesign Music"),
            0x0500 => Some("On2 AVC0 Audio"),
            0x0501 => Some("On2 AVC1 Audio"),
            0x0680 => Some("AT&T Labs VME VMPCM"),
            0x0681 => Some("AT&T Labs TPC"),
            0x08AE => Some("ClearJump Lightwave Lossless"),
            0x1000 => Some("Olivetti GSM"),
            0x1001 => Some("Olivetti ADPCM"),
            0x1002 => Some("Olivetti CELP"),
            0x1003 => Some("Olivetti SBC"),
            0x1004 => Some("Olivetti OPR"),
            0x1100 => Some("Lernout & Hauspie"),
            0x1101 => Some("Lernout & Hauspie CELP"),
            0x1102 => Some("Lernout & Hauspie SBC8"),
            0x1103 => Some("Lernout & Hauspie SBC12"),
            0x1104 => Some("Lernout & Hauspie SBC16"),
            0x1400 => Some("Norris Communication"),
            0x1401 => Some("ISIAudio"),
            0x1500 => Some("AT&T Labs Soundspace Music Compression"),
            0x1600 => Some("Microsoft MPEG ADTS AAC"),
            0x1601 => Some("Microsoft MPEG RAW AAC"),
            0x1608 => Some("Nokia MPEG ADTS AAC"),
            0x1609 => Some("Nokia MPEG RAW AAC"),
            0x181C => Some("VoxWare MetaVoice RT24"),
            0x1971 => Some("Sonic Foundry Lossless"),
            0x1979 => Some("Innings Telecom ADPCM"),
            0x1FC4 => Some("NTCSoft ALF2CD ACM"),
            0x2000 => Some("Dolby AC3"),
            0x2001 => Some("DTS"),
            0x4143 => Some("Divio AAC"),
            0x4201 => Some("Nokia Adaptive Multi-Rate"),
            0x4243 => Some("Divio G.726"),
            0x4261 => Some("ITU-T H.261"),
            0x4263 => Some("ITU-T H.263"),
            0x4264 => Some("ITU-T H.264"),
            0x674F => Some("Ogg Vorbis Mode 1"),
            0x6750 => Some("Ogg Vorbis Mode 2"),
            0x6751 => Some("Ogg Vorbis Mode 3"),
            0x676F => Some("Ogg Vorbis Mode 1+"),
            0x6770 => Some("Ogg Vorbis Mode 2+"),
            0x6771 => Some("Ogg Vorbis Mode 3+"),
            0x7000 => Some("3COM NBX Audio"),
            0x706D => Some("FAAD AAC Audio"),
            0x77A1 => Some("True Audio Lossless Audio"),
            0x7A21 => Some("GSM-AMR CBR 3GPP Audio"),
            0x7A22 => Some("GSM-AMR VBR 3GPP Audio"),
            0xA100 => Some("Comverse Infosys G723.1"),
            0xA101 => Some("Comverse Infosys AVQSBC"),
            0xA102 => Some("Comverse Infosys SBC"),
            0xA103 => Some("Symbol Technologies G729a"),
            0xA104 => Some("VoiceAge AMR WB"),
            0xA105 => Some("Ingenient Technologies G.726"),
            0xA106 => Some("ISO/MPEG-4 Advanced Audio Coding (AAC)"),
            0xA107 => Some("Encore Software Ltd's G.726"),
            0xA108 => Some("ZOLL Medical Corporation ASAO"),
            0xA109 => Some("Speex Voice"),
            0xA10A => Some("Vianix MASC Speech Compression"),
            0xA10B => Some("Windows Media 9 Spectrum Analyzer Output"),
            0xA10C => Some("Media Foundation Spectrum Analyzer Output"),
            0xA10D => Some("GSM 6.10 (Full-Rate) Speech"),
            0xA10E => Some("GSM 6.20 (Half-Rate) Speech"),
            0xA10F => Some("GSM 6.60 (Enhanced Full-Rate) Speech"),
            0xA110 => Some("GSM 6.90 (Adaptive Multi-Rate) Speech"),
            0xA111 => Some("GSM Adaptive Multi-Rate WideBand Speech"),
            0xA112 => Some("Polycom G.722"),
            0xA113 => Some("Polycom G.728"),
            0xA114 => Some("Polycom G.729a"),
            0xA115 => Some("Polycom Siren"),
            0xA116 => Some("Global IP Sound ILBC"),
            0xA117 => Some("Radio Time Time Shifted Radio"),
            0xA118 => Some("Nice Systems ACA"),
            0xA119 => Some("Nice Systems ADPCM"),
            0xA11A => Some("Vocord Group ITU-T G.721"),
            0xA11B => Some("Vocord Group ITU-T G.726"),
            0xA11C => Some("Vocord Group ITU-T G.722.1"),
            0xA11D => Some("Vocord Group ITU-T G.728"),
            0xA11E => Some("Vocord Group ITU-T G.729"),
            0xA11F => Some("Vocord Group ITU-T G.729a"),
            0xA120 => Some("Vocord Group ITU-T G.723.1"),
            0xA121 => Some("Vocord Group LBC"),
            0xA122 => Some("Nice G.728"),
            0xA123 => Some("France Telecom G.729 ACM Audio"),
            0xA124 => Some("CODIAN Audio"),
            0xCC12 => Some("Intel YUV12 Codec"),
            0xCFCC => Some("Digital Processing Systems Perception Motion JPEG"),
            0xD261 => Some("DEC H.261"),
            0xD263 => Some("DEC H.263"),
            0xFFFE => Some("Extensible Wave Format"),
            0xFFFF => Some("Unregistered"),
            _ => None,
        }
    }

    /// Get codec description.
    pub fn get_codec_description(codec_id: u16) -> Option<&'static str> {
        Self::get_codec_name(codec_id)
    }

    /// Get all supported codec IDs
    pub fn get_all_codec_ids() -> Vec<u16> {
        vec![
            0x0000, 0x0001, 0x0002, 0x0003, 0x0004, 0x0005, 0x0006, 0x0007, 0x0008, 0x0009, 0x000A,
            0x000B, 0x000C, 0x000D, 0x000E, 0x000F, 0x0010, 0x0011, 0x0012, 0x0013, 0x0014, 0x0015,
            0x0016, 0x0017, 0x0018, 0x0019, 0x001A, 0x0020, 0x0021, 0x0022, 0x0023, 0x0024, 0x0025,
            0x0026, 0x0027, 0x0028, 0x0030, 0x0031, 0x0032, 0x0033, 0x0034, 0x0035, 0x0036, 0x0037,
            0x0038, 0x0039, 0x003A, 0x003B, 0x003C, 0x003D, 0x0040, 0x0041, 0x0042, 0x0043, 0x0044,
            0x0045, 0x0050, 0x0052, 0x0053, 0x0055, 0x0059, 0x0060, 0x0061, 0x0062, 0x0063, 0x0064,
            0x0065, 0x0066, 0x0067, 0x0069, 0x0070, 0x0071, 0x0072, 0x0073, 0x0074, 0x0075, 0x0076,
            0x0077, 0x0078, 0x0079, 0x007A, 0x007B, 0x0080, 0x0081, 0x0082, 0x0083, 0x0084, 0x0085,
            0x0086, 0x0088, 0x0089, 0x008A, 0x008B, 0x008C, 0x008D, 0x0091, 0x0092, 0x0093, 0x0094,
            0x0097, 0x0098, 0x0099, 0x00A0, 0x00A1, 0x00A2, 0x00A3, 0x00A4, 0x00B0, 0x00FF, 0x0100,
            0x0101, 0x0111, 0x0112, 0x0120, 0x0121, 0x0123, 0x0125, 0x0130, 0x0131, 0x0132, 0x0133,
            0x0134, 0x0135, 0x0136, 0x0140, 0x0141, 0x0142, 0x0150, 0x0151, 0x0155, 0x0160, 0x0161,
            0x0162, 0x0163, 0x0164, 0x0170, 0x0171, 0x0172, 0x0173, 0x0174, 0x0175, 0x0176, 0x0177,
            0x0178, 0x0180, 0x0190, 0x0200, 0x0202, 0x0203, 0x0210, 0x0215, 0x0216, 0x0220, 0x0230,
            0x0240, 0x0249, 0x0250, 0x0251, 0x0260, 0x0270, 0x0271, 0x0272, 0x0273, 0x0280, 0x0281,
            0x0285, 0x0300, 0x0350, 0x0351, 0x0400, 0x0401, 0x0402, 0x0450, 0x0500, 0x0501, 0x0680,
            0x0681, 0x08AE, 0x1000, 0x1001, 0x1002, 0x1003, 0x1004, 0x1100, 0x1101, 0x1102, 0x1103,
            0x1104, 0x1400, 0x1401, 0x1500, 0x1600, 0x1601, 0x1608, 0x1609, 0x181C, 0x1971, 0x1979,
            0x1FC4, 0x2000, 0x2001, 0x4143, 0x4201, 0x4243, 0x4261, 0x4263, 0x4264, 0x674F, 0x6750,
            0x6751, 0x676F, 0x6770, 0x6771, 0x7000, 0x706D, 0x77A1, 0x7A21, 0x7A22, 0xA100, 0xA101,
            0xA102, 0xA103, 0xA104, 0xA105, 0xA106, 0xA107, 0xA108, 0xA109, 0xA10A, 0xA10B, 0xA10C,
            0xA10D, 0xA10E, 0xA10F, 0xA110, 0xA111, 0xA112, 0xA113, 0xA114, 0xA115, 0xA116, 0xA117,
            0xA118, 0xA119, 0xA11A, 0xA11B, 0xA11C, 0xA11D, 0xA11E, 0xA11F, 0xA120, 0xA121, 0xA122,
            0xA123, 0xA124, 0xCC12, 0xCFCC, 0xD261, 0xD263, 0xFFFE, 0xFFFF,
        ]
    }
}

/// Enhanced ASF value converters
pub struct ASFValueConverters;

impl ASFValueConverters {
    /// Convert string GUID to 16-byte array
    pub fn guid(s: &str) -> std::result::Result<[u8; 16], ASFUtilError> {
        if s.len() != 36 {
            return Err(ASFUtilError::InvalidGuid(format!(
                "Invalid GUID string length: {}",
                s.len()
            )));
        }

        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 5 {
            return Err(ASFUtilError::InvalidGuid(
                "Invalid GUID format - must have 5 parts".to_string(),
            ));
        }

        let mut bytes = [0u8; 16];

        // Parse first part (32-bit, little endian)
        let part1 = u32::from_str_radix(parts[0], 16)
            .map_err(|_| ASFUtilError::InvalidGuid(format!("Invalid GUID part 1: {}", parts[0])))?;
        bytes[0..4].copy_from_slice(&part1.to_le_bytes());

        // Parse second part (16-bit, little endian)
        let part2 = u16::from_str_radix(parts[1], 16)
            .map_err(|_| ASFUtilError::InvalidGuid(format!("Invalid GUID part 2: {}", parts[1])))?;
        bytes[4..6].copy_from_slice(&part2.to_le_bytes());

        // Parse third part (16-bit, little endian)
        let part3 = u16::from_str_radix(parts[2], 16)
            .map_err(|_| ASFUtilError::InvalidGuid(format!("Invalid GUID part 3: {}", parts[2])))?;
        bytes[6..8].copy_from_slice(&part3.to_le_bytes());

        // Parse fourth part (16-bit, big endian)
        let part4 = u16::from_str_radix(parts[3], 16)
            .map_err(|_| ASFUtilError::InvalidGuid(format!("Invalid GUID part 4: {}", parts[3])))?;
        bytes[8..10].copy_from_slice(&part4.to_be_bytes());

        // Parse fifth part (48-bit, big endian)
        let part5 = u64::from_str_radix(parts[4], 16)
            .map_err(|_| ASFUtilError::InvalidGuid(format!("Invalid GUID part 5: {}", parts[4])))?;
        bytes[10..16].copy_from_slice(&part5.to_be_bytes()[2..]);

        Ok(bytes)
    }

    /// Convert to 32-bit value
    pub fn dword(s: &str) -> std::result::Result<u32, ASFUtilError> {
        s.parse::<u32>()
            .map_err(|_| ASFUtilError::InvalidInteger(format!("Invalid DWORD: {}", s)))
    }

    /// Convert to 64-bit value
    pub fn qword(s: &str) -> std::result::Result<u64, ASFUtilError> {
        s.parse::<u64>()
            .map_err(|_| ASFUtilError::InvalidInteger(format!("Invalid QWORD: {}", s)))
    }

    /// Convert to 16-bit value
    pub fn word(s: &str) -> std::result::Result<u16, ASFUtilError> {
        s.parse::<u16>()
            .map_err(|_| ASFUtilError::InvalidInteger(format!("Invalid WORD: {}", s)))
    }

    /// Convert to boolean value
    pub fn bool_value(s: &str) -> std::result::Result<bool, ASFUtilError> {
        match s.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(ASFUtilError::InvalidBoolean(format!(
                "Invalid boolean: {}",
                s
            ))),
        }
    }

    /// Convert to byte array
    pub fn bytearray(s: &str) -> std::result::Result<Vec<u8>, ASFUtilError> {
        if s.starts_with("0x") || s.starts_with("0X") {
            // Parse as hex string
            let hex_str = &s[2..];
            if hex_str.len() % 2 != 0 {
                return Err(ASFUtilError::InvalidByteArray(
                    "Hex string must have even length".to_string(),
                ));
            }

            hex_str
                .chars()
                .collect::<Vec<_>>()
                .chunks(2)
                .map(|chunk| {
                    let hex_pair: String = chunk.iter().collect();
                    u8::from_str_radix(&hex_pair, 16).map_err(|_| {
                        ASFUtilError::InvalidByteArray(format!("Invalid hex pair: {}", hex_pair))
                    })
                })
                .collect::<std::result::Result<Vec<u8>, ASFUtilError>>()
        } else {
            // Parse as comma-separated bytes
            s.split(',')
                .map(|byte_str| {
                    byte_str.trim().parse::<u8>().map_err(|_| {
                        ASFUtilError::InvalidByteArray(format!("Invalid byte: {}", byte_str))
                    })
                })
                .collect::<std::result::Result<Vec<u8>, ASFUtilError>>()
        }
    }

    /// Convert to UTF-16LE unicode
    pub fn unicode(s: &str) -> std::result::Result<String, ASFUtilError> {
        // Return the string as-is; encoding to UTF-16LE happens during serialization
        Ok(s.to_string())
    }

    /// Convert 16-byte GUID array to string
    pub fn guid_to_string(guid: &[u8; 16]) -> String {
        let part1 = u32::from_le_bytes([guid[0], guid[1], guid[2], guid[3]]);
        let part2 = u16::from_le_bytes([guid[4], guid[5]]);
        let part3 = u16::from_le_bytes([guid[6], guid[7]]);
        let part4 = u16::from_be_bytes([guid[8], guid[9]]);
        let part5_bytes = [
            0u8, 0u8, guid[10], guid[11], guid[12], guid[13], guid[14], guid[15],
        ];
        let part5 = u64::from_be_bytes(part5_bytes);

        format!(
            "{:08X}-{:04X}-{:04X}-{:04X}-{:012X}",
            part1, part2, part3, part4, part5
        )
    }

    /// Convert byte slice to GUID array
    pub fn bytes_to_guid(bytes: &[u8]) -> std::result::Result<[u8; 16], ASFUtilError> {
        if bytes.len() < 16 {
            return Err(ASFUtilError::InvalidGuid(
                "GUID bytes too short".to_string(),
            ));
        }
        let mut guid = [0u8; 16];
        guid.copy_from_slice(&bytes[0..16]);
        Ok(guid)
    }
}

/// String formatting utilities for ASF values
pub struct ASFStringUtils;

impl ASFStringUtils {
    /// Format duration from 100-nanosecond units
    pub fn format_duration(duration_100ns: u64) -> String {
        let total_seconds = duration_100ns / 10_000_000;
        let hours = total_seconds / 3600;
        let minutes = (total_seconds % 3600) / 60;
        let seconds = total_seconds % 60;
        let milliseconds = (duration_100ns / 10_000) % 1000;

        if hours > 0 {
            format!(
                "{}:{:02}:{:02}.{:03}",
                hours, minutes, seconds, milliseconds
            )
        } else if minutes > 0 {
            format!("{}:{:02}.{:03}", minutes, seconds, milliseconds)
        } else {
            format!("{}.{:03}", seconds, milliseconds)
        }
    }

    /// Format bitrate in kbps
    pub fn format_bitrate(bitrate: u32) -> String {
        if bitrate >= 1_000_000 {
            format!("{:.1} Mbps", bitrate as f64 / 1_000_000.0)
        } else if bitrate >= 1000 {
            format!("{} kbps", bitrate / 1000)
        } else {
            format!("{} bps", bitrate)
        }
    }

    /// Format file size in bytes
    pub fn format_file_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size_f = size as f64;
        let mut unit_index = 0;

        while size_f >= 1024.0 && unit_index < UNITS.len() - 1 {
            size_f /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size_f, UNITS[unit_index])
        }
    }

    /// Format GUID string
    pub fn format_guid(guid: &[u8; 16]) -> String {
        ASFValueConverters::guid_to_string(guid)
    }

    /// Format GUID string in compact format (without dashes)
    pub fn format_guid_compact(guid: &[u8; 16]) -> String {
        let guid_str = ASFValueConverters::guid_to_string(guid);
        guid_str.replace('-', "")
    }
}

/// Module-level functions for compatibility
/// Convert GUID string to bytes
pub fn guid2bytes(s: &str) -> Result<[u8; 16]> {
    ASFValueConverters::guid(s).map_err(|e| AudexError::InvalidData(e.to_string()))
}

/// Convert bytes to GUID string  
pub fn bytes2guid(bytes: &[u8; 16]) -> String {
    ASFValueConverters::guid_to_string(bytes)
}

/// Utility functions for ASF parsing
pub struct ASFUtil;

impl ASFUtil {
    /// Convert GUID string to bytes
    pub fn guid_to_bytes(guid_str: &str) -> Result<[u8; 16]> {
        guid2bytes(guid_str)
    }

    /// Convert bytes to GUID string
    pub fn bytes_to_guid(bytes: &[u8; 16]) -> String {
        bytes2guid(bytes)
    }

    /// Parse GUID from data
    pub fn parse_guid(data: &[u8]) -> Result<[u8; 16]> {
        ASFValueConverters::bytes_to_guid(data).map_err(|e| AudexError::InvalidData(e.to_string()))
    }

    /// Parse little-endian u16
    pub fn parse_u16_le(data: &[u8]) -> Result<u16> {
        if data.len() < 2 {
            return Err(AudexError::InvalidData(
                "Not enough data for u16".to_string(),
            ));
        }
        Ok(u16::from_le_bytes([data[0], data[1]]))
    }

    /// Parse little-endian u32
    pub fn parse_u32_le(data: &[u8]) -> Result<u32> {
        if data.len() < 4 {
            return Err(AudexError::InvalidData(
                "Not enough data for u32".to_string(),
            ));
        }
        Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Parse little-endian u64
    pub fn parse_u64_le(data: &[u8]) -> Result<u64> {
        if data.len() < 8 {
            return Err(AudexError::InvalidData(
                "Not enough data for u64".to_string(),
            ));
        }
        Ok(u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]))
    }

    /// Parse UTF-16LE string
    pub fn parse_utf16_le(data: &[u8]) -> Result<String> {
        if data.len() % 2 != 0 {
            return Err(AudexError::InvalidData(
                "Invalid UTF-16LE data length".to_string(),
            ));
        }

        let utf16_data: Vec<u16> = data
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();

        // Use lossy conversion to handle real-world files that may contain
        // unpaired surrogate code units or other invalid UTF-16 sequences
        Ok(String::from_utf16_lossy(&utf16_data)
            .trim_end_matches('\0')
            .to_string())
    }

    /// Encode string as UTF-16LE with null terminator
    pub fn encode_utf16_le(s: &str) -> Vec<u8> {
        let utf16: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        utf16
            .iter()
            .flat_map(|&c| c.to_le_bytes().to_vec())
            .collect()
    }

    /// Get codec name for a given codec ID
    pub fn get_codec_name(codec_id: u16) -> Option<&'static str> {
        ASFCodecs::get_codec_name(codec_id)
    }
}
