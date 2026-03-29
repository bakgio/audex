//! Custom serde serialization helpers for audio metadata types.
//!
//! Provides format-friendly serialization for types that do not have
//! ideal default serde representations:
//!
//! - [`duration_as_secs_f64`]: Serializes `Option<Duration>` as an `f64`
//!   number of seconds instead of the default `{ secs, nanos }` struct.
//! - [`bytes_as_base64`]: Serializes `Vec<u8>` as a base64-encoded string
//!   instead of a JSON array of integers.

/// Serialize `Option<Duration>` as an `f64` number of seconds.
///
/// Standard `Duration` serialization produces `{ "secs": N, "nanos": N }`,
/// which is awkward for JSON consumers. This helper emits a single `f64`
/// value representing total seconds (e.g. `123.456`), or `null` when the
/// duration is absent.
///
/// # Usage
///
/// ```ignore
/// #[cfg_attr(feature = "serde", serde(with = "crate::serde_helpers::duration_as_secs_f64"))]
/// pub length: Option<Duration>,
/// ```
#[cfg(feature = "serde")]
pub mod duration_as_secs_f64 {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(dur: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match dur {
            Some(d) => serializer.serialize_some(&d.as_secs_f64()),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<f64> = Option::deserialize(deserializer)?;
        match opt {
            Some(secs) if secs.is_finite() && secs >= 0.0 => {
                // Use try_from_secs_f64 to gracefully handle values that
                // exceed Duration's maximum (~1.844e19 seconds).
                Ok(Duration::try_from_secs_f64(secs).ok())
            }
            Some(_) => {
                // NaN, negative, or infinite values would panic in
                // Duration::from_secs_f64 — return None instead
                Ok(None)
            }
            None => Ok(None),
        }
    }
}

/// Serialize `Vec<u8>` as a base64-encoded string.
///
/// Binary fields (album art, raw tag data) are unreadable as JSON integer
/// arrays.  This helper encodes them as standard base64 strings that are
/// compact and copy-pasteable.
///
/// # Usage
///
/// ```ignore
/// #[cfg_attr(feature = "serde", serde(with = "crate::serde_helpers::bytes_as_base64"))]
/// pub data: Vec<u8>,
/// ```
#[cfg(feature = "serde")]
pub mod bytes_as_base64 {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use serde::{self, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = STANDARD.encode(bytes);
        serializer.serialize_str(&encoded)
    }

    /// Maximum base64 string length allowed during deserialization.
    /// 128 MB of base64 decodes to ~96 MB of binary data, which is
    /// well beyond any reasonable embedded metadata payload. Inputs
    /// exceeding this are rejected to prevent memory exhaustion.
    const MAX_BASE64_LEN: usize = 128 * 1024 * 1024;

    /// Visitor that checks string length before taking ownership.
    /// For `from_str` inputs, serde_json provides a borrowed `&str` via
    /// `visit_borrowed_str`, so the length check happens without any
    /// additional allocation. For `from_reader` inputs, the JSON parser
    /// buffers the string internally, but we still avoid the extra clone
    /// into a standalone `String` when the limit is exceeded.
    struct Base64Visitor;

    impl<'de> serde::de::Visitor<'de> for Base64Visitor {
        type Value = Vec<u8>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a base64-encoded string")
        }

        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            check_and_decode(v)
        }

        fn visit_borrowed_str<E: serde::de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
            check_and_decode(v)
        }

        fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
            check_and_decode(&v)
        }
    }

    /// Check the string length against the limit, then decode.
    /// Shared by all visitor methods to keep the logic in one place.
    fn check_and_decode<E: serde::de::Error>(s: &str) -> Result<Vec<u8>, E> {
        if s.len() > MAX_BASE64_LEN {
            return Err(E::custom(format!(
                "base64 string too large: {} bytes exceeds {} byte limit",
                s.len(),
                MAX_BASE64_LEN
            )));
        }
        STANDARD
            .decode(s)
            .map_err(|e| E::custom(format!("invalid base64: {}", e)))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(Base64Visitor)
    }
}
