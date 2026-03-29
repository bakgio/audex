//! Fuzz target for format detection from raw bytes.
//!
//! Exercises `detect_format_from_bytes` with arbitrary input including very
//! short buffers, empty input, and random byte patterns. The format detection
//! code must never panic regardless of input content or length.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    // Accept inputs up to 256 bytes; format detection only uses the first 128
    // bytes internally, but we want to test boundary conditions around that limit.
    if data.len() > 256 {
        return;
    }

    // Test without any filename hint
    let _ = audex::detect_format_from_bytes(data, None);

    // Test with various extension hints to exercise extension-based fallback paths
    let extensions = [
        "mp3", "flac", "m4a", "ogg", "wma", "wav", "aif", "mp4",
        "aac", "ac3", "mid", "dsf", "dff", "mpc", "wv", "ape",
        "tak", "tta", "ofr", "opus", "spx",
        "",       // empty extension
        "xyz",    // unknown extension
    ];
    for ext in &extensions {
        let filename = format!("test.{}", ext);
        let hint = Path::new(&filename);
        let _ = audex::detect_format_from_bytes(data, Some(hint));
    }
});
