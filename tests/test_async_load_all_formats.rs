//! Async load parity tests for all supported audio formats.
//!
//! Verifies that `File::load_async()` and `File::load_from_buffer_async()`
//! produce stream info and format detection results identical to the
//! synchronous `File::load()` for every supported format.

#![cfg(feature = "async")]

mod common;

use audex::{File, StreamInfo};
use common::TestUtils;

/// Loads a test fixture via sync and both async methods, then asserts that
/// format name and stream info fields are identical across all three.
async fn assert_load_parity(filename: &str) {
    let path = TestUtils::data_path(filename);
    if !path.exists() {
        return;
    }

    let sync_file = File::load(&path).unwrap_or_else(|e| {
        panic!("{filename}: sync File::load failed: {e}");
    });

    // --- File::load_async (path-based) ---
    let async_file = File::load_async(&path).await.unwrap_or_else(|e| {
        panic!("{filename}: File::load_async failed: {e}");
    });

    assert_eq!(
        sync_file.format_name(),
        async_file.format_name(),
        "{filename}: format name mismatch (load_async)"
    );
    assert_eq!(
        sync_file.info().sample_rate(),
        async_file.info().sample_rate(),
        "{filename}: sample_rate mismatch (load_async)"
    );
    assert_eq!(
        sync_file.info().channels(),
        async_file.info().channels(),
        "{filename}: channels mismatch (load_async)"
    );
    assert_eq!(
        sync_file.info().bits_per_sample(),
        async_file.info().bits_per_sample(),
        "{filename}: bits_per_sample mismatch (load_async)"
    );
    assert_eq!(
        sync_file.info().length(),
        async_file.info().length(),
        "{filename}: duration mismatch (load_async)"
    );
    assert_eq!(
        sync_file.info().bitrate(),
        async_file.info().bitrate(),
        "{filename}: bitrate mismatch (load_async)"
    );

    // --- File::load_from_buffer_async (in-memory) ---
    let data = std::fs::read(&path).unwrap();
    let buf_file = File::load_from_buffer_async(data, Some(path.clone()))
        .await
        .unwrap_or_else(|e| {
            panic!("{filename}: File::load_from_buffer_async failed: {e}");
        });

    assert_eq!(
        sync_file.format_name(),
        buf_file.format_name(),
        "{filename}: format name mismatch (load_from_buffer_async)"
    );
    assert_eq!(
        sync_file.info().sample_rate(),
        buf_file.info().sample_rate(),
        "{filename}: sample_rate mismatch (load_from_buffer_async)"
    );
    assert_eq!(
        sync_file.info().channels(),
        buf_file.info().channels(),
        "{filename}: channels mismatch (load_from_buffer_async)"
    );
    assert_eq!(
        sync_file.info().bits_per_sample(),
        buf_file.info().bits_per_sample(),
        "{filename}: bits_per_sample mismatch (load_from_buffer_async)"
    );
    assert_eq!(
        sync_file.info().length(),
        buf_file.info().length(),
        "{filename}: duration mismatch (load_from_buffer_async)"
    );
    assert_eq!(
        sync_file.info().bitrate(),
        buf_file.info().bitrate(),
        "{filename}: bitrate mismatch (load_from_buffer_async)"
    );
}

// ---------------------------------------------------------------------------
// One test per format — covers all 19 formats from the test matrix
// ---------------------------------------------------------------------------

#[tokio::test]
async fn aiff() {
    assert_load_parity("8k-1ch-1s-silence.aif").await;
}

#[tokio::test]
async fn monkey_audio() {
    assert_load_parity("mac-399.ape").await;
}

#[tokio::test]
async fn asf() {
    assert_load_parity("silence-1.wma").await;
}

#[tokio::test]
async fn dsdiff() {
    assert_load_parity("5644800-2ch-s01-silence.dff").await;
}

#[tokio::test]
async fn dsf() {
    assert_load_parity("5644800-2ch-s01-silence.dsf").await;
}

#[tokio::test]
async fn flac() {
    assert_load_parity("silence-44-s.flac").await;
}

#[tokio::test]
async fn m4a() {
    assert_load_parity("has-tags.m4a").await;
}

#[tokio::test]
async fn mp3() {
    assert_load_parity("silence-44-s.mp3").await;
}

#[tokio::test]
async fn musepack() {
    assert_load_parity("click.mpc").await;
}

#[tokio::test]
async fn optimfrog() {
    assert_load_parity("silence-2s-44100-16.ofr").await;
}

#[tokio::test]
async fn ogg_flac() {
    assert_load_parity("empty.oggflac").await;
}

#[tokio::test]
async fn ogg_vorbis() {
    assert_load_parity("multipagecomment.ogg").await;
}

#[tokio::test]
async fn ogg_theora() {
    assert_load_parity("sample.oggtheora").await;
}

#[tokio::test]
async fn ogg_opus() {
    assert_load_parity("example.opus").await;
}

#[tokio::test]
async fn ogg_speex() {
    assert_load_parity("empty.spx").await;
}

#[tokio::test]
async fn tak() {
    assert_load_parity("silence-44-s.tak").await;
}

#[tokio::test]
async fn trueaudio() {
    assert_load_parity("empty.tta").await;
}

#[tokio::test]
async fn wave() {
    assert_load_parity("silence-2s-PCM-44100-16-ID3v23.wav").await;
}

#[tokio::test]
async fn wavpack() {
    assert_load_parity("silence-44-s.wv").await;
}
