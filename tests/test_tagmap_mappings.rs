//! Tests for the bidirectional field mapping tables.

mod common;

use audex::tagmap::StandardField;
use audex::tagmap::mappings::*;

// ---------------------------------------------------------------------------
// ID3v2 round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn test_id3_round_trip_all_fields() {
    // Every field that has an ID3 mapping should survive a round-trip
    let fields_and_keys = [
        (StandardField::Title, "TIT2"),
        (StandardField::Artist, "TPE1"),
        (StandardField::Album, "TALB"),
        (StandardField::AlbumArtist, "TPE2"),
        (StandardField::TrackNumber, "TRCK"),
        (StandardField::DiscNumber, "TPOS"),
        (StandardField::Date, "TDRC"),
        (StandardField::Genre, "TCON"),
        (StandardField::Comment, "COMM"),
        (StandardField::Composer, "TCOM"),
        (StandardField::Conductor, "TPE3"),
        (StandardField::Lyricist, "TEXT"),
        (StandardField::Publisher, "TPUB"),
        (StandardField::Copyright, "TCOP"),
        (StandardField::EncodedBy, "TENC"),
        (StandardField::Encoder, "TSSE"),
        (StandardField::BPM, "TBPM"),
        (StandardField::ISRC, "TSRC"),
        (StandardField::Compilation, "TCMP"),
        (StandardField::SortTitle, "TSOT"),
        (StandardField::SortArtist, "TSOP"),
        (StandardField::SortAlbum, "TSOA"),
        (StandardField::Mood, "TMOO"),
        (StandardField::Language, "TLAN"),
    ];

    for (field, expected_key) in &fields_and_keys {
        let key =
            standard_to_id3(field).unwrap_or_else(|| panic!("No ID3 mapping for {:?}", field));
        assert_eq!(key, *expected_key, "Forward mapping wrong for {:?}", field);

        let back =
            id3_to_standard(key).unwrap_or_else(|| panic!("No reverse ID3 mapping for {}", key));
        assert_eq!(back, *field, "Reverse mapping wrong for {}", key);
    }
}

#[test]
fn test_id3_legacy_date_frames() {
    // TYER and TDAT from ID3v2.3 should both map to Date
    assert_eq!(id3_to_standard("TYER"), Some(StandardField::Date));
    assert_eq!(id3_to_standard("TDAT"), Some(StandardField::Date));
}

// ---------------------------------------------------------------------------
// Vorbis round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn test_vorbis_round_trip_all_fields() {
    let fields_and_keys = [
        (StandardField::Title, "TITLE"),
        (StandardField::Artist, "ARTIST"),
        (StandardField::Album, "ALBUM"),
        (StandardField::AlbumArtist, "ALBUMARTIST"),
        (StandardField::TrackNumber, "TRACKNUMBER"),
        (StandardField::TotalTracks, "TRACKTOTAL"),
        (StandardField::DiscNumber, "DISCNUMBER"),
        (StandardField::TotalDiscs, "DISCTOTAL"),
        (StandardField::Date, "DATE"),
        (StandardField::Genre, "GENRE"),
        (StandardField::Comment, "COMMENT"),
        (StandardField::Composer, "COMPOSER"),
        (StandardField::Performer, "PERFORMER"),
        (StandardField::Conductor, "CONDUCTOR"),
        (StandardField::Lyricist, "LYRICIST"),
        (StandardField::Publisher, "ORGANIZATION"),
        (StandardField::Copyright, "COPYRIGHT"),
        (StandardField::EncodedBy, "ENCODED-BY"),
        (StandardField::Encoder, "ENCODER"),
        (StandardField::BPM, "BPM"),
        (StandardField::ISRC, "ISRC"),
        (StandardField::Compilation, "COMPILATION"),
        (StandardField::Mood, "MOOD"),
        (StandardField::Language, "LANGUAGE"),
        (StandardField::Label, "LABEL"),
    ];

    for (field, expected_key) in &fields_and_keys {
        let key = standard_to_vorbis(field)
            .unwrap_or_else(|| panic!("No Vorbis mapping for {:?}", field));
        assert_eq!(key, *expected_key, "Forward mapping wrong for {:?}", field);

        let back = vorbis_to_standard(key)
            .unwrap_or_else(|| panic!("No reverse Vorbis mapping for {}", key));
        assert_eq!(back, *field, "Reverse mapping wrong for {}", key);
    }
}

#[test]
fn test_case_insensitive_vorbis_lookup() {
    // Vorbis Comment keys should be looked up case-insensitively
    assert_eq!(vorbis_to_standard("title"), Some(StandardField::Title));
    assert_eq!(vorbis_to_standard("TITLE"), Some(StandardField::Title));
    assert_eq!(vorbis_to_standard("Title"), Some(StandardField::Title));
    assert_eq!(
        vorbis_to_standard("albumartist"),
        Some(StandardField::AlbumArtist)
    );
}

#[test]
fn test_vorbis_alias_keys() {
    // Alternate keys should also resolve
    assert_eq!(
        vorbis_to_standard("TOTALTRACKS"),
        Some(StandardField::TotalTracks)
    );
    assert_eq!(
        vorbis_to_standard("TOTALDISCS"),
        Some(StandardField::TotalDiscs)
    );
}

// ---------------------------------------------------------------------------
// MP4 round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn test_mp4_round_trip_all_fields() {
    let fields_and_keys = [
        (StandardField::Title, "\u{a9}nam"),
        (StandardField::Artist, "\u{a9}ART"),
        (StandardField::Album, "\u{a9}alb"),
        (StandardField::AlbumArtist, "aART"),
        (StandardField::TrackNumber, "trkn"),
        (StandardField::DiscNumber, "disk"),
        (StandardField::Date, "\u{a9}day"),
        (StandardField::Genre, "\u{a9}gen"),
        (StandardField::Comment, "\u{a9}cmt"),
        (StandardField::Composer, "\u{a9}wrt"),
        (StandardField::Encoder, "\u{a9}too"),
        (StandardField::Copyright, "cprt"),
        (StandardField::BPM, "tmpo"),
        (StandardField::Compilation, "cpil"),
        (StandardField::SortTitle, "sonm"),
        (StandardField::SortArtist, "soar"),
        (StandardField::SortAlbum, "soal"),
        (StandardField::SortAlbumArtist, "soaa"),
        (StandardField::SortComposer, "soco"),
    ];

    for (field, expected_key) in &fields_and_keys {
        let key =
            standard_to_mp4(field).unwrap_or_else(|| panic!("No MP4 mapping for {:?}", field));
        assert_eq!(key, *expected_key, "Forward mapping wrong for {:?}", field);

        let back =
            mp4_to_standard(key).unwrap_or_else(|| panic!("No reverse MP4 mapping for {}", key));
        assert_eq!(back, *field, "Reverse mapping wrong for {}", key);
    }
}

// ---------------------------------------------------------------------------
// APEv2 round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn test_ape_round_trip_all_fields() {
    let fields_and_keys = [
        (StandardField::Title, "Title"),
        (StandardField::Artist, "Artist"),
        (StandardField::Album, "Album"),
        (StandardField::AlbumArtist, "Album Artist"),
        (StandardField::TrackNumber, "Track"),
        (StandardField::DiscNumber, "Disc"),
        (StandardField::Date, "Year"),
        (StandardField::Genre, "Genre"),
        (StandardField::Comment, "Comment"),
        (StandardField::Composer, "Composer"),
        (StandardField::Conductor, "Conductor"),
        (StandardField::Publisher, "Publisher"),
        (StandardField::Copyright, "Copyright"),
        (StandardField::ISRC, "ISRC"),
        (StandardField::Compilation, "Compilation"),
        (StandardField::Encoder, "Encoder"),
        (StandardField::EncodedBy, "EncodedBy"),
        (StandardField::BPM, "BPM"),
        (StandardField::Mood, "Mood"),
        (StandardField::Language, "Language"),
        (StandardField::Label, "Label"),
    ];

    for (field, expected_key) in &fields_and_keys {
        let key =
            standard_to_ape(field).unwrap_or_else(|| panic!("No APE mapping for {:?}", field));
        assert_eq!(key, *expected_key, "Forward mapping wrong for {:?}", field);

        let back =
            ape_to_standard(key).unwrap_or_else(|| panic!("No reverse APE mapping for {}", key));
        assert_eq!(back, *field, "Reverse mapping wrong for {}", key);
    }
}

#[test]
fn test_ape_case_insensitive() {
    assert_eq!(ape_to_standard("title"), Some(StandardField::Title));
    assert_eq!(ape_to_standard("TITLE"), Some(StandardField::Title));
    assert_eq!(
        ape_to_standard("album artist"),
        Some(StandardField::AlbumArtist)
    );
}

// ---------------------------------------------------------------------------
// ASF round-trip tests
// ---------------------------------------------------------------------------

#[test]
fn test_asf_round_trip_all_fields() {
    let fields_and_keys = [
        (StandardField::Title, "Title"),
        (StandardField::Artist, "Author"),
        (StandardField::Album, "WM/AlbumTitle"),
        (StandardField::AlbumArtist, "WM/AlbumArtist"),
        (StandardField::TrackNumber, "WM/TrackNumber"),
        (StandardField::DiscNumber, "WM/PartOfSet"),
        (StandardField::Date, "WM/Year"),
        (StandardField::Genre, "WM/Genre"),
        (StandardField::Description, "Description"),
        (StandardField::Comment, "WM/Text"),
        (StandardField::Composer, "WM/Composer"),
        (StandardField::Conductor, "WM/Conductor"),
        (StandardField::Publisher, "WM/Publisher"),
        (StandardField::Copyright, "Copyright"),
        (StandardField::EncodedBy, "WM/EncodedBy"),
        (StandardField::ISRC, "WM/ISRC"),
        (StandardField::BPM, "WM/BeatsPerMinute"),
        (StandardField::Mood, "WM/Mood"),
        (StandardField::Language, "WM/Language"),
    ];

    for (field, expected_key) in &fields_and_keys {
        let key =
            standard_to_asf(field).unwrap_or_else(|| panic!("No ASF mapping for {:?}", field));
        assert_eq!(key, *expected_key, "Forward mapping wrong for {:?}", field);

        let back =
            asf_to_standard(key).unwrap_or_else(|| panic!("No reverse ASF mapping for {}", key));
        assert_eq!(back, *field, "Reverse mapping wrong for {}", key);
    }
}

// ---------------------------------------------------------------------------
// Cross-system: no duplicate mappings within a single table
// ---------------------------------------------------------------------------

#[test]
fn test_no_duplicate_id3_keys() {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let fields = [
        StandardField::Title,
        StandardField::Artist,
        StandardField::Album,
        StandardField::AlbumArtist,
        StandardField::TrackNumber,
        StandardField::DiscNumber,
        StandardField::Date,
        StandardField::Genre,
        StandardField::Comment,
        StandardField::Composer,
        StandardField::Conductor,
        StandardField::Lyricist,
        StandardField::Publisher,
        StandardField::Copyright,
        StandardField::EncodedBy,
        StandardField::Encoder,
        StandardField::BPM,
        StandardField::ISRC,
        StandardField::Compilation,
        StandardField::SortTitle,
        StandardField::SortArtist,
        StandardField::SortAlbum,
        StandardField::Mood,
        StandardField::Language,
    ];
    for field in &fields {
        if let Some(key) = standard_to_id3(field) {
            assert!(seen.insert(key), "Duplicate ID3 key: {}", key);
        }
    }
}

#[test]
fn test_unmapped_field_returns_none() {
    // Fields not in a mapping table should return None
    assert!(standard_to_id3(&StandardField::Barcode).is_none());
    assert!(standard_to_mp4(&StandardField::Conductor).is_none());
    assert!(standard_to_asf(&StandardField::Barcode).is_none());
}
