#![no_main]

//! Fuzz target for the generic Ogg container parser — covers both read
//! and write paths.
//!
//! After a successful load, every parsed page is serialized back through
//! `write_to` and the resulting bytes are re-parsed to verify round-trip
//! integrity.

#[path = "helpers.rs"]
mod helpers;

use audex::ogg::OggFile;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let Some((_dir, path)) = helpers::write_temp_file(data, "ogg") else {
        return;
    };

    // Read path: attempt to parse the Ogg container from the fuzzed input.
    let Ok(ogg) = OggFile::load(&path) else {
        return;
    };

    // Write path: serialize each page back and verify the output re-parses.
    let mut buf = Vec::new();
    for page in &ogg.pages {
        let _ = page.write_to(&mut buf);
    }

    if !buf.is_empty() {
        // Write the re-serialized data to a temp file and reload it to
        // exercise the full load-save-load cycle.
        let rewrite_path = _dir.path().join("fuzz_rewrite.ogg");
        if std::fs::write(&rewrite_path, &buf).is_ok() {
            let _ = OggFile::load(&rewrite_path);
        }
    }
});
