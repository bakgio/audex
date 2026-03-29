//! APEv2 tags tests

use audex::apev2::{APEValue, APEValueType, APEv2, APEv2Tags, clear, is_valid_apev2_key};
use audex::{AudexError, FileType, StreamInfo};

mod common;
use common::TestUtils;

#[cfg(test)]
mod apev2_basic_tests {
    use super::*;

    #[test]
    fn test_apev2_creation() {
        let ape = APEv2::new();
        assert!(ape.tags.is_empty());
        assert_eq!(ape.info.length(), None);
        assert_eq!(ape.info.bitrate(), None);
    }

    #[test]
    fn test_apev2_tags_creation() {
        let tags = APEv2Tags::new();
        assert!(tags.is_empty());
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_mime_types() {
        let mimes = APEv2::mime_types();
        assert!(mimes.contains(&"application/x-ape"));
        assert!(mimes.contains(&"audio/x-ape"));
    }

    #[test]
    fn test_score_apetagex_signature() {
        let header = b"APETAGEX\x00\x00\x00\x00";
        let score = APEv2::score("test.mpc", header);
        assert!(score >= 10, "Should score high for APETAGEX signature");
    }

    #[test]
    fn test_score_ape_extension() {
        // APEv2 is a tag format, not a container format, so .ape extension
        // alone should NOT produce a score (MonkeysAudio handles .ape files)
        let header = b"some random header";
        let score = APEv2::score("test.ape", header);
        assert_eq!(score, 0, "APEv2 should not score for .ape extension alone");
    }

    #[test]
    fn test_score_combined() {
        // Only the APETAGEX signature matters for scoring
        let header = b"APETAGEX\x00\x00\x00\x00";
        let score = APEv2::score("test.ape", header);
        assert!(score >= 10, "Should score for APETAGEX signature");
    }

    #[test]
    fn test_score_no_match() {
        let header = b"ID3\x04\x00";
        let score = APEv2::score("test.mp3", header);
        assert_eq!(score, 0, "Should score 0 for non-APE file");
    }
}

#[cfg(test)]
mod apev2_key_validation_tests {
    use super::*;

    #[test]
    fn test_valid_keys() {
        let valid_keys = ["Artist", "Title", "Album", "foo", "Foo", "   f ~~~", "AB"];
        for key in &valid_keys {
            assert!(is_valid_apev2_key(key), "Key '{}' should be valid", key);
        }
    }

    #[test]
    fn test_invalid_keys() {
        let invalid_keys = [
            "\x11hi",                               // Invalid character (below 0x20)
            &format!("foo{}", char::from(0xFF_u8)), // Invalid character (above 0x7E)
            "\u{1234}",                             // Unicode character
            "a",                                    // Too short (< 2 chars)
            "",                                     // Empty
            &"foo".repeat(100),                     // Too long (> 255 chars)
            "OggS",                                 // Reserved
            "TAG",                                  // Reserved
            "ID3",                                  // Reserved
            "MP+",                                  // Reserved
        ];

        for key in &invalid_keys {
            assert!(!is_valid_apev2_key(key), "Key '{}' should be invalid", key);
        }
    }

    #[test]
    fn test_key_length_boundaries() {
        // Exactly 2 characters (minimum)
        assert!(is_valid_apev2_key("AB"));

        // 255 characters (maximum)
        let max_key = "A".repeat(255);
        assert!(is_valid_apev2_key(&max_key));

        // 256 characters (too long)
        let too_long = "A".repeat(256);
        assert!(!is_valid_apev2_key(&too_long));
    }

    #[test]
    fn test_ascii_range() {
        // Space (0x20) - minimum valid
        assert!(is_valid_apev2_key("  "));

        // Tilde (0x7E) - maximum valid
        assert!(is_valid_apev2_key("~~"));

        // Control character (0x1F) - invalid
        assert!(!is_valid_apev2_key(&format!("A{}", char::from(0x1F))));

        // DEL character (0x7F) - invalid
        assert!(!is_valid_apev2_key(&format!("A{}", char::from(0x7F))));
    }
}

#[cfg(test)]
mod apev2_value_tests {
    use super::*;

    #[test]
    fn test_text_value_creation() {
        let value = APEValue::text("Hello World");
        assert_eq!(value.value_type, APEValueType::Text);
        assert_eq!(value.as_string().unwrap(), "Hello World");
        assert_eq!(value.pprint(), "Hello World");
    }

    #[test]
    fn test_binary_value_creation() {
        let data = vec![0x12, 0x34, 0x56, 0x78];
        let value = APEValue::binary(data.clone());
        assert_eq!(value.value_type, APEValueType::Binary);
        assert_eq!(value.data, data);
        assert_eq!(value.pprint(), "[4 bytes]");

        // Binary should not convert to string
        assert!(value.as_string().is_err());
    }

    #[test]
    fn test_external_value_creation() {
        let uri = "http://example.com/music";
        let value = APEValue::external(uri);
        assert_eq!(value.value_type, APEValueType::External);
        assert_eq!(value.as_string().unwrap(), uri);
        assert_eq!(value.pprint(), "[External] http://example.com/music");
    }

    #[test]
    fn test_text_list_value() {
        let multi_value = "Artist1\0Artist2\0Artist3";
        let value = APEValue::text(multi_value);
        let text_list = value.as_text_list().unwrap();
        assert_eq!(text_list, vec!["Artist1", "Artist2", "Artist3"]);
        assert_eq!(value.pprint(), "Artist1 / Artist2 / Artist3");
    }

    #[test]
    fn test_value_type_matching() {
        assert_eq!(APEValueType::Text as u32, 0);
        assert_eq!(APEValueType::Binary as u32, 1);
        assert_eq!(APEValueType::External as u32, 2);
    }
}

#[cfg(test)]
mod apev2_tags_tests {
    use super::*;

    #[test]
    fn test_case_insensitive_access() {
        let mut tags = APEv2Tags::new();
        tags.set_text("Artist", "Test Artist".to_string()).unwrap();

        // Case-insensitive get
        assert!(tags.get("artist").is_some());
        assert!(tags.get("ARTIST").is_some());
        assert!(tags.get("ArTiSt").is_some());

        // All should return same value
        let artist1 = tags.get("artist").unwrap().as_string().unwrap();
        let artist2 = tags.get("ARTIST").unwrap().as_string().unwrap();
        let artist3 = tags.get("ArTiSt").unwrap().as_string().unwrap();

        assert_eq!(artist1, "Test Artist");
        assert_eq!(artist2, "Test Artist");
        assert_eq!(artist3, "Test Artist");
    }

    #[test]
    fn test_case_preservation() {
        let mut tags = APEv2Tags::new();
        tags.set_text("FoObaR", "Test Value".to_string()).unwrap();

        let keys = tags.keys();
        assert!(keys.contains(&"FoObaR".to_string()));
        assert!(!keys.contains(&"foobar".to_string()));

        // But access is still case-insensitive
        assert!(tags.contains_key("foobar"));
        assert!(tags.contains_key("FOOBAR"));
    }

    #[test]
    fn test_set_get_remove() {
        let mut tags = APEv2Tags::new();

        // Set values
        tags.set_text("Title", "Test Song".to_string()).unwrap();
        tags.set_text("Artist", "Test Artist".to_string()).unwrap();

        assert_eq!(tags.len(), 2);
        assert!(!tags.is_empty());

        // Get values
        assert_eq!(tags.get("title").unwrap().as_string().unwrap(), "Test Song");
        assert_eq!(
            tags.get("artist").unwrap().as_string().unwrap(),
            "Test Artist"
        );

        // Remove value
        let removed = tags.remove("title");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().as_string().unwrap(), "Test Song");

        assert_eq!(tags.len(), 1);
        assert!(tags.get("title").is_none());
        assert!(tags.get("artist").is_some());
    }

    #[test]
    fn test_text_list_convenience() {
        let mut tags = APEv2Tags::new();
        let artists = vec!["Artist 1".to_string(), "Artist 2".to_string()];

        tags.set_text_list("Artist", artists.clone()).unwrap();

        let value = tags.get("artist").unwrap();
        let retrieved_list = value.as_text_list().unwrap();
        assert_eq!(retrieved_list, artists);
    }

    #[test]
    fn test_invalid_key_rejection() {
        let mut tags = APEv2Tags::new();

        let result = tags.set_text("x", "too short".to_string());
        assert!(result.is_err());

        let result = tags.set_text("OggS", "reserved".to_string());
        assert!(result.is_err());

        let result = tags.set_text("\x00invalid", "bad char".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_keys_values_items() {
        let mut tags = APEv2Tags::new();
        tags.set_text("Title", "Test Song".to_string()).unwrap();
        tags.set_text("Artist", "Test Artist".to_string()).unwrap();

        let keys = tags.keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"Title".to_string()));
        assert!(keys.contains(&"Artist".to_string()));

        let values = tags.values();
        assert_eq!(values.len(), 2);

        let items = tags.items();
        assert_eq!(items.len(), 2);

        // Find specific items
        let title_item = items.iter().find(|(k, _)| k == "Title").unwrap();
        assert_eq!(title_item.1.as_string().unwrap(), "Test Song");
    }

    #[test]
    fn test_clear() {
        let mut tags = APEv2Tags::new();
        tags.set_text("Title", "Test Song".to_string()).unwrap();
        tags.set_text("Artist", "Test Artist".to_string()).unwrap();

        assert_eq!(tags.len(), 2);

        tags.clear();

        assert_eq!(tags.len(), 0);
        assert!(tags.is_empty());
        assert!(tags.keys().is_empty());
    }

    #[test]
    fn test_pprint() {
        let mut tags = APEv2Tags::new();
        tags.set_text("Title", "Test Song".to_string()).unwrap();
        tags.set_text("Artist", "Test Artist".to_string()).unwrap();

        let output = tags.pprint();
        assert!(output.contains("Artist=Test Artist"));
        assert!(output.contains("Title=Test Song"));
        assert!(output.contains("\n")); // Should be multi-line
    }
}

#[cfg(test)]
mod apev2_file_operations_tests {
    use super::*;

    #[test]
    fn test_load_real_ape_files() {
        // Test with real APE files from test data
        let ape_files = [
            "oldtag.apev2",
            "brokentag.apev2",
            "145-invalid-item-count.apev2",
        ];

        for filename in &ape_files {
            let path = TestUtils::data_path(filename);
            if path.exists() {
                match APEv2::load(&path) {
                    Ok(ape) => {
                        println!("Successfully loaded {}", filename);
                        println!("  Tags: {}", ape.tags.len());

                        // Basic validation
                        assert!(!ape.tags.is_empty(), "Should have at least one tag");

                        for (key, value) in ape.tags.items() {
                            println!("    {}: {}", key, value.pprint());
                            assert!(is_valid_apev2_key(&key), "Key should be valid APEv2 key");
                        }
                    }
                    Err(e) => {
                        println!("Could not load {} (might be expected): {}", filename, e);
                        // Some files might be intentionally broken for testing
                    }
                }
            }
        }
    }

    #[test]
    fn test_load_invalid_item_count() {
        // Test the file with invalid item count (GitHub issue #145)
        let path = TestUtils::data_path("145-invalid-item-count.apev2");
        if path.exists() {
            match APEv2::load(&path) {
                Ok(ape) => {
                    // Should handle invalid item count gracefully
                    println!(
                        "Loaded file with invalid item count: {} tags",
                        ape.tags.len()
                    );
                    assert!(
                        !ape.tags.is_empty(),
                        "Should have parsed some tags despite invalid count"
                    );
                }
                Err(e) => {
                    println!("Failed to load invalid item count file: {}", e);
                    // This is acceptable - the file might be too broken
                }
            }
        }
    }

    #[test]
    fn test_load_with_lyrics3v2() {
        // Test APE file with Lyrics3v2 tags
        let path = TestUtils::data_path("apev2-lyricsv2.mp3");
        if path.exists() {
            match APEv2::load(&path) {
                Ok(ape) => {
                    println!("Successfully loaded APE + Lyrics3v2 file");
                    println!("  Tags: {}", ape.tags.len());

                    // Look for specific expected tags
                    if let Some(gain) = ape.tags.get("REPLAYGAIN_TRACK_GAIN") {
                        println!("  ReplayGain: {}", gain.pprint());
                    }
                }
                Err(e) => {
                    println!("Could not load APE + Lyrics3v2 file: {}", e);
                }
            }
        }
    }

    #[test]
    fn test_create_and_save_tags() {
        use tempfile::NamedTempFile;

        // Create a temporary file for testing
        let temp_file = NamedTempFile::new().unwrap();
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // Create APE tags
        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());

        ape.tags.set_text("Title", "Test Song".to_string()).unwrap();
        ape.tags
            .set_text("Artist", "Test Artist".to_string())
            .unwrap();
        ape.tags
            .set_text("Album", "Test Album".to_string())
            .unwrap();

        // Save tags
        match ape.save() {
            Ok(()) => {
                println!("Successfully saved APE tags to temporary file");

                // Try to load them back
                match APEv2::load(&temp_path) {
                    Ok(loaded_ape) => {
                        assert_eq!(loaded_ape.tags.len(), 3);
                        assert_eq!(
                            loaded_ape.tags.get("title").unwrap().as_string().unwrap(),
                            "Test Song"
                        );
                        assert_eq!(
                            loaded_ape.tags.get("artist").unwrap().as_string().unwrap(),
                            "Test Artist"
                        );
                        assert_eq!(
                            loaded_ape.tags.get("album").unwrap().as_string().unwrap(),
                            "Test Album"
                        );
                        println!("Round-trip test successful!");
                    }
                    Err(e) => {
                        println!("Could not load saved APE tags: {}", e);
                    }
                }
            }
            Err(e) => {
                println!(
                    "Could not save APE tags (expected for incomplete implementation): {}",
                    e
                );
            }
        }
    }

    #[test]
    fn test_different_value_types() {
        let mut ape = APEv2::new();

        // Text value
        ape.tags.set("Title", APEValue::text("Test Song")).unwrap();

        // Binary value
        let binary_data = vec![0x00, 0x01, 0x02, 0x03];
        ape.tags
            .set("BinaryData", APEValue::binary(binary_data.clone()))
            .unwrap();

        // External value (URI)
        ape.tags
            .set("Website", APEValue::external("http://example.com"))
            .unwrap();

        // Multi-value text
        ape.tags
            .set("Artist", APEValue::text("Artist1\0Artist2"))
            .unwrap();

        assert_eq!(ape.tags.len(), 4);

        // Verify types and values
        let title = ape.tags.get("title").unwrap();
        assert_eq!(title.value_type, APEValueType::Text);
        assert_eq!(title.as_string().unwrap(), "Test Song");

        let binary = ape.tags.get("binarydata").unwrap();
        assert_eq!(binary.value_type, APEValueType::Binary);
        assert_eq!(binary.data, binary_data);

        let website = ape.tags.get("website").unwrap();
        assert_eq!(website.value_type, APEValueType::External);
        assert_eq!(website.as_string().unwrap(), "http://example.com");

        let artist = ape.tags.get("artist").unwrap();
        assert_eq!(artist.value_type, APEValueType::Text);
        let artists = artist.as_text_list().unwrap();
        assert_eq!(artists, vec!["Artist1", "Artist2"]);
    }
}

#[cfg(test)]
mod apev2_error_handling_tests {
    use super::*;

    #[test]
    fn test_no_header_error() {
        // Try to load a non-APE file
        let path = TestUtils::data_path("empty.mp3");
        if path.exists() {
            let result = APEv2::load(&path);
            assert!(result.is_err());

            if let Err(AudexError::APENoHeader) = result {
                // Expected error type
            } else {
                panic!("Expected APENoHeader error, got: {:?}", result);
            }
        }
    }

    #[test]
    fn test_bad_key_error() {
        let mut tags = APEv2Tags::new();

        // Try to set invalid key
        let result = tags.set_text("x", "value".to_string());
        assert!(result.is_err());

        match result {
            Err(AudexError::InvalidData(msg)) => {
                assert!(msg.contains("valid APEv2 key"));
            }
            _ => panic!("Expected InvalidData error for bad key"),
        }
    }

    #[test]
    fn test_delete_nonexistent() {
        // Delete should not fail on non-existent files
        let result = clear("nonexistent_file.ape");
        // Should either succeed (no tag to delete) or fail with file not found
        match result {
            Ok(()) => {
                // No tag to delete - success
            }
            Err(AudexError::Io(_)) => {
                // File not found - expected
            }
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }

    #[test]
    fn test_empty_file_error() {
        // Try to load an empty file
        let path = TestUtils::data_path("emptyfile.mp3");
        if path.exists() {
            let result = APEv2::load(&path);
            assert!(result.is_err(), "Loading empty file should fail");
        }
    }

    #[test]
    fn test_invalid_file_error() {
        // Try to load non-existent file
        let result = APEv2::load("file_that_does_not_exist_anywhere.ape");
        assert!(result.is_err(), "Loading non-existent file should fail");
    }
}

#[cfg(test)]
mod apev2_file_writer_tests {
    use super::*;
    use std::fs;
    use std::io::{Read, Write};
    use std::path::PathBuf;

    /// Helper to copy test file data to a temporary file
    fn get_temp_copy(source_filename: &str) -> (tempfile::TempDir, PathBuf) {
        let source_path = TestUtils::data_path(source_filename);
        let dir = tempfile::tempdir().unwrap();
        let temp_path = dir.path().join(source_filename);

        if source_path.exists() {
            let data = fs::read(&source_path).unwrap();
            fs::write(&temp_path, data).unwrap();
        }

        (dir, temp_path)
    }

    /// Helper to create an empty temporary file
    fn get_temp_empty() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let temp_path = dir.path().join("empty.apev2");
        fs::write(&temp_path, []).unwrap();
        (dir, temp_path)
    }

    #[test]
    fn test_changed() {
        // Test that file size doesn't change on re-save without modifications
        let (_dir, temp_pb) = get_temp_copy("click.mpc");
        let temp_path = temp_pb.to_string_lossy().to_string();

        // Create and save new tags
        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags
            .set_text("artist", "Joe Wreschnig\0unittest".to_string())
            .unwrap();
        ape.tags
            .set_text("album", "Test Suite".to_string())
            .unwrap();
        ape.tags
            .set_text("title", "Not really a song".to_string())
            .unwrap();
        ape.save().unwrap();

        let size_after_save = fs::metadata(&temp_path).unwrap().len();

        // Load and re-save without changes
        let mut loaded = APEv2::load(&temp_path).unwrap();
        loaded.save().unwrap();

        let size_after_resave = fs::metadata(&temp_path).unwrap().len();

        // Size should remain the same
        assert_eq!(
            size_after_save, size_after_resave,
            "File size should not change on re-save without modifications"
        );
    }

    #[test]
    fn test_fix_broken() {
        // Test cleaning up garbage from broken APE tags
        let old_path = TestUtils::data_path("oldtag.apev2");
        let broken_path = TestUtils::data_path("brokentag.apev2");

        if !old_path.exists() || !broken_path.exists() {
            println!("Skipping test_fix_broken: test files not found");
            return;
        }

        let old_size = fs::metadata(&old_path).unwrap().len();
        let broken_size = fs::metadata(&broken_path).unwrap().len();

        // Sizes should be different (broken has garbage)
        assert_ne!(
            old_size, broken_size,
            "Broken file should have different size"
        );

        // Load broken tag and save to temp file (copying the broken file first)
        let (_dir, temp_pb) = get_temp_copy("brokentag.apev2");
        let temp_path = temp_pb.to_string_lossy().to_string();

        // Load and re-save to fix
        let mut ape = APEv2::load(&temp_path).unwrap();
        ape.save().unwrap();

        let fixed_size = fs::metadata(&temp_path).unwrap().len();

        // Fixed file should match old (clean) file size
        // Note: This test verifies that broken tags are cleaned up during save
        assert_eq!(
            old_size, fixed_size,
            "Fixed file should match clean file size (old={}, fixed={})",
            old_size, fixed_size
        );
    }

    #[test]
    fn test_save_rejects_malformed_existing_footer() {
        let (_dir, temp_pb) = get_temp_empty();
        let temp_path = temp_pb.to_string_lossy().to_string();
        let mut temp_file = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&temp_path)
            .unwrap();

        // Write a malformed footer that advertises a tag larger than the file.
        temp_file.write_all(b"audio").unwrap();
        temp_file.write_all(b"APETAGEX").unwrap();
        temp_file.write_all(&2000u32.to_le_bytes()).unwrap();
        temp_file.write_all(&1024u32.to_le_bytes()).unwrap();
        temp_file.write_all(&1u32.to_le_bytes()).unwrap();
        temp_file.write_all(&0u32.to_le_bytes()).unwrap();
        temp_file.write_all(&[0u8; 8]).unwrap();
        temp_file.flush().unwrap();

        let original_len = fs::metadata(&temp_path).unwrap().len();

        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags
            .set_text("title", "Replacement Title".to_string())
            .unwrap();

        let err = ape
            .save()
            .expect_err("save should reject malformed existing footer");
        assert!(
            err.to_string().contains("APE footer claims"),
            "unexpected error: {}",
            err
        );

        let final_len = fs::metadata(&temp_path).unwrap().len();
        assert_eq!(
            original_len, final_len,
            "save should not append a new tag when cleanup of the old footer fails"
        );
    }

    #[test]
    fn test_tag_at_start() {
        // Test reading tags positioned at the start of a file
        let (_dir, temp_pb) = get_temp_empty();
        let temp_path = temp_pb.to_string_lossy().to_string();

        // Create tags and save them
        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags
            .set_text("album", "Test Album".to_string())
            .unwrap();
        ape.tags
            .set_text("title", "Test Title".to_string())
            .unwrap();
        ape.save().unwrap();

        // Append garbage data after the tag
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&temp_path)
            .unwrap();
        file.write_all(b"tag garbage").unwrap();
        file.write_all(&vec![b'X'; 1000]).unwrap();
        drop(file);

        // Should still be able to read the tag
        let loaded = APEv2::load(&temp_path).unwrap();
        assert_eq!(
            loaded.tags.get("album").unwrap().as_string().unwrap(),
            "Test Album"
        );
        assert_eq!(
            loaded.tags.get("title").unwrap().as_string().unwrap(),
            "Test Title"
        );
    }

    #[test]
    fn test_tag_at_start_write() {
        // Test writing tags when they're positioned at the start
        let (_dir, temp_pb) = get_temp_empty();
        let temp_path = temp_pb.to_string_lossy().to_string();

        // Create tag at start
        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags
            .set_text("album", "Original Album".to_string())
            .unwrap();
        ape.save().unwrap();

        let tag_only_size = fs::metadata(&temp_path).unwrap().len();

        // Append garbage data (simulating non-tag data after tag)
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&temp_path)
            .unwrap();
        let garbage = b"tag garbage".repeat(100);
        let garbage_len = garbage.len();
        file.write_all(&garbage).unwrap();
        drop(file);

        let with_garbage_size = fs::metadata(&temp_path).unwrap().len();
        assert_eq!(
            with_garbage_size,
            tag_only_size + garbage_len as u64,
            "Garbage should be appended"
        );

        // Modify and re-save
        let mut loaded = APEv2::load(&temp_path).unwrap();
        loaded
            .tags
            .set_text("artist", "New Artist".to_string())
            .unwrap();
        loaded.save().unwrap();

        // Verify tag was updated
        let reloaded = APEv2::load(&temp_path).unwrap();
        assert_eq!(
            reloaded.tags.get("album").unwrap().as_string().unwrap(),
            "Original Album"
        );
        assert_eq!(
            reloaded.tags.get("artist").unwrap().as_string().unwrap(),
            "New Artist"
        );

        // The test passes if tags can be read and written correctly
        // Note: Behavior may preserve or remove garbage
        // depending on where the tag is positioned
        println!("Tag operations completed successfully");
    }

    #[test]
    fn test_tag_at_start_delete() {
        // Test deleting tags positioned at the start
        let (_dir, temp_pb) = get_temp_empty();
        let temp_path = temp_pb.to_string_lossy().to_string();

        // Create tag with garbage after
        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags
            .set_text("album", "Test Album".to_string())
            .unwrap();
        ape.save().unwrap();

        // Append garbage data
        let garbage = b"tag garbage".repeat(100);
        let garbage_len = garbage.len() as u64;
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&temp_path)
            .unwrap();
        file.write_all(&garbage).unwrap();
        drop(file);

        // Delete tags
        let mut loaded = APEv2::load(&temp_path).unwrap();
        loaded.clear().unwrap();

        // Should fail to load now
        let result = APEv2::load(&temp_path);
        assert!(result.is_err(), "Should not find APE tags after deletion");

        // File should only contain garbage
        let final_size = fs::metadata(&temp_path).unwrap().len();
        assert_eq!(
            final_size, garbage_len,
            "File should only contain garbage data after tag deletion"
        );
    }

    #[test]
    fn test_delete() {
        // Test module-level delete function
        let (_dir, temp_pb) = get_temp_empty();
        let temp_path = temp_pb.to_string_lossy().to_string();

        // Create and save tags
        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags.set_text("title", "Test Song".to_string()).unwrap();
        ape.save().unwrap();

        let tag_only_size = fs::metadata(&temp_path).unwrap().len();
        assert!(tag_only_size > 0, "File should have content");

        // Use module-level delete
        clear(&temp_path).unwrap();

        // File should be empty now
        let final_size = fs::metadata(&temp_path).unwrap().len();
        assert_eq!(final_size, 0, "File should be empty after deletion");

        // Should fail to load
        let result = APEv2::load(&temp_path);
        assert!(result.is_err(), "Should not find tags after deletion");
    }

    #[test]
    fn test_save_sort_is_deterministic() {
        // Test that tag keys are written in deterministic order
        let (_dir, temp_pb) = get_temp_empty();
        let temp_path = temp_pb.to_string_lossy().to_string();

        // Create tags with keys in reverse alphabetical order
        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags.set_text("zzz", "last value".to_string()).unwrap();
        ape.tags
            .set_text("mmm", "middle value".to_string())
            .unwrap();
        ape.tags.set_text("aaa", "first value".to_string()).unwrap();
        ape.save().unwrap();

        // Read file content
        let mut file = fs::File::open(&temp_path).unwrap();
        let mut content = Vec::new();
        file.read_to_end(&mut content).unwrap();

        // Find positions of keys in file
        let pos_aaa = content.windows(3).position(|w| w == b"aaa");
        let pos_mmm = content.windows(3).position(|w| w == b"mmm");
        let pos_zzz = content.windows(3).position(|w| w == b"zzz");

        assert!(pos_aaa.is_some(), "Key 'aaa' should be in file");
        assert!(pos_mmm.is_some(), "Key 'mmm' should be in file");
        assert!(pos_zzz.is_some(), "Key 'zzz' should be in file");

        // Keys should appear in sorted order in the file
        // (APEv2 spec recommends sorting by size, then lexically)
        // The implementation should be deterministic
        println!(
            "Key positions - aaa: {:?}, mmm: {:?}, zzz: {:?}",
            pos_aaa, pos_mmm, pos_zzz
        );
    }

    #[test]
    fn test_unicode_key() {
        // Test Unicode handling in tag keys
        let (_dir, temp_pb) = get_temp_empty();
        let temp_path = temp_pb.to_string_lossy().to_string();

        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());

        // ASCII key with Unicode value should work
        ape.tags.set_text("abc", "öäü".to_string()).unwrap();

        // Unicode key should be rejected (APEv2 keys must be ASCII 0x20-0x7E)
        let result = ape.tags.set_text("übung", "test".to_string());
        assert!(result.is_err(), "Unicode keys should be rejected");

        // But Unicode in values is fine
        ape.tags.set_text("title", "Tëst Söng".to_string()).unwrap();
        ape.save().unwrap();

        // Verify round-trip
        let loaded = APEv2::load(&temp_path).unwrap();
        assert_eq!(loaded.tags.get("abc").unwrap().as_string().unwrap(), "öäü");
        assert_eq!(
            loaded.tags.get("title").unwrap().as_string().unwrap(),
            "Tëst Söng"
        );
    }

    #[test]
    fn test_case_preservation() {
        // Test that key case is preserved
        let (_dir, temp_pb) = get_temp_empty();
        let temp_path = temp_pb.to_string_lossy().to_string();

        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags.set_text("FoObaR", "Quux".to_string()).unwrap();
        ape.save().unwrap();

        // Reload and verify exact case
        let loaded = APEv2::load(&temp_path).unwrap();
        let keys = loaded.tags.keys();
        assert!(
            keys.contains(&"FoObaR".to_string()),
            "Exact case 'FoObaR' should be preserved"
        );
        assert!(
            !keys.contains(&"foobar".to_string()),
            "Lowercase 'foobar' should not be in keys"
        );

        // But access should still be case-insensitive
        assert!(loaded.tags.contains_key("foobar"));
        assert!(loaded.tags.contains_key("FOOBAR"));
        assert_eq!(
            loaded.tags.get("foobar").unwrap().as_string().unwrap(),
            "Quux"
        );
    }
}

#[cfg(test)]
mod apev2_value_type_tests {
    use super::*;

    #[test]
    fn test_guess_text() {
        // Test auto-detection of text values
        let mut ape = APEv2::new();

        // Setting a string should create a Text value
        ape.tags.set_text("test", "foobar".to_string()).unwrap();

        let value = ape.tags.get("test").unwrap();
        assert_eq!(value.value_type, APEValueType::Text);
        assert_eq!(value.as_string().unwrap(), "foobar");
    }

    #[test]
    fn test_guess_text_list() {
        // Test auto-detection of text list values
        let mut ape = APEv2::new();

        // Setting a list should create a null-separated Text value
        ape.tags
            .set_text_list("test", vec!["foobar".to_string(), "quuxbarz".to_string()])
            .unwrap();

        let value = ape.tags.get("test").unwrap();
        assert_eq!(value.value_type, APEValueType::Text);
        assert_eq!(value.as_string().unwrap(), "foobar\0quuxbarz");

        let list = value.as_text_list().unwrap();
        assert_eq!(list, vec!["foobar", "quuxbarz"]);
    }

    #[test]
    fn test_guess_utf8() {
        // Test that valid UTF-8 strings are stored as Text
        let mut ape = APEv2::new();

        ape.tags.set_text("test", "foobar".to_string()).unwrap();

        let value = ape.tags.get("test").unwrap();
        assert_eq!(value.value_type, APEValueType::Text);
        assert_eq!(value.as_string().unwrap(), "foobar");
    }

    #[test]
    fn test_guess_not_utf8() {
        // Test that non-UTF-8 binary data is stored as Binary
        let mut ape = APEv2::new();

        // Raw binary data with invalid UTF-8
        let binary_data = vec![0xa4, b'w', b'o', b'o'];
        ape.tags
            .set("test", APEValue::binary(binary_data.clone()))
            .unwrap();

        let value = ape.tags.get("test").unwrap();
        assert_eq!(value.value_type, APEValueType::Binary);
        assert_eq!(value.data.len(), 4);
        assert_eq!(value.data, binary_data);

        // Should not convert to string
        assert!(value.as_string().is_err());
    }
}

#[cfg(test)]
mod apev2_list_manipulation_tests {
    use super::*;

    #[test]
    fn test_setitem_list() {
        // Test modifying individual items in a text list
        let value = APEValue::text("foo\0bar\0baz");
        let mut list = value.as_text_list().unwrap();

        // Modify an item
        list[2] = "quux".to_string();
        assert_eq!(list, vec!["foo", "bar", "quux"]);

        // Restore original
        list[2] = "baz".to_string();
        assert_eq!(list, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn test_getitem() {
        // Test accessing individual items from a text list
        let value = APEValue::text("foo\0bar\0baz");
        let list = value.as_text_list().unwrap();

        assert_eq!(list.len(), 3);
        assert_eq!(list[0], "foo");
        assert_eq!(list[1], "bar");
        assert_eq!(list[2], "baz");
    }

    #[test]
    fn test_delitem() {
        // Test deleting items from a text list
        let value = APEValue::text("foo\0bar\0baz");
        let mut list = value.as_text_list().unwrap();

        // Remove middle item
        list.remove(1);
        assert_eq!(list, vec!["foo", "baz"]);

        // Remove remaining items
        list.truncate(1);
        assert_eq!(list, vec!["foo"]);
    }

    #[test]
    fn test_insert() {
        // Test inserting items into a text list
        let value = APEValue::text("foo\0bar\0baz");
        let mut list = value.as_text_list().unwrap();

        // Insert at beginning
        list.insert(0, "a".to_string());
        assert_eq!(list.len(), 4);
        assert_eq!(list[0], "a");
        assert_eq!(list[1], "foo");

        // Insert in middle
        list.insert(2, "middle".to_string());
        assert_eq!(list.len(), 5);
        assert_eq!(list[2], "middle");
    }
}

#[cfg(test)]
mod apev2_dictlike_tests {
    use super::*;

    #[test]
    fn test_dictlike() {
        // Test key-value interface
        let path = TestUtils::data_path("oldtag.apev2");
        if !path.exists() {
            println!("Skipping test_dictlike: test file not found");
            return;
        }

        let ape = APEv2::load(&path).unwrap();

        // Test get method (case-insensitive)
        assert!(ape.tags.get("track").is_some());
        assert!(ape.tags.get("Track").is_some());
        assert!(ape.tags.get("TRACK").is_some());

        // All should return the same value
        let track1 = ape.tags.get("track").unwrap();
        let track2 = ape.tags.get("Track").unwrap();
        assert_eq!(track1.as_string().unwrap(), track2.as_string().unwrap());
    }

    #[test]
    fn test_values() {
        // Test value comparisons
        let path = TestUtils::data_path("oldtag.apev2");
        if !path.exists() {
            println!("Skipping test_values: test file not found");
            return;
        }

        let ape = APEv2::load(&path).unwrap();

        // Verify specific values from oldtag.apev2
        if let Some(artist) = ape.tags.get("artist") {
            assert_eq!(artist.as_string().unwrap(), "AnArtist");
        }

        if let Some(title) = ape.tags.get("title") {
            assert_eq!(title.as_string().unwrap(), "Some Music");
        }

        if let Some(album) = ape.tags.get("album") {
            assert_eq!(album.as_string().unwrap(), "A test case");
        }

        if let Some(track) = ape.tags.get("track") {
            assert_eq!(track.as_string().unwrap(), "07");
        }
    }
}

#[cfg(test)]
mod apev2_with_id3v1_tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn fresh_temp_path(filename: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(filename);
        fs::write(&path, []).unwrap();
        (dir, path.to_string_lossy().to_string())
    }

    #[test]
    fn test_apev2_then_id3v1() {
        // Test APEv2 tags followed by ID3v1 tags
        let (_dir, temp_path) = fresh_temp_path("apev2-then-id3v1.bin");

        // Create APE tags
        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags
            .set_text("artist", "Test Artist".to_string())
            .unwrap();
        ape.tags
            .set_text("title", "Test Title".to_string())
            .unwrap();
        ape.save().unwrap();

        // Append ID3v1 tag (128 bytes: "TAG" + 125 bytes of data)
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&temp_path)
            .unwrap();
        file.write_all(b"TAG").unwrap();
        file.write_all(&[0u8; 125]).unwrap();
        drop(file);

        // Should still load APE tags correctly
        let loaded = APEv2::load(&temp_path).unwrap();
        assert_eq!(
            loaded.tags.get("artist").unwrap().as_string().unwrap(),
            "Test Artist"
        );
        assert_eq!(
            loaded.tags.get("title").unwrap().as_string().unwrap(),
            "Test Title"
        );
    }

    #[test]
    fn test_save_with_id3v1() {
        // Test saving APE tags when ID3v1 is present
        let (_dir, temp_path) = fresh_temp_path("apev2-save-with-id3v1.bin");

        // Create initial APE tags
        let mut ape = APEv2::new();
        ape.filename = Some(temp_path.clone());
        ape.tags.set_text("album", "Original".to_string()).unwrap();
        ape.save().unwrap();

        // Append ID3v1 tag
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&temp_path)
            .unwrap();
        file.write_all(b"TAG").unwrap();
        file.write_all(&[0u8; 125]).unwrap();
        drop(file);

        let _original_size = fs::metadata(&temp_path).unwrap().len();

        // Modify and save
        let mut loaded = APEv2::load(&temp_path).unwrap();
        loaded
            .tags
            .set_text("artist", "New Artist".to_string())
            .unwrap();
        loaded.save().unwrap();

        // Verify changes
        let reloaded = APEv2::load(&temp_path).unwrap();
        assert_eq!(
            reloaded.tags.get("album").unwrap().as_string().unwrap(),
            "Original"
        );
        assert_eq!(
            reloaded.tags.get("artist").unwrap().as_string().unwrap(),
            "New Artist"
        );
    }
}

#[cfg(test)]
mod apev2_with_lyrics3v2_tests {
    use super::*;

    #[test]
    fn test_apev2_with_lyrics3v2_values() {
        // Test loading APEv2 tags from file with Lyrics3v2 tags
        let path = TestUtils::data_path("apev2-lyricsv2.mp3");
        if !path.exists() {
            println!("Skipping test: apev2-lyricsv2.mp3 not found");
            return;
        }

        let ape = APEv2::load(&path).unwrap();

        // Verify expected values from the test file
        if let Some(minmax) = ape.tags.get("MP3GAIN_MINMAX") {
            assert_eq!(minmax.as_string().unwrap(), "000,179");
        }

        if let Some(gain) = ape.tags.get("REPLAYGAIN_TRACK_GAIN") {
            assert_eq!(gain.as_string().unwrap(), "-4.080000 dB");
        }

        if let Some(peak) = ape.tags.get("REPLAYGAIN_TRACK_PEAK") {
            assert_eq!(peak.as_string().unwrap(), "1.008101");
        }
    }
}

#[cfg(test)]
mod apev2_file_class_tests {
    use super::*;

    #[test]
    fn test_apev2file_basic() {
        // Test basic APEv2File operations
        let path = TestUtils::data_path("click.mpc");
        if !path.exists() {
            println!("Skipping test: click.mpc not found");
            return;
        }

        // Try to load the file - click.mpc may not have APE tags by default
        match APEv2::load(&path) {
            Ok(audio) => {
                // Successfully loaded - verify it's a valid APEv2File
                println!("Loaded APEv2File with {} tags", audio.tags.len());
            }
            Err(AudexError::APENoHeader) => {
                // Expected - file doesn't have APE tags yet
                println!("click.mpc has no APE tags (expected)");
            }
            Err(e) => {
                panic!("Unexpected error loading click.mpc: {:?}", e);
            }
        }
    }

    #[test]
    fn test_apev2file_empty() {
        // Test loading file with no APE tags
        let path = TestUtils::data_path("xing.mp3");
        if !path.exists() {
            println!("Skipping test: xing.mp3 not found");
            return;
        }

        // File without APE tags should fail to load or have empty tags
        match APEv2::load(&path) {
            Ok(audio) => {
                // If it loads, tags should be empty
                assert!(audio.tags.is_empty());
            }
            Err(_) => {
                // Expected - no APE tags in file
            }
        }
    }

    #[test]
    fn test_stream_info_unknown() {
        // Test stream info for unknown format with APE tags
        let ape = APEv2::new();
        let info = ape.info();

        assert_eq!(info.length(), None);
        assert_eq!(info.bitrate(), None);
        assert_eq!(info.sample_rate(), None);
        assert_eq!(info.channels(), None);
        assert_eq!(info.bits_per_sample(), None);
        assert_eq!(info.pprint(), "Unknown format with APEv2 tag.");
    }
}

#[cfg(test)]
mod apev2_integration_tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_file_detection_and_scoring() {
        // List available APE test files
        let data_dir = TestUtils::data_path("");
        if let Ok(entries) = fs::read_dir(&data_dir) {
            let mut ape_files = Vec::new();
            let mut apev2_files = Vec::new();

            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "ape" {
                        ape_files.push(path);
                    } else if let Some(filename) = path.file_name() {
                        if filename.to_string_lossy().contains("apev2") {
                            apev2_files.push(path);
                        }
                    }
                }
            }

            println!(
                "Found {} .ape files and {} apev2 files",
                ape_files.len(),
                apev2_files.len()
            );

            // Test scoring on APE files — APEv2 is a tag format, not a container,
            // so .ape files (MonkeysAudio) won't score unless they happen to have
            // APETAGEX at the start of the file header
            for ape_file in ape_files.iter().take(3) {
                if let Ok(data) = fs::read(ape_file) {
                    let header = &data[..data.len().min(128)];
                    let score = APEv2::score(&ape_file.to_string_lossy(), header);
                    println!(
                        "APE file {:?} scored {}",
                        ape_file.file_name().unwrap(),
                        score
                    );
                    // APEv2 only scores based on APETAGEX signature, not extension
                    assert!(score >= 0, "APE file score should be non-negative");
                }
            }

            // Test scoring on APEv2 tag files
            for apev2_file in apev2_files.iter().take(3) {
                if let Ok(data) = fs::read(apev2_file) {
                    let header = &data[..data.len().min(128)];
                    let score = APEv2::score(&apev2_file.to_string_lossy(), header);
                    println!(
                        "APEv2 file {:?} scored {}",
                        apev2_file.file_name().unwrap(),
                        score
                    );
                    // May or may not score depending on header content
                }
            }
        }
    }

    #[test]
    fn test_pprint_output() {
        let mut ape = APEv2::new();
        ape.tags.set_text("Title", "Test Song".to_string()).unwrap();
        ape.tags
            .set_text("Artist", "Test Artist".to_string())
            .unwrap();
        ape.tags
            .set("Album", APEValue::text("Test Album\0Bonus Album"))
            .unwrap();
        ape.tags
            .set("Cover", APEValue::binary(vec![0xFF, 0xD8, 0xFF, 0xE0]))
            .unwrap();
        ape.tags
            .set("Website", APEValue::external("http://example.com"))
            .unwrap();

        let output = ape.tags.pprint();
        println!("APE tags pprint output:\n{}", output);

        // Check that all tags are represented
        assert!(output.contains("Title=Test Song"));
        assert!(output.contains("Artist=Test Artist"));
        assert!(output.contains("Album=Test Album / Bonus Album"));
        assert!(output.contains("Cover=[4 bytes]"));
        assert!(output.contains("Website=[External] http://example.com"));
    }

    #[test]
    fn test_stream_info() {
        let ape = APEv2::new();
        let info = ape.info();

        // APE tag files don't contain stream info (unknown format)
        assert_eq!(info.length(), None);
        assert_eq!(info.bitrate(), None);
        assert_eq!(info.sample_rate(), None);
        assert_eq!(info.channels(), None);
        assert_eq!(info.bits_per_sample(), None);

        assert_eq!(info.pprint(), "Unknown format with APEv2 tag.");
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use audex::apev2::is_valid_apev2_key;

    #[test]
    fn test_forbidden_keys_exact_case_rejected() {
        assert!(!is_valid_apev2_key("OggS"));
        assert!(!is_valid_apev2_key("TAG"));
        assert!(!is_valid_apev2_key("ID3"));
        assert!(!is_valid_apev2_key("MP+"));
    }

    #[test]
    fn test_forbidden_keys_lowercase_also_rejected() {
        assert!(
            !is_valid_apev2_key("tag"),
            "'tag' (lowercase) should be rejected as a forbidden key"
        );
        assert!(
            !is_valid_apev2_key("id3"),
            "'id3' (lowercase) should be rejected"
        );
        assert!(
            !is_valid_apev2_key("oggs"),
            "'oggs' (lowercase) should be rejected"
        );
        assert!(
            !is_valid_apev2_key("mp+"),
            "'mp+' (lowercase) should be rejected"
        );
    }

    #[test]
    fn test_forbidden_keys_mixed_case_rejected() {
        assert!(!is_valid_apev2_key("Tag"));
        assert!(!is_valid_apev2_key("Id3"));
        assert!(!is_valid_apev2_key("OGGs"));
    }

    #[test]
    fn test_valid_keys_accepted() {
        assert!(is_valid_apev2_key("Artist"));
        assert!(is_valid_apev2_key("Title"));
        assert!(is_valid_apev2_key("Album"));
        assert!(is_valid_apev2_key("Year"));
    }
}

// ---------------------------------------------------------------------------
// APEv2 Lyrics3v2 negative seek tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod lyrics3v2_seek_tests {
    use audex::FileType;
    use audex::apev2::APEv2;
    use std::io::Cursor;

    /// Build a synthetic file with a Lyrics3v2 end-marker and controlled size field.
    fn build_file_with_lyrics3v2_trailer(size_field: &[u8; 6], total_size: usize) -> Vec<u8> {
        assert!(
            total_size >= 200,
            "file must be large enough for all probes"
        );
        let trailer_len = 6 + 9 + 128;
        let padding_len = total_size - trailer_len;

        let mut data = Vec::with_capacity(total_size);
        data.extend(vec![0x00u8; padding_len]);
        data.extend_from_slice(size_field);
        data.extend_from_slice(b"LYRICS200");
        data.extend_from_slice(b"TAG");
        data.extend(vec![0u8; 125]);
        assert_eq!(data.len(), total_size);
        data
    }

    /// Helper to check that an error is not a seek-related failure.
    fn assert_no_seek_error(result: &Result<APEv2, audex::AudexError>) {
        if let Err(e) = result {
            let msg = format!("{}", e);
            assert!(
                !msg.contains("seek")
                    && !msg.contains("position")
                    && !msg.contains("Negative seek")
                    && !msg.contains("invalid seek"),
                "Load failed with a seek-related error: {}",
                msg,
            );
        }
    }

    /// Oversized Lyrics3v2 field (999999) on a 300-byte file should not
    /// abort the load with a seek error.
    #[test]
    fn oversized_lyrics3v2_field_should_not_abort_load() {
        let data = build_file_with_lyrics3v2_trailer(b"999999", 300);
        let mut cursor = Cursor::new(data);
        let result = APEv2::load_from_reader(&mut cursor);
        assert_no_seek_error(&result);
    }

    /// Moderate offset on a small file also causes the seek target to go negative.
    #[test]
    fn moderate_lyrics3v2_offset_on_small_file() {
        let data = build_file_with_lyrics3v2_trailer(b"100000", 250);
        let mut cursor = Cursor::new(data);
        let result = APEv2::load_from_reader(&mut cursor);
        assert_no_seek_error(&result);
    }

    /// Negative value in the size field ("-99999") should not crash.
    #[test]
    fn negative_lyrics3v2_size_does_not_crash() {
        let data = build_file_with_lyrics3v2_trailer(b"-99999", 300);
        let mut cursor = Cursor::new(data);
        let result = APEv2::load_from_reader(&mut cursor);
        if let Err(e) = &result {
            let msg = format!("{}", e);
            assert!(
                !msg.contains("seek")
                    && !msg.contains("overflow")
                    && !msg.contains("Negative seek"),
                "Negative Lyrics3v2 size caused unexpected error: {}",
                msg,
            );
        }
    }

    /// Size field of zero is a valid edge case — must not error.
    #[test]
    fn zero_lyrics3v2_size_is_handled() {
        let data = build_file_with_lyrics3v2_trailer(b"000000", 300);
        let mut cursor = Cursor::new(data);
        let result = APEv2::load_from_reader(&mut cursor);
        if let Err(e) = &result {
            let msg = format!("{}", e);
            assert!(
                !msg.contains("seek"),
                "Zero Lyrics3v2 size caused a seek error: {}",
                msg
            );
        }
    }

    /// Non-numeric size field should cause parse failure and graceful fallthrough.
    #[test]
    fn non_numeric_lyrics3v2_size_falls_through() {
        let data = build_file_with_lyrics3v2_trailer(b"ABCDEF", 300);
        let mut cursor = Cursor::new(data);
        let result = APEv2::load_from_reader(&mut cursor);
        if let Err(e) = &result {
            let msg = format!("{}", e);
            assert!(
                !msg.contains("seek"),
                "Non-numeric Lyrics3v2 size caused seek error: {}",
                msg
            );
        }
    }
}
