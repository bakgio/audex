//! Internal utility functions and data structures for audio file processing.
//!
//! This module provides low-level utilities used throughout the audex library for:
//! - Binary data parsing and manipulation
//! - Text encoding detection and conversion
//! - File I/O operations with memory fallback support
//! - Bit-level data reading
//! - General helper functions
//!
//! ## Stability Notice
//!
//! **Most interfaces in this module are internal implementation details and should not be relied upon.**
//! They may change between minor versions without notice. The primary stable API is exposed through
//! format-specific modules (mp3, flac, mp4, etc.) in the parent crate.
//!
//! ## Public Utilities
//!
//! Some utility functions are exposed for advanced use cases:
//!
//! ### File I/O with Memory Fallback
//!
//! The loadfile system provides automatic memory fallback when filesystem operations fail:
//!
//! ```rust,ignore
//! use audex::util::{loadfile_process, LoadFileOptions, loadfile_guard};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Basic usage for reading
//! let file_thing = loadfile_process("path/to/file.txt", &LoadFileOptions::read_function())?;
//!
//! // Usage with automatic write-back via RAII guard
//! let mut guard = loadfile_guard("path/to/file.txt", &LoadFileOptions::write_function())?;
//! // Modifications to guard will be automatically written back on drop
//!
//! // Manual write-back control
//! guard.write_back()?; // Explicit write-back
//! let file_thing = guard.into_inner(); // Take ownership without write-back
//! # Ok(())
//! # }
//! ```
//!
//! The loadfile system automatically handles:
//! - Memory fallback when filesystem operations fail (EOPNOTSUPP)
//! - Write-back from memory to disk when needed
//! - Proper error conversion from IO errors to AudexError
//! - RAII-based resource management
//!
//! ### Binary Data Processing
//!
//! Utilities for reading and writing binary data in various formats:
//!
//! ```rust,ignore
//! use audex::util::{CData, BinaryCursor};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Reading little-endian integers
//! let data = vec![0x01, 0x02, 0x03, 0x04];
//! let value = CData::uint32_le(&data)?;
//! assert_eq!(value, 0x04030201);
//!
//! // Using BinaryCursor for structured parsing
//! let mut reader = BinaryCursor::new(&data);
//! let byte = reader.read_u8()?;
//! let word = reader.read_u16_le()?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Text Encoding Utilities
//!
//! Functions for handling various text encodings common in audio metadata:
//!
//! ```rust,ignore
//! use audex::util::{decode_text, detect_bom, normalize_encoding};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Decode text with encoding detection
//! // Note: UTF-16 support depends on platform encoding availability
//! let data = b"\xFF\xFEH\x00e\x00l\x00l\x00o\x00"; // UTF-16 LE with BOM
//! let (text, _encoding, _bom) = decode_text(data, Some("utf-16"), "replace", false)?;
//! assert_eq!(text, "Hello");
//!
//! // Detect BOM (Byte Order Mark)
//! if let Some((encoding, bom_size)) = detect_bom(data) {
//!     println!("Detected encoding: {}, BOM size: {}", encoding, bom_size);
//! }
//!
//! // Normalize encoding names (e.g., "UTF8" -> "utf-8")
//! let normalized = normalize_encoding("UTF8");
//! assert_eq!(normalized, "utf-8");
//! # Ok(())
//! # }
//! ```
//!
//! ### Bit-Level Reading
//!
//! For formats requiring bit-level parsing (e.g., MP3 frames):
//!
//! ```rust,no_run
//! use audex::util::BitReader;
//! use std::io::Cursor;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let data = vec![0b10110011, 0b01010101];
//! let mut cursor = Cursor::new(data);
//! let mut reader = BitReader::new(&mut cursor)?;
//!
//! // Read individual bits or bit ranges
//! let bit = reader.read_bits(1)?;        // Read 1 bit
//! let nibble = reader.read_bits(4)?;     // Read 4 bits
//! let byte = reader.read_bits(8)?;       // Read 8 bits
//! # Ok(())
//! # }
//! ```
//!
//! ## Internal Implementation Details
//!
//! The following are internal utilities not intended for direct use:
//! - Dictionary mixins and proxy types
//! - Enum/flags utilities for metadata
//! - Low-level seek/read helpers
//! - Internal error conversion utilities

use crate::{AudexError, Result};
use std::collections::HashMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
const _DEFAULT_BUFFER_SIZE: usize = 2_usize.pow(20);

/// Round a float to the nearest integer using "round half to even" (banker's rounding).
/// NaN and infinity return 0. Values outside the i64 range saturate
/// to i64::MIN or i64::MAX via explicit bounds checks.
pub fn intround(value: f64) -> i64 {
    if value.is_nan() || value.is_infinite() {
        return 0;
    }

    let fract = value.fract();

    // Explicit saturation instead of relying on edition-specific cast behavior.
    // Prior to Rust 2024, `as i64` on out-of-range floats is undefined; from
    // 2024 onward it saturates. Spelling it out keeps the function correct on
    // any edition.
    let truncated = value.trunc();
    let integral = if truncated <= i64::MIN as f64 {
        i64::MIN
    } else if truncated >= i64::MAX as f64 {
        i64::MAX
    } else {
        truncated as i64
    };

    // Use scaled epsilon comparison to catch near-0.5 fractional values
    // that arise from floating-point arithmetic imprecision. The error
    // in fract() grows with the magnitude of the value, so we scale
    // the tolerance accordingly.
    let epsilon = value.abs().max(1.0) * f64::EPSILON * 4.0;
    if (fract.abs() - 0.5).abs() < epsilon {
        // Round half to even
        if integral % 2 == 0 {
            integral
        } else {
            // Use saturating arithmetic to prevent overflow at i64 boundaries
            if value > 0.0 {
                integral.saturating_add(1)
            } else {
                integral.saturating_sub(1)
            }
        }
    } else {
        let rounded = value.round();
        if rounded >= i64::MAX as f64 {
            i64::MAX
        } else if rounded <= i64::MIN as f64 {
            i64::MIN
        } else {
            rounded as i64
        }
    }
}

/// Error returned by [`BitReader`] operations when reading bits or bytes fails.
#[derive(Debug, Clone)]
pub struct BitReaderError {
    pub message: String,
}

impl fmt::Display for BitReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BitReaderError {}

impl From<std::io::Error> for BitReaderError {
    fn from(err: std::io::Error) -> Self {
        BitReaderError {
            message: format!("IO error: {}", err),
        }
    }
}

/// Error indicating an invalid or out-of-range value was encountered during parsing.
#[derive(Debug, Clone)]
pub struct ValueError {
    pub message: String,
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ValueError {}

/// Bit-level reader for parsing binary data with MSB-first bit ordering.
///
/// `BitReader` allows reading arbitrary numbers of bits from a byte stream,
/// which is essential for parsing certain audio formats (e.g., MP3 frame headers,
/// AAC ADTS headers) that pack data at the bit level rather than byte boundaries.
///
/// # Bit Ordering
///
/// This reader uses MSB (Most Significant Bit) first ordering, meaning bits are
/// read from left to right within each byte.
///
/// # Examples
///
/// ```rust,no_run
/// use audex::util::BitReader;
/// use std::io::Cursor;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let data = vec![0b10110011, 0b01010101];
/// let mut cursor = Cursor::new(data);
/// let mut reader = BitReader::new(cursor)?;
///
/// // Read individual bits
/// let bit1 = reader.read_bits(1)?;  // Reads 1
/// let bit2 = reader.read_bits(1)?;  // Reads 0
///
/// // Read multiple bits as an integer
/// let nibble = reader.read_bits(4)?;  // Reads 1100 (binary) = 12
///
/// // Read a full byte
/// let byte = reader.read_bits(8)?;  // Reads remaining bits
///
/// // Align to byte boundary
/// reader.align();
/// # Ok(())
/// # }
/// ```
///
/// # Position Tracking
///
/// The reader tracks both byte position and bit position within the stream,
/// allowing precise control over where data is read from.
pub struct BitReader<R: Read + Seek> {
    /// Underlying byte stream reader
    reader: R,
    /// Internal bit buffer (up to 64 bits)
    buffer: u64,
    /// Number of valid bits currently in buffer
    bits_in_buffer: u8,
    /// Total number of bits read from the stream
    bits_read: u64,
    /// Initial file position in bits
    initial_position: u64,
}

impl<R: Read + Seek> BitReader<R> {
    pub fn new(mut reader: R) -> std::result::Result<Self, BitReaderError> {
        // Record initial file position
        let byte_pos = reader.stream_position().map_err(|e| BitReaderError {
            message: format!("Unable to get initial position: {}", e),
        })?;
        // Convert byte offset to bit offset with overflow protection
        let initial_position = byte_pos.checked_mul(8).ok_or_else(|| BitReaderError {
            message: format!(
                "stream position {} overflows when converted to bits",
                byte_pos
            ),
        })?;

        Ok(Self {
            reader,
            buffer: 0,
            bits_in_buffer: 0,
            bits_read: 0,
            initial_position,
        })
    }

    /// Read `count` bits (MSB first) and return as a signed 32-bit integer.
    ///
    /// **Sign extension on 32-bit reads:** When `count` is 32 and the most
    /// significant bit of the read value is set, the result will be negative
    /// because the raw bits are reinterpreted as two's-complement `i32`.
    /// Callers that need unsigned semantics for 32-bit reads should use
    /// [`read_bits()`](Self::read_bits) instead, which returns `u64`.
    pub fn bits(&mut self, count: i32) -> std::result::Result<i32, BitReaderError> {
        // Handle negative count - return error
        if count < 0 {
            return Err(BitReaderError {
                message: "negative count".to_string(),
            });
        }

        // Handle zero count
        if count == 0 {
            return Ok(0);
        }

        // Reject counts that exceed the i32 return type width.
        // For reads wider than 32 bits, callers should use read_bits()
        // which returns u64 and handles the split correctly.
        if count > 32 {
            return Err(BitReaderError {
                message: format!(
                    "bits() cannot read {} bits into i32 — use read_bits() for counts > 32",
                    count
                ),
            });
        }

        // Check if we have enough bits in buffer, if not read more bytes
        if (count as u8) > self.bits_in_buffer {
            let n_bytes = (count as u8 - self.bits_in_buffer).div_ceil(8) as usize;
            let mut data = vec![0u8; n_bytes];
            match self.reader.read_exact(&mut data) {
                Ok(()) => {
                    for &b in &data {
                        self.buffer = (self.buffer << 8) | (b as u64);
                    }
                    // Detect overflow instead of silently clamping; a buffer
                    // exceeding capacity means the read state is corrupt.
                    self.bits_in_buffer = match self.bits_in_buffer.checked_add((n_bytes * 8) as u8)
                    {
                        Some(v) => v,
                        None => {
                            return Err(BitReaderError {
                                message: format!(
                                    "internal buffer overflow: adding {} bits to {} would exceed u8 range",
                                    n_bytes * 8,
                                    self.bits_in_buffer
                                ),
                            });
                        }
                    };
                    // The buffer is a u64, so it can hold at most 64 valid bits.
                    // If this invariant is violated, the high bits have already been
                    // shifted out of the u64 and any read would return corrupt data.
                    if self.bits_in_buffer > 64 {
                        return Err(BitReaderError {
                            message: format!(
                                "internal buffer overflow: {} bits exceeds 64-bit capacity",
                                self.bits_in_buffer
                            ),
                        });
                    }
                }
                Err(_) => {
                    return Err(BitReaderError {
                        message: "not enough data".to_string(),
                    });
                }
            }
        }

        // Extract the requested bits, masked to exactly `count` bits
        // to prevent stale upper bits from leaking into the result
        self.bits_in_buffer -= count as u8;
        // Use checked shift to handle count=64 without overflow
        let mask = if count >= 64 {
            u64::MAX
        } else {
            (1u64 << count) - 1
        };
        let value = ((self.buffer >> self.bits_in_buffer) & mask) as i32;

        // Clear used bits — use checked shift to avoid panic when
        // bits_in_buffer is 0 (no bits left) or could theoretically reach 64
        if self.bits_in_buffer > 0 && self.bits_in_buffer < 64 {
            self.buffer &= (1u64 << self.bits_in_buffer) - 1;
        } else if self.bits_in_buffer == 0 {
            self.buffer = 0;
        }
        // bits_in_buffer == 64: full buffer, no masking needed

        self.bits_read += count as u64;

        Ok(value)
    }

    /// Read count bytes
    pub fn bytes(&mut self, count: i32) -> std::result::Result<Vec<u8>, BitReaderError> {
        // Handle negative count
        if count < 0 {
            return Err(BitReaderError {
                message: "negative count".to_string(),
            });
        }

        // Handle zero count
        if count == 0 {
            return Ok(Vec::new());
        }

        // Reject counts that exceed the syncsafe limit (268 MB). No legitimate
        // audio metadata frame should be larger than this, and allowing unbounded
        // counts leads to CPU-based denial of service from crafted headers.
        const MAX_BYTE_COUNT: i32 = 0x0FFF_FFFF; // 268 MB, matches syncsafe ceiling
        if count > MAX_BYTE_COUNT {
            return Err(BitReaderError {
                message: format!("byte count {} exceeds maximum of {}", count, MAX_BYTE_COUNT),
            });
        }

        let total = count as usize;

        // If aligned, use bulk read_exact for efficiency instead of
        // reading one byte at a time (which is O(n) individual read calls)
        if self.is_aligned() {
            // Cap the initial allocation to avoid OOM from spoofed headers.
            // The vector will grow in chunks if the stream actually has more data.
            const MAX_INITIAL_CAPACITY: usize = 64 * 1024;

            if total <= MAX_INITIAL_CAPACITY {
                // Small enough to allocate and read in one shot
                let mut result = vec![0u8; total];
                self.reader.read_exact(&mut result).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        BitReaderError {
                            message: "not enough data".to_string(),
                        }
                    } else {
                        BitReaderError::from(e)
                    }
                })?;
                self.bits_read += (total as u64) * 8;
                Ok(result)
            } else {
                // Read in chunks to bound peak memory from spoofed headers.
                // If the stream runs out early, read_exact returns an error.
                let mut result = Vec::with_capacity(MAX_INITIAL_CAPACITY);
                let mut remaining = total;

                while remaining > 0 {
                    let chunk_size = std::cmp::min(remaining, MAX_INITIAL_CAPACITY);
                    let mut chunk = vec![0u8; chunk_size];
                    self.reader.read_exact(&mut chunk).map_err(|e| {
                        if e.kind() == std::io::ErrorKind::UnexpectedEof {
                            BitReaderError {
                                message: "not enough data".to_string(),
                            }
                        } else {
                            BitReaderError::from(e)
                        }
                    })?;
                    result.extend_from_slice(&chunk);
                    self.bits_read += (chunk_size as u64) * 8;
                    remaining -= chunk_size;
                }

                Ok(result)
            }
        } else {
            // Non-aligned: must read bit-by-bit (this path is rare and counts are small)
            let mut result = Vec::with_capacity(std::cmp::min(total, 64 * 1024));
            for _ in 0..count {
                let byte_value = self.bits(8)?;
                result.push(byte_value as u8);
            }
            Ok(result)
        }
    }

    /// Skip count bits
    pub fn skip(&mut self, count: i32) -> std::result::Result<(), BitReaderError> {
        // Handle negative count
        if count < 0 {
            return Err(BitReaderError {
                message: "negative count".to_string(),
            });
        }

        // Handle zero count
        if count == 0 {
            return Ok(());
        }

        let mut remaining = count as u64;

        // Skip bits from buffer first
        if remaining <= self.bits_in_buffer as u64 {
            self.bits_in_buffer -= remaining as u8;
            // Mask to retain only the valid lower bits — use checked shift
            // to avoid panic when bits_in_buffer reaches 0 or 64
            if self.bits_in_buffer > 0 && self.bits_in_buffer < 64 {
                self.buffer &= (1u64 << self.bits_in_buffer) - 1;
            } else if self.bits_in_buffer == 0 {
                self.buffer = 0;
            }
            self.bits_read += remaining;
            return Ok(());
        }

        // Clear buffer and update counters
        remaining -= self.bits_in_buffer as u64;
        self.bits_read += self.bits_in_buffer as u64;
        self.buffer = 0;
        self.bits_in_buffer = 0;

        // Skip complete bytes using seek for O(1) performance instead of
        // reading and discarding one byte at a time
        let bytes_to_skip = remaining / 8;
        if bytes_to_skip > 0 {
            // Guard against u64 values that would wrap to negative when
            // cast to i64, which would corrupt the stream position.
            if bytes_to_skip > i64::MAX as u64 {
                return Err(BitReaderError {
                    message: format!(
                        "skip too large: {} bytes exceeds seekable range",
                        bytes_to_skip
                    ),
                });
            }

            // Record position before seek to verify we didn't overshoot past EOF
            let pos_before = self.reader.stream_position().map_err(|e| BitReaderError {
                message: format!("seek failed: {}", e),
            })?;

            let pos_after = self
                .reader
                .seek(std::io::SeekFrom::Current(bytes_to_skip as i64))
                .map_err(|e| BitReaderError {
                    message: format!("seek failed: {}", e),
                })?;

            let actually_skipped = pos_after - pos_before;

            // Check if we seeked past the end of actual data by attempting a
            // single-byte read to verify the stream has data at this position
            if actually_skipped < bytes_to_skip {
                return Err(BitReaderError {
                    message: "not enough data".to_string(),
                });
            }

            // Verify we haven't seeked past EOF by checking stream length
            let end_pos =
                self.reader
                    .seek(std::io::SeekFrom::End(0))
                    .map_err(|e| BitReaderError {
                        message: format!("seek failed: {}", e),
                    })?;

            if pos_after > end_pos {
                // Seek went past EOF — rewind to where valid data ends
                self.reader.seek(std::io::SeekFrom::Start(end_pos)).ok();
                return Err(BitReaderError {
                    message: "not enough data".to_string(),
                });
            }

            // Restore position to where the skip landed (we moved to end for the check)
            self.reader
                .seek(std::io::SeekFrom::Start(pos_after))
                .map_err(|e| BitReaderError {
                    message: format!("seek failed: {}", e),
                })?;

            self.bits_read += actually_skipped * 8;
            remaining -= actually_skipped * 8;
        }

        // Skip remaining bits by reading them
        let remaining_bits = remaining % 8;
        if remaining_bits > 0 {
            // This may fail with EOF, which is expected behavior
            let _ = self.bits(remaining_bits as i32)?;
        }

        Ok(())
    }

    /// Align to the next byte boundary (clear bit buffer)
    pub fn align(&mut self) {
        // If we have bits in buffer, advance to next byte boundary
        if self.bits_in_buffer > 0 {
            // Calculate how many bits to skip to reach next byte boundary
            let bits_to_skip = self.bits_in_buffer;
            self.bits_read += bits_to_skip as u64;
            self.buffer = 0;
            self.bits_in_buffer = 0;
        }
    }

    /// Get total bits read since construction (including initial stream offset)
    pub fn tell(&self) -> u64 {
        self.initial_position + self.bits_read
    }

    /// Get current bit position relative to the start of this reader.
    /// Returns the number of bits read or skipped so far.
    pub fn get_position(&self) -> u64 {
        self.bits_read
    }

    /// Check if aligned (buffer empty)
    pub fn is_aligned(&self) -> bool {
        self.bits_in_buffer == 0
    }

    /// Read a single bit (0 or 1)
    pub fn read_bit(&mut self) -> std::result::Result<bool, BitReaderError> {
        Ok(self.bits(1)? != 0)
    }

    /// Read count bits as unsigned integer (max 64 bits)
    pub fn read_bits(&mut self, count: u32) -> std::result::Result<u64, BitReaderError> {
        if count > 64 {
            return Err(BitReaderError {
                message: "Cannot read more than 64 bits".to_string(),
            });
        }

        if count <= 32 {
            // Cast through u32 first to zero-extend, preventing sign extension
            // when the highest bit of the i32 result is set
            Ok((self.bits(count as i32)? as u32) as u64)
        } else {
            // Read in two parts for > 32 bits
            let high_bits = count - 32;
            let high = (self.bits(high_bits as i32)? as u32) as u64;
            // Cast through u32 to avoid sign extension when bit 31 is set
            let low = (self.bits(32)? as u32) as u64;
            Ok((high << 32) | low)
        }
    }

    /// Skip count bits (alias for skip for compatibility)
    pub fn skip_bits(&mut self, count: u32) -> std::result::Result<(), BitReaderError> {
        // Validate that count fits in i32 before casting, since skip()
        // rejects negative values and `as i32` wraps values > i32::MAX
        if count > i32::MAX as u32 {
            return Err(BitReaderError {
                message: format!("skip_bits count {} exceeds maximum ({})", count, i32::MAX),
            });
        }
        self.skip(count as i32)
    }

    /// Peek at next count bits without advancing position
    pub fn peek_bits(&mut self, count: u32) -> std::result::Result<u64, BitReaderError> {
        if count > 64 {
            return Err(BitReaderError {
                message: "Cannot peek more than 64 bits".to_string(),
            });
        }

        // Save current state
        let saved_buffer = self.buffer;
        let saved_bits_in_buffer = self.bits_in_buffer;
        let saved_bits_read = self.bits_read;
        let saved_position = self.reader.stream_position().map_err(|e| BitReaderError {
            message: format!("Unable to get position: {}", e),
        })?;

        // Read the bits
        let result = self.read_bits(count);

        // Restore state
        self.buffer = saved_buffer;
        self.bits_in_buffer = saved_bits_in_buffer;
        self.bits_read = saved_bits_read;
        self.reader
            .seek(SeekFrom::Start(saved_position))
            .map_err(|e| BitReaderError {
                message: format!("Unable to restore position: {}", e),
            })?;

        result
    }

    /// Read bytes directly (requires byte alignment)
    pub fn read_bytes_aligned(
        &mut self,
        count: usize,
    ) -> std::result::Result<Vec<u8>, BitReaderError> {
        if !self.is_aligned() {
            return Err(BitReaderError {
                message: "Cannot read bytes from non-byte-aligned position".to_string(),
            });
        }

        // Reject counts that exceed i32::MAX to prevent silent truncation
        // during the usize-to-i32 cast below
        if count > i32::MAX as usize {
            return Err(BitReaderError {
                message: format!("Byte count {} exceeds maximum of {}", count, i32::MAX),
            });
        }

        self.bytes(count as i32)
    }

    /// Get bit position relative to start (not accounting for initial file position)
    pub fn position(&self) -> u64 {
        self.bits_read
    }

    /// Seek to specific bit position (relative to initial position)
    pub fn seek_bits(&mut self, pos: u64) -> std::result::Result<(), BitReaderError> {
        // Calculate byte position and bit offset
        let byte_pos = pos / 8;
        let bit_offset = (pos % 8) as u8;

        // Seek to byte position, guarding against arithmetic overflow
        let absolute_byte = (self.initial_position / 8)
            .checked_add(byte_pos)
            .ok_or_else(|| BitReaderError {
                message: format!(
                    "Seek position overflow: initial_position / 8 ({}) + byte_pos ({}) exceeds u64 range",
                    self.initial_position / 8,
                    byte_pos
                ),
            })?;
        self.reader
            .seek(SeekFrom::Start(absolute_byte))
            .map_err(|e| BitReaderError {
                message: format!("Seek failed: {}", e),
            })?;

        // Reset buffer state
        self.buffer = 0;
        self.bits_in_buffer = 0;
        self.bits_read = pos;

        // Skip to correct bit position within byte if needed
        if bit_offset > 0 {
            self.skip_bits(bit_offset as u32)?;
        }

        Ok(())
    }

    /// Check if we have at least count bits available to read
    pub fn can_read(&mut self, count: u32) -> bool {
        // Try to peek without error handling for simplicity
        self.peek_bits(count).is_ok()
    }

    /// Read remaining bits until byte boundary (returns bits skipped)
    pub fn align_and_count(&mut self) -> u8 {
        let bits_to_skip = if self.bits_in_buffer > 0 {
            self.bits_in_buffer
        } else {
            0
        };
        self.align();
        bits_to_skip
    }

    /// Extract the underlying reader
    pub fn into_inner(self) -> R {
        self.reader
    }
}

/// BitPaddedInt - Syncsafe integer for ID3v2 tags
///
/// ID3v2 uses syncsafe integers where the MSB (bit 7) of each byte is always 0.
/// This means each byte can only store 7 bits of data, preventing false sync patterns.
///
/// For example:
/// - A 4-byte syncsafe integer can store values 0 to 268435455 (0x0FFFFFFF)
/// - The value 129 (0x81) would be stored as [0x00, 0x00, 0x01, 0x01]
#[derive(Debug, Clone, Copy)]
pub struct BitPaddedInt {
    value: u64,
    bits: u8,
    bigendian: bool,
}

impl BitPaddedInt {
    /// Create a new BitPaddedInt from a value
    pub fn new(value: u64, bits: u8, bigendian: bool) -> Result<Self> {
        if bits == 0 {
            return Err(AudexError::InvalidData(
                "BitPaddedInt bits must be >= 1".to_string(),
            ));
        }
        if bits > 8 {
            return Err(AudexError::InvalidData(
                "BitPaddedInt bits must be <= 8".to_string(),
            ));
        }

        Ok(Self {
            value,
            bits,
            bigendian,
        })
    }

    /// Create a new BitPaddedInt with optional parameters (for compatibility)
    pub fn new_optional(
        value: impl Into<u64>,
        bits: Option<u8>,
        bigendian: Option<bool>,
    ) -> Result<Self> {
        Self::new(value.into(), bits.unwrap_or(7), bigendian.unwrap_or(true))
    }

    /// Create from bytes (the most common use case for ID3v2)
    pub fn from_bytes(bytes: &[u8], bits: u8, bigendian: bool) -> Result<Self> {
        if bits == 0 {
            return Err(AudexError::InvalidData(
                "BitPaddedInt bits must be >= 1".to_string(),
            ));
        }
        if bits > 8 {
            return Err(AudexError::InvalidData(
                "BitPaddedInt bits must be <= 8".to_string(),
            ));
        }

        let mask = (1u64 << bits) - 1;
        let mut numeric_value = 0u64;
        let mut shift = 0;

        let byte_iter: Box<dyn Iterator<Item = u8>> = if bigendian {
            Box::new(bytes.iter().rev().copied())
        } else {
            Box::new(bytes.iter().copied())
        };

        for byte in byte_iter {
            // Reject inputs whose accumulated bit width exceeds u64
            // capacity. Without this check, bytes beyond the 64-bit
            // boundary are silently discarded or partially truncated.
            if shift >= 64 {
                return Err(AudexError::InvalidData(
                    "BitPaddedInt: input exceeds 64-bit capacity".to_string(),
                ));
            }

            let masked = byte as u64 & mask;

            // When the current shift straddles the 64-bit boundary,
            // verify that no set bits would be lost to overflow.
            if shift + bits > 64 && (masked >> (64 - shift)) != 0 {
                return Err(AudexError::InvalidData(
                    "BitPaddedInt: input exceeds 64-bit capacity".to_string(),
                ));
            }

            numeric_value |= masked << shift;
            shift += bits;
        }

        Ok(Self {
            value: numeric_value,
            bits,
            bigendian,
        })
    }

    /// Create from integer value (direct value, not encoded)
    pub fn from_int(value: u64, bits: u8, bigendian: bool) -> Result<Self> {
        if bits == 0 {
            return Err(AudexError::InvalidData(
                "BitPaddedInt bits must be >= 1".to_string(),
            ));
        }
        if bits > 8 {
            return Err(AudexError::InvalidData(
                "BitPaddedInt bits must be <= 8".to_string(),
            ));
        }

        // Store the value directly - the syncsafe conversion happens during to_bytes
        Ok(Self {
            value,
            bits,
            bigendian,
        })
    }

    pub fn value(&self) -> u64 {
        self.value
    }

    pub fn bits(&self) -> u8 {
        self.bits
    }

    pub fn is_bigendian(&self) -> bool {
        self.bigendian
    }

    /// Convert to byte representation
    pub fn as_bytes(&self, width: Option<usize>, minwidth: Option<usize>) -> Result<Vec<u8>> {
        Self::to_bytes(
            self.value,
            self.bits,
            self.bigendian,
            width.unwrap_or(4),
            minwidth.unwrap_or(4),
        )
    }

    /// Convert value to syncsafe byte representation
    pub fn to_bytes(
        value: u64,
        bits: u8,
        bigendian: bool,
        width: usize,
        minwidth: usize,
    ) -> Result<Vec<u8>> {
        // Reject bits=0: the mask would be 0, encoding every byte as 0x00
        // regardless of input (silent data loss), and variable-width mode
        // would loop infinitely since `value >>= 0` never reduces to zero
        if bits == 0 {
            return Err(AudexError::InvalidData(
                "BitPaddedInt bits must be >= 1".to_string(),
            ));
        }
        if bits > 8 {
            return Err(AudexError::InvalidData(
                "BitPaddedInt bits must be <= 8".to_string(),
            ));
        }

        // Cap minwidth to prevent unbounded allocation in variable-width mode.
        // No real-world encoding needs more than 1024 bytes of output.
        const MAX_MINWIDTH: usize = 1024;
        if minwidth > MAX_MINWIDTH {
            return Err(AudexError::InvalidData(format!(
                "BitPaddedInt minwidth {} exceeds maximum of {}",
                minwidth, MAX_MINWIDTH
            )));
        }

        let mask = (1u64 << bits) - 1;
        let mut bytes = Vec::new();
        let mut remaining_value = value;

        // Special handling for variable width (use 0 for variable width)
        if width == 0 {
            // Variable width - grow as needed, but respect minwidth
            while remaining_value > 0 || bytes.len() < minwidth {
                bytes.push((remaining_value & mask) as u8);
                if remaining_value > 0 {
                    remaining_value >>= bits;
                }
            }
        } else {
            // Fixed width
            for _ in 0..width {
                bytes.push((remaining_value & mask) as u8);
                remaining_value >>= bits;
            }

            // Check for overflow
            if remaining_value > 0 {
                return Err(AudexError::InvalidData(format!(
                    "Value too wide (>{} bytes)",
                    width
                )));
            }
        }

        if bigendian {
            bytes.reverse();
        }

        Ok(bytes)
    }

    /// Convert to string format (returns bytes as `Vec<u8>`)
    pub fn to_str(
        value: u64,
        bits: Option<u8>,
        bigendian: Option<bool>,
        width: Option<i32>,
        minwidth: Option<usize>,
    ) -> Result<Vec<u8>> {
        let bits = bits.unwrap_or(7);
        let bigendian = bigendian.unwrap_or(true);
        let width = match width {
            Some(-1) | None => 0, // Variable width
            Some(w) if w >= 0 => w as usize,
            Some(w) => return Err(AudexError::InvalidData(format!("Invalid width: {}", w))),
        };
        let minwidth = minwidth.unwrap_or(4);

        Self::to_bytes(value, bits, bigendian, width, minwidth)
    }

    /// Check if the padding bits in bytes/int are valid (all zero in unused bit positions)
    pub fn has_valid_padding(input: &[u8], bits: u8) -> bool {
        // Reject bits=0 — consistent with new(), from_bytes(), from_int(),
        // and to_bytes() which all require bits >= 1
        if bits == 0 || bits > 8 {
            return false;
        }

        let mask = (((1u64 << (8 - bits)) - 1) << bits) as u8;

        for &byte in input {
            if byte & mask != 0 {
                return false;
            }
        }

        true
    }

    /// Check if an integer value has valid padding when converted to bytes
    pub fn has_valid_padding_int(value: u64, bits: u8) -> bool {
        // Reject bits=0 to stay consistent with has_valid_padding (bytes variant),
        // which also returns false for zero-width padding.
        if bits == 0 {
            return false;
        }
        if bits >= 8 {
            return true;
        }

        let mask = ((1u64 << (8 - bits)) - 1) << bits;
        let mut remaining = value;

        while remaining > 0 {
            if (remaining & mask) != 0 {
                return false;
            }
            remaining >>= 8;
        }

        true
    }
}

// Implement standard traits for BitPaddedInt
impl From<BitPaddedInt> for u64 {
    fn from(bpi: BitPaddedInt) -> Self {
        bpi.value
    }
}

impl BitPaddedInt {
    /// Checked conversion from u64 with syncsafe range validation.
    ///
    /// Uses default parameters (bits=7, bigendian=true) and verifies that the
    /// value does not exceed the maximum representable syncsafe integer for a
    /// standard 4-byte encoding: (2^28) - 1 = 268_435_455.
    pub fn checked_from_u64(value: u64) -> std::result::Result<Self, AudexError> {
        // With bits=7 and a standard 4-byte width, only 28 bits of payload
        // are available. Reject values that would overflow on encoding.
        const SYNCSAFE_4BYTE_MAX: u64 = (1 << 28) - 1;
        if value > SYNCSAFE_4BYTE_MAX {
            return Err(AudexError::InvalidData(format!(
                "value {} exceeds maximum syncsafe integer ({}) for 4-byte encoding with bits=7",
                value, SYNCSAFE_4BYTE_MAX
            )));
        }
        Ok(Self {
            value,
            bits: 7,
            bigendian: true,
        })
    }

    /// Checked addition that returns an error when encoding parameters differ.
    ///
    /// Unlike the `+` operator (which uses `debug_assert!` and silently falls
    /// back to the left operand's encoding in release builds), this method
    /// always validates that both operands share the same `bits` and
    /// `bigendian` settings. Use this when processing untrusted input where
    /// a mismatch should be reported rather than silently tolerated.
    pub fn checked_add(&self, rhs: &Self) -> Result<Self> {
        if self.bits != rhs.bits || self.bigendian != rhs.bigendian {
            return Err(AudexError::InvalidData(format!(
                "Cannot add BitPaddedInt values with different encoding parameters \
                 (bits: {} vs {}, bigendian: {} vs {})",
                self.bits, rhs.bits, self.bigendian, rhs.bigendian
            )));
        }
        Ok(Self {
            value: self.value.saturating_add(rhs.value),
            bits: self.bits,
            bigendian: self.bigendian,
        })
    }

    /// Checked subtraction that returns an error when encoding parameters differ.
    ///
    /// Unlike the `-` operator (which uses `debug_assert!` and silently falls
    /// back to the left operand's encoding in release builds), this method
    /// always validates that both operands share the same `bits` and
    /// `bigendian` settings. Use this when processing untrusted input where
    /// a mismatch should be reported rather than silently tolerated.
    pub fn checked_sub(&self, rhs: &Self) -> Result<Self> {
        if self.bits != rhs.bits || self.bigendian != rhs.bigendian {
            return Err(AudexError::InvalidData(format!(
                "Cannot subtract BitPaddedInt values with different encoding parameters \
                 (bits: {} vs {}, bigendian: {} vs {})",
                self.bits, rhs.bits, self.bigendian, rhs.bigendian
            )));
        }
        Ok(Self {
            value: self.value.saturating_sub(rhs.value),
            bits: self.bits,
            bigendian: self.bigendian,
        })
    }
}

impl std::fmt::Display for BitPaddedInt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

// Compare only the decoded value so that Eq is consistent with Ord.
// Two BitPaddedInt instances representing the same integer are equal
// regardless of their encoding parameters (bits, bigendian).
impl PartialEq for BitPaddedInt {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for BitPaddedInt {}

impl PartialOrd for BitPaddedInt {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BitPaddedInt {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value.cmp(&other.value)
    }
}

/// # Panics
///
/// Panics if `self` and `rhs` have different `bits` or `bigendian` parameters,
/// since adding values with mismatched encoding is a logic error that would
/// produce a nonsensical result. For fallible arithmetic that returns a
/// `Result` instead of panicking, use [`BitPaddedInt::checked_add`].
impl std::ops::Add for BitPaddedInt {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        // Safety: all call sites in this codebase construct both operands with
        // identical encoding parameters, so this assert cannot fire in normal
        // use. It guards against future misuse rather than untrusted input.
        assert!(
            self.bits == rhs.bits && self.bigendian == rhs.bigendian,
            "Cannot add BitPaddedInt values with different encoding parameters (bits: {} vs {}, bigendian: {} vs {})",
            self.bits,
            rhs.bits,
            self.bigendian,
            rhs.bigendian
        );
        Self {
            value: self.value.saturating_add(rhs.value),
            bits: self.bits,
            bigendian: self.bigendian,
        }
    }
}

/// # Panics
///
/// Panics if `self` and `rhs` have different `bits` or `bigendian` parameters,
/// since subtracting values with mismatched encoding is a logic error that
/// would produce a nonsensical result. For fallible arithmetic that returns a
/// `Result` instead of panicking, use [`BitPaddedInt::checked_sub`].
impl std::ops::Sub for BitPaddedInt {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        // Safety: all call sites in this codebase construct both operands with
        // identical encoding parameters, so this assert cannot fire in normal
        // use. It guards against future misuse rather than untrusted input.
        assert!(
            self.bits == rhs.bits && self.bigendian == rhs.bigendian,
            "Cannot subtract BitPaddedInt values with different encoding parameters (bits: {} vs {}, bigendian: {} vs {})",
            self.bits,
            rhs.bits,
            self.bigendian,
            rhs.bigendian
        );
        Self {
            value: self.value.saturating_sub(rhs.value),
            bits: self.bits,
            bigendian: self.bigendian,
        }
    }
}

/// Byte Order Mark (BOM) constants for different encodings
pub mod bom {
    /// UTF-8 BOM: EF BB BF
    pub const UTF8: &[u8] = &[0xEF, 0xBB, 0xBF];
    /// UTF-16 Little Endian BOM: FF FE
    pub const UTF16LE: &[u8] = &[0xFF, 0xFE];
    /// UTF-16 Big Endian BOM: FE FF
    pub const UTF16BE: &[u8] = &[0xFE, 0xFF];
    /// UTF-32 Little Endian BOM: FF FE 00 00
    pub const UTF32LE: &[u8] = &[0xFF, 0xFE, 0x00, 0x00];
    /// UTF-32 Big Endian BOM: 00 00 FE FF
    pub const UTF32BE: &[u8] = &[0x00, 0x00, 0xFE, 0xFF];
}

/// Detect BOM at the start of byte data
///
/// Returns the detected encoding and the BOM length, or None if no BOM detected
///
/// # Arguments
/// * `data` - Input byte data to analyze
///
/// # Returns
/// * `Some((encoding_name, bom_length))` if BOM detected
/// * `None` if no BOM found
///
/// # Examples
/// ```
/// use audex::util::detect_bom;
///
/// let utf8_data = b"\xEF\xBB\xBFHello";
/// assert_eq!(detect_bom(utf8_data), Some(("utf-8".to_string(), 3)));
///
/// let utf16le_data = b"\xFF\xFEH\x00e\x00";
/// assert_eq!(detect_bom(utf16le_data), Some(("utf-16le".to_string(), 2)));
/// ```
pub fn detect_bom(data: &[u8]) -> Option<(String, usize)> {
    if data.len() >= 4 && data.starts_with(bom::UTF32LE) {
        Some(("utf-32le".to_string(), 4))
    } else if data.len() >= 4 && data.starts_with(bom::UTF32BE) {
        Some(("utf-32be".to_string(), 4))
    } else if data.len() >= 3 && data.starts_with(bom::UTF8) {
        Some(("utf-8".to_string(), 3))
    } else if data.len() >= 2 && data.starts_with(bom::UTF16LE) {
        Some(("utf-16le".to_string(), 2))
    } else if data.len() >= 2 && data.starts_with(bom::UTF16BE) {
        Some(("utf-16be".to_string(), 2))
    } else {
        None
    }
}

/// Enhanced decode_text function with BOM detection and removal
///
/// Decodes byte data to text with automatic BOM detection and comprehensive
/// error handling. This function provides enhanced functionality over basic
/// string decoding by handling various encodings and BOMs automatically.
///
/// # Arguments
/// * `data` - Input byte data to decode
/// * `encoding` - Optional encoding hint. If None, attempts auto-detection via BOM
/// * `errors` - Error handling mode ("strict", "ignore", "replace")
/// * `remove_bom_flag` - Whether to remove BOM from result (default: true)
///
/// # Returns
/// * `(decoded_string, detected_encoding, bom_removed)` tuple
///
/// # Examples
/// ```
/// use audex::util::decode_text;
///
/// // Auto-detect UTF-8 with BOM
/// let data = b"\xEF\xBB\xBFHello World";
/// let (text, encoding, bom_removed) = decode_text(data, None, "strict", true).unwrap();
/// assert_eq!(text, "Hello World");
/// assert_eq!(encoding, "utf-8");
/// assert_eq!(bom_removed, true);
///
/// // Explicit UTF-16LE decoding
/// let data = b"\xFF\xFEH\x00e\x00l\x00l\x00o\x00";
/// let (text, _, _) = decode_text(data, Some("utf-16le"), "strict", true).unwrap();
/// assert_eq!(text, "Hello");
/// ```
pub fn decode_text(
    data: &[u8],
    encoding: Option<&str>,
    errors: &str,
    remove_bom_flag: bool,
) -> Result<(String, String, bool)> {
    trace_event!(
        data_len = data.len(),
        encoding = ?encoding,
        "decoding text"
    );

    if data.is_empty() {
        return Ok((String::new(), "utf-8".to_string(), false));
    }

    // Detect BOM if no encoding specified or if we want to remove BOM
    let (clean_data, detected_encoding_from_bom): (&[u8], Option<String>) = if remove_bom_flag {
        if let Some((enc, bom_len)) = detect_bom(data) {
            (&data[bom_len..], Some(enc))
        } else {
            (data, None)
        }
    } else {
        (data, None)
    };

    // Determine final encoding to use.
    // When a BOM is present, its detected encoding takes precedence over
    // any explicit encoding hint — the BOM is the ground truth for how
    // the byte stream is actually encoded.
    let final_encoding = match (encoding, &detected_encoding_from_bom) {
        (_, Some(detected)) => detected.clone(), // BOM-detected encoding wins
        (Some(enc), None) => enc.to_string(),    // Fall back to explicit encoding
        (None, None) => "utf-8".to_string(),     // Default to UTF-8
    };

    let bom_was_removed = detected_encoding_from_bom.is_some() && remove_bom_flag;

    // Decode the text based on the determined encoding
    let decoded = decode_bytes_with_encoding(clean_data, &final_encoding, errors)?;

    Ok((decoded, final_encoding, bom_was_removed))
}

/// Decode bytes using specified encoding with error handling
///
/// Internal function used by decode_text for actual decoding logic.
///
/// # Arguments
/// * `data` - Byte data to decode
/// * `encoding` - Encoding to use for decoding
/// * `errors` - Error handling mode
///
/// # Returns
/// * Decoded string
fn decode_bytes_with_encoding(data: &[u8], encoding: &str, errors: &str) -> Result<String> {
    let encoding_lower = encoding.to_lowercase();
    let encoding_str = encoding_lower.as_str();

    match encoding_str {
        "utf-8" => match errors {
            "strict" => match String::from_utf8(data.to_vec()) {
                Ok(s) => Ok(s),
                Err(e) => {
                    warn_event!(encoding = "utf-8", %e, "text decode failed");
                    Err(AudexError::InvalidData(format!(
                        "UTF-8 decode error: {}",
                        e
                    )))
                }
            },
            "ignore" | "replace" => Ok(String::from_utf8_lossy(data).into_owned()),
            _ => Err(AudexError::InvalidData(format!(
                "Unsupported error mode: {}",
                errors
            ))),
        },
        "utf-16le" => decode_utf16_bytes(data, true, errors),
        "utf-16be" => decode_utf16_bytes(data, false, errors),
        "utf-32le" => decode_utf32_bytes(data, true, errors),
        "utf-32be" => decode_utf32_bytes(data, false, errors),
        "ascii" => match errors {
            "strict" => {
                for &byte in data {
                    if byte > 127 {
                        return Err(AudexError::InvalidData(format!(
                            "Non-ASCII byte: 0x{:02x}",
                            byte
                        )));
                    }
                }
                Ok(String::from_utf8(data.to_vec())
                    .expect("validated as ASCII; all bytes are <= 127"))
            }
            "ignore" => {
                let filtered: Vec<u8> = data.iter().filter(|&&b| b <= 127).copied().collect();
                Ok(String::from_utf8(filtered)
                    .expect("validated as ASCII; non-ASCII bytes were filtered out"))
            }
            "replace" => {
                let replaced: Vec<u8> = data
                    .iter()
                    .map(|&b| if b <= 127 { b } else { b'?' })
                    .collect();
                Ok(String::from_utf8(replaced)
                    .expect("validated as ASCII; non-ASCII bytes were replaced with '?'"))
            }
            _ => Err(AudexError::InvalidData(format!(
                "Unsupported error mode: {}",
                errors
            ))),
        },
        "latin-1" | "iso-8859-1" => {
            // Latin-1 is direct byte-to-char mapping for 0x00-0xFF
            let decoded: String = data.iter().map(|&b| b as char).collect();
            Ok(decoded)
        }
        _ => Err(AudexError::InvalidData(format!(
            "Unsupported encoding: {}",
            encoding
        ))),
    }
}

/// Decode UTF-16 byte data with proper endianness handling
fn decode_utf16_bytes(data: &[u8], is_little_endian: bool, errors: &str) -> Result<String> {
    if data.len() % 2 != 0 {
        return match errors {
            "strict" => Err(AudexError::InvalidData(
                "UTF-16 data length not aligned to 2-byte boundary".to_string(),
            )),
            "ignore" => {
                // Ignore the last incomplete byte
                let aligned_data = &data[..data.len() - 1];
                decode_utf16_bytes(aligned_data, is_little_endian, "ignore")
            }
            "replace" => {
                // Process what we can and add replacement character
                let aligned_data = &data[..data.len() - 1];
                let mut result = decode_utf16_bytes(aligned_data, is_little_endian, "replace")?;
                result.push('\u{FFFD}'); // Unicode replacement character
                Ok(result)
            }
            _ => Err(AudexError::InvalidData(format!(
                "Unsupported error mode: {}",
                errors
            ))),
        };
    }

    let mut utf16_values = Vec::new();
    for i in (0..data.len()).step_by(2) {
        let word = if is_little_endian {
            u16::from_le_bytes([data[i], data[i + 1]])
        } else {
            u16::from_be_bytes([data[i], data[i + 1]])
        };
        utf16_values.push(word);
    }

    match errors {
        "strict" => match String::from_utf16(&utf16_values) {
            Ok(s) => Ok(s),
            Err(e) => Err(AudexError::InvalidData(format!(
                "UTF-16 decode error: {}",
                e
            ))),
        },
        "ignore" | "replace" => Ok(String::from_utf16_lossy(&utf16_values)),
        _ => Err(AudexError::InvalidData(format!(
            "Unsupported error mode: {}",
            errors
        ))),
    }
}

/// Decode UTF-32 byte data with proper endianness handling
fn decode_utf32_bytes(data: &[u8], is_little_endian: bool, errors: &str) -> Result<String> {
    if data.len() % 4 != 0 {
        return match errors {
            "strict" => Err(AudexError::InvalidData(
                "UTF-32 data length not aligned to 4-byte boundary".to_string(),
            )),
            "ignore" => {
                // Ignore incomplete bytes at the end
                let aligned_len = (data.len() / 4) * 4;
                let aligned_data = &data[..aligned_len];
                decode_utf32_bytes(aligned_data, is_little_endian, "ignore")
            }
            "replace" => {
                // Process what we can and add replacement character
                let aligned_len = (data.len() / 4) * 4;
                let aligned_data = &data[..aligned_len];
                let mut result = decode_utf32_bytes(aligned_data, is_little_endian, "replace")?;
                result.push('\u{FFFD}'); // Unicode replacement character
                Ok(result)
            }
            _ => Err(AudexError::InvalidData(format!(
                "Unsupported error mode: {}",
                errors
            ))),
        };
    }

    let mut result = String::new();
    for i in (0..data.len()).step_by(4) {
        let code_point = if is_little_endian {
            u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
        } else {
            u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
        };

        match char::from_u32(code_point) {
            Some(ch) => result.push(ch),
            None => {
                match errors {
                    "strict" => {
                        return Err(AudexError::InvalidData(format!(
                            "Invalid UTF-32 code point: 0x{:08X}",
                            code_point
                        )));
                    }
                    "ignore" => {
                        // Skip invalid code point
                    }
                    "replace" => {
                        result.push('\u{FFFD}'); // Unicode replacement character
                    }
                    _ => {
                        return Err(AudexError::InvalidData(format!(
                            "Unsupported error mode: {}",
                            errors
                        )));
                    }
                }
            }
        }
    }

    Ok(result)
}

/// HashMap lookup with filename pattern matching support.
/// Returns the value of any key that matches the passed key.
///
/// Works as if the keys() are all filename patterns.
///
/// # Arguments
/// * `d` - A HashMap with filename patterns as keys
/// * `key` - A key potentially matching any of the keys
/// * `default` - The object to return if no pattern matched the passed in key
///
/// # Returns
/// * The value where a key matched the passed in key, or default if no match.
pub fn dict_match<V: Clone>(d: &HashMap<String, V>, key: &str, default: Option<V>) -> Option<V> {
    // Always try direct lookup first
    if let Some(value) = d.get(key) {
        return Some(value.clone());
    }

    // Pattern matching path: check each pattern in the dictionary
    for (pattern, value) in d.iter() {
        if fnmatch_recursive(pattern, key) {
            return Some(value.clone());
        }
    }

    // No match found, return default
    default
}

/// Iterative glob pattern matcher supporting `*`, `?`, `[...]`, and `\` escapes.
///
/// Uses a single-pass algorithm with O(n*m) worst-case time complexity,
/// avoiding the exponential backtracking of naive recursive approaches.
/// When a `*` is encountered, we record the positions and advance greedily.
/// On mismatch, we backtrack to the last `*` and try consuming one more
/// text character — this guarantees at most one restart per `*`.
fn fnmatch_recursive(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();

    let mut pi = 0; // pattern index
    let mut ti = 0; // text index

    // Saved positions for the most recent `*` wildcard.
    // When we hit a mismatch, we rewind to these and advance the
    // text pointer by one, effectively trying the next split point.
    let mut star_pi: Option<usize> = None;
    let mut star_ti: usize = 0;

    while ti < txt.len() {
        if pi < pat.len() {
            match pat[pi] {
                '*' => {
                    // Collapse consecutive stars
                    while pi < pat.len() && pat[pi] == '*' {
                        pi += 1;
                    }
                    // If `*` is at end of pattern, it matches everything remaining
                    if pi == pat.len() {
                        return true;
                    }
                    // Save this star position for potential backtracking
                    star_pi = Some(pi);
                    star_ti = ti;
                    continue;
                }
                '?' => {
                    // Matches any single character
                    pi += 1;
                    ti += 1;
                    continue;
                }
                '[' => {
                    // Collect bracket expression content up to closing ']'
                    let mut end = pi + 1;
                    let mut bracket_content = String::new();
                    let mut closed = false;
                    while end < pat.len() {
                        if pat[end] == ']' && !bracket_content.is_empty() {
                            closed = true;
                            break;
                        }
                        bracket_content.push(pat[end]);
                        end += 1;
                    }

                    if !closed {
                        // Unclosed bracket — treat '[' as literal
                        if txt[ti] == '[' {
                            pi += 1;
                            ti += 1;
                            continue;
                        }
                        // Literal '[' didn't match, try backtracking
                    } else if match_bracket_expression(&bracket_content, txt[ti]) {
                        pi = end + 1; // skip past ']'
                        ti += 1;
                        continue;
                    }

                    // Bracket didn't match — fall through to backtrack logic
                    if let Some(sp) = star_pi {
                        pi = sp;
                        star_ti += 1;
                        ti = star_ti;
                        continue;
                    }
                    return false;
                }
                '\\' => {
                    // Escaped character — match the next pattern char literally
                    let expected = if pi + 1 < pat.len() {
                        pi += 1;
                        pat[pi]
                    } else {
                        '\\'
                    };

                    if txt[ti] == expected {
                        pi += 1;
                        ti += 1;
                        continue;
                    }
                    // Fall through to backtrack
                }
                c => {
                    // Literal character
                    if txt[ti] == c {
                        pi += 1;
                        ti += 1;
                        continue;
                    }
                    // Fall through to backtrack
                }
            }
        }

        // Current characters don't match — backtrack to the last `*`
        if let Some(sp) = star_pi {
            pi = sp;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    // Text is exhausted — pattern matches only if remaining chars are all `*`
    while pi < pat.len() && pat[pi] == '*' {
        pi += 1;
    }
    pi == pat.len()
}

/// Match bracket expressions like [abc], [!xyz], [a-z]
fn match_bracket_expression(bracket_content: &str, ch: char) -> bool {
    if bracket_content.is_empty() {
        return false;
    }

    let negated = bracket_content.starts_with('!') || bracket_content.starts_with('^');
    let content = if negated {
        &bracket_content[1..]
    } else {
        bracket_content
    };

    let matches = match_bracket_content(content, ch);

    if negated { !matches } else { matches }
}

/// Match the actual content of bracket expression (after negation check)
fn match_bracket_content(content: &str, ch: char) -> bool {
    let mut chars = content.chars().peekable();

    while let Some(c) = chars.next() {
        if chars.peek() == Some(&'-') {
            chars.next(); // consume '-'
            if let Some(end) = chars.next() {
                // Range like a-z
                if ch >= c && ch <= end {
                    return true;
                }
            } else {
                // '-' at end, treat as literal
                if ch == c || ch == '-' {
                    return true;
                }
            }
        } else {
            // Single character
            if ch == c {
                return true;
            }
        }
    }

    false
}

/// Read exactly `size` bytes from file or return an error.
/// Validates that `size` does not exceed the remaining file length
/// before allocating, preventing out-of-memory on malformed headers.
pub fn read_full(fileobj: &mut File, size: usize) -> Result<Vec<u8>> {
    // Check remaining bytes to avoid allocating a huge buffer from
    // an untrusted size field in a malformed file header
    let current_pos = fileobj
        .stream_position()
        .map_err(|e| AudexError::InvalidData(format!("Cannot get file position: {}", e)))?;
    let end_pos = fileobj
        .seek(SeekFrom::End(0))
        .map_err(|e| AudexError::InvalidData(format!("Cannot seek to end: {}", e)))?;
    fileobj
        .seek(SeekFrom::Start(current_pos))
        .map_err(|e| AudexError::InvalidData(format!("Cannot restore file position: {}", e)))?;

    // Guard against cursor positioned past EOF — bare subtraction
    // would underflow and wrap to a huge value, bypassing the check.
    // Keep remaining as u64 so 32-bit platforms don't silently truncate
    // file sizes above 4 GB, which would bypass this validation.
    let remaining = end_pos.saturating_sub(current_pos);
    if (size as u64) > remaining {
        return Err(AudexError::InvalidData(format!(
            "Cannot read {} bytes: only {} bytes remaining in file",
            size, remaining
        )));
    }

    let mut buffer = vec![0u8; size];
    fileobj
        .read_exact(&mut buffer)
        .map_err(|e| AudexError::InvalidData(format!("Cannot read {} bytes: {}", size, e)))?;
    Ok(buffer)
}

/// Seek from end of file with the given offset
/// Bounds checking - don't allow seeking before start of file
/// Returns the new position
pub fn seek_end(fileobj: &mut File, offset: u64) -> Result<u64> {
    let file_size = get_size(fileobj)?;

    // Clamp to file start if offset > file_size
    if offset > file_size {
        let pos = fileobj
            .seek(SeekFrom::Start(0))
            .map_err(|e| AudexError::InvalidData(format!("Cannot seek to start: {}", e)))?;
        return Ok(pos);
    }

    // Convert to the negative i64 that SeekFrom::End expects. Values
    // above i64::MAX are already handled by the clamp above (no file
    // can be larger than i64::MAX on any supported platform).
    let neg_offset = i64::try_from(offset).map(|v| -v).map_err(|_| {
        AudexError::InvalidData(format!("seek_end: offset {} exceeds i64 range", offset))
    })?;

    let pos = fileobj
        .seek(SeekFrom::End(neg_offset))
        .map_err(|e| AudexError::InvalidData(format!("Cannot seek to end-{}: {}", offset, e)))?;
    Ok(pos)
}

/// Input types for the openfile function
#[derive(Debug)]
pub enum FileInput {
    /// A filesystem path to open
    Path(PathBuf),
    /// An already opened file handle
    File(File),
    /// An in-memory buffer
    Memory(Vec<u8>),
}

impl FileInput {
    /// Create from any path-like type
    pub fn from_path<P: AsRef<Path>>(path: P) -> Self {
        FileInput::Path(path.as_ref().to_path_buf())
    }

    /// Check if this is a file path input
    pub fn is_path(&self) -> bool {
        matches!(self, FileInput::Path(_))
    }

    /// Check if this is a file handle input
    pub fn is_file(&self) -> bool {
        matches!(self, FileInput::File(_))
    }

    /// Check if this is memory input
    pub fn is_memory(&self) -> bool {
        matches!(self, FileInput::Memory(_))
    }
}

/// Configuration for in-memory fallback behavior
#[derive(Debug, Clone)]
pub struct FallbackOptions {
    /// Enable in-memory fallback for problematic filesystems
    pub enable_memory_fallback: bool,
    /// Initial buffer size for memory operations
    pub initial_buffer_size: usize,
    /// Maximum file size to keep in memory
    pub max_memory_size: u64,
}

impl Default for FallbackOptions {
    fn default() -> Self {
        Self {
            enable_memory_fallback: true,
            initial_buffer_size: 64 * 1024,     // 64KB
            max_memory_size: 100 * 1024 * 1024, // 100MB
        }
    }
}

/// Comprehensive file opening with RAII context management
///
/// This function provides file opening with proper Rust patterns.
/// It handles different input types, file opening modes, and provides in-memory fallback
/// for problematic filesystems (like FUSE).
///
/// # Arguments
/// * `path_or_file` - Either a filesystem path or an existing file handle
/// * `options` - File opening options (read/write/create permissions)
/// * `fallback_options` - Configuration for memory fallback behavior
///
/// # Returns
/// * `Ok(AnyFileThing)` - A wrapper containing the opened file/memory with metadata
/// * `Err(AudexError)` - If file cannot be opened or options are invalid
///
/// # Examples
/// ```rust,no_run
/// use audex::util::{openfile, FileInput, LoadFileOptions, FallbackOptions};
/// use std::path::PathBuf;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Open a file for reading
/// let options = LoadFileOptions::read_method();
/// let file_thing = openfile(
///     FileInput::from_path("/path/to/file.txt"),
///     &options,
///     &FallbackOptions::default()
/// )?;
///
/// // Work with existing file handle
/// let file = std::fs::File::open("/path/to/file.txt")?;
/// let file_thing = openfile(
///     FileInput::File(file),
///     &options,
///     &FallbackOptions::default()
/// )?;
/// # Ok(())
/// # }
/// ```
pub fn openfile(
    path_or_file: FileInput,
    options: &LoadFileOptions,
    fallback_options: &FallbackOptions,
) -> Result<AnyFileThing> {
    trace_event!(writable = options.writable, "opening file");
    match path_or_file {
        FileInput::Path(path_buf) => openfile_from_path(path_buf, options, fallback_options),
        FileInput::File(file) => openfile_from_handle(file, options, fallback_options),
        FileInput::Memory(data) => openfile_from_memory(data, options),
    }
}

/// Open file from filesystem path with fallback handling
fn openfile_from_path(
    path: PathBuf,
    options: &LoadFileOptions,
    fallback_options: &FallbackOptions,
) -> Result<AnyFileThing> {
    trace_event!(
        path = %path.display(),
        writable = options.writable,
        "opening file from path"
    );
    // Validate the path is not a file object
    verify_filename(&path)?;

    // Determine file opening mode
    let mut open_options = OpenOptions::new();

    if options.writable {
        open_options.read(true).write(true);
        if options.create {
            open_options.create(true);
        }
    } else {
        open_options.read(true);
    }

    // Attempt to open the file
    match open_options.open(&path) {
        Ok(mut file) => {
            // Verify the file meets requirements
            if let Err(e) = verify_fileobj(&mut file, options.writable) {
                // If verification fails and fallback is enabled, try memory fallback
                if fallback_options.enable_memory_fallback && !options.writable {
                    return try_memory_fallback(&mut file, &path, fallback_options);
                } else {
                    return Err(e);
                }
            }

            // Successfully opened file
            let file_thing = FileThing::new(file, Some(path.to_path_buf()), path.to_path_buf());
            Ok(AnyFileThing::File(file_thing))
        }
        Err(e) => {
            // If file opening fails and it's a read operation, try memory fallback
            if fallback_options.enable_memory_fallback && !options.writable && path.exists() {
                let max = fallback_options.max_memory_size;

                // Reject early if file metadata reports a size above the limit
                if let Ok(meta) = std::fs::metadata(&path) {
                    if meta.len() > max {
                        warn_event!(
                            path = %path.display(),
                            size = meta.len(),
                            limit = max,
                            "file exceeds memory fallback limit"
                        );
                        return Err(AudexError::InvalidData(format!(
                            "File '{}' too large for memory fallback: {} bytes > {} byte cap",
                            path.display(),
                            meta.len(),
                            max
                        )));
                    }
                }

                // Use a bounded read so we never allocate more than the cap,
                // even if metadata was unavailable or inaccurate.
                if let Ok(mut fh) = std::fs::File::open(&path) {
                    let mut buffer = Vec::new();
                    let bound = max.saturating_add(1);
                    if let Ok(n) = Read::by_ref(&mut fh).take(bound).read_to_end(&mut buffer) {
                        if (n as u64) <= max {
                            let cursor = Cursor::new(buffer);
                            let file_thing = FileThing::new(
                                cursor,
                                Some(path.to_path_buf()),
                                path.to_path_buf(),
                            );
                            return Ok(AnyFileThing::Memory(file_thing));
                        }
                    }
                }
            }

            warn_event!(
                path = %path.display(),
                error = %e,
                "failed to open file"
            );
            Err(AudexError::InvalidData(format!(
                "Cannot open file '{}': {}",
                path.display(),
                e
            )))
        }
    }
}

/// Handle existing file handle
fn openfile_from_handle(
    mut file: File,
    options: &LoadFileOptions,
    fallback_options: &FallbackOptions,
) -> Result<AnyFileThing> {
    // Verify the file handle meets requirements
    if let Err(e) = verify_fileobj(&mut file, options.writable) {
        // If verification fails and fallback is enabled, try memory fallback
        if fallback_options.enable_memory_fallback && !options.writable {
            let name = fileobj_name(&file);
            let path = if name.is_empty() {
                PathBuf::new()
            } else {
                PathBuf::from(name)
            };
            return try_memory_fallback(&mut file, &path, fallback_options);
        } else {
            return Err(e);
        }
    }

    // Get the file name if possible
    let filename_str = fileobj_name(&file);
    let filename = if filename_str.is_empty() {
        None
    } else {
        Some(PathBuf::from(&filename_str))
    };
    let display_name = if filename_str.is_empty() {
        PathBuf::from("<file object>")
    } else {
        PathBuf::from(filename_str)
    };

    let file_thing = FileThing::new(file, filename, display_name);
    Ok(AnyFileThing::File(file_thing))
}

/// Create file thing from in-memory data
fn openfile_from_memory(data: Vec<u8>, options: &LoadFileOptions) -> Result<AnyFileThing> {
    if options.writable {
        // For writable access, use the data as-is
        let cursor = Cursor::new(data);
        let file_thing = FileThing::new(cursor, None, PathBuf::from("<memory>"));
        Ok(AnyFileThing::Memory(file_thing))
    } else {
        // For read-only access, also use cursor
        let cursor = Cursor::new(data);
        let file_thing = FileThing::new(cursor, None, PathBuf::from("<memory>"));
        Ok(AnyFileThing::Memory(file_thing))
    }
}

/// Attempt to create in-memory fallback when file operations fail
fn try_memory_fallback(
    file: &mut File,
    path: &Path,
    fallback_options: &FallbackOptions,
) -> Result<AnyFileThing> {
    let max = fallback_options.max_memory_size;

    // Early rejection when the file size is known upfront
    if let Ok(size) = get_size(file) {
        if size > max {
            return Err(AudexError::InvalidData(format!(
                "File '{}' too large for memory fallback: {} bytes > {} bytes",
                path.display(),
                size,
                max
            )));
        }
    }

    // Use a bounded read so we never allocate more than the configured limit,
    // even when the size probe above failed (non-seekable or special handles).
    file.seek(SeekFrom::Start(0))
        .map_err(|e| AudexError::InvalidData(format!("Cannot seek to file start: {}", e)))?;

    let mut buffer = Vec::new();
    let bound = max.saturating_add(1); // read one extra byte to detect overflow
    let bytes_read = Read::by_ref(file)
        .take(bound)
        .read_to_end(&mut buffer)
        .map_err(|e| AudexError::InvalidData(format!("Cannot read file into memory: {}", e)))?;

    if bytes_read as u64 > max {
        return Err(AudexError::InvalidData(format!(
            "File '{}' exceeds memory fallback limit: read {} bytes > {} byte cap",
            path.display(),
            bytes_read,
            max
        )));
    }

    let cursor = Cursor::new(buffer);
    let file_thing = FileThing::new(cursor, Some(path.to_path_buf()), path.to_path_buf());

    Ok(AnyFileThing::Memory(file_thing))
}

/// Convenience function for opening files with default fallback options
pub fn openfile_simple(path_or_file: FileInput, options: &LoadFileOptions) -> Result<AnyFileThing> {
    openfile(path_or_file, options, &FallbackOptions::default())
}

/// Check if a filename matches a Windows reserved device name.
///
/// Windows reserves: CON, PRN, AUX, NUL, COM1-COM9, LPT1-LPT9.
/// The comparison is case-insensitive to match Windows filesystem behavior.
pub fn is_windows_reserved_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Check if an argument should be treated as a file object
/// Returns true if an argument should be treated as a file object
///
/// Path types are always treated as paths, not file handles.
pub fn is_fileobj<T: AsRef<Path> + ?Sized>(_obj: &T) -> bool {
    // Path types are always treated as paths, not file handles
    false
}

/// For actual file objects (File type), this returns true
pub fn is_fileobj_file(_fileobj: &std::fs::File) -> bool {
    true
}

/// Returns a potential filename for a file object.
/// Always a valid path type, but might be empty or non-existent.
pub fn fileobj_name(_fileobj: &File) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = _fileobj.as_raw_fd();

        // Try to read the symlink /proc/self/fd/{fd}
        if let Ok(path) = std::fs::read_link(format!("/proc/self/fd/{}", fd)) {
            // Filter out special cases that aren't real files
            let path_str = path.to_string_lossy();
            if path_str.starts_with("pipe:")
                || path_str.starts_with("socket:")
                || path_str == "/dev/null"
                || path_str == "/dev/stdin"
                || path_str == "/dev/stdout"
                || path_str == "/dev/stderr"
            {
                return String::new(); // These are file objects, return empty string
            }
            return path.to_string_lossy().to_string();
        }

        // Fallback: return empty string if cannot determine path
        String::new()
    }

    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{HANDLE, MAX_PATH};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS,
        };

        let handle = _fileobj.as_raw_handle() as HANDLE;

        unsafe {
            // First, try with a reasonable buffer size
            let mut buffer: Vec<u16> = vec![0; MAX_PATH as usize + 1];
            let mut required_size = GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            );

            // If buffer was too small, allocate the required size.
            // Cap at 32768 wide characters (the Windows extended-length
            // path limit) to prevent OOM from a corrupted return value.
            const MAX_PATH_BUFFER: u32 = 32768;
            if required_size > buffer.len() as u32 {
                if required_size > MAX_PATH_BUFFER {
                    return String::new();
                }
                buffer.resize(required_size as usize + 1, 0);
                // Use the return value from this second call for truncation
                // rather than the first call's required_size, because the
                // actual written length may differ from the initially
                // reported buffer requirement.
                let actual_len = GetFinalPathNameByHandleW(
                    handle,
                    buffer.as_mut_ptr(),
                    buffer.len() as u32,
                    FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
                );
                if actual_len == 0 || actual_len > buffer.len() as u32 {
                    return String::new();
                }
                required_size = actual_len;
            }

            // Check if the call succeeded (covers the first-call-succeeded path)
            if required_size == 0 || required_size > buffer.len() as u32 {
                return String::new();
            }

            // Convert the result to a PathBuf.
            // The Windows API returns the length without the null terminator.
            buffer.truncate(required_size as usize);
            let os_string = OsString::from_wide(&buffer);
            let path_str = os_string.to_string_lossy();

            // Remove the \\?\ prefix if present (extended-length path prefix)
            let cleaned_path = path_str.strip_prefix("\\\\?\\").unwrap_or(&path_str);

            // Filter out special cases that aren't real files
            if cleaned_path.starts_with("pipe:")
                || cleaned_path.starts_with("socket:")
                || is_windows_reserved_name(cleaned_path)
            {
                return String::new();
            }

            cleaned_path.to_string()
        }
    }

    #[cfg(not(any(unix, windows)))]
    {
        // Other platforms don't have a standard way to get file paths from handles
        String::new()
    }
}

/// Macro to create an integer-backed enum with `From`, `Debug`, and `Display` support.
///
/// Converts between the enum type and its underlying integer representation.
/// Unknown values are preserved and displayed numerically.
///
/// # Example
/// ```
/// audex::int_enum! {
///     enum Status: u8 {
///         OK = 0,
///         ERROR = 1,
///     }
/// }
///
/// let s = Status::OK;
/// assert_eq!(u8::from(s), 0);
/// let unknown = Status::from(42u8);
/// assert_eq!(format!("{}", unknown), "42");
/// ```
#[macro_export]
macro_rules! int_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident: $int_type:ty {
            $(
                $(#[$field_meta:meta])*
                $variant:ident = $value:expr
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(transparent)]
        $vis struct $name($int_type);

        impl $name {
            $(
                $(#[$field_meta])*
                pub const $variant: Self = Self($value);
            )*
        }

        impl From<$int_type> for $name {
            fn from(value: $int_type) -> Self {
                Self(value)
            }
        }

        impl From<$name> for $int_type {
            fn from(value: $name) -> $int_type {
                value.0
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.0 {
                    $($value => f.write_str(concat!(stringify!($name), "::", stringify!($variant))),)*
                    other => write!(f, "{}({})", stringify!($name), other),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.0 {
                    $($value => f.write_str(stringify!($variant)),)*
                    other => write!(f, "{}", other),
                }
            }
        }

        // Allow casting to the integer type with `as`
        // Since we use #[repr(transparent)], the struct has the same layout as the inner type
    };
}

/// Macro to create a flags enum that supports bitwise operations.
/// This provides flag enum functionality.
///
/// # Example
/// ```
/// audex::flags! {
///     enum Permission: u32 {
///         READ = 1,
///         WRITE = 2,
///         EXECUTE = 4,
///         ALL = 7,
///     }
/// }
///
/// let perm = Permission::READ | Permission::WRITE;
/// assert!(perm.contains(Permission::READ));
/// assert!(perm.contains(Permission::WRITE));
/// assert!(!perm.contains(Permission::EXECUTE));
/// ```
#[macro_export]
macro_rules! flags {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident: $int_type:ty {
            $(
                $(#[$field_meta:meta])*
                $variant:ident = $value:expr
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        $vis struct $name {
            bits: $int_type,
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut first = true;
                let mut has_any = false;

                f.write_str(stringify!($name))?;
                f.write_str("(")?;

                $(
                    if self.bits & $value == $value && $value != 0 {
                        if !first {
                            f.write_str(" | ")?;
                        }
                        f.write_str(stringify!($variant))?;
                        first = false;
                        has_any = true;
                    }
                )*

                if !has_any {
                    f.write_str("EMPTY")?;
                }

                f.write_str(")")
            }
        }

        impl $name {
            $(
                $(#[$field_meta])*
                pub const $variant: Self = Self { bits: $value };
            )*

            /// Create a new flags value from raw bits
            pub const fn from_bits(bits: $int_type) -> Option<Self> {
                // Check if all bits are valid
                const ALL_BITS: $int_type = $($value)|*;
                if (bits & !ALL_BITS) == 0 {
                    Some(Self { bits })
                } else {
                    None
                }
            }

            /// Create a new flags value from raw bits, truncating invalid bits
            pub const fn from_bits_truncate(bits: $int_type) -> Self {
                const ALL_BITS: $int_type = $($value)|*;
                Self { bits: bits & ALL_BITS }
            }

            /// Get the raw bits value
            pub const fn bits(&self) -> $int_type {
                self.bits
            }

            /// Check if this flags value contains all bits of another flags value
            pub const fn contains(&self, other: Self) -> bool {
                (self.bits & other.bits) == other.bits
            }

            /// Check if this flags value is empty (no bits set)
            pub const fn is_empty(&self) -> bool {
                self.bits == 0
            }

            /// Check if this flags value contains all possible bits
            pub const fn is_all(&self) -> bool {
                const ALL_BITS: $int_type = $($value)|*;
                self.bits == ALL_BITS
            }

            /// Create an empty flags value
            pub const fn empty() -> Self {
                Self { bits: 0 }
            }

            /// Create a flags value with all bits set
            pub const fn all() -> Self {
                const ALL_BITS: $int_type = $($value)|*;
                Self { bits: ALL_BITS }
            }
        }

        impl std::ops::BitOr for $name {
            type Output = Self;

            fn bitor(self, other: Self) -> Self {
                Self { bits: self.bits | other.bits }
            }
        }

        impl std::ops::BitOrAssign for $name {
            fn bitor_assign(&mut self, other: Self) {
                self.bits |= other.bits;
            }
        }

        impl std::ops::BitAnd for $name {
            type Output = Self;

            fn bitand(self, other: Self) -> Self {
                Self { bits: self.bits & other.bits }
            }
        }

        impl std::ops::BitAndAssign for $name {
            fn bitand_assign(&mut self, other: Self) {
                self.bits &= other.bits;
            }
        }

        impl std::ops::BitXor for $name {
            type Output = Self;

            fn bitxor(self, other: Self) -> Self {
                Self { bits: self.bits ^ other.bits }
            }
        }

        impl std::ops::BitXorAssign for $name {
            fn bitxor_assign(&mut self, other: Self) {
                self.bits ^= other.bits;
            }
        }

        impl std::ops::Not for $name {
            type Output = Self;

            fn not(self) -> Self {
                const ALL_BITS: $int_type = $($value)|*;
                Self { bits: (!self.bits) & ALL_BITS }
            }
        }

        impl std::ops::Sub for $name {
            type Output = Self;

            fn sub(self, other: Self) -> Self {
                Self { bits: self.bits & !other.bits }
            }
        }

        impl std::ops::SubAssign for $name {
            fn sub_assign(&mut self, other: Self) {
                self.bits &= !other.bits;
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "0x{:x}", self.bits)
            }
        }

        impl From<$name> for $int_type {
            fn from(flags: $name) -> $int_type {
                flags.bits
            }
        }
    };
}

/// Default buffer size for file operations (1MB)
pub const DEFAULT_BUFFER_SIZE: usize = _DEFAULT_BUFFER_SIZE;

/// Resize a file by adding or removing bytes at the end
///
/// This function changes the size of a file by `diff` bytes. If `diff` is positive,
/// the file is extended with null bytes. If `diff` is negative, the file is truncated.
///
/// # Arguments
/// * `file` - A mutable reference to the file to resize
/// * `diff` - Number of bytes to add (positive) or remove (negative)
/// * `buffer_size` - Size of buffer to use for operations (defaults to 1MB)
///
/// # Returns
/// * `Ok(())` if the operation succeeds
/// * `Err(AudexError)` if an I/O error occurs or invalid parameters are provided
///
/// # Example
/// ```rust,no_run
/// use std::fs::OpenOptions;
/// use audex::util::{resize_file, DEFAULT_BUFFER_SIZE};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut file = OpenOptions::new().read(true).write(true).open("test.dat")?;
///
/// // Extend file by 1024 bytes
/// resize_file(&mut file, 1024, Some(DEFAULT_BUFFER_SIZE))?;
///
/// // Shrink file by 512 bytes
/// resize_file(&mut file, -512, Some(DEFAULT_BUFFER_SIZE))?;
/// # Ok(())
/// # }
/// ```
pub fn resize_file<F>(file: &mut F, diff: i64, buffer_size: Option<usize>) -> Result<()>
where
    F: Read + Write + Seek + 'static,
{
    if diff == 0 {
        return Ok(());
    }

    let buffer_size = buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    if buffer_size == 0 {
        return Err(AudexError::InvalidData(
            "Buffer size cannot be zero".to_string(),
        ));
    }

    let original_pos = file.stream_position()?;
    let file_size = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(original_pos))?;

    if diff > 0 {
        // Extend file - seek to end and write null bytes
        let original_size = file.seek(SeekFrom::End(0))?;
        // Use u64 for the remaining byte counter to avoid truncation
        // on 32-bit platforms where usize is only 32 bits
        let mut remaining = diff as u64;
        let buf_size_u64 = buffer_size as u64;
        let initial_chunk = std::cmp::min(buf_size_u64, remaining) as usize;
        let mut buffer = vec![0u8; initial_chunk];

        let write_result: Result<()> = (|| {
            while remaining > 0 {
                // Only cast the per-chunk size to usize for the buffer
                let chunk_size = std::cmp::min(buf_size_u64, remaining);
                buffer.resize(chunk_size as usize, 0);
                file.write_all(&buffer)?;
                remaining -= chunk_size;
            }
            Ok(())
        })();

        if let Err(e) = write_result {
            // Attempt to rollback to original size on failure (e.g. ENOSPC)
            let mut rollback_attempted = false;
            if let Some(file_ref) = (file as &mut dyn std::any::Any).downcast_mut::<File>() {
                rollback_attempted = true;
                let _ = file_ref.set_len(original_size);
            } else if let Some(cursor_ref) =
                (file as &mut dyn std::any::Any).downcast_mut::<std::io::Cursor<Vec<u8>>>()
            {
                rollback_attempted = true;
                // Use try_from to avoid silent truncation on 32-bit platforms
                // where usize is 32 bits and original_size could exceed 4 GB
                if let Ok(size) = usize::try_from(original_size) {
                    cursor_ref.get_mut().truncate(size);
                }
            }
            if !rollback_attempted {
                return Err(AudexError::InvalidOperation(format!(
                    "failed to extend writer and could not roll back partial changes: {}",
                    e
                )));
            }
            return Err(e);
        }
    } else {
        // Truncate file - validate and set new size
        // Use unsigned_abs() to safely convert negative diff to u64.
        // Direct negation (-diff) overflows when diff == i64::MIN.
        let shrink = diff.unsigned_abs();
        if shrink > file_size {
            return Err(AudexError::InvalidData(
                "Invalid size: shrink amount exceeds file size".to_string(),
            ));
        }

        let new_size = file_size - shrink;

        // Truncate the file (note: this requires the file to support truncation)
        if let Some(file_ref) = (file as &mut dyn std::any::Any).downcast_mut::<File>() {
            file_ref.set_len(new_size)?;
        } else if let Some(cursor_ref) =
            (file as &mut dyn std::any::Any).downcast_mut::<std::io::Cursor<Vec<u8>>>()
        {
            // Use try_from to avoid silent truncation on 32-bit platforms
            let safe_size = usize::try_from(new_size).map_err(|_| {
                AudexError::InvalidData(format!(
                    "Truncation target size ({} bytes) exceeds addressable range",
                    new_size
                ))
            })?;
            cursor_ref.get_mut().truncate(safe_size);
        } else {
            return Err(AudexError::Unsupported(
                "File truncation not supported for this file type".to_string(),
            ));
        }

        // Restore original position or clamp to new size
        let restore_pos = std::cmp::min(original_pos, new_size);
        file.seek(SeekFrom::Start(restore_pos))?;
    }

    Ok(())
}

/// Attempt to truncate a writer to the given size.
///
/// For recognized concrete types (`std::fs::File` and `Cursor<Vec<u8>>`),
/// performs real truncation via `set_len()` or `Vec::truncate()` so the
/// output has the correct size. For trait objects and unrecognized types,
/// zeroes the stale trailing region as a fallback to prevent old data
/// from remaining readable.
///
/// When called with a concrete type (not `dyn`), real truncation is
/// attempted first. Use this version whenever the concrete type is known
/// at the call site.
pub fn truncate_writer<W>(writer: &mut W, new_size: u64) -> Result<()>
where
    W: Read + Write + Seek + 'static,
{
    let current_end = writer.seek(SeekFrom::End(0))?;
    if new_size >= current_end {
        return Ok(()); // Nothing to truncate
    }

    // Try real truncation for known concrete types. This ensures the
    // output has the correct size rather than keeping the original size
    // with zero-padded trailing bytes.
    let writer_any = writer as &mut dyn std::any::Any;

    if let Some(file_ref) = writer_any.downcast_mut::<File>() {
        file_ref.set_len(new_size)?;
        file_ref.seek(SeekFrom::Start(new_size))?;
        return Ok(());
    }

    if let Some(cursor_ref) = writer_any.downcast_mut::<std::io::Cursor<Vec<u8>>>() {
        let safe_size = usize::try_from(new_size).map_err(|_| {
            crate::AudexError::InvalidData(format!(
                "truncation size {} exceeds platform addressable range",
                new_size
            ))
        })?;
        cursor_ref.get_mut().truncate(safe_size);
        std::io::Seek::seek(cursor_ref, SeekFrom::Start(new_size))?;
        return Ok(());
    }

    // Fallback: zero stale trailing bytes in chunks
    zero_trailing_bytes(writer, new_size, current_end)
}

/// Fallback truncation for trait objects (`dyn ReadWriteSeek`).
///
/// Because trait objects cannot be downcast to a concrete type that
/// supports OS-level truncation (e.g. `File::set_len`), this function
/// **zeroes all bytes** from `new_size` to the current end of the writer
/// instead of physically shrinking the underlying storage.
///
/// After this call the output file retains its original size on disk, but
/// the region beyond `new_size` contains only zero bytes, preventing old
/// data from leaking. Callers that require an exact file size should
/// verify and, if necessary, re-truncate through a concrete `File` handle
/// after the write-back is complete.
pub fn truncate_writer_dyn(writer: &mut dyn crate::ReadWriteSeek, new_size: u64) -> Result<()> {
    let current_end = writer.seek(SeekFrom::End(0))?;
    if new_size >= current_end {
        return Ok(()); // Nothing to truncate
    }

    // Zero the stale trailing region for trait-object writers that
    // do not support physical truncation.
    zero_trailing_bytes(writer, new_size, current_end)
}

/// Read an entire seekable writer into memory, rejecting oversized inputs first.
///
/// Writer-based save helpers occasionally need a `Cursor<Vec<u8>>` so they can
/// perform in-memory rewrites. This helper applies the same pre-read guard used
/// by other in-memory save paths to avoid buffering arbitrarily large files.
pub fn read_all_from_writer_limited(
    writer: &mut dyn crate::ReadWriteSeek,
    context: &str,
) -> Result<Vec<u8>> {
    let file_size = writer.seek(SeekFrom::End(0))?;
    // This helper is used for whole-file in-memory rewrite paths, so the limit
    // must allow typical audio payloads rather than only tag-sized buffers.
    const MAX_FULL_WRITER_READ: u64 = 512 * 1024 * 1024;
    let max_read_size = MAX_FULL_WRITER_READ;
    if file_size > max_read_size {
        return Err(crate::AudexError::InvalidData(format!(
            "File size ({} bytes) exceeds maximum for {} ({} bytes)",
            file_size, context, max_read_size
        )));
    }

    writer.seek(SeekFrom::Start(0))?;
    let mut data = Vec::new();
    writer.read_to_end(&mut data)?;
    Ok(data)
}

/// Zero out trailing bytes from `new_size` to `current_end` in chunks.
fn zero_trailing_bytes<W: Write + Seek + ?Sized>(
    writer: &mut W,
    new_size: u64,
    current_end: u64,
) -> Result<()> {
    writer.seek(SeekFrom::Start(new_size))?;
    let stale_len = current_end - new_size;
    const CHUNK_SIZE: usize = 64 * 1024; // 64 KB
    let zeros = [0u8; CHUNK_SIZE];
    let mut remaining = stale_len;
    while remaining > 0 {
        let to_write = (remaining as usize).min(CHUNK_SIZE);
        writer.write_all(&zeros[..to_write])?;
        remaining -= to_write as u64;
    }

    // Reposition to the logical end of content
    writer.seek(SeekFrom::Start(new_size))?;
    Ok(())
}

/// Move bytes within a file from one location to another
///
/// This function efficiently moves `count` bytes from `src` offset to `dest` offset
/// within the same file. The operation handles overlapping regions correctly and
/// uses buffered I/O for performance.
///
/// # Arguments
/// * `file` - A mutable reference to the file
/// * `dest` - Destination offset where bytes will be moved to
/// * `src` - Source offset where bytes will be moved from
/// * `count` - Number of bytes to move
/// * `buffer_size` - Size of buffer to use for the operation
///
/// # Returns
/// * `Ok(())` if the operation succeeds
/// * `Err(AudexError)` if an I/O error occurs or invalid parameters are provided
///
/// # Example
/// ```rust,no_run
/// use std::fs::OpenOptions;
/// use audex::util::{move_bytes, DEFAULT_BUFFER_SIZE};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut file = OpenOptions::new().read(true).write(true).open("test.dat")?;
///
/// // Move 100 bytes from offset 200 to offset 50
/// move_bytes(&mut file, 50, 200, 100, Some(DEFAULT_BUFFER_SIZE))?;
/// # Ok(())
/// # }
/// ```
pub fn move_bytes<F>(
    file: &mut F,
    dest: u64,
    src: u64,
    count: u64,
    buffer_size: Option<usize>,
) -> Result<()>
where
    F: Read + Write + Seek,
{
    trace_event!(from = src, to = dest, size = count, "moving bytes");

    if count == 0 {
        return Ok(());
    }

    let buffer_size = buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    if buffer_size == 0 {
        return Err(AudexError::InvalidData(
            "Buffer size cannot be zero".to_string(),
        ));
    }

    // Get file size to validate bounds
    let current_pos = file.stream_position()?;
    let file_size = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(current_pos))?;

    // Validate source bounds
    if src.saturating_add(count) > file_size {
        return Err(AudexError::InvalidData(
            "Area outside of file: source range exceeds file size".to_string(),
        ));
    }

    if dest > file_size || dest.saturating_add(count) > file_size {
        return Err(AudexError::InvalidData(
            "Area outside of file: destination range exceeds file size".to_string(),
        ));
    }

    // Handle the case where source and destination are the same
    if src == dest {
        return Ok(());
    }

    let chunk_size = std::cmp::min(buffer_size as u64, count) as usize;
    let mut buffer = vec![0u8; chunk_size];

    if src < dest && dest < src + count {
        // Overlapping regions - copy backwards to avoid overwriting data
        let mut remaining = count;
        while remaining > 0 {
            let current_chunk = std::cmp::min(chunk_size as u64, remaining) as usize;
            let src_offset = src + remaining - current_chunk as u64;
            let dest_offset = dest + remaining - current_chunk as u64;

            // Read from source
            file.seek(SeekFrom::Start(src_offset))?;
            buffer.resize(current_chunk, 0);
            file.read_exact(&mut buffer[..current_chunk])?;

            // Write to destination
            file.seek(SeekFrom::Start(dest_offset))?;
            file.write_all(&buffer[..current_chunk])?;

            remaining -= current_chunk as u64;
        }
    } else {
        // Non-overlapping or safe overlapping - copy forwards
        let mut remaining = count;
        let mut offset = 0u64;

        while remaining > 0 {
            let current_chunk = std::cmp::min(chunk_size as u64, remaining) as usize;

            // Read from source
            file.seek(SeekFrom::Start(src + offset))?;
            buffer.resize(current_chunk, 0);
            file.read_exact(&mut buffer[..current_chunk])?;

            // Write to destination
            file.seek(SeekFrom::Start(dest + offset))?;
            file.write_all(&buffer[..current_chunk])?;

            offset += current_chunk as u64;
            remaining -= current_chunk as u64;
        }
    }

    file.flush()?;
    Ok(())
}

/// Insert empty bytes at a specific offset in a file
///
/// This function inserts `size` null bytes at the specified `offset` in the file,
/// shifting existing content to the right. The file size increases by `size` bytes.
///
/// # Arguments
/// * `file` - A mutable reference to the file
/// * `size` - Number of bytes to insert
/// * `offset` - Position where bytes will be inserted
/// * `buffer_size` - Size of buffer to use for the operation
///
/// # Returns
/// * `Ok(())` if the operation succeeds
/// * `Err(AudexError)` if an I/O error occurs or invalid parameters are provided
///
/// # Example
/// ```rust,no_run
/// use std::fs::OpenOptions;
/// use audex::util::{insert_bytes, DEFAULT_BUFFER_SIZE};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut file = OpenOptions::new().read(true).write(true).open("test.dat")?;
///
/// // Insert 100 null bytes at offset 50
/// insert_bytes(&mut file, 100, 50, Some(DEFAULT_BUFFER_SIZE))?;
/// # Ok(())
/// # }
/// ```
pub fn insert_bytes<F>(
    file: &mut F,
    size: u64,
    offset: u64,
    buffer_size: Option<usize>,
) -> Result<()>
where
    F: Read + Write + Seek + 'static,
{
    trace_event!(offset = offset, size = size, "inserting bytes");

    if size == 0 {
        return Ok(());
    }

    // Guard against overflow: size must fit in i64 for resize_file
    let size_i64 = i64::try_from(size).map_err(|_| {
        AudexError::InvalidData(format!(
            "Insert size {} exceeds maximum supported value ({})",
            size,
            i64::MAX
        ))
    })?;

    let buffer_size = buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    if buffer_size == 0 {
        return Err(AudexError::InvalidData(
            "Buffer size cannot be zero".to_string(),
        ));
    }

    // Get current file size and validate offset
    let current_pos = file.stream_position()?;
    let file_size = file.seek(SeekFrom::End(0))?;

    if offset > file_size {
        return Err(AudexError::InvalidData(format!(
            "Offset beyond file size: {} > {}",
            offset, file_size
        )));
    }

    // If inserting at the end, just extend the file
    if offset == file_size {
        resize_file(file, size_i64, Some(buffer_size))?;
        file.seek(SeekFrom::Start(current_pos))?;
        return Ok(());
    }

    // First, extend the file to make room
    resize_file(file, size_i64, Some(buffer_size))?;

    // Then move the existing data to the right
    let dest_offset = offset.checked_add(size).ok_or_else(|| {
        AudexError::InvalidData(format!(
            "Destination offset overflow: {} + {} exceeds u64 range",
            offset, size
        ))
    })?;

    let bytes_to_move = file_size - offset;
    if bytes_to_move > 0 {
        move_bytes(file, dest_offset, offset, bytes_to_move, Some(buffer_size))?;
    }

    // Clear the inserted region with null bytes
    file.seek(SeekFrom::Start(offset))?;
    let mut remaining = size;
    let mut buffer = vec![
        0u8;
        std::cmp::min(
            buffer_size,
            usize::try_from(remaining).unwrap_or(usize::MAX)
        )
    ];

    while remaining > 0 {
        let chunk_size = std::cmp::min(buffer_size as u64, remaining) as usize;
        buffer.resize(chunk_size, 0);
        file.write_all(&buffer)?;
        remaining -= chunk_size as u64;
    }

    file.flush()?;
    file.seek(SeekFrom::Start(current_pos))?;
    Ok(())
}

/// Delete bytes from a specific offset in a file
///
/// This function removes `size` bytes starting at the specified `offset` in the file,
/// shifting remaining content to the left. The file size decreases by `size` bytes.
///
/// # Arguments
/// * `file` - A mutable reference to the file
/// * `size` - Number of bytes to delete
/// * `offset` - Position where deletion starts
/// * `buffer_size` - Size of buffer to use for the operation
///
/// # Returns
/// * `Ok(())` if the operation succeeds
/// * `Err(AudexError)` if an I/O error occurs or invalid parameters are provided
///
/// # Example
/// ```rust,no_run
/// use std::fs::OpenOptions;
/// use audex::util::{delete_bytes, DEFAULT_BUFFER_SIZE};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut file = OpenOptions::new().read(true).write(true).open("test.dat")?;
///
/// // Delete 100 bytes starting at offset 50
/// delete_bytes(&mut file, 100, 50, Some(DEFAULT_BUFFER_SIZE))?;
/// # Ok(())
/// # }
/// ```
pub fn delete_bytes<F>(
    file: &mut F,
    size: u64,
    offset: u64,
    buffer_size: Option<usize>,
) -> Result<()>
where
    F: Read + Write + Seek + 'static,
{
    trace_event!(offset = offset, size = size, "deleting bytes");

    if size == 0 {
        return Ok(());
    }

    // Guard against overflow: size must fit in i64 for resize_file
    if size > i64::MAX as u64 {
        return Err(AudexError::InvalidData(format!(
            "Delete size {} exceeds maximum supported value ({})",
            size,
            i64::MAX
        )));
    }

    let buffer_size = buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    if buffer_size == 0 {
        return Err(AudexError::InvalidData(
            "Buffer size cannot be zero".to_string(),
        ));
    }

    // Get current file size and validate parameters
    let current_pos = file.stream_position()?;
    let file_size = file.seek(SeekFrom::End(0))?;

    if offset > file_size {
        return Err(AudexError::InvalidData(format!(
            "Delete offset ({}) exceeds file size ({})",
            offset, file_size
        )));
    }

    if size > file_size - offset {
        return Err(AudexError::InvalidData("Area beyond file size".to_string()));
    }

    // Clamp size to not exceed file bounds
    let actual_size = std::cmp::min(size, file_size - offset);
    if actual_size == 0 {
        file.seek(SeekFrom::Start(current_pos))?;
        return Ok(());
    }

    let delete_end = offset + actual_size;
    let bytes_after_deletion = file_size - delete_end;

    // Move data after the deleted region to fill the gap
    if bytes_after_deletion > 0 {
        move_bytes(
            file,
            offset,
            delete_end,
            bytes_after_deletion,
            Some(buffer_size),
        )?;
    }

    // Shrink the file — validate that actual_size fits in i64 before casting
    let actual_size_i64 = i64::try_from(actual_size)
        .map_err(|_| AudexError::InvalidData("file size exceeds i64 range".to_string()))?;
    let new_size = file_size - actual_size;
    resize_file(file, -actual_size_i64, Some(buffer_size))?;

    // Restore position, clamping to new file size
    let new_pos = std::cmp::min(current_pos, new_size);
    file.seek(SeekFrom::Start(new_pos))?;

    Ok(())
}

/// Resize a region of bytes within a file
///
/// This is a convenience function that handles resizing a specific region of a file.
/// If `new_size > old_size`, bytes are inserted. If `new_size < old_size`, bytes are deleted.
/// This is commonly used when updating metadata blocks that may change in size.
///
/// # Arguments
/// * `file` - A mutable reference to the file
/// * `old_size` - Current size of the region in bytes
/// * `new_size` - Desired new size of the region in bytes
/// * `offset` - Starting position of the region to resize
///
/// # Returns
/// * `Ok(())` if the operation succeeds
/// * `Err(AudexError)` if an I/O error occurs or invalid parameters are provided
///
/// # Example
/// ```rust,no_run
/// use std::fs::OpenOptions;
/// use audex::util::resize_bytes;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut file = OpenOptions::new().read(true).write(true).open("test.dat")?;
///
/// // Resize region at offset 100 from 50 bytes to 75 bytes (insert 25 bytes)
/// resize_bytes(&mut file, 50, 75, 100)?;
///
/// // Resize region at offset 200 from 100 bytes to 60 bytes (delete 40 bytes)
/// resize_bytes(&mut file, 100, 60, 200)?;
/// # Ok(())
/// # }
/// ```
pub fn resize_bytes<F>(file: &mut F, old_size: u64, new_size: u64, offset: u64) -> Result<()>
where
    F: Read + Write + Seek + 'static,
{
    trace_event!(old_size = old_size, new_size = new_size, "resizing bytes");

    if old_size == new_size {
        return Ok(());
    }

    let buffer_size = DEFAULT_BUFFER_SIZE;

    let current_pos = file.stream_position()?;
    let file_size = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(current_pos))?;

    // Use subtraction instead of addition to avoid u64 overflow.
    // Since we already verified offset <= file_size, (file_size - offset) is safe.
    if offset > file_size || old_size > file_size - offset {
        return Err(AudexError::InvalidData(
            "Region extends beyond file size".to_string(),
        ));
    }

    if new_size > old_size {
        // Insert bytes - add space for the additional bytes
        let size_diff = new_size - old_size;
        let insert_offset = offset + old_size;
        insert_bytes(file, size_diff, insert_offset, Some(buffer_size))
    } else {
        // Delete bytes - remove the excess bytes
        let size_diff = old_size - new_size;
        let delete_offset = offset + new_size;
        delete_bytes(file, size_diff, delete_offset, Some(buffer_size))
    }
}

/// Verify that the given file handle is usable
///
/// This function verifies file handle capabilities by checking:
/// - The file can be read from
/// - If writable is true, the file can be written to
pub fn verify_fileobj(fileobj: &mut File, writable: bool) -> Result<()> {
    // Test reading capability by reading 0 bytes
    let current_pos = fileobj
        .stream_position()
        .map_err(|_| AudexError::InvalidData("not a valid file object".to_string()))?;

    let mut buffer = Vec::new();
    match fileobj.read(&mut buffer) {
        Ok(_) => {
            // Restore position after read test (read(0) doesn't advance position)
            fileobj.seek(SeekFrom::Start(current_pos)).map_err(|e| {
                AudexError::InvalidData(format!("Can't read from file object: {}", e))
            })?;
        }
        Err(e) => {
            return Err(AudexError::InvalidData(format!(
                "Can't read from file object: {}",
                e
            )));
        }
    }

    if writable {
        // Test writing capability by writing 0 bytes
        match fileobj.write_all(&[]) {
            Ok(()) => {}
            Err(e) => {
                return Err(AudexError::InvalidData(format!(
                    "Can't write to file object: {}",
                    e
                )));
            }
        }
    }

    Ok(())
}

/// Verify that the argument is a valid filename path.
///
/// Rejects paths that refer to OS device names or special device paths
/// which could cause unintended behavior when opened as regular files.
pub fn verify_filename<P: AsRef<Path> + ?Sized>(filename: &P) -> Result<()> {
    let path = filename.as_ref();
    let path_str = path.to_string_lossy();

    // Reject Windows reserved device names (CON, NUL, PRN, AUX, COM1-9, LPT1-9).
    // Extract the file stem so that "NUL.txt" is also caught.
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        if is_windows_reserved_name(stem) {
            return Err(AudexError::InvalidData(format!(
                "{:?} is a reserved device name",
                path
            )));
        }
    }

    // Reject Unix device paths
    if path_str.starts_with("/dev/") {
        return Err(AudexError::InvalidData(format!(
            "{:?} is a device path",
            path
        )));
    }

    Ok(())
}

/// A wrapper that holds either a file object or filename information.
#[derive(Debug)]
pub struct FileThing<F> {
    pub fileobj: F,
    pub filename: Option<PathBuf>,
    pub name: PathBuf,
}

/// Specialized FileThing for standard File objects
pub type FileFileThing = FileThing<File>;

/// FileThing for in-memory operations
pub type MemoryFileThing = FileThing<Cursor<Vec<u8>>>;

/// FileThing that can handle both file and memory operations
#[derive(Debug)]
pub enum AnyFileThing {
    File(FileFileThing),
    Memory(MemoryFileThing),
}

impl AnyFileThing {
    /// Get the display name for this file thing
    pub fn display_name(&self) -> &Path {
        match self {
            AnyFileThing::File(f) => f.display_name(),
            AnyFileThing::Memory(f) => f.display_name(),
        }
    }

    /// Check if this was created from a filename
    pub fn from_filename(&self) -> bool {
        match self {
            AnyFileThing::File(f) => f.from_filename(),
            AnyFileThing::Memory(f) => f.from_filename(),
        }
    }

    /// Check if this is using memory storage
    pub fn is_memory(&self) -> bool {
        matches!(self, AnyFileThing::Memory(_))
    }

    /// Check if this is using file storage
    pub fn is_file(&self) -> bool {
        matches!(self, AnyFileThing::File(_))
    }

    /// Get the size of the underlying data if possible
    pub fn size(&mut self) -> Result<u64> {
        match self {
            AnyFileThing::File(f) => get_size(&mut f.fileobj),
            AnyFileThing::Memory(f) => {
                let _pos = f.fileobj.position();
                let size = f.fileobj.get_ref().len() as u64;
                Ok(size)
            }
        }
    }

    /// Write back memory content to the original file if using memory storage.
    /// This provides automatic write-back behavior.
    pub fn write_back(&self) -> Result<()> {
        match self {
            AnyFileThing::File(_) => Ok(()), // File operations are already persistent
            AnyFileThing::Memory(memory_thing) => write_back_memory(memory_thing),
        }
    }

    /// Get the underlying filename if this FileThing was created from a file path
    pub fn filename(&self) -> Option<&Path> {
        match self {
            AnyFileThing::File(f) => f.filename.as_deref(),
            AnyFileThing::Memory(f) => f.filename.as_deref(),
        }
    }

    /// Truncate the file/memory to the specified size
    pub fn truncate(&mut self, size: u64) -> Result<()> {
        match self {
            AnyFileThing::File(f) => {
                f.fileobj.set_len(size).map_err(|e| {
                    AudexError::InvalidData(format!("Failed to truncate file: {}", e))
                })?;
                Ok(())
            }
            AnyFileThing::Memory(f) => {
                let data = f.fileobj.get_mut();
                let truncate_size = usize::try_from(size).map_err(|_| {
                    AudexError::InvalidData(
                        "Truncation size exceeds platform address space".to_string(),
                    )
                })?;
                data.truncate(truncate_size);
                Ok(())
            }
        }
    }

    /// Flush any pending writes
    pub fn flush(&mut self) -> Result<()> {
        match self {
            AnyFileThing::File(f) => {
                f.fileobj
                    .flush()
                    .map_err(|e| AudexError::InvalidData(format!("Failed to flush file: {}", e)))?;
                Ok(())
            }
            AnyFileThing::Memory(_) => Ok(()), // Memory operations don't need flushing
        }
    }

    /// Get a hint about the length of the data without seeking
    /// This is useful for padding calculators and other metadata operations
    pub fn len_hint(&mut self) -> Result<u64> {
        self.size()
    }
}

/// Implement Read trait for AnyFileThing
impl Read for AnyFileThing {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            AnyFileThing::File(f) => f.read(buf),
            AnyFileThing::Memory(f) => f.read(buf),
        }
    }
}

/// Implement Write trait for AnyFileThing
impl Write for AnyFileThing {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            AnyFileThing::File(f) => f.write(buf),
            AnyFileThing::Memory(f) => f.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            AnyFileThing::File(f) => f.flush(),
            AnyFileThing::Memory(f) => f.flush(),
        }
    }
}

/// Implement Seek trait for AnyFileThing
impl Seek for AnyFileThing {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        match self {
            AnyFileThing::File(f) => f.seek(pos),
            AnyFileThing::Memory(f) => f.seek(pos),
        }
    }
}

/// Try to construct AnyFileThing from a path reference.
/// Returns an error if the file cannot be opened, rather than silently
/// falling back to an empty in-memory buffer.
impl TryFrom<&Path> for AnyFileThing {
    type Error = AudexError;

    fn try_from(path: &Path) -> Result<Self> {
        let options = LoadFileOptions::read_method();
        openfile_simple(FileInput::from_path(path), &options)
    }
}

/// Try to construct AnyFileThing from a PathBuf.
/// Delegates to the `TryFrom<&Path>` implementation.
impl TryFrom<PathBuf> for AnyFileThing {
    type Error = AudexError;

    fn try_from(path: PathBuf) -> Result<Self> {
        AnyFileThing::try_from(path.as_path())
    }
}

/// Construct AnyFileThing from an in-memory cursor
impl From<Cursor<Vec<u8>>> for AnyFileThing {
    fn from(cursor: Cursor<Vec<u8>>) -> Self {
        let file_thing = FileThing::new(cursor, None, PathBuf::from("<memory>"));
        AnyFileThing::Memory(file_thing)
    }
}

/// Try to construct AnyFileThing from an open File handle
impl TryFrom<File> for AnyFileThing {
    type Error = AudexError;

    fn try_from(file: File) -> Result<Self> {
        let options = LoadFileOptions::read_method();
        openfile_simple(FileInput::File(file), &options)
    }
}

impl<F> FileThing<F> {
    pub fn new(fileobj: F, filename: Option<PathBuf>, name: PathBuf) -> Self {
        Self {
            fileobj,
            filename,
            name,
        }
    }

    /// Get the display name for this file thing
    pub fn display_name(&self) -> &Path {
        &self.name
    }

    /// Check if this was created from a filename
    pub fn from_filename(&self) -> bool {
        self.filename.is_some()
    }
}

impl<F: Read> Read for FileThing<F> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.fileobj.read(buf)
    }
}

impl<F: Write> Write for FileThing<F> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.fileobj.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.fileobj.flush()
    }
}

impl<F: Seek> Seek for FileThing<F> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.fileobj.seek(pos)
    }
}

/// Different types of file arguments that can be passed
#[derive(Debug)]
pub enum FileOrPath {
    File(File),
    Path(PathBuf),
    PathRef(PathBuf),
}

/// Configuration options for file loading operations with memory fallback support.
///
/// This struct controls how files are opened and whether memory fallback is used when
/// filesystem operations fail. It's primarily used with [`loadfile_process`] and
/// `loadfile_guard` functions.
///
/// # Fields
///
/// * `method` - Whether this is for a method call (true) or standalone function (false)
/// * `writable` - Whether write access is required
/// * `create` - Whether to create the file if it doesn't exist
///
/// # Examples
///
/// ```rust,no_run
/// use audex::util::{LoadFileOptions, loadfile_process};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Reading a file
/// let read_opts = LoadFileOptions::read_function();
/// let file_thing = loadfile_process("audio.mp3", &read_opts)?;
///
/// // Writing to a file
/// let write_opts = LoadFileOptions::write_function();
/// let file_thing = loadfile_process("audio.mp3", &write_opts)?;
///
/// // Creating a new file
/// let create_opts = LoadFileOptions::create_function();
/// let file_thing = loadfile_process("new.mp3", &create_opts)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct LoadFileOptions {
    /// Whether this is for a method call (true) or standalone function (false)
    pub method: bool,
    /// Whether write access is required
    pub writable: bool,
    /// Whether to create the file if it doesn't exist
    pub create: bool,
}

impl LoadFileOptions {
    pub fn new(method: bool, writable: bool, create: bool) -> Self {
        Self {
            method,
            writable,
            create,
        }
    }

    /// Create options for a method that only reads
    pub fn read_method() -> Self {
        Self::new(true, false, false)
    }

    /// Create options for a method that writes
    pub fn write_method() -> Self {
        Self::new(true, true, false)
    }

    /// Create options for a method that creates files
    pub fn create_method() -> Self {
        Self::new(true, true, true)
    }

    /// Create options for a function (not method) that only reads
    pub fn read_function() -> Self {
        Self::new(false, false, false)
    }

    /// Create options for a function that writes
    pub fn write_function() -> Self {
        Self::new(false, true, false)
    }

    /// Create options for a function that creates files
    pub fn create_function() -> Self {
        Self::new(false, true, true)
    }

    /// Check if these options require write access
    pub fn needs_write(&self) -> bool {
        self.writable
    }

    /// Check if these options allow file creation
    pub fn allows_create(&self) -> bool {
        self.create
    }

    /// Check if this is for a method call (vs standalone function)
    pub fn is_method(&self) -> bool {
        self.method
    }
}

/// Process a file argument into a FileThing with memory fallback capability.
/// This matches the loadfile processing behavior exactly.
pub fn process_file_arg<P: AsRef<Path>>(
    file_arg: FileOrPath,
    options: &LoadFileOptions,
) -> Result<AnyFileThing> {
    match file_arg {
        FileOrPath::File(file) => {
            // File object passed directly - create FileThing with no filename
            let display_name = PathBuf::from("<file-like>");
            let file_thing = FileThing::new(file, None, display_name);
            Ok(AnyFileThing::File(file_thing))
        }
        FileOrPath::Path(path) | FileOrPath::PathRef(path) => process_file_path(&path, options),
    }
}

/// Process a file path into a FileThing with automatic memory fallback.
/// Implements the core behavior including EOPNOTSUPP handling.
fn process_file_path(path: &Path, options: &LoadFileOptions) -> Result<AnyFileThing> {
    let path_buf = path.to_path_buf();
    let display_name = path_buf.clone();

    // Debug-build hook that forces writable paths through the memory fallback.
    // This keeps fallback behavior reproducible in tests without depending on
    // platform-specific filesystems or mount options.
    if cfg!(debug_assertions)
        && options.writable
        && std::env::var_os("AUDEX_FORCE_MEMORY_FALLBACK").is_some()
    {
        return create_memory_fallback(path, options);
    }

    // Try to open the file first
    match try_open_file(path, options) {
        Ok(file) => {
            let file_thing = FileThing::new(file, Some(path_buf), display_name);
            Ok(AnyFileThing::File(file_thing))
        }
        Err(err) => {
            // Check if we should fall back to memory
            if should_use_memory_fallback(&err, options) {
                create_memory_fallback(path, options)
            } else {
                Err(err)
            }
        }
    }
}

/// Try to open a file with the specified options.
fn try_open_file(path: &Path, options: &LoadFileOptions) -> Result<File> {
    let mut open_options = OpenOptions::new();

    if options.writable {
        open_options.write(true);
        if options.create {
            open_options.create(true);
        }
        open_options.read(true); // Always need read for audio metadata operations
    } else {
        open_options.read(true);
    }

    match open_options.open(path) {
        Ok(file) => Ok(file),
        Err(io_err) => {
            // Convert IO error to AudexError with proper error classification
            let operation = if options.writable {
                if options.create {
                    "open for write (create)"
                } else {
                    "open for write"
                }
            } else {
                "open for read"
            };
            Err(io_error_to_audex_error(io_err, path, operation))
        }
    }
}

/// Determine if we should use memory fallback based on the error.
/// This matches the EOPNOTSUPP handling behavior.
fn should_use_memory_fallback(err: &AudexError, options: &LoadFileOptions) -> bool {
    if !options.writable {
        return false; // Only fallback for writable operations
    }

    // Check if the error indicates filesystem doesn't support the operation
    if let AudexError::InvalidData(msg) = err {
        // Look for EOPNOTSUPP-like conditions that indicate filesystem limitations
        msg.contains("Operation not supported") ||
        msg.contains("Not supported") ||
        msg.contains("Read-only file system") ||
        msg.contains("Function not implemented") ||
        // Network filesystems that don't support certain operations
        msg.contains("Network is unreachable") ||
        msg.contains("Remote I/O error") ||
        msg.contains("Stale file handle") ||
        // Filesystems mounted with noatime, nosuid, etc
        msg.contains("Invalid argument") && msg.contains("mount") ||
        // Special filesystems like /proc, /sys
        (msg.contains("/proc") || msg.contains("/sys")) && msg.contains("Permission denied")
    } else {
        false
    }
}

/// Convert a std::io::Error to an appropriate AudexError with context.
fn io_error_to_audex_error(io_err: std::io::Error, path: &Path, operation: &str) -> AudexError {
    use std::io::ErrorKind;

    let path_str = path.display().to_string();
    let base_msg = format!("{}: {} ({})", path_str, io_err, operation);

    match io_err.kind() {
        ErrorKind::NotFound => AudexError::InvalidData(format!(
            "[Errno 2] No such file or directory: '{}'",
            path_str
        )),
        ErrorKind::PermissionDenied => {
            AudexError::InvalidData(format!("[Errno 13] Permission denied: '{}'", path_str))
        }
        ErrorKind::AlreadyExists => {
            AudexError::InvalidData(format!("[Errno 17] File exists: '{}'", path_str))
        }
        ErrorKind::InvalidInput => {
            AudexError::InvalidData(format!("[Errno 22] Invalid argument: '{}'", path_str))
        }
        ErrorKind::WriteZero => AudexError::InvalidData(format!(
            "[Errno 28] No space left on device: '{}'",
            path_str
        )),
        ErrorKind::Interrupted => {
            AudexError::InvalidData(format!("[Errno 4] Interrupted system call: '{}'", path_str))
        }
        ErrorKind::UnexpectedEof => {
            AudexError::InvalidData(format!("Unexpected end of file: '{}'", path_str))
        }
        _ => {
            // For other errors, preserve the original message but add context
            AudexError::InvalidData(base_msg)
        }
    }
}

/// Create a memory-based FileThing that can write back to disk.
fn create_memory_fallback(path: &Path, options: &LoadFileOptions) -> Result<AnyFileThing> {
    // Maximum file size for memory fallback — matches the default
    // cap used by try_memory_fallback to prevent OOM on large files
    const MAX_MEMORY_FALLBACK: u64 = 100 * 1024 * 1024; // 100 MB

    let path_buf = path.to_path_buf();
    let display_name = path_buf.clone();

    // Try to read existing file content into memory.
    // Use a bounded read (take) instead of a metadata-check-then-read
    // to avoid a race where the file grows between the size check and
    // the actual read, bypassing the size cap.
    let initial_data = if path.exists() {
        let file = File::open(path).map_err(|e| {
            AudexError::InvalidData(format!(
                "Failed to open '{}' for memory fallback: {}",
                path.display(),
                e
            ))
        })?;
        let mut reader = file.take(MAX_MEMORY_FALLBACK + 1);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).map_err(|e| {
            AudexError::InvalidData(format!(
                "I/O error reading '{}' for memory fallback: {}",
                path.display(),
                e
            ))
        })?;

        if buf.len() as u64 > MAX_MEMORY_FALLBACK {
            return Err(AudexError::InvalidData(format!(
                "File '{}' too large for memory fallback: exceeds {} byte cap",
                path.display(),
                MAX_MEMORY_FALLBACK
            )));
        }
        buf
    } else if options.create {
        Vec::new() // Start with empty data for new file
    } else {
        return Err(AudexError::InvalidData(format!(
            "{}: No such file or directory (memory fallback)",
            path.display()
        )));
    };

    let cursor = Cursor::new(initial_data);
    let memory_thing = MemoryFileThing::new(cursor, Some(path_buf), display_name);
    Ok(AnyFileThing::Memory(memory_thing))
}

/// Write back memory content to the original file.
/// This implements the write-back mechanism.
pub fn write_back_memory(memory_thing: &MemoryFileThing) -> Result<()> {
    if let Some(ref path) = memory_thing.filename {
        let data = memory_thing.fileobj.get_ref();
        match std::fs::write(path, data) {
            Ok(()) => Ok(()),
            Err(io_err) => {
                let msg = format!("Failed to write back to {}: {}", path.display(), io_err);
                Err(AudexError::InvalidData(msg))
            }
        }
    } else {
        Ok(()) // Nothing to write back if no filename
    }
}

/// Main loadfile processing function.
/// This is the primary entry point for file processing.
pub fn loadfile_process<P: AsRef<Path>>(
    file_arg: P,
    options: &LoadFileOptions,
) -> Result<AnyFileThing> {
    let path = file_arg.as_ref();
    trace_event!(
        path = %path.display(),
        writable = options.writable,
        "loading file for processing"
    );
    let file_or_path = FileOrPath::Path(path.to_path_buf());
    process_file_arg::<PathBuf>(file_or_path, options)
}

/// Convenience function to process a file for reading.
pub fn loadfile_read<P: AsRef<Path>>(file_path: P) -> Result<AnyFileThing> {
    loadfile_process(file_path, &LoadFileOptions::read_function())
}

/// Convenience function to process a file for writing.
pub fn loadfile_write<P: AsRef<Path>>(file_path: P) -> Result<AnyFileThing> {
    loadfile_process(file_path, &LoadFileOptions::write_function())
}

pub fn get_size(fileobj: &mut File) -> Result<u64> {
    let old_pos = fileobj.stream_position()?;
    let size = fileobj.seek(SeekFrom::End(0))?;
    fileobj.seek(SeekFrom::Start(old_pos))?;
    Ok(size)
}

/// Check if string starts with prefix
pub fn startswith(text: &str, prefix: &str) -> bool {
    text.starts_with(prefix)
}

/// Calculate CRC32 checksum
pub fn crc32(data: &[u8]) -> u32 {
    const CRC32_TABLE: [u32; 256] = [
        0x00000000, 0x77073096, 0xee0e612c, 0x990951ba, 0x076dc419, 0x706af48f, 0xe963a535,
        0x9e6495a3, 0x0edb8832, 0x79dcb8a4, 0xe0d5e91e, 0x97d2d988, 0x09b64c2b, 0x7eb17cbd,
        0xe7b82d07, 0x90bf1d91, 0x1db71064, 0x6ab020f2, 0xf3b97148, 0x84be41de, 0x1adad47d,
        0x6ddde4eb, 0xf4d4b551, 0x83d385c7, 0x136c9856, 0x646ba8c0, 0xfd62f97a, 0x8a65c9ec,
        0x14015c4f, 0x63066cd9, 0xfa0f3d63, 0x8d080df5, 0x3b6e20c8, 0x4c69105e, 0xd56041e4,
        0xa2677172, 0x3c03e4d1, 0x4b04d447, 0xd20d85fd, 0xa50ab56b, 0x35b5a8fa, 0x42b2986c,
        0xdbbbc9d6, 0xacbcf940, 0x32d86ce3, 0x45df5c75, 0xdcd60dcf, 0xabd13d59, 0x26d930ac,
        0x51de003a, 0xc8d75180, 0xbfd06116, 0x21b4f4b5, 0x56b3c423, 0xcfba9599, 0xb8bda50f,
        0x2802b89e, 0x5f058808, 0xc60cd9b2, 0xb10be924, 0x2f6f7c87, 0x58684c11, 0xc1611dab,
        0xb6662d3d, 0x76dc4190, 0x01db7106, 0x98d220bc, 0xefd5102a, 0x71b18589, 0x06b6b51f,
        0x9fbfe4a5, 0xe8b8d433, 0x7807c9a2, 0x0f00f934, 0x9609a88e, 0xe10e9818, 0x7f6a0dbb,
        0x086d3d2d, 0x91646c97, 0xe6635c01, 0x6b6b51f4, 0x1c6c6162, 0x856530d8, 0xf262004e,
        0x6c0695ed, 0x1b01a57b, 0x8208f4c1, 0xf50fc457, 0x65b0d9c6, 0x12b7e950, 0x8bbeb8ea,
        0xfcb9887c, 0x62dd1ddf, 0x15da2d49, 0x8cd37cf3, 0xfbd44c65, 0x4db26158, 0x3ab551ce,
        0xa3bc0074, 0xd4bb30e2, 0x4adfa541, 0x3dd895d7, 0xa4d1c46d, 0xd3d6f4fb, 0x4369e96a,
        0x346ed9fc, 0xad678846, 0xda60b8d0, 0x44042d73, 0x33031de5, 0xaa0a4c5f, 0xdd0d7cc9,
        0x5005713c, 0x270241aa, 0xbe0b1010, 0xc90c2086, 0x5768b525, 0x206f85b3, 0xb966d409,
        0xce61e49f, 0x5edef90e, 0x29d9c998, 0xb0d09822, 0xc7d7a8b4, 0x59b33d17, 0x2eb40d81,
        0xb7bd5c3b, 0xc0ba6cad, 0xedb88320, 0x9abfb3b6, 0x03b6e20c, 0x74b1d29a, 0xead54739,
        0x9dd277af, 0x04db2615, 0x73dc1683, 0xe3630b12, 0x94643b84, 0x0d6d6a3e, 0x7a6a5aa8,
        0xe40ecf0b, 0x9309ff9d, 0x0a00ae27, 0x7d079eb1, 0xf00f9344, 0x8708a3d2, 0x1e01f268,
        0x6906c2fe, 0xf762575d, 0x806567cb, 0x196c3671, 0x6e6b06e7, 0xfed41b76, 0x89d32be0,
        0x10da7a5a, 0x67dd4acc, 0xf9b9df6f, 0x8ebeeff9, 0x17b7be43, 0x60b08ed5, 0xd6d6a3e8,
        0xa1d1937e, 0x38d8c2c4, 0x4fdff252, 0xd1bb67f1, 0xa6bc5767, 0x3fb506dd, 0x48b2364b,
        0xd80d2bda, 0xaf0a1b4c, 0x36034af6, 0x41047a60, 0xdf60efc3, 0xa867df55, 0x316e8eef,
        0x4669be79, 0xcb61b38c, 0xbc66831a, 0x256fd2a0, 0x5268e236, 0xcc0c7795, 0xbb0b4703,
        0x220216b9, 0x5505262f, 0xc5ba3bbe, 0xb2bd0b28, 0x2bb45a92, 0x5cb36a04, 0xc2d7ffa7,
        0xb5d0cf31, 0x2cd99e8b, 0x5bdeae1d, 0x9b64c2b0, 0xec63f226, 0x756aa39c, 0x026d930a,
        0x9c0906a9, 0xeb0e363f, 0x72076785, 0x05005713, 0x95bf4a82, 0xe2b87a14, 0x7bb12bae,
        0x0cb61b38, 0x92d28e9b, 0xe5d5be0d, 0x7cdcefb7, 0x0bdbdf21, 0x86d3d2d4, 0xf1d4e242,
        0x68ddb3f8, 0x1fda836e, 0x81be16cd, 0xf6b9265b, 0x6fb077e1, 0x18b74777, 0x88085ae6,
        0xff0f6a70, 0x66063bca, 0x11010b5c, 0x8f659eff, 0xf862ae69, 0x616bffd3, 0x166ccf45,
        0xa00ae278, 0xd70dd2ee, 0x4e048354, 0x3903b3c2, 0xa7672661, 0xd06016f7, 0x4969474d,
        0x3e6e77db, 0xaed16a4a, 0xd9d65adc, 0x40df0b66, 0x37d83bf0, 0xa9bcae53, 0xdebb9ec5,
        0x47b2cf7f, 0x30b5ffe9, 0xbdbdf21c, 0xcabac28a, 0x53b39330, 0x24b4a3a6, 0xbad03605,
        0xcdd70693, 0x54de5729, 0x23d967bf, 0xb3667a2e, 0xc4614ab8, 0x5d681b02, 0x2a6f2b94,
        0xb40bbe37, 0xc30c8ea1, 0x5a05df1b, 0x2d02ef8d,
    ];

    let mut crc = 0xffffffff;
    for &byte in data {
        let table_index = ((crc ^ byte as u32) & 0xff) as usize;
        crc = CRC32_TABLE[table_index] ^ (crc >> 8);
    }
    crc ^ 0xffffffff
}

#[cfg(feature = "async")]
use tokio::fs::{File as TokioFile, OpenOptions as TokioOpenOptions};
#[cfg(feature = "async")]
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// Read exactly `size` bytes from file or return an error asynchronously.
///
/// This function reads exactly the requested number of bytes from the current
/// file position. If fewer bytes are available, an error is returned.
///
/// # Arguments
/// * `file` - A mutable reference to the async file
/// * `size` - Number of bytes to read
///
/// # Returns
/// * `Ok(Vec<u8>)` - The bytes read from the file
/// * `Err(AudexError)` - If not enough bytes are available
#[cfg(feature = "async")]
pub async fn read_full_async(file: &mut TokioFile, size: usize) -> Result<Vec<u8>> {
    // Check remaining bytes to avoid allocating a huge buffer from
    // an untrusted size field in a malformed file header
    let current_pos = file
        .stream_position()
        .await
        .map_err(|e| AudexError::InvalidData(format!("Cannot get file position: {}", e)))?;
    let end_pos = file
        .seek(SeekFrom::End(0))
        .await
        .map_err(|e| AudexError::InvalidData(format!("Cannot seek to end: {}", e)))?;
    file.seek(SeekFrom::Start(current_pos))
        .await
        .map_err(|e| AudexError::InvalidData(format!("Cannot restore file position: {}", e)))?;

    // Guard against cursor positioned past EOF — bare subtraction
    // would underflow and wrap to a huge value, bypassing the check.
    // Keep remaining as u64 so 32-bit platforms don't silently truncate
    // file sizes above 4 GB, which would bypass this validation.
    let remaining = end_pos.saturating_sub(current_pos);
    if (size as u64) > remaining {
        return Err(AudexError::InvalidData(format!(
            "Cannot read {} bytes: only {} bytes remaining in file",
            size, remaining
        )));
    }

    let mut buffer = vec![0u8; size];
    file.read_exact(&mut buffer)
        .await
        .map_err(|e| AudexError::InvalidData(format!("Cannot read {} bytes: {}", size, e)))?;
    Ok(buffer)
}

/// Open a file for reading asynchronously.
///
/// # Arguments
/// * `path` - Path to the file
///
/// # Returns
/// * `Ok(File)` - The opened file handle
/// * `Err(AudexError)` - If the file cannot be opened
#[cfg(feature = "async")]
pub async fn loadfile_read_async<P: AsRef<Path>>(path: P) -> Result<TokioFile> {
    TokioFile::open(path.as_ref()).await.map_err(AudexError::Io)
}

/// Open a file for reading and writing asynchronously.
///
/// # Arguments
/// * `path` - Path to the file
///
/// # Returns
/// * `Ok(File)` - The opened file handle
/// * `Err(AudexError)` - If the file cannot be opened
#[cfg(feature = "async")]
pub async fn loadfile_write_async<P: AsRef<Path>>(path: P) -> Result<TokioFile> {
    TokioOpenOptions::new()
        .read(true)
        .write(true)
        .open(path.as_ref())
        .await
        .map_err(AudexError::Io)
}

/// Move bytes within a file asynchronously.
///
/// This function copies `count` bytes from `src` position to `dest` position.
/// It properly handles overlapping regions by reading in the appropriate direction.
///
/// # Arguments
/// * `file` - A mutable reference to the async file
/// * `dest` - Destination position
/// * `src` - Source position
/// * `count` - Number of bytes to move
/// * `buffer_size` - Optional buffer size for the operation
///
/// # Returns
/// * `Ok(())` - Operation completed successfully
/// * `Err(AudexError)` - If an I/O error occurs
#[cfg(feature = "async")]
pub async fn move_bytes_async(
    file: &mut TokioFile,
    dest: u64,
    src: u64,
    count: u64,
    buffer_size: Option<usize>,
) -> Result<()> {
    if count == 0 || src == dest {
        return Ok(());
    }

    let buffer_size = buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    if buffer_size == 0 {
        return Err(AudexError::InvalidData(
            "Buffer size cannot be zero".to_string(),
        ));
    }

    // Validate source and destination ranges against file size,
    // matching the bounds checks in the sync move_bytes
    let current_pos = file.stream_position().await?;
    let file_size = file.seek(SeekFrom::End(0)).await?;
    file.seek(SeekFrom::Start(current_pos)).await?;

    if src.saturating_add(count) > file_size {
        return Err(AudexError::InvalidData(
            "Area outside of file: source range exceeds file size".to_string(),
        ));
    }

    if dest > file_size || dest.saturating_add(count) > file_size {
        return Err(AudexError::InvalidData(
            "Area outside of file: destination range exceeds file size".to_string(),
        ));
    }

    let mut buffer = vec![0u8; buffer_size];

    if dest < src {
        // Moving data left - read from start to end
        let mut bytes_moved = 0u64;
        while bytes_moved < count {
            let chunk_size = std::cmp::min(buffer_size as u64, count - bytes_moved) as usize;

            // Read from source (use read_exact to prevent short reads from corrupting data)
            file.seek(SeekFrom::Start(src + bytes_moved)).await?;
            file.read_exact(&mut buffer[..chunk_size]).await?;
            let bytes_read = chunk_size;

            // Write to destination
            file.seek(SeekFrom::Start(dest + bytes_moved)).await?;
            file.write_all(&buffer[..bytes_read]).await?;

            bytes_moved += bytes_read as u64;
        }
    } else {
        // Moving data right - read from end to start to avoid overwriting
        let mut bytes_remaining = count;
        while bytes_remaining > 0 {
            let chunk_size = std::cmp::min(buffer_size as u64, bytes_remaining) as usize;
            let chunk_offset = bytes_remaining - chunk_size as u64;

            // Read from source (use read_exact to prevent short reads from corrupting data)
            file.seek(SeekFrom::Start(src + chunk_offset)).await?;
            file.read_exact(&mut buffer[..chunk_size]).await?;
            let bytes_read = chunk_size;

            // Write to destination
            file.seek(SeekFrom::Start(dest + chunk_offset)).await?;
            file.write_all(&buffer[..bytes_read]).await?;

            bytes_remaining -= bytes_read as u64;
        }
    }

    file.flush().await?;
    Ok(())
}

/// Resize file by a relative amount asynchronously.
///
/// This function changes the file size by the specified difference.
/// Positive values extend the file, negative values shrink it.
///
/// # Arguments
/// * `file` - A mutable reference to the async file
/// * `diff` - Size change (positive to extend, negative to shrink)
/// * `buffer_size` - Optional buffer size for the operation
///
/// # Returns
/// * `Ok(())` - Operation completed successfully
/// * `Err(AudexError)` - If an I/O error occurs
#[cfg(feature = "async")]
pub async fn resize_file_async(
    file: &mut TokioFile,
    diff: i64,
    buffer_size: Option<usize>,
) -> Result<()> {
    if diff == 0 {
        return Ok(());
    }

    let buffer_size = buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    if buffer_size == 0 {
        return Err(AudexError::InvalidData(
            "Buffer size cannot be zero".to_string(),
        ));
    }

    // Get current file size
    let current_size = file.seek(SeekFrom::End(0)).await?;

    if diff > 0 {
        // Extend file — use checked arithmetic to prevent silent wraparound
        // when current_size is near u64::MAX
        let new_size = current_size.checked_add(diff as u64).ok_or_else(|| {
            AudexError::InvalidData(format!(
                "Cannot extend file: new size would overflow u64 (current {} + diff {})",
                current_size, diff
            ))
        })?;
        file.set_len(new_size).await?;
    } else {
        // Shrink file — use unsigned_abs() to safely convert negative diff to u64.
        // Direct negation (-diff) overflows when diff == i64::MIN.
        let shrink_amount = diff.unsigned_abs();
        if shrink_amount > current_size {
            return Err(AudexError::InvalidData(format!(
                "Cannot shrink file by {} bytes, file size is only {} bytes",
                shrink_amount, current_size
            )));
        }
        let new_size = current_size - shrink_amount;
        file.set_len(new_size).await?;
    }

    Ok(())
}

/// Insert bytes at a specific offset in a file asynchronously.
///
/// This function inserts `size` bytes filled with zeros at the specified `offset`.
/// Existing content from the offset onwards is shifted to the right.
///
/// # Arguments
/// * `file` - A mutable reference to the async file
/// * `size` - Number of bytes to insert
/// * `offset` - Position where insertion starts
/// * `buffer_size` - Optional buffer size for the operation
///
/// # Returns
/// * `Ok(())` - Operation completed successfully
/// * `Err(AudexError)` - If an I/O error occurs or invalid parameters are provided
#[cfg(feature = "async")]
pub async fn insert_bytes_async(
    file: &mut TokioFile,
    size: u64,
    offset: u64,
    buffer_size: Option<usize>,
) -> Result<()> {
    if size == 0 {
        return Ok(());
    }

    let buffer_size = buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    if buffer_size == 0 {
        return Err(AudexError::InvalidData(
            "Buffer size cannot be zero".to_string(),
        ));
    }

    // Get current file size and validate offset
    let current_pos = file.stream_position().await?;
    let file_size = file.seek(SeekFrom::End(0)).await?;

    if offset > file_size {
        return Err(AudexError::InvalidData(format!(
            "Offset beyond file size: {} > {}",
            offset, file_size
        )));
    }

    // Validate that size fits in i64 before casting
    let size_i64 = i64::try_from(size)
        .map_err(|_| AudexError::InvalidData("insert size exceeds i64 range".to_string()))?;

    // If inserting at the end, just extend the file
    if offset == file_size {
        resize_file_async(file, size_i64, Some(buffer_size)).await?;
        file.seek(SeekFrom::Start(current_pos)).await?;
        return Ok(());
    }

    // First, extend the file to make room
    resize_file_async(file, size_i64, Some(buffer_size)).await?;

    // Then move the existing data to the right
    let dest_offset = offset.checked_add(size).ok_or_else(|| {
        AudexError::InvalidData(format!(
            "Destination offset overflow: {} + {} exceeds u64 range",
            offset, size
        ))
    })?;

    let bytes_to_move = file_size - offset;
    if bytes_to_move > 0 {
        move_bytes_async(file, dest_offset, offset, bytes_to_move, Some(buffer_size)).await?;
    }

    // Clear the inserted region with null bytes
    file.seek(SeekFrom::Start(offset)).await?;
    let mut remaining = size;
    let mut buffer = vec![0u8; std::cmp::min(buffer_size, remaining as usize)];

    while remaining > 0 {
        let chunk_size = std::cmp::min(buffer_size as u64, remaining) as usize;
        buffer.resize(chunk_size, 0);
        file.write_all(&buffer).await?;
        remaining -= chunk_size as u64;
    }

    file.flush().await?;
    file.seek(SeekFrom::Start(current_pos)).await?;
    Ok(())
}

/// Delete bytes from a specific offset in a file asynchronously.
///
/// This function removes `size` bytes starting at the specified `offset`.
/// Content after the deleted region is shifted to the left.
///
/// # Arguments
/// * `file` - A mutable reference to the async file
/// * `size` - Number of bytes to delete
/// * `offset` - Position where deletion starts
/// * `buffer_size` - Optional buffer size for the operation
///
/// # Returns
/// * `Ok(())` - Operation completed successfully
/// * `Err(AudexError)` - If an I/O error occurs or invalid parameters are provided
#[cfg(feature = "async")]
pub async fn delete_bytes_async(
    file: &mut TokioFile,
    size: u64,
    offset: u64,
    buffer_size: Option<usize>,
) -> Result<()> {
    if size == 0 {
        return Ok(());
    }

    let buffer_size = buffer_size.unwrap_or(DEFAULT_BUFFER_SIZE);
    if buffer_size == 0 {
        return Err(AudexError::InvalidData(
            "Buffer size cannot be zero".to_string(),
        ));
    }

    // Get current file size and validate parameters
    let current_pos = file.stream_position().await?;
    let file_size = file.seek(SeekFrom::End(0)).await?;

    if offset > file_size {
        return Err(AudexError::InvalidData(format!(
            "Delete offset ({}) exceeds file size ({})",
            offset, file_size
        )));
    }

    if size > file_size - offset {
        return Err(AudexError::InvalidData("Area beyond file size".to_string()));
    }

    // Clamp size to not exceed file bounds
    let actual_size = std::cmp::min(size, file_size - offset);
    if actual_size == 0 {
        file.seek(SeekFrom::Start(current_pos)).await?;
        return Ok(());
    }

    let delete_end = offset + actual_size;
    let bytes_after_deletion = file_size - delete_end;

    // Move data after the deleted region to fill the gap
    if bytes_after_deletion > 0 {
        move_bytes_async(
            file,
            offset,
            delete_end,
            bytes_after_deletion,
            Some(buffer_size),
        )
        .await?;
    }

    // Shrink the file — validate that actual_size fits in i64 before casting
    let actual_size_i64 = i64::try_from(actual_size)
        .map_err(|_| AudexError::InvalidData("file size exceeds i64 range".to_string()))?;
    let new_size = file_size - actual_size;
    resize_file_async(file, -actual_size_i64, Some(buffer_size)).await?;

    // Restore position, clamping to new file size
    let new_pos = std::cmp::min(current_pos, new_size);
    file.seek(SeekFrom::Start(new_pos)).await?;

    Ok(())
}

/// Resize a region of bytes within a file asynchronously.
///
/// This is a convenience function that handles resizing a specific region.
/// If `new_size > old_size`, bytes are inserted. If `new_size < old_size`, bytes are deleted.
/// This is commonly used when updating metadata blocks that may change in size.
///
/// # Arguments
/// * `file` - A mutable reference to the async file
/// * `old_size` - Current size of the region in bytes
/// * `new_size` - Desired new size of the region in bytes
/// * `offset` - Starting position of the region to resize
///
/// # Returns
/// * `Ok(())` - Operation completed successfully
/// * `Err(AudexError)` - If an I/O error occurs or invalid parameters are provided
#[cfg(feature = "async")]
pub async fn resize_bytes_async(
    file: &mut TokioFile,
    old_size: u64,
    new_size: u64,
    offset: u64,
) -> Result<()> {
    if old_size == new_size {
        return Ok(());
    }

    let buffer_size = DEFAULT_BUFFER_SIZE;

    let current_pos = file.stream_position().await?;
    let file_size = file.seek(SeekFrom::End(0)).await?;
    file.seek(SeekFrom::Start(current_pos)).await?;

    // Use subtraction instead of addition to avoid u64 overflow.
    // Since we already verified offset <= file_size, (file_size - offset) is safe.
    if offset > file_size || old_size > file_size - offset {
        return Err(AudexError::InvalidData(
            "Region extends beyond file size".to_string(),
        ));
    }

    if new_size > old_size {
        // Insert bytes - add space for the additional bytes
        let size_diff = new_size - old_size;
        let insert_offset = offset + old_size;
        insert_bytes_async(file, size_diff, insert_offset, Some(buffer_size)).await
    } else {
        // Delete bytes - remove the excess bytes
        let size_diff = old_size - new_size;
        let delete_offset = offset + new_size;
        delete_bytes_async(file, size_diff, delete_offset, Some(buffer_size)).await
    }
}
