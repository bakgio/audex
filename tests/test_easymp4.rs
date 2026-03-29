//! EasyMP4 format tests

use audex::FileType;
use audex::easymp4::EasyMP4;
use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::path::PathBuf;
use tempfile::NamedTempFile;

// Test utilities
fn get_test_file(filename: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("data");
    path.push(filename);
    path
}

/// Create a temporary copy of a test file.
///
/// Reads the source into memory first, then writes to a NamedTempFile.
/// This avoids Windows file-locking issues that occur when parallel tests
/// hold a source-file handle open while `fs::copy` tries to read it.
fn get_temp_copy(filename: &str) -> NamedTempFile {
    let original = get_test_file(filename);
    let data = fs::read(&original).expect("Failed to read source test file");
    let mut tmp = NamedTempFile::new().expect("Failed to create temp file");
    tmp.write_all(&data).expect("Failed to write temp file");
    tmp.flush().expect("Failed to flush temp file");
    tmp.seek(SeekFrom::Start(0))
        .expect("Failed to seek temp file");
    tmp
}

#[test]
fn test_no_tags() {
    let no_tags_file = get_test_file("no-tags.m4a");
    let audio = EasyMP4::load(&no_tags_file).expect("Failed to load no-tags.m4a");

    assert!(audio.tags().is_none(), "Expected no tags");
}

#[test]
fn test_padding() {
    let temp_file = get_temp_copy("has-tags.m4a");
    let mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load has-tags.m4a");

    assert!(mp4.tags().is_some(), "Expected tags to exist");
}

#[test]
fn test_pprint() {
    let temp_file = get_temp_copy("has-tags.m4a");
    let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load has-tags.m4a");

    if let Some(tags) = mp4.tags_mut() {
        tags.set("artist", vec!["baz".to_string()])
            .expect("Failed to set artist");
        let keys = tags.keys();
        assert!(!keys.is_empty(), "Expected some fields");
    }
}

#[test]
fn test_has_key() {
    let temp_file = get_temp_copy("has-tags.m4a");
    let mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load has-tags.m4a");

    if let Some(tags) = mp4.tags() {
        // Test that invalid key doesn't exist
        let _fields = tags.keys();
        assert!(!tags.contains_key("foo"), "foo key should not exist");
    }
}

#[test]
fn test_empty_file() {
    let empty_file = get_test_file("emptyfile.mp3");
    let result = EasyMP4::load(&empty_file);

    assert!(result.is_err(), "Loading empty file should fail");
}

#[test]
fn test_nonexistent_file() {
    let nonexistent = get_test_file("does/not/exist.m4a");
    let result = EasyMP4::load(&nonexistent);

    assert!(result.is_err(), "Loading nonexistent file should fail");
}

#[test]
fn test_register_text_key_persists_custom_atom() {
    let temp_file = get_temp_copy("has-tags.m4a");
    let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load has-tags.m4a");

    mp4.register_text_key("custom_title", "©nam")
        .expect("register custom text mapping");
    mp4.set("custom_title", vec!["Mapped Title".to_string()])
        .expect("set custom title");
    mp4.save().expect("save custom text mapping");

    let reloaded = EasyMP4::load(temp_file.path()).expect("reload custom text mapping");
    assert_eq!(
        reloaded.get("title"),
        Some(vec!["Mapped Title".to_string()]),
        "custom text mapping should write to the registered MP4 atom"
    );
}

#[test]
fn test_register_freeform_text_key_roundtrips() {
    let temp_file = get_temp_copy("has-tags.m4a");
    let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load has-tags.m4a");

    mp4.register_text_key("custom_freeform", "----:com.example:Custom Field")
        .expect("register custom freeform mapping");
    mp4.set("custom_freeform", vec!["Custom Value".to_string()])
        .expect("set freeform value");
    mp4.save().expect("save freeform value");

    let mut reloaded = EasyMP4::load(temp_file.path()).expect("reload freeform value");
    reloaded
        .register_text_key("custom_freeform", "----:com.example:Custom Field")
        .expect("re-register freeform mapping");
    assert_eq!(
        reloaded.get("custom_freeform"),
        Some(vec!["Custom Value".to_string()]),
        "freeform mapping should survive save and reload"
    );
}

#[test]
fn test_write_single() {
    let temp_file = get_temp_copy("has-tags.m4a");

    // Get all supported keys from EasyMP4
    let test_keys = vec![
        "title",
        "album",
        "artist",
        "albumartist",
        "comment",
        "description",
        "grouping",
        "genre",
        "copyright",
        "albumsort",
        "albumartistsort",
        "artistsort",
        "titlesort",
        "composersort",
        "musicbrainz_artistid",
        "musicbrainz_trackid",
        "musicbrainz_albumid",
        "musicbrainz_albumartistid",
        "musicip_puid",
        "musicbrainz_albumstatus",
        "musicbrainz_albumtype",
        "releasecountry",
    ];

    for key in test_keys {
        // Skip special keys that need different handling
        if matches!(key, "tracknumber" | "discnumber" | "date" | "bpm") {
            continue;
        }

        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");

        // Delete existing tags first
        if let Some(tags) = mp4.tags_mut() {
            tags.remove(key).ok(); // Ignore errors for cleanup
        }

        // Test creation
        if let Some(tags) = mp4.tags_mut() {
            tags.set(key, vec!["a test value".to_string()])
                .expect("Failed to set field");
        }

        mp4.save().expect("Failed to save file");

        // Reload and verify
        let reloaded_mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = reloaded_mp4.tags() {
            let values = tags
                .get(key)
                .expect("Failed to get field")
                .unwrap_or_default();
            assert_eq!(
                values,
                vec!["a test value".to_string()],
                "Single value write failed for key: {}",
                key
            );
        }

        // Test non-creation setting (overwrite)
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = mp4.tags_mut() {
            tags.set(key, vec!["a test value".to_string()])
                .expect("Failed to set field");
        }

        mp4.save().expect("Failed to save file again");

        // Verify again
        let final_mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file final time");
        if let Some(tags) = final_mp4.tags() {
            let values = tags
                .get(key)
                .expect("Failed to get field")
                .unwrap_or_default();
            assert_eq!(
                values,
                vec!["a test value".to_string()],
                "Single value overwrite failed for key: {}",
                key
            );
        }

        // Clean up the key
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file for cleanup");
        if let Some(tags) = mp4.tags_mut() {
            tags.remove(key).ok(); // Ignore errors for cleanup
        }
        mp4.save().expect("Failed to save cleanup");
    }
}

#[test]
fn test_write_double() {
    let temp_file = get_temp_copy("has-tags.m4a");

    let test_keys = vec![
        "title",
        "album",
        "artist",
        "albumartist",
        "comment",
        "description",
        "grouping",
        "genre",
        "copyright",
        "albumsort",
        "albumartistsort",
        "artistsort",
        "titlesort",
        "composersort",
        "musicbrainz_artistid",
        "musicbrainz_trackid",
        "musicbrainz_albumid",
        "musicbrainz_albumartistid",
        "musicip_puid",
        "musicbrainz_albumstatus",
        "musicbrainz_albumtype",
        "releasecountry",
    ];

    for key in test_keys {
        // Skip special keys that need different handling
        if matches!(key, "tracknumber" | "discnumber" | "date" | "bpm") {
            continue;
        }

        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");

        // Delete existing tags first
        if let Some(tags) = mp4.tags_mut() {
            tags.remove(key).ok(); // Ignore errors for cleanup
        }

        // Test creation with multiple values
        if let Some(tags) = mp4.tags_mut() {
            tags.set(key, vec!["a test".to_string(), "value".to_string()])
                .expect("Failed to set field");
        }

        mp4.save().expect("Failed to save file");

        // Reload and verify
        let reloaded_mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = reloaded_mp4.tags() {
            let values = tags
                .get(key)
                .expect("Failed to get field")
                .unwrap_or_default();
            assert_eq!(
                values,
                vec!["a test".to_string(), "value".to_string()],
                "Double value write failed for key: {}",
                key
            );
        }

        // Test non-creation setting (overwrite)
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = mp4.tags_mut() {
            tags.set(key, vec!["a test".to_string(), "value".to_string()])
                .expect("Failed to set field");
        }

        mp4.save().expect("Failed to save file again");

        // Verify again
        let final_mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file final time");
        if let Some(tags) = final_mp4.tags() {
            let values = tags
                .get(key)
                .expect("Failed to get field")
                .unwrap_or_default();
            assert_eq!(
                values,
                vec!["a test".to_string(), "value".to_string()],
                "Double value overwrite failed for key: {}",
                key
            );
        }

        // Clean up the key
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file for cleanup");
        if let Some(tags) = mp4.tags_mut() {
            tags.remove(key).ok(); // Ignore errors for cleanup
        }
        mp4.save().expect("Failed to save cleanup");
    }
}

#[test]
fn test_write_date() {
    let temp_file = get_temp_copy("has-tags.m4a");
    let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");

    // Test single date
    if let Some(tags) = mp4.tags_mut() {
        tags.set("date", vec!["2004".to_string()])
            .expect("Failed to set date");
    }
    mp4.save().expect("Failed to save file");

    let reloaded = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
    if let Some(tags) = reloaded.tags() {
        assert_eq!(
            tags.get("date")
                .expect("Failed to get date")
                .unwrap_or_default(),
            vec!["2004".to_string()]
        );
    }

    // Test overwrite
    let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
    if let Some(tags) = mp4.tags_mut() {
        tags.set("date", vec!["2004".to_string()])
            .expect("Failed to set date");
    }
    mp4.save().expect("Failed to save file");

    let final_mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
    if let Some(tags) = final_mp4.tags() {
        assert_eq!(
            tags.get("date")
                .expect("Failed to get date")
                .unwrap_or_default(),
            vec!["2004".to_string()]
        );
    }
}

#[test]
fn test_date_delete() {
    let temp_file = get_temp_copy("has-tags.m4a");
    let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");

    if let Some(tags) = mp4.tags_mut() {
        tags.set("date", vec!["2004".to_string()])
            .expect("Failed to set date");
        assert_eq!(
            tags.get("date")
                .expect("Failed to get date")
                .unwrap_or_default(),
            vec!["2004".to_string()]
        );

        tags.remove("date").expect("Failed to remove date");
        assert!(tags.get("date").expect("Failed to get date").is_none());
    }
}

#[test]
fn test_write_date_double() {
    let temp_file = get_temp_copy("has-tags.m4a");
    let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");

    // Test multiple dates
    if let Some(tags) = mp4.tags_mut() {
        tags.set("date", vec!["2004".to_string(), "2005".to_string()])
            .expect("Failed to set date");
    }
    mp4.save().expect("Failed to save file");

    let reloaded = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
    if let Some(tags) = reloaded.tags() {
        assert_eq!(
            tags.get("date")
                .expect("Failed to get date")
                .unwrap_or_default(),
            vec!["2004".to_string(), "2005".to_string()]
        );
    }

    // Test overwrite
    let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
    if let Some(tags) = mp4.tags_mut() {
        tags.set("date", vec!["2004".to_string(), "2005".to_string()])
            .expect("Failed to set date");
    }
    mp4.save().expect("Failed to save file");

    let final_mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
    if let Some(tags) = final_mp4.tags() {
        assert_eq!(
            tags.get("date")
                .expect("Failed to get date")
                .unwrap_or_default(),
            vec!["2004".to_string(), "2005".to_string()]
        );
    }
}

#[test]
fn test_write_invalid() {
    let temp_file = get_temp_copy("has-tags.m4a");
    let mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");

    if let Some(tags) = mp4.tags() {
        // Test invalid key access
        assert!(
            tags.get("notvalid").is_err(),
            "Invalid key should return error"
        );
    }

    let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");
    if let Some(tags) = mp4.tags_mut() {
        // Test invalid key removal - should not panic
        tags.remove("notvalid").ok(); // Should not panic

        // Test invalid key setting - should not panic but may not store the value
        tags.set("notvalid", vec!["tests".to_string()]).ok(); // May fail silently
    }
}

#[test]
fn test_numeric() {
    let temp_file = get_temp_copy("has-tags.m4a");

    {
        let tag = "bpm";
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");

        // Set numeric value
        if let Some(tags) = mp4.tags_mut() {
            tags.set(tag, vec!["3".to_string()])
                .expect("Failed to set numeric field");
            assert_eq!(
                tags.get(tag)
                    .expect("Failed to get field")
                    .unwrap_or_default(),
                vec!["3".to_string()]
            );
        }

        mp4.save().expect("Failed to save file");

        // Reload and verify
        let reloaded = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = reloaded.tags() {
            assert_eq!(
                tags.get(tag)
                    .expect("Failed to get field")
                    .unwrap_or_default(),
                vec!["3".to_string()]
            );
        }

        // Test deletion
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = mp4.tags_mut() {
            tags.remove(tag).ok(); // Ignore errors for cleanup
            assert!(tags.get(tag).expect("Failed to get field").is_none());

            // Multiple deletion should not panic
            tags.remove(tag).ok(); // Ignore errors for cleanup
        }

        // Test invalid value - this should be handled gracefully
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = mp4.tags_mut() {
            tags.set(tag, vec!["hello".to_string()]).ok(); // May fail for invalid values
            // This handles invalid values gracefully
        }
    }
}

#[test]
fn test_numeric_pairs() {
    let temp_file = get_temp_copy("has-tags.m4a");

    for tag in ["tracknumber", "discnumber"] {
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");

        // Test single number
        if let Some(tags) = mp4.tags_mut() {
            tags.set(tag, vec!["3".to_string()])
                .expect("Failed to set numeric field");
            assert_eq!(
                tags.get(tag)
                    .expect("Failed to get field")
                    .unwrap_or_default(),
                vec!["3".to_string()]
            );
        }

        mp4.save().expect("Failed to save file");

        let reloaded = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = reloaded.tags() {
            assert_eq!(
                tags.get(tag)
                    .expect("Failed to get field")
                    .unwrap_or_default(),
                vec!["3".to_string()]
            );
        }

        // Test deletion
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = mp4.tags_mut() {
            tags.remove(tag).ok(); // Ignore errors for cleanup
            assert!(tags.get(tag).expect("Failed to get field").is_none());

            // Multiple deletion should not panic
            tags.remove(tag).ok(); // Ignore errors for cleanup
        }

        // Test number pair
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = mp4.tags_mut() {
            tags.set(tag, vec!["3/10".to_string()])
                .expect("Failed to set pair field");
            assert_eq!(
                tags.get(tag)
                    .expect("Failed to get field")
                    .unwrap_or_default(),
                vec!["3/10".to_string()]
            );
        }

        mp4.save().expect("Failed to save file");

        let reloaded = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = reloaded.tags() {
            assert_eq!(
                tags.get(tag)
                    .expect("Failed to get field")
                    .unwrap_or_default(),
                vec!["3/10".to_string()]
            );
        }

        // Test deletion again
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = mp4.tags_mut() {
            tags.remove(tag).ok(); // Ignore errors for cleanup
            assert!(tags.get(tag).expect("Failed to get field").is_none());

            // Multiple deletion should not panic
            tags.remove(tag).ok(); // Ignore errors for cleanup
        }

        // Test invalid value - this should be handled gracefully
        let mut mp4 = EasyMP4::load(temp_file.path()).expect("Failed to reload file");
        if let Some(tags) = mp4.tags_mut() {
            tags.set(tag, vec!["hello".to_string()]).ok(); // May fail for invalid values
            // This handles invalid values gracefully
        }
    }
}

#[cfg(test)]
mod easy_mp4_key_tests {
    use super::*;

    #[test]
    fn test_all_registered_keys() {
        // Test that all the expected keys are registered
        let temp_file = get_temp_copy("has-tags.m4a");
        let mp4 = EasyMP4::load(temp_file.path()).expect("Failed to load file");

        if let Some(tags) = mp4.tags() {
            // Test basic text keys
            let text_keys = vec![
                "title",
                "album",
                "artist",
                "albumartist",
                "date",
                "comment",
                "description",
                "grouping",
                "genre",
                "copyright",
                "albumsort",
                "albumartistsort",
                "artistsort",
                "titlesort",
                "composersort",
            ];

            for key in text_keys {
                // Each key should be recognized (not cause errors when accessed)
                let _values = tags.get(key);
            }

            // Test integer keys
            let _bpm = tags.get("bpm");

            // Test integer pair keys
            let _track = tags.get("tracknumber");
            let _disc = tags.get("discnumber");

            // Test freeform keys
            let freeform_keys = vec![
                "musicbrainz_artistid",
                "musicbrainz_trackid",
                "musicbrainz_albumid",
                "musicbrainz_albumartistid",
                "musicip_puid",
                "musicbrainz_albumstatus",
                "musicbrainz_albumtype",
                "releasecountry",
            ];

            for key in freeform_keys {
                let _values = tags.get(key);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MetadataFields trait on EasyMP4Tags
// ---------------------------------------------------------------------------

#[cfg(test)]
mod metadata_fields_tests {
    use super::*;
    use audex::MetadataFields;

    #[test]
    fn test_easymp4_metadata_fields_roundtrip() {
        let tmp = get_temp_copy("has-tags.m4a");

        let mut easy = EasyMP4::load(tmp.path()).expect("Should load M4A");

        // Set fields through the MetadataFields trait
        let tags = easy.tags_mut().expect("Should have mutable tags");
        tags.set_title("Trait Title".to_string());
        tags.set_artist("Trait Artist".to_string());
        tags.set_album("Trait Album".to_string());
        tags.set_date("2025-01-01".to_string());
        tags.set_genre("Ambient".to_string());
        tags.set_track_number(9);

        easy.save().expect("Should save");

        // Reload and verify through the same trait
        let reloaded = EasyMP4::load(tmp.path()).expect("Should reload");
        let t = reloaded.tags().expect("Should have tags after reload");

        assert_eq!(t.title().map(String::as_str), Some("Trait Title"));
        assert_eq!(t.artist().map(String::as_str), Some("Trait Artist"));
        assert_eq!(t.album().map(String::as_str), Some("Trait Album"));
        assert_eq!(t.date().map(String::as_str), Some("2025-01-01"));
        assert_eq!(t.genre().map(String::as_str), Some("Ambient"));
        assert_eq!(t.track_number(), Some(9));
    }

    #[test]
    fn test_easymp4_metadata_fields_empty_file() {
        let tmp = get_temp_copy("no-tags.m4a");

        let easy = EasyMP4::load(tmp.path()).expect("Should load tagless M4A");

        // Tagless file may return None for tags() — all fields should be absent
        match easy.tags() {
            Some(t) => {
                assert!(t.artist().is_none());
                assert!(t.album().is_none());
                assert!(t.title().is_none());
                assert!(t.date().is_none());
                assert!(t.genre().is_none());
                assert!(t.track_number().is_none());
            }
            None => {
                // No tag container at all — that's also correct for a tagless file
            }
        }
    }
}
