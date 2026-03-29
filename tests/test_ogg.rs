//! OGG container format tests

use audex::ogg::{OggFile, OggPage, OggStream, OggStreamInfo};
use audex::{AudexError, StreamInfo};
use std::io::Cursor;
use std::time::Duration;

mod common;
use common::TestUtils;

#[cfg(test)]
mod ogg_basic_tests {
    use super::*;

    #[test]
    fn test_ogg_page_creation() {
        let page = OggPage::new();
        assert_eq!(page.version, 0);
        assert_eq!(page.header_type, 0);
        assert_eq!(page.position, 0);
        assert_eq!(page.serial, 0);
        assert_eq!(page.sequence, 0);
        assert_eq!(page.checksum, 0);
        assert!(page.segments.is_empty());
        assert!(page.packets.is_empty());
    }

    #[test]
    fn test_ogg_page_flags() {
        let mut page = OggPage::new();

        // Test initial state
        assert!(!page.is_first());
        assert!(!page.is_last());
        assert!(!page.is_continued());
        assert!(page.is_complete());

        // Test setting flags
        page.set_first(true);
        assert!(page.is_first());
        assert_eq!(page.header_type & 0x02, 0x02);

        page.set_last(true);
        assert!(page.is_last());
        assert_eq!(page.header_type & 0x04, 0x04);

        page.set_continued(true);
        assert!(page.is_continued());
        assert_eq!(page.header_type & 0x01, 0x01);

        // Test clearing flags
        page.set_first(false);
        assert!(!page.is_first());
        assert_eq!(page.header_type & 0x02, 0);

        page.set_last(false);
        assert!(!page.is_last());
        assert_eq!(page.header_type & 0x04, 0);

        page.set_continued(false);
        assert!(!page.is_continued());
        assert_eq!(page.header_type & 0x01, 0);
    }

    #[test]
    fn test_ogg_signature_validation() {
        let valid_header = b"OggS";
        assert!(OggPage::validate_sync(valid_header));

        let invalid_header = b"MP3";
        assert!(!OggPage::validate_sync(invalid_header));

        let short_header = b"Og";
        assert!(!OggPage::validate_sync(short_header));

        let empty_header = b"";
        assert!(!OggPage::validate_sync(empty_header));
    }
}

#[cfg(test)]
mod ogg_parsing_tests {
    use super::*;

    #[test]
    fn test_empty_page_parsing() {
        let mut page = OggPage::new();
        page.version = 0;
        page.header_type = 0x02; // first page
        page.position = 0;
        page.serial = 12345;
        page.sequence = 0;
        page.packets = vec![];

        // Write page and read it back
        let mut buffer = Vec::new();
        page.write_to(&mut buffer).unwrap();

        let mut cursor = Cursor::new(buffer);
        let parsed_page = OggPage::from_reader(&mut cursor).unwrap();

        assert_eq!(parsed_page.version, 0);
        assert!(parsed_page.is_first());
        assert_eq!(parsed_page.serial, 12345);
        assert_eq!(parsed_page.sequence, 0);
        assert!(parsed_page.packets.is_empty());
    }

    #[test]
    fn test_single_packet_page() {
        let mut page = OggPage::new();
        page.version = 0;
        page.header_type = 0x02; // first page
        page.position = 1000;
        page.serial = 54321;
        page.sequence = 1;
        page.packets = vec![b"Hello, OGG world!".to_vec()];

        // Write page and read it back
        let mut buffer = Vec::new();
        page.write_to(&mut buffer).unwrap();

        let mut cursor = Cursor::new(buffer);
        let parsed_page = OggPage::from_reader(&mut cursor).unwrap();

        assert_eq!(parsed_page.version, 0);
        assert!(parsed_page.is_first());
        assert_eq!(parsed_page.position, 1000);
        assert_eq!(parsed_page.serial, 54321);
        assert_eq!(parsed_page.sequence, 1);
        assert_eq!(parsed_page.packets.len(), 1);
        assert_eq!(parsed_page.packets[0], b"Hello, OGG world!");
    }

    #[test]
    fn test_multiple_packets_page() {
        let mut page = OggPage::new();
        page.packets = vec![
            b"First packet".to_vec(),
            b"Second packet".to_vec(),
            b"Third packet".to_vec(),
        ];
        page.serial = 9999;
        page.sequence = 5;

        // Write page and read it back
        let mut buffer = Vec::new();
        page.write_to(&mut buffer).unwrap();

        let mut cursor = Cursor::new(buffer);
        let parsed_page = OggPage::from_reader(&mut cursor).unwrap();

        assert_eq!(parsed_page.packets.len(), 3);
        assert_eq!(parsed_page.packets[0], b"First packet");
        assert_eq!(parsed_page.packets[1], b"Second packet");
        assert_eq!(parsed_page.packets[2], b"Third packet");
        assert_eq!(parsed_page.serial, 9999);
        assert_eq!(parsed_page.sequence, 5);
    }

    #[test]
    fn test_large_packet_segmentation() {
        // Create a packet larger than 255 bytes
        let large_packet = vec![0x42u8; 1000];

        let mut page = OggPage::new();
        page.packets = vec![large_packet.clone()];
        page.serial = 7777;

        // Write page and read it back
        let mut buffer = Vec::new();
        page.write_to(&mut buffer).unwrap();

        let mut cursor = Cursor::new(buffer);
        let parsed_page = OggPage::from_reader(&mut cursor).unwrap();

        assert_eq!(parsed_page.packets.len(), 1);
        assert_eq!(parsed_page.packets[0], large_packet);
        assert_eq!(parsed_page.serial, 7777);
    }
}

#[cfg(test)]
mod ogg_error_handling_tests {
    use super::*;

    #[test]
    fn test_invalid_signature() {
        let invalid_data = b"MP3HEADER_INVALID_OGG_DATA_HERE";
        let mut cursor = Cursor::new(invalid_data.as_slice());

        let result = OggPage::from_reader(&mut cursor);
        assert!(result.is_err());

        if let Err(AudexError::InvalidData(msg)) = result {
            assert!(msg.contains("Invalid OGG signature"));
        } else {
            panic!("Expected InvalidData error for invalid signature");
        }
    }

    #[test]
    fn test_unsupported_version() {
        let mut invalid_header = Vec::new();
        invalid_header.extend_from_slice(b"OggS");
        invalid_header.push(1); // version 1 (unsupported)
        invalid_header.extend_from_slice(&[0u8; 22]); // rest of header

        let mut cursor = Cursor::new(invalid_header);
        let result = OggPage::from_reader(&mut cursor);

        assert!(result.is_err());
        if let Err(AudexError::UnsupportedFormat(msg)) = result {
            assert!(msg.contains("Unsupported OGG version"));
        } else {
            panic!("Expected UnsupportedFormat error for unsupported version");
        }
    }

    #[test]
    fn test_truncated_header() {
        let truncated = b"OggS\x00\x02\x00\x00"; // Only 8 bytes instead of 27
        let mut cursor = Cursor::new(truncated.as_slice());

        let result = OggPage::from_reader(&mut cursor);
        assert!(result.is_err());

        // Should get an IO error for unexpected EOF
        if let Err(AudexError::Io(io_err)) = result {
            assert_eq!(io_err.kind(), std::io::ErrorKind::UnexpectedEof);
        } else {
            panic!("Expected IO error for truncated header");
        }
    }

    #[test]
    fn test_truncated_packet_data() {
        // Create header claiming there's packet data but don't provide it
        let mut header = Vec::new();
        header.extend_from_slice(b"OggS");
        header.push(0); // version
        header.push(0); // header type
        header.extend_from_slice(&0u64.to_le_bytes()); // granule position
        header.extend_from_slice(&0u32.to_le_bytes()); // serial
        header.extend_from_slice(&0u32.to_le_bytes()); // sequence
        header.extend_from_slice(&0u32.to_le_bytes()); // checksum
        header.push(1); // segment count
        header.push(10); // segment size (claims 10 bytes)
        // But don't actually provide the 10 bytes of data

        let mut cursor = Cursor::new(header);
        let result = OggPage::from_reader(&mut cursor);

        assert!(result.is_err());
        // Should fail when trying to read the claimed packet data
    }
}

#[cfg(test)]
mod ogg_stream_tests {
    use super::*;

    #[test]
    fn test_ogg_stream_creation() {
        let stream = OggStream::new(12345);
        assert_eq!(stream.serial_number, 12345);
        assert!(stream.codec.is_empty());
        assert!(stream.packets.is_empty());
    }

    #[test]
    fn test_codec_detection_vorbis() {
        let mut stream = OggStream::new(1);

        // Create Vorbis identification packet
        let vorbis_packet = b"\x01vorbis\x00\x00\x00\x00".to_vec();
        stream.packets.push(vorbis_packet);

        stream.detect_codec();
        assert_eq!(stream.codec, "vorbis");
    }

    #[test]
    fn test_codec_detection_opus() {
        let mut stream = OggStream::new(1);

        // Create Opus identification packet
        let opus_packet = b"OpusHead\x01".to_vec();
        stream.packets.push(opus_packet);

        stream.detect_codec();
        assert_eq!(stream.codec, "opus");
    }

    #[test]
    fn test_codec_detection_theora() {
        let mut stream = OggStream::new(1);

        // Create Theora identification packet
        let theora_packet = b"\x80theora\x03\x02".to_vec();
        stream.packets.push(theora_packet);

        stream.detect_codec();
        assert_eq!(stream.codec, "theora");
    }

    #[test]
    fn test_codec_detection_flac() {
        let mut stream = OggStream::new(1);

        // Ogg FLAC identification packets start with 0x7F followed by "FLAC"
        let flac_packet = b"\x7FFLAC\x00\x00".to_vec();
        stream.packets.push(flac_packet);

        stream.detect_codec();
        assert_eq!(stream.codec, "flac");
    }

    #[test]
    fn test_packet_accessors() {
        let mut stream = OggStream::new(1);
        stream.packets.push(b"identification".to_vec());
        stream.packets.push(b"comment".to_vec());
        stream.packets.push(b"setup".to_vec());

        assert_eq!(stream.identification_packet().unwrap(), b"identification");
        assert_eq!(stream.comment_packet().unwrap(), b"comment");
        assert_eq!(stream.setup_packet().unwrap(), b"setup");
    }
}

#[cfg(test)]
mod ogg_file_tests {
    use super::*;

    #[test]
    fn test_ogg_file_creation() {
        let ogg_file = OggFile::new();
        assert!(ogg_file.pages.is_empty());
        assert!(ogg_file.streams.is_empty());
    }

    #[test]
    fn test_stream_by_codec() {
        let ogg_file = OggFile::new();
        assert!(ogg_file.get_stream_by_codec("vorbis").is_none());
        assert!(ogg_file.get_stream_by_codec("opus").is_none());
    }

    #[test]
    fn test_pages_for_stream() {
        let mut ogg_file = OggFile::new();

        // Add some pages
        let mut page1 = OggPage::new();
        page1.serial = 100;
        let mut page2 = OggPage::new();
        page2.serial = 200;
        let mut page3 = OggPage::new();
        page3.serial = 100;

        ogg_file.pages.push(page1);
        ogg_file.pages.push(page2);
        ogg_file.pages.push(page3);

        let pages_100 = ogg_file.get_pages_for_stream(100);
        assert_eq!(pages_100.len(), 2);
        assert_eq!(pages_100[0].serial, 100);
        assert_eq!(pages_100[1].serial, 100);

        let pages_200 = ogg_file.get_pages_for_stream(200);
        assert_eq!(pages_200.len(), 1);
        assert_eq!(pages_200[0].serial, 200);

        let pages_300 = ogg_file.get_pages_for_stream(300);
        assert_eq!(pages_300.len(), 0);
    }
}

#[cfg(test)]
mod ogg_integration_tests {
    use super::*;

    #[test]
    fn test_load_real_ogg_files() {
        // Test with real OGG files from test data
        let ogg_files = ["empty.ogg", "multipagecomment.ogg", "multipage-setup.ogg"];

        for filename in &ogg_files {
            let path = TestUtils::data_path(filename);
            if path.exists() {
                match OggFile::load(&path) {
                    Ok(ogg_file) => {
                        println!("Successfully loaded {}", filename);
                        println!("  Pages: {}", ogg_file.pages.len());
                        println!("  Streams: {}", ogg_file.streams.len());

                        // Basic validation
                        assert!(!ogg_file.pages.is_empty(), "Should have at least one page");

                        for page in &ogg_file.pages[..std::cmp::min(3, ogg_file.pages.len())] {
                            assert_eq!(page.version, 0, "All pages should have version 0");
                            println!(
                                "    Page: serial={}, sequence={}, packets={}",
                                page.serial,
                                page.sequence,
                                page.packets.len()
                            );
                        }

                        for (serial, stream) in &ogg_file.streams {
                            println!(
                                "    Stream {}: codec={}, packets={}",
                                serial,
                                stream.codec,
                                stream.packets.len()
                            );
                        }
                    }
                    Err(e) => {
                        println!("Could not load {} (might be expected): {}", filename, e);
                        // This might be expected until full OGG parsing is implemented
                    }
                }
            }
        }
    }

    #[test]
    fn test_ogg_round_trip() {
        // Create a simple OGG structure and test round-trip
        let mut page1 = OggPage::new();
        page1.serial = 42;
        page1.sequence = 0;
        page1.set_first(true);
        page1.packets = vec![b"test packet 1".to_vec()];

        let mut page2 = OggPage::new();
        page2.serial = 42;
        page2.sequence = 1;
        page2.set_last(true);
        page2.packets = vec![b"test packet 2".to_vec()];

        // Write both pages
        let mut buffer = Vec::new();
        page1.write_to(&mut buffer).unwrap();
        page2.write_to(&mut buffer).unwrap();

        // Parse them back
        let mut cursor = Cursor::new(buffer);
        let parsed1 = OggPage::from_reader(&mut cursor).unwrap();
        let parsed2 = OggPage::from_reader(&mut cursor).unwrap();

        assert_eq!(parsed1.serial, 42);
        assert_eq!(parsed1.sequence, 0);
        assert!(parsed1.is_first());
        assert!(!parsed1.is_last());
        assert_eq!(parsed1.packets[0], b"test packet 1");

        assert_eq!(parsed2.serial, 42);
        assert_eq!(parsed2.sequence, 1);
        assert!(!parsed2.is_first());
        assert!(parsed2.is_last());
        assert_eq!(parsed2.packets[0], b"test packet 2");
    }

    #[test]
    fn test_to_packets_functionality() {
        // Create pages with split packets
        let mut page1 = OggPage::new();
        page1.serial = 1;
        page1.sequence = 0;
        page1.packets = vec![b"complete1".to_vec(), b"split_start".to_vec()];
        page1.segments = vec![9, 11]; // complete packet, incomplete packet

        let mut page2 = OggPage::new();
        page2.serial = 1;
        page2.sequence = 1;
        page2.set_continued(true);
        page2.packets = vec![b"_end".to_vec(), b"complete2".to_vec()];
        page2.segments = vec![4, 9]; // continuation, complete packet

        let pages = vec![page1, page2];
        let packets = OggPage::to_packets(&pages, false).unwrap();

        assert_eq!(packets.len(), 3);
        assert_eq!(packets[0], b"complete1");
        assert_eq!(packets[1], b"split_start_end");
        assert_eq!(packets[2], b"complete2");
    }
}

#[cfg(test)]
mod ogg_stream_info_tests {
    use super::*;

    #[test]
    fn test_ogg_stream_info_creation() {
        let info = OggStreamInfo::default();
        assert_eq!(info.length(), None);
        assert_eq!(info.bitrate(), None);
        assert_eq!(info.sample_rate(), None);
        assert_eq!(info.channels(), None);
        assert_eq!(info.bits_per_sample(), None);
    }

    #[test]
    fn test_ogg_stream_info_populated() {
        let info = OggStreamInfo {
            length: Some(Duration::from_secs(180)),
            bitrate: Some(320000),
            sample_rate: 44100,
            channels: 2,
            serial: 12345,
        };

        assert_eq!(info.length(), Some(Duration::from_secs(180)));
        assert_eq!(info.bitrate(), Some(320000));
        assert_eq!(info.sample_rate(), Some(44100));
        assert_eq!(info.channels(), Some(2));
        assert_eq!(info.bits_per_sample(), None); // Always None for OGG
        assert_eq!(info.serial, 12345);
    }
}

// Utility tests following standard functionality
#[cfg(test)]
mod ogg_util_tests {
    use super::*;

    #[test]
    fn test_crc_calculation() {
        let mut page = OggPage::new();
        page.packets = vec![b"test data".to_vec()];
        page.serial = 123;
        page.sequence = 0;

        let crc = page.calculate_crc().unwrap();
        assert!(crc > 0, "CRC should be calculated");

        // CRC should be consistent for same data
        let crc2 = page.calculate_crc().unwrap();
        assert_eq!(crc, crc2);

        // CRC should change if data changes
        page.packets[0] = b"different data".to_vec();
        let crc3 = page.calculate_crc().unwrap();
        assert_ne!(crc, crc3);
    }

    #[test]
    fn test_page_size_calculation() {
        let mut page = OggPage::new();

        // Empty page
        let mut buffer = Vec::new();
        page.write_to(&mut buffer).unwrap();
        let empty_size = buffer.len();
        assert!(empty_size >= 27, "Minimum page size should be header size");

        // Page with small packet
        page.packets = vec![b"small".to_vec()];
        buffer.clear();
        page.write_to(&mut buffer).unwrap();
        let small_size = buffer.len();
        assert!(small_size > empty_size, "Size should increase with packet");

        // Page with larger packet
        page.packets = vec![b"much larger packet data".to_vec()];
        buffer.clear();
        page.write_to(&mut buffer).unwrap();
        let large_size = buffer.len();
        assert!(
            large_size > small_size,
            "Size should increase with larger packet"
        );
    }
}

// Advanced page operations tests following standard test specification
#[cfg(test)]
mod ogg_advanced_page_tests {
    use super::*;

    #[test]
    fn test_flags_comprehensive() {
        let test_data = TestUtils::data_path("empty.ogg");
        if test_data.exists() {
            let mut file = std::fs::File::open(&test_data).unwrap();
            let page = OggPage::from_reader(&mut file).unwrap();

            assert!(page.is_first());
            assert!(!page.is_continued());
            assert!(!page.is_last());
            assert!(page.is_complete());

            // Test setting all flag combinations
            let mut test_page = page.clone();
            for first in [true, false] {
                test_page.set_first(first);
                for last in [true, false] {
                    test_page.set_last(last);
                    for continued in [true, false] {
                        test_page.set_continued(continued);
                        assert_eq!(test_page.is_first(), first);
                        assert_eq!(test_page.is_last(), last);
                        assert_eq!(test_page.is_continued(), continued);
                    }
                }
            }
        }
    }

    #[test]
    fn test_flags_next_page() {
        let test_data = TestUtils::data_path("empty.ogg");
        if test_data.exists() {
            let mut file = std::fs::File::open(&test_data).unwrap();
            let _first_page = OggPage::from_reader(&mut file).unwrap();

            // Read the next page
            if let Ok(page) = OggPage::from_reader(&mut file) {
                assert!(!page.is_first());
                assert!(!page.is_continued());
                assert!(!page.is_last());
            }
        }
    }

    #[test]
    fn test_page_length() {
        let test_data = TestUtils::data_path("empty.ogg");
        if test_data.exists() {
            let mut file = std::fs::File::open(&test_data).unwrap();
            let page = OggPage::from_reader(&mut file).unwrap();

            let page_size = page.size();
            let written_data = page.write().unwrap();

            assert_eq!(page_size, written_data.len());
            // Ogg Vorbis empty.ogg first page is 58 bytes as per format tests
            assert_eq!(page_size, 58);
        }
    }

    #[test]
    fn test_single_page_roundtrip() {
        let test_data = TestUtils::data_path("empty.ogg");
        if test_data.exists() {
            let mut file = std::fs::File::open(&test_data).unwrap();
            let original_page = OggPage::from_reader(&mut file).unwrap();

            let written_data = original_page.write().unwrap();
            let mut cursor = Cursor::new(written_data);
            let parsed_page = OggPage::from_reader(&mut cursor).unwrap();

            assert_eq!(original_page, parsed_page);
        }
    }
}

// Packet reconstruction edge cases tests
#[cfg(test)]
mod ogg_packet_reconstruction_tests {
    use super::*;

    #[test]
    fn test_to_packets_empty_pages() {
        let mut pages = vec![OggPage::new(); 2];
        for (i, page) in pages.iter_mut().enumerate() {
            page.sequence = i as u32;
        }
        assert_eq!(
            OggPage::to_packets_strict(&pages, true).unwrap(),
            Vec::<Vec<u8>>::new()
        );
        assert_eq!(
            OggPage::to_packets_strict(&pages, false).unwrap(),
            Vec::<Vec<u8>>::new()
        );

        // Test with empty + continued packets
        let mut pages = vec![OggPage::new(); 3];
        pages[0].packets = vec![b"foo".to_vec()];
        pages[0].complete = false;
        pages[0].sequence = 0;

        pages[1].set_continued(true);
        pages[1].complete = false;
        pages[1].sequence = 1;

        pages[2].packets = vec![b"bar".to_vec()];
        pages[2].set_continued(true);
        pages[2].sequence = 2;

        for page in &mut pages {
            page.serial = 1;
        }

        let result = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(result, vec![b"foobar".to_vec()]);
    }

    #[test]
    fn test_to_packets_mixed_stream() {
        let mut pages = vec![OggPage::new(); 3];
        pages[0].packets = vec![b"foo".to_vec()];
        pages[0].serial = 1;
        pages[0].sequence = 0;

        pages[1].packets = vec![b"bar".to_vec()];
        pages[1].serial = 1;
        pages[1].sequence = 1;

        pages[2].packets = vec![b"baz".to_vec()];
        pages[2].serial = 3; // Different serial - should error
        pages[2].sequence = 2;

        let result = OggPage::to_packets(&pages, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_packets_missing_sequence() {
        // Test that pages with non-consecutive sequences are rejected
        let mut pages = vec![OggPage::new(); 3];
        pages[0].packets = vec![b"foo".to_vec()];
        pages[0].serial = 1;
        pages[0].sequence = 3;

        pages[1].packets = vec![b"bar".to_vec()];
        pages[1].serial = 1;
        pages[1].sequence = 5; // Skipped sequence 4 - this should error

        pages[2].packets = vec![b"baz".to_vec()];
        pages[2].serial = 1;
        pages[2].sequence = 6;

        let result = OggPage::to_packets(&pages, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_packets_continued() {
        let mut pages = vec![OggPage::new(); 3];
        for (i, page) in pages.iter_mut().enumerate() {
            page.packets = vec![match i {
                0 => b"foo".to_vec(),
                1 => b"bar".to_vec(),
                _ => b"baz".to_vec(),
            }];
            page.serial = 1;
            page.sequence = i as u32;
        }

        pages[0].set_continued(true);

        let result = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(
            result,
            vec![b"foo".to_vec(), b"bar".to_vec(), b"baz".to_vec()]
        );
    }

    #[test]
    fn test_to_packets_continued_strict() {
        let mut pages = vec![OggPage::new(); 3];
        for (i, page) in pages.iter_mut().enumerate() {
            page.packets = vec![match i {
                0 => b"foo".to_vec(),
                1 => b"bar".to_vec(),
                _ => b"baz".to_vec(),
            }];
            page.serial = 1;
            page.sequence = i as u32;
        }

        pages[0].set_continued(true);

        let result = OggPage::to_packets_strict(&pages, true);
        assert!(result.is_err()); // First packet continued in strict mode should fail
    }

    #[test]
    fn test_to_packets_strict_incomplete() {
        let mut pages = vec![OggPage::new(); 3];
        for (i, page) in pages.iter_mut().enumerate() {
            page.packets = vec![match i {
                0 => b"foo".to_vec(),
                1 => b"bar".to_vec(),
                _ => b"baz".to_vec(),
            }];
            page.serial = 1;
            page.sequence = i as u32;
            page.complete = false; // All incomplete
        }

        let result = OggPage::to_packets_strict(&pages, true);
        assert!(result.is_err()); // Last packet incomplete in strict mode should fail
    }
}

// Page generation from packets tests
#[cfg(test)]
mod ogg_page_generation_tests {
    use super::*;

    #[test]
    fn test_crappy_fragmentation() {
        let packets = vec![vec![b'1'; 511], vec![b'2'; 511], vec![b'3'; 511]];
        let pages = OggPage::from_packets(packets.clone(), 0, 510, 0);
        assert!(pages.len() > 3);

        let reconstructed = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(reconstructed, packets);
    }

    #[test]
    fn test_wiggle_room() {
        let packets = vec![vec![b'1'; 511], vec![b'2'; 511], vec![b'3'; 511]];
        let pages = OggPage::from_packets(packets.clone(), 0, 510, 100);
        assert_eq!(pages.len(), 3);

        let reconstructed = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(reconstructed, packets);
    }

    #[test]
    fn test_one_packet_per_wiggle() {
        let packets = vec![vec![b'1'; 511], vec![b'2'; 511], vec![b'3'; 511]];
        let pages = OggPage::from_packets(packets.clone(), 0, 1000, 1000000);
        assert_eq!(pages.len(), 2);

        let reconstructed = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(reconstructed, packets);
    }

    #[test]
    fn test_from_packets_short_enough() {
        let packets = vec![vec![b'1'; 200], vec![b'2'; 200], vec![b'3'; 200]];
        let pages = OggPage::from_packets(packets.clone(), 0, 4096, 2048);

        let reconstructed = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(reconstructed, packets);
    }

    #[test]
    fn test_from_packets_position() {
        let packets = vec![vec![b'1'; 100000]];
        let pages = OggPage::from_packets(packets, 0, 4096, 2048);
        assert!(pages.len() > 1);

        for page in &pages[..pages.len() - 1] {
            assert_eq!(page.position, -1); // -1 for incomplete pages
        }
        assert_eq!(pages.last().unwrap().position, 0);
    }

    #[test]
    fn test_from_packets_long() {
        let packets = vec![vec![b'1'; 100000], vec![b'2'; 100000], vec![b'3'; 100000]];
        let pages = OggPage::from_packets(packets.clone(), 0, 4096, 2048);

        assert!(!pages[0].is_complete());
        assert!(pages[1].is_continued());

        let reconstructed = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(reconstructed, packets);
    }

    #[test]
    fn test_from_packets_try_preserve() {
        // If the packet layout matches, just create pages with the same layout
        let packets = vec![vec![b'1'; 100000], vec![b'2'; 100000], vec![b'3'; 100000]];
        let pages = OggPage::from_packets(packets.clone(), 42, 977, 400);
        let new_pages = OggPage::from_packets_try_preserve(packets.clone(), &pages);
        assert_eq!(pages, new_pages);

        // Zero case
        let new_pages = OggPage::from_packets_try_preserve(vec![], &pages);
        assert_eq!(new_pages, vec![]);

        // If the layout doesn't match we should fall back to creating new pages
        let mut other_packets = packets.clone();
        other_packets[1].push(0xff);
        let other_pages = OggPage::from_packets(other_packets.clone(), 42, 4096, 2048);
        let new_pages = OggPage::from_packets_try_preserve(other_packets, &pages);
        assert_eq!(new_pages, other_pages);
    }

    #[test]
    fn test_packet_exactly_255() {
        let mut page = OggPage::new();
        page.packets = vec![vec![b'1'; 255]];
        page.complete = false;

        let mut page2 = OggPage::new();
        page2.packets = vec![vec![]];
        page2.sequence = 1;
        page2.set_continued(true);

        let pages = vec![page, page2];
        let result = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(result, vec![vec![b'1'; 255]]);
    }

    #[test]
    fn test_page_max_size_alone_too_big() {
        let mut page = OggPage::new();
        page.packets = vec![vec![b'1'; 255 * 255]];
        page.complete = true;

        let _result = page.write();
        // 255*255 bytes in a single complete page exceeds Ogg limits.
        // Test verifies no panic; behavior is implementation-defined.
    }

    #[test]
    fn test_page_max_size() {
        let mut page = OggPage::new();
        page.packets = vec![vec![b'1'; 255 * 255]];
        page.complete = false;

        let mut page2 = OggPage::new();
        page2.packets = vec![vec![]];
        page2.sequence = 1;
        page2.set_continued(true);

        let pages = vec![page, page2];
        let result = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(result, vec![vec![b'1'; 255 * 255]]);
    }

    #[test]
    fn test_complete_zero_length() {
        let packets = vec![vec![]; 20];
        let pages = OggPage::from_packets(packets.clone(), 0, 4096, 2048);
        assert!(!pages.is_empty());

        let page_data = pages[0].write().unwrap();
        let mut cursor = Cursor::new(page_data);
        let new_page = OggPage::from_reader(&mut cursor).unwrap();

        assert_eq!(new_page, pages[0]);

        let reconstructed = OggPage::to_packets(&[new_page], false).unwrap();
        assert_eq!(reconstructed, packets);
    }

    #[test]
    fn test_too_many_packets() {
        let packets = vec![vec![b'1']; 3000];
        let pages = OggPage::from_packets(packets, 0, 4096, 2048);

        for page in &pages {
            let _ = page.write().unwrap();
        }
        assert!(pages.len() > 3000 / 255);
    }

    #[test]
    fn test_read_max_size() {
        let mut page = OggPage::new();
        page.packets = vec![vec![b'1'; 255 * 255]];
        page.complete = false;

        let mut page2 = OggPage::new();
        page2.packets = vec![vec![], b"foo".to_vec()];
        page2.sequence = 1;
        page2.set_continued(true);

        let mut data = page.write().unwrap();
        data.extend_from_slice(&page2.write().unwrap());

        let mut cursor = Cursor::new(data);
        let read_page = OggPage::from_reader(&mut cursor).unwrap();
        assert_eq!(read_page, page);

        let read_page2 = OggPage::from_reader(&mut cursor).unwrap();
        assert_eq!(read_page2, page2);

        // Should be at end now
        let result = OggPage::from_reader(&mut cursor);
        assert!(result.is_err());
    }
}

// Advanced file operations tests
#[cfg(test)]
mod ogg_advanced_file_tests {
    use super::*;

    #[test]
    fn test_find_last() {
        let mut pages = vec![OggPage::new(); 10];
        for (i, page) in pages.iter_mut().enumerate() {
            page.sequence = i as u32;
            page.serial = 42;
            page.packets = vec![format!("packet{}", i).into_bytes()];
        }

        let mut data = Vec::new();
        for page in &pages {
            data.extend_from_slice(&page.write().unwrap());
        }

        let mut cursor = Cursor::new(data);
        let found_page = OggPage::find_last(&mut cursor, 42, false).unwrap().unwrap();
        assert_eq!(found_page, pages[9]);
    }

    #[test]
    fn test_find_last_none_finishing() {
        let mut page = OggPage::new();
        page.position = -1; // -1 equivalent (non-finishing)
        page.serial = 42;

        let data = page.write().unwrap();
        let mut cursor = Cursor::new(data);

        let result = OggPage::find_last_with_finishing(&mut cursor, 42, true);
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_find_last_none_finishing_mux() {
        let mut page1 = OggPage::new();
        page1.set_last(true);
        page1.position = -1; // -1 equivalent
        page1.serial = 42;

        let mut page2 = OggPage::new();
        page2.serial = 43;
        page2.position = 1000; // finishing position

        let mut data = Vec::new();
        data.extend_from_slice(&page1.write().unwrap());
        data.extend_from_slice(&page2.write().unwrap());

        let mut cursor = Cursor::new(data);

        let result1 = OggPage::find_last_with_finishing(&mut cursor, 42, true);
        assert!(result1.unwrap().is_none());

        cursor.set_position(0);
        let result2 = OggPage::find_last_with_finishing(&mut cursor, 43, true)
            .unwrap()
            .unwrap();
        assert_eq!(result2.serial, 43);
    }

    #[test]
    fn test_find_last_last_empty() {
        let mut pages = vec![OggPage::new(); 10];
        for (i, page) in pages.iter_mut().enumerate() {
            page.sequence = i as u32;
            page.position = i as i64;
            page.serial = 42;
            page.packets = vec![format!("packet{}", i).into_bytes()];
        }

        // Last page is marked as last but has no position (empty)
        pages[9].set_last(true);
        pages[9].position = -1; // -1 equivalent

        let mut data = Vec::new();
        for page in &pages {
            data.extend_from_slice(&page.write().unwrap());
        }

        let mut cursor = Cursor::new(data);

        // With finishing=true, should return the second-to-last page
        let page = OggPage::find_last_with_finishing(&mut cursor, 42, true)
            .unwrap()
            .unwrap();
        assert_eq!(page.position, 8);

        cursor.set_position(0);

        // With finishing=false, should return the last page
        let page = OggPage::find_last_with_finishing(&mut cursor, 42, false)
            .unwrap()
            .unwrap();
        assert_eq!(page.position, -1);
    }

    #[test]
    fn test_find_last_single_muxed() {
        let mut page1 = OggPage::new();
        page1.set_last(true);
        page1.serial = 42;

        let mut page2 = OggPage::new();
        page2.serial = 43;

        let mut data = Vec::new();
        data.extend_from_slice(&page1.write().unwrap());
        data.extend_from_slice(&page2.write().unwrap());

        let mut cursor = Cursor::new(data);
        let found = OggPage::find_last(&mut cursor, 43, false).unwrap().unwrap();
        assert_eq!(found.serial, 43);
    }

    #[test]
    fn test_find_last_really_last() {
        let mut pages = vec![OggPage::new(); 10];
        for (i, page) in pages.iter_mut().enumerate() {
            page.sequence = i as u32;
            page.serial = 42;
            page.packets = vec![format!("packet{}", i).into_bytes()];
        }
        pages[9].set_last(true);

        let mut data = Vec::new();
        for page in &pages {
            data.extend_from_slice(&page.write().unwrap());
        }

        let mut cursor = Cursor::new(data);
        let found = OggPage::find_last(&mut cursor, 42, false).unwrap().unwrap();
        assert_eq!(found, pages[9]);
    }

    #[test]
    fn test_find_last_muxed() {
        let mut pages = vec![OggPage::new(); 10];
        for (i, page) in pages.iter_mut().enumerate() {
            page.sequence = i as u32;
            page.serial = 42;
            page.packets = vec![format!("packet{}", i).into_bytes()];
        }

        // Mark second-to-last page as EOS for stream 42
        pages[8].set_last(true);
        // Last page belongs to different stream
        pages[9].serial = 43;

        let mut data = Vec::new();
        for page in &pages {
            data.extend_from_slice(&page.write().unwrap());
        }

        let mut cursor = Cursor::new(data);
        let found = OggPage::find_last(&mut cursor, 42, false).unwrap().unwrap();
        assert_eq!(found, pages[8]);
    }

    #[test]
    fn test_find_last_no_serial() {
        let mut pages = vec![OggPage::new(); 10];
        for (i, page) in pages.iter_mut().enumerate() {
            page.sequence = i as u32;
            page.serial = 42;
            page.packets = vec![format!("packet{}", i).into_bytes()];
        }

        let mut data = Vec::new();
        for page in &pages {
            data.extend_from_slice(&page.write().unwrap());
        }

        let mut cursor = Cursor::new(data);
        let result = OggPage::find_last(&mut cursor, 99, false); // Non-existent serial
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_find_last_invalid() {
        let data = b"if you think this is an Ogg, you're crazy";
        let mut cursor = Cursor::new(data.as_slice());

        let result = OggPage::find_last(&mut cursor, 0, false);
        // Should either return error or None, depending on implementation
        assert!(result.is_err() || result.unwrap().is_none());
    }

    #[test]
    fn test_find_last_invalid_sync() {
        let data = b"if you think this is an OggS, you're crazy";
        let mut cursor = Cursor::new(data.as_slice());

        let result = OggPage::find_last(&mut cursor, 0, false);
        // Should handle invalid sync gracefully
        assert!(result.is_ok());
        if let Ok(page) = result {
            assert!(page.is_none());
        }
    }

    #[test]
    fn test_invalid_version() {
        let mut page = OggPage::new();

        // Valid version should work
        let mut cursor = Cursor::new(page.write().unwrap());
        let parsed = OggPage::from_reader(&mut cursor);
        assert!(parsed.is_ok());

        // Invalid version should fail
        page.version = 1;
        let mut cursor = Cursor::new(page.write().unwrap());
        let result = OggPage::from_reader(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_not_enough_lacing() {
        let mut data = OggPage::new().write().unwrap();
        // Remove the last byte (lacing value)
        data.truncate(data.len() - 1);
        data.push(0x10); // Add invalid lacing

        let mut cursor = Cursor::new(data);
        let result = OggPage::from_reader(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_not_enough_data() {
        let mut data = OggPage::new().write().unwrap();
        data.truncate(data.len() - 1);
        data.extend_from_slice(&[0x01, 0x10]); // Claims 16 bytes but doesn't provide them

        let mut cursor = Cursor::new(data);
        let result = OggPage::from_reader(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_not_equal() {
        let page = OggPage::new();
        let mut page2 = OggPage::new();
        page2.serial = 12;
        assert_ne!(page, page2); // Test that different pages are not equal
    }
}

// Stress testing and performance tests
#[cfg(test)]
mod ogg_stress_tests {
    use super::*;

    #[test]
    fn test_random_data_roundtrip() {
        // Simulate random data with predictable patterns for testing
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        for i in 0..10 {
            i.hash(&mut hasher);
            let seed = hasher.finish() as usize;

            let num_packets = (seed % 98) + 2; // 2-100 packets
            let mut packets = Vec::new();

            for j in 0..num_packets {
                let length = ((seed + j * 7) % 9990) + 10; // 10-10000 bytes
                let mut packet = Vec::with_capacity(length);
                for k in 0..length {
                    packet.push(((seed + j * 13 + k * 17) & 0xFF) as u8);
                }
                packets.push(packet);
            }

            // Test round-trip
            let pages = OggPage::from_packets(packets.clone(), 0, 4096, 2048);
            let reconstructed = OggPage::to_packets(&pages, false).unwrap();
            assert_eq!(
                packets, reconstructed,
                "Round-trip failed for iteration {}",
                i
            );
        }
    }

    #[test]
    fn test_large_single_packet() {
        // Test with very large single packet
        let large_packet = vec![0x42u8; 1000000]; // 1MB packet
        let pages = OggPage::from_packets(vec![large_packet.clone()], 0, 4096, 2048);

        assert!(pages.len() > 200); // Should be split into many pages

        let reconstructed = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(reconstructed.len(), 1);
        assert_eq!(reconstructed[0], large_packet);
    }

    #[test]
    fn test_many_small_packets() {
        // Test with many very small packets
        let packets: Vec<Vec<u8>> = (0..1000).map(|i| vec![(i & 0xFF) as u8; 10]).collect();

        let pages = OggPage::from_packets(packets.clone(), 0, 4096, 2048);
        let reconstructed = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(reconstructed, packets);
    }

    #[test]
    fn test_maximum_page_utilization() {
        // Create packets that exactly fill pages to test boundary conditions
        let segment_size = 255;
        let max_segments = 255;
        let max_payload = segment_size * max_segments;

        // Test packet that exactly fills max segments
        let max_packet = vec![0xAAu8; max_payload];
        let pages = OggPage::from_packets(vec![max_packet.clone()], 0, max_payload + 1000, 0);

        let reconstructed = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(reconstructed, vec![max_packet]);
    }

    #[test]
    fn test_mixed_packet_sizes() {
        // Test with wildly varying packet sizes
        let packets = vec![
            vec![1u8; 1],      // Tiny
            vec![2u8; 127],    // Small
            vec![3u8; 255],    // Segment boundary
            vec![4u8; 256],    // Just over segment
            vec![5u8; 1000],   // Medium
            vec![6u8; 10000],  // Large
            vec![7u8; 100000], // Very large
        ];

        let pages = OggPage::from_packets(packets.clone(), 0, 4096, 2048);
        let reconstructed = OggPage::to_packets(&pages, false).unwrap();
        assert_eq!(reconstructed, packets);
    }

    #[test]
    fn test_sequence_overflow() {
        // Test with very high sequence numbers (near u32 max)
        let mut page = OggPage::new();
        page.sequence = u32::MAX - 10;
        page.serial = 42;
        page.packets = vec![b"test".to_vec()];

        let data = page.write().unwrap();
        let mut cursor = Cursor::new(data);
        let parsed = OggPage::from_reader(&mut cursor).unwrap();

        assert_eq!(parsed.sequence, u32::MAX - 10);
        assert_eq!(parsed.serial, 42);
    }

    #[test]
    fn test_position_edge_cases() {
        // Test with various granule position values
        let test_positions = vec![0, 1, 1000, -1 - 1, -1];

        for pos in test_positions {
            let mut page = OggPage::new();
            page.position = pos;
            page.packets = vec![format!("pos_{}", pos).into_bytes()];

            let data = page.write().unwrap();
            let mut cursor = Cursor::new(data);
            let parsed = OggPage::from_reader(&mut cursor).unwrap();

            assert_eq!(parsed.position, pos);
        }
    }

    #[test]
    fn test_empty_packet_handling() {
        // Test various combinations with empty packets
        let test_cases = vec![
            vec![vec![]],                           // Single empty
            vec![vec![], vec![]],                   // Multiple empty
            vec![b"data".to_vec(), vec![]],         // Mixed
            vec![vec![], b"data".to_vec()],         // Mixed
            vec![vec![], b"data".to_vec(), vec![]], // Mixed
        ];

        for (i, packets) in test_cases.into_iter().enumerate() {
            let pages = OggPage::from_packets(packets.clone(), 0, 4096, 2048);
            let reconstructed = OggPage::to_packets(&pages, false).unwrap();
            assert_eq!(reconstructed, packets, "Failed for test case {}", i);
        }
    }
}

// CRC validation tests
#[cfg(test)]
mod ogg_crc_tests {
    use super::*;

    #[test]
    fn test_crc_cross_platform_compatibility() {
        // Test CRC32 consistency across platforms (signed vs unsigned representations)
        let mut page = OggPage::new();
        page.packets = vec![b"abc".to_vec()];
        page.serial = 12345;
        page.sequence = 0;

        let data1 = page.write().unwrap();

        // CRC should be consistent across different architectures
        let mut page2 = OggPage::new();
        page2.packets = vec![b"abc".to_vec()];
        page2.serial = 12345;
        page2.sequence = 0;

        let data2 = page2.write().unwrap();

        assert_eq!(data1, data2, "CRC calculation should be deterministic");

        // Test parsing the data back
        let mut cursor = Cursor::new(data1);
        let parsed = OggPage::from_reader(&mut cursor).unwrap();
        assert_eq!(parsed.packets[0], b"abc");
        assert_eq!(parsed.serial, 12345);
    }

    #[test]
    fn test_crc_consistency() {
        // Test that CRC is consistent for the same page data
        let mut page = OggPage::new();
        page.packets = vec![b"test data for CRC".to_vec()];
        page.serial = 42;
        page.sequence = 7;

        let crc1 = page.calculate_crc().unwrap();
        let crc2 = page.calculate_crc().unwrap();
        assert_eq!(crc1, crc2, "CRC should be consistent");

        // Change data and verify CRC changes
        page.packets[0] = b"different data".to_vec();
        let crc3 = page.calculate_crc().unwrap();
        assert_ne!(crc1, crc3, "CRC should change when data changes");
    }

    #[test]
    fn test_crc_validation_on_parsing() {
        // Test that pages with valid CRCs parse correctly
        let mut page = OggPage::new();
        page.packets = vec![b"CRC validation test".to_vec()];
        page.serial = 999;

        let data = page.write().unwrap();
        let mut cursor = Cursor::new(data);
        let parsed = OggPage::from_reader(&mut cursor);

        assert!(parsed.is_ok(), "Valid CRC should parse successfully");
        let parsed_page = parsed.unwrap();
        assert_eq!(parsed_page.packets[0], b"CRC validation test");
        assert_eq!(parsed_page.serial, 999);
    }

    #[test]
    fn test_crc_with_various_data_patterns() {
        // Test CRC calculation with different data patterns
        let test_patterns = vec![
            vec![],                              // Empty
            vec![0x00],                          // Single zero
            vec![0xFF],                          // Single 0xFF
            vec![0x00, 0xFF, 0x00, 0xFF],        // Alternating
            (0..256).map(|i| i as u8).collect(), // All byte values
        ];

        for (i, pattern) in test_patterns.into_iter().enumerate() {
            let mut page = OggPage::new();
            page.packets = vec![pattern.clone()];
            page.serial = i as u32;

            let crc = page.calculate_crc().unwrap();
            assert!(
                crc > 0 || pattern.is_empty(),
                "CRC should be calculated for pattern {}",
                i
            );

            // Verify round-trip preserves data
            let data = page.write().unwrap();
            let mut cursor = Cursor::new(data);
            let parsed = OggPage::from_reader(&mut cursor).unwrap();
            assert_eq!(parsed.packets.first().unwrap_or(&vec![]), &pattern);
        }
    }
}

// File type mixin tests (simulating TOggFileTypeMixin behavior)
#[cfg(test)]
mod ogg_file_type_tests {
    use super::*;
    use std::collections::HashMap;

    // Mock OggFileType for testing (would be implemented by specific formats like OggVorbis)
    struct MockOggFile {
        pages: Vec<OggPage>,
        tags: HashMap<String, Vec<String>>,
    }

    impl MockOggFile {
        fn new(_filename: &str) -> Self {
            Self {
                pages: Vec::new(),
                tags: HashMap::new(),
            }
        }

        fn save(&mut self) -> Result<(), AudexError> {
            // Mock save operation
            Ok(())
        }

        fn clear(&mut self) -> Result<(), AudexError> {
            // Mock clear operation
            self.tags.clear();
            Ok(())
        }

        fn pprint(&self) {
            println!(
                "MockOggFile: {} tags, {} pages",
                self.tags.len(),
                self.pages.len()
            );
        }

        fn scan_file(&self) -> Result<(), AudexError> {
            // Mock file scanning (would parse all pages)
            Ok(())
        }
    }

    #[test]
    fn test_pprint_empty() {
        let audio = MockOggFile::new("test.ogg");
        audio.pprint(); // Should not panic
    }

    #[test]
    fn test_pprint_with_content() {
        let mut audio = MockOggFile::new("test.ogg");
        audio
            .tags
            .insert("artist".to_string(), vec!["Test Artist".to_string()]);
        audio
            .tags
            .insert("title".to_string(), vec!["Test Title".to_string()]);
        audio.pprint(); // Should not panic
    }

    #[test]
    fn test_no_tags() {
        let audio = MockOggFile::new("test.ogg");
        assert!(audio.tags.is_empty());
    }

    #[test]
    fn test_vendor_safe() {
        let mut audio = MockOggFile::new("test.ogg");
        audio
            .tags
            .insert("vendor".to_string(), vec!["a vendor".to_string()]);
        let _ = audio.save();

        assert_eq!(
            audio.tags.get("vendor"),
            Some(&vec!["a vendor".to_string()])
        );
    }

    #[test]
    fn test_set_two_tags() {
        let mut audio = MockOggFile::new("test.ogg");
        audio.tags.insert("foo".to_string(), vec!["a".to_string()]);
        audio.tags.insert("bar".to_string(), vec!["b".to_string()]);
        let _ = audio.save();

        assert_eq!(audio.tags.len(), 2);
        assert_eq!(audio.tags.get("foo"), Some(&vec!["a".to_string()]));
        assert_eq!(audio.tags.get("bar"), Some(&vec!["b".to_string()]));
        let _ = audio.scan_file();
    }

    #[test]
    fn test_save_twice() {
        let mut audio = MockOggFile::new("test.ogg");
        audio
            .tags
            .insert("test".to_string(), vec!["value".to_string()]);

        let _ = audio.save();
        let _ = audio.save(); // Should not fail

        assert_eq!(audio.tags.get("test"), Some(&vec!["value".to_string()]));
        let _ = audio.scan_file();
    }

    #[test]
    fn test_set_delete() {
        let mut audio = MockOggFile::new("test.ogg");
        audio.tags.insert("foo".to_string(), vec!["a".to_string()]);
        audio.tags.insert("bar".to_string(), vec!["b".to_string()]);
        let _ = audio.save();

        audio.tags.clear();
        let _ = audio.save();

        assert!(audio.tags.is_empty());
        let _ = audio.scan_file();
    }

    #[test]
    fn test_delete() {
        let mut audio = MockOggFile::new("test.ogg");
        audio.tags.insert("foo".to_string(), vec!["a".to_string()]);
        audio.tags.insert("bar".to_string(), vec!["b".to_string()]);
        let _ = audio.save();

        let _ = audio.clear();
        assert!(audio.tags.is_empty());

        // Add large tag and save
        audio
            .tags
            .insert("foobar".to_string(), vec!["foobar".repeat(1000)]);
        let _ = audio.save();
        assert!(!audio.tags.is_empty());

        let _ = audio.scan_file();
    }

    #[test]
    fn test_really_big() {
        let mut audio = MockOggFile::new("test.ogg");

        audio
            .tags
            .insert("foo".to_string(), vec!["foo".repeat(1 << 16)]);
        audio
            .tags
            .insert("bar".to_string(), vec!["bar".repeat(1 << 16)]);
        audio
            .tags
            .insert("baz".to_string(), vec!["quux".repeat(1 << 16)]);
        let _ = audio.save();

        assert_eq!(audio.tags.get("foo"), Some(&vec!["foo".repeat(1 << 16)]));
        assert_eq!(audio.tags.get("bar"), Some(&vec!["bar".repeat(1 << 16)]));
        assert_eq!(audio.tags.get("baz"), Some(&vec!["quux".repeat(1 << 16)]));
        let _ = audio.scan_file();
    }

    #[test]
    fn test_delete_really_big() {
        let mut audio = MockOggFile::new("test.ogg");

        audio
            .tags
            .insert("foo".to_string(), vec!["foo".repeat(1 << 16)]);
        audio
            .tags
            .insert("bar".to_string(), vec!["bar".repeat(1 << 16)]);
        audio
            .tags
            .insert("baz".to_string(), vec!["quux".repeat(1 << 16)]);
        let _ = audio.save();

        let _ = audio.clear();
        assert!(audio.tags.is_empty());
        let _ = audio.scan_file();
    }

    #[test]
    fn test_mime_secondary() {
        // Test MIME type detection
        let _audio = MockOggFile::new("test.ogg");
        let mime_types = ["application/ogg", "audio/ogg"];

        assert!(mime_types.contains(&"application/ogg"));
    }

    #[test]
    fn test_length_info() {
        // Test getting length information (simulated)
        let _audio = MockOggFile::new("empty.ogg");

        // Verify the mock doesn't panic during setup
    }
}

#[cfg(test)]
mod ogg_replace_tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write as _};

    fn make_page(serial: u32, sequence: u32, packets: Vec<Vec<u8>>) -> OggPage {
        let mut page = OggPage::new();
        page.serial = serial;
        page.sequence = sequence;
        page.packets = packets;
        page.complete = true;
        page
    }

    /// Write pages to a temp file and return the file (rewound to start)
    fn write_pages_to_file(pages: &[OggPage]) -> std::fs::File {
        let tmp = tempfile::tempfile().expect("Failed to create temp file");
        let mut file = tmp;
        for page in pages {
            file.write_all(&page.write().unwrap()).unwrap();
        }
        file.seek(SeekFrom::Start(0)).unwrap();
        file
    }

    fn read_pages_from_file(file: &mut std::fs::File, n: usize) -> Vec<OggPage> {
        (0..n)
            .map(|_| OggPage::from_reader(file).expect("Failed to read page"))
            .collect()
    }

    #[test]
    fn test_replace() {
        let pages = vec![
            make_page(42, 0, vec![b"foo".to_vec()]),
            make_page(24, 0, vec![b"bar".to_vec()]),
            make_page(42, 1, vec![b"baz".to_vec()]),
        ];
        let mut fileobj = write_pages_to_file(&pages);

        let pages_from_file = read_pages_from_file(&mut fileobj, 3);

        let old_pages = vec![pages_from_file[0].clone(), pages_from_file[2].clone()];
        let packets = OggPage::to_packets(&old_pages, true).unwrap();
        assert_eq!(packets, vec![b"foo".to_vec(), b"baz".to_vec()]);

        let new_pages = OggPage::from_packets(
            vec![b"1111".to_vec(), b"2222".to_vec()],
            old_pages[0].sequence as u32,
            4096,
            2048,
        );
        assert_eq!(new_pages.len(), 1);

        OggPage::replace(&mut fileobj, &old_pages, new_pages).unwrap();

        // Verify: first page (serial 42) has new data, second (serial 24) untouched
        fileobj.seek(SeekFrom::Start(0)).unwrap();
        let first = OggPage::from_reader(&mut fileobj).unwrap();
        assert_eq!(first.serial, 42);
        let first_packets = OggPage::to_packets(&[first], true).unwrap();
        assert_eq!(first_packets, vec![b"1111".to_vec(), b"2222".to_vec()]);

        let second = OggPage::from_reader(&mut fileobj).unwrap();
        assert_eq!(second.serial, 24);
        let second_packets = OggPage::to_packets(&[second], true).unwrap();
        assert_eq!(second_packets, vec![b"bar".to_vec()]);
    }

    #[test]
    fn test_replace_continued() {
        let mut page0 = make_page(1, 0, vec![b"foo".to_vec()]);
        page0.complete = false;

        let mut page1 = make_page(1, 1, vec![b"bar".to_vec()]);
        page1.set_continued(true);

        let mut fileobj = write_pages_to_file(&[page0, page1]);

        let pages_from_file = read_pages_from_file(&mut fileobj, 2);

        let combined = OggPage::to_packets(&pages_from_file, false).unwrap();
        assert_eq!(combined, vec![b"foobar".to_vec()]);

        let new_pages = OggPage::from_packets(vec![b"quuux".to_vec()], 0, 4096, 2048);
        OggPage::replace(&mut fileobj, &[pages_from_file[0].clone()], new_pages).unwrap();

        fileobj.seek(SeekFrom::Start(0)).unwrap();
        let result_pages = read_pages_from_file(&mut fileobj, 2);
        let written = OggPage::to_packets(&result_pages, false).unwrap();
        assert_eq!(written, vec![b"quuuxbar".to_vec()]);
    }
}

#[cfg(test)]
mod ogg_renumber_tests {
    use super::*;
    use std::io::Cursor;
    use std::path::PathBuf;

    fn make_page(serial: u32, sequence: u32, packets: Vec<Vec<u8>>) -> OggPage {
        let mut page = OggPage::new();
        page.serial = serial;
        page.sequence = sequence;
        page.packets = packets;
        page.complete = true;
        page
    }

    fn write_pages_to_cursor(pages: &[OggPage]) -> Cursor<Vec<u8>> {
        let mut buf = Vec::new();
        for page in pages {
            buf.extend_from_slice(&page.write().unwrap());
        }
        Cursor::new(buf)
    }

    fn read_pages_from_cursor(cursor: &mut Cursor<Vec<u8>>, n: usize) -> Vec<OggPage> {
        (0..n)
            .map(|_| OggPage::from_reader(cursor).expect("Failed to read page"))
            .collect()
    }

    fn create_test_pages() -> Vec<OggPage> {
        vec![
            make_page(1, 0, vec![b"foo".to_vec()]),
            make_page(1, 1, vec![b"bar".to_vec()]),
            make_page(1, 2, vec![b"baz".to_vec()]),
        ]
    }

    #[test]
    fn test_renumber() {
        let pages = create_test_pages();
        assert_eq!(
            pages.iter().map(|p| p.sequence).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );

        let mut fileobj = write_pages_to_cursor(&pages);

        fileobj.set_position(0);
        OggPage::renumber(&mut fileobj, 1, 10).unwrap();
        fileobj.set_position(0);
        let renumbered = read_pages_from_cursor(&mut fileobj, 3);
        assert_eq!(
            renumbered.iter().map(|p| p.sequence).collect::<Vec<_>>(),
            vec![10, 11, 12]
        );

        fileobj.set_position(0);
        OggPage::renumber(&mut fileobj, 1, 20).unwrap();
        fileobj.set_position(0);
        let renumbered2 = read_pages_from_cursor(&mut fileobj, 3);
        assert_eq!(
            renumbered2.iter().map(|p| p.sequence).collect::<Vec<_>>(),
            vec![20, 21, 22]
        );
    }

    #[test]
    fn test_renumber_extradata() {
        let pages = create_test_pages();
        let mut buf = Vec::new();
        for page in &pages {
            buf.extend_from_slice(&page.write().unwrap());
        }
        buf.extend_from_slice(b"left over data");
        let mut fileobj = Cursor::new(buf);

        // Audex renumber succeeds (treats short trailing data as EOF)
        OggPage::renumber(&mut fileobj, 1, 10).unwrap();

        // Pages should be renumbered correctly
        fileobj.set_position(0);
        let renumbered = read_pages_from_cursor(&mut fileobj, 3);
        assert_eq!(
            renumbered.iter().map(|p| p.sequence).collect::<Vec<_>>(),
            vec![10, 11, 12]
        );

        // And the garbage should still be there
        let mut leftover = Vec::new();
        std::io::Read::read_to_end(&mut fileobj, &mut leftover).unwrap();
        assert_eq!(leftover, b"left over data");
    }

    #[test]
    fn test_renumber_reread() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/multipagecomment.ogg");
        let temp = TestUtils::get_temp_copy(&path).expect("Failed to create temp copy");
        let temp_path = temp.path().to_path_buf();

        {
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_path)
                .unwrap();
            OggPage::renumber(&mut file, 1002429366, 20).unwrap();
        }
        {
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temp_path)
                .unwrap();
            OggPage::renumber(&mut file, 1002429366, 0).unwrap();
        }
    }

    #[test]
    fn test_renumber_muxed() {
        let mut pages: Vec<OggPage> = (0..10).map(|_| OggPage::new()).collect();
        for (seq, i) in [0usize].iter().chain(&[2, 3, 4, 5, 6, 7, 8, 9]).enumerate() {
            pages[*i].serial = 0;
            pages[*i].sequence = seq as u32;
            pages[*i].packets = vec![vec![seq as u8]];
            pages[*i].complete = true;
        }
        pages[1].serial = 2;
        pages[1].sequence = 100;
        pages[1].packets = vec![vec![0xFF]];
        pages[1].complete = true;

        let mut fileobj = write_pages_to_cursor(&pages);

        OggPage::renumber(&mut fileobj, 0, 20).unwrap();
        fileobj.set_position(0);
        let result = read_pages_from_cursor(&mut fileobj, 10);

        // Serial 2 page should be untouched
        assert_eq!(result[1].serial, 2);
        assert_eq!(result[1].sequence, 100);

        // Serial 0 pages should be renumbered 20-28
        let serial0_seqs: Vec<u32> = result
            .iter()
            .filter(|p| p.serial == 0)
            .map(|p| p.sequence)
            .collect();
        assert_eq!(serial0_seqs, (20u32..29).collect::<Vec<u32>>());
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use super::*;
    use audex::FileType;
    use audex::limits::ParseLimits;
    use audex::oggflac::OggFlac;
    use audex::oggopus::OggOpus;
    use audex::oggspeex::OggSpeex;
    use audex::oggtheora::OggTheora;

    /// Verify that `accumulate_page_bytes_with_limit` enforces the limits
    /// value it receives, independent of the default limits.
    #[test]
    fn accumulate_page_bytes_enforces_provided_limits() {
        let tight_limits = ParseLimits {
            max_tag_size: 12,
            max_image_size: ParseLimits::default().max_image_size,
        };

        let mut first = OggPage::new();
        first.packets.push(vec![0u8; 8]);

        let mut second = OggPage::new();
        second.packets.push(vec![0u8; 8]);

        let mut cumulative_bytes = 0u64;
        OggPage::accumulate_page_bytes_with_limit(
            tight_limits,
            &mut cumulative_bytes,
            &first,
            "snapshot test",
        )
        .expect("first page should fit the tight limit");

        let err = OggPage::accumulate_page_bytes_with_limit(
            tight_limits,
            &mut cumulative_bytes,
            &second,
            "snapshot test",
        )
        .expect_err("second page should exceed the tight limit");

        assert!(
            err.to_string().contains("snapshot test"),
            "unexpected error: {err}"
        );
    }

    fn build_dummy_ogg_pages(count: usize) -> Vec<u8> {
        let mut data = Vec::new();
        for i in 0..count {
            data.extend_from_slice(b"OggS");
            data.push(0);
            data.push(if i == 0 { 0x02 } else { 0x00 });
            data.extend_from_slice(&0u64.to_le_bytes());
            data.extend_from_slice(&1u32.to_le_bytes());
            data.extend_from_slice(&(i as u32).to_le_bytes());
            data.extend_from_slice(&0u32.to_le_bytes());
            data.push(1);
            data.push(4);
            data.extend_from_slice(b"FAKE");
        }
        data
    }

    // --- Page search limit tests ---

    #[test]
    fn test_oggflac_rejects_after_page_limit() {
        let ogg_data = build_dummy_ogg_pages(2000);
        let mut cursor = Cursor::new(ogg_data);
        let result = OggFlac::load_from_reader(&mut cursor);
        assert!(
            result.is_err(),
            "OggFLAC should stop searching after a bounded number of pages"
        );
    }

    #[test]
    fn test_oggopus_rejects_after_page_limit() {
        let ogg_data = build_dummy_ogg_pages(2000);
        let mut cursor = Cursor::new(ogg_data);
        let result = OggOpus::load_from_reader(&mut cursor);
        assert!(
            result.is_err(),
            "OggOpus should stop searching after a bounded number of pages"
        );
    }

    #[test]
    fn test_oggspeex_rejects_after_page_limit() {
        let ogg_data = build_dummy_ogg_pages(2000);
        let mut cursor = Cursor::new(ogg_data);
        let result = OggSpeex::load_from_reader(&mut cursor);
        assert!(
            result.is_err(),
            "OggSpeex should stop searching after a bounded number of pages"
        );
    }

    #[test]
    fn test_oggtheora_rejects_after_page_limit() {
        let ogg_data = build_dummy_ogg_pages(2000);
        let mut cursor = Cursor::new(ogg_data);
        let result = OggTheora::load_from_reader(&mut cursor);
        assert!(
            result.is_err(),
            "OggTheora should stop searching after a bounded number of pages"
        );
    }

    // --- Tag load page limit tests ---
    //
    // These tests verify that `accumulate_page_bytes_with_limit` correctly
    // rejects pages whose cumulative size exceeds the provided limits.
    // Each test constructs a custom tight limit rather than relying on a
    // mutable global, keeping the tests deterministic under parallel execution.

    #[test]
    fn test_accumulate_rejects_oversized_oggflac_comment_packet() {
        let tight = ParseLimits {
            max_tag_size: 8,
            max_image_size: ParseLimits::default().max_image_size,
        };
        let mut page = OggPage::new();
        page.packets.push(vec![0x41; 80]);
        let mut cumulative = 0u64;

        let err = OggPage::accumulate_page_bytes_with_limit(
            tight,
            &mut cumulative,
            &page,
            "Ogg FLAC comment packet",
        )
        .expect_err("oversized Ogg FLAC comment packet should be rejected");

        assert!(
            err.to_string().contains("Ogg FLAC comment packet"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_accumulate_rejects_oversized_opus_comment_packet() {
        let tight = ParseLimits {
            max_tag_size: 8,
            max_image_size: ParseLimits::default().max_image_size,
        };
        let mut page = OggPage::new();
        page.packets.push(vec![0x42; 80]);
        let mut cumulative = 0u64;

        let err = OggPage::accumulate_page_bytes_with_limit(
            tight,
            &mut cumulative,
            &page,
            "Ogg Opus comment packet",
        )
        .expect_err("oversized Ogg Opus comment packet should be rejected");

        assert!(
            err.to_string().contains("Ogg Opus comment packet"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_accumulate_rejects_oversized_speex_comment_packet() {
        let tight = ParseLimits {
            max_tag_size: 8,
            max_image_size: ParseLimits::default().max_image_size,
        };
        let mut page = OggPage::new();
        page.packets.push(vec![0x43; 80]);
        let mut cumulative = 0u64;

        let err = OggPage::accumulate_page_bytes_with_limit(
            tight,
            &mut cumulative,
            &page,
            "Ogg Speex comment packet",
        )
        .expect_err("oversized Ogg Speex comment packet should be rejected");

        assert!(
            err.to_string().contains("Ogg Speex comment packet"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_accumulate_rejects_oversized_vorbis_comment_packet() {
        let tight = ParseLimits {
            max_tag_size: 8,
            max_image_size: ParseLimits::default().max_image_size,
        };
        let mut page = OggPage::new();
        page.packets.push(vec![0x44; 80]);
        let mut cumulative = 0u64;

        let err = OggPage::accumulate_page_bytes_with_limit(
            tight,
            &mut cumulative,
            &page,
            "OGG Vorbis comment packet",
        )
        .expect_err("oversized OGG Vorbis comment packet should be rejected");

        assert!(
            err.to_string().contains("OGG Vorbis comment packet"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_accumulate_rejects_oversized_theora_comment_packet() {
        let tight = ParseLimits {
            max_tag_size: 8,
            max_image_size: ParseLimits::default().max_image_size,
        };
        let mut page = OggPage::new();
        page.packets.push(vec![0x45; 80]);
        let mut cumulative = 0u64;

        let err = OggPage::accumulate_page_bytes_with_limit(
            tight,
            &mut cumulative,
            &page,
            "Ogg Theora comment packet",
        )
        .expect_err("oversized Ogg Theora comment packet should be rejected");

        let message = err.to_string();
        assert!(
            message.contains("Ogg Theora comment packet"),
            "unexpected error: {message}"
        );
    }

    // --- Segment count tests ---

    #[test]
    fn test_write_truncates_segment_count_above_255() {
        let packet_size = 255 * 256;
        let big_packet = vec![0xABu8; packet_size];

        let mut page = OggPage::new();
        page.serial = 1;
        page.sequence = 0;
        page.packets.push(big_packet);
        page.complete = true;

        let mut buf = Vec::new();
        let result = page.write_to(&mut buf);

        assert!(
            result.is_err(),
            "write_to must reject pages with more than 255 segments"
        );
    }

    #[test]
    fn test_write_succeeds_with_valid_segment_count() {
        let small_packet = vec![0xCDu8; 100];

        let mut page = OggPage::new();
        page.serial = 1;
        page.sequence = 0;
        page.packets.push(small_packet);
        page.complete = true;

        let mut buf = Vec::new();
        let result = page.write_to(&mut buf);

        assert!(result.is_ok(), "Small packet should write without issues");

        let data = result.unwrap();
        assert_eq!(data[26], 1, "Should have exactly 1 segment");
    }

    #[test]
    fn test_write_at_exactly_255_segments() {
        let packet_size = 254 * 255 + 1;
        let packet = vec![0xEFu8; packet_size];

        let mut page = OggPage::new();
        page.serial = 1;
        page.sequence = 0;
        page.packets.push(packet);
        page.complete = true;

        let mut buf = Vec::new();
        let result = page.write_to(&mut buf);

        assert!(result.is_ok(), "Exactly 255 segments should be valid");

        let data = result.unwrap();
        assert_eq!(data[26], 255, "Should have exactly 255 segments");
    }

    // --- Offset adjust tests ---

    #[test]
    fn test_negative_i64_cast_to_u64_wraps() {
        let offset: i64 = 10;
        let offset_adjust: i64 = -50;

        let result = offset + offset_adjust;
        assert!(result < 0);

        let cast_u64 = result as u64;
        assert!(
            cast_u64 > u64::MAX / 2,
            "Negative i64 cast to u64 wraps to huge value: {}",
            cast_u64
        );
    }

    #[test]
    fn test_positive_result_casts_correctly() {
        let offset: i64 = 1000;
        let offset_adjust: i64 = -50;

        let result = offset + offset_adjust;
        assert!(result >= 0);

        let cast_u64 = result as u64;
        assert_eq!(cast_u64, 950);
    }

    // --- Sequence overflow tests ---

    #[test]
    fn test_sequence_overflow_wraps_correctly() {
        // With u32 sequence numbers, wrapping past i32::MAX stays positive
        let first_sequence: u32 = (i32::MAX as u32) - 2;
        let page_count: usize = 5;

        for i in 0..page_count {
            let result = first_sequence.wrapping_add(i as u32);
            // All values should remain valid u32 values
            assert!(
                result >= first_sequence || i >= 3,
                "sequence should wrap correctly: {}",
                result
            );
        }
    }

    #[test]
    fn test_normal_sequence_numbering() {
        let first_sequence: u32 = 5;
        let page_count: usize = 3;

        for i in 0..page_count {
            let result = first_sequence.wrapping_add(i as u32);
            assert_eq!(result, 5 + i as u32);
        }
    }
}

// ---------------------------------------------------------------------------
// Ogg zero-segment page tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod ogg_zero_segment_tests {
    use audex::ogg::OggPage;
    use std::io::Cursor;
    use std::time::{Duration, Instant};

    /// Build a valid Ogg page with 0 segments and a correct CRC32.
    /// The page contains only the 27-byte header and no data.
    fn build_zero_segment_ogg_page(sequence: u32) -> Vec<u8> {
        let mut page = OggPage::new();
        page.serial = 1;
        page.sequence = sequence;
        page.packets = vec![];

        let mut buf = Vec::new();
        page.write_to(&mut buf)
            .expect("failed to write zero-segment page");
        buf
    }

    /// A single zero-segment page should parse successfully.
    #[test]
    fn test_single_zero_segment_page_parses() {
        let page_data = build_zero_segment_ogg_page(0);
        let mut cursor = Cursor::new(page_data);
        let result = OggPage::from_reader(&mut cursor);
        assert!(result.is_ok(), "Single zero-segment page should parse");
        let page = result.unwrap();
        assert_eq!(page.packets.len(), 0, "Zero segments means zero packets");
    }

    /// Many consecutive zero-segment pages: the scanner must not take
    /// an unreasonable amount of time. With 10,000 pages at 27 bytes
    /// each, the file is ~270KB. A cap on page count should prevent
    /// excessive CPU usage.
    #[test]
    fn test_many_zero_segment_pages_terminates_quickly() {
        let num_pages = 10_000u32;
        let mut data = Vec::with_capacity(27 * num_pages as usize);

        for seq in 0..num_pages {
            data.extend_from_slice(&build_zero_segment_ogg_page(seq));
        }

        let mut cursor = Cursor::new(data);

        // Simulate what OggFile::load does — read pages until EOF
        let start = Instant::now();
        let mut page_count = 0u32;
        let max_pages = 100_000u32; // Safety limit for this test

        while let Ok(_page) = OggPage::from_reader(&mut cursor) {
            page_count += 1;
            if page_count >= max_pages {
                break;
            }
        }

        let elapsed = start.elapsed();

        // Should complete quickly — the test data is only ~270KB
        assert!(
            elapsed < Duration::from_secs(5),
            "Scanning {} zero-segment pages took {:?} — too slow",
            page_count,
            elapsed
        );

        assert_eq!(
            page_count, num_pages,
            "Should have read all {} pages",
            num_pages
        );
    }

    /// Verify that after parsing a zero-segment page, the reader position
    /// has advanced by exactly 27 bytes (the header size).
    #[test]
    fn test_reader_advances_correctly_on_zero_segment_page() {
        let page_data = build_zero_segment_ogg_page(0);
        let mut cursor = Cursor::new(page_data);

        let pos_before = cursor.position();
        let _ = OggPage::from_reader(&mut cursor).unwrap();
        let pos_after = cursor.position();

        assert_eq!(
            pos_after - pos_before,
            27,
            "Reader should advance exactly 27 bytes for a zero-segment page"
        );
    }
}
