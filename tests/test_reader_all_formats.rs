/// Verifies that File::load_from_reader produces equivalent results to
/// File::load (path-based) for every supported audio format. This catches
/// bugs where the reader path disagrees with the path-based path on format
/// detection, stream info, or tag keys.
use audex::{File, StreamInfo};
use std::io::Cursor;
use std::path::PathBuf;

fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Load a file via both path and reader, then assert that format name,
/// stream info, and tag keys are equivalent.
fn assert_reader_matches_path(filename: &str, expected_format: &str) {
    let path = data_path(filename);
    if !path.exists() {
        return;
    }

    let path_loaded = File::load(&path).expect("path-based load");

    let data = std::fs::read(&path).unwrap();
    let reader_loaded =
        File::load_from_reader(Cursor::new(data), Some(path.clone())).expect("reader-based load");

    // Format detection must agree
    assert!(
        reader_loaded.format_name().contains(expected_format),
        "{}: expected format containing '{}', got '{}'",
        filename,
        expected_format,
        reader_loaded.format_name()
    );
    assert_eq!(
        path_loaded.format_name(),
        reader_loaded.format_name(),
        "{}: format name mismatch between path and reader loading",
        filename,
    );

    // Stream info must match
    assert_eq!(
        path_loaded.info().sample_rate(),
        reader_loaded.info().sample_rate(),
        "{}: sample_rate mismatch",
        filename,
    );
    assert_eq!(
        path_loaded.info().channels(),
        reader_loaded.info().channels(),
        "{}: channels mismatch",
        filename,
    );
    assert_eq!(
        path_loaded.info().bits_per_sample(),
        reader_loaded.info().bits_per_sample(),
        "{}: bits_per_sample mismatch",
        filename,
    );

    // Tag key sets must match
    let mut path_keys: Vec<String> = path_loaded.keys();
    let mut reader_keys: Vec<String> = reader_loaded.keys();
    path_keys.sort();
    reader_keys.sort();
    assert_eq!(
        path_keys, reader_keys,
        "{}: tag keys mismatch between path and reader loading",
        filename,
    );
}

// ---------------------------------------------------------------------------
// ID3v2-based formats
// ---------------------------------------------------------------------------

#[test]
fn reader_matches_path_mp3() {
    assert_reader_matches_path("silence-44-s.mp3", "MP3");
}

#[test]
fn reader_matches_path_aiff() {
    assert_reader_matches_path("with-id3.aif", "AIFF");
}

#[test]
fn reader_matches_path_wave() {
    assert_reader_matches_path("silence-2s-PCM-44100-16-ID3v23.wav", "WAVE");
}

#[test]
fn reader_matches_path_dsf() {
    assert_reader_matches_path("with-id3.dsf", "DSF");
}

#[test]
fn reader_matches_path_dsdiff() {
    assert_reader_matches_path("5644800-2ch-s01-silence.dff", "DSDIFF");
}

// ---------------------------------------------------------------------------
// Vorbis Comment-based formats
// ---------------------------------------------------------------------------

#[test]
fn reader_matches_path_flac() {
    assert_reader_matches_path("silence-44-s.flac", "FLAC");
}

#[test]
fn reader_matches_path_ogg_vorbis() {
    assert_reader_matches_path("multipagecomment.ogg", "Vorbis");
}

#[test]
fn reader_matches_path_ogg_opus() {
    assert_reader_matches_path("example.opus", "Opus");
}

#[test]
fn reader_matches_path_ogg_speex() {
    assert_reader_matches_path("empty.spx", "Speex");
}

#[test]
fn reader_matches_path_ogg_flac() {
    assert_reader_matches_path("empty.oggflac", "OggFlac");
}

#[test]
fn reader_matches_path_ogg_theora() {
    assert_reader_matches_path("sample.oggtheora", "Theora");
}

// ---------------------------------------------------------------------------
// MP4 atoms
// ---------------------------------------------------------------------------

#[test]
fn reader_matches_path_m4a() {
    assert_reader_matches_path("has-tags.m4a", "MP4");
}

// ---------------------------------------------------------------------------
// ASF / WMA attributes
// ---------------------------------------------------------------------------

#[test]
fn reader_matches_path_asf() {
    assert_reader_matches_path("silence-1.wma", "ASF");
}

// ---------------------------------------------------------------------------
// APEv2-based formats
// ---------------------------------------------------------------------------

#[test]
fn reader_matches_path_monkeysaudio() {
    assert_reader_matches_path("mac-399.ape", "MonkeysAudio");
}

#[test]
fn reader_matches_path_musepack() {
    assert_reader_matches_path("click.mpc", "Musepack");
}

#[test]
fn reader_matches_path_wavpack() {
    assert_reader_matches_path("silence-44-s.wv", "WavPack");
}

#[test]
fn reader_matches_path_optimfrog() {
    assert_reader_matches_path("silence-2s-44100-16.ofr", "OptimFROG");
}

#[test]
fn reader_matches_path_tak() {
    assert_reader_matches_path("has-tags.tak", "TAK");
}

#[test]
fn reader_matches_path_trueaudio() {
    assert_reader_matches_path("empty.tta", "TrueAudio");
}
