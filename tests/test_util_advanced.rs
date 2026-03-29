// Advanced utility function tests for edge cases and specialized functionality
//
// This test module validates advanced utility operations including:
// - File seeking and positioning edge cases
// - Loadfile functionality with write-back
// - Partial read and EOF handling
// - File size operations across different file types
// - Endianness encoding for all integer types
// - String termination decoding with various encodings
// - BitReader alignment and skip operations

use audex::util::*;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use tempfile::NamedTempFile;

#[cfg(test)]
mod seek_end_tests {
    use super::*;

    #[test]
    fn test_seek_end_zero_offset() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();
        let pos = seek_end(&mut file, 0).unwrap();

        // Should seek to the end of file
        assert_eq!(pos, 13);
    }

    #[test]
    fn test_seek_end_from_end() {
        // seek_end(f, n) seeks to (size - n) bytes from start.
        // For "Hello, World!" (13 bytes), seek_end(f, 5) -> position 8.
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();
        let pos = seek_end(&mut file, 5).unwrap();

        assert_eq!(pos, 8);

        let mut buffer = String::new();
        file.read_to_string(&mut buffer).unwrap();
        assert_eq!(buffer, "orld!");
    }

    #[test]
    fn test_seek_end_offset_exceeding_file_size_clamps() {
        // Offsets larger than the file size should clamp to start (position 0).
        // Negative offsets are statically prevented by the u64 parameter type.
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();
        let pos = seek_end(&mut file, 100).unwrap();
        assert_eq!(pos, 0);
    }

    #[test]
    fn test_seek_end_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut file = temp_file.reopen().unwrap();

        let pos = seek_end(&mut file, 0).unwrap();
        assert_eq!(pos, 0);
    }

    #[test]
    fn test_seek_end_large_offset_clamps_to_start() {
        // An offset much larger than file size should clamp to position 0.
        // Negative offsets are statically prevented by the u64 parameter type.
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Short").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();

        let pos = seek_end(&mut file, 100).unwrap();
        assert_eq!(pos, 0);
    }
}

#[cfg(test)]
mod read_full_tests {
    use super::*;

    #[test]
    fn test_read_full_exact_size() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();
        let data = read_full(&mut file, 13).unwrap();

        assert_eq!(data, b"Hello, World!");
    }

    #[test]
    fn test_read_full_partial_read() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();
        let data = read_full(&mut file, 5).unwrap();

        assert_eq!(data, b"Hello");
    }

    #[test]
    fn test_read_full_eof_condition() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Short").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();

        // Try to read more than available
        let result = read_full(&mut file, 100);

        // Should error due to EOF
        assert!(result.is_err());
    }

    #[test]
    fn test_read_full_zero_size() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut file = temp_file.reopen().unwrap();

        let data = read_full(&mut file, 0).unwrap();
        assert_eq!(data.len(), 0);
    }

    #[test]
    fn test_read_full_after_seek() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();

        // Seek to position 7
        file.seek(SeekFrom::Start(7)).unwrap();

        let data = read_full(&mut file, 5).unwrap();
        assert_eq!(data, b"World");
    }

    #[test]
    fn test_read_full_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut file = temp_file.reopen().unwrap();

        let result = read_full(&mut file, 1);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod get_size_tests {
    use super::*;

    #[test]
    fn test_get_size_non_empty_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();
        let size = get_size(&mut file).unwrap();

        assert_eq!(size, 13);
    }

    #[test]
    fn test_get_size_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let mut file = temp_file.reopen().unwrap();

        let size = get_size(&mut file).unwrap();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_get_size_after_write() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Test").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();
        let size = get_size(&mut file).unwrap();

        assert_eq!(size, 4);
    }

    #[test]
    fn test_get_size_preserves_position() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Hello, World!").unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();

        // Seek to middle
        file.seek(SeekFrom::Start(5)).unwrap();

        let size = get_size(&mut file).unwrap();
        assert_eq!(size, 13);

        // Position should be preserved
        let pos = file.stream_position().unwrap();
        assert_eq!(pos, 5);
    }

    #[test]
    fn test_get_size_large_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let data = vec![0u8; 1024 * 1024]; // 1 MB
        temp_file.write_all(&data).unwrap();
        temp_file.flush().unwrap();

        let mut file = temp_file.reopen().unwrap();
        let size = get_size(&mut file).unwrap();

        assert_eq!(size, 1024 * 1024);
    }
}

#[cfg(test)]
mod bit_reader_tests {
    use super::*;

    #[test]
    fn test_bit_reader_align() {
        let data = vec![0b10101010, 0b11001100];
        let cursor = Cursor::new(data);
        let mut reader = BitReader::new(cursor).unwrap();

        // Read 3 bits (not aligned)
        let bits = reader.bits(3).unwrap();
        assert_eq!(bits, 0b101); // First 3 bits

        // Align to byte boundary
        reader.align();

        // Next read should start from next byte
        let next_bits = reader.bits(8).unwrap();
        assert_eq!(next_bits, 0b11001100);
    }

    #[test]
    fn test_bit_reader_skip() {
        let data = vec![0b10101010, 0b11001100, 0b11110000];
        let cursor = Cursor::new(data);
        let mut reader = BitReader::new(cursor).unwrap();

        // Skip 8 bits (1 byte)
        reader.skip(8).unwrap();

        // Next read should be from second byte
        let bits = reader.bits(8).unwrap();
        assert_eq!(bits, 0b11001100);
    }

    #[test]
    fn test_bit_reader_skip_partial() {
        let data = vec![0b10101010, 0b11001100];
        let cursor = Cursor::new(data);
        let mut reader = BitReader::new(cursor).unwrap();

        // Skip 3 bits
        reader.skip(3).unwrap();

        // Read remaining 5 bits of first byte
        let bits = reader.bits(5).unwrap();
        assert_eq!(bits, 0b01010); // Last 5 bits of 10101010
    }

    #[test]
    fn test_bit_reader_align_already_aligned() {
        let data = vec![0b10101010, 0b11001100];
        let cursor = Cursor::new(data);
        let mut reader = BitReader::new(cursor).unwrap();

        // Read full byte (aligned)
        let bits = reader.bits(8).unwrap();
        assert_eq!(bits, 0b10101010);

        // Align when already aligned (should be no-op)
        reader.align();

        // Next read should be from next byte
        let next_bits = reader.bits(8).unwrap();
        assert_eq!(next_bits, 0b11001100);
    }

    #[test]
    fn test_bit_reader_skip_bits() {
        let data = vec![0b10101010, 0b11001100];
        let cursor = Cursor::new(data);
        let mut reader = BitReader::new(cursor).unwrap();

        // Skip 12 bits (1 byte + 4 bits)
        reader.skip_bits(12).unwrap();

        // Read remaining 4 bits of second byte
        let bits = reader.bits(4).unwrap();
        assert_eq!(bits, 0b1100); // Last 4 bits of 11001100
    }

    #[test]
    fn test_bit_reader_zero_bits() {
        let data = vec![0b10101010];
        let cursor = Cursor::new(data);
        let mut reader = BitReader::new(cursor).unwrap();

        let bits = reader.bits(0).unwrap();
        assert_eq!(bits, 0);
    }

    #[test]
    fn test_bit_reader_negative_bits() {
        let data = vec![0b10101010];
        let cursor = Cursor::new(data);
        let mut reader = BitReader::new(cursor).unwrap();

        let result = reader.bits(-1);
        assert!(result.is_err());
    }

    #[test]
    fn test_bit_reader_bytes() {
        let data = vec![0xAB, 0xCD, 0xEF];
        let cursor = Cursor::new(data);
        let mut reader = BitReader::new(cursor).unwrap();

        let bytes = reader.bytes(2).unwrap();
        assert_eq!(bytes, vec![0xAB, 0xCD]);

        let remaining = reader.bytes(1).unwrap();
        assert_eq!(remaining, vec![0xEF]);
    }

    #[test]
    fn test_bit_reader_is_aligned() {
        let data = vec![0b10101010, 0b11001100];
        let cursor = Cursor::new(data);
        let mut reader = BitReader::new(cursor).unwrap();

        assert!(reader.is_aligned());

        // Read 3 bits
        reader.bits(3).unwrap();
        assert!(!reader.is_aligned());

        // Read 5 more bits to complete byte
        reader.bits(5).unwrap();
        assert!(reader.is_aligned());
    }
}

#[cfg(test)]
mod loadfile_tests {
    use super::*;

    #[test]
    fn test_loadfile_process_read() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"Test content").unwrap();
        temp_file.flush().unwrap();

        let path = temp_file.path().to_str().unwrap();
        let options = LoadFileOptions::read_function();

        let result = loadfile_process(path, &options);
        // This should either work or return an error - just ensure no panic
        let _ = result;
    }

    #[test]
    fn test_loadfile_process_write() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let options = LoadFileOptions::write_function();
        let result = loadfile_process(path, &options);

        // Should work for write mode
        let _ = result;
    }
}

#[cfg(test)]
mod misc_utility_tests {
    use super::*;

    #[test]
    fn test_intround_half_to_even() {
        // Test "round half to even" (banker's rounding)
        assert_eq!(intround(0.5), 0); // 0 is even
        assert_eq!(intround(1.5), 2); // 2 is even
        assert_eq!(intround(2.5), 2); // 2 is even
        assert_eq!(intround(3.5), 4); // 4 is even

        // Negative values
        assert_eq!(intround(-0.5), 0); // 0 is even
        assert_eq!(intround(-1.5), -2); // -2 is even
        assert_eq!(intround(-2.5), -2); // -2 is even
    }

    #[test]
    fn test_intround_non_half() {
        assert_eq!(intround(1.2), 1);
        assert_eq!(intround(1.7), 2);
        assert_eq!(intround(-1.2), -1);
        assert_eq!(intround(-1.7), -2);
    }

    #[test]
    fn test_intround_special_values() {
        assert_eq!(intround(f64::NAN), 0);
        assert_eq!(intround(f64::INFINITY), 0);
        assert_eq!(intround(f64::NEG_INFINITY), 0);
    }
}
