#![cfg(feature = "serde")]

//! ID3 custom serialization tests.
//!
//! Validates the manual Serialize/Deserialize implementation for
//! ID3Tags, which converts to/from SerializableID3Tags.

use audex::id3::tags::{ID3Tags, SerializableID3Tags};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Text frame serialization
// ---------------------------------------------------------------------------

#[test]
fn test_id3_text_frames_serialize() {
    let mut tags = ID3Tags::new();
    let _ = tags.add_text_frame("TIT2", vec!["Test Title".into()]);
    let _ = tags.add_text_frame("TPE1", vec!["Test Artist".into()]);

    let json = serde_json::to_string(&tags).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Verify text frames are present in the serialized output
    let text_frames = parsed["text_frames"].as_object().unwrap();
    assert!(text_frames.contains_key("TIT2"));
    assert!(text_frames.contains_key("TPE1"));
}

#[test]
fn test_id3_roundtrip_text_only() {
    let mut tags = ID3Tags::new();
    let _ = tags.add_text_frame("TIT2", vec!["Round Trip Title".into()]);
    let _ = tags.add_text_frame("TALB", vec!["Round Trip Album".into()]);
    let _ = tags.add_text_frame("TDRC", vec!["2024".into()]);

    let json = serde_json::to_string(&tags).unwrap();
    let deserialized: ID3Tags = serde_json::from_str(&json).unwrap();

    // Text frames should survive the round-trip
    assert_eq!(
        deserialized.get("TIT2"),
        Some(vec!["Round Trip Title".to_string()])
    );
    assert_eq!(
        deserialized.get("TALB"),
        Some(vec!["Round Trip Album".to_string()])
    );
    assert_eq!(deserialized.get("TDRC"), Some(vec!["2024".to_string()]));
}

// ---------------------------------------------------------------------------
// Comment frame serialization
// ---------------------------------------------------------------------------

#[test]
fn test_id3_comment_frame_serialize() {
    use audex::id3::frames::COMM;
    use audex::id3::specs::TextEncoding;

    let mut tags = ID3Tags::new();
    let comm = COMM::new(
        TextEncoding::Utf8,
        *b"eng",
        "".into(),
        "A great song".into(),
    );
    let _ = tags.add(Box::new(comm));

    let serializable = tags.to_serializable();
    assert_eq!(serializable.comment_frames.len(), 1);
    assert_eq!(serializable.comment_frames[0].language, "eng");
    assert_eq!(serializable.comment_frames[0].text, "A great song");
}

// ---------------------------------------------------------------------------
// Picture frame serialization
// ---------------------------------------------------------------------------

#[test]
fn test_id3_picture_frame_serialize() {
    use audex::id3::frames::{APIC, PictureType};
    use audex::id3::specs::TextEncoding;

    let mut tags = ID3Tags::new();
    let apic = APIC {
        encoding: TextEncoding::Utf8,
        mime: "image/jpeg".to_string(),
        type_: PictureType::CoverFront,
        desc: "Front".to_string(),
        data: vec![0xFF, 0xD8, 0xFF, 0xE0],
    };
    let _ = tags.add(Box::new(apic));

    let serializable = tags.to_serializable();
    assert_eq!(serializable.picture_frames.len(), 1);
    assert_eq!(serializable.picture_frames[0].picture_type, "CoverFront");
    assert_eq!(serializable.picture_frames[0].mime_type, "image/jpeg");
    assert_eq!(
        serializable.picture_frames[0].data,
        vec![0xFF, 0xD8, 0xFF, 0xE0]
    );
}

#[test]
fn test_id3_roundtrip_with_pictures() {
    use audex::id3::frames::{APIC, PictureType};
    use audex::id3::specs::TextEncoding;

    let mut tags = ID3Tags::new();
    let _ = tags.add_text_frame("TIT2", vec!["With Picture".into()]);
    let apic = APIC {
        encoding: TextEncoding::Utf8,
        mime: "image/png".to_string(),
        type_: PictureType::CoverFront,
        desc: "Cover".to_string(),
        data: vec![0x89, 0x50, 0x4E, 0x47],
    };
    let _ = tags.add(Box::new(apic));

    let json = serde_json::to_string(&tags).unwrap();
    let deserialized: ID3Tags = serde_json::from_str(&json).unwrap();

    // Text should survive
    assert_eq!(
        deserialized.get("TIT2"),
        Some(vec!["With Picture".to_string()])
    );

    // Picture should survive the round-trip
    let ser = deserialized.to_serializable();
    assert_eq!(ser.picture_frames.len(), 1);
    assert_eq!(ser.picture_frames[0].data, vec![0x89, 0x50, 0x4E, 0x47]);
}

// ---------------------------------------------------------------------------
// URL frames
// ---------------------------------------------------------------------------

#[test]
fn test_id3_url_frames_serialize() {
    use audex::id3::frames::WXXX;

    let mut tags = ID3Tags::new();
    let wxxx = WXXX::new("homepage".into(), "https://example.com".into());
    let _ = tags.add(Box::new(wxxx));

    let serializable = tags.to_serializable();
    assert_eq!(
        serializable.url_frames.get("homepage"),
        Some(&"https://example.com".to_string())
    );
}

// ---------------------------------------------------------------------------
// SerializableID3Tags direct construction
// ---------------------------------------------------------------------------

#[test]
fn test_serializable_id3_to_from() {
    let s = SerializableID3Tags {
        version: (2, 4), // ID3v2.4
        text_frames: {
            let mut m = HashMap::new();
            m.insert("TIT2".into(), vec!["Direct Title".into()]);
            m
        },
        user_text_frames: HashMap::new(),
        comment_frames: vec![],
        picture_frames: vec![],
        url_frames: HashMap::new(),
        unknown_frame_ids: vec![],
    };

    let tags = ID3Tags::from_serializable(s);

    // Verify version was preserved
    assert_eq!(tags.version(), (2, 4));

    // Verify text frame was added by checking the ID3Tags-specific get()
    let title = tags.get("TIT2");
    assert_eq!(title, Some(vec!["Direct Title".to_string()]));
}

// ---------------------------------------------------------------------------
// Unknown frames are listed
// ---------------------------------------------------------------------------

#[test]
fn test_id3_unknown_frames_listed() {
    // An empty ID3Tags should produce no unknown frames
    let tags = ID3Tags::new();
    let serializable = tags.to_serializable();
    assert!(serializable.unknown_frame_ids.is_empty());
}
