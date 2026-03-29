//! Fuzz target for the AAC (ADTS/ADIF) format parser.
//!
//! AAC is a read-only format for metadata purposes: ADTS and ADIF streams do
//! not have a standardized metadata container. The save/clear methods always
//! return an error. Only the parsing and info-extraction paths are fuzzed here.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::aac::AAC;
use audex::FileType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let Some((_dir, path)) = helpers::write_temp_file(data, "aac") else {
        return;
    };
    if let Ok(f) = AAC::load(&path) {
        let _ = f.info();
        let _ = f.tags();
        let _ = f.keys();
        let _ = f.pprint();
    }
});
