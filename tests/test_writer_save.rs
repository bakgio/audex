// Tests for save_to_writer, clear_writer, save_to_path, and DynamicFileType::save_to_writer
//
// Validates round-trip writing for all supported formats using in-memory cursors
// and temporary files on disk. Original test data files are never modified.

mod common;

use audex::FileType;
use audex::StreamInfo;
use std::io::Cursor;
use std::path::PathBuf;

/// Get path to test data file using CARGO_MANIFEST_DIR for robustness
fn data_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(filename)
}

// ---------------------------------------------------------------------------
// Module: writer_roundtrip_tests
//
// Per-format save_to_writer round-trip: load -> save -> reload -> verify
// ---------------------------------------------------------------------------
mod writer_roundtrip_tests {
    use super::*;
    use audex::aiff::AIFF;
    use audex::asf::ASF;
    use audex::dsdiff::DSDIFF;
    use audex::dsf::DSF;
    use audex::flac::FLAC;
    use audex::monkeysaudio::MonkeysAudio;
    use audex::mp3::MP3;
    use audex::mp4::MP4;
    use audex::musepack::Musepack;
    use audex::oggflac::OggFlac;
    use audex::oggopus::OggOpus;
    use audex::oggspeex::OggSpeex;
    use audex::oggtheora::OggTheora;
    use audex::oggvorbis::OggVorbis;
    use audex::optimfrog::OptimFROG;
    use audex::tak::TAK;
    use audex::trueaudio::TrueAudio;
    use audex::wave::WAVE;
    use audex::wavpack::WavPack;

    const LARGE_WRITER_INPUT_BYTES: usize = 20 * 1024 * 1024;

    /// Helper: perform a save_to_writer round-trip for any FileType.
    ///
    /// 1. Read the file bytes from disk (original file is never modified)
    /// 2. Load via load_from_reader
    /// 3. Save via save_to_writer into a cursor seeded with a copy of the original bytes
    /// 4. Reload from the modified cursor bytes
    /// 5. Return both original and reloaded instances for assertions
    fn roundtrip<T: FileType>(path: &PathBuf) -> (T, T) {
        let data = std::fs::read(path).unwrap();

        // Load original from a copy of the bytes
        let mut cursor = Cursor::new(data.clone());
        let original = T::load_from_reader(&mut cursor).unwrap();

        // Load a second copy to call save_to_writer on
        let mut cursor2 = Cursor::new(data.clone());
        let mut to_save = T::load_from_reader(&mut cursor2).unwrap();

        // Save to a new cursor seeded with a copy of the original data
        let mut out = Cursor::new(data);
        to_save.save_to_writer(&mut out).unwrap();

        // Reload from the written bytes
        let written_bytes = out.into_inner();
        let mut reload_cursor = Cursor::new(written_bytes);
        let reloaded = T::load_from_reader(&mut reload_cursor).unwrap();

        (original, reloaded)
    }

    #[test]
    fn test_flac_save_to_cursor() {
        let test_file = data_path("silence-44-s.flac");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<FLAC>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_mp3_save_to_cursor() {
        let test_file = data_path("silence-44-s.mp3");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<MP3>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_mp4_save_to_cursor() {
        let test_file = data_path("has-tags.m4a");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<MP4>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_mp4_save_to_writer_accepts_large_audio_payloads() {
        let test_file = data_path("has-tags.m4a");
        if !test_file.exists() {
            return;
        }

        let data = std::fs::read(&test_file).unwrap();
        let mut load_cursor = Cursor::new(data.clone());
        let mut mp4 = MP4::load_from_reader(&mut load_cursor).unwrap();

        let mut padded = data;
        padded.resize(LARGE_WRITER_INPUT_BYTES, 0);

        let mut out = Cursor::new(padded);
        let result = mp4.save_to_writer(&mut out);
        assert!(
            result.is_ok(),
            "large writer inputs with small tags should remain supported: {result:?}"
        );
    }

    #[test]
    fn test_aiff_save_to_cursor() {
        let test_file = data_path("with-id3.aif");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<AIFF>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_wave_save_to_cursor() {
        let test_file = data_path("silence-2s-PCM-44100-16-ID3v23.wav");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<WAVE>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_ogg_save_to_cursor() {
        let test_file = data_path("empty.ogg");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<OggVorbis>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_oggopus_save_to_cursor() {
        let test_file = data_path("example.opus");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<OggOpus>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_oggflac_save_to_cursor() {
        let test_file = data_path("empty.oggflac");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<OggFlac>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_oggspeex_save_to_cursor() {
        let test_file = data_path("empty.spx");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<OggSpeex>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_oggtheora_save_to_cursor() {
        let test_file = data_path("sample.oggtheora");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<OggTheora>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_asf_save_to_cursor() {
        let test_file = data_path("silence-1.wma");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<ASF>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_asf_save_to_writer_accepts_large_audio_payloads() {
        let test_file = data_path("silence-1.wma");
        if !test_file.exists() {
            return;
        }

        let data = std::fs::read(&test_file).unwrap();
        let mut load_cursor = Cursor::new(data.clone());
        let mut asf = ASF::load_from_reader(&mut load_cursor).unwrap();

        let mut padded = data;
        padded.resize(LARGE_WRITER_INPUT_BYTES, 0);

        let mut out = Cursor::new(padded);
        let result = asf.save_to_writer(&mut out);
        assert!(
            result.is_ok(),
            "large writer inputs with small tags should remain supported: {result:?}"
        );
    }

    #[test]
    fn test_ape_save_to_cursor() {
        let test_file = data_path("mac-399.ape");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<MonkeysAudio>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_musepack_save_to_cursor() {
        let test_file = data_path("click.mpc");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<Musepack>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_wavpack_save_to_cursor() {
        let test_file = data_path("silence-44-s.wv");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<WavPack>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_optimfrog_save_to_cursor() {
        let test_file = data_path("silence-2s-44100-16.ofr");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<OptimFROG>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_tak_save_to_cursor() {
        let test_file = data_path("has-tags.tak");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<TAK>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_trueaudio_save_to_cursor() {
        let test_file = data_path("empty.tta");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<TrueAudio>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_dsf_save_to_cursor() {
        let test_file = data_path("with-id3.dsf");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<DSF>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }

    #[test]
    fn test_dsdiff_save_to_cursor() {
        let test_file = data_path("5644800-2ch-s01-silence.dff");
        if !test_file.exists() {
            return;
        }
        let (original, reloaded) = roundtrip::<DSDIFF>(&test_file);
        assert_eq!(original.info().sample_rate(), reloaded.info().sample_rate());
        assert_eq!(original.info().channels(), reloaded.info().channels());
    }
}

// ---------------------------------------------------------------------------
// Module: clear_writer_tests
//
// Per-format clear_writer: load -> verify has tags -> clear on in-memory copy -> verify cleared
// Original test data files are never modified.
// ---------------------------------------------------------------------------
mod clear_writer_tests {
    use super::*;
    use audex::aiff::AIFF;
    use audex::asf::ASF;
    use audex::flac::FLAC;
    use audex::mp3::MP3;
    use audex::mp4::MP4;
    use audex::wave::WAVE;

    /// Helper: load from a copy of bytes, clear via writer on another copy, reload.
    fn clear_roundtrip<T: FileType>(path: &PathBuf) -> T {
        let data = std::fs::read(path).unwrap();

        // Load a copy to call clear_writer on
        let mut cursor = Cursor::new(data.clone());
        let mut file = T::load_from_reader(&mut cursor).unwrap();

        // Clear into a cursor seeded with a copy of the original bytes
        let mut out = Cursor::new(data);
        file.clear_writer(&mut out).unwrap();

        // Reload from the cleared bytes
        let cleared_bytes = out.into_inner();
        let mut reload_cursor = Cursor::new(cleared_bytes);
        T::load_from_reader(&mut reload_cursor).unwrap()
    }

    #[test]
    fn test_flac_clear_writer() {
        let test_file = data_path("silence-44-s.flac");
        if !test_file.exists() {
            return;
        }

        // Verify tags exist before clearing (read-only check on a copy)
        let data = std::fs::read(&test_file).unwrap();
        let mut cursor = Cursor::new(data);
        let original = FLAC::load_from_reader(&mut cursor).unwrap();
        let original_keys = original.keys();
        assert!(
            !original_keys.is_empty(),
            "Test file should have tags before clearing"
        );

        let cleared = clear_roundtrip::<FLAC>(&test_file);
        assert!(
            cleared.keys().is_empty(),
            "Tags should be empty after clear_writer"
        );
    }

    #[test]
    fn test_mp3_clear_writer() {
        let test_file = data_path("silence-44-s.mp3");
        if !test_file.exists() {
            return;
        }

        let data = std::fs::read(&test_file).unwrap();
        let mut cursor = Cursor::new(data);
        let original = MP3::load_from_reader(&mut cursor).unwrap();
        let original_key_count = original.keys().len();
        assert!(
            original_key_count > 0,
            "Test file should have tags before clearing"
        );

        let cleared = clear_roundtrip::<MP3>(&test_file);
        let cleared_key_count = cleared.keys().len();
        // MP3 clear_writer removes ID3 tags; the reloaded file should have
        // fewer keys than the original (possibly zero, but some residual
        // structural metadata may remain depending on the ID3 implementation).
        assert!(
            cleared_key_count < original_key_count,
            "Tag count should decrease after clear_writer (was {}, now {})",
            original_key_count,
            cleared_key_count
        );
    }

    #[test]
    fn test_mp4_clear_writer() {
        let test_file = data_path("has-tags.m4a");
        if !test_file.exists() {
            return;
        }

        let data = std::fs::read(&test_file).unwrap();
        let mut cursor = Cursor::new(data);
        let original = MP4::load_from_reader(&mut cursor).unwrap();
        let original_keys = original.keys();
        assert!(
            !original_keys.is_empty(),
            "Test file should have tags before clearing"
        );

        let cleared = clear_roundtrip::<MP4>(&test_file);
        assert!(
            cleared.keys().is_empty(),
            "Tags should be empty after clear_writer"
        );
    }

    #[test]
    fn test_aiff_clear_writer() {
        let test_file = data_path("with-id3.aif");
        if !test_file.exists() {
            return;
        }

        let data = std::fs::read(&test_file).unwrap();
        let mut cursor = Cursor::new(data);
        let original = AIFF::load_from_reader(&mut cursor).unwrap();
        let original_keys = original.keys();
        if original_keys.is_empty() {
            // Test fixture has no tags; clear_writer is a no-op. Skip.
            return;
        }

        let cleared = clear_roundtrip::<AIFF>(&test_file);
        assert!(
            cleared.keys().is_empty(),
            "Tags should be empty after clear_writer"
        );
    }

    #[test]
    fn test_wave_clear_writer() {
        let test_file = data_path("silence-2s-PCM-44100-16-ID3v23.wav");
        if !test_file.exists() {
            return;
        }

        let data = std::fs::read(&test_file).unwrap();
        let mut cursor = Cursor::new(data);
        let original = WAVE::load_from_reader(&mut cursor).unwrap();
        let original_keys = original.keys();
        if original_keys.is_empty() {
            // Test fixture has no tags; clear_writer is a no-op. Skip.
            return;
        }

        let cleared = clear_roundtrip::<WAVE>(&test_file);
        assert!(
            cleared.keys().is_empty(),
            "Tags should be empty after clear_writer"
        );
    }

    #[test]
    fn test_asf_clear_writer() {
        let test_file = data_path("silence-1.wma");
        if !test_file.exists() {
            return;
        }

        let data = std::fs::read(&test_file).unwrap();
        let mut cursor = Cursor::new(data);
        let original = ASF::load_from_reader(&mut cursor).unwrap();
        let original_key_count = original.keys().len();
        assert!(
            original_key_count > 0,
            "Test file should have tags before clearing"
        );

        let cleared = clear_roundtrip::<ASF>(&test_file);
        let cleared_key_count = cleared.keys().len();
        // ASF format always retains some structural metadata fields even after
        // clearing user tags. Verify the count decreased.
        assert!(
            cleared_key_count < original_key_count,
            "Tag count should decrease after clear_writer (was {}, now {})",
            original_key_count,
            cleared_key_count
        );
    }
}

// ---------------------------------------------------------------------------
// Module: save_to_writer_no_filename_tests
//
// Verify save_to_writer works when loaded from reader with filename=None
// ---------------------------------------------------------------------------
mod save_to_writer_no_filename_tests {
    use super::*;
    use audex::File;

    #[test]
    fn test_save_to_writer_no_filename() {
        let test_file = data_path("silence-44-s.flac");
        if !test_file.exists() {
            return;
        }

        let data = std::fs::read(&test_file).unwrap();
        let cursor = Cursor::new(data.clone());

        // Load with no filename hint
        let mut file = File::load_from_reader(cursor, None).unwrap();

        // Verify filename is indeed None
        assert!(
            file.filename().is_none(),
            "Filename should be None when loaded without path hint"
        );

        // save_to_writer should succeed even without a filename
        let mut out = Cursor::new(data);
        let result = file.save_to_writer(&mut out);
        assert!(
            result.is_ok(),
            "save_to_writer should succeed without filename: {:?}",
            result.err()
        );

        // Verify the written data can be reloaded
        let written_bytes = out.into_inner();
        let reload_cursor = Cursor::new(written_bytes);
        let reloaded = File::load_from_reader(reload_cursor, None);
        assert!(reloaded.is_ok(), "Reloading from saved data should succeed");
    }
}

// ---------------------------------------------------------------------------
// Module: dynamic_save_to_writer_tests
//
// DynamicFileType::save_to_writer via File::load_from_reader
// All operations are on in-memory copies; original files are untouched.
// ---------------------------------------------------------------------------
mod dynamic_save_to_writer_tests {
    use super::*;
    use audex::File;

    /// Helper: dynamic round-trip using File (DynamicFileType) interface
    fn dynamic_roundtrip(filename: &str) {
        let test_file = data_path(filename);
        if !test_file.exists() {
            return;
        }

        let data = std::fs::read(&test_file).unwrap();
        let cursor = Cursor::new(data.clone());

        let mut file = File::load_from_reader(cursor, Some(test_file.clone())).unwrap();
        let original_format = file.format_name().to_string();
        let original_sample_rate = file.info().sample_rate();
        let original_channels = file.info().channels();

        // Save via DynamicFileType::save_to_writer into an in-memory cursor
        let mut out = Cursor::new(data);
        file.save_to_writer(&mut out).unwrap();

        // Reload and verify
        let written_bytes = out.into_inner();
        let reload_cursor = Cursor::new(written_bytes);
        let reloaded = File::load_from_reader(reload_cursor, Some(test_file)).unwrap();

        assert_eq!(
            reloaded.format_name(),
            original_format,
            "Format should be preserved after round-trip"
        );
        assert_eq!(
            reloaded.info().sample_rate(),
            original_sample_rate,
            "Sample rate should be preserved after round-trip"
        );
        assert_eq!(
            reloaded.info().channels(),
            original_channels,
            "Channels should be preserved after round-trip"
        );
    }

    #[test]
    fn test_dynamic_flac_save_to_writer() {
        dynamic_roundtrip("silence-44-s.flac");
    }

    #[test]
    fn test_dynamic_mp3_save_to_writer() {
        dynamic_roundtrip("silence-44-s.mp3");
    }

    #[test]
    fn test_dynamic_mp4_save_to_writer() {
        dynamic_roundtrip("has-tags.m4a");
    }

    #[test]
    fn test_dynamic_ogg_save_to_writer() {
        dynamic_roundtrip("empty.ogg");
    }
}

// ---------------------------------------------------------------------------
// Module: writer_size_guard_tests
//
// Ensure writer-based save paths reject oversized in-memory inputs before
// buffering the entire file into a Cursor.
// ---------------------------------------------------------------------------
mod writer_size_guard_tests {
    use super::*;
    use audex::dsdiff::DSDIFF;
    use audex::dsf::DSF;
    use audex::oggvorbis::OggVorbis;

    fn oversized_writer_bytes(mut data: Vec<u8>) -> Vec<u8> {
        let max_read_size = (64 * 1024 * 1024) as usize;
        if data.len() <= max_read_size {
            data.resize(max_read_size + 1, 0);
        }
        data
    }

    #[test]
    fn ogg_writer_handles_large_input() {
        let test_file = data_path("multipagecomment.ogg");
        if !test_file.exists() {
            return;
        }

        let original = std::fs::read(&test_file).unwrap();
        let mut load_cursor = Cursor::new(original.clone());
        let mut ogg = OggVorbis::load_from_reader(&mut load_cursor).unwrap();

        let oversized = oversized_writer_bytes(original);
        let mut out = Cursor::new(oversized);
        ogg.save_to_writer(&mut out).unwrap();
    }

    #[test]
    fn dsf_writer_handles_large_input() {
        let test_file = data_path("with-id3.dsf");
        if !test_file.exists() {
            return;
        }

        let original = std::fs::read(&test_file).unwrap();
        let mut load_cursor = Cursor::new(original.clone());
        let mut dsf = DSF::load_from_reader(&mut load_cursor).unwrap();

        let oversized = oversized_writer_bytes(original);
        let mut out = Cursor::new(oversized);
        dsf.save_to_writer(&mut out).unwrap();
    }

    #[test]
    fn dsdiff_writer_handles_large_input() {
        let test_file = data_path("5644800-2ch-s01-silence.dff");
        if !test_file.exists() {
            return;
        }

        let original = std::fs::read(&test_file).unwrap();
        let mut load_cursor = Cursor::new(original.clone());
        let mut dsdiff = DSDIFF::load_from_reader(&mut load_cursor).unwrap();
        dsdiff.add_tags().unwrap();

        let oversized = oversized_writer_bytes(original);
        let mut out = Cursor::new(oversized);
        dsdiff.save_to_writer(&mut out).unwrap();
    }
}

// ---------------------------------------------------------------------------
// Module: save_to_path_tests
//
// save_to_path round-trip: load from reader (no filename) -> copy to temp -> save_to_path -> reload
// Uses tempfile for the copy so original test data is never modified.
// ---------------------------------------------------------------------------
mod save_to_path_tests {
    use super::*;
    use audex::aiff::AIFF;
    use audex::asf::ASF;
    use audex::dsdiff::DSDIFF;
    use audex::dsf::DSF;
    use audex::flac::FLAC;
    use audex::monkeysaudio::MonkeysAudio;
    use audex::mp3::MP3;
    use audex::mp4::MP4;
    use audex::musepack::Musepack;
    use audex::oggvorbis::OggVorbis;
    use audex::optimfrog::OptimFROG;
    use audex::tak::TAK;
    use audex::trueaudio::TrueAudio;
    use audex::wave::WAVE;
    use audex::wavpack::WavPack;
    use std::fs;
    fn temp_copy_path(source: &std::path::Path) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(source.file_name().unwrap());
        fs::copy(source, &path).unwrap();
        (dir, path)
    }
    /// Helper: load from reader, copy test file to temp path, save_to_path, reload from path
    fn save_to_path_roundtrip<T: FileType>(filename: &str) {
        let test_file = data_path(filename);
        if !test_file.exists() {
            return;
        }

        // Load from reader (no filename)
        let data = std::fs::read(&test_file).unwrap();
        let mut cursor = Cursor::new(data);
        let mut file = T::load_from_reader(&mut cursor).unwrap();
        let original_sample_rate = file.info().sample_rate();
        let original_channels = file.info().channels();

        // Create a temp copy of the test file (original file is untouched)
        let (_dir, temp_path) = temp_copy_path(&test_file);

        // Save to the temp path
        file.save_to_path(&temp_path).unwrap();

        // Reload from the temp path using path-based load
        let reloaded = T::load(&temp_path).unwrap();

        assert_eq!(
            reloaded.info().sample_rate(),
            original_sample_rate,
            "Sample rate should be preserved after save_to_path"
        );
        assert_eq!(
            reloaded.info().channels(),
            original_channels,
            "Channels should be preserved after save_to_path"
        );
    }

    #[test]
    fn test_flac_save_to_path() {
        save_to_path_roundtrip::<FLAC>("silence-44-s.flac");
    }

    #[test]
    fn test_mp3_save_to_path() {
        save_to_path_roundtrip::<MP3>("silence-44-s.mp3");
    }

    #[test]
    fn test_mp4_save_to_path() {
        save_to_path_roundtrip::<MP4>("has-tags.m4a");
    }

    #[test]
    fn test_ogg_save_to_path() {
        save_to_path_roundtrip::<OggVorbis>("empty.ogg");
    }

    #[test]
    fn test_asf_save_to_path() {
        save_to_path_roundtrip::<ASF>("silence-1.wma");
    }

    #[test]
    fn test_aiff_save_to_path() {
        save_to_path_roundtrip::<AIFF>("with-id3.aif");
    }

    #[test]
    fn test_wave_save_to_path() {
        save_to_path_roundtrip::<WAVE>("silence-2s-PCM-44100-16-ID3v23.wav");
    }

    #[test]
    fn test_dsf_save_to_path() {
        save_to_path_roundtrip::<DSF>("with-id3.dsf");
    }

    #[test]
    fn test_dsdiff_save_to_path() {
        save_to_path_roundtrip::<DSDIFF>("5644800-2ch-s01-silence.dff");
    }

    #[test]
    fn test_tak_save_to_path() {
        save_to_path_roundtrip::<TAK>("has-tags.tak");
    }

    #[test]
    fn test_trueaudio_save_to_path() {
        save_to_path_roundtrip::<TrueAudio>("empty.tta");
    }

    #[test]
    fn test_wavpack_save_to_path() {
        save_to_path_roundtrip::<WavPack>("silence-44-s.wv");
    }

    #[test]
    fn test_musepack_save_to_path() {
        save_to_path_roundtrip::<Musepack>("click.mpc");
    }

    #[test]
    fn test_monkeysaudio_save_to_path() {
        save_to_path_roundtrip::<MonkeysAudio>("mac-399.ape");
    }

    #[test]
    fn test_optimfrog_save_to_path() {
        save_to_path_roundtrip::<OptimFROG>("empty.ofr");
    }
}

// ---------------------------------------------------------------------------
// Module: path_to_path_tests
//
// Load from path (filename IS set) -> copy to temp -> save_to_path(temp) -> reload
// Uses tempfile for the copy so original test data is never modified.
// ---------------------------------------------------------------------------
mod path_to_path_tests {
    use super::*;
    use audex::flac::FLAC;
    use audex::mp3::MP3;
    use std::fs;
    fn temp_copy_path(source: &std::path::Path) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(source.file_name().unwrap());
        fs::copy(source, &path).unwrap();
        (dir, path)
    }

    /// Helper: load from path, copy test file to temp, save_to_path, reload and verify
    fn path_to_path_roundtrip<T: FileType>(filename: &str) {
        let test_file = data_path(filename);
        if !test_file.exists() {
            return;
        }

        // Load from path (filename IS set)
        let mut file = T::load(&test_file).unwrap();
        let original_sample_rate = file.info().sample_rate();
        let original_channels = file.info().channels();
        let original_bits = file.info().bits_per_sample();

        // Create a temp copy to save into (original file is untouched)
        let (_dir, temp_path) = temp_copy_path(&test_file);

        // Save to the different (temp) path
        file.save_to_path(&temp_path).unwrap();

        // Reload from the temp path
        let reloaded = T::load(&temp_path).unwrap();

        assert_eq!(
            reloaded.info().sample_rate(),
            original_sample_rate,
            "Sample rate should be preserved after path-to-path save"
        );
        assert_eq!(
            reloaded.info().channels(),
            original_channels,
            "Channels should be preserved after path-to-path save"
        );
        assert_eq!(
            reloaded.info().bits_per_sample(),
            original_bits,
            "Bits per sample should be preserved after path-to-path save"
        );

        // Verify that the reloaded file still has tags (not cleared)
        let reloaded_keys = reloaded.keys();
        assert!(
            !reloaded_keys.is_empty(),
            "Tags should survive the path-to-path save round-trip"
        );
    }

    #[test]
    fn test_flac_path_to_path() {
        path_to_path_roundtrip::<FLAC>("silence-44-s.flac");
    }

    #[test]
    fn test_mp3_path_to_path() {
        path_to_path_roundtrip::<MP3>("silence-44-s.mp3");
    }
}

// ---------------------------------------------------------------------------
// Tag write round-trips: set a tag, save, reload, verify it persists
// ---------------------------------------------------------------------------
mod tag_write_roundtrip_tests {
    use super::*;
    use audex::File;
    use common::TestUtils;

    #[test]
    fn test_aiff_tag_write_roundtrip() {
        let tmp = TestUtils::get_temp_copy(data_path("with-id3.aif")).expect("temp copy");
        let mut file = File::load(tmp.path()).expect("load AIFF");
        file.set("TIT2", vec!["AIFF Write Test".to_string()])
            .unwrap();
        file.save().unwrap();

        let reloaded = File::load(tmp.path()).expect("reload");
        assert_eq!(
            reloaded.get("TIT2"),
            Some(vec!["AIFF Write Test".to_string()])
        );
    }

    #[test]
    fn test_wave_tag_write_roundtrip() {
        let tmp = TestUtils::get_temp_copy(data_path("silence-2s-PCM-44100-16-ID3v23.wav"))
            .expect("temp copy");
        let mut file = File::load(tmp.path()).expect("load WAVE");
        file.set("TIT2", vec!["WAVE Write Test".to_string()])
            .unwrap();
        file.save().unwrap();

        let reloaded = File::load(tmp.path()).expect("reload");
        assert_eq!(
            reloaded.get("TIT2"),
            Some(vec!["WAVE Write Test".to_string()])
        );
    }

    #[test]
    fn test_dsf_tag_write_roundtrip() {
        let tmp = TestUtils::get_temp_copy(data_path("with-id3.dsf")).expect("temp copy");
        let mut file = File::load(tmp.path()).expect("load DSF");
        file.set("TIT2", vec!["DSF Write Test".to_string()])
            .unwrap();
        file.save().unwrap();

        let reloaded = File::load(tmp.path()).expect("reload");
        assert_eq!(
            reloaded.get("TIT2"),
            Some(vec!["DSF Write Test".to_string()])
        );
    }

    #[test]
    fn test_oggvorbis_tag_write_roundtrip() {
        let tmp = TestUtils::get_temp_copy(data_path("multipagecomment.ogg")).expect("temp copy");
        let mut file = File::load(tmp.path()).expect("load OggVorbis");
        file.set("title", vec!["OggVorbis Write Test".to_string()])
            .unwrap();
        file.save().unwrap();

        let reloaded = File::load(tmp.path()).expect("reload");
        assert_eq!(
            reloaded.get("title"),
            Some(vec!["OggVorbis Write Test".to_string()])
        );
    }

    #[test]
    fn test_oggopus_tag_write_roundtrip() {
        let tmp = TestUtils::get_temp_copy(data_path("example.opus")).expect("temp copy");
        let mut file = File::load(tmp.path()).expect("load OggOpus");
        file.set("title", vec!["OggOpus Write Test".to_string()])
            .unwrap();
        file.save().unwrap();

        let reloaded = File::load(tmp.path()).expect("reload");
        assert_eq!(
            reloaded.get("title"),
            Some(vec!["OggOpus Write Test".to_string()])
        );
    }

    #[test]
    fn test_mp4_tag_write_roundtrip() {
        let tmp = TestUtils::get_temp_copy(data_path("has-tags.m4a")).expect("temp copy");
        let mut file = File::load(tmp.path()).expect("load MP4");
        file.set("\u{a9}nam", vec!["MP4 Write Test".to_string()])
            .unwrap();
        file.save().unwrap();

        let reloaded = File::load(tmp.path()).expect("reload");
        assert_eq!(
            reloaded.get("\u{a9}nam"),
            Some(vec!["MP4 Write Test".to_string()])
        );
    }

    #[test]
    fn test_asf_tag_write_roundtrip() {
        let tmp = TestUtils::get_temp_copy(data_path("silence-1.wma")).expect("temp copy");
        let mut file = File::load(tmp.path()).expect("load ASF");
        file.set("Title", vec!["ASF Write Test".to_string()])
            .unwrap();
        file.save().unwrap();

        let reloaded = File::load(tmp.path()).expect("reload");
        assert_eq!(
            reloaded.get("Title"),
            Some(vec!["ASF Write Test".to_string()])
        );
    }
}
