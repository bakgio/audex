//! Tests for MPEG header parsing edge cases and frame size calculations.
//!
//! Exercises the validation logic in `parse_mpeg_header` and
//! `calculate_frame_size` with crafted byte patterns that hit
//! reserved/invalid fields defined in the MPEG audio specification.

use audex::mp3::util::{calculate_frame_size, parse_mpeg_header, skip_id3};
use audex::mp3::{MPEGLayer, MPEGVersion};
use std::io::Cursor;

/// Build a valid MPEG-1 Layer III header as a starting point.
/// Bytes: [0xFF, 0xFB, 0x90, 0x00]
///   sync=0x7FF, version=MPEG1(0b11), layer=LayerIII(0b01),
///   bitrate_idx=9 (128 kbps), samplerate_idx=0 (44100 Hz)
fn valid_mpeg1_layer3_header() -> [u8; 4] {
    [0xFF, 0xFB, 0x90, 0x00]
}

// ---------------------------------------------------------------------------
// parse_mpeg_header: reserved and invalid field rejection
// ---------------------------------------------------------------------------

#[test]
fn reserved_mpeg_version_rejected() {
    // Version bits 0b01 are reserved in the MPEG spec.
    // Byte layout: 0xFF 0xE9 keeps sync intact but sets version to 0b01.
    let mut header = valid_mpeg1_layer3_header();
    // Clear version bits and set to 0b01: byte[1] = (byte[1] & 0b11100111) | (0b01 << 3)
    header[1] = (header[1] & 0xE7) | (0x01 << 3);
    assert!(
        parse_mpeg_header(&header).is_err(),
        "Version bits 0b01 are reserved and must be rejected"
    );
}

#[test]
fn reserved_layer_rejected() {
    // Layer bits 0b00 are reserved in the MPEG spec.
    let mut header = valid_mpeg1_layer3_header();
    // Clear layer bits: byte[1] = byte[1] & 0b11111001
    header[1] &= 0xF9;
    assert!(
        parse_mpeg_header(&header).is_err(),
        "Layer bits 0b00 are reserved and must be rejected"
    );
}

#[test]
fn reserved_sample_rate_index_rejected() {
    // Sample rate index 0x03 is reserved in the MPEG spec.
    let mut header = valid_mpeg1_layer3_header();
    // Set sample rate index to 0b11: byte[2] = (byte[2] & 0b11110011) | (0b11 << 2)
    header[2] = (header[2] & 0xF3) | (0x03 << 2);
    assert!(
        parse_mpeg_header(&header).is_err(),
        "Sample rate index 0x03 is reserved and must be rejected"
    );
}

#[test]
fn invalid_bitrate_index_0xf_rejected() {
    // Bitrate index 15 (0x0F) is defined as "bad" in the MPEG spec.
    let mut header = valid_mpeg1_layer3_header();
    // Set bitrate index to 0x0F: byte[2] = (byte[2] & 0x0F) | (0x0F << 4)
    header[2] = (header[2] & 0x0F) | 0xF0;
    assert!(
        parse_mpeg_header(&header).is_err(),
        "Bitrate index 0x0F is invalid and must be rejected"
    );
}

#[test]
fn zero_bitrate_index_rejected() {
    // Bitrate index 0 means "free format" which is not supported.
    let mut header = valid_mpeg1_layer3_header();
    // Clear bitrate index: byte[2] = byte[2] & 0x0F
    header[2] &= 0x0F;
    assert!(
        parse_mpeg_header(&header).is_err(),
        "Bitrate index 0 (free format) must be rejected"
    );
}

#[test]
fn valid_header_parses_successfully() {
    // Sanity check: the baseline header must parse without error.
    let header = valid_mpeg1_layer3_header();
    let result = parse_mpeg_header(&header);
    assert!(
        result.is_ok(),
        "Valid MPEG-1 Layer III header must parse: {:?}",
        result.err()
    );
}

#[test]
fn header_too_short_rejected() {
    assert!(parse_mpeg_header(&[0xFF, 0xFB, 0x90]).is_err());
    assert!(parse_mpeg_header(&[0xFF]).is_err());
    assert!(parse_mpeg_header(&[]).is_err());
}

// ---------------------------------------------------------------------------
// calculate_frame_size: edge cases
// ---------------------------------------------------------------------------

#[test]
fn frame_size_zero_sample_rate_returns_minimum() {
    // Division by zero must be avoided; the function returns a safe minimum.
    let size = calculate_frame_size(
        MPEGVersion::MPEG1,
        MPEGLayer::Layer3,
        128_000,
        0, // zero sample rate
        false,
    );
    assert_eq!(size, 24, "Zero sample rate must return MIN_FRAME_SIZE (24)");
}

#[test]
fn frame_size_layer1_padding_uses_4byte_slots() {
    // Layer I uses 32-bit (4-byte) padding slots, not the 1-byte slots
    // used by Layer II/III.
    let without_padding = calculate_frame_size(
        MPEGVersion::MPEG1,
        MPEGLayer::Layer1,
        384_000, // 384 kbps
        44_100,
        false,
    );
    let with_padding =
        calculate_frame_size(MPEGVersion::MPEG1, MPEGLayer::Layer1, 384_000, 44_100, true);
    assert_eq!(
        with_padding - without_padding,
        4,
        "Layer I padding must add exactly 4 bytes"
    );
}

#[test]
fn frame_size_layer3_padding_uses_1byte_slots() {
    let without_padding = calculate_frame_size(
        MPEGVersion::MPEG1,
        MPEGLayer::Layer3,
        128_000,
        44_100,
        false,
    );
    let with_padding =
        calculate_frame_size(MPEGVersion::MPEG1, MPEGLayer::Layer3, 128_000, 44_100, true);
    assert_eq!(
        with_padding - without_padding,
        1,
        "Layer III padding must add exactly 1 byte"
    );
}

// ---------------------------------------------------------------------------
// skip_id3: chained valid headers and iteration limit
// ---------------------------------------------------------------------------

/// Encode a u32 as a 4-byte synchsafe integer (7 usable bits per byte).
fn encode_synchsafe(value: u32) -> [u8; 4] {
    [
        ((value >> 21) & 0x7F) as u8,
        ((value >> 14) & 0x7F) as u8,
        ((value >> 7) & 0x7F) as u8,
        (value & 0x7F) as u8,
    ]
}

/// Build a minimal ID3v2.4 header (10 bytes) with the given body size.
fn build_id3v2_header(body_size: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(10);
    h.extend_from_slice(b"ID3");
    h.push(4); // version 2.4
    h.push(0); // revision
    h.push(0); // flags
    h.extend_from_slice(&encode_synchsafe(body_size));
    h
}

#[test]
fn skip_id3_multiple_valid_consecutive_tags() {
    // Windows Media Player and some tools write multiple ID3v2 headers.
    // All must be skipped so the parser reaches the audio frames.
    let body_size: u32 = 16;
    let mut data = Vec::new();

    // Three consecutive valid ID3v2 headers, each with 16 bytes of body
    for _ in 0..3 {
        data.extend(build_id3v2_header(body_size));
        data.extend(vec![0u8; body_size as usize]);
    }
    // Sentinel bytes representing audio data after the tags
    data.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);

    let expected_pos = 3 * (10 + body_size as u64);
    let mut cursor = Cursor::new(data);
    let pos = skip_id3(&mut cursor).expect("skip_id3 must handle chained valid tags");
    assert_eq!(
        pos, expected_pos,
        "Cursor must be positioned after all three ID3 headers"
    );
}

#[test]
fn skip_id3_max_iterations_enforced() {
    // Craft a buffer with more than 1000 consecutive minimal ID3 headers.
    // The function must stop iterating at the limit, not loop indefinitely.
    let body_size: u32 = 4;
    let header = build_id3v2_header(body_size);
    let single_tag_len = 10 + body_size as usize;

    let count = 1002;
    let mut data = Vec::with_capacity(count * single_tag_len + 4);
    for _ in 0..count {
        data.extend(&header);
        data.extend(vec![0u8; body_size as usize]);
    }
    data.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);

    let mut cursor = Cursor::new(data);
    let result = skip_id3(&mut cursor);

    // The function must complete (not hang) and return a position
    // that is at most 1000 tags deep.
    let pos = result.expect("skip_id3 must not fail on many consecutive tags");
    let max_expected = (1000 * single_tag_len) as u64;
    assert!(
        pos <= max_expected,
        "skip_id3 must stop at the iteration limit; pos={pos}, max={max_expected}"
    );
}
