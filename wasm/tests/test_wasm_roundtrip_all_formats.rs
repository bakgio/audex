#![cfg(target_arch = "wasm32")]

// WASM roundtrip tests for formats not covered in web.rs.
//
// web.rs covers MP3, FLAC, M4A, OGG Vorbis, AIFF, WAV, ASF, Opus, DSF.
// This file adds the remaining 10 formats: APE, DFF, MPC, OFR, OGA,
// OGV, SPX, TAK, TTA, WV.
//
// All operations use in-memory byte buffers embedded at compile time.
// No filesystem writes occur.

use wasm_bindgen_test::*;

// ---------------------------------------------------------------------------
// Embedded test fixtures (compile-time, read-only)
// ---------------------------------------------------------------------------

const APE_BYTES: &[u8] = include_bytes!("../../tests/data/mac-399.ape");
const DFF_BYTES: &[u8] = include_bytes!("../../tests/data/5644800-2ch-s01-silence.dff");
const MPC_BYTES: &[u8] = include_bytes!("../../tests/data/click.mpc");
const OFR_BYTES: &[u8] = include_bytes!("../../tests/data/silence-2s-44100-16.ofr");
const OGA_BYTES: &[u8] = include_bytes!("../../tests/data/empty.oggflac");
const OGV_BYTES: &[u8] = include_bytes!("../../tests/data/sample.oggtheora");
const SPX_BYTES: &[u8] = include_bytes!("../../tests/data/empty.spx");
const TAK_BYTES: &[u8] = include_bytes!("../../tests/data/has-tags.tak");
const TTA_BYTES: &[u8] = include_bytes!("../../tests/data/empty.tta");
const WV_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.wv");

/// Helper: load bytes, add tags if needed, set a title, save, reload, verify.
/// Uses APEv2-style key ("Title") for APE-based formats.
fn ape_roundtrip(bytes: &[u8], ext: &str, format_hint: &str) {
    let filename = format!("test.{}", ext);
    let mut file = audex_wasm::AudioFile::new(bytes, Some(filename.clone())).expect("load");

    file.add_tags().ok();
    let tag_value = format!("{} Round Trip", format_hint);
    file.set_single("Title", &tag_value).expect("set Title");

    let saved = file.save().expect("save");
    let reloaded = audex_wasm::AudioFile::new(&saved, Some(filename)).expect("reload");

    assert_eq!(
        reloaded.get_first("Title").as_deref(),
        Some(tag_value.as_str()),
        "{}: Title should survive save/reload",
        format_hint,
    );
}

/// Helper for Vorbis Comment-based OGG formats.
fn vorbis_roundtrip(bytes: &[u8], ext: &str, format_hint: &str) {
    let filename = format!("test.{}", ext);
    let mut file = audex_wasm::AudioFile::new(bytes, Some(filename.clone())).expect("load");

    let tag_value = format!("{} Round Trip", format_hint);
    file.set_single("title", &tag_value).expect("set title");

    let saved = file.save().expect("save");
    let reloaded = audex_wasm::AudioFile::new(&saved, Some(filename)).expect("reload");

    assert_eq!(
        reloaded.get_first("title").as_deref(),
        Some(tag_value.as_str()),
        "{}: title should survive save/reload",
        format_hint,
    );
}

/// Helper for ID3v2-based formats.
fn id3_roundtrip(bytes: &[u8], ext: &str, format_hint: &str) {
    let filename = format!("test.{}", ext);
    let mut file = audex_wasm::AudioFile::new(bytes, Some(filename.clone())).expect("load");

    file.add_tags().ok();
    let tag_value = format!("{} Round Trip", format_hint);
    file.set_single("TIT2", &tag_value).expect("set TIT2");

    let saved = file.save().expect("save");
    let reloaded = audex_wasm::AudioFile::new(&saved, Some(filename)).expect("reload");

    assert_eq!(
        reloaded.get_first("TIT2").as_deref(),
        Some(tag_value.as_str()),
        "{}: TIT2 should survive save/reload",
        format_hint,
    );
}

// ---------------------------------------------------------------------------
// APEv2-based formats
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn roundtrip_monkeysaudio() {
    ape_roundtrip(APE_BYTES, "ape", "MonkeysAudio");
}

#[wasm_bindgen_test]
fn roundtrip_musepack() {
    ape_roundtrip(MPC_BYTES, "mpc", "Musepack");
}

#[wasm_bindgen_test]
fn roundtrip_wavpack() {
    ape_roundtrip(WV_BYTES, "wv", "WavPack");
}

#[wasm_bindgen_test]
fn roundtrip_optimfrog() {
    ape_roundtrip(OFR_BYTES, "ofr", "OptimFROG");
}

#[wasm_bindgen_test]
fn roundtrip_tak() {
    ape_roundtrip(TAK_BYTES, "tak", "TAK");
}

#[wasm_bindgen_test]
fn roundtrip_trueaudio() {
    ape_roundtrip(TTA_BYTES, "tta", "TrueAudio");
}

// ---------------------------------------------------------------------------
// Vorbis Comment-based OGG formats
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn roundtrip_ogg_flac() {
    vorbis_roundtrip(OGA_BYTES, "oggflac", "OggFlac");
}

#[wasm_bindgen_test]
fn roundtrip_ogg_theora() {
    vorbis_roundtrip(OGV_BYTES, "oggtheora", "OggTheora");
}

#[wasm_bindgen_test]
fn roundtrip_ogg_speex() {
    vorbis_roundtrip(SPX_BYTES, "spx", "OggSpeex");
}

// ---------------------------------------------------------------------------
// ID3v2-based format
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn roundtrip_dsdiff() {
    id3_roundtrip(DFF_BYTES, "dff", "DSDIFF");
}
