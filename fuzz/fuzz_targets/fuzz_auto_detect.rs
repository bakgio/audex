//! Fuzz target for automatic format detection.
//!
//! This is the single highest-value target: it exercises the format scoring
//! system and dispatches into every format parser in the crate.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }

    let original = data.to_vec();
    let cursor = Cursor::new(original.clone());
    if let Ok(mut file) = audex::File::load_from_reader(cursor, None) {
        helpers::exercise_dynamic_file(&mut file, Some(&original));
    }
});
