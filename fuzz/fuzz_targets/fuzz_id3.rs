#![no_main]

//! Fuzz target for the raw `id3` module — covers both read and write paths.
//!
//! After a successful load, we mutate a text frame, save the file back,
//! and re-parse the output to verify round-trip robustness.

#[path = "helpers.rs"]
mod helpers;

use audex::id3;
use audex::id3::file::ID3;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let Some((_dir, path)) = helpers::write_temp_file(data, "mp3") else {
        return;
    };

    // Read path: attempt to parse raw ID3 tags from the fuzzed input.
    let _ = id3::load(&path);

    // Write path: load via the file-level API, mutate, save, and re-parse.
    if let Ok(mut f) = ID3::load_from_file(&path) {
        if let Some(tags) = f.tags_mut() {
            let _ = tags.set_text("TIT2", "fuzz-title".to_string());
        }
        if f.save().is_ok() {
            // Re-read the saved output to verify it parses cleanly
            let _ = ID3::load_from_file(&path);
        }
    }
});
