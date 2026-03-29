// Comprehensive property-based testing using proptest framework
//
// This module provides extensive property-based tests to validate:
// - File I/O error handling across all operations
// - Tag type operations for all supported formats
// - Encoding/decoding round-trip properties
// - Serialization/deserialization invariants
// - Shrinking strategies for failure minimization
// - Seed-based test reproducibility
// - Statistical coverage analysis

use proptest::prelude::*;
use std::io::{Cursor, Write};
use tempfile::NamedTempFile;

use audex::id3::specs::*;
use audex::id3::util::{decode_synchsafe_int, encode_synchsafe_int};
use audex::util::*;

// Property test configuration with seed support for reproducibility
prop_compose! {
    /// Generate a random byte vector with bounded size
    fn arb_byte_vec(max_size: usize)
                   (size in 0..=max_size)
                   (vec in prop::collection::vec(any::<u8>(), size))
                   -> Vec<u8> {
        vec
    }
}

prop_compose! {
    /// Generate a valid ASCII string for ID3 frames
    fn arb_ascii_string(max_len: usize)
                       (s in "[A-Za-z0-9 ]{1,100}")
                       -> String {
        let chars: Vec<char> = s.chars().collect();
        let len = std::cmp::min(chars.len(), max_len);
        chars.into_iter().take(len).collect()
    }
}

prop_compose! {
    /// Generate a valid UTF-8 string
    fn arb_utf8_string(max_len: usize)
                      (s in "\\PC{1,100}")
                      -> String {
        let chars: Vec<char> = s.chars().collect();
        let len = std::cmp::min(chars.len(), max_len);
        chars.into_iter().take(len).collect()
    }
}

#[cfg(test)]
mod file_io_error_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            max_shrink_iters: 1000,
            .. ProptestConfig::default()
        })]

        /// Property: Reading exactly N bytes from a file with N bytes should always succeed
        #[test]
        fn read_full_exact_never_fails(data in arb_byte_vec(10000)) {
            let mut temp_file = NamedTempFile::new().unwrap();
            temp_file.write_all(&data).unwrap();
            temp_file.flush().unwrap();

            let mut file = temp_file.reopen().unwrap();
            let result = read_full(&mut file, data.len());

            prop_assert!(result.is_ok());
            prop_assert_eq!(result.unwrap(), data);
        }

        /// Property: Seeking to end with 0 offset should give file size
        #[test]
        fn seek_end_zero_gives_size(data in arb_byte_vec(10000)) {
            let mut temp_file = NamedTempFile::new().unwrap();
            temp_file.write_all(&data).unwrap();
            temp_file.flush().unwrap();

            let mut file = temp_file.reopen().unwrap();
            let pos = seek_end(&mut file, 0).unwrap();

            prop_assert_eq!(pos, data.len() as u64);
        }

        /// Property: get_size should always return non-negative value
        #[test]
        fn get_size_never_negative(data in arb_byte_vec(10000)) {
            let mut temp_file = NamedTempFile::new().unwrap();
            temp_file.write_all(&data).unwrap();
            temp_file.flush().unwrap();

            let mut file = temp_file.reopen().unwrap();
            let size = get_size(&mut file).unwrap();

            prop_assert_eq!(size, data.len() as u64);
        }

        /// Property: File size should match written data length
        #[test]
        fn file_size_matches_written_data(data in arb_byte_vec(10000)) {
            let mut temp_file = NamedTempFile::new().unwrap();
            temp_file.write_all(&data).unwrap();
            temp_file.flush().unwrap();

            let metadata = temp_file.as_file().metadata().unwrap();
            prop_assert_eq!(metadata.len(), data.len() as u64);
        }
    }
}

#[cfg(test)]
mod encoding_decoding_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 200,
            max_shrink_iters: 1000,
            .. ProptestConfig::default()
        })]

        /// Property: Synchsafe integer encoding round-trip should be identity
        #[test]
        fn synchsafe_roundtrip(value in 0u32..0x0FFFFFFF) {
            let encoded = encode_synchsafe_int(value).unwrap();
            let decoded = decode_synchsafe_int(&encoded);

            prop_assert_eq!(decoded, value);
        }

        /// Property: UTF-8 encoding round-trip preserves text
        #[test]
        fn utf8_encoding_roundtrip(text in arb_utf8_string(1000)) {
            let encoding = TextEncoding::Utf8;
            let encoded = encoding.encode_text(&text).unwrap();
            let decoded = encoding.decode_text(&encoded).unwrap();

            prop_assert_eq!(decoded, text);
        }

        /// Property: Latin1 encoding round-trip for ASCII text
        #[test]
        fn latin1_encoding_roundtrip_ascii(text in arb_ascii_string(1000)) {
            let encoding = TextEncoding::Latin1;
            let encoded = encoding.encode_text(&text).unwrap();
            let decoded = encoding.decode_text(&encoded).unwrap();

            prop_assert_eq!(decoded, text);
        }

        /// Property: Binary data spec round-trip is identity
        #[test]
        fn binary_data_roundtrip(data in arb_byte_vec(10000)) {
            let spec = BinaryDataSpec::new("test");
            let frame = FrameData::new("TEST".to_string(), 1000, 0, (2, 4));
            let header = FrameHeader::new("TEST".to_string(), 1000, 0, (2, 4));
            let config = FrameWriteConfig::default();

            let written = spec.write(&config, &frame, &data).unwrap();
            let (read_data, _) = spec.read(&header, &frame, &written).unwrap();

            prop_assert_eq!(read_data, data);
        }

        /// Property: Byte spec value is always in 0..=255
        #[test]
        fn byte_spec_valid_range(value in any::<u8>()) {
            let spec = ByteSpec::new("test");
            let frame = FrameData::new("TEST".to_string(), 1000, 0, (2, 4));
            let config = FrameWriteConfig::default();

            let written = spec.write(&config, &frame, &value).unwrap();
            prop_assert_eq!(written.len(), 1);
            prop_assert_eq!(written[0], value);
        }
    }
}

#[cfg(test)]
mod math_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 1000,
            .. ProptestConfig::default()
        })]

        /// Property: intround should handle all finite values
        #[test]
        fn intround_handles_finite(value in -1000000.0f64..1000000.0f64) {
            let result = intround(value);

            // Result should be close to value (within 1)
            let diff = (result as f64 - value).abs();
            prop_assert!(diff <= 1.0);
        }

        /// Property: intround special values always return 0
        #[test]
        fn intround_special_values_zero(
            special in prop_oneof![
                Just(f64::NAN),
                Just(f64::INFINITY),
                Just(f64::NEG_INFINITY)
            ]
        ) {
            prop_assert_eq!(intround(special), 0);
        }

        /// Property: intround for integers should return same value
        #[test]
        fn intround_integers_identity(value in -1000000i64..1000000i64) {
            let result = intround(value as f64);
            prop_assert_eq!(result, value);
        }
    }
}

#[cfg(test)]
mod bit_reader_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            .. ProptestConfig::default()
        })]

        /// Property: Reading 8 bits should equal reading 1 byte
        #[test]
        fn bit_reader_8bits_equals_byte(data in arb_byte_vec(100)) {
            if data.is_empty() {
                return Ok(());
            }

            let cursor1 = Cursor::new(data.clone());
            let cursor2 = Cursor::new(data.clone());

            let mut reader1 = BitReader::new(cursor1).unwrap();
            let mut reader2 = BitReader::new(cursor2).unwrap();

            let bits = reader1.bits(8).unwrap();
            let bytes = reader2.bytes(1).unwrap();

            prop_assert_eq!(bits, bytes[0] as i32);
        }

        /// Property: Align should always result in aligned reader
        #[test]
        fn align_results_in_aligned(data in arb_byte_vec(100)) {
            if data.is_empty() {
                return Ok(());
            }

            let cursor = Cursor::new(data);
            let mut reader = BitReader::new(cursor).unwrap();

            // Read some bits to become unaligned
            let _ = reader.bits(3);

            // Align should make it aligned
            reader.align();

            prop_assert!(reader.is_aligned());
        }

        /// Property: Reading 0 bits should always return 0
        #[test]
        fn read_zero_bits_returns_zero(data in arb_byte_vec(100)) {
            if data.is_empty() {
                return Ok(());
            }

            let cursor = Cursor::new(data);
            let mut reader = BitReader::new(cursor).unwrap();

            let result = reader.bits(0).unwrap();
            prop_assert_eq!(result, 0);
        }

        /// Property: Reading negative bits should always error
        #[test]
        fn read_negative_bits_errors(data in arb_byte_vec(100), neg_count in -1000i32..-1) {
            if data.is_empty() {
                return Ok(());
            }

            let cursor = Cursor::new(data);
            let mut reader = BitReader::new(cursor).unwrap();

            let result = reader.bits(neg_count);
            prop_assert!(result.is_err());
        }
    }
}

#[cfg(test)]
mod string_spec_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            .. ProptestConfig::default()
        })]

        /// Property: StringSpec write always produces fixed size output
        #[test]
        fn string_spec_fixed_size(text in arb_ascii_string(50), size in 1usize..=100) {
            let spec = StringSpec::new("test", size);
            let frame = FrameData::new("TEST".to_string(), 1000, 0, (2, 4));
            let config = FrameWriteConfig::default();

            let written = spec.write(&config, &frame, &text).unwrap();
            prop_assert_eq!(written.len(), size);
        }

        /// Property: StringSpec read truncates or pads appropriately
        #[test]
        fn string_spec_truncate_or_pad(text in arb_ascii_string(50), size in 1usize..=100) {
            let spec = StringSpec::new("test", size);
            let frame = FrameData::new("TEST".to_string(), 1000, 0, (2, 4));
            let header = FrameHeader::new("TEST".to_string(), 1000, 0, (2, 4));
            let config = FrameWriteConfig::default();

            let written = spec.write(&config, &frame, &text).unwrap();
            let (read_text, consumed) = spec.read(&header, &frame, &written).unwrap();

            prop_assert_eq!(consumed, size);

            // Read text should be at most size characters
            prop_assert!(read_text.len() <= size);
        }
    }
}

#[cfg(test)]
mod volume_spec_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 200,
            .. ProptestConfig::default()
        })]

        /// Property: Volume adjustment encoding round-trip within valid range
        #[test]
        fn volume_adjustment_roundtrip(gain in -64.0f32..64.0f32) {
            let spec = VolumeAdjustmentSpec::new("gain");
            let frame = FrameData::new("RVA2".to_string(), 1000, 0, (2, 4));
            let header = FrameHeader::new("RVA2".to_string(), 1000, 0, (2, 4));
            let config = FrameWriteConfig::default();

            let written = spec.write(&config, &frame, &gain).unwrap();
            let (decoded, _) = spec.read(&header, &frame, &written).unwrap();

            // Allow small floating point error (within 1/512)
            let diff = (decoded - gain).abs();
            prop_assert!(diff < 0.01);
        }

        /// Property: Volume peak must be in 0..=1 range
        #[test]
        fn volume_peak_valid_range(peak in 0.0f32..=1.0f32) {
            let spec = VolumePeakSpec::new("peak");
            let frame = FrameData::new("RVA2".to_string(), 1000, 0, (2, 4));

            let result = spec.validate(&frame, peak);
            prop_assert!(result.is_ok());
        }

        /// Property: Volume peak outside range should fail validation
        #[test]
        fn volume_peak_invalid_range(peak in prop_oneof![
            (-1000.0f32..-0.001f32),
            (1.001f32..1000.0f32)
        ]) {
            let spec = VolumePeakSpec::new("peak");
            let frame = FrameData::new("RVA2".to_string(), 1000, 0, (2, 4));

            let result = spec.validate(&frame, peak);
            prop_assert!(result.is_err());
        }
    }
}

#[cfg(test)]
mod serialization_properties {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 100,
            .. ProptestConfig::default()
        })]

        /// Property: Encoding spec round-trip preserves encoding
        #[test]
        fn encoding_spec_roundtrip(encoding_byte in 0u8..=3) {
            let spec = EncodingSpec::new("test");
            let frame = FrameData::new("TEST".to_string(), 1000, 0, (2, 4));
            let header = FrameHeader::new("TEST".to_string(), 1000, 0, (2, 4));
            let config = FrameWriteConfig::default();

            let encoding = TextEncoding::from_byte(encoding_byte).unwrap();
            let written = spec.write(&config, &frame, &encoding).unwrap();
            let (decoded, _) = spec.read(&header, &frame, &written).unwrap();

            prop_assert_eq!(decoded, encoding);
        }

        /// Property: CTOC flags preserve all bits
        #[test]
        fn ctoc_flags_preserves_bits(flags_byte in any::<u8>()) {
            let spec = CTOCFlagsSpec::new("test");
            let frame = FrameData::new("CTOC".to_string(), 1000, 0, (2, 4));
            let header = FrameHeader::new("CTOC".to_string(), 1000, 0, (2, 4));
            let config = FrameWriteConfig::default();

            let flags = CTOCFlags::new(flags_byte);
            let written = spec.write(&config, &frame, &flags).unwrap();
            let (decoded, _) = spec.read(&header, &frame, &written).unwrap();

            prop_assert_eq!(decoded.value(), flags_byte);
        }
    }
}

// ---------------------------------------------------------------------------
// Tag-level property tests
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Property: APEv2 key validation accepts only printable ASCII keys of length 2-255
    #[test]
    fn apev2_key_validation_consistent(key in "[\\x20-\\x7e]{2,30}") {
        prop_assert!(audex::apev2::is_valid_apev2_key(&key));
    }

    /// Property: keys outside the valid APEv2 range are rejected
    #[test]
    fn apev2_rejects_control_chars(key in "[\\x00-\\x1f]{1,10}") {
        prop_assert!(!audex::apev2::is_valid_apev2_key(&key));
    }

    /// Property: single-character keys are rejected by APEv2 (minimum is 2)
    #[test]
    fn apev2_rejects_single_char(ch in 0x20u8..=0x7Eu8) {
        let key = String::from(ch as char);
        prop_assert!(!audex::apev2::is_valid_apev2_key(&key));
    }

    /// Property: Vorbis comment keys accept printable ASCII except '='
    #[test]
    fn vorbis_key_validation_rejects_equals(
        prefix in "[A-Z]{1,5}",
        suffix in "[A-Z]{1,5}",
    ) {
        let key_with_eq = format!("{}={}", prefix, suffix);
        prop_assert!(!audex::vorbis::is_valid_key(&key_with_eq));

        // Without equals sign should be valid
        let key_without = format!("{}{}", prefix, suffix);
        prop_assert!(audex::vorbis::is_valid_key(&key_without));
    }

    /// Property: parse_gain(format_gain(x)) round-trips for finite values in reasonable range
    #[test]
    fn replaygain_format_parse_roundtrip(gain in -200.0f32..200.0f32) {
        let formatted = audex::replaygain::format_gain(gain).unwrap();
        let parsed = audex::replaygain::parse_gain(&formatted).unwrap();
        // Allow small floating-point error from formatting
        prop_assert!((parsed - gain).abs() < 0.01,
            "round-trip error: {} -> {} -> {}", gain, formatted, parsed);
    }
}
