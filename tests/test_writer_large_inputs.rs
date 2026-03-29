mod common;

use audex::FileType;
use audex::id3::file::clear_from_writer;
use common::TestUtils;
use std::io::Cursor;

const LARGE_WRITER_INPUT_BYTES: usize = 20 * 1024 * 1024;

#[test]
fn id3_clear_from_writer_accepts_large_inputs_without_tags() {
    let data = vec![0u8; LARGE_WRITER_INPUT_BYTES];
    let mut writer = Cursor::new(data);

    clear_from_writer(&mut writer, false, true)
        .expect("large inputs without ID3 headers should be handled as a no-op");
}

#[test]
fn large_vorbis_comment_write_many_tags() {
    let path = TestUtils::data_path("silence-44-s.flac");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut flac = audex::flac::FLAC::load(tmp.path()).expect("load FLAC");

    // Write 150 tags with 1 KB values each (~150 KB total metadata)
    let value = "X".repeat(1024);
    for i in 0..150 {
        let key = format!("CUSTOMTAG{:03}", i);
        flac.set(&key, vec![value.clone()]).expect("set tag");
    }
    flac.save().expect("save with many tags");

    let reloaded = audex::flac::FLAC::load(tmp.path()).expect("reload");
    assert_eq!(
        reloaded.get_first("CUSTOMTAG000"),
        Some(value.clone()),
        "first tag should survive save"
    );
    assert_eq!(
        reloaded.get_first("CUSTOMTAG149"),
        Some(value),
        "last tag should survive save"
    );
}

#[test]
fn large_id3v2_text_frame_write() {
    let path = TestUtils::data_path("silence-44-s.mp3");
    let tmp = TestUtils::get_temp_copy(&path).expect("temp copy");
    let mut mp3 = audex::mp3::MP3::load(tmp.path()).expect("load MP3");

    // Write a 1 MB text value into a single TXXX frame
    let large_value = "A".repeat(1024 * 1024);
    mp3.set("TXXX:LargeField", vec![large_value.clone()])
        .expect("set large TXXX frame");
    mp3.save().expect("save with 1 MB text frame");

    let reloaded = audex::mp3::MP3::load(tmp.path()).expect("reload");
    assert_eq!(
        reloaded.get_first("TXXX:LargeField"),
        Some(large_value),
        "1 MB text payload should survive save/reload"
    );
}
