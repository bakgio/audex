//! Bidirectional mapping tables between [`StandardField`] and format-specific keys.
//!
//! Each tagging system has its own naming convention for the same semantic concept:
//! "track title" is `TIT2` in ID3v2, `TITLE` in Vorbis Comments, `©nam` in MP4,
//! `Title` in APEv2, and `Title` in ASF. This module defines the lookup tables
//! that bridge those conventions.

use super::StandardField;

// ---------------------------------------------------------------------------
// Mapping entry: a pair of (StandardField, format-specific key)
// ---------------------------------------------------------------------------

/// Single mapping between a standard field and its format-specific key.
struct FieldMapping {
    field: StandardField,
    key: &'static str,
}

// ---------------------------------------------------------------------------
// ID3v2 frame mappings
// ---------------------------------------------------------------------------

/// Mappings from StandardField to ID3v2 frame IDs (v2.3 and v2.4 share most IDs).
/// ID3v2 field mappings. Note: TotalTracks and TotalDiscs are intentionally
/// absent because ID3v2 stores them combined with TrackNumber/DiscNumber
/// as "N/M" in the TRCK/TPOS frames. The combination logic lives in
/// `file::tag_map_to_items()` via `combine_track_disc()`.
static ID3_MAP: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::Title,
        key: "TIT2",
    },
    FieldMapping {
        field: StandardField::Artist,
        key: "TPE1",
    },
    FieldMapping {
        field: StandardField::Album,
        key: "TALB",
    },
    FieldMapping {
        field: StandardField::AlbumArtist,
        key: "TPE2",
    },
    FieldMapping {
        field: StandardField::TrackNumber,
        key: "TRCK",
    },
    FieldMapping {
        field: StandardField::DiscNumber,
        key: "TPOS",
    },
    FieldMapping {
        field: StandardField::Date,
        key: "TDRC",
    },
    FieldMapping {
        field: StandardField::Year,
        key: "TYER",
    },
    FieldMapping {
        field: StandardField::Genre,
        key: "TCON",
    },
    FieldMapping {
        field: StandardField::Comment,
        key: "COMM",
    },
    FieldMapping {
        field: StandardField::Composer,
        key: "TCOM",
    },
    FieldMapping {
        field: StandardField::Conductor,
        key: "TPE3",
    },
    FieldMapping {
        field: StandardField::Lyricist,
        key: "TEXT",
    },
    FieldMapping {
        field: StandardField::Publisher,
        key: "TPUB",
    },
    FieldMapping {
        field: StandardField::Copyright,
        key: "TCOP",
    },
    FieldMapping {
        field: StandardField::EncodedBy,
        key: "TENC",
    },
    FieldMapping {
        field: StandardField::Encoder,
        key: "TSSE",
    },
    FieldMapping {
        field: StandardField::BPM,
        key: "TBPM",
    },
    FieldMapping {
        field: StandardField::ISRC,
        key: "TSRC",
    },
    FieldMapping {
        field: StandardField::Compilation,
        key: "TCMP",
    },
    FieldMapping {
        field: StandardField::SortTitle,
        key: "TSOT",
    },
    FieldMapping {
        field: StandardField::SortArtist,
        key: "TSOP",
    },
    FieldMapping {
        field: StandardField::SortAlbum,
        key: "TSOA",
    },
    FieldMapping {
        field: StandardField::SortAlbumArtist,
        key: "TSO2",
    },
    FieldMapping {
        field: StandardField::SortComposer,
        key: "TSOC",
    },
    FieldMapping {
        field: StandardField::Mood,
        key: "TMOO",
    },
    FieldMapping {
        field: StandardField::Language,
        key: "TLAN",
    },
    // Performer is mapped via TXXX:PERFORMER rather than TMCL, because
    // TMCL is a structured frame for musician credits (instrument=name pairs)
    // which does not cleanly map to a single free-text performer field.
    // See ID3_TXXX_MAP and ID3_TXXX_WRITE for the Performer mapping.
    //
    // Lyrics use the USLT frame (unsynced lyrics / text transcription)
    FieldMapping {
        field: StandardField::Lyrics,
        key: "USLT",
    },
];

/// Get the ID3v2 frame ID for a standard field, if one exists.
pub fn standard_to_id3(field: &StandardField) -> Option<&'static str> {
    ID3_MAP.iter().find(|m| m.field == *field).map(|m| m.key)
}

/// Write-path lookup: maps a standard field to a TXXX description key
/// (e.g. `"TXXX:BARCODE"`).  Returns the first match only.
pub fn standard_to_id3_txxx(field: &StandardField) -> Option<&'static str> {
    // We store full "TXXX:{desc}" keys in a separate static table so the
    // returned &'static str can be used directly with file.set().
    static ID3_TXXX_WRITE: &[FieldMapping] = &[
        FieldMapping {
            field: StandardField::ReplayGainTrackGain,
            key: "TXXX:REPLAYGAIN_TRACK_GAIN",
        },
        FieldMapping {
            field: StandardField::ReplayGainTrackPeak,
            key: "TXXX:REPLAYGAIN_TRACK_PEAK",
        },
        FieldMapping {
            field: StandardField::ReplayGainAlbumGain,
            key: "TXXX:REPLAYGAIN_ALBUM_GAIN",
        },
        FieldMapping {
            field: StandardField::ReplayGainAlbumPeak,
            key: "TXXX:REPLAYGAIN_ALBUM_PEAK",
        },
        FieldMapping {
            field: StandardField::Barcode,
            key: "TXXX:BARCODE",
        },
        FieldMapping {
            field: StandardField::CatalogNumber,
            key: "TXXX:Catalog Number",
        },
        FieldMapping {
            field: StandardField::Label,
            key: "TXXX:LABEL",
        },
        FieldMapping {
            field: StandardField::MusicBrainzTrackId,
            key: "TXXX:MusicBrainz Track Id",
        },
        FieldMapping {
            field: StandardField::MusicBrainzAlbumId,
            key: "TXXX:MusicBrainz Album Id",
        },
        FieldMapping {
            field: StandardField::MusicBrainzArtistId,
            key: "TXXX:MusicBrainz Artist Id",
        },
        FieldMapping {
            field: StandardField::MusicBrainzReleaseGroupId,
            key: "TXXX:MusicBrainz Release Group Id",
        },
        FieldMapping {
            field: StandardField::AcoustIdFingerprint,
            key: "TXXX:Acoustid Fingerprint",
        },
        FieldMapping {
            field: StandardField::AcoustIdId,
            key: "TXXX:Acoustid Id",
        },
        FieldMapping {
            field: StandardField::Performer,
            key: "TXXX:PERFORMER",
        },
        FieldMapping {
            field: StandardField::Description,
            key: "TXXX:Description",
        },
    ];
    ID3_TXXX_WRITE
        .iter()
        .find(|m| m.field == *field)
        .map(|m| m.key)
}

/// Mappings from TXXX description to StandardField for well-known user-defined frames.
static ID3_TXXX_MAP: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::ReplayGainTrackGain,
        key: "REPLAYGAIN_TRACK_GAIN",
    },
    FieldMapping {
        field: StandardField::ReplayGainTrackPeak,
        key: "REPLAYGAIN_TRACK_PEAK",
    },
    FieldMapping {
        field: StandardField::ReplayGainAlbumGain,
        key: "REPLAYGAIN_ALBUM_GAIN",
    },
    FieldMapping {
        field: StandardField::ReplayGainAlbumPeak,
        key: "REPLAYGAIN_ALBUM_PEAK",
    },
    FieldMapping {
        field: StandardField::Barcode,
        key: "BARCODE",
    },
    FieldMapping {
        field: StandardField::Barcode,
        key: "UPC",
    },
    FieldMapping {
        field: StandardField::CatalogNumber,
        key: "Catalog Number",
    },
    FieldMapping {
        field: StandardField::Label,
        key: "LABEL",
    },
    FieldMapping {
        field: StandardField::MusicBrainzTrackId,
        key: "MusicBrainz Track Id",
    },
    FieldMapping {
        field: StandardField::MusicBrainzAlbumId,
        key: "MusicBrainz Album Id",
    },
    FieldMapping {
        field: StandardField::MusicBrainzArtistId,
        key: "MusicBrainz Artist Id",
    },
    FieldMapping {
        field: StandardField::MusicBrainzReleaseGroupId,
        key: "MusicBrainz Release Group Id",
    },
    FieldMapping {
        field: StandardField::AcoustIdFingerprint,
        key: "Acoustid Fingerprint",
    },
    FieldMapping {
        field: StandardField::AcoustIdId,
        key: "Acoustid Id",
    },
    FieldMapping {
        field: StandardField::Performer,
        key: "PERFORMER",
    },
    FieldMapping {
        field: StandardField::Description,
        key: "Description",
    },
];

/// Get the standard field for an ID3v2 frame ID, if one is mapped.
///
/// Handles both plain frame IDs ("TIT2") and hash-key formats used for
/// multi-instance frames ("TXXX:description", "COMM::lang", "USLT:desc:lang").
pub fn id3_to_standard(frame_id: &str) -> Option<StandardField> {
    // Both TYER (year, e.g. "2024") and TDAT (day-month, e.g. "0115" for Jan 15)
    // map to StandardField::Date because the standard field model has no separate
    // year-only or day-month-only representation. In ID3v2.4 the TDRC frame
    // supersedes both, encoding the full ISO-8601 timestamp in a single frame.
    //
    // This mapping is inherently lossy: converting a v2.3 tag to a StandardField
    // and back loses the distinction between TYER and TDAT, so a round-trip
    // cannot reconstruct the original pair. Callers converting from ID3v2.3
    // should merge TYER + TDAT into a single ISO-8601 date string *before*
    // mapping to StandardField::Date. When writing back to v2.3, the caller
    // is responsible for splitting the date into separate TYER and TDAT frames.
    if frame_id == "TYER" || frame_id == "TDAT" {
        return Some(StandardField::Date);
    }
    // Handle TXXX:description keys (user-defined text frames)
    if let Some(desc) = frame_id.strip_prefix("TXXX:") {
        return ID3_TXXX_MAP
            .iter()
            .find(|m| m.key.eq_ignore_ascii_case(desc))
            .map(|m| m.field.clone());
    }
    // Handle hash keys for multi-instance frames — extract the base frame ID
    // "COMM::eng" / "COMM:desc:lang" → "COMM"
    // "USLT::eng" / "USLT:desc:lang" → "USLT"
    // "APIC:Cover" → "APIC"
    // Use `get(..4)` instead of `&frame_id[..4]` to avoid panicking when
    // a corrupted frame ID contains multi-byte UTF-8 characters whose
    // byte boundary does not fall at index 4.
    let base_id = if frame_id.len() > 4 && frame_id.as_bytes().get(4) == Some(&b':') {
        frame_id.get(..4).unwrap_or(frame_id)
    } else {
        frame_id
    };
    ID3_MAP
        .iter()
        .find(|m| m.key == base_id)
        .map(|m| m.field.clone())
}

// ---------------------------------------------------------------------------
// Vorbis Comment mappings
// ---------------------------------------------------------------------------

/// Mappings from StandardField to Vorbis Comment keys (uppercase by convention).
static VORBIS_MAP: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::Title,
        key: "TITLE",
    },
    FieldMapping {
        field: StandardField::Artist,
        key: "ARTIST",
    },
    FieldMapping {
        field: StandardField::Album,
        key: "ALBUM",
    },
    FieldMapping {
        field: StandardField::AlbumArtist,
        key: "ALBUMARTIST",
    },
    FieldMapping {
        field: StandardField::TrackNumber,
        key: "TRACKNUMBER",
    },
    FieldMapping {
        field: StandardField::TotalTracks,
        key: "TRACKTOTAL",
    },
    FieldMapping {
        field: StandardField::DiscNumber,
        key: "DISCNUMBER",
    },
    FieldMapping {
        field: StandardField::TotalDiscs,
        key: "DISCTOTAL",
    },
    FieldMapping {
        field: StandardField::Date,
        key: "DATE",
    },
    FieldMapping {
        field: StandardField::Year,
        key: "YEAR",
    },
    FieldMapping {
        field: StandardField::Genre,
        key: "GENRE",
    },
    FieldMapping {
        field: StandardField::Comment,
        key: "COMMENT",
    },
    FieldMapping {
        field: StandardField::Description,
        key: "DESCRIPTION",
    },
    FieldMapping {
        field: StandardField::Composer,
        key: "COMPOSER",
    },
    FieldMapping {
        field: StandardField::Performer,
        key: "PERFORMER",
    },
    FieldMapping {
        field: StandardField::Conductor,
        key: "CONDUCTOR",
    },
    FieldMapping {
        field: StandardField::Lyricist,
        key: "LYRICIST",
    },
    FieldMapping {
        field: StandardField::Publisher,
        key: "ORGANIZATION",
    },
    FieldMapping {
        field: StandardField::Copyright,
        key: "COPYRIGHT",
    },
    FieldMapping {
        field: StandardField::EncodedBy,
        key: "ENCODED-BY",
    },
    FieldMapping {
        field: StandardField::Encoder,
        key: "ENCODER",
    },
    FieldMapping {
        field: StandardField::BPM,
        key: "BPM",
    },
    FieldMapping {
        field: StandardField::ISRC,
        key: "ISRC",
    },
    FieldMapping {
        field: StandardField::Barcode,
        key: "UPC",
    },
    FieldMapping {
        field: StandardField::Compilation,
        key: "COMPILATION",
    },
    FieldMapping {
        field: StandardField::Mood,
        key: "MOOD",
    },
    FieldMapping {
        field: StandardField::Language,
        key: "LANGUAGE",
    },
    FieldMapping {
        field: StandardField::Label,
        key: "LABEL",
    },
    FieldMapping {
        field: StandardField::CatalogNumber,
        key: "CATALOGNUMBER",
    },
    FieldMapping {
        field: StandardField::Lyrics,
        key: "LYRICS",
    },
    FieldMapping {
        field: StandardField::Work,
        key: "WORK",
    },
    FieldMapping {
        field: StandardField::Movement,
        key: "MOVEMENTNAME",
    },
    FieldMapping {
        field: StandardField::MovementCount,
        key: "MOVEMENTCOUNT",
    },
    FieldMapping {
        field: StandardField::MovementIndex,
        key: "MOVEMENTNUMBER",
    },
    FieldMapping {
        field: StandardField::ReplayGainTrackGain,
        key: "REPLAYGAIN_TRACK_GAIN",
    },
    FieldMapping {
        field: StandardField::ReplayGainTrackPeak,
        key: "REPLAYGAIN_TRACK_PEAK",
    },
    FieldMapping {
        field: StandardField::ReplayGainAlbumGain,
        key: "REPLAYGAIN_ALBUM_GAIN",
    },
    FieldMapping {
        field: StandardField::ReplayGainAlbumPeak,
        key: "REPLAYGAIN_ALBUM_PEAK",
    },
    FieldMapping {
        field: StandardField::MusicBrainzTrackId,
        key: "MUSICBRAINZ_TRACKID",
    },
    FieldMapping {
        field: StandardField::MusicBrainzAlbumId,
        key: "MUSICBRAINZ_ALBUMID",
    },
    FieldMapping {
        field: StandardField::MusicBrainzArtistId,
        key: "MUSICBRAINZ_ARTISTID",
    },
    FieldMapping {
        field: StandardField::MusicBrainzReleaseGroupId,
        key: "MUSICBRAINZ_RELEASEGROUPID",
    },
    FieldMapping {
        field: StandardField::AcoustIdFingerprint,
        key: "ACOUSTID_FINGERPRINT",
    },
    FieldMapping {
        field: StandardField::AcoustIdId,
        key: "ACOUSTID_ID",
    },
];

/// Alternate Vorbis Comment keys that map to the same standard field.
static VORBIS_ALIASES: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::TotalTracks,
        key: "TOTALTRACKS",
    },
    FieldMapping {
        field: StandardField::TotalDiscs,
        key: "TOTALDISCS",
    },
    FieldMapping {
        field: StandardField::CatalogNumber,
        key: "CATALOG NUMBER",
    },
    // MusicBrainz Picard writes "BARCODE" while some other tools use "UPC"
    FieldMapping {
        field: StandardField::Barcode,
        key: "BARCODE",
    },
    // "LABEL" intentionally NOT aliased to Publisher — the primary map
    // already assigns "LABEL" to StandardField::Label (line 349-351).
    // Some taggers use MOVEMENT instead of MOVEMENTNAME
    FieldMapping {
        field: StandardField::Movement,
        key: "MOVEMENT",
    },
];

/// Get the canonical Vorbis Comment key for a standard field.
pub fn standard_to_vorbis(field: &StandardField) -> Option<&'static str> {
    VORBIS_MAP.iter().find(|m| m.field == *field).map(|m| m.key)
}

/// Get the standard field for a Vorbis Comment key (case-insensitive).
pub fn vorbis_to_standard(key: &str) -> Option<StandardField> {
    let upper = key.to_uppercase();
    VORBIS_MAP
        .iter()
        .find(|m| m.key == upper)
        .or_else(|| VORBIS_ALIASES.iter().find(|m| m.key == upper))
        .map(|m| m.field.clone())
}

// ---------------------------------------------------------------------------
// MP4/iTunes atom mappings
// ---------------------------------------------------------------------------

/// Mappings from StandardField to iTunes/MP4 atom keys.
static MP4_MAP: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::Title,
        key: "\u{a9}nam",
    },
    FieldMapping {
        field: StandardField::Artist,
        key: "\u{a9}ART",
    },
    FieldMapping {
        field: StandardField::Album,
        key: "\u{a9}alb",
    },
    FieldMapping {
        field: StandardField::AlbumArtist,
        key: "aART",
    },
    FieldMapping {
        field: StandardField::TrackNumber,
        key: "trkn",
    },
    FieldMapping {
        field: StandardField::DiscNumber,
        key: "disk",
    },
    FieldMapping {
        field: StandardField::Date,
        key: "\u{a9}day",
    },
    FieldMapping {
        field: StandardField::Genre,
        key: "\u{a9}gen",
    },
    FieldMapping {
        field: StandardField::Comment,
        key: "\u{a9}cmt",
    },
    FieldMapping {
        field: StandardField::Description,
        key: "desc",
    },
    FieldMapping {
        field: StandardField::Composer,
        key: "\u{a9}wrt",
    },
    FieldMapping {
        field: StandardField::Encoder,
        key: "\u{a9}too",
    },
    FieldMapping {
        field: StandardField::Copyright,
        key: "cprt",
    },
    FieldMapping {
        field: StandardField::BPM,
        key: "tmpo",
    },
    FieldMapping {
        field: StandardField::Compilation,
        key: "cpil",
    },
    FieldMapping {
        field: StandardField::SortTitle,
        key: "sonm",
    },
    FieldMapping {
        field: StandardField::SortArtist,
        key: "soar",
    },
    FieldMapping {
        field: StandardField::SortAlbum,
        key: "soal",
    },
    FieldMapping {
        field: StandardField::SortAlbumArtist,
        key: "soaa",
    },
    FieldMapping {
        field: StandardField::SortComposer,
        key: "soco",
    },
    // Lyrics atom
    FieldMapping {
        field: StandardField::Lyrics,
        key: "\u{a9}lyr",
    },
    // Classical / Apple Music work and movement atoms
    FieldMapping {
        field: StandardField::Work,
        key: "\u{a9}wrk",
    },
    FieldMapping {
        field: StandardField::Movement,
        key: "\u{a9}mvn",
    },
    FieldMapping {
        field: StandardField::MovementCount,
        key: "\u{a9}mvc",
    },
    FieldMapping {
        field: StandardField::MovementIndex,
        key: "\u{a9}mvi",
    },
    // Publisher/Label atom
    FieldMapping {
        field: StandardField::Publisher,
        key: "\u{a9}pub",
    },
];

/// Mappings from freeform iTunes atom keys to StandardField.
static MP4_FREEFORM_MAP: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::ISRC,
        key: "----:com.apple.itunes:ISRC",
    },
    FieldMapping {
        field: StandardField::Barcode,
        key: "----:com.apple.itunes:UPC",
    },
    FieldMapping {
        field: StandardField::CatalogNumber,
        key: "----:com.apple.itunes:Catalog Number",
    },
    FieldMapping {
        field: StandardField::Label,
        key: "----:com.apple.itunes:Label",
    },
    FieldMapping {
        field: StandardField::Performer,
        key: "----:com.apple.itunes:PERFORMER",
    },
    FieldMapping {
        field: StandardField::Conductor,
        key: "----:com.apple.itunes:CONDUCTOR",
    },
    FieldMapping {
        field: StandardField::Lyricist,
        key: "----:com.apple.itunes:LYRICIST",
    },
    FieldMapping {
        field: StandardField::EncodedBy,
        key: "----:com.apple.itunes:ENCODED BY",
    },
    FieldMapping {
        field: StandardField::Mood,
        key: "----:com.apple.itunes:MOOD",
    },
    FieldMapping {
        field: StandardField::Language,
        key: "----:com.apple.itunes:LANGUAGE",
    },
    FieldMapping {
        field: StandardField::MusicBrainzTrackId,
        key: "----:com.apple.itunes:MusicBrainz Track Id",
    },
    FieldMapping {
        field: StandardField::MusicBrainzAlbumId,
        key: "----:com.apple.itunes:MusicBrainz Album Id",
    },
    FieldMapping {
        field: StandardField::MusicBrainzArtistId,
        key: "----:com.apple.itunes:MusicBrainz Artist Id",
    },
    FieldMapping {
        field: StandardField::MusicBrainzReleaseGroupId,
        key: "----:com.apple.itunes:MusicBrainz Release Group Id",
    },
    FieldMapping {
        field: StandardField::AcoustIdFingerprint,
        key: "----:com.apple.itunes:Acoustid Fingerprint",
    },
    FieldMapping {
        field: StandardField::AcoustIdId,
        key: "----:com.apple.itunes:Acoustid Id",
    },
];

/// Get the MP4 atom key for a standard field.
pub fn standard_to_mp4(field: &StandardField) -> Option<&'static str> {
    MP4_MAP.iter().find(|m| m.field == *field).map(|m| m.key)
}

/// Write-path lookup: maps a standard field to a freeform MP4 atom key
/// (e.g. `"----:com.apple.itunes:ISRC"`).
pub fn standard_to_mp4_freeform(field: &StandardField) -> Option<&'static str> {
    MP4_FREEFORM_MAP
        .iter()
        .find(|m| m.field == *field)
        .map(|m| m.key)
}

/// Get the standard field for an MP4 atom key.
pub fn mp4_to_standard(atom: &str) -> Option<StandardField> {
    MP4_MAP
        .iter()
        .find(|m| m.key == atom)
        .or_else(|| {
            // Freeform keys: case-insensitive match (mean application name varies)
            MP4_FREEFORM_MAP
                .iter()
                .find(|m| m.key.eq_ignore_ascii_case(atom))
        })
        .map(|m| m.field.clone())
}

// ---------------------------------------------------------------------------
// APEv2 key mappings
// ---------------------------------------------------------------------------

/// Mappings from StandardField to APEv2 item keys.
static APE_MAP: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::Title,
        key: "Title",
    },
    FieldMapping {
        field: StandardField::Artist,
        key: "Artist",
    },
    FieldMapping {
        field: StandardField::Album,
        key: "Album",
    },
    FieldMapping {
        field: StandardField::AlbumArtist,
        key: "Album Artist",
    },
    FieldMapping {
        field: StandardField::TrackNumber,
        key: "Track",
    },
    FieldMapping {
        field: StandardField::DiscNumber,
        key: "Disc",
    },
    FieldMapping {
        field: StandardField::Date,
        key: "Year",
    },
    FieldMapping {
        field: StandardField::Genre,
        key: "Genre",
    },
    FieldMapping {
        field: StandardField::Comment,
        key: "Comment",
    },
    FieldMapping {
        field: StandardField::Description,
        key: "Description",
    },
    FieldMapping {
        field: StandardField::Composer,
        key: "Composer",
    },
    FieldMapping {
        field: StandardField::Conductor,
        key: "Conductor",
    },
    FieldMapping {
        field: StandardField::Publisher,
        key: "Publisher",
    },
    FieldMapping {
        field: StandardField::Copyright,
        key: "Copyright",
    },
    FieldMapping {
        field: StandardField::ISRC,
        key: "ISRC",
    },
    FieldMapping {
        field: StandardField::Barcode,
        key: "UPC",
    },
    FieldMapping {
        field: StandardField::Compilation,
        key: "Compilation",
    },
    FieldMapping {
        field: StandardField::Encoder,
        key: "Encoder",
    },
    FieldMapping {
        field: StandardField::EncodedBy,
        key: "EncodedBy",
    },
    FieldMapping {
        field: StandardField::BPM,
        key: "BPM",
    },
    FieldMapping {
        field: StandardField::Mood,
        key: "Mood",
    },
    FieldMapping {
        field: StandardField::Language,
        key: "Language",
    },
    FieldMapping {
        field: StandardField::Label,
        key: "Label",
    },
    FieldMapping {
        field: StandardField::CatalogNumber,
        key: "Catalog Number",
    },
    FieldMapping {
        field: StandardField::Lyrics,
        key: "Lyrics",
    },
    FieldMapping {
        field: StandardField::Work,
        key: "Work",
    },
    FieldMapping {
        field: StandardField::Movement,
        key: "Movement",
    },
    FieldMapping {
        field: StandardField::MovementCount,
        key: "Movement Count",
    },
    FieldMapping {
        field: StandardField::MovementIndex,
        key: "Movement Index",
    },
    FieldMapping {
        field: StandardField::ReplayGainTrackGain,
        key: "REPLAYGAIN_TRACK_GAIN",
    },
    FieldMapping {
        field: StandardField::ReplayGainTrackPeak,
        key: "REPLAYGAIN_TRACK_PEAK",
    },
    FieldMapping {
        field: StandardField::ReplayGainAlbumGain,
        key: "REPLAYGAIN_ALBUM_GAIN",
    },
    FieldMapping {
        field: StandardField::ReplayGainAlbumPeak,
        key: "REPLAYGAIN_ALBUM_PEAK",
    },
    FieldMapping {
        field: StandardField::MusicBrainzTrackId,
        key: "MUSICBRAINZ_TRACKID",
    },
    FieldMapping {
        field: StandardField::MusicBrainzAlbumId,
        key: "MUSICBRAINZ_ALBUMID",
    },
    FieldMapping {
        field: StandardField::MusicBrainzArtistId,
        key: "MUSICBRAINZ_ARTISTID",
    },
    FieldMapping {
        field: StandardField::MusicBrainzReleaseGroupId,
        key: "MUSICBRAINZ_RELEASEGROUPID",
    },
    FieldMapping {
        field: StandardField::AcoustIdFingerprint,
        key: "ACOUSTID_FINGERPRINT",
    },
    FieldMapping {
        field: StandardField::AcoustIdId,
        key: "ACOUSTID_ID",
    },
    FieldMapping {
        field: StandardField::SortTitle,
        key: "TITLESORTORDER",
    },
    FieldMapping {
        field: StandardField::SortArtist,
        key: "ARTISTSORTORDER",
    },
    FieldMapping {
        field: StandardField::SortAlbum,
        key: "ALBUMSORTORDER",
    },
    FieldMapping {
        field: StandardField::SortAlbumArtist,
        key: "ALBUMARTISTSORTORDER",
    },
    FieldMapping {
        field: StandardField::SortComposer,
        key: "COMPOSERSORTORDER",
    },
];

/// Alternate APEv2 keys that map to the same standard field.
static APE_ALIASES: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::AlbumArtist,
        key: "albumartist",
    },
    FieldMapping {
        field: StandardField::TrackNumber,
        key: "tracknumber",
    },
    FieldMapping {
        field: StandardField::TotalTracks,
        key: "totaltracks",
    },
    FieldMapping {
        field: StandardField::DiscNumber,
        key: "discnumber",
    },
    FieldMapping {
        field: StandardField::TotalDiscs,
        key: "totaldiscs",
    },
    FieldMapping {
        field: StandardField::Date,
        key: "date",
    },
];

/// Get the APEv2 item key for a standard field.
pub fn standard_to_ape(field: &StandardField) -> Option<&'static str> {
    APE_MAP.iter().find(|m| m.field == *field).map(|m| m.key)
}

/// Get the standard field for an APEv2 item key (case-insensitive).
pub fn ape_to_standard(key: &str) -> Option<StandardField> {
    APE_MAP
        .iter()
        .find(|m| m.key.eq_ignore_ascii_case(key))
        .or_else(|| APE_ALIASES.iter().find(|m| m.key.eq_ignore_ascii_case(key)))
        .map(|m| m.field.clone())
}

// ---------------------------------------------------------------------------
// ASF attribute mappings
// ---------------------------------------------------------------------------

/// Mappings from StandardField to ASF/WMA attribute names.
static ASF_MAP: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::Title,
        key: "Title",
    },
    FieldMapping {
        field: StandardField::Artist,
        key: "Author",
    },
    FieldMapping {
        field: StandardField::Album,
        key: "WM/AlbumTitle",
    },
    FieldMapping {
        field: StandardField::AlbumArtist,
        key: "WM/AlbumArtist",
    },
    FieldMapping {
        field: StandardField::TrackNumber,
        key: "WM/TrackNumber",
    },
    FieldMapping {
        field: StandardField::DiscNumber,
        key: "WM/PartOfSet",
    },
    FieldMapping {
        field: StandardField::Date,
        key: "WM/Year",
    },
    FieldMapping {
        field: StandardField::Genre,
        key: "WM/Genre",
    },
    FieldMapping {
        field: StandardField::Description,
        key: "Description",
    },
    FieldMapping {
        field: StandardField::Comment,
        key: "WM/Text",
    },
    FieldMapping {
        field: StandardField::Composer,
        key: "WM/Composer",
    },
    FieldMapping {
        field: StandardField::Conductor,
        key: "WM/Conductor",
    },
    FieldMapping {
        field: StandardField::Publisher,
        key: "WM/Publisher",
    },
    FieldMapping {
        field: StandardField::Copyright,
        key: "Copyright",
    },
    FieldMapping {
        field: StandardField::EncodedBy,
        key: "WM/EncodedBy",
    },
    FieldMapping {
        field: StandardField::ISRC,
        key: "WM/ISRC",
    },
    FieldMapping {
        field: StandardField::BPM,
        key: "WM/BeatsPerMinute",
    },
    FieldMapping {
        field: StandardField::Mood,
        key: "WM/Mood",
    },
    FieldMapping {
        field: StandardField::Language,
        key: "WM/Language",
    },
    FieldMapping {
        field: StandardField::Lyrics,
        key: "WM/Lyrics",
    },
    FieldMapping {
        field: StandardField::Compilation,
        key: "WM/ContentGroupDescription",
    },
    FieldMapping {
        field: StandardField::Label,
        key: "WM/Label",
    },
    FieldMapping {
        field: StandardField::Performer,
        key: "WM/Performer",
    },
    FieldMapping {
        field: StandardField::Work,
        key: "WM/Work",
    },
    FieldMapping {
        field: StandardField::SortTitle,
        key: "WM/TitleSortOrder",
    },
    FieldMapping {
        field: StandardField::SortArtist,
        key: "WM/ArtistSortOrder",
    },
    FieldMapping {
        field: StandardField::SortAlbum,
        key: "WM/AlbumSortOrder",
    },
    FieldMapping {
        field: StandardField::SortAlbumArtist,
        key: "WM/AlbumArtistSortOrder",
    },
    FieldMapping {
        field: StandardField::SortComposer,
        key: "WM/ComposerSortOrder",
    },
];

/// Alternate ASF attribute names that map to the same standard field.
static ASF_ALIASES: &[FieldMapping] = &[
    FieldMapping {
        field: StandardField::Comment,
        key: "comment",
    },
    FieldMapping {
        field: StandardField::Copyright,
        key: "copyright",
    },
    FieldMapping {
        field: StandardField::Date,
        key: "date",
    },
    FieldMapping {
        field: StandardField::ISRC,
        key: "isrc",
    },
    FieldMapping {
        field: StandardField::Lyrics,
        key: "lyrics",
    },
    FieldMapping {
        field: StandardField::TotalDiscs,
        key: "WM/DiscTotal",
    },
    FieldMapping {
        field: StandardField::TotalTracks,
        key: "WM/TrackCount",
    },
    FieldMapping {
        field: StandardField::Encoder,
        key: "WM/EncodingSettings",
    },
    FieldMapping {
        field: StandardField::CatalogNumber,
        key: "Catalog Number",
    },
    FieldMapping {
        field: StandardField::Barcode,
        key: "UPC",
    },
    FieldMapping {
        field: StandardField::ReplayGainTrackGain,
        key: "REPLAYGAIN_TRACK_GAIN",
    },
    FieldMapping {
        field: StandardField::ReplayGainTrackPeak,
        key: "REPLAYGAIN_TRACK_PEAK",
    },
    FieldMapping {
        field: StandardField::ReplayGainAlbumGain,
        key: "REPLAYGAIN_ALBUM_GAIN",
    },
    FieldMapping {
        field: StandardField::ReplayGainAlbumPeak,
        key: "REPLAYGAIN_ALBUM_PEAK",
    },
];

/// Get the ASF attribute name for a standard field.
pub fn standard_to_asf(field: &StandardField) -> Option<&'static str> {
    ASF_MAP.iter().find(|m| m.field == *field).map(|m| m.key)
}

/// Write-path lookup: maps a standard field to an ASF alias attribute name
/// (e.g. TotalTracks → `"WM/TrackCount"`, ReplayGain → `"REPLAYGAIN_TRACK_GAIN"`).
pub fn standard_to_asf_alias(field: &StandardField) -> Option<&'static str> {
    ASF_ALIASES
        .iter()
        .find(|m| m.field == *field)
        .map(|m| m.key)
}

/// Get the standard field for an ASF attribute name.
/// Matching is case-insensitive per the ASF specification, which treats
/// attribute names as case-insensitive strings.
pub fn asf_to_standard(attr: &str) -> Option<StandardField> {
    ASF_MAP
        .iter()
        .find(|m| m.key.eq_ignore_ascii_case(attr))
        .or_else(|| {
            ASF_ALIASES
                .iter()
                .find(|m| m.key.eq_ignore_ascii_case(attr))
        })
        .map(|m| m.field.clone())
}
