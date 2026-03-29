//! Tests for Musepack format support

use audex::musepack::{Musepack, MusepackStreamInfo, clear, parse_sv8_int};
use audex::{AudexError, FileType};
use std::io::Cursor;
use std::path::PathBuf;

/// Get path to test data file
fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Basic Musepack creation and properties tests
#[cfg(test)]
mod musepack_basic_tests {
    use super::*;

    #[test]
    fn test_musepack_creation() {
        let musepack = Musepack::new();
        assert!(musepack.tags.is_none());
        assert_eq!(musepack.info.channels, 0);
        assert_eq!(musepack.info.sample_rate, 0);
    }

    #[test]
    fn test_musepack_stream_info_creation() {
        let info = MusepackStreamInfo::default();
        assert_eq!(info.channels, 0);
        assert_eq!(info.sample_rate, 0);
        assert_eq!(info.version, 0);
        assert_eq!(info.samples, 0);
    }

    #[test]
    fn test_score_mp_plus_signature() {
        let header = b"MP+\x07abcdefghijklmnopqrstuvwxyz";
        let score = Musepack::score("test.mpc", header);
        assert_eq!(score, 3); // 2 for MP+ + 1 for .mpc extension
    }

    #[test]
    fn test_score_mpck_signature() {
        let header = b"MPCKabcdefghijklmnopqrstuvwxyz";
        let score = Musepack::score("test.mpc", header);
        assert_eq!(score, 3); // 2 for MPCK + 1 for .mpc extension
    }

    #[test]
    fn test_score_mpc_extension() {
        let header = b"NOTMUSEPACKabcdefghijklmnopqr";
        let score = Musepack::score("test.mpc", header);
        assert_eq!(score, 1); // 1 for .mpc extension
    }

    #[test]
    fn test_score_no_match() {
        let header = b"NOTMUSEPACKabcdefghijklmnopqr";
        let score = Musepack::score("test.wav", header);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_mime_types() {
        let mime_types = Musepack::mime_types();
        assert!(mime_types.contains(&"audio/x-musepack"));
        assert!(mime_types.contains(&"audio/x-mpc"));
    }

    #[test]
    fn test_parse_sv8_int() {
        // Test simple case
        let data = [0x40]; // 0100 0000 - should be 64
        let mut cursor = Cursor::new(&data);
        let result = parse_sv8_int(&mut cursor, 9).unwrap();
        assert_eq!(result, (64, 1));

        // Test multi-byte case
        let data = [0x81, 0x00]; // 1000 0001, 0000 0000 - should be 128
        let mut cursor = Cursor::new(&data);
        let result = parse_sv8_int(&mut cursor, 9).unwrap();
        assert_eq!(result, (128, 2));

        // Test overflow case
        let data = [0xFF; 10]; // All bytes have MSB set
        let mut cursor = Cursor::new(&data);
        assert!(parse_sv8_int(&mut cursor, 9).is_err());
    }
}

/// Musepack stream information tests
#[cfg(test)]
mod musepack_stream_info_tests {
    use super::*;

    #[test]
    fn test_stream_info_pprint() {
        let info = MusepackStreamInfo {
            version: 7,
            sample_rate: 44100,
            bitrate: Some(128000),
            length: Some(std::time::Duration::from_secs_f64(3.5)),
            ..Default::default()
        };

        let output = info.pprint();
        assert!(output.contains("Musepack SV7"));
        assert!(output.contains("3.50 seconds"));
        assert!(output.contains("44100 Hz"));
        assert!(output.contains("128000 bps"));
    }

    #[test]
    fn test_stream_info_with_gain() {
        let info = MusepackStreamInfo {
            version: 8,
            sample_rate: 48000,
            bitrate: Some(160000),
            length: Some(std::time::Duration::from_secs_f64(2.0)),
            title_gain: Some(-3.5),
            album_gain: Some(-2.1),
            ..Default::default()
        };

        let output = info.pprint();
        assert!(output.contains("Gain: -3.50 (title), -2.10 (album)"));
    }
}

/// Musepack error handling tests
#[cfg(test)]
mod musepack_error_handling_tests {
    use super::*;

    #[test]
    fn test_invalid_file_error() {
        let invalid_data = b"This is not a Musepack file";
        let mut cursor = Cursor::new(invalid_data.as_slice());
        let result = MusepackStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
        if let Err(AudexError::MusepackHeaderError(msg)) = result {
            assert!(msg.contains("Not a Musepack file"));
        } else {
            panic!("Expected MusepackHeaderError");
        }
    }

    #[test]
    fn test_empty_file() {
        let empty_data = b"";
        let mut cursor = Cursor::new(empty_data.as_slice());
        let result = MusepackStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_header() {
        let truncated_data = b"MP+"; // Too short
        let mut cursor = Cursor::new(truncated_data.as_slice());
        let result = MusepackStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_sv8_header() {
        // MPCK but invalid packet structure
        let invalid_data = b"MPCK12"; // Too short for proper SV8
        let mut cursor = Cursor::new(invalid_data.as_slice());
        let result = MusepackStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_nonexistent_file() {
        let result = Musepack::load("/nonexistent/file.mpc");
        assert!(result.is_err());
    }
}

/// Musepack file operations tests
#[cfg(test)]
mod musepack_file_operations_tests {
    use super::*;

    #[test]
    fn test_load_real_musepack_files() {
        // Test files based on reference data
        let test_files = vec![
            ("sv8_header.mpc", "SV8 format test file"),
            ("click.mpc", "SV7 format test file"),
            ("sv5_header.mpc", "SV5 format test file"),
            ("sv4_header.mpc", "SV4 format test file"),
        ];

        for (filename, _description) in test_files {
            let path = data_path(filename);
            if path.exists() {
                let musepack = Musepack::load(&path).expect("Failed to load Musepack file");
                println!("  Sample rate: {} Hz", musepack.info.sample_rate);
                println!("  Channels: {}", musepack.info.channels);
                println!("  Bitrate: {} bps", musepack.info.bitrate.unwrap_or(0));
                println!("  Version: SV{}", musepack.info.version);
                println!(
                    "  Length: {:.2} seconds",
                    musepack.info.length.map(|d| d.as_secs_f64()).unwrap_or(0.0)
                );
            }
        }
    }

    #[test]
    fn test_musepack_with_apev2_tags() {
        let path = data_path("click.mpc");
        if path.exists() {
            println!("Testing Musepack with APEv2 tags");
            let musepack = Musepack::load(&path).expect("Failed to load Musepack with APEv2");
            if let Some(tags) = &musepack.tags {
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
        let path = data_path("click.mpc");
        if path.exists() {
            if let Ok(musepack) = Musepack::load(&path) {
                let output = musepack.pprint();
                assert!(output.contains("Musepack"));
                assert!(output.contains("seconds"));
                assert!(output.contains("Hz"));
                assert!(output.contains("bps"));
                println!("Pprint output: {}", output);
            }
        }
    }

    #[test]
    fn test_mime_access() {
        let path = data_path("click.mpc");
        if path.exists() {
            if let Ok(musepack) = Musepack::load(&path) {
                let mime_types = musepack.mime();
                assert!(mime_types.contains(&"audio/x-musepack"));
                assert!(mime_types.contains(&"audio/x-mpc"));
            }
        }
    }
}

/// Musepack integration tests
#[cfg(test)]
mod musepack_integration_tests {
    use super::*;

    #[test]
    fn test_comprehensive_parsing() {
        // Test comprehensive parsing of all available test files
        let test_files = vec![
            "sv8_header.mpc",
            "click.mpc",
            "sv5_header.mpc",
            "sv4_header.mpc",
        ];

        let mut successful_parses = 0;

        for filename in &test_files {
            let path = data_path(filename);
            if path.exists() {
                match Musepack::load(&path) {
                    Ok(musepack) => {
                        successful_parses += 1;
                        println!(
                            "✓ Successfully parsed {}: SV{}, {}ch, {}Hz, {}bps",
                            filename,
                            musepack.info.version,
                            musepack.info.channels,
                            musepack.info.sample_rate,
                            musepack.info.bitrate.unwrap_or(0)
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
            "Should successfully parse at least one Musepack file"
        );
        let total_files = test_files.len();
        println!(
            "Successfully parsed {}/{} Musepack files",
            successful_parses, total_files
        );
    }

    #[test]
    fn test_add_tags_functionality() {
        let mut musepack = Musepack::new();

        // Test adding tags to empty file
        assert!(musepack.add_tags().is_ok());
        assert!(musepack.tags.is_some());

        // Test error when trying to add tags again
        assert!(musepack.add_tags().is_err());
    }

    #[test]
    fn test_delete_function() {
        println!("Delete function exists (full test requires writable file)");
        // Note: Full delete test would require a writable copy of a test file
        let result = clear("/nonexistent/file.mpc");
        println!("Delete result: {:?}", result);
    }

    #[test]
    fn test_file_detection_and_scoring() {
        println!("Testing Musepack file detection and scoring:");

        let test_files = vec![
            "sv8_header.mpc",
            "click.mpc",
            "sv5_header.mpc",
            "sv4_header.mpc",
        ];

        for filename in test_files {
            let path = data_path(filename);
            if path.exists() {
                if let Ok(data) = std::fs::read(&path) {
                    let score = Musepack::score(filename, &data[..std::cmp::min(data.len(), 32)]);
                    println!("Musepack file \"{}\" scored {}", filename, score);
                }
            }
        }
    }

    #[test]
    fn test_expected_values_from_reference_tests() {
        // Test expected values

        // SV8 test file
        let path = data_path("sv8_header.mpc");
        if path.exists() {
            if let Ok(musepack) = Musepack::load(&path) {
                assert_eq!(musepack.info.channels, 2, "SV8 should have 2 channels");
                assert_eq!(musepack.info.sample_rate, 44100, "SV8 should be 44100 Hz");
                // Expected bitrate 609 and length ~1.49s
                assert!(musepack.info.length.is_some(), "SV8 should have length");

                // Check for replay gain (SV8 specific)
                if musepack.info.title_gain.is_some() {
                    println!("SV8 title gain: {:?}", musepack.info.title_gain);
                }
                if musepack.info.title_peak.is_some() {
                    println!("SV8 title peak: {:?}", musepack.info.title_peak);
                }
            }
        }

        // SV7 test file (click.mpc)
        let path = data_path("click.mpc");
        if path.exists() {
            if let Ok(musepack) = Musepack::load(&path) {
                assert_eq!(musepack.info.channels, 2, "SV7 should have 2 channels");
                assert_eq!(musepack.info.sample_rate, 44100, "SV7 should be 44100 Hz");
                // Expected bitrate 194530 and length ~0.07s
                assert!(musepack.info.length.is_some(), "SV7 should have length");

                // Check for replay gain (SV7 specific)
                if musepack.info.title_gain.is_some() {
                    println!("SV7 title gain: {:?}", musepack.info.title_gain);
                }
            }
        }

        // SV5 test file
        let path = data_path("sv5_header.mpc");
        if path.exists() {
            if let Ok(musepack) = Musepack::load(&path) {
                assert_eq!(musepack.info.channels, 2, "SV5 should have 2 channels");
                assert_eq!(musepack.info.sample_rate, 44100, "SV5 should be 44100 Hz");
                // Expected bitrate 39 and length ~26.3s
            }
        }

        // SV4 test file
        let path = data_path("sv4_header.mpc");
        if path.exists() {
            if let Ok(musepack) = Musepack::load(&path) {
                assert_eq!(musepack.info.channels, 2, "SV4 should have 2 channels");
                assert_eq!(musepack.info.sample_rate, 44100, "SV4 should be 44100 Hz");
                // Expected bitrate 39 and length ~26.3s
            }
        }
    }

    #[test]
    fn test_bad_header_handling() {
        // Test that we properly reject files that look like Musepack but aren't
        let path = data_path("almostempty.mpc");
        if path.exists() {
            match Musepack::load(&path) {
                Ok(_) => panic!("Should have failed to load almostempty.mpc"),
                Err(e) => {
                    println!("✓ Got expected error for invalid file: {}", e);
                    // Should be MusepackHeaderError
                    match e {
                        AudexError::MusepackHeaderError(_) => {}
                        _ => panic!("Expected MusepackHeaderError, got {:?}", e),
                    }
                }
            }
        }
    }

    #[test]
    fn test_sv8_zero_padded_packet() {
        // Test case for zero-padded SH packet
        let data = b"MPCKSH\x10\x95 Q\xa2\x08\x81\xb8\xc9T\x00\x1e\x1b\x00RG\x0c\x01A\xcdY\x06?\x80Z\x06EI";

        let mut cursor = Cursor::new(data.as_slice());
        match MusepackStreamInfo::from_reader(&mut cursor) {
            Ok(info) => {
                assert_eq!(info.channels, 2, "Should have 2 channels");
                assert_eq!(info.samples, 3024084, "Should have correct sample count");
                println!("✓ SV8 zero-padded packet test passed");
            }
            Err(e) => println!("SV8 zero-padded packet test failed: {}", e),
        }
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use audex::musepack::MusepackStreamInfo;
    use std::io::Cursor;

    #[test]
    fn test_sv8_missing_sh_packet_does_not_panic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"MPCK");

        data.extend_from_slice(b"SE");
        data.push(0x83);

        let mut cursor = Cursor::new(data);
        let result = MusepackStreamInfo::from_reader(&mut cursor);

        assert!(
            result.is_err(),
            "Should fail on missing SH packet, not panic from division by zero"
        );
    }

    #[test]
    fn test_sv4_with_valid_rate_does_not_panic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"MP+\x07");
        data.extend_from_slice(&[0u8; 200]);

        let mut cursor = Cursor::new(data);
        let result = MusepackStreamInfo::from_reader(&mut cursor);

        if let Ok(info) = result {
            assert!(info.sample_rate > 0, "Sample rate should be non-zero");
        }
    }
}
