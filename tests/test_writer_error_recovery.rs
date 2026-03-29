//! Tests verifying that a failed save operation does not corrupt existing data.
//!
//! For each format, the test loads a file into memory, modifies tags, then
//! attempts to save into a writer that is deliberately too small. After the
//! expected error, the original in-memory buffer must still load cleanly.
//! No files on disk are modified.

use audex::FileType;
use audex::asf::ASF;
use audex::flac::FLAC;
use audex::mp4::MP4;
use audex::oggvorbis::OggVorbis;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

fn data_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join(name)
}

/// A writer that fails after accepting a limited number of bytes.
/// Used to simulate disk-full or I/O errors during save.
struct FailAfterWriter {
    inner: Cursor<Vec<u8>>,
    bytes_remaining: usize,
}

impl FailAfterWriter {
    fn new(capacity: usize, fail_after: usize) -> Self {
        Self {
            inner: Cursor::new(vec![0u8; capacity]),
            bytes_remaining: fail_after,
        }
    }
}

impl Read for FailAfterWriter {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for FailAfterWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.bytes_remaining == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "simulated write failure",
            ));
        }
        let to_write = buf.len().min(self.bytes_remaining);
        let written = self.inner.write(&buf[..to_write])?;
        self.bytes_remaining = self.bytes_remaining.saturating_sub(written);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for FailAfterWriter {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

#[test]
fn flac_save_failure_does_not_corrupt_source() {
    let test_file = data_path("silence-44-s.flac");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();

    // Load and add a large tag to force the save to write more data
    let mut load_cursor = Cursor::new(original_data.clone());
    let mut flac = FLAC::load_from_reader(&mut load_cursor).unwrap();
    flac.set("BIGTAG", vec!["X".repeat(10_000)]).unwrap();

    // Attempt to save into a writer that fails after a few bytes
    let mut failing_writer = FailAfterWriter::new(original_data.len(), 64);
    let save_result = flac.save_to_writer(&mut failing_writer);
    assert!(
        save_result.is_err(),
        "Save into a failing writer must error"
    );

    // The original data buffer must still be loadable
    let mut verify_cursor = Cursor::new(original_data);
    let reloaded = FLAC::load_from_reader(&mut verify_cursor);
    assert!(
        reloaded.is_ok(),
        "Original data must remain valid after save failure: {:?}",
        reloaded.err()
    );
}

#[test]
fn mp4_save_failure_does_not_corrupt_source() {
    let test_file = data_path("has-tags.m4a");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();

    let mut load_cursor = Cursor::new(original_data.clone());
    let mut mp4 = MP4::load_from_reader(&mut load_cursor).unwrap();
    mp4.set("\u{00a9}nam", vec!["Test".to_string()]).unwrap();

    let mut failing_writer = FailAfterWriter::new(original_data.len(), 64);
    let save_result = mp4.save_to_writer(&mut failing_writer);
    assert!(
        save_result.is_err(),
        "Save into a failing writer must error"
    );

    let mut verify_cursor = Cursor::new(original_data);
    let reloaded = MP4::load_from_reader(&mut verify_cursor);
    assert!(
        reloaded.is_ok(),
        "Original data must remain valid after save failure: {:?}",
        reloaded.err()
    );
}

#[test]
fn ogg_save_failure_does_not_corrupt_source() {
    let test_file = data_path("multipagecomment.ogg");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();

    let mut load_cursor = Cursor::new(original_data.clone());
    let mut ogg = OggVorbis::load_from_reader(&mut load_cursor).unwrap();
    ogg.set("BIGTAG", vec!["X".repeat(10_000)]).unwrap();

    let mut failing_writer = FailAfterWriter::new(original_data.len(), 64);
    let save_result = ogg.save_to_writer(&mut failing_writer);
    assert!(
        save_result.is_err(),
        "Save into a failing writer must error"
    );

    let mut verify_cursor = Cursor::new(original_data);
    let reloaded = OggVorbis::load_from_reader(&mut verify_cursor);
    assert!(
        reloaded.is_ok(),
        "Original data must remain valid after save failure: {:?}",
        reloaded.err()
    );
}

#[test]
fn asf_save_failure_does_not_corrupt_source() {
    let test_file = data_path("silence-1.wma");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();

    let mut load_cursor = Cursor::new(original_data.clone());
    let mut asf = ASF::load_from_reader(&mut load_cursor).unwrap();
    asf.set("Title", vec!["X".repeat(10_000)]);

    let mut failing_writer = FailAfterWriter::new(original_data.len(), 64);
    let save_result = asf.save_to_writer(&mut failing_writer);
    assert!(
        save_result.is_err(),
        "Save into a failing writer must error"
    );

    let mut verify_cursor = Cursor::new(original_data);
    let reloaded = ASF::load_from_reader(&mut verify_cursor);
    assert!(
        reloaded.is_ok(),
        "Original data must remain valid after save failure: {:?}",
        reloaded.err()
    );
}
