//! Tests for MP4 atom parsing limits and offset table arithmetic.
//!
//! Exercises depth limits, children limits, and the stco/co64 offset
//! update logic with crafted binary data that hits overflow and
//! underflow boundaries.

use audex::mp4::atom::{AtomType, MP4Atom};
use std::io::{Cursor, Seek, SeekFrom};

/// Container atom names we cycle through when building deep nesting.
/// All of these are recognized as container atoms by the parser.
const CONTAINER_NAMES: &[&[u8; 4]] = &[
    b"moov", b"trak", b"mdia", b"udta", b"stbl", b"minf", b"moof", b"traf",
];

/// Build a binary buffer with `depth` levels of nested container atoms.
/// The innermost container holds a single non-container leaf atom.
fn build_nested_containers(depth: usize) -> Vec<u8> {
    // Start with a non-container leaf atom (8 bytes)
    let mut inner = Vec::new();
    inner.extend_from_slice(&8u32.to_be_bytes());
    inner.extend_from_slice(b"data");

    // Wrap `depth` container levels around the leaf, inside-out
    for level in (0..depth).rev() {
        let name = CONTAINER_NAMES[level % CONTAINER_NAMES.len()];
        let size = (8 + inner.len()) as u32;
        let mut outer = Vec::with_capacity(size as usize);
        outer.extend_from_slice(&size.to_be_bytes());
        outer.extend_from_slice(name);
        outer.append(&mut inner);
        inner = outer;
    }
    inner
}

// ---------------------------------------------------------------------------
// Atom depth limit
// ---------------------------------------------------------------------------

#[test]
fn atom_max_depth_rejected() {
    // MAX_ATOM_DEPTH is 128. A tree with 129 nested container levels
    // must fail: the parser at level 128 checks `128 + 1 > 128` before
    // recursing into children, and returns DepthLimitExceeded.
    let data = build_nested_containers(129);
    let mut cursor = Cursor::new(&data);
    let result = MP4Atom::parse(&mut cursor, 0);
    assert!(
        result.is_err(),
        "129 nesting levels must exceed the depth limit"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("depth") || err.contains("Depth"),
        "Error should mention depth: {err}"
    );
}

#[test]
fn atom_depth_at_limit_accepted() {
    // 128 levels of nesting is exactly at the boundary and must succeed.
    let data = build_nested_containers(128);
    let mut cursor = Cursor::new(&data);
    let result = MP4Atom::parse(&mut cursor, 0);
    assert!(
        result.is_ok(),
        "128 nesting levels is at the limit and must be accepted: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// Atom children limit
// ---------------------------------------------------------------------------

#[test]
fn atom_max_children_rejected() {
    // MAX_CHILDREN_PER_ATOM is 100,000. A container with 100,001
    // children must be rejected.
    let child_count: usize = 100_001;
    let child_size: usize = 8; // minimal non-container atom

    // Build the container: 8-byte header + (child_count * 8) bytes of children
    let container_size = (8 + child_count * child_size) as u32;
    let mut data = Vec::with_capacity(container_size as usize);
    data.extend_from_slice(&container_size.to_be_bytes());
    data.extend_from_slice(b"moov"); // known container type

    for _ in 0..child_count {
        data.extend_from_slice(&8u32.to_be_bytes());
        data.extend_from_slice(b"free");
    }

    let mut cursor = Cursor::new(&data);
    let result = MP4Atom::parse(&mut cursor, 0);
    assert!(
        result.is_err(),
        "100,001 children must exceed the children limit"
    );
}

#[test]
fn atom_children_at_limit_accepted() {
    // 100,000 children is the maximum allowed count.
    let child_count: usize = 100_000;
    let child_size: usize = 8;
    let container_size = (8 + child_count * child_size) as u32;

    let mut data = Vec::with_capacity(container_size as usize);
    data.extend_from_slice(&container_size.to_be_bytes());
    data.extend_from_slice(b"moov");

    for _ in 0..child_count {
        data.extend_from_slice(&8u32.to_be_bytes());
        data.extend_from_slice(b"free");
    }

    let mut cursor = Cursor::new(&data);
    let result = MP4Atom::parse(&mut cursor, 0);
    assert!(
        result.is_ok(),
        "100,000 children is at the limit and must be accepted: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// stco / co64 offset table updates
// ---------------------------------------------------------------------------

/// Build a minimal stco atom's data payload: 4 version/flags bytes,
/// 4-byte entry count, then `entries.len()` 4-byte big-endian offsets.
fn build_stco_data(entries: &[u32]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&[0u8; 4]); // version + flags
    data.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for &offset in entries {
        data.extend_from_slice(&offset.to_be_bytes());
    }
    data
}

/// Build a minimal co64 atom's data payload: 4 version/flags bytes,
/// 4-byte entry count, then `entries.len()` 8-byte big-endian offsets.
fn build_co64_data(entries: &[u64]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&[0u8; 4]); // version + flags
    data.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for &offset in entries {
        data.extend_from_slice(&offset.to_be_bytes());
    }
    data
}

/// Construct an MP4Atom tree where a container "moov" holds a single
/// stco (or co64) child whose data lives at position 0 in the reader.
fn make_tree_with_offset_atom(atom_name: &[u8; 4], data_len: u64) -> MP4Atom {
    let child = MP4Atom {
        atom_type: AtomType::from_bytes(atom_name),
        name: *atom_name,
        length: 8 + data_len,
        offset: 0,
        data_offset: 0, // payload starts at byte 0 of the reader
        data_length: data_len,
        children: None,
    };

    MP4Atom {
        atom_type: AtomType::Moov,
        name: *b"moov",
        length: 8 + child.length,
        offset: 0,
        data_offset: 8,
        data_length: child.length,
        children: Some(vec![child]),
    }
}

#[test]
fn stco_negative_delta_underflow_rejected() {
    // An offset of 1000 with a negative delta of -2000 would produce a
    // negative offset. The function must reject this.
    let stco_payload = build_stco_data(&[1000]);
    let tree = make_tree_with_offset_atom(b"stco", stco_payload.len() as u64);

    let reader_data = stco_payload.clone();
    let mut writer_data = stco_payload;
    let mut reader = Cursor::new(&reader_data);
    let mut writer = Cursor::new(&mut writer_data);

    // moov_offset=0 so the offset 1000 > 0, making it eligible for update.
    // Delta of -2000 would underflow 1000.
    let result = tree.update_offset_tables(&mut reader, &mut writer, -2000, 0);
    assert!(
        result.is_err(),
        "Negative delta causing underflow must be rejected"
    );
}

#[test]
fn stco_offset_exceeds_u32_max_rejected() {
    // An stco entry near u32::MAX with a positive delta must be rejected
    // because stco uses 32-bit offsets.
    let high_offset: u32 = u32::MAX - 100;
    let stco_payload = build_stco_data(&[high_offset]);
    let tree = make_tree_with_offset_atom(b"stco", stco_payload.len() as u64);

    let reader_data = stco_payload.clone();
    let mut writer_data = stco_payload;
    let mut reader = Cursor::new(&reader_data);
    let mut writer = Cursor::new(&mut writer_data);

    // Delta of +200 pushes the offset past u32::MAX
    let result = tree.update_offset_tables(&mut reader, &mut writer, 200, 0);
    assert!(
        result.is_err(),
        "stco offset exceeding u32::MAX must be rejected"
    );
}

#[test]
fn co64_negative_delta_underflow_rejected() {
    let co64_payload = build_co64_data(&[500]);
    let tree = make_tree_with_offset_atom(b"co64", co64_payload.len() as u64);

    let reader_data = co64_payload.clone();
    let mut writer_data = co64_payload;
    let mut reader = Cursor::new(&reader_data);
    let mut writer = Cursor::new(&mut writer_data);

    let result = tree.update_offset_tables(&mut reader, &mut writer, -1000, 0);
    assert!(result.is_err(), "co64 offset underflow must be rejected");
}

#[test]
fn stco_positive_delta_updates_correctly() {
    // Verify that a valid positive delta actually updates the offset.
    let original_offset: u32 = 5000;
    let delta: i64 = 100;
    let stco_payload = build_stco_data(&[original_offset]);
    let tree = make_tree_with_offset_atom(b"stco", stco_payload.len() as u64);

    let reader_data = stco_payload.clone();
    let mut writer_data = stco_payload;
    let mut reader = Cursor::new(&reader_data);
    let mut writer = Cursor::new(&mut writer_data);

    tree.update_offset_tables(&mut reader, &mut writer, delta, 0)
        .expect("Valid positive delta must succeed");

    // Read back the updated offset from the writer
    writer.seek(SeekFrom::Start(8)).unwrap();
    let mut buf = [0u8; 4];
    std::io::Read::read_exact(&mut writer, &mut buf).unwrap();
    let new_offset = u32::from_be_bytes(buf);

    assert_eq!(
        new_offset,
        (original_offset as i64 + delta) as u32,
        "Offset must be updated by the delta"
    );
}

#[test]
fn offset_below_moov_not_modified() {
    // Offsets pointing before the moov atom must NOT be updated.
    let offset_before_moov: u32 = 100;
    let moov_offset: u64 = 500;
    let stco_payload = build_stco_data(&[offset_before_moov]);
    let tree = make_tree_with_offset_atom(b"stco", stco_payload.len() as u64);

    let reader_data = stco_payload.clone();
    let mut writer_data = stco_payload;
    let mut reader = Cursor::new(&reader_data);
    let mut writer = Cursor::new(&mut writer_data);

    tree.update_offset_tables(&mut reader, &mut writer, 9999, moov_offset)
        .expect("Offsets below moov should be left alone");

    writer.seek(SeekFrom::Start(8)).unwrap();
    let mut buf = [0u8; 4];
    std::io::Read::read_exact(&mut writer, &mut buf).unwrap();
    let unchanged = u32::from_be_bytes(buf);

    assert_eq!(
        unchanged, offset_before_moov,
        "Offset before moov_offset must not be modified"
    );
}

#[test]
fn update_offset_tables_zero_delta_is_noop() {
    // size_delta == 0 must return Ok immediately without touching data.
    let stco_payload = build_stco_data(&[1000]);
    let tree = make_tree_with_offset_atom(b"stco", stco_payload.len() as u64);

    let reader_data = stco_payload.clone();
    let mut writer_data = stco_payload;
    let mut reader = Cursor::new(&reader_data);
    let mut writer = Cursor::new(&mut writer_data);

    tree.update_offset_tables(&mut reader, &mut writer, 0, 0)
        .expect("Zero delta must succeed");

    // Verify data was not touched (writer position should still be 0)
    assert_eq!(writer.position(), 0, "Zero delta must not seek or write");
}

#[test]
fn stco_i64_min_delta_uses_unsigned_abs_safely() {
    // i64::MIN cannot be negated without overflow. The function uses
    // unsigned_abs() which handles this correctly.
    let large_offset: u32 = u32::MAX;
    let stco_payload = build_stco_data(&[large_offset]);
    let tree = make_tree_with_offset_atom(b"stco", stco_payload.len() as u64);

    let reader_data = stco_payload.clone();
    let mut writer_data = stco_payload;
    let mut reader = Cursor::new(&reader_data);
    let mut writer = Cursor::new(&mut writer_data);

    // i64::MIN as delta: unsigned_abs() = 2^63 = 9223372036854775808.
    // Any u32 offset is far smaller, so this must underflow and be rejected.
    let result = tree.update_offset_tables(&mut reader, &mut writer, i64::MIN, 0);
    assert!(
        result.is_err(),
        "i64::MIN delta must be handled without panic (unsigned_abs)"
    );
}
