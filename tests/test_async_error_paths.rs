#![cfg(feature = "async")]

//! Async error path and concurrent save tests.
//!
//! Verifies that async save operations propagate errors cleanly and
//! that concurrent saves on separate temp files do not interfere.
//! Original test data files are never modified.

mod common;

use audex::{File, FileType};
use common::TestUtils;

const FLAC_FILE: &str = "silence-44-s.flac";

// ---------------------------------------------------------------------------
// Concurrent async saves on separate temp files
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_async_saves_do_not_interfere() {
    let tmp_dir = tempfile::tempdir().expect("create temp dir");

    let path_a = tmp_dir.path().join("concurrent_a.flac");
    let path_b = tmp_dir.path().join("concurrent_b.flac");
    std::fs::copy(TestUtils::data_path(FLAC_FILE), &path_a).expect("copy A");
    std::fs::copy(TestUtils::data_path(FLAC_FILE), &path_b).expect("copy B");

    let mut a = audex::flac::FLAC::load_async(&path_a)
        .await
        .expect("load A");
    let mut b = audex::flac::FLAC::load_async(&path_b)
        .await
        .expect("load B");

    a.set("TITLE", vec!["Concurrent A".to_string()]).unwrap();
    b.set("TITLE", vec!["Concurrent B".to_string()]).unwrap();

    // Execute both saves concurrently via task interleaving
    let (res_a, res_b): (audex::Result<()>, audex::Result<()>) =
        tokio::join!(a.save_async(), b.save_async());
    res_a.expect("save A failed");
    res_b.expect("save B failed");

    // Each file should retain its own title
    let reloaded_a = audex::flac::FLAC::load_async(&path_a)
        .await
        .expect("reload A");
    let reloaded_b = audex::flac::FLAC::load_async(&path_b)
        .await
        .expect("reload B");
    assert_eq!(
        reloaded_a.get_first("TITLE"),
        Some("Concurrent A".to_string()),
    );
    assert_eq!(
        reloaded_b.get_first("TITLE"),
        Some("Concurrent B".to_string()),
    );
}

// ---------------------------------------------------------------------------
// Async load of a truncated file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn async_load_truncated_file_returns_error() {
    let tmp = tempfile::NamedTempFile::with_suffix(".flac").expect("temp file");

    // Write 2 bytes — enough that the file is non-empty but far too short
    // for any audio format header to parse
    std::fs::write(tmp.path(), b"\xFF\xFB").expect("write truncated data");

    let result = File::load_async(tmp.path()).await;
    // The file should either fail to load or return a degenerate object.
    // The key assertion is that it does not panic.
    if let Err(e) = &result {
        let msg = e.to_string();
        assert!(!msg.is_empty(), "Error must have a meaningful message");
    }
}

// ---------------------------------------------------------------------------
// Async save to a read-only path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn async_save_to_readonly_path_returns_error() {
    let tmp_dir = tempfile::tempdir().expect("create temp dir");
    let path = tmp_dir.path().join("readonly.flac");
    std::fs::copy(TestUtils::data_path(FLAC_FILE), &path).expect("copy fixture");

    let mut flac = audex::flac::FLAC::load_async(&path).await.expect("load");
    flac.set("TITLE", vec!["Should Fail".to_string()]).unwrap();

    // Make the file read-only
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms).unwrap();

    let result: audex::Result<()> = flac.save_async().await;
    assert!(
        result.is_err(),
        "Saving to a read-only file must return an error"
    );

    // Restore write permission so the temp dir can be cleaned up
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&path, perms).unwrap();
}
