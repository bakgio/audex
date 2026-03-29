//! MP4 utility functions for atom parsing, key conversion, and file operations.
//!
//! Provides helpers for working with MP4/M4A metadata:
//! - Full atom (versioned atom) header parsing
//! - Atom name ↔ string key conversion (Latin-1 encoding)
//! - Tag clearing operations (sync and async)
//! - iTunes-compatible tag sort ordering
//! - Padding atom discovery adjacent to `ilst`

use crate::mp4::{MP4, MP4Tags};
use crate::{AudexError, FileType, Result};
use std::path::Path;

/// Parse a full atom (versioned atom) into version, flags, and payload
pub fn parse_full_atom(data: &[u8]) -> Result<(u8, u32, &[u8])> {
    if data.len() < 4 {
        return Err(AudexError::ParseError("not enough data".to_string()));
    }

    let version = data[0];
    let flags = u32::from_be_bytes([0, data[1], data[2], data[3]]);
    let payload = &data[4..];

    Ok((version, flags, payload))
}

/// Convert atom name bytes to string key (latin-1 decoding)
///
/// Convert atom name to metadata key
pub fn name2key(name: &[u8]) -> String {
    // Latin-1 is equivalent to treating each byte as a Unicode code point
    name.iter().map(|&b| b as char).collect()
}

/// Convert string key to atom name bytes (latin-1 encoding)
///
/// Convert metadata key to atom name
pub fn key2name(key: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::with_capacity(key.len());
    for c in key.chars() {
        let code = c as u32;
        if code <= 255 {
            bytes.push(code as u8);
        } else {
            return Err(AudexError::InvalidData(format!(
                "Non-Latin-1 character U+{:04X} in atom key \"{}\"",
                code, key
            )));
        }
    }
    Ok(bytes)
}

/// Remove tags from a file
///
/// Clear metadata from MP4 file
pub fn clear<P: AsRef<Path>>(path: P) -> Result<()> {
    let mut mp4 = MP4::load(&path)?;

    if mp4.tags.is_some() {
        mp4.tags = Some(MP4Tags::default());
        // Save with minimal padding (empty tags structure removes existing metadata)
        mp4.save()?;
    }

    Ok(())
}

/// Remove tags from a file asynchronously
///
/// Clear metadata from MP4 file using async I/O operations.
/// Wraps the synchronous clear operation in a blocking task
/// to prevent blocking the async runtime.
#[cfg(feature = "async")]
pub async fn clear_async<P: AsRef<Path>>(path: P) -> Result<()> {
    use crate::mp4::atom::Atoms;

    let path = path.as_ref();

    // Check if there are tags to clear by parsing atoms
    let mut file = tokio::fs::File::open(path).await?;
    let atoms = Atoms::parse_async(&mut file).await?;
    drop(file);

    if atoms.path("moov.udta.meta.ilst").is_some() {
        let empty_tags = MP4Tags::default();
        empty_tags.save_async(path).await?;
    }

    Ok(())
}

/// Get item sort key for iTunes-compatible tag ordering
///
/// Generate sort key for metadata items
pub fn item_sort_key(key: &str, value: &str) -> (usize, usize, String) {
    let order = [
        "©nam", "©ART", "©wrt", "©alb", "©gen", "gnre", "trkn", "disk", "©day", "cpil", "pgap",
        "pcst", "tmpo", "©too", "----", "covr", "©lyr",
    ];

    // For 4-character keys like ©nam, we need to handle the entire key, not just 4 bytes
    // since © is a multibyte UTF-8 character.
    // Use get(..4) to safely handle keys where byte index 4 falls inside
    // a multi-byte character — in that case, fall back to the full key.
    let key_prefix = if key.starts_with('©') && key.len() >= 4 {
        // For © tags, take the full key
        key
    } else {
        // Safely take first 4 bytes, falling back to full key if the
        // boundary would split a multi-byte UTF-8 character
        key.get(..4).unwrap_or(key)
    };

    let order_index = order
        .iter()
        .position(|&x| x == key_prefix)
        .unwrap_or(order.len());

    (order_index, value.len(), value.to_string())
}

/// Find padding atom adjacent to ilst atom
pub fn find_padding(
    meta_children: &[crate::mp4::atom::MP4Atom],
) -> Option<&crate::mp4::atom::MP4Atom> {
    // Find the ilst atom first
    let ilst_index = meta_children
        .iter()
        .position(|child| child.name == *b"ilst")?;

    // Check previous atom for free
    if ilst_index > 0 {
        let prev = &meta_children[ilst_index - 1];
        if prev.name == *b"free" {
            return Some(prev);
        }
    }

    // Check next atom for free
    if ilst_index + 1 < meta_children.len() {
        let next = &meta_children[ilst_index + 1];
        if next.name == *b"free" {
            return Some(next);
        }
    }

    None
}
