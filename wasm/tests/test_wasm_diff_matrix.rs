#![cfg(target_arch = "wasm32")]

// WASM cross-format normalized diff tests covering all five tag systems.
//
// web.rs only tests MP3 vs FLAC. This file adds APEv2 and ASF pairs plus
// self-diff sanity checks. All operations use in-memory byte buffers.
// No filesystem writes occur.

use wasm_bindgen_test::*;

const MP3_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.mp3");
const FLAC_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.flac");
const M4A_BYTES: &[u8] = include_bytes!("../../tests/data/has-tags.m4a");
const WMA_BYTES: &[u8] = include_bytes!("../../tests/data/silence-1.wma");
const WV_BYTES: &[u8] = include_bytes!("../../tests/data/silence-44-s.wv");
const TAK_BYTES: &[u8] = include_bytes!("../../tests/data/has-tags.tak");
const MPC_BYTES: &[u8] = include_bytes!("../../tests/data/click.mpc");

fn load(bytes: &[u8], name: &str) -> audex_wasm::AudioFile {
    audex_wasm::AudioFile::new(bytes, Some(name.to_string())).expect("load")
}

// ---------------------------------------------------------------------------
// APEv2 vs other tag systems
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn diff_wavpack_vs_mp3() {
    let a = load(WV_BYTES, "a.wv");
    let b = load(MP3_BYTES, "b.mp3");
    let d = a.diff_tags_normalized(&b, None, None, None);
    let _ = d.summary();
    let _ = d.diff_count();
}

#[wasm_bindgen_test]
fn diff_wavpack_vs_flac() {
    let a = load(WV_BYTES, "a.wv");
    let b = load(FLAC_BYTES, "b.flac");
    let d = a.diff_tags_normalized(&b, None, None, None);
    let _ = d.summary();
}

#[wasm_bindgen_test]
fn diff_wavpack_vs_m4a() {
    let a = load(WV_BYTES, "a.wv");
    let b = load(M4A_BYTES, "b.m4a");
    let d = a.diff_tags_normalized(&b, None, None, None);
    let _ = d.summary();
}

// ---------------------------------------------------------------------------
// ASF vs other tag systems
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn diff_asf_vs_mp3() {
    let a = load(WMA_BYTES, "a.wma");
    let b = load(MP3_BYTES, "b.mp3");
    let d = a.diff_tags_normalized(&b, None, None, None);
    let _ = d.summary();
}

#[wasm_bindgen_test]
fn diff_asf_vs_flac() {
    let a = load(WMA_BYTES, "a.wma");
    let b = load(FLAC_BYTES, "b.flac");
    let d = a.diff_tags_normalized(&b, None, None, None);
    let _ = d.summary();
}

#[wasm_bindgen_test]
fn diff_asf_vs_wavpack() {
    let a = load(WMA_BYTES, "a.wma");
    let b = load(WV_BYTES, "b.wv");
    let d = a.diff_tags_normalized(&b, None, None, None);
    let _ = d.summary();
}

// ---------------------------------------------------------------------------
// APEv2 vs APEv2 (different containers)
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn diff_wavpack_vs_tak() {
    let a = load(WV_BYTES, "a.wv");
    let b = load(TAK_BYTES, "b.tak");
    let d = a.diff_tags_normalized(&b, None, None, None);
    let _ = d.summary();
}

#[wasm_bindgen_test]
fn diff_wavpack_vs_musepack() {
    let a = load(WV_BYTES, "a.wv");
    let b = load(MPC_BYTES, "b.mpc");
    let d = a.diff_tags_normalized(&b, None, None, None);
    let _ = d.summary();
}

// ---------------------------------------------------------------------------
// Self-diff: same bytes diffed against themselves must be identical
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn self_diff_wavpack() {
    let a = load(WV_BYTES, "a.wv");
    let b = load(WV_BYTES, "b.wv");
    let d = a.diff_tags_normalized(&b, None, None, None);
    assert!(d.is_identical(), "WavPack self-diff should be identical");
}

#[wasm_bindgen_test]
fn self_diff_asf() {
    let a = load(WMA_BYTES, "a.wma");
    let b = load(WMA_BYTES, "b.wma");
    let d = a.diff_tags_normalized(&b, None, None, None);
    assert!(d.is_identical(), "ASF self-diff should be identical");
}

#[wasm_bindgen_test]
fn self_diff_m4a() {
    let a = load(M4A_BYTES, "a.m4a");
    let b = load(M4A_BYTES, "b.m4a");
    let d = a.diff_tags_normalized(&b, None, None, None);
    assert!(d.is_identical(), "M4A self-diff should be identical");
}
