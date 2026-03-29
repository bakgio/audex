//! Common test utilities for consistent test data access.

use std::fs;
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

/// Test utilities for consistent test data access
#[allow(dead_code)]
pub struct TestUtils;

#[allow(dead_code)]
impl TestUtils {
    /// Get path to test data files
    pub fn data_path(filename: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(filename)
    }

    /// Create a temporary copy of a test file
    pub fn get_temp_copy<P: AsRef<Path>>(path: P) -> io::Result<NamedTempFile> {
        let source = fs::read(path)?;
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(&source)?;
        temp_file.flush()?;
        temp_file.seek(SeekFrom::Start(0))?;
        Ok(temp_file)
    }

    /// Create test data with provided content
    pub fn create_test_data(data: &[u8]) -> io::Result<NamedTempFile> {
        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(data)?;
        temp_file.flush()?;
        temp_file.seek(SeekFrom::Start(0))?;
        Ok(temp_file)
    }

    /// Assert that floating point values are approximately equal
    pub fn assert_almost_equal(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() < tolerance,
            "Values not almost equal: {} vs {} (tolerance: {})",
            actual,
            expected,
            tolerance
        );
    }
}
