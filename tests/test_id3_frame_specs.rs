// Comprehensive tests for ID3v2 frame data specifications
//
// This test module validates the correctness of all ID3v2 specification types
// including encoding specs, string specs, binary data specs, timestamp specs,
// volume adjustment specs, and other specialized frame data specifications.

use audex::id3::specs::*;

/// Helper function to create test frame header
fn create_test_header(version: (u8, u8)) -> FrameHeader {
    FrameHeader::new("TEST".to_string(), 100, 0, version)
}

/// Helper function to create test frame data
fn create_test_frame(frame_id: &str, version: (u8, u8)) -> FrameData {
    FrameData::new(frame_id.to_string(), 100, 0, version)
}

#[cfg(test)]
mod synchronized_text_spec_tests {
    use super::*;

    #[test]
    fn test_write_utf16() {
        let spec = SynchronizedTextSpec::new("test");
        let frame = create_test_frame("SYLT", (2, 4));
        let header = create_test_header((2, 4));

        let values = vec![
            ("A".to_string(), 100),
            ("äxy".to_string(), 0),
            ("".to_string(), 42),
            ("".to_string(), 0),
        ];

        // Test write then read round-trip for UTF-8 (encoding 3)
        let config = FrameWriteConfig {
            version: (2, 4),
            use_synchsafe_ints: true,
            default_encoding: TextEncoding::Utf8,
            v23_separator: b'/',
        };

        let written = spec.write(&config, &frame, &values).unwrap();
        let (read_values, consumed) = spec.read(&header, &frame, &written).unwrap();

        assert_eq!(read_values.len(), values.len());
        assert_eq!(consumed, written.len());
    }

    #[test]
    fn test_write_single_entry() {
        let spec = SynchronizedTextSpec::new("test");
        let frame = create_test_frame("SYLT", (2, 4));

        let values = vec![("A".to_string(), 100)];

        let config = FrameWriteConfig {
            version: (2, 4),
            use_synchsafe_ints: true,
            default_encoding: TextEncoding::Utf8,
            v23_separator: b'/',
        };

        let written = spec.write(&config, &frame, &values).unwrap();

        // Expected: "A" (1 byte) + \x00 (terminator) + timestamp (4 bytes)
        // = 6 bytes total
        assert_eq!(written.len(), 6);
        assert_eq!(written[0], b'A');
        assert_eq!(written[1], 0x00); // terminator
        assert_eq!(written[2], 0x00); // timestamp byte 1
        assert_eq!(written[3], 0x00); // timestamp byte 2
        assert_eq!(written[4], 0x00); // timestamp byte 3
        assert_eq!(written[5], 100); // timestamp byte 4
    }
}

#[cfg(test)]
mod timestamp_spec_tests {
    use super::*;

    #[test]
    fn test_read() {
        let spec = TimeStampSpec::new("test");
        let frame = create_test_frame("TDRC", (2, 4));
        let header = create_test_header((2, 4));

        // Encoding byte (0) + "ab" + null + "fg"
        let data = b"\x00ab\x00fg";
        let (timestamp, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(timestamp.text, "ab");
        assert_eq!(consumed, 4); // encoding + "ab" + null
    }

    #[test]
    fn test_read_year() {
        let spec = TimeStampSpec::new("test");
        let frame = create_test_frame("TDRC", (2, 4));
        let header = create_test_header((2, 4));

        // Encoding byte (0) + "1234" + null
        let data = b"\x001234\x00";
        let (timestamp, _) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(timestamp.text, "1234");
    }

    #[test]
    fn test_write() {
        let spec = TimeStampSpec::new("test");
        let frame = create_test_frame("TDRC", (2, 4));

        let config = FrameWriteConfig {
            version: (2, 4),
            use_synchsafe_ints: true,
            default_encoding: TextEncoding::Latin1,
            v23_separator: b'/',
        };

        let timestamp = ID3TimeStamp::new("1234".to_string());
        let written = spec.write(&config, &frame, &timestamp).unwrap();

        // Should be: encoding byte + "1234" + null
        assert_eq!(written, b"\x001234\x00");
    }
}

#[cfg(test)]
mod encoded_text_spec_tests {
    use super::*;

    #[test]
    fn test_read() {
        let spec = EncodedTextSpec::new("test");
        let frame = create_test_frame("TIT2", (2, 4));
        let header = create_test_header((2, 4));

        // Encoding byte (0 = Latin1) + "abcd" + null + "fg"
        let data = b"\x00abcd\x00fg";
        let (text, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(text, "abcd");
        assert_eq!(consumed, 6); // encoding + "abcd" + null
    }

    #[test]
    fn test_write() {
        let spec = EncodedTextSpec::new("test");
        let frame = create_test_frame("TIT2", (2, 4));

        let config = FrameWriteConfig {
            version: (2, 4),
            use_synchsafe_ints: true,
            default_encoding: TextEncoding::Latin1,
            v23_separator: b'/',
        };

        let written = spec.write(&config, &frame, &"abcdefg".to_string()).unwrap();

        // Should be: encoding byte + text + null terminator
        assert_eq!(written, b"\x00abcdefg\x00");
    }
}

#[cfg(test)]
mod encoding_spec_tests {
    use super::*;

    #[test]
    fn test_read() {
        let spec = EncodingSpec::new("test");
        let frame = create_test_frame("TIT2", (2, 4));
        let header = create_test_header((2, 4));

        let data = b"\x03abcdefg";
        let (encoding, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(encoding, TextEncoding::Utf8);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_read_invalid_encoding() {
        let spec = EncodingSpec::new("test");
        let frame = create_test_frame("TIT2", (2, 4));
        let header = create_test_header((2, 4));

        let data = b"\x04abcdefg";
        let result = spec.read(&header, &frame, data);

        assert!(result.is_err());
    }

    #[test]
    fn test_write() {
        let spec = EncodingSpec::new("test");
        let frame = create_test_frame("TIT2", (2, 4));

        let config = FrameWriteConfig::default();
        let written = spec.write(&config, &frame, &TextEncoding::Latin1).unwrap();

        assert_eq!(written, b"\x00");
    }

    #[test]
    fn test_validate() {
        let spec = EncodingSpec::new("test");
        let frame = create_test_frame("TIT2", (2, 4));

        let result = spec.validate(&frame, TextEncoding::Utf8);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod aspi_index_spec_tests {
    use super::*;

    #[test]
    fn test_read_16bit_entries() {
        let spec = ASPIIndexSpec::new("test");
        let frame = create_test_frame("ASPI", (2, 4));
        let header = create_test_header((2, 4));

        // Two 16-bit values: 256 (0x0100) and 1 (0x0001)
        let data = b"\x01\x00\x00\x01";
        let (indices, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(indices.len(), 2);
        assert_eq!(indices[0], 256);
        assert_eq!(indices[1], 1);
        assert_eq!(consumed, 4);
    }

    #[test]
    fn test_read_empty_data() {
        let spec = ASPIIndexSpec::new("test");
        let frame = create_test_frame("ASPI", (2, 4));
        let header = create_test_header((2, 4));

        let data = b"";
        let (indices, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(indices.len(), 0);
        assert_eq!(consumed, 0);
    }
}

#[cfg(test)]
mod volume_adjustment_spec_tests {
    use super::*;

    #[test]
    fn test_validate() {
        let spec = VolumeAdjustmentSpec::new("gain");
        let frame = create_test_frame("RVA2", (2, 4));

        // Value out of range
        let result = spec.validate(&frame, 65.0);
        assert!(result.is_err());

        // Valid value
        let result = spec.validate(&frame, 2.0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_read() {
        let spec = VolumeAdjustmentSpec::new("gain");
        let frame = create_test_frame("RVA2", (2, 4));
        let header = create_test_header((2, 4));

        // 0.0 dB
        let (gain, _) = spec.read(&header, &frame, b"\x00\x00").unwrap();
        assert_eq!(gain, 0.0);

        // 2.0 dB (0x0400 = 1024, 1024/512 = 2.0)
        let (gain, _) = spec.read(&header, &frame, b"\x04\x00").unwrap();
        assert_eq!(gain, 2.0);

        // -2.0 dB (0xFC00 = -1024 signed, -1024/512 = -2.0)
        let (gain, _) = spec.read(&header, &frame, b"\xfc\x00").unwrap();
        assert_eq!(gain, -2.0);
    }

    #[test]
    fn test_write() {
        let spec = VolumeAdjustmentSpec::new("gain");
        let frame = create_test_frame("RVA2", (2, 4));
        let config = FrameWriteConfig::default();

        let written = spec.write(&config, &frame, &0.0).unwrap();
        assert_eq!(written, b"\x00\x00");

        let written = spec.write(&config, &frame, &2.0).unwrap();
        assert_eq!(written, b"\x04\x00");

        let written = spec.write(&config, &frame, &-2.0).unwrap();
        assert_eq!(written, b"\xfc\x00");
    }
}

#[cfg(test)]
mod byte_spec_tests {
    use super::*;

    #[test]
    fn test_validate() {
        let spec = ByteSpec::new("test");
        let frame = create_test_frame("TEST", (2, 4));

        // Valid byte value
        let result = spec.validate(&frame, 97);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 97);
    }

    #[test]
    fn test_read() {
        let spec = ByteSpec::new("test");
        let frame = create_test_frame("TEST", (2, 4));
        let header = create_test_header((2, 4));

        let data = b"abcdefg";
        let (byte, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(byte, 97); // 'a'
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_write() {
        let spec = ByteSpec::new("test");
        let frame = create_test_frame("TEST", (2, 4));
        let config = FrameWriteConfig::default();

        let written = spec.write(&config, &frame, &97).unwrap();
        assert_eq!(written, b"a");
    }
}

#[cfg(test)]
mod volume_peak_spec_tests {
    use super::*;

    #[test]
    fn test_validate() {
        let spec = VolumePeakSpec::new("peak");
        let frame = create_test_frame("RVA2", (2, 4));

        // Value out of range
        let result = spec.validate(&frame, 2.0);
        assert!(result.is_err());

        // Valid value
        let result = spec.validate(&frame, 0.5);
        assert!(result.is_ok());
    }

    #[test]
    fn test_write() {
        let spec = VolumePeakSpec::new("peak");
        let frame = create_test_frame("RVA2", (2, 4));
        let config = FrameWriteConfig::default();

        let written = spec.write(&config, &frame, &0.5).unwrap();

        // Should be 3 bytes: bits indicator + 2 bytes for 16-bit value
        assert_eq!(written.len(), 3);
        assert_eq!(written[0], 0x10); // 16 bits
    }
}

#[cfg(test)]
mod string_spec_tests {
    use super::*;

    #[test]
    fn test_validate() {
        let spec = StringSpec::new("test", 3);
        let frame = create_test_frame("TEST", (2, 4));

        // Correct length
        let result = spec.validate(&frame, "ABC".to_string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "ABC");

        // Wrong length
        let result = spec.validate(&frame, "ab".to_string());
        assert!(result.is_err());

        let result = spec.validate(&frame, "abc2".to_string());
        assert!(result.is_err());

        // Non-ASCII characters
        let result = spec.validate(&frame, "öäü".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_read() {
        let spec = StringSpec::new("test", 3);
        let frame = create_test_frame("TEST", (2, 4));
        let header = create_test_header((2, 4));

        let data = b"abcdefg";
        let (text, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(text, "abc");
        assert_eq!(consumed, 3);
    }

    #[test]
    fn test_read_invalid_utf8() {
        let spec = StringSpec::new("test", 3);
        let frame = create_test_frame("TEST", (2, 4));
        let header = create_test_header((2, 4));

        let data = b"\xff\xfe\xfd";
        let result = spec.read(&header, &frame, data);

        assert!(result.is_err());
    }

    #[test]
    fn test_write() {
        let spec = StringSpec::new("test", 3);
        let frame = create_test_frame("TEST", (2, 4));
        let config = FrameWriteConfig::default();

        // Exact size
        let written = spec.write(&config, &frame, &"abc".to_string()).unwrap();
        assert_eq!(written, b"abc");

        // Longer string (truncated)
        let written = spec.write(&config, &frame, &"abcdefg".to_string()).unwrap();
        assert_eq!(written, b"abc");

        // Shorter string (padded with nulls)
        let written = spec.write(&config, &frame, &"a".to_string()).unwrap();
        assert_eq!(written, b"a\x00\x00");

        // Single null character
        let written = spec.write(&config, &frame, &"\x00".to_string()).unwrap();
        assert_eq!(written, b"\x00\x00\x00");
    }
}

#[cfg(test)]
mod binary_data_spec_tests {
    use super::*;

    #[test]
    fn test_validate() {
        let spec = BinaryDataSpec::new("test");
        let frame = create_test_frame("TEST", (2, 4));

        // Valid binary data
        let result = spec.validate(&frame, b"abc".to_vec());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"abc");
    }

    #[test]
    fn test_read() {
        let spec = BinaryDataSpec::new("test");
        let frame = create_test_frame("TEST", (2, 4));
        let header = create_test_header((2, 4));

        let data = b"abcdefg";
        let (binary, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(binary, b"abcdefg");
        assert_eq!(consumed, 7);
    }

    #[test]
    fn test_write() {
        let spec = BinaryDataSpec::new("test");
        let frame = create_test_frame("TEST", (2, 4));
        let config = FrameWriteConfig::default();

        let written = spec.write(&config, &frame, &b"abc".to_vec()).unwrap();
        assert_eq!(written, b"abc");
    }
}

#[cfg(test)]
mod rva_spec_tests {
    use super::*;

    #[test]
    fn test_read() {
        let spec = RVASpec::new("test", false);
        let frame = create_test_frame("RVAD", (2, 3));
        let header = create_test_header((2, 3));

        let data = b"\x03\x10\xc7\xc7\xc7\xc7\x00\x00\x00\x00\x00\x00\x00\x00";
        let (values, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(consumed, 14);
        assert_eq!(values.len(), 6);
        assert_eq!(values[0], 51143);
        assert_eq!(values[1], 51143);
        assert_eq!(values[2], 0);
        assert_eq!(values[3], 0);
    }

    #[test]
    fn test_read_stereo_only() {
        let spec = RVASpec::new("test", true);
        let frame = create_test_frame("RVAD", (2, 3));
        let header = create_test_header((2, 3));

        let data = b"\x03\x10\xc7\xc7\xc7\xc7\x00\x00\x00\x00\x00\x00\x00\x00";
        let (values, consumed) = spec.read(&header, &frame, data).unwrap();

        // Stereo only reads 4 values
        assert_eq!(consumed, 10);
        assert_eq!(values.len(), 4);
        assert_eq!(values[0], 51143);
        assert_eq!(values[1], 51143);
    }

    #[test]
    fn test_write() {
        let spec = RVASpec::new("test", false);
        let frame = create_test_frame("RVAD", (2, 3));
        let config = FrameWriteConfig::default();

        let values = vec![0, 1, 2, 3, -4, -5];
        let written = spec.write(&config, &frame, &values).unwrap();

        // Validate structure: flags byte + bits byte + values
        assert!(written.len() >= 2);
        assert_eq!(written[0] & 0x0F, 0x03); // Flags for first two increments
        assert_eq!(written[1], 16); // 16 bits = 2 bytes per value
    }

    #[test]
    fn test_write_stereo_only_error() {
        let spec = RVASpec::new("test", true);
        let frame = create_test_frame("RVAD", (2, 3));
        let config = FrameWriteConfig::default();

        // Trying to write 6 values for stereo-only spec should fail
        let values = vec![0, 0, 0, 0, 0, 0];
        let result = spec.write(&config, &frame, &values);

        assert!(result.is_err());
    }

    #[test]
    fn test_validate() {
        let spec = RVASpec::new("test", false);
        let frame = create_test_frame("RVAD", (2, 3));

        // Empty list
        let result = spec.validate(&frame, vec![]);
        assert!(result.is_err());

        // Valid list
        let result = spec.validate(&frame, vec![1, 2]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![1, 2]);
    }
}

#[cfg(test)]
mod frame_id_spec_tests {
    use super::*;

    #[test]
    fn test_read() {
        let spec = FrameIDSpec::new("test", 3);
        let frame = create_test_frame("TEST", (2, 2));
        let header = create_test_header((2, 2));

        let data = b"FOOX";
        let (frame_id, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(frame_id, "FOO");
        assert_eq!(consumed, 3);
    }

    #[test]
    fn test_validate_v22() {
        let spec = FrameIDSpec::new("test", 3);
        let frame = create_test_frame("TT2", (2, 2));

        // Invalid: contains numbers
        let result = spec.validate(&frame, "123".to_string());
        assert!(result.is_err());

        // Invalid: reserved frame ID (TXXX is 4 chars, not valid for 3-char spec)
        // Valid 3-char ID
        let result = spec.validate(&frame, "TT2".to_string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "TT2");
    }

    #[test]
    fn test_validate_v24() {
        let spec = FrameIDSpec::new("test", 4);
        let frame = create_test_frame("TXXX", (2, 4));

        // Valid 4-char frame ID
        let result = spec.validate(&frame, "TXXX".to_string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "TXXX");
    }
}

#[cfg(test)]
mod ctoc_flags_spec_tests {
    use super::*;

    #[test]
    fn test_read() {
        let spec = CTOCFlagsSpec::new("test");
        let frame = create_test_frame("CTOC", (2, 4));
        let header = create_test_header((2, 4));

        let data = b"\x03";
        let (flags, consumed) = spec.read(&header, &frame, data).unwrap();

        assert_eq!(consumed, 1);
        assert_eq!(flags.value(), 3);
        assert!(flags.is_ordered());
        assert!(flags.is_top_level());
    }

    #[test]
    fn test_write() {
        let spec = CTOCFlagsSpec::new("test");
        let frame = create_test_frame("CTOC", (2, 4));
        let config = FrameWriteConfig::default();

        let flags = CTOCFlags::new(CTOCFlags::ORDERED);
        let written = spec.write(&config, &frame, &flags).unwrap();

        assert_eq!(written, b"\x01");
    }

    #[test]
    fn test_validate() {
        let spec = CTOCFlagsSpec::new("test");
        let frame = create_test_frame("CTOC", (2, 4));

        let flags = CTOCFlags::new(3);
        let result = spec.validate(&frame, flags);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().value(), 3);
    }
}

#[cfg(test)]
mod latin1_text_list_spec_tests {
    use super::*;

    #[test]
    fn test_read() {
        let spec = Latin1TextListSpec::new("test");
        let frame = create_test_frame("TEST", (2, 4));
        let header = create_test_header((2, 4));

        // Count = 0
        let (texts, consumed) = spec.read(&header, &frame, b"\x00xxx").unwrap();
        assert_eq!(texts.len(), 0);
        assert_eq!(consumed, 1);

        // Count = 1, one string
        let (texts, _) = spec.read(&header, &frame, b"\x01foo\x00").unwrap();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0], "foo");

        // Count = 1, empty string
        let (texts, _) = spec.read(&header, &frame, b"\x01\x00").unwrap();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0], "");

        // Count = 2, two strings
        let (texts, _) = spec.read(&header, &frame, b"\x02f\x00o\x00").unwrap();
        assert_eq!(texts.len(), 2);
        assert_eq!(texts[0], "f");
        assert_eq!(texts[1], "o");
    }

    #[test]
    fn test_write() {
        let spec = Latin1TextListSpec::new("test");
        let frame = create_test_frame("TEST", (2, 4));
        let config = FrameWriteConfig::default();

        // Empty list
        let written = spec.write(&config, &frame, &vec![]).unwrap();
        assert_eq!(written, b"\x00");

        // Single empty string
        let written = spec.write(&config, &frame, &vec!["".to_string()]).unwrap();
        assert_eq!(written, b"\x01\x00");
    }

    #[test]
    fn test_validate() {
        let spec = Latin1TextListSpec::new("test");
        let frame = create_test_frame("TEST", (2, 4));

        // Valid list
        let result = spec.validate(&frame, vec!["foo".to_string()]);
        assert!(result.is_ok());

        // Empty list
        let result = spec.validate(&frame, vec![]);
        assert!(result.is_ok());
    }
}

#[cfg(test)]
mod spec_error_tests {
    use super::*;

    #[test]
    fn test_spec_error_display() {
        let error = SpecError::new("TestSpec", "Test error message".to_string());

        assert_eq!(error.spec_name, "TestSpec");
        assert_eq!(error.message, "Test error message");

        let display = format!("{}", error);
        assert!(display.contains("TestSpec"));
        assert!(display.contains("Test error message"));
    }
}

#[cfg(test)]
mod text_encoding_tests {
    use super::*;

    #[test]
    fn test_from_byte() {
        assert_eq!(TextEncoding::from_byte(0).unwrap(), TextEncoding::Latin1);
        assert_eq!(TextEncoding::from_byte(1).unwrap(), TextEncoding::Utf16);
        assert_eq!(TextEncoding::from_byte(2).unwrap(), TextEncoding::Utf16Be);
        assert_eq!(TextEncoding::from_byte(3).unwrap(), TextEncoding::Utf8);

        // Invalid encoding
        assert!(TextEncoding::from_byte(4).is_err());
    }

    #[test]
    fn test_to_byte() {
        assert_eq!(TextEncoding::Latin1.to_byte(), 0);
        assert_eq!(TextEncoding::Utf16.to_byte(), 1);
        assert_eq!(TextEncoding::Utf16Be.to_byte(), 2);
        assert_eq!(TextEncoding::Utf8.to_byte(), 3);
    }

    #[test]
    fn test_null_terminator() {
        assert_eq!(TextEncoding::Latin1.null_terminator(), b"\x00");
        assert_eq!(TextEncoding::Utf8.null_terminator(), b"\x00");
        assert_eq!(TextEncoding::Utf16.null_terminator(), b"\x00\x00");
        assert_eq!(TextEncoding::Utf16Be.null_terminator(), b"\x00\x00");
    }

    #[test]
    fn test_is_valid_for_version() {
        // UTF-8 only valid for v2.4+
        assert!(!TextEncoding::Utf8.is_valid_for_version((2, 3)));
        assert!(TextEncoding::Utf8.is_valid_for_version((2, 4)));

        // Others valid for all versions
        assert!(TextEncoding::Latin1.is_valid_for_version((2, 3)));
        assert!(TextEncoding::Utf16.is_valid_for_version((2, 3)));
    }
}

#[cfg(test)]
mod frame_header_tests {
    use super::*;

    #[test]
    fn test_from_bytes_v24() {
        // "TEST" frame ID, size 100 (synchsafe), no flags.
        // Append 100 zero bytes as payload so the declared size does not
        // exceed the remaining data after the 10-byte header.
        let mut data = b"TEST\x00\x00\x00\x64\x00\x00".to_vec();
        data.extend_from_slice(&[0u8; 100]);

        let header = FrameHeader::from_bytes(&data, (2, 4)).unwrap();

        assert_eq!(header.frame_id, "TEST");
        assert_eq!(header.size, 100);
        assert_eq!(header.version, (2, 4));
    }

    #[test]
    fn test_from_bytes_v23() {
        // "TEST" frame ID, size 100 (regular), no flags.
        // Append 100 zero bytes as payload so the declared size does not
        // exceed the remaining data after the 10-byte header.
        let mut data = b"TEST\x00\x00\x00\x64\x00\x00".to_vec();
        data.extend_from_slice(&[0u8; 100]);

        let header = FrameHeader::from_bytes(&data, (2, 3)).unwrap();

        assert_eq!(header.frame_id, "TEST");
        assert_eq!(header.size, 100);
        assert_eq!(header.version, (2, 3));
    }

    #[test]
    fn test_to_bytes_v24() {
        let header = FrameHeader::new("TEST".to_string(), 100, 0, (2, 4));
        let bytes = header.to_bytes().unwrap();

        assert_eq!(&bytes[0..4], b"TEST");
        // Size should be synchsafe encoded
        assert_eq!(&bytes[4..8], &[0x00, 0x00, 0x00, 0x64]);
    }

    #[test]
    fn test_invalid_frame_id() {
        let data = b"\xFF\xFE\xFD\xFC\x00\x00\x00\x64\x00\x00";

        let result = FrameHeader::from_bytes(data, (2, 4));
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod frame_flags_tests {
    use super::*;

    #[test]
    fn test_from_v24() {
        let flags = FrameFlags::from_raw(0x4000, (2, 4));

        assert!(flags.alter_tag);
        assert!(!flags.alter_file);
    }

    #[test]
    fn test_from_v23() {
        let flags = FrameFlags::from_raw(0x8000, (2, 3));

        assert!(flags.alter_tag);
        assert!(!flags.alter_file);
    }

    #[test]
    fn test_to_v24() {
        let mut flags = FrameFlags::new();
        flags.alter_tag = true;
        flags.compression = true;

        let raw = flags.to_raw((2, 4));
        assert_eq!(raw, 0x4008);
    }

    #[test]
    fn test_to_v23() {
        let mut flags = FrameFlags::new();
        flags.alter_tag = true;
        flags.compression = true;

        let raw = flags.to_raw((2, 3));
        assert_eq!(raw, 0x8080);
    }

    #[test]
    fn test_validate_encryption() {
        let mut flags = FrameFlags::new();
        flags.encryption = true;

        let result = flags.validate((2, 4));
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod id3_header_tests {
    use super::*;

    #[test]
    fn test_from_bytes() {
        let data = b"ID3\x04\x00\x00\x00\x00\x00\x64extra";

        let header = ID3Header::from_bytes(data).unwrap();

        assert_eq!(header.major_version, 4);
        assert_eq!(header.revision, 0);
        assert_eq!(header.flags, 0);
        assert_eq!(header.size, 100);
    }

    #[test]
    fn test_to_bytes() {
        let header = ID3Header::new(4, 0, 0, 100);
        let bytes = header.to_bytes().unwrap();

        assert_eq!(&bytes[0..3], b"ID3");
        assert_eq!(bytes[3], 4);
        assert_eq!(bytes[4], 0);
    }

    #[test]
    fn test_version() {
        let header = ID3Header::new(4, 0, 0, 100);
        assert_eq!(header.version(), (4, 0));
    }

    #[test]
    fn test_has_unsynchronization() {
        let header = ID3Header::new(4, 0, 0x80, 100);
        assert!(header.has_unsynchronization());

        let header = ID3Header::new(4, 0, 0x00, 100);
        assert!(!header.has_unsynchronization());
    }

    #[test]
    fn test_invalid_signature() {
        let data = b"XXX\x04\x00\x00\x00\x00\x00\x64";
        let result = ID3Header::from_bytes(data);

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod time_stamp_spec_tests {
    use super::*;

    #[test]
    fn test_read() {
        let s = TimeStampSpec::new("name");
        let header = create_test_header((2, 4));
        let frame = create_test_frame("TEST", (2, 4));

        // Test reading timestamp with encoding byte and remaining data
        let data = b"\x03ab\x00fg"; // UTF-8 encoding + "ab" + null + remaining
        let result = s.read(&header, &frame, data);
        assert!(result.is_ok());
        let (timestamp, consumed) = result.unwrap();
        assert_eq!(timestamp.text, "ab");
        assert_eq!(consumed, 4); // encoding + "ab" + null

        // Test reading timestamp at end of data
        let data = b"\x031234\x00"; // UTF-8 encoding + "1234" + null
        let result = s.read(&header, &frame, data);
        assert!(result.is_ok());
        let (timestamp, consumed) = result.unwrap();
        assert_eq!(timestamp.text, "1234");
        assert_eq!(consumed, 6); // encoding + "1234" + null
    }

    #[test]
    fn test_write() {
        let s = TimeStampSpec::new("name");
        let frame = create_test_frame("TEST", (2, 4));
        let config = FrameWriteConfig::default();

        let timestamp = ID3TimeStamp::new("1234".to_string());
        let result = s.write(&config, &frame, &timestamp);
        assert!(result.is_ok());
        let data = result.unwrap();
        // TimeStampSpec uses EncodedTextSpec internally, so it includes encoding byte
        assert_eq!(data[0], 3); // UTF-8 encoding
        assert_eq!(&data[1..], b"1234\x00"); // Text + null terminator
    }
}

#[cfg(test)]
mod spec_validation_edge_cases {
    use super::*;
    use audex::id3::util::{decode_synchsafe_int, encode_synchsafe_int};
    use audex::util::BitPaddedInt;

    /// Test syncsafe integer boundary values
    /// Syncsafe integers use 7 bits per byte, preventing false sync patterns
    #[test]
    fn test_syncsafe_integer_boundaries() {
        // Test minimum value (0)
        let min_val = 0u32;
        let encoded_min = encode_synchsafe_int(min_val).unwrap();
        assert_eq!(encoded_min, [0x00, 0x00, 0x00, 0x00]);
        assert_eq!(decode_synchsafe_int(&encoded_min), min_val);

        // Test 7-bit boundary (127 = 0x7F)
        let val_7bit = 0x7F;
        let encoded_7bit = encode_synchsafe_int(val_7bit).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_7bit), val_7bit);
        // Ensure no byte has MSB set
        for &byte in &encoded_7bit {
            assert_eq!(byte & 0x80, 0);
        }

        // Test 14-bit boundary (16383 = 0x3FFF)
        let val_14bit = 0x3FFF;
        let encoded_14bit = encode_synchsafe_int(val_14bit).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_14bit), val_14bit);
        for &byte in &encoded_14bit {
            assert_eq!(byte & 0x80, 0);
        }

        // Test 21-bit boundary (2097151 = 0x1FFFFF)
        let val_21bit = 0x1FFFFF;
        let encoded_21bit = encode_synchsafe_int(val_21bit).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_21bit), val_21bit);
        for &byte in &encoded_21bit {
            assert_eq!(byte & 0x80, 0);
        }

        // Test maximum value for 4-byte syncsafe (28-bit: 268435455 = 0x0FFFFFFF)
        let max_val = 0x0FFFFFFFu32;
        let encoded_max = encode_synchsafe_int(max_val).unwrap();
        assert_eq!(encoded_max, [0x7F, 0x7F, 0x7F, 0x7F]);
        assert_eq!(decode_synchsafe_int(&encoded_max), max_val);
        for &byte in &encoded_max {
            assert_eq!(byte & 0x80, 0);
        }
    }

    /// Test BitPaddedInt with various bit widths
    #[test]
    fn test_bitpadded_int_boundaries() {
        // Test 7-bit encoding (standard syncsafe)
        let val = 127u64;
        let bpi = BitPaddedInt::new(val, 7, true).unwrap();
        assert_eq!(u64::from(bpi), val);

        // Test 8-bit encoding (maximum allowed)
        let val_8bit = 255u64;
        let bpi_8bit = BitPaddedInt::new(val_8bit, 8, true).unwrap();
        assert_eq!(u64::from(bpi_8bit), val_8bit);

        // Test invalid bit width (should fail)
        let result = BitPaddedInt::new(100, 9, true);
        assert!(result.is_err());

        // Test boundary value for 7-bit, 4-byte encoding
        let max_7bit_4byte = 0x0FFFFFFFu64;
        let bpi_max = BitPaddedInt::new(max_7bit_4byte, 7, true).unwrap();
        assert_eq!(u64::from(bpi_max), max_7bit_4byte);
    }

    /// Test maximum frame size handling.
    ///
    /// The maximum synchsafe size (268,435,455 bytes) cannot be backed by a
    /// real payload buffer in tests, so we verify that `from_bytes` correctly
    /// rejects the header when the remaining data is insufficient.
    #[test]
    fn test_maximum_frame_size() {
        // ID3v2.4 maximum frame size is 0x0FFFFFFF (268,435,455 bytes)
        let max_size = 0x0FFFFFFFu32;

        // Create frame header with maximum size
        let header = FrameHeader::new("TEST".to_string(), max_size, 0, (2, 4));
        assert_eq!(header.size, max_size);

        // Convert to bytes — verify synchsafe encoding
        let bytes = header.to_bytes().unwrap();
        let size_bytes = &bytes[4..8];
        assert_eq!(size_bytes, [0x7F, 0x7F, 0x7F, 0x7F]);

        // Parsing should fail because the 10-byte buffer has no payload to
        // back the declared 268 MB frame size.
        let result = FrameHeader::from_bytes(&bytes, (2, 4));
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("exceeds remaining tag data"),
            "Expected size-exceeds-data error, got: {}",
            err_msg
        );
    }

    /// Test frame size near boundaries.
    ///
    /// Each serialized header is padded with enough zero bytes to satisfy the
    /// frame-size-vs-remaining-data validation in `from_bytes`.
    #[test]
    fn test_frame_size_boundaries() {
        // Test small frame size (1 byte)
        let small_header = FrameHeader::new("TIT2".to_string(), 1, 0, (2, 4));
        let mut bytes = small_header.to_bytes().unwrap();
        bytes.extend_from_slice(&[0u8; 1]);
        let parsed = FrameHeader::from_bytes(&bytes, (2, 4)).unwrap();
        assert_eq!(parsed.size, 1);

        // Test medium frame size (64KB)
        let medium_header = FrameHeader::new("TIT2".to_string(), 65536, 0, (2, 4));
        let mut bytes = medium_header.to_bytes().unwrap();
        bytes.resize(10 + 65536, 0);
        let parsed = FrameHeader::from_bytes(&bytes, (2, 4)).unwrap();
        assert_eq!(parsed.size, 65536);

        // Test large frame size (1MB)
        let large_header = FrameHeader::new("TIT2".to_string(), 1048576, 0, (2, 4));
        let mut bytes = large_header.to_bytes().unwrap();
        bytes.resize(10 + 1048576, 0);
        let parsed = FrameHeader::from_bytes(&bytes, (2, 4)).unwrap();
        assert_eq!(parsed.size, 1048576);

        // Test ID3v2.3 (non-syncsafe) frame sizes
        let v23_header = FrameHeader::new("TIT2".to_string(), 1000, 0, (2, 3));
        let mut bytes = v23_header.to_bytes().unwrap();
        bytes.resize(10 + 1000, 0);
        let parsed = FrameHeader::from_bytes(&bytes, (2, 3)).unwrap();
        assert_eq!(parsed.size, 1000);
    }
}

#[cfg(test)]
mod integer_encoding_boundary_tests {
    use audex::id3::util::{decode_synchsafe_int, encode_synchsafe_int};
    use audex::util::BitPaddedInt;

    /// Test 7-bit boundary encoding
    #[test]
    fn test_7bit_boundary() {
        // Maximum value for 1 byte with 7 bits: 127 (0x7F)
        let val = 127u32;
        let encoded = encode_synchsafe_int(val).unwrap();
        let decoded = decode_synchsafe_int(&encoded);
        assert_eq!(decoded, val);

        // Just below boundary
        let val_below = 126u32;
        let encoded_below = encode_synchsafe_int(val_below).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_below), val_below);

        // Just above boundary (requires second byte)
        let val_above = 128u32;
        let encoded_above = encode_synchsafe_int(val_above).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_above), val_above);
    }

    /// Test 14-bit boundary encoding
    #[test]
    fn test_14bit_boundary() {
        // Maximum value for 2 bytes with 7 bits each: 16383 (0x3FFF)
        let val = 16383u32;
        let encoded = encode_synchsafe_int(val).unwrap();
        let decoded = decode_synchsafe_int(&encoded);
        assert_eq!(decoded, val);

        // Just below boundary
        let val_below = 16382u32;
        let encoded_below = encode_synchsafe_int(val_below).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_below), val_below);

        // Just above boundary (requires third byte)
        let val_above = 16384u32;
        let encoded_above = encode_synchsafe_int(val_above).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_above), val_above);
    }

    /// Test 21-bit boundary encoding
    #[test]
    fn test_21bit_boundary() {
        // Maximum value for 3 bytes with 7 bits each: 2097151 (0x1FFFFF)
        let val = 2097151u32;
        let encoded = encode_synchsafe_int(val).unwrap();
        let decoded = decode_synchsafe_int(&encoded);
        assert_eq!(decoded, val);

        // Just below boundary
        let val_below = 2097150u32;
        let encoded_below = encode_synchsafe_int(val_below).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_below), val_below);

        // Just above boundary (requires fourth byte)
        let val_above = 2097152u32;
        let encoded_above = encode_synchsafe_int(val_above).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_above), val_above);
    }

    /// Test 28-bit boundary encoding
    #[test]
    fn test_28bit_boundary() {
        // Maximum value for 4 bytes with 7 bits each: 268435455 (0x0FFFFFFF)
        let val = 268435455u32;
        let encoded = encode_synchsafe_int(val).unwrap();
        let decoded = decode_synchsafe_int(&encoded);
        assert_eq!(decoded, val);

        // Just below boundary
        let val_below = 268435454u32;
        let encoded_below = encode_synchsafe_int(val_below).unwrap();
        assert_eq!(decode_synchsafe_int(&encoded_below), val_below);

        // Verify encoding format
        assert_eq!(encoded, [0x7F, 0x7F, 0x7F, 0x7F]);
    }

    /// Test BitPaddedInt with different byte counts
    #[test]
    fn test_bitpadded_int_byte_boundaries() {
        // Test 1-byte value
        let bytes_1 = [0x7F];
        let bpi_1 = BitPaddedInt::from_bytes(&bytes_1, 7, true).unwrap();
        assert_eq!(u64::from(bpi_1), 127);

        // Test 2-byte value
        let bytes_2 = [0x7F, 0x7F];
        let bpi_2 = BitPaddedInt::from_bytes(&bytes_2, 7, true).unwrap();
        assert_eq!(u64::from(bpi_2), 16383);

        // Test 3-byte value
        let bytes_3 = [0x7F, 0x7F, 0x7F];
        let bpi_3 = BitPaddedInt::from_bytes(&bytes_3, 7, true).unwrap();
        assert_eq!(u64::from(bpi_3), 2097151);

        // Test 4-byte value
        let bytes_4 = [0x7F, 0x7F, 0x7F, 0x7F];
        let bpi_4 = BitPaddedInt::from_bytes(&bytes_4, 7, true).unwrap();
        assert_eq!(u64::from(bpi_4), 268435455);
    }
}

#[cfg(test)]
mod rare_spec_interpretation_tests {
    use super::*;
    use audex::id3::tags::ID3Header;

    /// Test extended header flag handling
    #[test]
    fn test_extended_header_flag() {
        // Create header with extended header flag set
        let mut header = ID3Header::new();
        header.version = (2, 4, 0); // ID3v2.4.0
        header.flags = 0x40; // Extended header flag

        assert!(header.has_extended_header());

        // Create header without extended header flag
        let mut header_no_ext = ID3Header::new();
        header_no_ext.version = (2, 4, 0);
        header_no_ext.flags = 0x00;

        assert!(!header_no_ext.has_extended_header());
    }

    /// Test footer flag combinations (ID3v2.4)
    #[test]
    fn test_footer_flag_combinations() {
        // Test footer flag alone
        let mut header = ID3Header::new();
        header.version = (2, 4, 0);
        header.flags = 0x10; // Footer flag

        assert!(header.f_footer());

        // Test footer flag with extended header
        let mut header_both = ID3Header::new();
        header_both.version = (2, 4, 0);
        header_both.flags = 0x50; // Extended header (0x40) + Footer (0x10)

        assert!(header_both.has_extended_header());
        assert!(header_both.f_footer());

        // Test all flags combined
        let mut header_all = ID3Header::new();
        header_all.version = (2, 4, 0);
        header_all.flags = 0xF0; // All flags set

        assert!(header_all.has_extended_header());
        assert!(header_all.f_footer());
        assert!(header_all.f_unsynch());

        // Test footer flag in ID3v2.3 (should not be supported)
        let mut header_v23 = ID3Header::new();
        header_v23.version = (2, 3, 0);
        header_v23.flags = 0x10; // Footer flag

        // Footer is only valid in v2.4+
        assert_eq!(header_v23.version.1, 3);
    }

    /// Test extended header size edge cases
    #[test]
    fn test_extended_header_size_edge_cases() {
        // Test minimum extended header size
        let header_min = FrameHeader::new("EXTH".to_string(), 6, 0, (2, 4));
        assert_eq!(header_min.size, 6);

        // Test large extended header size
        let header_large = FrameHeader::new("EXTH".to_string(), 65536, 0, (2, 4));
        assert_eq!(header_large.size, 65536);

        // Verify syncsafe encoding round-trips correctly.
        // Pad the buffer with enough zero bytes to satisfy the size validation.
        let mut bytes = header_large.to_bytes().unwrap();
        bytes.resize(10 + 65536, 0);
        let parsed = FrameHeader::from_bytes(&bytes, (2, 4)).unwrap();
        assert_eq!(parsed.size, 65536);
    }

    /// Test frame flags in combination with extended header
    #[test]
    fn test_frame_flags_with_extended_header() {
        // Create frame with compression flag
        let mut flags = FrameFlags::new();
        flags.compression = true;

        let header = FrameHeader {
            frame_id: "TIT2".to_string(),
            size: 100,
            flags: flags.clone(),
            version: (2, 4),
            global_unsync: false,
        };

        assert!(header.flags.compression);

        // Test multiple flags combined
        let mut multi_flags = FrameFlags::new();
        multi_flags.compression = true;
        multi_flags.data_length = true;
        multi_flags.unsync = true;

        let multi_header = FrameHeader {
            frame_id: "TIT2".to_string(),
            size: 200,
            flags: multi_flags,
            version: (2, 4),
            global_unsync: false,
        };

        assert!(multi_header.flags.compression);
        assert!(multi_header.flags.data_length);
        assert!(multi_header.flags.unsync);
    }

    /// Test unsynchronization flag edge cases
    #[test]
    fn test_unsynchronization_edge_cases() {
        // Test unsync flag in ID3v2.4 (supported)
        let mut flags_v24 = FrameFlags::new();
        flags_v24.unsync = true;

        let header_v24 = FrameHeader {
            frame_id: "TIT2".to_string(),
            size: 100,
            flags: flags_v24,
            version: (2, 4),
            global_unsync: false,
        };

        assert!(header_v24.flags.unsync);

        // Test unsync flag in ID3v2.3 (frame-level unsync not standard)
        let flags_v23 = FrameFlags::new();

        let header_v23 = FrameHeader {
            frame_id: "TIT2".to_string(),
            size: 100,
            flags: flags_v23,
            version: (2, 3),
            global_unsync: false,
        };

        // In v2.3, frame-level unsync is not standard
        assert!(!header_v23.flags.unsync);
    }
}

// Regression tests for security and correctness fixes
#[cfg(test)]
mod audit_regression_tests {
    use super::*;
    use std::io::{Cursor, Write};

    use audex::FileType;
    use audex::id3::ID3;
    use audex::id3::specs::{FrameFlags, FrameHeader, FrameProcessor};
    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    // --- zlib bomb tests ---

    fn zlib_compress(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(data).expect("compression write failed");
        encoder.finish().expect("compression finish failed")
    }

    fn compressed_frame_header(frame_data_len: u32) -> FrameHeader {
        let mut flags = FrameFlags::new();
        flags.compression = true;
        FrameHeader {
            frame_id: "TIT2".to_string(),
            size: frame_data_len,
            flags,
            version: (2, 3),
            global_unsync: false,
        }
    }

    fn pack_compressed_frame_body(uncompressed: &[u8]) -> Vec<u8> {
        let compressed = zlib_compress(uncompressed);
        let mut body = Vec::with_capacity(4 + compressed.len());
        body.extend_from_slice(&(uncompressed.len() as u32).to_be_bytes());
        body.extend_from_slice(&compressed);
        body
    }

    fn build_id3v23_with_compressed_frame(decompressed_size: usize) -> Vec<u8> {
        let uncompressed = vec![0u8; decompressed_size];
        let compressed = zlib_compress(&uncompressed);
        let uncompressed_size_be = (decompressed_size as u32).to_be_bytes();

        let frame_data_len = 4 + compressed.len();

        let mut frame_header_bytes = Vec::with_capacity(10);
        frame_header_bytes.extend_from_slice(b"TIT2");
        frame_header_bytes.extend_from_slice(&(frame_data_len as u32).to_be_bytes());
        frame_header_bytes.extend_from_slice(&0x0080u16.to_be_bytes());

        let mut frame_body = Vec::with_capacity(frame_data_len);
        frame_body.extend_from_slice(&uncompressed_size_be);
        frame_body.extend_from_slice(&compressed);

        let tag_payload_len = frame_header_bytes.len() + frame_body.len();

        let tag_size = tag_payload_len as u32;
        let syncsafe = [
            ((tag_size >> 21) & 0x7F) as u8,
            ((tag_size >> 14) & 0x7F) as u8,
            ((tag_size >> 7) & 0x7F) as u8,
            (tag_size & 0x7F) as u8,
        ];

        let mut tag = Vec::with_capacity(10 + tag_payload_len);
        tag.extend_from_slice(b"ID3");
        tag.push(3);
        tag.push(0);
        tag.push(0);
        tag.extend_from_slice(&syncsafe);
        tag.extend_from_slice(&frame_header_bytes);
        tag.extend_from_slice(&frame_body);
        tag
    }

    #[test]
    fn test_oversized_decompression_is_rejected() {
        let decompressed_size: usize = 50 * 1024 * 1024;
        let frame_data = pack_compressed_frame_body(&vec![0u8; decompressed_size]);
        let header = compressed_frame_header(frame_data.len() as u32);

        let result = FrameProcessor::process_read(&header, frame_data);

        assert!(
            result.is_err(),
            "Decompression above the size limit must be rejected"
        );
    }

    #[test]
    fn test_valid_compressed_frame_still_works() {
        let decompressed_size: usize = 1024 * 1024;
        let uncompressed = vec![0u8; decompressed_size];
        let frame_data = pack_compressed_frame_body(&uncompressed);
        let header = compressed_frame_header(frame_data.len() as u32);

        let result = FrameProcessor::process_read(&header, frame_data);

        assert!(
            result.is_ok(),
            "Valid compressed frame should decompress fine"
        );
        assert_eq!(result.unwrap().len(), decompressed_size);
    }

    #[test]
    fn test_id3_load_rejects_zlib_bomb() {
        let decompressed_size: usize = 50 * 1024 * 1024;
        let tag_bytes = build_id3v23_with_compressed_frame(decompressed_size);

        assert!(
            tag_bytes.len() < 100_000,
            "Compressed tag should be small ({} bytes) vs {} MB decompressed",
            tag_bytes.len(),
            decompressed_size / (1024 * 1024)
        );

        let mut cursor = Cursor::new(tag_bytes);
        let result = ID3::load_from_reader(&mut cursor);

        if let Ok(id3) = result {
            assert!(
                id3.get("TIT2").is_none(),
                "Oversized frame should have been rejected during decompression"
            );
        }
    }

    // --- ID3v2.4 data length indicator tests ---

    fn header_with_data_length(frame_data_size: u32) -> FrameHeader {
        let mut flags = FrameFlags::new();
        flags.data_length = true;
        FrameHeader {
            frame_id: "TIT2".to_string(),
            size: frame_data_size,
            flags,
            version: (2, 4),
            global_unsync: false,
        }
    }

    #[test]
    fn test_data_length_indicator_is_syncsafe() {
        let payload = vec![0xABu8; 3_000_000];
        let header = header_with_data_length(0);

        let result = FrameProcessor::process_write(&header, payload).unwrap();

        let indicator = &result[0..4];

        for (i, &byte) in indicator.iter().enumerate() {
            assert_eq!(
                byte & 0x80,
                0,
                "Data length indicator byte {} is 0x{:02X} -- high bit is set, \
                 violating syncsafe encoding",
                i,
                byte
            );
        }

        let decoded = ((indicator[0] as u32) << 21)
            | ((indicator[1] as u32) << 14)
            | ((indicator[2] as u32) << 7)
            | (indicator[3] as u32);

        assert_eq!(
            decoded, 3_000_000,
            "Syncsafe-decoded data length should match original payload size"
        );
    }

    #[test]
    fn test_small_data_length_indicator_unchanged() {
        let payload = vec![0xCDu8; 100];
        let header = header_with_data_length(0);

        let result = FrameProcessor::process_write(&header, payload).unwrap();

        let indicator = &result[0..4];
        assert_eq!(indicator, &[0x00, 0x00, 0x00, 100]);
    }

    #[test]
    fn test_read_write_roundtrip_with_data_length() {
        let original_payload = b"Hello, this is test data for round-trip verification.".to_vec();
        let write_header = header_with_data_length(0);

        let written =
            FrameProcessor::process_write(&write_header, original_payload.clone()).unwrap();

        let read_header = header_with_data_length(written.len() as u32);
        let read_back = FrameProcessor::process_read(&read_header, written).unwrap();

        assert_eq!(
            read_back, original_payload,
            "Payload should survive write -> read round-trip"
        );
    }

    // --- UTF-16 null terminator tests ---

    fn build_encoded_text(encoding: TextEncoding, text_bytes: &[u8]) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(encoding as u8);
        data.extend_from_slice(text_bytes);
        data
    }

    fn audit_test_header() -> FrameHeader {
        FrameHeader::new("TIT2".to_string(), 100, 0, (2, 4))
    }

    fn audit_test_frame() -> FrameData {
        FrameData::new("TIT2".to_string(), 100, 0, (2, 4))
    }

    #[test]
    fn test_utf16le_false_null_at_odd_offset() {
        let text_bytes: &[u8] = &[0xFF, 0xFE, 0x41, 0x00, 0x00, 0x01];

        let data = build_encoded_text(TextEncoding::Utf16, text_bytes);
        let spec = EncodedTextSpec::new("test");
        let header = audit_test_header();
        let frame = audit_test_frame();

        let (text, _consumed) = spec.read(&header, &frame, &data).unwrap();

        assert_eq!(
            text, "A\u{0100}",
            "UTF-16LE string should not be truncated at misaligned null terminator"
        );
    }

    #[test]
    fn test_utf16le_real_null_terminator_at_even_offset() {
        let text_bytes: &[u8] = &[0xFF, 0xFE, 0x41, 0x00, 0x00, 0x00, 0x42, 0x00];

        let data = build_encoded_text(TextEncoding::Utf16, text_bytes);
        let spec = EncodedTextSpec::new("test");
        let header = audit_test_header();
        let frame = audit_test_frame();

        let (text, _consumed) = spec.read(&header, &frame, &data).unwrap();

        assert_eq!(text, "A", "Should stop at the real aligned null terminator");
    }

    #[test]
    fn test_utf16be_false_null_at_odd_offset() {
        let text_bytes: &[u8] = &[0x01, 0x00, 0x00, 0x41];

        let data = build_encoded_text(TextEncoding::Utf16Be, text_bytes);
        let spec = EncodedTextSpec::new("test");
        let header = audit_test_header();
        let frame = audit_test_frame();

        let (text, _consumed) = spec.read(&header, &frame, &data).unwrap();

        assert_eq!(
            text, "\u{0100}A",
            "UTF-16BE string should not be truncated at misaligned null"
        );
    }

    // --- VolumePeakSpec shift overflow tests ---

    fn peak_test_header() -> FrameHeader {
        FrameHeader::new("RVA2".to_string(), 100, 0, (2, 4))
    }

    fn peak_test_frame() -> FrameData {
        FrameData::new("RVA2".to_string(), 100, 0, (2, 4))
    }

    #[test]
    fn test_bits_zero_does_not_panic() {
        let spec = VolumePeakSpec::new("peak");
        let header = peak_test_header();
        let frame = peak_test_frame();

        let data = [0u8];

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            spec.read(&header, &frame, &data)
        }));

        assert!(
            result.is_ok(),
            "bits=0 should not panic from shift overflow"
        );

        if let Ok(Ok((peak, consumed))) = result {
            assert_eq!(peak, 0.0);
            assert_eq!(consumed, 1);
        }
    }

    #[test]
    fn test_normal_8bit_peak() {
        let spec = VolumePeakSpec::new("peak");
        let header = peak_test_header();
        let frame = peak_test_frame();

        let data = [8, 0xFF];

        let result = spec.read(&header, &frame, &data);
        assert!(result.is_ok());

        let (peak, consumed) = result.unwrap();
        assert!(peak > 0.0, "Peak should be positive");
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_bits_one_does_not_panic() {
        let spec = VolumePeakSpec::new("peak");
        let header = peak_test_header();
        let frame = peak_test_frame();

        let data = [1, 0x80];

        let result = spec.read(&header, &frame, &data);
        assert!(result.is_ok());
    }
}

// ---------------------------------------------------------------------------
// ID3 header version validation tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod id3_header_version_tests {
    use audex::id3::specs::ID3Header;

    /// Build an ID3 header byte array with the given major version.
    fn build_id3_header(major_version: u8) -> Vec<u8> {
        let mut header = Vec::new();
        header.extend_from_slice(b"ID3"); // signature
        header.push(major_version); // major version
        header.push(0); // revision
        header.push(0); // flags
        // Size: 4 synchsafe bytes (all zero = size 0)
        header.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        header
    }

    /// Major version 0 is not defined in any ID3v2 spec.
    #[test]
    fn test_rejects_major_version_0() {
        let header = build_id3_header(0);
        let result = ID3Header::from_bytes(&header);
        assert!(result.is_err(), "Major version 0 should be rejected");
    }

    /// Major version 1 does not exist (ID3v1 has no v2-style header).
    #[test]
    fn test_rejects_major_version_1() {
        let header = build_id3_header(1);
        let result = ID3Header::from_bytes(&header);
        assert!(result.is_err(), "Major version 1 should be rejected");
    }

    /// Major version 5 is not defined.
    #[test]
    fn test_rejects_major_version_5() {
        let header = build_id3_header(5);
        let result = ID3Header::from_bytes(&header);
        assert!(result.is_err(), "Major version 5 should be rejected");
    }

    /// Major version 255 is not defined.
    #[test]
    fn test_rejects_major_version_255() {
        let header = build_id3_header(255);
        let result = ID3Header::from_bytes(&header);
        assert!(result.is_err(), "Major version 255 should be rejected");
    }

    /// Major version 2 is valid (ID3v2.2).
    #[test]
    fn test_accepts_major_version_2() {
        let header = build_id3_header(2);
        let result = ID3Header::from_bytes(&header);
        assert!(
            result.is_ok(),
            "Major version 2 (ID3v2.2) should be accepted"
        );
    }

    /// Major version 3 is valid (ID3v2.3).
    #[test]
    fn test_accepts_major_version_3() {
        let header = build_id3_header(3);
        let result = ID3Header::from_bytes(&header);
        assert!(
            result.is_ok(),
            "Major version 3 (ID3v2.3) should be accepted"
        );
    }

    /// Major version 4 is valid (ID3v2.4).
    #[test]
    fn test_accepts_major_version_4() {
        let header = build_id3_header(4);
        let result = ID3Header::from_bytes(&header);
        assert!(
            result.is_ok(),
            "Major version 4 (ID3v2.4) should be accepted"
        );
    }
}
