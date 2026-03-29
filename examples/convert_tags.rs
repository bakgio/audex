//! Demonstrates cross-format tag conversion between audio files.
//!
//! Usage:
//!   cargo run --example convert_tags -- <source> <destination>
//!
//! Example:
//!   cargo run --example convert_tags -- song.mp3 song.flac

use audex::File;
use audex::tagmap::{ConversionOptions, StandardField};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn temp_output_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("converted-audio");
    path.with_file_name(format!("{file_name}.audex.tmp"))
}

fn replace_destination(
    temp_path: &Path,
    dest_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // Attempt rename directly. On Windows, rename fails when the destination
    // already exists, so we handle that by removing the destination and
    // retrying rather than checking existence first (avoids a race condition
    // between the check and the rename).
    match std::fs::rename(temp_path, dest_path) {
        Ok(()) => Ok(()),
        #[cfg(windows)]
        Err(_) => {
            std::fs::remove_file(dest_path)?;
            std::fs::rename(temp_path, dest_path)?;
            Ok(())
        }
        #[cfg(not(windows))]
        Err(e) => Err(e.into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <source> <destination>", args[0]);
        std::process::exit(1);
    }

    let source_path = PathBuf::from(&args[1]);
    let dest_path = PathBuf::from(&args[2]);

    if source_path == dest_path {
        return Err("source and destination must be different files".into());
    }

    // Load both files
    let source = File::load(&source_path)?;
    let mut dest = File::load(&dest_path)?;

    println!(
        "Source: {} ({})",
        source_path.display(),
        source.format_name()
    );
    println!(
        "Destination: {} ({})",
        dest_path.display(),
        dest.format_name()
    );
    println!("Writing changes through a temporary file before replacing the destination.");

    // Simple conversion: transfer all tags
    let report = dest.import_tags_from(&source)?;

    println!("\n--- Conversion Report ---");
    println!("Transferred: {} standard fields", report.transferred.len());
    for field in &report.transferred {
        println!("  + {}", field);
    }

    if !report.custom_transferred.is_empty() {
        println!("Custom fields: {}", report.custom_transferred.len());
        for key in &report.custom_transferred {
            println!("  + {}", key);
        }
    }

    if !report.skipped.is_empty() {
        println!("Skipped: {}", report.skipped.len());
        for (field, reason) in &report.skipped {
            println!("  - {} ({})", field, reason);
        }
    }

    // Save the destination file with the new tags
    let temp_path = temp_output_path(&dest_path);
    std::fs::copy(&dest_path, &temp_path)?;
    dest.save_to_path(&temp_path)?;
    replace_destination(&temp_path, &dest_path)?;
    println!("\nDestination saved successfully.");

    // Demonstrate conversion with options: only transfer title and artist
    println!("\n--- Selective Conversion (title + artist only) ---");
    let mut dest2 = File::load(&dest_path)?;
    let mut include = HashSet::new();
    include.insert(StandardField::Title);
    include.insert(StandardField::Artist);

    let options = ConversionOptions {
        include_fields: Some(include),
        transfer_custom: false,
        ..Default::default()
    };

    let report2 = dest2.import_tags_from_with_options(&source, &options)?;
    println!("Transferred: {} fields", report2.transferred.len());
    for field in &report2.transferred {
        println!("  + {}", field);
    }

    // Save is intentionally omitted here to keep the example focused on
    // demonstrating the import/conversion step with filtered fields.

    Ok(())
}
