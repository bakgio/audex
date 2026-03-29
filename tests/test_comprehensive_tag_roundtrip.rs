/// Writes a full set of standard music metadata fields to every supported format
/// via the unified File API, saves to an in-memory buffer, reloads, and verifies
/// that every written tag value survived the round-trip.
///
/// Each tag system (ID3v2, Vorbis Comment, MP4, APEv2, ASF) has its own key
/// vocabulary; we use the native keys so the test validates real-world usage.
/// Original test data files are never modified — all writes target in-memory cursors.
use audex::{File, StreamInfo};
use std::io::Cursor;
use std::path::PathBuf;

fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

// ---------------------------------------------------------------------------
// Tag vocabularies per tag system
// ---------------------------------------------------------------------------

const ID3V2_TAGS: &[(&str, &str)] = &[
    ("TIT2", "Roundtrip Title"),
    ("TPE1", "Roundtrip Artist"),
    ("TALB", "Roundtrip Album"),
    ("TPE2", "Roundtrip Album Artist"),
    ("TRCK", "3/12"),
    ("TPOS", "1/2"),
    ("TCON", "Electronic"),
    ("TDRC", "2024-06-15"),
    ("TCOM", "Roundtrip Composer"),
    ("TCOP", "2024 Roundtrip Label"),
    ("TSRC", "USRC17000001"),
    ("TPUB", "Roundtrip Publisher"),
    ("TLAN", "eng"),
    ("TBPM", "128"),
    ("TIT3", "Roundtrip Subtitle"),
    ("TSOA", "Album Sort Key"),
    ("TSOP", "Artist Sort Key"),
    ("TSOT", "Title Sort Key"),
];

const VORBIS_TAGS: &[(&str, &str)] = &[
    ("TITLE", "Roundtrip Title"),
    ("ARTIST", "Roundtrip Artist"),
    ("ALBUM", "Roundtrip Album"),
    ("ALBUMARTIST", "Roundtrip Album Artist"),
    ("TRACKNUMBER", "3"),
    ("TOTALTRACKS", "12"),
    ("DISCNUMBER", "1"),
    ("TOTALDISCS", "2"),
    ("GENRE", "Electronic"),
    ("DATE", "2024-06-15"),
    ("COMPOSER", "Roundtrip Composer"),
    ("COPYRIGHT", "2024 Roundtrip Label"),
    ("ISRC", "USRC17000001"),
    ("PUBLISHER", "Roundtrip Publisher"),
    ("LANGUAGE", "eng"),
    ("BPM", "128"),
    ("COMMENT", "Roundtrip comment text"),
    ("DESCRIPTION", "Roundtrip description"),
    ("LABEL", "Roundtrip Label"),
    ("CATALOGNUMBER", "CAT-001"),
    ("MOOD", "Energetic"),
];

const MP4_TAGS: &[(&str, &str)] = &[
    ("\u{00a9}nam", "Roundtrip Title"),
    ("\u{00a9}ART", "Roundtrip Artist"),
    ("\u{00a9}alb", "Roundtrip Album"),
    ("aART", "Roundtrip Album Artist"),
    ("\u{00a9}gen", "Electronic"),
    ("\u{00a9}day", "2024-06-15"),
    ("\u{00a9}wrt", "Roundtrip Composer"),
    ("cprt", "2024 Roundtrip Label"),
    ("\u{00a9}cmt", "Roundtrip comment text"),
    ("\u{00a9}lyr", "Roundtrip lyrics text"),
    ("desc", "Roundtrip description"),
    ("\u{00a9}grp", "Roundtrip Grouping"),
    ("soal", "Album Sort Key"),
    ("soar", "Artist Sort Key"),
    ("sonm", "Title Sort Key"),
];

const APE_TAGS: &[(&str, &str)] = &[
    ("Title", "Roundtrip Title"),
    ("Artist", "Roundtrip Artist"),
    ("Album", "Roundtrip Album"),
    ("Album Artist", "Roundtrip Album Artist"),
    ("Track", "3/12"),
    ("Disc", "1/2"),
    ("Genre", "Electronic"),
    ("Year", "2024"),
    ("Composer", "Roundtrip Composer"),
    ("Copyright", "2024 Roundtrip Label"),
    ("ISRC", "USRC17000001"),
    ("Publisher", "Roundtrip Publisher"),
    ("Language", "eng"),
    ("BPM", "128"),
    ("Comment", "Roundtrip comment text"),
    ("Subtitle", "Roundtrip Subtitle"),
    ("Catalog", "CAT-001"),
];

const ASF_TAGS: &[(&str, &str)] = &[
    ("Title", "Roundtrip Title"),
    ("Author", "Roundtrip Artist"),
    ("WM/AlbumTitle", "Roundtrip Album"),
    ("WM/AlbumArtist", "Roundtrip Album Artist"),
    ("WM/TrackNumber", "3"),
    ("WM/PartOfSet", "1"),
    ("WM/Genre", "Electronic"),
    ("WM/Year", "2024"),
    ("WM/Composer", "Roundtrip Composer"),
    ("Copyright", "2024 Roundtrip Label"),
    ("WM/ISRC", "USRC17000001"),
    ("WM/Publisher", "Roundtrip Publisher"),
    ("WM/Language", "eng"),
    ("WM/BeatsPerMinute", "128"),
    ("WM/Mood", "Energetic"),
    ("Description", "Roundtrip description"),
];

// ---------------------------------------------------------------------------
// Core test helpers
// ---------------------------------------------------------------------------

/// Writes all given tags to the file, saves via save_to_writer, reloads, and
/// asserts every tag value survived. Also checks stream info is preserved.
fn write_and_verify(filename: &str, tags: &[(&str, &str)]) {
    let path = data_path(filename);
    if !path.exists() {
        eprintln!("skipping {}: file not found", filename);
        return;
    }

    let data = std::fs::read(&path).unwrap();

    let mut file = File::load_from_reader(Cursor::new(data.clone()), Some(path.clone()))
        .unwrap_or_else(|e| panic!("{}: load failed: {}", filename, e));

    let orig_rate = file.info().sample_rate();
    let orig_ch = file.info().channels();

    // Ensure tag container exists before writing
    if !file.has_tags() {
        file.add_tags()
            .unwrap_or_else(|e| panic!("{}: add_tags failed: {}", filename, e));
    }

    for &(key, value) in tags {
        file.set_single(key, value.to_string())
            .unwrap_or_else(|e| panic!("{}: set '{}' failed: {}", filename, key, e));
    }

    let mut out = Cursor::new(data);
    file.save_to_writer(&mut out)
        .unwrap_or_else(|e| panic!("{}: save_to_writer failed: {}", filename, e));

    let reloaded = File::load_from_reader(Cursor::new(out.into_inner()), Some(path))
        .unwrap_or_else(|e| panic!("{}: reload failed: {}", filename, e));

    // Stream info must be preserved
    assert_eq!(
        orig_rate,
        reloaded.info().sample_rate(),
        "{}: sample_rate changed after tagging",
        filename
    );
    assert_eq!(
        orig_ch,
        reloaded.info().channels(),
        "{}: channels changed after tagging",
        filename
    );

    // Every written tag must survive
    for &(key, expected) in tags {
        let values = reloaded.get(key);
        assert!(
            values.is_some(),
            "{}: tag '{}' missing after roundtrip",
            filename,
            key
        );
        let values = values.unwrap();
        assert!(
            values.iter().any(|v| v == expected),
            "{}: tag '{}' expected '{}', got {:?}",
            filename,
            key,
            expected,
            values
        );
    }
}

/// Same as write_and_verify but tests multi-value fields via `set()`.
fn write_multivalue_and_verify(
    filename: &str,
    single_tags: &[(&str, &str)],
    multi_tags: &[(&str, &[&str])],
) {
    let path = data_path(filename);
    if !path.exists() {
        eprintln!("skipping {}: file not found", filename);
        return;
    }

    let data = std::fs::read(&path).unwrap();

    let mut file = File::load_from_reader(Cursor::new(data.clone()), Some(path.clone()))
        .unwrap_or_else(|e| panic!("{}: load failed: {}", filename, e));

    if !file.has_tags() {
        file.add_tags()
            .unwrap_or_else(|e| panic!("{}: add_tags failed: {}", filename, e));
    }

    for &(key, value) in single_tags {
        file.set_single(key, value.to_string())
            .unwrap_or_else(|e| panic!("{}: set '{}' failed: {}", filename, key, e));
    }

    for &(key, values) in multi_tags {
        let vals: Vec<String> = values.iter().map(|s| s.to_string()).collect();
        file.set(key, vals)
            .unwrap_or_else(|e| panic!("{}: set multi '{}' failed: {}", filename, key, e));
    }

    let mut out = Cursor::new(data);
    file.save_to_writer(&mut out)
        .unwrap_or_else(|e| panic!("{}: save_to_writer failed: {}", filename, e));

    let reloaded = File::load_from_reader(Cursor::new(out.into_inner()), Some(path))
        .unwrap_or_else(|e| panic!("{}: reload failed: {}", filename, e));

    for &(key, expected) in single_tags {
        let values = reloaded.get(key);
        assert!(
            values.is_some(),
            "{}: tag '{}' missing after roundtrip",
            filename,
            key
        );
        assert!(
            values.unwrap().iter().any(|v| v == expected),
            "{}: tag '{}' value mismatch",
            filename,
            key
        );
    }

    for &(key, expected_vals) in multi_tags {
        let values = reloaded.get(key);
        assert!(
            values.is_some(),
            "{}: multi-value tag '{}' missing after roundtrip",
            filename,
            key
        );
        let values = values.unwrap();
        for expected in expected_vals {
            assert!(
                values.iter().any(|v| v == expected),
                "{}: multi-value tag '{}' missing value '{}', got {:?}",
                filename,
                key,
                expected,
                values
            );
        }
    }
}

// ===========================================================================
// ID3v2 formats
// ===========================================================================

#[test]
fn comprehensive_mp3() {
    write_and_verify("silence-44-s.mp3", ID3V2_TAGS);
}

#[test]
fn comprehensive_aiff() {
    write_and_verify("with-id3.aif", ID3V2_TAGS);
}

#[test]
fn comprehensive_wav() {
    write_and_verify("silence-2s-PCM-44100-16-ID3v23.wav", ID3V2_TAGS);
}

#[test]
fn comprehensive_dsf() {
    write_and_verify("with-id3.dsf", ID3V2_TAGS);
}

#[test]
fn comprehensive_dsdiff() {
    write_and_verify("5644800-2ch-s01-silence.dff", ID3V2_TAGS);
}

// ===========================================================================
// Vorbis Comment formats
// ===========================================================================

#[test]
fn comprehensive_flac() {
    write_and_verify("silence-44-s.flac", VORBIS_TAGS);
}

#[test]
fn comprehensive_ogg_vorbis() {
    write_and_verify("multipagecomment.ogg", VORBIS_TAGS);
}

#[test]
fn comprehensive_opus() {
    write_and_verify("example.opus", VORBIS_TAGS);
}

#[test]
fn comprehensive_speex() {
    write_and_verify("empty.spx", VORBIS_TAGS);
}

#[test]
fn comprehensive_ogg_flac() {
    write_and_verify("empty.oggflac", VORBIS_TAGS);
}

#[test]
fn comprehensive_ogg_theora() {
    write_and_verify("sample.oggtheora", VORBIS_TAGS);
}

// ===========================================================================
// MP4 atoms
// ===========================================================================

#[test]
fn comprehensive_m4a() {
    write_and_verify("has-tags.m4a", MP4_TAGS);
}

// ===========================================================================
// APEv2 formats
// ===========================================================================

#[test]
fn comprehensive_monkeysaudio() {
    write_and_verify("mac-399.ape", APE_TAGS);
}

#[test]
fn comprehensive_musepack() {
    write_and_verify("click.mpc", APE_TAGS);
}

#[test]
fn comprehensive_wavpack() {
    write_and_verify("silence-44-s.wv", APE_TAGS);
}

#[test]
fn comprehensive_optimfrog() {
    write_and_verify("silence-2s-44100-16.ofr", APE_TAGS);
}

#[test]
fn comprehensive_tak() {
    write_and_verify("has-tags.tak", APE_TAGS);
}

#[test]
fn comprehensive_trueaudio() {
    write_and_verify("empty.tta", APE_TAGS);
}

// ===========================================================================
// ASF / WMA
// ===========================================================================

#[test]
fn comprehensive_wma() {
    write_and_verify("silence-1.wma", ASF_TAGS);
}

// ===========================================================================
// Multi-value tag tests (representative formats per tag system)
// ===========================================================================

#[test]
fn multivalue_flac() {
    write_multivalue_and_verify(
        "silence-44-s.flac",
        &[("TITLE", "Multi Test")],
        &[
            ("ARTIST", &["Artist One", "Artist Two", "Artist Three"]),
            ("GENRE", &["Electronic", "Ambient"]),
        ],
    );
}

#[test]
fn multivalue_m4a() {
    write_multivalue_and_verify(
        "has-tags.m4a",
        &[("\u{00a9}nam", "Multi Test")],
        &[
            ("\u{00a9}ART", &["Artist One", "Artist Two"]),
            ("\u{00a9}gen", &["Electronic", "Ambient"]),
        ],
    );
}

#[test]
fn multivalue_ogg_vorbis() {
    write_multivalue_and_verify(
        "multipagecomment.ogg",
        &[("TITLE", "Multi Test")],
        &[
            ("ARTIST", &["Artist One", "Artist Two"]),
            ("GENRE", &["Electronic", "Ambient"]),
        ],
    );
}

#[test]
fn multivalue_wavpack() {
    write_multivalue_and_verify(
        "silence-44-s.wv",
        &[("Title", "Multi Test")],
        &[
            ("Artist", &["Artist One", "Artist Two"]),
            ("Genre", &["Electronic", "Ambient"]),
        ],
    );
}
