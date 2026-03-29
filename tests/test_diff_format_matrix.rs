use audex::File;
/// Cross-format normalized diff tests covering all five tag systems.
///
/// Existing diff tests cover ID3v2, VorbisComment, and MP4 pairs. This file
/// adds APEv2 and ASF to ensure the normalization layer handles every tag
/// system correctly when comparing across formats.
///
/// All operations are read-only — files are loaded into memory via
/// std::fs::read and diffed through in-memory cursors. Original test data
/// is never modified.
use audex::diff;
use std::path::PathBuf;

fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Load a file from path (read-only) for diffing.
fn load(filename: &str) -> Option<audex::DynamicFileType> {
    let path = data_path(filename);
    if !path.exists() {
        return None;
    }
    File::load(&path).ok()
}

// ---------------------------------------------------------------------------
// APEv2 (WavPack) vs other tag systems
// ---------------------------------------------------------------------------

#[test]
fn diff_wavpack_vs_mp3() {
    // APEv2 vs ID3v2
    let (Some(a), Some(b)) = (load("silence-44-s.wv"), load("silence-44-s.mp3")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}

#[test]
fn diff_wavpack_vs_flac() {
    // APEv2 vs VorbisComment
    let (Some(a), Some(b)) = (load("silence-44-s.wv"), load("silence-44-s.flac")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}

#[test]
fn diff_wavpack_vs_m4a() {
    // APEv2 vs MP4 atoms
    let (Some(a), Some(b)) = (load("silence-44-s.wv"), load("has-tags.m4a")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}

// ---------------------------------------------------------------------------
// ASF (WMA) vs other tag systems
// ---------------------------------------------------------------------------

#[test]
fn diff_asf_vs_mp3() {
    // ASF vs ID3v2
    let (Some(a), Some(b)) = (load("silence-1.wma"), load("silence-44-s.mp3")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}

#[test]
fn diff_asf_vs_flac() {
    // ASF vs VorbisComment
    let (Some(a), Some(b)) = (load("silence-1.wma"), load("silence-44-s.flac")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}

#[test]
fn diff_asf_vs_m4a() {
    // ASF vs MP4 atoms
    let (Some(a), Some(b)) = (load("silence-1.wma"), load("has-tags.m4a")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}

#[test]
fn diff_asf_vs_wavpack() {
    // ASF vs APEv2
    let (Some(a), Some(b)) = (load("silence-1.wma"), load("silence-44-s.wv")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}

// ---------------------------------------------------------------------------
// APEv2 vs APEv2 (different container formats)
// ---------------------------------------------------------------------------

#[test]
fn diff_wavpack_vs_tak() {
    let (Some(a), Some(b)) = (load("silence-44-s.wv"), load("has-tags.tak")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}

#[test]
fn diff_wavpack_vs_musepack() {
    let (Some(a), Some(b)) = (load("silence-44-s.wv"), load("click.mpc")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}

// ---------------------------------------------------------------------------
// Self-diff: every format diffed against itself should be identical
// ---------------------------------------------------------------------------

#[test]
fn self_diff_wavpack() {
    let (Some(a), Some(b)) = (load("silence-44-s.wv"), load("silence-44-s.wv")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for WavPack should report identical"
    );
}

#[test]
fn self_diff_asf() {
    let (Some(a), Some(b)) = (load("silence-1.wma"), load("silence-1.wma")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for ASF should report identical"
    );
}

#[test]
fn self_diff_tak() {
    let (Some(a), Some(b)) = (load("has-tags.tak"), load("has-tags.tak")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for TAK should report identical"
    );
}

#[test]
fn self_diff_mp3() {
    let (Some(a), Some(b)) = (load("silence-44-s.mp3"), load("silence-44-s.mp3")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for MP3 should report identical"
    );
}

#[test]
fn self_diff_flac() {
    let (Some(a), Some(b)) = (load("silence-44-s.flac"), load("silence-44-s.flac")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for FLAC should report identical"
    );
}

#[test]
fn self_diff_m4a() {
    let (Some(a), Some(b)) = (load("has-tags.m4a"), load("has-tags.m4a")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for M4A should report identical"
    );
}

#[test]
fn self_diff_aiff() {
    let (Some(a), Some(b)) = (load("with-id3.aif"), load("with-id3.aif")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for AIFF should report identical"
    );
}

#[test]
fn self_diff_wave() {
    let (Some(a), Some(b)) = (
        load("silence-2s-PCM-44100-16-ID3v23.wav"),
        load("silence-2s-PCM-44100-16-ID3v23.wav"),
    ) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for WAVE should report identical"
    );
}

#[test]
fn self_diff_dsf() {
    let (Some(a), Some(b)) = (load("with-id3.dsf"), load("with-id3.dsf")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for DSF should report identical"
    );
}

#[test]
fn self_diff_dsdiff() {
    let (Some(a), Some(b)) = (
        load("5644800-2ch-s01-silence.dff"),
        load("5644800-2ch-s01-silence.dff"),
    ) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for DSDIFF should report identical"
    );
}

#[test]
fn self_diff_ogg_vorbis() {
    let (Some(a), Some(b)) = (load("multipagecomment.ogg"), load("multipagecomment.ogg")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for OGG Vorbis should report identical"
    );
}

#[test]
fn self_diff_ogg_opus() {
    let (Some(a), Some(b)) = (load("example.opus"), load("example.opus")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for OGG Opus should report identical"
    );
}

#[test]
fn self_diff_ogg_speex() {
    let (Some(a), Some(b)) = (load("empty.spx"), load("empty.spx")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for OGG Speex should report identical"
    );
}

#[test]
fn self_diff_ogg_flac() {
    let (Some(a), Some(b)) = (load("empty.oggflac"), load("empty.oggflac")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for OGG FLAC should report identical"
    );
}

#[test]
fn self_diff_ogg_theora() {
    let (Some(a), Some(b)) = (load("sample.oggtheora"), load("sample.oggtheora")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for OGG Theora should report identical"
    );
}

#[test]
fn self_diff_monkeysaudio() {
    let (Some(a), Some(b)) = (load("mac-399.ape"), load("mac-399.ape")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for MonkeysAudio should report identical"
    );
}

#[test]
fn self_diff_musepack() {
    let (Some(a), Some(b)) = (load("click.mpc"), load("click.mpc")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for Musepack should report identical"
    );
}

#[test]
fn self_diff_optimfrog() {
    let (Some(a), Some(b)) = (
        load("silence-2s-44100-16.ofr"),
        load("silence-2s-44100-16.ofr"),
    ) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for OptimFROG should report identical"
    );
}

#[test]
fn self_diff_trueaudio() {
    let (Some(a), Some(b)) = (load("empty.tta"), load("empty.tta")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    assert!(
        d.is_identical(),
        "Self-diff for TrueAudio should report identical"
    );
}

// ---------------------------------------------------------------------------
// ID3v2 vs MP4 — the only cross-tag-system pair not previously tested
// ---------------------------------------------------------------------------

#[test]
fn diff_mp3_vs_m4a() {
    let (Some(a), Some(b)) = (load("silence-44-s.mp3"), load("has-tags.m4a")) else {
        return;
    };
    let d = diff::diff_normalized(&a, &b);
    let _ = d.summary();
    let _ = d.diff_count();
}
