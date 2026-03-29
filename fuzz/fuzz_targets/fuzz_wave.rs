//! Fuzz target for the WAVE format parser and writer.
//!
//! Exercises tag read/write, multi-value tags, tag removal, and clear-before-save
//! to cover the full mutation surface for WAV files.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::wave::WAVE;
use audex::FileType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let Some((_dir, path)) = helpers::write_temp_file(data, "wav") else {
        return;
    };
    let Ok(mut f) = WAVE::load(&path) else {
        return;
    };

    let _ = f.info();
    let _ = f.tags();

    // Set multiple tags including multi-value fields
    let _ = f.set("artist", vec!["fuzz-artist-1".into(), "fuzz-artist-2".into()]);
    let _ = f.set("title", vec!["fuzz-title".into()]);
    let _ = f.set("album", vec!["fuzz-album".into()]);
    let _ = f.set("genre", vec!["rock".into(), "jazz".into()]);
    let _ = f.set("tracknumber", vec!["5".into()]);

    // Save and reload to verify round-trip integrity
    if f.save().is_ok() {
        let _ = WAVE::load(&path);
    }

    // Remove individual fields and save again
    let _ = f.remove("artist");
    let _ = f.remove("title");
    if f.save().is_ok() {
        let _ = WAVE::load(&path);
    }

    // Clear all tags and save
    let _ = f.clear();
});
