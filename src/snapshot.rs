//! Unified serializable view of an audio file's metadata.
//!
//! [`TagSnapshot`] provides a single, format-agnostic struct that captures
//! everything a consumer typically needs — format name, stream properties,
//! and a flat tag dictionary — in a shape that serializes cleanly to JSON,
//! TOML, or any other serde-supported format.
//!
//! # When to use snapshots
//!
//! Use `to_snapshot()` when you need to send metadata over the wire (REST
//! APIs, message queues) or persist it to a database/config file. The
//! snapshot intentionally flattens format-specific details into a common
//! schema so that consumers do not need to understand every audio format.
//!
//! For lossless round-tripping that preserves format-specific fields (e.g.
//! MP4 freeform atoms), use `to_snapshot_with_raw()` which attempts to populate
//! the `raw_tags` field with a JSON value of the underlying tag container when
//! that format supports raw-tag serialization.

use std::collections::HashMap;

/// Primary serialization target for an audio file's metadata.
///
/// Captures format name, stream properties, and all tags as key-value
/// pairs.  Designed for clean JSON/TOML output.
///
/// # Serialization
///
/// When the `serde` feature is enabled, this type implements
/// `Serialize` and `Deserialize`, allowing conversion to/from
/// JSON, TOML, and other serde-supported formats.
///
/// # Deserialization safety
///
/// The `Deserialize` impl enforces resource limits on the decoded structure
/// (maximum tag count, cumulative string size, and `raw_tags` structural
/// bounds) regardless of how deserialization is invoked.
///
/// For untrusted input, callers should still apply an outer transport/body-size
/// limit. [`TagSnapshot::from_json_str`] adds an upfront byte-length check that
/// rejects oversized payloads before parsing begins.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TagSnapshot {
    /// Format identifier (e.g. "FLAC", "MP3", "MP4")
    pub format: String,

    /// Original file path, if the file was loaded from disk
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub filename: Option<String>,

    /// Stream properties (duration, bitrate, sample rate, etc.)
    pub stream_info: StreamInfoSnapshot,

    /// All tags as key → values, unified across formats
    pub tags: HashMap<String, Vec<String>>,

    /// Format-specific raw tag data for lossless round-trips.
    ///
    /// Only populated by `to_snapshot_with_raw()`. Consumers that do
    /// not need format-specific fidelity can ignore this field.
    ///
    /// When deserializing from untrusted sources, this field is validated
    /// against a depth and size limit to prevent resource exhaustion from
    /// deeply nested or excessively large JSON trees.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub raw_tags: Option<serde_json::Value>,
}

/// Custom deserializer for `raw_tags` that enforces depth, node count, and
/// cumulative string size limits on the JSON tree to prevent resource
/// exhaustion from untrusted input.
///
/// Validation runs immediately after deserialization, checking all three
/// budgets (depth, node count, string bytes) in a single pass. The string
/// byte budget catches oversized payloads (e.g. a single 100MB string) that
/// would slip past the node count limit.
#[cfg(feature = "serde")]
mod bounded_json_value {
    use serde::{Deserialize, Deserializer};

    /// Maximum nesting depth allowed in raw_tags JSON trees.
    const MAX_DEPTH: usize = 64;

    /// Maximum total number of JSON nodes (objects, arrays, strings, numbers, etc.)
    const MAX_NODES: usize = 100_000;

    /// Maximum cumulative byte size of all string content (keys + values)
    /// within the raw_tags tree. Prevents a single huge string or many large
    /// strings from exhausting memory before node/depth limits trigger.
    const MAX_STRING_BYTES: usize = 10 * 1024 * 1024; // 10 MB

    /// Tracks resource consumption while walking the deserialized JSON tree.
    struct Budget {
        node_count: usize,
        string_bytes: usize,
    }

    /// Recursively validate the JSON value tree against all resource budgets.
    /// Checks depth, node count, and cumulative string byte size in one pass.
    fn validate(
        value: &serde_json::Value,
        depth: usize,
        budget: &mut Budget,
    ) -> Result<(), String> {
        budget.node_count += 1;
        if budget.node_count > MAX_NODES {
            return Err(format!("raw_tags exceeds {} node limit", MAX_NODES));
        }
        if depth >= MAX_DEPTH {
            return Err(format!(
                "raw_tags exceeds {} nesting depth limit",
                MAX_DEPTH
            ));
        }

        match value {
            serde_json::Value::String(s) => {
                budget.string_bytes = budget.string_bytes.saturating_add(s.len());
                if budget.string_bytes > MAX_STRING_BYTES {
                    return Err(format!(
                        "raw_tags exceeds {} byte size limit for string content",
                        MAX_STRING_BYTES
                    ));
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    validate(item, depth + 1, budget)?;
                }
            }
            serde_json::Value::Object(map) => {
                for (k, v) in map {
                    // Object keys count toward the string byte budget
                    budget.string_bytes = budget.string_bytes.saturating_add(k.len());
                    if budget.string_bytes > MAX_STRING_BYTES {
                        return Err(format!(
                            "raw_tags exceeds {} byte size limit for string content",
                            MAX_STRING_BYTES
                        ));
                    }
                    validate(v, depth + 1, budget)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Maximum raw JSON input size allowed for raw_tags before deserialization.
    /// This prevents allocating a full serde_json::Value tree (which uses 2-5x
    /// the raw JSON size in heap memory) for oversized payloads. Checked BEFORE
    /// the expensive parse step so that crafted large inputs are rejected cheaply.
    const MAX_RAW_INPUT_BYTES: usize = 16 * 1024 * 1024; // 16 MB

    /// Deserialize an optional JSON value with size and structural validation.
    ///
    /// # Caller Responsibility
    ///
    /// This function deserializes the full raw JSON into a `serde_json::Value`
    /// tree before applying structural validation (depth, node count, string
    /// byte limits). The initial JSON tokenization (via `Option<Box<RawValue>>`)
    /// is performed by the outer deserializer before this function can inspect
    /// the payload size, so the raw JSON bytes are already buffered by the
    /// time the `MAX_RAW_INPUT_BYTES` check runs.
    ///
    /// **Callers that accept untrusted input must enforce a transport-level
    /// size limit (e.g. a max request body or [`TagSnapshot::from_json_str`])
    /// before invoking serde directly.** Without that outer guard, an
    /// attacker-controlled payload can force unbounded allocation during the
    /// tokenization phase, before any of the checks in this function execute.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<serde_json::Value>, D::Error>
    where
        D: Deserializer<'de>,
    {
        // First capture the raw JSON text without building a Value tree.
        // This lets us check the input size cheaply before committing to
        // the full deserialization, which would allocate 2-5x more memory
        // than the raw JSON payload itself.
        let opt_raw: Option<Box<serde_json::value::RawValue>> = Option::deserialize(deserializer)?;

        let raw = match opt_raw {
            Some(r) => r,
            None => return Ok(None),
        };

        // Reject oversized payloads before parsing into a Value tree
        let raw_str = raw.get();
        if raw_str.len() > MAX_RAW_INPUT_BYTES {
            return Err(serde::de::Error::custom(format!(
                "raw_tags JSON input ({} bytes) exceeds {} byte size limit",
                raw_str.len(),
                MAX_RAW_INPUT_BYTES
            )));
        }

        // Now parse the size-checked JSON into a Value tree
        let value: serde_json::Value =
            serde_json::from_str(raw_str).map_err(serde::de::Error::custom)?;

        // Run the structural validation (depth, node count, string bytes)
        let mut budget = Budget {
            node_count: 0,
            string_bytes: 0,
        };
        validate(&value, 0, &mut budget).map_err(serde::de::Error::custom)?;

        Ok(Some(value))
    }
}

/// Custom `Deserialize` implementation for `TagSnapshot` that enforces
/// resource limits on all fields, not just `raw_tags`.  This runs regardless
/// of whether the caller goes through `from_json_str` or calls
/// `serde_json::from_str::<TagSnapshot>` directly.
///
/// Limits enforced:
///   - `tags` map: at most 10,000 entries
///   - Cumulative string bytes across `format`, `filename`, and all tag
///     keys/values: at most 10 MB
///   - `raw_tags`: delegated to `bounded_json_value::deserialize` (depth,
///     node count, and string byte limits)
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for TagSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// Maximum number of entries allowed in the `tags` HashMap.
        const MAX_TAG_ENTRIES: usize = 10_000;

        /// Maximum cumulative bytes across all user-controlled string fields
        /// (format, filename, tag keys, tag values).  Prevents a payload
        /// with, e.g., 5,000 tags each holding a 1 MB value from exhausting
        /// memory even though the entry count is within limits.
        const MAX_TAG_STRING_BYTES: usize = 10 * 1024 * 1024; // 10 MB

        // Private shadow struct that gets the derived Deserialize impl.
        // This keeps the deserialization logic in sync with TagSnapshot's
        // field layout without manual visitor boilerplate.
        #[derive(serde::Deserialize)]
        struct Inner {
            format: String,
            #[serde(default)]
            filename: Option<String>,
            stream_info: StreamInfoSnapshot,
            tags: HashMap<String, Vec<String>>,
            #[serde(deserialize_with = "bounded_json_value::deserialize", default)]
            raw_tags: Option<serde_json::Value>,
        }

        let inner = Inner::deserialize(deserializer)?;

        // -- Validate tag entry count --
        if inner.tags.len() > MAX_TAG_ENTRIES {
            return Err(serde::de::Error::custom(format!(
                "tags map contains {} entries, exceeding the {} entry limit",
                inner.tags.len(),
                MAX_TAG_ENTRIES,
            )));
        }

        // -- Validate cumulative string byte budget --
        let mut string_bytes: usize = inner.format.len();
        if let Some(ref f) = inner.filename {
            string_bytes = string_bytes.saturating_add(f.len());
        }
        for (key, values) in &inner.tags {
            string_bytes = string_bytes.saturating_add(key.len());
            for v in values {
                string_bytes = string_bytes.saturating_add(v.len());
            }
            if string_bytes > MAX_TAG_STRING_BYTES {
                return Err(serde::de::Error::custom(format!(
                    "cumulative tag string content ({} bytes) exceeds {} byte limit",
                    string_bytes, MAX_TAG_STRING_BYTES,
                )));
            }
        }

        Ok(TagSnapshot {
            format: inner.format,
            filename: inner.filename,
            stream_info: inner.stream_info,
            tags: inner.tags,
            raw_tags: inner.raw_tags,
        })
    }
}

/// Maximum raw JSON input size (in bytes) accepted by [`TagSnapshot::from_json_str`].
/// Inputs exceeding this limit are rejected before any parsing takes place,
/// preventing large allocations from oversized payloads.
#[cfg(feature = "serde")]
const MAX_RAW_INPUT_BYTES: usize = 16 * 1024 * 1024; // 16 MB

#[cfg(feature = "serde")]
impl TagSnapshot {
    /// Deserialize a `TagSnapshot` from a JSON string, with an upfront size
    /// check that rejects inputs larger than 16 MB before any parsing occurs.
    ///
    /// # When to use this vs. `serde_json::from_str`
    ///
    /// Both paths are safe: the custom `Deserialize` impl enforces structural
    /// limits (tag count, cumulative string bytes, `raw_tags` bounds) on every
    /// deserialization regardless of entry point.
    ///
    /// This method adds one extra layer of defense: it checks the raw byte
    /// length of the input *before* the JSON tokenizer runs, so oversized
    /// payloads are rejected without any allocation at all.  Prefer this
    /// method when accepting input from untrusted sources (network requests,
    /// user-uploaded files) for the cheapest possible rejection of junk.
    pub fn from_json_str(input: &str) -> Result<Self, serde_json::Error> {
        if input.len() > MAX_RAW_INPUT_BYTES {
            return Err(serde::de::Error::custom(format!(
                "JSON input ({} bytes) exceeds {} byte size limit",
                input.len(),
                MAX_RAW_INPUT_BYTES,
            )));
        }
        serde_json::from_str(input)
    }
}

/// Stream properties extracted from any audio format.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StreamInfoSnapshot {
    /// Duration in fractional seconds (e.g. 243.72)
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub length_secs: Option<f64>,

    /// Bitrate in bits per second
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub bitrate: Option<u32>,

    /// Sample rate in Hz
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub sample_rate: Option<u32>,

    /// Number of audio channels
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub channels: Option<u16>,

    /// Bits per sample (bit depth)
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub bits_per_sample: Option<u16>,
}

impl StreamInfoSnapshot {
    /// Build a snapshot from a `DynamicStreamInfo`.
    pub fn from_dynamic(info: &crate::file::DynamicStreamInfo) -> Self {
        use crate::StreamInfo;
        Self {
            length_secs: info.length().map(|d| d.as_secs_f64()),
            bitrate: info.bitrate(),
            sample_rate: info.sample_rate(),
            channels: info.channels(),
            bits_per_sample: info.bits_per_sample(),
        }
    }
}
