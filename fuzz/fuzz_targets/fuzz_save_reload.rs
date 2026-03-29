//! Fuzz target for save-reload round-trip integrity.
//!
//! Loads an audio file from arbitrary bytes, modifies tags, saves to an
//! in-memory buffer, then reloads from the saved bytes. This exercises the
//! same serialization and deserialization paths that the WASM binding layer
//! uses, ensuring no panics or memory safety issues across the full cycle.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

/// Structured input for the save-reload round-trip fuzzer.
#[derive(arbitrary::Arbitrary, Debug)]
struct SaveReloadInput {
    /// Raw audio file bytes
    file_data: Vec<u8>,
    /// Tags to set before the first save
    initial_tags: Vec<(String, Vec<String>)>,
    /// Tags to set after reloading (second mutation round)
    second_tags: Vec<(String, Vec<String>)>,
    /// Keys to remove after the second mutation
    remove_keys: Vec<String>,
}

fuzz_target!(|input: SaveReloadInput| {
    if input.file_data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }

    // Cap tag operations for bounded execution time
    let max_ops = 100;

    // --- First load ---
    let cursor = Cursor::new(input.file_data.clone());
    let Ok(mut file) = audex::File::load_from_reader(cursor, None) else {
        return;
    };

    // Apply initial tags
    for (key, values) in input.initial_tags.iter().take(max_ops) {
        let _ = file.set(key, values.clone());
    }

    // Save to an in-memory buffer
    let mut save_buf = Cursor::new(input.file_data.clone());
    if file.save_to_writer(&mut save_buf).is_err() {
        return;
    }

    // --- Reload from saved bytes ---
    let saved_bytes = save_buf.into_inner();
    let reload_cursor = Cursor::new(saved_bytes.clone());
    let Ok(mut reloaded) = audex::File::load_from_reader(reload_cursor, None) else {
        // The saved output may not be valid for reload (format-dependent);
        // this is acceptable as long as we don't panic.
        return;
    };

    // Verify basic accessors work on the reloaded file
    let _ = reloaded.format_name();
    let _ = reloaded.keys();
    let _ = reloaded.items();
    let _ = reloaded.has_tags();

    // Apply second round of mutations
    for (key, values) in input.second_tags.iter().take(max_ops) {
        let _ = reloaded.set(key, values.clone());
    }

    // Remove selected keys
    for key in input.remove_keys.iter().take(max_ops) {
        let _ = reloaded.remove(key);
    }

    // Save again to verify the second round-trip
    let mut second_buf = Cursor::new(saved_bytes);
    let _ = reloaded.save_to_writer(&mut second_buf);
});
