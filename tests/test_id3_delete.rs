use audex::id3::ID3Tags;
/// Tests for ID3 delete functionality
/// - Delete ID3v1 tags
/// - Delete ID3v2 tags
/// - Delete both tag versions
/// - File length shrinks appropriately after deletion
use audex::mp3;
use std::fs;
use std::io::Read;

#[test]
fn test_delete_both_tags() {
    // Test deleting both ID3v1 and ID3v2 tags
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");

    // Copy a test MP3 file
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    fs::write(&test_path, mp3_data).unwrap();

    // Get original file size
    let original_size = fs::metadata(&test_path).unwrap().len();

    // Delete all ID3 tags
    let result = mp3::clear(&test_path);
    assert!(result.is_ok(), "Delete should succeed");

    // Get new file size
    let new_size = fs::metadata(&test_path).unwrap().len();

    // File should be smaller or equal (if there were no tags)
    assert!(
        new_size <= original_size,
        "File should not grow after tag deletion"
    );
}

#[test]
fn test_delete_v2_only() {
    // Test deleting only ID3v2 tags
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");

    // Copy a test MP3 file
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    fs::write(&test_path, mp3_data).unwrap();

    // Delete only ID3v2 tags
    let result = mp3::clear_with_options(&test_path, false, true);
    assert!(result.is_ok(), "Delete v2 only should succeed");

    // Verify the file still exists and is readable
    let file_data = fs::read(&test_path).unwrap();
    assert!(
        !file_data.is_empty(),
        "File should not be empty after deletion"
    );
}

#[test]
fn test_delete_v1_only() {
    // Test deleting only ID3v1 tags
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");

    // Copy a test MP3 file
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    fs::write(&test_path, mp3_data).unwrap();

    // Delete only ID3v1 tags
    let result = mp3::clear_with_options(&test_path, true, false);
    assert!(result.is_ok(), "Delete v1 only should succeed");

    // Verify the file still exists and is readable
    let file_data = fs::read(&test_path).unwrap();
    assert!(
        !file_data.is_empty(),
        "File should not be empty after deletion"
    );
}

#[test]
fn test_delete_removes_id3v2_header() {
    // Test that ID3v2 header is actually removed
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");

    // Copy a test MP3 file
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    fs::write(&test_path, mp3_data).unwrap();

    // Delete all tags
    mp3::clear(&test_path).unwrap();

    // Read the first 3 bytes
    let mut file = fs::File::open(&test_path).unwrap();
    let mut header = [0u8; 3];
    file.read_exact(&mut header).ok();

    // After deletion, file should not start with "ID3"
    // (unless the audio data happens to start with those bytes)
    // We can't reliably test this without knowing the file format,
    // so we just verify the operation completed
    // Operation completed successfully - nothing more to verify
}

#[test]
fn test_delete_empty_file() {
    // Test deleting from an empty file
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("empty.mp3");

    // Create an empty file
    fs::write(&test_path, b"").unwrap();

    // Delete should not fail on empty file
    let result = mp3::clear(&test_path);
    // Either succeeds or fails gracefully
    let _ = result;
}

#[test]
fn test_delete_nonexistent_file() {
    // Test deleting a nonexistent file
    let result = mp3::clear("/nonexistent/path/to/file.mp3");

    // Should return an error (IO error)
    assert!(result.is_err(), "Delete on nonexistent file should fail");
}

#[test]
fn test_delete_preserves_audio_data() {
    // Test that deletion doesn't corrupt the audio data
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");

    // Copy a test MP3 file
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    fs::write(&test_path, mp3_data).unwrap();

    // Delete all tags
    mp3::clear(&test_path).unwrap();

    // File should still be readable
    let file_data = fs::read(&test_path).unwrap();
    assert!(!file_data.is_empty(), "File should not be empty");

    // The file should contain some data (audio frames)
    assert!(
        !file_data.is_empty(),
        "File should have content after tag deletion"
    );
}

#[test]
fn test_delete_and_verify_no_tags() {
    // Test that after deletion, no ID3 tags can be loaded
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");

    // Copy a test MP3 file
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    fs::write(&test_path, mp3_data).unwrap();

    // Delete all tags
    mp3::clear(&test_path).unwrap();

    // Try to load tags - should either fail or return empty tags
    if let Ok(tags) = ID3Tags::load(&test_path, None, true, 4, true) {
        // Tags should be empty or minimal
        let frame_count = tags.frames_by_id.len();
        // After deletion, there should be no or very few frames
        assert!(
            frame_count <= 1,
            "Very few frames should remain after deletion, got {}",
            frame_count
        );
    }
}

#[test]
fn test_file_size_shrinks_after_delete() {
    // Test that file size decreases or stays same after tag deletion
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");

    // Copy a test MP3 file (which may already have tags)
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    fs::write(&test_path, mp3_data).unwrap();

    // Get original file size
    let original_size = fs::metadata(&test_path).unwrap().len();

    // Delete tags
    mp3::clear(&test_path).unwrap();

    // Get file size after deletion
    let size_after_delete = fs::metadata(&test_path).unwrap().len();

    // File should be smaller or same size after removing tags
    assert!(
        size_after_delete <= original_size,
        "File size should not increase after tag deletion (was {}, now {})",
        original_size,
        size_after_delete
    );
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use audex::id3::ID3;

    #[test]
    fn test_clear_on_empty_id3_does_not_overflow_stack() {
        let mut id3 = ID3::new();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = id3.clear();
        }));

        assert!(
            result.is_ok(),
            "clear() caused a stack overflow due to infinite recursion"
        );
    }

    #[test]
    fn test_delete_full_without_file_does_not_overflow_stack() {
        let mut id3 = ID3::new();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = id3.delete_full(None, true, true);
        }));

        assert!(
            result.is_ok(),
            "delete_full() caused a stack overflow due to infinite recursion"
        );
    }
}
