use audex::wavpack::{WavPack, WavPackStreamInfo, clear};
use audex::{AudexError, FileType, StreamInfo};
use std::io::Cursor;
use std::path::PathBuf;

/// Helper function to get test data path
fn data_path(filename: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("data");
    path.push(filename);
    path
}

/// Basic WavPack functionality tests
#[cfg(test)]
mod wavpack_basic_tests {
    use super::*;

    #[test]
    fn test_wavpack_creation() {
        let wavpack = WavPack::new();
        assert!(wavpack.tags.is_none());
        assert!(wavpack.filename.is_none());
        println!("✓ WavPack creation test passed");
    }

    #[test]
    fn test_wavpack_stream_info_creation() {
        let info = WavPackStreamInfo::default();
        assert_eq!(info.channels, 0);
        assert_eq!(info.sample_rate, 0);
        assert_eq!(info.bits_per_sample, 0);
        assert_eq!(info.version, 0);
        println!("✓ WavPack stream info creation test passed");
    }

    #[test]
    fn test_score_wvpk_signature() {
        let header = b"wvpktest";
        let score = WavPack::score("test.wv", header);
        assert_eq!(score, 3); // 2 for signature + 1 for extension
        println!("✓ WavPack signature scoring test passed");
    }

    #[test]
    fn test_score_wv_extension() {
        let header = b"notawav";
        let score = WavPack::score("test.wv", header);
        assert_eq!(score, 1); // 1 for extension only
        println!("✓ WavPack extension scoring test passed");
    }

    #[test]
    fn test_score_no_match() {
        let header = b"notawav";
        let score = WavPack::score("test.mp3", header);
        assert_eq!(score, 0);
        println!("✓ WavPack no match scoring test passed");
    }

    #[test]
    fn test_mime_types() {
        let mimes = WavPack::mime_types();
        assert_eq!(mimes, &["audio/x-wavpack"]);
        println!("✓ WavPack MIME types test passed");
    }
}

/// WavPack stream info parsing tests
#[cfg(test)]
mod wavpack_stream_info_tests {
    use super::*;

    #[test]
    fn test_stream_info_pprint() {
        let info = WavPackStreamInfo {
            sample_rate: 44100,
            length: Some(std::time::Duration::from_secs_f64(3.68)),
            ..Default::default()
        };

        let result = info.pprint();
        assert!(result.contains("WavPack"));
        assert!(result.contains("3.68 seconds"));
        assert!(result.contains("44100 Hz"));
        println!("✓ Stream info pprint test passed");
    }

    #[test]
    fn test_stream_info_pprint_no_length() {
        let info = WavPackStreamInfo {
            sample_rate: 44100,
            ..Default::default()
        };
        // No length set

        let result = info.pprint();
        assert!(result.contains("WavPack"));
        assert!(result.contains("0.00 seconds"));
        assert!(result.contains("44100 Hz"));
        println!("✓ Stream info pprint no length test passed");
    }
}

/// Error handling tests
#[cfg(test)]
mod wavpack_error_handling_tests {
    use super::*;

    #[test]
    fn test_invalid_signature() {
        let data = b"invalid_sig_test";
        let mut cursor = Cursor::new(data);
        let result = WavPackStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());

        match result.unwrap_err() {
            AudexError::WavPackHeaderError(_) => {
                println!("✓ Invalid signature error test passed");
            }
            _ => panic!("Expected WavPackHeaderError"),
        }
    }

    #[test]
    fn test_truncated_header() {
        let data = b"wvpktrunc"; // Too short for full header
        let mut cursor = Cursor::new(data);
        let result = WavPackStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
        println!("✓ Truncated header error test passed");
    }

    #[test]
    fn test_empty_file() {
        let data = b"";
        let mut cursor = Cursor::new(data);
        let result = WavPackStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
        println!("✓ Empty file error test passed");
    }

    #[test]
    fn test_nonexistent_file() {
        let result = WavPack::load("/nonexistent/file.wv");
        assert!(result.is_err());
        println!("✓ Nonexistent file error test passed");
    }

    #[test]
    fn test_invalid_rate_index() {
        // Create a minimal header with invalid rate index (15, which is out of bounds)
        let mut data = vec![0u8; 32];
        data[0..4].copy_from_slice(b"wvpk");
        // Set flags with rate index 15 (invalid)
        let flags = 15u32 << 23; // Rate index in bits 23-26
        data[24..28].copy_from_slice(&flags.to_le_bytes());

        let mut cursor = Cursor::new(&data);
        let result = WavPackStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());

        match result.unwrap_err() {
            AudexError::WavPackHeaderError(msg) => {
                assert!(msg.contains("invalid sample rate index"));
                println!("✓ Invalid rate index error test passed");
            }
            _ => panic!("Expected WavPackHeaderError with rate index message"),
        }
    }

    #[test]
    fn test_not_my_file() {
        // Test with non-WavPack files
        let test_files = vec!["empty.ogg", "click.mpc"];

        for filename in test_files {
            let path = data_path(filename);
            if path.exists() {
                match WavPack::load(&path) {
                    Ok(_) => panic!("Should have failed to load {} as WavPack", filename),
                    Err(e) => {
                        println!("✓ Got expected error for {}: {}", filename, e);
                        match e {
                            AudexError::WavPackHeaderError(_) => {}
                            _ => println!("Note: Got different error type than expected: {:?}", e),
                        }
                    }
                }
            }
        }
        println!("✓ Not my file test completed");
    }
}

/// WavPack file operations tests
#[cfg(test)]
mod wavpack_file_operations_tests {
    use super::*;

    #[test]
    fn test_load_real_wavpack_files() {
        // Test files based on reference data
        let test_files = vec![
            ("silence-44-s.wv", "Standard WavPack file"),
            ("no_length.wv", "WavPack file without length info"),
            ("dsd.wv", "DSD WavPack file"),
        ];

        for (filename, _description) in test_files {
            let path = data_path(filename);
            if path.exists() {
                let wavpack = WavPack::load(&path).expect("Failed to load WavPack file");
                println!("Successfully loaded {}", filename);
                println!("  Version: 0x{:x}", wavpack.info.version);
                println!("  Sample rate: {} Hz", wavpack.info.sample_rate);
                println!("  Channels: {}", wavpack.info.channels);
                println!("  Bits per sample: {}", wavpack.info.bits_per_sample);
                println!(
                    "  Length: {:.2} seconds",
                    wavpack.info.length.map(|d| d.as_secs_f64()).unwrap_or(0.0)
                );
            } else {
                println!("Test file {} not found", filename);
            }
        }
        println!("✓ Real WavPack file loading test completed");
    }

    #[test]
    fn test_wavpack_with_apev2_tags() {
        // Test tag functionality (basic)
        let mut wavpack = WavPack::new();
        assert!(wavpack.tags.is_none());

        // Add tags
        wavpack.add_tags().unwrap();
        assert!(wavpack.tags.is_some());

        // Try to add again (should fail)
        let result = wavpack.add_tags();
        assert!(result.is_err());

        // Delete tags
        wavpack.clear().unwrap();
        assert!(wavpack.tags.is_none());

        println!("✓ WavPack APEv2 tags test passed");
    }

    #[test]
    fn test_pprint() {
        let mut wavpack = WavPack::new();
        wavpack.info.sample_rate = 44100;
        wavpack.info.length = Some(std::time::Duration::from_secs_f64(3.68));

        let result = wavpack.pprint();
        assert!(result.contains("WavPack"));
        assert!(result.contains("3.68 seconds"));
        assert!(result.contains("44100 Hz"));
        println!("✓ WavPack pprint test passed");
    }

    #[test]
    fn test_mime_access() {
        let wavpack = WavPack::new();
        let mime_types = wavpack.mime();
        assert!(mime_types.contains(&"audio/x-wavpack"));
        println!("✓ WavPack MIME access test passed");
    }
}

/// Integration tests with expected values
#[cfg(test)]
mod wavpack_integration_tests {
    use super::*;

    #[test]
    fn test_expected_values_from_reference_tests() {
        // Expected values
        let expectations = vec![
            // (filename, version, channels, sample_rate, bits_per_sample, length_approx)
            ("silence-44-s.wv", 0x403, 2, 44100, 16, 3.68),
            ("no_length.wv", 0x407, 2, 44100, 16, 3.705),
            ("dsd.wv", 0x410, 2, 352800, 1, 0.01),
        ];

        for (
            filename,
            expected_version,
            expected_channels,
            expected_sample_rate,
            expected_bits,
            expected_length,
        ) in expectations
        {
            let path = data_path(filename);
            if path.exists() {
                match WavPack::load(&path) {
                    Ok(wavpack) => {
                        println!("Validating {}", filename);
                        assert_eq!(
                            wavpack.info.version, expected_version,
                            "Version mismatch for {}",
                            filename
                        );
                        assert_eq!(
                            wavpack.info.channels, expected_channels,
                            "Channels mismatch for {}",
                            filename
                        );
                        assert_eq!(
                            wavpack.info.sample_rate, expected_sample_rate,
                            "Sample rate mismatch for {}",
                            filename
                        );
                        assert_eq!(
                            wavpack.info.bits_per_sample, expected_bits,
                            "Bits per sample mismatch for {}",
                            filename
                        );

                        if let Some(length) = wavpack.info.length {
                            let length_secs = length.as_secs_f64();
                            assert!(
                                (length_secs - expected_length).abs() < 0.1,
                                "Length mismatch for {}: expected {}, got {}",
                                filename,
                                expected_length,
                                length_secs
                            );
                        }

                        println!("✓ {} validated successfully", filename);
                    }
                    Err(e) => {
                        panic!("Failed to load expected test file {}: {}", filename, e);
                    }
                }
            } else {
                println!(
                    "Warning: Test file {} not found, skipping validation",
                    filename
                );
            }
        }
        println!("✓ reference test expectations validation completed");
    }

    #[test]
    fn test_delete_function() {
        // Test the standalone delete function using a temporary copy
        let path = data_path("silence-44-s.wv");
        if !path.exists() {
            println!("Warning: Test file not found for delete function test");
            return;
        }

        let data = std::fs::read(&path).unwrap();
        let mut temp = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut temp, &data).unwrap();
        let temp_path = temp.path().to_path_buf();

        let result = clear(&temp_path);
        assert!(result.is_ok());
        println!("✓ Delete function test passed");
    }

    #[test]
    fn test_comprehensive_parsing() {
        // Comprehensive test of all parsing functionality
        let path = data_path("silence-44-s.wv");
        if path.exists() {
            match WavPack::load(&path) {
                Ok(wavpack) => {
                    // Test all StreamInfo trait methods
                    assert!(wavpack.info().length().is_some());
                    assert!(wavpack.info().sample_rate().is_some());
                    assert!(wavpack.info().channels().is_some());
                    assert!(wavpack.info().bits_per_sample().is_some());
                    // bitrate is not calculated for WavPack, should be None
                    assert!(wavpack.info().bitrate().is_none());

                    // Test FileType trait methods
                    // Note: test file may or may not have tags, just test the interface works
                    let _has_tags = wavpack.tags().is_some();

                    // Test scoring
                    let score = WavPack::score("test.wv", b"wvpk");
                    assert!(score > 0);

                    // Test MIME types
                    let mimes = WavPack::mime_types();
                    assert!(!mimes.is_empty());

                    println!("✓ Comprehensive parsing test passed");
                }
                Err(e) => panic!("Comprehensive parsing test failed: {}", e),
            }
        } else {
            println!("Warning: Test file not found for comprehensive parsing test");
        }
    }

    #[test]
    fn test_save_simulation() {
        let path = data_path("silence-44-s.wv");
        if !path.exists() {
            return;
        }
        let temp = tempfile::NamedTempFile::new().expect("Failed to create temp file");
        std::fs::copy(&path, temp.path()).expect("Failed to copy test file");

        let mut wavpack = WavPack::load(temp.path()).expect("Failed to load WavPack for save test");
        let result = wavpack.save();
        assert!(result.is_ok());
        println!("Save simulation test passed");
    }

    #[test]
    fn test_delete_functionality() {
        // Test delete functionality
        let mut wavpack = WavPack::new();

        // Add some tags first
        wavpack.add_tags().unwrap();
        assert!(wavpack.tags.is_some());

        // Test FileType::delete
        wavpack.clear().unwrap();
        assert!(wavpack.tags.is_none());

        println!("✓ Delete functionality test passed");
    }

    #[test]
    fn test_add_tags_functionality() {
        // Test tag addition functionality
        let mut wavpack = WavPack::new();
        assert!(wavpack.tags.is_none());

        // Add tags
        wavpack.add_tags().unwrap();
        assert!(wavpack.tags.is_some());

        // Get mutable tag reference
        if let Some(tags) = wavpack.tags_mut() {
            // Basic tag interface test
            assert!(tags.is_empty());
        }

        println!("✓ Add tags functionality test passed");
    }

    #[test]
    fn test_file_detection_and_scoring() {
        // Test file detection and scoring system
        let test_cases = vec![
            ("test.wv", b"wvpk", 3),  // Perfect match
            ("test.wv", b"wave", 1),  // Extension only
            ("test.wav", b"wvpk", 2), // Signature only
            ("test.mp3", b"mp3x", 0), // No match
        ];

        for (filename, header, expected_score) in test_cases {
            let score = WavPack::score(filename, header);
            assert_eq!(
                score, expected_score,
                "Score mismatch for {} with header {:?}",
                filename, header
            );
        }

        println!("✓ File detection and scoring test passed");
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use std::io::Cursor;

    fn build_wavpack_with_zero_block_size(block_count: usize) -> Vec<u8> {
        let mut data = Vec::new();

        for _i in 0..block_count {
            data.extend_from_slice(b"wvpk");
            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(&0x0410u16.to_le_bytes());
            data.push(0);
            data.push(0);

            data.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());

            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(&100u32.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
        }

        data
    }

    #[test]
    fn test_zero_block_size_does_not_slow_crawl() {
        let data = build_wavpack_with_zero_block_size(500);
        let mut cursor = Cursor::new(data);

        use audex::wavpack::WavPackStreamInfo;
        let start = std::time::Instant::now();
        let _result = WavPackStreamInfo::from_reader(&mut cursor);
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_secs() < 2,
            "Block scanning took {:?} -- likely crawling on zero-size blocks",
            elapsed
        );
    }
}
