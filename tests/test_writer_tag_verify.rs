/// Verifies that save_to_writer round-trips preserve tag values, not just
/// stream info. For each format: load from bytes, save to an in-memory
/// buffer, reload from that buffer, and assert that every original tag
/// key/value pair survived. Original test data is never modified.
use audex::{File, StreamInfo};
use std::io::Cursor;
use std::path::PathBuf;

fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Load from bytes, save_to_writer, reload, compare tag values and stream info.
fn writer_tag_roundtrip(filename: &str) {
    let path = data_path(filename);
    if !path.exists() {
        return;
    }

    let data = std::fs::read(&path).unwrap();

    let original = File::load_from_reader(Cursor::new(data.clone()), Some(path.clone()))
        .expect("load original");

    let original_rate = original.info().sample_rate();
    let original_channels = original.info().channels();
    let original_bps = original.info().bits_per_sample();

    // Capture all original tag key-value pairs
    let original_keys = original.keys();
    let original_tags: Vec<(String, Vec<String>)> = original_keys
        .iter()
        .filter_map(|k| original.get(k).map(|v| (k.clone(), v)))
        .collect();

    let mut to_save = File::load_from_reader(Cursor::new(data.clone()), Some(path.clone()))
        .expect("load copy for saving");

    // save_to_writer into cursor seeded with original bytes
    let mut out = Cursor::new(data);
    to_save.save_to_writer(&mut out).expect("save_to_writer");

    let reloaded = File::load_from_reader(Cursor::new(out.into_inner()), Some(path))
        .expect("reload from writer output");

    // Stream info must survive
    assert_eq!(
        original_rate,
        reloaded.info().sample_rate(),
        "{}: sample_rate mismatch after writer round-trip",
        filename,
    );
    assert_eq!(
        original_channels,
        reloaded.info().channels(),
        "{}: channels mismatch after writer round-trip",
        filename,
    );
    assert_eq!(
        original_bps,
        reloaded.info().bits_per_sample(),
        "{}: bits_per_sample mismatch after writer round-trip",
        filename,
    );

    // Every original tag value must survive the round-trip
    for (key, original_values) in &original_tags {
        let reloaded_values = reloaded.get(key);
        assert!(
            reloaded_values.is_some(),
            "{}: tag '{}' missing after writer round-trip",
            filename,
            key,
        );
        assert_eq!(
            original_values,
            &reloaded_values.unwrap(),
            "{}: tag '{}' value changed after writer round-trip",
            filename,
            key,
        );
    }
}

// ---------------------------------------------------------------------------
// ID3v2-based formats
// ---------------------------------------------------------------------------

#[test]
fn writer_tags_mp3() {
    writer_tag_roundtrip("silence-44-s.mp3");
}

#[test]
fn writer_tags_aiff() {
    writer_tag_roundtrip("with-id3.aif");
}

#[test]
fn writer_tags_wave() {
    writer_tag_roundtrip("silence-2s-PCM-44100-16-ID3v23.wav");
}

#[test]
fn writer_tags_dsf() {
    writer_tag_roundtrip("with-id3.dsf");
}

#[test]
fn writer_tags_dsdiff() {
    writer_tag_roundtrip("5644800-2ch-s01-silence.dff");
}

// ---------------------------------------------------------------------------
// Vorbis Comment-based formats
// ---------------------------------------------------------------------------

#[test]
fn writer_tags_flac() {
    writer_tag_roundtrip("silence-44-s.flac");
}

#[test]
fn writer_tags_ogg_vorbis() {
    writer_tag_roundtrip("multipagecomment.ogg");
}

#[test]
fn writer_tags_ogg_opus() {
    writer_tag_roundtrip("example.opus");
}

#[test]
fn writer_tags_ogg_speex() {
    writer_tag_roundtrip("empty.spx");
}

#[test]
fn writer_tags_ogg_flac() {
    writer_tag_roundtrip("empty.oggflac");
}

#[test]
fn writer_tags_ogg_theora() {
    writer_tag_roundtrip("sample.oggtheora");
}

// ---------------------------------------------------------------------------
// MP4 atoms
// ---------------------------------------------------------------------------

#[test]
fn writer_tags_m4a() {
    writer_tag_roundtrip("has-tags.m4a");
}

// ---------------------------------------------------------------------------
// ASF / WMA
// ---------------------------------------------------------------------------

#[test]
fn writer_tags_asf() {
    writer_tag_roundtrip("silence-1.wma");
}

// ---------------------------------------------------------------------------
// APEv2-based formats
// ---------------------------------------------------------------------------

#[test]
fn writer_tags_monkeysaudio() {
    writer_tag_roundtrip("mac-399.ape");
}

#[test]
fn writer_tags_musepack() {
    writer_tag_roundtrip("click.mpc");
}

#[test]
fn writer_tags_wavpack() {
    writer_tag_roundtrip("silence-44-s.wv");
}

#[test]
fn writer_tags_optimfrog() {
    writer_tag_roundtrip("silence-2s-44100-16.ofr");
}

#[test]
fn writer_tags_tak() {
    writer_tag_roundtrip("has-tags.tak");
}

#[test]
fn writer_tags_trueaudio() {
    writer_tag_roundtrip("empty.tta");
}
