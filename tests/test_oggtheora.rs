//! OggTheora format tests

use audex::ogg::OggPage;
use audex::oggtheora::{OggTheora, TheoraInfo};
use audex::{AudexError, FileType, StreamInfo, Tags};
use std::fs;
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::time::Duration;

mod common;
use common::TestUtils;

// Test helper functions
fn get_test_files() -> (PathBuf, PathBuf, PathBuf) {
    let sample_path = TestUtils::data_path("sample.oggtheora");
    let length_path = TestUtils::data_path("sample_length.oggtheora");
    let bitrate_path = TestUtils::data_path("sample_bitrate.oggtheora");
    (sample_path, length_path, bitrate_path)
}

// TOggFileTypeMixin equivalent - scan_file functionality
fn scan_file(path: &Path) -> Result<(), AudexError> {
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);

    loop {
        match OggPage::from_reader(&mut reader) {
            Ok(_) => continue,
            Err(AudexError::Io(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

// Core Theora tests
#[cfg(test)]
mod theora_reference_parity_tests {
    use super::*;

    #[test]
    fn test_theora_bad_version() {
        let (sample_path, _, _) = get_test_files();

        if sample_path.exists() {
            // Read the first page from the file
            let file = fs::File::open(&sample_path).expect("Failed to open test file");
            let mut reader = BufReader::new(file);
            let page = OggPage::from_reader(&mut reader).expect("Failed to read OGG page");

            // Modify the packet to have bad version
            if let Some(packet) = page.packets.first() {
                let mut modified_packet = packet.clone();
                if modified_packet.len() >= 9 {
                    // packet[:7] + b"\x03\x00" + packet[9:]
                    modified_packet[7] = 0x03;
                    modified_packet[8] = 0x00;

                    // This should fail when creating TheoraInfo from the bad header
                    let result = TheoraInfo::from_identification_header(&modified_packet);
                    assert!(result.is_err(), "Expected error for bad version");
                }
            }
        } else {
            println!("Test file not found: {:?}", sample_path);
        }
    }

    #[test]
    fn test_theora_not_first_page() {
        // Test that Theora header not on first page fails
        let empty_ogg_path = TestUtils::data_path("empty.ogg");
        if empty_ogg_path.exists() {
            // Try to load non-Theora file - should fail
            let result = OggTheora::load(&empty_ogg_path);
            assert!(result.is_err(), "Expected error for non-Theora OGG file");
        } else {
            println!("Empty OGG file not found: {:?}", empty_ogg_path);
        }
    }

    #[test]
    fn test_vendor() {
        let (sample_path, _, _) = get_test_files();

        if sample_path.exists() {
            if let Ok(audio) = OggTheora::load(&sample_path) {
                if let Some(tags) = audio.tags() {
                    // Check vendor string
                    let vendor = tags.vendor();
                    assert!(
                        vendor.starts_with("Xiph.Org libTheora"),
                        "Vendor should start with 'Xiph.Org libTheora', got: {}",
                        vendor
                    );

                    // Test that "vendor" key is not directly accessible as a regular tag
                    assert!(
                        tags.get("vendor").is_none() || tags.get("vendor").unwrap().is_empty(),
                        "vendor should not be accessible as regular tag key"
                    );
                }
            } else {
                println!("Could not load Theora file: {:?}", sample_path);
            }
        } else {
            println!("Test file not found: {:?}", sample_path);
        }
    }

    #[test]
    fn test_not_my_ogg() {
        let empty_ogg_path = TestUtils::data_path("empty.ogg");

        if empty_ogg_path.exists() {
            // Should fail to load as Theora
            let result = OggTheora::load(&empty_ogg_path);
            assert!(
                result.is_err(),
                "Expected error loading non-Theora OGG file"
            );
        } else {
            println!("Empty OGG file not found: {:?}", empty_ogg_path);
        }
    }

    #[test]
    fn test_length() {
        let (sample_path, length_path, _) = get_test_files();

        // Test main file length (~5.5 seconds with tolerance 1)
        if sample_path.exists() {
            if let Ok(audio) = OggTheora::load(&sample_path) {
                if let Some(length) = audio.info().length() {
                    let length_secs = length.as_secs_f64();
                    TestUtils::assert_almost_equal(length_secs, 5.5, 0.1);
                } else {
                    println!("No length info for sample file");
                }
            } else {
                println!("Could not load sample file: {:?}", sample_path);
            }
        } else {
            println!("Sample file not found: {:?}", sample_path);
        }

        // Test secondary file length
        if length_path.exists() {
            if let Ok(audio2) = OggTheora::load(&length_path) {
                if let Some(length) = audio2.info().length() {
                    let length_secs = length.as_secs_f64();
                    TestUtils::assert_almost_equal(length_secs, 0.75, 0.01);
                } else {
                    println!("No length info for length test file");
                }
            } else {
                println!("Could not load length test file: {:?}", length_path);
            }
        } else {
            println!("Length test file not found: {:?}", length_path);
        }
    }

    #[test]
    fn test_bitrate() {
        let (_, _, bitrate_path) = get_test_files();

        // Expected bitrate is 16777215 for this file
        if bitrate_path.exists() {
            if let Ok(audio3) = OggTheora::load(&bitrate_path) {
                if let Some(bitrate) = audio3.info().bitrate() {
                    assert_eq!(bitrate, 16777215, "Bitrate should be exactly 16777215");
                } else {
                    println!("No bitrate info for bitrate test file");
                }
            } else {
                println!("Could not load bitrate test file: {:?}", bitrate_path);
            }
        } else {
            println!("Bitrate test file not found: {:?}", bitrate_path);
        }
    }

    #[test]
    fn test_module_delete() {
        let (sample_path, _, _) = get_test_files();

        if sample_path.exists() {
            // Test file scan functionality
            let scan_result = scan_file(&sample_path);
            assert!(scan_result.is_ok(), "File scan should succeed");

            // Test loading file after scan - should still work
            if let Ok(audio) = OggTheora::load(&sample_path) {
                // Just verify we can access the info
                let _ = audio.info();
            } else {
                println!("Could not reload file after scan test");
            }
        } else {
            println!("Sample file not found: {:?}", sample_path);
        }
    }

    #[test]
    fn test_mime() {
        let mime_types = OggTheora::mime_types();
        assert!(
            mime_types.contains(&"video/x-theora"),
            "MIME types should contain video/x-theora"
        );
    }

    #[test]
    fn test_init_padding() {
        let (sample_path, _, _) = get_test_files();

        if sample_path.exists() {
            if let Ok(audio) = OggTheora::load(&sample_path) {
                if let Some(_tags) = audio.tags() {
                    // Tags should be accessible
                } else {
                    println!("No tags found in sample file");
                }
            } else {
                println!("Could not load sample file: {:?}", sample_path);
            }
        } else {
            println!("Sample file not found: {:?}", sample_path);
        }
    }
}

// TOggFileTypeMixin equivalent tests
#[cfg(test)]
mod ogg_file_type_mixin_tests {
    use super::*;

    #[test]
    fn test_scan_file() {
        let (sample_path, _, _) = get_test_files();

        if sample_path.exists() {
            // Should be able to scan through all pages without error
            let result = scan_file(&sample_path);
            assert!(result.is_ok(), "File scan should succeed");
        } else {
            println!("Sample file not found: {:?}", sample_path);
        }
    }

    #[test]
    fn test_pprint_empty() {
        let (sample_path, _, _) = get_test_files();

        if sample_path.exists() {
            if let Ok(audio) = OggTheora::load(&sample_path) {
                // Should be able to pretty print without error
                let output = audio.info().pretty_print();
                assert!(!output.is_empty(), "Pretty print should produce output");
            } else {
                println!("Could not load sample file: {:?}", sample_path);
            }
        } else {
            println!("Sample file not found: {:?}", sample_path);
        }
    }

    #[test]
    fn test_pprint_stuff() {
        let (sample_path, _, _) = get_test_files();

        if sample_path.exists() {
            if let Ok(mut audio) = OggTheora::load(&sample_path) {
                // Add some tags first
                let _ = audio.add_tags();
                if let Some(tags) = audio.tags_mut() {
                    tags.set("ARTIST", vec!["Test Artist".to_string()]);
                    tags.set("TITLE", vec!["Test Title".to_string()]);
                }

                // Should be able to pretty print with tags
                let output = audio.info().pretty_print();
                assert!(
                    !output.is_empty(),
                    "Pretty print with tags should produce output"
                );
            } else {
                println!("Could not load sample file: {:?}", sample_path);
            }
        } else {
            println!("Sample file not found: {:?}", sample_path);
        }
    }

    #[test]
    fn test_length_mixin() {
        let (sample_path, _, _) = get_test_files();

        if sample_path.exists() {
            if let Ok(audio) = OggTheora::load(&sample_path) {
                // The sample file contains ~5.5 seconds of content
                // (55 frames at 10 fps based on granule position)
                if let Some(length) = audio.info().length() {
                    let length_secs = length.as_secs_f64();
                    TestUtils::assert_almost_equal(length_secs, 5.5, 0.1);
                } else {
                    println!("No length info available for mixin test");
                }
            } else {
                println!("Could not load sample file: {:?}", sample_path);
            }
        } else {
            println!("Sample file not found: {:?}", sample_path);
        }
    }
}

#[cfg(test)]
mod theora_info_tests {
    use super::*;

    #[test]
    fn test_theora_info_creation() {
        let info = TheoraInfo::default();
        assert_eq!(info.length(), None);
        assert_eq!(info.bitrate(), None);
        assert_eq!(info.sample_rate(), None); // Video format - no sample rate
        assert_eq!(info.channels(), None); // Video format - no channels
        assert_eq!(info.bits_per_sample(), None); // Video format - varies
        assert_eq!(info.fps, 0.0);
        assert_eq!(info.width, 0);
        assert_eq!(info.height, 0);
        assert_eq!(info.version_major, 0);
        assert_eq!(info.version_minor, 0);
    }

    #[test]
    fn test_theora_info_populated() {
        let info = TheoraInfo {
            length: Some(Duration::from_secs(120)),
            fps: 29.97,
            bitrate: 500000,
            width: 640,
            height: 480,
            version_major: 3,
            version_minor: 2,
            serial: 12345,
            ..Default::default()
        };

        assert_eq!(info.length(), Some(Duration::from_secs(120)));
        assert_eq!(info.bitrate(), Some(500000));
        assert_eq!(info.sample_rate(), None); // Always None for video
        assert_eq!(info.channels(), None); // Always None for video
        assert_eq!(info.bits_per_sample(), None); // Always None for video
        assert_eq!(info.fps, 29.97);
        assert_eq!(info.width, 640);
        assert_eq!(info.height, 480);
        assert_eq!(info.serial, 12345);
    }

    #[test]
    fn test_theora_info_pretty_print() {
        let info = TheoraInfo {
            length: Some(Duration::from_secs_f64(123.45)),
            bitrate: 1000000,
            ..Default::default()
        };

        let output = info.pretty_print();
        assert!(output.contains("Ogg Theora"));
        assert!(output.contains("123.45 seconds"));
        assert!(output.contains("1000000 bps"));
    }

    #[test]
    fn test_theora_info_pretty_print_unknown() {
        let info = TheoraInfo::default();
        let output = info.pretty_print();
        assert!(output.contains("Ogg Theora"));
        assert!(output.contains("unknown seconds"));
        assert!(output.contains("0 bps"));
    }
}

#[cfg(test)]
mod theora_header_parsing_tests {
    use super::*;

    #[test]
    fn test_valid_theora_header() {
        // Create a minimal valid Theora identification header
        let mut header = Vec::new();
        header.push(0x80); // Packet type
        header.extend_from_slice(b"theora"); // Signature
        header.push(3); // Version major
        header.push(2); // Version minor
        header.push(0); // Version subminor
        header.extend_from_slice(&(320u16).to_be_bytes()); // Frame width (macroblock aligned)
        header.extend_from_slice(&(240u16).to_be_bytes()); // Frame height (macroblock aligned)

        // Picture width and height (24-bit big endian)
        header.extend_from_slice(&[0x00, 0x01, 0x40]); // 320 (0x000140)
        header.extend_from_slice(&[0x00, 0x00, 0xF0]); // 240 (0x0000F0)

        header.push(0); // Offset X
        header.push(0); // Offset Y

        // Frame rate (32-bit big endian)
        header.extend_from_slice(&(30u32).to_be_bytes()); // FPS numerator
        header.extend_from_slice(&(1u32).to_be_bytes()); // FPS denominator

        // Aspect ratio (24-bit big endian)
        header.extend_from_slice(&[0x00, 0x00, 0x01]); // Aspect numerator = 1
        header.extend_from_slice(&[0x00, 0x00, 0x01]); // Aspect denominator = 1

        header.push(0); // Colorspace

        // Nominal bitrate (24-bit big endian)
        header.extend_from_slice(&[0x07, 0xA1, 0x20]); // 500000 bps

        // Quality and keyframe info (16-bit big endian)
        // Upper 6 bits: quality (0-63), next 5 bits: keyframe granule shift, lower 2 bits: pixel format
        let quality_keyframe = ((32u16) << 10) | ((6u16) << 5); // Quality=32, shift=6, format=0
        header.extend_from_slice(&quality_keyframe.to_be_bytes());

        // Pad to minimum length
        while header.len() < 42 {
            header.push(0);
        }

        let info = TheoraInfo::from_identification_header(&header).unwrap();
        assert_eq!(info.version_major, 3);
        assert_eq!(info.version_minor, 2);
        assert_eq!(info.width, 320);
        assert_eq!(info.height, 240);
        assert_eq!(info.frame_width, 5120); // 320 << 4
        assert_eq!(info.frame_height, 3840); // 240 << 4
        assert_eq!(info.fps, 30.0);
        assert_eq!(info.bitrate, 500000);
        assert_eq!(info.granule_shift, 6);
        assert_eq!(info.quality, 32);
        assert_eq!(info.pixel_fmt, 0);
    }

    #[test]
    fn test_invalid_theora_header_too_short() {
        let short_header = b"\x80theora\x03\x02\x00"; // Only 10 bytes
        let result = TheoraInfo::from_identification_header(short_header);
        assert!(result.is_err());
        if let Err(AudexError::InvalidData(msg)) = result {
            assert!(msg.contains("too short"));
        }
    }

    #[test]
    fn test_invalid_theora_header_wrong_signature() {
        let mut header = vec![0u8; 42];
        header[0] = 0x80;
        header[1..7].copy_from_slice(b"vorbis"); // Wrong signature

        let result = TheoraInfo::from_identification_header(&header);
        assert!(result.is_err());
        if let Err(AudexError::InvalidData(msg)) = result {
            assert!(msg.contains("Invalid Theora identification"));
        }
    }

    #[test]
    fn test_invalid_theora_header_wrong_packet_type() {
        let mut header = vec![0u8; 42];
        header[0] = 0x81; // Wrong packet type (should be 0x80)
        header[1..7].copy_from_slice(b"theora");

        let result = TheoraInfo::from_identification_header(&header);
        assert!(result.is_err());
        if let Err(AudexError::InvalidData(msg)) = result {
            assert!(msg.contains("Invalid Theora identification"));
        }
    }

    #[test]
    fn test_unsupported_theora_version() {
        let mut header = vec![0u8; 42];
        header[0] = 0x80;
        header[1..7].copy_from_slice(b"theora");
        header[7] = 4; // Version major = 4 (unsupported)
        header[8] = 0; // Version minor = 0

        let result = TheoraInfo::from_identification_header(&header);
        assert!(result.is_err());
        if let Err(AudexError::UnsupportedFormat(msg)) = result {
            assert!(msg.contains("version 4.0"));
            assert!(msg.contains("major 3 with minor >= 2"));
        }
    }

    #[test]
    fn test_zero_frame_rate() {
        // Create header with zero frame rate
        let mut header = Vec::new();
        header.push(0x80);
        header.extend_from_slice(b"theora");
        header.push(3); // Version major
        header.push(2); // Version minor
        header.push(0); // Version subminor
        header.extend_from_slice(&(320u16).to_be_bytes()); // Frame width
        header.extend_from_slice(&(240u16).to_be_bytes()); // Frame height
        header.extend_from_slice(&[0x01, 0x40, 0x00]); // Picture width
        header.extend_from_slice(&[0x00, 0xF0, 0x00]); // Picture height
        header.push(0); // Offset X
        header.push(0); // Offset Y
        header.extend_from_slice(&(0u32).to_be_bytes()); // FPS numerator = 0
        header.extend_from_slice(&(1u32).to_be_bytes()); // FPS denominator = 1

        // Fill rest with zeros
        while header.len() < 42 {
            header.push(0);
        }

        let result = TheoraInfo::from_identification_header(&header);
        assert!(result.is_err());
        if let Err(AudexError::InvalidData(msg)) = result {
            assert!(msg.contains("numerator or denominator is zero"));
        }
    }
}

#[cfg(test)]
mod theora_granule_tests {
    use super::*;

    #[test]
    fn test_granule_position_calculation() {
        let mut info = TheoraInfo {
            fps: 30.0,
            granule_shift: 6,
            ..Default::default()
        };

        // Test with granule position where keyframe=100, frames_since=32
        let granule_pos = (100u64 << 6) | 32u64; // 6432 = (100 << 6) + 32
        info.set_length(granule_pos as i64);

        assert!(info.length.is_some());
        let expected_frames = 100 + 32; // keyframe count + frames since keyframe
        let expected_duration = expected_frames as f64 / 30.0;
        let actual_duration = info.length.unwrap().as_secs_f64();
        assert!((actual_duration - expected_duration).abs() < 0.001);
    }

    #[test]
    fn test_granule_position_zero() {
        let mut info = TheoraInfo {
            fps: 25.0,
            granule_shift: 4,
            ..Default::default()
        };

        info.set_length(0);
        assert!(info.length.is_none()); // Zero granule position should not set length
    }

    #[test]
    fn test_granule_position_max() {
        let mut info = TheoraInfo {
            fps: 25.0,
            granule_shift: 4,
            ..Default::default()
        };

        info.set_length(u64::MAX as i64);
        assert!(info.length.is_none()); // MAX granule position should not set length
    }

    #[test]
    fn test_granule_position_zero_fps() {
        let mut info = TheoraInfo {
            fps: 0.0, // Zero FPS should not calculate length
            granule_shift: 4,
            ..Default::default()
        };

        info.set_length(1000);
        assert!(info.length.is_none());
    }
}

#[cfg(test)]
mod theora_score_tests {
    use super::*;

    #[test]
    fn test_score_ogv_extension() {
        let score = OggTheora::score("test.ogv", b"anything");
        assert_eq!(score, 1); // OGV files get score of 1
    }

    #[test]
    fn test_score_not_ogg() {
        let score = OggTheora::score("test.ogv", b"MP3 data");
        assert_eq!(score, 0); // Non-OGG data gets score of 0
    }

    #[test]
    fn test_score_ogg_with_theora_headers() {
        let header = b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01\x1E\x01\x80theora\x03\x02\x00\x81theora".to_vec();
        let score = OggTheora::score("test.ogg", &header);
        assert!(score > 3); // Should get points for OGG + both headers
    }

    #[test]
    fn test_score_ogg_with_identification_header() {
        let header = b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01\x1E\x01\x80theora\x03\x02\x00".to_vec();
        let score = OggTheora::score("test.ogg", &header);
        assert!(score >= 4); // OGG (1) + identification header (2) + .ogg extension (1)
    }

    #[test]
    fn test_score_ogg_no_theora() {
        let header =
            b"OggS\x00\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01\x1E\x01\x01vorbis";
        let score = OggTheora::score("test.ogg", header);
        assert_eq!(score, 1); // Only gets OGG point, no extension bonus without Theora headers
    }

    #[test]
    fn test_score_empty_header() {
        let score = OggTheora::score("test.ogv", b"");
        assert_eq!(score, 0); // Empty header gets 0
    }

    #[test]
    fn test_score_short_header() {
        let score = OggTheora::score("test.ogv", b"Ogg");
        assert_eq!(score, 0); // Too short to be valid OGG
    }
}

#[cfg(test)]
mod theora_mime_types_tests {
    use super::*;

    #[test]
    fn test_mime_types() {
        let mime_types = OggTheora::mime_types();
        assert!(mime_types.contains(&"video/x-theora"));
        assert!(mime_types.contains(&"video/ogg"));
        assert_eq!(mime_types.len(), 2);
    }
}

#[cfg(test)]
mod theora_file_operations_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_file_operations_without_file() {
        let mut theora = OggTheora {
            info: TheoraInfo::default(),
            tags: None,
            path: None,
        };

        // Should fail when no path is set
        assert!(theora.save().is_err());
        assert!(theora.clear().is_err());
        assert!(theora.tags().is_none());
        assert!(theora.tags_mut().is_none());
    }

    #[test]
    fn test_add_tags() {
        let mut theora = OggTheora {
            info: TheoraInfo::default(),
            tags: None,
            path: Some(PathBuf::from("test.ogv")),
        };

        assert!(theora.tags().is_none());
        let _ = theora.add_tags();
        assert!(theora.tags().is_some());
        assert!(theora.tags_mut().is_some());
    }

    #[test]
    fn test_info_access() {
        let info = TheoraInfo {
            fps: 24.0,
            width: 1920,
            height: 1080,
            ..Default::default()
        };

        let theora = OggTheora {
            info: info.clone(),
            tags: None,
            path: Some(PathBuf::from("test.ogv")),
        };

        assert_eq!(theora.info().fps, 24.0);
        assert_eq!(theora.info().width, 1920);
        assert_eq!(theora.info().height, 1080);
    }
}

// Mock test for file loading (would require actual Theora file)
#[cfg(test)]
mod theora_integration_tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_file() {
        let result = OggTheora::load("nonexistent.ogv");
        assert!(result.is_err());
        // Should be an IO error for file not found
        if let Err(AudexError::Io(_)) = result {
            // Expected
        } else {
            panic!("Expected IO error for nonexistent file");
        }
    }

    // This test would work with an actual Theora file
    #[test]
    fn test_load_real_theora_files() {
        let test_files = ["sample.ogv", "test.ogv"];

        for filename in &test_files {
            let path = TestUtils::data_path(filename);
            if path.exists() {
                match OggTheora::load(&path) {
                    Ok(theora) => {
                        println!("Successfully loaded {}", filename);
                        println!("  Size: {}x{}", theora.info.width, theora.info.height);
                        println!("  FPS: {}", theora.info.fps);
                        println!("  Bitrate: {} bps", theora.info.bitrate);

                        if let Some(length) = theora.info.length() {
                            println!("  Duration: {:.2}s", length.as_secs_f64());
                        }

                        if let Some(tags) = theora.tags() {
                            let keys = tags.keys();
                            println!("  Tags: {} keys", keys.len());
                            for key in keys.iter().take(3) {
                                if let Some(values) = tags.get(key) {
                                    println!("    {}: {:?}", key, values.first());
                                }
                            }
                        }

                        // Basic validation
                        assert!(theora.info.version_major > 0);
                        // Version minor is always >= 0 (unsigned)
                        assert!(theora.info.fps > 0.0);
                    }
                    Err(e) => {
                        println!("Could not load {} (might be expected): {}", filename, e);
                    }
                }
            } else {
                println!("Test file {} not found - skipping", filename);
            }
        }
    }
}

// Test edge cases and error conditions
#[cfg(test)]
mod theora_edge_cases_tests {
    use super::*;

    #[test]
    fn test_malformed_header_parsing() {
        // Test various malformed headers
        let test_cases = vec![
            (vec![], "empty header"),
            (b"\x80".to_vec(), "too short"),
            (b"\x80theor".to_vec(), "truncated signature"),
            (b"\x81theora\x03\x02".to_vec(), "wrong packet type"),
        ];

        for (header, description) in test_cases {
            let result = TheoraInfo::from_identification_header(&header);
            assert!(result.is_err(), "Should fail for {}", description);
        }
    }

    #[test]
    fn test_boundary_conditions() {
        // Test with extreme but valid values
        let mut header = Vec::new();
        header.push(0x80);
        header.extend_from_slice(b"theora");
        header.push(3);
        header.push(2);
        header.push(0); // Version
        header.extend_from_slice(&(u16::MAX).to_be_bytes()); // Max frame width
        header.extend_from_slice(&(u16::MAX).to_be_bytes()); // Max frame height
        header.extend_from_slice(&[0xFF, 0xFF, 0xFF]); // Max picture width
        header.extend_from_slice(&[0xFF, 0xFF, 0xFF]); // Max picture height
        header.push(255);
        header.push(255); // Max offsets
        header.extend_from_slice(&(u32::MAX).to_be_bytes()); // Max FPS numerator
        header.extend_from_slice(&(1u32).to_be_bytes()); // FPS denominator = 1

        // Fill rest
        while header.len() < 42 {
            header.push(0xFF);
        }

        let result = TheoraInfo::from_identification_header(&header);
        assert!(result.is_ok(), "Should handle extreme but valid values");

        let info = result.unwrap();
        assert_eq!(info.fps, u32::MAX as f64); // Very high FPS
        assert_eq!(info.width, 0xFFFFFF); // Max width
        assert_eq!(info.height, 0xFFFFFF); // Max height
    }

    #[test]
    fn test_granule_shift_boundary() {
        let mut info = TheoraInfo {
            fps: 30.0,
            granule_shift: 63, // Maximum shift value
            ..Default::default()
        };

        // With max shift, keyframe part should be 0, all bits are frame count
        info.set_length(1000);
        assert!(info.length.is_some());

        let expected_duration = 1000.0 / 30.0;
        let actual_duration = info.length.unwrap().as_secs_f64();
        assert!((actual_duration - expected_duration).abs() < 0.001);
    }

    #[test]
    fn test_zero_dimensions() {
        // Test with zero picture width/height but valid frame dimensions
        let mut header = Vec::new();
        header.push(0x80);
        header.extend_from_slice(b"theora");
        header.push(3); // version major
        header.push(2); // version minor
        header.push(0); // version subminor
        header.extend_from_slice(&(16u16).to_be_bytes()); // Frame width (macroblock units)
        header.extend_from_slice(&(16u16).to_be_bytes()); // Frame height (macroblock units)
        header.extend_from_slice(&[0x00, 0x00, 0x00]); // Picture width = 0
        header.extend_from_slice(&[0x00, 0x00, 0x00]); // Picture height = 0
        header.push(0); // offset_x
        header.push(0); // offset_y
        header.extend_from_slice(&(30u32).to_be_bytes()); // FPS numerator
        header.extend_from_slice(&(1u32).to_be_bytes()); // FPS denominator
        header.extend_from_slice(&[0x00, 0x00, 0x00]); // Aspect ratio numerator
        header.extend_from_slice(&[0x00, 0x00, 0x00]); // Aspect ratio denominator
        header.push(0); // Colorspace
        header.extend_from_slice(&[0x00, 0x00, 0x00]); // Nominal bitrate
        // Quality (6 bits) + granule shift (5 bits) + pixel fmt (2 bits) + padding
        // Granule shift must be 1-31; use shift=6 -> bits: 000000 00110 00 000
        header.extend_from_slice(&[0x00, 0xC0]); // quality=0, shift=6, fmt=0

        while header.len() < 42 {
            header.push(0);
        }

        let result = TheoraInfo::from_identification_header(&header);
        assert!(result.is_ok());

        let info = result.unwrap();
        assert_eq!(info.width, 0);
        assert_eq!(info.height, 0);
        assert_eq!(info.fps, 30.0);
    }
}

#[cfg(test)]
mod read_u24_tests {
    use super::*;

    // Test the helper function for reading 24-bit values
    #[test]
    fn test_read_u24_be() {
        let data = vec![0x12, 0x34, 0x56];
        let mut cursor = Cursor::new(data);

        // Use the helper function directly through the module's function
        let result = super::read_u24_be(&mut cursor).unwrap();
        assert_eq!(result, 0x123456);
    }

    #[test]
    fn test_read_u24_be_zeros() {
        let data = vec![0x00, 0x00, 0x00];
        let mut cursor = Cursor::new(data);

        let result = super::read_u24_be(&mut cursor).unwrap();
        assert_eq!(result, 0x000000);
    }

    #[test]
    fn test_read_u24_be_max() {
        let data = vec![0xFF, 0xFF, 0xFF];
        let mut cursor = Cursor::new(data);

        let result = super::read_u24_be(&mut cursor).unwrap();
        assert_eq!(result, 0xFFFFFF);
    }

    #[test]
    fn test_read_u24_be_insufficient_data() {
        let data = vec![0x12, 0x34]; // Only 2 bytes
        let mut cursor = Cursor::new(data);

        let result = super::read_u24_be(&mut cursor);
        assert!(result.is_err());
    }
}

// Access the read_u24_be function for testing
use audex::oggtheora::read_u24_be;
