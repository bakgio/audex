//! End-to-end integration tests for the diff module using real audio files.

mod common;

use audex::File;
use audex::diff::{self, DiffOptions};
use common::TestUtils;
use std::io::{Seek, SeekFrom, Write};
use tempfile::NamedTempFile;

/// Create a temporary copy of a test file, preserving the given extension
/// so that format detection works correctly.
fn temp_copy_with_ext(filename: &str) -> NamedTempFile {
    let src = TestUtils::data_path(filename);
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let suffix = format!(".{}", ext);
    let mut tmp = NamedTempFile::with_suffix(&suffix).expect("create temp file");
    let data = std::fs::read(&src).expect("read source file");
    tmp.write_all(&data).expect("write temp file");
    tmp.flush().expect("flush");
    tmp.seek(SeekFrom::Start(0)).expect("seek");
    tmp
}

// ---------------------------------------------------------------------------
// Diff two MP3 files with different tags
// ---------------------------------------------------------------------------

#[test]
fn test_diff_two_mp3_files() {
    // lame.mp3 has ID3v2 tags; no-tags.mp3 has none
    let path_a = TestUtils::data_path("lame.mp3");
    let path_b = TestUtils::data_path("no-tags.mp3");

    let file_a = File::load(&path_a).expect("load file A");
    let file_b = File::load(&path_b).expect("load file B");

    let d = diff::diff(&file_a, &file_b);

    // lame.mp3 has tags — expect left-only entries
    if !file_a.keys().is_empty() {
        assert!(!d.is_identical(), "files with different tags should differ");
        assert!(
            !d.left_only.is_empty(),
            "tagged file vs untagged should have left-only entries"
        );
    }
}

// ---------------------------------------------------------------------------
// Cross-format diff (MP3 vs FLAC) — raw keys will differ
// ---------------------------------------------------------------------------

#[test]
fn test_diff_mp3_vs_flac() {
    let mp3_path = TestUtils::data_path("lame.mp3");
    let flac_path = TestUtils::data_path("silence-44-s.flac");

    let mp3 = File::load(&mp3_path).expect("load MP3");
    let flac = File::load(&flac_path).expect("load FLAC");

    let d = diff::diff(&mp3, &flac);

    // Raw-key diff between MP3 (ID3 frame IDs) and FLAC (Vorbis Comment keys)
    // will almost certainly show differences because the key namespaces differ.
    let _summary = d.summary();
}

// ---------------------------------------------------------------------------
// Before/after edit via snapshot (using FLAC which always has a tag container)
// ---------------------------------------------------------------------------

#[test]
fn test_diff_file_before_after_edit() {
    let tmp = temp_copy_with_ext("silence-44-s.flac");

    let mut file = File::load(tmp.path()).expect("load temp FLAC");

    // Take a snapshot of the current state
    let snapshot = diff::snapshot_tags(&file);

    // Edit a tag — FLAC always has Vorbis Comments
    file.set("TITLE", vec!["Diff Test Title".to_string()])
        .expect("set title");

    // Diff against the snapshot
    let d = diff::diff_against_snapshot(&file, &snapshot);

    // TITLE should appear as changed or added (Vorbis Comments normalise to lowercase)
    let all_diff_keys = d.differing_keys();
    assert!(
        all_diff_keys.contains(&"title") || all_diff_keys.contains(&"TITLE"),
        "title should appear in differing keys after edit, got: {:?}",
        all_diff_keys,
    );
}

// ---------------------------------------------------------------------------
// Stream info comparison
// ---------------------------------------------------------------------------

#[test]
fn test_diff_with_stream_info() {
    let mp3_path = TestUtils::data_path("silence-44-s.mp3");
    let flac_path = TestUtils::data_path("silence-44-s.flac");

    let mp3 = File::load(&mp3_path).expect("load MP3");
    let flac = File::load(&flac_path).expect("load FLAC");

    let opts = DiffOptions {
        compare_stream_info: true,
        ..Default::default()
    };
    let d = diff::diff_with_options(&mp3, &flac, &opts);

    // Stream info diff should be populated
    assert!(
        d.stream_info_diff.is_some(),
        "stream_info_diff should be present when compare_stream_info is enabled"
    );

    // MP3 (lossy) and FLAC (lossless) will differ in bitrate and/or bits_per_sample
    let si = d.stream_info_diff.as_ref().unwrap();
    let has_some_diff = si.bitrate.is_some()
        || si.bits_per_sample.is_some()
        || si.sample_rate.is_some()
        || si.channels.is_some()
        || si.length.is_some();
    assert!(has_some_diff, "MP3 vs FLAC should have stream info diffs");
}

// ---------------------------------------------------------------------------
// Identical copy
// ---------------------------------------------------------------------------

#[test]
fn test_diff_real_file_identical_copy() {
    let path = TestUtils::data_path("silence-44-s.flac");
    let file_a = File::load(&path).expect("load A");
    let file_b = File::load(&path).expect("load B");

    let d = diff::diff(&file_a, &file_b);
    assert!(
        d.is_identical(),
        "same file loaded twice should be identical"
    );
}

// ---------------------------------------------------------------------------
// Cleared file
// ---------------------------------------------------------------------------

#[test]
fn test_diff_cleared_file() {
    let tmp = temp_copy_with_ext("silence-44-s.flac");

    let file_before = File::load(tmp.path()).expect("load before clear");
    let snapshot = diff::snapshot_tags(&file_before);

    // Skip if the file has no tags to begin with
    if snapshot.is_empty() {
        return;
    }

    let mut file_after = File::load(tmp.path()).expect("reload for clear");
    file_after.clear().expect("clear tags");

    let d = diff::diff_items(&snapshot, &file_after.items());

    // All original tags should now be in left_only
    assert!(
        !d.left_only.is_empty(),
        "cleared file should have left-only entries"
    );
    assert!(
        d.right_only.is_empty(),
        "cleared file should have no right-only entries"
    );
}

// ---------------------------------------------------------------------------
// Convenience methods on DynamicFileType
// ---------------------------------------------------------------------------

#[test]
fn test_diff_convenience_methods() {
    let tmp = temp_copy_with_ext("silence-44-s.flac");

    let mut file = File::load(tmp.path()).expect("load");
    let snapshot = file.tag_snapshot_items();

    // Use the convenience diff_tags method
    let self_diff = file.diff_tags(&file);
    assert!(
        self_diff.is_identical(),
        "file compared to itself is identical"
    );

    // Edit and diff against snapshot via convenience method
    file.set("ARTIST", vec!["Convenience Test".to_string()])
        .expect("set");
    let d = file.diff_against(&snapshot);
    assert!(!d.is_identical(), "should detect the edit");
}

// ---------------------------------------------------------------------------
// Cross-format normalized diffs
// ---------------------------------------------------------------------------

#[test]
fn test_diff_mp4_vs_flac_normalized() {
    let mp4_path = TestUtils::data_path("has-tags.m4a");
    let flac_path = TestUtils::data_path("silence-44-s.flac");

    let mp4 = File::load(&mp4_path).expect("load MP4");
    let flac = File::load(&flac_path).expect("load FLAC");

    // Normalized diff uses StandardField names instead of raw format keys
    let d = diff::diff_normalized(&mp4, &flac);
    let _summary = d.summary();

    // At minimum the diff should run without panicking.
    // If MP4 has tags, there should be some left-only entries.
    if !mp4.keys().is_empty() {
        assert!(
            !d.is_identical(),
            "MP4 with tags vs bare FLAC should not be identical"
        );
    }
}

#[test]
fn test_diff_ogg_vs_mp3_normalized() {
    let ogg_path = TestUtils::data_path("multipagecomment.ogg");
    let mp3_path = TestUtils::data_path("lame.mp3");

    let ogg = File::load(&ogg_path).expect("load OGG");
    let mp3 = File::load(&mp3_path).expect("load MP3");

    let d = diff::diff_normalized(&ogg, &mp3);
    let _summary = d.summary();
}

#[test]
fn test_diff_wav_vs_aiff() {
    let wav_path = TestUtils::data_path("silence-2s-PCM-44100-16-ID3v23.wav");
    let aiff_path = TestUtils::data_path("with-id3.aif");

    let wav = File::load(&wav_path).expect("load WAV");
    let aiff = File::load(&aiff_path).expect("load AIFF");

    // Both use ID3v2 -- raw diff should use comparable keys
    let d = diff::diff(&wav, &aiff);
    let _summary = d.summary();
}

#[test]
fn test_diff_with_include_keys() {
    let tmp_a = temp_copy_with_ext("silence-44-s.flac");
    let tmp_b = temp_copy_with_ext("silence-44-s.flac");

    let mut a = File::load(tmp_a.path()).expect("load A");
    let mut b = File::load(tmp_b.path()).expect("load B");

    a.set("TITLE", vec!["A Title".to_string()]).unwrap();
    a.set("ARTIST", vec!["A Artist".to_string()]).unwrap();
    b.set("TITLE", vec!["B Title".to_string()]).unwrap();
    b.set("ARTIST", vec!["B Artist".to_string()]).unwrap();

    // Vorbis Comment keys are stored lowercase in the diff
    let mut include = std::collections::HashSet::new();
    include.insert("title".to_string());

    let opts = DiffOptions {
        include_keys: Some(include),
        ..Default::default()
    };
    let d = diff::diff_with_options(&a, &b, &opts);

    let keys = d.differing_keys();
    assert!(
        keys.iter().any(|k| k.eq_ignore_ascii_case("title")),
        "title should be in diff, got: {:?}",
        keys
    );
    assert!(
        !keys.iter().any(|k| k.eq_ignore_ascii_case("artist")),
        "artist should be excluded by include_keys filter"
    );
}

#[test]
fn test_diff_with_exclude_keys() {
    let tmp_a = temp_copy_with_ext("silence-44-s.flac");
    let tmp_b = temp_copy_with_ext("silence-44-s.flac");

    let mut a = File::load(tmp_a.path()).expect("load A");
    let mut b = File::load(tmp_b.path()).expect("load B");

    a.set("TITLE", vec!["A Title".to_string()]).unwrap();
    a.set("ARTIST", vec!["A Artist".to_string()]).unwrap();
    b.set("TITLE", vec!["B Title".to_string()]).unwrap();
    b.set("ARTIST", vec!["A Artist".to_string()]).unwrap();

    let mut exclude = std::collections::HashSet::new();
    exclude.insert("title".to_string());

    let opts = DiffOptions {
        exclude_keys: exclude,
        ..Default::default()
    };
    let d = diff::diff_with_options(&a, &b, &opts);

    let keys = d.differing_keys();
    assert!(
        !keys.iter().any(|k| k.eq_ignore_ascii_case("title")),
        "title should be excluded from diff, got: {:?}",
        keys
    );
}

#[test]
fn test_diff_snapshot_detect_added_removed_changed() {
    let tmp = temp_copy_with_ext("silence-44-s.flac");
    let mut file = File::load(tmp.path()).expect("load");

    // Set initial state
    file.set("TITLE", vec!["Original".to_string()]).unwrap();
    file.set("GENRE", vec!["Rock".to_string()]).unwrap();

    let snapshot = diff::snapshot_tags(&file);

    // Modify: change title, remove genre, add artist
    file.set("TITLE", vec!["Changed".to_string()]).unwrap();
    file.remove("GENRE").unwrap();
    file.set("ARTIST", vec!["New Artist".to_string()]).unwrap();

    let d = diff::diff_against_snapshot(&file, &snapshot);

    assert!(!d.is_identical());
    assert!(
        d.diff_count() >= 2,
        "at least title change and genre removal"
    );
}
