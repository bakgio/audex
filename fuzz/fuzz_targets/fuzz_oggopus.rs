#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::oggopus::OggOpus;
use audex::FileType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let Some((_dir, path)) = helpers::write_temp_file(data, "opus") else {
        return;
    };
    if let Ok(mut f) = OggOpus::load(&path) {
        let _ = f.info();
        let _ = f.tags();
        let _ = f.set("artist", vec!["fuzz".into()]);
        if f.save().is_ok() {
            // Re-read the saved output to verify it parses cleanly
            let _ = OggOpus::load(&path);
        }
    }
});
