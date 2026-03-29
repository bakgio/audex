//! Shared helpers for fuzz targets.

use audex::Tags;
use std::io::Cursor;
use std::path::PathBuf;

/// Maximum input size accepted by fuzz targets (2 MiB).
#[allow(dead_code)]
pub const MAX_INPUT_SIZE: usize = 2 * 1024 * 1024;

/// Write fuzz data to a temporary file and return the path.
///
/// Returns `None` if the write fails (non-fatal for the fuzzer).
#[allow(dead_code)]
pub fn write_temp_file(data: &[u8], extension: &str) -> Option<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir().ok()?;
    let path = dir.path().join(format!("fuzz.{}", extension));
    std::fs::write(&path, data).ok()?;
    Some((dir, path))
}

/// Write fuzz data to a temporary file asynchronously and return the path.
#[allow(dead_code)]
pub async fn write_temp_file_async(
    data: &[u8],
    extension: &str,
) -> Option<(tempfile::TempDir, PathBuf)> {
    let dir = tempfile::tempdir().ok()?;
    let path = dir.path().join(format!("fuzz.{}", extension));
    tokio::fs::write(&path, data).await.ok()?;
    Some((dir, path))
}

/// Exercise the dynamic wrapper without assuming any particular format.
#[allow(dead_code)]
pub fn exercise_dynamic_file(file: &mut audex::DynamicFileType, original_bytes: Option<&[u8]>) {
    let _ = file.format_name();
    let _ = file.has_tags();
    let _ = file.info();
    let _ = file.keys();
    let _ = file.items();
    let _ = file.to_tag_map();

    let _ = file.set("title", vec!["fuzz-title".to_string()]);
    let _ = file.update(vec![
        ("artist".to_string(), vec!["fuzz-artist".to_string()]),
        ("album".to_string(), vec!["fuzz-album".to_string()]),
    ]);
    let _ = file.remove("artist");

    let _ = file.to_snapshot();
    let _ = file.to_snapshot_with_raw();

    // Build a modified tag set so diffs are against genuinely different data
    let baseline = file.items();
    let mut modified_items = baseline.clone();
    modified_items.push(("fuzz-extra".to_string(), vec!["present".to_string()]));
    let _ = audex::diff::diff_items(&baseline, &modified_items);
    let _ = audex::diff::diff_items(&modified_items, &baseline);

    let mut comparison = audex::BasicTags::new();
    for (key, values) in &baseline {
        comparison.set(key, values.clone());
    }
    let _ = comparison.set("fuzz-extra", vec!["present".to_string()]);
    let comparison_items = comparison.items();
    let _ = audex::diff::diff_items(&file.items(), &comparison_items);
    let _ = audex::diff::diff_items(&comparison_items, &file.items());
    let _ = audex::diff::diff_against_snapshot(file, &baseline);

    if let Some(bytes) = original_bytes {
        let mut save_cursor = Cursor::new(bytes.to_vec());
        let _ = file.save_to_writer(&mut save_cursor);

        let mut clear_cursor = Cursor::new(bytes.to_vec());
        let _ = file.clear_writer(&mut clear_cursor);
    }

    let _ = file.save();
    let _ = file.clear();
}

/// Async mirror of [`exercise_dynamic_file`].
#[allow(dead_code)]
pub async fn exercise_dynamic_file_async(
    file: &mut audex::DynamicFileType,
    original_bytes: Option<&[u8]>,
) {
    let _ = file.format_name();
    let _ = file.has_tags();
    let _ = file.info();
    let _ = file.keys();
    let _ = file.items();
    let _ = file.to_tag_map();

    let _ = file.set("title", vec!["fuzz-title".to_string()]);
    let _ = file.update(vec![
        ("artist".to_string(), vec!["fuzz-artist".to_string()]),
        ("album".to_string(), vec!["fuzz-album".to_string()]),
    ]);
    let _ = file.remove("artist");

    let _ = file.to_snapshot();
    let _ = file.to_snapshot_with_raw();

    // Build a modified tag set so diffs are against genuinely different data
    let baseline = file.items();
    let mut modified_items = baseline.clone();
    modified_items.push(("fuzz-extra".to_string(), vec!["present".to_string()]));
    let _ = audex::diff::diff_items(&baseline, &modified_items);
    let _ = audex::diff::diff_items(&modified_items, &baseline);

    let mut comparison = audex::BasicTags::new();
    for (key, values) in &baseline {
        comparison.set(key, values.clone());
    }
    let _ = comparison.set("fuzz-extra", vec!["present".to_string()]);
    let comparison_items = comparison.items();
    let _ = audex::diff::diff_items(&file.items(), &comparison_items);
    let _ = audex::diff::diff_items(&comparison_items, &file.items());
    let _ = audex::diff::diff_against_snapshot(file, &baseline);

    if let Some(bytes) = original_bytes {
        let mut save_cursor = Cursor::new(bytes.to_vec());
        let _ = file.save_to_writer(&mut save_cursor);

        let mut clear_cursor = Cursor::new(bytes.to_vec());
        let _ = file.clear_writer(&mut clear_cursor);
    }

    let _ = file.save_async().await;
    let _ = file.clear_async().await;
}
