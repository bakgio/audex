//! End-to-end tag conversion tests using real audio files.

mod common;

use audex::tagmap::{ConversionOptions, StandardField};
use audex::{File, FileType};
use common::TestUtils;

// ---------------------------------------------------------------------------
// MP3 (ID3v2) -> FLAC (Vorbis Comment)
// ---------------------------------------------------------------------------

#[test]
fn test_tag_conversion_mp3_to_flac_extraction() {
    // Load an MP3 with tags and verify we can extract a TagMap
    let path = TestUtils::data_path("silence-44-s.mp3");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mp3 = File::load(tmp.path()).expect("load MP3");
    let tag_map = mp3.to_tag_map();
    // Should not panic — may or may not have tags
    let _ = tag_map.standard_fields();
}

#[test]
fn test_tag_conversion_flac_round_trip() {
    // Load a FLAC, set tags via the normal interface, extract to TagMap, verify
    let path = TestUtils::data_path("silence-44-s.flac");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut flac = audex::flac::FLAC::load(tmp.path()).expect("load FLAC");

    flac.set("TITLE", vec!["Test Title".to_string()]).unwrap();
    flac.set("ARTIST", vec!["Test Artist".to_string()]).unwrap();
    flac.set("ALBUM", vec!["Test Album".to_string()]).unwrap();
    flac.set("TRACKNUMBER", vec!["5".to_string()]).unwrap();
    flac.set("GENRE", vec!["Rock".to_string()]).unwrap();
    flac.save().unwrap();

    // Reload and convert to TagMap
    let loaded = File::load(tmp.path()).expect("reload FLAC");
    let tag_map = loaded.to_tag_map();

    assert_eq!(
        tag_map.get(&StandardField::Title),
        Some(["Test Title".to_string()].as_slice())
    );
    assert_eq!(
        tag_map.get(&StandardField::Artist),
        Some(["Test Artist".to_string()].as_slice())
    );
    assert_eq!(
        tag_map.get(&StandardField::Genre),
        Some(["Rock".to_string()].as_slice())
    );
}

#[test]
fn test_tag_conversion_apply_to_flac() {
    // Create a TagMap and apply it to a FLAC file
    let path = TestUtils::data_path("no-tags.flac");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut dest = File::load(tmp.path()).expect("load FLAC");

    let mut tag_map = audex::TagMap::new();
    tag_map.set(StandardField::Title, vec!["Applied Title".to_string()]);
    tag_map.set(StandardField::Artist, vec!["Applied Artist".to_string()]);
    tag_map.set(StandardField::TrackNumber, vec!["3".to_string()]);
    tag_map.set(StandardField::TotalTracks, vec!["12".to_string()]);

    let report = dest.apply_tag_map(&tag_map).expect("apply tag map");
    assert!(!report.transferred.is_empty());

    // Verify the tags were actually set
    let extracted = dest.to_tag_map();
    assert_eq!(
        extracted.get(&StandardField::Title),
        Some(["Applied Title".to_string()].as_slice())
    );
}

// ---------------------------------------------------------------------------
// Conversion with options
// ---------------------------------------------------------------------------

#[test]
fn test_convert_with_options_include() {
    let path = TestUtils::data_path("silence-44-s.flac");
    let tmp_src = TestUtils::get_temp_copy(&path).expect("temp src");
    let mut src = audex::flac::FLAC::load(tmp_src.path()).expect("load src");
    src.set("TITLE", vec!["Title".to_string()]).unwrap();
    src.set("ARTIST", vec!["Artist".to_string()]).unwrap();
    src.set("GENRE", vec!["Rock".to_string()]).unwrap();
    src.save().unwrap();

    let source = File::load(tmp_src.path()).expect("reload src");

    let tmp_dest =
        TestUtils::get_temp_copy(TestUtils::data_path("no-tags.flac")).expect("temp dest");
    let mut dest = File::load(tmp_dest.path()).expect("load dest");

    // Only include Title
    let mut include = std::collections::HashSet::new();
    include.insert(StandardField::Title);

    let options = ConversionOptions {
        include_fields: Some(include),
        ..Default::default()
    };

    let report = audex::convert_tags_with_options(&source, &mut dest, &options).expect("convert");

    // Title should be transferred, Artist and Genre should not
    assert!(report.transferred.contains(&StandardField::Title));
    assert!(!report.transferred.contains(&StandardField::Artist));
    assert!(!report.transferred.contains(&StandardField::Genre));
}

#[test]
fn test_convert_with_options_exclude() {
    let path = TestUtils::data_path("silence-44-s.flac");
    let tmp_src = TestUtils::get_temp_copy(&path).expect("temp src");
    let mut src = audex::flac::FLAC::load(tmp_src.path()).expect("load src");
    src.set("TITLE", vec!["Title".to_string()]).unwrap();
    src.set("ARTIST", vec!["Artist".to_string()]).unwrap();
    src.save().unwrap();

    let source = File::load(tmp_src.path()).expect("reload src");

    let tmp_dest =
        TestUtils::get_temp_copy(TestUtils::data_path("no-tags.flac")).expect("temp dest");
    let mut dest = File::load(tmp_dest.path()).expect("load dest");

    // Exclude Title
    let mut exclude = std::collections::HashSet::new();
    exclude.insert(StandardField::Title);

    let options = ConversionOptions {
        exclude_fields: exclude,
        ..Default::default()
    };

    let report = audex::convert_tags_with_options(&source, &mut dest, &options).expect("convert");

    assert!(!report.transferred.contains(&StandardField::Title));
}

#[test]
fn test_tag_conversion_mp3_to_mp4() {
    // Set tags on MP3, convert to MP4 via TagMap, verify field mapping
    let mp3_path = TestUtils::data_path("silence-44-s.mp3");
    let tmp_mp3 = TestUtils::get_temp_copy(&mp3_path).expect("temp mp3");
    let mut mp3 = audex::mp3::MP3::load(tmp_mp3.path()).expect("load MP3");

    mp3.set("TIT2", vec!["Cross Format Title".to_string()])
        .unwrap();
    mp3.set("TPE1", vec!["Cross Format Artist".to_string()])
        .unwrap();
    mp3.set("TALB", vec!["Cross Format Album".to_string()])
        .unwrap();
    mp3.save().unwrap();

    let source = File::load(tmp_mp3.path()).expect("reload MP3");

    let mp4_path = TestUtils::data_path("no-tags.m4a");
    let tmp_mp4 = TestUtils::get_temp_copy(&mp4_path).expect("temp m4a");
    let mut dest = File::load(tmp_mp4.path()).expect("load MP4");

    let report = audex::convert_tags(&source, &mut dest).expect("convert MP3 -> MP4");
    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer from ID3 TPE1 to MP4 ©nam"
    );
    assert!(report.transferred.contains(&StandardField::Artist));

    let extracted = dest.to_tag_map();
    assert_eq!(
        extracted.get(&StandardField::Title),
        Some(["Cross Format Title".to_string()].as_slice())
    );
    assert_eq!(
        extracted.get(&StandardField::Artist),
        Some(["Cross Format Artist".to_string()].as_slice())
    );
}

#[test]
fn test_tag_conversion_mp4_to_flac() {
    let mp4_path = TestUtils::data_path("has-tags.m4a");
    let tmp_mp4 = TestUtils::get_temp_copy(&mp4_path).expect("temp m4a");
    let source = File::load(tmp_mp4.path()).expect("load MP4");
    let source_map = source.to_tag_map();

    let flac_path = TestUtils::data_path("no-tags.flac");
    let tmp_flac = TestUtils::get_temp_copy(&flac_path).expect("temp flac");
    let mut dest = File::load(tmp_flac.path()).expect("load FLAC");

    let report = audex::convert_tags(&source, &mut dest).expect("convert MP4 -> FLAC");

    // Every field the source had should appear in transferred or skipped
    let dest_map = dest.to_tag_map();
    for field in &report.transferred {
        let src_vals = source_map.get(field);
        let dst_vals = dest_map.get(field);
        assert_eq!(
            src_vals, dst_vals,
            "Field {:?} should survive MP4 -> Vorbis Comment mapping",
            field
        );
    }
}

#[test]
fn test_tag_conversion_flac_to_mp3() {
    let flac_path = TestUtils::data_path("silence-44-s.flac");
    let tmp_flac = TestUtils::get_temp_copy(&flac_path).expect("temp flac");
    let mut flac = audex::flac::FLAC::load(tmp_flac.path()).expect("load FLAC");

    flac.set("TITLE", vec!["Vorbis Title".to_string()]).unwrap();
    flac.set("ARTIST", vec!["Vorbis Artist".to_string()])
        .unwrap();
    flac.set("ALBUM", vec!["Vorbis Album".to_string()]).unwrap();
    flac.save().unwrap();

    let source = File::load(tmp_flac.path()).expect("reload FLAC");

    let mp3_path = TestUtils::data_path("no-tags.mp3");
    let tmp_mp3 = TestUtils::get_temp_copy(&mp3_path).expect("temp mp3");
    let mut dest = File::load(tmp_mp3.path()).expect("load MP3");

    let report = audex::convert_tags(&source, &mut dest).expect("convert FLAC -> MP3");
    assert!(report.transferred.contains(&StandardField::Title));
    assert!(report.transferred.contains(&StandardField::Artist));
    assert!(report.transferred.contains(&StandardField::Album));

    let dest_map = dest.to_tag_map();
    assert_eq!(
        dest_map.get(&StandardField::Title),
        Some(["Vorbis Title".to_string()].as_slice())
    );
}

#[test]
fn test_tag_conversion_apev2_to_flac() {
    let wv_path = TestUtils::data_path("silence-44-s.wv");
    let tmp_wv = TestUtils::get_temp_copy(&wv_path).expect("temp wv");
    let mut wv = File::load(tmp_wv.path()).expect("load WavPack");

    wv.set("Title", vec!["APE Title".to_string()]).unwrap();
    wv.set("Artist", vec!["APE Artist".to_string()]).unwrap();
    wv.save().unwrap();

    let source = File::load(tmp_wv.path()).expect("reload WavPack");

    let flac_path = TestUtils::data_path("no-tags.flac");
    let tmp_flac = TestUtils::get_temp_copy(&flac_path).expect("temp flac");
    let mut dest = File::load(tmp_flac.path()).expect("load FLAC");

    let report = audex::convert_tags(&source, &mut dest).expect("convert APEv2 -> FLAC");
    assert!(report.transferred.contains(&StandardField::Title));
    assert!(report.transferred.contains(&StandardField::Artist));

    let dest_map = dest.to_tag_map();
    assert_eq!(
        dest_map.get(&StandardField::Title),
        Some(["APE Title".to_string()].as_slice())
    );
}

#[test]
fn test_conversion_report_skipped_fields() {
    // ReplayGain and Year fields have no MP4 mapping -- they should appear in skipped
    let mut tag_map = audex::TagMap::new();
    tag_map.set(StandardField::Title, vec!["T".to_string()]);
    tag_map.set(
        StandardField::ReplayGainTrackGain,
        vec!["+3.50 dB".to_string()],
    );
    tag_map.set(StandardField::Year, vec!["2024".to_string()]);

    let mp4_path = TestUtils::data_path("no-tags.m4a");
    let tmp = TestUtils::get_temp_copy(&mp4_path).expect("temp m4a");
    let mut dest = File::load(tmp.path()).expect("load MP4");

    let report = dest.apply_tag_map(&tag_map).expect("apply");

    assert!(
        report.transferred.contains(&StandardField::Title),
        "Title should transfer to MP4"
    );

    let skipped_names: Vec<&str> = report.skipped.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        skipped_names.iter().any(|n| n.contains("ReplayGain")),
        "ReplayGain field should be skipped for MP4, got: {:?}",
        skipped_names
    );
}

#[test]
fn test_convert_transfer_custom_true() {
    let flac_path = TestUtils::data_path("silence-44-s.flac");
    let tmp_src = TestUtils::get_temp_copy(&flac_path).expect("temp src");
    let mut src = audex::flac::FLAC::load(tmp_src.path()).expect("load src");

    src.set("TITLE", vec!["Title".to_string()]).unwrap();
    src.set("MYCUSTOMFIELD", vec!["custom value".to_string()])
        .unwrap();
    src.save().unwrap();

    let source = File::load(tmp_src.path()).expect("reload src");
    let tmp_dest =
        TestUtils::get_temp_copy(TestUtils::data_path("no-tags.flac")).expect("temp dest");
    let mut dest = File::load(tmp_dest.path()).expect("load dest");

    let options = ConversionOptions {
        transfer_custom: true,
        ..Default::default()
    };
    let report = audex::convert_tags_with_options(&source, &mut dest, &options).expect("convert");

    assert!(
        !report.custom_transferred.is_empty(),
        "Custom fields should be transferred when transfer_custom=true"
    );
}

#[test]
fn test_convert_transfer_custom_false() {
    let flac_path = TestUtils::data_path("silence-44-s.flac");
    let tmp_src = TestUtils::get_temp_copy(&flac_path).expect("temp src");
    let mut src = audex::flac::FLAC::load(tmp_src.path()).expect("load src");

    src.set("TITLE", vec!["Title".to_string()]).unwrap();
    src.set("MYCUSTOMFIELD", vec!["custom value".to_string()])
        .unwrap();
    src.save().unwrap();

    let source = File::load(tmp_src.path()).expect("reload src");
    let tmp_dest =
        TestUtils::get_temp_copy(TestUtils::data_path("no-tags.flac")).expect("temp dest");
    let mut dest = File::load(tmp_dest.path()).expect("load dest");

    let options = ConversionOptions {
        transfer_custom: false,
        ..Default::default()
    };
    let report = audex::convert_tags_with_options(&source, &mut dest, &options).expect("convert");

    assert!(
        report.custom_transferred.is_empty(),
        "Custom fields should NOT be transferred when transfer_custom=false"
    );
}

#[test]
fn test_convert_overwrite_false_preserves_existing() {
    // Pre-populate destination with only a title (no artist)
    let tmp_dest =
        TestUtils::get_temp_copy(TestUtils::data_path("no-tags.flac")).expect("temp dest");
    let mut dest_setup = audex::flac::FLAC::load(tmp_dest.path()).expect("load dest");
    dest_setup.add_tags().unwrap();
    dest_setup
        .set("TITLE", vec!["Original Title".to_string()])
        .unwrap();
    dest_setup.save().unwrap();

    // Source has a different title and a new artist
    let tmp_src = TestUtils::get_temp_copy(TestUtils::data_path("no-tags.flac")).expect("temp src");
    let mut src_setup = audex::flac::FLAC::load(tmp_src.path()).expect("load src");
    src_setup.add_tags().unwrap();
    src_setup
        .set("TITLE", vec!["New Title".to_string()])
        .unwrap();
    src_setup
        .set("ARTIST", vec!["New Artist".to_string()])
        .unwrap();
    src_setup.save().unwrap();

    let source = File::load(tmp_src.path()).expect("reload src");
    let mut dest = File::load(tmp_dest.path()).expect("reload dest");

    let options = ConversionOptions {
        overwrite: false,
        ..Default::default()
    };
    audex::convert_tags_with_options(&source, &mut dest, &options).expect("convert");

    let dest_map = dest.to_tag_map();

    // Title already existed -- overwrite=false should preserve the original
    assert_eq!(
        dest_map.get(&StandardField::Title),
        Some(["Original Title".to_string()].as_slice()),
        "overwrite=false should preserve existing Title"
    );
    // Artist was new -- should be written
    assert_eq!(
        dest_map.get(&StandardField::Artist),
        Some(["New Artist".to_string()].as_slice()),
        "Artist should be transferred since it didn't exist in destination"
    );
}

#[test]
fn test_convert_clear_destination() {
    // Destination has TITLE and GENRE pre-populated
    let tmp_dest =
        TestUtils::get_temp_copy(TestUtils::data_path("no-tags.flac")).expect("temp dest");
    let mut dest_setup = audex::flac::FLAC::load(tmp_dest.path()).expect("load dest");
    dest_setup.add_tags().unwrap();
    dest_setup
        .set("TITLE", vec!["Old Title".to_string()])
        .unwrap();
    dest_setup
        .set("GENRE", vec!["Old Genre".to_string()])
        .unwrap();
    dest_setup.save().unwrap();

    // Source has only an artist
    let tmp_src = TestUtils::get_temp_copy(TestUtils::data_path("no-tags.flac")).expect("temp src");
    let mut src_setup = audex::flac::FLAC::load(tmp_src.path()).expect("load src");
    src_setup.add_tags().unwrap();
    src_setup
        .set("ARTIST", vec!["Source Artist".to_string()])
        .unwrap();
    src_setup.save().unwrap();

    let source = File::load(tmp_src.path()).expect("reload src");
    let mut dest = File::load(tmp_dest.path()).expect("reload dest");

    let options = ConversionOptions {
        clear_destination: true,
        ..Default::default()
    };
    audex::convert_tags_with_options(&source, &mut dest, &options).expect("convert");

    let dest_map = dest.to_tag_map();

    // Old tags should be gone after clear
    assert!(
        dest_map.get(&StandardField::Title).is_none(),
        "Title should be cleared before import"
    );
    assert!(
        dest_map.get(&StandardField::Genre).is_none(),
        "Genre should be cleared before import"
    );
    // New artist should be present
    assert_eq!(
        dest_map.get(&StandardField::Artist),
        Some(["Source Artist".to_string()].as_slice()),
    );
}

#[test]
fn test_conversion_report_accuracy() {
    let mut tag_map = audex::TagMap::new();
    tag_map.set(StandardField::Title, vec!["T".to_string()]);
    tag_map.set(StandardField::Artist, vec!["A".to_string()]);
    // Barcode maps to Vorbis "UPC" — should be transferred
    tag_map.set(StandardField::Barcode, vec!["123".to_string()]);

    let path = TestUtils::data_path("no-tags.flac");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp");
    let mut dest = File::load(tmp.path()).expect("load");

    let report = dest.apply_tag_map(&tag_map).expect("apply");

    // Title, Artist, and Barcode should all be transferred
    assert!(report.transferred.contains(&StandardField::Title));
    assert!(report.transferred.contains(&StandardField::Artist));
    assert!(
        report.transferred.contains(&StandardField::Barcode),
        "Barcode should be transferred (maps to Vorbis UPC)"
    );
}
