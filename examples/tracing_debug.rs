//! Example: Debug audio file parsing with structured tracing
//!
//! This demonstrates how to use the `tracing` feature to get detailed
//! diagnostic output from audex's parsing and saving pipeline.
//!
//! # Running
//!
//! ```sh
//! cargo run --example tracing_debug --features tracing -- path/to/audio.mp3
//! ```
//!
//! # Controlling verbosity
//!
//! Set the `RUST_LOG` environment variable to adjust the detail level:
//!
//! ```sh
//! # Only errors and warnings
//! RUST_LOG=audex=warn cargo run --example tracing_debug --features tracing -- song.mp3
//!
//! # Operation lifecycle + parsed summaries
//! RUST_LOG=audex=debug cargo run --example tracing_debug --features tracing -- song.mp3
//!
//! # Full per-byte parsing details (very verbose)
//! RUST_LOG=audex=trace cargo run --example tracing_debug --features tracing -- song.mp3
//! ```

use audex::StreamInfo;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the tracing subscriber with an environment filter.
    // Defaults to "audex=debug" if RUST_LOG is not set.
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "audex=debug".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    // Grab the file path from command-line arguments
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("Usage: tracing_debug <audio-file>");
            eprintln!("       RUST_LOG=audex=trace tracing_debug <audio-file>");
            std::process::exit(1);
        }
    };

    println!("--- Loading: {} ---", path);
    let file = audex::File::load(&path)?;

    // Print basic info obtained from the loaded file
    println!("\nFormat : {}", file.format_name());
    println!("Tags   : {}", if file.has_tags() { "yes" } else { "no" });

    if let Some(info) = file.info().length() {
        println!("Length : {:.2}s", info.as_secs_f64());
    }

    // Show tag keys if present
    let keys = file.keys();
    if !keys.is_empty() {
        println!("\nTag keys ({}):", keys.len());
        for key in &keys {
            if let Some(values) = file.get(key) {
                println!("  {} = {:?}", key, values);
            }
        }
    }

    Ok(())
}
