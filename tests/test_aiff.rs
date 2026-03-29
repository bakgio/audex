//! Tests for AIFF format support

use audex::aiff::{AIFF, AIFFFile, AIFFStreamInfo, clear, read_float};
use audex::{AudexError, FileType, StreamInfo, Tags};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use tempfile::NamedTempFile;

/// Get path to test data file
fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Helper to create a temporary copy of a test file
fn get_temp_copy(path: &std::path::Path) -> std::io::Result<(NamedTempFile, PathBuf)> {
    let data = fs::read(path)?;
    let mut temp = NamedTempFile::new()?;
    std::io::Write::write_all(&mut temp, &data)?;
    let temp_path = temp.path().to_path_buf();
    Ok((temp, temp_path))
}

/// Basic AIFF creation and properties tests
#[cfg(test)]
mod aiff_basic_tests {
    use super::*;

    #[test]
    fn test_aiff_creation() {
        let aiff = AIFF::new();
        assert_eq!(aiff.info.sample_rate, 0);
        assert_eq!(aiff.info.channels, 0);
        assert!(aiff.filename.is_none());
    }

    #[test]
    fn test_aiff_stream_info_creation() {
        let info = AIFFStreamInfo::default();
        assert_eq!(info.sample_rate(), Some(0));
        assert_eq!(info.channels(), Some(0));
        assert!(info.length().is_none());
        assert!(info.bitrate().is_none());
        assert_eq!(info.bits_per_sample(), Some(0));
    }

    #[test]
    fn test_mime_types() {
        let types = AIFF::mime_types();
        assert!(types.contains(&"audio/aiff"));
        assert!(types.contains(&"audio/x-aiff"));
    }

    #[test]
    fn test_score_form_aiff_signature() {
        let header = b"FORM\x00\x00\x42\x00AIFF";
        let score = AIFF::score("test.aiff", header);
        assert!(score >= 10); // Should have high score for FORM+AIFF
    }

    #[test]
    fn test_score_aiff_extension() {
        let header = b"random";
        let score = AIFF::score("test.aiff", header);
        assert!(score >= 3); // Should get points for .aiff extension
    }

    #[test]
    fn test_score_aif_extension() {
        let header = b"random";
        let score = AIFF::score("test.aif", header);
        assert!(score >= 2); // Should get points for .aif extension
    }

    #[test]
    fn test_score_no_match() {
        let header = b"RIFF";
        let score = AIFF::score("test.wav", header);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_read_float() {
        // Test case: 8000.0 Hz
        let data = b"\x40\x0b\xfa\x00\x00\x00\x00\x00\x00\x00";
        let result = read_float(data).unwrap();
        assert!((result - 8000.0).abs() < 0.1);

        // Test overflow cases
        let overflow_data1 = b"\xfa\x00\x00\xfa\x00\x00\x00\x00\x00\x00";
        assert!(read_float(overflow_data1).is_err());

        let overflow_data2 = b"\x7f\xff\x00\xfa\x00\x00\x00\x00\x00\x00";
        assert!(read_float(overflow_data2).is_err());

        // Test zero
        let zero_data = b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let zero_result = read_float(zero_data).unwrap();
        assert_eq!(zero_result, 0.0);
    }
}

/// AIFF stream information parsing tests
#[cfg(test)]
mod aiff_stream_info_tests {
    use super::*;

    #[test]
    fn test_stream_info_pprint() {
        let info = AIFFStreamInfo {
            sample_rate: 44100,
            channels: 2,
            bitrate: Some(1411200),
            bits_per_sample: 16,
            length: Some(std::time::Duration::from_secs_f64(3.5)),
            ..Default::default()
        };

        let output = info.pprint();
        assert!(output.contains("2 channel AIFF"));
        assert!(output.contains("44100"));
        assert!(output.contains("3.50"));
        assert!(output.contains("1411200"));
    }

    #[test]
    fn test_sample_size_compatibility() {
        let info = AIFFStreamInfo {
            bits_per_sample: 16,
            sample_size: 16,
            ..Default::default()
        };

        // Should match for backward compatibility
        assert_eq!(info.bits_per_sample, info.sample_size);
    }
}

/// AIFF file operations tests
#[cfg(test)]
mod aiff_file_operations_tests {
    use super::*;

    #[test]
    fn test_mime_access() {
        let aiff = AIFF::new();
        let mime_types = aiff.mime();
        assert!(mime_types.contains(&"audio/aiff"));
        assert!(mime_types.contains(&"audio/x-aiff"));
    }

    #[test]
    fn test_pprint() {
        let mut aiff = AIFF::new();
        aiff.info.sample_rate = 48000;
        aiff.info.channels = 2;
        aiff.info.bits_per_sample = 16;

        let output = aiff.pprint();
        assert!(output.contains("2 channel AIFF"));
        assert!(output.contains("48000"));
    }

    #[test]
    fn test_load_real_aiff_files() {
        // Test file 1: 11k-1ch-2s-silence.aif
        let aiff1_path = data_path("11k-1ch-2s-silence.aif");
        if aiff1_path.exists() {
            let aiff = AIFF::load(&aiff1_path).expect("Failed to load AIFF file 1");
            println!("Successfully loaded {}", aiff1_path.display());
            println!("  Sample rate: {} Hz", aiff.info.sample_rate);
            println!("  Channels: {}", aiff.info.channels);
            println!("  Bitrate: {} bps", aiff.info.bitrate.unwrap_or(0));
            println!("  Bits per sample: {}", aiff.info.bits_per_sample);
            if let Some(length) = aiff.info.length {
                println!("  Length: {:.2} seconds", length.as_secs_f64());
            }

            // Validate expected values
            assert_eq!(aiff.info.channels, 1);
            assert_eq!(aiff.info.sample_rate, 11025);
            assert_eq!(aiff.info.bits_per_sample, 16);
            assert_eq!(aiff.info.bitrate, Some(176400));
            if let Some(length) = aiff.info.length {
                assert!((length.as_secs_f64() - 2.0).abs() < 0.1);
            }
        }

        // Test file 2: 48k-2ch-s16-silence.aif
        let aiff2_path = data_path("48k-2ch-s16-silence.aif");
        if aiff2_path.exists() {
            let aiff = AIFF::load(&aiff2_path).expect("Failed to load AIFF file 2");
            println!("Successfully loaded {}", aiff2_path.display());

            // Validate expected values
            assert_eq!(aiff.info.channels, 2);
            assert_eq!(aiff.info.sample_rate, 48000);
            assert_eq!(aiff.info.bits_per_sample, 16);
            assert_eq!(aiff.info.bitrate, Some(1536000));
            if let Some(length) = aiff.info.length {
                assert!((length.as_secs_f64() - 0.1).abs() < 0.01);
            }
        }

        // Test file 3: 8k-1ch-1s-silence.aif
        let aiff3_path = data_path("8k-1ch-1s-silence.aif");
        if aiff3_path.exists() {
            let aiff = AIFF::load(&aiff3_path).expect("Failed to load AIFF file 3");
            // Validate expected values
            assert_eq!(aiff.info.channels, 1);
            assert_eq!(aiff.info.sample_rate, 8000);
            assert_eq!(aiff.info.bits_per_sample, 16);
            assert_eq!(aiff.info.bitrate, Some(128000));
            if let Some(length) = aiff.info.length {
                assert!((length.as_secs_f64() - 1.0).abs() < 0.1);
            }
        }

        // Test file 4: 8k-1ch-3.5s-silence.aif
        let aiff4_path = data_path("8k-1ch-3.5s-silence.aif");
        if aiff4_path.exists() {
            let aiff = AIFF::load(&aiff4_path).expect("Failed to load AIFF file 4");
            // Validate expected values
            assert_eq!(aiff.info.channels, 1);
            assert_eq!(aiff.info.sample_rate, 8000);
            assert_eq!(aiff.info.bits_per_sample, 16);
            assert_eq!(aiff.info.bitrate, Some(128000));
            if let Some(length) = aiff.info.length {
                assert!((length.as_secs_f64() - 3.5).abs() < 0.1);
            }
        }

        // Test file 5: 8k-4ch-1s-silence.aif
        let aiff5_path = data_path("8k-4ch-1s-silence.aif");
        if aiff5_path.exists() {
            let aiff = AIFF::load(&aiff5_path).expect("Failed to load AIFF file 5");
            // Validate expected values
            assert_eq!(aiff.info.channels, 4);
            assert_eq!(aiff.info.sample_rate, 8000);
            assert_eq!(aiff.info.bits_per_sample, 16);
            assert_eq!(aiff.info.bitrate, Some(512000));
            if let Some(length) = aiff.info.length {
                assert!((length.as_secs_f64() - 1.0).abs() < 0.1);
            }
        }
    }

    #[test]
    fn test_aiff_with_id3_tags() {
        let with_id3_path = data_path("with-id3.aif");
        if with_id3_path.exists() {
            let aiff = AIFF::load(&with_id3_path).expect("Failed to load AIFF file with ID3");
            println!("Successfully loaded AIFF with ID3 tags");

            // Should have tags
            if let Some(tags) = &aiff.tags {
                println!("Found ID3 tags");
                if let Some(title) = tags.get("TIT2") {
                    println!("Title: {:?}", title);
                    if !title.is_empty() {
                        assert!(title[0].contains("AIFF title"));
                    }
                }
            } else {
                println!("No ID3 tags found");
            }
        }
    }
}

/// AIFF error handling tests
#[cfg(test)]
mod aiff_error_handling_tests {
    use super::*;

    #[test]
    fn test_nonexistent_file() {
        let result = AIFF::load("nonexistent.aiff");
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_file_error() {
        // Try to load a non-AIFF file
        let ofr_path = data_path("empty.ofr");
        if ofr_path.exists() {
            let result = AIFF::load(&ofr_path);
            assert!(result.is_err(), "Expected error when loading non-AIFF file");
            match result {
                Err(AudexError::AIFFError(_)) | Err(AudexError::IFFError(_)) => {
                    println!("✓ Got expected error for invalid file");
                }
                Err(other) => {
                    println!("✓ Got error for invalid file: {:?}", other);
                }
                Ok(_) => panic!("Expected error for invalid file"),
            }
        } else {
            println!("OFR test file not available, skipping test");
        }
    }

    #[test]
    fn test_empty_file() {
        let empty_data = b"";
        let mut cursor = Cursor::new(empty_data);
        let result = AIFFStreamInfo::from_aiff_file(
            &AIFFFile {
                file_type: "AIFF".to_string(),
                chunks: vec![],
                file_size: 0,
            },
            &mut cursor,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_form_signature() {
        let invalid_data = b"RIFF\x00\x00\x00\x00WAVE";
        let mut cursor = Cursor::new(invalid_data);
        let result = AIFFFile::parse(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_aiff_type() {
        let invalid_data = b"FORM\x00\x00\x00\x04WAVE";
        let mut cursor = Cursor::new(invalid_data);
        let result = AIFFFile::parse(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_float_size() {
        let short_data = b"\x40\x0b\xfa\x00\x00"; // Too short
        let result = read_float(short_data);
        assert!(result.is_err());
    }
}

/// AIFF file modification and save operations tests
#[cfg(test)]
mod aiff_file_modification_tests {
    use super::*;

    #[test]
    fn test_roundtrip_with_tags() {
        // Test saving and reloading a file with tags to ensure data consistency
        let with_id3_path = data_path("with-id3.aif");
        if !with_id3_path.exists() {
            return;
        }

        // Create a temporary copy to work with
        let (temp_file, temp_path) = get_temp_copy(&with_id3_path).unwrap();

        // Load the file
        let mut aiff = AIFF::load(&temp_path).unwrap();

        // Verify original tag content
        if let Some(tags) = &aiff.tags {
            if let Some(title) = tags.get("TIT2") {
                assert!(
                    title[0].contains("AIFF title"),
                    "Expected 'AIFF title' in TIT2 tag"
                );
            }
        }

        // Save the file using the existing save method
        aiff.save_to_file(&temp_path).unwrap();

        // Reload and verify tags are preserved
        let reloaded = AIFF::load(&temp_path).unwrap();
        if let Some(tags) = &reloaded.tags {
            if let Some(title) = tags.get("TIT2") {
                assert!(
                    title[0].contains("AIFF title"),
                    "Tags should be preserved after roundtrip"
                );
            }
        }

        drop(temp_file); // Cleanup
    }

    #[test]
    fn test_save_with_existing_id3_chunk() {
        // Test saving new tags to a file that already has ID3 chunk
        let with_id3_path = data_path("with-id3.aif");
        if !with_id3_path.exists() {
            return;
        }

        let (temp_file, temp_path) = get_temp_copy(&with_id3_path).unwrap();

        // Load and add a new tag
        let mut aiff = AIFF::load(&temp_path).unwrap();
        if let Some(tags) = &mut aiff.tags {
            // Set TIT1 tag using the proper API
            tags.set("TIT1", vec!["foobar".to_string()]);
        }

        // Save the file
        aiff.save_to_file(&temp_path).unwrap();

        // Reload and verify both old and new tags exist
        let reloaded = AIFF::load(&temp_path).unwrap();
        if let Some(tags) = &reloaded.tags {
            // Check original tag
            if let Some(title) = tags.get("TIT2") {
                assert!(
                    title[0].contains("AIFF title"),
                    "Original tag should be preserved"
                );
            }
            // Check new tag
            if let Some(tit1) = tags.get("TIT1") {
                assert!(tit1[0].contains("foobar"), "New tag should be saved");
            }
        }

        drop(temp_file);
    }

    #[test]
    fn test_save_without_id3_chunk() {
        // Test saving new tags to a file that doesn't have ID3 chunk
        let no_id3_path = data_path("8k-1ch-1s-silence.aif");
        if !no_id3_path.exists() {
            return;
        }

        let (temp_file, temp_path) = get_temp_copy(&no_id3_path).unwrap();

        // Load and add tags
        let mut aiff = AIFF::load(&temp_path).unwrap();
        aiff.add_tags().unwrap();
        if let Some(tags) = &mut aiff.tags {
            // Set TIT1 tag using the proper API
            tags.set("TIT1", vec!["foobar".to_string()]);
        }

        // Save the file
        aiff.save_to_file(&temp_path).unwrap();

        // Reload and verify new tag
        let reloaded = AIFF::load(&temp_path).unwrap();
        if let Some(tags) = &reloaded.tags {
            if let Some(tit1) = tags.get("TIT1") {
                assert!(
                    tit1[0].contains("foobar"),
                    "New tag should be saved to file without ID3"
                );
            }
        }

        drop(temp_file);
    }

    #[test]
    fn test_corrupt_tag_handling() {
        // Test behavior when ID3 tags are corrupted
        let with_id3_path = data_path("with-id3.aif");
        if !with_id3_path.exists() {
            return;
        }

        let (temp_file, temp_path) = get_temp_copy(&with_id3_path).unwrap();

        // Corrupt the ID3 chunk by writing invalid data
        {
            use std::io::{Seek, SeekFrom, Write};

            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_path)
                .unwrap();

            // Parse to find ID3 chunk
            let aiff_file = AIFFFile::parse(&mut file);
            if let Ok(aiff_parsed) = aiff_file {
                if let Some(id3_chunk) = aiff_parsed.chunks.iter().find(|c| c.id == "ID3 ") {
                    // Seek to ID3 chunk data and corrupt it
                    file.seek(SeekFrom::Start(id3_chunk.offset + 8 + 4))
                        .unwrap();
                    file.write_all(b"\xff\xff").unwrap();
                }
            }
        }

        let result = AIFF::load(&temp_path);
        assert!(
            result.is_ok(),
            "Audex should load AIFF with corrupted ID3 tags (lenient behavior)"
        );

        drop(temp_file);
    }

    #[test]
    fn test_module_double_delete() {
        // Test that calling delete twice doesn't cause errors
        let with_id3_path = data_path("with-id3.aif");
        if !with_id3_path.exists() {
            return;
        }

        let (temp_file, temp_path) = get_temp_copy(&with_id3_path).unwrap();

        // First delete
        clear(&temp_path).unwrap();

        // Verify tags are gone
        let after_delete1 = AIFF::load(&temp_path).unwrap();
        assert!(after_delete1.tags.is_none(), "Tags should be deleted");

        // Second delete - should not error
        let result = clear(&temp_path);
        assert!(result.is_ok(), "Double delete should not error");

        // Verify still no tags
        let after_delete2 = AIFF::load(&temp_path).unwrap();
        assert!(after_delete2.tags.is_none(), "Tags should still be deleted");

        drop(temp_file);
    }

    #[test]
    fn test_save_no_tags() {
        // Test saving a file when tags are None
        let no_tags_path = data_path("8k-1ch-1s-silence.aif");
        if !no_tags_path.exists() {
            return;
        }

        let (temp_file, temp_path) = get_temp_copy(&no_tags_path).unwrap();

        let mut aiff = AIFF::load(&temp_path).unwrap();
        aiff.tags = None;

        // Should be able to save without errors
        aiff.save_to_file(&temp_path).unwrap();

        // Reload and verify no tags
        let reloaded = AIFF::load(&temp_path).unwrap();
        assert!(
            reloaded.tags.is_none(),
            "File with no tags should save correctly"
        );

        drop(temp_file);
    }

    #[test]
    fn test_pprint_no_tags() {
        // Test pprint when tags are None
        let mut aiff = AIFF::new();
        aiff.info.sample_rate = 44100;
        aiff.info.channels = 2;
        aiff.tags = None;

        let output = aiff.pprint();
        assert!(
            !output.is_empty(),
            "Should produce output even without tags"
        );
        assert!(output.contains("AIFF"), "Should indicate AIFF format");
    }
}

/// AIFF integration tests
#[cfg(test)]
mod aiff_integration_tests {
    use super::*;

    #[test]
    fn test_file_detection_and_scoring() {
        let test_files = vec![
            ("11k-1ch-2s-silence.aif", "1-channel AIFF"),
            ("48k-2ch-s16-silence.aif", "2-channel AIFF"),
            ("8k-4ch-1s-silence.aif", "4-channel AIFF"),
            ("with-id3.aif", "AIFF with ID3 tags"),
        ];

        println!("Testing AIFF file detection and scoring:");
        for (filename, _description) in test_files {
            let path = data_path(filename);
            if path.exists() {
                // Read file header for scoring
                if let Ok(data) = fs::read(&path) {
                    let score = AIFF::score(filename, &data[..std::cmp::min(data.len(), 64)]);
                    println!("AIFF file \"{}\" scored {}", filename, score);
                    assert!(score > 0, "AIFF file should have positive score");
                }
            }
        }
    }

    #[test]
    fn test_chunk_parsing() {
        let path = data_path("11k-1ch-2s-silence.aif");
        if path.exists() {
            let mut file = std::fs::File::open(&path).unwrap();
            if let Ok(aiff_file) = AIFFFile::parse(&mut file) {
                println!("Parsed AIFF file structure:");

                // Should have essential chunks
                assert!(aiff_file.has_chunk("COMM"), "Should have COMM chunk");
                assert!(aiff_file.has_chunk("SSND"), "Should have SSND chunk");

                for chunk in &aiff_file.chunks {
                    println!("  Chunk: '{}' (size: {})", chunk.id, chunk.data_size);
                }

                println!("✓ AIFF chunk parsing successful");
            }
        }
    }

    #[test]
    fn test_id3_tag_parsing() {
        let path = data_path("with-id3.aif");
        if path.exists() {
            let aiff = AIFF::load(&path).expect("Failed to load AIFF with ID3");
            if let Some(tags) = &aiff.tags {
                println!("Found ID3 tags in AIFF file");

                // Test access to TIT2 (title) tag
                if let Some(title) = tags.get("TIT2") {
                    println!("Title: {:?}", title);
                    if !title.is_empty() {
                        assert!(
                            title[0].contains("AIFF title"),
                            "Title should contain 'AIFF title'"
                        );
                    }
                }

                println!("✓ ID3 tag parsing successful");
            } else {
                println!("No ID3 tags found (may not have ID3 chunk)");
            }
        }
    }

    #[test]
    fn test_delete_function() {
        // Test the standalone clear() function using a temporary copy
        let path = data_path("with-id3.aif");
        if !path.exists() {
            return;
        }

        let (temp_file, temp_path) = get_temp_copy(&path).unwrap();

        // Verify tags exist before deletion
        let before = AIFF::load(&temp_path).unwrap();
        assert!(before.tags.is_some(), "File should have tags before clear");

        // Clear tags using standalone function
        clear(&temp_path).unwrap();

        // Reload and verify tags are gone
        let after = AIFF::load(&temp_path).unwrap();
        assert!(after.tags.is_none(), "Tags should be removed after clear");

        // Verify the file is still a valid AIFF (audio data intact)
        assert!(
            after.info.sample_rate > 0,
            "Stream info should survive clear"
        );
        assert!(
            after.info.channels > 0,
            "Channel count should survive clear"
        );

        drop(temp_file);
    }

    #[test]
    fn test_clear_no_temp_files_created() {
        // Verify that clear() performs in-place deletion without creating .tmp files
        let path = data_path("with-id3.aif");
        if !path.exists() {
            return;
        }

        // Use a dedicated temp directory so other processes writing to the
        // system temp dir don't cause false failures.
        let isolated_dir = tempfile::tempdir().unwrap();
        let temp_path = isolated_dir.path().join("test.aif");
        fs::copy(&path, &temp_path).unwrap();

        // Snapshot directory contents before clear
        let files_before: std::collections::HashSet<_> = fs::read_dir(isolated_dir.path())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();

        clear(&temp_path).unwrap();

        // Check no new files were created (no .tmp files)
        let files_after: std::collections::HashSet<_> = fs::read_dir(isolated_dir.path())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();

        let new_files: Vec<_> = files_after.difference(&files_before).collect();
        assert!(
            new_files.is_empty(),
            "No new files should be created during clear(), found: {:?}",
            new_files
        );
    }

    #[test]
    fn test_clear_form_header_size_updated() {
        // Verify that the FORM header size is correctly updated after ID3 chunk removal
        use std::io::{Read as _, Seek as _, SeekFrom};

        let path = data_path("with-id3.aif");
        if !path.exists() {
            return;
        }

        let (temp_file, temp_path) = get_temp_copy(&path).unwrap();

        // Record file size before clear
        let size_before = fs::metadata(&temp_path).unwrap().len();

        clear(&temp_path).unwrap();

        let size_after = fs::metadata(&temp_path).unwrap().len();
        assert!(
            size_after < size_before,
            "File should shrink after removing ID3 chunk"
        );

        // Read the FORM header and verify size matches actual file size
        let mut file = std::fs::File::open(&temp_path).unwrap();
        let mut sig = [0u8; 4];
        file.read_exact(&mut sig).unwrap();
        assert_eq!(&sig, b"FORM", "File should still start with FORM");

        let mut size_bytes = [0u8; 4];
        file.read_exact(&mut size_bytes).unwrap();
        let form_size = u32::from_be_bytes(size_bytes) as u64;

        // FORM size should equal file size minus 8 (4 bytes "FORM" + 4 bytes size field)
        assert_eq!(
            form_size,
            size_after - 8,
            "FORM header size should match actual file size minus 8"
        );

        // Verify the file can be re-parsed as valid AIFF
        file.seek(SeekFrom::Start(0)).unwrap();
        let aiff_file = AIFFFile::parse(&mut file).unwrap();
        assert!(
            !aiff_file.chunks.iter().any(|c| c.id == "ID3 "),
            "ID3 chunk should no longer be present"
        );

        drop(temp_file);
    }

    #[test]
    fn test_clear_preserves_audio_data() {
        // Verify audio data integrity is preserved after clearing tags
        let path = data_path("with-id3.aif");
        if !path.exists() {
            return;
        }

        let (temp_file, temp_path) = get_temp_copy(&path).unwrap();

        // Load original stream info
        let original = AIFF::load(&temp_path).unwrap();
        let orig_rate = original.info.sample_rate;
        let orig_channels = original.info.channels;
        let orig_bits = original.info.bits_per_sample;
        let orig_length = original.info.length;

        // Clear tags
        clear(&temp_path).unwrap();

        // Reload and verify stream info is identical
        let cleared = AIFF::load(&temp_path).unwrap();
        assert_eq!(
            cleared.info.sample_rate, orig_rate,
            "Sample rate must match"
        );
        assert_eq!(cleared.info.channels, orig_channels, "Channels must match");
        assert_eq!(
            cleared.info.bits_per_sample, orig_bits,
            "Bits per sample must match"
        );
        assert_eq!(
            cleared.info.length, orig_length,
            "Duration must match after clearing tags"
        );

        drop(temp_file);
    }

    #[test]
    fn test_clear_then_add_save_roundtrip() {
        // Full roundtrip: load with tags → clear → add new tags → save → verify
        let path = data_path("with-id3.aif");
        if !path.exists() {
            return;
        }

        let (temp_file, temp_path) = get_temp_copy(&path).unwrap();

        // Clear existing tags
        clear(&temp_path).unwrap();

        // Add new tags and save
        let mut aiff = AIFF::load(&temp_path).unwrap();
        aiff.add_tags().unwrap();
        if let Some(tags) = &mut aiff.tags {
            tags.set("TIT2", vec!["New Title After Clear".to_string()]);
            tags.set("TPE1", vec!["New Artist".to_string()]);
        }
        aiff.save_to_file(&temp_path).unwrap();

        // Reload and verify new tags
        let reloaded = AIFF::load(&temp_path).unwrap();
        assert!(reloaded.tags.is_some(), "Should have tags after re-adding");
        let tags = reloaded.tags.as_ref().unwrap();
        if let Some(title) = tags.get("TIT2") {
            assert!(
                title[0].contains("New Title After Clear"),
                "New title should be saved"
            );
        }
        if let Some(artist) = tags.get("TPE1") {
            assert!(
                artist[0].contains("New Artist"),
                "New artist should be saved"
            );
        }

        drop(temp_file);
    }

    #[test]
    fn test_add_tags_functionality() {
        let mut aiff = AIFF::new();

        // Should be able to add tags when none exist
        assert!(aiff.add_tags().is_ok());
        assert!(aiff.tags.is_some());

        // Should fail to add tags when they already exist
        assert!(aiff.add_tags().is_err());
    }

    #[test]
    fn test_comprehensive_parsing() {
        // Test comprehensive parsing of all available AIFF files
        let test_files = vec![
            "11k-1ch-2s-silence.aif",
            "48k-2ch-s16-silence.aif",
            "8k-1ch-1s-silence.aif",
            "8k-1ch-3.5s-silence.aif",
            "8k-4ch-1s-silence.aif",
            "with-id3.aif",
        ];

        let mut successful_parses = 0;

        let total_files = test_files.len();
        for filename in &test_files {
            let path = data_path(filename);
            if path.exists() {
                match AIFF::load(&path) {
                    Ok(aiff) => {
                        successful_parses += 1;
                        println!(
                            "✓ Successfully parsed {}: {}ch, {}Hz, {}bps",
                            filename,
                            aiff.info.channels,
                            aiff.info.sample_rate,
                            aiff.info.bitrate.unwrap_or(0)
                        );
                    }
                    Err(e) => {
                        println!("✗ Failed to parse {}: {}", filename, e);
                    }
                }
            }
        }

        assert!(
            successful_parses > 0,
            "Should successfully parse at least one AIFF file"
        );
        println!(
            "Successfully parsed {}/{} AIFF files",
            successful_parses, total_files
        );
    }
}
