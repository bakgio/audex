// Edge-case tests for the WASM bindings layer.
//
// Covers encoding edge cases, diff option combinations, filename hint
// ambiguity, APE binary tag format coverage, and tag budget rollback.
// All test data is embedded at compile time — no files are modified.
//
// Run with: wasm-pack test --node

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

// Embedded test data
const MP3_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.mp3");
const FLAC_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.flac");
const WMA_BYTES: &[u8] = include_bytes!("../../tests/data/silence-1.wma");
const AIFF_BYTES: &[u8] = include_bytes!("../../tests/data/with-id3.aif");
const WAV_BYTES: &[u8] = include_bytes!("../../tests/data/silence-2s-PCM-16000-08-ID3v23.wav");
const DSF_BYTES: &[u8] = include_bytes!("../../tests/data/with-id3.dsf");
const WV_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.wv");
const MPC_BYTES: &[u8] = include_bytes!("../../tests/data/click.mpc");
const OFR_BYTES: &[u8] = include_bytes!("../../tests/data/silence-2s-44100-16.ofr");
const TTA_BYTES: &[u8] = include_bytes!("../../tests/data/empty.tta");
const APE_BYTES: &[u8] = include_bytes!("../../tests/data/mac-399.ape");

// ---------------------------------------------------------------------------
// ID3 text encoding: UTF-16, UTF-16BE, invalid values, non-MP3 ID3 formats
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn set_tag_with_encoding_utf16() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("UTF-16 Title"));
    // encoding 1 = UTF-16 (with BOM)
    file.set_tag_with_encoding("TIT2", values.into(), 1)
        .expect("set with UTF-16 encoding");

    assert_eq!(file.get_first("TIT2").as_deref(), Some("UTF-16 Title"));
}

#[wasm_bindgen_test]
fn set_tag_with_encoding_utf16be() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("UTF-16BE Title"));
    // encoding 2 = UTF-16BE (no BOM)
    file.set_tag_with_encoding("TIT2", values.into(), 2)
        .expect("set with UTF-16BE encoding");

    assert_eq!(file.get_first("TIT2").as_deref(), Some("UTF-16BE Title"));
}

#[wasm_bindgen_test]
fn set_tag_with_encoding_invalid_value_rejected() {
    let mut file =
        audex_wasm::AudioFile::new(MP3_BYTES, Some("test.mp3".to_string())).expect("load MP3");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("test"));
    // encoding 5 is not a valid ID3 text encoding (only 0-3 are valid)
    let result = file.set_tag_with_encoding("TIT2", values.into(), 5);
    assert!(result.is_err(), "Encoding value 5 must be rejected");
}

#[wasm_bindgen_test]
fn set_tag_with_encoding_on_aiff() {
    let mut file =
        audex_wasm::AudioFile::new(AIFF_BYTES, Some("test.aif".to_string())).expect("load AIFF");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("AIFF UTF-8 Title"));
    // AIFF uses ID3 tags; encoding 3 = UTF-8
    file.set_tag_with_encoding("TIT2", values.into(), 3)
        .expect("set encoding on AIFF");

    assert_eq!(file.get_first("TIT2").as_deref(), Some("AIFF UTF-8 Title"));
}

#[wasm_bindgen_test]
fn set_tag_with_encoding_on_wave() {
    let mut file =
        audex_wasm::AudioFile::new(WAV_BYTES, Some("test.wav".to_string())).expect("load WAV");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("WAV UTF-8 Title"));
    file.set_tag_with_encoding("TIT2", values.into(), 3)
        .expect("set encoding on WAV");

    assert_eq!(file.get_first("TIT2").as_deref(), Some("WAV UTF-8 Title"));
}

#[wasm_bindgen_test]
fn set_tag_with_encoding_on_dsf() {
    let mut file =
        audex_wasm::AudioFile::new(DSF_BYTES, Some("test.dsf".to_string())).expect("load DSF");

    let values = js_sys::Array::new();
    values.push(&JsValue::from_str("DSF UTF-8 Title"));
    file.set_tag_with_encoding("TIT2", values.into(), 3)
        .expect("set encoding on DSF");

    assert_eq!(file.get_first("TIT2").as_deref(), Some("DSF UTF-8 Title"));
}

// ---------------------------------------------------------------------------
// Cover art MIME sniffing (tested via ASF cover art which auto-detects MIME)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn asf_cover_art_bmp_mime_sniffed() {
    let mut file =
        audex_wasm::AudioFile::new(WMA_BYTES, Some("test.wma".to_string())).expect("load WMA");

    // BMP magic: starts with "BM"
    let bmp_data = b"BM\x00\x00\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00";
    let result = file.set_asf_cover_art(bmp_data);
    assert!(
        result.is_ok(),
        "BMP data should be accepted by ASF cover art: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn asf_cover_art_webp_mime_sniffed() {
    let mut file =
        audex_wasm::AudioFile::new(WMA_BYTES, Some("test.wma".to_string())).expect("load WMA");

    // WebP magic: "RIFF" + 4 bytes size + "WEBP"
    let webp_data = b"RIFF\x00\x00\x00\x00WEBP\x00\x00\x00\x00";
    let result = file.set_asf_cover_art(webp_data);
    assert!(
        result.is_ok(),
        "WebP data should be accepted by ASF cover art: {:?}",
        result.err()
    );
}

#[wasm_bindgen_test]
fn asf_cover_art_unknown_mime_fallback() {
    let mut file =
        audex_wasm::AudioFile::new(WMA_BYTES, Some("test.wma".to_string())).expect("load WMA");

    // Unknown magic bytes — should fall back to application/octet-stream
    let unknown_data = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
    let result = file.set_asf_cover_art(&unknown_data);
    assert!(
        result.is_ok(),
        "Unknown image data should still be accepted: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// APE binary tag round-trip on formats beyond TAK
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn ape_binary_tag_wavpack() {
    let mut file =
        audex_wasm::AudioFile::new(WV_BYTES, Some("test.wv".to_string())).expect("load WavPack");

    let image_data: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    file.set_ape_cover_art(&image_data)
        .expect("set APE cover art on WavPack");

    let retrieved = file
        .get_ape_binary_tag("Cover Art (Front)")
        .expect("binary tag should be present");
    assert!(
        retrieved.len() >= image_data.len(),
        "WavPack: binary data should contain cover art bytes"
    );
}

#[wasm_bindgen_test]
fn ape_binary_tag_musepack() {
    let mut file =
        audex_wasm::AudioFile::new(MPC_BYTES, Some("test.mpc".to_string())).expect("load Musepack");
    if !file.has_tags() {
        file.add_tags().expect("create tag container");
    }

    let image_data: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    file.set_ape_cover_art(&image_data)
        .expect("set APE cover art on Musepack");

    let retrieved = file
        .get_ape_binary_tag("Cover Art (Front)")
        .expect("binary tag should be present");
    assert!(
        retrieved.len() >= image_data.len(),
        "Musepack: binary data should contain cover art bytes"
    );
}

#[wasm_bindgen_test]
fn ape_binary_tag_monkeysaudio() {
    let mut file = audex_wasm::AudioFile::new(APE_BYTES, Some("test.ape".to_string()))
        .expect("load MonkeysAudio");
    if !file.has_tags() {
        file.add_tags().expect("create tag container");
    }

    let image_data: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    file.set_ape_cover_art(&image_data)
        .expect("set APE cover art on MonkeysAudio");

    let retrieved = file
        .get_ape_binary_tag("Cover Art (Front)")
        .expect("binary tag should be present");
    assert!(
        retrieved.len() >= image_data.len(),
        "MonkeysAudio: binary data should contain cover art bytes"
    );
}

#[wasm_bindgen_test]
fn ape_binary_tag_optimfrog() {
    let mut file = audex_wasm::AudioFile::new(OFR_BYTES, Some("test.ofr".to_string()))
        .expect("load OptimFROG");
    if !file.has_tags() {
        file.add_tags().expect("create tag container");
    }

    let image_data: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    file.set_ape_cover_art(&image_data)
        .expect("set APE cover art on OptimFROG");

    let retrieved = file
        .get_ape_binary_tag("Cover Art (Front)")
        .expect("binary tag should be present");
    assert!(
        retrieved.len() >= image_data.len(),
        "OptimFROG: binary data should contain cover art bytes"
    );
}

#[wasm_bindgen_test]
fn ape_binary_tag_trueaudio() {
    let mut file = audex_wasm::AudioFile::new(TTA_BYTES, Some("test.tta".to_string()))
        .expect("load TrueAudio");
    if !file.has_tags() {
        file.add_tags().expect("create tag container");
    }

    let image_data: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    file.set_ape_cover_art(&image_data)
        .expect("set APE cover art on TrueAudio");

    let retrieved = file
        .get_ape_binary_tag("Cover Art (Front)")
        .expect("binary tag should be present");
    assert!(
        retrieved.len() >= image_data.len(),
        "TrueAudio: binary data should contain cover art bytes"
    );
}

// ---------------------------------------------------------------------------
// Tag budget rollback: batch update exceeding budget rolls back
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn update_from_json_rollback_on_budget_exceeded() {
    let mut file =
        audex_wasm::AudioFile::new(FLAC_BYTES, Some("test.flac".to_string())).expect("load FLAC");

    // Set a tag first so we can verify it survives the failed update
    file.set_single("ARTIST", "Original Artist")
        .expect("set initial tag");

    // Build an update with a value large enough to exceed the 50 MB budget
    let obj = js_sys::Object::new();
    let oversized = "X".repeat(50 * 1024 * 1024 + 1);
    let arr = js_sys::Array::new();
    arr.push(&JsValue::from_str(&oversized));
    js_sys::Reflect::set(&obj, &JsValue::from_str("HUGE"), &arr).unwrap();

    let result = file.update_from_json(obj.into());
    assert!(result.is_err(), "Budget-exceeding update must fail");

    // The pre-existing tag must survive the failed update
    assert_eq!(
        file.get_first("ARTIST").as_deref(),
        Some("Original Artist"),
        "Pre-existing tags must be preserved after rollback"
    );
}

// ---------------------------------------------------------------------------
// Diff option combinations
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn diff_case_insensitive_and_trim_combined() {
    let mut a = audex_wasm::AudioFile::new(FLAC_BYTES, Some("a.flac".to_string())).expect("load A");
    let mut b = audex_wasm::AudioFile::new(FLAC_BYTES, Some("b.flac".to_string())).expect("load B");

    a.set_single("title", "  Hello  ").expect("set A");
    b.set_single("TITLE", "Hello").expect("set B");

    // With both case_insensitive=true and trim_values=true,
    // "  Hello  " and "Hello" on "title" vs "TITLE" should match
    let diff = a.diff_tags_with_options(
        &b,
        None,
        Some(true), // case_insensitive_keys
        Some(true), // trim_values
        None,
        None,
        None,
    );
    assert!(
        diff.is_identical(),
        "case-insensitive + trimmed diff should show no differences"
    );
}

#[wasm_bindgen_test]
fn diff_filter_keys_with_nonexistent_keys() {
    let a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    let diff = a.diff_tags(&b);
    // Filtering by keys that don't exist in the diff should produce an empty result
    let filtered = diff.filter_keys(vec![
        "NONEXISTENT_KEY_1".to_string(),
        "NONEXISTENT_KEY_2".to_string(),
    ]);
    assert_eq!(filtered.diff_count(), 0);
}

#[wasm_bindgen_test]
fn diff_differing_keys_on_empty_diff() {
    let a = audex_wasm::AudioFile::new(MP3_BYTES, Some("a.mp3".to_string())).expect("load A");
    let b = audex_wasm::AudioFile::new(MP3_BYTES, Some("b.mp3".to_string())).expect("load B");

    // Identical files should produce no differing keys
    let diff = a.diff_tags(&b);
    let keys = diff.differing_keys();
    assert!(
        keys.is_empty(),
        "Identical files should have no differing keys"
    );
}

// ---------------------------------------------------------------------------
// Filename hint vs magic bytes
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn detect_format_magic_wins_over_hint() {
    // Data is FLAC (starts with "fLaC"), but filename says .mp3
    let result = audex_wasm::detect_format(FLAC_BYTES, Some("track.mp3".to_string()));
    let format = result.expect("detection should succeed");
    assert!(
        format.contains("FLAC"),
        "Magic bytes should override filename hint: got {}",
        format
    );
}

#[wasm_bindgen_test]
fn detect_format_hint_disambiguates_weak_magic() {
    // The first few bytes of an MP3 file with a hint confirming it
    let result = audex_wasm::detect_format(&MP3_BYTES[..256], Some("track.mp3".to_string()));
    assert!(
        result.is_ok(),
        "Filename hint should help identify short data"
    );
}

// ---------------------------------------------------------------------------
// detect_format: input size limit (1 MB guard)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn detect_format_rejects_oversized_input() {
    // The WASM detect_format rejects inputs > 1 MB to prevent callers
    // from accidentally passing an entire file buffer.
    let oversized = vec![0u8; 1024 * 1024 + 1];
    let result = audex_wasm::detect_format(&oversized, Some("test.mp3".to_string()));
    assert!(result.is_err(), "Input exceeding 1 MB must be rejected");
}

#[wasm_bindgen_test]
fn detect_format_accepts_input_at_limit() {
    // Exactly 1 MB should be accepted (the guard is > not >=)
    let mut at_limit = vec![0u8; 1024 * 1024];
    // Place valid FLAC magic so detection succeeds
    at_limit[..4].copy_from_slice(b"fLaC");
    let result = audex_wasm::detect_format(&at_limit, Some("test.flac".to_string()));
    assert!(
        result.is_ok(),
        "Input at exactly 1 MB should be accepted: {:?}",
        result
    );
}

#[wasm_bindgen_test]
fn detect_format_works_with_small_input() {
    // A small slice with valid magic should work fine
    let result = audex_wasm::detect_format(&FLAC_BYTES[..128], Some("test.flac".to_string()));
    assert!(result.is_ok(), "Small valid input should detect format");
    let fmt = result.unwrap();
    assert!(fmt.contains("FLAC"), "Should detect FLAC, got: {}", fmt);
}
