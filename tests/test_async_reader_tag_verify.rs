// Verifies that async loading methods produce the same tag keys and values
// as synchronous loading for every supported format.
//
// The companion test_async_load_all_formats.rs confirms stream info parity;
// this test extends coverage to tag content.

#![cfg(feature = "async")]

mod common;

use audex::{File, StreamInfo};
use common::TestUtils;
use std::io::Cursor;

/// Loads via sync File::load_from_reader and async File::load_from_buffer_async,
/// then asserts every tag key and value is identical across both methods.
async fn assert_tag_parity(filename: &str) {
    let path = TestUtils::data_path(filename);
    if !path.exists() {
        return;
    }

    let data = std::fs::read(&path).unwrap();

    let sync_file = File::load_from_reader(Cursor::new(data.clone()), Some(path.clone()))
        .unwrap_or_else(|e| panic!("{}: sync load failed: {}", filename, e));

    let async_file = File::load_from_buffer_async(data, Some(path))
        .await
        .unwrap_or_else(|e| panic!("{}: async load failed: {}", filename, e));

    // Tag keys must match
    let mut sync_keys = sync_file.keys();
    let mut async_keys = async_file.keys();
    sync_keys.sort();
    async_keys.sort();
    assert_eq!(
        sync_keys, async_keys,
        "{}: tag keys differ between sync and async load",
        filename
    );

    // Every tag value must match
    for key in &sync_keys {
        let sync_vals = sync_file.get(key);
        let async_vals = async_file.get(key);
        assert_eq!(
            sync_vals, async_vals,
            "{}: tag '{}' values differ between sync and async load",
            filename, key
        );
    }

    // Stream info sanity check
    assert_eq!(
        sync_file.info().sample_rate(),
        async_file.info().sample_rate(),
        "{}: sample_rate mismatch",
        filename
    );
}

// ---------------------------------------------------------------------------
// ID3v2 formats
// ---------------------------------------------------------------------------

#[tokio::test]
async fn async_tags_mp3() {
    assert_tag_parity("silence-44-s.mp3").await;
}

#[tokio::test]
async fn async_tags_aiff() {
    assert_tag_parity("with-id3.aif").await;
}

#[tokio::test]
async fn async_tags_wav() {
    assert_tag_parity("silence-2s-PCM-44100-16-ID3v23.wav").await;
}

#[tokio::test]
async fn async_tags_dsf() {
    assert_tag_parity("with-id3.dsf").await;
}

#[tokio::test]
async fn async_tags_dsdiff() {
    assert_tag_parity("5644800-2ch-s01-silence.dff").await;
}

// ---------------------------------------------------------------------------
// Vorbis Comment formats
// ---------------------------------------------------------------------------

#[tokio::test]
async fn async_tags_flac() {
    assert_tag_parity("silence-44-s.flac").await;
}

#[tokio::test]
async fn async_tags_ogg_vorbis() {
    assert_tag_parity("multipagecomment.ogg").await;
}

#[tokio::test]
async fn async_tags_opus() {
    assert_tag_parity("example.opus").await;
}

#[tokio::test]
async fn async_tags_speex() {
    assert_tag_parity("empty.spx").await;
}

#[tokio::test]
async fn async_tags_ogg_flac() {
    assert_tag_parity("empty.oggflac").await;
}

#[tokio::test]
async fn async_tags_ogg_theora() {
    assert_tag_parity("sample.oggtheora").await;
}

// ---------------------------------------------------------------------------
// MP4 atoms
// ---------------------------------------------------------------------------

#[tokio::test]
async fn async_tags_m4a() {
    assert_tag_parity("has-tags.m4a").await;
}

// ---------------------------------------------------------------------------
// APEv2 formats
// ---------------------------------------------------------------------------

#[tokio::test]
async fn async_tags_monkeysaudio() {
    assert_tag_parity("mac-399.ape").await;
}

#[tokio::test]
async fn async_tags_musepack() {
    assert_tag_parity("click.mpc").await;
}

#[tokio::test]
async fn async_tags_wavpack() {
    assert_tag_parity("silence-44-s.wv").await;
}

#[tokio::test]
async fn async_tags_optimfrog() {
    assert_tag_parity("silence-2s-44100-16.ofr").await;
}

#[tokio::test]
async fn async_tags_tak() {
    assert_tag_parity("has-tags.tak").await;
}

#[tokio::test]
async fn async_tags_trueaudio() {
    assert_tag_parity("empty.tta").await;
}

// ---------------------------------------------------------------------------
// ASF / WMA
// ---------------------------------------------------------------------------

#[tokio::test]
async fn async_tags_wma() {
    assert_tag_parity("silence-1.wma").await;
}
