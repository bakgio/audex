// Tests for AIFF/WAV chunk parsing loop bounds.
//
// The chunk parsing loops use the declared file_size from the FORM/RIFF
// header as the loop bound. A crafted file with a huge declared size
// but very little actual data will cause the parser to seek far past
// EOF repeatedly before eventually breaking on read errors. The loop
// bound should be clamped to the actual file size.

use std::io::Cursor;

use audex::FileType;
use audex::aiff::AIFFFile;
use audex::wave::RiffFile;

#[test]
fn test_aiff_with_inflated_form_size() {
    // FORM header claiming 0xFFFFFFFF bytes, but the file is only 28 bytes.
    let mut data = Vec::new();
    data.extend_from_slice(b"FORM");
    data.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes()); // huge declared size
    data.extend_from_slice(b"AIFF");

    // One small chunk: "COMM" with 4 bytes of data
    data.extend_from_slice(b"COMM");
    data.extend_from_slice(&4u32.to_be_bytes());
    data.extend_from_slice(&[0u8; 4]);

    let mut cursor = Cursor::new(data);
    let result = AIFFFile::parse(&mut cursor);

    // Should parse successfully with just the one chunk, not hang
    // trying to seek to offsets near 4 GB.
    assert!(
        result.is_ok(),
        "Should parse without hanging: {:?}",
        result.err()
    );
    let aiff = result.unwrap();
    assert_eq!(aiff.chunks.len(), 1);
    assert_eq!(aiff.chunks[0].id, "COMM");
}

#[test]
fn test_wave_with_inflated_riff_size() {
    // RIFF header claiming 0xFFFFFFFF bytes, but the file is only 28 bytes.
    let mut data = Vec::new();
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // huge declared size (LE)
    data.extend_from_slice(b"WAVE");

    // One small chunk: "fmt " with 4 bytes of data
    data.extend_from_slice(b"fmt ");
    data.extend_from_slice(&4u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 4]);

    let mut cursor = Cursor::new(data);
    let result = RiffFile::parse(&mut cursor);

    assert!(
        result.is_ok(),
        "Should parse without hanging: {:?}",
        result.err()
    );
    let wave = result.unwrap();
    assert_eq!(wave.chunks.len(), 1);
    assert_eq!(wave.chunks[0].id, "fmt ");
}

#[test]
fn test_mp4_with_inflated_atom_size() {
    // A minimal MP4 with a "ftyp" atom declaring 0xFFFFFFFF bytes,
    // but only 20 bytes of actual data.
    let mut data = Vec::new();

    // ftyp atom: size=0xFFFFFFFF (inflated), name="ftyp"
    data.extend_from_slice(&0xFFFFFFFFu32.to_be_bytes());
    data.extend_from_slice(b"ftyp");
    data.extend_from_slice(b"isom"); // major brand

    let mut cursor = Cursor::new(data);
    let result = audex::mp4::MP4::load_from_reader(&mut cursor);

    // Should return an error (missing moov), not hang or OOM
    assert!(
        result.is_err(),
        "inflated MP4 atom should produce an error, not hang"
    );
}

#[test]
fn test_asf_with_inflated_object_size() {
    // Minimal ASF header object with inflated size.
    // ASF Header Object GUID: 30 26 B2 75 8E 66 CF 11 A6 D9 00 AA 00 62 CE 6C
    let header_guid: [u8; 16] = [
        0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62, 0xCE,
        0x6C,
    ];

    let mut data = Vec::new();
    data.extend_from_slice(&header_guid);
    // Object size: 0xFFFFFFFFFFFFFFFF (inflated, little-endian)
    data.extend_from_slice(&u64::MAX.to_le_bytes());
    // Number of header objects: 0
    data.extend_from_slice(&0u32.to_le_bytes());
    // Reserved bytes
    data.push(0x01);
    data.push(0x02);

    let mut cursor = Cursor::new(data);
    let result = audex::asf::ASF::load_from_reader(&mut cursor);

    assert!(
        result.is_err(),
        "inflated ASF object should produce an error, not hang"
    );
}

#[test]
fn test_ogg_with_excessive_pages() {
    // Craft an OGG stream where each page header is valid but points nowhere.
    // The parser should eventually terminate (either via EOF or page limit).
    let mut data = Vec::new();

    // Write 50 minimal OGG pages (each page has: capture pattern + header)
    for i in 0u32..50 {
        data.extend_from_slice(b"OggS"); // capture pattern
        data.push(0); // version
        data.push(if i == 0 { 0x02 } else { 0x00 }); // header_type (BOS for first)
        data.extend_from_slice(&[0u8; 8]); // granule position
        data.extend_from_slice(&1u32.to_le_bytes()); // serial number
        data.extend_from_slice(&i.to_le_bytes()); // page sequence number
        data.extend_from_slice(&[0u8; 4]); // checksum (will be wrong, but we test parsing)
        data.push(1); // number of segments
        data.push(4); // segment table: one segment of 4 bytes
        data.extend_from_slice(&[0u8; 4]); // 4 bytes of payload
    }

    let mut cursor = Cursor::new(data);
    let result = audex::oggvorbis::OggVorbis::load_from_reader(&mut cursor);

    // Should fail (not valid Vorbis), but should not hang
    assert!(
        result.is_err(),
        "invalid OGG pages should produce an error, not hang"
    );
}

#[test]
fn test_aiff_normal_file_still_parses() {
    // A well-formed AIFF with accurate size
    let chunk_data = [0u8; 8];
    let chunk_size = chunk_data.len() as u32;
    let form_size = 4 + 8 + chunk_size; // "AIFF" + chunk header + data

    let mut data = Vec::new();
    data.extend_from_slice(b"FORM");
    data.extend_from_slice(&form_size.to_be_bytes());
    data.extend_from_slice(b"AIFF");
    data.extend_from_slice(b"COMM");
    data.extend_from_slice(&chunk_size.to_be_bytes());
    data.extend_from_slice(&chunk_data);

    let mut cursor = Cursor::new(data);
    let result = AIFFFile::parse(&mut cursor);

    assert!(result.is_ok());
    assert_eq!(result.unwrap().chunks.len(), 1);
}
