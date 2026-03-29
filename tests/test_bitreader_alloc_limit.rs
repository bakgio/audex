//! Tests for bounded memory allocation in BitReader::bytes().
//!
//! When reading byte sequences from untrusted streams, the requested count
//! comes directly from parsed file data. A malicious file can claim an
//! extremely large byte count, causing the reader to pre-allocate a massive
//! buffer via Vec::with_capacity() before any data is actually read. This
//! triggers an immediate OOM on most systems.
//!
//! These tests verify that the initial allocation is capped at a safe upper
//! bound, and that the reader still works correctly for legitimate counts.

mod common;

use std::io::Cursor;

/// Verify that requesting an absurdly large byte count from a tiny stream
/// does not attempt to pre-allocate gigabytes of memory.
///
/// Before the fix, this would call Vec::with_capacity(i32::MAX as usize),
/// attempting a ~2 GB allocation and panicking with OOM on most machines.
/// After the fix, the initial capacity is capped and the reader fails
/// gracefully when it runs out of actual data.
#[test]
fn huge_byte_count_does_not_cause_oom() {
    // A tiny 16-byte stream — nowhere near enough to satisfy the request
    let data: Vec<u8> = vec![0xAA; 16];
    let cursor = Cursor::new(data);

    let mut reader = audex::util::BitReader::new(cursor).unwrap();

    // Request nearly 2 GB worth of bytes from a 16-byte stream.
    // This must NOT attempt a 2 GB allocation — it should either:
    //   (a) cap the pre-allocation and then fail when data runs out, or
    //   (b) reject the count outright as exceeding the stream size.
    let result = reader.bytes(i32::MAX);
    assert!(
        result.is_err(),
        "Reading i32::MAX bytes from a 16-byte stream should fail, not allocate 2 GB"
    );
}

/// Same idea with a moderately large but still unreasonable count.
/// A 100 MB pre-allocation from a 32-byte file is clearly a spoofed header.
#[test]
fn large_byte_count_from_small_stream_fails_gracefully() {
    let data: Vec<u8> = vec![0xBB; 32];
    let cursor = Cursor::new(data);

    let mut reader = audex::util::BitReader::new(cursor).unwrap();

    // 100 MB request from a 32-byte stream — must not pre-allocate 100 MB
    let result = reader.bytes(100_000_000);
    assert!(
        result.is_err(),
        "Reading 100 MB from a 32-byte stream should fail gracefully"
    );
}

/// Confirm that normal, legitimate byte reads still work correctly.
#[test]
fn normal_byte_reads_still_work() {
    let data: Vec<u8> = vec![0x01, 0x02, 0x03, 0x04, 0x05];
    let cursor = Cursor::new(data);

    let mut reader = audex::util::BitReader::new(cursor).unwrap();

    let result = reader.bytes(3).unwrap();
    assert_eq!(result, vec![0x01, 0x02, 0x03]);
}

/// Confirm that reading exactly the available amount succeeds.
#[test]
fn exact_byte_count_succeeds() {
    let data: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let cursor = Cursor::new(data);

    let mut reader = audex::util::BitReader::new(cursor).unwrap();

    let result = reader.bytes(4).unwrap();
    assert_eq!(result, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

/// Confirm that reading more bytes than available fails, even for small counts.
#[test]
fn overread_small_count_fails() {
    let data: Vec<u8> = vec![0xFF, 0xFE];
    let cursor = Cursor::new(data);

    let mut reader = audex::util::BitReader::new(cursor).unwrap();

    let result = reader.bytes(5);
    assert!(
        result.is_err(),
        "Reading 5 bytes from a 2-byte stream should fail"
    );
}
