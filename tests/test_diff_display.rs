//! Display and formatting tests for the diff module.

use audex::diff::{FieldChange, FieldEntry, StreamInfoDiff, TagDiff, ValueChange};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a TagDiff from known parts (no stream info).
fn make_diff(
    changed: Vec<FieldChange>,
    left_only: Vec<FieldEntry>,
    right_only: Vec<FieldEntry>,
    unchanged: Vec<FieldEntry>,
) -> TagDiff {
    TagDiff {
        changed,
        left_only,
        right_only,
        unchanged,
        stream_info_diff: None,
    }
}

// ---------------------------------------------------------------------------
// FieldChange display
// ---------------------------------------------------------------------------

#[test]
fn test_display_changed_field() {
    let fc = FieldChange {
        key: "artist".to_string(),
        left: vec!["Old".to_string()],
        right: vec!["New".to_string()],
    };
    let s = format!("{}", fc);
    assert!(s.contains("artist"));
    assert!(s.contains("Old"));
    assert!(s.contains("New"));
    assert!(s.contains("→"));
}

// ---------------------------------------------------------------------------
// TagDiff display — changed only
// ---------------------------------------------------------------------------

#[test]
fn test_display_tag_diff_changed_only() {
    let d = make_diff(
        vec![FieldChange {
            key: "artist".to_string(),
            left: vec!["A".to_string()],
            right: vec!["B".to_string()],
        }],
        vec![],
        vec![],
        vec![],
    );
    let s = format!("{}", d);
    assert!(s.contains("~ "), "changed lines should have ~ prefix");
    assert!(s.contains("artist"));
}

// ---------------------------------------------------------------------------
// TagDiff display — left-only / right-only
// ---------------------------------------------------------------------------

#[test]
fn test_display_tag_diff_left_only() {
    let d = make_diff(
        vec![],
        vec![FieldEntry {
            key: "comment".to_string(),
            values: vec!["gone".to_string()],
        }],
        vec![],
        vec![],
    );
    let s = format!("{}", d);
    assert!(s.contains("- "), "removed lines should have - prefix");
    assert!(s.contains("comment"));
}

#[test]
fn test_display_tag_diff_right_only() {
    let d = make_diff(
        vec![],
        vec![],
        vec![FieldEntry {
            key: "encoder".to_string(),
            values: vec!["LAME".to_string()],
        }],
        vec![],
    );
    let s = format!("{}", d);
    assert!(s.contains("+ "), "added lines should have + prefix");
    assert!(s.contains("encoder"));
}

// ---------------------------------------------------------------------------
// TagDiff display — mixed ordering
// ---------------------------------------------------------------------------

#[test]
fn test_display_tag_diff_mixed() {
    let d = make_diff(
        vec![FieldChange {
            key: "artist".to_string(),
            left: vec!["A".to_string()],
            right: vec!["B".to_string()],
        }],
        vec![FieldEntry {
            key: "comment".to_string(),
            values: vec!["removed".to_string()],
        }],
        vec![FieldEntry {
            key: "encoder".to_string(),
            values: vec!["LAME".to_string()],
        }],
        vec![],
    );
    let s = format!("{}", d);

    // Verify ordering: changes first, then removals, then additions.
    // Skip the "--- left" / "+++ right" header lines by searching from after them.
    let body_start = s.find("+++ right").expect("header") + 10;
    let body = &s[body_start..];
    let tilde_pos = body.find("~ ").expect("should have ~ line");
    let minus_pos = body.find("- ").expect("should have - line");
    let plus_pos = body.find("+ ").expect("should have + line");
    assert!(tilde_pos < minus_pos, "changes before removals");
    assert!(minus_pos < plus_pos, "removals before additions");
}

// ---------------------------------------------------------------------------
// Empty diff display
// ---------------------------------------------------------------------------

#[test]
fn test_display_empty_diff() {
    let d = make_diff(vec![], vec![], vec![], vec![]);
    let s = format!("{}", d);
    assert_eq!(s, "No differences");
}

// ---------------------------------------------------------------------------
// Pretty-print alignment
// ---------------------------------------------------------------------------

#[test]
fn test_pprint_alignment() {
    let d = make_diff(
        vec![
            FieldChange {
                key: "a".to_string(),
                left: vec!["1".to_string()],
                right: vec!["2".to_string()],
            },
            FieldChange {
                key: "longkey".to_string(),
                left: vec!["x".to_string()],
                right: vec!["y".to_string()],
            },
        ],
        vec![],
        vec![],
        vec![],
    );
    let pp = d.pprint();
    // Both key columns should be right-aligned to the same width
    // "a" should be padded, "longkey" should not
    assert!(pp.contains("      a:"), "short key should be right-aligned");
    assert!(pp.contains("longkey:"), "long key should not be padded");
}

// ---------------------------------------------------------------------------
// pprint_full includes unchanged fields
// ---------------------------------------------------------------------------

#[test]
fn test_pprint_full_includes_unchanged() {
    let d = make_diff(
        vec![],
        vec![],
        vec![],
        vec![FieldEntry {
            key: "album".to_string(),
            values: vec!["Same".to_string()],
        }],
    );
    let full = d.pprint_full();
    assert!(full.contains("= "), "unchanged lines should have = prefix");
    assert!(full.contains("album"));
}

// ---------------------------------------------------------------------------
// Summary format
// ---------------------------------------------------------------------------

#[test]
fn test_summary_format() {
    let d = make_diff(
        vec![FieldChange {
            key: "a".to_string(),
            left: vec![],
            right: vec![],
        }],
        vec![
            FieldEntry {
                key: "b".to_string(),
                values: vec![],
            },
            FieldEntry {
                key: "c".to_string(),
                values: vec![],
            },
        ],
        vec![FieldEntry {
            key: "d".to_string(),
            values: vec![],
        }],
        vec![
            FieldEntry {
                key: "e".to_string(),
                values: vec![],
            },
            FieldEntry {
                key: "f".to_string(),
                values: vec![],
            },
            FieldEntry {
                key: "g".to_string(),
                values: vec![],
            },
        ],
    );
    assert_eq!(d.summary(), "1 changed, 2 removed, 1 added, 3 unchanged");
}

#[test]
fn test_summary_zero_counts() {
    let d = make_diff(
        vec![],
        vec![],
        vec![],
        vec![
            FieldEntry {
                key: "a".to_string(),
                values: vec![],
            },
            FieldEntry {
                key: "b".to_string(),
                values: vec![],
            },
            FieldEntry {
                key: "c".to_string(),
                values: vec![],
            },
            FieldEntry {
                key: "d".to_string(),
                values: vec![],
            },
            FieldEntry {
                key: "e".to_string(),
                values: vec![],
            },
        ],
    );
    assert_eq!(d.summary(), "0 changed, 0 removed, 0 added, 5 unchanged");
}

// ---------------------------------------------------------------------------
// StreamInfoDiff display
// ---------------------------------------------------------------------------

#[test]
fn test_stream_info_diff_display() {
    let si = StreamInfoDiff {
        length: Some(ValueChange {
            left: Some(245.3),
            right: Some(245.1),
        }),
        bitrate: Some(ValueChange {
            left: Some(320),
            right: Some(256),
        }),
        sample_rate: None,
        channels: None,
        bits_per_sample: None,
    };
    let s = format!("{}", si);
    assert!(s.contains("length"));
    assert!(s.contains("245.3"));
    assert!(s.contains("245.1"));
    assert!(s.contains("bitrate"));
    assert!(s.contains("320"));
    assert!(s.contains("256"));
    // Fields that are None should not appear
    assert!(!s.contains("sample_rate"));
}
