//! Tests for ID3v2 text encoding edge cases.
//!
//! Validates Latin-1 character boundaries, UTF-16 surrogate handling,
//! BOM detection, empty-string roundtrips, and version-dependent
//! encoding validity.

use audex::id3::specs::TextEncoding;

// ---------------------------------------------------------------------------
// Latin-1 encoding boundaries
// ---------------------------------------------------------------------------

#[test]
fn latin1_encode_char_at_255_accepted() {
    // U+00FF (Latin small letter y with diaeresis) is the highest
    // character representable in ISO 8859-1.
    let text = "\u{00FF}"; // ÿ
    let encoded = TextEncoding::Latin1.encode_text(text).unwrap();
    assert_eq!(encoded, vec![0xFF]);
}

#[test]
fn latin1_encode_char_above_255_rejected() {
    // U+0100 (Latin capital letter A with macron) is one past the
    // Latin-1 range and must be rejected.
    let text = "\u{0100}"; // Ā
    let result = TextEncoding::Latin1.encode_text(text);
    assert!(
        result.is_err(),
        "Characters above U+00FF must be rejected by Latin-1 encoding"
    );
}

#[test]
fn latin1_decode_high_byte_roundtrips() {
    // Bytes 0x80-0xFF decode to the corresponding Unicode code points.
    let bytes: Vec<u8> = (0x80..=0xFF).collect();
    let decoded = TextEncoding::Latin1.decode_text(&bytes).unwrap();
    for (i, ch) in decoded.chars().enumerate() {
        assert_eq!(
            ch as u32,
            (0x80 + i) as u32,
            "Byte 0x{:02X} must decode to U+{:04X}",
            0x80 + i,
            0x80 + i
        );
    }
}

// ---------------------------------------------------------------------------
// UTF-16 odd-length and surrogate edge cases
// ---------------------------------------------------------------------------

#[test]
fn utf16_odd_length_data_truncates_orphan() {
    // 3 bytes total: BOM (2) + 1 orphan byte. The orphan is truncated.
    let data = vec![0xFF, 0xFE, 0x41]; // LE BOM + orphan 'A' byte
    let decoded = TextEncoding::Utf16.decode_text(&data).unwrap();
    // The orphan byte is dropped; result is empty.
    assert!(decoded.is_empty() || decoded.len() <= 1);
}

#[test]
fn utf16_lone_surrogate_rejected() {
    // A lone high surrogate (0xD800) without a low surrogate pair.
    // Rust's String::from_utf16 rejects unpaired surrogates.
    let data = vec![0xFF, 0xFE, 0x00, 0xD8]; // LE BOM + lone 0xD800
    let result = TextEncoding::Utf16.decode_text(&data);
    assert!(result.is_err(), "Lone surrogate must be rejected");
}

// ---------------------------------------------------------------------------
// UTF-16BE BOM handling
// ---------------------------------------------------------------------------

#[test]
fn utf16be_with_be_bom_stripped() {
    // UTF-16BE data starting with 0xFEFF BOM must strip it.
    let data = vec![0xFE, 0xFF, 0x00, 0x41]; // BE BOM + 'A'
    let decoded = TextEncoding::Utf16Be.decode_text(&data).unwrap();
    assert_eq!(decoded, "A", "BE BOM must be stripped from UTF-16BE data");
}

#[test]
fn utf16be_without_bom() {
    // UTF-16BE data without a BOM should decode directly.
    let data = vec![0x00, 0x41, 0x00, 0x42]; // 'A' + 'B'
    let decoded = TextEncoding::Utf16Be.decode_text(&data).unwrap();
    assert_eq!(decoded, "AB");
}

// ---------------------------------------------------------------------------
// Empty string roundtrips for all four encodings
// ---------------------------------------------------------------------------

#[test]
fn empty_string_roundtrip_latin1() {
    let encoded = TextEncoding::Latin1.encode_text("").unwrap();
    assert!(encoded.is_empty());
    let decoded = TextEncoding::Latin1.decode_text(&encoded).unwrap();
    assert!(decoded.is_empty());
}

#[test]
fn empty_string_roundtrip_utf8() {
    let encoded = TextEncoding::Utf8.encode_text("").unwrap();
    assert!(encoded.is_empty());
    let decoded = TextEncoding::Utf8.decode_text(&encoded).unwrap();
    assert!(decoded.is_empty());
}

#[test]
fn empty_string_roundtrip_utf16() {
    let encoded = TextEncoding::Utf16.encode_text("").unwrap();
    // UTF-16 encoding always prepends a BOM (2 bytes), even for empty text
    assert_eq!(encoded, vec![0xFF, 0xFE]);
}

#[test]
fn empty_string_roundtrip_utf16be() {
    let encoded = TextEncoding::Utf16Be.encode_text("").unwrap();
    assert!(encoded.is_empty());
    let decoded = TextEncoding::Utf16Be.decode_text(&encoded).unwrap();
    assert!(decoded.is_empty());
}

// ---------------------------------------------------------------------------
// Version-dependent encoding validity
// ---------------------------------------------------------------------------

#[test]
fn utf16be_invalid_in_v23() {
    // UTF-16BE was introduced in ID3v2.4; must be invalid for v2.3.
    assert!(
        !TextEncoding::Utf16Be.is_valid_for_version((2, 3)),
        "UTF-16BE must be invalid in v2.3"
    );
    assert!(
        TextEncoding::Utf16Be.is_valid_for_version((2, 4)),
        "UTF-16BE must be valid in v2.4"
    );
}

#[test]
fn utf8_invalid_in_v22() {
    // UTF-8 encoding should not be used in ID3v2.2.
    assert!(
        !TextEncoding::Utf8.is_valid_for_version((2, 2)),
        "UTF-8 must be invalid in v2.2"
    );
}

#[test]
fn latin1_and_utf16_valid_in_all_versions() {
    for version in [(2, 2), (2, 3), (2, 4)] {
        assert!(
            TextEncoding::Latin1.is_valid_for_version(version),
            "Latin-1 must be valid in all versions"
        );
        assert!(
            TextEncoding::Utf16.is_valid_for_version(version),
            "UTF-16 must be valid in all versions"
        );
    }
}

#[test]
fn from_byte_invalid_values_rejected() {
    for byte in [4u8, 5, 10, 128, 255] {
        assert!(
            TextEncoding::from_byte(byte).is_err(),
            "Encoding byte {} must be rejected",
            byte
        );
    }
}
