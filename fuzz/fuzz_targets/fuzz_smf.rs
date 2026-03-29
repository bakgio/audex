//! Fuzz target for the Standard MIDI File (SMF) format parser.
//!
//! MIDI files use meta events rather than embedded tag metadata. The set/remove
//! methods are no-ops, and save/clear always return errors. Only the parsing
//! and info-extraction paths are fuzzed here.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::smf::SMF;
use audex::FileType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let Some((_dir, path)) = helpers::write_temp_file(data, "mid") else {
        return;
    };
    if let Ok(f) = SMF::load(&path) {
        let _ = f.info();
        let _ = f.tags();
        let _ = f.keys();
        let _ = f.pprint();
    }
});
