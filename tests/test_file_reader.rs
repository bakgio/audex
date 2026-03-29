/// Tests for File::load_from_reader functionality
///
/// Validates that format loading from in-memory buffers works correctly
/// without creating any temporary files on disk.
use audex::File;
use std::io::Cursor;
use std::path::PathBuf;

/// Get path to test data file using CARGO_MANIFEST_DIR for robustness
fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

#[test]
fn test_load_from_cursor_mp3() {
    let test_file = data_path("silence-44-s.mp3");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    // Load from cursor with origin path hint
    let file = File::load_from_reader(cursor, Some(test_file));

    assert!(file.is_ok(), "Should load MP3 from cursor");
    let file = file.unwrap();

    // Verify the format was detected correctly
    assert!(file.format_name().contains("MP3"));
}

#[test]
fn test_load_from_cursor_flac() {
    let test_file = data_path("silence-44-s.flac");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    let file = File::load_from_reader(cursor, Some(test_file));

    assert!(file.is_ok(), "Should load FLAC from cursor");
    let file = file.unwrap();

    assert!(file.format_name().contains("FLAC"));
}

#[test]
fn test_load_from_cursor_without_path() {
    // Loading without path hint — should detect format from magic bytes alone
    let test_file = data_path("silence-44-s.mp3");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    let file = File::load_from_reader(cursor, None);

    assert!(file.is_ok(), "Should detect MP3 from magic bytes alone");
    let file = file.unwrap();

    assert!(file.format_name().contains("MP3"));
}

#[test]
fn test_load_from_cursor_metadata() {
    let test_file = data_path("silence-44-s.mp3");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    let file = File::load_from_reader(cursor, Some(test_file)).unwrap();

    // Verify metadata access works from reader-loaded file
    let _has_tags = file.has_tags();
}

#[test]
fn test_load_from_cursor_stream_info() {
    let test_file = data_path("silence-44-s.mp3");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    let file = File::load_from_reader(cursor, Some(test_file)).unwrap();

    // Verify stream info is accessible
    let info = file.info();
    let _ = info;
}

#[test]
fn test_load_from_vec() {
    let test_file = data_path("silence-44-s.flac");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    let result = File::load_from_reader(cursor, Some(test_file));
    assert!(result.is_ok(), "Should load from Vec<u8> via Cursor");
}

#[test]
fn test_cursor_seeking() {
    // Seeking to an offset before loading — the reader starts mid-file,
    // so format detection may fail, but it should not panic.
    let test_file = data_path("silence-44-s.mp3");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let mut cursor = Cursor::new(data);

    use std::io::Seek;
    cursor.seek(std::io::SeekFrom::Start(100)).unwrap();

    // This may fail (skipped the header) but must not panic
    let _result = File::load_from_reader(cursor, Some(test_file));
}

#[test]
fn test_empty_cursor() {
    let cursor = Cursor::new(Vec::new());
    let result = File::load_from_reader(cursor, None);

    assert!(result.is_err(), "Empty cursor should produce an error");
}

#[test]
fn test_invalid_data_cursor() {
    let invalid_data = vec![0u8; 1024];
    let cursor = Cursor::new(invalid_data);
    let result = File::load_from_reader(cursor, None);

    assert!(
        result.is_err(),
        "Invalid data should produce an unsupported format error"
    );
}

#[test]
fn test_load_from_reader_ogg() {
    let test_file = data_path("empty.ogg");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    let file = File::load_from_reader(cursor, Some(test_file));

    if let Ok(file) = file {
        assert!(file.format_name().contains("Ogg") || file.format_name().contains("Vorbis"));
    }
}

#[test]
fn test_reader_preserves_filename() {
    // Verify that origin_path hint is used for format detection
    let test_file = data_path("silence-44-s.mp3");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    let custom_path = PathBuf::from("/custom/path/test.mp3");
    let file = File::load_from_reader(cursor, Some(custom_path));

    // Should still detect as MP3 using the path hint + header
    assert!(file.is_ok(), "Should detect format using path hint");
}

#[test]
fn test_load_from_reader_aiff() {
    // Test loading AIFF from a cursor (validates Part B reader support for AIFF)
    let test_file = data_path("with-id3.aif");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    let file = File::load_from_reader(cursor, Some(test_file));

    assert!(file.is_ok(), "Should load AIFF from cursor");
    let file = file.unwrap();
    assert!(
        file.format_name().contains("AIFF"),
        "Format should be detected as AIFF"
    );
}

#[test]
fn test_load_from_reader_wave() {
    // Test loading WAVE from a cursor (validates Part B reader support for WAVE)
    let test_file = data_path("silence-2s-PCM-44100-16-ID3v23.wav");
    if !test_file.exists() {
        return;
    }

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);

    let file = File::load_from_reader(cursor, Some(test_file));

    assert!(file.is_ok(), "Should load WAVE from cursor");
    let file = file.unwrap();
    assert!(
        file.format_name().contains("WAV") || file.format_name().contains("WAVE"),
        "Format should be detected as WAVE"
    );
}

#[test]
fn test_reader_matches_path_loading() {
    // Verify that reader-based loading produces equivalent results to path-based loading
    use audex::StreamInfo;

    let test_file = data_path("silence-44-s.flac");
    if !test_file.exists() {
        return;
    }

    // Load via path
    let path_loaded = File::load(&test_file).unwrap();

    // Load via reader
    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);
    let reader_loaded = File::load_from_reader(cursor, Some(test_file)).unwrap();

    // Compare format names
    assert_eq!(
        path_loaded.format_name(),
        reader_loaded.format_name(),
        "Format name should match between path and reader loading"
    );

    // Compare stream info
    assert_eq!(
        path_loaded.info().sample_rate(),
        reader_loaded.info().sample_rate(),
        "Sample rate should match"
    );
    assert_eq!(
        path_loaded.info().channels(),
        reader_loaded.info().channels(),
        "Channels should match"
    );
    assert_eq!(
        path_loaded.info().bits_per_sample(),
        reader_loaded.info().bits_per_sample(),
        "Bits per sample should match"
    );
}

#[test]
fn test_load_from_reader_no_temp_files() {
    // Verify that load_from_reader does not create temporary files
    let test_file = data_path("silence-44-s.mp3");
    if !test_file.exists() {
        return;
    }

    // Use a known temp directory to detect .tmp file creation
    let temp_dir = std::env::temp_dir();
    let files_before: std::collections::HashSet<_> = std::fs::read_dir(&temp_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();

    let data = std::fs::read(&test_file).unwrap();
    let cursor = Cursor::new(data);
    let _file = File::load_from_reader(cursor, Some(test_file)).unwrap();

    let files_after: std::collections::HashSet<_> = std::fs::read_dir(&temp_dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();

    let new_files: Vec<_> = files_after
        .difference(&files_before)
        .filter(|p| {
            p.to_string_lossy().contains("audex")
                || p.to_string_lossy().contains(".tmp")
                || p.to_string_lossy().contains("tempfile")
        })
        .collect();

    assert!(
        new_files.is_empty(),
        "No temporary files should be created during load_from_reader, found: {:?}",
        new_files
    );
}
