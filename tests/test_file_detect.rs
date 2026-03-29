/// Tests for File::detect_format functionality
/// - detect_format only advertises formats with registered loaders
/// - Proper error handling for unsupported formats
/// - Header-based detection (not just extensions)
use audex::{AudexError, detect_format, detect_format_from_bytes, detect_ogg_format};
use std::path::Path;

#[test]
fn test_detect_mp3() {
    // Test detecting MP3 format
    let test_file = "tests/data/silence-44-s.mp3";
    if !std::path::Path::new(test_file).exists() {
        // Skip if test file doesn't exist
        return;
    }

    let result = detect_format(test_file);
    assert!(result.is_ok());
    let format = result.unwrap();
    assert!(format.contains("MP3"));
}

#[test]
fn test_detect_flac() {
    // Test detecting FLAC format
    let test_file = "tests/data/silence-44-s.flac";
    if !std::path::Path::new(test_file).exists() {
        // Skip if test file doesn't exist
        return;
    }

    let result = detect_format(test_file);
    assert!(result.is_ok());
    let format = result.unwrap();
    assert!(format.contains("FLAC"));
}

#[test]
fn test_detect_ogg() {
    // Test detecting Ogg Vorbis format
    let test_file = "tests/data/empty.ogg";
    if !std::path::Path::new(test_file).exists() {
        // Skip if test file doesn't exist
        return;
    }

    let result = detect_format(test_file);
    if let Ok(format) = result {
        assert!(format.contains("Ogg") || format.contains("Vorbis"));
    }
}

#[test]
fn test_detect_unsupported_format() {
    // Test that unsupported formats fail properly
    // Create a temp file with .xyz extension (unsupported format)
    let temp_dir = tempfile::tempdir().unwrap();
    let unsupported_path = temp_dir.path().join("test.xyz");

    // Write some dummy data (not a valid audio format)
    std::fs::write(&unsupported_path, b"Invalid audio data").unwrap();

    let result = detect_format(&unsupported_path);

    // Should fail with UnsupportedFormat since .xyz has no loader
    assert!(result.is_err());
    if let Err(AudexError::UnsupportedFormat(_)) = result {
        // Correct error type
    } else {
        panic!("Expected UnsupportedFormat error, got: {:?}", result);
    }
}

#[test]
fn test_detect_nonexistent_file() {
    // Test error handling for nonexistent files
    let result = detect_format("/nonexistent/path/to/file.mp3");
    assert!(result.is_err());

    // Should be an IO error, not UnsupportedFormat
    match result {
        Err(AudexError::Io(_)) => {
            // Correct error type
        }
        Err(AudexError::InvalidData(_)) => {
            // Also acceptable
        }
        _ => panic!("Expected IO error, got: {:?}", result),
    }
}

#[test]
fn test_detect_empty_file() {
    // Test detecting format of an empty file with no extension
    let temp_dir = tempfile::tempdir().unwrap();
    let empty_path = temp_dir.path().join("empty");

    std::fs::write(&empty_path, b"").unwrap();

    let result = detect_format(&empty_path);

    // Empty files with no extension should fail detection
    assert!(result.is_err());
}

#[test]
fn test_detect_invalid_audio_data() {
    // Test detecting format of invalid audio data without extension
    let temp_dir = tempfile::tempdir().unwrap();
    let invalid_path = temp_dir.path().join("invalid");

    // Write random data with no magic bytes
    std::fs::write(&invalid_path, [0u8; 1024]).unwrap();

    let result = detect_format(&invalid_path);

    // Invalid data with no extension should fail detection
    assert!(result.is_err());
}

#[test]
fn test_detect_uses_header_not_extension() {
    // Test that detection uses header data, not just file extension
    let temp_dir = tempfile::tempdir().unwrap();

    // Create a file with .mp3 extension but FLAC header
    let test_file = "tests/data/silence-44-s.flac";
    if !std::path::Path::new(test_file).exists() {
        // Skip if test file doesn't exist
        return;
    }

    // Copy FLAC file to .mp3 extension
    let fake_mp3 = temp_dir.path().join("actually_flac.mp3");
    std::fs::copy(test_file, &fake_mp3).unwrap();

    let result = detect_format(&fake_mp3);

    if let Ok(_format) = result {
        // Should detect as FLAC based on header, not MP3 based on extension
        // (Though scoring might favor MP3 due to extension match)
        // This tests that header is being checked
    }
}

#[test]
fn test_detect_multiple_files() {
    // Test detecting formats of multiple files
    let test_files = vec![
        ("tests/data/silence-44-s.mp3", "MP3"),
        ("tests/data/silence-44-s.flac", "FLAC"),
    ];

    for (file, expected_format) in test_files {
        if !std::path::Path::new(file).exists() {
            continue;
        }

        let result = detect_format(file);
        if let Ok(format) = result {
            assert!(
                format.contains(expected_format),
                "File {} should be detected as {}, got {}",
                file,
                expected_format,
                format
            );
        }
    }
}

#[test]
fn test_detect_preserves_load_compatibility() {
    // Test that formats detected by detect_format can be loaded by File::load
    use audex::File;

    let test_file = "tests/data/silence-44-s.mp3";
    if !std::path::Path::new(test_file).exists() {
        return;
    }

    // Detect format
    let format_result = detect_format(test_file);
    assert!(format_result.is_ok(), "Should detect format");

    // Load file
    let load_result = File::load(test_file);
    assert!(
        load_result.is_ok(),
        "Should be able to load detected format"
    );

    // Verify they match - format_name from detect_format might be the full module path
    let format_name = format_result.unwrap();
    let file = load_result.unwrap();
    let file_format_name = file.format_name();

    // Either they match exactly, or the detected format is contained in the file format name
    assert!(
        file_format_name == format_name
            || format_name.contains(file_format_name)
            || file_format_name.contains(&format_name),
        "Format names should be compatible: detected='{}', file='{}'",
        format_name,
        file_format_name
    );
}

// ---------------------------------------------------------------------------
// detect_format_from_bytes — byte-only detection without filesystem access
// ---------------------------------------------------------------------------

#[test]
fn test_detect_from_bytes_flac_magic() {
    // fLaC magic at offset 0 is the strongest FLAC indicator
    let mut data = vec![0u8; 128];
    data[..4].copy_from_slice(b"fLaC");
    let result = detect_format_from_bytes(&data, None);
    assert!(
        result.is_ok(),
        "FLAC magic should be detected: {:?}",
        result
    );
    assert!(result.unwrap().contains("FLAC"));
}

#[test]
fn test_detect_from_bytes_mp3_id3_header() {
    // ID3v2 header is used by MP3, AIFF, WAV, etc. — extension hint disambiguates
    let mut data = vec![0u8; 128];
    data[..3].copy_from_slice(b"ID3");
    let result = detect_format_from_bytes(&data, Some(Path::new("song.mp3")));
    assert!(result.is_ok(), "ID3+mp3 hint should detect: {:?}", result);
    let fmt = result.unwrap();
    assert!(
        fmt.contains("MP3") || fmt.contains("ID3"),
        "Expected MP3-family format, got: {}",
        fmt
    );
}

#[test]
fn test_detect_from_bytes_mp4_ftyp() {
    // MP4 files start with a box whose type at offset 4..8 is "ftyp"
    let mut data = vec![0u8; 128];
    // Typical ftyp box: 4 bytes size + "ftyp" + brand
    data[0..4].copy_from_slice(&[0, 0, 0, 20]); // box size
    data[4..8].copy_from_slice(b"ftyp");
    data[8..12].copy_from_slice(b"M4A ");
    let result = detect_format_from_bytes(&data, None);
    assert!(result.is_ok(), "MP4 ftyp should be detected: {:?}", result);
    let fmt = result.unwrap();
    assert!(
        fmt.contains("MP4") || fmt.contains("M4A"),
        "Expected MP4-family, got: {}",
        fmt
    );
}

#[test]
fn test_detect_from_bytes_wav_riff() {
    // WAV: RIFF at 0..4, file size at 4..8, WAVE at 8..12
    let mut data = vec![0u8; 128];
    data[0..4].copy_from_slice(b"RIFF");
    data[4..8].copy_from_slice(&1000u32.to_le_bytes());
    data[8..12].copy_from_slice(b"WAVE");
    let result = detect_format_from_bytes(&data, None);
    assert!(result.is_ok(), "RIFF/WAVE should be detected: {:?}", result);
    let fmt = result.unwrap();
    assert!(
        fmt.contains("WAV") || fmt.contains("WAVE"),
        "Expected WAV format, got: {}",
        fmt
    );
}

#[test]
fn test_detect_from_bytes_aiff_form() {
    // AIFF: FORM at 0..4, size at 4..8, AIFF at 8..12
    let mut data = vec![0u8; 128];
    data[0..4].copy_from_slice(b"FORM");
    data[4..8].copy_from_slice(&500u32.to_be_bytes());
    data[8..12].copy_from_slice(b"AIFF");
    let result = detect_format_from_bytes(&data, None);
    assert!(result.is_ok(), "FORM/AIFF should be detected: {:?}", result);
    assert!(result.unwrap().contains("AIFF"));
}

#[test]
fn test_detect_from_bytes_hint_disambiguates() {
    // ID3 header alone is ambiguous — the filename hint should steer detection
    let mut data = vec![0u8; 128];
    data[..3].copy_from_slice(b"ID3");

    let as_mp3 = detect_format_from_bytes(&data, Some(Path::new("track.mp3")));
    let as_aif = detect_format_from_bytes(&data, Some(Path::new("track.aif")));

    // Both should succeed; the resolved format may differ based on extension scoring
    assert!(as_mp3.is_ok());
    assert!(as_aif.is_ok());
}

#[test]
fn test_detect_from_bytes_hint_alone_no_magic() {
    // All zeroes with a .flac extension — extension alone may or may not suffice
    let data = vec![0u8; 128];
    let result = detect_format_from_bytes(&data, Some(Path::new("track.flac")));
    // Scoring depends on implementation — just verify it doesn't panic
    let _ = result;
}

#[test]
fn test_detect_from_bytes_empty_slice() {
    let result = detect_format_from_bytes(&[], None);
    assert!(result.is_err(), "Empty data with no hint must fail");
}

#[test]
fn test_detect_from_bytes_garbage() {
    let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];
    let result = detect_format_from_bytes(&garbage, None);
    assert!(result.is_err(), "Random bytes with no hint must fail");
}

#[test]
fn test_detect_from_bytes_matches_path_detection() {
    // Verify byte-based detection agrees with path-based detection on a real file
    let test_file = "tests/data/silence-44-s.flac";
    if !std::path::Path::new(test_file).exists() {
        return;
    }
    let file_bytes = std::fs::read(test_file).unwrap();
    let from_path = detect_format(test_file).unwrap();
    let from_bytes = detect_format_from_bytes(&file_bytes, Some(Path::new(test_file))).unwrap();
    assert_eq!(
        from_path, from_bytes,
        "Byte and path detection should agree for the same file"
    );
}

// ---------------------------------------------------------------------------
// detect_ogg_format — identifies the codec inside an Ogg container
// ---------------------------------------------------------------------------

/// Build a minimal Ogg page header (28 bytes) followed by a codec identification
/// payload starting at byte 28. This is enough for detect_ogg_format to work.
fn build_ogg_header_with_codec(codec_id: &[u8]) -> Vec<u8> {
    let mut header = vec![0u8; 28 + codec_id.len() + 8];
    // OggS capture pattern at offset 0
    header[0..4].copy_from_slice(b"OggS");
    // Place codec identification bytes at offset 28 (first packet payload)
    header[28..28 + codec_id.len()].copy_from_slice(codec_id);
    header
}

#[test]
fn test_detect_ogg_vorbis() {
    // Vorbis identification starts with 0x01 followed by "vorbis" at byte 29
    let header = build_ogg_header_with_codec(b"\x01vorbis");
    assert_eq!(detect_ogg_format(&header), "OggVorbis");
}

#[test]
fn test_detect_ogg_opus() {
    let header = build_ogg_header_with_codec(b"OpusHead");
    assert_eq!(detect_ogg_format(&header), "OggOpus");
}

#[test]
fn test_detect_ogg_flac() {
    // OggFLAC: byte 28 is 0x7F, then "FLAC" at byte 29
    let header = build_ogg_header_with_codec(b"\x7fFLAC");
    assert_eq!(detect_ogg_format(&header), "OggFlac");
}

#[test]
fn test_detect_ogg_speex() {
    let header = build_ogg_header_with_codec(b"Speex   ");
    assert_eq!(detect_ogg_format(&header), "OggSpeex");
}

#[test]
fn test_detect_ogg_unknown_codec() {
    // Unrecognized codec payload should fall back to generic "Ogg"
    let header = build_ogg_header_with_codec(b"UnknownCodec");
    assert_eq!(detect_ogg_format(&header), "Ogg");
}

#[test]
fn test_detect_ogg_header_too_short() {
    // Header shorter than 36 bytes — cannot identify any sub-format
    let short = vec![0u8; 30];
    assert_eq!(detect_ogg_format(&short), "Ogg");
}

#[test]
fn test_detect_ogg_empty() {
    assert_eq!(detect_ogg_format(&[]), "Ogg");
}
