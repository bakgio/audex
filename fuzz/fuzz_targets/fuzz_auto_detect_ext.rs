//! Fuzz target for extension-based format detection.
//!
//! The companion `fuzz_auto_detect` target uses `load_from_reader` which
//! bypasses the file-extension scoring heuristic. This target writes fuzz
//! data to a temp file with a random extension drawn from the set of
//! supported formats, then loads via `File::load()` — exercising the
//! code path where the extension influences format scoring.

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

    // Use the first byte of fuzz data to pick an extension, then use
    // the rest as the file content. This lets the fuzzer explore all
    // extension/content combinations.
    let ext_index = data[0] as usize % EXTENSIONS.len();
    let ext = EXTENSIONS[ext_index];
    let file_data = &data[1..];

    let Some((_dir, path)) = helpers::write_temp_file(file_data, ext) else {
        return;
    };

    // File::load uses the path's extension for format scoring.
    if let Ok(mut file) = audex::File::load(&path) {
        helpers::exercise_dynamic_file(&mut file, Some(file_data));
    }
});
