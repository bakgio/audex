//! Fuzz target for cross-format tag conversion and TagMap merging.
//!
//! Splits fuzz input into two halves, attempts to load each as an audio file
//! via auto-detection, and exercises `import_tags_from` between them. Also
//! tests `TagMap::merge` with the extracted tag data.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // Reject oversized inputs to keep iteration times reasonable.
    if data.len() > 4 * 1024 * 1024 || data.len() < 2 {
        return;
    }

    let mid = data.len() / 2;
    let (left_bytes, right_bytes) = data.split_at(mid);

    let left_cursor = Cursor::new(left_bytes.to_vec());
    let right_cursor = Cursor::new(right_bytes.to_vec());

    let left_result = audex::File::load_from_reader(left_cursor, None);
    let right_result = audex::File::load_from_reader(right_cursor, None);

    // If both halves parse as valid audio, exercise cross-format tag import.
    if let (Ok(mut left_file), Ok(right_file)) = (left_result, right_result) {
        let _ = left_file.import_tags_from(&right_file);

        // Also exercise TagMap merging between the two parsed files.
        let left_map = left_file.to_tag_map();
        let right_map = right_file.to_tag_map();

        let mut merged = left_map.clone();
        merged.merge(&right_map, false);

        let mut merged_overwrite = left_map.clone();
        merged_overwrite.merge(&right_map, true);
    }

    // Even if only one side parses, exercise TagMap operations on it.
    if let Ok(file) = audex::File::load_from_reader(
        Cursor::new(left_bytes.to_vec()),
        None,
    ) {
        let map = file.to_tag_map();
        let mut empty = audex::TagMap::new();
        empty.merge(&map, false);
        empty.merge(&map, true);
    }
});
