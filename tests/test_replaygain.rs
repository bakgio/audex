use std::collections::HashMap;

use audex::replaygain::{
    REPLAYGAIN_REFERENCE_LEVEL, ReplayGainInfo, clear_vorbis_comments, format_gain, format_peak,
    from_vorbis_comments, parse_gain, parse_peak, to_vorbis_comments, vorbis_keys,
};

// =========================================================================
// Construction and accessors
// =========================================================================

#[test]
fn test_new_defaults() {
    let rg = ReplayGainInfo::new();
    assert_eq!(rg.track_gain(), None);
    assert_eq!(rg.track_peak(), None);
    assert_eq!(rg.album_gain(), None);
    assert_eq!(rg.album_peak(), None);
    assert_eq!(rg.reference_level(), REPLAYGAIN_REFERENCE_LEVEL);
    assert!(rg.warnings.is_empty());
}

#[test]
fn test_with_track() {
    let rg = ReplayGainInfo::with_track(-3.5, 0.95).unwrap();
    assert_eq!(rg.track_gain(), Some(-3.5));
    assert_eq!(rg.track_peak(), Some(0.95));
    assert_eq!(rg.album_gain(), None);
    assert_eq!(rg.album_peak(), None);
}

#[test]
fn test_with_album() {
    let rg = ReplayGainInfo::with_album(-5.0, 0.98).unwrap();
    assert_eq!(rg.track_gain(), None);
    assert_eq!(rg.track_peak(), None);
    assert_eq!(rg.album_gain(), Some(-5.0));
    assert_eq!(rg.album_peak(), Some(0.98));
}

#[test]
fn test_with_both() {
    let rg = ReplayGainInfo::with_both(-3.5, 0.95, -5.0, 0.98).unwrap();
    assert_eq!(rg.track_gain(), Some(-3.5));
    assert_eq!(rg.track_peak(), Some(0.95));
    assert_eq!(rg.album_gain(), Some(-5.0));
    assert_eq!(rg.album_peak(), Some(0.98));
}

#[test]
fn test_with_track_rejects_nan() {
    assert!(ReplayGainInfo::with_track(f32::NAN, 0.5).is_err());
    assert!(ReplayGainInfo::with_track(0.5, f32::NAN).is_err());
}

#[test]
fn test_with_track_rejects_infinity() {
    assert!(ReplayGainInfo::with_track(f32::INFINITY, 0.5).is_err());
    assert!(ReplayGainInfo::with_track(f32::NEG_INFINITY, 0.5).is_err());
}

#[test]
fn test_with_both_rejects_any_nan() {
    // Each positional argument individually set to NaN should fail
    assert!(ReplayGainInfo::with_both(f32::NAN, 0.5, -1.0, 0.5).is_err());
    assert!(ReplayGainInfo::with_both(-1.0, f32::NAN, -1.0, 0.5).is_err());
    assert!(ReplayGainInfo::with_both(-1.0, 0.5, f32::NAN, 0.5).is_err());
    assert!(ReplayGainInfo::with_both(-1.0, 0.5, -1.0, f32::NAN).is_err());
}

// =========================================================================
// State query methods
// =========================================================================

#[test]
fn test_has_info_empty() {
    assert!(!ReplayGainInfo::new().has_info());
}

#[test]
fn test_has_info_partial() {
    // Any single field present should be enough
    let mut rg = ReplayGainInfo::new();
    rg.set_track_gain(Some(-1.0)).unwrap();
    assert!(rg.has_info());
}

#[test]
fn test_has_track_info() {
    let mut rg = ReplayGainInfo::new();

    // Only gain set — incomplete
    rg.set_track_gain(Some(-3.0)).unwrap();
    assert!(!rg.has_track_info());

    // Both gain and peak set — complete
    rg.set_track_peak(Some(0.9)).unwrap();
    assert!(rg.has_track_info());
}

#[test]
fn test_has_album_info() {
    let mut rg = ReplayGainInfo::new();

    rg.set_album_gain(Some(-5.0)).unwrap();
    assert!(!rg.has_album_info());

    rg.set_album_peak(Some(0.98)).unwrap();
    assert!(rg.has_album_info());
}

// =========================================================================
// Setter methods
// =========================================================================

#[test]
fn test_set_track_gain_valid() {
    let mut rg = ReplayGainInfo::new();
    rg.set_track_gain(Some(-3.5)).unwrap();
    assert_eq!(rg.track_gain(), Some(-3.5));
}

#[test]
fn test_set_track_gain_none_clears() {
    let mut rg = ReplayGainInfo::with_track(-3.5, 0.95).unwrap();
    rg.set_track_gain(None).unwrap();
    assert_eq!(rg.track_gain(), None);
}

#[test]
fn test_set_track_gain_rejects_nan() {
    let mut rg = ReplayGainInfo::with_track(-3.5, 0.95).unwrap();
    assert!(rg.set_track_gain(Some(f32::NAN)).is_err());
    // Original value must be preserved on error
    assert_eq!(rg.track_gain(), Some(-3.5));
}

#[test]
fn test_set_track_peak_valid() {
    let mut rg = ReplayGainInfo::new();
    rg.set_track_peak(Some(0.85)).unwrap();
    assert_eq!(rg.track_peak(), Some(0.85));
}

#[test]
fn test_set_album_gain_valid() {
    let mut rg = ReplayGainInfo::new();
    rg.set_album_gain(Some(-5.0)).unwrap();
    assert_eq!(rg.album_gain(), Some(-5.0));
}

#[test]
fn test_set_album_peak_valid() {
    let mut rg = ReplayGainInfo::new();
    rg.set_album_peak(Some(0.98)).unwrap();
    assert_eq!(rg.album_peak(), Some(0.98));
}

#[test]
fn test_set_reference_level() {
    let mut rg = ReplayGainInfo::new();
    rg.set_reference_level(83.0).unwrap();
    assert_eq!(rg.reference_level(), 83.0);
}

#[test]
fn test_set_reference_level_rejects_nan() {
    let mut rg = ReplayGainInfo::new();
    assert!(rg.set_reference_level(f32::NAN).is_err());
    assert!(rg.set_reference_level(f32::INFINITY).is_err());
    // Default must be preserved
    assert_eq!(rg.reference_level(), REPLAYGAIN_REFERENCE_LEVEL);
}

// =========================================================================
// Adjustment factors
// =========================================================================

#[test]
fn test_track_adjustment_factor_none() {
    assert_eq!(ReplayGainInfo::new().track_adjustment_factor(), None);
}

#[test]
fn test_track_adjustment_factor_zero_db() {
    let rg = ReplayGainInfo::with_track(0.0, 1.0).unwrap();
    let factor = rg.track_adjustment_factor().unwrap();
    assert!((factor - 1.0).abs() < 0.001);
}

#[test]
fn test_track_adjustment_factor_negative() {
    // -6.02 dB ≈ 0.5x multiplier (half volume)
    let rg = ReplayGainInfo::with_track(-6.02, 1.0).unwrap();
    let factor = rg.track_adjustment_factor().unwrap();
    assert!((factor - 0.5).abs() < 0.01);
}

#[test]
fn test_track_adjustment_factor_positive() {
    // +6.02 dB ≈ 2.0x multiplier (double volume)
    let rg = ReplayGainInfo::with_track(6.02, 1.0).unwrap();
    let factor = rg.track_adjustment_factor().unwrap();
    assert!((factor - 2.0).abs() < 0.02);
}

#[test]
fn test_album_adjustment_factor() {
    let rg = ReplayGainInfo::with_album(0.0, 1.0).unwrap();
    let factor = rg.album_adjustment_factor().unwrap();
    assert!((factor - 1.0).abs() < 0.001);

    assert_eq!(ReplayGainInfo::new().album_adjustment_factor(), None);
}

// =========================================================================
// Clear
// =========================================================================

#[test]
fn test_clear_resets_all() {
    let mut rg = ReplayGainInfo::with_both(-3.5, 0.95, -5.0, 0.98).unwrap();
    rg.set_reference_level(83.0).unwrap();
    rg.clear();

    assert_eq!(rg.track_gain(), None);
    assert_eq!(rg.track_peak(), None);
    assert_eq!(rg.album_gain(), None);
    assert_eq!(rg.album_peak(), None);
    assert_eq!(rg.reference_level(), REPLAYGAIN_REFERENCE_LEVEL);
}

// =========================================================================
// Display formatting
// =========================================================================

#[test]
fn test_display_empty() {
    let s = format!("{}", ReplayGainInfo::new());
    assert_eq!(s, "ReplayGain []");
}

#[test]
fn test_display_track_only() {
    let rg = ReplayGainInfo::with_track(-3.50, 0.95).unwrap();
    let s = format!("{}", rg);
    assert!(s.contains("track: -3.50 dB"));
    assert!(s.contains("peak: 0.950000"));
    assert!(!s.contains("album"));
}

#[test]
fn test_display_both() {
    let rg = ReplayGainInfo::with_both(-3.50, 0.95, -5.00, 0.98).unwrap();
    let s = format!("{}", rg);
    assert!(s.contains("track: -3.50 dB"));
    assert!(s.contains("album: -5.00 dB"));
    assert!(s.contains(", "));
}

// =========================================================================
// Parsing and formatting
// =========================================================================

#[test]
fn test_parse_gain_with_db_suffix() {
    assert!((parse_gain("+3.50 dB").unwrap() - 3.5).abs() < 0.001);
    assert!((parse_gain("-2.10 dB").unwrap() - (-2.1)).abs() < 0.001);
    // Also accepts "dB" without leading space
    assert!((parse_gain("+3.50dB").unwrap() - 3.5).abs() < 0.001);
}

#[test]
fn test_parse_gain_without_suffix() {
    assert!((parse_gain("-2.1").unwrap() - (-2.1)).abs() < 0.001);
    assert!((parse_gain("0").unwrap()).abs() < 0.001);
}

#[test]
fn test_parse_gain_whitespace() {
    assert!((parse_gain("  +3.5 dB  ").unwrap() - 3.5).abs() < 0.001);
}

#[test]
fn test_parse_gain_invalid() {
    assert!(parse_gain("not a number").is_err());
    assert!(parse_gain("").is_err());
}

#[test]
fn test_parse_peak_valid() {
    assert!((parse_peak("0.995117").unwrap() - 0.995117).abs() < 0.0001);
    assert!((parse_peak("1.0").unwrap() - 1.0).abs() < 0.001);
}

#[test]
fn test_parse_peak_invalid() {
    assert!(parse_peak("abc").is_err());
    assert!(parse_peak("").is_err());
}

#[test]
fn test_format_gain_roundtrip() {
    for &val in &[-10.0_f32, -3.5, 0.0, 3.5, 10.0] {
        let formatted = format_gain(val).unwrap();
        let parsed = parse_gain(&formatted).unwrap();
        assert!(
            (parsed - val).abs() < 0.01,
            "roundtrip failed for {}: formatted={:?}, parsed={}",
            val,
            formatted,
            parsed
        );
    }
}

#[test]
fn test_format_peak_roundtrip() {
    for &val in &[0.0_f32, 0.5, 0.95, 1.0] {
        let formatted = format_peak(val).unwrap();
        let parsed = parse_peak(&formatted).unwrap();
        assert!(
            (parsed - val).abs() < 0.0001,
            "roundtrip failed for {}: formatted={:?}, parsed={}",
            val,
            formatted,
            parsed
        );
    }
}

// =========================================================================
// Vorbis comment integration
// =========================================================================

/// Build a HashMap with all five standard ReplayGain fields.
fn full_vorbis_rg_comments() -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    m.insert(
        vorbis_keys::TRACK_GAIN.to_string(),
        vec!["-3.50 dB".to_string()],
    );
    m.insert(
        vorbis_keys::TRACK_PEAK.to_string(),
        vec!["0.950000".to_string()],
    );
    m.insert(
        vorbis_keys::ALBUM_GAIN.to_string(),
        vec!["-5.00 dB".to_string()],
    );
    m.insert(
        vorbis_keys::ALBUM_PEAK.to_string(),
        vec!["0.980000".to_string()],
    );
    m.insert(
        vorbis_keys::REFERENCE_LOUDNESS.to_string(),
        vec!["89.00 dB".to_string()],
    );
    m
}

#[test]
fn test_from_vorbis_comments_complete() {
    let comments = full_vorbis_rg_comments();
    let rg = from_vorbis_comments(&comments);

    assert!((rg.track_gain().unwrap() - (-3.5)).abs() < 0.01);
    assert!((rg.track_peak().unwrap() - 0.95).abs() < 0.001);
    assert!((rg.album_gain().unwrap() - (-5.0)).abs() < 0.01);
    assert!((rg.album_peak().unwrap() - 0.98).abs() < 0.001);
    assert!(rg.warnings.is_empty());
}

#[test]
fn test_from_vorbis_comments_partial() {
    let mut m = HashMap::new();
    m.insert(
        vorbis_keys::TRACK_GAIN.to_string(),
        vec!["-3.50 dB".to_string()],
    );
    let rg = from_vorbis_comments(&m);

    assert!(rg.track_gain().is_some());
    assert!(rg.track_peak().is_none());
    assert!(rg.album_gain().is_none());
}

#[test]
fn test_from_vorbis_comments_empty() {
    let rg = from_vorbis_comments(&HashMap::new());
    assert!(!rg.has_info());
}

#[test]
fn test_from_vorbis_comments_invalid_values() {
    let mut m = HashMap::new();
    m.insert(
        vorbis_keys::TRACK_GAIN.to_string(),
        vec!["garbage".to_string()],
    );
    let rg = from_vorbis_comments(&m);

    assert!(rg.track_gain().is_none());
    assert!(!rg.warnings.is_empty(), "should contain a parse warning");
}

#[test]
fn test_to_vorbis_comments_roundtrip() {
    let original = ReplayGainInfo::with_both(-3.5, 0.95, -5.0, 0.98).unwrap();
    let mut comments = HashMap::new();
    to_vorbis_comments(&original, &mut comments).unwrap();

    let recovered = from_vorbis_comments(&comments);
    assert!((recovered.track_gain().unwrap() - (-3.5)).abs() < 0.01);
    assert!((recovered.track_peak().unwrap() - 0.95).abs() < 0.001);
    assert!((recovered.album_gain().unwrap() - (-5.0)).abs() < 0.01);
    assert!((recovered.album_peak().unwrap() - 0.98).abs() < 0.001);
}

#[test]
fn test_clear_vorbis_comments() {
    let mut comments = full_vorbis_rg_comments();
    // Add a non-RG key to ensure it survives the clear
    comments.insert("ARTIST".to_string(), vec!["Test".to_string()]);

    clear_vorbis_comments(&mut comments);

    assert!(!comments.contains_key(vorbis_keys::TRACK_GAIN));
    assert!(!comments.contains_key(vorbis_keys::TRACK_PEAK));
    assert!(!comments.contains_key(vorbis_keys::ALBUM_GAIN));
    assert!(!comments.contains_key(vorbis_keys::ALBUM_PEAK));
    assert!(!comments.contains_key(vorbis_keys::REFERENCE_LOUDNESS));
    // Non-RG key must still be present
    assert!(comments.contains_key("ARTIST"));
}

// =========================================================================
// Vorbis key constants
// =========================================================================

#[test]
fn test_vorbis_key_values() {
    assert_eq!(vorbis_keys::TRACK_GAIN, "REPLAYGAIN_TRACK_GAIN");
    assert_eq!(vorbis_keys::TRACK_PEAK, "REPLAYGAIN_TRACK_PEAK");
    assert_eq!(vorbis_keys::ALBUM_GAIN, "REPLAYGAIN_ALBUM_GAIN");
    assert_eq!(vorbis_keys::ALBUM_PEAK, "REPLAYGAIN_ALBUM_PEAK");
    assert_eq!(
        vorbis_keys::REFERENCE_LOUDNESS,
        "REPLAYGAIN_REFERENCE_LOUDNESS"
    );
}
