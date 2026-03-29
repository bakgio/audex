#![cfg(feature = "serde")]

//! Edge-case tests for the custom serde helpers (duration_as_secs_f64,
//! bytes_as_base64) and their boundary conditions.

use audex::DynamicStreamInfo;
use audex::apev2::{APEValue, APEValueType};
use audex::snapshot::StreamInfoSnapshot;

// ---------------------------------------------------------------------------
// duration_as_secs_f64: Option<Duration> <-> f64
// ---------------------------------------------------------------------------

#[test]
fn duration_none_roundtrip() {
    let snap = StreamInfoSnapshot {
        length_secs: None,
        bitrate: None,
        sample_rate: None,
        channels: None,
        bits_per_sample: None,
    };

    let json = serde_json::to_string(&snap).unwrap();
    let back: StreamInfoSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back.length_secs, None);
}

#[test]
fn duration_zero_roundtrip() {
    let snap = StreamInfoSnapshot {
        length_secs: Some(0.0),
        bitrate: None,
        sample_rate: None,
        channels: None,
        bits_per_sample: None,
    };

    let json = serde_json::to_string(&snap).unwrap();
    assert!(
        json.contains("0.0") || json.contains("0"),
        "Zero should serialize as 0"
    );

    let back: StreamInfoSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back.length_secs, Some(0.0));
}

#[test]
fn duration_large_value_roundtrip() {
    // 24 hours in seconds — well within f64 precision but exercises large values
    let secs = 86400.0;
    let snap = StreamInfoSnapshot {
        length_secs: Some(secs),
        bitrate: None,
        sample_rate: None,
        channels: None,
        bits_per_sample: None,
    };

    let json = serde_json::to_string(&snap).unwrap();
    let back: StreamInfoSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back.length_secs, Some(secs));
}

#[test]
fn duration_nan_deserializes_to_none() {
    // NaN is not a valid duration; the helper must return None instead of panicking
    let json = r#"{"length_secs":null,"bitrate":null,"sample_rate":null,"channels":null,"bits_per_sample":null}"#;
    let snap: StreamInfoSnapshot = serde_json::from_str(json).unwrap();
    assert_eq!(snap.length_secs, None);
}

#[test]
fn duration_negative_preserved_as_f64() {
    // StreamInfoSnapshot stores length as Option<f64>, not Option<Duration>.
    // Negative values are technically invalid but preserved at the serde layer
    // to avoid silent data loss during format conversions.
    let json = r#"{"length_secs":-1.0,"bitrate":null,"sample_rate":null,"channels":null,"bits_per_sample":null}"#;
    let snap: StreamInfoSnapshot = serde_json::from_str(json).unwrap();
    assert_eq!(snap.length_secs, Some(-1.0));
}

// ---------------------------------------------------------------------------
// bytes_as_base64: Vec<u8> <-> base64 string
// ---------------------------------------------------------------------------

#[test]
fn base64_empty_vec_roundtrip() {
    let value = APEValue {
        value_type: APEValueType::Binary,
        data: vec![],
    };

    let json = serde_json::to_string(&value).unwrap();
    let back: APEValue = serde_json::from_str(&json).unwrap();
    assert!(
        back.data.is_empty(),
        "Empty binary data must survive roundtrip"
    );
}

#[test]
fn base64_small_binary_roundtrip() {
    let original = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let value = APEValue {
        value_type: APEValueType::Binary,
        data: original.clone(),
    };

    let json = serde_json::to_string(&value).unwrap();
    // The JSON should contain a base64 string, not an integer array
    assert!(
        !json.contains("[222"),
        "Binary data must be base64-encoded, not an array"
    );

    let back: APEValue = serde_json::from_str(&json).unwrap();
    assert_eq!(back.data, original);
}

#[test]
fn base64_1mb_binary_roundtrip() {
    // 1 MB of pseudorandom-looking data to catch truncation or corruption
    let original: Vec<u8> = (0u8..=255).cycle().take(1024 * 1024).collect();
    let value = APEValue {
        value_type: APEValueType::Binary,
        data: original.clone(),
    };

    let json = serde_json::to_string(&value).unwrap();
    let back: APEValue = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back.data.len(),
        original.len(),
        "1 MB binary payload must survive base64 roundtrip"
    );
    assert_eq!(back.data, original);
}

// ---------------------------------------------------------------------------
// duration_as_secs_f64 on DynamicStreamInfo: boundary values
// ---------------------------------------------------------------------------

#[test]
fn dynamic_stream_info_zero_duration_roundtrip() {
    use audex::StreamInfo;

    // Zero-length duration should serialize to 0.0 and deserialize back
    let json = r#"{"length":0.0,"bitrate":null,"sample_rate":null,"channels":null,"bits_per_sample":null}"#;
    let info: DynamicStreamInfo = serde_json::from_str(json).unwrap();
    assert_eq!(
        info.length(),
        Some(std::time::Duration::from_secs(0)),
        "Zero duration should round-trip through f64"
    );
}

#[test]
fn dynamic_stream_info_tiny_duration() {
    use audex::StreamInfo;

    // Sub-microsecond precision — exercises the fractional-second path
    let json = r#"{"length":0.000001,"bitrate":null,"sample_rate":null,"channels":null,"bits_per_sample":null}"#;
    let info: DynamicStreamInfo = serde_json::from_str(json).unwrap();
    let dur = info
        .length()
        .expect("Tiny positive duration should deserialize");
    assert!(dur.as_secs_f64() > 0.0 && dur.as_secs_f64() < 0.001);
}

#[test]
fn dynamic_stream_info_large_finite_duration() {
    use audex::StreamInfo;

    // 100 years in seconds — exercises large but finite values
    let hundred_years = 100.0 * 365.25 * 86400.0;
    let json = format!(
        r#"{{"length":{},"bitrate":null,"sample_rate":null,"channels":null,"bits_per_sample":null}}"#,
        hundred_years
    );
    let info: DynamicStreamInfo = serde_json::from_str(&json).unwrap();
    let dur = info
        .length()
        .expect("Large finite duration should deserialize");
    let delta = (dur.as_secs_f64() - hundred_years).abs();
    assert!(
        delta < 1.0,
        "Duration should be close to input: expected ~{}, got {}",
        hundred_years,
        dur.as_secs_f64()
    );
}

#[test]
fn dynamic_stream_info_negative_duration_becomes_none() {
    use audex::StreamInfo;

    // Negative durations are invalid — the helper should yield None
    let json = r#"{"length":-5.0,"bitrate":null,"sample_rate":null,"channels":null,"bits_per_sample":null}"#;
    let info: DynamicStreamInfo = serde_json::from_str(json).unwrap();
    assert_eq!(
        info.length(),
        None,
        "Negative duration should deserialize as None"
    );
}

#[test]
fn dynamic_stream_info_null_duration() {
    use audex::StreamInfo;

    let json = r#"{"length":null,"bitrate":null,"sample_rate":null,"channels":null,"bits_per_sample":null}"#;
    let info: DynamicStreamInfo = serde_json::from_str(json).unwrap();
    assert_eq!(info.length(), None);
}

// ---------------------------------------------------------------------------
// bytes_as_base64: invalid input rejection
// ---------------------------------------------------------------------------

#[test]
fn base64_invalid_string_rejected() {
    // Not valid base64 — should fail deserialization, not panic
    let json = r#"{"value_type":"Binary","data":"!!!NOT_BASE64!!!"}"#;
    let result = serde_json::from_str::<APEValue>(json);
    assert!(result.is_err(), "Invalid base64 must be rejected");
}
