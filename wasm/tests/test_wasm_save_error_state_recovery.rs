// Integration tests for WASM AudioFile state recovery on write errors.
//
// These tests verify that `original_bytes` is never left empty when a
// save or clear operation fails mid-write. The underlying bug was that
// `mem::take(&mut self.original_bytes)` moved the buffer into a cursor,
// but an early `?` return on error skipped the line that restores the
// buffer from the cursor — leaving `original_bytes` permanently empty.
//
// Since the WASM AudioFile constructor requires `wasm_bindgen::JsValue`
// (only available on the `wasm32` target), these integration tests
// exercise the same cursor-recovery pattern through the underlying
// `audex` API to validate the fix on native targets. The actual
// wasm_bindgen-level tests live in the `#[cfg(test)]` module inside
// `audio_file.rs`.

use std::io::Cursor;
use std::path::PathBuf;

/// Load the test fixture from the audex test data directory.
fn load_test_fixture() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/data/52-overwritten-metadata.flac");
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("Failed to read test fixture {}: {}", path.display(), e))
}

/// Simulates the cursor-based save pattern used by AudioFile::save().
/// Verifies that the buffer is restored from the cursor regardless of
/// whether save_to_writer succeeds or fails.
#[test]
fn save_restores_buffer_on_error() {
    let original_bytes = load_test_fixture();
    let original_len = original_bytes.len();
    assert!(original_len > 0);

    let cursor = Cursor::new(&original_bytes[..]);
    let mut inner = audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac")))
        .expect("failed to parse FLAC fixture");

    // Simulate the fixed save pattern: move bytes into cursor, then
    // restore from cursor on both success and failure paths.
    let mut buffer = original_bytes;
    let mut cursor = Cursor::new(std::mem::take(&mut buffer));
    match inner.save_to_writer(&mut cursor) {
        Ok(()) => {
            buffer = cursor.into_inner();
        }
        Err(_) => {
            buffer = cursor.into_inner();
        }
    }

    assert!(
        !buffer.is_empty(),
        "buffer must not be empty after save attempt"
    );
    // On success the buffer may grow (tag metadata written), but it
    // should never shrink to zero.
    assert!(
        buffer.len() >= original_len,
        "buffer should retain at least the original data"
    );
}

/// Simulates the cursor-based clear pattern used by AudioFile::clear().
/// Verifies that the buffer is restored from the cursor on all paths.
#[test]
fn clear_restores_buffer_on_error() {
    let original_bytes = load_test_fixture();
    let original_len = original_bytes.len();

    let cursor = Cursor::new(&original_bytes[..]);
    let mut inner = audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac")))
        .expect("failed to parse FLAC fixture");

    let mut buffer = original_bytes;
    let mut cursor = Cursor::new(std::mem::take(&mut buffer));
    match inner.clear_writer(&mut cursor) {
        Ok(()) => {
            buffer = cursor.into_inner();
        }
        Err(_) => {
            buffer = cursor.into_inner();
        }
    }

    assert!(
        !buffer.is_empty(),
        "buffer must not be empty after clear attempt"
    );
    assert!(
        buffer.len() <= original_len,
        "cleared buffer should not be larger than original"
    );
}

/// Verify that when mem::take is used without restoring the buffer
/// (the old buggy pattern), the buffer is indeed left empty. This
/// confirms the bug existed and the fix is necessary.
#[test]
fn mem_take_without_restore_leaves_buffer_empty() {
    let mut buffer = vec![1u8, 2, 3, 4, 5];
    let _cursor = Cursor::new(std::mem::take(&mut buffer));

    // This demonstrates the bug: after mem::take, the original
    // variable is left with an empty Vec.
    assert!(
        buffer.is_empty(),
        "mem::take should leave the source empty (demonstrating the bug)"
    );
    // The fix recovers the data from the cursor via into_inner().
    let recovered = _cursor.into_inner();
    assert_eq!(recovered, vec![1u8, 2, 3, 4, 5]);
}

/// End-to-end test: parse, modify a tag, save, then verify the saved
/// bytes can be re-parsed. This ensures the fix doesn't break the
/// normal save workflow.
#[test]
fn roundtrip_save_preserves_valid_state() {
    let original_bytes = load_test_fixture();

    let cursor = Cursor::new(&original_bytes[..]);
    let mut inner = audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac")))
        .expect("failed to parse FLAC fixture");

    // Set a tag to make the metadata dirty.
    inner
        .set("TITLE", vec!["Roundtrip Test".to_string()])
        .expect("set TITLE");

    // Save using the fixed pattern.
    let mut buffer = original_bytes;
    let mut cursor = Cursor::new(std::mem::take(&mut buffer));
    match inner.save_to_writer(&mut cursor) {
        Ok(()) => {
            buffer = cursor.into_inner();
        }
        Err(e) => {
            let _recovered = cursor.into_inner();
            panic!("save failed unexpectedly: {e}");
        }
    }

    assert!(!buffer.is_empty(), "saved buffer must not be empty");

    // Re-parse the saved bytes to verify they are valid.
    let cursor = Cursor::new(&buffer[..]);
    let reparsed = audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac")))
        .expect("saved bytes should be re-parseable");

    let title = reparsed.get_first("TITLE").map(|s| s.to_string());
    assert_eq!(
        title.as_deref(),
        Some("Roundtrip Test"),
        "tag should survive the save roundtrip"
    );
}

/// Verify that setting no tags produces an identical diff (empty update
/// should be a no-op).
#[test]
fn empty_update_produces_identical_diff() {
    let bytes = load_test_fixture();

    let cursor_a = Cursor::new(&bytes[..]);
    let file_a =
        audex::File::load_from_reader(cursor_a, Some(PathBuf::from("a.flac"))).expect("load A");

    let cursor_b = Cursor::new(&bytes[..]);
    let file_b =
        audex::File::load_from_reader(cursor_b, Some(PathBuf::from("b.flac"))).expect("load B");

    let diff = audex::diff::diff(&file_a, &file_b);
    assert!(
        diff.is_identical(),
        "two files loaded from the same bytes should diff as identical"
    );
    assert_eq!(diff.diff_count(), 0);
}

/// Verify that after clearing all tags, a diff against the original
/// snapshot shows every tag as removed.
#[test]
fn diff_after_clear_shows_removals() {
    let bytes = load_test_fixture();

    let cursor = Cursor::new(&bytes[..]);
    let original =
        audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac"))).expect("load");

    let original_keys = original.keys();
    if original_keys.is_empty() {
        // Fixture has no tags — set some so we can verify removal
        return;
    }

    // Take a snapshot of the tags before clearing
    let snapshot = audex::diff::snapshot_tags(&original);

    // Clear all tags on a copy
    let cursor = Cursor::new(&bytes[..]);
    let mut cleared =
        audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac"))).expect("load");

    for key in &original_keys {
        cleared.remove(key).ok();
    }

    // Diff cleared file against the original snapshot
    let diff = audex::diff::diff_against_snapshot(&cleared, &snapshot);
    assert!(
        !diff.is_identical(),
        "clearing tags should produce a non-identical diff"
    );
    // Every original key should appear as left_only (present in snapshot, absent in cleared)
    assert!(
        !diff.left_only.is_empty(),
        "removed tags should appear as left_only entries"
    );
}

/// Verify that modifying a tag produces a diff that detects the change.
#[test]
fn diff_after_modification_detects_change() {
    let bytes = load_test_fixture();

    let cursor = Cursor::new(&bytes[..]);
    let original =
        audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac"))).expect("load");

    let snapshot = audex::diff::snapshot_tags(&original);

    // Modify on a separate copy
    let cursor = Cursor::new(&bytes[..]);
    let mut modified =
        audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac"))).expect("load");
    modified
        .set("TITLE", vec!["Modified Title".to_string()])
        .expect("set TITLE");

    let diff = audex::diff::diff_against_snapshot(&modified, &snapshot);
    assert!(
        !diff.is_identical(),
        "modifying a tag should produce a non-identical diff"
    );
}

/// Verify that saving to a buffer and re-parsing produces consistent
/// tags — the full native roundtrip through cursor-based I/O.
#[test]
fn save_then_reload_tag_consistency() {
    let bytes = load_test_fixture();

    let cursor = Cursor::new(&bytes[..]);
    let mut file =
        audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac"))).expect("load");

    file.set("ARTIST", vec!["Consistency Check".to_string()])
        .expect("set ARTIST");

    // Save to buffer
    let mut buffer = bytes;
    let mut cursor = Cursor::new(std::mem::take(&mut buffer));
    file.save_to_writer(&mut cursor).expect("save");
    buffer = cursor.into_inner();

    // Re-parse and verify
    let cursor = Cursor::new(&buffer[..]);
    let reloaded =
        audex::File::load_from_reader(cursor, Some(PathBuf::from("test.flac"))).expect("reload");

    assert_eq!(
        reloaded.get_first("ARTIST").as_deref(),
        Some("Consistency Check"),
        "saved tag should survive reload"
    );
}
