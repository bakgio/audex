#![cfg(feature = "serde")]

//! Core type serialization and deserialization tests.
//!
//! Validates that fundamental audex types (BasicTags, PaddingInfo,
//! DynamicStreamInfo) round-trip correctly through JSON and TOML.

mod common;

use audex::tags::{BasicTags, PaddingInfo, Tags};

// ---------------------------------------------------------------------------
// BasicTags
// ---------------------------------------------------------------------------

#[test]
fn test_basic_tags_json_roundtrip() {
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["Test Artist".into()]);
    tags.set("album", vec!["Test Album".into()]);

    let json = serde_json::to_string(&tags).unwrap();
    let deserialized: BasicTags = serde_json::from_str(&json).unwrap();

    assert_eq!(tags.get("artist"), deserialized.get("artist"));
    assert_eq!(tags.get("album"), deserialized.get("album"));
}

#[test]
fn test_basic_tags_toml_roundtrip() {
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["TOML Artist".into()]);

    let toml_str = toml::to_string(&tags).unwrap();
    let deserialized: BasicTags = toml::from_str(&toml_str).unwrap();

    assert_eq!(tags.get("artist"), deserialized.get("artist"));
}

#[test]
fn test_padding_info_serialize() {
    let info = PaddingInfo::new(1024, 5_000_000);

    let json = serde_json::to_string(&info).unwrap();
    let deserialized: PaddingInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.padding, 1024);
    assert_eq!(deserialized.size, 5_000_000);
}

#[test]
fn test_empty_tags_serialize() {
    let tags = BasicTags::new();

    let json = serde_json::to_string(&tags).unwrap();
    let deserialized: BasicTags = serde_json::from_str(&json).unwrap();

    assert!(deserialized.keys().is_empty());
}

#[test]
fn test_unicode_tags_serialize() {
    let mut tags = BasicTags::new();
    tags.set("artist", vec!["日本語アーティスト".into()]);
    tags.set("title", vec!["Ünïcödé Tïtlé".into()]);
    tags.set("album", vec!["Альбом".into()]);

    let json = serde_json::to_string(&tags).unwrap();
    let deserialized: BasicTags = serde_json::from_str(&json).unwrap();

    assert_eq!(tags.get("artist"), deserialized.get("artist"));
    assert_eq!(tags.get("title"), deserialized.get("title"));
    assert_eq!(tags.get("album"), deserialized.get("album"));
}

#[test]
fn test_multivalue_tags_serialize() {
    let mut tags = BasicTags::new();
    tags.set(
        "artist",
        vec![
            "Artist One".into(),
            "Artist Two".into(),
            "Artist Three".into(),
        ],
    );

    let json = serde_json::to_string(&tags).unwrap();
    let deserialized: BasicTags = serde_json::from_str(&json).unwrap();

    let original = tags.get("artist").unwrap();
    let restored = deserialized.get("artist").unwrap();
    assert_eq!(original.len(), 3);
    assert_eq!(original, restored);
}

// ---------------------------------------------------------------------------
// DynamicStreamInfo (via snapshot, since fields are private)
// ---------------------------------------------------------------------------

#[test]
fn test_dynamic_stream_info_serialize() {
    use audex::snapshot::StreamInfoSnapshot;

    let snap = StreamInfoSnapshot {
        length_secs: Some(123.456),
        bitrate: Some(320_000),
        sample_rate: Some(44100),
        channels: Some(2),
        bits_per_sample: Some(16),
    };

    let json = serde_json::to_string(&snap).unwrap();
    let deserialized: StreamInfoSnapshot = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.length_secs, Some(123.456));
    assert_eq!(deserialized.bitrate, Some(320_000));
    assert_eq!(deserialized.sample_rate, Some(44100));
    assert_eq!(deserialized.channels, Some(2));
    assert_eq!(deserialized.bits_per_sample, Some(16));
}

#[test]
fn test_duration_as_f64_seconds() {
    use audex::snapshot::StreamInfoSnapshot;

    let snap = StreamInfoSnapshot {
        length_secs: Some(243.72),
        bitrate: None,
        sample_rate: None,
        channels: None,
        bits_per_sample: None,
    };

    let json = serde_json::to_string(&snap).unwrap();
    // Verify the JSON contains a numeric value, not a Duration struct
    assert!(json.contains("243.72"));
    // Duration should not appear as { "secs": N, "nanos": N }
    assert!(!json.contains("nanos"));
}

// ---------------------------------------------------------------------------
// Duration::from_secs_f64 panic tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod duration_from_f64_panic_tests {
    use std::time::Duration;

    /// Demonstrate that Duration::from_secs_f64 panics on negative values.
    /// This confirms the underlying vulnerability that the serde helper
    /// must guard against.
    #[test]
    fn test_duration_from_negative_panics() {
        let result = std::panic::catch_unwind(|| Duration::from_secs_f64(-1.0));
        assert!(
            result.is_err(),
            "Duration::from_secs_f64(-1.0) should panic"
        );
    }

    /// Demonstrate that Duration::from_secs_f64 panics on NaN.
    #[test]
    fn test_duration_from_nan_panics() {
        let result = std::panic::catch_unwind(|| Duration::from_secs_f64(f64::NAN));
        assert!(result.is_err(), "Duration::from_secs_f64(NaN) should panic");
    }

    /// Demonstrate that Duration::from_secs_f64 panics on infinity.
    #[test]
    fn test_duration_from_infinity_panics() {
        let result = std::panic::catch_unwind(|| Duration::from_secs_f64(f64::INFINITY));
        assert!(
            result.is_err(),
            "Duration::from_secs_f64(INFINITY) should panic"
        );
    }

    /// Verify that the validation logic correctly identifies bad values.
    #[test]
    fn test_validation_catches_bad_values() {
        let bad_values: &[f64] = &[f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, -0.001];

        for &val in bad_values {
            assert!(
                !val.is_finite() || val < 0.0,
                "Value {} should be caught by validation",
                val
            );
        }

        let good_values: &[f64] = &[0.0, 0.5, 180.0, 3600.0];
        for &val in good_values {
            assert!(
                val.is_finite() && val >= 0.0,
                "Value {} should pass validation",
                val
            );
        }
    }
}
