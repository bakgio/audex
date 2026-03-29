use audex::riff::RiffFile;
use std::cell::RefCell;
use std::fs::{File, copy};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::rc::Rc;
use tempfile::NamedTempFile;

/// Test utilities for RIFF tests
struct TestUtils;

impl TestUtils {
    /// Get path to test data file
    pub fn data_path(filename: &str) -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests");
        path.push("data");
        path.push(filename);
        path
    }
}

struct TRiffFile {
    has_tags_path: String,
    no_tags_path: String,
}

impl TRiffFile {
    fn new() -> Self {
        Self {
            has_tags_path: TestUtils::data_path("silence-2s-PCM-44100-16-ID3v23.wav")
                .to_string_lossy()
                .to_string(),
            no_tags_path: TestUtils::data_path("silence-2s-PCM-16000-08-notags.wav")
                .to_string_lossy()
                .to_string(),
        }
    }

    fn setup(
        &self,
    ) -> (
        RiffFile,
        RiffFile,
        NamedTempFile,
        RiffFile,
        NamedTempFile,
        RiffFile,
    ) {
        // Open read-only files
        let file_1 = File::open(&self.has_tags_path).expect("Failed to open has_tags file");
        let riff_1 =
            RiffFile::new(Rc::new(RefCell::new(file_1))).expect("Failed to create RiffFile 1");

        let file_2 = File::open(&self.no_tags_path).expect("Failed to open no_tags file");
        let riff_2 =
            RiffFile::new(Rc::new(RefCell::new(file_2))).expect("Failed to create RiffFile 2");

        // Create temporary files for write tests
        let tmp_1 = NamedTempFile::new().expect("Failed to create temp file 1");
        copy(&self.has_tags_path, tmp_1.path()).expect("Failed to copy has_tags to temp");
        let file_1_tmp = File::options()
            .read(true)
            .write(true)
            .open(tmp_1.path())
            .expect("Failed to open temp file 1");
        let riff_1_tmp = RiffFile::new(Rc::new(RefCell::new(file_1_tmp)))
            .expect("Failed to create temp RiffFile 1");

        let tmp_2 = NamedTempFile::new().expect("Failed to create temp file 2");
        copy(&self.no_tags_path, tmp_2.path()).expect("Failed to copy no_tags to temp");
        let file_2_tmp = File::options()
            .read(true)
            .write(true)
            .open(tmp_2.path())
            .expect("Failed to open temp file 2");
        let riff_2_tmp = RiffFile::new(Rc::new(RefCell::new(file_2_tmp)))
            .expect("Failed to create temp RiffFile 2");

        (riff_1, riff_2, tmp_1, riff_1_tmp, tmp_2, riff_2_tmp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_chunks() {
        let test_case = TRiffFile::new();
        let (mut riff_1, mut riff_2, _tmp_1, _riff_1_tmp, _tmp_2, _riff_2_tmp) = test_case.setup();

        // Test riff_1 has required chunks
        assert!(riff_1.contains("fmt ").expect("Failed to check fmt chunk"));
        assert!(riff_1.contains("data").expect("Failed to check data chunk"));
        assert!(riff_1.contains("id3 ").expect("Failed to check id3 chunk"));

        // Test riff_2 has required chunks
        assert!(riff_2.contains("fmt ").expect("Failed to check fmt chunk"));
        assert!(riff_2.contains("data").expect("Failed to check data chunk"));
    }

    #[test]
    fn test_is_chunks() {
        let test_case = TRiffFile::new();
        let (mut riff_1, _riff_2, _tmp_1, _riff_1_tmp, _tmp_2, _riff_2_tmp) = test_case.setup();

        // Test chunk types - all should be RiffChunk instances
        let _fmt_chunk = riff_1.get_chunk("fmt ").expect("Failed to get fmt chunk");

        let _data_chunk = riff_1.get_chunk("data").expect("Failed to get data chunk");

        let _id3_chunk = riff_1.get_chunk("id3 ").expect("Failed to get id3 chunk");
    }

    #[test]
    fn test_chunk_size() {
        let test_case = TRiffFile::new();
        let (mut riff_1, mut riff_2, _tmp_1, _riff_1_tmp, _tmp_2, _riff_2_tmp) = test_case.setup();

        // Test chunk sizes
        let data_chunk = riff_1.get_chunk("data").expect("Failed to get data chunk");
        assert_eq!(data_chunk.size(), 352808);

        let id3_chunk = riff_1.get_chunk("id3 ").expect("Failed to get id3 chunk");
        assert_eq!(id3_chunk.size(), 376);

        let data_chunk_2 = riff_2.get_chunk("data").expect("Failed to get data chunk");
        assert_eq!(data_chunk_2.size(), 64008);
    }

    #[test]
    fn test_chunk_data_size() {
        let test_case = TRiffFile::new();
        let (mut riff_1, mut riff_2, _tmp_1, _riff_1_tmp, _tmp_2, _riff_2_tmp) = test_case.setup();

        // Test chunk data sizes
        let data_chunk = riff_1.get_chunk("data").expect("Failed to get data chunk");
        assert_eq!(data_chunk.data_size(), 352800);

        let id3_chunk = riff_1.get_chunk("id3 ").expect("Failed to get id3 chunk");
        assert_eq!(id3_chunk.data_size(), 368);

        let data_chunk_2 = riff_2.get_chunk("data").expect("Failed to get data chunk");
        assert_eq!(data_chunk_2.data_size(), 64000);
    }

    #[test]
    fn test_riff_chunk_resize() {
        let test_case = TRiffFile::new();
        let (_riff_1, _riff_2, _tmp_1, mut riff_1_tmp, _tmp_2, mut riff_2_tmp) = test_case.setup();

        // Test resizing chunks
        let mut data_chunk_1 = riff_1_tmp
            .get_chunk("data")
            .expect("Failed to get data chunk");
        data_chunk_1.resize(17000).expect("Failed to resize chunk");

        // Verify the resize worked by creating a new RiffFile
        let file_1_tmp_reopen = File::options()
            .read(true)
            .write(true)
            .open(_tmp_1.path())
            .expect("Failed to reopen temp file");
        let mut riff_1_tmp_new = RiffFile::new(Rc::new(RefCell::new(file_1_tmp_reopen)))
            .expect("Failed to create new RiffFile");
        let data_chunk_1_new = riff_1_tmp_new
            .get_chunk("data")
            .expect("Failed to get data chunk after resize");
        assert_eq!(data_chunk_1_new.data_size(), 17000);

        // Test resizing to 0
        let mut data_chunk_2 = riff_2_tmp
            .get_chunk("data")
            .expect("Failed to get data chunk");
        data_chunk_2.resize(0).expect("Failed to resize chunk to 0");

        let file_2_tmp_reopen = File::options()
            .read(true)
            .write(true)
            .open(_tmp_2.path())
            .expect("Failed to reopen temp file");
        let mut riff_2_tmp_new = RiffFile::new(Rc::new(RefCell::new(file_2_tmp_reopen)))
            .expect("Failed to create new RiffFile");
        let data_chunk_2_new = riff_2_tmp_new
            .get_chunk("data")
            .expect("Failed to get data chunk after resize");
        assert_eq!(data_chunk_2_new.data_size(), 0);
    }

    #[test]
    fn test_insert_chunk() {
        let test_case = TRiffFile::new();
        let (_riff_1, _riff_2, _tmp_1, _riff_1_tmp, _tmp_2, mut riff_2_tmp) = test_case.setup();

        // Insert new chunk
        riff_2_tmp
            .insert_chunk("id3 ", None)
            .expect("Failed to insert chunk");

        // Verify the chunk was inserted by creating a new RiffFile
        let file_2_tmp_reopen = File::options()
            .read(true)
            .write(true)
            .open(_tmp_2.path())
            .expect("Failed to reopen temp file");
        let mut new_riff = RiffFile::new(Rc::new(RefCell::new(file_2_tmp_reopen)))
            .expect("Failed to create new RiffFile");

        assert!(
            new_riff
                .contains("id3 ")
                .expect("Failed to check id3 chunk")
        );

        let id3_chunk = new_riff.get_chunk("id3 ").expect("Failed to get id3 chunk");
        assert_eq!(id3_chunk.size(), 8);
        assert_eq!(id3_chunk.data_size(), 0);
    }

    #[test]
    fn test_insert_padded_chunks() {
        let test_case = TRiffFile::new();
        let (_riff_1, _riff_2, _tmp_1, _riff_1_tmp, tmp_2, mut riff_2_tmp) = test_case.setup();

        // Insert two test chunks
        let mut padded = riff_2_tmp
            .insert_chunk("TST1", None)
            .expect("Failed to insert TST1 chunk");
        let mut unpadded = riff_2_tmp
            .insert_chunk("TST2", None)
            .expect("Failed to insert TST2 chunk");

        // The second chunk needs no padding
        unpadded.resize(4).expect("Failed to resize unpadded chunk");
        assert_eq!(4, unpadded.data_size());
        assert_eq!(0, unpadded.padding());
        assert_eq!(12, unpadded.size());

        // Resize the first chunk so it needs padding
        padded.resize(3).expect("Failed to resize padded chunk");
        assert_eq!(3, padded.data_size());
        assert_eq!(1, padded.padding());
        assert_eq!(12, padded.size());
        assert_eq!(padded.offset() + padded.size() as u64, unpadded.offset());

        // Verify the padding byte gets written correctly
        let mut file = tmp_2.as_file();
        file.seek(SeekFrom::Start(padded.data_offset()))
            .expect("Failed to seek");
        file.write_all(b"ABCD").expect("Failed to write test data");
        padded
            .write(b"ABC")
            .expect("Failed to write to padded chunk");

        file.seek(SeekFrom::Start(padded.data_offset()))
            .expect("Failed to seek");
        let mut buffer = [0u8; 4];
        file.read_exact(&mut buffer).expect("Failed to read");
        assert_eq!(&buffer, b"ABC\x00");

        // Verify the second chunk got not overwritten
        file.seek(SeekFrom::Start(unpadded.offset()))
            .expect("Failed to seek");
        let mut buffer = [0u8; 4];
        file.read_exact(&mut buffer).expect("Failed to read");
        assert_eq!(&buffer, b"TST2");
    }

    #[test]
    fn test_delete_padded_chunks() {
        let test_case = TRiffFile::new();
        let (_riff_1, _riff_2, _tmp_1, _riff_1_tmp, tmp_2, mut riff_2_tmp) = test_case.setup();

        // Get initial root size
        let root_size_initial = riff_2_tmp.get_root().expect("Failed to get root").size();
        assert_eq!(root_size_initial, 64044);

        // Insert and resize first chunk
        riff_2_tmp
            .insert_chunk("TST ", None)
            .expect("Failed to insert TST chunk");
        let mut tst_chunk = riff_2_tmp
            .get_chunk("TST ")
            .expect("Failed to get TST chunk");
        tst_chunk.resize(3).expect("Failed to resize TST chunk"); // Resize to odd length, should insert 1 padding byte

        // Verify root size after first insertion
        let root_size_after_first = riff_2_tmp.get_root().expect("Failed to get root").size();
        assert_eq!(root_size_after_first, 64056);

        // Insert another chunk after the first one
        riff_2_tmp
            .insert_chunk("TST2", None)
            .expect("Failed to insert TST2 chunk");
        let mut tst2_chunk = riff_2_tmp
            .get_chunk("TST2")
            .expect("Failed to get TST2 chunk");
        tst2_chunk.resize(2).expect("Failed to resize TST2 chunk");

        let root_size_after_second = riff_2_tmp.get_root().expect("Failed to get root").size();
        assert_eq!(root_size_after_second, 64066);
        assert_eq!(tst_chunk.size(), 12);
        assert_eq!(tst_chunk.data_size(), 3);
        assert_eq!(tst_chunk.data_offset(), 64052);
        assert_eq!(tst2_chunk.size(), 10);
        assert_eq!(tst2_chunk.data_size(), 2);
        assert_eq!(tst2_chunk.data_offset(), 64064);

        // Delete the odd chunk
        riff_2_tmp
            .delete_chunk("TST ")
            .expect("Failed to delete TST chunk");
        let root_size_after_delete = riff_2_tmp.get_root().expect("Failed to get root").size();
        assert_eq!(root_size_after_delete, 64054);

        let tst2_chunk_after_delete = riff_2_tmp
            .get_chunk("TST2")
            .expect("Failed to get TST2 chunk after delete");
        assert_eq!(tst2_chunk_after_delete.size(), 10);
        assert_eq!(tst2_chunk_after_delete.data_size(), 2);
        assert_eq!(tst2_chunk_after_delete.data_offset(), 64052);

        // Reloading the file should give the same results
        let file_2_tmp_reopen = File::options()
            .read(true)
            .write(true)
            .open(tmp_2.path())
            .expect("Failed to reopen temp file");
        let mut new_riff_file = RiffFile::new(Rc::new(RefCell::new(file_2_tmp_reopen)))
            .expect("Failed to create new RiffFile");

        let new_root_size = new_riff_file
            .get_root()
            .expect("Failed to get new root")
            .size();
        assert_eq!(new_root_size, root_size_after_delete);

        let new_tst2_chunk = new_riff_file
            .get_chunk("TST2")
            .expect("Failed to get TST2 chunk from new file");
        assert_eq!(new_tst2_chunk.size(), tst2_chunk_after_delete.size());
        assert_eq!(
            new_tst2_chunk.data_size(),
            tst2_chunk_after_delete.data_size()
        );
        assert_eq!(
            new_tst2_chunk.data_offset(),
            tst2_chunk_after_delete.data_offset()
        );
    }

    #[test]
    fn test_read_list_info() {
        let test_case = TRiffFile::new();
        let (_riff_1, _riff_2, _tmp_1, mut riff_1_tmp, _tmp_2, _riff_2_tmp) = test_case.setup();

        // Get the LIST chunk
        let list_chunk = riff_1_tmp
            .get_chunk("LIST")
            .expect("Failed to get LIST chunk");

        // Verify we can get the LIST chunk
        assert_eq!(list_chunk.id(), "LIST");
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use audex::riff::RiffChunk;

    // --- Chunk read size limit tests ---

    #[test]
    fn test_riff_chunk_read_rejects_oversized_data() {
        let chunk = RiffChunk::new("data", 512 * 1024 * 1024, 0).unwrap();

        let mut tmpfile = tempfile::tempfile().unwrap();
        use std::io::Write;
        tmpfile.write_all(&[0u8; 64]).unwrap();

        let result = chunk.read(&mut tmpfile);

        assert!(result.is_err(), "Should reject chunk above size limit");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("limit"),
            "Error should mention size limit, got: {}",
            msg
        );
    }

    #[test]
    fn test_riff_chunk_read_accepts_normal_size() {
        let chunk = RiffChunk::new("data", 16, 0).unwrap();

        let mut tmpfile = tempfile::tempfile().unwrap();
        use std::io::Write;
        tmpfile.write_all(&[0u8; 24]).unwrap();

        let result = chunk.read(&mut tmpfile);
        assert!(result.is_ok(), "Normal-sized chunk should read fine");
    }

    // --- Container size arithmetic tests ---

    #[test]
    fn test_u32_subtraction_wraps_on_underflow() {
        let data_size: u32 = 100;
        let chunk_size: u32 = 200;

        let wrapped = data_size.wrapping_sub(chunk_size);
        assert_ne!(wrapped, 0, "Wrapping subtraction produces garbage");
        assert!(wrapped > u32::MAX / 2, "Wrapped value is huge: {}", wrapped);

        let checked = data_size.checked_sub(chunk_size);
        assert!(checked.is_none(), "checked_sub correctly detects underflow");
    }

    #[test]
    fn test_u32_addition_wraps_on_overflow() {
        let data_size: u32 = u32::MAX - 10;
        let chunk_size: u32 = 100;

        let wrapped = data_size.wrapping_add(chunk_size);
        assert!(
            wrapped < data_size,
            "Wrapping addition produces a smaller value: {}",
            wrapped
        );

        let checked = data_size.checked_add(chunk_size);
        assert!(checked.is_none(), "checked_add correctly detects overflow");
    }

    #[test]
    fn test_i64_to_u32_cast_wraps_on_negative() {
        let data_size: u32 = 100;
        let size_change: i64 = -200;

        let result_i64 = data_size as i64 + size_change;
        assert!(result_i64 < 0, "Result is negative: {}", result_i64);

        let cast_u32 = result_i64 as u32;
        assert!(
            cast_u32 > u32::MAX / 2,
            "Negative-to-u32 cast wraps: {}",
            cast_u32
        );
    }
}
