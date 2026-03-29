//! Tests for DSDIFF (Direct Stream Digital Interchange File Format) support

use audex::FileType;
use audex::dsdiff::DSDIFF;
use std::path::Path;

/// Basic DSDIFF functionality tests
#[cfg(test)]
mod dsdiff_basic_tests {
    use super::*;

    #[test]
    fn test_dsdiff_creation() {
        let dsdiff = DSDIFF::new();
        assert!(dsdiff.tags.is_none());
        assert!(dsdiff.filename.is_none());
    }

    #[test]
    fn test_dsdiff_mime_types() {
        let mime_types = DSDIFF::mime_types();
        assert_eq!(mime_types.len(), 1);
        assert!(mime_types.contains(&"audio/x-dff"));
    }

    #[test]
    fn test_score_with_frm8_header() {
        let header = b"FRM8";
        let score = DSDIFF::score("test.dff", header);
        assert_eq!(score, 3); // Header + extension
    }

    #[test]
    fn test_score_without_extension() {
        let header = b"FRM8";
        let score = DSDIFF::score("test.wav", header);
        assert_eq!(score, 2); // Header only
    }

    #[test]
    fn test_score_with_extension_only() {
        let header = b"RIFF";
        let score = DSDIFF::score("test.dff", header);
        assert_eq!(score, 1); // Extension only
    }
}

/// Integration tests with real files
#[cfg(test)]
mod dsdiff_integration_tests {
    use super::*;

    #[test]
    fn test_load_silence_1ch_0s() {
        let test_file = "tests/data/2822400-1ch-0s-silence.dff";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let dsdiff = DSDIFF::load(test_file).unwrap();

        // Expected values
        assert_eq!(dsdiff.info.channels, 1);
        assert_eq!(dsdiff.info.sample_rate, 2822400);
        assert_eq!(dsdiff.info.bits_per_sample, 1);
        assert_eq!(dsdiff.info.compression, "DSD");

        // Length should be 0 seconds
        let length = dsdiff.info.length.unwrap().as_secs_f64();
        assert!(length.abs() < 0.001, "Expected ~0s, got {:.3}s", length);

        // Bitrate should be sample_rate * channels * bits_per_sample
        assert_eq!(dsdiff.info.bitrate.unwrap(), 2822400);
    }

    #[test]
    fn test_load_silence_2ch_01s() {
        let test_file = "tests/data/5644800-2ch-s01-silence.dff";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let dsdiff = DSDIFF::load(test_file).unwrap();

        // Expected values
        assert_eq!(dsdiff.info.channels, 2);
        assert_eq!(dsdiff.info.sample_rate, 5644800);
        assert_eq!(dsdiff.info.bits_per_sample, 1);
        assert_eq!(dsdiff.info.compression, "DSD");

        // Length should be 0.01 seconds
        let length = dsdiff.info.length.unwrap().as_secs_f64();
        assert!(
            (length - 0.01).abs() < 0.001,
            "Expected ~0.01s, got {:.3}s",
            length
        );

        // Bitrate should be sample_rate * channels * bits_per_sample
        assert_eq!(dsdiff.info.bitrate.unwrap(), 11289600); // 5644800 * 2 * 1
    }

    #[test]
    fn test_load_silence_dst_compression() {
        let test_file = "tests/data/5644800-2ch-s01-silence-dst.dff";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let dsdiff = DSDIFF::load(test_file).unwrap();

        // Expected values
        assert_eq!(dsdiff.info.channels, 2);
        assert_eq!(dsdiff.info.sample_rate, 5644800);
        assert_eq!(dsdiff.info.bits_per_sample, 1);
        // Note: DST compression type may be detected

        // Length should be 0 seconds for this test file (may be None for DST)
        if let Some(length) = dsdiff.info.length {
            let length_secs = length.as_secs_f64();
            assert!(
                length_secs.abs() < 0.001,
                "Expected ~0s, got {:.3}s",
                length_secs
            );
        }

        // For DST compression, bitrate may be 0 or calculated differently
        if let Some(bitrate) = dsdiff.info.bitrate {
            assert_eq!(bitrate, 0);
        }
    }

    #[test]
    fn test_pprint() {
        let test_file = "tests/data/2822400-1ch-0s-silence.dff";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let dsdiff = DSDIFF::load(test_file).unwrap();
        let pprint_str = dsdiff.pprint();

        // Should contain key information
        assert!(pprint_str.contains("DSDIFF"));
        assert!(pprint_str.contains("DSD"));
        assert!(pprint_str.contains("1 channel"));
        assert!(pprint_str.contains("2822400"));
    }

    #[test]
    fn test_mime() {
        let test_file = "tests/data/2822400-1ch-0s-silence.dff";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let dsdiff = DSDIFF::load(test_file).unwrap();
        let mime_types = dsdiff.mime();

        assert!(mime_types.contains(&"audio/x-dff"));
    }
}

/// DSDIFF tag save/update/delete tests
#[cfg(test)]
mod dsdiff_tag_operations_tests {
    use super::*;
    use audex::Tags;

    #[test]
    fn test_add_tags_in_memory() {
        // Test adding tags to a DSDIFF file in memory
        let test_file = "tests/data/5644800-2ch-s01-silence.dff";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        // Load file and add tags
        let mut dsdiff = DSDIFF::load(test_file).unwrap();

        // Initially should have no tags
        assert!(dsdiff.tags.is_none());

        // Add tags
        dsdiff.add_tags().unwrap();
        assert!(dsdiff.tags.is_some());

        // Set a tag in memory
        if let Some(tags) = &mut dsdiff.tags {
            tags.set("TIT1", vec!["foobar".to_string()]);

            // Verify tag is set in memory
            if let Some(tit1) = tags.get("TIT1") {
                assert!(tit1[0].contains("foobar"), "Tag should be set in memory");
            }
        }
    }

    #[test]
    fn test_delete_tags_in_memory() {
        // Test deleting tags from DSDIFF in memory
        let test_file = "tests/data/5644800-2ch-s01-silence.dff";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        // Load file and add tags
        let mut dsdiff = DSDIFF::load(test_file).unwrap();
        dsdiff.add_tags().unwrap();
        assert!(dsdiff.tags.is_some());

        // Delete tags
        dsdiff.clear().unwrap();
        assert!(dsdiff.tags.is_none(), "Tags should be deleted");
    }

    #[test]
    fn test_tag_operations() {
        // Test various tag operations on DSDIFF in memory
        let test_file = "tests/data/5644800-2ch-s01-silence.dff";
        if !Path::new(test_file).exists() {
            return; // Skip if test file doesn't exist
        }

        let mut dsdiff = DSDIFF::load(test_file).unwrap();

        // Add tags if not present
        if dsdiff.tags.is_none() {
            dsdiff.add_tags().unwrap();
        }

        if let Some(tags) = &mut dsdiff.tags {
            // Set multiple tags
            tags.set("TIT1", vec!["Title 1".to_string()]);
            tags.set("TIT2", vec!["Title 2".to_string()]);
            tags.set("TPE1", vec!["Artist".to_string()]);

            // Verify tags are set
            assert!(tags.get("TIT1").is_some());
            assert!(tags.get("TIT2").is_some());
            assert!(tags.get("TPE1").is_some());

            // Verify tag values
            if let Some(tit1) = tags.get("TIT1") {
                assert_eq!(tit1[0], "Title 1");
            }
        }
    }

    #[test]
    fn test_error_on_wrong_format() {
        // Test that loading a non-DSDIFF file produces an error
        let dsf_file = "tests/data/2822400-1ch-0s-silence.dsf";
        if !Path::new(dsf_file).exists() {
            return; // Skip if test file doesn't exist
        }

        // Should error when trying to load DSF file as DSDIFF
        let result = DSDIFF::load(dsf_file);
        assert!(
            result.is_err(),
            "Loading DSF file as DSDIFF should produce error"
        );
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use audex::dsdiff::IffChunk;
    use std::io::Cursor;

    #[test]
    fn test_chunk_with_large_valid_size() {
        let mut data = Vec::new();
        data.extend_from_slice(b"PROP");
        let size: u64 = 1_000_000_000;
        data.extend_from_slice(&size.to_be_bytes());
        data.extend_from_slice(&[0u8; 64]);

        let mut cursor = Cursor::new(data);
        let chunk = IffChunk::from_reader(&mut cursor).unwrap();

        assert_eq!(&chunk.id, b"PROP");
        assert_eq!(chunk.size, 1_000_000_000);

        let result = chunk.read_data(&mut cursor);
        assert!(result.is_err(), "Should fail on insufficient data");
    }

    #[test]
    fn test_chunk_with_i64_max_rejected() {
        let mut data = Vec::new();
        data.extend_from_slice(b"PROP");
        let size: u64 = (i64::MAX as u64) + 1;
        data.extend_from_slice(&size.to_be_bytes());
        data.extend_from_slice(&[0u8; 64]);

        let mut cursor = Cursor::new(data);
        let result = IffChunk::from_reader(&mut cursor);

        assert!(result.is_err(), "Sizes > i64::MAX should be rejected");
    }

    #[test]
    fn test_dsdiff_chunk_read_rejects_oversized_data() {
        let mut data = Vec::new();
        data.extend_from_slice(b"PROP");
        let huge_size: u64 = 512 * 1024 * 1024;
        data.extend_from_slice(&huge_size.to_be_bytes());
        data.extend_from_slice(&[0u8; 64]);

        let mut cursor = Cursor::new(data);
        let chunk = IffChunk::from_reader(&mut cursor).unwrap();

        let result = chunk.read_data(&mut cursor);

        assert!(result.is_err(), "Should reject chunk above size limit");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("limit"),
            "Error should mention size limit, got: {}",
            msg
        );
    }

    #[test]
    fn test_dsdiff_chunk_read_accepts_normal_size() {
        let mut data = Vec::new();
        data.extend_from_slice(b"PROP");
        data.extend_from_slice(&16u64.to_be_bytes());
        data.extend_from_slice(&[0u8; 16]);

        let mut cursor = Cursor::new(data);
        let chunk = IffChunk::from_reader(&mut cursor).unwrap();

        let result = chunk.read_data(&mut cursor);
        assert!(result.is_ok(), "Normal-sized DSDIFF chunk should read fine");
    }
}

// ---------------------------------------------------------------------------
// DSDIFF chunk size overflow tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod dsdiff_size_overflow_tests {
    use audex::dsdiff::DSDIFFFile;
    use std::io::Cursor;

    /// Build a minimal DSDIFF file header with a chunk whose size
    /// exceeds i64::MAX. The parser should reject this rather than
    /// performing a backward seek.
    #[test]
    fn test_chunk_size_exceeding_i64_max_does_not_backward_seek() {
        let mut data = Vec::new();

        // FRM8 container header
        data.extend_from_slice(b"FRM8");
        // Container size (8 bytes, big-endian) — large but valid-looking
        data.extend_from_slice(&100u64.to_be_bytes());
        // Form type: DSD
        data.extend_from_slice(b"DSD ");

        // A property chunk with a size that overflows i64
        data.extend_from_slice(b"PROP"); // chunk ID
        // Size = 0x8000_0000_0000_0000 (i64::MAX + 1) — wraps to negative as i64
        let overflow_size: u64 = (i64::MAX as u64) + 1;
        data.extend_from_slice(&overflow_size.to_be_bytes());

        // Pad with some bytes so the cursor has something to work with
        data.extend_from_slice(&[0u8; 64]);

        let mut cursor = Cursor::new(data);
        let result = DSDIFFFile::parse(&mut cursor);

        // Should return an error, not seek backwards or panic
        assert!(
            result.is_err(),
            "Should reject chunk size > i64::MAX instead of backward seeking"
        );
    }

    /// A chunk with size = u64::MAX should also be rejected.
    #[test]
    fn test_chunk_size_u64_max_rejected() {
        let mut data = Vec::new();

        // FRM8 container header
        data.extend_from_slice(b"FRM8");
        data.extend_from_slice(&200u64.to_be_bytes());
        data.extend_from_slice(b"DSD ");

        // Unknown chunk with u64::MAX size
        data.extend_from_slice(b"XXXX");
        data.extend_from_slice(&u64::MAX.to_be_bytes());

        data.extend_from_slice(&[0u8; 64]);

        let mut cursor = Cursor::new(data);
        let result = DSDIFFFile::parse(&mut cursor);

        // Should error, not wrap to a negative seek
        assert!(result.is_err(), "Should reject chunk with u64::MAX size");
    }

    /// Normal small chunks must still parse correctly.
    #[test]
    fn test_normal_small_chunks_still_work() {
        // A minimal valid DSDIFF won't fully parse from synthetic data,
        // but we verify it doesn't panic on small well-formed chunks
        let mut data = Vec::new();

        data.extend_from_slice(b"FRM8");
        data.extend_from_slice(&28u64.to_be_bytes()); // container size
        data.extend_from_slice(b"DSD ");

        // A small unknown chunk (size=4, 4 bytes of data)
        data.extend_from_slice(b"TEST");
        data.extend_from_slice(&4u64.to_be_bytes());
        data.extend_from_slice(&[0xAB; 4]);

        let mut cursor = Cursor::new(data);
        let result = DSDIFFFile::parse(&mut cursor);
        // May fail for other reasons (incomplete file), but must not panic
        let _ = result;
    }
}

// ---------------------------------------------------------------------------
// DSDIFF zero-size chunk tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod dsdiff_zero_chunk_tests {
    use audex::dsdiff::DSDIFFFile;
    use std::io::Cursor;
    use std::time::{Duration, Instant};

    /// Build a minimal DSDIFF container packed with zero-size chunks.
    /// If the scanner doesn't reject zero-size chunks, it reads 12 bytes
    /// per iteration — very slow for large containers.
    #[test]
    fn test_many_zero_size_chunks_terminates_quickly() {
        let num_chunks = 1000u32;
        let chunk_header_size = 12u64; // 4-byte ID + 8-byte size
        let content_size = (num_chunks as u64) * chunk_header_size;

        let mut data = Vec::new();

        // FRM8 container header
        data.extend_from_slice(b"FRM8");
        data.extend_from_slice(&(content_size + 4).to_be_bytes()); // +4 for form type
        data.extend_from_slice(b"DSD ");

        // Fill with zero-size chunks
        for _ in 0..num_chunks {
            data.extend_from_slice(b"ZZZZ"); // chunk ID
            data.extend_from_slice(&0u64.to_be_bytes()); // size = 0
        }

        let mut cursor = Cursor::new(data);
        let start = Instant::now();
        let result = DSDIFFFile::parse(&mut cursor);
        let elapsed = start.elapsed();

        // Must complete quickly — not stall on zero-size chunks
        assert!(
            elapsed < Duration::from_secs(5),
            "Parsing took {:?} — likely stalled on zero-size chunks",
            elapsed
        );

        // May fail for other reasons, but must not hang
        let _ = result;
    }

    /// A single zero-size chunk should be handled without issues.
    #[test]
    fn test_single_zero_size_chunk_handled() {
        let mut data = Vec::new();

        // FRM8 container
        data.extend_from_slice(b"FRM8");
        data.extend_from_slice(&16u64.to_be_bytes()); // content size
        data.extend_from_slice(b"DSD ");

        // One zero-size chunk
        data.extend_from_slice(b"XXXX");
        data.extend_from_slice(&0u64.to_be_bytes());

        let mut cursor = Cursor::new(data);
        let result = DSDIFFFile::parse(&mut cursor);

        // Must not hang — error is acceptable
        let _ = result;
    }
}
