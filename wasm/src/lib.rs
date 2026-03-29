//! WebAssembly bindings for the **audex** audio metadata library.
//!
//! This crate exposes audex's tag reading, writing, and stream info
//! capabilities to JavaScript and TypeScript consumers running in
//! web browsers or Node.js, without requiring filesystem access.
//!
//! # Supported formats
//!
//! All 30+ formats supported by audex are available through these
//! bindings, including MP3, FLAC, MP4/M4A, Ogg Vorbis, Ogg Opus,
//! WAV, AIFF, WMA/ASF, APE, WavPack, and more.
//!
//! # Quick start (JavaScript)
//!
//! ```javascript
//! import init, { AudioFile } from './audex_wasm.js';
//!
//! await init();
//!
//! const response = await fetch('/audio/track.mp3');
//! const bytes = new Uint8Array(await response.arrayBuffer());
//! const file = new AudioFile(bytes, 'track.mp3');
//!
//! console.log(file.formatName());       // "MP3"
//! console.log(file.getFirst('artist')); // "Artist Name"
//! console.log(file.streamInfo());       // { lengthSecs: 234.5, ... }
//!
//! file.setSingle('artist', 'New Artist');
//! const savedBytes = file.save();
//!
//! file.free(); // release WASM memory
//! ```

mod audio_file;
mod error;
mod stream_info;
mod utils;

use std::path::PathBuf;

use wasm_bindgen::prelude::*;

// Re-export the primary types for JS consumers
pub use audio_file::AudioFile;
pub use audio_file::WasmTagDiff;
pub use stream_info::WasmStreamInfo;

const MAX_DETECT_INPUT_BYTES: usize = 1024 * 1024;

fn detect_input_len_error(len: usize) -> Option<String> {
    if len > MAX_DETECT_INPUT_BYTES {
        return Some(format!(
            "detect_format input too large ({} bytes, max {} bytes). \
                 Pass only the first few hundred bytes for format detection.",
            len, MAX_DETECT_INPUT_BYTES,
        ));
    }
    None
}

fn check_detect_input_len(len: usize) -> Result<(), JsValue> {
    if let Some(message) = detect_input_len_error(len) {
        return Err(error::to_js_error(audex::AudexError::InvalidData(message)));
    }
    Ok(())
}

/// WASM module initialiser — called automatically by wasm-bindgen's
/// generated glue code.  Installs the panic hook so that Rust panics
/// produce readable stack traces in the browser console.
#[wasm_bindgen(start)]
pub fn init() {
    utils::set_panic_hook();
}

/// Detect the audio format of a byte buffer without fully parsing it.
///
/// Returns the format name (e.g. "MP3", "FLAC") or throws if the
/// format cannot be determined.  Pass an optional filename or extension
/// to improve detection accuracy.
///
/// Uses header-based scoring only (first 128 bytes) — no full parse is
/// attempted, so this works reliably even on partial or truncated buffers.
///
/// # Performance Note
///
/// Only the first 128 bytes of `data` are examined, but wasm-bindgen
/// copies the entire input buffer into WASM linear memory. To avoid
/// unnecessary memory consumption, **callers should pass only the first
/// 512 bytes** (or fewer) of the file rather than the entire file content.
/// Inputs larger than 1 MB are rejected to prevent accidental misuse.
#[wasm_bindgen(js_name = "detectFormat")]
pub fn detect_format(data: &[u8], filename: Option<String>) -> Result<String, JsValue> {
    // Guard against callers accidentally passing an entire file buffer.
    // Format detection only needs the first 128 bytes, so anything above
    // 1 MB is almost certainly a mistake that would waste WASM memory.
    check_detect_input_len(data.len())?;

    error::catch_panic(|| {
        let hint = filename.map(PathBuf::from);
        let hint_ref = hint.as_deref();

        // Score-only detection: examines magic bytes and header structures
        // without attempting a full parse. Safe on partial buffers.
        audex::detect_format_from_bytes(data, hint_ref).map_err(error::to_js_error)
    })
}
