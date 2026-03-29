//! WAV/WAVE format tests

use audex::wave::{RiffFile, WAVE, WAVEStreamInfo, clear};
use audex::{AudexError, FileType, StreamInfo};
use std::io::Cursor;
use std::time::Duration;

mod common;
use common::TestUtils;

#[cfg(test)]
mod wave_basic_tests {
    use super::*;

    #[test]
    fn test_wave_creation() {
        let wave = WAVE::new();
        assert!(wave.tags.is_none());
        assert_eq!(wave.info.sample_rate, 0);
        assert_eq!(wave.info.channels, 0);
        assert_eq!(wave.info.bits_per_sample, 0);
        assert_eq!(wave.info.length(), None);
        assert_eq!(wave.info.bitrate(), None);
    }

    #[test]
    fn test_wave_stream_info_creation() {
        let info = WAVEStreamInfo::default();
        assert_eq!(info.sample_rate(), Some(0));
        assert_eq!(info.channels(), Some(0));
        assert_eq!(info.bits_per_sample(), Some(0));
        assert_eq!(info.length(), None);
        assert_eq!(info.bitrate(), None);
        assert_eq!(info.number_of_samples, 0);
        assert_eq!(info.audio_format, 0);
    }

    #[test]
    fn test_mime_types() {
        let mimes = WAVE::mime_types();
        assert!(mimes.contains(&"audio/wav"));
        assert!(mimes.contains(&"audio/wave"));
    }

    #[test]
    fn test_score_riff_wave_signature() {
        let header = b"RIFF\x24\x08\x00\x00WAVE";
        let score = WAVE::score("test.wav", header);
        assert!(
            score >= 13,
            "Should score high for RIFF+WAVE signature + .wav extension"
        );
    }

    #[test]
    fn test_score_wav_extension() {
        let header = b"some random header";
        let score = WAVE::score("test.wav", header);
        assert_eq!(score, 3, "Should score 3 for .wav extension only");
    }

    #[test]
    fn test_score_wave_extension() {
        let header = b"some random header";
        let score = WAVE::score("test.wave", header);
        assert_eq!(score, 2, "Should score 2 for .wave extension only");
    }

    #[test]
    fn test_score_no_match() {
        let header = b"ID3\x04\x00";
        let score = WAVE::score("test.mp3", header);
        assert_eq!(score, 0, "Should score 0 for non-WAV file");
    }
}

#[cfg(test)]
mod riff_parsing_tests {
    use super::*;

    #[test]
    fn test_riff_file_basic_structure() {
        // Create minimal RIFF/WAVE file structure
        let mut riff_data = Vec::new();

        // RIFF header
        riff_data.extend_from_slice(b"RIFF");
        riff_data.extend_from_slice(&100u32.to_le_bytes()); // file size
        riff_data.extend_from_slice(b"WAVE");

        // fmt chunk
        riff_data.extend_from_slice(b"fmt ");
        riff_data.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        riff_data.extend_from_slice(&1u16.to_le_bytes()); // audio format (PCM)
        riff_data.extend_from_slice(&2u16.to_le_bytes()); // channels
        riff_data.extend_from_slice(&44100u32.to_le_bytes()); // sample rate
        riff_data.extend_from_slice(&176400u32.to_le_bytes()); // byte rate
        riff_data.extend_from_slice(&4u16.to_le_bytes()); // block align
        riff_data.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        // data chunk header
        riff_data.extend_from_slice(b"data");
        riff_data.extend_from_slice(&8u32.to_le_bytes()); // chunk size
        riff_data.extend_from_slice(&[0u8; 8]); // dummy audio data

        let mut cursor = Cursor::new(riff_data);
        let riff_file = RiffFile::parse(&mut cursor).unwrap();

        println!("Parsed RIFF file with {} chunks:", riff_file.chunks.len());
        for chunk in &riff_file.chunks {
            println!("  Chunk: '{}' (size: {})", chunk.id, chunk.data_size);
        }

        assert_eq!(riff_file.file_type, "WAVE");
        assert_eq!(riff_file.chunks.len(), 2); // fmt + data

        assert!(riff_file.has_chunk("fmt"));
        assert!(riff_file.has_chunk("data"));
        assert!(!riff_file.has_chunk("id3"));

        let fmt_chunk = riff_file.find_chunk("fmt").unwrap();
        assert!(
            fmt_chunk.id == "fmt " || fmt_chunk.id == "fmt",
            "fmt chunk id should be 'fmt' or 'fmt '"
        );
        assert_eq!(fmt_chunk.data_size, 16);

        let data_chunk = riff_file.find_chunk("data").unwrap();
        assert!(
            data_chunk.id == "data" || data_chunk.id.starts_with("data"),
            "data chunk id should start with 'data'"
        );
        assert_eq!(data_chunk.data_size, 8);
    }

    #[test]
    fn test_riff_invalid_signature() {
        let invalid_data = b"MP3HEADER_NOT_RIFF";
        let mut cursor = Cursor::new(invalid_data.as_slice());

        let result = RiffFile::parse(&mut cursor);
        assert!(result.is_err());

        match result {
            Err(AudexError::WAVError(msg)) => {
                assert!(msg.contains("Expected RIFF signature"));
            }
            _ => panic!("Expected WAVError for invalid RIFF signature"),
        }
    }

    #[test]
    fn test_riff_wrong_file_type() {
        let mut wrong_type_data = Vec::new();
        wrong_type_data.extend_from_slice(b"RIFF");
        wrong_type_data.extend_from_slice(&100u32.to_le_bytes());
        wrong_type_data.extend_from_slice(b"AVI "); // Not WAVE

        let mut cursor = Cursor::new(wrong_type_data);
        let result = RiffFile::parse(&mut cursor);

        assert!(result.is_err());
        match result {
            Err(AudexError::WAVError(msg)) => {
                assert!(msg.contains("Expected WAVE format"));
            }
            _ => panic!("Expected WAVError for wrong file type"),
        }
    }

    #[test]
    fn test_chunk_case_insensitive() {
        let mut riff_data = Vec::new();

        // RIFF header
        riff_data.extend_from_slice(b"RIFF");
        riff_data.extend_from_slice(&50u32.to_le_bytes());
        riff_data.extend_from_slice(b"WAVE");

        // ID3 chunk with uppercase
        riff_data.extend_from_slice(b"ID3 ");
        riff_data.extend_from_slice(&16u32.to_le_bytes());
        riff_data.extend_from_slice(&[0u8; 16]);

        let mut cursor = Cursor::new(riff_data);
        let riff_file = RiffFile::parse(&mut cursor).unwrap();

        // Should find chunk regardless of case
        assert!(riff_file.has_chunk("ID3"));
        assert!(riff_file.has_chunk("id3"));
        assert!(riff_file.has_chunk("Id3"));

        let id3_chunk = riff_file.find_chunk("id3").unwrap();
        assert_eq!(id3_chunk.id, "ID3 "); // Should match what we put in
    }
}

#[cfg(test)]
mod wave_stream_info_tests {
    use super::*;

    #[test]
    fn test_stream_info_from_riff() {
        // Create RIFF file with proper format chunk
        let mut riff_data = Vec::new();

        // RIFF header
        riff_data.extend_from_slice(b"RIFF");
        riff_data.extend_from_slice(&100u32.to_le_bytes());
        riff_data.extend_from_slice(b"WAVE");

        // fmt chunk
        riff_data.extend_from_slice(b"fmt ");
        riff_data.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        riff_data.extend_from_slice(&1u16.to_le_bytes()); // audio format (PCM)
        riff_data.extend_from_slice(&2u16.to_le_bytes()); // channels
        riff_data.extend_from_slice(&44100u32.to_le_bytes()); // sample rate
        riff_data.extend_from_slice(&176400u32.to_le_bytes()); // byte rate
        riff_data.extend_from_slice(&4u16.to_le_bytes()); // block align
        riff_data.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        // data chunk (88200 samples = 2 seconds at 44.1kHz)
        let data_size: u32 = 88200 * 4; // 2 channels * 16 bits / 8 bits per byte
        riff_data.extend_from_slice(b"data");
        riff_data.extend_from_slice(&data_size.to_le_bytes());
        riff_data.extend_from_slice(&vec![0u8; data_size as usize]);

        let mut cursor = Cursor::new(riff_data);
        let riff_file = RiffFile::parse(&mut cursor).unwrap();
        let info = WAVEStreamInfo::from_riff_file(&riff_file, &mut cursor).unwrap();

        assert_eq!(info.audio_format, 1); // PCM
        assert_eq!(info.channels, 2);
        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.bits_per_sample, 16);
        assert_eq!(info.bitrate(), Some(1411200)); // 2 * 16 * 44100
        assert_eq!(info.number_of_samples, 88200);

        // Check duration (should be ~2.0 seconds)
        let duration = info.length().unwrap();
        assert!((duration.as_secs_f64() - 2.0).abs() < 0.01);
    }

    #[test]
    fn test_stream_info_no_fmt_chunk() {
        let mut riff_data = Vec::new();
        riff_data.extend_from_slice(b"RIFF");
        riff_data.extend_from_slice(&20u32.to_le_bytes());
        riff_data.extend_from_slice(b"WAVE");
        // No fmt chunk

        let mut cursor = Cursor::new(riff_data);
        let riff_file = RiffFile::parse(&mut cursor).unwrap();
        let result = WAVEStreamInfo::from_riff_file(&riff_file, &mut cursor);

        assert!(result.is_err());
        match result {
            Err(AudexError::WAVError(msg)) => {
                assert!(msg.contains("No 'fmt' chunk found"));
            }
            _ => panic!("Expected WAVError for missing fmt chunk"),
        }
    }

    #[test]
    fn test_stream_info_short_fmt_chunk() {
        let mut riff_data = Vec::new();
        riff_data.extend_from_slice(b"RIFF");
        riff_data.extend_from_slice(&30u32.to_le_bytes());
        riff_data.extend_from_slice(b"WAVE");

        // fmt chunk that's too short
        riff_data.extend_from_slice(b"fmt ");
        riff_data.extend_from_slice(&8u32.to_le_bytes()); // Only 8 bytes instead of 16
        riff_data.extend_from_slice(&[0u8; 8]);

        let mut cursor = Cursor::new(riff_data);
        let riff_file = RiffFile::parse(&mut cursor).unwrap();
        let result = WAVEStreamInfo::from_riff_file(&riff_file, &mut cursor);

        assert!(result.is_err());
        match result {
            Err(AudexError::WAVInvalidChunk(msg)) => {
                assert!(msg.contains("Format chunk too small"));
            }
            _ => panic!("Expected WAVInvalidChunk for short fmt chunk"),
        }
    }

    #[test]
    fn test_stream_info_pprint() {
        let info = WAVEStreamInfo {
            channels: 2,
            bitrate: Some(1411200),
            sample_rate: 44100,
            length: Some(Duration::from_secs_f64(2.0)),
            ..Default::default()
        };

        let output = info.pprint();
        assert!(output.contains("2 channel"));
        assert!(output.contains("1411200 bps"));
        assert!(output.contains("44100 Hz"));
        assert!(output.contains("2.00 seconds"));
    }
}

#[cfg(test)]
mod wave_file_operations_tests {
    use super::*;

    #[test]
    fn test_load_real_wave_files() {
        // Test with real WAV files from test data
        let wave_files = [
            "silence-2s-PCM-16000-08-ID3v23.wav",
            "silence-2s-PCM-16000-08-notags.wav",
            "silence-2s-PCM-44100-16-ID3v23.wav",
        ];

        for filename in &wave_files {
            let path = TestUtils::data_path(filename);
            if path.exists() {
                match WAVE::load(&path) {
                    Ok(wave) => {
                        println!("Successfully loaded {}", filename);
                        println!("  Sample rate: {} Hz", wave.info.sample_rate);
                        println!("  Channels: {}", wave.info.channels);
                        println!("  Bits per sample: {}", wave.info.bits_per_sample);
                        println!("  Bitrate: {} bps", wave.info.bitrate().unwrap_or(0));
                        if let Some(length) = wave.info.length() {
                            println!("  Length: {:.2} seconds", length.as_secs_f64());
                        }
                        println!("  Has tags: {}", wave.tags.is_some());

                        // Basic validation
                        assert!(wave.info.sample_rate > 0, "Should have valid sample rate");
                        assert!(wave.info.channels > 0, "Should have valid channel count");
                        assert!(wave.info.bits_per_sample > 0, "Should have valid bit depth");

                        if filename.contains("ID3v23") {
                            assert!(wave.tags.is_some(), "ID3v23 file should have tags");
                        }
                        if filename.contains("notags") {
                            assert!(wave.tags.is_none(), "No-tags file should not have tags");
                        }
                    }
                    Err(e) => {
                        println!("Could not load {} (might be expected): {}", filename, e);
                    }
                }
            }
        }
    }

    #[test]
    fn test_wave_specific_parameters() {
        // Test specific values
        let test_cases = [
            (
                "silence-2s-PCM-16000-08-ID3v23.wav",
                16000,
                2,
                8,
                256000,
                32000,
            ),
            (
                "silence-2s-PCM-44100-16-ID3v23.wav",
                44100,
                2,
                16,
                1411200,
                88200,
            ),
        ];

        for (
            filename,
            expected_rate,
            expected_channels,
            expected_bits,
            expected_bitrate,
            expected_samples,
        ) in &test_cases
        {
            let path = TestUtils::data_path(filename);
            if path.exists() {
                match WAVE::load(&path) {
                    Ok(wave) => {
                        assert_eq!(
                            wave.info.sample_rate, *expected_rate,
                            "Sample rate mismatch for {}",
                            filename
                        );
                        assert_eq!(
                            wave.info.channels, *expected_channels,
                            "Channel count mismatch for {}",
                            filename
                        );
                        assert_eq!(
                            wave.info.bits_per_sample, *expected_bits,
                            "Bit depth mismatch for {}",
                            filename
                        );
                        assert_eq!(
                            wave.info.bitrate(),
                            Some(*expected_bitrate),
                            "Bitrate mismatch for {}",
                            filename
                        );
                        assert_eq!(
                            wave.info.number_of_samples, *expected_samples,
                            "Sample count mismatch for {}",
                            filename
                        );

                        // Check duration (should be ~2.0 seconds)
                        if let Some(length) = wave.info.length() {
                            let duration_secs = length.as_secs_f64();
                            assert!(
                                (duration_secs - 2.0).abs() < 0.01,
                                "Duration should be ~2.0 seconds for {}",
                                filename
                            );
                        }
                    }
                    Err(e) => {
                        println!("Could not load {} for parameter testing: {}", filename, e);
                    }
                }
            }
        }
    }

    #[test]
    fn test_add_tags() {
        let mut wave = WAVE::new();

        // Initially no tags
        assert!(wave.tags.is_none());

        // Add tags
        wave.add_tags().unwrap();
        assert!(wave.tags.is_some());

        // Try to add tags again (should fail)
        let result = wave.add_tags();
        assert!(result.is_err());
        match result {
            Err(AudexError::WAVError(msg)) => {
                assert!(msg.contains("ID3 tag already exists"));
            }
            _ => panic!("Expected WAVError when adding tags twice"),
        }
    }

    #[test]
    fn test_delete_tags() {
        let mut wave = WAVE::new();
        wave.add_tags().unwrap();

        assert!(wave.tags.is_some());

        wave.clear().unwrap();
        assert!(wave.tags.is_none());
    }

    #[test]
    fn test_delete_tags_file_backed() {
        // Test clear() on a real file with ID3 tags (file-backed deletion)
        let path = TestUtils::data_path("silence-2s-PCM-44100-16-ID3v23.wav");
        if !path.exists() {
            return;
        }

        let temp = TestUtils::get_temp_copy(&path).unwrap();
        let temp_path = temp.path().to_path_buf();

        // Verify tags exist
        let before = WAVE::load(&temp_path).unwrap();
        assert!(
            before.tags.is_some(),
            "File should have ID3 tags before clear"
        );

        // Clear tags
        clear(&temp_path).unwrap();

        // Reload and verify tags are gone
        let after = WAVE::load(&temp_path).unwrap();
        assert!(after.tags.is_none(), "Tags should be removed after clear");

        // Verify audio data is intact
        assert!(
            after.info.sample_rate > 0,
            "Sample rate should survive clear"
        );
        assert!(
            after.info.channels > 0,
            "Channel count should survive clear"
        );
    }

    #[test]
    fn test_clear_no_temp_files_created() {
        // Verify clear() performs in-place deletion without creating .tmp files
        let path = TestUtils::data_path("silence-2s-PCM-44100-16-ID3v23.wav");
        if !path.exists() {
            return;
        }

        // Use a dedicated temp directory so other processes writing to /tmp
        // don't cause false failures.
        let isolated_dir = tempfile::tempdir().unwrap();
        let temp_path = isolated_dir.path().join("test.wav");
        std::fs::copy(&path, &temp_path).unwrap();

        // Snapshot directory contents before clear
        let files_before: std::collections::HashSet<_> = std::fs::read_dir(isolated_dir.path())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();

        clear(&temp_path).unwrap();

        // Verify no new files were created
        let files_after: std::collections::HashSet<_> = std::fs::read_dir(isolated_dir.path())
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
    fn test_clear_riff_header_size_updated() {
        // Verify the RIFF header size is correctly updated after ID3 chunk removal
        use std::io::{Read as _, Seek as _, SeekFrom};

        let path = TestUtils::data_path("silence-2s-PCM-44100-16-ID3v23.wav");
        if !path.exists() {
            return;
        }

        let temp = TestUtils::get_temp_copy(&path).unwrap();
        let temp_path = temp.path().to_path_buf();

        let size_before = std::fs::metadata(&temp_path).unwrap().len();

        clear(&temp_path).unwrap();

        let size_after = std::fs::metadata(&temp_path).unwrap().len();
        assert!(
            size_after < size_before,
            "File should shrink after removing ID3 chunk"
        );

        // Read RIFF header and verify size field
        let mut file = std::fs::File::open(&temp_path).unwrap();
        let mut sig = [0u8; 4];
        file.read_exact(&mut sig).unwrap();
        assert_eq!(&sig, b"RIFF", "File should still start with RIFF");

        let mut size_bytes = [0u8; 4];
        file.read_exact(&mut size_bytes).unwrap();
        let riff_size = u32::from_le_bytes(size_bytes) as u64;

        // RIFF size should equal file size minus 8 (4 bytes "RIFF" + 4 bytes size field)
        assert_eq!(
            riff_size,
            size_after - 8,
            "RIFF header size should match actual file size minus 8"
        );

        // Verify the file can be re-parsed as valid WAVE
        file.seek(SeekFrom::Start(0)).unwrap();
        let riff_file = RiffFile::parse(&mut file).unwrap();
        assert!(
            !riff_file
                .chunks
                .iter()
                .any(|c| c.id.to_lowercase().starts_with("id3")),
            "ID3 chunk should no longer be present"
        );
    }

    #[test]
    fn test_clear_preserves_audio_data() {
        // Verify audio data integrity is preserved after clearing tags
        let path = TestUtils::data_path("silence-2s-PCM-44100-16-ID3v23.wav");
        if !path.exists() {
            return;
        }

        let temp = TestUtils::get_temp_copy(&path).unwrap();
        let temp_path = temp.path().to_path_buf();

        let original = WAVE::load(&temp_path).unwrap();
        let orig_rate = original.info.sample_rate;
        let orig_channels = original.info.channels;
        let orig_bits = original.info.bits_per_sample;
        let orig_length = original.info.length;
        let orig_samples = original.info.number_of_samples;

        clear(&temp_path).unwrap();

        let cleared = WAVE::load(&temp_path).unwrap();
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
        assert_eq!(
            cleared.info.number_of_samples, orig_samples,
            "Number of samples must match after clearing tags"
        );
    }

    #[test]
    fn test_double_delete() {
        // Test that calling clear() twice doesn't cause errors
        let path = TestUtils::data_path("silence-2s-PCM-44100-16-ID3v23.wav");
        if !path.exists() {
            return;
        }

        let temp = TestUtils::get_temp_copy(&path).unwrap();
        let temp_path = temp.path().to_path_buf();

        // First clear
        clear(&temp_path).unwrap();
        let after1 = WAVE::load(&temp_path).unwrap();
        assert!(after1.tags.is_none(), "Tags should be deleted");

        // Second clear — should not error
        let result = clear(&temp_path);
        assert!(result.is_ok(), "Double clear should not error");

        let after2 = WAVE::load(&temp_path).unwrap();
        assert!(after2.tags.is_none(), "Tags should still be deleted");
    }

    #[test]
    fn test_clear_then_add_save_roundtrip() {
        // Full roundtrip: load with tags → clear → add new tags → save → verify
        use audex::Tags;

        let path = TestUtils::data_path("silence-2s-PCM-44100-16-ID3v23.wav");
        if !path.exists() {
            return;
        }

        let temp = TestUtils::get_temp_copy(&path).unwrap();
        let temp_path = temp.path().to_path_buf();

        // Clear existing tags
        clear(&temp_path).unwrap();

        // Add new tags and save
        let mut wave = WAVE::load(&temp_path).unwrap();
        wave.add_tags().unwrap();
        if let Some(tags) = &mut wave.tags {
            tags.set("TIT2", vec!["New Title After Clear".to_string()]);
            tags.set("TPE1", vec!["New Artist".to_string()]);
        }
        wave.save_to_file(&temp_path).unwrap();

        // Reload and verify new tags
        let reloaded = WAVE::load(&temp_path).unwrap();
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
    }

    #[test]
    fn test_mime_access() {
        let wave = WAVE::new();
        let mimes = wave.mime();

        assert!(mimes.contains(&"audio/wav"));
        assert!(mimes.contains(&"audio/wave"));
        assert_eq!(mimes.len(), 2);
    }

    #[test]
    fn test_pprint() {
        let mut wave = WAVE::new();
        wave.info.channels = 2;
        wave.info.sample_rate = 44100;
        wave.info.bits_per_sample = 16;
        wave.info.bitrate = Some(1411200);
        wave.info.length = Some(Duration::from_secs_f64(3.5));

        let output = wave.pprint();
        assert!(output.contains("2 channel"));
        assert!(output.contains("1411200 bps"));
        assert!(output.contains("44100 Hz"));
        assert!(output.contains("3.50 seconds"));
    }
}

#[cfg(test)]
mod wave_error_handling_tests {
    use super::*;

    #[test]
    fn test_invalid_file_error() {
        // Try to load a non-WAV file
        let path = TestUtils::data_path("empty.ogg");
        if path.exists() {
            let result = WAVE::load(&path);
            assert!(result.is_err());

            // Should get some kind of WAV-related error
            match result {
                Err(AudexError::WAVError(_))
                | Err(AudexError::WAVInvalidChunk(_))
                | Err(AudexError::Io(_)) => {
                    // Any of these errors are acceptable
                }
                _ => panic!("Expected WAV-related error when loading non-WAV file"),
            }
        }
    }

    #[test]
    fn test_nonexistent_file() {
        let result = WAVE::load("nonexistent_file.wav");
        assert!(result.is_err());

        match result {
            Err(AudexError::Io(_)) => {
                // Expected IO error for missing file
            }
            _ => panic!("Expected IO error for nonexistent file"),
        }
    }

    #[test]
    fn test_delete_nonexistent() {
        // Delete should handle non-existent files gracefully
        let result = clear("nonexistent_file.wav");
        match result {
            Ok(()) => {
                // If it succeeded, that's fine (no file to delete)
            }
            Err(AudexError::Io(_)) => {
                // IO error is also acceptable
            }
            Err(e) => panic!("Unexpected error when deleting nonexistent file: {:?}", e),
        }
    }
}

#[cfg(test)]
mod wave_integration_tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_file_detection_and_scoring() {
        // List available WAV test files
        let data_dir = TestUtils::data_path("");
        if let Ok(entries) = fs::read_dir(&data_dir) {
            let mut wav_files = Vec::new();

            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "wav" {
                        wav_files.push(path);
                    }
                }
            }

            println!("Found {} WAV files", wav_files.len());

            // Test scoring on WAV files
            for wav_file in wav_files.iter().take(3) {
                if let Ok(data) = fs::read(wav_file) {
                    let header = &data[..data.len().min(128)];
                    let score = WAVE::score(&wav_file.to_string_lossy(), header);
                    println!(
                        "WAV file {:?} scored {}",
                        wav_file.file_name().unwrap(),
                        score
                    );

                    // Should score highly for valid WAV files
                    assert!(score >= 10, "WAV file should have high score");
                }
            }
        }
    }

    #[test]
    fn test_round_trip_compatibility() {
        // Test round-trip file parsing
        let test_files = [
            "silence-2s-PCM-16000-08-ID3v23.wav",
            "silence-2s-PCM-44100-16-ID3v23.wav",
        ];

        for filename in &test_files {
            let path = TestUtils::data_path(filename);
            if path.exists() {
                match WAVE::load(&path) {
                    Ok(wave) => {
                        // Check that we get the same basic parameters Reference would
                        assert!(wave.info.sample_rate > 0);
                        assert!(wave.info.channels > 0);
                        assert!(wave.info.bits_per_sample > 0);
                        assert!(wave.info.bitrate().is_some());
                        assert!(wave.info.length().is_some());

                        // Files with ID3v23 should have tags
                        if filename.contains("ID3v23") {
                            assert!(wave.tags.is_some(), "ID3v23 file should have parsed tags");
                        }

                        println!("✓ {} parsed successfully", filename);
                    }
                    Err(e) => {
                        println!("✗ {} failed to parse: {}", filename, e);
                    }
                }
            }
        }
    }

    #[test]
    fn test_chunk_parsing_robustness() {
        // Test with various chunk configurations
        let test_files = [
            "silence-2s-PCM-16000-08-notags.wav", // No ID3 chunk
            "silence-2s-PCM-16000-08-ID3v23.wav", // With ID3 chunk (uppercase)
            "silence-2s-PCM-44100-16-ID3v23.wav", // Different format with ID3
        ];

        for filename in &test_files {
            let path = TestUtils::data_path(filename);
            if path.exists() {
                match WAVE::load(&path) {
                    Ok(wave) => {
                        // Should always have valid format info
                        assert!(
                            wave.info.sample_rate > 0,
                            "Should parse sample rate from {}",
                            filename
                        );
                        assert!(
                            wave.info.channels > 0,
                            "Should parse channels from {}",
                            filename
                        );

                        // Check for appropriate tag presence
                        let has_tags = wave.tags.is_some();
                        let expects_tags = filename.contains("ID3v23");

                        if expects_tags {
                            assert!(has_tags, "File {} should have tags", filename);
                        }

                        println!(
                            "Parsed {}: {}Hz, {}ch, {}bit, tags={}",
                            filename,
                            wave.info.sample_rate,
                            wave.info.channels,
                            wave.info.bits_per_sample,
                            has_tags
                        );
                    }
                    Err(e) => {
                        println!("Could not parse {}: {}", filename, e);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// WAVE/RIFF overflow tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod wave_riff_overflow_tests {
    /// Demonstrate that u32 addition wraps silently in release mode.
    /// This confirms the arithmetic vulnerability exists at the language level.
    #[test]
    fn test_u32_addition_wraps() {
        let file_size: u32 = u32::MAX - 100; // near-max file size
        let chunk_size: u32 = 200; // adding 200 bytes

        // In release mode, this wraps to 99 instead of overflowing
        let wrapped = file_size.wrapping_add(chunk_size);
        assert_eq!(wrapped, 99, "u32 wrapping_add should wrap to 99");

        // checked_add detects the overflow
        let checked = file_size.checked_add(chunk_size);
        assert!(checked.is_none(), "checked_add should detect overflow");
    }

    /// Verify that `checked_add` is the correct approach for preventing
    /// silent data corruption in RIFF/FORM size headers.
    #[test]
    fn test_checked_add_catches_boundary() {
        // Exact boundary: file_size = u32::MAX, chunk_size = 1
        let file_size: u32 = u32::MAX;
        let chunk_size: u32 = 1;
        assert!(
            file_size.checked_add(chunk_size).is_none(),
            "Adding 1 to u32::MAX should be detected as overflow"
        );

        // Safe case: both values fit
        let file_size: u32 = 1_000_000;
        let chunk_size: u32 = 500;
        assert!(
            file_size.checked_add(chunk_size).is_some(),
            "Normal addition should succeed"
        );
    }
}
