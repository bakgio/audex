//! Tests for ID3 frame parsing edge cases.
//!
//! Covers genre text resolution, numeric "n/total" frame parsing,
//! paired text frames (TIPL/TMCL), and junk frame recovery.

use audex::id3::frames::FrameData;
use audex::id3::util::{JunkFrameRecovery, Unsynch};

// ---------------------------------------------------------------------------
// parse_genre_text: ID3v1 parenthesized notation and fallbacks
// ---------------------------------------------------------------------------

#[test]
fn genre_single_parenthesized_id() {
    // "(17)" should resolve to genre 17 = "Rock"
    let result = FrameData::parse_genre_text("(17)");
    assert_eq!(result, vec!["Rock"]);
}

#[test]
fn genre_multiple_parenthesized_ids() {
    // "(17)(18)" should resolve both IDs
    let result = FrameData::parse_genre_text("(17)(18)");
    assert_eq!(result, vec!["Rock", "Techno"]);
}

#[test]
fn genre_parenthesized_with_trailing_text() {
    // "(17)Goa" -> Rock + Goa
    let result = FrameData::parse_genre_text("(17)Goa");
    assert_eq!(result, vec!["Rock", "Goa"]);
}

#[test]
fn genre_out_of_range_id_becomes_unknown() {
    // "(999)" references a nonexistent ID3v1 genre
    let result = FrameData::parse_genre_text("(999)");
    assert_eq!(result, vec!["Unknown"]);
}

#[test]
fn genre_bare_numeric_resolves() {
    // "17" without parentheses should still resolve to "Rock"
    let result = FrameData::parse_genre_text("17");
    assert_eq!(result, vec!["Rock"]);
}

#[test]
fn genre_bare_numeric_out_of_range() {
    let result = FrameData::parse_genre_text("999");
    assert_eq!(result, vec!["Unknown"]);
}

#[test]
fn genre_literal_text_passthrough() {
    let result = FrameData::parse_genre_text("Post-Punk");
    assert_eq!(result, vec!["Post-Punk"]);
}

#[test]
fn genre_special_cr_and_rx() {
    assert_eq!(FrameData::parse_genre_text("CR"), vec!["Cover"]);
    assert_eq!(FrameData::parse_genre_text("RX"), vec!["Remix"]);
}

#[test]
fn genre_parenthesized_cr_rx() {
    let result = FrameData::parse_genre_text("(CR)(RX)");
    assert_eq!(result, vec!["Cover", "Remix"]);
}

#[test]
fn genre_null_separated_multiple() {
    // Null-separated genres (ID3v2.4 convention)
    let result = FrameData::parse_genre_text("Rock\0Electronic");
    assert_eq!(result, vec!["Rock", "Electronic"]);
}

#[test]
fn genre_empty_string() {
    let result = FrameData::parse_genre_text("");
    assert!(result.is_empty());
}

// ---------------------------------------------------------------------------
// parse_numeric_part_text_frame: TRCK/TPOS "n/total" format
// ---------------------------------------------------------------------------

#[test]
fn numeric_part_standard_track_number() {
    // "4/15" -> text=["4/15"], parsed value=4
    let data = build_latin1_text_data("4/15");
    let result = FrameData::parse_numeric_part_text_frame("TRCK", &data).unwrap();
    match result {
        FrameData::NumericPartText { text, value, .. } => {
            assert_eq!(text, vec!["4/15"]);
            assert_eq!(value, Some(4));
        }
        other => panic!("Expected NumericPartText, got {:?}", other),
    }
}

#[test]
fn numeric_part_only_total_no_track() {
    // "/15" -> the part before "/" is empty, parse fails -> value=None
    let data = build_latin1_text_data("/15");
    let result = FrameData::parse_numeric_part_text_frame("TRCK", &data).unwrap();
    match result {
        FrameData::NumericPartText { text, value, .. } => {
            assert_eq!(text, vec!["/15"]);
            assert_eq!(value, None, "Empty string before '/' cannot parse as u64");
        }
        other => panic!("Expected NumericPartText, got {:?}", other),
    }
}

#[test]
fn numeric_part_plain_number() {
    // "7" with no slash -> value=7
    let data = build_latin1_text_data("7");
    let result = FrameData::parse_numeric_part_text_frame("TPOS", &data).unwrap();
    match result {
        FrameData::NumericPartText { value, .. } => {
            assert_eq!(value, Some(7));
        }
        other => panic!("Expected NumericPartText, got {:?}", other),
    }
}

#[test]
fn numeric_part_non_numeric_text() {
    // "foo/bar" -> cannot parse "foo" as u64 -> value=None
    let data = build_latin1_text_data("foo/bar");
    let result = FrameData::parse_numeric_part_text_frame("TRCK", &data).unwrap();
    match result {
        FrameData::NumericPartText { value, .. } => {
            assert_eq!(value, None);
        }
        other => panic!("Expected NumericPartText, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// parse_paired_text_frame: TIPL/TMCL handling of odd fields
// ---------------------------------------------------------------------------

#[test]
fn paired_text_valid_pairs() {
    // Two complete pairs: "producer\0John\0mixer\0Jane"
    let data = build_latin1_paired_data(&["producer", "John", "mixer", "Jane"]);
    let result = FrameData::parse_paired_text_frame("TIPL", &data).unwrap();
    match result {
        FrameData::PairedText { people, .. } => {
            assert_eq!(people.len(), 2);
            assert_eq!(people[0], ("producer".to_string(), "John".to_string()));
            assert_eq!(people[1], ("mixer".to_string(), "Jane".to_string()));
        }
        other => panic!("Expected PairedText, got {:?}", other),
    }
}

#[test]
fn paired_text_odd_field_count_drops_unpaired() {
    // Three fields = one complete pair + one orphan. The orphan is silently dropped.
    let data = build_latin1_paired_data(&["producer", "John", "orphan"]);
    let result = FrameData::parse_paired_text_frame("TIPL", &data).unwrap();
    match result {
        FrameData::PairedText { people, .. } => {
            assert_eq!(people.len(), 1, "Odd trailing field must be dropped");
            assert_eq!(people[0], ("producer".to_string(), "John".to_string()));
        }
        other => panic!("Expected PairedText, got {:?}", other),
    }
}

#[test]
fn paired_text_empty_data() {
    let result = FrameData::parse_paired_text_frame("TIPL", &[]).unwrap();
    match result {
        FrameData::PairedText { people, .. } => {
            assert!(people.is_empty());
        }
        other => panic!("Expected PairedText, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// JunkFrameRecovery: scanning for next valid frame header
// ---------------------------------------------------------------------------

#[test]
fn junk_recovery_finds_valid_frame_after_garbage() {
    // Build a buffer: 20 bytes of garbage, then a valid ID3v2.4 TIT2 header
    let mut data = vec![0xAA; 20];
    // Valid frame header: "TIT2" + 4-byte size + 2-byte flags
    data.extend_from_slice(b"TIT2");
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x0A]); // size = 10
    data.extend_from_slice(&[0x00, 0x00]); // flags
    data.extend_from_slice(&[0x00; 10]); // payload

    let result = JunkFrameRecovery::recover_from_junk(&data, 0, (2, 4));
    assert_eq!(result, Some(20), "Must find valid frame at offset 20");
}

#[test]
fn junk_recovery_returns_none_on_pure_garbage() {
    let data = vec![0xAA; 100];
    let result = JunkFrameRecovery::recover_from_junk(&data, 0, (2, 4));
    assert_eq!(result, None, "Must return None when no valid frame exists");
}

#[test]
fn junk_recovery_rejects_unsupported_version() {
    let result = JunkFrameRecovery::recover_from_junk(&[0; 20], 0, (2, 5));
    assert_eq!(result, None, "Unsupported version must return None");
}

// ---------------------------------------------------------------------------
// Unsynch: encode/decode roundtrip edge cases
// ---------------------------------------------------------------------------

#[test]
fn unsynch_roundtrip_consecutive_ff() {
    // Multiple consecutive 0xFF bytes
    let original = vec![0xFF, 0xFF, 0xFF, 0xE0];
    let encoded = Unsynch::encode(&original);
    let decoded = Unsynch::decode(&encoded).unwrap();
    assert_eq!(decoded, original, "Consecutive 0xFF bytes must roundtrip");
}

#[test]
fn unsynch_roundtrip_ff_at_end() {
    // 0xFF as the last byte (no following byte to protect)
    let original = vec![0x01, 0x02, 0xFF];
    let encoded = Unsynch::encode(&original);
    let decoded = Unsynch::decode(&encoded).unwrap();
    assert_eq!(decoded, original);
}

#[test]
fn unsynch_empty_data() {
    assert_eq!(Unsynch::encode(&[]), Vec::<u8>::new());
    assert_eq!(Unsynch::decode(&[]).unwrap(), Vec::<u8>::new());
}

#[test]
fn unsynch_no_ff_passthrough() {
    // Data without 0xFF should pass through unchanged
    let data = vec![0x01, 0x02, 0x03, 0xFE];
    assert!(!Unsynch::needs_encode(&data));
    assert_eq!(Unsynch::encode(&data), data);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a Latin-1 encoded text frame payload: [0x00 (encoding), ...text_bytes]
fn build_latin1_text_data(text: &str) -> Vec<u8> {
    let mut data = vec![0x00]; // Latin-1 encoding
    data.extend_from_slice(text.as_bytes());
    data
}

/// Build a Latin-1 paired text payload: [0x00 (encoding), field1\0field2\0...]
fn build_latin1_paired_data(fields: &[&str]) -> Vec<u8> {
    let mut data = vec![0x00]; // Latin-1 encoding
    for (i, field) in fields.iter().enumerate() {
        data.extend_from_slice(field.as_bytes());
        if i < fields.len() - 1 {
            data.push(0x00); // null separator
        }
    }
    data
}
