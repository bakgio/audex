//! Demonstrates reading metadata tags and stream information from audio files.
//!
//! Audex supports two approaches for reading tags:
//!
//! 1. **Unified `File` interface** — auto-detects format, provides `keys()`/`get()`
//!    for tag access and `info()` for stream properties. Handles any supported format.
//!
//! 2. **Direct format types** — load a specific format (e.g. `FLAC::load`) for
//!    format-specific access to internal tag structures.
//!
//! You can also combine both: load via `File` and `downcast_ref` to the concrete type.
//!
//! # Usage
//!
//! ```sh
//! cargo run --example read_tags -- <audio_file>
//! ```

use audex::File;
use audex::FileType;
use audex::StreamInfo;
use std::error::Error;

/// Read tags and stream info using the unified `File` interface.
///
/// This works with any supported format — audex detects the format automatically.
fn read_with_file(file_path: &str) -> Result<(), Box<dyn Error>> {
    let audio = File::load(file_path)?;

    println!("Format: {}", audio.format_name());

    let info = audio.info();
    if let Some(length) = info.length() {
        println!("Duration: {:.2}s", length.as_secs_f64());
    }
    if let Some(bitrate) = info.bitrate() {
        println!("Bitrate: {} bps", bitrate);
    }
    if let Some(sample_rate) = info.sample_rate() {
        println!("Sample rate: {} Hz", sample_rate);
    }
    if let Some(channels) = info.channels() {
        println!("Channels: {}", channels);
    }
    if let Some(bps) = info.bits_per_sample() {
        println!("Bits per sample: {}", bps);
    }

    let keys = audio.keys();
    println!("\nTags ({}):", keys.len());
    for key in &keys {
        if let Some(values) = audio.get(key) {
            println!("  {} = {:?}", key, values);
        }
    }

    Ok(())
}

/// Read tags by loading a specific format type directly.
///
/// This gives access to format-specific internals. Here we show FLAC
/// (VorbisComment-based tags) and MP3 (ID3v2 frames) as examples.
fn read_with_direct_type(file_path: &str) -> Result<(), Box<dyn Error>> {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "flac" => {
            use audex::flac::FLAC;

            let flac = FLAC::load(file_path)?;

            println!("Sample rate: {:?}", flac.info.sample_rate());
            println!("Channels: {:?}", flac.info.channels());

            // FLAC uses VorbisComment — keys()/get() return Vec<String>
            for key in flac.keys() {
                if let Some(values) = flac.get(&key) {
                    println!("  {} = {:?}", key, values);
                }
            }
        }
        "mp3" => {
            use audex::mp3::MP3;

            let mp3 = MP3::load(file_path)?;

            println!("Bitrate: {:?}", mp3.info.bitrate());
            println!("Sample rate: {:?}", mp3.info.sample_rate());

            // MP3 uses ID3v2 — tags are stored as Frame objects in a map
            if let Some(ref tags) = mp3.tags {
                for (key, frame) in tags.dict.iter() {
                    if let Some(text) = frame.text_values() {
                        println!("  {} = {:?}", key, text);
                    } else {
                        println!("  {} = <non-text frame>", key);
                    }
                }
            }
        }
        "m4a" | "mp4" | "m4b" => {
            use audex::mp4::MP4;

            let mp4 = MP4::load(file_path)?;

            println!("Sample rate: {:?}", mp4.info.sample_rate());
            println!("Channels: {:?}", mp4.info.channels());

            // MP4 tags have standard atoms, cover art, and freeform tags
            if let Some(ref tags) = mp4.tags {
                for (key, values) in tags.tags.iter() {
                    println!("  {} = {:?}", key, values);
                }
                if !tags.covers.is_empty() {
                    println!("  covr = [{} image(s)]", tags.covers.len());
                }
                for (key, freeforms) in tags.freeforms.iter() {
                    println!("  {} = [{} freeform value(s)]", key, freeforms.len());
                }
            }
        }
        _ => {
            println!(
                "Direct-type example not shown for .{} — use the File interface instead",
                ext
            );
        }
    }

    Ok(())
}

/// Demonstrates loading via `File` and then downcasting to a concrete format type.
///
/// This is useful when you want format auto-detection but also need
/// access to format-specific fields.
fn read_with_downcast(file_path: &str) -> Result<(), Box<dyn Error>> {
    let audio = File::load(file_path)?;
    let format = audio.format_name();

    match format.split("::").last().unwrap_or("") {
        "FLAC" => {
            use audex::flac::FLAC;
            if let Some(flac) = audio.downcast_ref::<FLAC>() {
                println!("FLAC bits per sample: {:?}", flac.info.bits_per_sample());
                for key in flac.keys() {
                    if let Some(values) = flac.get(&key) {
                        println!("  {} = {:?}", key, values);
                    }
                }
            }
        }
        "MP3" => {
            use audex::mp3::MP3;
            if let Some(mp3) = audio.downcast_ref::<MP3>() {
                println!("MP3 bitrate: {:?}", mp3.info.bitrate());
                if let Some(ref tags) = mp3.tags {
                    for (key, frame) in tags.dict.iter() {
                        if let Some(text) = frame.text_values() {
                            println!("  {} = {:?}", key, text);
                        }
                    }
                }
            }
        }
        other => {
            // For any format, the unified interface still works
            println!("No downcast example for {} — showing unified keys:", other);
            for key in audio.keys() {
                if let Some(values) = audio.get(&key) {
                    println!("  {} = {:?}", key, values);
                }
            }
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <audio_file>", args[0]);
        eprintln!();
        eprintln!("Reads and prints metadata tags and stream info from an audio file.");
        eprintln!("Supports MP3, FLAC, MP4/M4A, Ogg Vorbis, Ogg Opus, WAVE, AIFF, DSF, and more.");
        std::process::exit(1);
    }

    let file_path = &args[1];

    if !std::path::Path::new(file_path).exists() {
        eprintln!("Error: File not found: {}", file_path);
        std::process::exit(1);
    }

    println!("=== Unified File Interface ===");
    read_with_file(file_path)?;

    println!("\n=== Direct Format Type ===");
    read_with_direct_type(file_path)?;

    println!("\n=== File + Downcast ===");
    read_with_downcast(file_path)?;

    Ok(())
}
