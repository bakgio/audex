#![cfg(feature = "serde")]

//! TagSnapshot integration tests.
//!
//! Validates that `to_snapshot()` and `to_snapshot_with_raw()` produce
//! correct, round-trippable JSON and TOML output for real audio files.

mod common;

use audex::File;
use audex::snapshot::{StreamInfoSnapshot, TagSnapshot};
use common::TestUtils;

// ---------------------------------------------------------------------------
// Snapshot from real files
// ---------------------------------------------------------------------------

#[test]
fn test_snapshot_from_mp3() {
    let path = TestUtils::data_path("id3v1v2-combined.mp3");
    if !path.exists() {
        return; // Skip if test data not available
    }

    let file = File::load(&path).unwrap();
    let snapshot = file.to_snapshot();

    assert_eq!(snapshot.format, "MP3");
    assert!(snapshot.filename.is_some());
}

#[test]
fn test_snapshot_from_flac() {
    let path = TestUtils::data_path("flac_application.flac");
    if !path.exists() {
        return;
    }

    let file = File::load(&path).unwrap();
    let snapshot = file.to_snapshot();

    assert_eq!(snapshot.format, "FLAC");
}

#[test]
fn test_snapshot_from_mp4() {
    let path = TestUtils::data_path("has-tags.m4a");
    if !path.exists() {
        return;
    }

    let file = File::load(&path).unwrap();
    let snapshot = file.to_snapshot();

    assert_eq!(snapshot.format, "MP4");
}

#[test]
fn test_snapshot_from_ogg() {
    let path = TestUtils::data_path("empty.ogg");
    if !path.exists() {
        return;
    }

    let file = File::load(&path).unwrap();
    let snapshot = file.to_snapshot();

    // Ogg Vorbis format name
    assert!(
        snapshot.format.contains("Ogg") || snapshot.format.contains("Vorbis"),
        "Expected Ogg/Vorbis format, got: {}",
        snapshot.format
    );
}

// ---------------------------------------------------------------------------
// Snapshot JSON structure
// ---------------------------------------------------------------------------

#[test]
fn test_snapshot_json_output_structure() {
    let path = TestUtils::data_path("has-tags.m4a");
    if !path.exists() {
        return;
    }

    let file = File::load(&path).unwrap();
    let snapshot = file.to_snapshot();

    let json = serde_json::to_string_pretty(&snapshot).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    // Verify top-level structure
    assert!(parsed["format"].is_string());
    assert!(parsed["stream_info"].is_object());
    assert!(parsed["tags"].is_object());
}

#[test]
fn test_snapshot_toml_roundtrip() {
    let snapshot = TagSnapshot {
        format: "TEST".to_string(),
        filename: Some("test.mp3".to_string()),
        stream_info: StreamInfoSnapshot {
            length_secs: Some(120.5),
            bitrate: Some(320_000),
            sample_rate: Some(44100),
            channels: Some(2),
            bits_per_sample: Some(16),
        },
        tags: {
            let mut m = std::collections::HashMap::new();
            m.insert("artist".into(), vec!["Test Artist".into()]);
            m.insert("title".into(), vec!["Test Title".into()]);
            m
        },
        raw_tags: None, // serde_json::Value is not TOML-compatible, skip it
    };

    let toml_str = toml::to_string(&snapshot).unwrap();
    let deserialized: TagSnapshot = toml::from_str(&toml_str).unwrap();

    assert_eq!(deserialized.format, "TEST");
    assert_eq!(deserialized.stream_info.bitrate, Some(320_000));
    assert_eq!(
        deserialized.tags.get("artist"),
        Some(&vec!["Test Artist".to_string()])
    );
}

// ---------------------------------------------------------------------------
// Null fields omitted
// ---------------------------------------------------------------------------

#[test]
fn test_snapshot_null_fields_omitted() {
    let snapshot = TagSnapshot {
        format: "TEST".to_string(),
        filename: None,
        stream_info: StreamInfoSnapshot {
            length_secs: None,
            bitrate: None,
            sample_rate: None,
            channels: None,
            bits_per_sample: None,
        },
        tags: std::collections::HashMap::new(),
        raw_tags: None,
    };

    let json = serde_json::to_string(&snapshot).unwrap();

    // Optional fields set to None with skip_serializing_if should be absent
    assert!(!json.contains("filename"));
    assert!(!json.contains("raw_tags"));
}

// ---------------------------------------------------------------------------
// Stream info duration as f64
// ---------------------------------------------------------------------------

#[test]
fn test_snapshot_stream_info_duration_f64() {
    let snap = StreamInfoSnapshot {
        length_secs: Some(301.44),
        bitrate: None,
        sample_rate: None,
        channels: None,
        bits_per_sample: None,
    };

    let json = serde_json::to_string(&snap).unwrap();
    // Should contain the raw f64 value
    assert!(json.contains("301.44"));
}

// ---------------------------------------------------------------------------
// Snapshot with raw tags
// ---------------------------------------------------------------------------

#[test]
fn test_snapshot_with_raw_mp3() {
    let path = TestUtils::data_path("id3v1v2-combined.mp3");
    if !path.exists() {
        return;
    }

    let file = File::load(&path).unwrap();
    let snapshot = file.to_snapshot_with_raw();

    // MP3 files with ID3Tags should produce raw_tags
    assert!(snapshot.raw_tags.is_some());
}

#[test]
fn test_snapshot_with_raw_flac() {
    let path = TestUtils::data_path("flac_application.flac");
    if !path.exists() {
        return;
    }

    let file = File::load(&path).unwrap();
    let snapshot = file.to_snapshot_with_raw();

    // FLAC files with VCommentDict should produce raw_tags
    if !file.keys().is_empty() {
        assert!(snapshot.raw_tags.is_some());
    }
}

#[test]
fn test_snapshot_stream_info_from_real_file() {
    let path = TestUtils::data_path("silence-44-s.flac");
    if !path.exists() {
        return;
    }

    let file = File::load(&path).unwrap();
    let snapshot = file.to_snapshot();
    let si = &snapshot.stream_info;

    assert_eq!(si.sample_rate, Some(44100), "FLAC fixture is 44.1 kHz");
    assert!(si.channels.is_some(), "channels should be populated");
    assert!(
        si.bits_per_sample.is_some(),
        "bits_per_sample should be populated"
    );
    assert!(si.length_secs.is_some(), "length should be populated");
}

#[test]
fn test_snapshot_json_roundtrip() {
    let path = TestUtils::data_path("has-tags.m4a");
    if !path.exists() {
        return;
    }

    let file = File::load(&path).unwrap();
    let original = file.to_snapshot();

    let json = serde_json::to_string_pretty(&original).unwrap();
    let deserialized: TagSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(original.format, deserialized.format);
    assert_eq!(
        original.stream_info.sample_rate,
        deserialized.stream_info.sample_rate
    );
    assert_eq!(
        original.stream_info.channels,
        deserialized.stream_info.channels
    );
    assert_eq!(original.tags, deserialized.tags);
}
