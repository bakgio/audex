//! Async fuzz target for extension-based format detection.
//!
//! Async mirror of `fuzz_auto_detect_ext`. Writes fuzz data to a temp
//! file with a random supported extension, then loads via the async
//! path to exercise extension-influenced format scoring.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use libfuzzer_sys::fuzz_target;

/// Supported file extensions used by the format scoring system.
const EXTENSIONS: &[&str] = &[
    "mp3", "mp2", "mpg", "mpeg", "mp4", "m4a", "m4b", "m4p", "m4v", "3gp",
    "3g2", "flac", "ogg", "oggflac", "oga", "ogv", "opus", "spx", "wma",
    "asf", "aiff", "aif", "aifc", "wav", "wave", "ape", "mpc", "mpp", "mp+",
    "wv", "ofr", "ofs", "tta", "tak", "dsf", "dff", "dst", "aac", "aacp",
    "adts", "adif", "ac3", "eac3", "mid", "midi", "kar",
];

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE || data.is_empty() {
        return;
    }

    let data_owned = data.to_vec();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        // Use the first byte of fuzz data to pick an extension
        let ext_index = data_owned[0] as usize % EXTENSIONS.len();
        let ext = EXTENSIONS[ext_index];
        let file_data = &data_owned[1..];

        let Some((_dir, path)) = helpers::write_temp_file_async(file_data, ext).await else {
            return;
        };

        // File::load_async uses the path's extension for format scoring.
        if let Ok(mut file) = audex::File::load_async(&path).await {
            helpers::exercise_dynamic_file_async(&mut file, Some(file_data)).await;
        }
    });
});
