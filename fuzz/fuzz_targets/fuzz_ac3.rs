//! Fuzz target for the AC-3 (Dolby Digital) format parser.
//!
//! AC-3 is a read-only format for metadata purposes: it does not support
//! embedded tags. The save/clear methods always return an error. Only the
//! parsing and info-extraction paths are fuzzed here.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::ac3::AC3;
use audex::FileType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let Some((_dir, path)) = helpers::write_temp_file(data, "ac3") else {
        return;
    };
    if let Ok(f) = AC3::load(&path) {
        let _ = f.info();
        let _ = f.tags();
        let _ = f.keys();
        let _ = f.pprint();
    }
});
