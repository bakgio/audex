//! Tracing instrumentation tests for audex
//!
//! These tests verify that the tracing integration emits the expected
//! structured events at each verbosity level when audio files are
//! loaded, saved, and processed.

#![cfg(feature = "tracing")]

mod common;

use common::TestUtils;
use std::io::{Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
use tempfile::NamedTempFile;

use audex::FileType;

// ---------------------------------------------------------------------------
// Capture infrastructure — collect tracing events as formatted strings
// ---------------------------------------------------------------------------

/// Shared buffer that collects formatted tracing output.
#[derive(Clone, Default)]
struct CapturedLogs {
    inner: Arc<Mutex<Vec<String>>>,
}

impl CapturedLogs {
    fn contains(&self, needle: &str) -> bool {
        let logs = self.inner.lock().unwrap();
        logs.iter().any(|line| line.contains(needle))
    }
}

impl std::io::Write for CapturedLogs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = String::from_utf8_lossy(buf).to_string();
        self.inner.lock().unwrap().push(s);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl tracing_subscriber::fmt::MakeWriter<'_> for CapturedLogs {
    type Writer = CapturedLogs;

    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}

/// Install a global subscriber that captures all events to a `CapturedLogs`
/// buffer. Returns the buffer for assertion checking.
///
/// Because `tracing` only allows one global subscriber, we use
/// `set_global_default` on first call and rely on test ordering
/// within the binary. Each test reads the shared buffer.
///
/// This helper uses `with_default` so the subscriber is scoped to the
/// calling closure.
fn with_tracing<F>(f: F, logs: &CapturedLogs)
where
    F: FnOnce(),
{
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;

    let fmt_layer = fmt::layer()
        .with_writer(logs.clone())
        .with_ansi(false)
        .with_target(true)
        .with_level(true);

    let subscriber = tracing_subscriber::registry().with(fmt_layer);

    tracing::subscriber::with_default(subscriber, f);
}

// ---------------------------------------------------------------------------
// Helper: create a temp copy suitable for read-write operations
// ---------------------------------------------------------------------------

fn get_temp_copy(filename: &str) -> NamedTempFile {
    let path = TestUtils::data_path(filename);
    TestUtils::get_temp_copy(&path).expect("failed to create temp copy of test file")
}

// ---------------------------------------------------------------------------
// File::load — lifecycle events
// ---------------------------------------------------------------------------

/// Loading an MP3 must emit an info-level "loading audio file" event.
#[test]
fn test_load_mp3_emits_info_event() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("silence-44-s.mp3");
            let _file = audex::File::load(&path).expect("failed to load MP3");
        },
        &logs,
    );
    assert!(
        logs.contains("loading audio file"),
        "expected 'loading audio file' event"
    );
}

/// After format detection, a debug-level "format detected" event must appear.
#[test]
fn test_load_mp3_emits_format_detected() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("silence-44-s.mp3");
            let _file = audex::File::load(&path).expect("failed to load MP3");
        },
        &logs,
    );
    assert!(
        logs.contains("format detected"),
        "expected 'format detected' event"
    );
}

/// The MP3 parser should log that it is parsing an MP3 file.
#[test]
fn test_load_mp3_emits_stream_info() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("silence-44-s.mp3");
            let _file = audex::File::load(&path).expect("failed to load MP3");
        },
        &logs,
    );
    assert!(
        logs.contains("parsing MP3 file"),
        "expected 'parsing MP3 file' event"
    );
}

// ---------------------------------------------------------------------------
// FLAC loading events
// ---------------------------------------------------------------------------

/// FLAC loading must emit a "parsing FLAC file" event.
#[test]
fn test_load_flac_emits_metadata_blocks() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("silence-44-s.flac");
            let _file = audex::File::load(&path).expect("failed to load FLAC");
        },
        &logs,
    );
    assert!(
        logs.contains("parsing FLAC file"),
        "expected 'parsing FLAC file' event"
    );
}

/// After loading FLAC, a STREAMINFO parsed event should appear.
#[test]
fn test_load_flac_emits_tag_count() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("silence-44-s.flac");
            let _file = audex::File::load(&path).expect("failed to load FLAC");
        },
        &logs,
    );
    assert!(
        logs.contains("STREAMINFO parsed"),
        "expected 'STREAMINFO parsed' event"
    );
}

// ---------------------------------------------------------------------------
// MP4 loading events
// ---------------------------------------------------------------------------

/// MP4 loading should emit parsing events.
#[test]
fn test_load_mp4_emits_atom_traversal() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("has-tags.m4a");
            let _file = audex::File::load(&path).expect("failed to load M4A");
        },
        &logs,
    );
    assert!(
        logs.contains("parsing MP4 file"),
        "expected 'parsing MP4 file' event"
    );
}

// ---------------------------------------------------------------------------
// Save lifecycle events
// ---------------------------------------------------------------------------

/// Saving a file must emit an info-level "saving audio file" event.
#[test]
fn test_save_emits_info_event() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let temp = get_temp_copy("silence-44-s.mp3");
            let path = temp.path().to_path_buf();
            let mut file = audex::File::load(&path).expect("failed to load MP3");
            file.save().expect("failed to save");
        },
        &logs,
    );
    assert!(
        logs.contains("saving audio file"),
        "expected 'saving audio file' event"
    );
}

/// After a successful save, a "file saved successfully" event must appear.
#[test]
fn test_save_emits_success() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let temp = get_temp_copy("silence-44-s.mp3");
            let path = temp.path().to_path_buf();
            let mut file = audex::File::load(&path).expect("failed to load MP3");
            file.save().expect("failed to save");
        },
        &logs,
    );
    assert!(
        logs.contains("file saved successfully"),
        "expected 'file saved successfully' event"
    );
}

// ---------------------------------------------------------------------------
// Format detection scoring events
// ---------------------------------------------------------------------------

/// Format detection should log the winning format.
#[test]
fn test_format_detection_emits_winner() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("silence-44-s.mp3");
            let _file = audex::File::load(&path).expect("failed to load MP3");
        },
        &logs,
    );
    assert!(
        logs.contains("winning format detected"),
        "expected 'winning format detected' event"
    );
}

/// Format detection must log individual format scores at trace level.
#[test]
fn test_format_detection_emits_scores() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("silence-44-s.mp3");
            let _file = audex::File::load(&path).expect("failed to load MP3");
        },
        &logs,
    );
    assert!(
        logs.contains("format score"),
        "expected 'format score' events"
    );
}

// ---------------------------------------------------------------------------
// Error path events
// ---------------------------------------------------------------------------

/// Attempting to load garbage data should emit warn events.
#[test]
fn test_error_path_emits_warn() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let mut temp = NamedTempFile::new().expect("failed to create temp file");
            temp.write_all(b"NOT_AN_AUDIO_FILE_HEADER_GARBAGE_DATA_1234567890")
                .unwrap();
            temp.flush().unwrap();
            temp.seek(SeekFrom::Start(0)).unwrap();
            let _result = audex::File::load(temp.path());
        },
        &logs,
    );
    assert!(
        logs.contains("no format could handle this file"),
        "expected warning about unrecognized format"
    );
}

// ---------------------------------------------------------------------------
// Vorbis Comment trace events
// ---------------------------------------------------------------------------

/// Loading an OGG Vorbis file should emit parsing events.
#[test]
fn test_vorbis_comment_emits_trace() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("empty.ogg");
            let _file = audex::File::load(&path).expect("failed to load OGG Vorbis");
        },
        &logs,
    );
    assert!(
        logs.contains("parsing OGG Vorbis file"),
        "expected 'parsing OGG Vorbis file' event"
    );
}

// ---------------------------------------------------------------------------
// APEv2 loading events
// ---------------------------------------------------------------------------

/// Loading an APEv2-tagged file should emit debug events.
#[test]
fn test_apev2_load_emits_debug() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("click.mpc");
            let _file = audex::File::load(&path).expect("failed to load MPC");
        },
        &logs,
    );
    assert!(
        logs.contains("parsing Musepack file"),
        "expected 'parsing Musepack file' event"
    );
}

// ---------------------------------------------------------------------------
// EasyID3 key registration events
// ---------------------------------------------------------------------------

/// Registering an EasyID3 key should emit a debug event.
#[test]
fn test_easyid3_key_registration_emits_debug() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let temp = get_temp_copy("silence-44-s.mp3");
            let mut easy =
                audex::easyid3::EasyID3::load(temp.path()).expect("failed to load EasyID3");
            easy.register_text_key("my_field", "TXXX")
                .expect("failed to register key");
        },
        &logs,
    );
    assert!(
        logs.contains("registered EasyID3 text key"),
        "expected 'registered EasyID3 text key' event"
    );
}

// ---------------------------------------------------------------------------
// No-subscriber safety
// ---------------------------------------------------------------------------

/// Loading without any tracing subscriber must not panic.
#[test]
fn test_no_events_without_subscriber() {
    let path = TestUtils::data_path("silence-44-s.mp3");
    let _file = audex::File::load(&path).expect("failed to load MP3");
}

// ---------------------------------------------------------------------------
// Custom filter level verification
// ---------------------------------------------------------------------------

/// Verify that info-level events fire correctly.
#[test]
fn test_custom_filter_level() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let path = TestUtils::data_path("silence-44-s.mp3");
            let _file = audex::File::load(&path).expect("failed to load MP3");
        },
        &logs,
    );
    assert!(
        logs.contains("loading audio file"),
        "expected 'loading audio file' event"
    );
    assert!(
        logs.contains("file loaded successfully"),
        "expected 'file loaded successfully' event"
    );
}

// ---------------------------------------------------------------------------
// Integration: corrupted file trace story
// ---------------------------------------------------------------------------

/// Load a truncated FLAC file and verify the trace output includes
/// diagnostic events.
#[test]
fn test_debug_corrupted_file_trace() {
    let logs = CapturedLogs::default();
    with_tracing(
        || {
            let mut temp = NamedTempFile::new().expect("temp file");
            temp.write_all(b"fLaC").unwrap();
            temp.write_all(&[0x00, 0x00, 0x00, 0x02]).unwrap();
            temp.flush().unwrap();
            temp.seek(SeekFrom::Start(0)).unwrap();
            let _result = audex::File::load(temp.path());
        },
        &logs,
    );
    assert!(
        logs.contains("format score") || logs.contains("parsing FLAC file"),
        "expected tracing events for corrupted FLAC"
    );
}
