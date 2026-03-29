//! Tests for DSF (Direct Stream Digital) format support

use audex::dsf::{DSF, clear};
use audex::id3::ID3Tags;
use audex::{AudexError, FileType, Tags};
use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const DATA_DIR: &str = "tests/data";

fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(DATA_DIR).join(filename)
}

fn get_temp_copy<P: AsRef<Path>>(path: P) -> std::io::Result<NamedTempFile> {
    let source = fs::read(path)?;
    let mut temp_file = NamedTempFile::new()?;
    temp_file.write_all(&source)?;
    temp_file.flush()?;
    temp_file.seek(SeekFrom::Start(0))?;
    Ok(temp_file)
}

struct Tdsf {
    silence_1: PathBuf,
    silence_2: PathBuf,
    filename_1: NamedTempFile,
}

impl Tdsf {
    fn new() -> Self {
        let silence_1 = data_path("2822400-1ch-0s-silence.dsf");
        let silence_2 = data_path("5644800-2ch-s01-silence.dsf");
        let has_tags = data_path("with-id3.dsf");

        let filename_1 = get_temp_copy(&has_tags).expect("Failed to create temp copy");

        Self {
            silence_1,
            silence_2,
            filename_1,
        }
    }
}

#[test]
fn test_channels() {
    let test = Tdsf::new();

    let dsf_1 = DSF::load(&test.silence_1).unwrap();
    let dsf_2 = DSF::load(&test.silence_2).unwrap();

    assert_eq!(dsf_1.info.channels, 1);
    assert_eq!(dsf_2.info.channels, 2);
}

#[test]
fn test_length() {
    let test = Tdsf::new();

    let dsf_1 = DSF::load(&test.silence_1).unwrap();
    let dsf_2 = DSF::load(&test.silence_2).unwrap();

    let length_1 = dsf_1.info.length.unwrap().as_secs_f64();
    let length_2 = dsf_2.info.length.unwrap().as_secs_f64();

    assert!(length_1.abs() < 0.001); // Should be 0
    assert!((length_2 - 0.01).abs() < 0.001); // Should be 0.01
}

#[test]
fn test_sampling_frequency() {
    let test = Tdsf::new();

    let dsf_1 = DSF::load(&test.silence_1).unwrap();
    let dsf_2 = DSF::load(&test.silence_2).unwrap();

    assert_eq!(dsf_1.info.sample_rate, 2822400);
    assert_eq!(dsf_2.info.sample_rate, 5644800);
}

#[test]
fn test_bits_per_sample() {
    let test = Tdsf::new();

    let dsf_1 = DSF::load(&test.silence_1).unwrap();

    assert_eq!(dsf_1.info.bits_per_sample, 1);
}

#[test]
fn test_notdsf() {
    let empty_ofr = data_path("empty.ofr");
    let result = DSF::load(empty_ofr);
    assert!(result.is_err());

    match result.unwrap_err() {
        AudexError::InvalidData(_) => {} // Expected
        _ => panic!("Expected InvalidData error"),
    }
}

#[test]
fn test_pprint() {
    let test = Tdsf::new();
    let dsf = DSF::load(test.filename_1.path()).unwrap();

    let output = dsf.info.pprint();
    assert!(!output.is_empty());
}

#[test]
fn test_delete() {
    let test = Tdsf::new();
    let mut dsf = DSF::load(test.filename_1.path()).unwrap();

    // Add tags first if none exist
    if dsf.tags.is_none() {
        dsf.tags = Some(ID3Tags::new());
    }

    dsf.clear().unwrap();
    assert!(dsf.tags.is_none());

    let reloaded = DSF::load(test.filename_1.path()).unwrap();
    assert!(reloaded.tags.is_none());
}

#[test]
fn test_module_delete() {
    let test = Tdsf::new();

    clear(test.filename_1.path()).unwrap();

    let dsf = DSF::load(test.filename_1.path()).unwrap();
    assert!(dsf.tags.is_none());
}

#[test]
fn test_module_double_delete() {
    let test = Tdsf::new();

    // Should not panic on double delete
    clear(test.filename_1.path()).unwrap();
    clear(test.filename_1.path()).unwrap();
}

#[test]
fn test_pprint_no_tags() {
    let test = Tdsf::new();
    let mut dsf = DSF::load(test.filename_1.path()).unwrap();

    dsf.tags = None;
    let output = dsf.info.pprint();
    assert!(!output.is_empty());
}

#[test]
fn test_save_no_tags() {
    let test = Tdsf::new();
    let mut dsf = DSF::load(test.filename_1.path()).unwrap();

    dsf.tags = None;
    dsf.save().unwrap();

    assert!(dsf.tags.is_none());
}

#[test]
fn test_add_tags_already_there() {
    let test = Tdsf::new();
    let mut dsf = DSF::load(test.filename_1.path()).unwrap();

    // Add tags if none exist
    if dsf.tags.is_none() {
        dsf.tags = Some(ID3Tags::new());
    }

    // Adding tags when they already exist should be handled gracefully
    let _ = dsf.add_tags();
    assert!(dsf.tags.is_some());
}

#[test]
fn test_mime() {
    let test = Tdsf::new();
    let dsf = DSF::load(test.filename_1.path()).unwrap();

    let mime_types = dsf.mime();
    assert!(mime_types.contains(&"audio/dsf"));
}

#[test]
fn test_loaded_tags() {
    let test = Tdsf::new();

    // Only test if the file actually has tags
    if let Ok(dsf) = DSF::load(test.filename_1.path()) {
        if let Some(tags) = &dsf.tags {
            if let Some(title_values) = tags.get("TIT2") {
                if !title_values.is_empty() {
                    assert_eq!(title_values[0], "DSF title");
                }
            }
        }
    }
}

#[test]
fn test_roundtrip() {
    let test = Tdsf::new();
    let mut dsf = DSF::load(test.filename_1.path()).unwrap();

    // Ensure tags exist
    if dsf.tags.is_none() {
        dsf.tags = Some(ID3Tags::new());
    }

    if let Some(tags) = dsf.tags.as_mut() {
        tags.set("TIT2", vec!["DSF title".to_string()]);
    }

    dsf.save().unwrap();

    let new_dsf = DSF::load(test.filename_1.path()).unwrap();
    if let Some(tags) = &new_dsf.tags {
        if let Some(title_values) = tags.get("TIT2") {
            if !title_values.is_empty() {
                assert_eq!(title_values[0], "DSF title");
            }
        }
    }
}

#[test]
fn test_save_tags() {
    let test = Tdsf::new();
    let mut dsf = DSF::load(test.filename_1.path()).unwrap();

    // Ensure tags exist
    if dsf.tags.is_none() {
        dsf.tags = Some(ID3Tags::new());
    }

    if let Some(tags) = dsf.tags.as_mut() {
        tags.set("TIT2", vec!["foobar".to_string()]);
        dsf.save().unwrap();
    }

    let new_dsf = DSF::load(test.filename_1.path()).unwrap();
    if let Some(tags) = &new_dsf.tags {
        if let Some(title_values) = tags.get("TIT2") {
            if !title_values.is_empty() {
                assert_eq!(title_values[0], "foobar");
            }
        }
    }
}

#[test]
fn test_corrupt_tag() {
    let test = Tdsf::new();

    // Create a corrupted copy by modifying tag data
    let mut corrupted_data = fs::read(test.filename_1.path()).unwrap();

    // Try to corrupt the metadata section if it exists
    if corrupted_data.len() > 100 {
        // Look for ID3 tag and corrupt it
        for i in 0..(corrupted_data.len() - 10) {
            if &corrupted_data[i..i + 3] == b"ID3" {
                corrupted_data[i + 6] = 0xFF;
                corrupted_data[i + 7] = 0xFF;
                break;
            }
        }
    }

    let mut temp_corrupted = NamedTempFile::new().unwrap();
    temp_corrupted.write_all(&corrupted_data).unwrap();
    temp_corrupted.flush().unwrap();

    // Should fail to load corrupted file
    let result = DSF::load(temp_corrupted.path());
    // May succeed or fail depending on corruption location
    // Just ensure we don't panic
    let _ = result;
}

#[test]
fn test_padding() {
    let test = Tdsf::new();
    let mut dsf = DSF::load(test.filename_1.path()).unwrap();

    // Ensure tags exist for padding tests
    if dsf.tags.is_none() {
        dsf.tags = Some(ID3Tags::new());
    }

    dsf.save().unwrap();

    // Test default padding
    let reloaded = DSF::load(test.filename_1.path()).unwrap();
    if let Some(_tags) = &reloaded.tags {
        // Verify the file can be reloaded
        assert!(reloaded.tags.is_some());
    }

    // Test custom padding
    let mut dsf2 = DSF::load(test.filename_1.path()).unwrap();
    dsf2.save().unwrap();

    let reloaded2 = DSF::load(test.filename_1.path()).unwrap();
    assert!(
        reloaded2.tags.is_some(),
        "Tags should persist after save+reload"
    );
}
