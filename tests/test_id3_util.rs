//! ID3 utilities tests

use audex::id3::util::*;

#[cfg(test)]
mod bit_padded_int_tests {
    use super::*;

    #[test]
    fn test_negative() {
        let result = BitPaddedInt::new((-1i32).into(), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_zero() {
        let bytes = vec![0x00, 0x00, 0x00, 0x00];
        let bpi = BitPaddedInt::new(bytes.into(), None, None).unwrap();
        assert_eq!(bpi.value(), 0);
    }

    #[test]
    fn test_1() {
        // BitPaddedInt(b'\x00\x00\x00\x01') should equal 1
        let bytes = vec![0x00, 0x00, 0x00, 0x01];
        let bpi = BitPaddedInt::new(bytes.into(), None, None).unwrap();
        assert_eq!(bpi.value(), 1);
    }

    #[test]
    fn test_1l() {
        // BitPaddedInt(b'\x01\x00\x00\x00', bigendian=False) should equal 1
        let bytes = vec![0x01, 0x00, 0x00, 0x00];
        let bpi = BitPaddedInt::new(bytes.into(), None, Some(false)).unwrap();
        assert_eq!(bpi.value(), 1);
    }

    #[test]
    fn test_129() {
        // BitPaddedInt(b'\x00\x00\x01\x01') should equal 0x81
        let bytes = vec![0x00, 0x00, 0x01, 0x01];
        let bpi = BitPaddedInt::new(bytes.into(), None, None).unwrap();
        assert_eq!(bpi.value(), 0x81);
    }

    #[test]
    fn test_129b() {
        // BitPaddedInt(b'\x00\x00\x01\x81') should equal 0x81
        let bytes = vec![0x00, 0x00, 0x01, 0x81];
        let bpi = BitPaddedInt::new(bytes.into(), None, None).unwrap();
        assert_eq!(bpi.value(), 0x81);
    }

    #[test]
    fn test_65() {
        // BitPaddedInt(b'\x00\x00\x01\x81', 6) should equal 0x41
        let bytes = vec![0x00, 0x00, 0x01, 0x81];
        let bpi = BitPaddedInt::new(bytes.into(), Some(6), None).unwrap();
        assert_eq!(bpi.value(), 0x41);
    }

    #[test]
    fn test_32b() {
        // BitPaddedInt(b'\xFF\xFF\xFF\xFF', bits=8) should equal 0xFFFFFFFF
        let bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let bpi = BitPaddedInt::new(bytes.into(), Some(8), None).unwrap();
        assert_eq!(bpi.value(), 0xFFFFFFFF);
    }

    #[test]
    fn test_32bi() {
        // BitPaddedInt(0xFFFFFFFF, bits=8) should equal 0xFFFFFFFF
        // When all bits are used with bits=8, simulate with byte array
        let bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let bpi = BitPaddedInt::new(bytes.into(), Some(8), None).unwrap();
        assert_eq!(bpi.value(), 0xFFFFFFFF);
    }

    #[test]
    fn test_s32b() {
        // BitPaddedInt(b'\xFF\xFF\xFF\xFF', bits=8).as_str() should equal b'\xFF\xFF\xFF\xFF'
        let bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];
        let bpi = BitPaddedInt::new(bytes.into(), Some(8), None).unwrap();
        let result = bpi.as_str(None, None).unwrap();
        assert_eq!(result, vec![0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn test_s0() {
        // BitPaddedInt.to_str(0) should equal b'\x00\x00\x00\x00'
        let result = BitPaddedInt::to_str(0, None, None, None, None).unwrap();
        assert_eq!(result, vec![0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_s1() {
        // BitPaddedInt.to_str(1) should equal b'\x00\x00\x00\x01'
        let result = BitPaddedInt::to_str(1, None, None, None, None).unwrap();
        assert_eq!(result, vec![0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn test_s1l() {
        // BitPaddedInt.to_str(1, bigendian=False) should equal b'\x01\x00\x00\x00'
        let result = BitPaddedInt::to_str(1, None, Some(false), None, None).unwrap();
        assert_eq!(result, vec![0x01, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_s129() {
        // BitPaddedInt.to_str(129) should equal b'\x00\x00\x01\x01'
        let result = BitPaddedInt::to_str(129, None, None, None, None).unwrap();
        assert_eq!(result, vec![0x00, 0x00, 0x01, 0x01]);
    }

    #[test]
    fn test_s65() {
        // BitPaddedInt.to_str(0x41, 6) should equal b'\x00\x00\x01\x01'
        let result = BitPaddedInt::to_str(0x41, Some(6), None, None, None).unwrap();
        assert_eq!(result, vec![0x00, 0x00, 0x01, 0x01]);
    }

    #[test]
    fn test_w129() {
        // BitPaddedInt.to_str(129, width=2) should equal b'\x01\x01'
        let result = BitPaddedInt::to_str(129, None, None, Some(2), None).unwrap();
        assert_eq!(result, vec![0x01, 0x01]);
    }

    #[test]
    fn test_w129l() {
        // BitPaddedInt.to_str(129, width=2, bigendian=False) should equal b'\x01\x01'
        let result = BitPaddedInt::to_str(129, None, Some(false), Some(2), None).unwrap();
        assert_eq!(result, vec![0x01, 0x01]);
    }

    #[test]
    fn test_wsmall() {
        let result = BitPaddedInt::to_str(129, None, None, Some(1), None);
        assert!(result.is_err());
    }

    #[test]
    fn test_str_int_init() {
        // Verify string vs int initialization consistency
        let int_bpi = BitPaddedInt::new((238u32).into(), None, None).unwrap();
        let bytes_bpi = BitPaddedInt::new(vec![0x00, 0x00, 0x00, 0xEE].into(), None, None).unwrap();

        let int_str = int_bpi.as_str(None, None).unwrap();
        let bytes_str = bytes_bpi.as_str(None, None).unwrap();
        assert_eq!(int_str, bytes_str);
    }

    #[test]
    fn test_varwidth() {
        // Test variable width behavior (width=-1)
        let result = BitPaddedInt::to_str(100, None, None, Some(4), None).unwrap();
        assert_eq!(result.len(), 4);

        let result = BitPaddedInt::to_str(100, None, None, Some(-1), None).unwrap();
        assert_eq!(result.len(), 4);

        // 2^32 = 4294967296, which is larger than u32::MAX, use large value
        let result = BitPaddedInt::to_str(u32::MAX, None, None, Some(-1), None).unwrap();
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_minwidth() {
        // Test minimum width parameter
        let result = BitPaddedInt::to_str(100, None, None, Some(-1), Some(6)).unwrap();
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn test_inval_input() {
        // We test with invalid negative input instead
        let result = BitPaddedInt::new((-1i32).into(), None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_has_valid_padding() {
        // Test all 13 padding validation cases

        // BitPaddedInt.has_valid_padding(b"\xff\xff", bits=8) should be true
        assert!(BitPaddedInt::has_valid_padding(
            vec![0xFF, 0xFF].into(),
            Some(8)
        ));

        // BitPaddedInt.has_valid_padding(b"\xff") should be false
        assert!(!BitPaddedInt::has_valid_padding(vec![0xFF].into(), None));

        // BitPaddedInt.has_valid_padding(b"\x00\xff") should be false
        assert!(!BitPaddedInt::has_valid_padding(
            vec![0x00, 0xFF].into(),
            None
        ));

        // BitPaddedInt.has_valid_padding(b"\x7f\x7f") should be true
        assert!(BitPaddedInt::has_valid_padding(
            vec![0x7F, 0x7F].into(),
            None
        ));

        // BitPaddedInt.has_valid_padding(b"\x7f", bits=6) should be false
        assert!(!BitPaddedInt::has_valid_padding(vec![0x7F].into(), Some(6)));

        // BitPaddedInt.has_valid_padding(b"\x9f", bits=6) should be false
        assert!(!BitPaddedInt::has_valid_padding(vec![0x9F].into(), Some(6)));

        // BitPaddedInt.has_valid_padding(b"\x3f", bits=6) should be true
        assert!(BitPaddedInt::has_valid_padding(vec![0x3F].into(), Some(6)));

        // Integer tests

        // BitPaddedInt.has_valid_padding(0xff, bits=8) should be true
        assert!(BitPaddedInt::has_valid_padding((0xFF).into(), Some(8)));

        // BitPaddedInt.has_valid_padding(0xff) should be false
        assert!(!BitPaddedInt::has_valid_padding((0xFF).into(), None));

        // BitPaddedInt.has_valid_padding(0xff << 8) should be false
        assert!(!BitPaddedInt::has_valid_padding((0xFF << 8).into(), None));

        // BitPaddedInt.has_valid_padding(0x7f << 8) should be true
        assert!(BitPaddedInt::has_valid_padding((0x7F << 8).into(), None));

        // BitPaddedInt.has_valid_padding(0x9f << 32, bits=6) should be false
        // Note: Large integers are allowed, simulate with appropriate values
        assert!(!BitPaddedInt::has_valid_padding(
            (0x9Fi32 << 16).into(),
            Some(6)
        ));

        // BitPaddedInt.has_valid_padding(0x3f << 16, bits=6) should be true
        assert!(BitPaddedInt::has_valid_padding(
            (0x3F << 16).into(),
            Some(6)
        ));
    }
}

#[cfg(test)]
mod unsynch_tests {
    use super::*;

    #[test]
    fn test_unsync_encode_decode() {
        // Test exact pairs from format test
        let pairs = [
            (vec![], vec![]),
            (vec![0x00], vec![0x00]),
            (vec![0x44], vec![0x44]),
            (vec![0x44, 0xFF], vec![0x44, 0xFF, 0x00]),
            (vec![0xE0], vec![0xE0]),
            (vec![0xE0, 0xE0], vec![0xE0, 0xE0]),
            (vec![0xE0, 0xFF], vec![0xE0, 0xFF, 0x00]),
            (vec![0xFF], vec![0xFF, 0x00]),
            (vec![0xFF, 0x00], vec![0xFF, 0x00, 0x00]),
            (vec![0xFF, 0x00, 0x00], vec![0xFF, 0x00, 0x00, 0x00]),
            (vec![0xFF, 0x01], vec![0xFF, 0x01]),
            (vec![0xFF, 0x44], vec![0xFF, 0x44]),
            (vec![0xFF, 0xE0], vec![0xFF, 0x00, 0xE0]),
            (vec![0xFF, 0xE0, 0xFF], vec![0xFF, 0x00, 0xE0, 0xFF, 0x00]),
            (
                vec![0xFF, 0xF0, 0x0F, 0x00],
                vec![0xFF, 0x00, 0xF0, 0x0F, 0x00],
            ),
            (vec![0xFF, 0xFF], vec![0xFF, 0x00, 0xFF, 0x00]),
            (vec![0xFF, 0xFF, 0x01], vec![0xFF, 0x00, 0xFF, 0x01]),
            (
                vec![0xFF, 0xFF, 0xFF, 0xFF],
                vec![0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00],
            ),
        ];

        for (d, e) in pairs.iter() {
            assert_eq!(Unsynch::encode(d), *e);

            assert_eq!(Unsynch::decode(e).unwrap(), *d);

            let encoded_e = Unsynch::encode(e);
            assert_eq!(Unsynch::decode(&encoded_e).unwrap(), *e);

            let mut e_plus_e = e.clone();
            e_plus_e.extend_from_slice(e);
            let mut d_plus_d = d.clone();
            d_plus_d.extend_from_slice(d);
            assert_eq!(Unsynch::decode(&e_plus_e).unwrap(), d_plus_d);
        }
    }

    #[test]
    fn test_unsync_decode_lenient() {
        // The decoder is lenient: non-conformant sequences are passed through
        // rather than rejected, matching real-world tagger behavior.

        let result = Unsynch::decode(&[0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(result.is_ok(), "consecutive 0xFF bytes should be tolerated");

        let result = Unsynch::decode(&[0xFF, 0xF0, 0x0F, 0x00]);
        assert!(result.is_ok(), "unprotected high bytes should be tolerated");

        let result = Unsynch::decode(&[0xFF, 0xE0]);
        assert!(
            result.is_ok(),
            "0xFF 0xE0 without protection byte should be tolerated"
        );

        // A trailing 0xFF is tolerated (some taggers produce this).
        // The decode preserves the 0xFF in the output instead of erroring.
        let result = Unsynch::decode(&[0xFF]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![0xFF]);
    }
}

// Keep existing useful functionality tests below

#[cfg(test)]
mod synchsafe_tests {
    use super::*;

    #[test]
    fn test_synchsafe_decode() {
        // Test basic synchsafe decoding
        assert_eq!(decode_synchsafe_int(&[0x00, 0x00, 0x00, 0x00]), 0);
        assert_eq!(decode_synchsafe_int(&[0x00, 0x00, 0x00, 0x7F]), 127);
        assert_eq!(decode_synchsafe_int(&[0x00, 0x00, 0x01, 0x00]), 128);
        assert_eq!(decode_synchsafe_int(&[0x7F, 0x7F, 0x7F, 0x7F]), 268435455);
    }

    #[test]
    fn test_synchsafe_encode() {
        // Test basic synchsafe encoding (returns Result now)
        assert_eq!(encode_synchsafe_int(0).unwrap(), [0x00, 0x00, 0x00, 0x00]);
        assert_eq!(encode_synchsafe_int(127).unwrap(), [0x00, 0x00, 0x00, 0x7F]);
        assert_eq!(encode_synchsafe_int(128).unwrap(), [0x00, 0x00, 0x01, 0x00]);
        assert_eq!(
            encode_synchsafe_int(268435455).unwrap(),
            [0x7F, 0x7F, 0x7F, 0x7F]
        );
    }

    #[test]
    fn test_synchsafe_roundtrip() {
        let test_values = [0, 1, 127, 128, 255, 256, 16383, 16384, 2097151, 268435455];

        for &value in &test_values {
            let encoded = encode_synchsafe_int(value).unwrap();
            let decoded = decode_synchsafe_int(&encoded);
            assert_eq!(decoded, value, "Synchsafe roundtrip failed for {}", value);

            // Verify no bytes have high bit set
            for &byte in &encoded {
                assert_eq!(
                    byte & 0x80,
                    0,
                    "High bit set in synchsafe byte: {:#02x}",
                    byte
                );
            }
        }
    }
}

#[cfg(test)]
mod frame_validation_tests {
    use super::*;

    #[test]
    fn test_valid_frame_ids() {
        // Valid frame IDs (alphanumeric and uppercase)
        assert!(is_valid_frame_id("TIT2"));
        assert!(is_valid_frame_id("TPE1"));
        assert!(is_valid_frame_id("TALB"));
        assert!(is_valid_frame_id("TXXX"));
        assert!(is_valid_frame_id("APIC"));
        assert!(is_valid_frame_id("TYER"));

        // ID3v2.2 frame IDs (3 characters) - also valid
        assert!(is_valid_frame_id("TT2"));
        assert!(is_valid_frame_id("TP1"));
        assert!(is_valid_frame_id("TAL"));

        // Mixed alphanumeric
        assert!(is_valid_frame_id("TRCK"));
        assert!(is_valid_frame_id("TPOS"));
    }

    #[test]
    fn test_invalid_frame_ids() {
        // Invalid characters
        assert!(!is_valid_frame_id("tit2")); // lowercase
        assert!(!is_valid_frame_id("TIT!")); // special character
        assert!(!is_valid_frame_id("TI 2")); // space

        // Empty or non-alphanumeric
        assert!(!is_valid_frame_id("")); // empty
        assert!(!is_valid_frame_id("TI-2")); // hyphen
        assert!(!is_valid_frame_id("TI_2")); // underscore

        // Mixed case
        assert!(!is_valid_frame_id("TiT2")); // mixed case
        assert!(!is_valid_frame_id("tPE1")); // mixed case

        // Numbers only (no letters)
        assert!(!is_valid_frame_id("1234")); // no letters
    }

    #[test]
    fn test_frame_id_upgrade() {
        // Test upgrading from v2.2 to v2.3/v2.4
        assert_eq!(upgrade_frame_id("TT2"), Some("TIT2".to_string()));
        assert_eq!(upgrade_frame_id("TP1"), Some("TPE1".to_string()));
        assert_eq!(upgrade_frame_id("TAL"), Some("TALB".to_string()));
        assert_eq!(upgrade_frame_id("TYE"), Some("TYER".to_string()));
        assert_eq!(upgrade_frame_id("TCO"), Some("TCON".to_string()));
        assert_eq!(upgrade_frame_id("TRK"), Some("TRCK".to_string()));
        assert_eq!(upgrade_frame_id("COM"), Some("COMM".to_string()));
        assert_eq!(upgrade_frame_id("PIC"), Some("APIC".to_string()));

        // Unknown frame
        assert_eq!(upgrade_frame_id("XYZ"), None);
    }

    #[test]
    fn test_frame_id_downgrade() {
        // Test downgrading from v2.3/v2.4 to v2.2
        assert_eq!(downgrade_frame_id("TIT2"), Some("TT2".to_string()));
        assert_eq!(downgrade_frame_id("TPE1"), Some("TP1".to_string()));
        assert_eq!(downgrade_frame_id("TALB"), Some("TAL".to_string()));
        assert_eq!(downgrade_frame_id("TYER"), Some("TYE".to_string()));
        assert_eq!(downgrade_frame_id("TDRC"), Some("TYE".to_string())); // v2.4 -> v2.2
        assert_eq!(downgrade_frame_id("TCON"), Some("TCO".to_string()));
        assert_eq!(downgrade_frame_id("TRCK"), Some("TRK".to_string()));
        assert_eq!(downgrade_frame_id("COMM"), Some("COM".to_string()));
        assert_eq!(downgrade_frame_id("APIC"), Some("PIC".to_string()));

        // Unknown frame
        assert_eq!(downgrade_frame_id("WXYZ"), None);
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn test_min_version_for_frame() {
        // ID3v2.4 only frames
        assert_eq!(min_version_for_frame("TDRC"), Some((2, 4)));
        assert_eq!(min_version_for_frame("TDRL"), Some((2, 4)));
        assert_eq!(min_version_for_frame("TIPL"), Some((2, 4)));
        assert_eq!(min_version_for_frame("TMCL"), Some((2, 4)));

        // ID3v2.3+ frames
        assert_eq!(min_version_for_frame("TYER"), Some((2, 3)));
        assert_eq!(min_version_for_frame("TDAT"), Some((2, 3)));
        assert_eq!(min_version_for_frame("TIME"), Some((2, 3)));

        // Common frames (v2.3+)
        assert_eq!(min_version_for_frame("TIT2"), Some((2, 3)));
        assert_eq!(min_version_for_frame("TPE1"), Some((2, 3)));

        // v2.2 frames
        assert_eq!(min_version_for_frame("TT2"), Some((2, 2)));
        assert_eq!(min_version_for_frame("TP1"), Some((2, 2)));

        // Unknown frame
        assert_eq!(min_version_for_frame("UNKNOWN"), None);
    }

    #[test]
    fn test_default_text_encoding() {
        // Older versions default to Latin-1
        assert_eq!(default_text_encoding((2, 2)), 0);
        assert_eq!(default_text_encoding((2, 3)), 0);

        // ID3v2.4 defaults to UTF-8
        assert_eq!(default_text_encoding((2, 4)), 3);

        // Unknown version defaults to Latin-1
        assert_eq!(default_text_encoding((1, 0)), 0);
    }
}

#[cfg(test)]
mod utility_tests {
    use super::*;

    #[test]
    fn test_calculate_tag_size() {
        // Basic tag size calculation
        assert_eq!(calculate_tag_size(100, 0), 110); // 100 + 10 header
        assert_eq!(calculate_tag_size(100, 50), 160); // 100 + 10 header + 50 padding
        assert_eq!(calculate_tag_size(0, 1024), 1034); // Empty tag with padding
    }

    #[test]
    fn test_find_sync_pattern() {
        // Test finding MPEG sync patterns (11 consecutive 1 bits)
        let data_with_sync = &[0x12, 0xFF, 0xFB, 0x90]; // 0xFFFB has sync pattern
        assert_eq!(find_sync_pattern(data_with_sync), Some(1));

        let data_without_sync = &[0x12, 0x34, 0x56, 0x78];
        assert_eq!(find_sync_pattern(data_without_sync), None);

        // Test at beginning
        let data_start_sync = &[0xFF, 0xFB, 0x90, 0x00];
        assert_eq!(find_sync_pattern(data_start_sync), Some(0));
    }

    #[test]
    fn test_validate_id3_header() {
        // Valid header
        let valid_header = b"ID3\x03\x00\x00\x00\x00\x00\x0A";
        assert!(validate_id3_header(valid_header).is_ok());

        // Invalid signature
        let invalid_sig = b"XYZ\x03\x00\x00\x00\x00\x00\x0A";
        assert!(validate_id3_header(invalid_sig).is_err());

        // Too short
        let too_short = b"ID3\x03\x00";
        assert!(validate_id3_header(too_short).is_err());

        // Invalid version
        let invalid_version = b"ID3\x01\x00\x00\x00\x00\x00\x0A"; // v2.1 doesn't exist
        assert!(validate_id3_header(invalid_version).is_err());

        let invalid_version2 = b"ID3\x05\x00\x00\x00\x00\x00\x0A"; // v2.5 doesn't exist
        assert!(validate_id3_header(invalid_version2).is_err());

        // Invalid synchsafe size (high bit set)
        let invalid_size = b"ID3\x03\x00\x00\x80\x00\x00\x0A";
        assert!(validate_id3_header(invalid_size).is_err());
    }
}

// ---------------------------------------------------------------------------
// BitPaddedInt shift overflow tests (u32 version in id3::util)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod shift_overflow_tests {
    use audex::id3::util::*;

    /// Six bytes with 7-bit encoding: shift reaches 35 on the 6th byte,
    /// exceeding the u32 bit width. Must not panic.
    #[test]
    fn test_six_bytes_causes_shift_past_u32_width() {
        let bytes = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let result = BitPaddedInt::new(bytes.into(), Some(7), Some(true));
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle 6-byte input without panicking"
        );
    }

    /// Eight bytes with 8-bit encoding: shift reaches 32 on the 5th byte.
    #[test]
    fn test_eight_bytes_with_8bit_encoding() {
        let bytes = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let result = BitPaddedInt::new(bytes.into(), Some(8), Some(true));
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle 8-byte input without panicking"
        );
    }

    /// Five bytes with 7-bit encoding at the edge of u32 capacity.
    #[test]
    fn test_five_bytes_with_7bit_encoding() {
        let bytes = vec![0x7F, 0x7F, 0x7F, 0x7F, 0x7F];
        let result = BitPaddedInt::new(bytes.into(), Some(7), Some(true));
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle 5-byte input without panicking"
        );
    }

    /// Large integer value on the Int path.
    #[test]
    fn test_int_path_large_value_shift_overflow() {
        let result = BitPaddedInt::new((i32::MAX).into(), Some(7), Some(true));
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle large int without panicking"
        );
    }

    /// Standard 4-byte synchsafe must still work.
    #[test]
    fn test_normal_4byte_synchsafe_still_works() {
        let bytes = vec![0x00, 0x00, 0x02, 0x01];
        let result = BitPaddedInt::new(bytes.into(), Some(7), Some(true));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().value(), 257);
    }
}

// ---------------------------------------------------------------------------
// BitPaddedInt::from_bytes shift overflow tests (u64 version in util)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod from_bytes_shift_overflow_tests {
    use audex::util::BitPaddedInt;

    /// 11 bytes with bits=7: shift reaches 70, exceeding u64 width.
    #[test]
    fn test_eleven_bytes_7bit_causes_shift_past_u64_width() {
        let bytes = vec![0x01; 11];
        let result = BitPaddedInt::from_bytes(&bytes, 7, true);
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle 11-byte input without panicking"
        );
    }

    /// 9 bytes with bits=8: shift reaches 64.
    #[test]
    fn test_nine_bytes_8bit_causes_shift_at_64() {
        let bytes = vec![0xFF; 9];
        let result = BitPaddedInt::from_bytes(&bytes, 8, true);
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle 9-byte input without panicking"
        );
    }

    /// 20 bytes with bits=7: shift reaches 133 — well past u64 width.
    #[test]
    fn test_large_buffer_overflow() {
        let bytes = vec![0x7F; 20];
        let result = BitPaddedInt::from_bytes(&bytes, 7, true);
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle 20-byte input without panicking"
        );
    }

    /// 10 bytes with bits=7: shift=63 on the last byte — edge of u64.
    #[test]
    fn test_ten_bytes_7bit_encoding_at_edge() {
        let bytes = vec![0x01; 10];
        let result = BitPaddedInt::from_bytes(&bytes, 7, true);
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle 10-byte input without panicking"
        );
    }

    /// Standard 4-byte synchsafe must still work.
    #[test]
    fn test_standard_4byte_synchsafe_unaffected() {
        let bytes = vec![0x00, 0x00, 0x02, 0x01];
        let result = BitPaddedInt::from_bytes(&bytes, 7, true);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().value(), 257);
    }
}

// ---------------------------------------------------------------------------
// BitPaddedInt zero-bits tests (util version)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod zero_bits_tests {
    use audex::util::BitPaddedInt;

    /// With bits=0 and fixed width, encoding silently produces all zeroes.
    #[test]
    fn test_to_bytes_with_zero_bits_fixed_width() {
        let result = BitPaddedInt::to_bytes(255, 0, true, 4, 4);
        assert!(
            result.is_err(),
            "to_bytes with bits=0 should error, not silently zero the value"
        );
    }

    /// With bits=0 and variable width, the loop becomes infinite.
    #[test]
    fn test_to_bytes_with_zero_bits_variable_width_does_not_hang() {
        let result = BitPaddedInt::to_bytes(1, 0, true, 0, 4);
        assert!(
            result.is_err(),
            "to_bytes with bits=0 should error, not loop forever"
        );
    }

    /// Constructing with bits=0 should also be rejected.
    #[test]
    fn test_from_int_with_zero_bits() {
        let result = BitPaddedInt::from_int(42, 0, true);
        assert!(
            result.is_err(),
            "from_int with bits=0 should return an error"
        );
    }

    /// Normal encoding must still work.
    #[test]
    fn test_normal_encoding_unaffected() {
        let result = BitPaddedInt::to_bytes(257, 7, true, 4, 4);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![0x00, 0x00, 0x02, 0x01]);
    }
}

// ---------------------------------------------------------------------------
// BitPaddedInt Add overflow tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod bitpadded_add_overflow_tests {
    use audex::util::BitPaddedInt;

    /// Adding two BitPaddedInt values near u64::MAX should not panic.
    /// The Sub impl correctly uses saturating_sub, but Add uses plain +.
    #[test]
    fn test_add_near_max_does_not_panic() {
        let a = BitPaddedInt::new(u64::MAX - 1, 7, true).unwrap();
        let b = BitPaddedInt::new(2, 7, true).unwrap();

        // This panics in debug mode without saturating_add
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a + b));

        // After fix: should succeed with saturated value instead of panicking
        assert!(
            result.is_ok(),
            "BitPaddedInt::Add panicked on overflow — should use saturating_add"
        );
    }

    /// Adding two large values that would overflow u64.
    #[test]
    fn test_add_both_large_saturates() {
        let a = BitPaddedInt::new(u64::MAX, 7, true).unwrap();
        let b = BitPaddedInt::new(1, 7, true).unwrap();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a + b));
        assert!(
            result.is_ok(),
            "Adding u64::MAX + 1 should saturate, not panic"
        );

        if let Ok(sum) = result {
            assert_eq!(
                sum.value(),
                u64::MAX,
                "Saturating add of u64::MAX + 1 should yield u64::MAX"
            );
        }
    }

    /// Normal addition must still produce correct results.
    #[test]
    fn test_add_normal_case_works() {
        let a = BitPaddedInt::new(100, 7, true).unwrap();
        let b = BitPaddedInt::new(200, 7, true).unwrap();
        let sum = a + b;
        assert_eq!(sum.value(), 300, "100 + 200 should equal 300");
    }
}
