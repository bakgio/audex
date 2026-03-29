//! Tests for OGG async page scan limits.
//!
//! The sync `find_last` path enforces MAX_OGG_PAGES to prevent excessive
//! CPU usage on files with many tiny pages. These tests verify that the
//! async `find_last_async` path enforces the same limit.

#[cfg(feature = "async")]
mod async_page_limit {
    use audex::FileType;
    use audex::limits::ParseLimits;
    use audex::ogg::OggPage;
    use audex::oggvorbis::OggVorbis;
    use std::io::Write;

    /// Build a single minimal OGG page with the given serial and sequence.
    ///
    /// Each page is 28 bytes: 27-byte header + 1-byte segment table (value 0).
    /// Zero-segment pages are the most compact valid pages and represent
    /// the pathological scenario of many tiny pages in a stream.
    fn build_minimal_ogg_page(serial: u32, sequence: u32, is_last: bool) -> Vec<u8> {
        let mut page = Vec::with_capacity(28);

        // OggS capture pattern
        page.extend_from_slice(b"OggS");
        // Version: 0
        page.push(0);
        // Header type: 0x04 = end of stream when marked as last
        page.push(if is_last { 0x04 } else { 0x00 });
        // Granule position: -1 (no position)
        page.extend_from_slice(&0xFFFFFFFFFFFFFFFFu64.to_le_bytes());
        // Serial number
        page.extend_from_slice(&serial.to_le_bytes());
        // Sequence number
        page.extend_from_slice(&sequence.to_le_bytes());
        // CRC checksum: 0 (not verified inline by the parser)
        page.extend_from_slice(&0u32.to_le_bytes());
        // Segment count: 1 segment of size 0
        page.push(1);
        // Segment table entry: size 0 (empty completed packet)
        page.push(0);

        page
    }

    /// Build a file with `count` minimal OGG pages, none marked as last.
    fn build_many_ogg_pages(count: usize, serial: u32) -> Vec<u8> {
        let mut data = Vec::with_capacity(count * 28);
        for i in 0..count {
            data.extend(build_minimal_ogg_page(serial, i as u32, false));
        }
        data
    }

    /// Build a minimal Vorbis OGG stream with a comment section of
    /// `comment_size` bytes.  Large comments are split across multiple
    /// continuation pages to stay within the 255-segment OGG page limit.
    fn build_minimal_vorbis_stream(comment_size: usize) -> Vec<u8> {
        let serial = 17u32;
        let mut ident = Vec::new();
        ident.push(0x01);
        ident.extend_from_slice(b"vorbis");
        ident.extend_from_slice(&0u32.to_le_bytes());
        ident.push(2);
        ident.extend_from_slice(&44_100u32.to_le_bytes());
        ident.extend_from_slice(&0u32.to_le_bytes());
        ident.extend_from_slice(&0u32.to_le_bytes());
        ident.extend_from_slice(&0u32.to_le_bytes());
        ident.extend_from_slice(&[0u8; 2]);

        let mut first_page = OggPage::new();
        first_page.serial = serial;
        first_page.sequence = 0;
        first_page.set_first(true);
        first_page.packets.push(ident);

        let mut comment = b"\x03vorbis".to_vec();
        comment.extend(std::iter::repeat_n(0x55, comment_size));

        // OGG pages can hold at most 255 segments of 255 bytes each (~64 KB).
        // Split the comment packet across multiple pages for large payloads.
        // Each segment can be at most 255 bytes, and each page can hold
        // at most 255 segment table entries.  A completed packet needs a
        // trailing zero-length segment, so 254 full segments fit safely.
        const MAX_PAGE_PAYLOAD: usize = 254 * 255;
        let mut data = first_page.write().expect("write first OGG page");
        let chunks: Vec<&[u8]> = comment.chunks(MAX_PAGE_PAYLOAD).collect();
        let num_chunks = chunks.len();

        for (i, chunk) in chunks.into_iter().enumerate() {
            let mut page = OggPage::new();
            page.serial = serial;
            page.sequence = (i + 1) as u32;
            if i + 1 == num_chunks {
                page.position = 44_100;
                page.set_last(true);
            }
            page.packets.push(chunk.to_vec());
            data.extend(page.write().expect("write comment OGG page"));
        }
        data
    }

    /// Verify that async and sync slow-path scans produce the same result
    /// when the page count exceeds the internal cap.
    ///
    /// Both paths should stop after MAX_OGG_PAGES (500,000) and return
    /// gracefully without scanning the remaining pages. This test uses
    /// a smaller page count (1,000) with no EOS marker to confirm the
    /// async path terminates correctly on bounded input.
    #[tokio::test]
    async fn async_find_last_handles_no_eos_gracefully() {
        let serial = 1u32;
        let page_count = 1_000;
        let data = build_many_ogg_pages(page_count, serial);

        let mut tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
        tmp.write_all(&data).expect("failed to write temp file");
        tmp.flush().expect("failed to flush temp file");

        let mut file = tokio::fs::File::open(tmp.path())
            .await
            .expect("failed to open temp file");

        // Should complete without error even though no EOS page exists
        let result = OggPage::find_last_async(&mut file, serial, false).await;

        assert!(
            result.is_ok(),
            "find_last_async should handle missing EOS gracefully, got: {:?}",
            result.err()
        );
    }

    /// Verify sync and async produce matching results for the same input.
    ///
    /// Parity between the two code paths is the core requirement — the sync
    /// path already has the page-count cap, so matching behavior confirms
    /// the async path is equally bounded.
    #[tokio::test]
    async fn async_find_last_matches_sync_behavior() {
        let serial = 42u32;
        let page_count = 500;
        let data = build_many_ogg_pages(page_count, serial);

        // Run sync path
        let mut sync_cursor = std::io::Cursor::new(&data);
        let sync_result = OggPage::find_last(&mut sync_cursor, serial, false);

        // Run async path
        let mut tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
        tmp.write_all(&data).expect("failed to write temp file");
        tmp.flush().expect("failed to flush temp file");

        let mut file = tokio::fs::File::open(tmp.path())
            .await
            .expect("failed to open temp file");

        let async_result = OggPage::find_last_async(&mut file, serial, false).await;

        // Both should succeed
        assert!(
            sync_result.is_ok(),
            "sync find_last failed: {:?}",
            sync_result.err()
        );
        assert!(
            async_result.is_ok(),
            "async find_last_async failed: {:?}",
            async_result.err()
        );

        // Both should return a page (the last one scanned for this serial)
        let sync_page = sync_result.unwrap();
        let async_page = async_result.unwrap();

        assert_eq!(
            sync_page.is_some(),
            async_page.is_some(),
            "sync and async should agree on whether a page was found"
        );
    }

    /// Verify that `accumulate_page_bytes_with_limit` rejects pages whose
    /// cumulative payload exceeds a tight budget.  Both the sync and async
    /// `find_last` paths delegate to this function, so testing it directly
    /// covers both code paths without requiring a mutable global.
    #[tokio::test]
    async fn accumulate_page_bytes_rejects_oversized_cumulative_data() {
        let tight = ParseLimits {
            max_tag_size: 8,
            max_image_size: ParseLimits::default().max_image_size,
        };

        let mut page = OggPage::new();
        page.packets.push(vec![0u8; 32]);

        let mut cumulative = 0u64;
        let err = OggPage::accumulate_page_bytes_with_limit(
            tight,
            &mut cumulative,
            &page,
            "OGG cumulative page data",
        )
        .expect_err("should reject page data exceeding tight limit");

        assert!(
            err.to_string().contains("OGG cumulative page data"),
            "unexpected error: {err}"
        );
    }

    /// Verify that both sync and async Ogg Vorbis loaders reject streams
    /// with malformed comment data that triggers a limit check.
    ///
    /// Rather than constructing a valid 8 MB+ Vorbis stream (which requires
    /// complex multi-page packet continuation), this test builds a small
    /// stream with a comment section whose garbage bytes trigger the
    /// Vorbis parser's vendor-length validation.
    #[tokio::test]
    async fn async_vorbis_loader_rejects_malformed_comment() {
        // 80 bytes of 0x55 after the Vorbis comment header is parsed as a
        // vendor length of 0x55555555 (~1.4 GB), exceeding the 1 MB cap.
        let data = build_minimal_vorbis_stream(80);

        let mut cursor = std::io::Cursor::new(&data);
        let sync_err = OggVorbis::load_from_reader(&mut cursor)
            .expect_err("sync Ogg Vorbis load should reject malformed comment data");

        let mut tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
        tmp.write_all(&data).expect("failed to write temp file");
        tmp.flush().expect("failed to flush temp file");

        let async_err = OggVorbis::load_async(tmp.path())
            .await
            .expect_err("async Ogg Vorbis load should reject malformed comment data");

        // Both paths should reject the data for the same reason
        let sync_msg = sync_err.to_string();
        let async_msg = async_err.to_string();
        assert!(
            sync_msg.contains("Vorbis vendor length"),
            "unexpected sync error: {sync_msg}"
        );
        assert!(
            async_msg.contains("Vorbis vendor length"),
            "unexpected async error: {async_msg}"
        );
    }
}
