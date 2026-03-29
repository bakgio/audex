//! Core diff computation tests.
//!
//! These tests verify the tag-diffing engine using in-memory tag collections
//! (no real audio files required).

use audex::diff::{self, DiffOptions};
use audex::tags::{BasicTags, Tags};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a simple items vec from (key, value) pairs (single-value shorthand).
fn items(pairs: &[(&str, &str)]) -> Vec<(String, Vec<String>)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), vec![v.to_string()]))
        .collect()
}

/// Build items with multi-value support.
fn items_multi(pairs: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    pairs
        .iter()
        .map(|(k, vs)| (k.to_string(), vs.iter().map(|v| v.to_string()).collect()))
        .collect()
}

// ---------------------------------------------------------------------------
// Identical / empty edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_diff_identical_tags() {
    let left = items(&[("artist", "Beatles"), ("album", "Abbey Road")]);
    let right = items(&[("artist", "Beatles"), ("album", "Abbey Road")]);
    let d = diff::diff_items(&left, &right);

    assert!(d.is_identical());
    assert_eq!(d.diff_count(), 0);
}

#[test]
fn test_diff_both_empty() {
    let d = diff::diff_items(&[], &[]);
    assert!(d.is_identical());
    assert_eq!(d.diff_count(), 0);
}

// ---------------------------------------------------------------------------
// Completely different / no overlap
// ---------------------------------------------------------------------------

#[test]
fn test_diff_completely_different() {
    let left = items(&[("artist", "Beatles")]);
    let right = items(&[("genre", "Rock")]);
    let d = diff::diff_items(&left, &right);

    assert!(!d.is_identical());
    assert_eq!(d.changed.len(), 0);
    assert_eq!(d.left_only.len(), 1);
    assert_eq!(d.right_only.len(), 1);
    assert_eq!(d.left_only[0].key, "artist");
    assert_eq!(d.right_only[0].key, "genre");
}

// ---------------------------------------------------------------------------
// Single and multiple changes
// ---------------------------------------------------------------------------

#[test]
fn test_diff_single_field_changed() {
    let left = items(&[("artist", "Beatles"), ("album", "Abbey Road")]);
    let right = items(&[("artist", "Rolling Stones"), ("album", "Abbey Road")]);
    let d = diff::diff_items(&left, &right);

    assert_eq!(d.changed.len(), 1);
    assert_eq!(d.changed[0].key, "artist");
    assert_eq!(d.changed[0].left, vec!["Beatles"]);
    assert_eq!(d.changed[0].right, vec!["Rolling Stones"]);
}

#[test]
fn test_diff_multiple_changes() {
    let left = items(&[("artist", "A"), ("album", "B"), ("year", "2020")]);
    let right = items(&[("artist", "X"), ("album", "Y"), ("year", "2021")]);
    let d = diff::diff_items(&left, &right);

    assert_eq!(d.changed.len(), 3);
}

// ---------------------------------------------------------------------------
// Left-only and right-only
// ---------------------------------------------------------------------------

#[test]
fn test_diff_left_only_fields() {
    let left = items(&[("artist", "A"), ("comment", "nice")]);
    let right = items(&[("artist", "A")]);
    let d = diff::diff_items(&left, &right);

    assert_eq!(d.left_only.len(), 1);
    assert_eq!(d.left_only[0].key, "comment");
    assert_eq!(d.right_only.len(), 0);
}

#[test]
fn test_diff_right_only_fields() {
    let left = items(&[("artist", "A")]);
    let right = items(&[("artist", "A"), ("encoder", "LAME")]);
    let d = diff::diff_items(&left, &right);

    assert_eq!(d.right_only.len(), 1);
    assert_eq!(d.right_only[0].key, "encoder");
    assert_eq!(d.left_only.len(), 0);
}

// ---------------------------------------------------------------------------
// Mixed scenario
// ---------------------------------------------------------------------------

#[test]
fn test_diff_mixed_changes() {
    let left = items(&[("artist", "A"), ("album", "Same"), ("comment", "old")]);
    let right = items(&[("artist", "B"), ("album", "Same"), ("encoder", "LAME")]);

    let opts = DiffOptions {
        include_unchanged: true,
        ..Default::default()
    };
    let d = diff::diff_items_with_options(&left, &right, &opts);

    assert_eq!(d.changed.len(), 1, "artist changed");
    assert_eq!(d.left_only.len(), 1, "comment is left-only");
    assert_eq!(d.right_only.len(), 1, "encoder is right-only");
    assert_eq!(d.unchanged.len(), 1, "album is unchanged");
}

// ---------------------------------------------------------------------------
// Multi-value fields
// ---------------------------------------------------------------------------

#[test]
fn test_diff_multivalue_field_changed() {
    let left = items_multi(&[("artist", &["A", "B"])]);
    let right = items_multi(&[("artist", &["A", "C"])]);
    let d = diff::diff_items(&left, &right);

    assert_eq!(d.changed.len(), 1);
    assert_eq!(d.changed[0].key, "artist");
}

#[test]
fn test_diff_multivalue_field_order_matters() {
    // Order of values matters — ["A","B"] != ["B","A"]
    let left = items_multi(&[("artist", &["A", "B"])]);
    let right = items_multi(&[("artist", &["B", "A"])]);
    let d = diff::diff_items(&left, &right);

    assert_eq!(d.changed.len(), 1, "different order counts as a change");
}

// ---------------------------------------------------------------------------
// Empty vs populated
// ---------------------------------------------------------------------------

#[test]
fn test_diff_empty_vs_populated() {
    let left: Vec<(String, Vec<String>)> = vec![];
    let right = items(&[("artist", "A"), ("album", "B")]);
    let d = diff::diff_items(&left, &right);

    assert_eq!(d.right_only.len(), 2);
    assert_eq!(d.left_only.len(), 0);
    assert_eq!(d.changed.len(), 0);
}

// ---------------------------------------------------------------------------
// Unicode
// ---------------------------------------------------------------------------

#[test]
fn test_diff_unicode_values() {
    let left = items(&[("artist", "ビートルズ"), ("title", "🎵 Song")]);
    let right = items(&[("artist", "ビートルズ"), ("title", "🎶 Song")]);
    let d = diff::diff_items(&left, &right);

    assert_eq!(d.changed.len(), 1);
    assert_eq!(d.changed[0].key, "title");
}

// ---------------------------------------------------------------------------
// Case sensitivity options
// ---------------------------------------------------------------------------

#[test]
fn test_diff_case_sensitive_keys() {
    // Default behaviour: "Artist" and "artist" are distinct keys
    let left = items(&[("Artist", "A")]);
    let right = items(&[("artist", "A")]);
    let d = diff::diff_items(&left, &right);

    assert_eq!(d.left_only.len(), 1);
    assert_eq!(d.right_only.len(), 1);
    assert_eq!(d.changed.len(), 0);
}

#[test]
fn test_diff_case_insensitive_option() {
    let left = items(&[("Artist", "A")]);
    let right = items(&[("artist", "A")]);
    let opts = DiffOptions {
        case_insensitive_keys: true,
        ..Default::default()
    };
    let d = diff::diff_items_with_options(&left, &right, &opts);

    // Both normalise to "artist" with identical values — should be identical
    assert!(d.is_identical());
}

// ---------------------------------------------------------------------------
// Trim values option
// ---------------------------------------------------------------------------

#[test]
fn test_diff_trim_values_option() {
    let left = items(&[("artist", "  Beatles  ")]);
    let right = items(&[("artist", "Beatles")]);
    let opts = DiffOptions {
        trim_values: true,
        ..Default::default()
    };
    let d = diff::diff_items_with_options(&left, &right, &opts);

    assert!(d.is_identical());
}

// ---------------------------------------------------------------------------
// Key filters (include / exclude)
// ---------------------------------------------------------------------------

#[test]
fn test_diff_include_keys_filter() {
    let left = items(&[("artist", "A"), ("album", "X"), ("year", "2020")]);
    let right = items(&[("artist", "B"), ("album", "Y"), ("year", "2021")]);

    let mut include = HashSet::new();
    include.insert("artist".to_string());
    let opts = DiffOptions {
        include_keys: Some(include),
        ..Default::default()
    };
    let d = diff::diff_items_with_options(&left, &right, &opts);

    // Only "artist" is compared
    assert_eq!(d.changed.len(), 1);
    assert_eq!(d.changed[0].key, "artist");
}

#[test]
fn test_diff_exclude_keys_filter() {
    let left = items(&[("artist", "A"), ("comment", "X")]);
    let right = items(&[("artist", "B"), ("comment", "Y")]);

    let mut exclude = HashSet::new();
    exclude.insert("comment".to_string());
    let opts = DiffOptions {
        exclude_keys: exclude,
        ..Default::default()
    };
    let d = diff::diff_items_with_options(&left, &right, &opts);

    assert_eq!(d.changed.len(), 1);
    assert_eq!(d.changed[0].key, "artist");
}

// ---------------------------------------------------------------------------
// Query methods
// ---------------------------------------------------------------------------

#[test]
fn test_diff_count() {
    let left = items(&[("a", "1"), ("b", "2"), ("c", "3")]);
    let right = items(&[("a", "X"), ("d", "4")]);
    let d = diff::diff_items(&left, &right);

    // a changed, b+c left_only, d right_only → 1 + 2 + 1 = 4
    assert_eq!(
        d.diff_count(),
        d.changed.len() + d.left_only.len() + d.right_only.len()
    );
    assert_eq!(d.diff_count(), 4);
}

#[test]
fn test_differing_keys() {
    let left = items(&[("b", "1"), ("a", "2")]);
    let right = items(&[("a", "X"), ("c", "3")]);
    let d = diff::diff_items(&left, &right);

    let keys = d.differing_keys();
    // a (changed), b (left-only), c (right-only) — sorted
    assert_eq!(keys, vec!["a", "b", "c"]);
}

#[test]
fn test_get_change() {
    let left = items(&[("artist", "A"), ("album", "B")]);
    let right = items(&[("artist", "X"), ("album", "B")]);
    let d = diff::diff_items(&left, &right);

    let change = d.get_change("artist").expect("should find artist change");
    assert_eq!(change.left, vec!["A"]);
    assert_eq!(change.right, vec!["X"]);
    assert!(d.get_change("album").is_none());
}

#[test]
fn test_filter_keys() {
    let left = items(&[("a", "1"), ("b", "2"), ("c", "3")]);
    let right = items(&[("a", "X"), ("b", "Y"), ("d", "4")]);
    let d = diff::diff_items(&left, &right);

    let filtered = d.filter_keys(&["a"]);
    assert_eq!(filtered.changed.len(), 1);
    assert_eq!(filtered.changed[0].key, "a");
    assert_eq!(filtered.left_only.len(), 0);
    assert_eq!(filtered.right_only.len(), 0);
}

#[test]
fn test_exclude_keys_method() {
    let left = items(&[("a", "1"), ("b", "2")]);
    let right = items(&[("a", "X"), ("b", "Y")]);
    let d = diff::diff_items(&left, &right);

    let filtered = d.exclude_keys(&["b"]);
    assert_eq!(filtered.changed.len(), 1);
    assert_eq!(filtered.changed[0].key, "a");
}

// ---------------------------------------------------------------------------
// diff_items direct usage
// ---------------------------------------------------------------------------

#[test]
fn test_diff_items_direct() {
    let left = vec![
        ("title".to_string(), vec!["Song A".to_string()]),
        ("artist".to_string(), vec!["Band".to_string()]),
    ];
    let right = vec![
        ("title".to_string(), vec!["Song B".to_string()]),
        ("artist".to_string(), vec!["Band".to_string()]),
    ];

    let d = diff::diff_items(&left, &right);
    assert_eq!(d.changed.len(), 1);
    assert_eq!(d.changed[0].key, "title");
}

// ---------------------------------------------------------------------------
// Snapshot-based diffing (using BasicTags)
// ---------------------------------------------------------------------------

#[test]
fn test_snapshot_and_diff() {
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["Old".to_string()]);
    tags.set("album", vec!["Same".to_string()]);

    // Capture the "before" state
    let snapshot = tags.items();

    // Modify
    tags.set("artist", vec!["New".to_string()]);
    let current = tags.items();

    let d = diff::diff_items(&snapshot, &current);
    assert_eq!(d.changed.len(), 1);
    assert_eq!(d.changed[0].key, "artist");
    assert_eq!(d.changed[0].left, vec!["Old"]);
    assert_eq!(d.changed[0].right, vec!["New"]);
}
