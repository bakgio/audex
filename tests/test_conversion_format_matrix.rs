/// Additional cross-format tag conversion tests.
///
/// The existing test_tag_conversion_cross_format.rs covers 6 pairs but never
/// tests APEv2 as a destination. This file fills the remaining gaps so that
/// every tag system (ID3v2, VorbisComment, MP4, APEv2, ASF) has been tested
/// as both source and destination.
///
/// All modifications happen on temporary copies. Original test data is never
/// modified.
mod common;

use audex::File;
use audex::tagmap::StandardField;
use common::TestUtils;
use std::io::{Seek, SeekFrom, Write};
use tempfile::NamedTempFile;

/// Create a temp copy preserving the file extension (needed for format detection).
fn temp_copy(filename: &str) -> NamedTempFile {
    let src = TestUtils::data_path(filename);
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let suffix = format!(".{}", ext);
    let mut tmp = NamedTempFile::with_suffix(&suffix).expect("create temp file");
    let data = std::fs::read(&src).expect("read source file");
    tmp.write_all(&data).expect("write temp");
    tmp.flush().expect("flush");
    tmp.seek(SeekFrom::Start(0)).expect("seek");
    tmp
}

// ---------------------------------------------------------------------------
// ID3v2 -> APEv2 (MP3 -> WavPack) — APEv2 as destination
// ---------------------------------------------------------------------------

#[test]
fn mp3_to_wavpack() {
    let src_tmp = temp_copy("silence-44-s.mp3");
    let mut src = File::load(src_tmp.path()).expect("load MP3");
    src.set("title", vec!["MP3 Title".into()]).ok();
    src.set("artist", vec!["MP3 Artist".into()]).ok();
    src.save().unwrap();
    let source = File::load(src_tmp.path()).expect("reload MP3");

    let dst_tmp = temp_copy("silence-44-s.wv");
    let mut dest = File::load(dst_tmp.path()).expect("load WavPack");
    dest.add_tags().ok();

    let report = audex::convert_tags(&source, &mut dest).expect("convert MP3 -> WavPack");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from ID3v2 to APEv2"
    );
}

// ---------------------------------------------------------------------------
// VorbisComment -> APEv2 (FLAC -> WavPack)
// ---------------------------------------------------------------------------

#[test]
fn flac_to_wavpack() {
    let src_tmp = temp_copy("silence-44-s.flac");
    let mut src = File::load(src_tmp.path()).expect("load FLAC");
    src.set("title", vec!["FLAC Title".into()]).ok();
    src.set("artist", vec!["FLAC Artist".into()]).ok();
    src.save().unwrap();
    let source = File::load(src_tmp.path()).expect("reload FLAC");

    let dst_tmp = temp_copy("silence-44-s.wv");
    let mut dest = File::load(dst_tmp.path()).expect("load WavPack");
    dest.add_tags().ok();

    let report = audex::convert_tags(&source, &mut dest).expect("convert FLAC -> WavPack");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from VorbisComment to APEv2"
    );
}

// ---------------------------------------------------------------------------
// APEv2 -> VorbisComment (WavPack -> FLAC)
// ---------------------------------------------------------------------------

#[test]
fn wavpack_to_flac() {
    let src_tmp = temp_copy("silence-44-s.wv");
    let mut src = File::load(src_tmp.path()).expect("load WavPack");
    src.add_tags().ok();
    src.set("title", vec!["WV Title".into()]).ok();
    src.set("artist", vec!["WV Artist".into()]).ok();
    src.save().unwrap();
    let source = File::load(src_tmp.path()).expect("reload WavPack");

    let dst_tmp = temp_copy("silence-44-s.flac");
    let mut dest = File::load(dst_tmp.path()).expect("load FLAC");

    let report = audex::convert_tags(&source, &mut dest).expect("convert WavPack -> FLAC");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from APEv2 to VorbisComment"
    );
}

// ---------------------------------------------------------------------------
// APEv2 -> ID3v2 (WavPack -> MP3)
// ---------------------------------------------------------------------------

#[test]
fn wavpack_to_mp3() {
    let src_tmp = temp_copy("silence-44-s.wv");
    let mut src = File::load(src_tmp.path()).expect("load WavPack");
    src.add_tags().ok();
    src.set("title", vec!["WV Title".into()]).ok();
    src.save().unwrap();
    let source = File::load(src_tmp.path()).expect("reload WavPack");

    let dst_tmp = temp_copy("silence-44-s.mp3");
    let mut dest = File::load(dst_tmp.path()).expect("load MP3");

    let report = audex::convert_tags(&source, &mut dest).expect("convert WavPack -> MP3");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from APEv2 to ID3v2"
    );
}

// ---------------------------------------------------------------------------
// APEv2 -> ASF (WavPack -> WMA)
// ---------------------------------------------------------------------------

#[test]
fn wavpack_to_asf() {
    let src_tmp = temp_copy("silence-44-s.wv");
    let mut src = File::load(src_tmp.path()).expect("load WavPack");
    src.add_tags().ok();
    src.set("title", vec!["WV Title".into()]).ok();
    src.save().unwrap();
    let source = File::load(src_tmp.path()).expect("reload WavPack");

    let dst_tmp = temp_copy("silence-1.wma");
    let mut dest = File::load(dst_tmp.path()).expect("load ASF");

    let report = audex::convert_tags(&source, &mut dest).expect("convert WavPack -> ASF");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from APEv2 to ASF"
    );
}

// ---------------------------------------------------------------------------
// MP4 -> VorbisComment (M4A -> FLAC)
// ---------------------------------------------------------------------------

#[test]
fn mp4_to_flac() {
    let src_tmp = temp_copy("has-tags.m4a");
    let source = File::load(src_tmp.path()).expect("load M4A");

    let dst_tmp = temp_copy("silence-44-s.flac");
    let mut dest = File::load(dst_tmp.path()).expect("load FLAC");

    let report = audex::convert_tags(&source, &mut dest).expect("convert M4A -> FLAC");
    // M4A has-tags.m4a should have at least one standard field to transfer
    assert!(
        !report.transferred.is_empty(),
        "At least one field should transfer from MP4 to VorbisComment"
    );
}

// ---------------------------------------------------------------------------
// ID3v2 -> VorbisComment (MP3 -> FLAC)
// ---------------------------------------------------------------------------

#[test]
fn mp3_to_flac() {
    let src_tmp = temp_copy("silence-44-s.mp3");
    let mut src = File::load(src_tmp.path()).expect("load MP3");
    src.set("title", vec!["ID3 Title".into()]).ok();
    src.save().unwrap();
    let source = File::load(src_tmp.path()).expect("reload MP3");

    let dst_tmp = temp_copy("silence-44-s.flac");
    let mut dest = File::load(dst_tmp.path()).expect("load FLAC");

    let report = audex::convert_tags(&source, &mut dest).expect("convert MP3 -> FLAC");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from ID3v2 to VorbisComment"
    );
}

// ---------------------------------------------------------------------------
// ID3v2 -> MP4 (MP3 -> M4A)
// ---------------------------------------------------------------------------

#[test]
fn mp3_to_m4a() {
    let src_tmp = temp_copy("silence-44-s.mp3");
    let mut src = File::load(src_tmp.path()).expect("load MP3");
    src.set("title", vec!["ID3 Title".into()]).ok();
    src.set("artist", vec!["ID3 Artist".into()]).ok();
    src.save().unwrap();
    let source = File::load(src_tmp.path()).expect("reload MP3");

    let dst_tmp = temp_copy("no-tags.m4a");
    let mut dest = File::load(dst_tmp.path()).expect("load M4A");

    let report = audex::convert_tags(&source, &mut dest).expect("convert MP3 -> M4A");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from ID3v2 to MP4"
    );
}

// ---------------------------------------------------------------------------
// VorbisComment -> ASF (FLAC -> WMA)
// ---------------------------------------------------------------------------

#[test]
fn flac_to_asf() {
    let src_tmp = temp_copy("silence-44-s.flac");
    let mut src = File::load(src_tmp.path()).expect("load FLAC");
    src.set("title", vec!["FLAC Title".into()]).ok();
    src.set("artist", vec!["FLAC Artist".into()]).ok();
    src.save().unwrap();
    let source = File::load(src_tmp.path()).expect("reload FLAC");

    let dst_tmp = temp_copy("silence-1.wma");
    let mut dest = File::load(dst_tmp.path()).expect("load ASF");

    let report = audex::convert_tags(&source, &mut dest).expect("convert FLAC -> ASF");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from VorbisComment to ASF"
    );
}

// ---------------------------------------------------------------------------
// MP4 -> APEv2 (M4A -> WavPack)
// ---------------------------------------------------------------------------

#[test]
fn m4a_to_wavpack() {
    let src_tmp = temp_copy("has-tags.m4a");
    let source = File::load(src_tmp.path()).expect("load M4A");

    let dst_tmp = temp_copy("silence-44-s.wv");
    let mut dest = File::load(dst_tmp.path()).expect("load WavPack");
    dest.add_tags().ok();

    let report = audex::convert_tags(&source, &mut dest).expect("convert M4A -> WavPack");
    assert!(
        !report.transferred.is_empty(),
        "At least one field should transfer from MP4 to APEv2"
    );
}

// ---------------------------------------------------------------------------
// MP4 -> ASF (M4A -> WMA)
// ---------------------------------------------------------------------------

#[test]
fn m4a_to_asf() {
    let src_tmp = temp_copy("has-tags.m4a");
    let source = File::load(src_tmp.path()).expect("load M4A");

    let dst_tmp = temp_copy("silence-1.wma");
    let mut dest = File::load(dst_tmp.path()).expect("load ASF");

    let report = audex::convert_tags(&source, &mut dest).expect("convert M4A -> ASF");
    assert!(
        !report.transferred.is_empty(),
        "At least one field should transfer from MP4 to ASF"
    );
}

// ---------------------------------------------------------------------------
// ASF -> ID3v2 (WMA -> MP3)
// ---------------------------------------------------------------------------

#[test]
fn asf_to_mp3() {
    let src_tmp = temp_copy("silence-1.wma");
    let source = File::load(src_tmp.path()).expect("load ASF");

    let dst_tmp = temp_copy("no-tags.mp3");
    let mut dest = File::load(dst_tmp.path()).expect("load MP3");

    let report = audex::convert_tags(&source, &mut dest).expect("convert ASF -> MP3");
    assert!(
        !report.transferred.is_empty(),
        "At least one field should transfer from ASF to ID3v2"
    );
}

// ---------------------------------------------------------------------------
// ASF -> MP4 (WMA -> M4A)
// ---------------------------------------------------------------------------

#[test]
fn asf_to_m4a() {
    let src_tmp = temp_copy("silence-1.wma");
    let source = File::load(src_tmp.path()).expect("load ASF");

    let dst_tmp = temp_copy("no-tags.m4a");
    let mut dest = File::load(dst_tmp.path()).expect("load M4A");

    let report = audex::convert_tags(&source, &mut dest).expect("convert ASF -> M4A");
    assert!(
        !report.transferred.is_empty(),
        "At least one field should transfer from ASF to MP4"
    );
}

// ---------------------------------------------------------------------------
// ASF -> APEv2 (WMA -> WavPack)
// ---------------------------------------------------------------------------

#[test]
fn asf_to_wavpack() {
    let src_tmp = temp_copy("silence-1.wma");
    let source = File::load(src_tmp.path()).expect("load ASF");

    let dst_tmp = temp_copy("silence-44-s.wv");
    let mut dest = File::load(dst_tmp.path()).expect("load WavPack");
    dest.add_tags().ok();

    let report = audex::convert_tags(&source, &mut dest).expect("convert ASF -> WavPack");
    assert!(
        !report.transferred.is_empty(),
        "At least one field should transfer from ASF to APEv2"
    );
}
