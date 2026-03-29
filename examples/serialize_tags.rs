//! Example: Serialize audio metadata to JSON and TOML
//!
//! Demonstrates how to use the `serde` feature to convert audio
//! metadata into machine-readable formats suitable for web APIs,
//! config files, or database storage.
//!
//! Run with:
//!   cargo run --example serialize_tags --features serde -- path/to/audio.mp3

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "serde")]
    {
        let path = match std::env::args().nth(1) {
            Some(p) => p,
            None => {
                eprintln!("Usage: serialize_tags <audio_file>");
                eprintln!();
                eprintln!("Serializes audio metadata to JSON and TOML formats.");
                std::process::exit(1);
            }
        };

        // Load the audio file (format is auto-detected)
        let file = audex::File::load(&path)?;

        // Build a format-agnostic snapshot of all metadata
        let snapshot = file.to_snapshot();

        // Pretty-print as JSON
        let json = serde_json::to_string_pretty(&snapshot)?;
        println!("--- JSON ---");
        println!("{}", json);

        // Serialize to TOML (raw_tags must be None for TOML compatibility)
        let toml_str = toml::to_string_pretty(&snapshot)?;
        println!("\n--- TOML ---");
        println!("{}", toml_str);

        // Demonstrate round-trip: deserialize the JSON back
        let restored: audex::TagSnapshot = serde_json::from_str(&json)?;
        println!("\n--- Round-trip verified ---");
        println!("Format:  {}", restored.format);
        println!("Tags:    {} entries", restored.tags.len());
    }

    #[cfg(not(feature = "serde"))]
    {
        eprintln!("This example requires the `serde` feature.");
        eprintln!("Run with: cargo run --example serialize_tags --features serde -- <file>");
    }

    Ok(())
}
