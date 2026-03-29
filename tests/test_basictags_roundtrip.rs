/// Tests for BasicTags round-trip functionality
/// - Comment preservation through save/load cycles
/// - Update semantics for key-value tags
/// - Multiple value support
/// - Case handling
use audex::tags::{BasicTags, Metadata, Tags};
use audex::util::{AnyFileThing, FileInput, LoadFileOptions};
use std::io::{Cursor, Seek, SeekFrom};

#[test]
fn test_roundtrip_single_values() {
    // Test round-trip with single values for each key
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["Test Artist".to_string()]);
    tags.set("album", vec!["Test Album".to_string()]);
    tags.set("title", vec!["Test Title".to_string()]);
    tags.set("date", vec!["2025".to_string()]);

    // Save to buffer
    let buffer = Cursor::new(Vec::new());
    let mut filething = AnyFileThing::from(buffer.clone());
    tags.save_to_fileobj(&mut filething).unwrap();

    // Reload
    filething.seek(SeekFrom::Start(0)).unwrap();
    let loaded_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();

    // Verify all values preserved
    assert_eq!(
        loaded_tags.get("artist"),
        Some(&["Test Artist".to_string()][..])
    );
    assert_eq!(
        loaded_tags.get("album"),
        Some(&["Test Album".to_string()][..])
    );
    assert_eq!(
        loaded_tags.get("title"),
        Some(&["Test Title".to_string()][..])
    );
    assert_eq!(loaded_tags.get("date"), Some(&["2025".to_string()][..]));
}

#[test]
fn test_roundtrip_multiple_values() {
    // Test round-trip with multiple values per key
    let mut tags = BasicTags::new();
    tags.set(
        "artist",
        vec![
            "Artist 1".to_string(),
            "Artist 2".to_string(),
            "Artist 3".to_string(),
        ],
    );
    tags.set("genre", vec!["Rock".to_string(), "Alternative".to_string()]);

    // Save to buffer
    let buffer = Cursor::new(Vec::new());
    let mut filething = AnyFileThing::from(buffer.clone());
    tags.save_to_fileobj(&mut filething).unwrap();

    // Reload
    filething.seek(SeekFrom::Start(0)).unwrap();
    let loaded_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();

    // Verify all values preserved
    assert_eq!(
        loaded_tags.get("artist"),
        Some(
            &[
                "Artist 1".to_string(),
                "Artist 2".to_string(),
                "Artist 3".to_string()
            ][..]
        )
    );
    assert_eq!(
        loaded_tags.get("genre"),
        Some(&["Rock".to_string(), "Alternative".to_string()][..])
    );
}

#[test]
fn test_update_semantics() {
    // Test that updates properly replace values
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["Old Artist".to_string()]);

    // Save
    let buffer = Cursor::new(Vec::new());
    let mut filething = AnyFileThing::from(buffer.clone());
    tags.save_to_fileobj(&mut filething).unwrap();

    // Reload and update
    filething.seek(SeekFrom::Start(0)).unwrap();
    let mut loaded_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();
    loaded_tags.set("artist", vec!["New Artist".to_string()]);
    loaded_tags.set("album", vec!["New Album".to_string()]);

    // Save again
    filething.seek(SeekFrom::Start(0)).unwrap();
    loaded_tags.save_to_fileobj(&mut filething).unwrap();

    // Reload again
    filething.seek(SeekFrom::Start(0)).unwrap();
    let final_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();

    // Verify updates
    assert_eq!(
        final_tags.get("artist"),
        Some(&["New Artist".to_string()][..])
    );
    assert_eq!(
        final_tags.get("album"),
        Some(&["New Album".to_string()][..])
    );
}

#[test]
fn test_remove_key() {
    // Test removing keys
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["Test Artist".to_string()]);
    tags.set("album", vec!["Test Album".to_string()]);
    tags.set("title", vec!["Test Title".to_string()]);

    // Save
    let buffer = Cursor::new(Vec::new());
    let mut filething = AnyFileThing::from(buffer.clone());
    tags.save_to_fileobj(&mut filething).unwrap();

    // Reload and remove a key
    filething.seek(SeekFrom::Start(0)).unwrap();
    let mut loaded_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();
    loaded_tags.remove("album");

    // Save again
    filething.seek(SeekFrom::Start(0)).unwrap();
    loaded_tags.save_to_fileobj(&mut filething).unwrap();

    // Reload and verify
    filething.seek(SeekFrom::Start(0)).unwrap();
    let final_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();

    assert_eq!(
        final_tags.get("artist"),
        Some(&["Test Artist".to_string()][..])
    );
    assert_eq!(final_tags.get("album"), None);
    assert_eq!(
        final_tags.get("title"),
        Some(&["Test Title".to_string()][..])
    );
}

#[test]
fn test_empty_tags() {
    // Test round-trip with empty tags
    let tags = BasicTags::new();

    // Save
    let buffer = Cursor::new(Vec::new());
    let mut filething = AnyFileThing::from(buffer.clone());
    tags.save_to_fileobj(&mut filething).unwrap();

    // Reload
    filething.seek(SeekFrom::Start(0)).unwrap();
    let loaded_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();

    // Should be empty
    assert_eq!(loaded_tags.keys().len(), 0);
}

#[test]
fn test_unicode_values() {
    // Test round-trip with Unicode values
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["测试艺术家".to_string()]);
    tags.set("album", vec!["Тестовый альбом".to_string()]);
    tags.set("title", vec!["🎵 Test Title 🎶".to_string()]);

    // Save
    let buffer = Cursor::new(Vec::new());
    let mut filething = AnyFileThing::from(buffer.clone());
    tags.save_to_fileobj(&mut filething).unwrap();

    // Reload
    filething.seek(SeekFrom::Start(0)).unwrap();
    let loaded_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();

    // Verify Unicode preserved
    assert_eq!(
        loaded_tags.get("artist"),
        Some(&["测试艺术家".to_string()][..])
    );
    assert_eq!(
        loaded_tags.get("album"),
        Some(&["Тестовый альбом".to_string()][..])
    );
    assert_eq!(
        loaded_tags.get("title"),
        Some(&["🎵 Test Title 🎶".to_string()][..])
    );
}

#[test]
fn test_large_values() {
    // Test with large values
    let mut tags = BasicTags::new();
    let large_value = "x".repeat(10000);
    tags.set("comment", vec![large_value.clone()]);

    // Save
    let buffer = Cursor::new(Vec::new());
    let mut filething = AnyFileThing::from(buffer.clone());
    tags.save_to_fileobj(&mut filething).unwrap();

    // Reload
    filething.seek(SeekFrom::Start(0)).unwrap();
    let loaded_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();

    // Verify large value preserved
    assert_eq!(loaded_tags.get("comment"), Some(&[large_value][..]));
}

#[test]
fn test_special_characters_in_keys() {
    // Test keys with special characters (valid in Vorbis/APEv2)
    let mut tags = BasicTags::new();
    tags.set("CUSTOM_KEY", vec!["Value 1".to_string()]);
    tags.set("another-key", vec!["Value 2".to_string()]);
    tags.set("key.with.dots", vec!["Value 3".to_string()]);

    // Save
    let buffer = Cursor::new(Vec::new());
    let mut filething = AnyFileThing::from(buffer.clone());
    tags.save_to_fileobj(&mut filething).unwrap();

    // Reload
    filething.seek(SeekFrom::Start(0)).unwrap();
    let loaded_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();

    // Verify all keys preserved
    assert_eq!(
        loaded_tags.get("CUSTOM_KEY"),
        Some(&["Value 1".to_string()][..])
    );
    assert_eq!(
        loaded_tags.get("another-key"),
        Some(&["Value 2".to_string()][..])
    );
    assert_eq!(
        loaded_tags.get("key.with.dots"),
        Some(&["Value 3".to_string()][..])
    );
}

#[test]
fn test_delete_all_tags() {
    // Test deleting all tags from a file
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["Test Artist".to_string()]);
    tags.set("album", vec!["Test Album".to_string()]);

    // Save
    let buffer = Cursor::new(Vec::new());
    let mut filething = AnyFileThing::from(buffer.clone());
    tags.save_to_fileobj(&mut filething).unwrap();

    // Delete all tags
    filething.seek(SeekFrom::Start(0)).unwrap();
    BasicTags::delete_from_fileobj(&mut filething).unwrap();

    // Try to load - should get empty tags
    filething.seek(SeekFrom::Start(0)).unwrap();
    let loaded_tags = BasicTags::load_from_fileobj(&mut filething).unwrap();

    assert_eq!(loaded_tags.keys().len(), 0);
}

#[test]
fn test_keys_list() {
    // Test that keys() returns all keys
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["Test".to_string()]);
    tags.set("album", vec!["Test".to_string()]);
    tags.set("title", vec!["Test".to_string()]);

    let keys = tags.keys();
    assert_eq!(keys.len(), 3);
    assert!(keys.contains(&"artist".to_string()));
    assert!(keys.contains(&"album".to_string()));
    assert!(keys.contains(&"title".to_string()));
}

#[test]
fn test_set_empty_values_removes_key() {
    // Test that setting empty values removes the key
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["Test Artist".to_string()]);
    assert!(tags.get("artist").is_some());

    // Set to empty
    tags.set("artist", vec![]);
    assert!(tags.get("artist").is_none());
}

#[test]
fn test_pprint_format() {
    // Test pretty print format
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["Test Artist".to_string()]);
    tags.set("album", vec!["Test Album".to_string()]);

    let output = tags.pprint();
    assert!(output.contains("artist=Test Artist"));
    assert!(output.contains("album=Test Album"));
}

#[test]
fn test_basictags_file_backed() {
    // Test with actual file
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test_tags.dat");

    // Create and save tags
    let mut tags = BasicTags::new();
    tags.set("ARTIST", vec!["File Artist".to_string()]);

    // Create the file first
    std::fs::write(&test_path, b"").unwrap();

    let options = LoadFileOptions::write_method();
    let mut file_thing =
        audex::util::openfile_simple(FileInput::from_path(&test_path), &options).unwrap();

    tags.save_to_fileobj(&mut file_thing).unwrap();

    // Load in a new file_thing
    let options2 = LoadFileOptions::read_method();
    let mut file_thing2 =
        audex::util::openfile_simple(FileInput::from_path(&test_path), &options2).unwrap();

    let loaded_tags = BasicTags::load_from_fileobj(&mut file_thing2).unwrap();
    assert_eq!(
        loaded_tags.get("ARTIST"),
        Some(&vec!["File Artist".to_string()][..])
    );
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use audex::tags::{BasicTags, Metadata, Tags};
    use audex::util::AnyFileThing;
    use std::io::Cursor;

    fn build_basictags_with_entry_count(entry_count: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"ADXBTAGS");
        data.extend_from_slice(&entry_count.to_le_bytes());
        data
    }

    fn build_basictags_with_string_length(string_len: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"ADXBTAGS");
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&string_len.to_le_bytes());
        data
    }

    fn build_basictags_with_value_count(value_count: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"ADXBTAGS");
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(b"foo");
        data.extend_from_slice(&value_count.to_le_bytes());
        data
    }

    #[test]
    fn test_rejects_huge_entry_count() {
        let data = build_basictags_with_entry_count(0xFFFF_FFFF);
        let cursor = Cursor::new(data);
        let mut filething = AnyFileThing::from(cursor);

        let result = BasicTags::load_from_fileobj(&mut filething);

        assert!(
            result.is_err(),
            "Should reject absurd entry count before looping 4 billion times"
        );
    }

    #[test]
    fn test_rejects_huge_string_length() {
        let data = build_basictags_with_string_length(2 * 1024 * 1024 * 1024);
        let cursor = Cursor::new(data);
        let mut filething = AnyFileThing::from(cursor);

        let result = BasicTags::load_from_fileobj(&mut filething);

        assert!(
            result.is_err(),
            "Should reject oversized string length before allocation"
        );
    }

    #[test]
    fn test_rejects_huge_value_count() {
        let data = build_basictags_with_value_count(0xFFFF_FFFF);
        let cursor = Cursor::new(data);
        let mut filething = AnyFileThing::from(cursor);

        let result = BasicTags::load_from_fileobj(&mut filething);

        assert!(
            result.is_err(),
            "Should reject absurd value count before allocating"
        );
    }

    #[test]
    fn test_valid_basictags_still_loads() {
        let mut data = Vec::new();
        data.extend_from_slice(b"ADXBTAGS");
        data.extend_from_slice(&2u32.to_le_bytes());

        let key = b"artist";
        data.extend_from_slice(&(key.len() as u32).to_le_bytes());
        data.extend_from_slice(key);
        data.extend_from_slice(&1u32.to_le_bytes());
        let val = b"Someone";
        data.extend_from_slice(&(val.len() as u32).to_le_bytes());
        data.extend_from_slice(val);

        let key2 = b"title";
        data.extend_from_slice(&(key2.len() as u32).to_le_bytes());
        data.extend_from_slice(key2);
        data.extend_from_slice(&1u32.to_le_bytes());
        let val2 = b"Song";
        data.extend_from_slice(&(val2.len() as u32).to_le_bytes());
        data.extend_from_slice(val2);

        let cursor = Cursor::new(data);
        let mut filething = AnyFileThing::from(cursor);

        let result = BasicTags::load_from_fileobj(&mut filething);
        assert!(result.is_ok(), "Valid BasicTags should load fine");

        let tags = result.unwrap();
        assert_eq!(tags.get("artist").map(|v| v[0].as_str()), Some("Someone"));
        assert_eq!(tags.get("title").map(|v| v[0].as_str()), Some("Song"));
    }
}
