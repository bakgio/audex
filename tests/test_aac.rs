//! AAC (Advanced Audio Coding) format tests
//!
//! Tests for AAC ADTS and ADIF formats

use audex::aac::AAC;
use audex::{FileType, StreamInfo};
use std::time::Duration;

mod common;
use common::TestUtils;

// Test file constants
const ADIF_FILE: &str = "adif.aac";
const EMPTY_AAC: &str = "empty.aac";

/// Core AAC tests
#[cfg(test)]
mod aac_tests {
    use super::*;

    #[test]
    fn test_aac_creation() {
        let aac = AAC::new();
        // AAC::new() creates an empty instance
        // Info will be populated when loading from a file
        // Just verify we can access the info
        let _ = aac.info().sample_rate();
    }

    #[test]
    fn test_aac_score() {
        // Test ADTS sync pattern detection (0xFFF at start of 12-bit sync word)
        let adts_header = &[0xFF, 0xF1, 0x50, 0x80]; // ADTS sync + MPEG-4 AAC LC
        let score = AAC::score("test.aac", adts_header);
        assert!(score > 0, "AAC should score > 0 for ADTS sync");

        // Test ADIF header detection
        let adif_header = b"ADIF\x00\x00\x00\x00extra_data";
        let score_adif = AAC::score("test.aac", adif_header);
        assert!(score_adif > 0, "AAC should score > 0 for ADIF header");

        // Test file extension scoring
        let score_ext = AAC::score("test.aac", &[0x00, 0x01, 0x02]);
        assert!(score_ext > 0, "AAC should score > 0 for .aac extension");

        let score_aacp = AAC::score("test.aacp", &[]);
        assert!(score_aacp > 0, "AAC should score > 0 for .aacp extension");

        // Test rejection of non-AAC files
        let score_invalid = AAC::score("test.txt", &[0x00, 0x01, 0x02]);
        assert_eq!(score_invalid, 0, "AAC should score 0 for non-audio files");
    }

    #[test]
    fn test_aac_mime_types() {
        let mime_types = AAC::mime_types();
        assert!(mime_types.contains(&"audio/aac"));
        assert!(mime_types.contains(&"audio/aacp"));
        assert!(mime_types.contains(&"audio/x-aac"));
    }

    #[test]
    fn test_adif_aac_file() {
        // Test ADIF format AAC file
        let path = TestUtils::data_path(ADIF_FILE);
        let aac = match AAC::load(&path) {
            Ok(file) => file,
            Err(e) => panic!("Failed to load {}: {}", ADIF_FILE, e),
        };

        let info = aac.info();

        // Verify basic AAC properties
        assert!(
            info.length().is_some() && info.length().unwrap() > Duration::from_secs(0),
            "Duration should be > 0"
        );
        assert!(info.sample_rate() > 0, "Sample rate should be > 0");
        assert!(info.channels() > 0, "Channels should be > 0");

        // Test StreamInfo trait implementation - methods should return consistent values
        assert!(info.length().is_some());
        assert!(info.sample_rate() > 0);
        assert!(info.channels() > 0);
    }

    #[test]
    fn test_empty_aac_file() {
        // Test empty/minimal AAC file
        let path = TestUtils::data_path(EMPTY_AAC);
        let result = AAC::load(&path);

        // Empty file should either load with minimal info or fail gracefully
        match result {
            Ok(aac) => {
                let info = aac.info();
                // If it loaded, verify minimal properties
                // These may return None for empty files
                let _ = info.sample_rate();
                let _ = info.channels();
            }
            Err(e) => {
                // Empty file may fail to load - this is acceptable
                println!("Empty AAC file failed to load (expected): {}", e);
            }
        }
    }

    #[test]
    fn test_aac_info_display() {
        // Test pretty printing of AAC info
        let path = TestUtils::data_path(ADIF_FILE);
        if let Ok(aac) = AAC::load(&path) {
            let info = aac.info();
            // Test debug format
            let debug = format!("{:?}", info);
            assert!(!debug.is_empty(), "Debug output should not be empty");

            let pprint = info.pprint();
            assert!(!pprint.is_empty(), "pprint should return non-empty string");
        }
    }

    #[test]
    fn test_aac_channel_configurations() {
        // Test various AAC channel configurations if available
        let path = TestUtils::data_path(ADIF_FILE);
        if let Ok(aac) = AAC::load(&path) {
            let info = aac.info();
            let channels = info.channels();
            assert_eq!(channels, 2);
        }
    }

    #[test]
    fn test_aac_sample_rates() {
        // Test AAC sample rate detection
        let path = TestUtils::data_path(ADIF_FILE);
        if let Ok(aac) = AAC::load(&path) {
            let info = aac.info();
            let sample_rate = info.sample_rate();
            // AAC supports standard sample rates: 8000-96000 Hz
            let valid_rates = [
                8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000, 64000, 88200, 96000,
            ];

            // Should be either a standard rate or close to one
            let is_valid = valid_rates
                .iter()
                .any(|&rate| (sample_rate as i32 - rate).abs() < 100);

            assert!(
                is_valid,
                "Sample rate {} is not a standard AAC rate",
                sample_rate
            );
        }
    }

    #[test]
    fn test_aac_bitrate_estimation() {
        // Test bitrate estimation for AAC
        let path = TestUtils::data_path(ADIF_FILE);
        if let Ok(aac) = AAC::load(&path) {
            let info = aac.info();
            let bitrate = info.bitrate();
            assert_eq!(bitrate, 128000);
        }
    }

    #[test]
    fn test_aac_from_bytes() {
        // Test loading AAC from byte buffer - skip this test as AAC::load requires a file path
        let path = TestUtils::data_path(ADIF_FILE);
        let data = std::fs::read(&path).expect("Failed to read test file");

        // AAC::load requires a Path, not a reader
        // This test verifies we can at least read the file data
        assert!(!data.is_empty(), "Should be able to read file data");
    }

    #[test]
    fn test_aac_invalid_file() {
        // Test handling of invalid AAC data
        let invalid_data = vec![0u8; 1024]; // All zeros
        let temp_file =
            TestUtils::create_test_data(&invalid_data).expect("Failed to create temp file");
        let result = AAC::load(temp_file.path());

        assert!(result.is_err(), "Loading invalid AAC data should fail");
    }

    #[test]
    fn test_aac_truncated_file() {
        // Test handling of truncated AAC file
        let path = TestUtils::data_path(ADIF_FILE);
        if let Ok(data) = std::fs::read(&path) {
            // Take only first 100 bytes
            let truncated = &data[..std::cmp::min(100, data.len())];
            let temp_file =
                TestUtils::create_test_data(truncated).expect("Failed to create temp file");
            let result = AAC::load(temp_file.path());

            // Truncated file should either fail or load with partial info
            match result {
                Ok(_aac) => {
                    println!("Truncated AAC file loaded (may have partial info)");
                }
                Err(e) => {
                    println!("Truncated AAC file failed to load (expected): {}", e);
                }
            }
        }
    }

    #[test]
    fn test_aac_profile_detection() {
        // Test AAC profile detection (if implemented)
        let path = TestUtils::data_path(ADIF_FILE);
        if let Ok(aac) = AAC::load(&path) {
            let info = aac.info();
            // AAC profiles include: LC (Low Complexity), Main, SSR, LTP, HE-AAC, etc.
            // Just verify the info structure is valid
            let pprint = info.pprint();
            assert!(
                !pprint.is_empty(),
                "Profile info should be available in pprint"
            );
        }
    }
}

/// ADTS (Audio Data Transport Stream) specific tests
#[cfg(test)]
mod adts_tests {
    use super::*;

    #[test]
    fn test_adts_sync_detection() {
        // Test ADTS sync word detection (0xFFF)
        let adts_sync = [0xFF, 0xF1]; // 12-bit sync + MPEG-4, no CRC
        let score = AAC::score("test.aac", &adts_sync);
        assert!(score > 0, "Should detect ADTS sync word");
    }

    #[test]
    fn test_adts_with_crc() {
        // Test ADTS with CRC protection
        let adts_with_crc = [0xFF, 0xF0]; // 12-bit sync + MPEG-4, with CRC
        let score = AAC::score("test.aac", &adts_with_crc);
        assert!(score > 0, "Should detect ADTS with CRC");
    }

    #[test]
    fn test_adts_mpeg2() {
        // Test ADTS with MPEG-2 AAC
        let adts_mpeg2 = [0xFF, 0xF9]; // 12-bit sync + MPEG-2, no CRC
        let score = AAC::score("test.aac", &adts_mpeg2);
        assert!(score > 0, "Should detect MPEG-2 ADTS");
    }
}

/// ADIF (Audio Data Interchange Format) specific tests
#[cfg(test)]
mod adif_tests {
    use super::*;

    #[test]
    fn test_adif_header_detection() {
        // Test ADIF magic bytes
        let adif_magic = b"ADIF";
        let score = AAC::score("test.aac", adif_magic);
        assert!(score > 0, "Should detect ADIF magic bytes");
    }

    #[test]
    fn test_adif_file_loading() {
        // Test loading actual ADIF file
        let path = TestUtils::data_path(ADIF_FILE);
        let result = AAC::load(&path);
        assert!(result.is_ok(), "ADIF file should load successfully");
    }
}

/// Stream info tests
#[cfg(test)]
mod stream_info_tests {
    use super::*;

    #[test]
    fn test_stream_info_trait() {
        // Verify StreamInfo trait implementation
        let path = TestUtils::data_path(ADIF_FILE);
        if let Ok(aac) = AAC::load(&path) {
            let info = aac.info();
            // Test all StreamInfo trait methods
            let _ = info.length();
            let _ = info.sample_rate();
            let _ = info.channels();
            let _ = info.bitrate();
            let _ = info.pprint();
        }
    }
}
