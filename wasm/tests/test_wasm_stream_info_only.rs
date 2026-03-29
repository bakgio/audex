#![cfg(target_arch = "wasm32")]

// Tests for stream-info-only formats (AAC, AC-3, SMF) in WASM.
//
// These formats support metadata extraction (stream properties like
// bitrate, sample rate, channels) but do not support tag writing.
// The WASM API should load them successfully and expose stream info
// without errors, while tag mutations should fail gracefully.

use wasm_bindgen_test::*;

const AAC_BYTES: &[u8] = include_bytes!("../../tests/data/empty.aac");
const AC3_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.ac3");
const MIDI_BYTES: &[u8] = include_bytes!("../../tests/data/sample.mid");

// ---------------------------------------------------------------------------
// AAC
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn aac_loads_and_reports_format() {
    let file =
        audex_wasm::AudioFile::new(AAC_BYTES, Some("test.aac".into())).expect("AAC should load");
    let name = file.format_name();
    assert!(
        name.contains("AAC") || name.contains("aac"),
        "Format name should contain AAC, got: {}",
        name
    );
}

#[wasm_bindgen_test]
fn aac_stream_info_available() {
    let file =
        audex_wasm::AudioFile::new(AAC_BYTES, Some("test.aac".into())).expect("AAC should load");
    let info = file.stream_info();
    // At minimum, sample_rate or channels should be present for a valid AAC
    let has_any =
        info.sample_rate().is_some() || info.channels().is_some() || info.bitrate().is_some();
    assert!(has_any, "AAC stream info should have at least one property");
}

#[wasm_bindgen_test]
fn aac_set_tag_returns_error() {
    let mut file =
        audex_wasm::AudioFile::new(AAC_BYTES, Some("test.aac".into())).expect("AAC should load");
    // Stream-info-only formats should reject tag writes
    let result = file.set_single("TITLE", "test");
    assert!(
        result.is_err(),
        "Setting tags on AAC should fail — stream-info-only format"
    );
}

// ---------------------------------------------------------------------------
// AC-3
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn ac3_loads_and_reports_format() {
    let file =
        audex_wasm::AudioFile::new(AC3_BYTES, Some("test.ac3".into())).expect("AC-3 should load");
    let name = file.format_name();
    assert!(
        name.contains("AC3") || name.contains("ac3") || name.contains("AC-3"),
        "Format name should indicate AC-3, got: {}",
        name
    );
}

#[wasm_bindgen_test]
fn ac3_stream_info_available() {
    let file =
        audex_wasm::AudioFile::new(AC3_BYTES, Some("test.ac3".into())).expect("AC-3 should load");
    let info = file.stream_info();
    let has_any =
        info.sample_rate().is_some() || info.channels().is_some() || info.bitrate().is_some();
    assert!(
        has_any,
        "AC-3 stream info should have at least one property"
    );
}

#[wasm_bindgen_test]
fn ac3_set_tag_returns_error() {
    let mut file =
        audex_wasm::AudioFile::new(AC3_BYTES, Some("test.ac3".into())).expect("AC-3 should load");
    let result = file.set_single("TITLE", "test");
    assert!(
        result.is_err(),
        "Setting tags on AC-3 should fail — stream-info-only format"
    );
}

// ---------------------------------------------------------------------------
// SMF (Standard MIDI File)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn smf_loads_and_reports_format() {
    let file =
        audex_wasm::AudioFile::new(MIDI_BYTES, Some("test.mid".into())).expect("MIDI should load");
    let name = file.format_name();
    assert!(
        name.contains("SMF") || name.contains("MIDI") || name.contains("midi"),
        "Format name should indicate SMF/MIDI, got: {}",
        name
    );
}

#[wasm_bindgen_test]
fn smf_stream_info_available() {
    let file =
        audex_wasm::AudioFile::new(MIDI_BYTES, Some("test.mid".into())).expect("MIDI should load");
    // MIDI may have limited stream info — just verify it doesn't panic
    let _info = file.stream_info();
}

#[wasm_bindgen_test]
fn smf_set_tag_returns_error() {
    let mut file =
        audex_wasm::AudioFile::new(MIDI_BYTES, Some("test.mid".into())).expect("MIDI should load");
    let result = file.set_single("TITLE", "test");
    assert!(
        result.is_err(),
        "Setting tags on MIDI should fail — stream-info-only format"
    );
}
