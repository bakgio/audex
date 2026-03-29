//! Async clear tests for all taggable formats.
//!
//! Each test copies a fixture to a temp directory, writes a tag via `save_async`,
//! then calls `clear_async` and verifies tags are removed while stream info is
//! preserved.

#![cfg(feature = "async")]

use audex::{FileType, StreamInfo};

mod common;
use common::TestUtils;

// ---------------------------------------------------------------------------
// ID3v2-based formats (MP3, AIFF, WAVE, DSDIFF)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mp3_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.mp3");
    std::fs::copy(TestUtils::data_path("silence-44-s.mp3"), &path).expect("copy fixture");

    let mut mp3 = audex::mp3::MP3::load_async(&path).await.expect("load");
    mp3.set("TIT2", vec!["Temporary title".to_string()])
        .expect("set tag");
    mp3.save_async().await.expect("save");

    let sample_rate_before = mp3.info().sample_rate();

    let mut mp3 = audex::mp3::MP3::load_async(&path).await.expect("reload");
    mp3.clear_async().await.expect("clear");

    let cleared = audex::mp3::MP3::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "MP3: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "MP3: sample rate must survive clear"
    );
}

#[tokio::test]
async fn aiff_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.aif");
    std::fs::copy(TestUtils::data_path("with-id3.aif"), &path).expect("copy fixture");

    let mut aiff = audex::aiff::AIFF::load_async(&path).await.expect("load");
    aiff.add_tags().ok();
    aiff.set("TIT2", vec!["Temporary title".to_string()])
        .expect("set tag");
    aiff.save_async().await.expect("save");

    let sample_rate_before = aiff.info().sample_rate();

    let mut aiff = audex::aiff::AIFF::load_async(&path).await.expect("reload");
    aiff.clear_async().await.expect("clear");

    let cleared = audex::aiff::AIFF::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "AIFF: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "AIFF: sample rate must survive clear"
    );
}

#[tokio::test]
async fn wave_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.wav");
    std::fs::copy(
        TestUtils::data_path("silence-2s-PCM-44100-16-ID3v23.wav"),
        &path,
    )
    .expect("copy fixture");

    let mut wav = audex::wave::WAVE::load_async(&path).await.expect("load");
    wav.set("TIT2", vec!["Temporary title".to_string()])
        .expect("set tag");
    wav.save_async().await.expect("save");

    let sample_rate_before = wav.info().sample_rate();

    let mut wav = audex::wave::WAVE::load_async(&path).await.expect("reload");
    wav.clear_async().await.expect("clear");

    let cleared = audex::wave::WAVE::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "WAVE: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "WAVE: sample rate must survive clear"
    );
}

#[tokio::test]
async fn dsdiff_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.dff");
    std::fs::copy(TestUtils::data_path("5644800-2ch-s01-silence.dff"), &path)
        .expect("copy fixture");

    let mut dff = audex::dsdiff::DSDIFF::load_async(&path)
        .await
        .expect("load");
    dff.add_tags().ok();
    dff.set("TIT2", vec!["Temporary title".to_string()])
        .expect("set tag");
    dff.save_async().await.expect("save");

    let sample_rate_before = dff.info().sample_rate();

    let mut dff = audex::dsdiff::DSDIFF::load_async(&path)
        .await
        .expect("reload");
    dff.clear_async().await.expect("clear");

    let cleared = audex::dsdiff::DSDIFF::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "DSDIFF: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "DSDIFF: sample rate must survive clear"
    );
}

// ---------------------------------------------------------------------------
// Ogg-based formats (OggVorbis, OggOpus, OggSpeex, OggFlac, OggTheora)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn ogg_vorbis_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.ogg");
    std::fs::copy(TestUtils::data_path("empty.ogg"), &path).expect("copy fixture");

    let mut ogg = audex::oggvorbis::OggVorbis::load_async(&path)
        .await
        .expect("load");
    ogg.set("title", vec!["Temporary title".to_string()])
        .expect("set tag");
    ogg.save_async().await.expect("save");

    let sample_rate_before = ogg.info().sample_rate();

    let mut ogg = audex::oggvorbis::OggVorbis::load_async(&path)
        .await
        .expect("reload");
    ogg.clear_async().await.expect("clear");

    let cleared = audex::oggvorbis::OggVorbis::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "OggVorbis: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "OggVorbis: sample rate must survive clear"
    );
}

#[tokio::test]
async fn ogg_opus_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.opus");
    std::fs::copy(TestUtils::data_path("example.opus"), &path).expect("copy fixture");

    let mut opus = audex::oggopus::OggOpus::load_async(&path)
        .await
        .expect("load");
    opus.set("title", vec!["Temporary title".to_string()])
        .expect("set tag");
    opus.save_async().await.expect("save");

    let sample_rate_before = opus.info().sample_rate();

    let mut opus = audex::oggopus::OggOpus::load_async(&path)
        .await
        .expect("reload");
    opus.clear_async().await.expect("clear");

    let cleared = audex::oggopus::OggOpus::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "OggOpus: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "OggOpus: sample rate must survive clear"
    );
}

#[tokio::test]
async fn ogg_speex_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.spx");
    std::fs::copy(TestUtils::data_path("empty.spx"), &path).expect("copy fixture");

    let mut spx = audex::oggspeex::OggSpeex::load_async(&path)
        .await
        .expect("load");
    spx.set("title", vec!["Temporary title".to_string()])
        .expect("set tag");
    spx.save_async().await.expect("save");

    let sample_rate_before = spx.info().sample_rate();

    let mut spx = audex::oggspeex::OggSpeex::load_async(&path)
        .await
        .expect("reload");
    spx.clear_async().await.expect("clear");

    let cleared = audex::oggspeex::OggSpeex::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "OggSpeex: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "OggSpeex: sample rate must survive clear"
    );
}

#[tokio::test]
async fn ogg_flac_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.oggflac");
    std::fs::copy(TestUtils::data_path("empty.oggflac"), &path).expect("copy fixture");

    let mut of = audex::oggflac::OggFlac::load_async(&path)
        .await
        .expect("load");
    of.set("title", vec!["Temporary title".to_string()])
        .expect("set tag");
    of.save_async().await.expect("save");

    let sample_rate_before = of.info().sample_rate();

    let mut of = audex::oggflac::OggFlac::load_async(&path)
        .await
        .expect("reload");
    of.clear_async().await.expect("clear");

    let cleared = audex::oggflac::OggFlac::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "OggFlac: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "OggFlac: sample rate must survive clear"
    );
}

#[tokio::test]
async fn ogg_theora_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.oggtheora");
    std::fs::copy(TestUtils::data_path("sample.oggtheora"), &path).expect("copy fixture");

    let mut theora = audex::oggtheora::OggTheora::load_async(&path)
        .await
        .expect("load");
    theora
        .set("title", vec!["Temporary title".to_string()])
        .expect("set tag");
    theora.save_async().await.expect("save");

    let channels_before = theora.info().channels();

    let mut theora = audex::oggtheora::OggTheora::load_async(&path)
        .await
        .expect("reload");
    theora.clear_async().await.expect("clear");

    let cleared = audex::oggtheora::OggTheora::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "OggTheora: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().channels(),
        channels_before,
        "OggTheora: channels must survive clear"
    );
}

// ---------------------------------------------------------------------------
// APEv2-based formats (WavPack, Musepack, OptimFROG, TAK, TrueAudio)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn wavpack_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.wv");
    std::fs::copy(TestUtils::data_path("silence-44-s.wv"), &path).expect("copy fixture");

    let mut wv = audex::wavpack::WavPack::load_async(&path)
        .await
        .expect("load");
    wv.add_tags().ok();
    wv.set("Title", vec!["Temporary title".to_string()])
        .expect("set tag");
    wv.save_async().await.expect("save");

    let sample_rate_before = wv.info().sample_rate();

    let mut wv = audex::wavpack::WavPack::load_async(&path)
        .await
        .expect("reload");
    wv.clear_async().await.expect("clear");

    let cleared = audex::wavpack::WavPack::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.tags.is_none() || cleared.keys().is_empty(),
        "WavPack: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "WavPack: sample rate must survive clear"
    );
}

#[tokio::test]
async fn musepack_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.mpc");
    std::fs::copy(TestUtils::data_path("click.mpc"), &path).expect("copy fixture");

    let mut mpc = audex::musepack::Musepack::load_async(&path)
        .await
        .expect("load");
    mpc.add_tags().ok();
    mpc.set("Title", vec!["Temporary title".to_string()])
        .expect("set tag");
    mpc.save_async().await.expect("save");

    let sample_rate_before = mpc.info().sample_rate();

    let mut mpc = audex::musepack::Musepack::load_async(&path)
        .await
        .expect("reload");
    mpc.clear_async().await.expect("clear");

    let cleared = audex::musepack::Musepack::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.tags.is_none() || cleared.keys().is_empty(),
        "Musepack: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "Musepack: sample rate must survive clear"
    );
}

#[tokio::test]
async fn optimfrog_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.ofr");
    std::fs::copy(TestUtils::data_path("empty.ofr"), &path).expect("copy fixture");

    let mut ofr = audex::optimfrog::OptimFROG::load_async(&path)
        .await
        .expect("load");
    ofr.add_tags().ok();
    ofr.set("Title", vec!["Temporary title".to_string()])
        .expect("set tag");
    ofr.save_async().await.expect("save");

    let sample_rate_before = ofr.info().sample_rate();

    let mut ofr = audex::optimfrog::OptimFROG::load_async(&path)
        .await
        .expect("reload");
    ofr.clear_async().await.expect("clear");

    let cleared = audex::optimfrog::OptimFROG::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.tags.is_none() || cleared.keys().is_empty(),
        "OptimFROG: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "OptimFROG: sample rate must survive clear"
    );
}

#[tokio::test]
async fn tak_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.tak");
    std::fs::copy(TestUtils::data_path("has-tags.tak"), &path).expect("copy fixture");

    let mut tak = audex::tak::TAK::load_async(&path).await.expect("load");
    tak.add_tags().ok();
    tak.set("Title", vec!["Temporary title".to_string()])
        .expect("set tag");
    tak.save_async().await.expect("save");

    let sample_rate_before = tak.info().sample_rate();

    let mut tak = audex::tak::TAK::load_async(&path).await.expect("reload");
    tak.clear_async().await.expect("clear");

    let cleared = audex::tak::TAK::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.tags.is_none() || cleared.keys().is_empty(),
        "TAK: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "TAK: sample rate must survive clear"
    );
}

#[tokio::test]
async fn trueaudio_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.tta");
    std::fs::copy(TestUtils::data_path("empty.tta"), &path).expect("copy fixture");

    // TrueAudio supports both ID3 and APE; async operations use APE tags
    let mut tta = audex::trueaudio::TrueAudio::load_async(&path)
        .await
        .expect("load");
    tta.assign_ape_tags();
    tta.set("Title", vec!["Temporary title".to_string()])
        .expect("set tag");
    tta.save_async().await.expect("save");

    let sample_rate_before = tta.info().sample_rate();

    let mut tta = audex::trueaudio::TrueAudio::load_async(&path)
        .await
        .expect("reload");
    tta.clear_async().await.expect("clear");

    let cleared = audex::trueaudio::TrueAudio::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().is_empty(),
        "TrueAudio: tags should be empty after clear"
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "TrueAudio: sample rate must survive clear"
    );
}

// ---------------------------------------------------------------------------
// ASF/WMA format
// ---------------------------------------------------------------------------

#[tokio::test]
async fn asf_clear_async_removes_tags_preserves_stream_info() {
    let tmp = tempfile::tempdir().expect("create temp dir");
    let path = tmp.path().join("clear.wma");
    std::fs::copy(TestUtils::data_path("silence-1.wma"), &path).expect("copy fixture");

    let mut asf = audex::asf::ASF::load_async(&path).await.expect("load");
    asf.set("Title", vec!["Temporary title".to_string()]);
    asf.save_async().await.expect("save");

    let asf = audex::asf::ASF::load_async(&path)
        .await
        .expect("reload after save");
    let keys_before = asf.keys().len();
    let sample_rate_before = asf.info().sample_rate();

    let mut asf = audex::asf::ASF::load_async(&path)
        .await
        .expect("reload for clear");
    asf.clear_async().await.expect("clear");

    // ASF may retain structural metadata keys after clearing, so we verify the
    // count decreased rather than requiring it to be zero.
    let cleared = audex::asf::ASF::load_async(&path)
        .await
        .expect("reload cleared");
    assert!(
        cleared.keys().len() < keys_before,
        "ASF: key count should decrease after clear (was {}, now {})",
        keys_before,
        cleared.keys().len(),
    );
    assert_eq!(
        cleared.info().sample_rate(),
        sample_rate_before,
        "ASF: sample rate must survive clear"
    );
}
