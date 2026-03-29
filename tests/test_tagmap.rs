//! Unit tests for the TagMap struct and related types.

mod common;

use audex::tagmap::{ConversionReport, SkipReason, StandardField, TagMap};

#[test]
fn test_tagmap_new_empty() {
    let map = TagMap::new();
    assert!(map.is_empty());
    assert!(map.standard_fields().is_empty());
    assert!(map.custom_fields().is_empty());
}

#[test]
fn test_tagmap_set_get_standard() {
    let mut map = TagMap::new();
    map.set(StandardField::Title, vec!["Hello World".to_string()]);
    map.set(
        StandardField::Artist,
        vec!["Artist A".to_string(), "Artist B".to_string()],
    );

    let title = map.get(&StandardField::Title).unwrap();
    assert_eq!(title, &["Hello World"]);

    let artists = map.get(&StandardField::Artist).unwrap();
    assert_eq!(artists, &["Artist A", "Artist B"]);

    // Non-existent field returns None
    assert!(map.get(&StandardField::Album).is_none());
}

#[test]
fn test_tagmap_set_get_custom() {
    let mut map = TagMap::new();
    map.set_custom(
        "id3:TXXX:CUSTOM".to_string(),
        vec!["custom value".to_string()],
    );

    let val = map.get_custom("id3:TXXX:CUSTOM").unwrap();
    assert_eq!(val, &["custom value"]);

    assert!(map.get_custom("nonexistent").is_none());
}

#[test]
fn test_tagmap_remove() {
    let mut map = TagMap::new();
    map.set(StandardField::Title, vec!["Title".to_string()]);
    map.set_custom("custom".to_string(), vec!["val".to_string()]);

    assert!(!map.is_empty());
    map.remove(&StandardField::Title);
    assert!(map.get(&StandardField::Title).is_none());

    map.remove_custom("custom");
    assert!(map.get_custom("custom").is_none());
    assert!(map.is_empty());
}

#[test]
fn test_tagmap_merge_no_overwrite() {
    let mut map_a = TagMap::new();
    map_a.set(StandardField::Title, vec!["Original".to_string()]);
    map_a.set(StandardField::Album, vec!["Album A".to_string()]);

    let mut map_b = TagMap::new();
    map_b.set(StandardField::Title, vec!["Replacement".to_string()]);
    map_b.set(StandardField::Artist, vec!["New Artist".to_string()]);

    // Merge without overwrite: Title should remain "Original"
    map_a.merge(&map_b, false);
    assert_eq!(map_a.get(&StandardField::Title).unwrap(), &["Original"]);
    assert_eq!(map_a.get(&StandardField::Artist).unwrap(), &["New Artist"]);
    assert_eq!(map_a.get(&StandardField::Album).unwrap(), &["Album A"]);
}

#[test]
fn test_tagmap_merge_with_overwrite() {
    let mut map_a = TagMap::new();
    map_a.set(StandardField::Title, vec!["Original".to_string()]);

    let mut map_b = TagMap::new();
    map_b.set(StandardField::Title, vec!["Replacement".to_string()]);

    map_a.merge(&map_b, true);
    assert_eq!(map_a.get(&StandardField::Title).unwrap(), &["Replacement"]);
}

#[test]
fn test_tagmap_is_empty() {
    let mut map = TagMap::new();
    assert!(map.is_empty());

    map.set(StandardField::Title, vec!["T".to_string()]);
    assert!(!map.is_empty());

    map.remove(&StandardField::Title);
    assert!(map.is_empty());

    // Custom field also makes it non-empty
    map.set_custom("x".to_string(), vec!["y".to_string()]);
    assert!(!map.is_empty());
}

#[test]
fn test_tagmap_clear() {
    let mut map = TagMap::new();
    map.set(StandardField::Title, vec!["T".to_string()]);
    map.set(StandardField::Artist, vec!["A".to_string()]);
    map.set_custom("c".to_string(), vec!["v".to_string()]);

    map.clear();
    assert!(map.is_empty());
    assert!(map.standard_fields().is_empty());
    assert!(map.custom_fields().is_empty());
}

#[test]
fn test_tagmap_set_empty_removes() {
    let mut map = TagMap::new();
    map.set(StandardField::Title, vec!["T".to_string()]);
    // Setting empty values should remove the field
    map.set(StandardField::Title, vec![]);
    assert!(map.get(&StandardField::Title).is_none());

    map.set_custom("k".to_string(), vec!["v".to_string()]);
    map.set_custom("k".to_string(), vec![]);
    assert!(map.get_custom("k").is_none());
}

#[test]
fn test_tagmap_standard_field_display() {
    assert_eq!(StandardField::Title.to_string(), "Title");
    assert_eq!(StandardField::AlbumArtist.to_string(), "Album Artist");
    assert_eq!(StandardField::TrackNumber.to_string(), "Track Number");
    assert_eq!(StandardField::BPM.to_string(), "BPM");
    assert_eq!(StandardField::ISRC.to_string(), "ISRC");
}

#[test]
fn test_tagmap_standard_field_from_str() {
    assert_eq!(
        "title".parse::<StandardField>().unwrap(),
        StandardField::Title
    );
    assert_eq!(
        "ALBUM ARTIST".parse::<StandardField>().unwrap(),
        StandardField::AlbumArtist
    );
    assert_eq!(
        "track_number".parse::<StandardField>().unwrap(),
        StandardField::TrackNumber
    );
    assert_eq!("bpm".parse::<StandardField>().unwrap(), StandardField::BPM);
    assert!("unknown_field".parse::<StandardField>().is_err());
}

#[test]
fn test_conversion_report_display() {
    let report = ConversionReport {
        transferred: vec![StandardField::Title, StandardField::Artist],
        custom_transferred: vec!["id3:TXXX:FOO".to_string()],
        skipped: vec![("Mood".to_string(), SkipReason::UnsupportedByTarget)],
        warnings: vec!["test warning".to_string()],
    };
    let display = report.to_string();
    assert!(display.contains("2 standard"));
    assert!(display.contains("1 custom"));
    assert!(display.contains("Skipped: 1"));
    assert!(display.contains("Mood"));
    assert!(display.contains("test warning"));
}

#[test]
fn test_skip_reason_display() {
    assert_eq!(
        SkipReason::UnsupportedByTarget.to_string(),
        "unsupported by target format"
    );
    assert_eq!(
        SkipReason::ReadOnlyFormat.to_string(),
        "target format is read-only"
    );
    assert!(
        SkipReason::ValueTooLong { max_len: 100 }
            .to_string()
            .contains("100")
    );
    assert_eq!(
        SkipReason::IncompatibleType.to_string(),
        "incompatible value type"
    );
}
