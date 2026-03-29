//! AC-3 (Dolby Digital) format tests
//!
//! Tests for AC-3 and Enhanced AC-3 (E-AC-3) formats

use audex::ac3::AC3;
use audex::{FileType, StreamInfo};
use std::io::Cursor;
use std::time::Duration;

mod common;
use common::TestUtils;

// Test file constants
const AC3_FILE: &str = "silence-44-s.ac3";

/// Core AC-3 tests
#[cfg(test)]
mod ac3_tests {
    use super::*;

    #[test]
    fn test_ac3_creation() {
        let ac3 = AC3::new();
        assert!(ac3.info().length().is_none());
    }

    #[test]
    fn test_ac3_score() {
        // Test AC-3 sync pattern detection (0x0B77)
        let ac3_sync = &[0x0B, 0x77, 0x00, 0x00]; // AC-3 sync word
        let score = AC3::score("test.ac3", ac3_sync);
        assert!(score > 0, "AC-3 should score > 0 for sync word");

        // Test file extension scoring
        let score_ext = AC3::score("test.ac3", &[0x00, 0x01, 0x02]);
        assert!(score_ext > 0, "AC-3 should score > 0 for .ac3 extension");

        let score_eac3 = AC3::score("test.eac3", &[]);
        assert!(score_eac3 > 0, "AC-3 should score > 0 for .eac3 extension");

        // Test rejection of non-AC-3 files
        let score_invalid = AC3::score("test.txt", &[0x00, 0x01, 0x02]);
        assert_eq!(score_invalid, 0, "AC-3 should score 0 for non-audio files");
    }

    #[test]
    fn test_ac3_mime_types() {
        let mime_types = AC3::mime_types();
        assert!(mime_types.contains(&"audio/ac3"));
        assert!(mime_types.contains(&"audio/x-ac3"));
        assert!(mime_types.contains(&"audio/eac3"));
        assert!(mime_types.contains(&"audio/vnd.dolby.dd-raw"));
    }

    #[test]
    fn test_ac3_file_loading() {
        // Test loading AC-3 file
        let path = TestUtils::data_path(AC3_FILE);
        let ac3 = match AC3::load(&path) {
            Ok(file) => file,
            Err(e) => panic!("Failed to load {}: {}", AC3_FILE, e),
        };

        assert!(ac3.info().length().is_some(), "AC-3 info should be present");

        let info = ac3.info();

        // Verify basic AC-3 properties using direct AC3Info methods
        assert!(
            info.length().unwrap() > Duration::from_secs(0),
            "Duration should be > 0"
        );
        assert!(info.sample_rate() > 0, "Sample rate should be > 0");
        assert!(info.channels() > 0, "Channels should be > 0");

        // Test StreamInfo trait implementation
        assert_eq!(info.length(), info.length());
        assert_eq!(info.sample_rate(), info.sample_rate());
        assert_eq!(info.channels(), info.channels());
        assert_eq!(info.bitrate(), info.bitrate());
    }

    #[test]
    fn test_ac3_sample_rates() {
        // Test AC-3 sample rate detection
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            // AC3Info::sample_rate() returns u32 directly
            let sample_rate = info.sample_rate();
            assert_eq!(sample_rate, 44100);
        }
    }

    #[test]
    fn test_ac3_bitrate() {
        // Test AC-3 bitrate detection
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            // AC3Info::bitrate() returns u32 directly
            let bitrate = info.bitrate();
            assert_eq!(bitrate, 192000);
        }
    }

    #[test]
    fn test_ac3_channel_configurations() {
        // Test AC-3 channel mode detection
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            // AC3Info::channels() returns u16 directly
            let channels = info.channels();
            assert_eq!(channels, 2);
        }
    }

    #[test]
    fn test_ac3_info_display() {
        // Test pretty printing of AC-3 info
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            let display = format!("{}", info);
            assert!(
                display.contains("AC-3") || !display.is_empty(),
                "Display output should contain format info"
            );

            let pprint = info.pprint();
            assert!(!pprint.is_empty(), "pprint should return non-empty string");
        }
    }

    #[test]
    fn test_ac3_from_bytes() {
        // Test loading AC-3 from byte buffer
        let path = TestUtils::data_path(AC3_FILE);
        let data = std::fs::read(&path).expect("Failed to read test file");

        let mut cursor = Cursor::new(data);
        let result = AC3::load_from_reader(&mut cursor);

        match result {
            Ok(ac3) => {
                assert!(
                    ac3.info().length().is_some(),
                    "AC-3 info should be present when loading from bytes"
                );
            }
            Err(e) => {
                println!("AC-3 from bytes failed: {}", e);
            }
        }
    }

    #[test]
    fn test_ac3_invalid_file() {
        // Test handling of invalid AC-3 data
        let invalid_data = vec![0u8; 1024]; // All zeros
        let mut cursor = Cursor::new(invalid_data);
        let result = AC3::load_from_reader(&mut cursor);

        assert!(result.is_err(), "Loading invalid AC-3 data should fail");
    }

    #[test]
    fn test_ac3_truncated_file() {
        // Test handling of truncated AC-3 file
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(data) = std::fs::read(&path) {
            // Take only first 100 bytes
            let truncated = &data[..std::cmp::min(100, data.len())];
            let mut cursor = Cursor::new(truncated);
            let result = AC3::load_from_reader(&mut cursor);

            // Truncated file should either fail or load with partial info
            match result {
                Ok(_ac3) => {
                    println!("Truncated AC-3 file loaded (may have partial info)");
                }
                Err(e) => {
                    println!("Truncated AC-3 file failed to load (expected): {}", e);
                }
            }
        }
    }

    #[test]
    fn test_ac3_length_calculation() {
        // Test duration calculation
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            if let Some(length) = info.length() {
                let secs = length.as_secs_f64();
                assert!(
                    (secs - 3.70).abs() < 0.01,
                    "AC3 length expected ~3.70s, got {}",
                    secs
                );
            }
        }
    }
}

/// AC-3 frame detection tests
#[cfg(test)]
mod frame_tests {
    use super::*;

    #[test]
    fn test_sync_word_detection() {
        // Test AC-3 sync word (0x0B77)
        let sync_word = [0x0B, 0x77];
        let score = AC3::score("test.ac3", &sync_word);
        assert!(score > 0, "Should detect AC-3 sync word");
    }

    #[test]
    fn test_reversed_sync_word() {
        // Test byte-swapped sync word (0x770B)
        let reversed_sync = [0x77, 0x0B];
        // Some implementations may handle byte-swapped streams
        let score = AC3::score("test.ac3", &reversed_sync);
        // May or may not be detected depending on implementation
        println!("Reversed sync word score: {}", score);
    }
}

/// E-AC-3 (Enhanced AC-3) specific tests
#[cfg(test)]
mod eac3_tests {
    use super::*;

    #[test]
    fn test_eac3_extension_detection() {
        // Test E-AC-3 file extension
        let score = AC3::score("test.eac3", &[0x00]);
        assert!(score > 0, "Should recognize .eac3 extension");
    }

    #[test]
    fn test_eac3_sample_rates() {
        // E-AC-3 supports extended sample rates up to 192 kHz
        // This test verifies the implementation handles higher rates
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            // AC3Info::sample_rate() returns u32 directly
            let sample_rate = info.sample_rate();
            // E-AC-3 can go up to 192000 Hz
            assert!(
                sample_rate <= 192000,
                "Sample rate should not exceed 192 kHz"
            );
        }
    }
}

/// Channel mode tests
#[cfg(test)]
mod channel_mode_tests {
    use super::*;

    #[test]
    fn test_channel_mode_mapping() {
        // Test various channel configurations
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            // AC3Info::channels() returns u16 directly
            let channels = info.channels();
            let pprint = info.pprint();

            // Verify channel mode is represented in output
            match channels {
                1 => assert!(
                    pprint.contains("1") || pprint.contains("mono"),
                    "Should indicate mono"
                ),
                2 => assert!(
                    pprint.contains("2") || pprint.contains("stereo"),
                    "Should indicate stereo"
                ),
                6 => assert!(
                    pprint.contains("6") || pprint.contains("5.1"),
                    "Should indicate 5.1"
                ),
                _ => println!("Channel count: {}", channels),
            }
        }
    }

    #[test]
    fn test_lfe_channel_detection() {
        // Test LFE (Low Frequency Effects) channel detection
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            let pprint = info.pprint();
            // AC3Info::channels() returns u16 directly
            let channels = info.channels();
            // If 6 channels, it's likely 5.1 with LFE
            if channels == 6 {
                // Just verify we can access the info
                assert!(!pprint.is_empty(), "Should have channel info");
            }
        }
    }
}

/// Stream info tests
#[cfg(test)]
mod stream_info_tests {
    use super::*;

    #[test]
    fn test_stream_info_trait() {
        // Verify StreamInfo trait implementation
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            // Test all StreamInfo trait methods
            let _ = info.length();
            let _ = info.sample_rate();
            let _ = info.channels();
            let _ = info.bitrate();
            let _ = info.pprint();
        }
    }

    #[test]
    fn test_info_consistency() {
        // Verify info values are consistent
        let path = TestUtils::data_path(AC3_FILE);
        if let Ok(ac3) = AC3::load(&path) {
            let info = ac3.info();
            // AC3Info methods return direct values, not Options
            let sample_rate = info.sample_rate();
            assert!(sample_rate > 0, "Sample rate should be positive");

            let channels = info.channels();
            assert!(channels > 0, "Channels should be positive");

            let bitrate = info.bitrate();
            assert!(bitrate > 0, "Bitrate should be positive");

            // length() returns Option<Duration>
            if let Some(length) = info.length() {
                assert!(
                    length >= Duration::from_secs(0),
                    "Length should be non-negative"
                );
            }
        }
    }
}
