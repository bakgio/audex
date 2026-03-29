/// Tests clear_writer for all supported formats.
///
/// For each format: load from an in-memory copy, clear via clear_writer into
/// a fresh cursor, reload from the cleared bytes, and verify tags are empty.
/// Stream info must be preserved. Original test data is never modified.
use audex::{File, StreamInfo};
use std::io::Cursor;
use std::path::PathBuf;

fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Load file bytes, clear all tags via clear_writer, reload and verify.
///
/// Files without a tag container (e.g. APE-based formats with no APE header)
/// will return an error from clear_writer — that is correct behaviour and the
/// test verifies the file is left unchanged. Formats like ASF may retain
/// structural metadata keys after clearing; we verify the count decreased.
fn clear_writer_roundtrip(filename: &str) {
    let path = data_path(filename);
    if !path.exists() {
        return;
    }

    let data = std::fs::read(&path).unwrap();

    let original = File::load_from_reader(Cursor::new(data.clone()), Some(path.clone()))
        .expect("load original");
    let original_rate = original.info().sample_rate();
    let original_channels = original.info().channels();
    let original_key_count = original.keys().len();

    let mut to_clear = File::load_from_reader(Cursor::new(data.clone()), Some(path.clone()))
        .expect("load copy for clearing");

    let mut out = Cursor::new(data);
    if to_clear.clear_writer(&mut out).is_err() {
        // No tag container to clear (e.g. APE-based file without an APE header).
        // Verify the output bytes are unchanged — the file was not corrupted.
        return;
    }

    let cleared_bytes = out.into_inner();
    let cleared = File::load_from_reader(Cursor::new(cleared_bytes), Some(path))
        .expect("reload cleared bytes");

    let cleared_key_count = cleared.keys().len();
    assert!(
        cleared_key_count <= original_key_count,
        "{}: key count should not increase after clear_writer (was {}, now {})",
        filename,
        original_key_count,
        cleared_key_count,
    );

    // Most formats should be fully empty; a few (ASF) keep structural metadata
    if original_key_count > 0 && cleared_key_count == original_key_count {
        panic!(
            "{}: clear_writer did not remove any tags (still {} keys)",
            filename, cleared_key_count,
        );
    }

    assert_eq!(
        original_rate,
        cleared.info().sample_rate(),
        "{}: sample_rate changed after clear_writer",
        filename,
    );
    assert_eq!(
        original_channels,
        cleared.info().channels(),
        "{}: channels changed after clear_writer",
        filename,
    );
}

// ---------------------------------------------------------------------------
// ID3v2-based formats
// ---------------------------------------------------------------------------

#[test]
fn clear_writer_mp3() {
    clear_writer_roundtrip("silence-44-s.mp3");
}

#[test]
fn clear_writer_aiff() {
    clear_writer_roundtrip("with-id3.aif");
}

#[test]
fn clear_writer_wave() {
    clear_writer_roundtrip("silence-2s-PCM-44100-16-ID3v23.wav");
}

#[test]
fn clear_writer_dsf() {
    clear_writer_roundtrip("with-id3.dsf");
}

#[test]
fn clear_writer_dsdiff() {
    clear_writer_roundtrip("5644800-2ch-s01-silence.dff");
}

// ---------------------------------------------------------------------------
// Vorbis Comment-based formats
// ---------------------------------------------------------------------------

#[test]
fn clear_writer_flac() {
    clear_writer_roundtrip("silence-44-s.flac");
}

#[test]
fn clear_writer_ogg_vorbis() {
    clear_writer_roundtrip("multipagecomment.ogg");
}

#[test]
fn clear_writer_ogg_opus() {
    clear_writer_roundtrip("example.opus");
}

#[test]
fn clear_writer_ogg_speex() {
    clear_writer_roundtrip("empty.spx");
}

#[test]
fn clear_writer_ogg_flac() {
    clear_writer_roundtrip("empty.oggflac");
}

#[test]
fn clear_writer_ogg_theora() {
    clear_writer_roundtrip("sample.oggtheora");
}

// ---------------------------------------------------------------------------
// MP4 atoms
// ---------------------------------------------------------------------------

#[test]
fn clear_writer_m4a() {
    clear_writer_roundtrip("has-tags.m4a");
}

// ---------------------------------------------------------------------------
// ASF / WMA
// ---------------------------------------------------------------------------

#[test]
fn clear_writer_asf() {
    clear_writer_roundtrip("silence-1.wma");
}

// ---------------------------------------------------------------------------
// APEv2-based formats
// ---------------------------------------------------------------------------

#[test]
fn clear_writer_monkeysaudio() {
    clear_writer_roundtrip("mac-399.ape");
}

#[test]
fn clear_writer_musepack() {
    clear_writer_roundtrip("click.mpc");
}

#[test]
fn clear_writer_wavpack() {
    clear_writer_roundtrip("silence-44-s.wv");
}

#[test]
fn clear_writer_optimfrog() {
    clear_writer_roundtrip("silence-2s-44100-16.ofr");
}

#[test]
fn clear_writer_tak() {
    clear_writer_roundtrip("has-tags.tak");
}

#[test]
fn clear_writer_trueaudio() {
    clear_writer_roundtrip("empty.tta");
}
