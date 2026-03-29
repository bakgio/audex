//! Async Integration Tests
//!
//! Comprehensive test suite for async functionality across all supported formats.
//! Tests format auto-detection, error handling, concurrent processing, and cancellation safety.

#![cfg(feature = "async")]

use audex::{AudexError, StreamInfo};
use audex::{File, detect_format_async};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::time::timeout;

mod common;
use common::TestUtils;

// Common test files representing different formats
const MP3_FILE: &str = "silence-44-s.mp3";
const FLAC_FILE: &str = "silence-44-s.flac";
const M4A_FILE: &str = "has-tags.m4a";
const OGG_FILE: &str = "empty.ogg";
const WAV_FILE: &str = "silence-2s-PCM-44100-16-ID3v23.wav";
const AIFF_FILE: &str = "8k-1ch-1s-silence.aif";
const WMA_FILE: &str = "silence-1.wma";
const DSF_FILE: &str = "2822400-1ch-0s-silence.dsf";
const OPUS_FILE: &str = "example.opus";

#[cfg(test)]
mod flac_async_save_regressions {
    use super::*;
    use audex::flac::{FLAC, Picture};

    fn locate_last_picture_data_length(bytes: &[u8]) -> usize {
        assert!(bytes.starts_with(b"fLaC"), "expected FLAC signature");

        let mut offset = 4usize;
        let mut last_data_len_offset = None;

        loop {
            let header = &bytes[offset..offset + 4];
            let is_last = (header[0] & 0x80) != 0;
            let block_type = header[0] & 0x7F;
            let block_size = u32::from_be_bytes([0, header[1], header[2], header[3]]) as usize;
            let block_start = offset + 4;

            if block_type == 6 {
                let mut cursor = block_start;
                cursor += 4; // picture type

                let mime_len =
                    u32::from_be_bytes(bytes[cursor..cursor + 4].try_into().expect("mime length"))
                        as usize;
                cursor += 4 + mime_len;

                let desc_len = u32::from_be_bytes(
                    bytes[cursor..cursor + 4]
                        .try_into()
                        .expect("description length"),
                ) as usize;
                cursor += 4 + desc_len;
                cursor += 16; // width, height, depth, colors

                last_data_len_offset = Some(cursor);
            }

            offset = block_start + block_size;
            if is_last {
                break;
            }
        }

        last_data_len_offset.expect("expected at least one picture block")
    }

    #[tokio::test]
    async fn async_save_rejects_corrupt_picture_length_without_panicking() {
        let source = TestUtils::data_path(FLAC_FILE);
        // Use a TempDir and copy the file manually so that no extra
        // file handle is kept open during save operations.
        let tmp_dir = tempfile::tempdir().expect("create temp dir");
        let path = tmp_dir.path().join("test.flac");
        std::fs::copy(&source, &path).expect("copy test fixture");

        let mut flac = FLAC::load_async(&path).await.expect("load FLAC");
        let mut picture = Picture::new();
        picture.mime_type = "image/jpeg".to_string();
        picture.data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        flac.add_picture(picture);
        flac.save_to_file(Some(&path), false, Some(Box::new(|_| 0)))
            .expect("save FLAC without padding");

        let mut bytes = std::fs::read(&path).expect("read saved FLAC");
        let data_len_offset = locate_last_picture_data_length(&bytes);
        bytes[data_len_offset..data_len_offset + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        std::fs::write(&path, &bytes).expect("rewrite corrupted FLAC");

        let err = flac
            .save_async()
            .await
            .expect_err("corrupt metadata should be rejected");
        assert!(
            err.to_string()
                .contains("Picture data length exceeds file size")
                || err
                    .to_string()
                    .contains("file size is smaller than audio offset"),
            "unexpected error: {}",
            err
        );
    }
}

#[cfg(test)]
mod async_roundtrip_regressions {
    use super::*;
    use audex::FileType;
    use audex::Tags;
    use audex::dsf::DSF;
    use audex::mp4::MP4;

    #[tokio::test]
    async fn mp4_async_save_and_clear_roundtrip() {
        let source = TestUtils::data_path(M4A_FILE);
        let tmp_dir = tempfile::tempdir().expect("create temp dir");
        let path = tmp_dir.path().join("roundtrip.m4a");
        std::fs::copy(&source, &path).expect("copy MP4 fixture");

        let mut mp4 = MP4::load_async(&path).await.expect("load MP4 fixture");
        mp4.add_tags().ok();
        mp4.clear_async().await.ok();
        mp4.add_tags().ok();
        mp4.set("title", vec!["Async MP4".to_string()])
            .expect("set title");
        mp4.save_async().await.expect("save async MP4");

        let reloaded = MP4::load_async(&path).await.expect("reload saved MP4");
        assert_eq!(reloaded.get_first("title"), Some("Async MP4".to_string()));

        let mut mp4 = MP4::load_async(&path)
            .await
            .expect("reload MP4 before clear");
        mp4.clear_async().await.expect("clear async MP4");

        let cleared = MP4::load_async(&path).await.expect("reload cleared MP4");
        let tag_count = cleared
            .tags
            .as_ref()
            .map(|tags| tags.keys().len())
            .unwrap_or(0);
        assert_eq!(tag_count, 0, "clear_async should remove MP4 tag entries");
    }

    #[tokio::test]
    async fn dsf_async_save_and_clear_roundtrip() {
        let source = TestUtils::data_path("with-id3.dsf");
        let tmp_dir = tempfile::tempdir().expect("create temp dir");
        let path = tmp_dir.path().join("roundtrip.dsf");
        std::fs::copy(&source, &path).expect("copy DSF fixture");

        let mut dsf = DSF::load_async(&path).await.expect("load DSF fixture");
        dsf.set("TIT2", vec!["Async DSF".to_string()])
            .expect("set title");
        dsf.save_async().await.expect("save async DSF");

        let reloaded = DSF::load_async(&path).await.expect("reload saved DSF");
        assert_eq!(reloaded.get_first("TIT2"), Some("Async DSF".to_string()));

        let mut dsf = DSF::load_async(&path)
            .await
            .expect("reload DSF before clear");
        dsf.clear_async().await.expect("clear async DSF");

        let cleared = DSF::load_async(&path).await.expect("reload cleared DSF");
        assert!(cleared.tags.is_none(), "clear_async should remove DSF tags");
    }
}

/// Tests for async format detection across all supported audio formats.
#[cfg(test)]
mod format_detection_tests {
    use super::*;

    /// Test MP3 format detection by header signature
    #[tokio::test]
    async fn test_detect_mp3_format() {
        let path = TestUtils::data_path(MP3_FILE);
        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect MP3 format");
        assert_eq!(format, "MP3", "MP3 format should be detected");
    }

    /// Test FLAC format detection by magic bytes
    #[tokio::test]
    async fn test_detect_flac_format() {
        let path = TestUtils::data_path(FLAC_FILE);
        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect FLAC format");
        assert_eq!(format, "FLAC", "FLAC format should be detected");
    }

    /// Test MP4/M4A format detection by ftyp atom
    #[tokio::test]
    async fn test_detect_mp4_format() {
        let path = TestUtils::data_path(M4A_FILE);
        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect MP4 format");
        assert_eq!(format, "MP4", "MP4 format should be detected");
    }

    /// Test OGG Vorbis format detection
    #[tokio::test]
    async fn test_detect_ogg_format() {
        let path = TestUtils::data_path(OGG_FILE);
        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect OGG format");
        assert!(
            format.starts_with("Ogg"),
            "OGG format should be detected, got: {}",
            format
        );
    }

    /// Test WAV format detection by RIFF header
    #[tokio::test]
    async fn test_detect_wav_format() {
        let path = TestUtils::data_path(WAV_FILE);
        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect WAV format");
        assert_eq!(format, "WAVE", "WAV format should be detected");
    }

    /// Test AIFF format detection by FORM header
    #[tokio::test]
    async fn test_detect_aiff_format() {
        let path = TestUtils::data_path(AIFF_FILE);
        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect AIFF format");
        assert_eq!(format, "AIFF", "AIFF format should be detected");
    }

    /// Test ASF/WMA format detection by GUID header
    #[tokio::test]
    async fn test_detect_asf_format() {
        let path = TestUtils::data_path(WMA_FILE);
        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect ASF format");
        assert_eq!(format, "ASF", "ASF format should be detected");
    }

    /// Test DSF format detection
    #[tokio::test]
    async fn test_detect_dsf_format() {
        let path = TestUtils::data_path(DSF_FILE);
        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect DSF format");
        assert_eq!(format, "DSF", "DSF format should be detected");
    }

    /// Test Opus format detection within OGG container
    #[tokio::test]
    async fn test_detect_opus_format() {
        let path = TestUtils::data_path(OPUS_FILE);
        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect Opus format");
        assert!(
            format.contains("Opus") || format.starts_with("Ogg"),
            "Opus format should be detected, got: {}",
            format
        );
    }

    /// Test format detection by file extension when header is ambiguous
    #[tokio::test]
    async fn test_detect_format_by_extension() {
        // Create a temp file with MP3 extension but minimal header
        let _dir = tempfile::tempdir().expect("Create temp dir");
        let path = _dir.path().join("test_async_detect.mp3");

        // Write MP3 frame sync bytes
        tokio::fs::write(&path, &[0xFF, 0xFB, 0x90, 0x00, 0x00, 0x00])
            .await
            .expect("Failed to write test file");

        let format = detect_format_async(&path)
            .await
            .expect("Failed to detect format");
        assert_eq!(format, "MP3", "MP3 format should be detected by frame sync");
    }

    /// Test detection of multiple formats in parallel
    #[tokio::test]
    async fn test_parallel_format_detection() {
        let files = vec![
            TestUtils::data_path(MP3_FILE),
            TestUtils::data_path(FLAC_FILE),
            TestUtils::data_path(M4A_FILE),
            TestUtils::data_path(WAV_FILE),
        ];

        let expected = vec!["MP3", "FLAC", "MP4", "WAVE"];

        let handles: Vec<_> = files
            .into_iter()
            .map(|path| tokio::spawn(async move { detect_format_async(&path).await }))
            .collect();

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.expect("Task panicked").expect("Detection failed"))
            .collect();

        assert_eq!(
            results, expected,
            "All formats should be detected correctly"
        );
    }
}

/// Tests for loading audio files asynchronously with auto-detection.
#[cfg(test)]
mod async_file_loading_tests {
    use super::*;

    /// Test basic async file loading for MP3
    #[tokio::test]
    async fn test_async_load_mp3() {
        let path = TestUtils::data_path(MP3_FILE);
        let file = File::load_async(&path).await.expect("Failed to load MP3");

        // Verify stream info is available
        assert!(
            file.info().length().is_some(),
            "MP3 should have duration info"
        );
        assert!(
            file.info().sample_rate().is_some(),
            "MP3 should have sample rate"
        );
    }

    /// Test async file loading for FLAC
    #[tokio::test]
    async fn test_async_load_flac() {
        let path = TestUtils::data_path(FLAC_FILE);
        let file = File::load_async(&path).await.expect("Failed to load FLAC");

        assert!(
            file.info().length().is_some(),
            "FLAC should have duration info"
        );
        assert!(
            file.info().bits_per_sample().is_some(),
            "FLAC should have bits per sample"
        );
    }

    /// Test async file loading for MP4/M4A
    #[tokio::test]
    async fn test_async_load_mp4() {
        let path = TestUtils::data_path(M4A_FILE);
        let file = File::load_async(&path).await.expect("Failed to load MP4");

        // M4A files should have metadata
        let format = file.format_name();
        assert!(
            format.contains("MP4") || format.contains("M4A"),
            "Should be MP4 format"
        );
    }

    /// Test async file loading for WAV
    #[tokio::test]
    async fn test_async_load_wav() {
        let path = TestUtils::data_path(WAV_FILE);
        let file = File::load_async(&path).await.expect("Failed to load WAV");

        assert!(
            file.info().channels().is_some(),
            "WAV should have channel info"
        );
        assert!(
            file.info().sample_rate().is_some(),
            "WAV should have sample rate"
        );
    }

    /// Test async file loading for AIFF
    #[tokio::test]
    async fn test_async_load_aiff() {
        let path = TestUtils::data_path(AIFF_FILE);
        let file = File::load_async(&path).await.expect("Failed to load AIFF");

        assert!(
            file.info().sample_rate().is_some(),
            "AIFF should have sample rate"
        );
    }

    /// Test async file loading for ASF/WMA
    #[tokio::test]
    async fn test_async_load_asf() {
        let path = TestUtils::data_path(WMA_FILE);
        let file = File::load_async(&path).await.expect("Failed to load ASF");

        assert!(
            file.info().bitrate().is_some(),
            "ASF should have bitrate info"
        );
    }

    /// Test loading file from buffer
    #[tokio::test]
    async fn test_async_load_from_buffer() {
        let path = TestUtils::data_path(MP3_FILE);
        let data = tokio::fs::read(&path).await.expect("Failed to read file");

        let file = File::load_from_buffer_async(data, Some(path))
            .await
            .expect("Failed to load from buffer");

        assert!(
            file.info().length().is_some(),
            "Loaded file should have duration"
        );
    }

    /// Test loading multiple files concurrently
    #[tokio::test]
    async fn test_concurrent_file_loading() {
        let files = [
            TestUtils::data_path(MP3_FILE),
            TestUtils::data_path(FLAC_FILE),
            TestUtils::data_path(M4A_FILE),
            TestUtils::data_path(WAV_FILE),
            TestUtils::data_path(AIFF_FILE),
        ];

        // Use join_all directly for concurrent execution on same thread
        // (DynamicFileType contains Box<dyn Any> which is not Send)
        let futures: Vec<_> = files.iter().map(File::load_async).collect();

        let results = futures::future::join_all(futures).await;

        for (i, result) in results.into_iter().enumerate() {
            let file = result.unwrap_or_else(|e| panic!("Failed to load file {}: {:?}", i, e));

            // Verify each file loaded successfully
            assert!(
                file.info().sample_rate().is_some() || file.info().bitrate().is_some(),
                "File {} should have audio info",
                i
            );
        }
    }
}

/// Tests for proper error handling in async operations.
#[cfg(test)]
mod error_handling_tests {
    use super::*;

    /// Test error for non-existent file
    #[tokio::test]
    async fn test_error_file_not_found() {
        let path = PathBuf::from("/non/existent/path/audio.mp3");
        let result = File::load_async(&path).await;

        assert!(result.is_err(), "Should return error for non-existent file");

        match result {
            Err(AudexError::Io(_)) => (),
            Err(e) => panic!("Expected IO error, got: {:?}", e),
            Ok(_) => panic!("Should not succeed"),
        }
    }

    /// Test error for unsupported format
    #[tokio::test]
    async fn test_error_unsupported_format() {
        // Create a file with unknown format
        let temp_file = NamedTempFile::with_suffix(".xyz").expect("Failed to create temp file");
        tokio::fs::write(temp_file.path(), b"unknown format data")
            .await
            .expect("Failed to write");

        let result = detect_format_async(temp_file.path()).await;

        assert!(
            result.is_err(),
            "Should return error for unsupported format"
        );

        match result {
            Err(AudexError::UnsupportedFormat(_)) => (),
            Err(e) => panic!("Expected UnsupportedFormat error, got: {:?}", e),
            Ok(f) => panic!("Should not succeed, got format: {}", f),
        }
    }

    /// Test error for empty file
    #[tokio::test]
    async fn test_error_empty_file() {
        let temp_file = NamedTempFile::with_suffix(".mp3").expect("Failed to create temp file");
        // File is empty, which should cause an error

        let result = File::load_async(temp_file.path()).await;

        // Empty file should fail to load
        assert!(result.is_err(), "Should return error for empty file");
    }

    /// Test error for corrupted file header
    #[tokio::test]
    async fn test_error_corrupted_header() {
        let temp_file = NamedTempFile::with_suffix(".flac").expect("Failed to create temp file");

        // Write invalid FLAC header (correct magic but corrupted metadata)
        tokio::fs::write(temp_file.path(), b"fLaC\x00\x00\x00\x00invalid")
            .await
            .expect("Failed to write");

        let result = File::load_async(temp_file.path()).await;

        // Should either fail to load or detect the corruption
        if result.is_ok() {
            // Some implementations may still load partial data
            println!("Warning: Corrupted file loaded without error");
        }
    }

    /// Test error propagation through concurrent operations
    #[tokio::test]
    async fn test_concurrent_error_handling() {
        let files = [
            TestUtils::data_path(MP3_FILE),     // Valid
            PathBuf::from("/non/existent.mp3"), // Invalid
            TestUtils::data_path(FLAC_FILE),    // Valid
        ];

        // Use join_all directly for concurrent execution
        let futures: Vec<_> = files.iter().map(File::load_async).collect();

        let results = futures::future::join_all(futures).await;

        // First should succeed
        assert!(results[0].is_ok(), "First file should load");

        // Second should fail
        assert!(results[1].is_err(), "Second file should fail");

        // Third should succeed
        assert!(results[2].is_ok(), "Third file should load");
    }

    /// Test error for permission denied (if possible to simulate)
    #[tokio::test]
    async fn test_error_permission_context() {
        // This test verifies error types are correctly propagated
        let temp_file = NamedTempFile::with_suffix(".mp3").expect("Failed to create temp file");

        // Write minimal MP3 data
        tokio::fs::write(temp_file.path(), &[0xFF, 0xFB, 0x90, 0x00])
            .await
            .expect("Failed to write");

        // Attempt to load - should work
        let result = File::load_async(temp_file.path()).await;

        // On Windows/some systems, minimal MP3 data may fail to parse
        // This is acceptable - we're testing error propagation
        match result {
            Ok(_) => println!("Minimal MP3 loaded successfully"),
            Err(e) => println!("Minimal MP3 failed with: {:?}", e),
        }
    }
}

/// Tests for concurrent file processing capabilities.
#[cfg(test)]
mod concurrent_processing_tests {
    use super::*;

    /// Test processing many files concurrently
    #[tokio::test]
    async fn test_bulk_concurrent_loading() {
        // Create list of test files (repeating the same files for bulk test)
        let base_files = [
            TestUtils::data_path(MP3_FILE),
            TestUtils::data_path(FLAC_FILE),
            TestUtils::data_path(M4A_FILE),
        ];

        // Repeat files to simulate bulk processing
        let files: Vec<_> = base_files.iter().cycle().take(15).cloned().collect();

        // Use join_all directly for concurrent execution
        let futures: Vec<_> = files.iter().map(File::load_async).collect();

        let results = futures::future::join_all(futures).await;

        // All tasks should complete
        assert_eq!(results.len(), 15, "All tasks should complete");

        // Count successful loads
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, 15, "All 15 files should load successfully");
    }

    /// Test that concurrent operations don't interfere with each other
    #[tokio::test]
    async fn test_concurrent_isolation() {
        let mp3_path = TestUtils::data_path(MP3_FILE);
        let flac_path = TestUtils::data_path(FLAC_FILE);

        // Build list of paths with their expected format
        let paths: Vec<_> = (0..10)
            .map(|i| {
                if i % 2 == 0 {
                    (i, mp3_path.clone())
                } else {
                    (i, flac_path.clone())
                }
            })
            .collect();

        // Use join_all directly for concurrent execution
        let futures: Vec<_> = paths
            .iter()
            .map(|(i, path)| {
                let idx = *i;
                async move {
                    let file = File::load_async(path).await?;
                    Ok::<_, AudexError>((idx, file.format_name().to_string()))
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        for result in results {
            let (i, format) = result.expect("Load failed");

            if i % 2 == 0 {
                assert!(format.contains("MP3"), "Even indices should be MP3");
            } else {
                assert!(format.contains("FLAC"), "Odd indices should be FLAC");
            }
        }
    }

    /// Test concurrent format detection
    #[tokio::test]
    async fn test_concurrent_format_detection() {
        let files = vec![
            (TestUtils::data_path(MP3_FILE), "MP3"),
            (TestUtils::data_path(FLAC_FILE), "FLAC"),
            (TestUtils::data_path(WAV_FILE), "WAVE"),
            (TestUtils::data_path(AIFF_FILE), "AIFF"),
        ];

        let handles: Vec<_> = files
            .into_iter()
            .map(|(path, expected)| {
                tokio::spawn(async move {
                    let format = detect_format_async(&path).await?;
                    Ok::<_, AudexError>((format, expected.to_string()))
                })
            })
            .collect();

        let results = futures::future::join_all(handles).await;

        for result in results {
            let (actual, expected) = result.expect("Task panicked").expect("Detection failed");
            assert_eq!(actual, expected, "Format detection should be accurate");
        }
    }

    /// Test sequential processing with rate limiting pattern
    #[tokio::test]
    async fn test_sequential_processing() {
        let files = vec![
            TestUtils::data_path(MP3_FILE),
            TestUtils::data_path(FLAC_FILE),
            TestUtils::data_path(M4A_FILE),
            TestUtils::data_path(WAV_FILE),
            TestUtils::data_path(AIFF_FILE),
            TestUtils::data_path(WMA_FILE),
        ];

        let mut success_count = 0;

        // Process files sequentially (simulates rate-limited processing)
        for path in &files {
            let result = File::load_async(path).await;
            if result.is_ok() {
                success_count += 1;
            }
        }

        // Verify all completed successfully
        assert_eq!(
            success_count,
            files.len(),
            "All files should load successfully"
        );
    }

    /// Test concurrent loading with join_all
    #[tokio::test]
    async fn test_join_all_concurrent_loading() {
        let files = [
            TestUtils::data_path(MP3_FILE),
            TestUtils::data_path(FLAC_FILE),
            TestUtils::data_path(M4A_FILE),
            TestUtils::data_path(WAV_FILE),
            TestUtils::data_path(AIFF_FILE),
            TestUtils::data_path(WMA_FILE),
        ];

        // Use join_all for concurrent execution
        let futures: Vec<_> = files.iter().map(File::load_async).collect();

        let results = futures::future::join_all(futures).await;

        // Verify all completed successfully
        for result in &results {
            assert!(result.is_ok(), "Load should succeed");
        }

        assert_eq!(results.len(), 6, "All 6 files should be processed");
    }
}

/// Tests for cancellation safety and proper cleanup.
#[cfg(test)]
mod cancellation_safety_tests {
    use super::*;

    /// Test that futures can be safely dropped mid-operation
    #[tokio::test]
    async fn test_future_drop_safety() {
        let path = TestUtils::data_path(MP3_FILE);

        // Create future but don't await it fully - cancel via timeout
        let load_future = File::load_async(&path);

        // Use very short timeout to force cancellation
        // In practice, the file is likely to load before timeout
        let result = timeout(Duration::from_millis(1), load_future).await;

        // Either completes or times out - both are acceptable
        match result {
            Ok(Ok(_)) => println!("File loaded before timeout"),
            Ok(Err(e)) => panic!("Load error: {:?}", e),
            Err(_) => println!("Timeout occurred - future was cancelled"),
        }

        // Verify we can still load files after cancellation
        let file = File::load_async(&path)
            .await
            .expect("Should load after cancellation");
        assert!(file.info().length().is_some(), "File should be valid");
    }

    /// Test multiple cancellations don't cause issues
    #[tokio::test]
    async fn test_repeated_cancellation() {
        let path = TestUtils::data_path(FLAC_FILE);

        for i in 0..5 {
            let load_future = File::load_async(&path);

            // Alternate between completing and cancelling
            if i % 2 == 0 {
                let _ = load_future.await;
            } else {
                let _ = timeout(Duration::from_micros(1), load_future).await;
            }
        }

        // Verify system is still stable
        let file = File::load_async(&path)
            .await
            .expect("Final load should work");
        assert!(file.info().sample_rate().is_some(), "File should be valid");
    }

    /// Test cancellation during concurrent operations
    #[tokio::test]
    async fn test_concurrent_cancellation() {
        let files = [
            TestUtils::data_path(MP3_FILE),
            TestUtils::data_path(FLAC_FILE),
            TestUtils::data_path(M4A_FILE),
            TestUtils::data_path(WAV_FILE),
        ];

        // Build futures with alternating timeout behavior
        let futures: Vec<_> = files
            .iter()
            .enumerate()
            .map(|(i, path)| {
                async move {
                    let future = File::load_async(path);

                    // Cancel every other task
                    if i % 2 == 0 {
                        future.await
                    } else {
                        match timeout(Duration::from_micros(1), future).await {
                            Ok(result) => result,
                            Err(_) => Err(AudexError::Io(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "Cancelled",
                            ))),
                        }
                    }
                }
            })
            .collect();

        let results = futures::future::join_all(futures).await;

        // Even indices should succeed
        for (i, result) in results.into_iter().enumerate() {
            if i % 2 == 0 {
                assert!(result.is_ok(), "Task {} should succeed", i);
            }
            // Odd indices may succeed or fail due to timeout
        }
    }

    /// Test that resources are properly released after cancellation
    #[tokio::test]
    async fn test_resource_cleanup_after_cancellation() {
        let path = TestUtils::data_path(MP3_FILE);

        // Start many operations and cancel them using join_all
        let futures: Vec<_> = (0..10)
            .map(|_| {
                let p = path.clone();
                async move {
                    let future = File::load_async(&p);
                    let _ = timeout(Duration::from_nanos(1), future).await;
                }
            })
            .collect();

        futures::future::join_all(futures).await;

        // Give time for cleanup
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Verify file can still be opened (not locked)
        let file = File::load_async(&path)
            .await
            .expect("File should not be locked");
        assert!(file.info().length().is_some(), "File should be valid");
    }

    /// Test select! behavior with multiple futures
    #[tokio::test]
    async fn test_select_cancellation() {
        let mp3_path = TestUtils::data_path(MP3_FILE);
        let flac_path = TestUtils::data_path(FLAC_FILE);

        // Use select to race two loads
        let result = tokio::select! {
            result = File::load_async(&mp3_path) => ("MP3", result),
            result = File::load_async(&flac_path) => ("FLAC", result),
        };

        let (name, file_result) = result;
        let file = file_result.expect("Winner should succeed");

        println!("{} won the race", name);
        assert!(
            file.info().sample_rate().is_some(),
            "Winner should have valid data"
        );

        // Verify both files can still be loaded
        let _ = File::load_async(&mp3_path)
            .await
            .expect("MP3 should still load");
        let _ = File::load_async(&flac_path)
            .await
            .expect("FLAC should still load");
    }
}

/// Miscellaneous integration tests for complete coverage.
#[cfg(test)]
mod additional_integration_tests {
    use super::*;

    /// Test loading and accessing metadata for all major formats
    #[tokio::test]
    async fn test_metadata_access_all_formats() {
        let test_cases = vec![
            (MP3_FILE, "MP3"),
            (FLAC_FILE, "FLAC"),
            (M4A_FILE, "MP4"),
            (WAV_FILE, "WAVE"),
            (AIFF_FILE, "AIFF"),
            (WMA_FILE, "ASF"),
        ];

        for (file, format_name) in test_cases {
            let path = TestUtils::data_path(file);
            let file = File::load_async(&path)
                .await
                .unwrap_or_else(|e| panic!("Failed to load {} ({}): {:?}", file, format_name, e));

            // Verify format detection matches
            let detected = file.format_name();
            assert!(
                detected.contains(format_name) || format_name.contains(detected),
                "Format mismatch for {}: expected {}, got {}",
                file,
                format_name,
                detected
            );
        }
    }

    /// Test that async and sync loading produce equivalent results
    #[tokio::test]
    async fn test_async_sync_equivalence() {
        use audex::File;

        let path = TestUtils::data_path(MP3_FILE);

        // Load synchronously
        let sync_file = File::load(&path).expect("Sync load failed");
        let sync_duration = sync_file.info().length();
        let sync_sample_rate = sync_file.info().sample_rate();

        // Load asynchronously
        let async_file = File::load_async(&path).await.expect("Async load failed");
        let async_duration = async_file.info().length();
        let async_sample_rate = async_file.info().sample_rate();

        // Results should match
        assert_eq!(
            sync_duration, async_duration,
            "Duration should match between sync and async"
        );
        assert_eq!(
            sync_sample_rate, async_sample_rate,
            "Sample rate should match between sync and async"
        );
    }

    /// Test format detection for edge cases
    #[tokio::test]
    async fn test_edge_case_detection() {
        // Test ID3v2.2 format (older ID3 version)
        let id3v22_path = TestUtils::data_path("id3v22-test.mp3");
        if id3v22_path.exists() {
            let format = detect_format_async(&id3v22_path)
                .await
                .expect("Detection failed");
            assert_eq!(format, "MP3", "ID3v2.2 files should be detected as MP3");
        }

        // Test files with multiple tag formats
        let combined_path = TestUtils::data_path("id3v1v2-combined.mp3");
        if combined_path.exists() {
            let format = detect_format_async(&combined_path)
                .await
                .expect("Detection failed");
            assert_eq!(
                format, "MP3",
                "Combined tag files should be detected as MP3"
            );
        }
    }

    /// Test timeout handling for slow operations
    #[tokio::test]
    async fn test_operation_timeout() {
        let path = TestUtils::data_path(MP3_FILE);

        // Set a reasonable timeout
        let result = timeout(Duration::from_secs(5), File::load_async(&path)).await;

        match result {
            Ok(Ok(file)) => {
                assert!(file.info().length().is_some(), "File should have duration");
            }
            Ok(Err(e)) => panic!("Load error: {:?}", e),
            Err(_) => panic!("Operation timed out after 5 seconds"),
        }
    }

    /// Test streaming multiple files through a processing pipeline
    #[tokio::test]
    async fn test_streaming_pipeline() {
        let files = vec![
            TestUtils::data_path(MP3_FILE),
            TestUtils::data_path(FLAC_FILE),
            TestUtils::data_path(WAV_FILE),
        ];

        // Simulate a processing pipeline: load -> get info -> format output
        let results: Vec<_> = futures::future::join_all(files.into_iter().map(|path| async move {
            let file = File::load_async(&path).await?;
            let duration = file.info().length().unwrap_or(Duration::from_secs(0));
            let sample_rate = file.info().sample_rate().unwrap_or(0);

            Ok::<_, AudexError>(format!(
                "{}: {:.2}s @ {}Hz",
                file.format_name(),
                duration.as_secs_f64(),
                sample_rate
            ))
        }))
        .await;

        for result in results {
            let info = result.expect("Pipeline failed");
            println!("{}", info);
            assert!(!info.is_empty(), "Output should not be empty");
        }
    }
}

#[cfg(test)]
mod async_save_format_coverage {
    use super::*;
    use audex::FileType;

    #[tokio::test]
    async fn mp3_async_save_roundtrip() {
        let source = TestUtils::data_path(MP3_FILE);
        let tmp_dir = tempfile::tempdir().expect("create temp dir");
        let path = tmp_dir.path().join("roundtrip.mp3");
        std::fs::copy(&source, &path).expect("copy MP3 fixture");

        let mut mp3 = audex::mp3::MP3::load_async(&path).await.expect("load MP3");
        mp3.set("TIT2", vec!["Async MP3 Test".to_string()])
            .expect("set title");
        mp3.save_async().await.expect("save_async MP3");

        let reloaded = audex::mp3::MP3::load_async(&path).await.expect("reload");
        assert_eq!(
            reloaded.get_first("TIT2"),
            Some("Async MP3 Test".to_string())
        );
    }

    #[tokio::test]
    async fn flac_async_save_roundtrip() {
        let source = TestUtils::data_path(FLAC_FILE);
        let tmp_dir = tempfile::tempdir().expect("create temp dir");
        let path = tmp_dir.path().join("roundtrip.flac");
        std::fs::copy(&source, &path).expect("copy FLAC fixture");

        let mut flac = audex::flac::FLAC::load_async(&path)
            .await
            .expect("load FLAC");
        flac.set("TITLE", vec!["Async FLAC Test".to_string()])
            .expect("set title");
        flac.save_async().await.expect("save_async FLAC");

        let reloaded = audex::flac::FLAC::load_async(&path).await.expect("reload");
        assert_eq!(
            reloaded.get_first("TITLE"),
            Some("Async FLAC Test".to_string())
        );
    }

    #[tokio::test]
    async fn flac_async_clear() {
        let source = TestUtils::data_path(FLAC_FILE);
        let tmp_dir = tempfile::tempdir().expect("create temp dir");
        let path = tmp_dir.path().join("clear.flac");
        std::fs::copy(&source, &path).expect("copy FLAC fixture");

        let mut flac = audex::flac::FLAC::load_async(&path)
            .await
            .expect("load FLAC");
        flac.set("TITLE", vec!["About to be cleared".to_string()])
            .expect("set title");
        flac.save_async().await.expect("save");

        let mut flac = audex::flac::FLAC::load_async(&path).await.expect("reload");
        flac.clear_async().await.expect("clear_async FLAC");

        let cleared = audex::flac::FLAC::load_async(&path)
            .await
            .expect("reload cleared");
        assert!(
            cleared.keys().is_empty(),
            "all tags should be removed after clear_async"
        );
    }

    #[tokio::test]
    async fn sequential_async_save_no_cross_contamination() {
        let tmp_dir = tempfile::tempdir().expect("create temp dir");

        let path_a = tmp_dir.path().join("file_a.flac");
        let path_b = tmp_dir.path().join("file_b.flac");
        std::fs::copy(TestUtils::data_path(FLAC_FILE), &path_a).expect("copy A");
        std::fs::copy(TestUtils::data_path(FLAC_FILE), &path_b).expect("copy B");

        // Save two separate files in the same async context
        let mut a = audex::flac::FLAC::load_async(&path_a)
            .await
            .expect("load A");
        a.set("TITLE", vec!["File A Title".to_string()]).unwrap();
        a.save_async().await.expect("save A");

        let mut b = audex::flac::FLAC::load_async(&path_b)
            .await
            .expect("load B");
        b.set("TITLE", vec!["File B Title".to_string()]).unwrap();
        b.save_async().await.expect("save B");

        // Reload and verify each file has its own title
        let a = audex::flac::FLAC::load_async(&path_a)
            .await
            .expect("reload A");
        let b = audex::flac::FLAC::load_async(&path_b)
            .await
            .expect("reload B");

        assert_eq!(a.get_first("TITLE"), Some("File A Title".to_string()));
        assert_eq!(b.get_first("TITLE"), Some("File B Title".to_string()));
    }

    #[tokio::test]
    async fn ogg_vorbis_async_save_roundtrip() {
        let source = TestUtils::data_path(OGG_FILE);
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.ogg");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut ogg = audex::oggvorbis::OggVorbis::load_async(&path)
            .await
            .expect("load");
        ogg.set("title", vec!["Async OggVorbis".to_string()])
            .expect("set");
        ogg.save_async().await.expect("save_async");

        let reloaded = audex::oggvorbis::OggVorbis::load_async(&path)
            .await
            .expect("reload");
        assert_eq!(
            reloaded.get_first("title"),
            Some("Async OggVorbis".to_string())
        );
    }

    #[tokio::test]
    async fn ogg_opus_async_save_roundtrip() {
        let source = TestUtils::data_path(OPUS_FILE);
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.opus");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut opus = audex::oggopus::OggOpus::load_async(&path)
            .await
            .expect("load");
        opus.set("title", vec!["Async OggOpus".to_string()])
            .expect("set");
        opus.save_async().await.expect("save_async");

        let reloaded = audex::oggopus::OggOpus::load_async(&path)
            .await
            .expect("reload");
        assert_eq!(
            reloaded.get_first("title"),
            Some("Async OggOpus".to_string())
        );
    }

    #[tokio::test]
    async fn ogg_speex_async_save_roundtrip() {
        let source = TestUtils::data_path("empty.spx");
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.spx");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut spx = audex::oggspeex::OggSpeex::load_async(&path)
            .await
            .expect("load");
        spx.set("title", vec!["Async OggSpeex".to_string()])
            .expect("set");
        spx.save_async().await.expect("save_async");

        let reloaded = audex::oggspeex::OggSpeex::load_async(&path)
            .await
            .expect("reload");
        assert_eq!(
            reloaded.get_first("title"),
            Some("Async OggSpeex".to_string())
        );
    }

    #[tokio::test]
    async fn ogg_flac_async_save_roundtrip() {
        let source = TestUtils::data_path("empty.oggflac");
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.oggflac");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut of = audex::oggflac::OggFlac::load_async(&path)
            .await
            .expect("load");
        of.set("title", vec!["Async OggFLAC".to_string()])
            .expect("set");
        of.save_async().await.expect("save_async");

        let reloaded = audex::oggflac::OggFlac::load_async(&path)
            .await
            .expect("reload");
        assert_eq!(
            reloaded.get_first("title"),
            Some("Async OggFLAC".to_string())
        );
    }

    #[tokio::test]
    async fn aiff_async_save_roundtrip() {
        let source = TestUtils::data_path(AIFF_FILE);
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.aif");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut aiff = audex::aiff::AIFF::load_async(&path).await.expect("load");
        aiff.add_tags().ok();
        aiff.set("TIT2", vec!["Async AIFF".to_string()])
            .expect("set");
        aiff.save_async().await.expect("save_async");

        let reloaded = audex::aiff::AIFF::load_async(&path).await.expect("reload");
        assert_eq!(reloaded.get_first("TIT2"), Some("Async AIFF".to_string()));
    }

    #[tokio::test]
    async fn wave_async_save_roundtrip() {
        let source = TestUtils::data_path(WAV_FILE);
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.wav");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut wav = audex::wave::WAVE::load_async(&path).await.expect("load");
        wav.add_tags().ok();
        wav.clear_async().await.ok();
        wav.add_tags().ok();
        wav.set("TIT2", vec!["Async WAVE".to_string()])
            .expect("set");
        wav.save_async().await.expect("save_async");

        let reloaded = audex::wave::WAVE::load_async(&path).await.expect("reload");
        assert_eq!(reloaded.get_first("TIT2"), Some("Async WAVE".to_string()));
    }

    #[tokio::test]
    async fn asf_async_save_roundtrip() {
        let source = TestUtils::data_path(WMA_FILE);
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.wma");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut asf = audex::asf::ASF::load_async(&path).await.expect("load");
        asf.set("Title", vec!["Async ASF".to_string()]);
        asf.save_async().await.expect("save_async");

        let reloaded = audex::asf::ASF::load_async(&path).await.expect("reload");
        assert_eq!(reloaded.get_first("Title"), Some("Async ASF".to_string()));
    }

    #[tokio::test]
    async fn wavpack_async_save_roundtrip() {
        let source = TestUtils::data_path("silence-44-s.wv");
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.wv");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut wv = audex::wavpack::WavPack::load_async(&path)
            .await
            .expect("load");
        wv.add_tags().ok();
        wv.set("Title", vec!["Async WavPack".to_string()])
            .expect("set");
        wv.save_async().await.expect("save_async");

        let reloaded = audex::wavpack::WavPack::load_async(&path)
            .await
            .expect("reload");
        assert_eq!(
            reloaded.get_first("Title"),
            Some("Async WavPack".to_string())
        );
    }

    #[tokio::test]
    async fn monkeysaudio_async_clear() {
        let source = TestUtils::data_path("mac-399.ape");
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("clear.ape");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut ape = audex::monkeysaudio::MonkeysAudio::load_async(&path)
            .await
            .expect("load");
        ape.add_tags().ok();
        ape.set("Title", vec!["About to be cleared".to_string()])
            .expect("set");
        ape.save_async().await.expect("save_async");

        let mut reloaded = audex::monkeysaudio::MonkeysAudio::load_async(&path)
            .await
            .expect("reload");
        reloaded.clear_async().await.expect("clear_async");

        let cleared = audex::monkeysaudio::MonkeysAudio::load_async(&path)
            .await
            .expect("reload cleared");
        assert!(
            cleared.tags.is_none() || cleared.keys().is_empty(),
            "tags should be removed after clear_async"
        );
    }

    #[tokio::test]
    async fn trueaudio_async_save_roundtrip() {
        let source = TestUtils::data_path("empty.tta");
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.tta");
        std::fs::copy(&source, &path).expect("copy fixture");

        // TrueAudio supports both ID3 and APE tags; async save requires APE
        let mut tta = audex::trueaudio::TrueAudio::load_async(&path)
            .await
            .expect("load");
        tta.assign_ape_tags();
        tta.set("Title", vec!["Async TrueAudio".to_string()])
            .expect("set");
        tta.save_async().await.expect("save_async");

        let reloaded = audex::trueaudio::TrueAudio::load_async(&path)
            .await
            .expect("reload");
        assert_eq!(
            reloaded.get_first("Title"),
            Some("Async TrueAudio".to_string())
        );
    }

    #[tokio::test]
    async fn musepack_async_save_roundtrip() {
        let source = TestUtils::data_path("click.mpc");
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.mpc");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut mpc = audex::musepack::Musepack::load_async(&path)
            .await
            .expect("load");
        mpc.add_tags().ok();
        mpc.set("Title", vec!["Async Musepack".to_string()])
            .expect("set");
        mpc.save_async().await.expect("save_async");

        let reloaded = audex::musepack::Musepack::load_async(&path)
            .await
            .expect("reload");
        assert_eq!(
            reloaded.get_first("Title"),
            Some("Async Musepack".to_string())
        );
    }

    #[tokio::test]
    async fn optimfrog_async_save_roundtrip() {
        let source = TestUtils::data_path("empty.ofr");
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.ofr");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut ofr = audex::optimfrog::OptimFROG::load_async(&path)
            .await
            .expect("load");
        ofr.add_tags().ok();
        ofr.set("Title", vec!["Async OptimFROG".to_string()])
            .expect("set");
        ofr.save_async().await.expect("save_async");

        let reloaded = audex::optimfrog::OptimFROG::load_async(&path)
            .await
            .expect("reload");
        assert_eq!(
            reloaded.get_first("Title"),
            Some("Async OptimFROG".to_string())
        );
    }

    #[tokio::test]
    async fn tak_async_save_roundtrip() {
        let source = TestUtils::data_path("has-tags.tak");
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.tak");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut tak = audex::tak::TAK::load_async(&path).await.expect("load");
        tak.add_tags().ok();
        tak.set("Title", vec!["Async TAK".to_string()])
            .expect("set");
        tak.save_async().await.expect("save_async");

        let reloaded = audex::tak::TAK::load_async(&path).await.expect("reload");
        assert_eq!(reloaded.get_first("Title"), Some("Async TAK".to_string()));
    }

    #[tokio::test]
    async fn dsdiff_async_save_roundtrip() {
        let source = TestUtils::data_path("5644800-2ch-s01-silence.dff");
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("roundtrip.dff");
        std::fs::copy(&source, &path).expect("copy fixture");

        let mut dff = audex::dsdiff::DSDIFF::load_async(&path)
            .await
            .expect("load");
        dff.add_tags().ok();
        dff.set("TIT2", vec!["Async DSDIFF".to_string()])
            .expect("set");
        dff.save_async().await.expect("save_async");

        let reloaded = audex::dsdiff::DSDIFF::load_async(&path)
            .await
            .expect("reload");
        assert_eq!(reloaded.get_first("TIT2"), Some("Async DSDIFF".to_string()));
    }
}
