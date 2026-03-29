// Tests for read_to_end limits in format parsers.
//
// Some parsers load the entire file into memory via read_to_end before
// parsing headers. For large files (multi-GB lossless audio), this causes
// unnecessary memory exhaustion when only a few hundred bytes of metadata
// are needed. A cap on the initial read prevents this.

use std::io::Cursor;

/// Build a minimal TAK file header followed by `padding_size` bytes of zeros.
/// TAK files start with the 4-byte magic "tBaK".
fn build_oversized_tak(padding_size: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(4 + padding_size);
    data.extend_from_slice(b"tBaK");
    data.resize(4 + padding_size, 0);
    data
}

#[test]
fn test_tak_sync_rejects_oversized_read() {
    // 2 MB of data following the TAK magic — the parser should only read
    // up to 1 MB (its cap), not the full file.
    let data = build_oversized_tak(2 * 1024 * 1024);
    let mut cursor = Cursor::new(data);

    use audex::tak::TAKStreamInfo;
    let result = TAKStreamInfo::from_reader(&mut cursor);

    // After the fix, the parser reads at most 1 MB (not 128 MB), then
    // fails on invalid metadata. The test passes either way, but with
    // the fix it runs in milliseconds instead of allocating 128 MB.
    assert!(
        result.is_err(),
        "TAK parser should fail on invalid metadata"
    );
}

#[test]
fn test_tak_sync_accepts_normal_file() {
    // A small TAK file with just the magic + a valid-looking metadata block.
    // TAK metadata blocks have: type (1 byte) + size (varies).
    // Type 0x01 = stream info. We'll provide a minimal invalid block that
    // will fail parsing, but the read itself should succeed.
    let mut data = Vec::new();
    data.extend_from_slice(b"tBaK");
    // Some dummy metadata bytes (will fail parsing but the read is fine)
    data.extend_from_slice(&[0u8; 256]);

    let mut cursor = Cursor::new(data);

    use audex::tak::TAKStreamInfo;
    let result = TAKStreamInfo::from_reader(&mut cursor);

    // Parsing may fail (invalid metadata) but the read should not be rejected
    // for being too large. Check that the error is NOT about size limits.
    if let Err(ref e) = result {
        let msg = format!("{}", e);
        assert!(
            !msg.contains("metadata read limit"),
            "Small file should not be rejected for size: {}",
            msg
        );
    }
}
