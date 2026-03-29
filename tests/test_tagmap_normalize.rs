//! Tests for value normalization functions.

mod common;

use audex::tagmap::normalize::*;

// ---------------------------------------------------------------------------
// Track/disc number splitting
// ---------------------------------------------------------------------------

#[test]
fn test_split_track_number_with_total() {
    let (num, total) = normalize_track_disc("5/12");
    assert_eq!(num.as_deref(), Some("5"));
    assert_eq!(total.as_deref(), Some("12"));
}

#[test]
fn test_split_track_number_without_total() {
    let (num, total) = normalize_track_disc("7");
    assert_eq!(num.as_deref(), Some("7"));
    assert_eq!(total, None);
}

#[test]
fn test_split_track_number_padded() {
    // Leading zeros are stripped for cross-format compatibility
    let (num, total) = normalize_track_disc("03/15");
    assert_eq!(num.as_deref(), Some("3"));
    assert_eq!(total.as_deref(), Some("15"));
}

#[test]
fn test_split_track_number_empty() {
    let (num, total) = normalize_track_disc("");
    assert_eq!(num, None);
    assert_eq!(total, None);
}

#[test]
fn test_split_track_number_whitespace() {
    let (num, total) = normalize_track_disc("  3 / 10  ");
    assert_eq!(num.as_deref(), Some("3"));
    assert_eq!(total.as_deref(), Some("10"));
}

#[test]
fn test_split_track_only_total() {
    // Edge case: "/12" means no track number, just total
    let (num, total) = normalize_track_disc("/12");
    assert_eq!(num, None);
    assert_eq!(total.as_deref(), Some("12"));
}

// ---------------------------------------------------------------------------
// Combining track/disc numbers
// ---------------------------------------------------------------------------

#[test]
fn test_combine_track_number() {
    assert_eq!(combine_track_disc(Some("5"), Some("12")), "5/12");
    assert_eq!(combine_track_disc(Some("3"), None), "3");
    assert_eq!(combine_track_disc(None, Some("10")), "0/10");
    assert_eq!(combine_track_disc(None, None), "");
}

// ---------------------------------------------------------------------------
// ID3 genre resolution
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_id3_genre_numeric_parens() {
    assert_eq!(resolve_id3_genre("(17)"), "Rock");
    assert_eq!(resolve_id3_genre("(0)"), "Blues");
}

#[test]
fn test_resolve_id3_genre_mixed() {
    // Parenthesized number with text suffix: prefer the text suffix
    assert_eq!(resolve_id3_genre("(17)Rock"), "Rock");
    assert_eq!(resolve_id3_genre("(52)Electronic"), "Electronic");
}

#[test]
fn test_resolve_id3_genre_bare_number() {
    assert_eq!(resolve_id3_genre("17"), "Rock");
    assert_eq!(resolve_id3_genre("52"), "Electronic");
}

#[test]
fn test_resolve_id3_genre_text_passthrough() {
    assert_eq!(resolve_id3_genre("Electronic"), "Electronic");
    assert_eq!(resolve_id3_genre("Synthwave"), "Synthwave");
}

#[test]
fn test_resolve_id3_genre_empty() {
    assert_eq!(resolve_id3_genre(""), "");
}

#[test]
fn test_resolve_id3_genre_unknown_numeric() {
    // Out-of-range numeric ID: returned as-is (no genre table entry)
    let result = resolve_id3_genre("255");
    assert_eq!(result, "255");
}

// ---------------------------------------------------------------------------
// Date normalization
// ---------------------------------------------------------------------------

#[test]
fn test_normalize_date_passthrough() {
    // ISO 8601 dates should pass through unchanged
    assert_eq!(normalize_date("2024-03-15", TagSystem::ID3v2), "2024-03-15");
    assert_eq!(normalize_date("2024", TagSystem::VorbisComment), "2024");
}

#[test]
fn test_normalize_date_empty() {
    assert_eq!(normalize_date("", TagSystem::MP4), "");
}

// ---------------------------------------------------------------------------
// Boolean normalization
// ---------------------------------------------------------------------------

#[test]
fn test_normalize_boolean_truthy() {
    assert_eq!(normalize_boolean("1", TagSystem::ID3v2), "1");
    assert_eq!(normalize_boolean("true", TagSystem::MP4), "1");
    assert_eq!(normalize_boolean("yes", TagSystem::VorbisComment), "1");
    assert_eq!(normalize_boolean("TRUE", TagSystem::ASF), "1");
}

#[test]
fn test_normalize_boolean_falsy() {
    assert_eq!(normalize_boolean("0", TagSystem::ID3v2), "0");
    assert_eq!(normalize_boolean("false", TagSystem::MP4), "0");
    assert_eq!(normalize_boolean("no", TagSystem::VorbisComment), "0");
    assert_eq!(normalize_boolean("", TagSystem::APEv2), "0");
}

#[test]
fn test_normalize_boolean_unknown() {
    // Unrecognized values are preserved as-is to avoid silent data loss
    assert_eq!(normalize_boolean("maybe", TagSystem::ID3v2), "maybe");
}
