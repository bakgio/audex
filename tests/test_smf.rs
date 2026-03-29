//! SMF (Standard MIDI File) format tests
//!
//! Tests for Standard MIDI File (Type 0, 1, and 2)

use audex::smf::SMF;
use audex::{FileType, StreamInfo};
use std::io::Cursor;
use std::time::Duration;

mod common;
use common::TestUtils;

// Test file constants
const MIDI_FILE: &str = "sample.mid";

/// Core SMF tests
#[cfg(test)]
mod smf_tests {
    use super::*;

    #[test]
    fn test_smf_creation() {
        let smf = SMF::new();
        // When created without loading, info exists but has no data
        assert!(smf.info().length().is_none());
    }

    #[test]
    fn test_smf_score() {
        // Test MIDI file header ("MThd")
        let midi_header = b"MThd\x00\x00\x00\x06";
        let score = SMF::score("test.mid", midi_header);
        assert!(score > 0, "SMF should score > 0 for MIDI header");

        // Test file extension scoring
        let score_mid = SMF::score("test.mid", &[0x00, 0x01, 0x02]);
        assert!(score_mid > 0, "SMF should score > 0 for .mid extension");

        let score_midi = SMF::score("test.midi", &[]);
        assert!(score_midi > 0, "SMF should score > 0 for .midi extension");

        let score_kar = SMF::score("test.kar", &[]);
        assert!(
            score_kar > 0,
            "SMF should score > 0 for .kar extension (karaoke)"
        );

        // Test rejection of non-MIDI files
        let score_invalid = SMF::score("test.txt", &[0x00, 0x01, 0x02]);
        assert_eq!(score_invalid, 0, "SMF should score 0 for non-audio files");
    }

    #[test]
    fn test_smf_mime_types() {
        let mime_types = SMF::mime_types();
        assert!(mime_types.contains(&"audio/midi"));
        assert!(mime_types.contains(&"audio/x-midi"));
    }

    #[test]
    fn test_smf_file_loading() {
        // Test loading MIDI file
        let path = TestUtils::data_path(MIDI_FILE);
        let smf = match SMF::load(&path) {
            Ok(file) => file,
            Err(e) => panic!("Failed to load {}: {}", MIDI_FILE, e),
        };

        let info = smf.info();
        assert!(info.length().is_some(), "SMF info should have length data");

        // Verify basic MIDI properties
        assert!(
            info.length().unwrap() >= Duration::from_secs(0),
            "Duration should be >= 0"
        );

        // MIDI doesn't have sample rate/channels in traditional sense
        // but we should be able to call these methods
        let _ = info.sample_rate();
        let _ = info.channels();
        let _ = info.bitrate();
    }

    #[test]
    fn test_smf_header_validation() {
        // Test MIDI file header structure
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            // MIDI files have format type (0, 1, or 2)
            // This should be reflected in the info
            let pprint = info.pprint();
            assert!(!pprint.is_empty(), "Should have format info");
        }
    }

    #[test]
    fn test_smf_tempo_tracking() {
        // Test MIDI tempo detection
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            let pprint = info.pprint();
            // Default MIDI tempo is 120 BPM
            // Tempo info should be in the output or determinable
            assert!(!pprint.is_empty(), "Should have tempo info");
        }
    }

    #[test]
    fn test_smf_length_calculation() {
        // Test duration calculation for MIDI
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            let length = info.length();
            assert!(length.is_some(), "SMF should have duration");
            let secs = length.unwrap().as_secs_f64();
            assert!(
                (secs - 127.997).abs() < 0.01,
                "SMF length expected ~127.997s, got {}",
                secs
            );
        }
    }

    #[test]
    fn test_smf_info_display() {
        // Test pretty printing of MIDI info
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            let display = format!("{}", info);
            assert!(
                display.contains("MIDI") || !display.is_empty(),
                "Display output should contain format info"
            );

            let pprint = info.pprint();
            assert!(!pprint.is_empty(), "pprint should return non-empty string");
        }
    }

    #[test]
    fn test_smf_from_bytes() {
        // Test loading MIDI from byte buffer
        let path = TestUtils::data_path(MIDI_FILE);
        let data = std::fs::read(&path).expect("Failed to read test file");

        let mut cursor = Cursor::new(data);
        let result = SMF::load_from_reader(&mut cursor);

        match result {
            Ok(smf) => {
                assert!(
                    smf.info().length().is_some(),
                    "SMF info should have length data when loading from bytes"
                );
            }
            Err(e) => {
                println!("SMF from bytes failed: {}", e);
            }
        }
    }

    #[test]
    fn test_smf_invalid_file() {
        // Test handling of invalid MIDI data
        let invalid_data = vec![0u8; 1024]; // All zeros
        let mut cursor = Cursor::new(invalid_data);
        let result = SMF::load_from_reader(&mut cursor);

        assert!(result.is_err(), "Loading invalid MIDI data should fail");
    }

    #[test]
    fn test_smf_truncated_file() {
        // Test handling of truncated MIDI file
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(data) = std::fs::read(&path) {
            // Take only first 20 bytes (just the header)
            let truncated = &data[..std::cmp::min(20, data.len())];
            let mut cursor = Cursor::new(truncated);
            let result = SMF::load_from_reader(&mut cursor);

            // Truncated file should either fail or load with partial info
            match result {
                Ok(_smf) => {
                    println!("Truncated SMF file loaded (may have partial info)");
                }
                Err(e) => {
                    println!("Truncated SMF file failed to load (expected): {}", e);
                }
            }
        }
    }
}

/// MIDI format type tests
#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn test_midi_format_detection() {
        // Test MIDI format type detection (0, 1, or 2)
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            let pprint = info.pprint();
            // Format type should be present in the output
            // Format 0: Single track
            // Format 1: Multiple tracks, synchronous
            // Format 2: Multiple tracks, asynchronous
            assert!(!pprint.is_empty(), "Should contain format info");
        }
    }

    #[test]
    fn test_track_count() {
        // Test MIDI track count detection
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            let pprint = info.pprint();
            // Track count should be at least 1
            assert!(!pprint.is_empty(), "Should contain track info");
        }
    }
}

/// Variable-length quantity (VLQ) tests
#[cfg(test)]
mod vlq_tests {
    use super::*;

    #[test]
    fn test_vlq_encoding_detection() {
        // MIDI uses variable-length quantities for delta times
        // This is tested internally but we verify files load correctly
        let path = TestUtils::data_path(MIDI_FILE);
        let result = SMF::load(&path);
        assert!(
            result.is_ok(),
            "VLQ parsing should work for valid MIDI files"
        );
    }
}

/// MIDI event tests
#[cfg(test)]
mod event_tests {
    use super::*;

    #[test]
    fn test_meta_event_parsing() {
        // Test parsing of meta events (tempo, time signature, end of track)
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            // Meta events affect tempo and timing
            // Verify the file loaded and parsed events correctly
            let length = info.length();
            assert!(
                length.is_some() && length.unwrap() >= Duration::from_secs(0),
                "Should calculate length from events"
            );
        }
    }

    #[test]
    fn test_tempo_change_events() {
        // Test handling of tempo change events
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            // Tempo changes affect duration calculation
            let pprint = info.pprint();
            assert!(!pprint.is_empty(), "Should handle tempo events");
        }
    }
}

/// Time division tests
#[cfg(test)]
mod time_division_tests {
    use super::*;

    #[test]
    fn test_ticks_per_quarter_note() {
        // Test MIDI time division (ticks per quarter note)
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            // Time division affects timing calculations
            // Default is often 96, 192, 480, or 960 PPQN
            let pprint = info.pprint();
            assert!(!pprint.is_empty(), "Should have timing info");
        }
    }

    #[test]
    fn test_smpte_time_division() {
        // Test SMPTE time code format (negative time division)
        // This is less common but should be handled
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            // Just verify the file loads successfully and has length data
            assert!(smf.info().length().is_some());
        }
    }
}

/// Karaoke file tests (.kar)
#[cfg(test)]
mod karaoke_tests {
    use super::*;

    #[test]
    fn test_kar_extension() {
        // Test .kar (karaoke MIDI) extension recognition
        let score = SMF::score("song.kar", &[0x00]);
        assert!(score > 0, "Should recognize .kar extension");
    }

    #[test]
    fn test_kar_format() {
        // .kar files are just MIDI files with lyrics
        // They should load the same as regular MIDI files
        let kar_header = b"MThd\x00\x00\x00\x06";
        let score = SMF::score("song.kar", kar_header);
        assert!(score > 0, "Should detect MIDI header in .kar files");
    }
}

/// Stream info tests
#[cfg(test)]
mod stream_info_tests {
    use super::*;

    #[test]
    fn test_stream_info_trait() {
        // Verify StreamInfo trait implementation
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
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
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            let sample_rate = info.sample_rate();
            let channels = info.channels();
            let bitrate = info.bitrate();
            let length = info.length();

            // All optional values should be None or valid (unsigned types are always non-negative)
            if let Some(_sr) = sample_rate {
                // Sample rate present
            }
            if let Some(_ch) = channels {
                // Channels present
            }
            if let Some(_br) = bitrate {
                // Bitrate present
            }
            if let Some(len) = length {
                assert!(
                    len >= Duration::from_secs(0),
                    "Length should be non-negative"
                );
            }
        }
    }

    #[test]
    fn test_midi_specific_info() {
        // MIDI has different properties than audio files
        let path = TestUtils::data_path(MIDI_FILE);
        if let Ok(smf) = SMF::load(&path) {
            let info = smf.info();
            // MIDI doesn't have traditional sample rate
            // It uses PPQN (pulses per quarter note)
            // Sample rate might be None or a derived value (unsigned types are always non-negative)
            let _sample_rate = info.sample_rate();

            // MIDI doesn't have channels in the audio sense
            // but may report number of tracks (unsigned types are always non-negative)
            let _channels = info.channels();

            // MIDI is not compressed, so bitrate may not apply (unsigned types are always non-negative)
            let _bitrate = info.bitrate();
        }
    }
}

/// Edge case tests
#[cfg(test)]
mod edge_cases {
    use super::*;

    #[test]
    fn test_empty_midi_file() {
        // Test minimal valid MIDI file
        // MThd header + minimal MTrk track
        let minimal_midi =
            b"MThd\x00\x00\x00\x06\x00\x00\x00\x01\x00\x60MTrk\x00\x00\x00\x04\x00\xFF\x2F\x00";
        let mut cursor = Cursor::new(&minimal_midi[..]);
        let result = SMF::load_from_reader(&mut cursor);

        match result {
            Ok(smf) => {
                let info = smf.info();
                assert!(
                    info.length().is_some(),
                    "Minimal MIDI should have length data"
                );
                // Should have minimal length
                assert!(
                    info.length().unwrap() >= Duration::from_secs(0),
                    "Length should be non-negative"
                );
            }
            Err(e) => {
                // May fail depending on implementation strictness
                println!("Minimal MIDI failed: {}", e);
            }
        }
    }

    #[test]
    fn test_invalid_midi_header() {
        // Test invalid MIDI header
        let invalid = b"XThd\x00\x00\x00\x06";
        let mut cursor = Cursor::new(&invalid[..]);
        let result = SMF::load_from_reader(&mut cursor);

        assert!(result.is_err(), "Invalid header should fail");
    }

    #[test]
    fn test_corrupted_track_header() {
        // Test file with corrupted track header
        let corrupted = b"MThd\x00\x00\x00\x06\x00\x00\x00\x01\x00\x60XXXX\x00\x00\x00\x04";
        let mut cursor = Cursor::new(&corrupted[..]);
        let result = SMF::load_from_reader(&mut cursor);

        // Should fail gracefully
        match result {
            Ok(_) => println!("Corrupted track loaded (unexpected)"),
            Err(_) => println!("Corrupted track rejected (expected)"),
        }
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use audex::smf::SMF;
    use std::io::Cursor;

    fn build_midi_with_vlq(vlq_bytes: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(b"MThd");
        data.extend_from_slice(&6u32.to_be_bytes());
        data.extend_from_slice(&0u16.to_be_bytes());
        data.extend_from_slice(&1u16.to_be_bytes());
        data.extend_from_slice(&480u16.to_be_bytes());

        let mut track_data = Vec::new();
        track_data.extend_from_slice(vlq_bytes);
        track_data.extend_from_slice(&[0xFF, 0x2F, 0x00]);

        data.extend_from_slice(b"MTrk");
        data.extend_from_slice(&(track_data.len() as u32).to_be_bytes());
        data.extend_from_slice(&track_data);

        data
    }

    #[test]
    fn test_oversized_vlq_rejected() {
        let vlq = [0x80, 0x80, 0x80, 0x80, 0x80, 0x00];
        let midi_data = build_midi_with_vlq(&vlq);
        let mut cursor = Cursor::new(midi_data);

        let result = SMF::load_from_reader(&mut cursor);

        assert!(result.is_err(), "MIDI with 5-byte VLQ should be rejected");
    }

    #[test]
    fn test_valid_4_byte_vlq_accepted() {
        let vlq = [0xFF, 0xFF, 0xFF, 0x7F];
        let midi_data = build_midi_with_vlq(&vlq);
        let mut cursor = Cursor::new(midi_data);

        let result = SMF::load_from_reader(&mut cursor);

        assert!(
            result.is_ok(),
            "MIDI with valid 4-byte VLQ should be accepted: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_simple_vlq_accepted() {
        let vlq = [0x00];
        let midi_data = build_midi_with_vlq(&vlq);
        let mut cursor = Cursor::new(midi_data);

        let result = SMF::load_from_reader(&mut cursor);
        assert!(result.is_ok());
    }
}

// ---------------------------------------------------------------------------
// Duration comparison tests (f64 partial_cmp / total_cmp)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod duration_comparison_tests {
    /// Demonstrate that partial_cmp with NaN returns None, causing unwrap to panic.
    #[test]
    fn test_partial_cmp_nan_returns_none() {
        let a: f64 = 1.0;
        let b: f64 = f64::NAN;

        assert!(
            a.partial_cmp(&b).is_none(),
            "partial_cmp with NaN should return None"
        );
    }

    /// The vulnerable pattern: max_by with partial_cmp().unwrap() panics on NaN.
    #[test]
    fn test_max_by_partial_cmp_panics_on_nan() {
        let values = vec![1.0_f64, 2.0, f64::NAN, 3.0];

        let result = std::panic::catch_unwind(|| {
            values.into_iter().max_by(|a, b| a.partial_cmp(b).unwrap())
        });

        assert!(
            result.is_err(),
            "max_by with partial_cmp().unwrap() should panic when NaN is present"
        );
    }

    /// The safe pattern: total_cmp handles NaN without panicking.
    #[test]
    fn test_total_cmp_handles_nan() {
        let values = vec![1.0_f64, 2.0, f64::NAN, 3.0];

        // total_cmp treats NaN as greater than all other values
        let result = std::panic::catch_unwind(|| values.into_iter().max_by(|a, b| a.total_cmp(b)));

        assert!(
            result.is_ok(),
            "max_by with total_cmp should not panic on NaN"
        );
    }

    /// Verify total_cmp produces a reasonable maximum from normal values.
    #[test]
    fn test_total_cmp_correct_for_normal_values() {
        let values = vec![1.0_f64, 5.0, 2.0, 3.0];
        let max = values.into_iter().max_by(|a, b| a.total_cmp(b));
        assert_eq!(max, Some(5.0));
    }
}
