//! # Tracing / Logging
//!
//! Audex supports structured tracing via the `tracing` crate when
//! the `"tracing"` feature is enabled. All trace points compile away
//! to nothing when the feature is disabled, ensuring zero overhead
//! for users who do not need observability.
//!
//! ## Setup
//!
//! ```toml
//! [dependencies]
//! audex = { version = "0.1", features = ["tracing"] }
//! tracing-subscriber = "0.3"
//! ```
//!
//! ```rust,ignore
//! // Initialize a subscriber in your application:
//! tracing_subscriber::fmt()
//!     .with_env_filter("audex=debug")
//!     .init();
//!
//! // Now all audex operations emit structured events:
//! let file = audex::File::load("song.mp3")?;
//! // Output:
//! // DEBUG audex::file: format detected format="MP3"
//! // DEBUG audex::mp3: MPEG stream info parsed bitrate=320 sample_rate=44100 channels=2
//! // INFO  audex::file: file loaded successfully format="MP3" tags_present=true
//! ```
//!
//! ## Verbosity Levels
//!
//! | Filter          | What you see                                    |
//! |-----------------|-------------------------------------------------|
//! | `audex=error`   | Only unrecoverable failures                     |
//! | `audex=warn`    | Recoverable issues + errors                     |
//! | `audex=info`    | Operation lifecycle + warnings + errors          |
//! | `audex=debug`   | Parsed summaries + all above                    |
//! | `audex=trace`   | Per-item parsing details (very verbose)          |
//!
//! ## Level Semantics
//!
//! - **ERROR** — Unrecoverable failures that will be returned as `Err`.
//!   Example: `error_event!("FLAC header magic not found")`
//!
//! - **WARN** — Recoverable issues, degraded results, or skipped data.
//!   Example: `warn_event!("unknown ID3v2 frame skipped: {}", frame_id)`
//!
//! - **INFO** — High-level operation lifecycle (start/end of load/save).
//!   Example: `info_event!("file loaded successfully")`
//!
//! - **DEBUG** — Format detection results, parsed component summaries.
//!   Example: `debug_event!(tag_count = 15, "Vorbis Comment parsed")`
//!
//! - **TRACE** — Per-item/per-byte parsing details (very verbose).
//!   Example: `trace_event!(frame_id = "TIT2", size = 42, "parsing frame")`
//!
//! ## Recommended Subscriber Configuration
//!
//! ```rust,ignore
//! // In your application:
//! tracing_subscriber::fmt()
//!     .with_env_filter("audex=debug") // or "audex=trace" for full detail
//!     .init();
//! ```

// ---------------------------------------------------------------------------
// Conditional event macros — forward to `tracing` when the feature is enabled,
// compile to nothing when it is disabled.
// ---------------------------------------------------------------------------

/// Emit a TRACE-level event (per-item parsing details, very verbose).
#[cfg(feature = "tracing")]
macro_rules! trace_event {
    ($($arg:tt)*) => { tracing::trace!($($arg)*) }
}
#[cfg(not(feature = "tracing"))]
macro_rules! trace_event {
    ($($arg:tt)*) => {
        ()
    };
}

/// Emit a DEBUG-level event (parsed summaries, format detection results).
#[cfg(feature = "tracing")]
macro_rules! debug_event {
    ($($arg:tt)*) => { tracing::debug!($($arg)*) }
}
#[cfg(not(feature = "tracing"))]
macro_rules! debug_event {
    ($($arg:tt)*) => {
        ()
    };
}

/// Emit an INFO-level event (operation lifecycle — load/save start and end).
#[cfg(feature = "tracing")]
macro_rules! info_event {
    ($($arg:tt)*) => { tracing::info!($($arg)*) }
}
#[cfg(not(feature = "tracing"))]
macro_rules! info_event {
    ($($arg:tt)*) => {
        ()
    };
}

/// Emit a WARN-level event (recoverable issues, skipped data).
#[cfg(feature = "tracing")]
macro_rules! warn_event {
    ($($arg:tt)*) => { tracing::warn!($($arg)*) }
}
#[cfg(not(feature = "tracing"))]
macro_rules! warn_event {
    ($($arg:tt)*) => {
        ()
    };
}

/// Emit an ERROR-level event (unrecoverable failures).
#[cfg(feature = "tracing")]
macro_rules! error_event {
    ($($arg:tt)*) => { tracing::error!($($arg)*) }
}
#[cfg(not(feature = "tracing"))]
macro_rules! error_event {
    ($($arg:tt)*) => {
        ()
    };
}
