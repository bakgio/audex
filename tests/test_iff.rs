//! Tests for IFF functionality

use audex::iff::is_valid_chunk_id;

#[test]
fn test_is_valid_chunk_id() {
    assert!(!is_valid_chunk_id(""));
    assert!(is_valid_chunk_id("QUUX"));
    assert!(!is_valid_chunk_id("FOOBAR"));
}

// ---------------------------------------------------------------------------
// IFF insert_bytes allocation limit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod iff_insert_alloc_tests {
    use audex::iff::insert_bytes;
    use std::fs::OpenOptions;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Requesting a massive insert should fail with an error, not attempt
    /// to allocate gigabytes of zeroes.
    #[test]
    fn test_insert_bytes_rejects_huge_size() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0xAA; 100]).unwrap();
        temp.flush().unwrap();

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(temp.path())
            .unwrap();

        // Request 1 GB insert — should be rejected before allocation
        let huge_size = 1_073_741_824u32; // 1 GB
        let result = insert_bytes(&mut file, huge_size, 0);

        assert!(
            result.is_err(),
            "insert_bytes should reject a 1 GB allocation request"
        );
    }

    /// Normal small inserts must still work correctly.
    #[test]
    fn test_insert_bytes_normal_case_works() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0xBB; 50]).unwrap();
        temp.flush().unwrap();

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(temp.path())
            .unwrap();

        // Insert 10 bytes at position 10 — small and reasonable
        let result = insert_bytes(&mut file, 10, 10);
        assert!(result.is_ok(), "Normal small insert should succeed");
    }

    /// Zero-byte insert should succeed trivially.
    #[test]
    fn test_insert_bytes_zero_size() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0xCC; 20]).unwrap();
        temp.flush().unwrap();

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(temp.path())
            .unwrap();

        let result = insert_bytes(&mut file, 0, 0);
        assert!(result.is_ok(), "Zero-byte insert should succeed");
    }
}

// ---------------------------------------------------------------------------
// Async byte-manipulation parity tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "async"))]
mod iff_async_byte_ops {
    use audex::{delete_bytes_async, insert_bytes_async, resize_bytes_async};
    use std::io::Write;
    use tempfile::NamedTempFile;
    use tokio::fs::OpenOptions;
    use tokio::io::{AsyncReadExt, AsyncSeekExt};

    /// Helper: create a temp file with known content and return the path.
    fn make_temp(content: &[u8]) -> NamedTempFile {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(content).unwrap();
        tmp.flush().unwrap();
        tmp
    }

    #[tokio::test]
    async fn test_insert_bytes_async_grows_file() {
        let original = [0xAA; 50];
        let tmp = make_temp(&original);

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(tmp.path())
            .await
            .unwrap();

        // Insert 10 bytes at offset 20
        insert_bytes_async(&mut file, 10, 20, None)
            .await
            .expect("insert should succeed");

        let meta = tokio::fs::metadata(tmp.path()).await.unwrap();
        assert_eq!(meta.len(), 60, "File should grow by 10 bytes");

        // Data before the insertion point should be unchanged
        file.seek(std::io::SeekFrom::Start(0)).await.unwrap();
        let mut buf = vec![0u8; 20];
        file.read_exact(&mut buf).await.unwrap();
        assert!(buf.iter().all(|&b| b == 0xAA), "Prefix should be intact");
    }

    #[tokio::test]
    async fn test_delete_bytes_async_shrinks_file() {
        let original = [0xBB; 80];
        let tmp = make_temp(&original);

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(tmp.path())
            .await
            .unwrap();

        // Delete 20 bytes starting at offset 10
        delete_bytes_async(&mut file, 20, 10, None)
            .await
            .expect("delete should succeed");

        let meta = tokio::fs::metadata(tmp.path()).await.unwrap();
        assert_eq!(meta.len(), 60, "File should shrink by 20 bytes");
    }

    #[tokio::test]
    async fn test_resize_bytes_async_grow() {
        let original = [0xCC; 40];
        let tmp = make_temp(&original);

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(tmp.path())
            .await
            .unwrap();

        // Grow a 10-byte region at offset 5 to 25 bytes
        resize_bytes_async(&mut file, 10, 25, 5)
            .await
            .expect("resize grow should succeed");

        let meta = tokio::fs::metadata(tmp.path()).await.unwrap();
        assert_eq!(meta.len(), 55, "File should grow by 15 bytes (25-10)");
    }

    #[tokio::test]
    async fn test_resize_bytes_async_shrink() {
        let original = [0xDD; 60];
        let tmp = make_temp(&original);

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(tmp.path())
            .await
            .unwrap();

        // Shrink a 30-byte region at offset 10 to 10 bytes
        resize_bytes_async(&mut file, 30, 10, 10)
            .await
            .expect("resize shrink should succeed");

        let meta = tokio::fs::metadata(tmp.path()).await.unwrap();
        assert_eq!(meta.len(), 40, "File should shrink by 20 bytes (30-10)");
    }

    #[tokio::test]
    async fn test_async_matches_sync_insert() {
        // Verify async insert produces the same result as sync insert
        let content = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

        // Sync version
        let tmp_sync = make_temp(content);
        {
            let mut f = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(tmp_sync.path())
                .unwrap();
            audex::iff::insert_bytes(&mut f, 5, 10).unwrap();
        }
        let sync_result = std::fs::read(tmp_sync.path()).unwrap();

        // Async version
        let tmp_async = make_temp(content);
        {
            let mut f = OpenOptions::new()
                .read(true)
                .write(true)
                .open(tmp_async.path())
                .await
                .unwrap();
            insert_bytes_async(&mut f, 5, 10, None).await.unwrap();
        }
        let async_result = std::fs::read(tmp_async.path()).unwrap();

        assert_eq!(
            sync_result.len(),
            async_result.len(),
            "Sync and async should produce same file size"
        );
        // Bytes outside the inserted region should match
        assert_eq!(
            &sync_result[..10],
            &async_result[..10],
            "Prefix should match between sync and async"
        );
        assert_eq!(
            &sync_result[15..],
            &async_result[15..],
            "Suffix should match between sync and async"
        );
    }
}
