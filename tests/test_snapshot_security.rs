#![cfg(feature = "serde")]

use audex::snapshot::TagSnapshot;

/// Minimal valid JSON for a TagSnapshot (no raw_tags).
fn minimal_snapshot_json() -> String {
    serde_json::json!({
        "format": "FLAC",
        "stream_info": {},
        "tags": {}
    })
    .to_string()
}

// =========================================================================
// from_json_str: valid input
// =========================================================================

#[test]
fn test_from_json_str_valid_input() {
    let json = serde_json::json!({
        "format": "FLAC",
        "stream_info": {
            "length_secs": 120.5,
            "bitrate": 320000,
            "sample_rate": 44100,
            "channels": 2,
            "bits_per_sample": 16
        },
        "tags": {
            "TITLE": ["Test Song"],
            "ARTIST": ["Test Artist"]
        }
    })
    .to_string();

    let snap = TagSnapshot::from_json_str(&json).unwrap();
    assert_eq!(snap.format, "FLAC");
    assert_eq!(snap.tags.get("TITLE").unwrap(), &vec!["Test Song"]);
    assert_eq!(snap.stream_info.sample_rate, Some(44100));
}

#[test]
fn test_from_json_str_empty_tags() {
    let json = minimal_snapshot_json();
    let snap = TagSnapshot::from_json_str(&json).unwrap();
    assert_eq!(snap.format, "FLAC");
    assert!(snap.tags.is_empty());
    assert!(snap.raw_tags.is_none());
}

// =========================================================================
// raw_tags depth limit (MAX_DEPTH = 64)
// =========================================================================

/// Build a JSON object nested to the given depth: {"a":{"a":{..."leaf"...}}}
fn nested_json(depth: usize) -> String {
    let open: String = r#"{"a":"#.repeat(depth);
    let close: String = "}".repeat(depth);
    format!("{}\"leaf\"{}", open, close)
}

#[test]
fn test_deeply_nested_raw_tags_rejected() {
    // 70 levels of nesting exceeds the 64-level limit
    let raw_tags = nested_json(70);
    let json = format!(
        r#"{{"format":"TEST","stream_info":{{}},"tags":{{}},"raw_tags":{}}}"#,
        raw_tags
    );
    let err = TagSnapshot::from_json_str(&json).unwrap_err();
    assert!(
        err.to_string().contains("depth"),
        "expected depth error, got: {}",
        err
    );
}

#[test]
fn test_depth_at_boundary() {
    // Depth 63 should succeed (0-indexed, so 63 levels of nesting = depth < 64)
    let raw_tags_ok = nested_json(63);
    let json_ok = format!(
        r#"{{"format":"TEST","stream_info":{{}},"tags":{{}},"raw_tags":{}}}"#,
        raw_tags_ok
    );
    assert!(
        TagSnapshot::from_json_str(&json_ok).is_ok(),
        "depth 63 should be accepted"
    );

    // Depth 65 should fail
    let raw_tags_over = nested_json(65);
    let json_over = format!(
        r#"{{"format":"TEST","stream_info":{{}},"tags":{{}},"raw_tags":{}}}"#,
        raw_tags_over
    );
    assert!(
        TagSnapshot::from_json_str(&json_over).is_err(),
        "depth 65 should be rejected"
    );
}

// =========================================================================
// raw_tags node count limit (MAX_NODES = 100,000)
// =========================================================================

#[test]
fn test_excessive_node_count_rejected() {
    // Build an array with 100,001 integer nodes
    let elements: Vec<String> = (0..100_001).map(|i| i.to_string()).collect();
    let array_json = format!("[{}]", elements.join(","));
    let json = format!(
        r#"{{"format":"TEST","stream_info":{{}},"tags":{{}},"raw_tags":{}}}"#,
        array_json
    );
    let err = TagSnapshot::from_json_str(&json).unwrap_err();
    assert!(
        err.to_string().contains("node"),
        "expected node limit error, got: {}",
        err
    );
}

// =========================================================================
// raw_tags string byte limit (MAX_STRING_BYTES = 10 MB)
// =========================================================================

#[test]
fn test_excessive_string_bytes_rejected() {
    // A single string just over 10 MB
    let big_string = "x".repeat(10 * 1024 * 1024 + 1);
    let json = format!(
        r#"{{"format":"TEST","stream_info":{{}},"tags":{{}},"raw_tags":"{}"}}"#,
        big_string
    );
    let err = TagSnapshot::from_json_str(&json).unwrap_err();
    assert!(
        err.to_string().contains("byte") || err.to_string().contains("size"),
        "expected string byte limit error, got: {}",
        err
    );
}

// =========================================================================
// from_json_str raw input size limit (MAX_RAW_INPUT_BYTES = 16 MB)
// =========================================================================

#[test]
fn test_raw_input_size_rejected() {
    // Build a JSON input that exceeds 16 MB via padding in a tag value
    let padding = "x".repeat(16 * 1024 * 1024 + 1);
    let json = format!(
        r#"{{"format":"TEST","stream_info":{{}},"tags":{{"BIG":["{}"]}}}}"#,
        padding
    );
    let err = TagSnapshot::from_json_str(&json).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("size limit") || msg.contains("byte"),
        "expected input size error, got: {}",
        msg
    );
}

#[test]
fn test_raw_input_at_boundary() {
    // A small valid input should pass regardless of exact size checks
    let json = minimal_snapshot_json();
    assert!(
        json.len() < 16 * 1024 * 1024,
        "test setup: input should be small"
    );
    assert!(TagSnapshot::from_json_str(&json).is_ok());
}

// =========================================================================
// Valid complex raw_tags within limits
// =========================================================================

#[test]
fn test_valid_complex_raw_tags() {
    // A moderately complex but within-limits raw_tags tree
    let raw_tags = serde_json::json!({
        "id3_frames": {
            "TIT2": {"encoding": "utf8", "text": ["Test"]},
            "TPE1": {"encoding": "utf8", "text": ["Artist"]},
            "APIC": {"mime": "image/jpeg", "data_base64": "abc="}
        },
        "extra": [1, 2, 3, "four", null, true]
    });
    let json = serde_json::json!({
        "format": "MP3",
        "stream_info": {"sample_rate": 44100},
        "tags": {"TITLE": ["Test"]},
        "raw_tags": raw_tags
    })
    .to_string();

    let snap = TagSnapshot::from_json_str(&json).unwrap();
    assert!(snap.raw_tags.is_some());
}

// =========================================================================
// Invalid JSON
// =========================================================================

#[test]
fn test_from_json_str_invalid_json() {
    assert!(TagSnapshot::from_json_str("{not valid json}").is_err());
    assert!(TagSnapshot::from_json_str("").is_err());
}
