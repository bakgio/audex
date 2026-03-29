use audex::FileType;
/// Tests for EasyID3 key handlers
/// - ReplayGain handlers (track_gain, album_gain, track_peak, album_peak)
/// - MusicBrainz ID handlers
/// - Website field handlers
/// - Performer handlers
use audex::easyid3::EasyID3;

#[test]
fn test_replaygain_handlers() {
    // Test that replaygain handlers work without errors
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Test all replaygain setters
    assert!(
        tags.set("replaygain_track_gain", &["-6.00 dB".to_string()])
            .is_ok()
    );
    assert!(
        tags.set("replaygain_album_gain", &["-8.50 dB".to_string()])
            .is_ok()
    );
    assert!(
        tags.set("replaygain_track_peak", &["0.985".to_string()])
            .is_ok()
    );
    assert!(
        tags.set("replaygain_album_peak", &["0.999".to_string()])
            .is_ok()
    );

    // Save should work
    assert!(tags.save().is_ok());
}

#[test]
fn test_musicbrainz_handlers() {
    // Test that MusicBrainz handlers work without errors
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Test MusicBrainz setters
    assert!(
        tags.set(
            "musicbrainz_trackid",
            &["d6118046-407d-4e06-a1ba-49c399a4c42f".to_string()]
        )
        .is_ok()
    );
    assert!(
        tags.set(
            "musicbrainz_albumid",
            &["f6118046-407d-4e06-a1ba-49c399a4c42f".to_string()]
        )
        .is_ok()
    );

    // Save should work
    assert!(tags.save().is_ok());
}

#[test]
fn test_website_handler() {
    // Test that website handler works without errors
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Test website setter (requires colon)
    assert!(
        tags.set(
            "website:official",
            &["https://example.com/artist".to_string()]
        )
        .is_ok()
    );

    // Save should work
    assert!(tags.save().is_ok());
}

#[test]
fn test_performer_handler() {
    // Test that performer handler works without errors
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Test performer setter
    assert!(
        tags.set("performer:guitar", &["John Doe".to_string()])
            .is_ok()
    );
    assert_eq!(
        tags.get("performer:guitar"),
        Some(vec!["John Doe".to_string()])
    );

    // Save should work
    assert!(tags.save().is_ok());

    let loaded_tags = EasyID3::load(&test_path).unwrap();
    assert_eq!(
        audex::Tags::get(&loaded_tags, "performer:guitar"),
        Some(&["John Doe".to_string()][..])
    );
}

#[test]
fn test_delete_replaygain() {
    // Test deleting replaygain fields
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Set and then delete
    tags.set("replaygain_track_gain", &["-6.00 dB".to_string()])
        .unwrap();
    tags.save().unwrap();

    let mut loaded_tags = EasyID3::load(&test_path).unwrap();
    let _ = loaded_tags.remove("replaygain_track_gain");

    // Saving after delete should work
    assert!(loaded_tags.save().is_ok());
}

#[test]
fn test_delete_musicbrainz() {
    // Test deleting MusicBrainz fields
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Set and then delete
    tags.set(
        "musicbrainz_trackid",
        &["d6118046-407d-4e06-a1ba-49c399a4c42f".to_string()],
    )
    .unwrap();
    tags.save().unwrap();

    let mut loaded_tags = EasyID3::load(&test_path).unwrap();
    let _ = loaded_tags.remove("musicbrainz_trackid");

    // Saving after delete should work
    assert!(loaded_tags.save().is_ok());
}

#[test]
fn test_delete_website() {
    // Test deleting website field
    let temp_dir = tempfile::tempdir().unwrap();
    let test_path = temp_dir.path().join("test.mp3");
    let mp3_data = include_bytes!("data/silence-44-s.mp3");
    std::fs::write(&test_path, mp3_data).unwrap();

    let mut tags = EasyID3::load(&test_path).unwrap();

    // Set and then delete
    tags.set("website:official", &["https://example.com".to_string()])
        .unwrap();
    tags.save().unwrap();

    let mut loaded_tags = EasyID3::load(&test_path).unwrap();
    let _ = loaded_tags.remove("website:official");

    // Saving after delete should work
    assert!(loaded_tags.save().is_ok());
}
