//! Fuzz target for `TagSnapshot` JSON deserialization.
//!
//! Exercises `TagSnapshot::from_json_str` with arbitrary byte sequences to
//! verify that malformed or adversarial JSON inputs are handled gracefully
//! without panics, hangs, or unbounded allocations.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only process valid UTF-8 input since from_json_str requires a str
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = audex::snapshot::TagSnapshot::from_json_str(s);
    }
});
