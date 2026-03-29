//! Fuzz target for the FLAC format parser and writer.
//!
//! Exercises tag read/write, multi-value tags, tag removal, embedded picture
//! handling, and clear-before-save to cover the full mutation surface.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::flac::{FLAC, Picture};
use audex::FileType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let Some((_dir, path)) = helpers::write_temp_file(data, "flac") else {
        return;
    };
    let Ok(mut f) = FLAC::load(&path) else {
        return;
    };

    let _ = f.info();
    let _ = f.tags();

    // Set multiple tags including multi-value fields
    let _ = f.set("artist", vec!["fuzz-artist-1".into(), "fuzz-artist-2".into()]);
    let _ = f.set("title", vec!["fuzz-title".into()]);
    let _ = f.set("album", vec!["fuzz-album".into()]);
    let _ = f.set("genre", vec!["rock".into(), "jazz".into(), "electronic".into()]);
    let _ = f.set("tracknumber", vec!["7".into()]);

    // Add an embedded picture (front cover)
    let mut picture = Picture::new();
    picture.picture_type = 3; // Front cover
    picture.mime_type = "image/jpeg".to_string();
    picture.description = "Fuzz Cover".to_string();
    picture.width = 100;
    picture.height = 100;
    picture.color_depth = 24;
    picture.data = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00];
    f.add_picture(picture);

    // Save and reload to verify round-trip integrity
    if f.save().is_ok() {
        let _ = FLAC::load(&path);
    }

    // Remove individual fields and save again
    let _ = f.remove("artist");
    let _ = f.remove("genre");
    if f.save().is_ok() {
        let _ = FLAC::load(&path);
    }

    // Clear all pictures and tags, then save
    f.clear_pictures();
    let _ = f.clear();
});
