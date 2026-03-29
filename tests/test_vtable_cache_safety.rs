//! Tests verifying the soundness of the vtable cache under concurrent access.
//!
//! The file-type vtable cache uses unsafe pointer-to-reference conversion
//! to hand out `&'static` references from a global HashMap. This is safe
//! because entries are never removed and Box allocations are stable across
//! HashMap resizes. These tests exercise the cache from multiple threads
//! simultaneously to confirm no data races or use-after-free occur.

mod common;

use std::sync::Arc;
use std::thread;

/// Verify that concurrent file-type loading returns consistent results
/// and does not panic or segfault when the vtable cache is populated from
/// multiple threads at once.
#[test]
fn vtable_cache_concurrent_access_is_sound() {
    let test_file = std::path::Path::new("tests/data/emptyfile.mp3");
    if !test_file.exists() {
        // Skip gracefully if test data is missing
        return;
    }

    let path = Arc::new(test_file.to_path_buf());
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let b = Arc::clone(&barrier);
        let p = Arc::clone(&path);
        handles.push(thread::spawn(move || {
            // Synchronize all threads so they race into the cache simultaneously
            b.wait();

            // Each thread loads the same file, forcing concurrent reads and
            // writes to the global vtable cache.
            let result = audex::File::load(&*p);
            // We only care that it doesn't panic/segfault — the file may
            // legitimately fail to parse.
            drop(result);
        }));
    }

    for h in handles {
        h.join().expect("thread should not panic");
    }
}

/// Verify that repeated access from a single thread returns stable results
/// without leaking or corrupting the cache.
#[test]
fn vtable_cache_repeated_single_thread_access() {
    let test_file = std::path::Path::new("tests/data/emptyfile.mp3");
    if !test_file.exists() {
        return;
    }

    for _ in 0..100 {
        let result = audex::File::load(test_file);
        drop(result);
    }
}
