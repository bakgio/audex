#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::mp4::MP4;
use audex::FileType;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }
    let Some((_dir, path)) = helpers::write_temp_file(data, "m4a") else {
        return;
    };
    if let Ok(mut f) = MP4::load(&path) {
        let _ = f.info();
        let _ = f.tags();
        let _ = f.set("artist", vec!["fuzz".into()]);
        let _ = f.set("title", vec!["fuzz-title".into(), "fuzz-alt".into()]);
        let _ = f.remove("artist");
        if let Some(tags) = f.tags.as_mut() {
            tags.covers.clear();
            tags.covers.push(audex::mp4::MP4Cover::new_jpeg(vec![0xFF, 0xD8, 0xFF]));
        }
        if f.save().is_ok() {
            // Re-read the saved output to verify it parses cleanly
            let _ = MP4::load(&path);
        }
        let _ = f.clear();
    }
});
