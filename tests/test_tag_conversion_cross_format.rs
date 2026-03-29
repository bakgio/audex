//! Cross-format tag conversion tests for format pairs not covered
//! by the main conversion test suite.
//!
//! Each test sets tags on a source file (via temp copy), converts to
//! a destination file (also a temp copy), and verifies the standard
//! fields survived the mapping. Original test data files are never modified.

mod common;

use audex::tagmap::StandardField;
use audex::{File, FileType};
use common::TestUtils;

// ---------------------------------------------------------------------------
// OGG Vorbis (Vorbis Comment) -> MP3 (ID3v2)
// ---------------------------------------------------------------------------

#[test]
fn ogg_vorbis_to_mp3() {
    let ogg_path = TestUtils::data_path("multipagecomment.ogg");
    let tmp_ogg = TestUtils::get_temp_copy(&ogg_path).expect("temp ogg");
    let mut ogg = audex::oggvorbis::OggVorbis::load(tmp_ogg.path()).expect("load OGG");

    ogg.set("TITLE", vec!["Vorbis Title".to_string()]).unwrap();
    ogg.set("ARTIST", vec!["Vorbis Artist".to_string()])
        .unwrap();
    ogg.set("ALBUM", vec!["Vorbis Album".to_string()]).unwrap();
    ogg.save().unwrap();

    let source = File::load(tmp_ogg.path()).expect("reload OGG");

    let mp3_path = TestUtils::data_path("no-tags.mp3");
    let tmp_mp3 = TestUtils::get_temp_copy(&mp3_path).expect("temp mp3");
    let mut dest = File::load(tmp_mp3.path()).expect("load MP3");

    let report = audex::convert_tags(&source, &mut dest).expect("convert OGG -> MP3");
    assert!(report.transferred.contains(&StandardField::Title));
    assert!(report.transferred.contains(&StandardField::Artist));
    assert!(report.transferred.contains(&StandardField::Album));

    let dest_map = dest.to_tag_map();
    assert_eq!(
        dest_map.get(&StandardField::Title),
        Some(["Vorbis Title".to_string()].as_slice())
    );
    assert_eq!(
        dest_map.get(&StandardField::Artist),
        Some(["Vorbis Artist".to_string()].as_slice())
    );
}

// ---------------------------------------------------------------------------
// MP3 (ID3v2) -> ASF (WMA metadata)
// ---------------------------------------------------------------------------

#[test]
fn mp3_to_asf() {
    let mp3_path = TestUtils::data_path("silence-44-s.mp3");
    let tmp_mp3 = TestUtils::get_temp_copy(&mp3_path).expect("temp mp3");
    let mut mp3 = audex::mp3::MP3::load(tmp_mp3.path()).expect("load MP3");

    mp3.set("TIT2", vec!["ID3 Title".to_string()]).unwrap();
    mp3.set("TPE1", vec!["ID3 Artist".to_string()]).unwrap();
    mp3.save().unwrap();

    let source = File::load(tmp_mp3.path()).expect("reload MP3");

    let wma_path = TestUtils::data_path("silence-1.wma");
    let tmp_wma = TestUtils::get_temp_copy(&wma_path).expect("temp wma");
    let mut dest = File::load(tmp_wma.path()).expect("load ASF");

    let report = audex::convert_tags(&source, &mut dest).expect("convert MP3 -> ASF");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from ID3 to ASF"
    );
    assert!(report.transferred.contains(&StandardField::Artist));

    let dest_map = dest.to_tag_map();
    assert_eq!(
        dest_map.get(&StandardField::Title),
        Some(["ID3 Title".to_string()].as_slice())
    );
}

// ---------------------------------------------------------------------------
// ASF (WMA metadata) -> FLAC (Vorbis Comment)
// ---------------------------------------------------------------------------

#[test]
fn asf_to_flac() {
    let wma_path = TestUtils::data_path("silence-1.wma");
    let tmp_wma = TestUtils::get_temp_copy(&wma_path).expect("temp wma");
    let source = File::load(tmp_wma.path()).expect("load ASF");
    let source_map = source.to_tag_map();

    // Verify the source has at least a title
    assert!(
        source_map.get(&StandardField::Title).is_some() || !source_map.standard_fields().is_empty(),
        "ASF source should have some standard fields"
    );

    let flac_path = TestUtils::data_path("no-tags.flac");
    let tmp_flac = TestUtils::get_temp_copy(&flac_path).expect("temp flac");
    let mut dest = File::load(tmp_flac.path()).expect("load FLAC");

    let report = audex::convert_tags(&source, &mut dest).expect("convert ASF -> FLAC");

    // At least some fields should transfer
    assert!(
        !report.transferred.is_empty(),
        "ASF -> FLAC should transfer at least one standard field"
    );

    let dest_map = dest.to_tag_map();
    for field in &report.transferred {
        assert_eq!(
            source_map.get(field),
            dest_map.get(field),
            "Field {:?} should survive ASF -> Vorbis Comment mapping",
            field
        );
    }
}

// ---------------------------------------------------------------------------
// MP4 (iTunes atoms) -> MP3 (ID3v2)
// ---------------------------------------------------------------------------

#[test]
fn mp4_to_mp3() {
    let mp4_path = TestUtils::data_path("has-tags.m4a");
    let tmp_mp4 = TestUtils::get_temp_copy(&mp4_path).expect("temp m4a");
    let source = File::load(tmp_mp4.path()).expect("load MP4");
    let source_map = source.to_tag_map();

    let mp3_path = TestUtils::data_path("no-tags.mp3");
    let tmp_mp3 = TestUtils::get_temp_copy(&mp3_path).expect("temp mp3");
    let mut dest = File::load(tmp_mp3.path()).expect("load MP3");

    let report = audex::convert_tags(&source, &mut dest).expect("convert MP4 -> MP3");

    assert!(
        !report.transferred.is_empty(),
        "MP4 -> MP3 should transfer at least one standard field"
    );

    let dest_map = dest.to_tag_map();
    for field in &report.transferred {
        assert_eq!(
            source_map.get(field),
            dest_map.get(field),
            "Field {:?} should survive iTunes -> ID3v2 mapping",
            field
        );
    }
}

// ---------------------------------------------------------------------------
// APEv2 (via WavPack) -> MP4 (iTunes atoms)
// ---------------------------------------------------------------------------

#[test]
fn apev2_to_mp4() {
    let wv_path = TestUtils::data_path("silence-44-s.wv");
    let tmp_wv = TestUtils::get_temp_copy(&wv_path).expect("temp wv");
    let mut wv = File::load(tmp_wv.path()).expect("load WavPack");

    wv.set("Title", vec!["APE Title".to_string()]).unwrap();
    wv.set("Artist", vec!["APE Artist".to_string()]).unwrap();
    wv.set("Album", vec!["APE Album".to_string()]).unwrap();
    wv.save().unwrap();

    let source = File::load(tmp_wv.path()).expect("reload WavPack");

    let mp4_path = TestUtils::data_path("no-tags.m4a");
    let tmp_mp4 = TestUtils::get_temp_copy(&mp4_path).expect("temp m4a");
    let mut dest = File::load(tmp_mp4.path()).expect("load MP4");

    let report = audex::convert_tags(&source, &mut dest).expect("convert APEv2 -> MP4");
    assert!(report.transferred.contains(&StandardField::Title));
    assert!(report.transferred.contains(&StandardField::Artist));
    assert!(report.transferred.contains(&StandardField::Album));

    let dest_map = dest.to_tag_map();
    assert_eq!(
        dest_map.get(&StandardField::Title),
        Some(["APE Title".to_string()].as_slice())
    );
}

// ---------------------------------------------------------------------------
// OGG Vorbis (Vorbis Comment) -> MP4 (iTunes atoms)
// ---------------------------------------------------------------------------

#[test]
fn ogg_vorbis_to_mp4() {
    let ogg_path = TestUtils::data_path("multipagecomment.ogg");
    let tmp_ogg = TestUtils::get_temp_copy(&ogg_path).expect("temp ogg");
    let mut ogg = audex::oggvorbis::OggVorbis::load(tmp_ogg.path()).expect("load OGG");

    ogg.set("TITLE", vec!["Vorbis Title".to_string()]).unwrap();
    ogg.set("ARTIST", vec!["Vorbis Artist".to_string()])
        .unwrap();
    ogg.set("TRACKNUMBER", vec!["7".to_string()]).unwrap();
    ogg.save().unwrap();

    let source = File::load(tmp_ogg.path()).expect("reload OGG");

    let mp4_path = TestUtils::data_path("no-tags.m4a");
    let tmp_mp4 = TestUtils::get_temp_copy(&mp4_path).expect("temp m4a");
    let mut dest = File::load(tmp_mp4.path()).expect("load MP4");

    let report = audex::convert_tags(&source, &mut dest).expect("convert OGG -> MP4");
    assert!(report.transferred.contains(&StandardField::Title));
    assert!(report.transferred.contains(&StandardField::Artist));

    let dest_map = dest.to_tag_map();
    assert_eq!(
        dest_map.get(&StandardField::Title),
        Some(["Vorbis Title".to_string()].as_slice())
    );
    assert_eq!(
        dest_map.get(&StandardField::Artist),
        Some(["Vorbis Artist".to_string()].as_slice())
    );
}
