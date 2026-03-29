#![cfg(target_arch = "wasm32")]

// Writes a full set of standard music metadata fields to every supported format
// via the WASM AudioFile API, saves, reloads, and verifies all tag values
// survived the round-trip.
//
// Existing WASM tests write at most 2 fields per test. This file ensures the
// complete set of standard fields (15-21 per tag system) survives save+reload.
//
// All operations use in-memory byte buffers embedded at compile time.
// No filesystem writes or original file modifications occur.

use wasm_bindgen_test::*;

// ---------------------------------------------------------------------------
// Test fixtures (compile-time embedded, read-only)
// ---------------------------------------------------------------------------

const MP3_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.mp3");
const FLAC_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.flac");
const M4A_BYTES: &[u8] = include_bytes!("../../tests/data/has-tags.m4a");
const OGG_BYTES: &[u8] = include_bytes!("../../tests/data/multipagecomment.ogg");
const WMA_BYTES: &[u8] = include_bytes!("../../tests/data/silence-1.wma");
const AIFF_BYTES: &[u8] = include_bytes!("../../tests/data/with-id3.aif");
const WAV_BYTES: &[u8] = include_bytes!("../../tests/data/silence-2s-PCM-44100-16-ID3v23.wav");
const DSF_BYTES: &[u8] = include_bytes!("../../tests/data/with-id3.dsf");
const DFF_BYTES: &[u8] = include_bytes!("../../tests/data/5644800-2ch-s01-silence.dff");
const OPUS_BYTES: &[u8] = include_bytes!("../../tests/data/example.opus");
const SPX_BYTES: &[u8] = include_bytes!("../../tests/data/empty.spx");
const OGA_BYTES: &[u8] = include_bytes!("../../tests/data/empty.oggflac");
const OGV_BYTES: &[u8] = include_bytes!("../../tests/data/sample.oggtheora");
const APE_BYTES: &[u8] = include_bytes!("../../tests/data/mac-399.ape");
const MPC_BYTES: &[u8] = include_bytes!("../../tests/data/click.mpc");
const WV_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.wv");
const OFR_BYTES: &[u8] = include_bytes!("../../tests/data/silence-2s-44100-16.ofr");
const TAK_BYTES: &[u8] = include_bytes!("../../tests/data/has-tags.tak");
const TTA_BYTES: &[u8] = include_bytes!("../../tests/data/empty.tta");

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
// Core helper
// ---------------------------------------------------------------------------

/// Writes all given tags, saves, reloads from the saved bytes, and verifies
/// every tag value survived.
fn write_and_verify(bytes: &[u8], ext: &str, tags: &[(&str, &str)], label: &str) {
    let filename = format!("test.{}", ext);

    let mut file = audex_wasm::AudioFile::new(bytes, Some(filename.clone()))
        .unwrap_or_else(|e| panic!("{}: load failed: {:?}", label, e));

    file.add_tags().ok();

    for &(key, value) in tags {
        file.set_single(key, value)
            .unwrap_or_else(|e| panic!("{}: set '{}' failed: {:?}", label, key, e));
    }

    let saved = file
        .save()
        .unwrap_or_else(|e| panic!("{}: save failed: {:?}", label, e));

    let reloaded = audex_wasm::AudioFile::new(&saved, Some(filename))
        .unwrap_or_else(|e| panic!("{}: reload failed: {:?}", label, e));

    for &(key, expected) in tags {
        let val = reloaded.get_first(key);
        assert_eq!(
            val.as_deref(),
            Some(expected),
            "{}: tag '{}' expected '{}', got {:?}",
            label,
            key,
            expected,
            val,
        );
    }
}

// ===========================================================================
// ID3v2 formats
// ===========================================================================

#[wasm_bindgen_test]
fn comprehensive_mp3() {
    write_and_verify(MP3_BYTES, "mp3", ID3V2_TAGS, "MP3");
}

#[wasm_bindgen_test]
fn comprehensive_aiff() {
    write_and_verify(AIFF_BYTES, "aif", ID3V2_TAGS, "AIFF");
}

#[wasm_bindgen_test]
fn comprehensive_wav() {
    write_and_verify(WAV_BYTES, "wav", ID3V2_TAGS, "WAV");
}

#[wasm_bindgen_test]
fn comprehensive_dsf() {
    write_and_verify(DSF_BYTES, "dsf", ID3V2_TAGS, "DSF");
}

#[wasm_bindgen_test]
fn comprehensive_dsdiff() {
    write_and_verify(DFF_BYTES, "dff", ID3V2_TAGS, "DSDIFF");
}

// ===========================================================================
// Vorbis Comment formats
// ===========================================================================

#[wasm_bindgen_test]
fn comprehensive_flac() {
    write_and_verify(FLAC_BYTES, "flac", VORBIS_TAGS, "FLAC");
}

#[wasm_bindgen_test]
fn comprehensive_ogg_vorbis() {
    write_and_verify(OGG_BYTES, "ogg", VORBIS_TAGS, "OGG Vorbis");
}

#[wasm_bindgen_test]
fn comprehensive_opus() {
    write_and_verify(OPUS_BYTES, "opus", VORBIS_TAGS, "Opus");
}

#[wasm_bindgen_test]
fn comprehensive_speex() {
    write_and_verify(SPX_BYTES, "spx", VORBIS_TAGS, "Speex");
}

#[wasm_bindgen_test]
fn comprehensive_ogg_flac() {
    write_and_verify(OGA_BYTES, "oga", VORBIS_TAGS, "OGG FLAC");
}

#[wasm_bindgen_test]
fn comprehensive_ogg_theora() {
    write_and_verify(OGV_BYTES, "ogv", VORBIS_TAGS, "OGG Theora");
}

// ===========================================================================
// MP4 atoms
// ===========================================================================

#[wasm_bindgen_test]
fn comprehensive_m4a() {
    write_and_verify(M4A_BYTES, "m4a", MP4_TAGS, "M4A");
}

// ===========================================================================
// APEv2 formats
// ===========================================================================

#[wasm_bindgen_test]
fn comprehensive_monkeysaudio() {
    write_and_verify(APE_BYTES, "ape", APE_TAGS, "MonkeysAudio");
}

#[wasm_bindgen_test]
fn comprehensive_musepack() {
    write_and_verify(MPC_BYTES, "mpc", APE_TAGS, "Musepack");
}

#[wasm_bindgen_test]
fn comprehensive_wavpack() {
    write_and_verify(WV_BYTES, "wv", APE_TAGS, "WavPack");
}

#[wasm_bindgen_test]
fn comprehensive_optimfrog() {
    write_and_verify(OFR_BYTES, "ofr", APE_TAGS, "OptimFROG");
}

#[wasm_bindgen_test]
fn comprehensive_tak() {
    write_and_verify(TAK_BYTES, "tak", APE_TAGS, "TAK");
}

#[wasm_bindgen_test]
fn comprehensive_trueaudio() {
    write_and_verify(TTA_BYTES, "tta", APE_TAGS, "TrueAudio");
}

// ===========================================================================
// ASF / WMA
// ===========================================================================

#[wasm_bindgen_test]
fn comprehensive_wma() {
    write_and_verify(WMA_BYTES, "wma", ASF_TAGS, "WMA");
}
