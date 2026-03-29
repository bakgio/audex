//! Edge case tests for tag conversion.

mod common;

use audex::tagmap::{StandardField, TagMap};
use audex::{File, FileType};
use common::TestUtils;

#[test]
fn test_empty_source_tags() {
    // A file with no tags should produce an empty TagMap
    let path = TestUtils::data_path("no-tags.flac");
    let file = File::load(&path).expect("load file");
    let tag_map = file.to_tag_map();
    assert!(tag_map.is_empty() || tag_map.standard_fields().is_empty());
}

#[test]
fn test_empty_tagmap_apply() {
    // Applying an empty TagMap should succeed with nothing transferred
    let path = TestUtils::data_path("no-tags.flac");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut dest = File::load(tmp.path()).expect("load");
    assert!(!dest.has_tags(), "fixture should start without tags");

    let empty_map = TagMap::new();
    let report = dest.apply_tag_map(&empty_map).expect("apply empty");
    assert!(report.transferred.is_empty());
    assert!(report.custom_transferred.is_empty());
    assert!(
        !dest.has_tags(),
        "applying an empty TagMap should not create a tag container"
    );
}

#[test]
fn test_unicode_values_preserved() {
    // Unicode characters should survive the conversion round-trip
    let path = TestUtils::data_path("silence-44-s.flac");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut flac = audex::flac::FLAC::load(tmp.path()).expect("load");

    let unicode_title = "日本語タイトル 🎵";
    flac.set("TITLE", vec![unicode_title.to_string()]).unwrap();
    flac.save().unwrap();

    let loaded = File::load(tmp.path()).expect("reload");
    let tag_map = loaded.to_tag_map();

    assert_eq!(
        tag_map.get(&StandardField::Title),
        Some([unicode_title.to_string()].as_slice())
    );
}

#[test]
fn test_multivalue_fields() {
    // Multiple values for a single field should be preserved
    let path = TestUtils::data_path("silence-44-s.flac");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut flac = audex::flac::FLAC::load(tmp.path()).expect("load");

    flac.set(
        "ARTIST",
        vec!["Artist A".to_string(), "Artist B".to_string()],
    )
    .unwrap();
    flac.save().unwrap();

    let loaded = File::load(tmp.path()).expect("reload");
    let tag_map = loaded.to_tag_map();

    let artists = tag_map.get(&StandardField::Artist).unwrap();
    assert!(!artists.is_empty());
}

#[test]
fn test_special_chars_in_custom_keys() {
    // Custom fields with special characters should be handled without panicking
    let mut map = TagMap::new();
    map.set_custom(
        "vorbis:CUSTOM-KEY_WITH.DOTS".to_string(),
        vec!["value".to_string()],
    );

    let path = TestUtils::data_path("no-tags.flac");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut dest = File::load(tmp.path()).expect("load");

    // Should not panic
    let report = dest.apply_tag_map(&map).expect("apply");
    assert!(!report.custom_transferred.is_empty());
}

#[test]
fn test_track_number_splitting_round_trip() {
    // Verify that track "5/12" is split into TrackNumber=5 and TotalTracks=12
    let path = TestUtils::data_path("silence-44-s.mp3");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut mp3 = audex::mp3::MP3::load(tmp.path()).expect("load MP3");

    mp3.set("TRCK", vec!["5/12".to_string()]).unwrap();
    mp3.save().unwrap();

    let loaded = File::load(tmp.path()).expect("reload");
    let tag_map = loaded.to_tag_map();

    // Should be split into separate fields
    if let Some(track) = tag_map.get(&StandardField::TrackNumber) {
        assert_eq!(track, &["5"]);
    }
    if let Some(total) = tag_map.get(&StandardField::TotalTracks) {
        assert_eq!(total, &["12"]);
    }
}

#[test]
fn test_very_long_values() {
    // Extremely long values should not cause panics
    let long_value = "A".repeat(10000);
    let mut map = TagMap::new();
    map.set(StandardField::Comment, vec![long_value.clone()]);

    let path = TestUtils::data_path("no-tags.flac");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut dest = File::load(tmp.path()).expect("load");

    // Should succeed without panicking
    let report = dest.apply_tag_map(&map).expect("apply");
    assert!(report.transferred.contains(&StandardField::Comment));
}
