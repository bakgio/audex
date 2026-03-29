#![cfg(feature = "serde")]

//! Enum serialization tests.
//!
//! Validates that all public enums with serde derives produce
//! stable, readable JSON representations and round-trip correctly.

use audex::apev2::APEValueType;
use audex::asf::attrs::{ASFAttribute, ASFPictureType, ASFUnicodeAttribute};
use audex::id3::frames::PictureType;
use audex::mp3::util::BitrateMode;
use audex::mp3::{ChannelMode, Emphasis, MPEGLayer, MPEGVersion};
use audex::mp4::AtomDataType;

// ---------------------------------------------------------------------------
// ID3 PictureType
// ---------------------------------------------------------------------------

#[test]
fn test_picture_type_serialize() {
    let pt = PictureType::CoverFront;
    let json = serde_json::to_string(&pt).unwrap();
    let deserialized: PictureType = serde_json::from_str(&json).unwrap();
    assert_eq!(pt, deserialized);
}

// ---------------------------------------------------------------------------
// MPEG enums
// ---------------------------------------------------------------------------

#[test]
fn test_mpeg_version_serialize() {
    for version in [MPEGVersion::MPEG1, MPEGVersion::MPEG2, MPEGVersion::MPEG25] {
        let json = serde_json::to_string(&version).unwrap();
        let deserialized: MPEGVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(version, deserialized);
    }
}

#[test]
fn test_mpeg_layer_serialize() {
    for layer in [MPEGLayer::Layer1, MPEGLayer::Layer2, MPEGLayer::Layer3] {
        let json = serde_json::to_string(&layer).unwrap();
        let deserialized: MPEGLayer = serde_json::from_str(&json).unwrap();
        assert_eq!(layer, deserialized);
    }
}

#[test]
fn test_channel_mode_serialize() {
    for mode in [
        ChannelMode::Stereo,
        ChannelMode::JointStereo,
        ChannelMode::DualChannel,
        ChannelMode::Mono,
    ] {
        let json = serde_json::to_string(&mode).unwrap();
        let deserialized: ChannelMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deserialized);
    }
}

#[test]
fn test_emphasis_serialize() {
    for emp in [
        Emphasis::None,
        Emphasis::MS50_15,
        Emphasis::Reserved,
        Emphasis::CCITT,
    ] {
        let json = serde_json::to_string(&emp).unwrap();
        let deserialized: Emphasis = serde_json::from_str(&json).unwrap();
        assert_eq!(emp, deserialized);
    }
}

#[test]
fn test_bitrate_mode_serialize() {
    for mode in [
        BitrateMode::Unknown,
        BitrateMode::CBR,
        BitrateMode::VBR,
        BitrateMode::ABR,
    ] {
        let json = serde_json::to_string(&mode).unwrap();
        let deserialized: BitrateMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deserialized);
    }
}

// ---------------------------------------------------------------------------
// MP4 AtomDataType
// ---------------------------------------------------------------------------

#[test]
fn test_atom_data_type_serialize() {
    for adt in [
        AtomDataType::Implicit,
        AtomDataType::Utf8,
        AtomDataType::Jpeg,
        AtomDataType::Png,
        AtomDataType::Integer,
    ] {
        let json = serde_json::to_string(&adt).unwrap();
        let deserialized: AtomDataType = serde_json::from_str(&json).unwrap();
        assert_eq!(adt, deserialized);
    }
}

// ---------------------------------------------------------------------------
// APE APEValueType
// ---------------------------------------------------------------------------

#[test]
fn test_ape_value_type_serialize() {
    for vt in [
        APEValueType::Text,
        APEValueType::Binary,
        APEValueType::External,
    ] {
        let json = serde_json::to_string(&vt).unwrap();
        let deserialized: APEValueType = serde_json::from_str(&json).unwrap();
        assert_eq!(vt, deserialized);
    }
}

// ---------------------------------------------------------------------------
// ASF types
// ---------------------------------------------------------------------------

#[test]
fn test_asf_picture_type_serialize() {
    let pt = ASFPictureType::FrontCover;
    let json = serde_json::to_string(&pt).unwrap();
    let deserialized: ASFPictureType = serde_json::from_str(&json).unwrap();
    assert_eq!(pt, deserialized);
}

#[test]
fn test_asf_attribute_variants() {
    let attr = ASFAttribute::Unicode(ASFUnicodeAttribute::new("test value".to_string()));
    let json = serde_json::to_string(&attr).unwrap();
    let deserialized: ASFAttribute = serde_json::from_str(&json).unwrap();
    assert_eq!(attr, deserialized);
}
