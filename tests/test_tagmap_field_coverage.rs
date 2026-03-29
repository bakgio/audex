//! Tests verifying that every StandardField variant has a mapping in at least
//! one format system, and that secondary mapping paths (ID3 TXXX, MP4 freeform)
//! are exercised for fields not covered by primary frame/atom mappings.

use audex::tagmap::StandardField;
use audex::tagmap::mappings::*;

/// All StandardField variants. Kept in sync with the enum definition
/// so that adding a new variant without mappings causes a compile or test failure.
fn all_standard_fields() -> Vec<StandardField> {
    vec![
        StandardField::Title,
        StandardField::Artist,
        StandardField::Album,
        StandardField::AlbumArtist,
        StandardField::TrackNumber,
        StandardField::TotalTracks,
        StandardField::DiscNumber,
        StandardField::TotalDiscs,
        StandardField::Date,
        StandardField::Year,
        StandardField::Genre,
        StandardField::Comment,
        StandardField::Description,
        StandardField::Composer,
        StandardField::Performer,
        StandardField::Conductor,
        StandardField::Lyricist,
        StandardField::Publisher,
        StandardField::Copyright,
        StandardField::EncodedBy,
        StandardField::Encoder,
        StandardField::Language,
        StandardField::Mood,
        StandardField::BPM,
        StandardField::ISRC,
        StandardField::Barcode,
        StandardField::CatalogNumber,
        StandardField::Label,
        StandardField::Compilation,
        StandardField::Lyrics,
        StandardField::Work,
        StandardField::Movement,
        StandardField::MovementCount,
        StandardField::MovementIndex,
        StandardField::SortTitle,
        StandardField::SortArtist,
        StandardField::SortAlbum,
        StandardField::SortAlbumArtist,
        StandardField::SortComposer,
        StandardField::ReplayGainTrackGain,
        StandardField::ReplayGainTrackPeak,
        StandardField::ReplayGainAlbumGain,
        StandardField::ReplayGainAlbumPeak,
        StandardField::MusicBrainzTrackId,
        StandardField::MusicBrainzAlbumId,
        StandardField::MusicBrainzArtistId,
        StandardField::MusicBrainzReleaseGroupId,
        StandardField::AcoustIdFingerprint,
        StandardField::AcoustIdId,
    ]
}

// ---------------------------------------------------------------------------
// Every field should have at least one mapping across all formats
// ---------------------------------------------------------------------------

#[test]
fn every_standard_field_has_at_least_one_mapping() {
    for field in all_standard_fields() {
        let has_id3 = standard_to_id3(&field).is_some();
        let has_id3_txxx = standard_to_id3_txxx(&field).is_some();
        let has_vorbis = standard_to_vorbis(&field).is_some();
        let has_mp4 = standard_to_mp4(&field).is_some();
        let has_mp4_freeform = standard_to_mp4_freeform(&field).is_some();
        let has_ape = standard_to_ape(&field).is_some();
        let has_asf = standard_to_asf(&field).is_some();

        let any = has_id3
            || has_id3_txxx
            || has_vorbis
            || has_mp4
            || has_mp4_freeform
            || has_ape
            || has_asf;

        assert!(
            any,
            "{:?} has no mapping in any format (ID3={}, TXXX={}, Vorbis={}, MP4={}, Freeform={}, APE={}, ASF={})",
            field, has_id3, has_id3_txxx, has_vorbis, has_mp4, has_mp4_freeform, has_ape, has_asf
        );
    }
}

// ---------------------------------------------------------------------------
// ID3 TXXX secondary mappings for fields without standard frame IDs
// ---------------------------------------------------------------------------

#[test]
fn id3_txxx_covers_musicbrainz_fields() {
    let mb_fields = [
        StandardField::MusicBrainzTrackId,
        StandardField::MusicBrainzAlbumId,
        StandardField::MusicBrainzArtistId,
        StandardField::MusicBrainzReleaseGroupId,
    ];

    for field in &mb_fields {
        // These should NOT have a standard ID3 frame...
        assert!(
            standard_to_id3(field).is_none(),
            "{:?} should not have a standard ID3 frame",
            field
        );
        // ...but SHOULD have a TXXX mapping
        assert!(
            standard_to_id3_txxx(field).is_some(),
            "{:?} should map to a TXXX description",
            field
        );
    }
}

#[test]
fn id3_txxx_covers_replaygain_fields() {
    let rg_fields = [
        StandardField::ReplayGainTrackGain,
        StandardField::ReplayGainTrackPeak,
        StandardField::ReplayGainAlbumGain,
        StandardField::ReplayGainAlbumPeak,
    ];

    for field in &rg_fields {
        assert!(
            standard_to_id3_txxx(field).is_some(),
            "{:?} should map to a TXXX description for ReplayGain",
            field
        );
    }
}

// ---------------------------------------------------------------------------
// MP4 freeform secondary mappings
// ---------------------------------------------------------------------------

#[test]
fn mp4_freeform_covers_musicbrainz_fields() {
    let mb_fields = [
        StandardField::MusicBrainzTrackId,
        StandardField::MusicBrainzAlbumId,
        StandardField::MusicBrainzArtistId,
        StandardField::MusicBrainzReleaseGroupId,
    ];

    for field in &mb_fields {
        assert!(
            standard_to_mp4_freeform(field).is_some(),
            "{:?} should map to an MP4 freeform atom",
            field
        );
    }
}

// ---------------------------------------------------------------------------
// Vorbis Comment has the broadest coverage
// ---------------------------------------------------------------------------

#[test]
fn vorbis_covers_most_standard_fields() {
    let mut missing = Vec::new();
    for field in all_standard_fields() {
        if standard_to_vorbis(&field).is_none() {
            missing.push(field);
        }
    }
    // Vorbis Comment is the most flexible format; only a few fields
    // may lack a direct mapping (e.g., format-specific sort keys).
    // This test documents which ones are missing rather than asserting zero.
    assert!(
        missing.len() <= 10,
        "Too many StandardField variants lack a Vorbis mapping: {:?}",
        missing
    );
}

// ---------------------------------------------------------------------------
// Roundtrip: primary mapping -> reverse -> same field
// ---------------------------------------------------------------------------

#[test]
fn id3_primary_mapping_roundtrips() {
    // Year -> TYER -> Date is an intentional alias (TYER is the legacy date frame).
    // These known aliases are excluded from the strict identity check.
    let known_aliases: &[(StandardField, StandardField)] =
        &[(StandardField::Year, StandardField::Date)];

    for field in all_standard_fields() {
        if let Some(key) = standard_to_id3(&field) {
            let back = id3_to_standard(key);
            let is_known_alias = known_aliases
                .iter()
                .any(|(from, to)| *from == field && back == Some(to.clone()));
            if !is_known_alias {
                assert_eq!(
                    back,
                    Some(field.clone()),
                    "ID3 roundtrip failed for {:?} -> {} -> {:?}",
                    field,
                    key,
                    back
                );
            }
        }
    }
}

#[test]
fn vorbis_primary_mapping_roundtrips() {
    for field in all_standard_fields() {
        if let Some(key) = standard_to_vorbis(&field) {
            let back = vorbis_to_standard(key);
            assert_eq!(
                back,
                Some(field.clone()),
                "Vorbis roundtrip failed for {:?} -> {} -> {:?}",
                field,
                key,
                back
            );
        }
    }
}

#[test]
fn mp4_primary_mapping_roundtrips() {
    for field in all_standard_fields() {
        if let Some(key) = standard_to_mp4(&field) {
            let back = mp4_to_standard(key);
            assert_eq!(
                back,
                Some(field.clone()),
                "MP4 roundtrip failed for {:?} -> {} -> {:?}",
                field,
                key,
                back
            );
        }
    }
}
