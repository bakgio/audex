//! Tests for TAK format support

use audex::tak::{LSBBitReader, TAK, TAKStreamInfo, clear};
use audex::{AudexError, FileType};
use std::io::Cursor;
use std::path::PathBuf;

/// Get path to test data file
fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Basic TAK creation and properties tests
#[cfg(test)]
mod tak_basic_tests {
    use super::*;

    #[test]
    fn test_tak_creation() {
        let tak = TAK::new();
        assert!(tak.tags.is_none());
        assert_eq!(tak.info.channels, 0);
        assert_eq!(tak.info.sample_rate, 0);
        assert!(tak.info.length.is_none());
    }

    #[test]
    fn test_tak_stream_info_creation() {
        let info = TAKStreamInfo::default();
        assert_eq!(info.channels, 0);
        assert_eq!(info.sample_rate, 0);
        assert_eq!(info.bits_per_sample, 0);
        assert!(info.length.is_none());
        assert!(info.encoder_info.is_empty());
    }

    #[test]
    fn test_score_tbak_signature() {
        let header = b"tBaKabcdefghijklmnopqrstuvwxyz";
        let score = TAK::score("test.tak", header);
        assert_eq!(score, 2); // 1 for tBaK + 1 for .tak extension
    }

    #[test]
    fn test_score_tak_extension() {
        let header = b"NOTTAKabcdefghijklmnopqr";
        let score = TAK::score("test.tak", header);
        assert_eq!(score, 1); // 1 for .tak extension
    }

    #[test]
    fn test_score_no_match() {
        let header = b"NOTTAKabcdefghijklmnopqr";
        let score = TAK::score("test.wav", header);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_mime_types() {
        let mime_types = TAK::mime_types();
        assert!(mime_types.contains(&"audio/x-tak"));
    }

    #[test]
    fn test_lsb_bit_reader() {
        // Test LSB bit reading functionality
        let data = [0b10110101u8]; // Binary: 10110101
        let mut cursor = Cursor::new(&data);
        let mut reader = LSBBitReader::new(&mut cursor);

        // Read 1 bit (LSB) = 1
        assert_eq!(reader.bits(1).unwrap(), 1);

        // Read 2 bits = 10 (binary) = 2
        assert_eq!(reader.bits(2).unwrap(), 2);

        // Read 3 bits = 110 (binary) = 6
        assert_eq!(reader.bits(3).unwrap(), 6);

        // Read remaining 2 bits = 10 (binary) = 2
        assert_eq!(reader.bits(2).unwrap(), 2);

        println!("✓ LSB BitReader test passed");
    }

    #[test]
    fn test_lsb_bit_reader_multi_byte() {
        // Test reading across byte boundaries
        let data = [0b10110101u8, 0b11010010u8];
        let mut cursor = Cursor::new(&data);
        let mut reader = LSBBitReader::new(&mut cursor);

        // Read 12 bits across two bytes
        let value = reader.bits(12).unwrap();

        // Should read: first 8 bits from first byte + 4 bits from second byte
        // First byte LSB: 10110101 (0xB5 = 181)
        // Second byte first 4 bits LSB: 0010 (2)
        // Combined: 0010 10110101 = 0x2B5 = 693
        assert_eq!(value, 693);

        println!("✓ Multi-byte LSB BitReader test passed");
    }
}

/// TAK stream information tests
#[cfg(test)]
mod tak_stream_info_tests {
    use super::*;

    #[test]
    fn test_stream_info_pprint() {
        let info = TAKStreamInfo {
            sample_rate: 44100,
            bits_per_sample: 16,
            channels: 2,
            length: Some(std::time::Duration::from_secs_f64(3.68)),
            encoder_info: "TAK 2.3.0".to_string(),
            ..Default::default()
        };

        let output = info.pprint();
        assert!(output.contains("TAK 2.3.0"));
        assert!(output.contains("44100 Hz"));
        assert!(output.contains("16 bits"));
        assert!(output.contains("3.68 seconds"));
        assert!(output.contains("2 channel"));
        println!("Pprint output: {}", output);
    }

    #[test]
    fn test_stream_info_pprint_no_encoder() {
        let info = TAKStreamInfo {
            sample_rate: 48000,
            bits_per_sample: 24,
            channels: 1,
            length: Some(std::time::Duration::from_secs_f64(1.5)),
            ..Default::default()
        };
        // No encoder_info set

        let output = info.pprint();
        assert!(output.contains("TAK")); // Should default to "TAK"
        assert!(output.contains("48000 Hz"));
        assert!(output.contains("24 bits"));
        assert!(output.contains("1.50 seconds"));
        assert!(output.contains("1 channel"));
    }
}

/// TAK error handling tests
#[cfg(test)]
mod tak_error_handling_tests {
    use super::*;

    #[test]
    fn test_invalid_file_error() {
        let invalid_data = b"This is not a TAK file";
        let mut cursor = Cursor::new(invalid_data.as_slice());
        let result = TAKStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
        if let Err(AudexError::TAKHeaderError(msg)) = result {
            assert!(msg.contains("not a TAK file"));
        } else {
            panic!("Expected TAKHeaderError");
        }
    }

    #[test]
    fn test_empty_file() {
        let empty_data = b"";
        let mut cursor = Cursor::new(empty_data.as_slice());
        let result = TAKStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_header() {
        let truncated_data = b"tBa"; // Too short
        let mut cursor = Cursor::new(truncated_data.as_slice());
        let result = TAKStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_signature() {
        let invalid_data = b"XXXXabcdefghijklmnopqr"; // Wrong signature
        let mut cursor = Cursor::new(invalid_data.as_slice());
        let result = TAKStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
        if let Err(AudexError::TAKHeaderError(msg)) = result {
            assert!(msg.contains("not a TAK file"));
        }
    }

    #[test]
    fn test_fuzz_only_end_metadata() {
        // Test case: only END metadata should cause error
        let fuzz_data = b"tBaK\x00\x00\x00\x00"; // tBaK + END metadata
        let mut cursor = Cursor::new(fuzz_data.as_slice());
        let result = TAKStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
        if let Err(AudexError::TAKHeaderError(msg)) = result {
            assert!(msg.contains("missing stream info"));
        }
    }

    #[test]
    fn test_nonexistent_file() {
        let result = TAK::load("/nonexistent/file.tak");
        assert!(result.is_err());
    }

    #[test]
    fn test_not_my_file() {
        // Test with non-TAK files
        let test_files = vec!["empty.ogg", "click.mpc"];

        for filename in test_files {
            let path = data_path(filename);
            if path.exists() {
                match TAK::load(&path) {
                    Ok(_) => panic!("Should have failed to load {} as TAK", filename),
                    Err(e) => {
                        println!("✓ Got expected error for {}: {}", filename, e);
                        match e {
                            AudexError::TAKHeaderError(_) => {}
                            _ => println!("Note: Got different error type than expected: {:?}", e),
                        }
                    }
                }
            }
        }
    }
}

/// TAK file operations tests
#[cfg(test)]
mod tak_file_operations_tests {
    use super::*;

    #[test]
    fn test_load_real_tak_files() {
        // Test files based on reference data
        let test_files = vec![
            ("silence-44-s.tak", "TAK file without tags"),
            ("has-tags.tak", "TAK file with APEv2 tags"),
        ];

        for (filename, _description) in test_files {
            let path = data_path(filename);
            if path.exists() {
                let tak = TAK::load(&path)
                    .unwrap_or_else(|_| panic!("Failed to load TAK file {}", filename));
                println!("  Sample rate: {} Hz", tak.info.sample_rate);
                println!("  Channels: {}", tak.info.channels);
                println!("  Bits per sample: {}", tak.info.bits_per_sample);
                println!(
                    "  Length: {:.2} seconds",
                    tak.info.length.map(|d| d.as_secs_f64()).unwrap_or(0.0)
                );
                println!("  Encoder: {}", tak.info.encoder_info);
            } else {
                println!("Test file {} not found", filename);
            }
        }
    }

    #[test]
    fn test_tak_with_apev2_tags() {
        let path = data_path("has-tags.tak");
        if path.exists() {
            println!("Testing TAK with APEv2 tags");
            let tak = TAK::load(&path).expect("Failed to load has-tags.tak");
            if let Some(tags) = &tak.tags {
                println!("Found APEv2 tags");

                // Test access to common tags
                if let Some(title) = tags.get("TITLE") {
                    println!("Title: {:?}", title);
                }
                if let Some(artist) = tags.get("ARTIST") {
                    println!("Artist: {:?}", artist);
                }

                println!("✓ APEv2 tag parsing successful");
            } else {
                println!("No APEv2 tags found");
            }
        }
    }

    #[test]
    fn test_pprint() {
        let path = data_path("silence-44-s.tak");
        if path.exists() {
            let tak = TAK::load(&path).expect("Failed to load silence-44-s.tak");
            let output = tak.pprint();
            assert!(output.contains("TAK"));
            assert!(output.contains("Hz"));
            assert!(output.contains("bits"));
            assert!(output.contains("seconds"));
            assert!(output.contains("channel"));
            println!("Pprint output: {}", output);
        }
    }

    #[test]
    fn test_mime_access() {
        let path = data_path("silence-44-s.tak");
        if path.exists() {
            let tak = TAK::load(&path).expect("Failed to load silence-44-s.tak");
            let mime_types = tak.mime();
            assert!(mime_types.contains(&"audio/x-tak"));
        }
    }
}

/// TAK integration tests
#[cfg(test)]
mod tak_integration_tests {
    use super::*;

    #[test]
    fn test_comprehensive_parsing() {
        // Test comprehensive parsing of all available test files
        let test_files = vec!["silence-44-s.tak", "has-tags.tak"];

        let mut successful_parses = 0;
        let total_files = test_files.len();

        for filename in &test_files {
            let path = data_path(filename);
            if path.exists() {
                let tak =
                    TAK::load(&path).unwrap_or_else(|_| panic!("Failed to load {}", filename));
                successful_parses += 1;
                println!(
                    "✓ Successfully parsed {}: {}Hz, {}bit, {}ch, {:.2}s, {}",
                    filename,
                    tak.info.sample_rate,
                    tak.info.bits_per_sample,
                    tak.info.channels,
                    tak.info.length.map(|d| d.as_secs_f64()).unwrap_or(0.0),
                    tak.info.encoder_info
                );
            } else {
                println!("Test file {} not found", filename);
            }
        }

        if successful_parses > 0 {
            println!(
                "Successfully parsed {}/{} TAK files",
                successful_parses, total_files
            );
        } else {
            println!("No TAK files could be parsed (files may not exist)");
        }
    }

    #[test]
    fn test_add_tags_functionality() {
        let mut tak = TAK::new();

        // Test adding tags to empty file
        assert!(tak.add_tags().is_ok());
        assert!(tak.tags.is_some());

        // Test error when trying to add tags again
        assert!(tak.add_tags().is_err());
    }

    #[test]
    fn test_delete_functionality() {
        let mut tak = TAK::new();
        tak.add_tags().unwrap();

        // Test deleting tags
        assert!(tak.clear().is_ok());
        assert!(tak.tags.is_none());
    }

    #[test]
    fn test_delete_function() {
        println!("Delete function exists (full test requires writable file)");
        // Note: Full delete test would require a writable copy of a test file
        let result = clear("/nonexistent/file.tak");
        println!("Delete result: {:?}", result);
    }

    #[test]
    fn test_file_detection_and_scoring() {
        println!("Testing TAK file detection and scoring:");

        let test_files = vec!["silence-44-s.tak", "has-tags.tak"];

        for filename in test_files {
            let path = data_path(filename);
            if path.exists() {
                if let Ok(data) = std::fs::read(&path) {
                    let score = TAK::score(filename, &data[..std::cmp::min(data.len(), 32)]);
                    println!("TAK file \"{}\" scored {}", filename, score);
                }
            }
        }
    }

    #[test]
    fn test_expected_values_from_reference_tests() {
        // Test expected values

        // silence-44-s.tak (TAK without tags)
        let path = data_path("silence-44-s.tak");
        if path.exists() {
            let tak = TAK::load(&path).expect("Failed to load silence-44-s.tak");
            assert_eq!(
                tak.info.channels, 2,
                "silence-44-s.tak should have 2 channels"
            );
            assert_eq!(
                tak.info.sample_rate, 44100,
                "silence-44-s.tak should be 44100 Hz"
            );
            assert_eq!(
                tak.info.bits_per_sample, 16,
                "silence-44-s.tak should be 16 bits"
            );
            assert_eq!(
                tak.info.encoder_info, "TAK 2.3.0",
                "silence-44-s.tak should have TAK 2.3.0 encoder"
            );

            let length = tak.info.length.map(|d| d.as_secs_f64()).unwrap_or(0.0);
            assert!(
                (length - 3.68).abs() < 0.01,
                "silence-44-s.tak length should be ~3.68 seconds, got {}",
                length
            );

            println!("✓ silence-44-s.tak values match reference tests");
        }

        // has-tags.tak (TAK with APEv2 tags)
        let path = data_path("has-tags.tak");
        if path.exists() {
            let tak = TAK::load(&path).expect("Failed to load has-tags.tak");
            assert_eq!(tak.info.channels, 2, "has-tags.tak should have 2 channels");
            assert_eq!(
                tak.info.sample_rate, 44100,
                "has-tags.tak should be 44100 Hz"
            );
            assert_eq!(
                tak.info.bits_per_sample, 16,
                "has-tags.tak should be 16 bits"
            );
            assert_eq!(
                tak.info.encoder_info, "TAK 2.3.0",
                "has-tags.tak should have TAK 2.3.0 encoder"
            );

            let length = tak.info.length.map(|d| d.as_secs_f64()).unwrap_or(0.0);
            assert!(
                (length - 0.08).abs() < 0.01,
                "has-tags.tak length should be ~0.08 seconds, got {}",
                length
            );

            println!("✓ has-tags.tak values match reference tests");
        }
    }

    #[test]
    fn test_save_simulation() {
        let path = data_path("silence-44-s.tak");
        if !path.exists() {
            return;
        }
        let temp = tempfile::NamedTempFile::new().expect("Failed to create temp file");
        std::fs::copy(&path, temp.path()).expect("Failed to copy test file");

        let mut tak =
            TAK::load(temp.path()).expect("Failed to load silence-44-s.tak for save test");
        if tak.tags.is_none() {
            assert!(tak.add_tags().is_ok());
        }
        assert!(tak.tags.is_some());

        if let Some(_tags) = tak.tags_mut() {
            println!("Tag access works");
        }

        tak.save().expect("Failed to save TAK file");
        println!("Save operation completed");
    }

    #[test]
    fn test_bitreader_edge_cases() {
        // Test bit reader with various edge cases

        // Test reading 0 bits
        let data = [0u8];
        let mut cursor = Cursor::new(&data);
        let mut reader = LSBBitReader::new(&mut cursor);
        assert_eq!(reader.bits(0).unwrap(), 0);

        // Test reading full bytes
        let data = [0xFFu8];
        let mut cursor = Cursor::new(&data);
        let mut reader = LSBBitReader::new(&mut cursor);
        assert_eq!(reader.bits(8).unwrap(), 0xFF);

        // Test byte alignment
        let data = [0u8];
        let mut cursor = Cursor::new(&data);
        let reader = LSBBitReader::new(&mut cursor);
        assert!(reader.is_aligned());

        println!("✓ BitReader edge cases passed");
    }
}
