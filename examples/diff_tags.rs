//! Demonstrates the tag-diffing API.
//!
//! Usage:
//!   cargo run --example diff_tags -- <file_a> <file_b>
//!
//! Compares the metadata of two audio files and prints the results
//! in several formats.

use audex::File;
use audex::diff::{self, DiffOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <file_a> <file_b>", args[0]);
        std::process::exit(1);
    }

    let file_a = File::load(&args[1])?;
    let file_b = File::load(&args[2])?;

    // --- Basic diff ---
    println!("=== Basic diff ===");
    let d = diff::diff(&file_a, &file_b);
    if d.is_identical() {
        println!("Tags are identical.");
    } else {
        println!("{}", d);
    }

    // --- Summary ---
    println!("\n=== Summary ===");
    println!("{}", d.summary());

    // --- Pretty-print (right-aligned keys) ---
    println!("\n=== Pretty-print ===");
    print!("{}", d.pprint());

    // --- Diff with options (stream info + case-insensitive) ---
    println!("\n=== With options (stream info + case-insensitive) ===");
    let opts = DiffOptions {
        compare_stream_info: true,
        case_insensitive_keys: true,
        include_unchanged: true,
        ..Default::default()
    };
    let d2 = diff::diff_with_options(&file_a, &file_b, &opts);
    println!("{}", d2);

    // --- Full pretty-print (including unchanged) ---
    println!("\n=== Full pretty-print ===");
    print!("{}", d2.pprint_full());

    // --- Snapshot-based diffing ---
    println!("\n=== Snapshot-based diff ===");
    let snapshot = diff::snapshot_tags(&file_a);
    let d3 = diff::diff_against_snapshot(&file_b, &snapshot);
    println!("{}", d3.summary());

    // --- Filtering ---
    println!("\n=== Filtered diff (artist only) ===");
    let filtered = d.filter_keys(&["artist", "ARTIST", "TPE1", "\u{00a9}ART"]);
    if filtered.diff_count() == 0 {
        println!("No artist differences.");
    } else {
        println!("{}", filtered);
    }

    // --- Convenience methods ---
    println!("\n=== Convenience method: file_a.diff_tags(&file_b) ===");
    let d4 = file_a.diff_tags(&file_b);
    println!("{}", d4.summary());

    Ok(())
}
