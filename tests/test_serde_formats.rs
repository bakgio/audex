#![cfg(feature = "serde")]

//! Format-specific tag serialization tests.
//!
//! Validates that VCommentDict, MP4Tags, MP4Cover, MP4FreeForm,
//! APEv2Tags, ASFTags, ASFAttribute variants, and FLAC Picture
//! all round-trip correctly through JSON.

use audex::Tags;
use audex::flac::Picture;
use audex::mp4::{AtomDataType, MP4Cover, MP4FreeForm};
use audex::vorbis::VCommentDict;

// ---------------------------------------------------------------------------
// VCommentDict
// ---------------------------------------------------------------------------

#[test]
fn test_vorbis_comment_json_roundtrip() {
    let mut vc = VCommentDict::new();
    vc.set("TITLE", vec!["Test Song".into()]);
    vc.set("ARTIST", vec!["Test Artist".into()]);

    let json = serde_json::to_string(&vc).unwrap();
    let deserialized: VCommentDict = serde_json::from_str(&json).unwrap();

    assert_eq!(vc.get("TITLE"), deserialized.get("TITLE"));
    assert_eq!(vc.get("ARTIST"), deserialized.get("ARTIST"));
}

// ---------------------------------------------------------------------------
// MP4 types
// ---------------------------------------------------------------------------

#[test]
fn test_mp4_cover_base64_roundtrip() {
    let cover = MP4Cover {
        data: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10],
        imageformat: AtomDataType::Jpeg,
    };

    let json = serde_json::to_string(&cover).unwrap();
    let deserialized: MP4Cover = serde_json::from_str(&json).unwrap();

    assert_eq!(cover.data, deserialized.data);
    assert_eq!(cover.imageformat, deserialized.imageformat);
    // Data should be encoded as base64, not an integer array
    assert!(json.contains("/9j/"));
}

#[test]
fn test_mp4_freeform_roundtrip() {
    let ff = MP4FreeForm {
        data: b"custom value".to_vec(),
        dataformat: AtomDataType::Utf8,
        version: 0,
    };

    let json = serde_json::to_string(&ff).unwrap();
    let deserialized: MP4FreeForm = serde_json::from_str(&json).unwrap();

    assert_eq!(ff.data, deserialized.data);
    assert_eq!(ff.dataformat, deserialized.dataformat);
}

// ---------------------------------------------------------------------------
// APEv2 types
// ---------------------------------------------------------------------------

#[test]
fn test_apev2_value_json_roundtrip() {
    use audex::apev2::{APEValue, APEValueType};

    let text_val = APEValue::text("Hello APE");
    let json = serde_json::to_string(&text_val).unwrap();
    let deserialized: APEValue = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.value_type, APEValueType::Text);
    assert_eq!(text_val.data, deserialized.data);
}

#[test]
fn test_apev2_binary_item_base64() {
    use audex::apev2::APEValue;

    let binary_val = APEValue::binary(vec![0xFF, 0xD8, 0xFF, 0xE0]);
    let json = serde_json::to_string(&binary_val).unwrap();

    // Data should be base64-encoded, not a raw integer array
    assert!(!json.contains("[255"));
    assert!(json.contains("base64") || json.contains("/9j/")); // base64 of JPEG header

    let deserialized: APEValue = serde_json::from_str(&json).unwrap();
    assert_eq!(binary_val.data, deserialized.data);
}

// ---------------------------------------------------------------------------
// ASF types
// ---------------------------------------------------------------------------

#[test]
fn test_asf_attribute_variants_serialize() {
    use audex::asf::attrs::{
        ASFAttribute, ASFBoolAttribute, ASFDWordAttribute, ASFQWordAttribute, ASFUnicodeAttribute,
        ASFWordAttribute,
    };

    let attrs: Vec<ASFAttribute> = vec![
        ASFAttribute::Unicode(ASFUnicodeAttribute::new("test".to_string())),
        ASFAttribute::Bool(ASFBoolAttribute::new(true)),
        ASFAttribute::DWord(ASFDWordAttribute::new(42)),
        ASFAttribute::QWord(ASFQWordAttribute::new(999)),
        ASFAttribute::Word(ASFWordAttribute::new(7)),
    ];

    for attr in &attrs {
        let json = serde_json::to_string(attr).unwrap();
        let deserialized: ASFAttribute = serde_json::from_str(&json).unwrap();
        assert_eq!(*attr, deserialized);
    }
}

// ---------------------------------------------------------------------------
// FLAC Picture
// ---------------------------------------------------------------------------

#[test]
fn test_flac_picture_json_roundtrip() {
    let pic = Picture {
        picture_type: 3, // Front cover
        mime_type: "image/jpeg".to_string(),
        description: "Album Cover".to_string(),
        width: 500,
        height: 500,
        color_depth: 24,
        colors_used: 0,
        data: vec![0xFF, 0xD8, 0xFF, 0xE0],
    };

    let json = serde_json::to_string(&pic).unwrap();
    let deserialized: Picture = serde_json::from_str(&json).unwrap();

    assert_eq!(pic, deserialized);
}

#[test]
fn test_flac_picture_data_base64() {
    let pic = Picture {
        picture_type: 0,
        mime_type: "image/png".to_string(),
        description: String::new(),
        width: 0,
        height: 0,
        color_depth: 0,
        colors_used: 0,
        data: vec![0x89, 0x50, 0x4E, 0x47], // PNG magic
    };

    let json = serde_json::to_string(&pic).unwrap();

    // Data should be base64-encoded, not an integer array
    assert!(!json.contains("[137"));
}
