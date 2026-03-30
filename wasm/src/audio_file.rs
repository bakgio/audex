// AudioFile: the primary WASM-facing wrapper around audex's DynamicFileType.
//
// This struct holds both the parsed metadata and the original byte buffer,
// enabling in-memory load/modify/save workflows without filesystem access.

use std::collections::HashSet;
use std::io::Cursor;
use std::path::PathBuf;

use wasm_bindgen::prelude::*;

use audex::diff::TagDiff;
use audex::{DynamicFileType, File};

use crate::error::{catch_panic, catch_panic_with_status, to_js_error};
use crate::stream_info::{self, WasmStreamInfo};

/// Best-effort MIME sniffing for image payloads used by cover-art helpers.
///
/// `setAsfCoverArt()` accepts raw bytes without a MIME string, but ASF
/// `WM/Picture` payloads require one. We infer the common image formats from
/// their signatures and fall back to a generic binary type when the format is
/// unknown.
fn sniff_image_mime(data: &[u8]) -> &'static str {
    if data.len() >= 3 && data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return "image/jpeg";
    }
    if data.len() >= 8 && data.starts_with(b"\x89PNG\r\n\x1A\n") {
        return "image/png";
    }
    if data.len() >= 6 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        return "image/gif";
    }
    if data.len() >= 2 && data.starts_with(b"BM") {
        return "image/bmp";
    }
    if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return "image/webp";
    }
    "application/octet-stream"
}

/// Serialize a serde-compatible value to a plain JS object via JSON.
///
/// serde_wasm_bindgen::to_value serializes HashMaps as JS Map objects
/// rather than plain objects, which breaks Object.keys() / Object.entries()
/// on the JS side.  Going through serde_json → JSON.parse avoids this.
fn to_js_object<T: serde::Serialize>(value: &T) -> Result<JsValue, JsValue> {
    let json = serde_json::to_string(value)
        .map_err(|e| JsValue::from_str(&format!("serialization error: {e}")))?;
    js_sys::JSON::parse(&json).map_err(|e| JsValue::from_str(&format!("JSON parse error: {e:?}")))
}

/// An audio file loaded entirely in memory.
///
/// Wraps audex's `DynamicFileType` and exposes tag reading, writing,
/// stream info, and serialization to JavaScript consumers.
///
/// # Memory management
///
/// Call `.free()` when you are done with the instance to release the
/// WASM-side heap allocation.  Failing to do so will leak memory.
#[wasm_bindgen]
pub struct AudioFile {
    inner: DynamicFileType,
    // Keep the raw bytes so save() can produce an updated copy of the
    // full audio file, not just the metadata.
    original_bytes: Vec<u8>,
    // Set to true when a panic is caught during a method call. Once
    // poisoned, the instance must be discarded — internal invariants
    // may have been violated by the interrupted operation.
    poisoned: bool,
}

// ---------------------------------------------------------------------------
// Input size limits for deserialized tag data
// ---------------------------------------------------------------------------

/// Maximum number of tag entries (key-value pairs) accepted from JS callers.
/// Prevents denial-of-service via payloads with millions of keys.
const MAX_TAG_ENTRIES: usize = 10_000;

/// Maximum total string bytes across all keys and values (50 MB).
/// Guards against payloads that fit within the entry count limit but contain
/// extremely large string values that would exhaust WASM linear memory.
const MAX_TAG_TOTAL_BYTES: usize = 50 * 1024 * 1024;

/// Compute the total string byte count of all tags currently stored in a file.
///
/// Iterates every tag key and value to produce an accurate byte total.
/// Used after mutations to enforce the per-instance cumulative tag budget.
fn current_tag_bytes(file: &DynamicFileType) -> usize {
    file.items().iter().fold(0usize, |acc, (key, values)| {
        acc.saturating_add(key.len())
            .saturating_add(values.iter().fold(0usize, |a, v| a.saturating_add(v.len())))
    })
}

/// Check that the total tag data stored in the file does not exceed the
/// per-instance budget. Returns an error suitable for returning to JS
/// callers if the limit is breached.
fn check_tag_budget(file: &DynamicFileType) -> Result<(), JsValue> {
    let total = current_tag_bytes(file);
    if total > MAX_TAG_TOTAL_BYTES {
        return Err(JsValue::from_str(&format!(
            "Total tag data ({} bytes) exceeds per-instance budget ({} bytes). \
             Remove some tags before adding more.",
            total, MAX_TAG_TOTAL_BYTES
        )));
    }
    Ok(())
}

/// Validates that a deserialized tag collection does not exceed safe size limits.
///
/// Returns an error message if the collection has too many entries or if the
/// aggregate string byte count exceeds the allowed budget.
fn validate_tag_payload(pairs: &[(String, Vec<String>)]) -> Result<(), String> {
    if pairs.len() > MAX_TAG_ENTRIES {
        return Err(format!(
            "Tag payload has {} entries, maximum allowed is {}",
            pairs.len(),
            MAX_TAG_ENTRIES,
        ));
    }

    // Use saturating arithmetic throughout to prevent usize overflow on
    // 32-bit platforms (e.g. wasm32) where many large values could wrap.
    let total_bytes: usize = pairs.iter().fold(0usize, |acc, (key, values)| {
        acc.saturating_add(key.len())
            .saturating_add(values.iter().fold(0usize, |a, v| a.saturating_add(v.len())))
    });

    if total_bytes > MAX_TAG_TOTAL_BYTES {
        return Err(format!(
            "Tag payload total string size ({} bytes) exceeds maximum ({} bytes)",
            total_bytes, MAX_TAG_TOTAL_BYTES,
        ));
    }

    Ok(())
}

/// Maximum input file size (256 MB).
///
/// save() requires cloning `original_bytes` to hand ownership to JS, so peak
/// memory during a save is roughly 2x the input size (the live buffer plus
/// the clone returned to the caller). A backup clone for rollback on write
/// failure brings the worst case to ~3x. At 256 MB input this means ~768 MB
/// peak, which is still within WASM's typical 2 GB linear memory ceiling.
const MAX_AUDIO_INPUT_BYTES: usize = 256 * 1024 * 1024;

fn audio_input_len_error(len: usize) -> Option<String> {
    if len > MAX_AUDIO_INPUT_BYTES {
        return Some(format!(
            "Input size ({} bytes) exceeds maximum allowed ({} bytes)",
            len, MAX_AUDIO_INPUT_BYTES
        ));
    }
    None
}

fn check_audio_input_len(len: usize) -> Result<(), JsValue> {
    if let Some(message) = audio_input_len_error(len) {
        return Err(JsValue::from_str(&message));
    }
    Ok(())
}

/// Validate a single-key string array before converting it into a full Rust collection.
///
/// `running_total` tracks accumulated string bytes across all keys in the
/// outer payload, ensuring the 50 MB budget is shared rather than reset
/// per key.
fn validate_js_string_array_payload(
    _key: &str,
    values: &JsValue,
    method: &str,
    running_total: &mut usize,
) -> Result<(), JsValue> {
    if !js_sys::Array::is_array(values) {
        return Err(JsValue::from_str(&format!(
            "{} expects an array of strings",
            method
        )));
    }

    for value in js_sys::Array::from(values).iter() {
        let string_value = value.as_string().ok_or_else(|| {
            JsValue::from_str(&format!("{} expects every value to be a string", method))
        })?;
        *running_total = running_total.saturating_add(string_value.len());
        if *running_total > MAX_TAG_TOTAL_BYTES {
            return Err(JsValue::from_str(&format!(
                "Tag payload total string size ({} bytes) exceeds maximum ({} bytes)",
                *running_total, MAX_TAG_TOTAL_BYTES
            )));
        }
    }

    Ok(())
}

/// Validate an object shaped like `{ key: [values] }` before deserialization.
fn validate_js_update_payload(value: &JsValue) -> Result<(), JsValue> {
    if value.is_null()
        || value.is_undefined()
        || js_sys::Array::is_array(value)
        || !value.is_object()
    {
        return Err(JsValue::from_str(
            "expected { key: [values] } object with string-array values",
        ));
    }

    let object = js_sys::Object::from(value.clone());
    let keys = js_sys::Object::keys(&object);
    if keys.length() as usize > MAX_TAG_ENTRIES {
        return Err(JsValue::from_str(&format!(
            "Tag payload has {} entries, maximum allowed is {}",
            keys.length(),
            MAX_TAG_ENTRIES
        )));
    }

    let mut total_bytes = 0usize;
    for key_js in keys.iter() {
        let key = key_js
            .as_string()
            .ok_or_else(|| JsValue::from_str("expected object keys to be strings"))?;
        total_bytes = total_bytes.saturating_add(key.len());
        if total_bytes > MAX_TAG_TOTAL_BYTES {
            return Err(JsValue::from_str(&format!(
                "Tag payload total string size ({} bytes) exceeds maximum ({} bytes)",
                total_bytes, MAX_TAG_TOTAL_BYTES
            )));
        }

        let values = js_sys::Reflect::get(value, &key_js)
            .map_err(|_| JsValue::from_str("failed to read tag values from input object"))?;
        validate_js_string_array_payload(&key, &values, "updateFromJson", &mut total_bytes)?;
    }

    Ok(())
}

/// Validate a snapshot shaped like `[[key, [values]], ...]` before deserialization.
fn validate_js_snapshot_payload(value: &JsValue) -> Result<(), JsValue> {
    if !js_sys::Array::is_array(value) {
        return Err(JsValue::from_str(
            "snapshot must be an array of [key, [values]] pairs",
        ));
    }

    let pairs = js_sys::Array::from(value);
    if pairs.length() as usize > MAX_TAG_ENTRIES {
        return Err(JsValue::from_str(&format!(
            "Tag payload has {} entries, maximum allowed is {}",
            pairs.length(),
            MAX_TAG_ENTRIES
        )));
    }

    let mut total_bytes = 0usize;
    for pair_value in pairs.iter() {
        if !js_sys::Array::is_array(&pair_value) {
            return Err(JsValue::from_str(
                "snapshot entries must be [key, [values]] pairs",
            ));
        }

        let pair = js_sys::Array::from(&pair_value);
        if pair.length() != 2 {
            return Err(JsValue::from_str(
                "snapshot entries must contain exactly two elements",
            ));
        }

        let key_value = pair.get(0);
        let key = key_value
            .as_string()
            .ok_or_else(|| JsValue::from_str("snapshot keys must be strings"))?;
        total_bytes = total_bytes.saturating_add(key.len());
        if total_bytes > MAX_TAG_TOTAL_BYTES {
            return Err(JsValue::from_str(&format!(
                "Tag payload total string size ({} bytes) exceeds maximum ({} bytes)",
                total_bytes, MAX_TAG_TOTAL_BYTES
            )));
        }

        let values = pair.get(1);
        validate_js_string_array_payload(&key, &values, "diffAgainstSnapshot", &mut total_bytes)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Construction and format detection
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl AudioFile {
    /// Load audio metadata from a byte buffer.
    ///
    /// The optional `filename` parameter aids format detection — pass
    /// the original filename (e.g. `"track.mp3"`) or just an extension
    /// (e.g. `".flac"`).  When omitted, detection relies on magic bytes.
    #[wasm_bindgen(constructor)]
    pub fn new(data: &[u8], filename: Option<String>) -> Result<AudioFile, JsValue> {
        // Reject excessively large inputs to prevent OOM in WASM environments.
        // The constructor copies the input via to_vec(), so peak memory usage
        // is roughly 2x the input size. Additionally, save() clones the buffer
        // on return, creating a third simultaneous copy. With 3x peak memory
        // amplification during save, 256 MB input requires ~768 MB — still
        // within WASM's typical 2 GB linear memory ceiling while leaving room
        // for other allocations (parsed metadata, tag updates, etc.).
        check_audio_input_len(data.len())?;

        // Keep one owned copy of the input. The parser reads from a borrowed
        // cursor so no second allocation is needed.
        let original_bytes = data.to_vec();

        // SAFETY: The closure captures `original_bytes` by move and `filename`
        // by move. No `&mut self` exists yet (we are constructing the value),
        // so there is no partially-mutable state that could be left inconsistent
        // if a panic occurs.
        catch_panic(|| {
            let cursor = Cursor::new(&original_bytes[..]);

            // Convert the optional filename string into a PathBuf hint for
            // audex's format detection scoring system.
            let hint = filename.map(PathBuf::from);

            let inner = File::load_from_reader(cursor, hint).map_err(to_js_error)?;

            Ok(AudioFile {
                inner,
                original_bytes,
                poisoned: false,
            })
        })
    }

    /// Return the detected format name (e.g. "MP3", "FLAC", "MP4").
    #[wasm_bindgen(js_name = "formatName")]
    pub fn format_name(&self) -> String {
        if self.poison_read_fallback() {
            return "Poisoned".to_string();
        }
        self.inner.format_name().to_string()
    }
}

// ---------------------------------------------------------------------------
// Tag reading
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl AudioFile {
    /// Get all values for a tag key.
    ///
    /// Returns a JS array of strings, or `null` if the key does not exist.
    /// Throws an error if tag values cannot be serialized.
    /// Tags can be multi-valued (e.g. multiple artists).
    pub fn get(&self, key: &str) -> Result<JsValue, JsValue> {
        self.check_poisoned()?;
        match self.inner.get(key) {
            Some(values) => serde_wasm_bindgen::to_value(&values).map_err(|e| {
                JsValue::from_str(&format!(
                    "Failed to serialize tag values for '{}': {}",
                    key, e
                ))
            }),
            None => Ok(JsValue::NULL),
        }
    }

    /// Get the first value for a tag key, or `null`.
    ///
    /// Convenience shorthand when you know the tag is single-valued.
    #[wasm_bindgen(js_name = "getFirst")]
    pub fn get_first(&self, key: &str) -> Option<String> {
        if self.poison_read_fallback() {
            return None;
        }
        self.inner.get_first(key).map(|s| s.to_string())
    }

    /// List all tag keys present in the file.
    ///
    /// Throws an error if the key list cannot be serialized.
    pub fn keys(&self) -> Result<JsValue, JsValue> {
        self.check_poisoned()?;
        let keys = self.inner.keys();
        serde_wasm_bindgen::to_value(&keys)
            .map_err(|e| JsValue::from_str(&format!("Failed to serialize tag keys: {}", e)))
    }

    /// Check whether a tag key exists.
    #[wasm_bindgen(js_name = "containsKey")]
    pub fn contains_key(&self, key: &str) -> bool {
        if self.poison_read_fallback() {
            return false;
        }
        self.inner.contains_key(key)
    }

    /// Get all tags as a JSON-compatible object: `{ key: [values] }`.
    #[wasm_bindgen(js_name = "tagsJson")]
    pub fn tags_json(&self) -> Result<JsValue, JsValue> {
        self.check_poisoned()?;
        let items = self.inner.items();
        let map: std::collections::HashMap<String, Vec<String>> = items.into_iter().collect();
        to_js_object(&map)
    }

    /// Get raw bytes for an APE binary tag (e.g. "Cover Art (Front)").
    ///
    /// Returns the raw binary data as a `Uint8Array`, or `null` if the
    /// key does not exist or is not a binary value.  Works for APE-based
    /// formats only (TrueAudio, MonkeysAudio, Musepack, WavPack,
    /// OptimFROG, TAK).
    #[wasm_bindgen(js_name = "getApeBinaryTag")]
    pub fn get_ape_binary_tag(&self, key: &str) -> Option<Vec<u8>> {
        if self.poison_read_fallback() {
            return None;
        }
        use audex::apev2::{APEValueType, APEv2Tags};

        fn extract(tags: &APEv2Tags, key: &str) -> Option<Vec<u8>> {
            let val = tags.get(key)?;
            if val.value_type == APEValueType::Binary {
                Some(val.data.clone())
            } else {
                None
            }
        }

        if let Some(tta) = self.inner.downcast_ref::<audex::trueaudio::TrueAudio>() {
            return tta.ape_tags().and_then(|t| extract(t, key));
        }
        if let Some(ape) = self
            .inner
            .downcast_ref::<audex::monkeysaudio::MonkeysAudio>()
        {
            return ape.tags.as_ref().and_then(|t| extract(t, key));
        }
        if let Some(mpc) = self.inner.downcast_ref::<audex::musepack::Musepack>() {
            return mpc.tags.as_ref().and_then(|t| extract(t, key));
        }
        if let Some(wv) = self.inner.downcast_ref::<audex::wavpack::WavPack>() {
            return wv.tags.as_ref().and_then(|t| extract(t, key));
        }
        if let Some(ofr) = self.inner.downcast_ref::<audex::optimfrog::OptimFROG>() {
            return ofr.tags.as_ref().and_then(|t| extract(t, key));
        }
        if let Some(tak) = self.inner.downcast_ref::<audex::tak::TAK>() {
            return tak.tags.as_ref().and_then(|t| extract(t, key));
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Stream info
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl AudioFile {
    /// Get audio stream properties (duration, bitrate, sample rate, etc.).
    #[wasm_bindgen(js_name = "streamInfo")]
    pub fn stream_info(&self) -> WasmStreamInfo {
        if self.poison_read_fallback() {
            return stream_info::empty();
        }
        stream_info::from_dynamic(&self.inner.info())
    }

    /// Get stream info as a plain JSON object for easy consumption.
    #[wasm_bindgen(js_name = "streamInfoJson")]
    pub fn stream_info_json(&self) -> Result<JsValue, JsValue> {
        self.check_poisoned()?;
        let snapshot = audex::snapshot::StreamInfoSnapshot::from_dynamic(&self.inner.info());
        to_js_object(&snapshot)
    }
}

// ---------------------------------------------------------------------------
// Tag container management
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl AudioFile {
    /// Check whether the file already has a tag container.
    ///
    /// Some formats (e.g. an untagged MP3) need `addTags()` called
    /// before any `set()` / `setSingle()` calls will succeed.
    #[wasm_bindgen(js_name = "hasTags")]
    pub fn has_tags(&self) -> bool {
        if self.poison_read_fallback() {
            return false;
        }
        self.inner.has_tags()
    }

    /// Create an empty tag container for the file's format.
    ///
    /// This is required before writing tags to a file that was loaded
    /// without any existing metadata.  Safe to call on files that
    /// already have tags (it is a no-op in that case for most formats).
    /// Wrap in `run_mutation_with_poison` so that a panic during tag
    /// container creation correctly marks the instance as poisoned.
    #[wasm_bindgen(js_name = "addTags")]
    pub fn add_tags(&mut self) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| this.inner.add_tags().map_err(to_js_error))
    }
}

// ---------------------------------------------------------------------------
// Internal helpers for tag budget enforcement
// ---------------------------------------------------------------------------

impl AudioFile {
    /// Return an error if this instance was poisoned by a previous panic.
    ///
    /// After a caught panic, internal state may be inconsistent (partially
    /// applied mutations, moved-out buffers, etc.). All public entry points
    /// must call this before doing any work.
    fn check_poisoned(&self) -> Result<(), JsValue> {
        if self.poisoned {
            return Err(JsValue::from_str(
                "AudioFile is in an inconsistent state after a previous panic; \
                 please reload the file",
            ));
        }
        Ok(())
    }

    fn run_mutation_with_poison<T, F>(&mut self, f: F) -> Result<T, JsValue>
    where
        F: FnOnce(&mut Self) -> Result<T, JsValue>,
    {
        self.check_poisoned()?;
        // SAFETY: The closure receives `&mut Self` and may mutate tag fields
        // through the audex API. If a panic occurs mid-mutation, the instance
        // is immediately poisoned below, preventing any further use of
        // potentially inconsistent state.
        let caught = catch_panic_with_status(|| f(self));
        if caught.panicked {
            self.poisoned = true;
        }
        caught.result
    }

    fn restore_backup_after_panic(&mut self, backup: Vec<u8>) -> bool {
        if self.original_bytes.is_empty() {
            self.original_bytes = backup;
            return true;
        }
        false
    }

    /// Run a closure that mutates `original_bytes` through cursor I/O, with
    /// automatic backup, restore-on-error, and poison-on-panic semantics.
    ///
    /// This consolidates the shared pattern used by `save()` and `clear()`:
    ///   1. Clone `original_bytes` as a backup.
    ///   2. Move the buffer into a cursor (avoids holding two copies at once).
    ///   3. Run the closure, which performs I/O on the cursor.
    ///   4. On success, store the cursor contents back and return the result.
    ///   5. On error, restore the cursor contents so the instance stays usable.
    ///   6. On panic, restore from backup and poison the instance.
    fn run_byte_mutation_with_poison<T, F>(&mut self, f: F) -> Result<T, JsValue>
    where
        F: FnOnce(&mut Self, &mut Cursor<Vec<u8>>) -> Result<T, JsValue>,
    {
        self.check_poisoned()?;
        let backup = self.original_bytes.clone();
        let mut completed = false;
        // SAFETY: The closure operates on `self` via a cursor that owns the
        // byte buffer. If a panic occurs before `completed` is set, the backup
        // is restored and the instance is poisoned, preventing use of any
        // partially-written state.
        let caught = catch_panic_with_status(|| {
            let buf = std::mem::take(&mut self.original_bytes);
            let mut cursor = Cursor::new(buf);
            let result = f(self, &mut cursor);
            // Always reclaim the buffer from the cursor, whether the closure
            // succeeded or failed. The caller can still retry after a
            // recoverable error.
            self.original_bytes = cursor.into_inner();
            if result.is_ok() {
                completed = true;
            }
            result
        });
        if caught.panicked && !completed {
            let _ = self.restore_backup_after_panic(backup);
            self.poisoned = true;
        }
        caught.result
    }

    /// Check poison state for read-only methods that return direct values
    /// (not `Result`). Returns `true` if poisoned so callers can return a
    /// safe default, avoiding a panic that would abort the WASM runtime.
    fn poison_read_fallback(&self) -> bool {
        if self.poisoned {
            web_sys::console::warn_1(&JsValue::from_str(
                "AudioFile is poisoned after a previous panic; returning default value",
            ));
            return true;
        }
        false
    }

    /// Verify cumulative tag budget after a single-key mutation. If the total
    /// exceeds the limit, roll back to `prev` (or remove the key) and return
    /// the budget error.
    fn check_budget_or_rollback(
        &mut self,
        key: &str,
        prev: Option<Vec<String>>,
    ) -> Result<(), JsValue> {
        if let Err(e) = check_tag_budget(&self.inner) {
            if let Some(old) = prev {
                let _ = self.inner.set(key, old);
            } else {
                let _ = self.inner.remove(key);
            }
            return Err(e);
        }
        Ok(())
    }

    /// Restore all tags to a previously captured snapshot. Removes any keys
    /// that were not present in the snapshot and resets existing keys to their
    /// prior values. Used for rollback after a budget violation.
    fn restore_tag_snapshot(&mut self, snapshot: Vec<(String, Vec<String>)>) {
        // Collect current keys so we can remove any that were added.
        let current_keys: Vec<String> = self.inner.keys();
        let snapshot_keys: std::collections::HashSet<&str> =
            snapshot.iter().map(|(k, _)| k.as_str()).collect();

        for key in &current_keys {
            if !snapshot_keys.contains(key.as_str()) {
                let _ = self.inner.remove(key);
            }
        }
        for (key, values) in snapshot {
            let _ = self.inner.set(&key, values);
        }
    }
}

// ---------------------------------------------------------------------------
// Tag writing
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl AudioFile {
    /// Set a tag to one or more values.
    ///
    /// Pass a JS array of strings (e.g. `["Artist A", "Artist B"]`).
    pub fn set(&mut self, key: &str, values: JsValue) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            validate_js_string_array_payload(key, &values, "set", &mut 0)?;
            let vals: Vec<String> = serde_wasm_bindgen::from_value(values)
                .map_err(|e| JsValue::from_str(&format!("expected string array: {e}")))?;
            validate_tag_payload(&[(key.to_string(), vals.clone())])
                .map_err(|msg| JsValue::from_str(&msg))?;
            let prev = this.inner.get(key).map(|v| v.to_vec());
            this.inner.set(key, vals).map_err(to_js_error)?;
            this.check_budget_or_rollback(key, prev)
        })
    }

    /// Set a tag to a single string value (convenience method).
    #[wasm_bindgen(js_name = "setSingle")]
    pub fn set_single(&mut self, key: &str, value: &str) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            validate_tag_payload(&[(key.to_string(), vec![value.to_string()])])
                .map_err(|msg| JsValue::from_str(&msg))?;
            let prev = this.inner.get(key).map(|v| v.to_vec());
            this.inner
                .set(key, vec![value.to_string()])
                .map_err(to_js_error)?;
            this.check_budget_or_rollback(key, prev)
        })
    }

    /// Remove a tag by key.
    pub fn remove(&mut self, key: &str) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| this.inner.remove(key).map_err(to_js_error))
    }

    /// Remove all tags from the file.
    ///
    /// Uses cursor-based I/O internally so it works without filesystem
    /// access.  The cleared state is written into the stored bytes
    /// immediately so a subsequent `save()` reflects the change.
    pub fn clear(&mut self) -> Result<(), JsValue> {
        self.run_byte_mutation_with_poison(|this, cursor| {
            this.inner.clear_writer(cursor).map_err(to_js_error)
        })
    }

    /// Batch-update tags from a JSON object: `{ key: [values] }`.
    ///
    /// Existing tags not present in the input are left unchanged.
    #[wasm_bindgen(js_name = "updateFromJson")]
    pub fn update_from_json(&mut self, json: JsValue) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            validate_js_update_payload(&json)?;
            let map: std::collections::HashMap<String, Vec<String>> =
                serde_wasm_bindgen::from_value(json).map_err(|e| {
                    JsValue::from_str(&format!("expected {{ key: [values] }} object: {e}"))
                })?;

            let pairs: Vec<(String, Vec<String>)> = map.into_iter().collect();
            validate_tag_payload(&pairs).map_err(|msg| JsValue::from_str(&msg))?;

            let full_snapshot = this.inner.items();

            if let Err(e) = this.inner.update(pairs).map_err(to_js_error) {
                this.restore_tag_snapshot(full_snapshot);
                return Err(e);
            }

            if let Err(e) = check_tag_budget(&this.inner) {
                this.restore_tag_snapshot(full_snapshot);
                return Err(e);
            }
            Ok(())
        })
    }
    /// Set a single ID3 tag with a specific text encoding.
    ///
    /// Encoding values: 0 = Latin-1, 1 = UTF-16 (with BOM), 2 = UTF-16BE, 3 = UTF-8.
    ///
    /// The encoding parameter only takes effect for ID3-based formats (MP3, AIFF,
    /// WAVE, DSF, DSDIFF). For other formats (Vorbis, MP4, APE, etc.), UTF-8 is
    /// always used regardless of this parameter, since those formats do not support
    /// alternative text encodings.
    #[wasm_bindgen(js_name = "setTagWithEncoding")]
    pub fn set_tag_with_encoding(
        &mut self,
        key: &str,
        values: JsValue,
        encoding: u8,
    ) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            validate_js_string_array_payload(key, &values, "setTagWithEncoding", &mut 0)?;
            let vals: Vec<String> = serde_wasm_bindgen::from_value(values)
                .map_err(|e| JsValue::from_str(&format!("expected string array: {e}")))?;
            validate_tag_payload(&[(key.to_string(), vals.clone())])
                .map_err(|msg| JsValue::from_str(&msg))?;

            if encoding > 3 {
                return Err(JsValue::from_str(&format!(
                    "Invalid text encoding {}: must be 0 (Latin-1), 1 (UTF-16), 2 (UTF-16BE), or 3 (UTF-8)",
                    encoding
                )));
            }
            use audex::id3::TextEncoding;
            let enc = TextEncoding::from_byte(encoding)
                .map_err(|err| JsValue::from_str(&err.to_string()))?;

            let prev = this.inner.get(key).map(|v| v.to_vec());

            if let Some(mp3) = this.inner.downcast_mut::<audex::mp3::MP3>() {
                match mp3.tags {
                    Some(ref mut tags) => tags.set_with_encoding(key, vals, enc).map_err(to_js_error)?,
                    None => {
                        return Err(JsValue::from_str(
                            "MP3 file has no ID3 tags — load or create tags first",
                        ));
                    }
                }
                return this.check_budget_or_rollback(key, prev);
            }
            if let Some(aiff) = this.inner.downcast_mut::<audex::aiff::AIFF>() {
                match aiff.tags {
                    Some(ref mut tags) => tags.set_with_encoding(key, vals, enc).map_err(to_js_error)?,
                    None => {
                        return Err(JsValue::from_str(
                            "AIFF file has no ID3 tags — load or create tags first",
                        ));
                    }
                }
                return this.check_budget_or_rollback(key, prev);
            }
            if let Some(wave) = this.inner.downcast_mut::<audex::wave::WAVE>() {
                match wave.tags {
                    Some(ref mut tags) => tags.set_with_encoding(key, vals, enc).map_err(to_js_error)?,
                    None => {
                        return Err(JsValue::from_str(
                            "WAVE file has no ID3 tags — load or create tags first",
                        ));
                    }
                }
                return this.check_budget_or_rollback(key, prev);
            }
            if let Some(dsf) = this.inner.downcast_mut::<audex::dsf::DSF>() {
                match dsf.tags {
                    Some(ref mut tags) => tags.set_with_encoding(key, vals, enc).map_err(to_js_error)?,
                    None => {
                        return Err(JsValue::from_str(
                            "DSF file has no ID3 tags — load or create tags first",
                        ));
                    }
                }
                return this.check_budget_or_rollback(key, prev);
            }
            if let Some(dsdiff) = this.inner.downcast_mut::<audex::dsdiff::DSDIFF>() {
                match dsdiff.tags {
                    Some(ref mut tags) => tags.set_with_encoding(key, vals, enc).map_err(to_js_error)?,
                    None => {
                        return Err(JsValue::from_str(
                            "DSDIFF file has no ID3 tags — load or create tags first",
                        ));
                    }
                }
                return this.check_budget_or_rollback(key, prev);
            }

            this.inner.set(key, vals).map_err(to_js_error)?;
            this.check_budget_or_rollback(key, prev)
        })
    }
}

// ---------------------------------------------------------------------------
// Format-specific: ID3 binary frames (cover art, ReplayGain)
// ---------------------------------------------------------------------------

/// Maximum allowed size for cover art data (50 MB).
///
/// Cover art is cloned during embedding and coexists with the full original
/// file buffer in memory. A generous but bounded limit prevents OOM in WASM
/// environments while still accommodating unusually large artwork. Typical
/// album covers are well under 5 MB.
const MAX_COVER_ART_BYTES: usize = 50 * 1024 * 1024;

/// Validate that cover art data does not exceed the size limit.
fn check_cover_art_size(data: &[u8], method: &str) -> Result<(), JsValue> {
    if data.len() > MAX_COVER_ART_BYTES {
        return Err(JsValue::from_str(&format!(
            "{}: cover art data ({} bytes) exceeds maximum allowed size ({} bytes)",
            method,
            data.len(),
            MAX_COVER_ART_BYTES
        )));
    }
    Ok(())
}

/// Replace all RVA2 frames for a key and restore the previous state if the
/// insert step fails.
fn replace_rva2_frames_atomically<F>(
    tags: &mut audex::id3::ID3Tags,
    key: &str,
    add_new: F,
) -> audex::Result<()>
where
    F: FnOnce(&mut audex::id3::ID3Tags) -> audex::Result<()>,
{
    let existing_frames: Vec<audex::id3::RVA2> = tags
        .getall(key)
        .iter()
        .filter_map(|frame| frame.as_any().downcast_ref::<audex::id3::RVA2>())
        .map(|frame| audex::id3::RVA2::new(frame.identification.clone(), frame.channels.clone()))
        .collect();

    tags.delall(key);
    if let Err(err) = add_new(tags) {
        for frame in existing_frames {
            let _ = tags.add(Box::new(frame));
        }
        return Err(err);
    }

    Ok(())
}

fn mutate_with_rollback<T, E, F>(state: &mut T, mutate: F) -> Result<(), E>
where
    T: Clone,
    F: FnOnce(&mut T) -> Result<(), E>,
{
    let snapshot = state.clone();
    if let Err(err) = mutate(state) {
        *state = snapshot;
        return Err(err);
    }
    Ok(())
}

#[wasm_bindgen]
impl AudioFile {
    /// Embed cover art as an APIC frame (ID3-based formats only).
    ///
    /// Accepts raw image bytes and an image MIME type string (for example JPEG,
    /// PNG, GIF, BMP, or other `image/*` types).
    /// Returns an error for non-ID3 formats.
    #[wasm_bindgen(js_name = "setCoverArt")]
    pub fn set_cover_art(&mut self, data: &[u8], mime_type: &str) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            check_cover_art_size(data, "setCoverArt")?;

            // Validate MIME type: must be a non-empty image MIME type.
            // Common types (jpeg, jpg, png, gif, bmp, webp) are accepted explicitly,
            // and any other "image/" prefix is allowed for forward compatibility.
            if mime_type.is_empty() || !mime_type.starts_with("image/") {
                return Err(JsValue::from_str(
                    "setCoverArt: invalid MIME type. Expected an image MIME type \
                 (e.g. image/jpeg, image/png, image/gif, image/bmp, image/webp).",
                ));
            }

            use audex::id3::{APIC, PictureType, TextEncoding};

            // Helper: set cover art on an ID3 tag container, returning an error
            // if the tag container is absent (None).
            fn set_id3_cover(
                tags: Option<&mut audex::id3::tags::ID3Tags>,
                data: &[u8],
                mime_type: &str,
                format_name: &str,
            ) -> Result<(), JsValue> {
                let tags = tags.ok_or_else(|| {
                    JsValue::from_str(&format!(
                        "setCoverArt: {} file has no ID3 tag container — \
                     call addTags() first or load a file that already has tags",
                        format_name
                    ))
                })?;
                mutate_with_rollback(tags, |tags| {
                    tags.delall("APIC:");
                    let frame = APIC {
                        encoding: TextEncoding::Utf16,
                        mime: mime_type.to_string(),
                        type_: PictureType::CoverFront,
                        desc: "Cover".to_string(),
                        data: data.to_vec(),
                    };
                    tags.add(Box::new(frame))
                })
                .map_err(to_js_error)?;
                Ok(())
            }

            // Try each ID3-based format in order
            if let Some(mp3) = this.inner.downcast_mut::<audex::mp3::MP3>() {
                return set_id3_cover(mp3.tags.as_mut(), data, mime_type, "MP3");
            }
            if let Some(aiff) = this.inner.downcast_mut::<audex::aiff::AIFF>() {
                return set_id3_cover(aiff.tags.as_mut(), data, mime_type, "AIFF");
            }
            if let Some(wave) = this.inner.downcast_mut::<audex::wave::WAVE>() {
                return set_id3_cover(wave.tags.as_mut(), data, mime_type, "WAVE");
            }
            if let Some(dsf) = this.inner.downcast_mut::<audex::dsf::DSF>() {
                return set_id3_cover(dsf.tags.as_mut(), data, mime_type, "DSF");
            }
            if let Some(dsdiff) = this.inner.downcast_mut::<audex::dsdiff::DSDIFF>() {
                return set_id3_cover(dsdiff.tags.as_mut(), data, mime_type, "DSDIFF");
            }

            Err(JsValue::from_str(
                "setCoverArt: format does not support ID3 cover art",
            ))
        })
    }

    /// Embed cover art as an APE binary value (APEv2-based formats only).
    ///
    /// Stores raw image bytes under "Cover Art (Front)".
    /// Supported formats: TrueAudio, MonkeysAudio, Musepack, WavPack, OptimFROG, TAK.
    #[wasm_bindgen(js_name = "setApeCoverArt")]
    pub fn set_ape_cover_art(&mut self, data: &[u8]) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            check_cover_art_size(data, "setApeCoverArt")?;
            use audex::apev2::APEv2Tags;

            fn insert_cover(tags: &mut APEv2Tags, data: &[u8]) -> audex::Result<()> {
                mutate_with_rollback(tags, |tags| {
                    tags.remove_cover_art("Cover Art (Front)");
                    tags.add_cover_art("Cover Art (Front)", data.to_vec())
                })
            }

            // Helper: return an error when tags are absent
            fn require_tags(
                tags: Option<&mut APEv2Tags>,
                data: &[u8],
                format_name: &str,
            ) -> Result<(), JsValue> {
                let tags = tags.ok_or_else(|| {
                    JsValue::from_str(&format!(
                        "setApeCoverArt: {} file has no APE tag container — \
                         call addTags() first or load a file that already has tags",
                        format_name
                    ))
                })?;
                insert_cover(tags, data).map_err(to_js_error)
            }

            if let Some(tta) = this.inner.downcast_mut::<audex::trueaudio::TrueAudio>() {
                return require_tags(tta.ape_tags_mut(), data, "TrueAudio");
            }
            if let Some(ape) = this
                .inner
                .downcast_mut::<audex::monkeysaudio::MonkeysAudio>()
            {
                return require_tags(ape.tags.as_mut(), data, "MonkeysAudio");
            }
            if let Some(mpc) = this.inner.downcast_mut::<audex::musepack::Musepack>() {
                return require_tags(mpc.tags.as_mut(), data, "Musepack");
            }
            if let Some(wv) = this.inner.downcast_mut::<audex::wavpack::WavPack>() {
                return require_tags(wv.tags.as_mut(), data, "WavPack");
            }
            if let Some(ofr) = this.inner.downcast_mut::<audex::optimfrog::OptimFROG>() {
                return require_tags(ofr.tags.as_mut(), data, "OptimFROG");
            }
            if let Some(tak) = this.inner.downcast_mut::<audex::tak::TAK>() {
                return require_tags(tak.tags.as_mut(), data, "TAK");
            }

            Err(JsValue::from_str(
                "setApeCoverArt: format does not support APE cover art",
            ))
        })
    }

    /// Embed cover art as a structured `WM/Picture` attribute (ASF/WMA only).
    #[wasm_bindgen(js_name = "setAsfCoverArt")]
    pub fn set_asf_cover_art(&mut self, data: &[u8]) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            check_cover_art_size(data, "setAsfCoverArt")?;
            use audex::asf::attrs::{ASFAttribute, ASFPicture, ASFPictureType};

            if let Some(asf) = this.inner.downcast_mut::<audex::asf::ASF>() {
                mutate_with_rollback(&mut asf.tags, |tags| {
                    tags.remove("WM/Picture");
                    let picture = ASFPicture::new(
                        ASFPictureType::FrontCover,
                        sniff_image_mime(data).to_string(),
                        "Cover".to_string(),
                        data.to_vec(),
                    );
                    let attr = ASFAttribute::picture(picture)?;
                    tags.add("WM/Picture".to_string(), attr);
                    Ok(())
                })
                .map_err(to_js_error)?;
                return Ok(());
            }
            Err(JsValue::from_str(
                "setAsfCoverArt: file is not an ASF/WMA format",
            ))
        })
    }

    /// Embed cover art in an MP4/M4A file (covr atom).
    #[wasm_bindgen(js_name = "setMp4CoverArt")]
    pub fn set_mp4_cover_art(&mut self, data: &[u8], mime_type: &str) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            check_cover_art_size(data, "setMp4CoverArt")?;
            use audex::mp4::MP4Cover;

            if let Some(mp4) = this.inner.downcast_mut::<audex::mp4::MP4>() {
                let tags = mp4.tags.as_mut().ok_or_else(|| {
                    JsValue::from_str(
                        "setMp4CoverArt: MP4 file has no tag container — \
                     call addTags() first or load a file that already has tags",
                    )
                })?;
                // Validate MIME type: only JPEG and PNG are supported for MP4 cover art
                let mime_lower = mime_type.to_ascii_lowercase();
                if mime_lower != "image/jpeg"
                    && mime_lower != "image/jpg"
                    && mime_lower != "image/png"
                {
                    return Err(JsValue::from_str(
                        "setMp4CoverArt: unsupported MIME type. Only image/jpeg and image/png \
                     are supported for MP4 cover art.",
                    ));
                }

                tags.covers.clear();
                let cover = if mime_lower == "image/png" {
                    MP4Cover::new_png(data.to_vec())
                } else {
                    MP4Cover::new_jpeg(data.to_vec())
                };
                tags.covers.push(cover);
                return Ok(());
            }
            Err(JsValue::from_str(
                "setMp4CoverArt: file is not an MP4/M4A format",
            ))
        })
    }

    /// Embed cover art as a METADATA_BLOCK_PICTURE in Vorbis comments.
    ///
    /// Creates a FLAC Picture block, base64-encodes it, and stores it
    /// under the "metadata_block_picture" key. This is the standard way
    /// to embed cover art in Ogg formats (Vorbis, Opus, Speex, Theora)
    /// and OggFlac.
    ///
    /// Supported formats: OggVorbis, OggOpus, OggFlac, OggSpeex, OggTheora.
    #[wasm_bindgen(js_name = "setVorbisCoverArt")]
    pub fn set_vorbis_cover_art(
        &mut self,
        data: &[u8],
        mime_type: &str,
        picture_type: u32,
        width: u32,
        height: u32,
        color_depth: u32,
    ) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            check_cover_art_size(data, "setVorbisCoverArt")?;
            use audex::flac::Picture;

            let mut picture = Picture::new();
            picture.picture_type = picture_type;
            picture.mime_type = mime_type.to_string();
            picture.description = "Cover Art".to_string();
            picture.width = width;
            picture.height = height;
            picture.color_depth = color_depth;
            picture.data = data.to_vec();

            // Helper macro: require tags or return a descriptive error
            macro_rules! require_tags_or_err {
                ($tags:expr, $fmt:expr) => {
                    $tags.ok_or_else(|| {
                        JsValue::from_str(&format!(
                            "setVorbisCoverArt: {} file has no Vorbis Comment container — \
                             call addTags() first or load a file that already has tags",
                            $fmt
                        ))
                    })?
                };
            }

            if let Some(ogg) = this.inner.downcast_mut::<audex::oggvorbis::OggVorbis>() {
                let tags = require_tags_or_err!(ogg.tags.as_mut(), "OggVorbis");
                let snapshot = tags.get_pictures();
                tags.clear_pictures();
                if let Err(err) = tags.add_picture(picture.clone()) {
                    tags.clear_pictures();
                    for previous in snapshot {
                        let _ = tags.add_picture(previous);
                    }
                    return Err(to_js_error(err));
                }
                return Ok(());
            }
            if let Some(opus) = this.inner.downcast_mut::<audex::oggopus::OggOpus>() {
                let tags = require_tags_or_err!(opus.tags.as_mut(), "OggOpus");
                let snapshot = tags.get_pictures();
                tags.clear_pictures();
                if let Err(err) = tags.add_picture(picture.clone()) {
                    tags.clear_pictures();
                    for previous in snapshot {
                        let _ = tags.add_picture(previous);
                    }
                    return Err(to_js_error(err));
                }
                return Ok(());
            }
            if let Some(oflac) = this.inner.downcast_mut::<audex::oggflac::OggFlac>() {
                let tags = require_tags_or_err!(oflac.tags.as_mut(), "OggFlac");
                let snapshot = tags.get_pictures();
                tags.clear_pictures();
                if let Err(err) = tags.add_picture(picture.clone()) {
                    tags.clear_pictures();
                    for previous in snapshot {
                        let _ = tags.add_picture(previous);
                    }
                    return Err(to_js_error(err));
                }
                return Ok(());
            }
            if let Some(speex) = this.inner.downcast_mut::<audex::oggspeex::OggSpeex>() {
                let tags = require_tags_or_err!(speex.tags.as_mut(), "OggSpeex");
                let snapshot = tags.get_pictures();
                tags.clear_pictures();
                if let Err(err) = tags.add_picture(picture.clone()) {
                    tags.clear_pictures();
                    for previous in snapshot {
                        let _ = tags.add_picture(previous);
                    }
                    return Err(to_js_error(err));
                }
                return Ok(());
            }
            if let Some(theora) = this.inner.downcast_mut::<audex::oggtheora::OggTheora>() {
                let tags = require_tags_or_err!(theora.tags.as_mut(), "OggTheora");
                let snapshot = tags.get_pictures();
                tags.clear_pictures();
                if let Err(err) = tags.add_picture(picture.clone()) {
                    tags.clear_pictures();
                    for previous in snapshot {
                        let _ = tags.add_picture(previous);
                    }
                    return Err(to_js_error(err));
                }
                return Ok(());
            }
            Err(JsValue::from_str(
                "setVorbisCoverArt: format does not support Vorbis Comment picture blocks",
            ))
        })
    }

    /// Embed cover art as a FLAC Picture metadata block (FLAC only).
    ///
    /// Adds a Picture metadata block directly to the FLAC file.
    /// This is the standard way to embed cover art in native FLAC files
    /// (as opposed to OggFlac which uses METADATA_BLOCK_PICTURE in Vorbis comments).
    #[wasm_bindgen(js_name = "setFlacCoverArt")]
    pub fn set_flac_cover_art(
        &mut self,
        data: &[u8],
        mime_type: &str,
        picture_type: u32,
        width: u32,
        height: u32,
        color_depth: u32,
    ) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            check_cover_art_size(data, "setFlacCoverArt")?;
            use audex::flac::Picture;

            if let Some(flac) = this.inner.downcast_mut::<audex::flac::FLAC>() {
                // Use snapshot/restore to match the rollback pattern used by
                // setVorbisCoverArt, setApeCoverArt, and setAsfCoverArt.
                // If picture construction or insertion ever becomes fallible,
                // the previous pictures are automatically restored.
                mutate_with_rollback(&mut flac.pictures, |pictures| -> Result<(), JsValue> {
                    pictures.clear();
                    let mut picture = Picture::new();
                    picture.picture_type = picture_type;
                    picture.mime_type = mime_type.to_string();
                    picture.width = width;
                    picture.height = height;
                    picture.color_depth = color_depth;
                    picture.data = data.to_vec();
                    pictures.push(picture);
                    Ok(())
                })?;
                return Ok(());
            }
            Err(JsValue::from_str(
                "setFlacCoverArt: file is not a FLAC format",
            ))
        })
    }

    /// Get MP4 cover art as raw bytes, or `null` if not present.
    #[wasm_bindgen(js_name = "getMp4CoverArt")]
    pub fn get_mp4_cover_art(&self) -> Option<Vec<u8>> {
        if self.poison_read_fallback() {
            return None;
        }
        if let Some(mp4) = self.inner.downcast_ref::<audex::mp4::MP4>() {
            if let Some(ref tags) = mp4.tags {
                if let Some(cover) = tags.covers.first() {
                    return Some(cover.data.clone());
                }
            }
        }
        None
    }

    /// Set ReplayGain as a native RVA2 frame (ID3-based formats only).

    #[wasm_bindgen(js_name = "setReplayGain")]
    pub fn set_replay_gain(
        &mut self,
        identification: &str,
        gain_db: f32,
        peak: f32,
    ) -> Result<(), JsValue> {
        self.run_mutation_with_poison(|this| {
            use audex::id3::RVA2;
            use audex::id3::frames::ChannelType;

            // Reject non-finite values before they reach frame creation
            if !gain_db.is_finite() {
                return Err(JsValue::from_str(&format!(
                    "gain_db must be finite, got: {}",
                    gain_db
                )));
            }
            if !peak.is_finite() {
                return Err(JsValue::from_str(&format!(
                    "peak must be finite, got: {}",
                    peak
                )));
            }

            let rva2_key = format!("RVA2:{}", identification);

            // Helper closure to insert RVA2 into ID3Tags
            fn insert_rva2(
                tags: &mut audex::id3::ID3Tags,
                key: &str,
                identification: &str,
                gain_db: f32,
                peak: f32,
            ) -> audex::Result<()> {
                replace_rva2_frames_atomically(tags, key, |tags| {
                    let mut frame = RVA2::new(identification.to_string(), Vec::new());
                    frame.add_channel(ChannelType::MasterVolume, gain_db, peak)?;
                    tags.add(Box::new(frame))
                })
            }

            // Helper: extract tags from a format, returning an error if absent
            fn require_tags<'a>(
                tags: Option<&'a mut audex::id3::ID3Tags>,
                format_name: &str,
            ) -> Result<&'a mut audex::id3::ID3Tags, JsValue> {
                tags.ok_or_else(|| {
                    JsValue::from_str(&format!(
                        "setReplayGain: {} file has no ID3 tag container — \
                     call addTags() first or load a file that already has tags",
                        format_name
                    ))
                })
            }

            if let Some(mp3) = this.inner.downcast_mut::<audex::mp3::MP3>() {
                let tags = require_tags(mp3.tags.as_mut(), "MP3")?;
                insert_rva2(tags, &rva2_key, identification, gain_db, peak).map_err(to_js_error)?;
                return Ok(());
            }
            if let Some(aiff) = this.inner.downcast_mut::<audex::aiff::AIFF>() {
                let tags = require_tags(aiff.tags.as_mut(), "AIFF")?;
                insert_rva2(tags, &rva2_key, identification, gain_db, peak).map_err(to_js_error)?;
                return Ok(());
            }
            if let Some(wave) = this.inner.downcast_mut::<audex::wave::WAVE>() {
                let tags = require_tags(wave.tags.as_mut(), "WAVE")?;
                insert_rva2(tags, &rva2_key, identification, gain_db, peak).map_err(to_js_error)?;
                return Ok(());
            }
            if let Some(dsf) = this.inner.downcast_mut::<audex::dsf::DSF>() {
                let tags = require_tags(dsf.tags.as_mut(), "DSF")?;
                insert_rva2(tags, &rva2_key, identification, gain_db, peak).map_err(to_js_error)?;
                return Ok(());
            }
            if let Some(dsdiff) = this.inner.downcast_mut::<audex::dsdiff::DSDIFF>() {
                let tags = require_tags(dsdiff.tags.as_mut(), "DSDIFF")?;
                insert_rva2(tags, &rva2_key, identification, gain_db, peak).map_err(to_js_error)?;
                return Ok(());
            }

            Err(JsValue::from_str(
                "setReplayGain: format does not support RVA2 replay gain frames",
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// Save and reload
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl AudioFile {
    /// Save the modified metadata and return the complete file as bytes.
    ///
    /// The returned `Uint8Array` contains the full audio file with
    /// updated tags, ready to be written to disk or transmitted.
    pub fn save(&mut self) -> Result<Vec<u8>, JsValue> {
        self.run_byte_mutation_with_poison(|this, cursor| {
            this.inner.save_to_writer(cursor).map_err(to_js_error)?;
            // Clone is architecturally required: JS must own the returned
            // buffer while we retain original_bytes for future saves.
            // The actual buffer is written back by run_byte_mutation_with_poison.
            Ok(cursor.get_ref().clone())
        })
    }

    /// Save with format-specific options (ID3 formats only).
    ///
    /// For MP3/AIFF/WAVE/DSF/DSDIFF this controls:
    /// - `v1`: ID3v1 handling (0=REMOVE, 1=UPDATE, 2=CREATE)
    /// - `v2_version`: ID3v2 version (3 or 4)
    /// - `v23_sep`: Multi-value separator for v2.3 (e.g. "/")
    /// - `convert_v24_frames`: Whether to convert v2.4 frames to v2.3 equivalents
    ///
    /// ID3 save configuration is currently only applied for MP3 files, where the
    /// full set of options (v1 handling, v2 version, separator, frame conversion)
    /// is honored. For other ID3-based formats (AIFF, WAVE, DSF, DSDIFF), custom
    /// options are rejected and the method falls through to the standard save path
    /// with fixed defaults (v2.3, "/" separator). Non-ID3 formats also use the
    /// standard save path.
    #[wasm_bindgen(js_name = "saveWithOptions")]
    pub fn save_with_options(
        &mut self,
        v1: u8,
        v2_version: u8,
        v23_sep: &str,
        convert_v24_frames: bool,
    ) -> Result<Vec<u8>, JsValue> {
        self.check_poisoned()?;
        use audex::id3::tags::ID3SaveConfig;

        // Reject invalid ID3v2 versions early with a clear error message,
        // rather than letting them propagate deep into the tag writer.
        if v2_version != 3 && v2_version != 4 {
            return Err(JsValue::from_str(&format!(
                "v2_version must be 3 or 4, got {}",
                v2_version
            )));
        }

        // Validate that the separator is a single ASCII byte — multi-byte
        // UTF-8 characters would be silently truncated to an invalid fragment
        if v23_sep.len() != 1 || !v23_sep.is_ascii() {
            return Err(JsValue::from_str(&format!(
                "v23_sep must be a single ASCII character, got {:?} ({} bytes)",
                v23_sep,
                v23_sep.len()
            )));
        }

        let config = ID3SaveConfig {
            v2_version,
            v2_minor: 0,
            v23_sep: v23_sep.to_string(),
            v23_separator: v23_sep.as_bytes()[0],
            padding: None,
            merge_frames: true,
            preserve_unknown: true,
            compress_frames: false,
            write_v1: match v1 {
                0 => audex::id3::file::ID3v1SaveOptions::REMOVE,
                1 => audex::id3::file::ID3v1SaveOptions::UPDATE,
                2 => audex::id3::file::ID3v1SaveOptions::CREATE,
                _ => {
                    return Err(JsValue::from_str(&format!(
                        "v1 must be 0 (REMOVE), 1 (UPDATE), or 2 (CREATE), got {}",
                        v1
                    )));
                }
            },
            unsync: false,
            extended_header: false,
            convert_v24_frames,
        };

        // MP3: ID3 tags are at the start of the file, use save_to_writer
        // directly for full control over v1/v2 version/encoding options.
        if let Some(mp3) = self.inner.downcast_ref::<audex::mp3::MP3>() {
            if let Some(ref tags) = mp3.tags {
                let backup = self.original_bytes.clone();
                let hint = self.inner.filename().map(|p| p.to_path_buf());
                let mut completed = false;
                // SAFETY: The closure moves `original_bytes` into a cursor and
                // only writes it back after save_to_writer succeeds. On error
                // the buffer is restored from the cursor; on panic the backup
                // clone is restored and the instance is poisoned.
                let caught = catch_panic_with_status(|| {
                    // Move the buffer into the cursor instead of cloning to avoid
                    // holding two full copies simultaneously. Restored on failure.
                    let buf = std::mem::take(&mut self.original_bytes);
                    let mut cursor = Cursor::new(buf);
                    if let Err(e) = tags.save_to_writer(&mut cursor, &config) {
                        // Restore the buffer before propagating the error.
                        self.original_bytes = cursor.into_inner();
                        return Err(to_js_error(e));
                    }
                    let candidate = cursor.into_inner();
                    if let Err(e) =
                        File::load_from_reader(Cursor::new(&candidate[..]), hint.clone())
                    {
                        // Validation failed; restore the pre-save bytes so the
                        // instance remains usable for subsequent save attempts.
                        self.original_bytes = backup.clone();
                        return Err(to_js_error(e));
                    }
                    self.original_bytes = candidate;
                    completed = true;
                    Ok(self.original_bytes.clone())
                });
                if caught.panicked && !completed {
                    let _ = self.restore_backup_after_panic(backup);
                    self.poisoned = true;
                }
                return caught.result;
            }
        }

        // Non-MP3 formats (AIFF, WAVE, DSF, DSDIFF) embed ID3 inside their
        // container. Their save path uses fixed defaults (v2.3, "/" separator).
        // Warn the caller if they passed non-default options that would be ignored.
        let has_custom_options = v2_version != 3 || v23_sep != "/" || v1 != 0 || convert_v24_frames;

        if has_custom_options {
            return Err(JsValue::from_str(
                "saveWithOptions: custom v2_version, separator, v1, and \
                 convert_v24_frames options are only supported for MP3 files. \
                 For other formats, use save() or pass default values \
                 (v2_version=3, v23_sep=\"/\", v1=0, convert_v24_frames=false).",
            ));
        }

        self.save()
    }

    /// Re-parse metadata from the last saved bytes.
    ///
    /// Useful after multiple rounds of modifications to ensure the
    /// in-memory representation is consistent with the serialized form.
    pub fn reload(&mut self) -> Result<(), JsValue> {
        self.check_poisoned()?;
        // SAFETY: The closure only assigns to `self.inner` after the parse
        // succeeds (load_from_reader returns Ok). If parsing panics,
        // `original_bytes` is untouched and `self.inner` retains its previous
        // value, so no partial mutation occurs. The poison check above ensures
        // we are not operating on an already-inconsistent instance.
        catch_panic(|| {
            // Borrow the buffer instead of cloning — reload only needs read access,
            // so a Cursor over a slice avoids allocating a second copy entirely.
            let cursor = Cursor::new(&self.original_bytes[..]);

            // Preserve the filename hint for consistent format detection
            let hint = self.inner.filename().map(|p| p.to_path_buf());

            let reloaded = File::load_from_reader(cursor, hint).map_err(to_js_error)?;
            self.inner = reloaded;
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// WasmTagDiff — wraps audex::diff::TagDiff with JS-accessible methods
// ---------------------------------------------------------------------------

/// A tag diff result with methods for summarising, filtering, and formatting.
///
/// Returned by `AudioFile.diffTags()`, `AudioFile.diffTagsNormalized()`,
/// `AudioFile.diffTagsWithOptions()`, and `AudioFile.diffAgainstSnapshot()`.
/// Call `.toJson()` to get the raw diff data as a plain JS object.
#[wasm_bindgen]
pub struct WasmTagDiff {
    inner: TagDiff,
}

impl WasmTagDiff {
    fn empty() -> Self {
        Self {
            inner: TagDiff {
                changed: Vec::new(),
                left_only: Vec::new(),
                right_only: Vec::new(),
                unchanged: Vec::new(),
                stream_info_diff: None,
            },
        }
    }
}

#[wasm_bindgen]
impl WasmTagDiff {
    /// Whether the two sides are identical (no changed, left-only, or right-only fields).
    #[wasm_bindgen(js_name = "isIdentical")]
    pub fn is_identical(&self) -> bool {
        self.inner.is_identical()
    }

    /// Number of differences (changed + left-only + right-only).
    #[wasm_bindgen(js_name = "diffCount")]
    pub fn diff_count(&self) -> usize {
        self.inner.diff_count()
    }

    /// One-line summary: "N changed, N removed, N added, N unchanged".
    pub fn summary(&self) -> String {
        self.inner.summary()
    }

    /// Pretty-print the diff with right-aligned keys (excludes unchanged).
    pub fn pprint(&self) -> String {
        self.inner.pprint()
    }

    /// Pretty-print including unchanged fields (prefixed with `=`).
    #[wasm_bindgen(js_name = "pprintFull")]
    pub fn pprint_full(&self) -> String {
        self.inner.pprint_full()
    }

    /// Filter the diff to only include the given keys.
    ///
    /// Returns a new `WasmTagDiff` containing only fields whose key matches
    /// one of the provided strings.
    #[wasm_bindgen(js_name = "filterKeys")]
    pub fn filter_keys(&self, keys: Vec<String>) -> WasmTagDiff {
        let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
        WasmTagDiff {
            inner: self.inner.filter_keys(&key_refs),
        }
    }

    /// Collect every key that differs in any way (changed, left-only, or right-only).
    #[wasm_bindgen(js_name = "differingKeys")]
    pub fn differing_keys(&self) -> Vec<String> {
        self.inner
            .differing_keys()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Get the full diff data as a JSON object.
    ///
    /// Returns the same structure as `diffTags()` / `diffTagsNormalized()`:
    /// `{ changed, left_only, right_only, unchanged, stream_info_diff }`.
    #[wasm_bindgen(js_name = "toJson")]
    pub fn to_json(&self) -> Result<JsValue, JsValue> {
        to_js_object(&self.inner)
    }
}

// ---------------------------------------------------------------------------
// Advanced: diffing, tag conversion, snapshots
// ---------------------------------------------------------------------------

#[wasm_bindgen]
impl AudioFile {
    /// Compare tags between this file and another AudioFile.
    ///
    /// Returns a `WasmTagDiff` with methods like `isIdentical()`, `summary()`,
    /// `pprint()`, `filterKeys()`, etc.  Call `.toJson()` for the raw data.
    #[wasm_bindgen(js_name = "diffTags")]
    pub fn diff_tags(&self, other: &AudioFile) -> WasmTagDiff {
        if self.poison_read_fallback() || other.poison_read_fallback() {
            return WasmTagDiff::empty();
        }
        let result = audex::diff::diff(&self.inner, &other.inner);
        WasmTagDiff { inner: result }
    }

    /// Compare tags with full control over diff options.
    ///
    /// Options:
    /// - `compare_stream_info` — include stream info differences
    /// - `case_insensitive_keys` — normalise keys to lowercase before comparing
    /// - `trim_values` — trim whitespace from values before comparing
    /// - `include_unchanged` — populate the unchanged fields list
    /// - `include_keys` — if provided, only compare these keys (array of strings)
    /// - `exclude_keys` — exclude these keys from comparison (array of strings)
    #[wasm_bindgen(js_name = "diffTagsWithOptions")]
    #[allow(clippy::too_many_arguments)]
    pub fn diff_tags_with_options(
        &self,
        other: &AudioFile,
        compare_stream_info: Option<bool>,
        case_insensitive_keys: Option<bool>,
        trim_values: Option<bool>,
        include_unchanged: Option<bool>,
        include_keys: Option<Vec<String>>,
        exclude_keys: Option<Vec<String>>,
    ) -> WasmTagDiff {
        if self.poison_read_fallback() || other.poison_read_fallback() {
            return WasmTagDiff::empty();
        }
        let options = audex::diff::DiffOptions {
            compare_stream_info: compare_stream_info.unwrap_or(false),
            case_insensitive_keys: case_insensitive_keys.unwrap_or(false),
            trim_values: trim_values.unwrap_or(false),
            include_unchanged: include_unchanged.unwrap_or(false),
            include_keys: include_keys.map(|v| v.into_iter().collect::<HashSet<String>>()),
            exclude_keys: exclude_keys
                .map(|v| v.into_iter().collect::<HashSet<String>>())
                .unwrap_or_default(),
            normalize_custom_keys: false,
        };
        let result = audex::diff::diff_with_options(&self.inner, &other.inner, &options);
        WasmTagDiff { inner: result }
    }

    /// Compare tags using normalized (standard) field names.
    ///
    /// Keys are mapped through `TagMap` so that format-specific names
    /// (e.g. `TIT2`, `©nam`) become their standard equivalents (`Title`).
    ///
    /// Optional flags:
    /// - `include_unchanged` (bool, default `true`) — include unchanged fields
    /// - `compare_stream_info` (bool, default `false`) — include stream info diff
    #[wasm_bindgen(js_name = "diffTagsNormalized")]
    pub fn diff_tags_normalized(
        &self,
        other: &AudioFile,
        include_unchanged: Option<bool>,
        compare_stream_info: Option<bool>,
        normalize_custom_keys: Option<bool>,
    ) -> WasmTagDiff {
        if self.poison_read_fallback() || other.poison_read_fallback() {
            return WasmTagDiff::empty();
        }
        let options = audex::diff::DiffOptions {
            include_unchanged: include_unchanged.unwrap_or(true),
            compare_stream_info: compare_stream_info.unwrap_or(false),
            normalize_custom_keys: normalize_custom_keys.unwrap_or(true),
            ..Default::default()
        };
        let result = audex::diff::diff_normalized_with_options(&self.inner, &other.inner, &options);
        WasmTagDiff { inner: result }
    }

    /// Capture a snapshot of this file's tags for later comparison.
    ///
    /// Returns a JSON array of `[key, [values]]` pairs.  Pass the result
    /// to `diffAgainstSnapshot()` on another file to compare.
    #[wasm_bindgen(js_name = "snapshotTags")]
    pub fn snapshot_tags(&self) -> Result<JsValue, JsValue> {
        self.check_poisoned()?;
        let snap = audex::diff::snapshot_tags(&self.inner);
        to_js_object(&snap)
    }

    /// Compare this file's current tags against a previously captured snapshot.
    ///
    /// The snapshot (from `snapshotTags()`) is treated as the "before" side.
    /// Returns a `WasmTagDiff`.
    #[wasm_bindgen(js_name = "diffAgainstSnapshot")]
    pub fn diff_against_snapshot(&self, snapshot: JsValue) -> Result<WasmTagDiff, JsValue> {
        self.check_poisoned()?;
        validate_js_snapshot_payload(&snapshot)?;
        let snap: Vec<(String, Vec<String>)> = serde_wasm_bindgen::from_value(snapshot)
            .map_err(|e| JsValue::from_str(&format!("snapshot parse error: {e}")))?;

        // Reject snapshot payloads that exceed safe size limits
        validate_tag_payload(&snap).map_err(|msg| JsValue::from_str(&msg))?;

        let result = audex::diff::diff_against_snapshot(&self.inner, &snap);
        Ok(WasmTagDiff { inner: result })
    }

    /// Import tags from another AudioFile, mapping field names between formats.
    ///
    /// Returns a JSON report listing transferred and skipped fields.
    /// Wrapped in `run_mutation_with_poison` so a panic during tag
    /// conversion correctly poisons the instance rather than leaving
    /// it in a silently inconsistent state.
    #[wasm_bindgen(js_name = "importTagsFrom")]
    pub fn import_tags_from(&mut self, source: &AudioFile) -> Result<JsValue, JsValue> {
        source.check_poisoned()?;
        self.run_mutation_with_poison(|this| {
            let snapshot_before = this.inner.items();
            let report = match audex::convert_tags(&source.inner, &mut this.inner) {
                Ok(report) => report,
                Err(err) => {
                    this.restore_tag_snapshot(snapshot_before.clone());
                    return Err(to_js_error(err));
                }
            };
            if let Err(e) = check_tag_budget(&this.inner) {
                this.restore_tag_snapshot(snapshot_before);
                return Err(e);
            }
            to_js_object(&report)
        })
    }

    /// Import tags with conversion options.
    ///
    /// Options:
    /// - `include_fields` — array of standard field names to include (e.g.
    ///   `["Title", "Artist"]`).  If omitted, all fields are transferred.
    /// - `exclude_fields` — array of standard field names to exclude.
    /// - `transfer_custom` (bool, default `true`) — transfer non-standard fields.
    /// - `overwrite` (bool, default `true`) — overwrite existing destination values.
    /// - `clear_destination` (bool, default `false`) — clear all destination tags first.
    /// Wrapped in `run_mutation_with_poison` so a panic during tag
    /// conversion correctly poisons the instance. Option parsing is
    /// performed before entering the guarded closure since it does
    /// not mutate self.
    #[wasm_bindgen(js_name = "importTagsFromWithOptions")]
    pub fn import_tags_from_with_options(
        &mut self,
        source: &AudioFile,
        include_fields: Option<Vec<String>>,
        exclude_fields: Option<Vec<String>>,
        transfer_custom: Option<bool>,
        overwrite: Option<bool>,
        clear_destination: Option<bool>,
    ) -> Result<JsValue, JsValue> {
        source.check_poisoned()?;
        use audex::tagmap::{ConversionOptions, StandardField};

        let parse_fields = |names: Vec<String>| -> Result<HashSet<StandardField>, JsValue> {
            names
                .iter()
                .map(|s| {
                    s.parse::<StandardField>()
                        .map_err(|e| JsValue::from_str(&format!("unknown field '{}': {}", s, e)))
                })
                .collect()
        };

        // Parse options before entering the panic guard — this is pure
        // validation that does not mutate the destination file.
        let options = ConversionOptions {
            include_fields: include_fields.map(parse_fields).transpose()?,
            exclude_fields: exclude_fields
                .map(parse_fields)
                .transpose()?
                .unwrap_or_default(),
            transfer_custom: transfer_custom.unwrap_or(true),
            overwrite: overwrite.unwrap_or(true),
            clear_destination: clear_destination.unwrap_or(false),
        };

        self.run_mutation_with_poison(|this| {
            let snapshot_before = this.inner.items();
            let report =
                match audex::convert_tags_with_options(&source.inner, &mut this.inner, &options) {
                    Ok(report) => report,
                    Err(err) => {
                        this.restore_tag_snapshot(snapshot_before.clone());
                        return Err(to_js_error(err));
                    }
                };
            if let Err(e) = check_tag_budget(&this.inner) {
                this.restore_tag_snapshot(snapshot_before);
                return Err(e);
            }
            to_js_object(&report)
        })
    }

    /// Get a complete metadata snapshot as JSON.
    ///
    /// Includes format, filename, stream info, and all tags in a single
    /// serializable structure.
    pub fn snapshot(&self) -> Result<JsValue, JsValue> {
        self.check_poisoned()?;
        let snap = self.inner.to_snapshot();
        to_js_object(&snap)
    }
}
