#![cfg(feature = "serde")]

//! Boundary-value tests for TagSnapshot deserialization limits.
//!
//! Verifies that values exactly at the limit are accepted and values
//! one past the limit are rejected.

use audex::snapshot::TagSnapshot;

// ---------------------------------------------------------------------------
// Tag entry count: limit is 10,000
// ---------------------------------------------------------------------------

#[test]
fn tag_entries_at_limit_accepted() {
    // 10,000 entries is the maximum allowed
    let mut tags = serde_json::Map::new();
    for i in 0..10_000 {
        tags.insert(format!("KEY{:05}", i), serde_json::json!(["value"]));
    }

    let json = serde_json::json!({
        "format": "TEST",
        "stream_info": {},
        "tags": tags
    })
    .to_string();

    assert!(
        TagSnapshot::from_json_str(&json).is_ok(),
        "10,000 tag entries must be accepted"
    );
}

#[test]
fn tag_entries_over_limit_rejected() {
    // 10,001 entries exceeds the limit
    let mut tags = serde_json::Map::new();
    for i in 0..10_001 {
        tags.insert(format!("KEY{:05}", i), serde_json::json!(["v"]));
    }

    let json = serde_json::json!({
        "format": "TEST",
        "stream_info": {},
        "tags": tags
    })
    .to_string();

    let err = TagSnapshot::from_json_str(&json).unwrap_err();
    assert!(
        err.to_string().contains("entry") || err.to_string().contains("limit"),
        "Expected tag entry limit error, got: {}",
        err
    );
}

// ---------------------------------------------------------------------------
// raw_tags node count: limit is 100,000
// ---------------------------------------------------------------------------

#[test]
fn raw_tags_nodes_at_limit_accepted() {
    // The array container itself counts as 1 node, plus each element.
    // 99,999 elements + 1 array = exactly 100,000 nodes (the limit).
    let elements: Vec<String> = (0..99_999).map(|i| i.to_string()).collect();
    let array_json = format!("[{}]", elements.join(","));
    let json = format!(
        r#"{{"format":"TEST","stream_info":{{}},"tags":{{}},"raw_tags":{}}}"#,
        array_json
    );

    assert!(
        TagSnapshot::from_json_str(&json).is_ok(),
        "Exactly 100,000 raw_tags nodes must be accepted"
    );
}
