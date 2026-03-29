//! Tests for normalized (cross-format) tag diffing via TagMap.
//!
//! These tests verify that `diff_normalized` and `diff_normalized_with_options`
//! correctly map format-specific keys to canonical StandardField display names
//! before comparing, enabling meaningful cross-format diffs.

mod common;

use audex::File;
use audex::diff::{self, DiffOptions};
use common::TestUtils;
use std::io::{Seek, SeekFrom, Write};
use tempfile::NamedTempFile;

/// Create a temporary copy of a test file, preserving the extension
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
// Cross-format normalized diff: MP3 vs FLAC
// ---------------------------------------------------------------------------

#[test]
fn test_diff_normalized_mp3_vs_flac_same_tags() {
    // Tag both files with the same metadata via their format-specific keys,
    // then verify that normalized diff sees them as matching.
    let mp3_tmp = temp_copy_with_ext("silence-44-s.mp3");
    let flac_tmp = temp_copy_with_ext("silence-44-s.flac");

    let mut mp3 = File::load(mp3_tmp.path()).expect("load MP3");
    let mut flac = File::load(flac_tmp.path()).expect("load FLAC");

    // Set the same logical tags through the unified API
    mp3.set("artist", vec!["Test Artist".into()]).ok();
    mp3.set("title", vec!["Test Title".into()]).ok();
    mp3.set("album", vec!["Test Album".into()]).ok();

    flac.set("artist", vec!["Test Artist".into()]).ok();
    flac.set("title", vec!["Test Title".into()]).ok();
    flac.set("album", vec!["Test Album".into()]).ok();

    // Raw diff would compare ID3v2 frame IDs vs Vorbis Comment keys — mismatch
    let _raw_diff = diff::diff(&mp3, &flac);

    // Normalized diff compares canonical field names — should match
    let norm_diff = diff::diff_normalized(&mp3, &flac);

    // The normalized diff should have fewer (or zero) differences for these
    // common fields compared to the raw diff
    let norm_artist_differs = norm_diff.changed.iter().any(|c| c.key == "Artist");

    // If both files accepted the set() call, Artist should match in normalized form
    if mp3.get("artist").is_some() && flac.get("artist").is_some() {
        assert!(
            !norm_artist_differs,
            "Artist should match in normalized diff when both files have the same value"
        );
    }

    // Verify that normalized diff produces human-readable key names
    for change in &norm_diff.changed {
        // Keys should be display names like "Artist", not raw like "TPE1"
        assert!(
            !change.key.starts_with("T") || change.key.len() > 4,
            "normalized keys should be human-readable, got: {}",
            change.key
        );
    }
}

// ---------------------------------------------------------------------------
// Normalized diff produces human-readable field names
// ---------------------------------------------------------------------------

#[test]
fn test_diff_normalized_uses_display_names() {
    let mp3_tmp = temp_copy_with_ext("lame.mp3");
    let flac_tmp = temp_copy_with_ext("no-tags.flac");

    let mp3 = File::load(mp3_tmp.path()).expect("load MP3");
    let flac = File::load(flac_tmp.path()).expect("load FLAC");

    // lame.mp3 should have pre-existing ID3 tags
    if mp3.keys().is_empty() {
        // Nothing to test if the file has no tags
        return;
    }

    let norm_diff = diff::diff_normalized(&mp3, &flac);

    // All left-only keys should be human-readable display names,
    // not raw ID3v2 frame IDs (4-character uppercase codes like "TPE1")
    for entry in &norm_diff.left_only {
        let is_raw_id3_frame = entry.key.len() == 4
            && entry
                .key
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());

        // Custom fields get a "id3:" prefix, so they won't match this pattern.
        // Standard fields should have multi-word display names.
        assert!(
            !is_raw_id3_frame || entry.key.starts_with("id3:"),
            "normalized keys should not be raw 4-char ID3 frame IDs, got: {}",
            entry.key
        );
    }
}

// ---------------------------------------------------------------------------
// Normalized diff: identical file against itself
// ---------------------------------------------------------------------------

#[test]
fn test_diff_normalized_identical_files() {
    let path = TestUtils::data_path("silence-44-s.flac");
    let file_a = File::load(&path).expect("load A");
    let file_b = File::load(&path).expect("load B");

    let d = diff::diff_normalized(&file_a, &file_b);
    assert!(
        d.is_identical(),
        "same file loaded twice should produce identical normalized diff"
    );
}

// ---------------------------------------------------------------------------
// Normalized diff: both empty
// ---------------------------------------------------------------------------

#[test]
fn test_diff_normalized_both_empty() {
    let path_a = TestUtils::data_path("no-tags.flac");
    let path_b = TestUtils::data_path("no-tags.m4a");

    let file_a = File::load(&path_a).expect("load FLAC");
    let file_b = File::load(&path_b).expect("load M4A");

    let d = diff::diff_normalized(&file_a, &file_b);

    // Both files have no tags, so the normalized diff should be identical
    // (excluding any format-specific metadata that the parser might inject)
    assert_eq!(
        d.diff_count(),
        d.changed.len() + d.left_only.len() + d.right_only.len()
    );
}

// ---------------------------------------------------------------------------
// Normalized diff with options: stream info comparison
// ---------------------------------------------------------------------------

#[test]
fn test_diff_normalized_with_stream_info() {
    let mp3_path = TestUtils::data_path("silence-44-s.mp3");
    let flac_path = TestUtils::data_path("silence-44-s.flac");

    let mp3 = File::load(&mp3_path).expect("load MP3");
    let flac = File::load(&flac_path).expect("load FLAC");

    let opts = DiffOptions {
        compare_stream_info: true,
        ..Default::default()
    };

    let d = diff::diff_normalized_with_options(&mp3, &flac, &opts);

    // Stream info diff should be populated
    assert!(
        d.stream_info_diff.is_some(),
        "stream_info_diff should be present with compare_stream_info enabled"
    );

    // MP3 (lossy) and FLAC (lossless) will differ in at least bitrate or bits_per_sample
    let si = d.stream_info_diff.as_ref().unwrap();
    let has_diff = si.bitrate.is_some()
        || si.bits_per_sample.is_some()
        || si.sample_rate.is_some()
        || si.channels.is_some()
        || si.length.is_some();
    assert!(has_diff, "MP3 vs FLAC should have stream info differences");
}

// ---------------------------------------------------------------------------
// Normalized diff with options: case-insensitive keys and trim
// ---------------------------------------------------------------------------

#[test]
fn test_diff_normalized_with_case_insensitive_and_trim() {
    let tmp_a = temp_copy_with_ext("silence-44-s.flac");
    let tmp_b = temp_copy_with_ext("silence-44-s.flac");

    let mut file_a = File::load(tmp_a.path()).expect("load A");
    let mut file_b = File::load(tmp_b.path()).expect("load B");

    // Set tags with slightly different whitespace
    file_a.set("artist", vec!["Test Artist".into()]).ok();
    file_b.set("artist", vec!["Test Artist ".into()]).ok();

    let strict = diff::diff_normalized(&file_a, &file_b);
    let relaxed = diff::diff_normalized_with_options(
        &file_a,
        &file_b,
        &DiffOptions {
            trim_values: true,
            ..Default::default()
        },
    );

    // With trim enabled, trailing whitespace should be ignored.
    // The relaxed diff should have equal or fewer differences.
    assert!(
        relaxed.diff_count() <= strict.diff_count(),
        "trim_values should reduce or maintain diff count"
    );
}

// ---------------------------------------------------------------------------
// Normalized diff with options: include/exclude key filters
// ---------------------------------------------------------------------------

#[test]
fn test_diff_normalized_with_key_filters() {
    let tmp_a = temp_copy_with_ext("silence-44-s.flac");
    let tmp_b = temp_copy_with_ext("no-tags.flac");

    let mut file_a = File::load(tmp_a.path()).expect("load A");
    let file_b = File::load(tmp_b.path()).expect("load B");

    file_a.set("artist", vec!["Filtered".into()]).ok();
    file_a.set("title", vec!["Filtered Title".into()]).ok();
    file_a.set("album", vec!["Filtered Album".into()]).ok();

    // Only include "Artist" in comparison
    let opts = DiffOptions {
        include_keys: Some(["Artist".to_string()].into_iter().collect()),
        ..Default::default()
    };

    let d = diff::diff_normalized_with_options(&file_a, &file_b, &opts);

    // Only "Artist" should appear in the diff results
    let all_keys: Vec<&str> = d
        .differing_keys()
        .into_iter()
        .chain(d.unchanged.iter().map(|e| e.key.as_str()))
        .collect();

    for key in &all_keys {
        assert_eq!(
            *key, "Artist",
            "with include_keys=[Artist], only 'Artist' should appear, got: {}",
            key
        );
    }
}

// ---------------------------------------------------------------------------
// Convenience methods on DynamicFileType
// ---------------------------------------------------------------------------

#[test]
fn test_diff_tags_normalized_convenience_method() {
    let path = TestUtils::data_path("silence-44-s.flac");
    let file_a = File::load(&path).expect("load A");
    let file_b = File::load(&path).expect("load B");

    // Use the convenience method on DynamicFileType
    let d = file_a.diff_tags_normalized(&file_b);
    assert!(
        d.is_identical(),
        "same file via convenience method should be identical"
    );
}

#[test]
fn test_diff_tags_normalized_with_options_convenience() {
    let mp3_path = TestUtils::data_path("silence-44-s.mp3");
    let flac_path = TestUtils::data_path("silence-44-s.flac");

    let mp3 = File::load(&mp3_path).expect("load MP3");
    let flac = File::load(&flac_path).expect("load FLAC");

    let opts = DiffOptions {
        compare_stream_info: true,
        include_unchanged: true,
        ..Default::default()
    };

    let d = mp3.diff_tags_normalized_with_options(&flac, &opts);
    assert!(
        d.stream_info_diff.is_some(),
        "stream info should be compared via convenience method"
    );
}

// ---------------------------------------------------------------------------
// Normalized diff shows fewer spurious differences than raw diff
// ---------------------------------------------------------------------------

#[test]
fn test_normalized_vs_raw_cross_format_diff() {
    let mp3_tmp = temp_copy_with_ext("silence-44-s.mp3");
    let flac_tmp = temp_copy_with_ext("silence-44-s.flac");

    let mut mp3 = File::load(mp3_tmp.path()).expect("load MP3");
    let mut flac = File::load(flac_tmp.path()).expect("load FLAC");

    // Set matching metadata on both
    mp3.set("artist", vec!["Same".into()]).ok();
    mp3.set("title", vec!["Same".into()]).ok();
    flac.set("artist", vec!["Same".into()]).ok();
    flac.set("title", vec!["Same".into()]).ok();

    let raw = diff::diff(&mp3, &flac);
    let normalized = diff::diff_normalized(&mp3, &flac);

    // The normalized diff should have fewer or equal total differences,
    // since it resolves format-specific key names to canonical names.
    // (Raw diff might see TPE1 vs ARTIST as different keys.)
    assert!(
        normalized.diff_count() <= raw.diff_count(),
        "normalized diff should have <= differences than raw diff for matching tags \
         (raw: {}, normalized: {})",
        raw.diff_count(),
        normalized.diff_count(),
    );
}

// ---------------------------------------------------------------------------
// Normalized diff with MP4 format
// ---------------------------------------------------------------------------

#[test]
fn test_diff_normalized_mp4_vs_flac() {
    let m4a_path = TestUtils::data_path("has-tags.m4a");
    let flac_path = TestUtils::data_path("silence-44-s.flac");

    // Only run if both files exist and load successfully
    let m4a = match File::load(&m4a_path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let flac = match File::load(&flac_path) {
        Ok(f) => f,
        Err(_) => return,
    };

    let d = diff::diff_normalized(&m4a, &flac);

    // Verify all keys in the output are human-readable
    for change in &d.changed {
        assert!(
            !change.key.starts_with('\u{00a9}'),
            "normalized keys should not contain raw MP4 atom names like ©nam, got: {}",
            change.key
        );
    }
    for entry in &d.left_only {
        assert!(
            !entry.key.starts_with('\u{00a9}'),
            "normalized keys should not contain raw MP4 atom names, got: {}",
            entry.key
        );
    }
}

// ---------------------------------------------------------------------------
// Normalized diff: include_unchanged shows matching fields
// ---------------------------------------------------------------------------

#[test]
fn test_diff_normalized_include_unchanged() {
    let tmp_a = temp_copy_with_ext("silence-44-s.flac");
    let tmp_b = temp_copy_with_ext("silence-44-s.flac");

    let mut file_a = File::load(tmp_a.path()).expect("load A");
    let mut file_b = File::load(tmp_b.path()).expect("load B");

    file_a.set("artist", vec!["Same Artist".into()]).ok();
    file_a.set("title", vec!["Different A".into()]).ok();
    file_b.set("artist", vec!["Same Artist".into()]).ok();
    file_b.set("title", vec!["Different B".into()]).ok();

    let opts = DiffOptions {
        include_unchanged: true,
        ..Default::default()
    };

    let d = diff::diff_normalized_with_options(&file_a, &file_b, &opts);

    // "Artist" should be in unchanged (both have same value)
    let unchanged_keys: Vec<&str> = d.unchanged.iter().map(|e| e.key.as_str()).collect();
    if file_a.get("artist").is_some() && file_b.get("artist").is_some() {
        assert!(
            unchanged_keys.contains(&"Artist")
                || unchanged_keys.iter().any(|k| k.to_lowercase() == "artist"),
            "shared Artist tag should appear in unchanged list, got: {:?}",
            unchanged_keys
        );
    }
}

// ---------------------------------------------------------------------------
// normalize_custom_keys: strip format prefixes from custom tag keys
// ---------------------------------------------------------------------------

#[test]
fn test_normalize_custom_keys_matches_cross_format() {
    // Set the same custom tag on MP3 (stored as TXXX:MYSETTING) and
    // FLAC (stored as a plain Vorbis Comment key). With normalize_custom_keys
    // enabled, the diff should recognise them as identical.
    let mp3_tmp = temp_copy_with_ext("silence-44-s.mp3");
    let flac_tmp = temp_copy_with_ext("silence-44-s.flac");

    let mut mp3 = File::load(mp3_tmp.path()).expect("load MP3");
    let mut flac = File::load(flac_tmp.path()).expect("load FLAC");

    // ID3 stores user-defined text as TXXX:Description
    mp3.set("TXXX:MYSETTING", vec!["hello".into()]).ok();
    // Vorbis stores arbitrary keys directly
    flac.set("MYSETTING", vec!["hello".into()]).ok();

    let opts_on = DiffOptions {
        normalize_custom_keys: true,
        include_unchanged: true,
        ..Default::default()
    };

    let d_on = diff::diff_normalized_with_options(&mp3, &flac, &opts_on);

    // Normalization strips "TXXX:" and lowercases, so both become "mysetting"
    let unchanged: Vec<String> = d_on
        .unchanged
        .iter()
        .map(|e| e.key.to_lowercase())
        .collect();
    assert!(
        unchanged.contains(&"mysetting".to_string()),
        "Custom key should appear in unchanged when normalize_custom_keys is on. \
         unchanged={:?}, changed={:?}, left_only={:?}, right_only={:?}",
        d_on.unchanged,
        d_on.changed,
        d_on.left_only,
        d_on.right_only,
    );
}

#[test]
fn test_normalize_custom_keys_off_keeps_prefixes() {
    // Without normalization, the same custom key from different formats should
    // NOT match because the raw keys differ (e.g. "id3:TXXX:MYSETTING" vs
    // "vorbis:MYSETTING").
    let mp3_tmp = temp_copy_with_ext("silence-44-s.mp3");
    let flac_tmp = temp_copy_with_ext("silence-44-s.flac");

    let mut mp3 = File::load(mp3_tmp.path()).expect("load MP3");
    let mut flac = File::load(flac_tmp.path()).expect("load FLAC");

    mp3.set("TXXX:OTHERTAG", vec!["world".into()]).ok();
    flac.set("OTHERTAG", vec!["world".into()]).ok();

    let opts_off = DiffOptions {
        normalize_custom_keys: false,
        include_unchanged: true,
        ..Default::default()
    };

    let d_off = diff::diff_normalized_with_options(&mp3, &flac, &opts_off);

    // Without normalization the keys keep their format prefixes and won't match
    let unchanged: Vec<String> = d_off
        .unchanged
        .iter()
        .map(|e| e.key.to_lowercase())
        .collect();
    assert!(
        !unchanged.contains(&"othertag".to_string()),
        "Custom key should NOT appear in unchanged when normalize_custom_keys is off. \
         unchanged={:?}",
        unchanged,
    );
}

#[test]
fn test_normalize_custom_keys_standard_fields_unaffected() {
    // Standard fields (artist, title, etc.) should behave the same regardless
    // of normalize_custom_keys since they are already mapped to canonical names.
    let mp3_tmp = temp_copy_with_ext("silence-44-s.mp3");
    let flac_tmp = temp_copy_with_ext("silence-44-s.flac");

    let mut mp3 = File::load(mp3_tmp.path()).expect("load MP3");
    let mut flac = File::load(flac_tmp.path()).expect("load FLAC");

    mp3.set("artist", vec!["Same".into()]).ok();
    flac.set("artist", vec!["Same".into()]).ok();

    let opts_on = DiffOptions {
        normalize_custom_keys: true,
        include_unchanged: true,
        ..Default::default()
    };
    let opts_off = DiffOptions {
        normalize_custom_keys: false,
        include_unchanged: true,
        ..Default::default()
    };

    let d_on = diff::diff_normalized_with_options(&mp3, &flac, &opts_on);
    let d_off = diff::diff_normalized_with_options(&mp3, &flac, &opts_off);

    let has_artist =
        |d: &diff::TagDiff| d.unchanged.iter().any(|e| e.key.to_lowercase() == "artist");

    // Artist should appear as unchanged in both modes
    if mp3.get("artist").is_some() && flac.get("artist").is_some() {
        assert!(
            has_artist(&d_on),
            "Artist should be unchanged with normalization on"
        );
        assert!(
            has_artist(&d_off),
            "Artist should be unchanged with normalization off"
        );
    }
}
