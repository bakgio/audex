//! Fuzz target exercising the MP4 write path in depth.
//!
//! Loads from fuzz input, sets multiple metadata fields including cover art,
//! performs save-reload cycles, then removes fields and clears to stress the
//! full write/delete/clear pipeline.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::mp4::{MP4, MP4Cover};
use audex::FileType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }

    let Some((_dir, path)) = helpers::write_temp_file(data, "m4a") else {
        return;
    };

    let Ok(mut file) = MP4::load(&path) else {
        return;
    };

    // Set multiple metadata fields
    let _ = file.set("title", vec!["fuzz-title".into()]);
    let _ = file.set("artist", vec!["fuzz-artist".into()]);
    let _ = file.set("album", vec!["fuzz-album".into()]);
    let _ = file.set("tracknumber", vec!["3".into()]);

    // Add cover art
    if let Some(tags) = file.tags.as_mut() {
        tags.covers.push(MP4Cover::new_jpeg(vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00]));
    }

    // Save and reload to verify round-trip integrity
    if file.save().is_ok() {
        let _ = MP4::load(&path);
    }

    // Remove individual fields and save again
    let _ = file.remove("artist");
    let _ = file.remove("title");
    if file.save().is_ok() {
        let _ = MP4::load(&path);
    }

    // Clear all metadata and save
    let _ = file.clear();
});
