//! Stale-tail tests for additional formats not covered by the main suite.
//!
//! When metadata is removed via `clear_writer`, the output may be shorter
//! than the original input. These tests verify that the trailing bytes
//! beyond the cursor position are zeroed, preventing data leakage.

use audex::FileType;
use audex::asf::ASF;
use audex::dsf::DSF;
use audex::musepack::Musepack;
use std::io::Cursor;
use std::path::PathBuf;

fn data_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join(name)
}

/// Count non-zero bytes in the region after `logical_end`.
fn trailing_non_zero_count(data: &[u8], logical_end: usize) -> usize {
    data[logical_end..].iter().filter(|&&b| b != 0).count()
}

/// For formats whose test files lack tags, add a tag, save back into the
/// original buffer, then return that buffer so `clear_writer` has
/// something to clear.
fn create_tagged_buffer<T: FileType>(path: &str, key: &str, value: &str) -> Option<Vec<u8>> {
    let test_file = data_path(path);
    if !test_file.exists() {
        return None;
    }

    let original_data = std::fs::read(&test_file).unwrap();
    let mut load_cursor = Cursor::new(original_data.clone());
    let mut file = T::load_from_reader(&mut load_cursor).unwrap();

    if file.tags().is_none() {
        file.add_tags().unwrap();
    }
    file.set(key, vec![value.to_string()]).unwrap();

    // save_to_writer operates on a cursor seeded with the original data
    let mut save_cursor = Cursor::new(original_data);
    file.save_to_writer(&mut save_cursor).unwrap();
    Some(save_cursor.into_inner())
}

// ---------------------------------------------------------------------------
// ASF / WMA
// ---------------------------------------------------------------------------

#[test]
fn asf_clear_writer_no_stale_trailing_content() {
    let test_file = data_path("silence-1.wma");
    if !test_file.exists() {
        eprintln!("Skipping: silence-1.wma not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();
    let mut load_cursor = Cursor::new(original_data.clone());
    let mut asf = ASF::load_from_reader(&mut load_cursor).unwrap();
    assert!(!asf.keys().is_empty(), "Test file must have tags");

    let mut out = Cursor::new(original_data);
    asf.clear_writer(&mut out).unwrap();

    let cursor_pos = out.position() as usize;
    let output = out.into_inner();

    if cursor_pos < output.len() {
        let non_zero = trailing_non_zero_count(&output, cursor_pos);
        assert_eq!(
            non_zero, 0,
            "Found {} non-zero stale bytes after position {} in ASF output",
            non_zero, cursor_pos
        );
    }
}

// ---------------------------------------------------------------------------
// DSF
// ---------------------------------------------------------------------------

#[test]
fn dsf_clear_writer_no_stale_trailing_content() {
    let test_file = data_path("with-id3.dsf");
    if !test_file.exists() {
        eprintln!("Skipping: with-id3.dsf not found");
        return;
    }

    let original_data = std::fs::read(&test_file).unwrap();
    let mut load_cursor = Cursor::new(original_data.clone());
    let mut dsf = DSF::load_from_reader(&mut load_cursor).unwrap();
    assert!(dsf.tags().is_some(), "with-id3.dsf must have tags");

    let mut out = Cursor::new(original_data);
    dsf.clear_writer(&mut out).unwrap();

    let cursor_pos = out.position() as usize;
    let output = out.into_inner();

    if cursor_pos < output.len() {
        let non_zero = trailing_non_zero_count(&output, cursor_pos);
        assert_eq!(
            non_zero, 0,
            "Found {} non-zero stale bytes after position {} in DSF output",
            non_zero, cursor_pos
        );
    }
}

// ---------------------------------------------------------------------------
// Musepack
// ---------------------------------------------------------------------------

#[test]
fn musepack_clear_writer_no_stale_trailing_content() {
    // click.mpc ships without APE tags, so add one first.
    let tagged = create_tagged_buffer::<Musepack>("click.mpc", "Title", "stale-tail-test");
    let tagged = match tagged {
        Some(buf) => buf,
        None => {
            eprintln!("Skipping: click.mpc not found");
            return;
        }
    };

    let mut load_cursor = Cursor::new(tagged.clone());
    let mut mpc = Musepack::load_from_reader(&mut load_cursor).unwrap();

    let mut out = Cursor::new(tagged);
    mpc.clear_writer(&mut out).unwrap();

    let cursor_pos = out.position() as usize;
    let output = out.into_inner();

    if cursor_pos < output.len() {
        let non_zero = trailing_non_zero_count(&output, cursor_pos);
        assert_eq!(
            non_zero, 0,
            "Found {} non-zero stale bytes after position {} in Musepack output",
            non_zero, cursor_pos
        );
    }
}
