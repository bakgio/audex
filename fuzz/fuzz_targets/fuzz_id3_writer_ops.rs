#![no_main]

#[path = "helpers.rs"]
mod helpers;

use audex::FileType;
use audex::id3::frames::ChannelType;
use audex::id3::{APIC, PictureType, RVA2, TextEncoding};
use audex::mp3::MP3;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > helpers::MAX_INPUT_SIZE {
        return;
    }

    let Some((_dir, path)) = helpers::write_temp_file(data, "mp3") else {
        return;
    };

    if let Ok(mut file) = MP3::load(&path) {
        if file.tags.is_none() {
            let _ = file.add_tags();
        }

        if let Some(tags) = file.tags.as_mut() {
            let _ = tags.set_with_encoding(
                "TXXX:FUZZ",
                vec!["fuzz-value".to_string()],
                TextEncoding::Utf8,
            );

            let picture = APIC {
                encoding: TextEncoding::Utf16,
                mime: "image/jpeg".to_string(),
                type_: PictureType::CoverFront,
                desc: "Fuzz Cover".to_string(),
                data: vec![0xFF, 0xD8, 0xFF, 0xD9],
            };
            let _ = tags.add(Box::new(picture));

            let mut replay_gain = RVA2::new("track".to_string(), Vec::new());
            let _ = replay_gain.add_channel(ChannelType::MasterVolume, -6.0, 0.95);
            let _ = tags.add(Box::new(replay_gain));
        }

        if file.save().is_ok() {
            let _ = MP3::load(&path);
        }
    }
});
