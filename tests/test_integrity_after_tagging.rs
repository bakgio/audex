/// Verifies that audio stream properties are fully preserved after tag operations.
///
/// For each format: loads the original file, records all stream info fields
/// (sample_rate, channels, bits_per_sample, length, bitrate), writes tags,
/// saves to an in-memory buffer, reloads, and asserts all audio properties
/// are unchanged. This ensures tag writes never corrupt the audio stream.
///
/// Original test data files are never modified — all saves target in-memory cursors.
use audex::{File, StreamInfo};
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;

fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

/// Snapshot of all stream info properties for comparison.
#[derive(Debug)]
struct StreamSnapshot {
    sample_rate: Option<u32>,
    channels: Option<u16>,
    bits_per_sample: Option<u16>,
    length: Option<Duration>,
    bitrate: Option<u32>,
}

impl StreamSnapshot {
    fn capture(info: &dyn StreamInfo) -> Self {
        Self {
            sample_rate: info.sample_rate(),
            channels: info.channels(),
            bits_per_sample: info.bits_per_sample(),
            length: info.length(),
            bitrate: info.bitrate(),
        }
    }

    fn assert_matches(&self, other: &StreamSnapshot, filename: &str) {
        assert_eq!(
            self.sample_rate, other.sample_rate,
            "{}: sample_rate changed after tagging",
            filename
        );
        assert_eq!(
            self.channels, other.channels,
            "{}: channels changed after tagging",
            filename
        );
        assert_eq!(
            self.bits_per_sample, other.bits_per_sample,
            "{}: bits_per_sample changed after tagging",
            filename
        );

        self.assert_bitrate(other, filename);

        match (self.length, other.length) {
            (Some(a), Some(b)) => {
                let diff = a.abs_diff(b);
                assert!(
                    diff < Duration::from_millis(50),
                    "{}: length changed after tagging ({:?} -> {:?})",
                    filename,
                    a,
                    b
                );
            }
            (None, None) => {}
            _ => panic!(
                "{}: length availability changed after tagging ({:?} -> {:?})",
                filename, self.length, other.length
            ),
        }
    }

    fn assert_bitrate(&self, other: &StreamSnapshot, filename: &str) {
        // Musepack SV7+ derives bitrate from total file size / duration,
        // so adding tag data increases the reported bitrate. For these
        // files we verify the bitrate is still present and reasonable
        // rather than demanding an exact match.
        let is_musepack = filename.ends_with(".mpc");

        match (self.bitrate, other.bitrate) {
            (Some(a), Some(b)) if is_musepack => {
                assert!(
                    b >= a,
                    "{}: bitrate decreased after tagging ({} -> {})",
                    filename,
                    a,
                    b
                );
            }
            (Some(a), Some(b)) => {
                assert_eq!(a, b, "{}: bitrate changed after tagging", filename);
            }
            (None, None) => {}
            _ => panic!(
                "{}: bitrate availability changed after tagging ({:?} -> {:?})",
                filename, self.bitrate, other.bitrate
            ),
        }
    }
}

/// Adds tags to a file and verifies all audio stream properties are preserved.
fn tag_and_verify_integrity(filename: &str, tag_key: &str, tag_value: &str) {
    let path = data_path(filename);
    if !path.exists() {
        eprintln!("skipping {}: file not found", filename);
        return;
    }

    let data = std::fs::read(&path).unwrap();

    // Capture original stream properties
    let original = File::load_from_reader(Cursor::new(data.clone()), Some(path.clone()))
        .unwrap_or_else(|e| panic!("{}: load failed: {}", filename, e));

    let original_format = original.format_name();
    let before = StreamSnapshot::capture(&original.info());
    drop(original);

    // Load fresh copy, write tags, save to buffer
    let mut file = File::load_from_reader(Cursor::new(data.clone()), Some(path.clone()))
        .unwrap_or_else(|e| panic!("{}: second load failed: {}", filename, e));

    if !file.has_tags() {
        file.add_tags()
            .unwrap_or_else(|e| panic!("{}: add_tags failed: {}", filename, e));
    }

    file.set_single(tag_key, tag_value.to_string())
        .unwrap_or_else(|e| panic!("{}: set tag failed: {}", filename, e));

    let mut out = Cursor::new(data);
    file.save_to_writer(&mut out)
        .unwrap_or_else(|e| panic!("{}: save_to_writer failed: {}", filename, e));

    // Reload and verify all properties survived
    let reloaded = File::load_from_reader(Cursor::new(out.into_inner()), Some(path))
        .unwrap_or_else(|e| panic!("{}: reload failed: {}", filename, e));

    assert_eq!(
        original_format,
        reloaded.format_name(),
        "{}: format detection changed after tagging",
        filename
    );

    let after = StreamSnapshot::capture(&reloaded.info());
    before.assert_matches(&after, filename);
}

// ===========================================================================
// ID3v2 formats
// ===========================================================================

#[test]
fn integrity_mp3() {
    tag_and_verify_integrity("silence-44-s.mp3", "TIT2", "Integrity Check");
}

#[test]
fn integrity_mp3_untagged() {
    tag_and_verify_integrity("no-tags.mp3", "TIT2", "Integrity Check");
}

#[test]
fn integrity_aiff() {
    tag_and_verify_integrity("with-id3.aif", "TIT2", "Integrity Check");
}

#[test]
fn integrity_wav() {
    tag_and_verify_integrity(
        "silence-2s-PCM-44100-16-ID3v23.wav",
        "TIT2",
        "Integrity Check",
    );
}

#[test]
fn integrity_wav_untagged() {
    tag_and_verify_integrity(
        "silence-2s-PCM-16000-08-notags.wav",
        "TIT2",
        "Integrity Check",
    );
}

#[test]
fn integrity_dsf() {
    tag_and_verify_integrity("with-id3.dsf", "TIT2", "Integrity Check");
}

#[test]
fn integrity_dsf_untagged() {
    tag_and_verify_integrity("without-id3.dsf", "TIT2", "Integrity Check");
}

#[test]
fn integrity_dsdiff() {
    tag_and_verify_integrity("5644800-2ch-s01-silence.dff", "TIT2", "Integrity Check");
}

// ===========================================================================
// Vorbis Comment formats
// ===========================================================================

#[test]
fn integrity_flac() {
    tag_and_verify_integrity("silence-44-s.flac", "TITLE", "Integrity Check");
}

#[test]
fn integrity_flac_untagged() {
    tag_and_verify_integrity("no-tags.flac", "TITLE", "Integrity Check");
}

#[test]
fn integrity_ogg_vorbis() {
    tag_and_verify_integrity("multipagecomment.ogg", "TITLE", "Integrity Check");
}

#[test]
fn integrity_opus() {
    tag_and_verify_integrity("example.opus", "TITLE", "Integrity Check");
}

#[test]
fn integrity_speex() {
    tag_and_verify_integrity("empty.spx", "TITLE", "Integrity Check");
}

#[test]
fn integrity_ogg_flac() {
    tag_and_verify_integrity("empty.oggflac", "TITLE", "Integrity Check");
}

#[test]
fn integrity_ogg_theora() {
    tag_and_verify_integrity("sample.oggtheora", "TITLE", "Integrity Check");
}

// ===========================================================================
// MP4 atoms
// ===========================================================================

#[test]
fn integrity_m4a() {
    tag_and_verify_integrity("has-tags.m4a", "\u{00a9}nam", "Integrity Check");
}

#[test]
fn integrity_m4a_untagged() {
    tag_and_verify_integrity("no-tags.m4a", "\u{00a9}nam", "Integrity Check");
}

// ===========================================================================
// APEv2 formats
// ===========================================================================

#[test]
fn integrity_monkeysaudio() {
    tag_and_verify_integrity("mac-399.ape", "Title", "Integrity Check");
}

#[test]
fn integrity_musepack() {
    tag_and_verify_integrity("click.mpc", "Title", "Integrity Check");
}

#[test]
fn integrity_wavpack() {
    tag_and_verify_integrity("silence-44-s.wv", "Title", "Integrity Check");
}

#[test]
fn integrity_optimfrog() {
    tag_and_verify_integrity("silence-2s-44100-16.ofr", "Title", "Integrity Check");
}

#[test]
fn integrity_tak() {
    tag_and_verify_integrity("has-tags.tak", "Title", "Integrity Check");
}

#[test]
fn integrity_trueaudio() {
    tag_and_verify_integrity("empty.tta", "Title", "Integrity Check");
}

// ===========================================================================
// ASF / WMA
// ===========================================================================

#[test]
fn integrity_wma() {
    tag_and_verify_integrity("silence-1.wma", "Title", "Integrity Check");
}
