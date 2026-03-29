//! Structured fuzz target for ID3v2 tags.
//!
//! Uses `arbitrary` to generate syntactically plausible ID3v2 headers and
//! frames, giving the fuzzer a head start at reaching deep parsing logic
//! rather than spending cycles discovering the "ID3" magic bytes.

#![no_main]

#[path = "helpers.rs"]
mod helpers;

use arbitrary::Arbitrary;
use audex::Tags;
use audex::id3;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct FuzzId3Input {
    version: u8,
    flags: u8,
    frames: Vec<FuzzFrame>,
    trailing: Vec<u8>,
}

#[derive(Arbitrary, Debug)]
struct FuzzFrame {
    id: [u8; 4],
    flags: u16,
    data: Vec<u8>,
}

fn encode_synchsafe(value: u32) -> [u8; 4] {
    [
        ((value >> 21) & 0x7F) as u8,
        ((value >> 14) & 0x7F) as u8,
        ((value >> 7) & 0x7F) as u8,
        (value & 0x7F) as u8,
    ]
}

fuzz_target!(|input: FuzzId3Input| {
    let mut frame_data = Vec::new();
    for frame in &input.frames {
        if frame.data.len() > 0xFFFF {
            continue;
        }
        frame_data.extend_from_slice(&frame.id);
        frame_data.extend_from_slice(&encode_synchsafe(frame.data.len() as u32));
        frame_data.push((frame.flags >> 8) as u8);
        frame_data.push((frame.flags & 0xFF) as u8);
        frame_data.extend_from_slice(&frame.data);
    }

    if frame_data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }

    let mut buf = Vec::with_capacity(10 + frame_data.len() + input.trailing.len());
    buf.extend_from_slice(b"ID3");
    buf.push(input.version.clamp(2, 4)); // version major (2, 3, or 4)
    buf.push(0); // version minor
    buf.push(input.flags);
    buf.extend_from_slice(&encode_synchsafe(frame_data.len() as u32));
    buf.extend_from_slice(&frame_data);
    buf.extend_from_slice(&input.trailing);

    let Some((_dir, path)) = helpers::write_temp_file(&buf, "mp3") else {
        return;
    };

    // Load, mutate, save, and reload to exercise the write-round-trip path
    if let Ok(mut tags) = id3::load(&path) {
        tags.set("TIT2", vec!["fuzz-title".to_string()]);
        let _ = tags.save(&path, 0, 4, None, None);
        let _ = id3::load(&path);
    }
});
