#![cfg(target_arch = "wasm32")]

// WASM cross-format tag conversion (importTagsFrom) tests.
//
// web.rs only tests MP3 -> FLAC. This file adds APEv2 and ASF pairs to
// ensure every tag system has been tested as both source and destination.
//
// All operations use in-memory byte buffers. Source files are loaded from
// compile-time embedded data, tags are set on in-memory copies, and
// conversion happens entirely in memory. No filesystem writes occur.

use wasm_bindgen_test::*;

const MP3_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.mp3");
const FLAC_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.flac");
const M4A_BYTES: &[u8] = include_bytes!("../../tests/data/has-tags.m4a");
const WMA_BYTES: &[u8] = include_bytes!("../../tests/data/silence-1.wma");
const WV_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.wv");

fn load(bytes: &[u8], name: &str) -> audex_wasm::AudioFile {
    audex_wasm::AudioFile::new(bytes, Some(name.to_string())).expect("load")
}

// ---------------------------------------------------------------------------
// ID3v2 -> APEv2 (MP3 -> WavPack)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn convert_mp3_to_wavpack() {
    let mut src = load(MP3_BYTES, "src.mp3");
    src.set_single("TIT2", "Convert Test").ok();

    let mut dest = load(WV_BYTES, "dest.wv");
    dest.add_tags().ok();

    let report = dest.import_tags_from(&src);
    assert!(report.is_ok(), "MP3 -> WavPack conversion should succeed");
}

// ---------------------------------------------------------------------------
// VorbisComment -> APEv2 (FLAC -> WavPack)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn convert_flac_to_wavpack() {
    let mut src = load(FLAC_BYTES, "src.flac");
    src.set_single("TITLE", "Convert Test").ok();

    let mut dest = load(WV_BYTES, "dest.wv");
    dest.add_tags().ok();

    let report = dest.import_tags_from(&src);
    assert!(report.is_ok(), "FLAC -> WavPack conversion should succeed");
}

// ---------------------------------------------------------------------------
// APEv2 -> VorbisComment (WavPack -> FLAC)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn convert_wavpack_to_flac() {
    let mut src = load(WV_BYTES, "src.wv");
    src.add_tags().ok();
    src.set_single("Title", "Convert Test").ok();

    let mut dest = load(FLAC_BYTES, "dest.flac");

    let report = dest.import_tags_from(&src);
    assert!(report.is_ok(), "WavPack -> FLAC conversion should succeed");
}

// ---------------------------------------------------------------------------
// APEv2 -> ID3v2 (WavPack -> MP3)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn convert_wavpack_to_mp3() {
    let mut src = load(WV_BYTES, "src.wv");
    src.add_tags().ok();
    src.set_single("Title", "Convert Test").ok();

    let mut dest = load(MP3_BYTES, "dest.mp3");

    let report = dest.import_tags_from(&src);
    assert!(report.is_ok(), "WavPack -> MP3 conversion should succeed");
}

// ---------------------------------------------------------------------------
// APEv2 -> ASF (WavPack -> WMA)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn convert_wavpack_to_asf() {
    let mut src = load(WV_BYTES, "src.wv");
    src.add_tags().ok();
    src.set_single("Title", "Convert Test").ok();

    let mut dest = load(WMA_BYTES, "dest.wma");

    let report = dest.import_tags_from(&src);
    assert!(report.is_ok(), "WavPack -> ASF conversion should succeed");
}

// ---------------------------------------------------------------------------
// ASF -> APEv2 (WMA -> WavPack)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn convert_asf_to_wavpack() {
    let src = load(WMA_BYTES, "src.wma");

    let mut dest = load(WV_BYTES, "dest.wv");
    dest.add_tags().ok();

    let report = dest.import_tags_from(&src);
    assert!(report.is_ok(), "ASF -> WavPack conversion should succeed");
}

// ---------------------------------------------------------------------------
// MP4 -> VorbisComment (M4A -> FLAC)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn convert_mp4_to_flac() {
    let src = load(M4A_BYTES, "src.m4a");
    let mut dest = load(FLAC_BYTES, "dest.flac");

    let report = dest.import_tags_from(&src);
    assert!(report.is_ok(), "MP4 -> FLAC conversion should succeed");
}
