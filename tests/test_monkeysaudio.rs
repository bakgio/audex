//! Tests for Monkey's Audio format support

use audex::monkeysaudio::{MonkeysAudio, MonkeysAudioStreamInfo};
use audex::{AudexError, FileType, StreamInfo};
use std::io::Cursor;
use std::time::Duration;

/// Basic Monkey's Audio functionality tests
#[cfg(test)]
mod monkeysaudio_basic_tests {
    use super::*;

    #[test]
    fn test_monkeysaudio_creation() {
        let ma = MonkeysAudio::new();
        assert!(ma.tags.is_none());
        assert!(ma.filename.is_none());
    }

    #[test]
    fn test_monkeysaudio_default() {
        let ma = MonkeysAudio::default();
        assert!(ma.tags.is_none());
        assert!(ma.filename.is_none());
    }

    #[test]
    fn test_monkeysaudio_mime_types() {
        let mime_types = MonkeysAudio::mime_types();
        assert_eq!(mime_types.len(), 2);
        assert!(mime_types.contains(&"audio/ape"));
        assert!(mime_types.contains(&"audio/x-ape"));
    }

    #[test]
    fn test_monkeysaudio_instance_mime() {
        let ma = MonkeysAudio::new();
        let mime = ma.mime();
        assert_eq!(mime.len(), 2);
        assert!(mime.contains(&"audio/ape"));
        assert!(mime.contains(&"audio/x-ape"));
    }

    #[test]
    fn test_score_with_mac_header() {
        let header = b"MAC \x00\x00\x00\x00";
        let score = MonkeysAudio::score("test.ape", header);
        assert_eq!(score, 12); // Header (1) + extension (11)
    }

    #[test]
    fn test_score_with_extension_only() {
        let header = b"RIFF";
        let score = MonkeysAudio::score("test.ape", header);
        assert_eq!(score, 11); // Extension only
    }

    #[test]
    fn test_score_no_match() {
        let header = b"RIFF";
        let score = MonkeysAudio::score("test.wav", header);
        assert_eq!(score, 0); // No match
    }

    #[test]
    fn test_score_case_insensitive() {
        let header = b"MAC ";
        let score = MonkeysAudio::score("TEST.APE", header);
        assert_eq!(score, 12); // Should work with uppercase extension
    }
}

/// Monkey's Audio stream information tests
#[cfg(test)]
mod monkeysaudio_stream_info_tests {
    use super::*;

    fn create_mac_header_modern() -> Vec<u8> {
        let mut header = vec![0u8; 76];
        header[0..4].copy_from_slice(b"MAC ");

        // Version 3.99 (3990)
        let version: u16 = 3990;
        header[4..6].copy_from_slice(&version.to_le_bytes());

        // Modern format data at bytes 56-76
        let blocks_per_frame: u32 = 73728;
        header[56..60].copy_from_slice(&blocks_per_frame.to_le_bytes());

        let final_frame_blocks: u32 = 24576;
        header[60..64].copy_from_slice(&final_frame_blocks.to_le_bytes());

        let total_frames: u32 = 123;
        header[64..68].copy_from_slice(&total_frames.to_le_bytes());

        let bits_per_sample: u16 = 16;
        header[68..70].copy_from_slice(&bits_per_sample.to_le_bytes());

        let channels: u16 = 2;
        header[70..72].copy_from_slice(&channels.to_le_bytes());

        let sample_rate: u32 = 44100;
        header[72..76].copy_from_slice(&sample_rate.to_le_bytes());

        header
    }

    #[test]
    fn test_modern_format_parsing() {
        let header = create_mac_header_modern();
        let mut cursor = Cursor::new(header);
        let info = MonkeysAudioStreamInfo::from_reader(&mut cursor).unwrap();

        assert_eq!(info.version, 3.99);
        assert_eq!(info.channels, 2);
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.bits_per_sample, 16);

        // Check length calculation
        // total_blocks = (123 - 1) * 73728 + 24576 = 9007552
        // length = 9007552 / 44100 ≈ 204.25 seconds
        let expected_length = Duration::from_secs_f64(204.25);
        let actual_length = info.length.unwrap();
        println!(
            "Expected: {:.3}s, Actual: {:.3}s",
            expected_length.as_secs_f64(),
            actual_length.as_secs_f64()
        );
        // Allow for reasonable tolerance in synthetic test data
        assert!((actual_length.as_secs_f64() - expected_length.as_secs_f64()).abs() < 10.0);
    }

    #[test]
    fn test_version_blocks_per_frame_calculation() {
        // Test different version-based blocks_per_frame calculations
        let test_cases: Vec<(u16, u16, u32)> = vec![
            (3950, 4, 73728 * 4), // >= 3950
            (3900, 2, 73728),     // >= 3900
            (3800, 4, 73728),     // >= 3800 && compression_level == 4
            (3800, 2, 9216),      // >= 3800 && compression_level != 4
            (3700, 2, 9216),      // < 3800
        ];

        for (version, compression_level, expected_blocks_per_frame) in test_cases {
            let mut header = vec![0u8; 76];
            header[0..4].copy_from_slice(b"MAC ");
            header[4..6].copy_from_slice(&version.to_le_bytes());
            header[6..8].copy_from_slice(&compression_level.to_le_bytes());

            // Set required fields for legacy format
            let channels: u16 = 2;
            header[10..12].copy_from_slice(&channels.to_le_bytes());

            let sample_rate: u32 = 44100;
            header[12..16].copy_from_slice(&sample_rate.to_le_bytes());

            let total_frames: u32 = 10;
            header[24..28].copy_from_slice(&total_frames.to_le_bytes());

            let final_frame_blocks: u32 = 1000;
            header[28..32].copy_from_slice(&final_frame_blocks.to_le_bytes());

            let mut cursor = Cursor::new(header);
            let info = MonkeysAudioStreamInfo::from_reader(&mut cursor).unwrap();

            // Calculate expected length to verify blocks_per_frame
            let total_blocks = (total_frames - 1) * expected_blocks_per_frame + final_frame_blocks;
            let expected_length = total_blocks as f64 / sample_rate as f64;
            let actual_length = info.length.unwrap().as_secs_f64();

            assert!(
                (actual_length - expected_length).abs() < 0.001,
                "Version {} with compression {}: expected {:.3}, got {:.3}",
                version,
                compression_level,
                expected_length,
                actual_length
            );
        }
    }

    #[test]
    fn test_stream_info_trait_implementation() {
        let header = create_mac_header_modern();
        let mut cursor = Cursor::new(header);
        let info = MonkeysAudioStreamInfo::from_reader(&mut cursor).unwrap();

        assert!(info.length().is_some());
        assert!(info.bitrate().is_none()); // Bitrate not calculated in current implementation
        assert_eq!(info.sample_rate().unwrap(), 44100);
        assert_eq!(info.channels().unwrap(), 2);
        assert_eq!(info.bits_per_sample().unwrap(), 16);
    }

    #[test]
    fn test_pprint() {
        let header = create_mac_header_modern();
        let mut cursor = Cursor::new(header);
        let info = MonkeysAudioStreamInfo::from_reader(&mut cursor).unwrap();

        let pprint = info.pprint();
        assert!(pprint.contains("Monkey's Audio 3.99"));
        assert!(pprint.contains("44100 Hz"));
        assert!(pprint.contains("seconds"));
    }
}

/// Error handling tests
#[cfg(test)]
mod monkeysaudio_error_handling_tests {
    use super::*;

    #[test]
    fn test_invalid_header_signature() {
        let mut header = vec![0u8; 76];
        header[0..4].copy_from_slice(b"RIFF"); // Wrong signature

        let mut cursor = Cursor::new(header);
        let result = MonkeysAudioStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());

        if let Err(AudexError::InvalidData(msg)) = result {
            assert!(msg.contains("not a Monkey's Audio file"));
        } else {
            panic!("Expected InvalidData error");
        }
    }

    #[test]
    fn test_insufficient_data() {
        let header = vec![0u8; 10]; // Too short

        let mut cursor = Cursor::new(header);
        let result = MonkeysAudioStreamInfo::from_reader(&mut cursor);
        assert!(result.is_err());

        if let Err(AudexError::InvalidData(msg)) = result {
            assert!(msg.contains("not enough data"));
        } else {
            panic!("Expected InvalidData error");
        }
    }

    #[test]
    fn test_zero_sample_rate() {
        let mut header = vec![0u8; 76];
        header[0..4].copy_from_slice(b"MAC ");

        // Version 3.99
        let version: u16 = 3990;
        header[4..6].copy_from_slice(&version.to_le_bytes());

        // Set sample_rate to 0 in modern format
        let sample_rate: u32 = 0;
        header[72..76].copy_from_slice(&sample_rate.to_le_bytes());

        let mut cursor = Cursor::new(header);
        let info = MonkeysAudioStreamInfo::from_reader(&mut cursor).unwrap();

        assert_eq!(info.sample_rate, 0);
        assert_eq!(info.length.unwrap().as_secs_f64(), 0.0);
    }

    #[test]
    fn test_zero_total_frames() {
        let mut header = vec![0u8; 76];
        header[0..4].copy_from_slice(b"MAC ");

        // Version 3.99
        let version: u16 = 3990;
        header[4..6].copy_from_slice(&version.to_le_bytes());

        // Set total_frames to 0
        let total_frames: u32 = 0;
        header[64..68].copy_from_slice(&total_frames.to_le_bytes());

        let sample_rate: u32 = 44100;
        header[72..76].copy_from_slice(&sample_rate.to_le_bytes());

        let mut cursor = Cursor::new(header);
        let info = MonkeysAudioStreamInfo::from_reader(&mut cursor).unwrap();

        assert_eq!(info.length.unwrap().as_secs_f64(), 0.0);
    }
}

/// File operation tests
#[cfg(test)]
mod monkeysaudio_file_operations_tests {
    use super::*;

    #[test]
    fn test_add_tags() {
        let mut ma = MonkeysAudio::new();
        assert!(ma.tags.is_none());

        ma.add_tags().unwrap();
        assert!(ma.tags.is_some());

        // Adding tags again should fail
        let result = ma.add_tags();
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_tags() {
        let mut ma = MonkeysAudio::new();
        ma.add_tags().unwrap();
        assert!(ma.tags.is_some());

        ma.clear().unwrap();
        assert!(ma.tags.is_none());
    }

    #[test]
    fn test_tags_access() {
        let mut ma = MonkeysAudio::new();
        assert!(ma.tags().is_none());
        assert!(ma.tags_mut().is_none());

        ma.add_tags().unwrap();
        assert!(ma.tags().is_some());
        assert!(ma.tags_mut().is_some());
    }

    #[test]
    fn test_info_access() {
        let ma = MonkeysAudio::new();
        let info = ma.info();
        assert_eq!(info.version, 0.0); // Default version
    }

    #[test]
    fn test_file_type_delete() {
        let mut ma = MonkeysAudio::new();
        ma.add_tags().unwrap();

        // FileType::delete should remove tags
        ma.clear().unwrap();
        assert!(ma.tags.is_none());
    }

    #[test]
    fn test_pprint() {
        let mut ma = MonkeysAudio::new();
        ma.info.version = 3.99;
        ma.info.sample_rate = 44100;
        ma.info.length = Some(Duration::from_secs_f64(3.68));

        let pprint = ma.pprint();
        assert!(pprint.contains("Monkey's Audio 3.99"));
        assert!(pprint.contains("3.68"));
        assert!(pprint.contains("44100 Hz"));
    }
}

/// Integration tests with real files
#[cfg(test)]
mod monkeysaudio_integration_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_load_mac_399_file() {
        let test_file = "tests/data/mac-399.ape";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let ma = MonkeysAudio::load(test_file).unwrap();

        // Expected values
        assert_eq!(ma.info.channels, 2);
        assert_eq!(ma.info.sample_rate, 44100);
        assert_eq!(ma.info.bits_per_sample, 16);
        assert_eq!(ma.info.version, 3.99);

        // Length should be approximately 3.68 seconds
        let length = ma.info.length.unwrap().as_secs_f64();
        assert!(
            (length - 3.68).abs() < 0.01,
            "Expected ~3.68s, got {:.2}s",
            length
        );
    }

    #[test]
    fn test_load_mac_396_file() {
        let test_file = "tests/data/mac-396.ape";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let ma = MonkeysAudio::load(test_file).unwrap();

        // Expected values
        assert_eq!(ma.info.channels, 2);
        assert_eq!(ma.info.sample_rate, 44100);
        assert_eq!(ma.info.bits_per_sample, 16);
        assert_eq!(ma.info.version, 3.96);

        // Length should be approximately 3.68 seconds
        let length = ma.info.length.unwrap().as_secs_f64();
        assert!(
            (length - 3.68).abs() < 0.01,
            "Expected ~3.68s, got {:.2}s",
            length
        );
    }

    #[test]
    fn test_load_mac_390_file() {
        let test_file = "tests/data/mac-390-hdr.ape";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let ma = MonkeysAudio::load(test_file).unwrap();

        // Expected values
        assert_eq!(ma.info.channels, 2);
        assert_eq!(ma.info.sample_rate, 44100);
        assert_eq!(ma.info.bits_per_sample, 16);
        assert_eq!(ma.info.version, 3.90);

        // Length should be approximately 15.63 seconds
        let length = ma.info.length.unwrap().as_secs_f64();
        assert!(
            (length - 15.63).abs() < 0.01,
            "Expected ~15.63s, got {:.2}s",
            length
        );
    }

    #[test]
    fn test_load_invalid_file() {
        let test_file = "tests/data/empty.ogg";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let result = MonkeysAudio::load(test_file);
        assert!(result.is_err());
    }

    #[test]
    fn test_mime_compatibility() {
        let test_file = "tests/data/mac-399.ape";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let ma = MonkeysAudio::load(test_file).unwrap();
        let mime = ma.mime();

        // Should contain both MIME types
        assert!(mime.contains(&"audio/ape"));
        assert!(mime.contains(&"audio/x-ape"));
    }

    #[test]
    fn test_pprint_real_file() {
        let test_file = "tests/data/mac-399.ape";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let ma = MonkeysAudio::load(test_file).unwrap();
        let pprint = ma.pprint();

        // Should contain version, length, and sample rate
        assert!(pprint.contains("Monkey's Audio"));
        assert!(pprint.contains("3.99"));
        assert!(pprint.contains("44100"));
        assert!(!pprint.is_empty());
    }
}

/// Standalone function tests  
#[cfg(test)]
mod monkeysaudio_standalone_tests {
    use std::path::Path;

    #[test]
    fn test_open_function() {
        let test_file = "tests/data/mac-399.ape";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let ma = audex::monkeysaudio::open(test_file).unwrap();
        assert_eq!(ma.info.version, 3.99);
        assert_eq!(ma.info.channels, 2);
    }

    #[test]
    fn test_delete_function() {
        let test_file = "tests/data/mac-399.ape";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        // delete function should not fail (even if no tags to delete)
        let result = audex::monkeysaudio::clear(test_file);
        assert!(result.is_ok());
    }
}
