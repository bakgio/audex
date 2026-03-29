#![no_main]

//! Fuzz target for tagmap string processing and normalization functions.
//!
//! Exercises `resolve_id3_genre`, `normalize_track_disc`, and `normalize_boolean`
//! with arbitrary untrusted strings to catch panics, hangs, or unexpected behavior
//! in the normalization layer.

use audex::tagmap::normalize::{
    combine_track_disc, normalize_boolean, normalize_date, normalize_track_disc,
    resolve_id3_genre, TagSystem,
};
use libfuzzer_sys::fuzz_target;

/// All tag systems to exercise during boolean normalization.
const TAG_SYSTEMS: &[TagSystem] = &[
    TagSystem::ID3v2,
    TagSystem::VorbisComment,
    TagSystem::MP4,
    TagSystem::APEv2,
    TagSystem::ASF,
];

fuzz_target!(|data: &[u8]| {
    // Normalization functions operate on string data; skip non-UTF-8 inputs
    // since real-world tag values are always valid text.
    let Ok(input) = std::str::from_utf8(data) else {
        return;
    };

    // Cap input length to avoid spending excessive time on a single iteration.
    if input.len() > 4096 {
        return;
    }

    // Exercise genre resolution with arbitrary strings, including numeric
    // references, parenthesized groups, and free-text values.
    let _ = resolve_id3_genre(input);

    // Exercise track/disc splitting on slash-separated and bare values.
    let _ = normalize_track_disc(input);

    // Exercise boolean normalization across every tag system variant.
    for &system in TAG_SYSTEMS {
        let _ = normalize_boolean(input, system);
    }

    // Exercise date normalization across every tag system variant.
    for &system in TAG_SYSTEMS {
        let _ = normalize_date(input, system);
    }

    // Exercise track/disc combining with various argument combinations.
    let _ = combine_track_disc(Some(input), None);
    let _ = combine_track_disc(None, Some(input));
    let _ = combine_track_disc(Some(input), Some(input));
});
