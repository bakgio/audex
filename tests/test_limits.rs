use audex::limits::{
    FLAC_SPEC_BLOCK_MAX, ID3V2_SPEC_MAX, IFF_CHUNK_DATA_MAX, MAX_IN_MEMORY_WRITER_FILE,
    MP4_ATOM_DATA_MAX, ParseLimits, VORBIS_COMMENT_MAX,
};

const MB: u64 = 1024 * 1024;

// --- Default constructor values ---

#[test]
fn test_default_limits_values() {
    let limits = ParseLimits::default();
    assert_eq!(limits.max_tag_size, 8 * MB);
    assert_eq!(limits.max_image_size, 16 * MB);
}

// --- Restrictive preset matches the safe defaults ---

#[test]
fn test_restrictive_limits_values() {
    let restrictive = ParseLimits::restrictive();
    let default = ParseLimits::default();
    assert_eq!(restrictive, default);
}

// --- Permissive preset accommodates full spec ceilings ---

#[test]
fn test_permissive_limits_values() {
    let limits = ParseLimits::permissive();
    assert_eq!(limits.max_tag_size, 256 * MB);
    assert_eq!(limits.max_image_size, 128 * MB);
}

// --- check_tag_size: values within the limit succeed ---

#[test]
fn test_check_tag_size_within_limit() {
    let limits = ParseLimits::default();
    assert!(limits.check_tag_size(0, "empty").is_ok());
    assert!(limits.check_tag_size(1024, "small tag").is_ok());
    assert!(limits.check_tag_size(4 * MB, "moderate tag").is_ok());
}

// --- check_tag_size: boundary at exactly max_tag_size ---

#[test]
fn test_check_tag_size_at_boundary() {
    let limits = ParseLimits::default();
    // Exactly at the limit should pass
    assert!(limits.check_tag_size(8 * MB, "at limit").is_ok());
    // One byte over should fail
    assert!(limits.check_tag_size(8 * MB + 1, "over limit").is_err());
}

// --- check_image_size: values within the limit succeed ---

#[test]
fn test_check_image_size_within_limit() {
    let limits = ParseLimits::default();
    assert!(limits.check_image_size(0, "empty").is_ok());
    assert!(limits.check_image_size(1024, "small image").is_ok());
    assert!(limits.check_image_size(10 * MB, "large cover art").is_ok());
}

// --- check_image_size: boundary at exactly max_image_size ---

#[test]
fn test_check_image_size_at_boundary() {
    let limits = ParseLimits::default();
    assert!(limits.check_image_size(16 * MB, "at limit").is_ok());
    assert!(limits.check_image_size(16 * MB + 1, "over limit").is_err());
}

// --- check_tag_size: error message includes context, actual size, and limit ---

#[test]
fn test_check_tag_size_error_message() {
    let limits = ParseLimits::default();
    let err = limits.check_tag_size(100 * MB, "ID3v2 header").unwrap_err();
    let msg = err.to_string();

    assert!(msg.contains("ID3v2 header"), "missing context in: {}", msg);
    assert!(
        msg.contains(&(100 * MB).to_string()),
        "missing actual size in: {}",
        msg
    );
    assert!(
        msg.contains(&(8 * MB).to_string()),
        "missing limit in: {}",
        msg
    );
}

// --- check_image_size: error message includes context, actual size, and limit ---

#[test]
fn test_check_image_size_error_message() {
    let limits = ParseLimits::default();
    let err = limits.check_image_size(50 * MB, "APIC frame").unwrap_err();
    let msg = err.to_string();

    assert!(msg.contains("APIC frame"), "missing context in: {}", msg);
    assert!(
        msg.contains(&(50 * MB).to_string()),
        "missing actual size in: {}",
        msg
    );
    assert!(
        msg.contains(&(16 * MB).to_string()),
        "missing limit in: {}",
        msg
    );
}

// --- Custom limits enforce the values provided by the caller ---

#[test]
fn test_custom_limits() {
    let limits = ParseLimits {
        max_tag_size: 100,
        max_image_size: 200,
    };

    assert!(limits.check_tag_size(100, "ok").is_ok());
    assert!(limits.check_tag_size(101, "over").is_err());

    assert!(limits.check_image_size(200, "ok").is_ok());
    assert!(limits.check_image_size(201, "over").is_err());
}

// --- Spec-derived reference constants match documented values ---

#[test]
fn test_spec_constants() {
    // ID3v2: synchsafe 4-byte integer maximum (2^28 - 1)
    assert_eq!(ID3V2_SPEC_MAX, 268_435_455);

    // FLAC: 24-bit metadata block size (2^24 - 1)
    assert_eq!(FLAC_SPEC_BLOCK_MAX, 16_777_215);

    // MP4 atom data buffer ceiling
    assert_eq!(MP4_ATOM_DATA_MAX, 256 * MB);

    // AIFF / WAVE chunk data ceiling
    assert_eq!(IFF_CHUNK_DATA_MAX, 256 * MB);

    // Vorbis comment single-comment ceiling
    assert_eq!(VORBIS_COMMENT_MAX, 10 * MB);

    // In-memory writer file ceiling
    assert_eq!(MAX_IN_MEMORY_WRITER_FILE, 512 * MB);
}

// --- Zero limits reject even a single byte ---

#[test]
fn test_zero_limits_reject_everything() {
    let limits = ParseLimits {
        max_tag_size: 0,
        max_image_size: 0,
    };

    // Zero-byte inputs still pass (not *exceeding* zero)
    assert!(limits.check_tag_size(0, "empty").is_ok());
    assert!(limits.check_image_size(0, "empty").is_ok());

    // Any non-zero input is rejected
    assert!(limits.check_tag_size(1, "one byte").is_err());
    assert!(limits.check_image_size(1, "one byte").is_err());
}
