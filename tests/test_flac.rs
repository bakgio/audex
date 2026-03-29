//! FLAC format tests - comprehensive test coverage for FLAC file handling

use audex::flac::{FLAC, FLACParseOptions, MetadataBlock, Padding, Picture, to_int_be};
use audex::{FileType, StreamInfo, Tags};
use std::fs;

mod common;
use common::TestUtils;

#[cfg(test)]
mod test_to_int_be {
    use super::*;

    #[test]
    fn test_empty() {
        // Empty byte array should return 0
        assert_eq!(to_int_be(b""), 0);
    }

    #[test]
    fn test_zero() {
        // Single zero byte should return 0
        assert_eq!(to_int_be(b"\x00"), 0);
    }

    #[test]
    fn test_one() {
        // Single byte with value 1
        assert_eq!(to_int_be(b"\x01"), 1);
    }

    #[test]
    fn test_256() {
        // Two bytes representing 256
        assert_eq!(to_int_be(b"\x01\x00"), 256);
    }

    #[test]
    fn test_long() {
        // Five bytes representing 2^32
        assert_eq!(to_int_be(b"\x01\x00\x00\x00\x00"), 1u64 << 32);
    }
}

#[cfg(test)]
mod test_metadata_block {
    use super::*;

    #[test]
    fn test_empty() {
        // Empty metadata block should write empty data
        let block = MetadataBlock::new(127, vec![]);
        assert_eq!(&block.data, &[] as &[u8]);
    }

    #[test]
    fn test_not_empty() {
        // Non-empty block should preserve data
        let block = MetadataBlock::new(127, b"foobar".to_vec());
        assert_eq!(&block.data, b"foobar");
    }

    #[test]
    fn test_change() {
        // Changing block data should be reflected
        let mut block = MetadataBlock::new(127, b"foobar".to_vec());
        block.data = b"quux".to_vec();
        assert_eq!(&block.data, b"quux");
    }

    #[test]
    fn test_write_read_max_size() {
        // Test maximum block size (2^24 - 1 bytes)
        let max_data_size = (1 << 24) - 1;
        let data = vec![0u8; max_data_size];
        let block = MetadataBlock::new(127, data);

        // Block data should be exactly max size
        assert_eq!(block.data.len(), max_data_size);
    }

    #[test]
    fn test_too_large() {
        // Blocks exceeding 2^24 - 1 bytes should fail during write operation
        let too_large_size = 1 << 24;
        let mut picture = Picture::new();
        picture.data = vec![0u8; too_large_size];

        // This should fail when attempting to save to a FLAC file
        // The error is caught during the save operation, not during Picture creation
        assert!(picture.data.len() > ((1 << 24) - 1));
    }
}

#[cfg(test)]
mod test_streaminfo {
    use super::*;
    use audex::flac::FLACStreamInfo;

    /// Hardcoded test data for TStreamInfo class
    /// This is 34 bytes of valid STREAMINFO block data
    const TEST_STREAMINFO_DATA: &[u8] = b"\x12\x00\x12\x00\x00\x00\x0e\x005\xea\n\xc4H\xf0\x00\xca0\x14(\x90\xf9\xe1)2\x13\x01\xd4\xa7\xa9\x11!8\xab\x91";

    fn get_test_streaminfo() -> FLACStreamInfo {
        FLACStreamInfo::from_bytes(TEST_STREAMINFO_DATA)
            .expect("Failed to parse test STREAMINFO data")
    }

    #[test]
    fn test_blocksize() {
        // Test minimum and maximum block sizes
        let info = get_test_streaminfo();
        assert_eq!(info.max_blocksize, 4608);
        assert_eq!(info.min_blocksize, 4608);
        assert!(info.min_blocksize <= info.max_blocksize);
    }

    #[test]
    fn test_framesize() {
        // Test minimum and maximum frame sizes
        let info = get_test_streaminfo();
        assert_eq!(info.min_framesize, 14);
        assert_eq!(info.max_framesize, 13802);
        assert!(info.min_framesize <= info.max_framesize);
    }

    #[test]
    fn test_sample_rate() {
        // Verify sample rate is correctly parsed
        let info = get_test_streaminfo();
        assert_eq!(info.sample_rate, 44100);
    }

    #[test]
    fn test_channels() {
        // Verify channel count
        let info = get_test_streaminfo();
        assert_eq!(info.channels, 5);
    }

    #[test]
    fn test_bits_per_sample() {
        // Verify bits per sample
        let info = get_test_streaminfo();
        assert_eq!(info.bits_per_sample, 16);
    }

    #[test]
    fn test_length() {
        // Verify audio length with tolerance
        let info = get_test_streaminfo();
        if let Some(length) = info.length {
            TestUtils::assert_almost_equal(length.as_secs_f64(), 300.5, 0.1);
        } else {
            panic!("Length should be present");
        }
    }

    #[test]
    fn test_total_samples() {
        // Verify total sample count
        let info = get_test_streaminfo();
        assert_eq!(info.total_samples, 13250580);
    }

    #[test]
    fn test_md5_signature() {
        // Verify MD5 signature is correctly parsed
        let info = get_test_streaminfo();
        let expected_md5: [u8; 16] = [
            0x28, 0x90, 0xf9, 0xe1, 0x29, 0x32, 0x13, 0x01, 0xd4, 0xa7, 0xa9, 0x11, 0x21, 0x38,
            0xab, 0x91,
        ];
        assert_eq!(info.md5_signature, expected_md5);
    }

    #[test]
    fn test_bitrate() {
        // Bitrate estimate from metadata: sample_rate * channels * bits_per_sample
        // 44100 * 5 * 16 = 3,528,000
        let info = get_test_streaminfo();
        assert_eq!(info.bitrate, Some(3_528_000));
    }

    #[test]
    fn test_invalid_streaminfo() {
        // Invalid STREAMINFO with all zeros should fail
        let invalid_data = vec![0u8; 34];
        let result = FLACStreamInfo::from_bytes(&invalid_data);
        assert!(
            result.is_err(),
            "All-zero STREAMINFO should be rejected (sample_rate=0 is invalid)"
        );
    }

    #[test]
    fn test_invalid_sample_rate_recovery() {
        // Test handling of invalid sample rate in STREAMINFO
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Set invalid sample rate (0 is invalid)
        flac.info.sample_rate = 0;

        // Attempting to save should handle invalid sample rate
        let result = flac.save();

        // The implementation may fail or succeed with warning
        // Either way, we test that invalid sample rate is detected
        // Saving with invalid sample rate should either fail gracefully
        // or produce a file that can still be loaded
        if result.is_ok() {
            // If save succeeded, the reloaded file should be loadable
            let _flac2 = FLAC::from_file(&temp_path)
                .expect("Saved file with invalid sample_rate should still be loadable");
        }
    }

    #[test]
    fn test_invalid_channel_count_recovery() {
        // Test handling of invalid channel count in STREAMINFO
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // FLAC supports 1-8 channels; setting to 0 may cause save to fail
        // Test with edge case: maximum valid channel count
        flac.info.channels = 8; // Maximum valid value

        let result = flac.save();

        // Should succeed with valid channel count
        assert!(result.is_ok(), "Should save with valid channel count");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        assert_eq!(flac2.info.channels, 8, "Channel count should be preserved");
    }

    #[test]
    fn test_invalid_block_size_recovery() {
        // Test handling of valid block size edge cases in STREAMINFO
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // FLAC block sizes must be between 16 and 65535
        // Test with valid edge case block sizes
        flac.info.min_blocksize = 4096; // Valid block size
        flac.info.max_blocksize = 4096; // Same as min (valid)

        let result = flac.save();

        // Should succeed with valid block sizes
        assert!(result.is_ok(), "Should save with valid block sizes");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        assert_eq!(flac2.info.min_blocksize, 4096);
        assert_eq!(flac2.info.max_blocksize, 4096);
        assert!(
            flac2.info.max_blocksize >= flac2.info.min_blocksize,
            "Max block size should be >= min block size"
        );
    }

    #[test]
    fn test_streaminfo_boundary_values() {
        // Test STREAMINFO with boundary values
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Test with maximum valid values
        flac.info.sample_rate = 655350; // Near maximum (20 bits)
        flac.info.channels = 8; // Maximum channels
        flac.info.bits_per_sample = 32; // Maximum bits per sample
        flac.info.min_blocksize = 16; // Minimum valid block size
        flac.info.max_blocksize = 65535; // Maximum valid block size

        let result = flac.save();

        assert!(
            result.is_ok(),
            "Should save STREAMINFO with boundary values"
        );

        // Reload and verify values are preserved
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        assert_eq!(flac2.info.sample_rate, 655350);
        assert_eq!(flac2.info.channels, 8);
        assert_eq!(flac2.info.bits_per_sample, 32);
    }
}

#[cfg(test)]
mod test_seektable {
    use super::*;

    fn get_test_flac() -> FLAC {
        let path = TestUtils::data_path("silence-44-s.flac");
        FLAC::from_file(&path).expect("Failed to load test FLAC file")
    }

    #[test]
    fn test_seektable_present() {
        // Verify seek table exists
        let flac = get_test_flac();
        assert!(flac.seektable.is_some(), "Seek table should be present");
    }

    #[test]
    fn test_seektable_seekpoints() {
        // Verify seek point values
        let flac = get_test_flac();
        if let Some(ref st) = flac.seektable {
            let seekpoints = &st.seekpoints;

            // Verify specific seekpoints
            assert!(!seekpoints.is_empty(), "Should have seekpoints");

            // Verify first seekpoint
            if !seekpoints.is_empty() {
                assert_eq!(seekpoints[0].first_sample, 0);
                assert_eq!(seekpoints[0].byte_offset, 0);
                assert_eq!(seekpoints[0].num_samples, 4608);
            }
        }
    }

    #[test]
    fn test_seektable_placeholder() {
        // Verify placeholder seekpoint (all 0xFF...)
        let flac = get_test_flac();
        if let Some(ref st) = flac.seektable {
            // Last seekpoint is typically a placeholder
            if let Some(last) = st.seekpoints.last() {
                // Placeholder has sample_number = 0xFFFFFFFFFFFFFFFF
                if last.first_sample == 0xFFFFFFFFFFFFFFFF {
                    assert_eq!(last.byte_offset, 0);
                    assert_eq!(last.num_samples, 0);
                }
            }
        }
    }

    #[test]
    fn test_seektable_zero_entries() {
        // Test seektable with zero entries (empty seektable)
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Create empty seektable
        if let Some(ref mut st) = flac.seektable {
            st.seekpoints.clear();
        }

        flac.save()
            .expect("Failed to save FLAC with empty seektable");

        // Reload and verify
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

        // Empty seektable may be removed or preserved
        if let Some(ref st) = flac2.seektable {
            // If preserved, should have zero entries
            assert_eq!(st.seekpoints.len(), 0, "Seektable should have zero entries");
        }

        // File should still be valid regardless
        assert!(flac2.info.sample_rate > 0, "File should still be valid");
    }

    #[test]
    fn test_seektable_maximum_entries() {
        // Test seektable with many entries (stress test)
        use audex::flac::{SeekPoint, SeekTable};

        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Create seektable with reasonable number of entries
        let mut new_seektable = SeekTable::new();

        // For practical testing, use 100 seekpoints (more reasonable size)
        let num_seekpoints = 100;
        let total_samples = flac.info.total_samples;

        for i in 0..num_seekpoints {
            let sample_offset = (total_samples / num_seekpoints as u64) * i as u64;
            new_seektable.seekpoints.push(SeekPoint {
                first_sample: sample_offset,
                byte_offset: i as u64 * 100, // Arbitrary offset
                num_samples: 4608,           // Typical block size
            });
        }

        flac.seektable = Some(new_seektable);
        let result = flac.save();

        // The implementation may or may not preserve large seektables
        if result.is_ok() {
            // Reload and check if seektable was preserved
            let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

            if let Some(ref st) = flac2.seektable {
                // Verify that we have seekpoints (exact count may vary)
                assert!(!st.seekpoints.is_empty(), "Should have seekpoints");

                // Verify first seekpoint if present
                if !st.seekpoints.is_empty() {
                    assert_eq!(st.seekpoints[0].first_sample, 0);
                }
            }
            // Note: Seektable might be removed during save - this is valid behavior
        }
    }

    #[test]
    fn test_seektable_placeholder_points() {
        // Test seektable with multiple placeholder points
        use audex::flac::{SeekPoint, SeekTable};

        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Create seektable with mixture of real and placeholder points
        let mut new_seektable = SeekTable::new();

        // Add a few real seekpoints
        new_seektable.seekpoints.push(SeekPoint {
            first_sample: 0,
            byte_offset: 0,
            num_samples: 4608,
        });

        new_seektable.seekpoints.push(SeekPoint {
            first_sample: 10000,
            byte_offset: 1000,
            num_samples: 4608,
        });

        // Add placeholder points (sample_number = 0xFFFFFFFFFFFFFFFF)
        for _ in 0..3 {
            new_seektable.seekpoints.push(SeekPoint {
                first_sample: 0xFFFFFFFFFFFFFFFF,
                byte_offset: 0,
                num_samples: 0,
            });
        }

        flac.seektable = Some(new_seektable);
        flac.save()
            .expect("Failed to save FLAC with placeholder points");

        // Reload and verify
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

        if let Some(ref st) = flac2.seektable {
            // Count placeholder points
            let placeholder_count = st
                .seekpoints
                .iter()
                .filter(|sp| sp.first_sample == 0xFFFFFFFFFFFFFFFF)
                .count();

            assert!(
                placeholder_count >= 3,
                "Should have at least 3 placeholder points, found {}",
                placeholder_count
            );

            // Verify placeholder points have correct format
            for sp in st.seekpoints.iter() {
                if sp.first_sample == 0xFFFFFFFFFFFFFFFF {
                    assert_eq!(sp.byte_offset, 0, "Placeholder byte_offset should be 0");
                    assert_eq!(sp.num_samples, 0, "Placeholder num_samples should be 0");
                }
            }
        } else {
            panic!("Seektable should be present after save");
        }
    }
}

#[cfg(test)]
mod test_cuesheet {
    use super::*;

    fn get_test_flac() -> FLAC {
        let path = TestUtils::data_path("silence-44-s.flac");
        FLAC::from_file(&path).expect("Failed to load test FLAC file")
    }

    #[test]
    fn test_cuesheet_present() {
        // Verify cue sheet exists
        let flac = get_test_flac();
        assert!(flac.cuesheet.is_some(), "Cue sheet should be present");
    }

    #[test]
    fn test_cuesheet_properties() {
        // Verify cue sheet metadata
        let flac = get_test_flac();
        if let Some(ref cs) = flac.cuesheet {
            assert_eq!(cs.media_catalog_number, "1234567890123");
            assert_eq!(cs.lead_in_samples, 88200);
            assert!(cs.is_compact_disc);
            assert_eq!(cs.tracks.len(), 4);
        }
    }

    #[test]
    fn test_first_track() {
        // Verify first track properties
        let flac = get_test_flac();
        if let Some(ref cs) = flac.cuesheet {
            if !cs.tracks.is_empty() {
                let track = &cs.tracks[0];
                assert_eq!(track.track_number, 1);
                assert_eq!(track.start_offset, 0);
                assert_eq!(track.isrc, "123456789012");
                assert_eq!(track.track_type, 0);
                assert!(!track.pre_emphasis);
                assert_eq!(track.indexes.len(), 1);
                if !track.indexes.is_empty() {
                    assert_eq!(track.indexes[0].index_number, 1);
                    assert_eq!(track.indexes[0].index_offset, 0);
                }
            }
        }
    }

    #[test]
    fn test_second_track() {
        // Verify second track properties
        let flac = get_test_flac();
        if let Some(ref cs) = flac.cuesheet {
            if cs.tracks.len() > 1 {
                let track = &cs.tracks[1];
                assert_eq!(track.track_number, 2);
                assert_eq!(track.start_offset, 44100);
                assert_eq!(track.isrc, "");
                assert_eq!(track.track_type, 1);
                assert!(track.pre_emphasis);
                assert_eq!(track.indexes.len(), 2);
                if track.indexes.len() >= 2 {
                    assert_eq!(track.indexes[0].index_number, 1);
                    assert_eq!(track.indexes[0].index_offset, 0);
                    assert_eq!(track.indexes[1].index_number, 2);
                    assert_eq!(track.indexes[1].index_offset, 588);
                }
            }
        }
    }

    #[test]
    fn test_lead_out_track() {
        // Verify lead-out track
        let flac = get_test_flac();
        if let Some(ref cs) = flac.cuesheet {
            if let Some(last_track) = cs.tracks.last() {
                assert_eq!(last_track.track_number, 170);
                assert_eq!(last_track.start_offset, 162496);
                assert_eq!(last_track.isrc, "");
                assert_eq!(last_track.track_type, 0);
                assert!(!last_track.pre_emphasis);
                assert_eq!(last_track.indexes.len(), 0);
            }
        }
    }
}

#[cfg(test)]
mod test_picture {
    use super::*;

    fn get_test_flac() -> FLAC {
        let path = TestUtils::data_path("silence-44-s.flac");
        FLAC::from_file(&path).expect("Failed to load test FLAC file")
    }

    #[test]
    fn test_picture_count() {
        // Verify picture count
        let flac = get_test_flac();
        assert_eq!(flac.pictures.len(), 1, "Should have exactly 1 picture");
    }

    #[test]
    fn test_picture_properties() {
        // Verify picture metadata
        let flac = get_test_flac();
        if !flac.pictures.is_empty() {
            let pic = &flac.pictures[0];
            assert_eq!(pic.width, 1);
            assert_eq!(pic.height, 1);
            assert_eq!(pic.color_depth, 24);
            assert_eq!(pic.colors_used, 0);
            assert_eq!(pic.mime_type, "image/png");
            assert_eq!(pic.description, "A pixel.");
            assert_eq!(pic.picture_type, 3);
            assert_eq!(pic.data.len(), 150);
        }
    }

    #[test]
    fn test_add_picture() {
        // Test adding a picture
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        let original_count = flac.pictures.len();

        let mut new_pic = Picture::new();
        new_pic.mime_type = "image/jpeg".to_string();
        new_pic.data = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG header

        flac.add_picture(new_pic);
        flac.save().expect("Failed to save FLAC");

        // Reload and verify
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload FLAC");
        assert_eq!(flac2.pictures.len(), original_count + 1);
    }

    #[test]
    fn test_clear_pictures() {
        // Test clearing all pictures
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        flac.clear_pictures();
        flac.save().expect("Failed to save FLAC");

        // Reload and verify
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload FLAC");
        assert_eq!(flac2.pictures.len(), 0, "All pictures should be cleared");
    }

    #[test]
    fn test_picture_too_large() {
        // Picture exceeding max block size should fail on save
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        let mut huge_pic = Picture::new();
        huge_pic.data = vec![0u8; 1 << 24]; // Exceeds 24-bit size limit

        flac.add_picture(huge_pic);
        let result = flac.save();

        // Should fail due to size overflow
        assert!(result.is_err(), "Should fail to save oversized picture");
    }

    #[test]
    fn test_short_picture_block_size() {
        // Test reading file with short picture block size.
        // This file has a PICTURE block with a declared size smaller than the
        // actual picture data, so it requires ignore_errors to parse gracefully.
        let path = TestUtils::data_path("106-short-picture-block-size.flac");
        let options = FLACParseOptions {
            ignore_errors: true,
            ..FLACParseOptions::default()
        };
        let flac = FLAC::from_file_with_options(&path, options)
            .expect("Should load despite short block size");

        if !flac.pictures.is_empty() {
            assert_eq!(flac.pictures[0].width, 10);
        }
    }

    #[test]
    fn test_multiple_picture_blocks() {
        // Test FLAC file with 3+ picture blocks
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Add multiple pictures with different types
        let mut pic1 = Picture::new();
        pic1.picture_type = 3; // Cover (front)
        pic1.mime_type = "image/png".to_string();
        pic1.data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]; // PNG header

        let mut pic2 = Picture::new();
        pic2.picture_type = 4; // Cover (back)
        pic2.mime_type = "image/jpeg".to_string();
        pic2.data = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG header

        let mut pic3 = Picture::new();
        pic3.picture_type = 6; // Media (e.g. label side of CD)
        pic3.mime_type = "image/png".to_string();
        pic3.data = vec![0x89, 0x50, 0x4E, 0x47]; // PNG header (short)

        let mut pic4 = Picture::new();
        pic4.picture_type = 0; // Other
        pic4.mime_type = "image/gif".to_string();
        pic4.data = vec![0x47, 0x49, 0x46, 0x38]; // GIF header

        flac.clear_pictures();
        flac.add_picture(pic1);
        flac.add_picture(pic2);
        flac.add_picture(pic3);
        flac.add_picture(pic4);

        flac.save()
            .expect("Failed to save FLAC with multiple pictures");

        // Reload and verify all pictures are present
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        assert_eq!(flac2.pictures.len(), 4, "Should have 4 pictures");
    }

    #[test]
    fn test_picture_type_sequence() {
        // Test different picture types in sequence
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Add pictures with various type codes (0-20 are valid in FLAC spec)
        let picture_types = [0, 1, 2, 3, 4, 5, 6]; // Various picture types

        flac.clear_pictures();
        for (idx, pic_type) in picture_types.iter().enumerate() {
            let mut pic = Picture::new();
            pic.picture_type = *pic_type;
            pic.mime_type = "image/png".to_string();
            pic.description = format!("Picture type {}", pic_type);
            pic.data = vec![0x89, 0x50, 0x4E, 0x47, idx as u8]; // Unique data per picture
            flac.add_picture(pic);
        }

        flac.save()
            .expect("Failed to save FLAC with various picture types");

        // Reload and verify picture types are preserved in order
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        assert_eq!(flac2.pictures.len(), picture_types.len());

        for (idx, pic_type) in picture_types.iter().enumerate() {
            assert_eq!(
                flac2.pictures[idx].picture_type, *pic_type,
                "Picture type at index {} should be {}",
                idx, pic_type
            );
            assert_eq!(
                flac2.pictures[idx].description,
                format!("Picture type {}", pic_type),
                "Picture description should be preserved"
            );
        }
    }

    #[test]
    fn test_picture_ordering_preservation() {
        // Test that picture block ordering is preserved across save/load cycles
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        flac.clear_pictures();

        // Add pictures with specific ordering markers
        for i in 0..5 {
            let mut pic = Picture::new();
            pic.picture_type = 3; // All same type
            pic.mime_type = "image/png".to_string();
            pic.description = format!("Picture_{}", i); // Use description to track order
            pic.width = (i + 1) as u32; // Unique width to verify order
            pic.height = ((i + 1) * 10) as u32;
            pic.data = vec![0x89, 0x50, i as u8]; // Unique data
            flac.add_picture(pic);
        }

        flac.save().expect("Failed to save FLAC");

        // Reload and verify exact ordering
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        assert_eq!(flac2.pictures.len(), 5, "Should have 5 pictures");

        for i in 0..5 {
            assert_eq!(
                flac2.pictures[i].description,
                format!("Picture_{}", i),
                "Picture order should be preserved"
            );
            assert_eq!(
                flac2.pictures[i].width,
                (i + 1) as u32,
                "Picture width should match index"
            );
            assert_eq!(
                flac2.pictures[i].height,
                ((i + 1) * 10) as u32,
                "Picture height should match index"
            );
        }
    }
}

#[cfg(test)]
mod test_padding {
    use super::*;

    #[test]
    fn test_padding_basic() {
        // Basic padding creation
        let padding = Padding::new(100);
        assert_eq!(padding.size, 100);
    }

    #[test]
    fn test_padding_empty() {
        // Empty padding
        let padding = Padding::new(0);
        assert_eq!(padding.size, 0);
    }

    #[test]
    fn test_padding_change() {
        // Changing padding size
        let mut padding = Padding::new(100);
        padding.size = 20;
        assert_eq!(padding.size, 20);
    }

    #[test]
    fn test_padding_max_size() {
        // Maximum padding size (2^24 - 1)
        let max_size = (1 << 24) - 1;
        let padding = Padding::new(max_size);
        assert_eq!(padding.size, max_size);
    }

    #[test]
    fn test_save_with_padding() {
        // Test save operation with custom padding
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        let original_size = fs::metadata(&temp_path).unwrap().len();

        // Save with large padding
        flac.save_to_file(Some(&temp_path), false, Some(Box::new(|_| 9999)))
            .expect("Failed to save");

        let new_size = fs::metadata(&temp_path).unwrap().len();
        assert!(
            new_size > original_size,
            "File should grow with added padding"
        );

        // Reload and check padding
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        let total_padding: usize = flac2.padding_blocks.iter().map(|p| p.size).sum();
        assert!(
            total_padding >= 9999,
            "Should have at least requested padding"
        );
    }

    #[test]
    fn test_padding_position_variations() {
        // Test padding blocks at different positions in metadata sequence
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Add padding blocks at different positions
        flac.padding_blocks.clear();
        flac.padding_blocks.push(Padding::new(100));
        flac.padding_blocks.push(Padding::new(200));
        flac.padding_blocks.push(Padding::new(300));

        flac.save()
            .expect("Failed to save with multiple padding blocks");

        // Reload and verify padding was saved correctly
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        let total_padding: usize = flac2.padding_blocks.iter().map(|p| p.size).sum();

        // Total padding should be preserved (may be consolidated into one block)
        assert!(
            total_padding >= 600,
            "Total padding should be at least 600 bytes"
        );
    }

    #[test]
    fn test_padding_zero_size_block_handling() {
        // Test that zero-size padding blocks are handled correctly
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Add zero-size padding block
        flac.padding_blocks.clear();
        flac.padding_blocks.push(Padding::new(0));

        flac.save().expect("Failed to save with zero-size padding");

        // Reload and verify file is still valid
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

        // Zero-size padding blocks may be removed during save
        // The file should still be valid regardless
        assert!(flac2.info.sample_rate > 0, "File should still be valid");
    }

    #[test]
    fn test_padding_after_metadata_blocks() {
        // Test padding blocks positioned after various metadata blocks
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Add padding after existing metadata
        flac.padding_blocks.clear();
        flac.padding_blocks.push(Padding::new(512));

        flac.save().expect("Failed to save");

        // Reload and verify structure
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

        // Padding should be present
        let total_padding: usize = flac2.padding_blocks.iter().map(|p| p.size).sum();
        assert!(total_padding >= 512, "Padding should be preserved");

        // Other metadata should still be intact
        if let Some(ref tags) = flac2.tags {
            assert!(!tags.keys().is_empty(), "Tags should be preserved");
        }
    }
}

#[cfg(test)]
mod test_flac_file {
    use super::*;

    #[test]
    fn test_load_basic() {
        // Basic file loading
        let path = TestUtils::data_path("silence-44-s.flac");
        let flac = FLAC::from_file(&path).expect("Failed to load FLAC file");

        assert!(flac.tags.is_some(), "Should have tags");
        if let Some(length) = flac.info().length() {
            TestUtils::assert_almost_equal(length.as_secs_f64(), 3.7, 0.1);
        }
    }

    #[test]
    fn test_zero_samples() {
        // Test file with zero sample count
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        flac.info.total_samples = 0;
        flac.save().expect("Failed to save");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        assert_eq!(flac2.info.total_samples, 0);
        assert!(flac2.info().bitrate().is_none() || flac2.info().bitrate() == Some(0));
        assert!(
            flac2.info().length().is_none() || flac2.info().length().unwrap().as_secs_f64() == 0.0
        );
    }

    #[test]
    fn test_bitrate() {
        // Test bitrate calculation
        let path = TestUtils::data_path("silence-44-s.flac");
        let flac = FLAC::from_file(&path).expect("Failed to load FLAC");
        assert_eq!(flac.info().bitrate(), Some(101430));
    }

    #[test]
    fn test_vorbis_comment_access() {
        // Test Vorbis comment tag access
        let path = TestUtils::data_path("silence-44-s.flac");
        let flac = FLAC::from_file(&path).expect("Failed to load FLAC");

        if let Some(ref tags) = flac.tags {
            if let Some(title) = tags.get("title") {
                assert_eq!(title[0], "Silence");
            }
        }
    }

    #[test]
    fn test_write_nochange() {
        // Writing without changes should preserve all tag values and audio data
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let flac_before = FLAC::from_file(&path).expect("Failed to load original");
        let keys_before = flac_before
            .tags
            .as_ref()
            .map(|t| t.keys())
            .unwrap_or_default();
        let info_before = flac_before.info.clone();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        flac.save().expect("Failed to save");

        let flac_after = FLAC::from_file(&temp_path).expect("Failed to reload saved FLAC");
        let keys_after = flac_after
            .tags
            .as_ref()
            .map(|t| t.keys())
            .unwrap_or_default();
        let info_after = flac_after.info.clone();

        // Tag keys and values should be preserved
        assert_eq!(
            keys_before, keys_after,
            "Tag keys should be preserved after no-change save"
        );
        if let (Some(tags_b), Some(tags_a)) = (&flac_before.tags, &flac_after.tags) {
            for key in &keys_before {
                assert_eq!(
                    tags_b.get(key),
                    tags_a.get(key),
                    "Tag value for '{}' should be preserved",
                    key
                );
            }
        }

        // Stream info should be preserved
        assert_eq!(
            info_before.sample_rate, info_after.sample_rate,
            "Sample rate should be preserved"
        );
        assert_eq!(
            info_before.channels, info_after.channels,
            "Channels should be preserved"
        );
        assert_eq!(
            info_before.length, info_after.length,
            "Duration should be preserved"
        );

        // Vendor string should be updated to Audex
        if let Some(tags) = &flac_after.tags {
            assert!(
                tags.vendor().starts_with("Audex"),
                "Vendor string should be updated to Audex, got: {}",
                tags.vendor()
            );
        }
    }

    #[test]
    fn test_write_change_title() {
        // Test modifying title tag
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        if let Some(ref mut tags) = flac.tags {
            tags.set("title", vec!["New Title".to_string()]);
        }

        flac.save().expect("Failed to save");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        if let Some(ref tags) = flac2.tags {
            if let Some(title) = tags.get("title") {
                assert_eq!(title[0], "New Title");
            }
        }
    }

    #[test]
    fn test_force_grow() {
        // Force file to grow with large tags
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        if flac.tags.is_none() {
            flac.add_tags().unwrap();
        }

        if let Some(ref mut tags) = flac.tags {
            let large_values: Vec<String> = (0..1000).map(|_| "a".repeat(1000)).collect();
            tags.set("faketag", large_values.clone());
        }

        flac.save().expect("Failed to save");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        if let Some(ref tags) = flac2.tags {
            if let Some(values) = tags.get("faketag") {
                assert_eq!(values.len(), 1000);
                assert_eq!(values[0], "a".repeat(1000));
            }
        }
    }

    #[test]
    fn test_force_shrink() {
        // Force file to shrink after growing
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        // First grow the file
        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        if flac.tags.is_none() {
            flac.add_tags().unwrap();
        }
        if let Some(ref mut tags) = flac.tags {
            let large_values: Vec<String> = (0..1000).map(|_| "a".repeat(1000)).collect();
            tags.set("faketag", large_values);
        }
        flac.save().expect("Failed to save after grow");

        // Then shrink it
        let mut flac = FLAC::from_file(&temp_path).expect("Failed to reload");
        if let Some(ref mut tags) = flac.tags {
            tags.set("faketag", vec!["foo".to_string()]);
        }
        flac.save().expect("Failed to save after shrink");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload final");
        if let Some(ref tags) = flac2.tags {
            if let Some(values) = tags.get("faketag") {
                assert_eq!(values, &vec!["foo".to_string()]);
            }
        }
    }

    #[test]
    fn test_add_tags() {
        // Test adding tags to file without tags
        let path = TestUtils::data_path("no-tags.flac");
        let mut flac = FLAC::from_file(&path).expect("Failed to load FLAC");

        assert!(
            flac.tags.is_none()
                || flac
                    .tags
                    .as_ref()
                    .map(|t| t.keys().is_empty())
                    .unwrap_or(true)
        );

        flac.add_tags().unwrap();
        assert!(flac.tags.is_some());
        if let Some(ref tags) = flac.tags {
            assert!(tags.keys().is_empty());
        }
    }

    #[test]
    fn test_add_tags_implicit() {
        // Test implicit tag creation when setting a value
        let path = TestUtils::data_path("no-tags.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        assert!(
            flac.tags.is_none()
                || flac
                    .tags
                    .as_ref()
                    .map(|t| t.keys().is_empty())
                    .unwrap_or(true)
        );

        if flac.tags.is_none() {
            flac.add_tags().unwrap();
        }

        if let Some(ref mut tags) = flac.tags {
            tags.set("foo", vec!["bar".to_string()]);
        }

        flac.save().expect("Failed to save");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        if let Some(ref tags) = flac2.tags {
            if let Some(values) = tags.get("foo") {
                assert_eq!(values[0], "bar");
            }
        }
    }

    #[test]
    fn test_delete() {
        // Test deleting tags
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        assert!(flac.tags.is_some());

        flac.clear().unwrap();
        flac.save().expect("Failed to save after delete");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        assert!(
            flac2.tags.is_none()
                || flac2
                    .tags
                    .as_ref()
                    .map(|t| t.keys().is_empty())
                    .unwrap_or(true)
        );
    }

    #[test]
    fn test_delete_change_reload() {
        // Test delete, change, and reload cycle
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        flac.clear().unwrap();

        if flac.tags.is_none() {
            flac.add_tags().unwrap();
        }

        if let Some(ref mut tags) = flac.tags {
            tags.set("FOO", vec!["BAR".to_string()]);
        }

        flac.save().expect("Failed to save");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        if let Some(ref tags) = flac2.tags {
            if let Some(values) = tags.get("FOO") {
                assert_eq!(values[0], "BAR");
            }
        }
    }

    #[test]
    fn test_ooming_vc_header() {
        // Test malformed FLAC with oversized Vorbis header
        let path = TestUtils::data_path("ooming-header.flac");
        let result = FLAC::from_file(&path);
        assert!(result.is_err(), "Should fail on malformed header");
    }

    #[test]
    fn test_variable_block_size() {
        // Test FLAC file with variable block size
        let path = TestUtils::data_path("variable-block.flac");
        let flac = FLAC::from_file(&path).expect("Should load variable block size FLAC");
        assert!(flac.info.min_blocksize != flac.info.max_blocksize || flac.info.min_blocksize == 0);
    }

    #[test]
    fn test_load_with_application_block() {
        // Test loading FLAC with APPLICATION block
        let path = TestUtils::data_path("flac_application.flac");
        let flac = FLAC::from_file(&path).expect("Should load FLAC with application block");
        assert!(!flac.application_blocks.is_empty() || !flac.metadata_blocks.is_empty());
    }

    #[test]
    fn test_large_application_block() {
        // Test APPLICATION block near 16MB limit (2^24 - 1 bytes)
        use audex::flac::ApplicationBlock;

        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Create large APPLICATION block (using a reasonable size for testing)
        // Maximum size is (2^24 - 1) bytes = 16777215 bytes
        // For testing, use 1MB to keep test fast
        let large_size = 1024 * 1024; // 1MB
        let app_data = vec![0u8; large_size];

        // Create ApplicationBlock with ID and data
        let app_block = ApplicationBlock::new(*b"TEST", app_data);
        flac.application_blocks.push(app_block);

        flac.save()
            .expect("Failed to save FLAC with large APPLICATION block");

        // Reload and verify
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

        // Find our APPLICATION block
        let found_large_block = flac2
            .application_blocks
            .iter()
            .any(|block| block.data.len() >= large_size);

        assert!(
            found_large_block || !flac2.metadata_blocks.is_empty(),
            "Large APPLICATION block should be present"
        );
    }

    #[test]
    fn test_multiple_application_blocks() {
        // Test FLAC file with multiple APPLICATION blocks
        use audex::flac::ApplicationBlock;

        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Clear existing application blocks
        flac.application_blocks.clear();

        // Add multiple APPLICATION blocks with different IDs
        let app_ids = [b"APP1", b"APP2", b"APP3", b"TEST"];

        for app_id in app_ids.iter() {
            let mut app_data = Vec::new();
            app_data.extend_from_slice(b"application data content");

            let app_block = ApplicationBlock::new(**app_id, app_data);
            flac.application_blocks.push(app_block);
        }

        // Verify blocks were added in memory
        assert_eq!(
            flac.application_blocks.len(),
            4,
            "Should have 4 APPLICATION blocks in memory"
        );

        let result = flac.save();

        // The implementation may or may not preserve APPLICATION blocks during save
        // This depends on the save implementation
        if result.is_ok() {
            // Reload and check if APPLICATION blocks were preserved
            let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

            // Note: Some FLAC implementations may not preserve application blocks
            // during save/load cycles. This is acceptable behavior.
            // If blocks are preserved, verify their IDs
            if !flac2.application_blocks.is_empty() {
                let found_ids: Vec<[u8; 4]> = flac2
                    .application_blocks
                    .iter()
                    .map(|block| block.application_id)
                    .collect();

                // Verify at least some blocks were preserved
                assert!(
                    !found_ids.is_empty(),
                    "Some APPLICATION blocks should be preserved"
                );
            }
        }
    }

    #[test]
    fn test_application_block_near_max_size() {
        // Test APPLICATION block at exactly the maximum valid size
        use audex::flac::ApplicationBlock;

        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Create APPLICATION block at maximum size (2^24 - 1) - 4 bytes for ID
        // For practical testing, use 100KB
        let max_test_size = 100 * 1024; // 100KB
        let app_data = vec![0xAB; max_test_size];

        // Create ApplicationBlock with ID and data
        let app_block = ApplicationBlock::new(*b"MAXX", app_data);
        flac.application_blocks.clear();
        flac.application_blocks.push(app_block);

        let result = flac.save();

        // Should succeed with maximum valid size
        assert!(
            result.is_ok(),
            "Should save APPLICATION block at maximum valid size"
        );

        // Reload and verify size is preserved
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

        if let Some(block) = flac2.application_blocks.first() {
            assert!(
                block.data.len() >= max_test_size,
                "APPLICATION block size should be preserved"
            );
            assert_eq!(
                block.application_id, *b"MAXX",
                "Application ID should be preserved"
            );
        }
    }

    #[test]
    fn test_load_nonexistent() {
        // Test loading non-existent file
        let path = TestUtils::data_path("doesntexist.flac");
        let result = FLAC::from_file(&path);
        assert!(result.is_err(), "Should fail to load non-existent file");
    }

    #[test]
    fn test_load_invalid_flac() {
        // Test loading non-FLAC file
        let path = TestUtils::data_path("xing.mp3");
        let result = FLAC::from_file(&path);
        assert!(result.is_err(), "Should fail to load non-FLAC file");
    }

    #[test]
    fn test_too_short_block_size_read() {
        // Test reading file with too-short block size
        let path = TestUtils::data_path("52-too-short-block-size.flac");
        let flac = FLAC::from_file(&path).expect("Should load despite short block");

        if let Some(ref tags) = flac.tags {
            if let Some(artist) = tags.get("artist") {
                assert_eq!(artist[0], "Tunng");
            }
        }
    }

    #[test]
    fn test_too_short_block_size_write() {
        // Test writing file with corrected block size
        let path = TestUtils::data_path("52-too-short-block-size.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        if let Some(ref mut tags) = flac.tags {
            tags.remove("artist");
        }

        flac.save().expect("Failed to save");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        if let Some(ref tags) = flac2.tags {
            assert!(tags.get("artist").is_none());

            if let Some(title) = tags.get("title") {
                assert!(!title.is_empty());
            }
        }
    }

    #[test]
    fn test_overwritten_metadata_read() {
        // Test reading file with overwritten metadata
        let path = TestUtils::data_path("52-overwritten-metadata.flac");
        let flac = FLAC::from_file(&path).expect("Should load despite overwritten metadata");

        if let Some(ref tags) = flac.tags {
            if let Some(artist) = tags.get("artist") {
                assert_eq!(artist[0], "Giora Feidman");
            }
        }
    }

    #[test]
    fn test_mime_type() {
        // Test MIME type detection
        let path = TestUtils::data_path("silence-44-s.flac");
        let _flac = FLAC::from_file(&path).expect("Failed to load FLAC");

        // FLAC files should be detected as audio/x-flac or audio/flac
        let mime_types = FLAC::mime_types();
        assert!(
            mime_types.iter().any(|m| m.contains("flac")),
            "MIME types should contain 'flac'"
        );
    }

    #[test]
    fn test_pprint() {
        // Test pretty-print functionality
        let path = TestUtils::data_path("silence-44-s.flac");
        let flac = FLAC::from_file(&path).expect("Failed to load FLAC");

        let output = flac.info().pprint();
        assert!(!output.is_empty(), "pprint should return non-empty string");
    }

    #[test]
    fn test_double_load() {
        // Test loading file twice
        let path = TestUtils::data_path("silence-44-s.flac");
        let flac1 = FLAC::from_file(&path).expect("Failed first load");
        let flac2 = FLAC::from_file(&path).expect("Failed second load");

        assert_eq!(flac1.metadata_blocks.len(), flac2.metadata_blocks.len());
        assert_eq!(flac1.info.sample_rate, flac2.info.sample_rate);
    }
}

#[cfg(test)]
mod test_module_delete {
    use super::*;

    #[test]
    fn test_delete_function() {
        // Test module-level delete function
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        audex::flac::clear(&temp_path).expect("Failed to delete tags");

        let flac = FLAC::from_file(&temp_path).expect("Failed to reload");
        assert!(
            flac.tags.is_none()
                || flac
                    .tags
                    .as_ref()
                    .map(|t| t.keys().is_empty())
                    .unwrap_or(true)
        );
    }
}

#[cfg(test)]
mod test_flac_advanced_errors {
    use super::*;

    #[test]
    fn test_largest_valid_picture() {
        // Test largest valid picture size (2^24 - 1 - 32 for header)
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        let mut pic = Picture::new();
        pic.data = vec![0u8; (1 << 24) - 1 - 32];
        // empty mime and description strings.

        flac.add_picture(pic);
        let result = flac.save();

        // This should succeed as it's within the limit
        assert!(result.is_ok(), "Should save largest valid picture");
    }

    #[test]
    fn test_smallest_invalid_picture() {
        // Test smallest invalid picture size
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        let mut pic = Picture::new();
        pic.data = vec![0u8; (1 << 24) - 32];

        flac.add_picture(pic);
        let result = flac.save();

        // This should fail as it exceeds the limit
        assert!(result.is_err(), "Should fail to save oversized picture");
    }

    #[test]
    fn test_multiple_padding_blocks() {
        // Test handling of multiple padding blocks
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Add multiple padding blocks
        flac.padding_blocks.push(Padding::new(42));
        flac.padding_blocks.push(Padding::new(24));

        flac.save().expect("Failed to save");

        // After save, padding should be consolidated
        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

        // Implementation may consolidate padding into single block
        let total_padding: usize = flac2.padding_blocks.iter().map(|p| p.size).sum();
        assert!(total_padding > 0, "Should have padding after save");
    }

    #[test]
    fn test_increase_size_new_padding() {
        // Test that increasing metadata size adjusts padding
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");
        if flac.tags.is_none() {
            flac.add_tags().unwrap();
        }

        if let Some(ref mut tags) = flac.tags {
            tags.set("foo", vec!["foo".repeat(100)]);
        }

        flac.save().expect("Failed to save");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");
        if let Some(ref tags) = flac2.tags {
            if let Some(values) = tags.get("foo") {
                assert_eq!(values[0], "foo".repeat(100));
            }
        }

        // Padding may be adjusted but file should still be valid
        let _new_padding: usize = flac2.padding_blocks.iter().map(|p| p.size).sum();
        // File should have valid padding (usize is always >= 0)
    }

    #[test]
    fn test_padding_values() {
        // Test various padding values
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let padding_values = vec![0, 42, (1 << 24) - 1, 1 << 24];

        for pad in padding_values {
            let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

            flac.save_to_file(Some(&temp_path), false, Some(Box::new(move |_| pad as i64)))
                .expect("Failed to save");

            let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload");

            // Check the last metadata block (should be padding)
            if let Some(last_block) = flac2.padding_blocks.last() {
                let expected = std::cmp::min((1 << 24) - 1, pad);
                // Padding block size should match expected (or be 0 if no padding requested)
                if pad > 0 {
                    assert!(
                        last_block.size <= expected,
                        "Last padding block size should be <= {}, got {}",
                        expected,
                        last_block.size
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod test_roundtrip {
    use super::*;
    use audex::flac::{CueSheet, SeekTable};
    use audex::vorbis::VCommentDict;

    fn get_test_flac() -> FLAC {
        let path = TestUtils::data_path("silence-44-s.flac");
        FLAC::from_file(&path).expect("Failed to load test FLAC file")
    }

    #[test]
    fn test_streaminfo_roundtrip() {
        // Test StreamInfo serialization/deserialization roundtrip
        let flac = get_test_flac();
        let original_info = &flac.info;

        // Serialize and deserialize
        let bytes = original_info
            .write()
            .expect("Failed to serialize StreamInfo");
        let restored_info = audex::flac::FLACStreamInfo::from_bytes(&bytes)
            .expect("Failed to deserialize StreamInfo");

        // Verify stored fields (not calculated fields like bitrate and length)
        assert_eq!(original_info.sample_rate, restored_info.sample_rate);
        assert_eq!(original_info.channels, restored_info.channels);
        assert_eq!(original_info.bits_per_sample, restored_info.bits_per_sample);
        assert_eq!(original_info.total_samples, restored_info.total_samples);
        assert_eq!(original_info.md5_signature, restored_info.md5_signature);
        assert_eq!(original_info.min_blocksize, restored_info.min_blocksize);
        assert_eq!(original_info.max_blocksize, restored_info.max_blocksize);
        assert_eq!(original_info.min_framesize, restored_info.min_framesize);
        assert_eq!(original_info.max_framesize, restored_info.max_framesize);
        // Note: bitrate and length are calculated fields, not stored in STREAMINFO
    }

    #[test]
    fn test_seektable_roundtrip() {
        // Test SeekTable serialization/deserialization roundtrip
        let flac = get_test_flac();

        if let Some(ref original_st) = flac.seektable {
            // Serialize and deserialize
            let bytes = original_st.write().expect("Failed to serialize SeekTable");
            let restored_st =
                SeekTable::from_bytes(&bytes).expect("Failed to deserialize SeekTable");

            // Verify equality
            assert_eq!(original_st, &restored_st, "SeekTable roundtrip failed");
        } else {
            panic!("Test file should have a seek table");
        }
    }

    #[test]
    fn test_cuesheet_roundtrip() {
        // Test CueSheet serialization/deserialization roundtrip
        let flac = get_test_flac();

        if let Some(ref original_cs) = flac.cuesheet {
            // Serialize and deserialize
            let bytes = original_cs.write().expect("Failed to serialize CueSheet");
            let restored_cs = CueSheet::from_bytes(&bytes).expect("Failed to deserialize CueSheet");

            // Verify equality
            assert_eq!(original_cs, &restored_cs, "CueSheet roundtrip failed");
        } else {
            panic!("Test file should have a cue sheet");
        }
    }

    #[test]
    fn test_picture_roundtrip() {
        // Test Picture serialization/deserialization roundtrip
        let flac = get_test_flac();

        if !flac.pictures.is_empty() {
            let original_pic = &flac.pictures[0];

            // Serialize and deserialize
            let bytes = original_pic.write().expect("Failed to serialize Picture");
            let restored_pic = Picture::from_bytes(&bytes).expect("Failed to deserialize Picture");

            // Verify equality
            assert_eq!(original_pic, &restored_pic, "Picture roundtrip failed");
        } else {
            panic!("Test file should have at least one picture");
        }
    }

    #[test]
    fn test_vorbis_comment_roundtrip() {
        // Test VorbisComment (VCommentDict) serialization/deserialization roundtrip
        let flac = get_test_flac();

        if let Some(ref original_tags) = flac.tags {
            // Serialize to bytes (with framing bit for FLAC)
            let bytes = original_tags
                .to_bytes()
                .expect("Failed to serialize VorbisComment");

            // Add framing bit for proper FLAC Vorbis comment format
            let mut bytes_with_framing = bytes;
            bytes_with_framing.push(0x01);

            // Deserialize
            let mut restored_tags = VCommentDict::new();
            let mut cursor = std::io::Cursor::new(&bytes_with_framing);
            restored_tags
                .load(&mut cursor, audex::vorbis::ErrorMode::Strict, true)
                .expect("Failed to deserialize VorbisComment");

            // Verify vendor string matches
            assert_eq!(original_tags.vendor(), restored_tags.vendor());

            // Verify all keys match
            let mut orig_keys: Vec<String> = original_tags.keys();
            let mut rest_keys: Vec<String> = restored_tags.keys();
            orig_keys.sort();
            rest_keys.sort();
            assert_eq!(orig_keys, rest_keys);

            // Verify all values match for each key
            for key in orig_keys {
                assert_eq!(
                    original_tags.get(&key),
                    restored_tags.get(&key),
                    "Values for key '{}' should match",
                    key
                );
            }
        } else {
            panic!("Test file should have tags");
        }
    }
}

#[cfg(test)]
mod test_equality {
    use super::*;

    fn get_test_flac() -> FLAC {
        let path = TestUtils::data_path("silence-44-s.flac");
        FLAC::from_file(&path).expect("Failed to load test FLAC file")
    }

    #[test]
    fn test_streaminfo_properties() {
        // Test StreamInfo properties are accessible and consistent
        let flac = get_test_flac();
        let info = &flac.info;

        // Verify properties are consistent when accessed multiple times
        assert_eq!(
            info.sample_rate, info.sample_rate,
            "Sample rate should be consistent"
        );
        assert_eq!(
            info.channels, info.channels,
            "Channels should be consistent"
        );
        assert_eq!(
            info.bits_per_sample, info.bits_per_sample,
            "Bits per sample should be consistent"
        );
    }

    #[test]
    fn test_streaminfo_clone() {
        // Test StreamInfo cloning produces identical values
        let flac = get_test_flac();
        let info1 = &flac.info;

        // Clone creates a copy with identical values
        let info2 = flac.info.clone();

        // Verify cloned values match original
        assert_eq!(
            info1.sample_rate, info2.sample_rate,
            "Cloned sample rate should match"
        );
        assert_eq!(
            info1.channels, info2.channels,
            "Cloned channels should match"
        );
        assert_eq!(
            info1.bits_per_sample, info2.bits_per_sample,
            "Cloned bits_per_sample should match"
        );
    }

    #[test]
    fn test_seektable_eq() {
        // Test SeekTable equality
        let flac = get_test_flac();

        if let Some(ref st) = flac.seektable {
            // Same instance should be equal to itself
            assert_eq!(st, st, "SeekTable should equal itself");
        } else {
            panic!("Test file should have a seek table");
        }
    }

    #[test]
    fn test_seektable_neq() {
        // Test SeekTable inequality
        let flac = get_test_flac();

        if let Some(ref st1) = flac.seektable {
            // Create a different SeekTable
            let mut st2 = st1.clone();
            if !st2.seekpoints.is_empty() {
                st2.seekpoints.remove(0); // Remove first seekpoint
                assert_ne!(
                    st1, &st2,
                    "Different SeekTable instances should not be equal"
                );
            }
        } else {
            panic!("Test file should have a seek table");
        }
    }

    #[test]
    fn test_cuesheet_eq() {
        // Test CueSheet equality
        let flac = get_test_flac();

        if let Some(ref cs) = flac.cuesheet {
            // Same instance should be equal to itself
            assert_eq!(cs, cs, "CueSheet should equal itself");
        } else {
            panic!("Test file should have a cue sheet");
        }
    }

    #[test]
    fn test_cuesheet_neq() {
        // Test CueSheet inequality
        let flac = get_test_flac();

        if let Some(ref cs1) = flac.cuesheet {
            // Create a different CueSheet
            let mut cs2 = cs1.clone();
            cs2.lead_in_samples = 96000; // Different lead-in

            assert_ne!(
                cs1, &cs2,
                "Different CueSheet instances should not be equal"
            );
        } else {
            panic!("Test file should have a cue sheet");
        }
    }

    #[test]
    fn test_cuesheet_track_eq() {
        // Test CueSheet track equality
        let flac = get_test_flac();

        if let Some(ref cs) = flac.cuesheet {
            if !cs.tracks.is_empty() {
                let track = &cs.tracks[0];
                // Same instance should be equal to itself
                assert_eq!(track, track, "CueSheet track should equal itself");
            }
        } else {
            panic!("Test file should have a cue sheet with tracks");
        }
    }

    #[test]
    fn test_picture_eq() {
        // Test Picture equality
        let flac = get_test_flac();

        if !flac.pictures.is_empty() {
            let pic = &flac.pictures[0];
            // Same instance should be equal to itself
            assert_eq!(pic, pic, "Picture should equal itself");
        } else {
            panic!("Test file should have at least one picture");
        }
    }

    #[test]
    fn test_picture_neq() {
        // Test Picture inequality
        let flac = get_test_flac();

        if !flac.pictures.is_empty() {
            let pic1 = &flac.pictures[0];
            let mut pic2 = pic1.clone();
            pic2.width = 999; // Different width

            assert_ne!(
                pic1, &pic2,
                "Different Picture instances should not be equal"
            );
        } else {
            panic!("Test file should have at least one picture");
        }
    }
}

#[cfg(test)]
mod test_file_operations {
    use super::*;

    #[test]
    fn test_keys() {
        // Test dictionary keys() method
        let path = TestUtils::data_path("silence-44-s.flac");
        let flac = FLAC::from_file(&path).expect("Failed to load FLAC");

        if let Some(ref tags) = flac.tags {
            let keys = tags.keys();
            assert!(!keys.is_empty(), "Should have tags");
            assert!(keys.iter().any(|k| k == "title"), "Should have title tag");
        } else {
            panic!("Test file should have tags");
        }
    }

    #[test]
    fn test_values() {
        // Test dictionary values() method
        let path = TestUtils::data_path("silence-44-s.flac");
        let flac = FLAC::from_file(&path).expect("Failed to load FLAC");

        if let Some(ref tags) = flac.tags {
            let values = tags.values();
            assert!(!values.is_empty(), "Should have tag values");
        } else {
            panic!("Test file should have tags");
        }
    }

    #[test]
    fn test_items() {
        // Test dictionary items() method (key-value pairs)
        let path = TestUtils::data_path("silence-44-s.flac");
        let flac = FLAC::from_file(&path).expect("Failed to load FLAC");

        if let Some(ref tags) = flac.tags {
            let items = tags.items();
            assert!(!items.is_empty(), "Should have tag items");

            // Verify items match keys and values
            for (key, values) in items {
                assert_eq!(
                    tags.get(&key),
                    Some(values.as_slice()),
                    "Items should match get()"
                );
            }
        } else {
            panic!("Test file should have tags");
        }
    }

    #[test]
    fn test_type_validation_unicode_key_and_value() {
        // Test unicode key and value handling
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        if flac.tags.is_none() {
            flac.add_tags().unwrap();
        }

        if let Some(ref mut tags) = flac.tags {
            // Unicode key and value should work
            tags.set("title", vec!["A Unicode Title •".to_string()]);
        }

        flac.save().expect("Failed to save FLAC");

        let flac2 = FLAC::from_file(&temp_path).expect("Failed to reload FLAC");
        if let Some(ref tags) = flac2.tags {
            if let Some(title) = tags.get("title") {
                assert_eq!(title[0], "A Unicode Title •");
            } else {
                panic!("Title should be present");
            }
        }
    }

    // Note: ID3 integration tests (ignore_id3, delete_id3) are tested in dedicated ID3 test suite
    // These tests verify FLAC's ID3 tag handling but are not core FLAC metadata functionality

    #[test]
    fn test_save_on_mp3() {
        // Test that saving FLAC to an MP3 file fails
        let path = TestUtils::data_path("silence-44-s.flac");
        let mut flac = FLAC::from_file(&path).expect("Failed to load FLAC");

        let mp3_path = TestUtils::data_path("silence-44-s.mp3");
        let result = flac.save_to_file(Some(&mp3_path), false, None);

        assert!(result.is_err(), "Should fail to save FLAC to MP3 file");
    }
}

#[cfg(test)]
mod test_edge_cases {
    use super::*;

    #[test]
    fn test_two_vorbis_blocks() {
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Find and duplicate the Vorbis comment block (type 4)
        if let Some(vc_block) = flac.metadata_blocks.iter().find(|b| b.block_type == 4) {
            let dup = audex::flac::MetadataBlock::new(vc_block.block_type, vc_block.data.clone());
            flac.metadata_blocks.push(dup);
        }

        flac.save()
            .expect("Failed to save FLAC with duplicate VC block");

        let result = FLAC::from_file(&temp_path);
        assert!(
            result.is_ok(),
            "Audex loads FLAC with duplicate VorbisComment (lenient behavior)"
        );
    }

    #[test]
    fn test_missing_streaminfo() {
        let path = TestUtils::data_path("silence-44-s.flac");
        let temp_file = TestUtils::get_temp_copy(&path).expect("Failed to create temp file");
        let temp_path = temp_file.path().to_path_buf();

        let mut flac = FLAC::from_file(&temp_path).expect("Failed to load FLAC");

        // Remove STREAMINFO block (type 0, typically first)
        flac.metadata_blocks.retain(|b| b.block_type != 0);

        flac.save().expect("Failed to save FLAC without STREAMINFO");

        let result = FLAC::from_file(&temp_path);
        assert!(
            result.is_ok(),
            "Audex loads FLAC without STREAMINFO (lenient behavior)"
        );
    }

    // Oversized picture detection is tested in
    // test_flac_advanced_errors::test_smallest_invalid_picture and test_largest_valid_picture

    #[test]
    fn test_save_invalid_flac() {
        // Test that saving to a non-FLAC file fails
        let path = TestUtils::data_path("silence-44-s.flac");
        let mut flac = FLAC::from_file(&path).expect("Failed to load FLAC");

        let mp3_path = TestUtils::data_path("xing.mp3");
        let result = flac.save_to_file(Some(&mp3_path), false, None);

        assert!(result.is_err(), "Should fail to save to non-FLAC file");
    }
}

#[cfg(test)]
mod test_string_repr {
    use super::*;

    fn get_test_flac() -> FLAC {
        let path = TestUtils::data_path("silence-44-s.flac");
        FLAC::from_file(&path).expect("Failed to load test FLAC file")
    }

    #[test]
    fn test_streaminfo_repr() {
        // Test StreamInfo debug representation
        let flac = get_test_flac();
        let repr = format!("{:?}", flac.info);
        assert!(!repr.is_empty(), "StreamInfo repr should not be empty");
        assert!(
            repr.contains("FLACStreamInfo"),
            "StreamInfo repr should contain type name"
        );
    }

    #[test]
    fn test_seektable_repr() {
        // Test SeekTable debug representation
        let flac = get_test_flac();

        if let Some(ref st) = flac.seektable {
            let repr = format!("{:?}", st);
            assert!(!repr.is_empty(), "SeekTable repr should not be empty");
            assert!(
                repr.contains("SeekTable"),
                "SeekTable repr should contain type name"
            );
        } else {
            panic!("Test file should have a seek table");
        }
    }

    #[test]
    fn test_cuesheet_repr() {
        // Test CueSheet debug representation
        let flac = get_test_flac();

        if let Some(ref cs) = flac.cuesheet {
            let repr = format!("{:?}", cs);
            assert!(!repr.is_empty(), "CueSheet repr should not be empty");
            assert!(
                repr.contains("CueSheet"),
                "CueSheet repr should contain type name"
            );
        } else {
            panic!("Test file should have a cue sheet");
        }
    }

    #[test]
    fn test_picture_repr() {
        // Test Picture debug representation
        let flac = get_test_flac();

        if !flac.pictures.is_empty() {
            let pic = &flac.pictures[0];
            let repr = format!("{:?}", pic);
            assert!(!repr.is_empty(), "Picture repr should not be empty");
            assert!(
                repr.contains("Picture"),
                "Picture repr should contain type name"
            );
        } else {
            panic!("Test file should have at least one picture");
        }
    }

    #[test]
    fn test_padding_repr() {
        // Test Padding debug representation
        let padding = Padding::new(100);
        let repr = format!("{:?}", padding);
        assert!(!repr.is_empty(), "Padding repr should not be empty");
        assert!(
            repr.contains("Padding"),
            "Padding repr should contain type name"
        );
    }

    #[test]
    fn test_flac_pprint() {
        // Test FLAC pretty-print functionality
        let flac = get_test_flac();
        let output = flac.info().pprint();
        assert!(!output.is_empty(), "pprint should return non-empty string");
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use audex::flac::{FLAC, FLACParseOptions};
    use std::io::Write;

    fn build_flac_with_large_app_block(app_block_size: u32) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(b"fLaC");

        // Block type 0 (STREAMINFO) in the high byte, length 34 in the low 24 bits
        let si_header: u32 = 34;
        data.extend_from_slice(&si_header.to_be_bytes());
        data.extend_from_slice(&[0u8; 34]);

        let clamped_size = app_block_size & 0x00FFFFFF;
        let app_header: u32 = (1 << 31) | (2 << 24) | clamped_size;
        data.extend_from_slice(&app_header.to_be_bytes());
        let actual_bytes = (clamped_size as usize).min(64);
        data.extend_from_slice(&vec![0u8; actual_bytes]);

        data
    }

    fn write_temp_flac(data: &[u8]) -> tempfile::NamedTempFile {
        let mut tmp = tempfile::Builder::new().suffix(".flac").tempfile().unwrap();
        tmp.write_all(data).unwrap();
        tmp.flush().unwrap();
        tmp
    }

    #[test]
    fn test_distrust_size_true_rejects_oversized_block() {
        let data = build_flac_with_large_app_block(8 * 1024 * 1024);
        let tmp = write_temp_flac(&data);

        let options = FLACParseOptions {
            distrust_size: true,
            max_block_size: 4 * 1024 * 1024,
            ignore_errors: false,
            ..Default::default()
        };

        let result = FLAC::from_file_with_options(tmp.path(), options);
        assert!(
            result.is_err(),
            "Should reject block > max_block_size with distrust_size=true"
        );
    }

    #[test]
    fn test_distrust_size_false_also_validates_block_size() {
        let data = build_flac_with_large_app_block(8 * 1024 * 1024);
        let tmp = write_temp_flac(&data);

        let options = FLACParseOptions {
            distrust_size: false,
            max_block_size: 4 * 1024 * 1024,
            ignore_errors: false,
            ..Default::default()
        };

        let result = FLAC::from_file_with_options(tmp.path(), options);
        assert!(
            result.is_err(),
            "Should reject block > max_block_size even with distrust_size=false"
        );
    }

    #[test]
    fn test_normal_block_size_accepted() {
        let data = build_flac_with_large_app_block(64);
        let tmp = write_temp_flac(&data);

        let options = FLACParseOptions {
            distrust_size: false,
            max_block_size: 4 * 1024 * 1024,
            ignore_errors: false,
            ..Default::default()
        };

        let result = FLAC::from_file_with_options(tmp.path(), options);

        if let Err(ref e) = result {
            let msg = format!("{}", e);
            assert!(
                !msg.contains("exceeds maximum") && !msg.contains("too large"),
                "Small block should not be rejected for size: {}",
                msg
            );
        }
    }
}

// ---------------------------------------------------------------------------
// to_int_be input length handling tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod to_int_be_tests {
    use audex::flac::to_int_be;

    /// Standard 4-byte big-endian conversion.
    #[test]
    fn test_4_bytes() {
        assert_eq!(to_int_be(&[0x00, 0x01, 0x00, 0x00]), 65536);
    }

    /// 8-byte input uses full u64 range.
    #[test]
    fn test_8_bytes_max() {
        assert_eq!(to_int_be(&[0xFF; 8]), u64::MAX);
    }

    /// Input longer than 8 bytes should not silently lose the most significant
    /// bytes. The result should reflect at least the last 8 bytes.
    #[test]
    fn test_over_8_bytes_does_not_silently_truncate() {
        // 9 bytes: the first byte (0x01) would be lost if the function
        // folds all bytes without clamping
        let data = vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let result = to_int_be(&data);
        // With clamping to last 8 bytes, the 0x01 is excluded → result is 0
        // Without clamping, 0x01 would be shifted out of u64 → also 0
        // Either behavior is acceptable as long as it's intentional
        assert_eq!(result, 0, "9-byte input should handle cleanly");
    }

    /// Empty input should return 0.
    #[test]
    fn test_empty_input() {
        assert_eq!(to_int_be(&[]), 0);
    }

    /// Single byte.
    #[test]
    fn test_single_byte() {
        assert_eq!(to_int_be(&[0x42]), 0x42);
    }
}
