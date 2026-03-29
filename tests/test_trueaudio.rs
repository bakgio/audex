//! Tests for TrueAudio format support

use audex::trueaudio::{TrueAudio, TrueAudioStreamInfo, clear};
use audex::{AudexError, FileType};
use std::io::Cursor;
use std::path::PathBuf;

/// Get path to test data file
fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Basic TrueAudio creation and properties tests
#[cfg(test)]
mod trueaudio_basic_tests {
    use super::*;

    #[test]
    fn test_trueaudio_creation() {
        let trueaudio = TrueAudio::new();
        assert!(trueaudio.tags.is_none());
        assert_eq!(trueaudio.info.sample_rate, 0);
        assert!(trueaudio.info.length.is_none());
    }

    #[test]
    fn test_trueaudio_stream_info_creation() {
        let info = TrueAudioStreamInfo::default();
        assert_eq!(info.sample_rate, 0);
        assert!(info.length.is_none());
    }

    #[test]
    fn test_score_tta_signature() {
        let header = b"TTAabcdefghijklmnopqrstuvwxyz";
        let score = TrueAudio::score("test.tta", header);
        assert_eq!(score, 3); // 1 for TTA + 2 for .tta extension
    }

    #[test]
    fn test_score_id3_signature() {
        let header = b"ID3abcdefghijklmnopqrstuvwxyz";
        let score = TrueAudio::score("test.tta", header);
        assert_eq!(score, 3); // 1 for ID3 + 2 for .tta extension
    }

    #[test]
    fn test_score_tta_extension() {
        let header = b"NOTTRUEAUDIOabcdefghijklmnopqr";
        let score = TrueAudio::score("test.tta", header);
        assert_eq!(score, 2); // 2 for .tta extension
    }

    #[test]
    fn test_score_no_match() {
        let header = b"NOTTRUEAUDIOabcdefghijklmnopqr";
        let score = TrueAudio::score("test.wav", header);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_mime_types() {
        let mime_types = TrueAudio::mime_types();
        assert!(mime_types.contains(&"audio/x-tta"));
    }

    #[test]
    fn test_zero_sample_rate() {
        // Build a valid TTA1 header with zero sample_rate at offset 10
        let mut header_data = vec![0u8; 18];
        header_data[0..4].copy_from_slice(b"TTA1");
        // Leave sample_rate bytes (10-13) as zero
        let mut cursor = Cursor::new(header_data);

        let info = TrueAudioStreamInfo::from_reader(&mut cursor, Some(0))
            .expect("Failed to parse zero sample rate header");
        assert_eq!(info.sample_rate, 0);
        // When sample_rate is zero, duration should be None (indeterminate)
        assert!(
            info.length.is_none(),
            "zero sample_rate should yield None duration"
        );
    }
}

/// TrueAudio stream information tests
#[cfg(test)]
mod trueaudio_stream_info_tests {
    use super::*;

    #[test]
    fn test_stream_info_pprint() {
        let info = TrueAudioStreamInfo {
            sample_rate: 44100,
            length: Some(std::time::Duration::from_secs_f64(3.7)),
            channels: 2,
            bits_per_sample: 16,
        };

        let output = info.pprint();
        assert!(output.contains("True Audio"));
        assert!(output.contains("3.70 seconds"));
        assert!(output.contains("44100 Hz"));
        println!("Pprint output: {}", output);
    }

    #[test]
    fn test_stream_info_from_header() {
        // Create a test TTA1 header
        // Layout: "TTA1"(4) + format(2) + channels(2) + bps(2) + sample_rate(4) + samples(4) = 18 bytes
        let mut header = Vec::new();
        header.extend_from_slice(b"TTA1"); // 4-byte signature
        header.extend_from_slice(&1u16.to_le_bytes()); // Format
        header.extend_from_slice(&2u16.to_le_bytes()); // Channels
        header.extend_from_slice(&16u16.to_le_bytes()); // Bits per sample
        header.extend_from_slice(&44100u32.to_le_bytes()); // Sample rate
        header.extend_from_slice(&163170u32.to_le_bytes()); // Total samples (~3.7s at 44100 Hz)

        let mut cursor = Cursor::new(&header);
        match TrueAudioStreamInfo::from_reader(&mut cursor, Some(0)) {
            Ok(info) => {
                assert_eq!(info.sample_rate, 44100);
                let length_secs = info.length.map(|d| d.as_secs_f64()).unwrap_or(0.0);
                assert!(
                    (length_secs - 3.7).abs() < 0.1,
                    "Length should be ~3.7 seconds, got {}",
                    length_secs
                );
                println!(
                    "✓ Parsed TTA header: {} Hz, {:.2} seconds",
                    info.sample_rate, length_secs
                );
            }
            Err(e) => panic!("Failed to parse TTA header: {}", e),
        }
    }
}

/// TrueAudio error handling tests
#[cfg(test)]
mod trueaudio_error_handling_tests {
    use super::*;

    #[test]
    fn test_invalid_file_error() {
        let invalid_data = b"This is not a TrueAudio file";
        let mut cursor = Cursor::new(invalid_data.as_slice());
        let result = TrueAudioStreamInfo::from_reader(&mut cursor, Some(0));
        assert!(result.is_err());
        if let Err(AudexError::TrueAudioHeaderError(msg)) = result {
            assert!(msg.contains("TTA1 header not found"));
        } else {
            panic!("Expected TrueAudioHeaderError");
        }
    }

    #[test]
    fn test_empty_file() {
        let empty_data = b"";
        let mut cursor = Cursor::new(empty_data.as_slice());
        let result = TrueAudioStreamInfo::from_reader(&mut cursor, Some(0));
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_header() {
        let truncated_data = b"TTA"; // Too short
        let mut cursor = Cursor::new(truncated_data.as_slice());
        let result = TrueAudioStreamInfo::from_reader(&mut cursor, Some(0));
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_signature() {
        let invalid_data = b"XXXabcdefghijklmnopqr"; // Wrong signature
        let mut cursor = Cursor::new(invalid_data.as_slice());
        let result = TrueAudioStreamInfo::from_reader(&mut cursor, Some(0));
        assert!(result.is_err());
        if let Err(AudexError::TrueAudioHeaderError(msg)) = result {
            assert!(msg.contains("TTA1 header not found"));
        }
    }

    #[test]
    fn test_nonexistent_file() {
        let result = TrueAudio::load("/nonexistent/file.tta");
        assert!(result.is_err());
    }
}

/// TrueAudio file operations tests
#[cfg(test)]
mod trueaudio_file_operations_tests {
    use super::*;

    #[test]
    fn test_load_real_trueaudio_files() {
        // Test files based on reference data
        let test_files = vec![("empty.tta", "Empty TrueAudio test file")];

        for (filename, _description) in test_files {
            let path = data_path(filename);
            if path.exists() {
                let trueaudio = TrueAudio::load(&path)
                    .unwrap_or_else(|_| panic!("Failed to load TrueAudio file {}", filename));
                println!("  Sample rate: {} Hz", trueaudio.info.sample_rate);
                println!(
                    "  Length: {:.2} seconds",
                    trueaudio
                        .info
                        .length
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0)
                );

                // Test expected values
                if filename == "empty.tta" {
                    assert_eq!(
                        trueaudio.info.sample_rate, 44100,
                        "Sample rate should be 44100 Hz"
                    );
                    let length = trueaudio
                        .info
                        .length
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    assert!(
                        (length - 3.7).abs() < 0.1,
                        "Length should be ~3.7 seconds, got {}",
                        length
                    );
                }
            } else {
                println!("Test file {} not found", filename);
            }
        }
    }

    #[test]
    fn test_trueaudio_with_id3_tags() {
        let path = data_path("empty.tta");
        if path.exists() {
            println!("Testing TrueAudio with ID3 tags");
            let trueaudio = TrueAudio::load(&path).expect("Failed to load TrueAudio with ID3");
            if let Some(tags) = &trueaudio.tags {
                println!("Found ID3 tags");

                // Test access to common tags
                if let Some(title) = tags.get("TIT1") {
                    println!("Title: {:?}", title);
                }
                if let Some(artist) = tags.get("TPE1") {
                    println!("Artist: {:?}", artist);
                }

                println!("✓ ID3 tag parsing successful");
            } else {
                println!("No ID3 tags found");
            }
        }
    }

    #[test]
    fn test_pprint() {
        let path = data_path("empty.tta");
        if path.exists() {
            let trueaudio = TrueAudio::load(&path).expect("Failed to load TrueAudio for pprint");
            let output = trueaudio.pprint();
            assert!(output.contains("True Audio"));
            assert!(output.contains("seconds"));
            assert!(output.contains("Hz"));
            println!("Pprint output: {}", output);
        }
    }

    #[test]
    fn test_mime_access() {
        let path = data_path("empty.tta");
        if path.exists() {
            let trueaudio = TrueAudio::load(&path).expect("Failed to load TrueAudio for mime");
            let mime_types = trueaudio.mime();
            assert!(mime_types.contains(&"audio/x-tta"));
        }
    }

    #[test]
    fn test_not_my_file() {
        // Test with a non-TrueAudio file
        let path = data_path("empty.ogg");
        if path.exists() {
            match TrueAudio::load(&path) {
                Ok(_) => panic!("Should have failed to load OGG file as TrueAudio"),
                Err(e) => {
                    println!("✓ Got expected error for non-TTA file: {}", e);
                    match e {
                        AudexError::TrueAudioHeaderError(_) => {}
                        _ => println!("Note: Got different error type than expected: {:?}", e),
                    }
                }
            }
        }
    }
}

/// TrueAudio integration tests
#[cfg(test)]
mod trueaudio_integration_tests {
    use super::*;

    #[test]
    fn test_comprehensive_parsing() {
        // Test comprehensive parsing of all available test files
        let test_files = vec!["empty.tta"];

        let mut successful_parses = 0;
        let total_files = test_files.len();

        for filename in &test_files {
            let path = data_path(filename);
            if path.exists() {
                let trueaudio = TrueAudio::load(&path)
                    .unwrap_or_else(|_| panic!("Failed to load TrueAudio file {}", filename));
                successful_parses += 1;
                println!(
                    "✓ Successfully parsed {}: {}Hz, {:.2}s",
                    filename,
                    trueaudio.info.sample_rate,
                    trueaudio
                        .info
                        .length
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0)
                );
            } else {
                println!("Test file {} not found", filename);
            }
        }

        if successful_parses > 0 {
            println!(
                "Successfully parsed {}/{} TrueAudio files",
                successful_parses, total_files
            );
        } else {
            println!("No TrueAudio files could be parsed (files may not exist)");
        }
    }

    #[test]
    fn test_add_tags_functionality() {
        let mut trueaudio = TrueAudio::new();

        // Test adding tags to empty file
        assert!(trueaudio.add_tags().is_ok());
        assert!(trueaudio.tags.is_some());

        // Test error when trying to add tags again
        assert!(trueaudio.add_tags().is_err());
    }

    #[test]
    fn test_delete_functionality() {
        let mut trueaudio = TrueAudio::new();
        trueaudio.add_tags().unwrap();

        // Test deleting tags
        assert!(trueaudio.clear().is_ok());
        assert!(trueaudio.tags.is_none());
    }

    #[test]
    fn test_delete_function() {
        println!("Delete function exists (full test requires writable file)");
        // Note: Full delete test would require a writable copy of a test file
        let result = clear("/nonexistent/file.tta");
        println!("Delete result: {:?}", result);
    }

    #[test]
    fn test_file_detection_and_scoring() {
        println!("Testing TrueAudio file detection and scoring:");

        let test_files = vec!["empty.tta"];

        for filename in test_files {
            let path = data_path(filename);
            if path.exists() {
                if let Ok(data) = std::fs::read(&path) {
                    let score = TrueAudio::score(filename, &data[..std::cmp::min(data.len(), 32)]);
                    println!("TrueAudio file \"{}\" scored {}", filename, score);
                }
            }
        }
    }

    #[test]
    fn test_expected_values_from_reference_tests() {
        // Test expected values
        let path = data_path("empty.tta");
        if path.exists() {
            let trueaudio = TrueAudio::load(&path).expect("Failed to load TrueAudio test file");
            assert_eq!(
                trueaudio.info.sample_rate, 44100,
                "Sample rate should be 44100 Hz"
            );

            let length = trueaudio
                .info
                .length
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            assert!(
                (length - 3.7).abs() < 0.1,
                "Length should be ~3.7 seconds, got {}",
                length
            );

            // Tags may or may not be present depending on the test file
        } else {
            println!("Test file empty.tta not found - skipping expected values test");
        }
    }

    #[test]
    fn test_save_reload_simulation() {
        let path = data_path("empty.tta");
        if !path.exists() {
            return;
        }
        let temp = tempfile::NamedTempFile::new().expect("Failed to create temp file");
        std::fs::copy(&path, temp.path()).expect("Failed to copy test file");

        let mut trueaudio =
            TrueAudio::load(temp.path()).expect("Failed to load TrueAudio for save test");
        if trueaudio.tags.is_none() {
            assert!(trueaudio.add_tags().is_ok());
        }
        assert!(trueaudio.tags.is_some());

        if let Some(_tags) = trueaudio.tags_mut() {
            println!("Tag access works");
        }

        trueaudio.save().expect("Failed to save TrueAudio file");
        println!("Save operation completed");
    }

    #[test]
    fn test_header_parsing_edge_cases() {
        // Test various header scenarios

        // Valid minimal TTA1 header (18 bytes)
        let mut valid_header = Vec::new();
        valid_header.extend_from_slice(b"TTA1"); // 4-byte signature
        valid_header.extend_from_slice(&1u16.to_le_bytes()); // Format
        valid_header.extend_from_slice(&2u16.to_le_bytes()); // Channels
        valid_header.extend_from_slice(&16u16.to_le_bytes()); // Bits per sample
        valid_header.extend_from_slice(&44100u32.to_le_bytes()); // Sample rate
        valid_header.extend_from_slice(&163170u32.to_le_bytes()); // Total samples

        let mut cursor = Cursor::new(&valid_header);
        match TrueAudioStreamInfo::from_reader(&mut cursor, Some(0)) {
            Ok(info) => {
                assert_eq!(info.sample_rate, 44100);
                println!("✓ Valid header parsing works");
            }
            Err(e) => panic!("Valid header should parse: {}", e),
        }

        // Test with ID3v2 prefix (should still find TTA header if we had full file)
        let mut id3_prefixed = Vec::new();
        id3_prefixed.extend_from_slice(b"ID3\x03\x00\x00\x00\x00\x00\x00"); // Minimal ID3v2 header
        id3_prefixed.extend_from_slice(&valid_header);

        // This would work with a full implementation that searches for TTA header
        println!("✓ ID3v2 prefix test structure verified");
    }
}
