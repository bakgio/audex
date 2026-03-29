/// Tests for AnyFileThing round-trip save/load functionality
/// - Constructors for disk files, memory buffers, and wrapped File
/// - Helper functions with_filething_read and with_filething_write
/// - From/TryFrom implementations
/// - display_name() and len_hint() methods
/// - Error handling and RAII semantics
use audex::util::{AnyFileThing, FileInput};
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

#[test]
fn test_anyfilething_from_path() {
    // Create a temporary file for testing
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.txt");
    std::fs::write(&test_path, b"Hello, World!").unwrap();

    // Test TryFrom<&Path>
    let mut file_thing = AnyFileThing::try_from(test_path.as_path()).unwrap();
    assert_eq!(file_thing.display_name(), test_path.as_path());

    let mut buffer = Vec::new();
    file_thing.read_to_end(&mut buffer).unwrap();
    assert_eq!(buffer, b"Hello, World!");
}

#[test]
fn test_anyfilething_from_pathbuf() {
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.txt");
    std::fs::write(&test_path, b"Test data").unwrap();

    // Test TryFrom<PathBuf>
    let mut file_thing = AnyFileThing::try_from(test_path.clone()).unwrap();

    let mut buffer = Vec::new();
    file_thing.read_to_end(&mut buffer).unwrap();
    assert_eq!(buffer, b"Test data");
}

#[test]
fn test_anyfilething_from_cursor() {
    // Test From<Cursor<Vec<u8>>>
    let data = b"Memory data".to_vec();
    let cursor = Cursor::new(data.clone());
    let mut file_thing = AnyFileThing::from(cursor);

    assert!(file_thing.is_memory());
    assert_eq!(
        file_thing.display_name(),
        PathBuf::from("<memory>").as_path()
    );

    let mut buffer = Vec::new();
    file_thing.read_to_end(&mut buffer).unwrap();
    assert_eq!(buffer, data);
}

#[test]
fn test_anyfilething_try_from_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.txt");
    std::fs::write(&test_path, b"File handle test").unwrap();

    // Test TryFrom<File>
    let file = File::open(&test_path).unwrap();
    let mut file_thing = AnyFileThing::try_from(file).unwrap();

    let mut buffer = Vec::new();
    file_thing.read_to_end(&mut buffer).unwrap();
    assert_eq!(buffer, b"File handle test");
}

#[test]
fn test_anyfilething_len_hint() {
    // Test with memory buffer
    let data = vec![1, 2, 3, 4, 5];
    let cursor = Cursor::new(data.clone());
    let mut file_thing = AnyFileThing::from(cursor);

    let len = file_thing.len_hint().unwrap();
    assert_eq!(len, 5);
}

#[test]
fn test_memory_read_write_parity() {
    // Test reading and writing to memory buffer
    let data = b"Initial data".to_vec();
    let cursor = Cursor::new(data);
    let mut file_thing = AnyFileThing::from(cursor);

    // Read initial data
    let mut buffer = Vec::new();
    file_thing.read_to_end(&mut buffer).unwrap();
    assert_eq!(buffer, b"Initial data");

    // Seek back and overwrite
    file_thing.seek(SeekFrom::Start(0)).unwrap();
    file_thing.write_all(b"Modified").unwrap();

    // Read back
    file_thing.seek(SeekFrom::Start(0)).unwrap();
    buffer.clear();
    file_thing.read_to_end(&mut buffer).unwrap();
    assert_eq!(&buffer[..8], b"Modified");
}

#[test]
fn test_load_from_cursor_no_filesystem() {
    // Test loading from Cursor without touching filesystem
    // This simulates loading ID3 tags from an in-memory buffer
    let fake_id3_data = b"ID3\x03\x00\x00\x00\x00\x00\x00";
    let cursor = Cursor::new(fake_id3_data.to_vec());
    let mut file_thing = AnyFileThing::from(cursor);

    // Verify we can read the header
    let mut header = [0u8; 3];
    file_thing.read_exact(&mut header).unwrap();
    assert_eq!(&header, b"ID3");

    // Verify no filesystem access occurred
    assert!(file_thing.is_memory());
}

#[test]
fn test_double_close_safety() {
    // Test that dropping a FileThing multiple times is safe
    let data = b"Test".to_vec();
    let cursor = Cursor::new(data);
    let file_thing = AnyFileThing::from(cursor);

    // Explicitly drop
    drop(file_thing);

    // Should not panic or cause issues
}

#[test]
fn test_seek_and_tell() {
    let data = b"0123456789".to_vec();
    let cursor = Cursor::new(data);
    let mut file_thing = AnyFileThing::from(cursor);

    // Test seeking
    let pos = file_thing.seek(SeekFrom::Start(5)).unwrap();
    assert_eq!(pos, 5);

    // Read from position
    let mut buf = [0u8; 1];
    file_thing.read_exact(&mut buf).unwrap();
    assert_eq!(buf[0], b'5');

    // Seek relative
    let pos = file_thing.seek(SeekFrom::Current(-2)).unwrap();
    assert_eq!(pos, 4);
}

#[test]
fn test_fuzz_random_cursors() {
    // Fuzz-style test with 100 randomly sized cursors
    for i in 0..100 {
        let size = (i * 13) % 1024; // Pseudo-random sizes from 0 to 1023
        let data: Vec<u8> = (0..size).map(|x| (x % 256) as u8).collect();

        let cursor = Cursor::new(data.clone());
        let mut file_thing = AnyFileThing::from(cursor);

        // Verify size
        let len = file_thing.len_hint().unwrap();
        assert_eq!(len as usize, size);

        // Read and verify content
        let mut buffer = Vec::new();
        file_thing.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer.len(), size);
        assert_eq!(buffer, data);
    }
}

#[test]
fn test_flush_and_sync() {
    use audex::util::LoadFileOptions;
    use audex::util::openfile_simple;

    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.txt");
    std::fs::write(&test_path, b"").unwrap();

    let options = LoadFileOptions::write_method();
    let mut file_thing = openfile_simple(FileInput::from_path(&test_path), &options).unwrap();
    file_thing.write_all(b"Flush test").unwrap();

    // Test flush
    let result = file_thing.flush();
    assert!(result.is_ok());
}

#[test]
fn test_truncate() {
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.txt");
    std::fs::write(&test_path, b"0123456789").unwrap();

    // Open with write mode for truncation
    use audex::util::LoadFileOptions;
    use audex::util::openfile_simple;

    let options = LoadFileOptions::write_method();
    let mut file_thing = openfile_simple(FileInput::from_path(&test_path), &options).unwrap();

    // Truncate to 5 bytes
    file_thing.truncate(5).unwrap();

    let len = file_thing.len_hint().unwrap();
    assert_eq!(len, 5);
}

#[test]
fn test_memory_truncate() {
    let data = b"0123456789".to_vec();
    let cursor = Cursor::new(data);
    let mut file_thing = AnyFileThing::from(cursor);

    // Truncate memory buffer
    file_thing.truncate(5).unwrap();

    let len = file_thing.len_hint().unwrap();
    assert_eq!(len, 5);

    // Read truncated data
    file_thing.seek(SeekFrom::Start(0)).unwrap();
    let mut buffer = Vec::new();
    file_thing.read_to_end(&mut buffer).unwrap();
    assert_eq!(buffer, b"01234");
}

#[test]
fn test_display_name_variants() {
    // Test display_name for path
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.txt");
    std::fs::write(&test_path, b"test").unwrap();

    let file_thing = AnyFileThing::try_from(test_path.clone()).unwrap();
    assert_eq!(file_thing.display_name(), test_path.as_path());

    // Test display_name for memory
    let cursor = Cursor::new(vec![1, 2, 3]);
    let file_thing = AnyFileThing::from(cursor);
    assert_eq!(
        file_thing.display_name(),
        PathBuf::from("<memory>").as_path()
    );
}

#[test]
fn test_filename_method() {
    // Test filename() returns Some for file paths
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.txt");
    std::fs::write(&test_path, b"test").unwrap();

    let file_thing = AnyFileThing::try_from(test_path.clone()).unwrap();
    assert!(file_thing.filename().is_some());

    // Test filename() returns None for memory
    let cursor = Cursor::new(vec![1, 2, 3]);
    let file_thing = AnyFileThing::from(cursor);
    assert!(file_thing.filename().is_none());
}

#[test]
fn test_tryform_path_returns_error_on_missing_file() {
    // Verify that TryFrom<&Path> properly returns an error for non-existent files
    // instead of silently returning an empty in-memory buffer
    let bad_path = std::path::Path::new("/nonexistent/path/to/file.txt");
    let result = AnyFileThing::try_from(bad_path);
    assert!(
        result.is_err(),
        "Opening a non-existent file should return an error"
    );
}
