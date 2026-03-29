use audex::FileType;
/// Tests for EasyID3 dynamic key registration
/// - register_text_key() allows runtime registration of new keys
/// - register_txxx_key() allows runtime registration of TXXX keys
/// - Registered keys can be set and saved without errors
use audex::easyid3::EasyID3;

#[test]
fn test_register_text_key_barcode() {
    // Test that registering a custom text key works
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Register a custom "barcode" key mapped to TXXX:BARCODE
    assert!(tags.register_txxx_key("barcode", "BARCODE").is_ok());

    // Set the barcode value
    assert!(tags.set("barcode", &["1234567890123".to_string()]).is_ok());

    // Save should work without errors
    assert!(tags.save().is_ok());
}

#[test]
fn test_register_multiple_custom_keys() {
    // Test registering multiple custom keys
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Register multiple custom keys
    assert!(tags.register_txxx_key("catalog", "CATALOG").is_ok());
    assert!(tags.register_txxx_key("isrc", "ISRC").is_ok());

    // Set values
    assert!(tags.set("catalog", &["CAT-001".to_string()]).is_ok());
    assert!(tags.set("isrc", &["USPR37300012".to_string()]).is_ok());

    // Save should work without errors
    assert!(tags.save().is_ok());
}

#[test]
fn test_register_text_key_standard_frame() {
    // Test registering a standard ID3 frame as a custom key
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Register a custom key mapped to TPUB (Publisher)
    assert!(tags.register_text_key("publisher", "TPUB").is_ok());

    // Set the publisher
    assert!(
        tags.set("publisher", &["Example Records".to_string()])
            .is_ok()
    );

    // Save should work without errors
    assert!(tags.save().is_ok());
}

#[test]
fn test_delete_registered_key() {
    // Test deleting a registered custom key
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Register and set a custom key
    tags.register_txxx_key("customfield", "CUSTOMFIELD")
        .unwrap();
    tags.set("customfield", &["test value".to_string()])
        .unwrap();
    tags.save().unwrap();

    // Reload
    let mut loaded_tags = EasyID3::load(&test_path).unwrap();

    // Delete the custom field - should work without errors
    let _ = loaded_tags.remove("customfield");

    // Saving after delete should work
    assert!(loaded_tags.save().is_ok());
}

#[test]
fn test_registration_persists_across_instances() {
    // Test that registration in one instance affects other instances
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path1 = temp_dir.path().join("test1.mp3");
    let test_path2 = temp_dir.path().join("test2.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path1, mp3_data).unwrap();
    std::fs::write(&test_path2, mp3_data).unwrap();

    // Register a key in first instance
    let mut tags1 = EasyID3::load(&test_path1).unwrap();
    tags1.register_txxx_key("globalkey", "GLOBALKEY").unwrap();

    // The key should now work in a second instance
    let mut tags2 = EasyID3::load(&test_path2).unwrap();
    assert!(
        tags2
            .set("globalkey", &["shared value".to_string()])
            .is_ok()
    );
    assert!(tags2.save().is_ok());
}

#[test]
fn test_register_text_key_rejects_builtin_override() {
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();
    let result = tags.register_text_key("title", "TXXX:EVIL");

    assert!(
        result.is_err(),
        "built-in EasyID3 keys must not be replaceable"
    );
}
