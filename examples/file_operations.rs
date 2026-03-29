//! Demonstrates tag operations using the audex `File` type.
//!
//! The `File` type provides a unified interface for reading and writing
//! metadata tags across all supported audio formats (MP3, FLAC, MP4/M4A,
//! Ogg Vorbis, Ogg Opus, WAVE, AIFF, DSDIFF, DSF, and more).
//!
//! This example covers both synchronous and asynchronous workflows:
//! - Loading a file and initializing tags
//! - Setting, getting, and removing individual tags
//! - Setting multiple tags at once
//! - Selectively removing one tag while keeping others
//! - Clearing all tags
//!
//! # Usage
//!
//! ```sh
//! # Sync operations only
//! cargo run --example file_operations -- <audio_file> sync
//!
//! # Async operations only (requires "async" feature)
//! cargo run --example file_operations --features async -- <audio_file> async
//!
//! # Both sync and async (default)
//! cargo run --example file_operations --features async -- <audio_file>
//! ```

use audex::File;
use std::error::Error;
use std::path::Path;

/// Copies the given audio file into a temporary directory so that destructive
/// operations (set, remove, clear) do not modify the user's original file.
/// Returns the path to the temporary copy. The caller must keep the returned
/// `TempDir` alive for the duration of the demo — dropping it deletes the copy.
fn copy_to_temp(original: &str) -> Result<(tempfile::TempDir, String), Box<dyn Error>> {
    let src = Path::new(original);
    let file_name = src.file_name().ok_or("input path has no file name")?;
    let tmp_dir = tempfile::tempdir()?;
    let dest = tmp_dir.path().join(file_name);
    std::fs::copy(src, &dest)?;
    let dest_str = dest.to_string_lossy().into_owned();
    Ok((tmp_dir, dest_str))
}

/// Synchronous tag operations using `File`.
fn run_sync(file_path: &str) -> Result<(), Box<dyn Error>> {
    println!("\n--- Sync: Load and set a tag ---");
    let mut tagger = File::load(file_path)?;

    if !tagger.has_tags() {
        tagger.add_tags()?;
    }

    tagger.set("title", vec!["Example Title".to_string()])?;
    tagger.save()?;

    let tagger = File::load(file_path)?;
    println!("title = {:?}", tagger.get("title"));

    println!("\n--- Sync: Remove a tag ---");
    let mut tagger = File::load(file_path)?;
    tagger.remove("title")?;
    tagger.save()?;

    let tagger = File::load(file_path)?;
    println!("title after remove = {:?}", tagger.get("title"));

    println!("\n--- Sync: Set multiple tags ---");
    let mut tagger = File::load(file_path)?;

    if !tagger.has_tags() {
        tagger.add_tags()?;
    }

    tagger.set("title", vec!["Track One".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save()?;

    let tagger = File::load(file_path)?;
    println!("title  = {:?}", tagger.get("title"));
    println!("artist = {:?}", tagger.get("artist"));

    println!("\n--- Sync: Selective removal ---");
    let mut tagger = File::load(file_path)?;
    tagger.remove("title")?;
    tagger.save()?;

    let tagger = File::load(file_path)?;
    println!("title  = {:?} (removed)", tagger.get("title"));
    println!("artist = {:?} (kept)", tagger.get("artist"));

    println!("\n--- Sync: Clear all tags ---");
    let mut tagger = File::load(file_path)?;
    tagger.clear()?;

    let tagger = File::load(file_path)?;
    println!("remaining keys = {:?}", tagger.keys());

    Ok(())
}

/// Asynchronous tag operations using `File`.
#[cfg(feature = "async")]
async fn run_async(file_path: &str) -> Result<(), Box<dyn Error>> {
    println!("\n--- Async: Load and set a tag ---");
    let mut tagger = File::load_async(file_path).await?;

    if !tagger.has_tags() {
        tagger.add_tags()?;
    }

    tagger.set("title", vec!["Example Title".to_string()])?;
    tagger.save_async().await?;

    let tagger = File::load_async(file_path).await?;
    println!("title = {:?}", tagger.get("title"));

    println!("\n--- Async: Remove a tag ---");
    let mut tagger = File::load_async(file_path).await?;
    tagger.remove("title")?;
    tagger.save_async().await?;

    let tagger = File::load_async(file_path).await?;
    println!("title after remove = {:?}", tagger.get("title"));

    println!("\n--- Async: Set multiple tags ---");
    let mut tagger = File::load_async(file_path).await?;

    if !tagger.has_tags() {
        tagger.add_tags()?;
    }

    tagger.set("title", vec!["Track One".to_string()])?;
    tagger.set("artist", vec!["Some Artist".to_string()])?;
    tagger.save_async().await?;

    let tagger = File::load_async(file_path).await?;
    println!("title  = {:?}", tagger.get("title"));
    println!("artist = {:?}", tagger.get("artist"));

    println!("\n--- Async: Selective removal ---");
    let mut tagger = File::load_async(file_path).await?;
    tagger.remove("title")?;
    tagger.save_async().await?;

    let tagger = File::load_async(file_path).await?;
    println!("title  = {:?} (removed)", tagger.get("title"));
    println!("artist = {:?} (kept)", tagger.get("artist"));

    println!("\n--- Async: Clear all tags ---");
    let mut tagger = File::load_async(file_path).await?;
    tagger.clear_async().await?;

    let tagger = File::load_async(file_path).await?;
    println!("remaining keys = {:?}", tagger.keys());

    Ok(())
}

#[cfg(feature = "async")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <audio_file_path> [mode]", args[0]);
        eprintln!();
        eprintln!("Modes:");
        eprintln!("  sync  - Run synchronous operations only");
        eprintln!("  async - Run asynchronous operations only");
        eprintln!("  both  - Run both (default)");
        std::process::exit(1);
    }

    let file_path = &args[1];
    let mode = if args.len() > 2 {
        args[2].to_lowercase()
    } else {
        "both".to_string()
    };

    if mode != "sync" && mode != "async" && mode != "both" {
        eprintln!(
            "Error: Invalid mode '{}'. Must be 'sync', 'async', or 'both'",
            mode
        );
        std::process::exit(1);
    }

    if !std::path::Path::new(file_path).exists() {
        eprintln!("Error: File not found: {}", file_path);
        std::process::exit(1);
    }

    // WARNING: This example performs destructive tag operations (set, remove,
    // clear). To protect the original file we work on a temporary copy.
    let (_tmp_dir, tmp_path) = copy_to_temp(file_path)?;
    println!("Working on temporary copy: {}", tmp_path);

    if mode == "sync" || mode == "both" {
        println!("=== Synchronous Operations ===");
        run_sync(&tmp_path)?;
    }

    if mode == "async" || mode == "both" {
        println!("\n=== Asynchronous Operations ===");
        run_async(&tmp_path).await?;
    }

    println!("\nDone. Original file was not modified.");
    Ok(())
}

#[cfg(not(feature = "async"))]
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <audio_file_path> [mode]", args[0]);
        eprintln!();
        eprintln!("Modes:");
        eprintln!("  sync  - Run synchronous operations only");
        std::process::exit(1);
    }

    let file_path = &args[1];

    if !std::path::Path::new(file_path).exists() {
        eprintln!("Error: File not found: {}", file_path);
        std::process::exit(1);
    }

    // WARNING: This example performs destructive tag operations (set, remove,
    // clear). To protect the original file we work on a temporary copy.
    let (_tmp_dir, tmp_path) = copy_to_temp(file_path)?;
    println!("Working on temporary copy: {}", tmp_path);

    println!("=== Synchronous Operations ===");
    run_sync(&tmp_path)?;

    println!("\nDone. Original file was not modified.");
    Ok(())
}
