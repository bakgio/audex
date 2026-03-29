//! Fuzz target for tag budget and bulk tag operations.
//!
//! Exercises the core tag manipulation paths used by the WASM binding layer:
//! setting many tags with varying lengths, get/remove/clear cycles, and saving
//! to a writer. This ensures the library handles extreme tag counts and sizes
//! without panicking or corrupting state.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

/// Derive structured input so the fuzzer can explore tag key/value combinations
/// more effectively than with raw byte slicing.
#[derive(arbitrary::Arbitrary, Debug)]
struct TagBudgetInput {
    /// Raw audio file bytes (will be tried as multiple formats)
    file_data: Vec<u8>,
    /// Tags to set: each entry is a (key, values) pair
    tags: Vec<(String, Vec<String>)>,
    /// Keys to remove after setting
    remove_keys: Vec<String>,
    /// Whether to clear all tags before saving
    clear_before_save: bool,
}

fuzz_target!(|input: TagBudgetInput| {
    if input.file_data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }

    // Cap the number of tag operations to keep iteration bounded
    let max_tags = 200;
    let tags = if input.tags.len() > max_tags {
        &input.tags[..max_tags]
    } else {
        &input.tags
    };

    let cursor = Cursor::new(input.file_data.clone());
    let Ok(mut file) = audex::File::load_from_reader(cursor, None) else {
        return;
    };

    // Set a batch of tags with fuzzer-controlled keys and values
    for (key, values) in tags {
        let _ = file.set(key, values.clone());
    }

    // Verify reads don't panic after bulk writes
    let _ = file.keys();
    let _ = file.items();
    let _ = file.has_tags();

    // Remove selected keys
    for key in &input.remove_keys {
        let _ = file.remove(key);
    }

    // Optionally clear all tags to exercise the clear path
    if input.clear_before_save {
        let _ = file.clear();
    }

    // Save to a cursor to exercise the serialization path
    let mut save_buf = Cursor::new(input.file_data.clone());
    let _ = file.save_to_writer(&mut save_buf);

    // Final read-back: verify no state corruption
    let _ = file.keys();
    let _ = file.items();
});
