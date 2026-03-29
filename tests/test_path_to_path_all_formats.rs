/// Path-to-path round-trip tests for all supported formats.
///
/// For each format: load from the original path, save to a temporary copy,
/// reload from the copy, and verify that stream info and all original tag
/// keys survive the round-trip. Original test data is never modified.
///
/// Note: save operations may add encoder-stamp tags (e.g. TSSE for ID3,
/// WM/EncodingSettings for ASF). The assertion checks that every key
/// present before save is still present after — extra keys are allowed.
use audex::{File, StreamInfo};
use std::path::{Path, PathBuf};

fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Create a temp directory and copy the source file into it, preserving the
/// original filename and extension. Returns (temp dir handle, path to copy).
/// The temp dir is deleted when the handle is dropped.
fn temp_copy_with_extension(source: &Path) -> (tempfile::TempDir, PathBuf) {
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let dest = tmp_dir
        .path()
        .join(source.file_name().expect("source has filename"));
    std::fs::copy(source, &dest).expect("copy to temp dir");
    (tmp_dir, dest)
}

/// Load from source path, save_to_path into a temp copy, reload and compare.
fn path_to_path_roundtrip(filename: &str) {
    let source = data_path(filename);
    if !source.exists() {
        return;
    }

    let mut original = File::load(&source).expect("load from source path");

    let original_rate = original.info().sample_rate();
    let original_channels = original.info().channels();
    let original_bps = original.info().bits_per_sample();
    let mut original_keys: Vec<String> = original.keys();
    original_keys.sort();

    // Temp copy preserves extension for correct format detection on reload
    let (_tmp_dir, dest) = temp_copy_with_extension(&source);

    original
        .save_to_path(&dest)
        .expect("save_to_path into temp copy");

    let reloaded = File::load(&dest).expect("reload from temp path");

    assert_eq!(
        original_rate,
        reloaded.info().sample_rate(),
        "{}: sample_rate changed after path-to-path save",
        filename,
    );
    assert_eq!(
        original_channels,
        reloaded.info().channels(),
        "{}: channels changed after path-to-path save",
        filename,
    );
    assert_eq!(
        original_bps,
        reloaded.info().bits_per_sample(),
        "{}: bits_per_sample changed after path-to-path save",
        filename,
    );

    // Every tag key from the original must still be present after save.
    // Extra keys (e.g. encoder stamps) are acceptable.
    let reloaded_keys: Vec<String> = reloaded.keys();
    for key in &original_keys {
        assert!(
            reloaded_keys.contains(key),
            "{}: original tag key '{}' missing after path-to-path save",
            filename,
            key,
        );
    }
}

// ---------------------------------------------------------------------------
// ID3v2-based formats
// ---------------------------------------------------------------------------

#[test]
fn path_to_path_mp3() {
    path_to_path_roundtrip("silence-44-s.mp3");
}

#[test]
fn path_to_path_aiff() {
    path_to_path_roundtrip("with-id3.aif");
}

#[test]
fn path_to_path_wave() {
    path_to_path_roundtrip("silence-2s-PCM-44100-16-ID3v23.wav");
}

#[test]
fn path_to_path_dsf() {
    path_to_path_roundtrip("with-id3.dsf");
}

#[test]
fn path_to_path_dsdiff() {
    path_to_path_roundtrip("5644800-2ch-s01-silence.dff");
}

// ---------------------------------------------------------------------------
// Vorbis Comment-based formats
// ---------------------------------------------------------------------------

#[test]
fn path_to_path_flac() {
    path_to_path_roundtrip("silence-44-s.flac");
}

#[test]
fn path_to_path_ogg_vorbis() {
    path_to_path_roundtrip("multipagecomment.ogg");
}

#[test]
fn path_to_path_ogg_opus() {
    path_to_path_roundtrip("example.opus");
}

#[test]
fn path_to_path_ogg_speex() {
    path_to_path_roundtrip("empty.spx");
}

#[test]
fn path_to_path_ogg_flac() {
    path_to_path_roundtrip("empty.oggflac");
}

#[test]
fn path_to_path_ogg_theora() {
    path_to_path_roundtrip("sample.oggtheora");
}

// ---------------------------------------------------------------------------
// MP4 atoms
// ---------------------------------------------------------------------------

#[test]
fn path_to_path_m4a() {
    path_to_path_roundtrip("has-tags.m4a");
}

// ---------------------------------------------------------------------------
// ASF / WMA
// ---------------------------------------------------------------------------

#[test]
fn path_to_path_asf() {
    path_to_path_roundtrip("silence-1.wma");
}

// ---------------------------------------------------------------------------
// APEv2-based formats
// ---------------------------------------------------------------------------

#[test]
fn path_to_path_monkeysaudio() {
    path_to_path_roundtrip("mac-399.ape");
}

#[test]
fn path_to_path_musepack() {
    path_to_path_roundtrip("click.mpc");
}

#[test]
fn path_to_path_wavpack() {
    path_to_path_roundtrip("silence-44-s.wv");
}

#[test]
fn path_to_path_optimfrog() {
    path_to_path_roundtrip("silence-2s-44100-16.ofr");
}

#[test]
fn path_to_path_tak() {
    path_to_path_roundtrip("has-tags.tak");
}

#[test]
fn path_to_path_trueaudio() {
    path_to_path_roundtrip("empty.tta");
}
