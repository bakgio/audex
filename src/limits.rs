//! Parse limits for all audio format parsers.
//!
//! Provides a unified [`ParseLimits`] configuration that caps memory
//! allocations during metadata parsing — regardless of which audio format
//! is being processed.  Two independent ceilings are enforced:
//!
//! - **`max_tag_size`** — maximum bytes for an entire tag/metadata block
//!   (text frames, comments, chapters, etc.).
//! - **`max_image_size`** — maximum bytes for a single embedded image
//!   (cover art, booklet scans, etc.).  This is intentionally separate
//!   because high-resolution artwork can legitimately be very large.
//!
//! # Specification Compatibility
//!
//! The per-format constants and default limits are summarized below.
//! Note that some library ceilings are lower than the format specification
//! allows; use [`ParseLimits::permissive()`] for files near those limits:
//!
//! | Format     | Effective parse ceiling          | Spec ceiling             |
//! |------------|----------------------------------|--------------------------|
//! | ID3v2      | 256 MB (2²⁸ − 1 synchsafe)      | 256 MB (synchsafe)       |
//! | FLAC       | ~16 MB (2²⁴ − 1 per block)       | ~16 MB (24-bit)          |
//! | APEv2      | no hard limit                    | no hard spec limit       |
//! | Vorbis     | 10 MB per comment                | ~4 GB (32-bit length)    |
//! | MP4        | 256 MB (atom data buffer)        | ~unlimited (64-bit ext.) |
//! | AIFF/WAVE  | 256 MB (chunk data)              | ~4 GB (32-bit size)      |
//!
//! Separately, ASF saving uses a whole-file in-memory rewrite path guarded by
//! [`MAX_IN_MEMORY_WRITER_FILE`], currently 512 MiB.
//!
//! The default limits (8 MB tags / 16 MB images) are safe for untrusted
//! input. If you need to accept spec-legal files with very large metadata,
//! opt into [`ParseLimits::permissive()`] explicitly.
//!
//! # Usage
//!
//! The defaults are safe for most applications, including those that handle
//! untrusted uploads.  Construct a `ParseLimits` value and pass it through
//! your parser pipeline, or rely on `ParseLimits::default()` for safe
//! defaults.

// ---------------------------------------------------------------------------
// Spec-derived reference constants (not enforced directly — used to document
// where the defaults come from)
// ---------------------------------------------------------------------------

/// ID3v2 maximum tag size: synchsafe 4-byte integer → 2²⁸ − 1 bytes.
pub const ID3V2_SPEC_MAX: u64 = (1 << 28) - 1; // 268_435_455

/// FLAC maximum metadata block: 24-bit size field → 2²⁴ − 1 bytes.
pub const FLAC_SPEC_BLOCK_MAX: u64 = (1 << 24) - 1; // 16_777_215

/// MP4 atom data buffer ceiling used by this library.
pub const MP4_ATOM_DATA_MAX: u64 = 256 * 1024 * 1024; // 256 MB

/// AIFF / WAVE chunk data ceiling used by this library.
pub const IFF_CHUNK_DATA_MAX: u64 = 256 * 1024 * 1024; // 256 MB

/// Vorbis comment maximum single comment length used by this library.
pub const VORBIS_COMMENT_MAX: u64 = 10 * 1024 * 1024; // 10 MB

/// Maximum file size accepted by whole-file in-memory writer operations.
///
/// Some writer-based save/clear paths need to buffer the complete file in
/// memory because the underlying transformation logic rewrites container
/// structures on a `Cursor`. This limit is intentionally separate from
/// `max_tag_size`: a large audio payload with tiny metadata should not be
/// rejected just because the tag-allocation budget is restrictive.
pub const MAX_IN_MEMORY_WRITER_FILE: u64 = 512 * 1024 * 1024; // 512 MB

// ---------------------------------------------------------------------------
// ParseLimits
// ---------------------------------------------------------------------------

/// Library-wide limits on metadata and image allocations during parsing.
///
/// Every format-specific parser checks these limits in addition to its own
/// format-level constraints.  Construct an instance with [`Default::default()`]
/// for safe defaults, or use [`ParseLimits::permissive()`] for trusted media.
///
/// # Security
///
/// The default limits (8 MB tags / 16 MB images) are safe for processing
/// untrusted or user-uploaded content. They cover virtually all legitimate
/// media while preventing resource exhaustion from crafted files.
///
/// If you need to process trusted files that may have very large metadata
/// (up to the format spec ceilings), use [`ParseLimits::permissive()`]
/// explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLimits {
    /// Maximum bytes allowed for a single tag or metadata block.
    ///
    /// Covers text frames, Vorbis comments, ASF attributes, MP4 metadata
    /// atoms, etc.  Does **not** include embedded images — those are
    /// governed by [`max_image_size`](Self::max_image_size).
    ///
    /// **Default:** 8 MB — safe for untrusted input while covering virtually
    /// all real-world metadata. Use [`ParseLimits::permissive()`] (256 MB) if
    /// you need full spec-ceiling compliance with trusted files.
    pub max_tag_size: u64,

    /// Maximum bytes allowed for a single embedded image.
    ///
    /// Cover art, booklet scans, and other picture data can be very large
    /// (vinyl-sleeve scans routinely exceed 20 MB).  This limit is kept
    /// separate so that applications handling trusted media can allow large
    /// artwork without raising the general metadata ceiling.
    ///
    /// **Default:** 16 MB — covers high-resolution cover art and booklet
    /// scans while preventing unbounded allocations. FLAC's own 16 MB block
    /// limit aligns with this value. Use [`ParseLimits::permissive()`]
    /// (128 MB) if you need to handle exceptionally large embedded artwork
    /// from trusted sources.
    pub max_image_size: u64,
}

impl Default for ParseLimits {
    /// Returns restrictive default limits (8 MB tags / 16 MB images).
    ///
    /// These defaults are safe for processing untrusted input. They cap
    /// metadata at 8 MB and images at 16 MB, which covers virtually all
    /// legitimate media while preventing resource exhaustion from crafted
    /// files.
    ///
    /// If you need to parse spec-legal files with extremely large metadata
    /// (e.g. ID3v2 tags up to 256 MB), use [`ParseLimits::permissive()`]
    /// explicitly.
    fn default() -> Self {
        Self {
            // 8 MB — sufficient for virtually all real-world metadata while
            // preventing denial-of-service from crafted files that exploit
            // the 256 MB spec ceilings in ID3v2/MP4/AIFF/WAVE.
            max_tag_size: 8 * 1024 * 1024,

            // 16 MB — covers high-resolution cover art (vinyl scans, booklets)
            // while preventing unbounded allocations. FLAC's own 16 MB block
            // limit aligns with this value.
            max_image_size: 16 * 1024 * 1024,
        }
    }
}

impl ParseLimits {
    /// Tighter limits suitable for processing untrusted or user-uploaded files.
    ///
    /// Caps metadata at 8 MB and images at 16 MB, which covers virtually all
    /// legitimate media while preventing a single malformed file from consuming
    /// hundreds of megabytes.
    ///
    /// Note: As of the current version, these values are identical to the
    /// defaults. This method is retained for clarity and forward compatibility.
    pub fn restrictive() -> Self {
        Self {
            max_tag_size: 8 * 1024 * 1024,    // 8 MB
            max_image_size: 16 * 1024 * 1024, // 16 MB
        }
    }

    /// Permissive limits that accommodate every format's hard specification
    /// ceiling (256 MB tags / 128 MB images).
    ///
    /// Use this preset only when processing trusted, known-good media files
    /// where spec compliance is more important than resource protection.
    ///
    /// # Security Warning
    ///
    /// **These limits allow a single crafted file to force allocations of up
    /// to 256 MB.** Never use permissive limits with untrusted or
    /// user-uploaded content. Prefer the default (restrictive) limits or
    /// set custom values appropriate for your threat model.
    pub fn permissive() -> Self {
        Self {
            // 256 MB — matches ID3v2 synchsafe max, MP4/AIFF/WAVE chunk ceilings
            max_tag_size: 256 * 1024 * 1024,
            // 128 MB — generous ceiling for embedded artwork in any format
            max_image_size: 128 * 1024 * 1024,
        }
    }

    /// Check whether `size` bytes of tag/metadata data would exceed the
    /// configured [`max_tag_size`](Self::max_tag_size).
    ///
    /// Returns `Ok(())` on success or an [`crate::AudexError::InvalidData`] on
    /// overflow.
    pub fn check_tag_size(&self, size: u64, context: &str) -> crate::Result<()> {
        if size > self.max_tag_size {
            return Err(crate::AudexError::InvalidData(format!(
                "{}: tag data size ({} bytes) exceeds configured limit ({} bytes)",
                context, size, self.max_tag_size
            )));
        }
        Ok(())
    }

    /// Check whether `size` bytes of image data would exceed the
    /// configured [`max_image_size`](Self::max_image_size).
    ///
    /// Returns `Ok(())` on success or an [`crate::AudexError::InvalidData`] on
    /// overflow.
    pub fn check_image_size(&self, size: u64, context: &str) -> crate::Result<()> {
        if size > self.max_image_size {
            return Err(crate::AudexError::InvalidData(format!(
                "{}: image data size ({} bytes) exceeds configured limit ({} bytes)",
                context, size, self.max_image_size
            )));
        }
        Ok(())
    }
}
