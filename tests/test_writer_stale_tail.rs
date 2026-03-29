//! Tests for stale trailing bytes after writer-based metadata operations.
//!
//! When metadata is removed or shrunk via `save_to_writer` or `clear_writer`,
//! the rewritten output may be shorter than the original input. If the writer
//! is not properly truncated or zeroed, stale bytes from the original content
//! remain at the tail of the output buffer. These tests verify that no
//! recoverable metadata leaks into the trailing region.

use audex::FileType;
use audex::aiff::AIFF;
use audex::flac::FLAC;
use audex::id3::file::clear_from_writer;
use audex::mp4::MP4;
use audex::oggvorbis::OggVorbis;
use audex::wave::WAVE;
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::path::PathBuf;

/// Resolve a path to a file in the test data directory.
fn data_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join(name)
}

/// Count non-zero bytes after the logical end of a writer result.
fn trailing_non_zero_count(data: &[u8], logical_end: usize) -> usize {
    data[logical_end..].iter().filter(|&&b| b != 0).count()
}

/// Minimal writer wrapper that rejects oversized single write calls.
///
/// This helps verify that stale-tail cleanup happens in bounded chunks rather
/// than through one large zero-filled allocation.
struct ChunkLimitedWriter {
    inner: Cursor<Vec<u8>>,
    max_chunk: usize,
    largest_write: usize,
    largest_cleanup_write: usize,
    allow_first_full_rewrite: bool,
}

impl ChunkLimitedWriter {
    fn new(data: Vec<u8>, max_chunk: usize) -> Self {
        Self {
            inner: Cursor::new(data),
            max_chunk,
            largest_write: 0,
            largest_cleanup_write: 0,
            allow_first_full_rewrite: true,
        }
    }

    fn largest_cleanup_write(&self) -> usize {
        self.largest_cleanup_write
    }
}

impl std::io::Read for ChunkLimitedWriter {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl std::io::Write for ChunkLimitedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let is_initial_rewrite =
            self.allow_first_full_rewrite && self.inner.position() == 0 && self.largest_write == 0;

        if buf.len() > self.max_chunk && !is_initial_rewrite {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "single write of {} bytes exceeds {} byte chunk limit",
                    buf.len(),
                    self.max_chunk
                ),
            ));
        }
        if is_initial_rewrite {
            self.allow_first_full_rewrite = false;
        } else {
            self.largest_cleanup_write = self.largest_cleanup_write.max(buf.len());
        }
        self.largest_write = self.largest_write.max(buf.len());
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl std::io::Seek for ChunkLimitedWriter {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Direct demonstration of the stale-tail problem using raw I/O.
///
/// Writing shorter data into a Cursor seeded with longer content
/// leaves the original bytes intact beyond the written region.
#[test]
fn cursor_write_all_does_not_truncate_shorter_data() {
    let original = vec![0xFFu8; 100];
    let mut cursor = Cursor::new(original);

    // Write only 60 bytes from position 0
    let shorter_data = vec![0xAAu8; 60];
    cursor.seek(SeekFrom::Start(0)).unwrap();
    cursor.write_all(&shorter_data).unwrap();

    let output = cursor.into_inner();

    // The Vec is still 100 bytes — the last 40 bytes are stale 0xFF
    assert_eq!(output.len(), 100, "Cursor Vec retains original length");
    assert_eq!(
        &output[60..],
        &[0xFFu8; 40],
        "Trailing bytes are stale original content"
    );
}

/// Verify that ID3 clear_from_writer leaves stale bytes when tags are removed.
///
/// After clearing ID3v2 tags from an MP3 file, the meaningful content is
/// shorter (the tag header + frames are removed). The writer's buffer
/// should be truncated or zeroed to prevent stale data retention.
#[test]
fn id3_clear_writer_leaves_stale_tail() {
    let test_file = data_path("silence-44-s.mp3");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();
    let original_len = original_data.len();

    // First, determine the expected output size by clearing into a
    // properly-sized buffer. Use a Vec that starts empty and gets
    // populated by the clear operation.
    // Since clear_from_writer reads from the writer, we need to seed it.
    let mut writer = Cursor::new(original_data.clone());
    clear_from_writer(&mut writer, true, true).unwrap();

    // The cursor position after the write tells us how many bytes
    // of meaningful content were written back
    let cursor_pos_after_write = writer.position() as usize;
    let output = writer.into_inner();

    println!("Original file size: {} bytes", original_len);
    println!(
        "Cursor position after clear: {} bytes",
        cursor_pos_after_write
    );
    println!("Output buffer size: {} bytes", output.len());

    // If the cursor position is less than the buffer size, stale bytes exist
    if cursor_pos_after_write < output.len() {
        let stale_region = &output[cursor_pos_after_write..];
        let non_zero_count = stale_region.iter().filter(|&&b| b != 0).count();

        println!(
            "Stale tail: {} bytes ({}..{}), {} non-zero bytes",
            stale_region.len(),
            cursor_pos_after_write,
            output.len(),
            non_zero_count
        );

        // The stale region should be all zeros after fix.
        // Before fix, it contains original file content.
        assert_eq!(
            non_zero_count,
            0,
            "Found {} non-zero stale bytes after position {} — \
             trailing content from the original file was not cleared \
             (buffer: {} bytes, meaningful: {} bytes)",
            non_zero_count,
            cursor_pos_after_write,
            output.len(),
            cursor_pos_after_write
        );
    }
}

/// Verify that MP4 clear_writer does not leave recoverable tag content
/// in the writer's trailing bytes.
#[test]
fn mp4_clear_writer_no_stale_trailing_content() {
    let test_file = data_path("has-tags.m4a");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();
    let original_len = original_data.len();

    // Load the file and confirm it has tags
    let mut load_cursor = Cursor::new(original_data.clone());
    let mut mp4 = MP4::load_from_reader(&mut load_cursor).unwrap();
    let tag_keys = mp4.keys();
    assert!(!tag_keys.is_empty(), "Test file must have tags");

    // Clear tags into a cursor seeded with original data
    let mut out = Cursor::new(original_data);
    mp4.clear_writer(&mut out).unwrap();

    let cursor_pos = out.position() as usize;
    let output = out.into_inner();

    println!("MP4 original: {} bytes", original_len);
    println!("MP4 cursor position after clear: {}", cursor_pos);
    println!("MP4 output buffer: {} bytes", output.len());

    // Check for stale tail
    if cursor_pos < output.len() {
        let stale_region = &output[cursor_pos..];
        let non_zero_count = stale_region.iter().filter(|&&b| b != 0).count();

        println!(
            "MP4 stale tail: {} bytes, {} non-zero",
            stale_region.len(),
            non_zero_count
        );

        assert_eq!(
            non_zero_count, 0,
            "Found {} non-zero stale bytes after position {} in MP4 writer output",
            non_zero_count, cursor_pos
        );
    }
}

/// Verify that Ogg Vorbis clear_writer scrubs the trailing region after shrink.
#[test]
fn ogg_clear_writer_no_stale_trailing_content() {
    let test_file = data_path("multipagecomment.ogg");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();
    let mut load_cursor = Cursor::new(original_data.clone());
    let mut ogg = OggVorbis::load_from_reader(&mut load_cursor).unwrap();

    let mut out = Cursor::new(original_data);
    ogg.clear_writer(&mut out).unwrap();

    let cursor_pos = out.position() as usize;
    let output = out.into_inner();

    if cursor_pos < output.len() {
        let non_zero_count = trailing_non_zero_count(&output, cursor_pos);
        assert_eq!(
            non_zero_count, 0,
            "Found {} non-zero stale bytes after position {} in Ogg writer output",
            non_zero_count, cursor_pos
        );
    }
}

/// Verify that FLAC clear_writer scrubs the trailing region after shrink.
#[test]
fn flac_clear_writer_no_stale_trailing_content() {
    let test_file = data_path("52-overwritten-metadata.flac");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();
    let mut load_cursor = Cursor::new(original_data.clone());
    let mut flac = FLAC::load_from_reader(&mut load_cursor).unwrap();
    assert!(flac.tags().is_some(), "Test file must contain FLAC tags");

    let mut out = Cursor::new(original_data);
    flac.clear_writer(&mut out).unwrap();

    let cursor_pos = out.position() as usize;
    let output = out.into_inner();

    if cursor_pos < output.len() {
        let non_zero_count = trailing_non_zero_count(&output, cursor_pos);
        assert_eq!(
            non_zero_count, 0,
            "Found {} non-zero stale bytes after position {} in FLAC writer output",
            non_zero_count, cursor_pos
        );
    }
}

/// Verify that AIFF clear_writer scrubs the trailing region after removing ID3.
#[test]
fn aiff_clear_writer_no_stale_trailing_content() {
    let test_file = data_path("with-id3.aif");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();
    let mut load_cursor = Cursor::new(original_data.clone());
    let mut aiff = AIFF::load_from_reader(&mut load_cursor).unwrap();
    assert!(aiff.tags().is_some(), "Test file must contain AIFF tags");

    let mut out = Cursor::new(original_data);
    aiff.clear_writer(&mut out).unwrap();

    let cursor_pos = out.position() as usize;
    let output = out.into_inner();

    if cursor_pos < output.len() {
        let non_zero_count = trailing_non_zero_count(&output, cursor_pos);
        assert_eq!(
            non_zero_count, 0,
            "Found {} non-zero stale bytes after position {} in AIFF writer output",
            non_zero_count, cursor_pos
        );
    }
}

#[test]
fn wave_clear_writer_no_stale_trailing_content() {
    let test_file = data_path("silence-2s-PCM-44100-16-ID3v23.wav");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();
    let mut load_cursor = Cursor::new(original_data.clone());
    let mut wave = WAVE::load_from_reader(&mut load_cursor).unwrap();
    assert!(wave.tags().is_some(), "Test file must contain WAVE tags");

    let mut out = Cursor::new(original_data);
    wave.clear_writer(&mut out).unwrap();

    let cursor_pos = out.position() as usize;
    let output = out.into_inner();

    if cursor_pos < output.len() {
        let non_zero_count = trailing_non_zero_count(&output, cursor_pos);
        assert_eq!(
            non_zero_count, 0,
            "Found {} non-zero stale bytes after position {} in WAVE writer output",
            non_zero_count, cursor_pos
        );
    }
}

/// Verify that large stale-tail cleanup uses bounded writes instead of one
/// oversized zero-filled buffer.
#[test]
fn stale_tail_cleanup_uses_bounded_writes() {
    let test_file = data_path("silence-44-s.mp3");
    if !test_file.exists() {
        eprintln!("Skipping: test file not found");
        return;
    }

    let mut data = std::fs::read(&test_file).unwrap();
    data.extend(std::iter::repeat_n(0xA5u8, 2 * 1024 * 1024));

    let mut writer = ChunkLimitedWriter::new(data, 64 * 1024);
    clear_from_writer(&mut writer, true, true).unwrap();

    assert!(
        writer.largest_cleanup_write() <= 64 * 1024,
        "cleanup issued a {} byte write despite the configured chunk limit",
        writer.largest_cleanup_write()
    );
}
