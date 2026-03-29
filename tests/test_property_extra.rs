//! Additional property-based tests for format-specific round-trip invariants.
//!
//! Uses the proptest framework to verify that encoding/decoding, key mapping,
//! and serialization functions are true inverses over their valid input domains.
//! No files on disk are modified.

use audex::Tags;
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// MP4 atom name <-> key round-trip
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Any 4-byte Latin-1 atom name must survive name2key -> key2name unchanged.
    #[test]
    fn mp4_atom_name_roundtrip(bytes in prop::array::uniform4(0u8..=255u8)) {
        let key = audex::mp4::name2key(&bytes);
        let back = audex::mp4::key2name(&key).unwrap();
        prop_assert_eq!(&back[..], &bytes[..]);
    }
}

// ---------------------------------------------------------------------------
// Vorbis Comment key+value serialize/deserialize round-trip
// ---------------------------------------------------------------------------

/// Generate a valid Vorbis Comment key: printable ASCII (0x20-0x7D),
/// no '=' sign, length 1-20.
fn arb_vorbis_key() -> impl Strategy<Value = String> {
    prop::collection::vec(0x20u8..=0x7Du8, 1..20)
        .prop_filter("key must not contain '='", |bytes| !bytes.contains(&b'='))
        .prop_map(|bytes| bytes.into_iter().map(|b| b as char).collect::<String>())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// A VComment with a single valid key+value must survive to_bytes -> from_bytes.
    #[test]
    fn vorbis_key_value_roundtrip(
        key in arb_vorbis_key(),
        value in "[\\x20-\\x7e]{0,100}",
    ) {
        let mut comment = audex::vorbis::VComment::new();
        comment.push(key.clone(), value.clone()).unwrap();
        let bytes = comment.to_bytes().unwrap();
        let restored = audex::vorbis::VComment::from_bytes(&bytes).unwrap();
        // Vorbis keys are case-insensitive; compare lowercased
        let restored_value = restored.get(&key.to_uppercase());
        prop_assert!(
            restored_value.is_some(),
            "key {:?} not found after round-trip",
            key
        );
        prop_assert_eq!(restored_value.unwrap(), &vec![value]);
    }
}

// ---------------------------------------------------------------------------
// ASF UTF-16LE encode/decode round-trip
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Any valid Unicode string must survive encode_utf16_le -> parse_utf16_le.
    #[test]
    fn asf_utf16le_roundtrip(text in "\\PC{0,200}") {
        let encoded = audex::asf::util::ASFUtil::encode_utf16_le(&text);
        let decoded = audex::asf::util::ASFUtil::parse_utf16_le(&encoded).unwrap();
        prop_assert_eq!(decoded, text);
    }
}

// ---------------------------------------------------------------------------
// StandardField Display -> FromStr round-trip
// ---------------------------------------------------------------------------

/// All StandardField variants listed explicitly (no EnumIter available).
fn all_standard_fields() -> Vec<audex::tagmap::StandardField> {
    use audex::tagmap::StandardField::*;
    vec![
        Title,
        Artist,
        Album,
        AlbumArtist,
        TrackNumber,
        TotalTracks,
        DiscNumber,
        TotalDiscs,
        Date,
        Year,
        Genre,
        Comment,
        Description,
        Composer,
        Performer,
        Conductor,
        Lyricist,
        Publisher,
        Copyright,
        EncodedBy,
        Encoder,
        Language,
        Mood,
        BPM,
        ISRC,
        Barcode,
        CatalogNumber,
        Label,
        Compilation,
        Lyrics,
        Work,
        Movement,
        MovementCount,
        MovementIndex,
        SortTitle,
        SortArtist,
        SortAlbum,
        SortAlbumArtist,
        SortComposer,
        ReplayGainTrackGain,
        ReplayGainTrackPeak,
        ReplayGainAlbumGain,
        ReplayGainAlbumPeak,
        MusicBrainzTrackId,
        MusicBrainzAlbumId,
        MusicBrainzArtistId,
        MusicBrainzReleaseGroupId,
        AcoustIdFingerprint,
        AcoustIdId,
    ]
}

#[test]
fn standard_field_display_fromstr_roundtrip() {
    for field in all_standard_fields() {
        let display = field.to_string();
        let parsed: audex::tagmap::StandardField = display
            .parse()
            .unwrap_or_else(|e| panic!("{:?} Display={:?} failed to parse: {}", field, display, e));
        assert_eq!(
            parsed, field,
            "Display/FromStr roundtrip failed: {:?} -> {:?} -> {:?}",
            field, display, parsed
        );
    }
}
