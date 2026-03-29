//! Tests for Vorbis comment functionality.

use audex::{AudexError, Tags, VERSION_STRING};
use std::io::Cursor;

// For testing internal vorbis module - access from the crate directly
use audex::limits::ParseLimits;
use audex::vorbis::{ErrorMode, VComment, VCommentDict, VorbisError, is_valid_key};

// Helper function to create test VComment data
fn create_valid_vorbis_data() -> Vec<u8> {
    let mut data = Vec::new();

    // Vendor string
    let vendor = b"reference libVorbis I 20050304";
    data.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    data.extend_from_slice(vendor);

    // Comment count
    data.extend_from_slice(&2u32.to_le_bytes());

    // Comment 1: TITLE=Test
    let comment1 = b"TITLE=Test";
    data.extend_from_slice(&(comment1.len() as u32).to_le_bytes());
    data.extend_from_slice(comment1);

    // Comment 2: ARTIST=Artist
    let comment2 = b"ARTIST=Artist";
    data.extend_from_slice(&(comment2.len() as u32).to_le_bytes());
    data.extend_from_slice(comment2);

    // Framing bit
    data.push(1);

    data
}

fn create_empty_valid_vorbis_data() -> Vec<u8> {
    let mut data = Vec::new();

    // Vendor string
    let vendor = b"reference libVorbis I 20050304";
    data.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    data.extend_from_slice(vendor);

    // Comment count (0)
    data.extend_from_slice(&0u32.to_le_bytes());

    // Framing bit
    data.push(1);

    data
}

fn create_invalid_vorbis_data_no_framing() -> Vec<u8> {
    let mut data = create_empty_valid_vorbis_data();
    data.pop(); // Remove framing bit
    data
}

fn create_invalid_key_vorbis_data() -> Vec<u8> {
    let mut data = Vec::new();

    // Vendor string
    let vendor = b"test";
    data.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    data.extend_from_slice(vendor);

    // Comment count
    data.extend_from_slice(&1u32.to_le_bytes());

    // Invalid comment with character above 0x7D in key
    let comment = b"TI\x7E=Test";
    data.extend_from_slice(&(comment.len() as u32).to_le_bytes());
    data.extend_from_slice(comment);

    // Framing bit
    data.push(1);

    data
}

#[cfg(test)]
mod tistag_tests {
    use super::*;

    #[test]
    fn test_empty() {
        assert!(!is_valid_key(""));
    }

    #[test]
    fn test_tilde() {
        // Character 0x7E (~) is above 0x7D and should be invalid
        assert!(!is_valid_key("title~"));
    }

    #[test]
    fn test_equals() {
        assert!(!is_valid_key("ti=tle"));
        assert!(!is_valid_key("=title"));
        assert!(!is_valid_key("title="));
    }

    #[test]
    fn test_less() {
        // Characters below 0x20 should be invalid
        assert!(!is_valid_key("title\x1f"));
        assert!(!is_valid_key("title\x00"));
        assert!(!is_valid_key("title\x10"));
        assert!(!is_valid_key("\x1ftitle"));
    }

    #[test]
    fn test_greater() {
        // Characters above 0x7D should be invalid
        assert!(!is_valid_key("title\u{7e}"));
        assert!(!is_valid_key("title\u{80}"));
        assert!(!is_valid_key("title\u{ff}"));
    }

    #[test]
    fn test_simple() {
        assert!(is_valid_key("title"));
        assert!(is_valid_key("TITLE"));
        assert!(is_valid_key("Title"));
    }

    #[test]
    fn test_space() {
        assert!(is_valid_key("ti tle"));
        assert!(is_valid_key(" title"));
        assert!(is_valid_key("title "));
    }

    #[test]
    fn test_ugly() {
        assert!(is_valid_key("!{}[]-_()*&"));
        assert!(is_valid_key("@#$%^"));
        assert!(is_valid_key("+,./"));
    }

    #[test]
    fn test_unicode() {
        // Unicode characters above ASCII range should be invalid
        assert!(!is_valid_key("titleé"));
        assert!(!is_valid_key("título"));
        assert!(!is_valid_key("タイトル"));
    }

    #[test]
    fn test_ascii_key_boundaries() {
        assert!(is_valid_key("valid_ascii"));
        assert!(!is_valid_key("invalid\u{80}"));
    }
}

#[cfg(test)]
mod tvcomment_tests {
    use super::*;

    #[test]
    fn test_invalid_init() {
        // Test loading with invalid data
        let mut comment = VComment::new();
        let invalid_data = &[0x01, 0x02, 0x03];
        let mut cursor = Cursor::new(invalid_data);

        let result = comment.load(&mut cursor, ErrorMode::Strict, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_equal() {
        let mut comment1 = VComment::new();
        let mut comment2 = VComment::new();

        comment1
            .push("TITLE".to_string(), "Test".to_string())
            .unwrap();
        comment2
            .push("TITLE".to_string(), "Test".to_string())
            .unwrap();

        // VComment doesn't implement PartialEq, so we test data equality
        assert_eq!(comment1.len(), comment2.len());
        assert_eq!(comment1.get("title"), comment2.get("title"));
    }

    #[test]
    fn test_not_header() {
        let mut comment = VComment::new();
        let invalid_data = &[0x00, 0x01, 0x02, 0x03];
        let mut cursor = Cursor::new(invalid_data);

        let result = comment.load(&mut cursor, ErrorMode::Strict, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_unset_framing_bit() {
        let mut comment = VComment::new();
        let data = create_invalid_vorbis_data_no_framing();
        let mut cursor = Cursor::new(&data);

        let result = comment.load(&mut cursor, ErrorMode::Strict, true);
        assert!(result.is_err());

        // Test that it contains UnsetFrameError
        let error = result.unwrap_err();
        match &error {
            AudexError::FormatError(_) => {
                // Extract the actual error for testing
                if let AudexError::FormatError(e) = error {
                    if let Ok(vorbis_error) = e.downcast::<VorbisError>() {
                        assert_eq!(*vorbis_error, VorbisError::UnsetFrameError);
                    } else {
                        panic!("Expected VorbisError::UnsetFrameError, got different FormatError");
                    }
                }
            }
            _ => panic!(
                "Expected FormatError with VorbisError::UnsetFrameError, got: {:?}",
                error
            ),
        }
    }

    #[test]
    fn test_empty_valid() {
        let mut comment = VComment::new();
        let data = create_empty_valid_vorbis_data();
        let mut cursor = Cursor::new(&data);

        let result = comment.load(&mut cursor, ErrorMode::Strict, true);
        assert!(result.is_ok());
        assert_eq!(comment.len(), 0);
        assert_eq!(comment.vendor, "reference libVorbis I 20050304");
    }

    #[test]
    fn test_from_bytes_rejects_cumulative_payload_above_default_limit() {
        // Build a Vorbis comment block whose cumulative payload exceeds
        // the default ParseLimits::default().max_tag_size (8 MB).
        let default_limit = ParseLimits::default().max_tag_size as usize;
        let oversized_value_len = default_limit + 1;

        let mut data = Vec::new();
        // Vendor string (empty)
        data.extend_from_slice(&0u32.to_le_bytes());
        // One comment
        data.extend_from_slice(&1u32.to_le_bytes());
        // Comment: "X=<oversized_value>"
        let comment_len = 2 + oversized_value_len; // "X=" + value
        data.extend_from_slice(&(comment_len as u32).to_le_bytes());
        data.extend_from_slice(b"X=");
        data.extend(std::iter::repeat_n(b'A', oversized_value_len));
        // Framing bit
        data.push(1);

        let err = VComment::from_bytes(&data).expect_err("expected parse limit failure");
        assert!(
            err.to_string().contains("Vorbis comment data"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_from_bytes_accepts_payload_within_default_limit() {
        // Small valid data is well within the 8 MB default limit.
        let data = create_valid_vorbis_data();
        let comment = VComment::from_bytes(&data).expect("payload within limit should parse");
        assert_eq!(comment.get("title"), Some(&["Test".to_string()][..]));
    }

    #[test]
    fn test_validate() {
        let mut comment = VComment::new();
        comment
            .push("TITLE".to_string(), "Valid Title".to_string())
            .unwrap();
        comment
            .push("ARTIST".to_string(), "Valid Artist".to_string())
            .unwrap();

        assert!(comment.validate().is_ok());
    }

    #[test]
    fn test_validate_broken_key() {
        let mut comment = VComment::new();
        // Manually add invalid key to bypass normal validation
        comment
            .data
            .push(("invalid=key".to_string(), "value".to_string()));

        let result = comment.validate();
        assert!(result.is_err());
        match result.unwrap_err() {
            AudexError::FormatError(e) => {
                let vorbis_error = e.downcast::<VorbisError>().unwrap();
                matches!(*vorbis_error, VorbisError::InvalidKey(_));
            }
            _ => panic!("Expected FormatError with InvalidKey"),
        }
    }

    #[test]
    fn test_validate_broken_value() {
        // String values are always valid UTF-8, so just verify validation passes
        let mut comment = VComment::new();
        comment
            .push("TITLE".to_string(), "Valid Value".to_string())
            .unwrap();
        assert!(comment.validate().is_ok());
    }

    #[test]
    fn test_vendor_default() {
        let comment = VComment::new();
        assert_eq!(comment.vendor, format!("Audex {}", VERSION_STRING));
    }

    #[test]
    fn test_vendor_set() {
        let custom_vendor = "Custom Encoder 2.0".to_string();
        let comment = VComment::with_vendor(custom_vendor.clone());
        assert_eq!(comment.vendor, custom_vendor);
    }

    #[test]
    fn test_vendor_invalid() {
        // In strict mode, invalid UTF-8 in vendor should fail
        let mut data = Vec::new();

        // Invalid UTF-8 vendor string
        let invalid_vendor = &[0xff, 0xfe, 0xfd];
        data.extend_from_slice(&(invalid_vendor.len() as u32).to_le_bytes());
        data.extend_from_slice(invalid_vendor);

        // Comment count (0)
        data.extend_from_slice(&0u32.to_le_bytes());

        // Framing bit
        data.push(1);

        let mut comment = VComment::new();
        let mut cursor = Cursor::new(&data);

        let result = comment.load(&mut cursor, ErrorMode::Strict, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_format_strict() {
        let data = create_invalid_key_vorbis_data();
        let mut comment = VComment::new();
        let mut cursor = Cursor::new(&data);

        let result = comment.load(&mut cursor, ErrorMode::Strict, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_format_replace() {
        let data = create_invalid_key_vorbis_data();
        let mut comment = VComment::new();
        let mut cursor = Cursor::new(&data);

        let result = comment.load(&mut cursor, ErrorMode::Replace, true);
        assert!(result.is_ok());
        assert_eq!(comment.len(), 1);
        assert_eq!(comment.data[0].0, "TI?");
        assert_eq!(comment.data[0].1, "Test");
    }

    #[test]
    fn test_invalid_format_ignore() {
        let data = create_invalid_key_vorbis_data();
        let mut comment = VComment::new();
        let mut cursor = Cursor::new(&data);

        let result = comment.load(&mut cursor, ErrorMode::Ignore, true);
        assert!(result.is_ok());
        // Invalid keys should be skipped
        assert_eq!(comment.len(), 0);
    }

    #[test]
    fn test_standard_key_value_type() {
        // Test proper handling of string keys and values
        let mut comment = VComment::new();

        comment
            .push("TITLE".to_string(), "String Value".to_string())
            .unwrap();
        assert_eq!(comment.len(), 1);

        let title_values = comment.get("title").unwrap();
        assert_eq!(title_values[0], "String Value");
    }

    #[test]
    fn test_invalid_tag_strict() {
        let mut comment = VComment::new();
        // Invalid keys must produce an error
        let result = comment.push("invalid=key".to_string(), "value".to_string());
        assert!(result.is_err());
        assert_eq!(comment.len(), 0);
    }

    #[test]
    fn test_invalid_tag_replace() {
        let mut comment = VComment::new();
        let result = comment.push("invalid=key".to_string(), "value".to_string());
        assert!(result.is_err());
        assert_eq!(comment.len(), 0);
    }

    #[test]
    fn test_invalid_tag_ignore() {
        let mut comment = VComment::new();
        let result = comment.push("invalid=key".to_string(), "value".to_string());
        assert!(result.is_err());
        assert_eq!(comment.len(), 0);
    }

    #[test]
    fn test_roundtrip() {
        let mut original = VComment::new();
        original
            .push("TITLE".to_string(), "Test Song".to_string())
            .unwrap();
        original
            .push("ARTIST".to_string(), "Test Artist".to_string())
            .unwrap();
        original
            .push("ALBUM".to_string(), "Test Album".to_string())
            .unwrap();

        // Serialize
        let bytes = original.to_bytes().unwrap();

        // Deserialize
        let loaded = VComment::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.len(), original.len());
        assert_eq!(loaded.vendor, original.vendor);
        assert_eq!(loaded.get("title"), original.get("title"));
        assert_eq!(loaded.get("artist"), original.get("artist"));
        assert_eq!(loaded.get("album"), original.get("album"));
    }
}

#[cfg(feature = "async")]
mod tvcomment_async_limit_tests {
    use super::*;
    use std::io::Write;

    /// Verify that the async load path rejects Vorbis comment data whose
    /// cumulative size exceeds the default ParseLimits tag ceiling (8 MB).
    #[tokio::test]
    async fn test_load_async_rejects_cumulative_payload_above_default_limit() {
        let default_limit = ParseLimits::default().max_tag_size as usize;
        let oversized_value_len = default_limit + 1;

        // Build a single-comment Vorbis block that exceeds the limit.
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // vendor (empty)
        data.extend_from_slice(&1u32.to_le_bytes()); // comment count
        let comment_len = 2 + oversized_value_len;
        data.extend_from_slice(&(comment_len as u32).to_le_bytes());
        data.extend_from_slice(b"X=");
        data.extend(std::iter::repeat_n(b'A', oversized_value_len));
        data.push(1); // framing bit

        let mut temp = tempfile::NamedTempFile::new().expect("create temp file");
        temp.write_all(&data).expect("write temp data");
        temp.flush().expect("flush temp data");

        let mut reader = tokio::fs::File::open(temp.path())
            .await
            .expect("open temp file");
        let mut comment = VComment::new();
        let err = comment
            .load_async(&mut reader, ErrorMode::Strict, true)
            .await
            .expect_err("expected parse limit failure");

        assert!(
            err.to_string().contains("Vorbis comment data"),
            "unexpected error: {err}"
        );
    }
}

#[cfg(test)]
mod tvcommentdict_tests {
    use super::*;

    #[test]
    fn test_correct_len() {
        let mut dict = VCommentDict::new();

        // Add multiple values for same key
        dict.set("ARTIST", vec!["Artist1".to_string(), "Artist2".to_string()]);
        dict.set("TITLE", vec!["Title1".to_string()]);

        // Length should count total values, not unique keys
        // In our implementation, len() returns the total count of key-value pairs
        assert_eq!(dict.inner().len(), 3);
    }

    #[test]
    fn test_keys() {
        let mut dict = VCommentDict::new();
        dict.set_single("TITLE", "Song".to_string());
        dict.set_single("ARTIST", "Artist".to_string());

        let keys = dict.keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"title".to_string()));
        assert!(keys.contains(&"artist".to_string()));
    }

    #[test]
    fn test_values() {
        let mut dict = VCommentDict::new();
        dict.set("ARTIST", vec!["Artist1".to_string(), "Artist2".to_string()]);

        let values = dict.get_values("artist");
        assert_eq!(values.len(), 2);
        assert!(values.contains(&"Artist1".to_string()));
        assert!(values.contains(&"Artist2".to_string()));
    }

    #[test]
    fn test_items() {
        let mut dict = VCommentDict::new();
        dict.set_single("TITLE", "Song".to_string());
        dict.set_single("ARTIST", "Artist".to_string());

        // Test iteration over items (inner VComment data)
        let items: Vec<_> = dict.inner().iter().collect();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_equal() {
        let mut dict1 = VCommentDict::new();
        let mut dict2 = VCommentDict::new();

        dict1.set_single("TITLE", "Song".to_string());
        dict2.set_single("TITLE", "Song".to_string());

        // Test equality through data comparison
        assert_eq!(dict1.inner().len(), dict2.inner().len());
        assert_eq!(dict1.get_values("title"), dict2.get_values("title"));
    }

    #[test]
    fn test_get() {
        let mut dict = VCommentDict::new();
        dict.set_single("TITLE", "Song Title".to_string());

        assert_eq!(dict.get_first("TITLE"), Some("Song Title".to_string()));
        assert_eq!(dict.get_first("title"), Some("Song Title".to_string()));
        assert_eq!(dict.get_first("TiTlE"), Some("Song Title".to_string()));
        assert_eq!(dict.get_first("nonexistent"), None);
    }

    #[test]
    fn test_set() {
        let mut dict = VCommentDict::new();

        dict.set_single("TITLE", "Original Title".to_string());
        assert_eq!(dict.get_first("title"), Some("Original Title".to_string()));

        dict.set_single("title", "New Title".to_string());
        assert_eq!(dict.get_first("title"), Some("New Title".to_string()));
        assert_eq!(dict.inner().len(), 1); // Should replace, not add
    }

    #[test]
    fn test_slice() {
        let mut dict = VCommentDict::new();
        dict.set_single("TITLE", "Song".to_string());
        dict.set_single("ARTIST", "Artist".to_string());

        // Test slice-like access through inner VComment
        let inner = dict.inner();
        assert_eq!(inner.len(), 2);

        // Access by index
        let first_item = &inner[0];
        let second_item = &inner[1];

        // Keys preserve original case (case-insensitive lookups use lowercase internally)
        assert!(first_item.0 == "TITLE" || first_item.0 == "ARTIST");
        assert!(second_item.0 == "TITLE" || second_item.0 == "ARTIST");
    }

    #[test]
    fn test_iter() {
        let mut dict = VCommentDict::new();
        dict.set_single("TITLE", "Song".to_string());
        dict.set_single("ARTIST", "Artist".to_string());

        let items: Vec<_> = dict.inner().iter().collect();
        assert_eq!(items.len(), 2);

        for (key, _value) in items {
            assert!(key == "TITLE" || key == "ARTIST");
        }
    }

    #[test]
    fn test_del() {
        let mut dict = VCommentDict::new();
        dict.set_single("TITLE", "Song".to_string());
        dict.set_single("ARTIST", "Artist".to_string());

        assert_eq!(dict.inner().len(), 2);

        dict.remove_key("TITLE");
        assert_eq!(dict.inner().len(), 1);
        assert_eq!(dict.get_first("title"), None);
        assert_eq!(dict.get_first("artist"), Some("Artist".to_string()));
    }

    #[test]
    fn test_contains() {
        let mut dict = VCommentDict::new();
        dict.set_single("TITLE", "Song".to_string());

        assert!(dict.contains_key("TITLE"));
        assert!(dict.contains_key("title"));
        assert!(dict.contains_key("TiTlE"));
        assert!(!dict.contains_key("ARTIST"));
    }

    #[test]
    fn test_case_contains() {
        let mut dict = VCommentDict::new();
        dict.set_single("Title", "Song".to_string());

        assert!(dict.contains_key("TITLE"));
        assert!(dict.contains_key("title"));
        assert!(dict.contains_key("Title"));
    }

    #[test]
    fn test_case_get() {
        let mut dict = VCommentDict::new();
        dict.set_single("Title", "Song".to_string());

        assert_eq!(dict.get_first("TITLE"), Some("Song".to_string()));
        assert_eq!(dict.get_first("title"), Some("Song".to_string()));
        assert_eq!(dict.get_first("Title"), Some("Song".to_string()));
    }

    #[test]
    fn test_case_set() {
        let mut dict = VCommentDict::new();

        dict.set_single("TITLE", "Title1".to_string());
        dict.set_single("title", "Title2".to_string());
        dict.set_single("Title", "Title3".to_string());

        // All should be the same key, so only one entry
        assert_eq!(dict.inner().len(), 1);
        assert_eq!(dict.get_first("title"), Some("Title3".to_string()));
    }

    #[test]
    fn test_case_del() {
        let mut dict = VCommentDict::new();
        dict.set_single("Title", "Song".to_string());

        dict.remove_key("TITLE");
        assert!(!dict.contains_key("title"));
        assert_eq!(dict.inner().len(), 0);
    }

    #[test]
    fn test_get_failure() {
        let dict = VCommentDict::new();
        assert_eq!(dict.get_first("nonexistent"), None);
        assert!(dict.get_values("nonexistent").is_empty());
    }

    #[test]
    fn test_del_failure() {
        let mut dict = VCommentDict::new();
        // Removing nonexistent key should not panic
        dict.remove_key("nonexistent");
        assert_eq!(dict.inner().len(), 0);
    }

    #[test]
    fn test_roundtrip() {
        let mut original = VCommentDict::new();
        original.set_single("TITLE", "Test Song".to_string());
        original.set("ARTIST", vec!["Artist1".to_string(), "Artist2".to_string()]);

        // Serialize
        let bytes = original.to_bytes().unwrap();

        // Deserialize
        let loaded = VCommentDict::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.inner().len(), original.inner().len());
        assert_eq!(loaded.get_first("title"), original.get_first("title"));
        assert_eq!(loaded.get_values("artist"), original.get_values("artist"));
    }

    #[test]
    fn test_case_items_426() {
        // Test case sensitivity edge case from format test
        let mut dict = VCommentDict::new();
        dict.set_single("Foo", "1".to_string());
        dict.set_single("foo", "2".to_string());
        dict.set_single("FOO", "3".to_string());

        // All should be normalized to the same key
        assert_eq!(dict.inner().len(), 1);
        assert_eq!(dict.get_first("foo"), Some("3".to_string()));

        let items: Vec<_> = dict.inner().iter().collect();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, "FOO"); // Preserves case of the last set call
    }

    #[test]
    fn test_empty() {
        let dict = VCommentDict::new();
        assert_eq!(dict.inner().len(), 0);
        assert!(dict.keys().is_empty());
        assert!(dict.get_values("anything").is_empty());
        assert_eq!(dict.get_first("anything"), None);
    }

    #[test]
    fn test_as_dict() {
        let mut dict = VCommentDict::new();
        dict.set_single("TITLE", "Song".to_string());
        dict.set("ARTIST", vec!["Artist1".to_string(), "Artist2".to_string()]);

        let hashmap = dict.as_dict();
        assert_eq!(hashmap.len(), 2);

        assert_eq!(hashmap.get("title").unwrap().len(), 1);
        assert_eq!(hashmap.get("title").unwrap()[0], "Song");

        assert_eq!(hashmap.get("artist").unwrap().len(), 2);
        assert!(
            hashmap
                .get("artist")
                .unwrap()
                .contains(&"Artist1".to_string())
        );
        assert!(
            hashmap
                .get("artist")
                .unwrap()
                .contains(&"Artist2".to_string())
        );
    }

    #[test]
    fn test_bad_key() {
        let mut dict = VCommentDict::new();

        // Try to set invalid key - should be ignored
        dict.set_single("invalid=key", "value".to_string());
        assert_eq!(dict.inner().len(), 0);

        // Valid key should work
        dict.set_single("valid_key", "value".to_string());
        assert_eq!(dict.inner().len(), 1);
    }

    #[test]
    fn test_duplicate_keys() {
        let mut dict = VCommentDict::new();

        // Add multiple values for the same key
        dict.set(
            "ARTIST",
            vec![
                "Artist1".to_string(),
                "Artist2".to_string(),
                "Artist3".to_string(),
            ],
        );

        assert_eq!(dict.get_values("artist").len(), 3);
        assert_eq!(dict.inner().len(), 3); // Should have 3 entries

        // All entries preserve the original case from set()
        for (key, _) in dict.inner().iter() {
            assert_eq!(key, "ARTIST");
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_load_real_vorbis_data() {
        let data = create_valid_vorbis_data();

        let mut comment = VComment::new();
        let mut cursor = Cursor::new(&data);
        let result = comment.load(&mut cursor, ErrorMode::Strict, true);

        assert!(result.is_ok());
        assert_eq!(comment.len(), 2);
        assert_eq!(comment.vendor, "reference libVorbis I 20050304");
        assert_eq!(comment.get("title").unwrap()[0], "Test");
        assert_eq!(comment.get("artist").unwrap()[0], "Artist");
    }

    #[test]
    fn test_write_read_consistency() {
        let mut original = VComment::new();
        original.vendor = "Test Encoder 1.0".to_string();
        original
            .push("TITLE".to_string(), "Test Song".to_string())
            .unwrap();
        original
            .push("ARTIST".to_string(), "Test Artist".to_string())
            .unwrap();
        original
            .push("ALBUM".to_string(), "Test Album".to_string())
            .unwrap();
        original
            .push("ARTIST".to_string(), "Second Artist".to_string())
            .unwrap(); // Multiple values

        // Write to bytes
        let bytes = original.to_bytes().unwrap();

        // Read back
        let loaded = VComment::from_bytes(&bytes).unwrap();

        // Verify all data matches
        assert_eq!(loaded.vendor, original.vendor);
        assert_eq!(loaded.len(), original.len());

        // Check each key
        for key in ["title", "artist", "album"] {
            assert_eq!(loaded.get(key), original.get(key));
        }

        // Specifically check multiple artist values
        let artist_values = loaded.get("artist").unwrap();
        assert_eq!(artist_values.len(), 2);
        assert!(artist_values.contains(&"Test Artist".to_string()));
        assert!(artist_values.contains(&"Second Artist".to_string()));
    }

    #[test]
    fn test_vcomment_dict_interoperability() {
        let mut comment = VComment::new();
        comment
            .push("TITLE".to_string(), "Song".to_string())
            .unwrap();
        comment
            .push("ARTIST".to_string(), "Artist".to_string())
            .unwrap();

        // Convert through bytes to VCommentDict
        let bytes = comment.to_bytes().unwrap();
        let dict = VCommentDict::from_bytes(&bytes).unwrap();

        assert_eq!(dict.get_first("title"), Some("Song".to_string()));
        assert_eq!(dict.get_first("artist"), Some("Artist".to_string()));
        assert_eq!(dict.inner().len(), 2);
    }

    #[test]
    fn test_error_mode_handling() {
        // Create data with invalid UTF-8
        let mut data = Vec::new();

        // Vendor string with invalid UTF-8
        let vendor = &[0x74, 0x65, 0x73, 0x74, 0xFF]; // "test" + invalid byte
        data.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        data.extend_from_slice(vendor);

        // Comment count
        data.extend_from_slice(&1u32.to_le_bytes());

        // Comment with invalid UTF-8
        let comment = &[0x54, 0x49, 0x54, 0x4C, 0x45, 0x3D, 0xFF]; // "TITLE=" + invalid byte
        data.extend_from_slice(&(comment.len() as u32).to_le_bytes());
        data.extend_from_slice(comment);

        // Framing bit
        data.push(1);

        // Test strict mode
        let result_strict = VComment::from_bytes_with_options(&data, ErrorMode::Strict, true);
        assert!(result_strict.is_err());

        // Test replace mode
        let result_replace = VComment::from_bytes_with_options(&data, ErrorMode::Replace, true);
        assert!(result_replace.is_ok());

        // Test ignore mode
        let result_ignore = VComment::from_bytes_with_options(&data, ErrorMode::Ignore, true);
        assert!(result_ignore.is_ok());
    }

    #[test]
    fn test_large_comment_data() {
        let mut comment = VComment::new();

        // Add a large number of comments
        for i in 0..1000 {
            comment
                .push(format!("TAG{:04}", i), format!("Value {}", i))
                .unwrap();
        }

        assert_eq!(comment.len(), 1000);

        // Test serialization/deserialization
        let bytes = comment.to_bytes().unwrap();
        let loaded = VComment::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.len(), 1000);

        // Verify some values
        assert_eq!(loaded.get("tag0000").unwrap()[0], "Value 0");
        assert_eq!(loaded.get("tag0999").unwrap()[0], "Value 999");
    }

    #[test]
    fn test_empty_values() {
        let mut dict = VCommentDict::new();
        dict.set_single("TITLE", "".to_string());
        dict.set_single("ARTIST", "Real Artist".to_string());

        assert_eq!(dict.get_first("title"), Some("".to_string()));
        assert_eq!(dict.get_first("artist"), Some("Real Artist".to_string()));

        // Empty values should serialize/deserialize correctly
        let bytes = dict.to_bytes().unwrap();
        let loaded = VCommentDict::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.get_first("title"), Some("".to_string()));
        assert_eq!(loaded.get_first("artist"), Some("Real Artist".to_string()));
    }

    #[test]
    fn test_special_characters() {
        let mut comment = VComment::new();

        // Test values with special characters (but valid UTF-8)
        comment
            .push(
                "TITLE".to_string(),
                "Song with \"quotes\" and 'apostrophes'".to_string(),
            )
            .unwrap();
        comment
            .push("COMMENT".to_string(), "Multi\nline\ncomment".to_string())
            .unwrap();
        comment
            .push(
                "DESCRIPTION".to_string(),
                "Unicode: éñüñüñ 你好".to_string(),
            )
            .unwrap();

        // Test serialization/deserialization
        let bytes = comment.to_bytes().unwrap();
        let loaded = VComment::from_bytes(&bytes).unwrap();

        assert_eq!(
            loaded.get("title").unwrap()[0],
            "Song with \"quotes\" and 'apostrophes'"
        );
        assert_eq!(loaded.get("comment").unwrap()[0], "Multi\nline\ncomment");
        assert_eq!(
            loaded.get("description").unwrap()[0],
            "Unicode: éñüñüñ 你好"
        );
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use std::io::Cursor;

    fn build_vorbis_comment_block(comment_count: u32) -> Vec<u8> {
        let vendor = b"test";
        let comment = b"k=v";

        let mut data = Vec::new();
        data.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        data.extend_from_slice(vendor);
        data.extend_from_slice(&comment_count.to_le_bytes());
        for _ in 0..comment_count.min(200) {
            data.extend_from_slice(&(comment.len() as u32).to_le_bytes());
            data.extend_from_slice(comment);
        }

        data
    }

    #[test]
    fn test_rejects_comment_count_above_limit() {
        let data = build_vorbis_comment_block(200_000);
        let mut cursor = Cursor::new(data);

        use audex::vorbis::{ErrorMode, VComment};
        let mut vc = VComment::new();
        let result = vc.load(&mut cursor, ErrorMode::Strict, false);

        assert!(result.is_err(), "Should reject comment count above 100,000");
    }

    #[test]
    fn test_accepts_comment_count_within_limit() {
        let data = build_vorbis_comment_block(100);
        let mut cursor = Cursor::new(data);

        use audex::vorbis::{ErrorMode, VComment};
        let mut vc = VComment::new();
        let result = vc.load(&mut cursor, ErrorMode::Strict, false);

        assert!(result.is_ok(), "Should accept reasonable comment count");
    }
}
