//! Additional normalization edge cases not covered by the main suite.

use audex::tagmap::normalize::*;

// ---------------------------------------------------------------------------
// Multi-parenthesized ID3 genre resolution
// ---------------------------------------------------------------------------

#[test]
fn resolve_id3_genre_multiple_parenthesized() {
    // "(17)(18)" = Rock + Techno, joined with "/"
    let result = resolve_id3_genre("(17)(18)");
    assert_eq!(result, "Rock/Techno");
}

#[test]
fn resolve_id3_genre_three_parenthesized() {
    // "(0)(17)(52)" = Blues + Rock + Electronic
    let result = resolve_id3_genre("(0)(17)(52)");
    assert_eq!(result, "Blues/Rock/Electronic");
}

#[test]
fn resolve_id3_genre_parenthesized_with_suffix_replaces_last() {
    // "(17)(18)Custom" -> the trailing text replaces the last resolved genre
    let result = resolve_id3_genre("(17)(18)Custom");
    assert_eq!(result, "Rock/Custom");
}

#[test]
fn resolve_id3_genre_parenthesized_out_of_range() {
    // Genre ID 999 exceeds the u8 range, so it cannot index the standard table.
    // The function falls back to the raw number.
    let result = resolve_id3_genre("(999)");
    assert_eq!(result, "999");
}

#[test]
fn resolve_id3_genre_parenthesized_out_of_range_but_valid_u8() {
    // Genre ID 200 fits in u8 but is beyond the 192-entry genre table.
    let result = resolve_id3_genre("(200)");
    assert_eq!(result, "200");
}
