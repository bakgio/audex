// WASM integration tests for the audex-wasm module.
//
// Run with: wasm-pack test --node
//        or: wasm-pack test --headless --chrome
//
// All test data is embedded at compile time via include_bytes! — no filesystem
// access is needed and no files are modified.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

/// Embed the test MP3 at compile time so tests work without filesystem access.
const MP3_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.mp3");

/// Embed a FLAC file for format-diversity coverage.
const FLAC_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.flac");

/// FLAC file without any existing tag container.
const FLAC_NO_TAGS: &[u8] = include_bytes!("../../tests/data/no-tags.flac");

/// TAK file with APEv2 tags (small fixture for APE-based format tests).
const TAK_BYTES: &[u8] = include_bytes!("../../tests/data/has-tags.tak");

/// M4A file with existing tags.
const M4A_BYTES: &[u8] = include_bytes!("../../tests/data/has-tags.m4a");

/// OGG Vorbis file with Vorbis Comments.
const OGG_BYTES: &[u8] = include_bytes!("../../tests/data/multipagecomment.ogg");

/// WMA/ASF file.
const WMA_BYTES: &[u8] = include_bytes!("../../tests/data/silence-1.wma");

/// AIFF file with ID3 tags.
const AIFF_BYTES: &[u8] = include_bytes!("../../tests/data/with-id3.aif");

/// WAV file with ID3v2.3 tags.
const WAV_BYTES: &[u8] = include_bytes!("../../tests/data/silence-2s-PCM-16000-08-ID3v23.wav");

/// DSF file with ID3 tags.
const DSF_BYTES: &[u8] = include_bytes!("../../tests/data/with-id3.dsf");

/// Opus file.
const OPUS_BYTES: &[u8] = include_bytes!("../../tests/data/example.opus");

// ---------------------------------------------------------------------------
// Initialization and error handling
// ---------------------------------------------------------------------------

/// Verify that the WASM module initialises without panicking.
#[wasm_bindgen_test]
fn module_initializes() {
    // The wasm_bindgen(start) function runs automatically.
    // If we reach this point, initialisation succeeded.
}

// ---------------------------------------------------------------------------
// detect_format
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn detect_format_mp3_header() {
    let result = audex_wasm::detect_format(&MP3_BYTES[..512], Some("test.mp3".to_string()));
    assert!(
        result.is_ok(),
        "detect_format should succeed for MP3 header"
    );
    assert_eq!(result.unwrap(), "MP3");
}

#[wasm_bindgen_test]
fn detect_format_flac_header() {
    let result = audex_wasm::detect_format(&FLAC_BYTES[..512], Some("test.flac".to_string()));
    assert!(
        result.is_ok(),
        "detect_format should succeed for FLAC header"
    );
    assert_eq!(result.unwrap(), "FLAC");
}

#[wasm_bindgen_test]
fn detect_format_garbage_returns_error() {
    let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x00, 0x00, 0x00];
    let result = audex_wasm::detect_format(&garbage, None);
    assert!(result.is_err(), "garbage bytes should not match any format");
}

#[wasm_bindgen_test]
fn detect_format_with_filename_hint() {
    // Even minimal data should match when a filename hint is provided
    let result = audex_wasm::detect_format(&MP3_BYTES[..128], Some("track.mp3".to_string()));
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// Tag reading: get(), keys(), containsKey(), tagsJson()
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn get_returns_multi_value_array() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("Artist One"));
    values.push(&JsValue::from_str("Artist Two"));
    file.set("ARTIST", values.into()).expect("set multi-value");

    let result = file.get("ARTIST").expect("get should succeed");
    let arr = js_sys::Array::from(&result);
    assert_eq!(arr.length(), 2, "should return both values");
    assert_eq!(arr.get(0).as_string().unwrap(), "Artist One");
    assert_eq!(arr.get(1).as_string().unwrap(), "Artist Two");
}

#[wasm_bindgen_test]
fn keys_returns_all_tag_keys() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    file.set_single("TITLE", "Test Title").expect("set title");
    file.set_single("ARTIST", "Test Artist")
        .expect("set artist");

    let result = file.keys().expect("keys should succeed");
    let arr = js_sys::Array::from(&result);
    let keys: Vec<String> = arr.iter().filter_map(|v| v.as_string()).collect();

    assert!(
        keys.iter().any(|k| k.eq_ignore_ascii_case("title")),
        "keys should contain TITLE"
    );
    assert!(
        keys.iter().any(|k| k.eq_ignore_ascii_case("artist")),
        "keys should contain ARTIST"
    );
}

#[wasm_bindgen_test]
fn contains_key_checks_presence() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    file.set_single("TITLE", "Exists").expect("set title");

    assert!(file.contains_key("TITLE"), "TITLE should be present");
    assert!(
        !file.contains_key("NONEXISTENT"),
        "absent key should return false"
    );
}

#[wasm_bindgen_test]
fn tags_json_returns_object() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    file.set_single("TITLE", "Json Test").expect("set title");

    let json = file.tags_json().expect("tags_json should succeed");
    let obj = js_sys::Object::from(json);
    let title_val =
        js_sys::Reflect::get(&obj, &JsValue::from_str("title")).expect("access title key");
    assert!(
        !title_val.is_undefined(),
        "tags_json should contain a title key"
    );
}

#[wasm_bindgen_test]
fn stream_info_json_returns_object() {
    let file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let json = file
        .stream_info_json()
        .expect("stream_info_json should succeed");
    let obj = js_sys::Object::from(json);
    let sample_rate =
        js_sys::Reflect::get(&obj, &JsValue::from_str("sample_rate")).expect("access sample_rate");
    assert!(
        !sample_rate.is_undefined(),
        "stream_info_json should contain sample_rate"
    );
}

#[wasm_bindgen_test]
fn add_tags_on_tagless_file() {
    let mut file = audex_wasm::AudioFile::new(FLAC_NO_TAGS, Some("test.flac".to_string()))
        .expect("load tagless FLAC");

    assert!(!file.has_tags(), "fixture should start without tags");

    file.add_tags().expect("add_tags should succeed");
    assert!(file.has_tags(), "has_tags should be true after add_tags");
}

#[wasm_bindgen_test]
fn add_tags_on_tagged_file_is_noop() {
    let mut file = audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string()))
        .expect("load FLAC with tags");

    // File already has tags -- add_tags should not error
    let result = file.add_tags();
    assert!(
        result.is_ok(),
        "add_tags on already-tagged file should not error: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// set() with JS array, updateFromJson()
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn set_with_array_multi_value() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("Genre One"));
    values.push(&JsValue::from_str("Genre Two"));
    file.set("GENRE", values.into()).expect("set array");

    let result = file.get("GENRE").expect("get genres");
    let arr = js_sys::Array::from(&result);
    assert_eq!(arr.length(), 2);
}

#[wasm_bindgen_test]
fn update_from_json_batch_update() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    let payload = js_sys::Object::new();
    let title_arr = js_sys::Array::new();
    title_arr.push(&JsValue::from_str("Batch Title"));
    let artist_arr = js_sys::Array::new();
    artist_arr.push(&JsValue::from_str("Batch Artist"));
    js_sys::Reflect::set(&payload, &JsValue::from_str("TITLE"), &title_arr.into()).unwrap();
    js_sys::Reflect::set(&payload, &JsValue::from_str("ARTIST"), &artist_arr.into()).unwrap();

    file.update_from_json(payload.into()).expect("batch update");

    assert_eq!(file.get_first("TITLE").as_deref(), Some("Batch Title"));
    assert_eq!(file.get_first("ARTIST").as_deref(), Some("Batch Artist"));
}

#[wasm_bindgen_test]
fn update_from_json_preserves_unmentioned_tags() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    file.set_single("TITLE", "Original Title")
        .expect("set title");
    file.set_single("ARTIST", "Original Artist")
        .expect("set artist");

    // Update only TITLE -- ARTIST should be preserved
    let payload = js_sys::Object::new();
    let title_arr = js_sys::Array::new();
    title_arr.push(&JsValue::from_str("Updated Title"));
    js_sys::Reflect::set(&payload, &JsValue::from_str("TITLE"), &title_arr.into()).unwrap();

    file.update_from_json(payload.into())
        .expect("partial update");

    assert_eq!(file.get_first("TITLE").as_deref(), Some("Updated Title"));
    assert_eq!(
        file.get_first("ARTIST").as_deref(),
        Some("Original Artist"),
        "unmentioned key should be preserved"
    );
}

// ---------------------------------------------------------------------------
// setTagWithEncoding
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn set_tag_with_encoding_latin1() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("Latin1 Title"));
    // encoding 0 = Latin-1
    file.set_tag_with_encoding("TIT2", values.into(), 0)
        .expect("set with Latin-1 encoding");

    assert_eq!(file.get_first("TIT2").as_deref(), Some("Latin1 Title"));
}

#[wasm_bindgen_test]
fn set_tag_with_encoding_utf8() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("UTF-8 タイトル"));
    // encoding 3 = UTF-8
    file.set_tag_with_encoding("TIT2", values.into(), 3)
        .expect("set with UTF-8 encoding");

    assert_eq!(file.get_first("TIT2").as_deref(), Some("UTF-8 タイトル"));
}

#[wasm_bindgen_test]
fn set_tag_with_encoding_ignored_on_non_id3() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("Encoding Ignored"));
    // FLAC uses Vorbis Comments -- encoding param should be silently ignored
    let result = file.set_tag_with_encoding("TITLE", values.into(), 3);
    assert!(
        result.is_ok(),
        "non-ID3 format should accept any encoding param"
    );

    assert_eq!(file.get_first("TITLE").as_deref(), Some("Encoding Ignored"));
}

// ---------------------------------------------------------------------------
// snapshot()
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn snapshot_returns_metadata_object() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");
    file.set_single("TIT2", "Snapshot Title")
        .expect("set title");

    let snap = file.snapshot().expect("snapshot should succeed");
    let obj = js_sys::Object::from(snap);

    let format =
        js_sys::Reflect::get(&obj, &JsValue::from_str("format")).expect("access format field");
    assert_eq!(format.as_string().unwrap(), "MP3");

    let stream_info =
        js_sys::Reflect::get(&obj, &JsValue::from_str("stream_info")).expect("access stream_info");
    assert!(stream_info.is_object(), "stream_info should be an object");

    let tags = js_sys::Reflect::get(&obj, &JsValue::from_str("tags")).expect("access tags");
    assert!(tags.is_object(), "tags should be an object");
}

#[wasm_bindgen_test]
fn get_ape_binary_tag_roundtrip() {
    let mut file =
        audex_wasm::AudioFile::new(TAK_BYTES, Some("test.tak".to_string())).expect("load TAK");

    let image_data: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    file.set_ape_cover_art(&image_data)
        .expect("set APE cover art");

    let retrieved = file
        .get_ape_binary_tag("Cover Art (Front)")
        .expect("binary tag should be present");
    assert!(
        retrieved.len() >= image_data.len(),
        "retrieved binary data should contain the cover art bytes"
    );
}

/// Loading invalid data should produce a JS Error, not a panic.
#[wasm_bindgen_test]
fn invalid_data_returns_error() {
    let garbage = vec![0u8; 64];
    let result = audex_wasm::AudioFile::new(&garbage, Some("test.mp3".to_string()));
    assert!(result.is_err(), "expected an error for invalid audio data");
}

/// An empty buffer should produce a clear error.
#[wasm_bindgen_test]
fn empty_buffer_returns_error() {
    let result = audex_wasm::AudioFile::new(&[], None);
    assert!(result.is_err(), "expected an error for empty buffer");
}

// ---------------------------------------------------------------------------
// Round-trip: load → set → save → reload → verify
// ---------------------------------------------------------------------------

/// Verify that tags written via set_single() survive a save/reload cycle.
#[wasm_bindgen_test]
fn round_trip_save_reload_preserves_tags() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    // Write a title tag
    file.set_single("TIT2", "Round Trip Title")
        .expect("set_single should succeed");

    // Save to get updated bytes, then reload from those bytes
    let saved = file.save().expect("save should succeed");
    let reloaded = audex_wasm::AudioFile::new(&saved, Some("test.mp3".to_string()))
        .expect("should reload saved MP3");

    let title = reloaded.get_first("TIT2");
    assert_eq!(
        title.as_deref(),
        Some("Round Trip Title"),
        "title should survive save/reload"
    );
}

/// Verify round-trip works for FLAC as well (Vorbis Comment tags).
#[wasm_bindgen_test]
fn round_trip_flac_save_reload() {
    let mut file = audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string()))
        .expect("should load FLAC");

    file.set_single("TITLE", "FLAC Round Trip")
        .expect("set TITLE on FLAC");

    let saved = file.save().expect("FLAC save should succeed");
    let reloaded = audex_wasm::AudioFile::new(&saved, Some("test.flac".to_string()))
        .expect("should reload saved FLAC");

    let title = reloaded.get_first("TITLE");
    assert_eq!(
        title.as_deref(),
        Some("FLAC Round Trip"),
        "FLAC title should survive save/reload"
    );
}

// ---------------------------------------------------------------------------
// clear()
// ---------------------------------------------------------------------------

/// Verify that clear() removes tags from the in-memory representation.
#[wasm_bindgen_test]
fn clear_removes_tags() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    // Set a tag so we have something to clear
    file.set_single("TIT2", "To Be Cleared")
        .expect("set should work");

    // Clear all tags — this updates the stored bytes internally
    file.clear().expect("clear should succeed");

    // Reload from the cleared bytes to verify tags are gone
    file.reload().expect("reload after clear");
    let title = file.get_first("TIT2");
    assert!(
        title.is_none(),
        "title should be gone after clear + reload, got: {:?}",
        title
    );
}

// ---------------------------------------------------------------------------
// saveWithOptions — valid and invalid parameters
// ---------------------------------------------------------------------------

/// saveWithOptions with valid ID3v2.3 settings should succeed.
#[wasm_bindgen_test]
fn save_with_options_valid_v23() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    // v1=0 (REMOVE), v2_version=3, separator="/", no frame conversion
    let result = file.save_with_options(0, 3, "/", false);
    assert!(
        result.is_ok(),
        "saveWithOptions with v2.3 should succeed: {:?}",
        result.err().map(|e| format!("{:?}", e))
    );
}

/// saveWithOptions with valid ID3v2.4 settings should succeed.
#[wasm_bindgen_test]
fn save_with_options_valid_v24() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    // v1=0 (REMOVE), v2_version=4, separator=null-byte, no conversion
    let result = file.save_with_options(0, 4, "\0", false);
    assert!(
        result.is_ok(),
        "saveWithOptions with v2.4 should succeed: {:?}",
        result.err().map(|e| format!("{:?}", e))
    );
}

/// saveWithOptions with an invalid version (e.g. 2) should return an error.
#[wasm_bindgen_test]
fn save_with_options_rejects_invalid_version() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let result = file.save_with_options(0, 2, "/", false);
    assert!(
        result.is_err(),
        "saveWithOptions with v2_version=2 should fail"
    );
}

/// saveWithOptions round-trip: save with v2.3, reload, verify tags.
#[wasm_bindgen_test]
fn save_with_options_round_trip() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    file.set_single("TIT2", "Options Round Trip")
        .expect("set title");

    let saved = file
        .save_with_options(0, 3, "/", false)
        .expect("save with v2.3 options");

    let reloaded = audex_wasm::AudioFile::new(&saved, Some("test.mp3".to_string()))
        .expect("reload after saveWithOptions");

    let title = reloaded.get_first("TIT2");
    assert_eq!(
        title.as_deref(),
        Some("Options Round Trip"),
        "title should survive saveWithOptions round-trip"
    );
}

// ---------------------------------------------------------------------------
// setCoverArt
// ---------------------------------------------------------------------------

/// setCoverArt should succeed on an MP3 that has tags.
#[wasm_bindgen_test]
fn set_cover_art_on_mp3_succeeds() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    // Minimal JPEG-like bytes (the APIC frame stores raw bytes, no validation)
    let tiny_image: Vec<u8> = vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9,
    ];

    let result = file.set_cover_art(&tiny_image, "image/jpeg");
    assert!(
        result.is_ok(),
        "setCoverArt should succeed on MP3 with tags: {:?}",
        result.err().map(|e| format!("{:?}", e))
    );

    // Save and reload — the cover art bytes should survive
    let saved = file.save().expect("save with cover art");
    let reloaded = audex_wasm::AudioFile::new(&saved, Some("test.mp3".to_string()))
        .expect("reload with cover art");

    let keys = reloaded.keys().expect("serialize keys");
    let keys = js_sys::Array::from(&keys);
    let has_apic = keys.iter().any(|value| {
        value
            .as_string()
            .map(|key| key.starts_with("APIC:"))
            .unwrap_or(false)
    });
    assert!(has_apic, "saved file should retain an APIC frame");
}

/// setReplayGain on a file saved as v2.3 then reloaded should succeed,
/// because the parser always upgrades to v2.4 on load.
#[wasm_bindgen_test]
fn set_replay_gain_on_v23_saved_mp3_succeeds() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");
    let saved_v23 = file
        .save_with_options(0, 3, "/", false)
        .expect("save MP3 as ID3v2.3");

    let mut reloaded = audex_wasm::AudioFile::new(&saved_v23, Some("test.mp3".to_string()))
        .expect("reload saved MP3");

    // The parser auto-upgrades to v2.4 on load, so RVA2 is accepted
    let result = reloaded.set_replay_gain("TRACK", -6.5, 0.95);
    assert!(
        result.is_ok(),
        "setReplayGain should succeed after v2.3 save + reload (auto-upgraded to v2.4): {:?}",
        result.err().map(|e| format!("{:?}", e))
    );
}

/// setTagWithEncoding on a file saved as v2.3 then reloaded should succeed,
/// because the parser always upgrades to v2.4 on load.
#[wasm_bindgen_test]
fn set_tag_with_encoding_on_v23_saved_mp3_succeeds() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");
    let saved_v23 = file
        .save_with_options(0, 3, "/", false)
        .expect("save MP3 as ID3v2.3");

    let mut reloaded = audex_wasm::AudioFile::new(&saved_v23, Some("test.mp3".to_string()))
        .expect("reload saved MP3");
    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("2024"));

    // The parser auto-upgrades to v2.4 on load, so DATE/TDRC is accepted
    let result = reloaded.set_tag_with_encoding("DATE", values.into(), 3);
    assert!(
        result.is_ok(),
        "setTagWithEncoding(DATE) should succeed after v2.3 save + reload (auto-upgraded to v2.4): {:?}",
        result.err().map(|e| format!("{:?}", e))
    );
}

// ---------------------------------------------------------------------------
// Tag reading and key enumeration
// ---------------------------------------------------------------------------

/// has_tags should return true when tags are present.
#[wasm_bindgen_test]
fn has_tags_returns_true_when_present() {
    let file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    assert!(file.has_tags(), "MP3 with ID3 header should have tags");
}

/// Tags set via set_single should be retrievable via get_first.
#[wasm_bindgen_test]
fn set_and_get_tag_value() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    file.set_single("TPE1", "Test Artist").expect("set artist");

    let artist = file.get_first("TPE1");
    assert_eq!(
        artist.as_deref(),
        Some("Test Artist"),
        "should read back the value that was set"
    );
}

/// set_single should reject oversized strings before mutating tags.
#[wasm_bindgen_test]
fn set_single_rejects_oversized_total_string_bytes() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let oversized = "X".repeat(50 * 1024 * 1024 + 1);
    let err = file
        .set_single("TIT2", &oversized)
        .expect_err("oversized payload should be rejected");
    let message = format!("{:?}", err);
    assert!(
        message.contains("exceeds maximum"),
        "unexpected error: {}",
        message
    );
}

/// format_name should report the correct format.
#[wasm_bindgen_test]
fn format_name_reports_mp3() {
    let file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    assert_eq!(file.format_name(), "MP3");
}

/// format_name should report FLAC for FLAC files.
#[wasm_bindgen_test]
fn format_name_reports_flac() {
    let file = audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string()))
        .expect("should load FLAC");

    assert_eq!(file.format_name(), "FLAC");
}

// ---------------------------------------------------------------------------
// remove() individual key
// ---------------------------------------------------------------------------

/// Removing a specific key should delete only that tag.
#[wasm_bindgen_test]
fn remove_key_deletes_single_tag() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    file.set_single("TIT2", "Remove Me").expect("set title");
    assert!(file.get_first("TIT2").is_some(), "title should exist");

    file.remove("TIT2").expect("remove should succeed");

    assert!(
        file.get_first("TIT2").is_none(),
        "title should be gone after remove"
    );
}

// ---------------------------------------------------------------------------
// reload() from original bytes
// ---------------------------------------------------------------------------

/// reload() should discard unsaved changes and restore the original state.
#[wasm_bindgen_test]
fn reload_discards_unsaved_changes() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    // Read original title (if any) for comparison
    let original_title = file.get_first("TIT2");

    // Modify the title but don't save
    file.set_single("TIT2", "Unsaved Change")
        .expect("set should work");

    // Reload should restore the original state
    file.reload().expect("reload should succeed");

    let after_reload = file.get_first("TIT2");
    assert_eq!(
        after_reload, original_title,
        "reload should restore original state"
    );
}

// ---------------------------------------------------------------------------
// stream_info
// ---------------------------------------------------------------------------

/// stream_info should return valid metadata for a loaded file.
#[wasm_bindgen_test]
fn stream_info_returns_valid_data() {
    let file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let info = file.stream_info();
    // The test MP3 is 44.1kHz — verify we get a sane sample rate
    let rate = info.sample_rate().unwrap_or(0);
    assert!(rate > 0, "sample rate should be positive, got {}", rate);
}

// ---------------------------------------------------------------------------
// setReplayGain
// ---------------------------------------------------------------------------

/// setReplayGain should succeed on an MP3 that has tags.
#[wasm_bindgen_test]
fn set_replay_gain_on_mp3_succeeds() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let result = file.set_replay_gain("TRACK", -6.5, 0.95);
    assert!(
        result.is_ok(),
        "setReplayGain should succeed on MP3 with tags: {:?}",
        result.err().map(|e| format!("{:?}", e))
    );
}

/// setReplayGain on a format that doesn't support RVA2 should return an error.
#[wasm_bindgen_test]
fn set_replay_gain_on_flac_returns_error() {
    let mut file = audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string()))
        .expect("should load FLAC");

    let result = file.set_replay_gain("TRACK", -6.5, 0.95);
    assert!(
        result.is_err(),
        "setReplayGain should fail on FLAC (no RVA2 support)"
    );
}

// ---------------------------------------------------------------------------
// Format-specific cover art error handling
// ---------------------------------------------------------------------------

/// setMp4CoverArt on a non-MP4 file should return an error.
#[wasm_bindgen_test]
fn set_mp4_cover_art_on_mp3_returns_error() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let result = file.set_mp4_cover_art(&[0xFF, 0xD8], "image/jpeg");
    assert!(
        result.is_err(),
        "setMp4CoverArt should fail on non-MP4 format"
    );
}

/// setFlacCoverArt on a non-FLAC file should return an error.
#[wasm_bindgen_test]
fn set_flac_cover_art_on_mp3_returns_error() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let result = file.set_flac_cover_art(&[0xFF, 0xD8], "image/jpeg", 3, 300, 300, 24);
    assert!(
        result.is_err(),
        "setFlacCoverArt should fail on non-FLAC format"
    );
}

/// setVorbisCoverArt on a non-Ogg file should return an error.
#[wasm_bindgen_test]
fn set_vorbis_cover_art_on_mp3_returns_error() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let result = file.set_vorbis_cover_art(&[0xFF, 0xD8], "image/jpeg", 3, 300, 300, 24);
    assert!(
        result.is_err(),
        "setVorbisCoverArt should fail on non-Ogg format"
    );
}

// ---------------------------------------------------------------------------
// Oversized payload rejection
// ---------------------------------------------------------------------------

/// set should reject oversized value arrays before mutating tags.
#[wasm_bindgen_test]
fn set_rejects_oversized_total_string_bytes() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let values = js_sys::Array::new();
    let oversized = "X".repeat(50 * 1024 * 1024 + 1);
    values.push(&JsValue::from_str(&oversized));

    let err = file
        .set("TIT2", values.into())
        .expect_err("oversized payload should be rejected");
    let message = format!("{:?}", err);
    assert!(
        message.contains("exceeds maximum"),
        "unexpected error: {}",
        message
    );
}

/// setTagWithEncoding should reject oversized value arrays before mutating tags.
#[wasm_bindgen_test]
fn set_tag_with_encoding_rejects_oversized_total_string_bytes() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let values = js_sys::Array::new();
    let oversized = "X".repeat(50 * 1024 * 1024 + 1);
    values.push(&JsValue::from_str(&oversized));

    let err = file
        .set_tag_with_encoding("TIT2", values.into(), 3)
        .expect_err("oversized payload should be rejected");
    let message = format!("{:?}", err);
    assert!(
        message.contains("exceeds maximum"),
        "unexpected error: {}",
        message
    );
}

/// updateFromJson should reject objects with too many entries.
#[wasm_bindgen_test]
fn update_from_json_rejects_too_many_entries() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let payload = js_sys::Object::new();
    for i in 0..10_001u32 {
        let key = format!("KEY{i}");
        let values = js_sys::Array::new();
        values.push(&JsValue::from_str("value"));
        js_sys::Reflect::set(&payload, &JsValue::from_str(&key), &values.into())
            .expect("set object property");
    }

    let err = file
        .update_from_json(payload.into())
        .expect_err("oversized entry count should be rejected");
    let message = format!("{:?}", err);
    assert!(
        message.contains("maximum allowed is 10000"),
        "unexpected error: {}",
        message
    );
}

/// updateFromJson should reject payloads that exceed the total string budget.
#[wasm_bindgen_test]
fn update_from_json_rejects_oversized_total_string_bytes() {
    let mut file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let payload = js_sys::Object::new();
    let values = js_sys::Array::new();
    let oversized = "X".repeat(50 * 1024 * 1024 + 1);
    values.push(&JsValue::from_str(&oversized));
    js_sys::Reflect::set(&payload, &JsValue::from_str("TIT2"), &values.into())
        .expect("set object property");

    let err = file
        .update_from_json(payload.into())
        .expect_err("oversized byte budget should be rejected");
    let message = format!("{:?}", err);
    assert!(
        message.contains("exceeds maximum"),
        "unexpected error: {}",
        message
    );
}

/// diffAgainstSnapshot should reject arrays with too many entries.
#[wasm_bindgen_test]
fn diff_against_snapshot_rejects_too_many_entries() {
    let file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let snapshot = js_sys::Array::new();
    for i in 0..10_001u32 {
        let pair = js_sys::Array::new();
        pair.push(&JsValue::from_str(&format!("KEY{i}")));
        let values = js_sys::Array::new();
        values.push(&JsValue::from_str("value"));
        pair.push(&values.into());
        snapshot.push(&pair.into());
    }

    let err = file
        .diff_against_snapshot(snapshot.into())
        .err()
        .expect("oversized snapshot entry count should be rejected");
    let message = format!("{:?}", err);
    assert!(
        message.contains("maximum allowed is 10000"),
        "unexpected error: {}",
        message
    );
}

/// diffAgainstSnapshot should reject payloads that exceed the total string budget.
#[wasm_bindgen_test]
fn diff_against_snapshot_rejects_oversized_total_string_bytes() {
    let file = audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string()))
        .expect("should load MP3");

    let snapshot = js_sys::Array::new();
    let pair = js_sys::Array::new();
    pair.push(&JsValue::from_str("TIT2"));
    let values = js_sys::Array::new();
    let oversized = "X".repeat(50 * 1024 * 1024 + 1);
    values.push(&JsValue::from_str(&oversized));
    pair.push(&values.into());
    snapshot.push(&pair.into());

    let err = file
        .diff_against_snapshot(snapshot.into())
        .err()
        .expect("oversized snapshot byte budget should be rejected");
    let message = format!("{:?}", err);
    assert!(
        message.contains("exceeds maximum"),
        "unexpected error: {}",
        message
    );
}

// ---------------------------------------------------------------------------
// Diff and comparison API
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn diff_tags_identical_files() {
    let a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    let diff = a.diff_tags(&b);
    assert!(
        diff.is_identical(),
        "same bytes should produce identical diff"
    );
    assert_eq!(diff.diff_count(), 0);
}

#[wasm_bindgen_test]
fn diff_tags_detects_change() {
    let mut a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    a.set_single("TIT2", "Different Title")
        .expect("set title on A");

    let diff = a.diff_tags(&b);
    assert!(!diff.is_identical());
    assert!(diff.diff_count() >= 1);
}

#[wasm_bindgen_test]
fn diff_tags_with_options_case_insensitive() {
    let mut a = audex_wasm::AudioFile::new(FLAC_BYTES, Some("a.flac".to_string())).expect("load A");
    let mut b = audex_wasm::AudioFile::new(FLAC_BYTES, Some("b.flac".to_string())).expect("load B");

    a.set_single("title", "Same Value").expect("set on A");
    b.set_single("TITLE", "Same Value").expect("set on B");

    let diff = a.diff_tags_with_options(&b, None, Some(true), None, None, None, None);
    let keys = diff.differing_keys();
    let title_differs = keys.iter().any(|k| k.eq_ignore_ascii_case("title"));
    assert!(
        !title_differs,
        "case-insensitive diff should treat title/TITLE as same key"
    );
}

#[wasm_bindgen_test]
fn diff_tags_with_options_include_unchanged() {
    let a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    let diff = a.diff_tags_with_options(&b, None, None, None, Some(true), None, None);
    let full_output = diff.pprint_full();

    // On identical files with tags, pprint_full shows unchanged entries
    let tag_count = a
        .keys()
        .map(|k| js_sys::Array::from(&k).length())
        .unwrap_or(0);
    if tag_count > 0 {
        assert!(
            !full_output.is_empty(),
            "pprint_full should include unchanged fields"
        );
    }
}

#[wasm_bindgen_test]
fn diff_tags_normalized_cross_format() {
    let mp3 =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");
    let flac =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    let diff = mp3.diff_tags_normalized(&flac, None, None, None);
    let _summary = diff.summary();
}

#[wasm_bindgen_test]
fn snapshot_tags_captures_state() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    file.set_single("TITLE", "Snapshot State")
        .expect("set title");
    let snapshot = file.snapshot_tags().expect("snapshot_tags should succeed");

    file.set_single("TITLE", "Changed After Snapshot")
        .expect("change title");

    let arr = js_sys::Array::from(&snapshot);
    assert!(
        arr.length() > 0,
        "snapshot should contain at least one entry"
    );
}

#[wasm_bindgen_test]
fn diff_against_snapshot_detects_change() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    file.set_single("TITLE", "Before").expect("set title");
    let snapshot = file.snapshot_tags().expect("capture snapshot");

    file.set_single("TITLE", "After").expect("change title");

    let diff = file
        .diff_against_snapshot(snapshot)
        .expect("diff_against_snapshot should succeed");

    assert!(!diff.is_identical(), "title change should be detected");
    assert!(diff.diff_count() >= 1);
}

#[wasm_bindgen_test]
fn wasm_tag_diff_summary_format() {
    let mut a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    a.set_single("TIT2", "New Title").expect("set title");
    let diff = a.diff_tags(&b);
    let summary = diff.summary();
    assert!(!summary.is_empty(), "summary should not be empty");
}

#[wasm_bindgen_test]
fn wasm_tag_diff_pprint_vs_pprint_full() {
    let a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    let diff = a.diff_tags_with_options(&b, None, None, None, Some(true), None, None);
    let short = diff.pprint();
    let full = diff.pprint_full();

    assert!(
        full.len() >= short.len(),
        "pprint_full ({}) should be >= pprint ({})",
        full.len(),
        short.len()
    );
}

#[wasm_bindgen_test]
fn wasm_tag_diff_filter_keys() {
    let mut a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    a.set_single("TIT2", "New Title").expect("set title");
    a.set_single("TPE1", "New Artist").expect("set artist");

    let diff = a.diff_tags(&b);
    let filtered = diff.filter_keys(vec!["TIT2".to_string()]);

    let keys = filtered.differing_keys();
    assert!(
        keys.iter().all(|k| k == "TIT2"),
        "filtered diff should only contain TIT2, got: {:?}",
        keys
    );
}

#[wasm_bindgen_test]
fn wasm_tag_diff_differing_keys() {
    let mut a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    a.set_single("TIT2", "Different Title").expect("set title");

    let diff = a.diff_tags(&b);
    let keys = diff.differing_keys();
    assert!(
        keys.iter().any(|k| k == "TIT2"),
        "TIT2 should be in differing keys, got: {:?}",
        keys
    );
}

#[wasm_bindgen_test]
fn wasm_tag_diff_to_json() {
    let a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    let diff = a.diff_tags(&b);
    let json = diff.to_json().expect("to_json should succeed");
    assert!(json.is_object(), "to_json should return an object");
}

// ---------------------------------------------------------------------------
// Import / conversion API
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn import_tags_from_mp3_to_flac() {
    let mut source = audex_wasm::AudioFile::new(MP3_BYTES, Some("src.mp3".to_string()))
        .expect("load source MP3");
    source
        .set_single("TIT2", "Imported Title")
        .expect("set title");

    let mut dest = audex_wasm::AudioFile::new(FLAC_BYTES, Some("dst.flac".to_string()))
        .expect("load dest FLAC");

    let report = dest
        .import_tags_from(&source)
        .expect("import should succeed");
    assert!(report.is_object(), "report should be an object");

    // Verify the title was transferred (mapped from ID3 TIT2 -> Vorbis TITLE)
    let title = dest.get_first("TITLE");
    assert!(
        title.is_some(),
        "TITLE should be present after import from MP3"
    );
}

#[wasm_bindgen_test]
fn import_tags_from_returns_report() {
    let mut source = audex_wasm::AudioFile::new(MP3_BYTES, Some("src.mp3".to_string()))
        .expect("load source MP3");
    source.set_single("TIT2", "Report Test").expect("set title");

    let mut dest = audex_wasm::AudioFile::new(FLAC_NO_TAGS, Some("dst.flac".to_string()))
        .expect("load dest FLAC");

    let report_js = dest.import_tags_from(&source).expect("import");
    let obj = js_sys::Object::from(report_js);

    // The report should have a "transferred" field
    let transferred = js_sys::Reflect::get(&obj, &JsValue::from_str("transferred"));
    assert!(
        transferred.is_ok(),
        "report should contain a transferred field"
    );
}

#[wasm_bindgen_test]
fn import_tags_with_options_include_fields() {
    let mut source =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("src.mp3".to_string())).expect("load source");
    source
        .set_single("TIT2", "Include Title")
        .expect("set title");
    source
        .set_single("TPE1", "Include Artist")
        .expect("set artist");

    let mut dest =
        audex_wasm::AudioFile::new(FLAC_NO_TAGS, Some("dst.flac".to_string())).expect("load dest");

    // Only include Title
    dest.import_tags_from_with_options(
        &source,
        Some(vec!["Title".to_string()]),
        None,
        None,
        None,
        None,
    )
    .expect("import with include filter");

    assert!(
        dest.get_first("TITLE").is_some(),
        "Title should be transferred"
    );
    assert!(
        dest.get_first("ARTIST").is_none(),
        "Artist should NOT be transferred when not in include list"
    );
}

#[wasm_bindgen_test]
fn import_tags_with_options_exclude_fields() {
    let mut source =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("src.mp3".to_string())).expect("load source");
    source
        .set_single("TIT2", "Exclude Title")
        .expect("set title");
    source
        .set_single("TPE1", "Exclude Artist")
        .expect("set artist");

    let mut dest =
        audex_wasm::AudioFile::new(FLAC_NO_TAGS, Some("dst.flac".to_string())).expect("load dest");

    // Exclude Title
    dest.import_tags_from_with_options(
        &source,
        None,
        Some(vec!["Title".to_string()]),
        None,
        None,
        None,
    )
    .expect("import with exclude filter");

    assert!(
        dest.get_first("TITLE").is_none(),
        "Title should be excluded"
    );
}

#[wasm_bindgen_test]
fn import_tags_with_options_overwrite_false() {
    let mut source =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("src.mp3".to_string())).expect("load source");
    source
        .set_single("TIT2", "Source Title")
        .expect("set source title");

    let mut dest =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("dst.flac".to_string())).expect("load dest");
    dest.set_single("TITLE", "Existing Title")
        .expect("set dest title");

    // overwrite=false should preserve the existing title
    dest.import_tags_from_with_options(&source, None, None, None, Some(false), None)
        .expect("import with overwrite=false");

    assert_eq!(
        dest.get_first("TITLE").as_deref(),
        Some("Existing Title"),
        "overwrite=false should preserve existing values"
    );
}

#[wasm_bindgen_test]
fn import_tags_with_options_clear_destination() {
    let mut source =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("src.mp3".to_string())).expect("load source");
    source
        .set_single("TIT2", "New Title")
        .expect("set source title");

    let mut dest =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("dst.flac".to_string())).expect("load dest");
    // Set a custom tag that the source does NOT have
    dest.set_single("CUSTOMDESTONLY", "Will Be Cleared")
        .expect("set custom tag");

    dest.import_tags_from_with_options(&source, None, None, None, None, Some(true))
        .expect("import with clear");

    assert!(
        dest.get_first("CUSTOMDESTONLY").is_none(),
        "destination-only tag should be removed by clear_destination=true"
    );
}

// ---------------------------------------------------------------------------
// Format-specific cover art
// ---------------------------------------------------------------------------

/// Minimal valid JPEG header for cover art tests.
fn tiny_jpeg() -> Vec<u8> {
    vec![
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
        0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9,
    ]
}

#[wasm_bindgen_test]
fn set_ape_cover_art_on_tak() {
    let mut file =
        audex_wasm::AudioFile::new(TAK_BYTES, Some("test.tak".to_string())).expect("load TAK");

    let result = file.set_ape_cover_art(&tiny_jpeg());
    assert!(
        result.is_ok(),
        "setApeCoverArt should succeed on TAK: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn set_asf_cover_art_on_wma() {
    let mut file =
        audex_wasm::AudioFile::new(WMA_BYTES, Some("test.wma".to_string())).expect("load WMA");

    // ASF auto-detects MIME from magic bytes
    let result = file.set_asf_cover_art(&tiny_jpeg());
    assert!(
        result.is_ok(),
        "setAsfCoverArt should succeed on WMA: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn set_mp4_cover_art_jpeg() {
    let mut file =
        audex_wasm::AudioFile::new(M4A_BYTES, Some("test.m4a".to_string())).expect("load M4A");

    let result = file.set_mp4_cover_art(&tiny_jpeg(), "image/jpeg");
    assert!(
        result.is_ok(),
        "setMp4CoverArt with image/jpeg should succeed: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn set_mp4_cover_art_rejects_invalid_mime() {
    let mut file =
        audex_wasm::AudioFile::new(M4A_BYTES, Some("test.m4a".to_string())).expect("load M4A");

    let result = file.set_mp4_cover_art(&tiny_jpeg(), "image/gif");
    assert!(
        result.is_err(),
        "setMp4CoverArt should reject image/gif (only jpeg/png allowed)"
    );
}

#[wasm_bindgen_test]
fn get_mp4_cover_art_roundtrip() {
    let mut file =
        audex_wasm::AudioFile::new(M4A_BYTES, Some("test.m4a".to_string())).expect("load M4A");

    let img = tiny_jpeg();
    file.set_mp4_cover_art(&img, "image/jpeg")
        .expect("set cover art");

    let saved = file.save().expect("save");
    let reloaded =
        audex_wasm::AudioFile::new(&saved, Some("test.m4a".to_string())).expect("reload");

    let retrieved = reloaded.get_mp4_cover_art();
    assert!(retrieved.is_some(), "cover art should survive save/reload");
    assert_eq!(retrieved.unwrap(), img);
}

#[wasm_bindgen_test]
fn set_vorbis_cover_art_on_ogg() {
    let mut file =
        audex_wasm::AudioFile::new(OGG_BYTES, Some("test.ogg".to_string())).expect("load OGG");

    let result = file.set_vorbis_cover_art(&tiny_jpeg(), "image/jpeg", 3, 15, 15, 24);
    assert!(
        result.is_ok(),
        "setVorbisCoverArt should succeed on OGG: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn set_flac_cover_art_on_flac() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    let result = file.set_flac_cover_art(&tiny_jpeg(), "image/jpeg", 3, 15, 15, 24);
    assert!(
        result.is_ok(),
        "setFlacCoverArt should succeed on FLAC: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn set_cover_art_on_aiff() {
    let mut file =
        audex_wasm::AudioFile::new(AIFF_BYTES, Some("test.aif".to_string())).expect("load AIFF");

    let result = file.set_cover_art(&tiny_jpeg(), "image/jpeg");
    assert!(
        result.is_ok(),
        "setCoverArt should succeed on AIFF (ID3-based): {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// Format coverage: round-trip and format_name for formats beyond MP3/FLAC
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn mp4_roundtrip() {
    let mut file =
        audex_wasm::AudioFile::new(M4A_BYTES, Some("test.m4a".to_string())).expect("load M4A");

    file.set_single("\u{a9}nam", "MP4 Round Trip")
        .expect("set title");
    let saved = file.save().expect("save");

    let reloaded =
        audex_wasm::AudioFile::new(&saved, Some("test.m4a".to_string())).expect("reload");
    assert_eq!(
        reloaded.get_first("\u{a9}nam").as_deref(),
        Some("MP4 Round Trip")
    );
    assert_eq!(reloaded.format_name(), "MP4");
}

#[wasm_bindgen_test]
fn ogg_vorbis_roundtrip() {
    let mut file =
        audex_wasm::AudioFile::new(OGG_BYTES, Some("test.ogg".to_string())).expect("load OGG");

    file.set_single("title", "OGG Round Trip")
        .expect("set title");
    let saved = file.save().expect("save");

    let reloaded =
        audex_wasm::AudioFile::new(&saved, Some("test.ogg".to_string())).expect("reload");
    assert_eq!(
        reloaded.get_first("title").as_deref(),
        Some("OGG Round Trip")
    );
    assert!(
        reloaded.format_name().contains("Ogg") || reloaded.format_name().contains("Vorbis"),
        "expected OGG/Vorbis format, got: {}",
        reloaded.format_name()
    );
}

#[wasm_bindgen_test]
fn aiff_roundtrip() {
    let mut file =
        audex_wasm::AudioFile::new(AIFF_BYTES, Some("test.aif".to_string())).expect("load AIFF");

    file.set_single("TIT2", "AIFF Round Trip")
        .expect("set title");
    let saved = file.save().expect("save");

    let reloaded =
        audex_wasm::AudioFile::new(&saved, Some("test.aif".to_string())).expect("reload");
    assert_eq!(
        reloaded.get_first("TIT2").as_deref(),
        Some("AIFF Round Trip")
    );
    assert_eq!(reloaded.format_name(), "AIFF");
}

#[wasm_bindgen_test]
fn wave_roundtrip() {
    let mut file =
        audex_wasm::AudioFile::new(WAV_BYTES, Some("test.wav".to_string())).expect("load WAV");

    file.set_single("TIT2", "WAV Round Trip")
        .expect("set title");
    let saved = file.save().expect("save");

    let reloaded =
        audex_wasm::AudioFile::new(&saved, Some("test.wav".to_string())).expect("reload");
    assert_eq!(
        reloaded.get_first("TIT2").as_deref(),
        Some("WAV Round Trip")
    );
    assert_eq!(reloaded.format_name(), "WAVE");
}

#[wasm_bindgen_test]
fn asf_roundtrip() {
    let mut file =
        audex_wasm::AudioFile::new(WMA_BYTES, Some("test.wma".to_string())).expect("load WMA");

    file.set_single("Title", "ASF Round Trip")
        .expect("set title");
    let saved = file.save().expect("save");

    let reloaded =
        audex_wasm::AudioFile::new(&saved, Some("test.wma".to_string())).expect("reload");
    assert_eq!(
        reloaded.get_first("Title").as_deref(),
        Some("ASF Round Trip")
    );
    assert_eq!(reloaded.format_name(), "ASF");
}

#[wasm_bindgen_test]
fn opus_roundtrip() {
    let mut file =
        audex_wasm::AudioFile::new(OPUS_BYTES, Some("test.opus".to_string())).expect("load Opus");

    file.set_single("title", "Opus Round Trip")
        .expect("set title");
    let saved = file.save().expect("save");

    let reloaded =
        audex_wasm::AudioFile::new(&saved, Some("test.opus".to_string())).expect("reload");
    assert_eq!(
        reloaded.get_first("title").as_deref(),
        Some("Opus Round Trip")
    );
}

#[wasm_bindgen_test]
fn dsf_roundtrip() {
    let mut file =
        audex_wasm::AudioFile::new(DSF_BYTES, Some("test.dsf".to_string())).expect("load DSF");

    file.set_single("TIT2", "DSF Round Trip")
        .expect("set title");
    let saved = file.save().expect("save");

    let reloaded =
        audex_wasm::AudioFile::new(&saved, Some("test.dsf".to_string())).expect("reload");
    assert_eq!(
        reloaded.get_first("TIT2").as_deref(),
        Some("DSF Round Trip")
    );
    assert_eq!(reloaded.format_name(), "DSF");
}

#[wasm_bindgen_test]
fn format_name_all_formats() {
    assert_eq!(
        audex_wasm::AudioFile::new(M4A_BYTES, Some("t.m4a".into()))
            .unwrap()
            .format_name(),
        "MP4"
    );
    assert_eq!(
        audex_wasm::AudioFile::new(WMA_BYTES, Some("t.wma".into()))
            .unwrap()
            .format_name(),
        "ASF"
    );
    assert_eq!(
        audex_wasm::AudioFile::new(AIFF_BYTES, Some("t.aif".into()))
            .unwrap()
            .format_name(),
        "AIFF"
    );
    assert_eq!(
        audex_wasm::AudioFile::new(WAV_BYTES, Some("t.wav".into()))
            .unwrap()
            .format_name(),
        "WAVE"
    );
    assert_eq!(
        audex_wasm::AudioFile::new(DSF_BYTES, Some("t.dsf".into()))
            .unwrap()
            .format_name(),
        "DSF"
    );
    assert_eq!(
        audex_wasm::AudioFile::new(TAK_BYTES, Some("t.tak".into()))
            .unwrap()
            .format_name(),
        "TAK"
    );
}

// ---------------------------------------------------------------------------
// Input validation edge cases
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn set_cover_art_rejects_invalid_mime_prefix() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let result = file.set_cover_art(&[0xFF, 0xD8], "audio/mpeg");
    assert!(
        result.is_err(),
        "setCoverArt should reject MIME type without 'image/' prefix"
    );
}

#[wasm_bindgen_test]
fn set_replay_gain_rejects_nan() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let result = file.set_replay_gain("TRACK", f32::NAN, 0.95);
    assert!(result.is_err(), "setReplayGain should reject NaN gain");
}

#[wasm_bindgen_test]
fn set_replay_gain_rejects_infinity() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let result = file.set_replay_gain("TRACK", -6.5, f32::INFINITY);
    assert!(result.is_err(), "setReplayGain should reject Infinity peak");
}

#[wasm_bindgen_test]
fn save_with_options_rejected_for_non_mp3() {
    let mut file =
        audex_wasm::AudioFile::new(AIFF_BYTES, Some("test.aif".to_string())).expect("load AIFF");

    // Custom options (v2_version=4) should be rejected for non-MP3 formats
    let result = file.save_with_options(0, 4, "/", false);
    assert!(
        result.is_err(),
        "saveWithOptions with custom options should be rejected for AIFF"
    );
}

#[wasm_bindgen_test]
fn save_with_options_rejects_multibyte_separator() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let result = file.save_with_options(0, 3, "é", false);
    assert!(
        result.is_err(),
        "saveWithOptions should reject multi-byte UTF-8 separator"
    );
}
